//! Reading a modpack: which jars the server loads, which it only hands to clients, and what
//! has to be fetched. Three formats, one answer.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

/// Where a file has to end up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    /// Loaded by the server, and sent to clients.
    Both,
    /// Sent to clients, never loaded by the server.
    ClientOnly,
    /// Loaded by the server, never sent.
    ServerOnly,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackFile {
    /// Relative path inside the pack, e.g. `mods/sodium.jar`.
    pub path: String,
    pub side: Side,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// Present when the pack ships the bytes rather than a link to them.
    #[serde(skip)]
    pub bytes: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pack {
    pub name: String,
    pub version: String,
    pub minecraft: String,
    pub loader: Option<(String, String)>,
    pub files: Vec<PackFile>,
    /// Files the pack lists but nobody may download for us - CurseForge lets authors opt out
    /// of third-party distribution, and silently dropping those makes a broken pack.
    pub blocked: Vec<String>,
}

impl Pack {
    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for file in &self.files {
            let key = match file.side {
                Side::Both => "shared",
                Side::ClientOnly => "client",
                Side::ServerOnly => "server",
            };
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
    }
}

pub fn read(path: &Path) -> Result<Pack> {
    let file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file).context("not a zip archive")?;

    let names: Vec<String> = archive.file_names().map(str::to_string).collect();

    if names.iter().any(|n| n == "modrinth.index.json") {
        return read_mrpack(&mut archive);
    }
    if names.iter().any(|n| n == "manifest.json") {
        return read_curseforge_manifest(&mut archive);
    }
    read_server_pack(&mut archive, path)
}

fn entry<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>, name: &str) -> Result<Vec<u8>> {
    let mut file = archive.by_name(name)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Modrinth's format. The index says which side each file is for, which is exactly the
/// question everything else here has to reconstruct by hand.
fn read_mrpack<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Result<Pack> {
    let index: serde_json::Value = serde_json::from_slice(&entry(archive, "modrinth.index.json")?)?;

    let dependencies = index.get("dependencies").cloned().unwrap_or_default();
    let minecraft = dependencies
        .get("minecraft")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("the index names no Minecraft version"))?
        .to_string();

    let loader = ["neoforge", "forge", "fabric-loader", "quilt-loader"]
        .iter()
        .find_map(|key| {
            let version = dependencies.get(*key)?.as_str()?.to_string();
            let name = key.trim_end_matches("-loader").to_string();
            Some((name, version))
        });

    let mut files = Vec::new();
    if let Some(listed) = index.get("files").and_then(|f| f.as_array()) {
        for file in listed {
            let path = file
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if path.is_empty() {
                continue;
            }
            let env = file.get("env");
            let wants = |which: &str| {
                env.and_then(|e| e.get(which))
                    .and_then(|v| v.as_str())
                    .map(|v| v != "unsupported")
                    .unwrap_or(true) // no env block: the pack means it for both
            };
            let side = match (wants("client"), wants("server")) {
                (true, true) => Side::Both,
                (true, false) => Side::ClientOnly,
                (false, true) => Side::ServerOnly,
                (false, false) => continue,
            };
            files.push(PackFile {
                path,
                side,
                sha1: file
                    .get("hashes")
                    .and_then(|h| h.get("sha1"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                url: file
                    .get("downloads")
                    .and_then(|d| d.as_array())
                    .and_then(|d| d.first())
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                bytes: None,
            });
        }
    }

    files.extend(overrides(archive)?);

    Ok(Pack {
        name: index
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Pack")
            .to_string(),
        version: index
            .get("versionId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        minecraft,
        loader,
        files,
        blocked: Vec::new(),
    })
}

// CurseForge's client zip: project and file ids to resolve through their API. Nothing in it
// says which side a mod is for.
fn read_curseforge_manifest<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<Pack> {
    let manifest: serde_json::Value = serde_json::from_slice(&entry(archive, "manifest.json")?)?;

    let minecraft = manifest
        .get("minecraft")
        .and_then(|m| m.get("version"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("the manifest names no Minecraft version"))?
        .to_string();

    let loader = manifest
        .get("minecraft")
        .and_then(|m| m.get("modLoaders"))
        .and_then(|l| l.as_array())
        .and_then(|loaders| {
            let id = loaders
                .iter()
                .find(|l| l.get("primary").and_then(|p| p.as_bool()).unwrap_or(false))
                .or_else(|| loaders.first())?
                .get("id")?
                .as_str()?;
            let (name, version) = id.split_once('-')?;
            Some((name.to_string(), version.to_string()))
        });

    // ids only for now; resolve() turns them into files
    let mut files = Vec::new();
    if let Some(listed) = manifest.get("files").and_then(|f| f.as_array()) {
        for file in listed {
            let (Some(project), Some(file_id)) = (
                file.get("projectID").and_then(|v| v.as_u64()),
                file.get("fileID").and_then(|v| v.as_u64()),
            ) else {
                continue;
            };
            files.push(PackFile {
                path: format!("cf://{project}/{file_id}"),
                side: Side::Both, // corrected during resolve, once the catalogue can be asked
                sha1: None,
                url: None,
                bytes: None,
            });
        }
    }

    files.extend(overrides(archive)?);

    Ok(Pack {
        name: manifest
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Pack")
            .to_string(),
        version: manifest
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        minecraft,
        loader,
        files,
        blocked: Vec::new(),
    })
}

/// A finished server zip: the jars are in it, nothing to resolve. No side information either -
/// everything in it is what the server runs, and the client half has to come from elsewhere.
fn read_server_pack<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    path: &Path,
) -> Result<Pack> {
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        let interesting = name.starts_with("mods/") || name.starts_with("config/");
        if !interesting {
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        files.push(PackFile {
            path: name,
            side: Side::Both,
            sha1: None,
            url: None,
            bytes: Some(bytes),
        });
    }

    if files.is_empty() {
        return Err(anyhow!(
            "{} has no mods/ or config/ - is this a Minecraft pack?",
            path.display()
        ));
    }

    Ok(Pack {
        name: path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Server pack".into()),
        version: String::new(),
        minecraft: String::new(), // the zip does not say; the caller has to
        loader: None,
        files,
        blocked: Vec::new(),
    })
}

/// Files a pack ships directly: configs, scripts, sometimes a jar the author bundled.
fn overrides<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Result<Vec<PackFile>> {
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        let (rest, side) = if let Some(rest) = name.strip_prefix("overrides/") {
            (rest, Side::Both)
        } else if let Some(rest) = name.strip_prefix("client-overrides/") {
            (rest, Side::ClientOnly)
        } else if let Some(rest) = name.strip_prefix("server-overrides/") {
            (rest, Side::ServerOnly)
        } else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        files.push(PackFile {
            path: rest.to_string(),
            side,
            sha1: None,
            url: None,
            bytes: Some(bytes),
        });
    }
    Ok(files)
}

/// Backups and per-world state are not configuration; nobody else has any use for them.
pub fn is_private(path: &str) -> bool {
    let lower = path.to_lowercase();
    const JUNK: [&str; 5] = [".bak", ".old", ".tmp", ".dat_old", ".log"];
    const PRIVATE: [&str; 3] = ["config/jei/world/", "config/ftbchunks/", "config/ftbteams/"];
    JUNK.iter().any(|suffix| lower.ends_with(suffix))
        || PRIVATE.iter().any(|dir| lower.starts_with(dir))
}

/// Ask Modrinth about the files the pack left undeclared.
///
/// An mrpack entry may say `env: {client: unknown, server: unknown}`, which the reader has to
/// treat as both. On a server that guess is expensive: one client-only mod left in mods/ takes
/// the start down. Nothing here is fatal - no network leaves the guess in place.
pub async fn resolve_unknown_sides(client: &reqwest::Client, pack: &mut Pack) -> usize {
    let undeclared: Vec<String> = pack
        .files
        .iter()
        .filter(|file| file.side == Side::Both && file.path.starts_with("mods/"))
        .filter_map(|file| file.sha1.clone())
        .collect();
    if undeclared.is_empty() {
        return 0;
    }

    let answer = client
        .post("https://api.modrinth.com/v2/version_files")
        .json(&serde_json::json!({ "hashes": undeclared, "algorithm": "sha1" }))
        .send()
        .await;
    let Ok(response) = answer else { return 0 };
    let Ok(versions) = response.json::<serde_json::Value>().await else {
        return 0;
    };
    let Some(by_hash) = versions.as_object() else {
        return 0;
    };

    // hash -> project id, then one bulk call for the projects themselves: the version tells us
    // which project a file belongs to, and only the project states the sides.
    let projects: Vec<String> = by_hash
        .values()
        .filter_map(|v| v.get("project_id")?.as_str().map(str::to_string))
        .collect();
    if projects.is_empty() {
        return 0;
    }
    let Ok(listed) = client
        .get("https://api.modrinth.com/v2/projects")
        .query(&[("ids", serde_json::to_string(&projects).unwrap_or_default())])
        .send()
        .await
    else {
        return 0;
    };
    let Ok(details) = listed.json::<serde_json::Value>().await else {
        return 0;
    };

    let mut client_only = std::collections::HashSet::new();
    if let Some(array) = details.as_array() {
        for project in array {
            if project.get("server_side").and_then(|v| v.as_str()) == Some("unsupported") {
                if let Some(id) = project.get("id").and_then(|v| v.as_str()) {
                    client_only.insert(id.to_string());
                }
            }
        }
    }

    let mut moved = 0usize;
    for file in &mut pack.files {
        let Some(sha1) = &file.sha1 else { continue };
        let Some(version) = by_hash.get(sha1) else {
            continue;
        };
        let Some(id) = version.get("project_id").and_then(|v| v.as_str()) else {
            continue;
        };
        if file.side == Side::Both && client_only.contains(id) {
            file.side = Side::ClientOnly;
            moved += 1;
        }
    }
    moved
}

/// Turn the `cf://<project>/<file>` placeholders a CurseForge manifest leaves behind into real
/// paths and download links.
///
/// A CurseForge manifest names ids and nothing else - not a filename, not a hash, not even
/// which folder the file belongs in. Two bulk calls answer all of it: the files say what they
/// are called and where they live, the projects say which category they are. Needs an API key;
/// without one every entry stays a placeholder and the caller has to say so.
///
/// Returns how many were resolved. Anything the author opted out of third-party distribution
/// for is moved to `blocked`, because silently dropping it makes a pack that starts and then
/// crashes on a missing dependency.
pub async fn resolve_curseforge(client: &reqwest::Client, key: &str, pack: &mut Pack) -> usize {
    /// What CurseForge calls each of its categories, and where a file of that class goes.
    fn folder(class: u64) -> &'static str {
        match class {
            12 => "resourcepacks",
            6552 => "shaderpacks",
            6945 => "datapacks",
            _ => "mods",
        }
    }

    let wanted: Vec<(usize, u64, u64)> = pack
        .files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| {
            let rest = file.path.strip_prefix("cf://")?;
            let (project, id) = rest.split_once('/')?;
            Some((index, project.parse().ok()?, id.parse().ok()?))
        })
        .collect();
    if wanted.is_empty() || key.is_empty() {
        return 0;
    }

    let cf = |path: &str, body: serde_json::Value| {
        client
            .post(format!("https://api.curseforge.com/v1{path}"))
            .header("x-api-key", key)
            .json(&body)
            .send()
    };

    let file_ids: Vec<u64> = wanted.iter().map(|(_, _, id)| *id).collect();
    let Ok(response) = cf("/mods/files", serde_json::json!({ "fileIds": file_ids })).await else {
        return 0;
    };
    let Ok(listed) = response.json::<serde_json::Value>().await else {
        return 0;
    };
    let Some(files) = listed.get("data").and_then(|d| d.as_array()) else {
        return 0;
    };

    // The category lives on the project, not on the file, and it decides the folder.
    let project_ids: Vec<u64> = wanted.iter().map(|(_, project, _)| *project).collect();
    let mut class_of: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    if let Ok(response) = cf("/mods", serde_json::json!({ "modIds": project_ids })).await {
        if let Ok(projects) = response.json::<serde_json::Value>().await {
            for project in projects
                .get("data")
                .and_then(|d| d.as_array())
                .unwrap_or(&Vec::new())
            {
                let (Some(id), Some(class)) = (
                    project.get("id").and_then(serde_json::Value::as_u64),
                    project.get("classId").and_then(serde_json::Value::as_u64),
                ) else {
                    continue;
                };
                class_of.insert(id, class);
            }
        }
    }

    let mut by_file_id: std::collections::HashMap<u64, &serde_json::Value> =
        std::collections::HashMap::new();
    for file in files {
        if let Some(id) = file.get("id").and_then(serde_json::Value::as_u64) {
            by_file_id.insert(id, file);
        }
    }

    let mut resolved = 0usize;
    let mut blocked = Vec::new();
    for (index, project, file_id) in wanted {
        let Some(entry) = by_file_id.get(&file_id) else {
            continue;
        };
        let Some(name) = entry.get("fileName").and_then(|v| v.as_str()) else {
            continue;
        };
        // A name out of somebody else's manifest becomes a path here, so it may only be a name.
        if !crate::sync::plain_name(name) {
            continue;
        }
        let folder = folder(class_of.get(&project).copied().unwrap_or(6));

        // downloadUrl is null exactly when the author opted out of third-party distribution.
        let Some(url) = entry
            .get("downloadUrl")
            .and_then(|v| v.as_str())
            .filter(|url| !url.is_empty())
        else {
            blocked.push(name.to_string());
            continue;
        };

        pack.files[index].path = format!("{folder}/{name}");
        pack.files[index].url = Some(url.to_string());
        pack.files[index].sha1 =
            entry
                .get("hashes")
                .and_then(|h| h.as_array())
                .and_then(|hashes| {
                    hashes
                        .iter()
                        .find(|hash| {
                            hash.get("algo").and_then(serde_json::Value::as_u64) == Some(1)
                        })?
                        .get("value")?
                        .as_str()
                        .map(str::to_string)
                });
        resolved += 1;
    }

    // Dropped rather than left as a placeholder: an unresolved entry would be written as a
    // file called "cf:", and the blocked list is what tells the admin to fetch it by hand.
    pack.files.retain(|file| !file.path.starts_with("cf://"));
    pack.blocked.extend(blocked);
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zip_with(entries: &[(&str, &str)]) -> Vec<u8> {
        use std::io::Write;
        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            for (name, body) in entries {
                writer
                    .start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(body.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        buffer
    }

    fn read_bytes(bytes: Vec<u8>) -> Result<Pack> {
        let path = std::env::temp_dir().join(format!("unifiedmc-test-{}.zip", bytes.len()));
        std::fs::write(&path, bytes)?;
        let pack = read(&path);
        let _ = std::fs::remove_file(&path);
        pack
    }

    #[test]
    fn mrpack_carries_the_side_split_we_would_otherwise_have_to_guess() {
        let index = r#"{
            "name": "Test", "versionId": "1.0",
            "dependencies": {"minecraft": "1.21.1", "neoforge": "21.1.247"},
            "files": [
                {"path": "mods/shared.jar", "hashes": {"sha1": "a"}, "downloads": ["http://x/a"]},
                {"path": "mods/client.jar", "hashes": {"sha1": "b"}, "downloads": ["http://x/b"],
                 "env": {"client": "required", "server": "unsupported"}},
                {"path": "mods/server.jar", "hashes": {"sha1": "c"}, "downloads": ["http://x/c"],
                 "env": {"client": "unsupported", "server": "required"}}
            ]
        }"#;
        let pack = read_bytes(zip_with(&[
            ("modrinth.index.json", index),
            ("overrides/config/a.toml", "x"),
            ("client-overrides/config/b.toml", "y"),
        ]))
        .unwrap();

        assert_eq!(pack.minecraft, "1.21.1");
        assert_eq!(pack.loader, Some(("neoforge".into(), "21.1.247".into())));

        let side = |path: &str| pack.files.iter().find(|f| f.path == path).unwrap().side;
        assert_eq!(
            side("mods/shared.jar"),
            Side::Both,
            "no env block means both"
        );
        assert_eq!(side("mods/client.jar"), Side::ClientOnly);
        assert_eq!(side("mods/server.jar"), Side::ServerOnly);
        assert_eq!(side("config/a.toml"), Side::Both);
        assert_eq!(side("config/b.toml"), Side::ClientOnly);
    }

    #[test]
    fn curseforge_manifest_keeps_the_ids_for_later() {
        let manifest = r#"{
            "name": "CF Pack", "version": "2.0",
            "minecraft": {"version": "1.21.1",
                          "modLoaders": [{"id": "neoforge-21.1.247", "primary": true}]},
            "files": [{"projectID": 238222, "fileID": 5678}]
        }"#;
        let pack = read_bytes(zip_with(&[("manifest.json", manifest)])).unwrap();
        assert_eq!(pack.minecraft, "1.21.1");
        assert_eq!(pack.loader, Some(("neoforge".into(), "21.1.247".into())));
        assert_eq!(pack.files[0].path, "cf://238222/5678");
    }

    #[test]
    fn a_server_zip_carries_its_own_bytes() {
        let pack = read_bytes(zip_with(&[
            ("mods/a.jar", "jar bytes"),
            ("config/a.toml", "config"),
            ("start.sh", "ignored"),
        ]))
        .unwrap();
        assert_eq!(pack.files.len(), 2, "only mods/ and config/");
        assert!(pack.files.iter().all(|f| f.bytes.is_some()));
    }

    #[test]
    fn backups_and_per_world_state_are_not_config() {
        assert!(is_private("config/create-common.toml.bak"));
        assert!(is_private("config/jei/world/server/bookmarks.json"));
        assert!(!is_private("config/create-common.toml"));
        assert!(!is_private("config/fancymenu/customization/main.txt"));
    }
}
