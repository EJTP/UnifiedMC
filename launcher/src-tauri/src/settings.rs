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
    /// "balanced", "throughput", or anything else for the JVM's own defaults.
    pub jvm_profile: String,
    /// Only used when jvm_profile is "custom".
    pub jvm_args: String,
    /// "system", "de" or "en". "system" follows the browser locale the webview reports.
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            memory: 0,
            offline_name: "Player".into(),
            manifest_port: 25566,
            keep_open: true,
            curseforge_key: String::new(),
            jvm_profile: "balanced".into(),
            jvm_args: String::new(),
            language: "system".into(),
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

/// How much memory this machine has, so a picker cannot offer more than exists.
pub fn machine_memory_mb() -> u64 {
    total_memory_mb()
}

/// JVM flags, as a choice rather than a text box nobody knows how to fill.
///
/// The default collector pauses to collect, and a pause during a chunk load is a stutter. Both
/// profiles here move that work off the pause; which one suits depends on whether the machine
/// has cores to spare.
pub fn jvm_args(settings: &Settings, heap_mb: u64) -> Vec<String> {
    match settings.jvm_profile.as_str() {
        // G1 tuned the way large modpacks are usually run: small regions, collect early,
        // never let a pause grow long enough to see.
        "balanced" => vec![
            "-XX:+UseG1GC".into(),
            "-XX:MaxGCPauseMillis=37".into(),
            "-XX:G1HeapRegionSize=16M".into(),
            "-XX:G1NewSizePercent=23".into(),
            "-XX:G1ReservePercent=20".into(),
            "-XX:InitiatingHeapOccupancyPercent=20".into(),
            "-XX:+ParallelRefProcEnabled".into(),
            "-XX:+PerfDisableSharedMem".into(),
            "-XX:+AlwaysPreTouch".into(),
        ],
        // ZGC collects concurrently, so pauses stay flat as the heap grows - at the cost of
        // cores and memory. Worth it above 8 GB on a machine that has them.
        "throughput" if heap_mb >= 6144 => vec![
            "-XX:+UseZGC".into(),
            "-XX:+ZGenerational".into(),
            "-XX:+AlwaysPreTouch".into(),
        ],
        "throughput" => vec!["-XX:+UseG1GC".into(), "-XX:+AlwaysPreTouch".into()],
        "custom" => settings
            .jvm_args
            .split_whitespace()
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
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
    fn a_profile_is_flags_and_custom_is_whatever_was_typed() {
        let balanced = Settings::default();
        assert!(jvm_args(&balanced, 4096)
            .iter()
            .any(|f| f.contains("UseG1GC")));

        let custom = Settings {
            jvm_profile: "custom".into(),
            jvm_args: "-Xss2M  -Dfoo=bar".into(),
            ..Default::default()
        };
        assert_eq!(jvm_args(&custom, 4096), vec!["-Xss2M", "-Dfoo=bar"]);

        let none = Settings {
            jvm_profile: "default".into(),
            ..Default::default()
        };
        assert!(jvm_args(&none, 4096).is_empty(), "the JVM's own defaults");

        // ZGC wants room to work; below that it would collect more often, not less
        let throughput = Settings {
            jvm_profile: "throughput".into(),
            ..Default::default()
        };
        assert!(jvm_args(&throughput, 8192)
            .iter()
            .any(|f| f.contains("UseZGC")));
        assert!(!jvm_args(&throughput, 2048)
            .iter()
            .any(|f| f.contains("UseZGC")));
    }

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
