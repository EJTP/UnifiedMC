//! Finding mods a single player can add on top of what the server already sends.
//!
//! Two catalogues, because plenty of mods live on only one of them. Both are asked the same
//! two questions: does this run without the server carrying it too, and does the server
//! already ship it.

use std::collections::HashSet;

use anyhow::{anyhow, Result};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::servers::{Manifest, ModEntry};

const MODRINTH: &str = "https://api.modrinth.com/v2";
const CURSEFORGE: &str = "https://api.curseforge.com/v1";

const CF_GAME_MINECRAFT: u32 = 432;
const CF_CLASS_MOD: u32 = 6;
const CF_SORT_DOWNLOADS: u32 = 6;
const CF_REQUIRED_DEPENDENCY: u32 = 3;
const CF_SHA1: u32 = 1;
const CF_RELEASE: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hit {
    pub id: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
    pub source: String,
    /// The server already sends this one. Shown, not hidden - hiding it does not answer the
    /// question somebody has when they type "sodium".
    pub on_server: bool,
    pub icon: Option<String>,
}

/// Can one player add this and have it work?
///
/// Only if the server does not have to carry it too. A backpack or a new block is
/// `server_side: required`; in a personal profile it does nothing at best. Whether the server
/// should have it is the server owner's decision, not a browser's.
fn modrinth_verdict(project: &serde_json::Value) -> &'static str {
    let client = project.get("client_side").and_then(|v| v.as_str());
    let server = project.get("server_side").and_then(|v| v.as_str());

    if client == Some("unsupported") || server == Some("required") {
        return "no";
    }
    match (client, server) {
        (Some("unknown"), _) | (_, Some("unknown")) | (None, _) | (_, None) => "unknown",
        _ => "yes",
    }
}

/// CurseForge publishes it too, just not where you would look: a file's `gameVersions` array
/// carries "Client" and "Server" beside the version and loader.
fn cf_tags(files: &[serde_json::Value], minecraft: &str, loader: &str) -> HashSet<String> {
    let mut tags = HashSet::new();
    for file in files {
        let Some(versions) = file.get("gameVersions").and_then(|v| v.as_array()) else {
            continue;
        };
        let names: Vec<&str> = versions.iter().filter_map(|v| v.as_str()).collect();
        let right_version = names.contains(&minecraft);
        let right_loader =
            loader.is_empty() || names.iter().any(|n| n.eq_ignore_ascii_case(loader));
        if right_version && right_loader {
            for name in names {
                if name == "Client" || name == "Server" {
                    tags.insert(name.to_string());
                }
            }
        }
    }
    tags
}

/// 32-bit MurmurHash2, the variant CurseForge fingerprints files with.
pub fn murmur2(data: &[u8], seed: u32) -> u32 {
    const M: u32 = 0x5BD1_E995;
    const R: u32 = 24;

    let mut length = data.len();
    let mut h = seed ^ (length as u32);
    let mut i = 0;

    while length >= 4 {
        let mut k = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        k = k.wrapping_mul(M);
        k ^= k >> R;
        k = k.wrapping_mul(M);
        h = h.wrapping_mul(M);
        h ^= k;
        i += 4;
        length -= 4;
    }
    if length == 3 {
        h ^= (data[i + 2] as u32) << 16;
    }
    if length >= 2 {
        h ^= (data[i + 1] as u32) << 8;
    }
    if length >= 1 {
        h ^= data[i] as u32;
        h = h.wrapping_mul(M);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^= h >> 15;
    h
}

/// CurseForge hashes the file with tabs, newlines, carriage returns and spaces removed.
pub fn fingerprint(bytes: &[u8]) -> u32 {
    let stripped: Vec<u8> = bytes
        .iter()
        .copied()
        .filter(|b| !matches!(b, 9 | 10 | 13 | 32))
        .collect();
    murmur2(&stripped, 1)
}

pub struct Catalogue<'a> {
    pub client: &'a reqwest::Client,
    pub cf_key: &'a str,
}

impl Catalogue<'_> {
    async fn modrinth<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        Ok(self
            .client
            .get(format!("{MODRINTH}{path}"))
            .query(query)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn cf_post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        if self.cf_key.is_empty() {
            return Err(anyhow!("no CurseForge key"));
        }
        let response: serde_json::Value = self
            .client
            .post(format!("{CURSEFORGE}{path}"))
            .header("x-api-key", self.cf_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    async fn cf_get(&self, path: &str, query: &[(&str, String)]) -> Result<serde_json::Value> {
        if self.cf_key.is_empty() {
            return Err(anyhow!("no CurseForge key"));
        }
        let response: serde_json::Value = self
            .client
            .get(format!("{CURSEFORGE}{path}"))
            .header("x-api-key", self.cf_key)
            .query(query)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Which Modrinth projects the server already ships, resolved from the file hashes.
    /// One batch lookup instead of guessing from names.
    async fn served_modrinth(&self, manifest: &Manifest) -> HashSet<String> {
        let hashes: Vec<&str> = manifest.mods.iter().map(|m| m.sha1.as_str()).collect();
        if hashes.is_empty() {
            return HashSet::new();
        }
        let body = json!({ "hashes": hashes, "algorithm": "sha1" });
        let Ok(response) = self
            .client
            .post(format!("{MODRINTH}/version_files"))
            .json(&body)
            .send()
            .await
        else {
            return HashSet::new();
        };
        let Ok(found) = response.json::<serde_json::Value>().await else {
            return HashSet::new();
        };
        found
            .as_object()
            .map(|map| {
                map.values()
                    .filter_map(|v| v.get("project_id")?.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The same question for CurseForge, which has no hash lookup - only its own fingerprint.
    async fn served_curseforge(&self, manifest: &Manifest) -> HashSet<String> {
        let mut prints = Vec::new();
        let mut missing = 0usize;
        for entry in &manifest.mods {
            match std::fs::read(crate::paths::blobs().join(&entry.sha1)) {
                Ok(bytes) => prints.push(fingerprint(&bytes)),
                Err(_) => missing += 1,
            }
        }
        if prints.is_empty() {
            return HashSet::new();
        }
        // A partial answer must not be remembered: the browser would keep offering mods the
        // pack already contains. Nothing is cached here for exactly that reason.
        let _ = missing;

        let Ok(data) = self
            .cf_post("/fingerprints", json!({ "fingerprints": prints }))
            .await
        else {
            return HashSet::new();
        };
        data.get("exactMatches")
            .and_then(|m| m.as_array())
            .map(|matches| {
                matches
                    .iter()
                    .filter_map(|m| m.get("file")?.get("modId")?.as_u64())
                    .map(|id| format!("cf:{id}"))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A second opinion for a CurseForge project with no side tags. Most are on Modrinth too,
    /// where the field is filled in.
    async fn modrinth_side(&self, slug: &str) -> &'static str {
        if slug.is_empty() {
            return "unknown";
        }
        match self
            .modrinth::<serde_json::Value>(&format!("/project/{slug}"), &[])
            .await
        {
            Ok(project) => modrinth_verdict(&project),
            Err(_) => "unknown",
        }
    }

    /// The free half: the tags a search hit already carries. Most answer here.
    fn cf_verdict_from_hit(hit: &serde_json::Value, manifest: &Manifest) -> Option<&'static str> {
        let loader = manifest
            .loader
            .as_ref()
            .map(|l| l.kind.to_lowercase())
            .unwrap_or_default();
        let empty = Vec::new();
        let latest = hit
            .get("latestFiles")
            .and_then(|f| f.as_array())
            .unwrap_or(&empty);

        let tags = cf_tags(latest, &manifest.minecraft, &loader);
        if tags.contains("Server") {
            Some("no")
        } else if tags.contains("Client") {
            Some("yes")
        } else {
            None
        }
    }

    /// The expensive half, for the ones whose search hit said nothing. Their files first,
    /// then Modrinth as a second opinion.
    async fn cf_verdict_looked_up(&self, id: u64, slug: &str, manifest: &Manifest) -> &'static str {
        let loader = manifest
            .loader
            .as_ref()
            .map(|l| l.kind.to_lowercase())
            .unwrap_or_default();

        let mut query = vec![
            ("gameVersion", manifest.minecraft.clone()),
            ("pageSize", "8".to_string()),
        ];
        if let Some(kind) = cf_loader(&loader) {
            query.push(("modLoaderType", kind.to_string()));
        }
        if let Ok(files) = self.cf_get(&format!("/mods/{id}/files"), &query).await {
            if let Some(files) = files.as_array() {
                let tags = cf_tags(files, &manifest.minecraft, &loader);
                if tags.contains("Server") {
                    return "no";
                }
                if tags.contains("Client") {
                    return "yes";
                }
            }
        }
        self.modrinth_side(slug).await
    }

    pub async fn search(
        &self,
        manifest: &Manifest,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Hit>> {
        let loader = manifest
            .loader
            .as_ref()
            .map(|l| l.kind.to_lowercase())
            .unwrap_or_default();

        let mut facets = vec![
            json!([format!("versions:{}", manifest.minecraft)]),
            json!(["project_type:mod"]),
        ];
        if !loader.is_empty() {
            facets.push(json!([format!("categories:{loader}")]));
        }

        let response: serde_json::Value = self
            .modrinth(
                "/search",
                &[
                    ("query", query.to_string()),
                    ("limit", limit.to_string()),
                    ("offset", offset.to_string()),
                    ("index", "downloads".into()),
                    ("facets", serde_json::to_string(&facets)?),
                ],
            )
            .await?;

        let served = self.served_modrinth(manifest).await;
        let mut found = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        if let Some(hits) = response.get("hits").and_then(|h| h.as_array()) {
            for hit in hits {
                let project_id = hit.get("project_id").and_then(|v| v.as_str()).unwrap_or("");
                let on_server = served.contains(project_id);
                if !on_server && modrinth_verdict(hit) != "yes" {
                    continue;
                }
                let title = hit
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                seen.insert(title.to_lowercase());
                found.push(Hit {
                    id: hit
                        .get("slug")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    title,
                    description: hit
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    downloads: hit.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0),
                    source: "modrinth".into(),
                    on_server,
                    icon: hit
                        .get("icon_url")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                });
            }
        }

        if !self.cf_key.is_empty() {
            found.extend(
                self.search_curseforge(manifest, query, limit, offset, &seen)
                    .await,
            );
        }

        found.sort_by_key(|hit| std::cmp::Reverse(hit.downloads));
        Ok(found)
    }

    async fn search_curseforge(
        &self,
        manifest: &Manifest,
        query: &str,
        limit: usize,
        offset: usize,
        seen: &HashSet<String>,
    ) -> Vec<Hit> {
        let loader = manifest
            .loader
            .as_ref()
            .map(|l| l.kind.to_lowercase())
            .unwrap_or_default();

        let mut params = vec![
            ("gameId", CF_GAME_MINECRAFT.to_string()),
            ("classId", CF_CLASS_MOD.to_string()),
            ("gameVersion", manifest.minecraft.clone()),
            ("searchFilter", query.to_string()),
            ("sortField", CF_SORT_DOWNLOADS.to_string()),
            ("sortOrder", "desc".into()),
            ("pageSize", limit.to_string()),
            ("index", offset.to_string()),
        ];
        if let Some(kind) = cf_loader(&loader) {
            params.push(("modLoaderType", kind.to_string()));
        }

        let Ok(data) = self.cf_get("/mods/search", &params).await else {
            return Vec::new();
        };
        let Some(hits) = data.as_array() else {
            return Vec::new();
        };

        let served = self.served_curseforge(manifest).await;
        let mut out = Vec::new();

        for hit in hits {
            let title = hit
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if seen.contains(&title.to_lowercase()) {
                continue; // the same mod on both sites, under two different ids
            }
            let Some(id) = hit.get("id").and_then(|v| v.as_u64()) else {
                continue;
            };
            let key = format!("cf:{id}");
            let on_server = served.contains(&key);

            if !on_server {
                let verdict = match Self::cf_verdict_from_hit(hit, manifest) {
                    Some(verdict) => verdict,
                    None => {
                        let slug = hit.get("slug").and_then(|v| v.as_str()).unwrap_or_default();
                        self.cf_verdict_looked_up(id, slug, manifest).await
                    }
                };
                if verdict != "yes" {
                    continue;
                }
            }
            out.push(Hit {
                id: key,
                title,
                description: hit
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                downloads: hit
                    .get("downloadCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                source: "curseforge".into(),
                on_server,
                icon: hit
                    .get("logo")
                    .and_then(|l| l.get("thumbnailUrl"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            });
        }
        out
    }

    /// Resolve each mod plus everything it requires. Installing Iris without Sodium is a
    /// crash, not a preference.
    pub async fn resolve(&self, manifest: &Manifest, wanted: &[String]) -> Result<Vec<ModEntry>> {
        let mut queue: Vec<String> = wanted.to_vec();
        let mut seen: HashSet<String> = HashSet::new();
        let mut chosen = Vec::new();

        while let Some(id) = queue.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let picked = if let Some(project) = id.strip_prefix("cf:") {
                self.pick_curseforge(project, manifest, &mut queue).await
            } else {
                self.pick_modrinth(&id, manifest, &mut queue).await
            };
            match picked {
                Some(entry) => chosen.push(entry),
                None => continue,
            }
        }
        Ok(chosen)
    }

    async fn pick_modrinth(
        &self,
        slug: &str,
        manifest: &Manifest,
        queue: &mut Vec<String>,
    ) -> Option<ModEntry> {
        let loader = manifest
            .loader
            .as_ref()
            .map(|l| l.kind.to_lowercase())
            .unwrap_or_default();
        let mut query = vec![("game_versions", format!("[\"{}\"]", manifest.minecraft))];
        if !loader.is_empty() {
            query.push(("loaders", format!("[\"{loader}\"]")));
        }
        let versions: Vec<serde_json::Value> = self
            .modrinth(&format!("/project/{slug}/version"), &query)
            .await
            .ok()?;

        // release before beta: the list is newest-first regardless of type, and handing
        // somebody a beta as another mod's dependency is a choice they never made
        let version = versions
            .iter()
            .find(|v| v.get("version_type").and_then(|t| t.as_str()) == Some("release"))
            .or_else(|| versions.first())?;

        for dependency in version.get("dependencies")?.as_array()? {
            if dependency.get("dependency_type").and_then(|t| t.as_str()) == Some("required") {
                if let Some(project) = dependency.get("project_id").and_then(|v| v.as_str()) {
                    queue.push(project.to_string());
                }
            }
        }

        let file = version.get("files")?.as_array()?.first()?;
        Some(ModEntry {
            name: file.get("filename")?.as_str()?.to_string(),
            sha1: file.get("hashes")?.get("sha1")?.as_str()?.to_string(),
            url: file.get("url")?.as_str()?.to_string(),
        })
    }

    async fn pick_curseforge(
        &self,
        project: &str,
        manifest: &Manifest,
        queue: &mut Vec<String>,
    ) -> Option<ModEntry> {
        let loader = manifest
            .loader
            .as_ref()
            .map(|l| l.kind.to_lowercase())
            .unwrap_or_default();
        let mut query = vec![("gameVersion", manifest.minecraft.clone())];
        if let Some(kind) = cf_loader(&loader) {
            query.push(("modLoaderType", kind.to_string()));
        }
        let data = self
            .cf_get(&format!("/mods/{project}/files"), &query)
            .await
            .ok()?;
        let mut files = data.as_array()?.clone();
        files.sort_by_key(|f| f.get("releaseType").and_then(|v| v.as_u64()).unwrap_or(9));
        let _ = CF_RELEASE;

        for file in files {
            let Some(hashes) = file.get("hashes").and_then(|h| h.as_array()) else {
                continue;
            };
            let sha1 = hashes.iter().find_map(|h| {
                (h.get("algo").and_then(|a| a.as_u64()) == Some(CF_SHA1 as u64))
                    .then(|| h.get("value")?.as_str().map(str::to_string))
                    .flatten()
            });
            let (Some(sha1), Some(url)) = (
                sha1,
                file.get("downloadUrl")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            ) else {
                // the author opted out of third-party downloads; nothing to do but skip it
                continue;
            };
            if let Some(dependencies) = file.get("dependencies").and_then(|d| d.as_array()) {
                for dependency in dependencies {
                    if dependency.get("relationType").and_then(|r| r.as_u64())
                        == Some(CF_REQUIRED_DEPENDENCY as u64)
                    {
                        if let Some(id) = dependency.get("modId").and_then(|v| v.as_u64()) {
                            queue.push(format!("cf:{id}"));
                        }
                    }
                }
            }
            return Some(ModEntry {
                name: file.get("fileName")?.as_str()?.to_string(),
                sha1,
                url,
            });
        }
        None
    }
}

fn cf_loader(loader: &str) -> Option<u32> {
    match loader {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn murmur2_matches_what_curseforge_agrees_with() {
        // pinned after the live api confirmed this implementation
        // (xaerominimap-neoforge-1.21.1-26.4.2.jar fingerprints to their project 263420)
        assert_eq!(murmur2(b"", 1), 1_540_447_798);
        assert_eq!(murmur2(b"hello", 1), 2_788_266_382);
        assert_eq!(murmur2(b"abcd", 1), 3_376_380_438);
        assert_eq!(murmur2(b"abc", 1), 1_621_425_345);
    }

    #[test]
    fn whitespace_is_stripped_before_hashing() {
        assert_eq!(fingerprint(b"a b\tc\n"), murmur2(b"abc", 1));
    }

    #[test]
    fn a_mod_the_server_must_carry_is_not_one_a_player_can_add() {
        let yes = json!({"client_side": "required", "server_side": "unsupported"});
        let no = json!({"client_side": "required", "server_side": "required"});
        let unknown = json!({"client_side": "unknown", "server_side": "unknown"});
        assert_eq!(modrinth_verdict(&yes), "yes");
        assert_eq!(modrinth_verdict(&no), "no");
        assert_eq!(modrinth_verdict(&unknown), "unknown");
        assert_eq!(modrinth_verdict(&json!({})), "unknown");
    }

    #[test]
    fn curseforge_side_tags_only_count_for_the_right_version() {
        let both = vec![json!({"gameVersions": ["Client", "1.21.1", "NeoForge", "Server"]})];
        let client = vec![json!({"gameVersions": ["Client", "1.21.1", "NeoForge"]})];
        let other = vec![json!({"gameVersions": ["Client", "1.20.1", "NeoForge"]})];
        assert!(cf_tags(&both, "1.21.1", "neoforge").contains("Server"));
        assert!(!cf_tags(&client, "1.21.1", "neoforge").contains("Server"));
        assert!(cf_tags(&other, "1.21.1", "neoforge").is_empty());
    }
}
