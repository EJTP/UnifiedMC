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
    /// Closing the launcher while the game runs is usually not what anyone meant.
    pub keep_open: bool,
    /// "balanced", "throughput", or anything else for the JVM's own defaults.
    pub jvm_profile: String,
    /// Only used when jvm_profile is "custom".
    pub jvm_args: String,
    /// "system", "de" or "en". "system" follows the browser locale the webview reports.
    pub language: String,
    /// Which accent the window is painted in. The names live in the frontend, which is the
    /// only thing that has any use for them; this only has to remember the choice.
    pub accent: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            memory: 0,
            offline_name: "Player".into(),
            keep_open: true,
            jvm_profile: "balanced".into(),
            jvm_args: String::new(),
            language: "system".into(),
            accent: "violet".into(),
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
        fs::write(&file, serde_json::to_vec_pretty(self)?)?;
        // The umask is 0022 on most boxes, which would leave this readable by every account.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&file, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

/// The CurseForge key this build was compiled with, if it was given one.
///
/// A credential, so never written down here: `option_env!` reads it at build time. A build
/// without one searches Modrinth only. It is extractable from the binary either way, which is
/// why CurseForge issues per-application keys they can revoke.
const BUILT_IN_CURSEFORGE_KEY: Option<&str> = option_env!("UNIFIEDMC_CF_KEY");

pub fn curseforge_key() -> &'static str {
    BUILT_IN_CURSEFORGE_KEY.unwrap_or_default()
}

/// How much memory this machine has, so a picker cannot offer more than exists.
pub fn machine_memory_mb() -> u64 {
    total_memory_mb()
}

/// JVM flags as a choice rather than a text box. The default collector pauses to collect, and
/// a pause during a chunk load is a stutter; both profiles move that work off the pause.
pub fn jvm_args(settings: &Settings, heap_mb: u64) -> Vec<String> {
    match settings.jvm_profile.as_str() {
        // G1 tuned the way large packs are run: small regions, collect early, short pauses.
        "balanced" => vec![
            // Must come first: G1NewSizePercent is experimental, and a JVM handed one without this
            // refuses to start - no window, no crash report.
            "-XX:+UnlockExperimentalVMOptions".into(),
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
        // ZGC collects concurrently, at the cost of cores and memory. Worth it above 8 GB.
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

/// How much heap this pack needs. The JVM default is a quarter of physical memory, which a two
/// hundred mod pack runs out of. Capped at half the machine, or the stalling moves into swap.
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

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

        let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        // SAFETY: dwLength is the struct's own size, which is the whole contract of this call.
        if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 {
            return status.ullTotalPhys / (1024 * 1024);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let mut bytes: u64 = 0;
        let mut size = std::mem::size_of::<u64>();
        // SAFETY: hw.memsize is a u64, and size says how much room the answer has.
        let read = unsafe {
            libc::sysctlbyname(
                c"hw.memsize".as_ptr(),
                (&mut bytes as *mut u64).cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if read == 0 && bytes > 0 {
            return bytes / (1024 * 1024);
        }
    }

    // Every platform has its own way of being asked, and this is what is left when the answer
    // does not come. Small on purpose: a machine that cannot say is not one to hand 16 GB to.
    8192
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every option the JVM calls experimental. Adding one to a profile without the unlock
    /// flag in front of it is the difference between a game that starts and one that does not.
    const EXPERIMENTAL: &[&str] = &["G1NewSizePercent", "G1MaxNewSizePercent"];

    #[test]
    fn an_experimental_flag_is_always_unlocked_first() {
        for profile in ["balanced", "throughput", "default"] {
            let settings = Settings {
                jvm_profile: profile.into(),
                ..Default::default()
            };
            for heap in [2048, 8192] {
                let flags = jvm_args(&settings, heap);
                let experimental = flags
                    .iter()
                    .position(|f| EXPERIMENTAL.iter().any(|e| f.contains(e)));
                let Some(at) = experimental else { continue };
                let unlock = flags
                    .iter()
                    .position(|f| f == "-XX:+UnlockExperimentalVMOptions");
                assert!(
                    unlock.is_some_and(|u| u < at),
                    "{profile} at {heap} MB passes an experimental option without unlocking it \
                     first, and the JVM refuses to start: {flags:?}"
                );
            }
        }
    }

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
