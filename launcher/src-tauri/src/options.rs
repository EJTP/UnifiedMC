//! Sharing Minecraft's own settings between instances, so a new server does not start with
//! default keybindings and full volume.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;

/// Lines that belong to the instance rather than the player.
const INSTANCE_LOCAL: [&str; 6] = [
    "resourcePacks",
    "incompatibleResourcePacks",
    "lastServer",
    "hideServerAddress",
    "joinedFirstServer",
    "allowServerListing",
];

fn shared_file() -> std::path::PathBuf {
    crate::paths::data().join("shared-options.txt")
}

fn is_instance_local(key: &str) -> bool {
    INSTANCE_LOCAL.contains(&key)
}

/// options.txt is `key:value`, one per line, and values contain colons - so split once.
fn parse(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

fn render(settings: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (key, value) in settings {
        out.push_str(key);
        out.push(':');
        out.push_str(value);
        out.push('\n');
    }
    out
}

/// Put the player's settings into an instance, keeping what belongs to that instance.
pub fn apply(instance: &Path) -> Result<()> {
    let shared = shared_file();
    let Ok(text) = std::fs::read_to_string(&shared) else {
        return Ok(()); // nothing shared yet; this instance defines it when it exits
    };

    let target = instance.join("options.txt");
    let mut merged = parse(&text);
    merged.retain(|key, _| !is_instance_local(key));

    if let Ok(existing) = std::fs::read_to_string(&target) {
        for (key, value) in parse(&existing) {
            if is_instance_local(&key) {
                merged.insert(key, value);
            }
        }
    }

    std::fs::create_dir_all(instance)?;
    std::fs::write(target, render(&merged))?;
    Ok(())
}

/// Take what the player changed back out, so the next instance starts with it.
pub fn collect(instance: &Path) -> Result<()> {
    let Ok(text) = std::fs::read_to_string(instance.join("options.txt")) else {
        return Ok(()); // the game never got far enough to write one
    };

    let mut settings = parse(&text);
    settings.retain(|key, _| !is_instance_local(key));

    let shared = shared_file();
    if let Some(parent) = shared.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // written then renamed: a crash mid-write must not leave everyone with half a settings file
    let temporary = shared.with_extension("part");
    std::fs::write(&temporary, render(&settings))?;
    std::fs::rename(temporary, shared)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_containing_colons_survive_a_round_trip() {
        // a key binding is "key_key.attack:key.mouse.left" - three colons in one line
        let text = "key_key.attack:key.mouse.left\nrenderDistance:16\n";
        let parsed = parse(text);
        assert_eq!(parsed.get("key_key.attack").unwrap(), "key.mouse.left");
        assert_eq!(parse(&render(&parsed)), parsed);
    }

    #[test]
    fn what_belongs_to_the_instance_stays_there() {
        assert!(is_instance_local("resourcePacks"));
        assert!(is_instance_local("lastServer"));
        assert!(!is_instance_local("renderDistance"));
        assert!(!is_instance_local("key_key.attack"));
    }

    #[test]
    fn applying_keeps_the_instance_own_packs() {
        let dir = std::env::temp_dir().join("unifiedmc-options-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            dir.join("options.txt"),
            "resourcePacks:[\"only-here\"]\nrenderDistance:2\n",
        )
        .unwrap();

        // stand in for the shared file without touching the real one
        let mut shared = parse("renderDistance:16\nresourcePacks:[\"elsewhere\"]\n");
        shared.retain(|key, _| !is_instance_local(key));
        let mut merged = shared;
        for (key, value) in parse(&std::fs::read_to_string(dir.join("options.txt")).unwrap()) {
            if is_instance_local(&key) {
                merged.insert(key, value);
            }
        }

        assert_eq!(
            merged.get("renderDistance").unwrap(),
            "16",
            "player's setting wins"
        );
        assert_eq!(
            merged.get("resourcePacks").unwrap(),
            "[\"only-here\"]",
            "packs from another instance would point at files that are not here"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
