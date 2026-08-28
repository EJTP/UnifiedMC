//! Whether there is a newer release than the one running.
//!
//! GitHub's releases API, not an update server: the release workflow already publishes every
//! installer to a v-tag, so the tag is the version and the release page is the download. What
//! this does not do is replace the running binary - that needs a signing key, a manifest
//! published per platform, and a way to roll back a bad one.

use anyhow::Result;
use serde::Serialize;

const LATEST: &str = "https://api.github.com/repos/EJTP/UnifiedMC/releases/latest";

/// The manifest the updater plugin installs from. The same url `tauri.conf.json` points it at,
/// because this has to answer the question the plugin is about to ask.
const MANIFEST: &str = "https://github.com/EJTP/UnifiedMC/releases/latest/download/latest.json";

#[derive(Clone, Debug, Serialize)]
pub struct Release {
    /// What is running, out of Cargo.toml at compile time.
    pub current: String,
    pub latest: String,
    pub url: String,
    pub notes: String,
}

/// What this build calls itself in the update manifest.
///
/// Must match the keys `.github/scripts/latest_json.py` writes, or an update is never offered
/// to anybody. A target we do not publish for answers None, and is then told about nothing -
/// which is the honest answer, since there would be nothing to install.
fn platform_key() -> Option<String> {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return None;
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        return None;
    };
    Some(format!("{os}-{arch}"))
}

/// Whether the manifest is up and actually offers this platform a build of `version`.
///
/// The release is tagged minutes before its manifest is uploaded - the three platform builds
/// have to finish first. In that window the tag says there is a new version and the updater
/// has nothing to read, so pressing the button fails on a 404 that parses as "not valid JSON".
/// Nobody is told about an update that cannot be installed yet.
async fn installable(client: &reqwest::Client, version: &str) -> bool {
    let Some(key) = platform_key() else {
        return false;
    };
    let Ok(response) = client.get(MANIFEST).send().await else {
        return false;
    };
    let Ok(manifest) = response.error_for_status() else {
        return false;
    };
    let Ok(manifest) = manifest.json::<serde_json::Value>().await else {
        return false;
    };
    offers(&manifest, version, &key)
}

/// The manifest is for this very version, and has an entry for this platform.
///
/// Both halves matter: a manifest left over from a build that only half succeeded can name an
/// older version, and one that names the right version can still be missing the platform whose
/// job failed.
fn offers(manifest: &serde_json::Value, version: &str, key: &str) -> bool {
    if manifest.get("version").and_then(|v| v.as_str()) != Some(version) {
        return false;
    }
    manifest
        .get("platforms")
        .and_then(|p| p.get(key))
        .and_then(|entry| entry.get("url"))
        .and_then(|url| url.as_str())
        .is_some_and(|url| !url.is_empty())
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

    // Asked before anything is shown, not when the button is pressed: an update badge that
    // leads to an error is worse than no badge.
    if !installable(client, latest).await {
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
    fn nothing_is_offered_until_the_manifest_can_actually_install_it() {
        let key = "linux-x86_64";
        let ready = serde_json::json!({
            "version": "0.1.10",
            "platforms": { "linux-x86_64": { "url": "https://x/a.AppImage", "signature": "s" } }
        });
        assert!(offers(&ready, "0.1.10", key));

        // The window this exists for: the tag is up, the manifest job has not run yet.
        assert!(!offers(&serde_json::json!({}), "0.1.10", key));

        // A manifest left behind by the previous release.
        assert!(!offers(&ready, "0.1.11", key));

        // Built, but this platform's job failed, so there is nothing for us in it.
        let partial = serde_json::json!({
            "version": "0.1.10",
            "platforms": { "windows-x86_64": { "url": "https://x/a.exe", "signature": "s" } }
        });
        assert!(!offers(&partial, "0.1.10", key));

        // Present but empty is not an offer either.
        let empty = serde_json::json!({
            "version": "0.1.10",
            "platforms": { "linux-x86_64": { "url": "", "signature": "s" } }
        });
        assert!(!offers(&empty, "0.1.10", key));
    }

    #[test]
    fn this_build_knows_which_manifest_entry_is_its_own() {
        let key = platform_key().expect("a target we publish for");
        // The four the manifest generator writes; anything else would never match.
        assert!(
            [
                "linux-x86_64",
                "darwin-x86_64",
                "darwin-aarch64",
                "windows-x86_64"
            ]
            .contains(&key.as_str()),
            "{key} is not a key latest_json.py writes"
        );
    }

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
}
