//! Whether there is a newer release than the one running.
//!
//! GitHub's releases API, not an update server: the release workflow already publishes every
//! installer to a v-tag, so the tag is the version and the release page is the download. What
//! this does not do is replace the running binary - that needs a signing key, a manifest
//! published per platform, and a way to roll back a bad one.

use anyhow::Result;
use serde::Serialize;

const LATEST: &str = "https://api.github.com/repos/EJTP/UnifiedMC/releases/latest";

#[derive(Clone, Debug, Serialize)]
pub struct Release {
    /// What is running, out of Cargo.toml at compile time.
    pub current: String,
    pub latest: String,
    pub url: String,
    pub notes: String,
    /// The installer for this platform, when the release has one.
    pub download: Option<String>,
}

/// The newer release, or none - including when GitHub cannot be reached, which is not an error
/// worth putting on the screen.
pub async fn check(client: &reqwest::Client) -> Option<Release> {
    let current = env!("CARGO_PKG_VERSION");
    let release: serde_json::Value = client
        .get(LATEST)
        // GitHub refuses a request with no User-Agent; the shared client already sets one.
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;

    if release.get("draft").and_then(serde_json::Value::as_bool) == Some(true)
        || release
            .get("prerelease")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    {
        return None;
    }

    let tag = release.get("tag_name")?.as_str()?;
    let latest = tag.trim_start_matches('v');
    if !newer(latest, current) {
        return None;
    }

    Some(Release {
        current: current.to_string(),
        latest: latest.to_string(),
        url: release
            .get("html_url")
            .and_then(|v| v.as_str())
            .unwrap_or("https://github.com/EJTP/UnifiedMC/releases/latest")
            .to_string(),
        notes: release
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        download: asset_for_this_platform(&release),
    })
}

/// Which of the release's files this machine can actually run.
fn asset_for_this_platform(release: &serde_json::Value) -> Option<String> {
    let wanted: &[&str] = if cfg!(windows) {
        // NSIS over WiX: the .exe is the one a person double-clicks.
        &["-setup.exe", ".msi"]
    } else if cfg!(target_os = "macos") {
        &[".dmg", ".app.tar.gz"]
    } else {
        &[".AppImage", ".deb", ".rpm"]
    };

    let assets = release.get("assets")?.as_array()?;
    // In preference order, so a release carrying both formats hands over the better one.
    wanted.iter().find_map(|suffix| {
        assets.iter().find_map(|asset| {
            let name = asset.get("name")?.as_str()?;
            name.ends_with(suffix)
                .then(|| asset.get("browser_download_url")?.as_str())
                .flatten()
                .map(str::to_string)
        })
    })
}

/// Compare two dotted versions numerically.
///
/// String order says 0.1.10 is older than 0.1.9, which would leave everyone on the release
/// before the one where the numbers rolled over.
fn newer(candidate: &str, current: &str) -> bool {
    parts(candidate) > parts(current)
}

fn parts(version: &str) -> Vec<u64> {
    version
        // a pre-release suffix is not part of the ordering; the check refuses prereleases anyway
        .split(['-', '+'])
        .next()
        .unwrap_or(version)
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// Open the release page in the player's browser. Downloading and swapping the running binary
/// is what an updater plugin is for; this is the honest half of one.
pub fn open(app: &tauri::AppHandle, url: &str) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(url, None::<&str>)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_compared_as_numbers_not_as_text() {
        assert!(newer("0.1.8", "0.1.7"));
        assert!(!newer("0.1.7", "0.1.7"));
        assert!(!newer("0.1.6", "0.1.7"));
        // the one string order gets wrong
        assert!(newer("0.1.10", "0.1.9"));
        assert!(!newer("0.1.9", "0.1.10"));
        assert!(newer("0.2.0", "0.1.99"));
        assert!(newer("1.0", "0.9.9"));
        // a tag that is not a version must not read as an upgrade
        assert!(!newer("nightly", "0.1.7"));
    }

    #[test]
    fn the_offered_download_is_one_this_machine_can_run() {
        let release = serde_json::json!({
            "assets": [
                { "name": "UnifiedMC_0.1.8_amd64.deb", "browser_download_url": "https://x/deb" },
                { "name": "UnifiedMC_0.1.8_x64-setup.exe", "browser_download_url": "https://x/exe" },
                { "name": "UnifiedMC_0.1.8_x64_en-US.msi", "browser_download_url": "https://x/msi" },
                { "name": "UnifiedMC_0.1.8.AppImage", "browser_download_url": "https://x/appimage" }
            ]
        });
        let got = asset_for_this_platform(&release).unwrap();
        if cfg!(windows) {
            assert_eq!(got, "https://x/exe", "the double-clickable one wins");
        } else if cfg!(target_os = "linux") {
            assert_eq!(got, "https://x/appimage");
        }

        // A release with only the CLI attached offers nothing rather than the wrong file.
        let cli_only = serde_json::json!({
            "assets": [{ "name": "unifiedmc-server-cli.exe", "browser_download_url": "https://x/cli" }]
        });
        assert_eq!(asset_for_this_platform(&cli_only), None);
    }
}
