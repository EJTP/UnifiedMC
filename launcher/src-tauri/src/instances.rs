//! Instances that exist on their own.
//!
//! A server's instance is defined by the server: it says which version and which mods, and
//! the launcher follows. An instance here is defined by the player instead - they pick the
//! version and loader, add what they like, and play singleplayer or join anything compatible.
//!
//! Kept in its own file rather than as a server with no address. The two are different
//! things, and storing them as one made every function ask "but is it really".

use std::fs;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub minecraft: String,
    #[serde(default)]
    pub loader: Option<String>,
    /// A specific loader build, or none for whatever is newest at launch.
    ///
    /// Pinning matters once a pack works: a newer loader can break a mod, and "it ran
    /// yesterday" is not a debugging tool.
    #[serde(default)]
    pub loader_version: Option<String>,
    /// Where it came from, when it was imported rather than made by hand.
    #[serde(default)]
    pub source: Option<String>,
}

fn file() -> std::path::PathBuf {
    paths::data().join("instances.json")
}

pub fn load() -> Vec<Instance> {
    fs::read_to_string(file())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(instances: &[Instance]) -> Result<()> {
    let path = file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(instances)?)?;
    Ok(())
}

/// A stable directory name. The id is generated, so this never collides with another instance
/// even when two carry the same name.
/// The pieces are player-typed today, but this is a directory name either way, and the day an
/// instance becomes importable it stops being player-typed without anyone noticing.
pub fn key(instance: &Instance) -> String {
    use crate::servers::tame;
    let loader = instance
        .loader
        .as_deref()
        .map(|l| format!("-{}", tame(l)))
        .unwrap_or_default();
    format!(
        "instance-{}-{}{}",
        tame(&instance.id),
        tame(&instance.minecraft),
        loader
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_instances_with_one_name_do_not_share_a_directory() {
        let first = Instance {
            id: "a1".into(),
            name: "Test".into(),
            minecraft: "1.21.1".into(),
            loader: Some("fabric".into()),
            loader_version: None,
            source: None,
        };
        let second = Instance {
            id: "b2".into(),
            ..first.clone()
        };
        assert_ne!(key(&first), key(&second));
        assert!(key(&first).contains("1.21.1"));
        assert!(key(&first).contains("fabric"));
        assert_eq!(key(&first), "instance-a1-1.21.1-fabric");
    }

    #[test]
    fn an_instance_key_is_a_directory_name_not_a_path() {
        let hostile = Instance {
            id: "../..".into(),
            name: "Test".into(),
            minecraft: "../../../etc".into(),
            loader: Some("a/b".into()),
            loader_version: None,
            source: None,
        };
        let key = key(&hostile);
        assert!(!key.contains(".."), "{key}");
        assert!(!key.contains('/'), "{key}");
    }
}
