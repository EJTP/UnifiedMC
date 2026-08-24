//! Getting an instance to match what the server publishes.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use futures::stream::{self, StreamExt};
use sha1::{Digest, Sha1};

use crate::paths;
use crate::servers::{ConfigEntry, ModEntry};

const PARALLEL_DOWNLOADS: usize = 8;

pub fn have(sha1: &str) -> bool {
    paths::blobs().join(sha1).is_file()
}

pub fn hash_file(path: &Path) -> Result<String> {
    use std::io::Read;

    let mut file = fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Fetch into the shared blob store, verified by hash so a truncated file never sticks.
async fn download(client: &reqwest::Client, entry: &ModEntry) -> Result<()> {
    if entry.url.is_empty() {
        return Err(anyhow!("{}: not stored locally and no url", entry.name));
    }
    let blobs = paths::blobs();
    fs::create_dir_all(&blobs)?;

    let bytes = client
        .get(&entry.url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    let got = hex::encode(Sha1::digest(&bytes));
    if got != entry.sha1 {
        return Err(anyhow!("{}: hash mismatch", entry.name));
    }

    let temporary = blobs.join(format!(".{}.part", entry.sha1));
    fs::write(&temporary, &bytes)?;
    fs::rename(temporary, blobs.join(&entry.sha1))?;
    Ok(())
}

pub struct SyncReport {
    pub total: usize,
    pub downloaded: usize,
}

/// Make `mods_dir` contain exactly `mods`, downloading only what is missing.
pub async fn mods<F>(
    client: &reqwest::Client,
    mods: &[ModEntry],
    mods_dir: &Path,
    mut progress: F,
) -> Result<SyncReport>
where
    F: FnMut(usize, usize, &str) + Send,
{
    // owned, not borrowed: a reference living inside the futures makes the whole stream
    // higher-ranked, and the command it ends up in stops being nameable
    let missing: Vec<ModEntry> = mods.iter().filter(|m| !have(&m.sha1)).cloned().collect();
    let count = missing.len();

    if count > 0 {
        let mut done = 0usize;
        let mut running = stream::iter(missing.into_iter().map(|entry| {
            let client = client.clone();
            async move {
                let name = entry.name.clone();
                (name, download(&client, &entry).await)
            }
        }))
        .buffer_unordered(PARALLEL_DOWNLOADS);

        while let Some((name, outcome)) = running.next().await {
            outcome.with_context(|| format!("downloading {name}"))?;
            done += 1;
            progress(done, count, &name);
        }
    }

    fs::create_dir_all(mods_dir)?;
    let wanted: HashMap<&str, &str> = mods
        .iter()
        .map(|m| (m.name.as_str(), m.sha1.as_str()))
        .collect();

    // a mod the server dropped has to go here too, or it stays loaded forever
    if let Ok(existing) = fs::read_dir(mods_dir) {
        for entry in existing.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !wanted.contains_key(name.as_str()) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    for (name, sha1) in &wanted {
        link(&paths::blobs().join(sha1), &mods_dir.join(name))?;
    }

    Ok(SyncReport {
        total: mods.len(),
        downloaded: count,
    })
}

/// Hard link, so the same jar on ten servers costs one copy.
fn link(from: &Path, to: &Path) -> Result<()> {
    if to.exists() {
        fs::remove_file(to)?;
    }
    match fs::hard_link(from, to) {
        Ok(()) => Ok(()),
        // different filesystem, or one that has no links: a copy is correct, just larger
        Err(_) => {
            fs::copy(from, to)?;
            Ok(())
        }
    }
}

/// Write the server's config into the instance.
///
/// A file is replaced only when the server's copy changed since it was last delivered.
/// Otherwise whatever changed it locally keeps it - the player, or the mod itself, and many
/// rewrite their own config on shutdown. Files the pack marks `force` win regardless.
///
/// Copied rather than linked: config is meant to be edited, and editing a link would write
/// the change back into the shared store for every other server.
pub async fn config(
    client: &reqwest::Client,
    entries: &[ConfigEntry],
    instance: &Path,
) -> Result<(usize, usize)> {
    if entries.is_empty() {
        return Ok((0, 0));
    }

    let ledger_path = instance.join(".unifiedmc-config.json");
    let mut delivered: HashMap<String, String> = fs::read_to_string(&ledger_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();

    let (mut written, mut kept) = (0usize, 0usize);

    for entry in entries {
        let relative = PathBuf::from(&entry.path);
        if relative.is_absolute() || relative.components().any(|c| c.as_os_str() == "..") {
            continue; // the server names paths on our disk; that is a boundary, not a hint
        }
        let target = instance.join("config").join(&relative);

        if target.exists() {
            let current = hash_file(&target)?;
            if current == entry.sha1 {
                delivered.insert(entry.path.clone(), current);
                continue;
            }
            if !entry.force && delivered.get(&entry.path) == Some(&entry.sha1) {
                kept += 1;
                continue;
            }
        }

        if !have(&entry.sha1) {
            download(
                client,
                &ModEntry {
                    name: entry.path.clone(),
                    sha1: entry.sha1.clone(),
                    url: entry.url.clone(),
                },
            )
            .await?;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(paths::blobs().join(&entry.sha1), &target)?;
        delivered.insert(entry.path.clone(), entry.sha1.clone());
        written += 1;
    }

    fs::write(ledger_path, serde_json::to_vec_pretty(&delivered)?)?;
    Ok((written, kept))
}

/// Mods the player brought themselves, minus anything the server already ships.
pub fn personal(server_mods: &[ModEntry], profile: &Path) -> Vec<PathBuf> {
    let served: HashSet<&str> = server_mods.iter().map(|m| m.sha1.as_str()).collect();
    let folder = profile.join("mods");
    let _ = fs::create_dir_all(&folder);

    let Ok(entries) = fs::read_dir(&folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jar"))
        .filter(|p| {
            hash_file(p)
                .map(|h| !served.contains(h.as_str()))
                .unwrap_or(true)
        })
        .collect()
}
