//! What a mod jar says about itself: display name, description, version, and often an icon.
//! Listing one by its filename throws all of that away.

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

/// What the row draws it at, near enough - and small enough that a two hundred mod
/// pack is a megabyte of icons rather than a hundred.
const ICON_PIXELS: u32 = 64;

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
    // toml::from_str, not raw.parse(): since toml 1.0 FromStr on Value parses a single value, so
    // a whole document fails at the first "=" - which read every mods.toml as nothing.
    let parsed: toml::Value = toml::from_str(&raw).ok()?;
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
    // A jar is not a trusted source, and a decoder handed four megabytes of PNG is a decision
    // somebody else gets to make. Past this it is not an icon.
    if bytes.len() > 4 * 1024 * 1024 || !bytes.starts_with(b"\x89PNG") {
        return None;
    }

    // Scaled rather than refused: icons are routinely 512 pixels and hundreds of kilobytes, the
    // row draws them at forty. Dropping them was half a pack listing without a picture.
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes));
    reader.set_format(image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    reader.limits(limits);

    let decoded = reader.decode().ok()?;
    let scaled = if decoded.width() > ICON_PIXELS || decoded.height() > ICON_PIXELS {
        decoded.resize(
            ICON_PIXELS,
            ICON_PIXELS,
            image::imageops::FilterType::CatmullRom,
        )
    } else {
        decoded
    };

    let mut out = Vec::new();
    scaled
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;

    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&out)
    ))
}

/// Minecraft's own placeholder for a server with no icon, read from the client jar.
pub fn unknown_server_icon() -> Option<String> {
    let bytes = read_from_client_jar("assets/minecraft/textures/misc/unknown_server.png")?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

/// Read one file out of the client jar this machine already downloaded from Mojang. Textures
/// are theirs, so a public repository must not hand them out.
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

#[cfg(test)]
mod tests {
    use std::io::Write;

    /// A 512 pixel icon, the size mods really ship - written by the same encoder that reads it.
    fn big_icon() -> Vec<u8> {
        let mut image = image::RgbaImage::new(512, 512);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]);
        }
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    /// A jar shaped the way NeoForge ships them, built here so no fixture has to be checked in.
    fn jar(entry: &str, tag: &str, icon: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("unifiedmc-modinfo-{tag}.jar"));
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::SimpleFileOptions = Default::default();

        zip.start_file(entry, options).unwrap();
        zip.write_all(
            b"modLoader=\"javafml\"\nloaderVersion=\"[1,)\"\n[[mods]]\nmodId=\"testmod\"\n\
              version=\"1.2.3\"\ndisplayName=\"Test Mod\"\nlogoFile=\"icon.png\"\n\
              description=\"Renders a thing\"\n",
        )
        .unwrap();

        zip.start_file("icon.png", options).unwrap();
        zip.write_all(icon).unwrap();
        zip.finish().unwrap();
        path
    }

    fn decode_uri(uri: &str) -> image::DynamicImage {
        use base64::Engine;
        let raw = uri
            .strip_prefix("data:image/png;base64,")
            .expect("a png data uri");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(raw)
            .unwrap();
        image::load_from_memory(&bytes).unwrap()
    }

    #[test]
    fn a_neoforge_jar_gives_up_its_name_version_and_icon() {
        // The regression this exists for: toml 1.0 changed what str::parse means on Value, so
        // every jar carrying a mods.toml read as nothing at all - a list of filenames where
        // the catalogue shows names and pictures.
        for (entry, tag) in [
            ("META-INF/neoforge.mods.toml", "neoforge"),
            ("META-INF/mods.toml", "forge"),
        ] {
            let info = super::read(&jar(entry, tag, &big_icon())).expect(entry);
            assert_eq!(info.id, "testmod", "{entry}");
            assert_eq!(info.name, "Test Mod", "{entry}");
            assert_eq!(info.version, "1.2.3", "{entry}");
            assert_eq!(info.description, "Renders a thing", "{entry}");

            let icon = decode_uri(&info.icon.expect(entry));
            assert!(
                icon.width() <= super::ICON_PIXELS && icon.height() <= super::ICON_PIXELS,
                "{entry}: a 512 pixel icon has to come out at {} or less, not {}x{} - the other \
                 half of the same bug was that large icons were dropped instead of scaled",
                super::ICON_PIXELS,
                icon.width(),
                icon.height()
            );
        }
    }

    #[test]
    fn a_small_icon_is_left_at_its_own_size() {
        let mut small = image::RgbaImage::new(16, 16);
        small.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(small)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();

        let info = super::read(&jar("META-INF/mods.toml", "small", &bytes)).unwrap();
        // pixel art upscaled by a smooth filter is mush; there is nothing to gain by resizing up
        assert_eq!(decode_uri(&info.icon.unwrap()).width(), 16);
    }

    #[test]
    fn a_jar_that_declares_nothing_is_not_an_error() {
        let path = std::env::temp_dir().join("unifiedmc-modinfo-empty.jar");
        let file = std::fs::File::create(&path).unwrap();
        zip::ZipWriter::new(file).finish().unwrap();
        assert!(super::read(&path).is_none(), "not every zip is a mod");
    }
}
