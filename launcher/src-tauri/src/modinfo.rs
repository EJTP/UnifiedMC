//! What a mod jar says about itself.
//!
//! A jar is not an opaque file: it carries a display name, a description, a version and often
//! an icon. Listing one as its filename throws all of that away and makes a player's own mods
//! look worse than the catalogue's.

use std::io::Read;
use std::path::Path;

use base64::Engine;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ModInfo {
    /// The id the mod registers itself under. What actually decides a duplicate.
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    /// A data: URI, because the file lives inside the jar and nothing else can reach it.
    pub icon: Option<String>,
}

pub fn read(path: &Path) -> Option<ModInfo> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    let names: Vec<String> = archive.file_names().map(str::to_string).collect();
    let mut info = if names.iter().any(|n| n == "META-INF/neoforge.mods.toml") {
        from_toml(&mut archive, "META-INF/neoforge.mods.toml")
    } else if names.iter().any(|n| n == "META-INF/mods.toml") {
        from_toml(&mut archive, "META-INF/mods.toml")
    } else if names.iter().any(|n| n == "fabric.mod.json") {
        from_fabric(&mut archive)
    } else {
        None
    }?;

    if info.name.is_empty() {
        info.name = path.file_name()?.to_string_lossy().into_owned();
    }
    if let Some(logo) = info.icon.take() {
        info.icon = embed_icon(&mut archive, &logo);
    }
    Some(info)
}

fn entry<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    let mut file = archive.by_name(name).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn from_toml<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<ModInfo> {
    let raw = String::from_utf8(entry(archive, name)?).ok()?;
    let parsed: toml::Value = raw.parse().ok()?;
    let first = parsed.get("mods")?.as_array()?.first()?;

    Some(ModInfo {
        id: string(first, "modId"),
        name: string(first, "displayName"),
        description: string(first, "description").trim().replace('\n', " "),
        version: string(first, "version"),
        // logoFile sits on the mod or at the top level, depending on who wrote the file
        icon: Some(string(first, "logoFile"))
            .filter(|s| !s.is_empty())
            .or_else(|| Some(string(&parsed, "logoFile")).filter(|s| !s.is_empty())),
    })
}

fn from_fabric<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Option<ModInfo> {
    let parsed: serde_json::Value =
        serde_json::from_slice(&entry(archive, "fabric.mod.json")?).ok()?;

    Some(ModInfo {
        id: parsed.get("id")?.as_str()?.to_string(),
        name: parsed
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        description: parsed
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .replace('\n', " "),
        version: parsed
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        icon: parsed
            .get("icon")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

fn string(value: &toml::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Icons live inside the jar, so the only way to hand one to a web view is to carry it along.
fn embed_icon<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<String> {
    // The webview only decodes what it decodes; a jar can hold anything.
    if !name.to_lowercase().ends_with(".png") {
        return None;
    }
    let bytes = entry(archive, name)?;
    // 256 KB of base64 in a list of forty rows is not worth the memory
    if bytes.len() > 256 * 1024 || !bytes.starts_with(b"\x89PNG") {
        return None;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

/// Minecraft's own placeholder for a server with no icon.
///
/// Read out of the client jar the launcher already downloaded from Mojang, rather than shipped
/// with us: the texture is theirs, and a public repository handing it out is redistributing a
/// game file. Every player has it legitimately through their own copy - so use that one.
pub fn unknown_server_icon() -> Option<String> {
    let bytes = read_from_client_jar("assets/minecraft/textures/misc/unknown_server.png")?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

/// Read one file out of the client jar this machine already downloaded from Mojang.
///
/// Textures are theirs; a public repository handing them out is redistributing a game file.
/// Every player has them legitimately through their own copy, so use that one.
pub fn read_from_client_jar(path: &str) -> Option<Vec<u8>> {
    let versions = crate::paths::minecraft().join("versions");
    let mut newest: Option<std::path::PathBuf> = None;

    for entry in std::fs::read_dir(versions).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // fabric-loader-x-1.21.1 and the like inherit their assets; only the plain ones have a jar
        let jar = entry.path().join(format!("{name}.jar"));
        if !jar.is_file() {
            continue;
        }
        let newer = newest
            .as_ref()
            .and_then(|current| Some(jar.metadata().ok()?.len() > current.metadata().ok()?.len()))
            .unwrap_or(true);
        if newer {
            newest = Some(jar);
        }
    }

    let file = std::fs::File::open(newest?).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut wanted = archive.by_name(path).ok()?;

    let mut bytes = Vec::new();
    wanted.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}
