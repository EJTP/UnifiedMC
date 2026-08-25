//! Picking a mod loader for a server that announces none.
//!
//! Client-side mods run against a Vanilla or Paper server perfectly well - the server never
//! sees them - so the player chooses, and this resolves that choice to a version.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Vanilla,
    Fabric,
    NeoForge,
    Forge,
    Quilt,
}

impl Kind {
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "" | "vanilla" => Some(Kind::Vanilla),
            "fabric" => Some(Kind::Fabric),
            "neoforge" => Some(Kind::NeoForge),
            "forge" => Some(Kind::Forge),
            "quilt" => Some(Kind::Quilt),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Vanilla => "vanilla",
            Kind::Fabric => "fabric",
            Kind::NeoForge => "neoforge",
            Kind::Forge => "forge",
            Kind::Quilt => "quilt",
        }
    }
}

/// Fabric alone lists 250+ loader builds; nobody scrolls that far.
const LIMIT: usize = 50;

/// Every build of `kind` for this Minecraft version, newest first. Two readers, because the
/// projects publish this in two different formats.
pub async fn versions(
    client: &reqwest::Client,
    kind: Kind,
    minecraft: &str,
) -> Result<Vec<String>> {
    match kind {
        Kind::Vanilla => Ok(Vec::new()),
        Kind::Fabric => {
            let meta = fetch_json(client, "https://meta.fabricmc.net/v2/versions/loader").await?;
            Ok(from_fabric_meta(&meta))
        }
        Kind::Quilt => {
            let meta = fetch_json(client, "https://meta.quiltmc.org/v3/versions/loader").await?;
            Ok(from_fabric_meta(&meta))
        }
        Kind::NeoForge => {
            let xml = fetch_text(
                client,
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
            )
            .await?;
            Ok(from_maven_metadata(&xml, &neoforge_prefix(minecraft)?))
        }
        Kind::Forge => {
            let xml = fetch_text(
                client,
                "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml",
            )
            .await?;
            // Forge publishes "1.21.1-52.1.16"; lyceris wants the whole thing
            Ok(from_maven_metadata(&xml, &format!("{minecraft}-")))
        }
    }
}

/// The newest build of `kind` for this Minecraft version, preferring a stable one.
pub async fn latest(client: &reqwest::Client, kind: Kind, minecraft: &str) -> Result<String> {
    if kind == Kind::Vanilla {
        return Err(anyhow!("vanilla has no loader version"));
    }
    versions(client, kind, minecraft)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| {
            anyhow!(
                "no {} build for Minecraft {minecraft} - is that version supported?",
                kind.as_str()
            )
        })
}

async fn fetch_json(client: &reqwest::Client, url: &str) -> Result<Vec<serde_json::Value>> {
    Ok(client.get(url).send().await?.json().await?)
}

async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String> {
    Ok(client.get(url).send().await?.text().await?)
}

/// Fabric and Quilt both list newest first and both mark prereleases with `"stable": false`.
/// A prerelease is still installable, so it moves to the back rather than out.
fn from_fabric_meta(meta: &[serde_json::Value]) -> Vec<String> {
    let mut entries: Vec<(bool, &str)> = meta
        .iter()
        .filter_map(|v| {
            let version = v.get("version")?.as_str()?;
            let stable = v.get("stable").and_then(serde_json::Value::as_bool) == Some(true);
            Some((!stable, version))
        })
        .collect();
    // Stable sort, so the feed's newest-first order survives within each group
    entries.sort_by_key(|(unstable, _)| *unstable);
    entries.truncate(LIMIT);
    entries.into_iter().map(|(_, v)| v.to_string()).collect()
}

/// maven-metadata lists versions oldest first, so reading it backwards gives newest first.
fn from_maven_metadata(xml: &str, prefix: &str) -> Vec<String> {
    // Split is not double-ended, so this collects before turning the order around
    let mut found: Vec<String> = xml
        .split("<version>")
        .skip(1)
        .filter_map(|chunk| chunk.split("</version>").next())
        .filter(|version| version.starts_with(prefix))
        .filter(|version| !version.contains("beta") && !version.contains("alpha"))
        .map(str::to_string)
        .collect();
    found.reverse();
    found.truncate(LIMIT);
    found
}

/// NeoForge numbers itself after the Minecraft it targets: 1.21.1 becomes 21.1.x, 1.21 is 21.0.x.
fn neoforge_prefix(minecraft: &str) -> Result<String> {
    let mut parts = minecraft.split('.');
    let (Some("1"), Some(major)) = (parts.next(), parts.next()) else {
        return Err(anyhow!("cannot map {minecraft} to a NeoForge version"));
    };
    let minor = parts.next().unwrap_or("0");
    Ok(format!("{major}.{minor}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_numbers_itself_after_its_minecraft() {
        assert_eq!(neoforge_prefix("1.21.1").unwrap(), "21.1.");
        assert_eq!(neoforge_prefix("1.21").unwrap(), "21.0.");
        assert_eq!(neoforge_prefix("1.20.4").unwrap(), "20.4.");
        assert!(neoforge_prefix("26.2").is_err(), "not a 1.x version");
    }

    fn fabric_entry(version: &str, stable: bool) -> serde_json::Value {
        serde_json::json!({ "version": version, "stable": stable })
    }

    #[test]
    fn prereleases_sink_below_stable_builds_without_being_dropped() {
        let meta = [
            fabric_entry("0.17.3", false),
            fabric_entry("0.17.2", true),
            fabric_entry("0.17.1", false),
            fabric_entry("0.17.0", true),
        ];
        assert_eq!(
            from_fabric_meta(&meta),
            ["0.17.2", "0.17.0", "0.17.3", "0.17.1"]
        );
    }

    #[test]
    fn fabric_list_is_capped() {
        let meta: Vec<_> = (0..251)
            .map(|n| fabric_entry(&format!("0.1.{n}"), true))
            .collect();
        let picked = from_fabric_meta(&meta);
        assert_eq!(picked.len(), LIMIT);
        assert_eq!(picked[0], "0.1.0", "feed order is newest first, keep it");
    }

    #[test]
    fn maven_metadata_is_filtered_by_prefix_and_turned_around() {
        let xml = "<versions>\
            <version>1.20.4-49.0.0</version>\
            <version>1.21.1-52.0.0</version>\
            <version>1.21.1-52.1.16</version>\
            <version>1.21.1-53.0.0-beta</version>\
            <version>1.21.4-54.0.0</version>\
        </versions>";
        assert_eq!(
            from_maven_metadata(xml, "1.21.1-"),
            ["1.21.1-52.1.16", "1.21.1-52.0.0"]
        );
        assert!(from_maven_metadata(xml, "1.99.9-").is_empty());
    }

    #[test]
    fn maven_metadata_is_capped() {
        let xml: String = (0..80)
            .map(|n| format!("<version>21.1.{n}</version>"))
            .collect();
        let picked = from_maven_metadata(&xml, "21.1.");
        assert_eq!(picked.len(), LIMIT);
        assert_eq!(picked[0], "21.1.79", "newest first");
    }

    #[test]
    fn loader_names_round_trip() {
        for name in ["fabric", "neoforge", "forge", "quilt", "vanilla"] {
            assert_eq!(Kind::parse(name).unwrap().as_str(), name);
        }
        assert_eq!(Kind::parse("").unwrap(), Kind::Vanilla);
        assert_eq!(Kind::parse("NeoForge").unwrap(), Kind::NeoForge);
        assert!(Kind::parse("liteloader").is_none());
    }
}
