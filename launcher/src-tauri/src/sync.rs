//! Getting an instance to match what the server publishes.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use futures::stream::{self, StreamExt};
use sha1::{Digest, Sha1};

use crate::paths;
use crate::servers::{ConfigEntry, ModEntry};

const PARALLEL_DOWNLOADS: usize = 8;

/// Every string in a manifest is written by whoever answered on that port, and `Path::join`
/// reads "../.." as a direction. These three say what a manifest may name, and everything that
/// turns manifest text into a path goes through one of them.
pub fn plain_name(name: &str) -> bool {
    Path::new(name).file_name().is_some_and(|f| f == name)
}

/// A relative path inside a directory we chose. `is_absolute()` is not enough on Windows,
/// where `PathBuf::push` replaces the whole path with `C:x` or `\Windows\x`.
pub fn plain_relative(path: &str) -> bool {
    let path = Path::new(path);
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

/// A blob store filename. Lowercase so one blob cannot have two names.
pub fn is_hash(sha1: &str) -> bool {
    sha1.len() == 40 && sha1.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Whether the blob store already holds this file. The hash check lives in `download`, so a
/// sha1 that is not a hash must never look present here.
pub fn have(sha1: &str) -> bool {
    is_hash(sha1) && paths::blobs().join(sha1).is_file()
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

/// Fetch into the blob store, verified by hash. Catches a truncated download, not a
/// substituted one: the hash arrives on the same unauthenticated channel as the bytes.
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
    let landed = blobs.join(&entry.sha1);
    fs::rename(&temporary, &landed).with_context(|| format!("storing {}", landed.display()))?;
    Ok(())
}

pub struct SyncReport {
    pub total: usize,
    pub downloaded: usize,
}

/// What the manifest asks for, minus anything whose name or hash is not one. One filter for
/// all three loops, or the delete loop removes what the link loop is about to write.
fn wanted(mods: &[ModEntry]) -> HashMap<&str, &str> {
    mods.iter()
        .filter(|m| plain_name(&m.name) && is_hash(&m.sha1))
        .map(|m| (m.name.as_str(), m.sha1.as_str()))
        .collect()
}

/// Get every listed blob into the store. Returns how many had to be fetched.
async fn fetch<F>(client: &reqwest::Client, mods: &[ModEntry], mut progress: F) -> Result<usize>
where
    F: FnMut(usize, usize, &str) + Send,
{
    // owned, not borrowed: a reference inside the futures makes the stream higher-ranked
    let missing: Vec<ModEntry> = mods
        .iter()
        .filter(|m| plain_name(&m.name) && is_hash(&m.sha1) && !have(&m.sha1))
        .cloned()
        .collect();
    let count = missing.len();

    if count == 0 {
        return Ok(0);
    }
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
    Ok(count)
}

fn link_all(entries: &[ModEntry], dir: &Path) -> Result<()> {
    for (name, sha1) in wanted(entries) {
        link(&paths::blobs().join(sha1), &dir.join(name))?;
    }
    Ok(())
}

/// Make `dir` contain exactly `entries`, fetching only what is missing.
///
/// The same job for mods, datapacks, resource packs and shaders - only the directory differs.
/// Only for a directory the launcher owns; one the player fills is `add()`'s job.
pub async fn into_dir<F>(
    client: &reqwest::Client,
    entries: &[ModEntry],
    dir: &Path,
    progress: F,
) -> Result<SyncReport>
where
    F: FnMut(usize, usize, &str) + Send,
{
    let count = fetch(client, entries, progress).await?;

    fs::create_dir_all(dir)?;
    let wanted = wanted(entries);

    // something the server dropped has to go here too, or it stays loaded forever
    if let Ok(existing) = fs::read_dir(dir) {
        for entry in existing.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !wanted.contains_key(name.as_str()) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    link_all(entries, dir)?;

    Ok(SyncReport {
        total: entries.len(),
        downloaded: count,
    })
}

/// Put these files in a directory, and leave everything else in it alone.
///
/// The player's own profile is not ours to reconcile: installing one mod must not take away
/// the nine from last week.
pub async fn add(client: &reqwest::Client, entries: &[ModEntry], dir: &Path) -> Result<()> {
    fetch(client, entries, |_, _, _| {}).await?;
    fs::create_dir_all(dir)?;
    link_all(entries, dir)
}

/// Hard link, so the same jar on ten servers costs one copy.
fn link(from: &Path, to: &Path) -> Result<()> {
    clear(to).with_context(|| format!("replacing {}", to.display()))?;
    match fs::hard_link(from, to) {
        Ok(()) => Ok(()),
        // different filesystem, or one that has no links: a copy is correct, just larger
        Err(_) => {
            fs::copy(from, to).with_context(|| format!("writing {}", to.display()))?;
            Ok(())
        }
    }
}

/// Get whatever is at `to` out of the way.
///
/// Windows refuses to delete a file another process holds open, one marked read-only, and a
/// directory handed to remove_file - all three as "Access is denied (os error 5)", with no path
/// in the message. A running game holding its own jars is the common one, and an antivirus
/// reading a jar it has just seen appear is the transient one, which is what the retry is for.
fn clear(to: &Path) -> std::io::Result<()> {
    let Ok(existing) = fs::symlink_metadata(to) else {
        return Ok(()); // nothing there
    };

    if existing.is_dir() {
        return fs::remove_dir_all(to);
    }
    if existing.permissions().readonly() {
        let mut relaxed = existing.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        relaxed.set_readonly(false);
        let _ = fs::set_permissions(to, relaxed);
    }

    let mut last = fs::remove_file(to);
    for _ in 0..4 {
        if last.is_ok() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
        last = fs::remove_file(to);
    }
    last
}

/// Write the server's config into the instance.
///
/// Replaced only when the server's copy changed since it was last delivered; otherwise whatever
/// changed it locally keeps it. Files marked `force` win regardless. Copied, not linked -
/// editing a link would write the change back into the shared store.
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
        if !plain_relative(&entry.path) {
            continue; // the server names paths on our disk; that is a boundary, not a hint
        }
        let target = instance.join("config").join(&entry.path);

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
        // Cleared first, not written over. Windows refuses CREATE_ALWAYS on a file carrying the
        // hidden attribute - which is what it puts on the dot-files mods write beside their
        // config - and reports it as "Access is denied" with the copy, not the file, to blame.
        clear(&target).with_context(|| format!("replacing {}", target.display()))?;
        fs::copy(paths::blobs().join(&entry.sha1), &target)
            .with_context(|| format!("writing {}", target.display()))?;
        delivered.insert(entry.path.clone(), entry.sha1.clone());
        written += 1;
    }

    fs::write(ledger_path, serde_json::to_vec_pretty(&delivered)?)?;
    Ok((written, kept))
}

/// What the player put in one of their own folders, minus anything the server already ships.
/// The folder is passed in whole: a mod is a jar, a pack a zip, a datapack either.
pub fn personal(served: &[ModEntry], folder: &Path) -> Vec<PathBuf> {
    let served: HashSet<&str> = served.iter().map(|m| m.sha1.as_str()).collect();

    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            hash_file(p)
                .map(|h| !served.contains(h.as_str()))
                .unwrap_or(true)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    /// Windows reports all three of these as "Access is denied (os error 5)". The directory case
    /// fails on Linux too, so it is the one this can actually prove here.
    #[test]
    fn whatever_is_in_the_way_gets_out_of_the_way() {
        let dir = std::env::temp_dir().join("unifiedmc-link-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let source = dir.join("source.jar");
        fs::write(&source, b"a jar").unwrap();

        // a directory where the jar belongs - remove_file refuses this on every platform
        let target = dir.join("target.jar");
        fs::create_dir_all(target.join("something")).unwrap();
        super::link(&source, &target).expect("a directory in the way");
        assert_eq!(fs::read(&target).unwrap(), b"a jar");

        // a read-only file, which is what Windows leaves behind often enough to matter.
        // Removed first: target is a hard link to source now, so writing through it would
        // change the source as well and the test would be measuring its own mistake.
        fs::remove_file(&target).unwrap();
        fs::write(&target, b"old").unwrap();
        let mut locked = fs::metadata(&target).unwrap().permissions();
        locked.set_readonly(true);
        fs::set_permissions(&target, locked).unwrap();
        super::link(&source, &target).expect("a read-only file in the way");
        assert_eq!(fs::read(&target).unwrap(), b"a jar");

        // and the ordinary case still works
        fs::remove_file(&target).unwrap();
        fs::write(&target, b"old").unwrap();
        super::link(&source, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"a jar");

        let _ = fs::remove_dir_all(&dir);
    }

    use super::*;

    #[test]
    fn a_manifest_name_addresses_a_file_and_never_builds_a_path() {
        // every one of these is a real manifest a hostile server can publish
        for hostile in [
            "../../../../.bashrc",
            "../evil.jar",
            "/home/player/.bashrc",
            "..",
            ".",
            "",
            "sub/dir.jar",
        ] {
            assert!(!plain_name(hostile), "accepted {hostile:?}");
        }
        // and a player must still be able to install any of these
        for real in [
            "sodium.jar",
            "Xaero's Minimap.jar",
            "create+extras [1.21].jar",
            "journeymap-1.21.1-fabric.jar",
            "Überlauf.jar",
        ] {
            assert!(plain_name(real), "rejected {real:?}");
        }
    }

    #[test]
    fn a_config_path_stays_inside_the_instance() {
        assert!(plain_relative("config/create-common.toml"));
        assert!(plain_relative("a.toml"));
        for hostile in ["", "..", "../../.ssh/authorized_keys", "/etc/passwd"] {
            assert!(!plain_relative(hostile), "accepted {hostile:?}");
        }
        // drive- and root-relative are neither absolute nor harmless on Windows
        #[cfg(windows)]
        for hostile in ["C:evil", r"\Windows\System32\x", r"\\server\share\x"] {
            assert!(!plain_relative(hostile), "accepted {hostile:?}");
        }
    }

    #[test]
    fn a_blob_is_only_ever_looked_up_by_a_hash() {
        assert!(is_hash("da39a3ee5e6b4b0d3255bfef95601890afd80709"));
        // uppercase would give one blob two names, and Sha1 only ever produces lowercase
        assert!(!is_hash("DA39A3EE5E6B4B0D3255BFEF95601890AFD80709"));
        assert!(!is_hash("../../.ssh/id_rsa"));
        assert!(!is_hash(""));
        assert!(!is_hash("da39a3ee"));

        // have() must not answer yes about a file outside the store, or download() never runs
        assert!(!have("../../.ssh/id_rsa"));
        assert!(!have("/etc/passwd"));
    }

    #[test]
    fn a_hostile_entry_is_dropped_before_any_loop_sees_it() {
        let entries = vec![
            ModEntry {
                name: "sodium.jar".into(),
                sha1: "da39a3ee5e6b4b0d3255bfef95601890afd80709".into(),
                url: String::new(),
            },
            ModEntry {
                name: "../../../../.bashrc".into(),
                sha1: "da39a3ee5e6b4b0d3255bfef95601890afd80709".into(),
                url: String::new(),
            },
            ModEntry {
                name: "stolen.jar".into(),
                sha1: "../../.ssh/id_rsa".into(),
                url: String::new(),
            },
        ];
        let kept = wanted(&entries);
        assert_eq!(kept.len(), 1);
        assert!(kept.contains_key("sodium.jar"));
    }

    #[test]
    fn nothing_a_manifest_names_can_land_outside_the_target_directory() {
        // a resource pack is named by a human, so spaces and brackets have to survive
        let named = |name: &str| ModEntry {
            name: name.into(),
            sha1: "da39a3ee5e6b4b0d3255bfef95601890afd80709".into(),
            url: String::new(),
        };
        let entries = vec![
            named("Faithful 32x [1.21].zip"),
            named("../../../../.minecraft/options.txt"),
            named("/etc/cron.d/evil"),
            named("world/../../escape.zip"),
        ];

        let kept = wanted(&entries);
        assert_eq!(kept.len(), 1);
        assert!(kept.contains_key("Faithful 32x [1.21].zip"));

        // the invariant link_all's join depends on: whatever survives is a child of our directory
        let target = Path::new("/home/player/.unifiedmc/instances/x/resourcepacks");
        for name in kept.keys() {
            assert_eq!(target.join(name).parent(), Some(target), "{name} escaped");
        }
    }
}
