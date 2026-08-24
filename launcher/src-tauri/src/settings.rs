//! What the player chose. Small enough to read and write whole every time.

use std::fs;

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Megabytes. 0 means: work it out from how large the pack is.
    pub memory: u64,
    /// Used when no signed-in session is available.
    pub offline_name: String,
    /// Port the server-side mod publishes its manifest on.
    pub manifest_port: u16,
    /// Closing the launcher while the game runs is usually not what anyone meant.
    pub keep_open: bool,
    /// Without one the catalogue is Modrinth only - no error, just fewer results.
    pub curseforge_key: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            memory: 0,
            offline_name: "Player".into(),
            manifest_port: 25566,
            keep_open: true,
            curseforge_key: String::new(),
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        fs::read_to_string(paths::settings_file())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let file = paths::settings_file();
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

/// How much heap this pack needs.
///
/// The JVM default is a quarter of physical memory, which a two hundred mod pack runs out
/// of. Scaling with the mod count tracks the actual driver. Capped at half the machine:
/// handing the game more than the system has just moves the stalling into the swap file.
pub fn heap_mb(chosen: u64, mods: usize) -> u64 {
    let total = total_memory_mb();
    let room = total.saturating_sub(2048).max(2048);

    if chosen > 0 {
        return chosen.min(room);
    }
    let ceiling = (total / 2).clamp(2048, 8192);
    (2048 + 12 * mods as u64).min(ceiling).min(room)
}

fn total_memory_mb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(info) = std::fs::read_to_string("/proc/meminfo") {
            for line in info.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    if let Some(kb) = rest.split_whitespace().next() {
                        if let Ok(kb) = kb.parse::<u64>() {
                            return kb / 1024;
                        }
                    }
                }
            }
        }
    }
    8192
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_scales_with_the_pack_and_never_exceeds_the_machine() {
        let room = total_memory_mb().saturating_sub(2048).max(2048);
        assert_eq!(heap_mb(0, 0), 2048.min(room));
        assert!(heap_mb(0, 214) > heap_mb(0, 0));
        assert!(heap_mb(0, 5000) <= room);
        assert!(
            heap_mb(999_999, 0) <= room,
            "an explicit choice is still capped"
        );
        assert_eq!(heap_mb(6144, 214), 6144.min(room));
    }
}
