//! Finding what a single player can add on top of what the server already sends.
//!
//! Two catalogues, four kinds. Every hit is asked whether it runs without a server side, and
//! whether the server already ships it.

use std::collections::HashSet;

use anyhow::{anyhow, Result};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::servers::{Manifest, ModEntry};

const MODRINTH: &str = "https://api.modrinth.com/v2";
const CURSEFORGE: &str = "https://api.curseforge.com/v1";

const CF_GAME_MINECRAFT: u32 = 432;
const CF_REQUIRED_DEPENDENCY: u32 = 3;
const CF_SHA1: u32 = 1;
const CF_RELEASE: u32 = 1;

/// What the player is browsing for. Everything below branches on this and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mod,
    ResourcePack,
    Shader,
    Datapack,
}

/// Every kind, for the callers that answer for all four.
pub const KINDS: [Kind; 4] = [Kind::Mod, Kind::ResourcePack, Kind::Shader, Kind::Datapack];

impl Kind {
    /// Where a file of this kind lives inside an instance. Minecraft's own directory names,
    /// which is why shaders land in `shaderpacks` and not in `shaders`.
    pub fn dir(&self) -> &str {
        match self {
            Kind::Mod => "mods",
            Kind::ResourcePack => "resourcepacks",
            Kind::Shader => "shaderpacks",
            Kind::Datapack => "datapacks",
        }
    }

    /// The wire name, which is also Modrinth's `project_type` facet value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Mod => "mod",
            Kind::ResourcePack => "resourcepack",
            Kind::Shader => "shader",
            Kind::Datapack => "datapack",
        }
    }

    /// What the server already sends of this kind. The manifest keys these by directory, so
    /// `dir()` is the whole translation.
    pub fn entries<'m>(&self, manifest: &'m Manifest) -> &'m [ModEntry] {
        manifest.entries(self.dir())
    }

    /// CurseForge's class ids, confirmed against
    /// `GET /v1/categories?gameId=432&classesOnly=true` on 2026-08-25: 6 mc-mods,
    /// 12 texture-packs, 6552 shaders, 6945 data-packs.
    /// What Modrinth calls this in a project url. Not the same words as its search facets.
    pub fn modrinth_path(&self) -> &'static str {
        match self {
            Kind::Mod => "mod",
            Kind::ResourcePack => "resourcepack",
            Kind::Shader => "shader",
            Kind::Datapack => "datapack",
        }
    }

    fn cf_class(&self) -> u32 {
        match self {
            Kind::Mod => 6,
            Kind::ResourcePack => 12,
            Kind::Shader => 6552,
            Kind::Datapack => 6945,
        }
    }
}

/// The Modrinth search facets for one kind. The loader facet is a mod's alone.
///
/// `also_fabric` widens it: Sinytra Connector runs Fabric mods on NeoForge, and a pack carrying
/// it has both catalogues available. Matched on the filename, because a manifest carries names.
pub fn bridges_fabric(entries: &[ModEntry]) -> bool {
    entries
        .iter()
        .any(|entry| entry.name.to_lowercase().contains("connector"))
}

fn modrinth_facets(
    kind: Kind,
    minecraft: &str,
    loader: &str,
    also_fabric: bool,
) -> Vec<serde_json::Value> {
    let mut facets = vec![
        json!([format!("versions:{minecraft}")]),
        json!([format!("project_type:{}", kind.as_str())]),
    ];
    if kind == Kind::Mod && !loader.is_empty() {
        // The inner array is an OR on Modrinth's side, so this reads "neoforge or fabric".
        let mut loaders = vec![format!("categories:{loader}")];
        if also_fabric && loader != "fabric" {
            loaders.push("categories:fabric".into());
        }
        facets.push(json!(loaders));
    }
    facets
}

/// Which Modrinth "loaders" a file of this kind carries: a resource pack is `minecraft`, a
/// datapack `datapack`, a shader `iris` or `optifine`. Without it, Terralith answers with its
/// NeoForge jar and its datapack zip in one list.
fn modrinth_loaders(kind: Kind, loader: &str) -> Option<String> {
    Some(match kind {
        Kind::Mod if loader.is_empty() => return None,
        Kind::Mod => format!("[\"{loader}\"]"),
        Kind::ResourcePack => "[\"minecraft\"]".into(),
        Kind::Shader => "[\"iris\",\"optifine\"]".into(),
        Kind::Datapack => "[\"datapack\"]".into(),
    })
}

/// How a search is ordered. The two catalogues spell these differently, so the name the
/// interface uses is translated once, here, rather than at both call sites.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    /// What the catalogue thinks the query meant. Meaningless without a query, which is why
    /// the default is downloads instead.
    Relevance,
    #[default]
    Downloads,
    Follows,
    Updated,
    Newest,
}

impl Sort {
    fn modrinth(self) -> &'static str {
        match self {
            Sort::Relevance => "relevance",
            Sort::Downloads => "downloads",
            Sort::Follows => "follows",
            Sort::Updated => "updated",
            Sort::Newest => "newest",
        }
    }

    /// CurseForge numbers its sort fields, and has no "follows" - popularity is the nearest
    /// thing it offers.
    fn curseforge(self) -> u32 {
        match self {
            Sort::Relevance => 1,
            Sort::Downloads => 6,
            Sort::Follows => 2,
            Sort::Updated => 3,
            Sort::Newest => 11,
        }
    }
}

/// What to ask the catalogue for, beyond which manifest and which kind.
///
/// Grouped rather than passed one after another: `limit` and `offset` are both a `usize`, and
/// swapping them at a call site pages the catalogue wrongly without failing anywhere.
#[derive(Clone, Copy, Debug)]
pub struct Search<'a> {
    pub query: &'a str,
    pub limit: usize,
    pub offset: usize,
    pub sort: Sort,
    /// No server on the other side, so the client-side filter would only hide most of the
    /// catalogue for no reason.
    pub own_setup: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hit {
    pub id: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
    pub source: String,
    /// The server already sends this one. Shown, not hidden.
    pub on_server: bool,
    /// Already in the player's own profile.
    #[serde(default)]
    pub installed: bool,
    pub icon: Option<String>,
    /// Who wrote it. A title alone does not tell two mods of the same name apart.
    #[serde(default)]
    pub author: String,
    /// The build that would be installed, so what arrives is not a surprise.
    #[serde(default)]
    pub version: String,
    /// The project's own page. Nothing here is a substitute for reading it.
    #[serde(default)]
    pub url: String,
    /// Whether this one can be taken away again. What the server ships cannot.
    #[serde(default)]
    pub removable: bool,
}

/// Can one player add this and have it work? Only if the server does not have to carry it too.
///
/// A mod's question alone: a resource pack has no server half, and Modrinth's datapack search
/// returns the mod projects that ship one - Terralith is `server_side: required` and its
/// datapack is still the player's to install.
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

/// CurseForge puts it in a file's `gameVersions` array, beside the version and loader.
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

    /// Which Modrinth projects the server already ships, by file hash - one batch lookup.
    /// Takes entries rather than the manifest: only the kind being browsed counts.
    async fn served_modrinth(&self, entries: &[ModEntry]) -> HashSet<String> {
        let hashes: Vec<&str> = entries.iter().map(|m| m.sha1.as_str()).collect();
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
    async fn served_curseforge(&self, entries: &[ModEntry]) -> HashSet<String> {
        let mut prints = Vec::new();
        let mut missing = 0usize;
        for entry in entries {
            // a fingerprint of a file we were pointed at is an oracle on its contents
            if !crate::sync::is_hash(&entry.sha1) {
                missing += 1;
                continue;
            }
            match std::fs::read(crate::paths::blobs().join(&entry.sha1)) {
                Ok(bytes) => prints.push(fingerprint(&bytes)),
                Err(_) => missing += 1,
            }
        }
        if prints.is_empty() {
            return HashSet::new();
        }
        // A partial answer must not be remembered, so nothing here is cached.
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
        kind: Kind,
        wanted: Search<'_>,
    ) -> Result<Vec<Hit>> {
        let loader = manifest
            .loader
            .as_ref()
            .map(|l| l.kind.to_lowercase())
            .unwrap_or_default();

        let connected = bridges_fabric(&manifest.mods);
        let facets = modrinth_facets(kind, &manifest.minecraft, &loader, connected);

        let response: serde_json::Value = self
            .modrinth(
                "/search",
                &[
                    ("query", wanted.query.to_string()),
                    ("limit", wanted.limit.to_string()),
                    ("offset", wanted.offset.to_string()),
                    ("index", wanted.sort.modrinth().to_string()),
                    ("facets", serde_json::to_string(&facets)?),
                ],
            )
            .await?;

        let served = self.served_modrinth(kind.entries(manifest)).await;
        let mut found = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        if let Some(hits) = response.get("hits").and_then(|h| h.as_array()) {
            for hit in hits {
                let project_id = hit.get("project_id").and_then(|v| v.as_str()).unwrap_or("");
                let on_server = served.contains(project_id);
                if kind == Kind::Mod
                    && !wanted.own_setup
                    && !on_server
                    && modrinth_verdict(hit) != "yes"
                {
                    continue;
                }
                let title = hit
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                seen.insert(title.to_lowercase());
                let slug = hit
                    .get("slug")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                found.push(Hit {
                    // Modrinth's own url is the project type plus the slug, and the type is
                    // what the tab already decided.
                    url: format!("https://modrinth.com/{}/{slug}", kind.modrinth_path()),
                    id: slug,
                    title,
                    description: hit
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    downloads: hit.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0),
                    source: "modrinth".into(),
                    on_server,
                    installed: false,
                    icon: hit
                        .get("icon_url")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    author: hit
                        .get("author")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    version: hit
                        .get("latest_version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    // Nothing in the catalogue is installed yet, so nothing is removable yet.
                    removable: false,
                });
            }
        }

        if !self.cf_key.is_empty() {
            found.extend(self.search_curseforge(manifest, kind, wanted, &seen).await);
        }

        // Two catalogues, one list. Only a download order can be merged meaningfully - for
        // anything else each side is already ordered and interleaving by a field we did not
        // fetch would be a guess, so they stay as they came.
        if wanted.sort == Sort::Downloads {
            found.sort_by_key(|hit| std::cmp::Reverse(hit.downloads));
        }
        Ok(found)
    }

    async fn search_curseforge(
        &self,
        manifest: &Manifest,
        kind: Kind,
        wanted: Search<'_>,
        seen: &HashSet<String>,
    ) -> Vec<Hit> {
        let loader = manifest
            .loader
            .as_ref()
            .map(|l| l.kind.to_lowercase())
            .unwrap_or_default();

        let mut params = vec![
            ("gameId", CF_GAME_MINECRAFT.to_string()),
            ("classId", kind.cf_class().to_string()),
            ("gameVersion", manifest.minecraft.clone()),
            ("searchFilter", wanted.query.to_string()),
            ("sortField", wanted.sort.curseforge().to_string()),
            ("sortOrder", "desc".into()),
            ("pageSize", wanted.limit.to_string()),
            ("index", wanted.offset.to_string()),
        ];
        // One loader even with Connector in the pack: CurseForge takes a single modLoaderType, and
        // dropping it would pull in Forge-only mods. Modrinth, which takes a list, is widened instead.
        // Sent for mods only - class 12 or 6552 with a loader filters every pack out of its own tab.
        if kind == Kind::Mod {
            if let Some(loader) = cf_loader(&loader) {
                params.push(("modLoaderType", loader.to_string()));
            }
        }

        let Ok(data) = self.cf_get("/mods/search", &params).await else {
            return Vec::new();
        };
        let Some(hits) = data.as_array() else {
            return Vec::new();
        };

        let served = self.served_curseforge(kind.entries(manifest)).await;
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

            if kind == Kind::Mod && !on_server {
                let verdict = match Self::cf_verdict_from_hit(hit, manifest) {
                    Some(verdict) => verdict,
                    None => {
                        let slug = hit.get("slug").and_then(|v| v.as_str()).unwrap_or_default();
                        self.cf_verdict_looked_up(id, slug, manifest).await
                    }
                };
                if !wanted.own_setup && verdict != "yes" {
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
                installed: false,
                icon: hit
                    .get("logo")
                    .and_then(|l| l.get("thumbnailUrl"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                author: hit
                    .get("authors")
                    .and_then(|a| a.as_array())
                    .and_then(|authors| authors.first())
                    .and_then(|a| a.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                // The newest file's display name. CurseForge has no "latest version" field,
                // and the file list is newest last.
                version: hit
                    .get("latestFiles")
                    .and_then(|f| f.as_array())
                    .and_then(|files| files.last())
                    .and_then(|f| f.get("displayName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                url: hit
                    .get("links")
                    .and_then(|l| l.get("websiteUrl"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                removable: false,
            });
        }
        out
    }

    /// Resolve each pick plus what it requires: Iris without Sodium is a crash.
    ///
    /// Followed for mods only - what a shader declares as required is a mod, and belongs in `mods/`
    /// after the client-side question, not in `shaderpacks/` because that tab was open.
    pub async fn resolve(
        &self,
        manifest: &Manifest,
        kind: Kind,
        wanted: &[String],
    ) -> Result<Vec<ModEntry>> {
        let mut queue: Vec<String> = wanted.to_vec();
        let mut seen: HashSet<String> = HashSet::new();
        let mut chosen = Vec::new();
        let mut dependencies = Vec::new();

        while let Some(id) = queue.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            dependencies.clear();
            let picked = if let Some(project) = id.strip_prefix("cf:") {
                self.pick_curseforge(project, manifest, kind, &mut dependencies)
                    .await
            } else {
                self.pick_modrinth(&id, manifest, kind, &mut dependencies)
                    .await
            };
            if kind == Kind::Mod {
                queue.append(&mut dependencies);
            }
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
        kind: Kind,
        queue: &mut Vec<String>,
    ) -> Option<ModEntry> {
        let loader = manifest
            .loader
            .as_ref()
            .map(|l| l.kind.to_lowercase())
            .unwrap_or_default();
        // game_versions plus loaders is the whole match: Modrinth already turned a pack's pack_format
        // into the versions it fits. Nothing is guessed from the filename.
        let mut query = vec![("game_versions", format!("[\"{}\"]", manifest.minecraft))];
        if let Some(loaders) = modrinth_loaders(kind, &loader) {
            query.push(("loaders", loaders));
        }
        let versions: Vec<serde_json::Value> = self
            .modrinth(&format!("/project/{slug}/version"), &query)
            .await
            .ok()?;

        // release before beta: handing somebody a beta as a dependency is a choice they never made
        let version = versions
            .iter()
            .find(|v| v.get("version_type").and_then(|t| t.as_str()) == Some("release"))
            .or_else(|| versions.first())?;

        // Not `?`: a version can come back without the field, and losing the file over that is odd.
        if let Some(dependencies) = version.get("dependencies").and_then(|d| d.as_array()) {
            for dependency in dependencies {
                if dependency.get("dependency_type").and_then(|t| t.as_str()) == Some("required") {
                    if let Some(project) = dependency.get("project_id").and_then(|v| v.as_str()) {
                        queue.push(project.to_string());
                    }
                }
            }
        }

        // A version can carry several files; `primary` is which one to install.
        let files = version.get("files")?.as_array()?;
        let file = files
            .iter()
            .find(|f| f.get("primary").and_then(|p| p.as_bool()) == Some(true))
            .or_else(|| files.first())?;
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
        kind: Kind,
        queue: &mut Vec<String>,
    ) -> Option<ModEntry> {
        let loader = manifest
            .loader
            .as_ref()
            .map(|l| l.kind.to_lowercase())
            .unwrap_or_default();
        let mut query = vec![("gameVersion", manifest.minecraft.clone())];
        if kind == Kind::Mod {
            if let Some(loader) = cf_loader(&loader) {
                query.push(("modLoaderType", loader.to_string()));
            }
        }
        let data = self
            .cf_get(&format!("/mods/{project}/files"), &query)
            .await
            .ok()?;
        // gameVersion is all CurseForge offers for a pack - no pack_format, no shader-loader field.
        // release is taken rather than checked. Sorting is stable, so within one release type
        // the API's own newest-first order survives.
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
    fn every_kind_knows_its_directory_and_its_wire_name() {
        let dirs: Vec<&str> = KINDS.iter().map(|k| k.dir()).collect();
        assert_eq!(dirs, ["mods", "resourcepacks", "shaderpacks", "datapacks"]);
        for kind in KINDS {
            // the wire name has to survive both directions: the frontend sends a string and
            // a command takes a Kind, so serde is the only translation there is
            assert_eq!(json!(kind), json!(kind.as_str()));
            assert_eq!(
                serde_json::from_value::<Kind>(json!(kind.as_str())).unwrap(),
                kind
            );
        }
        assert!(serde_json::from_value::<Kind>(json!("shaderpack")).is_err());
    }

    #[test]
    fn a_pack_is_not_searched_by_loader() {
        let mods = modrinth_facets(Kind::Mod, "1.21.1", "neoforge", false);
        assert_eq!(mods[1], json!(["project_type:mod"]));
        assert!(mods.contains(&json!(["categories:neoforge"])));
        for kind in [Kind::ResourcePack, Kind::Shader, Kind::Datapack] {
            let facets = modrinth_facets(kind, "1.21.1", "neoforge", false);
            assert_eq!(facets[0], json!(["versions:1.21.1"]));
            assert_eq!(facets.len(), 2, "{kind:?} must not carry a loader facet");
        }
    }

    #[test]
    fn each_kind_asks_modrinth_for_its_own_file() {
        assert_eq!(
            modrinth_loaders(Kind::Mod, "fabric").as_deref(),
            Some(r#"["fabric"]"#)
        );
        // no loader chosen means vanilla, where every filter would be wrong
        assert_eq!(modrinth_loaders(Kind::Mod, ""), None);
        assert_eq!(
            modrinth_loaders(Kind::Datapack, "neoforge").as_deref(),
            Some(r#"["datapack"]"#),
            "otherwise Terralith's NeoForge jar answers a datapack request"
        );
        assert_eq!(
            modrinth_loaders(Kind::ResourcePack, "").as_deref(),
            Some(r#"["minecraft"]"#)
        );
        assert_eq!(
            modrinth_loaders(Kind::Shader, "").as_deref(),
            Some(r#"["iris","optifine"]"#)
        );
    }

    #[test]
    fn curseforge_classes_are_the_ones_the_api_lists() {
        // GET /v1/categories?gameId=432&classesOnly=true, read 2026-08-25:
        // 6 mc-mods, 12 texture-packs, 6552 shaders, 6945 data-packs
        let classes: Vec<u32> = KINDS.iter().map(|k| k.cf_class()).collect();
        assert_eq!(classes, [6, 12, 6552, 6945]);
    }

    #[test]
    fn on_server_follows_the_kind_being_browsed() {
        let manifest = Manifest {
            mods: vec![ModEntry {
                name: "sodium.jar".into(),
                ..Default::default()
            }],
            shaders: vec![ModEntry {
                name: "complementary.zip".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(Kind::Mod.entries(&manifest).len(), 1);
        assert_eq!(Kind::Shader.entries(&manifest)[0].name, "complementary.zip");
        assert!(Kind::ResourcePack.entries(&manifest).is_empty());
        assert!(Kind::Datapack.entries(&manifest).is_empty());
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

#[cfg(test)]
mod connector_tests {
    use super::*;

    fn entry(name: &str) -> ModEntry {
        ModEntry {
            name: name.into(),
            sha1: "0".repeat(40),
            url: String::new(),
        }
    }

    #[test]
    fn a_pack_carrying_connector_widens_the_search_to_fabric() {
        let pack = vec![
            entry("sodium-neoforge-0.8.12.jar"),
            entry("Connector-1.0.0-beta.46+1.21.1-full.jar"),
        ];
        assert!(bridges_fabric(&pack));
        assert!(!bridges_fabric(&[entry("sodium-neoforge-0.8.12.jar")]));

        let widened = modrinth_facets(Kind::Mod, "1.21.1", "neoforge", true);
        let loaders = widened.last().unwrap().to_string();
        assert!(loaders.contains("categories:neoforge") && loaders.contains("categories:fabric"));

        // Without it, the loader is the loader - offering Fabric mods for a plain NeoForge
        // instance offers mods that will not load.
        let plain = modrinth_facets(Kind::Mod, "1.21.1", "neoforge", false);
        assert!(!plain.last().unwrap().to_string().contains("fabric"));

        // A Fabric instance is already where those mods run; naming it twice is noise.
        let fabric = modrinth_facets(Kind::Mod, "1.21.1", "fabric", true);
        assert_eq!(
            fabric.last().unwrap().as_array().unwrap().len(),
            1,
            "one loader, listed once"
        );
    }
}
