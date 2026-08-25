//! Instances the player defines: version, loader, and whatever mods they add. Separate from
//! servers, which define themselves.

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
    /// A specific loader build, or none for whatever is newest at launch. Pinning matters once a
    /// pack works: a newer loader can break a mod.
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

/// A stable directory name. The id is generated, so two instances with one name never collide.
/// Tamed because it is a directory name, whatever it was typed as.
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
