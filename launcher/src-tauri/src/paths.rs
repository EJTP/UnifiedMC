//! Where everything lives. Mods are stored by hash and shared across servers; instances are
//! per server. The same layout the Python shell uses.

use std::path::PathBuf;

pub fn data() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".unifiedmc")
}

pub fn blobs() -> PathBuf {
    data().join("blobs")
}

/// Shared assets, libraries, versions and runtimes. One copy for every instance.
pub fn minecraft() -> PathBuf {
    data().join("mc")
}

pub fn instances() -> PathBuf {
    data().join("instances")
}

pub fn instance(key: &str) -> PathBuf {
    instances().join(key)
}

pub fn profiles() -> PathBuf {
    data().join("profiles")
}

pub fn settings_file() -> PathBuf {
    data().join("launcher.json")
}

pub fn servers_file() -> PathBuf {
    data().join("launcher-servers.json")
}

/// Written by the hub mod when it is started from a launcher that is already signed in.
pub fn session_file() -> PathBuf {
    data().join("session.json")
}
