//! Servers the player runs on this machine.
//!
//! The same job `unifiedmc-server-cli init` does - turn a pack, or a bare version, into a
//! directory a Minecraft server starts out of - plus the part a CLI cannot have: starting it,
//! reading its console, and stopping it again. Both entry points call the functions here, so
//! "where does a client-only jar go" is answered once.
//!
//! Java is not a prerequisite. Mojang publishes a JRE per Minecraft version and the launcher
//! has already downloaded one to play; a server runs on the same runtime.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

use crate::pack::{self, Side};
use crate::sync::plain_relative;

/// Where the publisher mod is published. The launcher and the server mod are versioned
/// separately, so this follows whatever the server repository last released.
pub const PUBLISHER_URL: &str =
    "https://github.com/EJTP/UnifiedMC-Server/releases/latest/download/unifiedmc-server.jar";

/// Only these load a jar the server can publish a pack from.
pub fn publisher_runs_on(loader: &str) -> bool {
    matches!(loader, "neoforge" | "fabric" | "quilt")
}

/// One server on this machine.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hosted {
    pub id: String,
    pub name: String,
    pub minecraft: String,
    /// None is vanilla: no loader, no mods, the jar Mojang publishes.
    #[serde(default)]
    pub loader: Option<String>,
    #[serde(default)]
    pub loader_version: Option<String>,
    pub port: u16,
    /// Heap in MB. 0 means sized from how much the pack turned out to be.
    #[serde(default)]
    pub memory: u64,
    /// The pack it was built from, when it was built from one.
    #[serde(default)]
    pub source: Option<String>,
    /// The argv the server starts with, decided once at creation: Fabric hands out a jar to
    /// run, NeoForge an installer that writes its own script.
    #[serde(default)]
    pub command: Vec<String>,
    /// Whether whoever made it said they had read the Minecraft EULA.
    #[serde(default)]
    pub eula: bool,
    /// Whether the publisher mod is in `mods/` - the whole point of hosting here rather than
    /// anywhere else, so it is stated rather than inferred from a directory listing.
    #[serde(default)]
    pub publishes: bool,
}

/* ------------------------------------------------------------------ the list. */

fn file() -> PathBuf {
    crate::paths::data().join("hosted.json")
}

pub fn root() -> PathBuf {
    crate::paths::data().join("hosted")
}

/// A server's own directory. The id is generated here, but it still becomes a directory name.
pub fn dir(id: &str) -> PathBuf {
    root().join(crate::servers::tame(id))
}

pub fn load() -> Vec<Hosted> {
    std::fs::read_to_string(file())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(list: &[Hosted]) -> Result<()> {
    let path = file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(list)?)?;
    Ok(())
}

pub fn find(id: &str) -> Option<Hosted> {
    load().into_iter().find(|server| server.id == id)
}

/// A pack name becomes a directory name, so it stops being a path.
pub fn sanitise(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "pack".into()
    } else {
        trimmed.to_lowercase()
    }
}

/* ------------------------------------------------------------------- the JVM. */

/// Which Java a Minecraft server of this version needs.
///
/// Mojang moved twice: 1.17 to 17, and 1.20.5 to 21. Running below the floor fails on the
/// first class the server loads, with a message nobody reads as "wrong Java".
pub fn java_needed(minecraft: &str) -> u32 {
    let mut parts = minecraft.split('.').skip(1);
    let major: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let minor: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    match (major, minor) {
        (0..=16, _) => 8,
        (17..=19, _) => 17,
        (20, 0..=4) => 17,
        _ => 21,
    }
}

/// The `java` inside a runtime directory, wherever that platform keeps it.
fn binary_in(home: &Path) -> Option<(PathBuf, PathBuf)> {
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    // macOS nests the runtime one level further down inside the bundle.
    [home.to_path_buf(), home.join("jre.bundle/Contents/Home")]
        .into_iter()
        .map(|home| {
            let java = home.join("bin").join(name);
            (home, java)
        })
        .find(|(_, java)| java.is_file())
}

/// Every JRE the launcher has downloaded, as (major version, the java binary).
fn runtimes() -> Vec<(u32, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(crate::paths::minecraft().join("runtimes")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| binary_in(&entry.path()))
        .map(|(home, java)| (release_major(&home).unwrap_or(0), java))
        .collect()
}

/// `JAVA_VERSION="21.0.4"` out of the `release` file every JDK build ships.
fn release_major(home: &Path) -> Option<u32> {
    let release = std::fs::read_to_string(home.join("release")).ok()?;
    let line = release
        .lines()
        .find_map(|line| line.strip_prefix("JAVA_VERSION="))?;
    major_of(line.trim().trim_matches('"'))
}

/// "21.0.4" is 21; "1.8.0_452" is 8, because Java numbered itself differently until 9.
fn major_of(version: &str) -> Option<u32> {
    let first: u32 = version.split('.').next()?.parse().ok()?;
    if first == 1 {
        version.split('.').nth(1)?.parse().ok()
    } else {
        Some(first)
    }
}

/// What a `java` binary says it is, or None if it is not one.
///
/// Asking rather than trusting: a JVM below the floor does not refuse to start the server, it
/// starts it and dies on the first class it loads, and `UnsupportedClassVersionError` in a
/// console is not something anybody reads as "wrong Java".
fn probe_major(java: &Path) -> Option<u32> {
    let output = std::process::Command::new(java)
        .arg("-version")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // -version prints to stderr on every JVM ever shipped, as `openjdk version "21.0.4"`.
    let text = String::from_utf8_lossy(&output.stderr);
    major_of(text.split('"').nth(1)?)
}

/// A JVM new enough to run this Minecraft.
///
/// The lowest one that clears the floor, not the newest: Forge on 1.16 wants Java 8 and dies
/// on 21, and picking the smallest that works keeps that case right for free. Whatever is on
/// PATH is the last resort - it is often a Java 8 that was installed for something else.
///
/// Resolved on every start rather than kept: a runtime can be deleted, and the player can
/// point at a different one in the settings, and neither should need the server rebuilt.
pub fn java(minecraft: &str) -> Result<PathBuf> {
    let needed = java_needed(minecraft);

    // The player's own overrules everything. It is the only lever they have when the launcher
    // has no runtime that fits and PATH holds the wrong one, so a bad one is said out loud
    // rather than quietly skipped - a setting that silently does nothing is worse than none.
    let chosen = crate::settings::Settings::load().java_path;
    let chosen = chosen.trim();
    if !chosen.is_empty() {
        let path = PathBuf::from(chosen);
        return match probe_major(&path) {
            None => Err(anyhow!("error.javaNotRunnable")),
            Some(major) if major < needed => Err(anyhow!("error.javaTooOld")),
            Some(_) => Ok(path),
        };
    }

    let mut installed = runtimes();
    installed.sort_by_key(|(major, _)| *major);
    if let Some((_, java)) = installed.iter().find(|(major, _)| *major >= needed) {
        return Ok(java.clone());
    }

    match probe_major(Path::new("java")) {
        Some(major) if major >= needed => Ok(PathBuf::from("java")),
        Some(_) => Err(anyhow!("error.javaTooOld")),
        None => Err(anyhow!("error.noJava")),
    }
}

/* ------------------------------------------------ getting one when there is none. */

/// Mojang's index of the JREs it ships, the same one the game's own installer reads. What is
/// fetched here lands where that one looks, so playing the version later downloads nothing.
const JAVA_MANIFEST: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

/// How many of a runtime's four hundred small files are fetched at once.
const PARALLEL_DOWNLOADS: usize = 8;

/// A JVM that can run this Minecraft, fetching one from Mojang if this machine has none.
///
/// `java()` answers whenever there already is one. This is the part that makes "Java is not a
/// prerequisite" true for a player who hosts a version they have never played - until now the
/// launcher only ever had a runtime because the game had downloaded one to play with.
pub async fn provide_java(
    client: &reqwest::Client,
    minecraft: &str,
    say: Say<'_>,
) -> Result<PathBuf> {
    let error = match java(minecraft) {
        Ok(java) => return Ok(java),
        Err(error) => error,
    };
    // A path the player typed is theirs to be wrong about. Quietly downloading a second JVM
    // and running on that would leave the setting doing nothing, with no way to tell.
    let chosen = crate::settings::Settings::load().java_path;
    if !chosen.trim().is_empty() {
        return Err(error);
    }
    download_java(client, java_needed(minecraft), say).await
}

/// Which of Mojang's runtimes clears a floor. They are named rather than numbered: `gamma` is
/// 17, `delta` is 21, and `jre-legacy` is the 8 that everything before 1.17 runs on.
fn component_for(needed: u32) -> &'static str {
    match needed {
        0..=8 => "jre-legacy",
        9..=17 => "java-runtime-gamma",
        _ => "java-runtime-delta",
    }
}

/// The key Mojang files its runtimes under for this machine.
///
/// Mostly `os-arch`, except where they only ever built one and dropped the suffix: Linux is
/// plain "linux" unless it is 32-bit, and an Apple Silicon Mac gets the Intel build of Java 8
/// because no ARM one was ever made.
fn platform(needed: u32) -> Result<String> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "mac-os",
        "windows" => "windows",
        _ => return Err(anyhow!("error.noJava")),
    };
    let arch = match std::env::consts::ARCH {
        "x86" if os == "linux" => "i386",
        "x86" => "x86",
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return Err(anyhow!("error.noJava")),
    };
    let sole_build =
        (os == "linux" && arch != "i386") || (os == "mac-os" && (arch != "arm64" || needed <= 8));
    Ok(if sole_build {
        os.to_string()
    } else {
        format!("{os}-{arch}")
    })
}

/// Fetch the runtime Mojang publishes for this floor, and return the `java` inside it.
async fn download_java(client: &reqwest::Client, needed: u32, say: Say<'_>) -> Result<PathBuf> {
    let component = component_for(needed);
    let platform = platform(needed)?;

    let index: lyceris::json::java::JavaManifest = client
        .get(JAVA_MANIFEST)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("reading Mojang's list of Java runtimes")?;
    let build = index
        .get(&platform)
        .and_then(|by_component| by_component.get(component))
        .and_then(|builds| builds.first())
        .ok_or_else(|| anyhow!("error.noJava"))?;

    let version = build.version.name.clone();
    say("host.java", version.clone(), 0, 0);

    let listing: lyceris::json::java::JavaFileManifest = client
        .get(&build.manifest.url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .with_context(|| format!("reading the file list for Java {version}"))?;

    // Built beside the real name and moved over at the end: half a runtime is worse than none,
    // because it looks installed and then dies on the first class it cannot find. A failed run
    // leaves the staging directory behind, and the next one starts by clearing it.
    let runtimes = crate::paths::minecraft().join("runtimes");
    let home = runtimes.join(component);
    let staging = crate::paths::minecraft()
        .join("java-download")
        .join(component);
    let _ = std::fs::remove_dir_all(&staging);

    let mut wanted = Vec::new();
    let mut links = Vec::new();
    for (name, file) in &listing.files {
        let path = staging.join(name.replace('/', std::path::MAIN_SEPARATOR_STR));
        match file.r#type.as_str() {
            "directory" => {
                std::fs::create_dir_all(&path)?;
            }
            "link" => links.push((path, file.target.clone().unwrap_or_default())),
            _ => {
                if let Some(downloads) = &file.downloads {
                    wanted.push((
                        path,
                        downloads.raw.url.clone(),
                        file.executable == Some(true),
                    ));
                }
            }
        }
    }
    // The list is a map, so a file can come up before the directory holding it does.
    let parents = wanted
        .iter()
        .map(|(path, ..)| path)
        .chain(links.iter().map(|(path, _)| path));
    for parent in parents.filter_map(|path| path.parent()) {
        std::fs::create_dir_all(parent)?;
    }

    let total = wanted.len() as u64;
    let mut done = 0u64;
    use futures::StreamExt;
    let mut running = futures::stream::iter(wanted.into_iter().map(|(path, url, executable)| {
        let client = client.clone();
        async move {
            let bytes = fetch(&client, &url)
                .await
                .with_context(|| format!("fetching {url}"))?;
            std::fs::write(&path, &bytes)?;
            #[cfg(unix)]
            if executable {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
            }
            #[cfg(not(unix))]
            let _ = executable;
            Ok::<(), anyhow::Error>(())
        }
    }))
    .buffer_unordered(PARALLEL_DOWNLOADS);

    while let Some(outcome) = running.next().await {
        outcome?;
        done += 1;
        say("host.java", version.clone(), done, total);
    }

    // Only Mojang's licence texts are links, and a Windows symlink needs a privilege the
    // launcher has no business asking for, so there they are simply left out.
    #[cfg(unix)]
    for (path, target) in &links {
        let _ = std::os::unix::fs::symlink(target, path);
    }
    #[cfg(not(unix))]
    let _ = &links;

    std::fs::create_dir_all(&runtimes)?;
    let _ = std::fs::remove_dir_all(&home);
    std::fs::rename(&staging, &home)?;

    // Asked rather than assumed: a runtime that will not say what it is would be invisible to
    // `runtimes()` on the next start, and downloaded again every single time.
    let java = binary_in(&home).map(|(_, java)| java);
    match java.as_deref().and_then(probe_major) {
        Some(major) if major >= needed => Ok(java.unwrap_or_default()),
        _ => {
            let _ = std::fs::remove_dir_all(&home);
            Err(anyhow!("error.noJava"))
        }
    }
}

/* -------------------------------------------------------------- making one. */

/// What to build. A pack decides its own version and loader, so those fields are only read
/// when there is no pack.
pub struct Spec {
    pub name: String,
    pub minecraft: String,
    pub loader: Option<String>,
    pub loader_version: Option<String>,
    pub port: u16,
    pub memory: u64,
    /// Whether whoever asked for this says they have read https://aka.ms/MinecraftEULA.
    pub eula: bool,
    /// Put the publisher mod in, so a player with no mods installed can still join: the
    /// launcher pulls the pack from the server before it starts the game.
    pub publish: bool,
    /// A .mrpack, a CurseForge zip or a server pack to build it out of.
    pub pack: Option<PathBuf>,
    /// Needed to turn a CurseForge manifest's ids into files. Empty is the common case: an
    /// mrpack carries its own links, and a build without a key still imports those.
    pub cf_key: String,
}

/// Progress, in the shape the launcher already reports it: a dotted key naming the step, a
/// detail that is data rather than prose, and how far along it is. The CLI writes these to the
/// terminal in English; the launcher translates the key and shows the detail as it came.
pub type Say<'a> = &'a (dyn Fn(&str, String, u64, u64) + Send + Sync);

/// Build a server directory and write it down. Everything that touches the network is in here,
/// so the caller only has to have somewhere to put the messages.
pub async fn create(client: &reqwest::Client, spec: Spec, say: Say<'_>) -> Result<Hosted> {
    let id = format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let root = dir(&id);
    if root.exists() {
        return Err(anyhow!("{} already exists", root.display()));
    }
    std::fs::create_dir_all(&root)?;

    // Anything from here on can fail with a half-written directory behind it, so a failure
    // takes the directory with it rather than leaving a server that cannot start.
    match build(client, &root, &spec, say).await {
        Ok(mut server) => {
            server.id = id;
            let mut list = load();
            list.push(server.clone());
            save(&list)?;
            Ok(server)
        }
        Err(error) => {
            let _ = std::fs::remove_dir_all(&root);
            Err(error)
        }
    }
}

/// Provision one directory. `create` picks the directory and writes the server down; the CLI
/// names its own and writes nothing down, so this is the half they share.
pub async fn build(
    client: &reqwest::Client,
    root: &Path,
    spec: &Spec,
    say: Say<'_>,
) -> Result<Hosted> {
    // A pack overrules the picker: it was built against one version and one loader, and any
    // other pair is a server that starts and then drops every mod on the floor.
    let (minecraft, loader, loader_version, source, files) = match &spec.pack {
        Some(path) => {
            let mut pack = pack::read(path)?;
            say("host.read", format!("{} {}", pack.name, pack.version), 0, 0);

            // A CurseForge manifest names ids and nothing else; without this the pack is a
            // list of placeholders. Modrinth packs carry their own links and skip straight past.
            let resolved = pack::resolve_curseforge(client, &spec.cf_key, &mut pack).await;
            if resolved > 0 {
                say("host.resolved", resolved.to_string(), 0, 0);
            }

            let moved = pack::resolve_unknown_sides(client, &mut pack).await;
            if moved > 0 {
                say("host.clientOnly", moved.to_string(), 0, 0);
            }
            let (loader, version) = match pack.loader.clone() {
                Some((loader, version)) => (Some(loader), Some(version)),
                None => (None, None),
            };
            let count = write_pack(client, root, &pack, say).await?;
            say("host.wrote", count.to_string(), 0, 0);
            (
                pack.minecraft.clone(),
                loader,
                version.filter(|v| !v.is_empty()),
                Some(format!("{} {}", pack.name, pack.version)),
                pack.files.len(),
            )
        }
        None => (
            spec.minecraft.clone(),
            spec.loader
                .clone()
                .filter(|l| !l.is_empty() && l != "vanilla"),
            spec.loader_version.clone().filter(|v| !v.is_empty()),
            None,
            0,
        ),
    };

    if minecraft.trim().is_empty() {
        return Err(anyhow!("error.noVersionChosen"));
    }

    // Sized like the launcher sizes a client: the JVM's default quarter of the machine does
    // not hold a big pack, and a server that dies at 40 players is not a mystery.
    let memory = if spec.memory > 0 {
        spec.memory
    } else {
        crate::settings::heap_mb(0, files)
    };

    // Fetched here rather than demanded: hosting is often the first time a machine needs a
    // JVM at all, and "install Java first" is not a launcher doing its job.
    let java = provide_java(client, &minecraft, say).await?;
    let publishes = spec.publish && loader.as_deref().is_some_and(publisher_runs_on);

    if publishes {
        say("host.publisher", String::new(), 0, 0);
        let mods = root.join("mods");
        std::fs::create_dir_all(&mods)?;
        // Not fatal: everything else here still makes a working server, only one that hands
        // nothing to players who arrive without the pack.
        match fetch(client, PUBLISHER_URL).await {
            Ok(bytes) => std::fs::write(mods.join("unifiedmc-server.jar"), &bytes)?,
            Err(error) => say("host.publisherFailed", error.to_string(), 0, 0),
        }
    }

    // Resolved now rather than at every start: a build that exists today is what the start
    // script names, so a server keeps working when the loader publishes a breaking update.
    let loader_version = match (&loader, loader_version) {
        (Some(name), None) => {
            let kind =
                crate::loaders::Kind::parse(name).ok_or_else(|| anyhow!("error.unknownLoader"))?;
            Some(crate::loaders::latest(client, kind, &minecraft).await?)
        }
        (_, pinned) => pinned,
    };

    say(
        "host.loader",
        match (&loader, &loader_version) {
            (Some(name), Some(version)) => format!("{name} {version}"),
            _ => format!("Minecraft {minecraft}"),
        },
        0,
        0,
    );
    let command = install_loader(
        client,
        root,
        &java,
        loader.as_deref(),
        loader_version.as_deref().unwrap_or(""),
        &minecraft,
        memory,
        say,
    )
    .await?;

    write_config(root, &loader, &loader_version, &minecraft, spec.port)?;
    write_eula(root, spec.eula)?;
    write_properties(root, &spec.name, spec.port)?;
    write_scripts(root, &command)?;

    Ok(Hosted {
        // create() fills this in; it owns the directory name.
        id: String::new(),
        name: if spec.name.trim().is_empty() {
            format!("Minecraft {minecraft}")
        } else {
            spec.name.clone()
        },
        minecraft,
        loader,
        loader_version,
        port: spec.port,
        memory,
        source,
        command,
        eula: spec.eula,
        publishes,
    })
}

/// Where a file out of a pack belongs on a server.
///
/// A pack is written for a client, so `resourcepacks/` and `shaderpacks/` mean nothing to a
/// server and everything to the player. They go into the areas the publisher hands out, or a
/// resource pack reaches players as a config file.
pub fn place(path: &str, side: Side) -> PathBuf {
    let under = |prefix: &str| path.strip_prefix(prefix).unwrap_or(path).to_string();

    // Client-side by definition, whatever the pack says about sides: no server loads either.
    if let Some(rest) = path.strip_prefix("resourcepacks/") {
        return PathBuf::from("unifiedmc/client-resourcepacks").join(rest);
    }
    if let Some(rest) = path.strip_prefix("shaderpacks/") {
        return PathBuf::from("unifiedmc/client-shaders").join(rest);
    }
    // A datapack is world data. The server's own live in the world directory and it applies
    // them itself; one a pack marks client-only is for the player's own copy of the world.
    for prefix in ["datapacks/", "world/datapacks/"] {
        if let Some(rest) = path.strip_prefix(prefix) {
            return match side {
                Side::ClientOnly => PathBuf::from("unifiedmc/client-datapacks").join(rest),
                _ => PathBuf::from("world/datapacks").join(rest),
            };
        }
    }

    match (side, path.starts_with("mods/")) {
        (Side::ClientOnly, true) => PathBuf::from("unifiedmc/client").join(under("mods/")),
        // client-config/ mirrors config/, so the prefix must not be repeated inside it
        (Side::ClientOnly, false) => {
            PathBuf::from("unifiedmc/client-config").join(under("config/"))
        }
        (_, _) => PathBuf::from(path),
    }
}

/// Unpack a pack into a server directory. Returns how many files landed.
async fn write_pack(
    client: &reqwest::Client,
    root: &Path,
    pack: &pack::Pack,
    say: Say<'_>,
) -> Result<usize> {
    // Left over when the pack is a CurseForge manifest and no key could resolve it. Writing
    // them as files would make a mods/ full of entries called "cf:".
    if pack.files.iter().any(|f| f.path.starts_with("cf://")) {
        return Err(anyhow!("error.curseforgeKeyNeeded"));
    }

    let mut written = 0usize;
    let mut failed: Vec<String> = Vec::new();
    let total = pack.files.len();

    for file in &pack.files {
        // A pack is a zip somebody else wrote, and every member name is attacker text.
        if !plain_relative(&file.path) || pack::is_private(&file.path) {
            continue;
        }
        let target = root.join(place(&file.path, file.side));
        std::fs::create_dir_all(
            target
                .parent()
                .ok_or_else(|| anyhow!("odd path: {}", file.path))?,
        )?;

        if let Some(bytes) = &file.bytes {
            std::fs::write(&target, bytes)?;
        } else if let Some(url) = &file.url {
            // One dead link must not end a two hundred file import; collect and report at the
            // end so the whole list arrives at once.
            match fetch(client, url).await {
                // The pack states a hash for a reason: a repointed link would otherwise become
                // a jar in mods/, and from there a jar in every player's.
                Ok(bytes) => match verify(&bytes, file.sha1.as_deref()) {
                    Ok(()) => std::fs::write(&target, &bytes)?,
                    Err(error) => {
                        failed.push(format!("{}: {error}", file.path));
                        continue;
                    }
                },
                Err(error) => {
                    failed.push(format!("{}: {error}", file.path));
                    continue;
                }
            }
        } else {
            continue;
        }
        written += 1;
        say(
            "host.files",
            file.path
                .rsplit('/')
                .next()
                .unwrap_or(&file.path)
                .to_string(),
            written as u64,
            total as u64,
        );
    }

    if !pack.blocked.is_empty() {
        say("host.blocked", pack.blocked.join(", "), 0, 0);
    }
    if !failed.is_empty() {
        say("host.failed", failed.join("\n"), 0, 0);
    }
    Ok(written)
}

fn verify(bytes: &[u8], expected: Option<&str>) -> Result<()> {
    use sha1::{Digest, Sha1};
    let Some(expected) = expected else {
        return Ok(()); // a pack that states no hash cannot be held to one
    };
    let got = hex::encode(Sha1::digest(bytes));
    if got.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    Err(anyhow!("hash mismatch: expected {expected}, got {got}"))
}

async fn fetch(client: &reqwest::Client, url: &str) -> Result<bytes::Bytes> {
    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?)
}

/* --------------------------------------------------------- the files around it. */

/// What the publisher mod reads to decide what it tells clients to install.
pub fn write_config(
    root: &Path,
    loader: &Option<String>,
    loader_version: &Option<String>,
    minecraft: &str,
    port: u16,
) -> Result<()> {
    let name = loader.as_deref().unwrap_or("vanilla");
    let version = loader_version.as_deref().unwrap_or("");
    // The publisher answers on its own port, next to the game's.
    let http = port.saturating_add(1);

    let config = format!(
        "#UnifiedMC - what this server tells clients to install\n\
         port={http}\n\
         loader={name}\n\
         loader-version={version}\n\
         minecraft={minecraft}\n\
         \n\
         # Remote control is off without a token. Generate one with:\n\
         #   unifiedmc-server-cli token\n\
         #admin-token=\n"
    );
    std::fs::create_dir_all(root.join("config"))?;
    std::fs::write(root.join("config/unifiedmc.properties"), config)?;

    std::fs::create_dir_all(root.join("unifiedmc/client"))?;
    std::fs::create_dir_all(root.join("unifiedmc/client-config"))?;
    let server_only = root.join("unifiedmc/server-only.txt");
    if !server_only.exists() {
        std::fs::write(
            &server_only,
            "# One jar filename per line: mods this server loads but clients must NOT get.\n\
             # Client-only mods do not belong here - those live in unifiedmc/client/.\n",
        )?;
    }
    Ok(())
}

/// Minecraft will not start without being told the licence was read, and that is the person's
/// statement to make, not ours.
pub fn write_eula(root: &Path, accepted: bool) -> Result<()> {
    std::fs::write(
        root.join("eula.txt"),
        if accepted {
            "# Accepted in UnifiedMC by whoever made this server\neula=true\n"
        } else {
            "eula=false\n"
        },
    )?;
    Ok(())
}

fn write_properties(root: &Path, name: &str, port: u16) -> Result<()> {
    let properties = root.join("server.properties");
    if properties.exists() {
        return Ok(());
    }
    // online-mode stays true: this is somebody's own server, not a way around owning the game.
    std::fs::write(
        properties,
        format!(
            "motd={name}\nserver-port={port}\nonline-mode=true\nmax-players=20\n\
             view-distance=10\nsimulation-distance=8\nlevel-name=world\nmotd-escaped=false\n"
        ),
    )?;
    Ok(())
}

/// Something to double-click, for the day this directory is copied to a real machine.
///
/// `java`, not the JVM this launcher found: the path in `command` points inside one player's
/// `~/.unifiedmc`, and a script carrying it is a script that only runs on the machine it was
/// written on. Anywhere else it is whatever java is on PATH, which is the whole convention.
fn write_scripts(root: &Path, command: &[String]) -> Result<()> {
    let portable: Vec<&str> = std::iter::once("java")
        .chain(command.iter().skip(1).map(String::as_str))
        .collect();
    let line = portable.join(" ");
    std::fs::write(
        root.join("start.sh"),
        format!("#!/bin/sh\ncd \"$(dirname \"$0\")\"\nexec {line}\n"),
    )?;
    std::fs::write(
        root.join("start.bat"),
        format!("@echo off\r\ncd /d \"%~dp0\"\r\n{line}\r\npause\r\n"),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            root.join("start.sh"),
            std::fs::Permissions::from_mode(0o755),
        )?;
    }
    Ok(())
}

/// Put the loader's server in place. Returns the argv that starts it, heap flags included.
#[allow(clippy::too_many_arguments)]
pub async fn install_loader(
    client: &reqwest::Client,
    root: &Path,
    java: &Path,
    loader: Option<&str>,
    version: &str,
    minecraft: &str,
    heap: u64,
    say: Say<'_>,
) -> Result<Vec<String>> {
    let java = java.to_string_lossy().into_owned();
    let jar = |name: &str| {
        vec![
            java.clone(),
            format!("-Xmx{heap}M"),
            format!("-Xms{heap}M"),
            "-jar".into(),
            name.into(),
            "nogui".into(),
        ]
    };

    match loader {
        // Mojang's own jar. The manifest names a per-version file; there is no "latest server".
        None => {
            let url = vanilla_server_url(client, minecraft).await?;
            let bytes = fetch(client, &url)
                .await
                .with_context(|| format!("fetching the Minecraft server from {url}"))?;
            std::fs::write(root.join("server.jar"), &bytes)?;
            Ok(jar("server.jar"))
        }
        Some(name @ ("fabric" | "quilt")) => {
            let installer = fetch_json_field(
                client,
                "https://meta.fabricmc.net/v2/versions/installer",
                "version",
            )
            .await
            .unwrap_or_else(|| "1.0.1".into());
            let url = format!(
                "https://meta.fabricmc.net/v2/versions/loader/{minecraft}/{version}/{installer}/server/jar"
            );
            let bytes = fetch(client, &url)
                .await
                .with_context(|| format!("fetching the {name} server from {url}"))?;
            std::fs::write(root.join("server.jar"), &bytes)?;
            Ok(jar("server.jar"))
        }
        Some(name) => {
            let url = if name == "forge" {
                format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{version}/forge-{version}-installer.jar")
            } else {
                format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar")
            };
            let bytes = fetch(client, &url)
                .await
                .with_context(|| format!("fetching the {name} installer from {url}"))?;
            let installer = root.join("installer.jar");
            std::fs::write(&installer, &bytes)?;

            say("host.installer", name.to_string(), 0, 0);
            // The installer writes the libraries and its own run script. It needs a JVM, and
            // the one we found is the same one that will run the result.
            let status = Command::new(&java)
                .arg("-jar")
                .arg("installer.jar")
                .arg("--install-server")
                .arg(".")
                .current_dir(root)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await
                .with_context(|| format!("running the {name} installer"))?;
            if !status.success() {
                return Err(anyhow!("the {name} installer exited with {status}"));
            }
            let _ = std::fs::remove_file(&installer);
            let _ = std::fs::remove_file(root.join("installer.jar.log"));

            // run.sh reads this. A -Xmx after -jar would be an argument to the program.
            std::fs::write(
                root.join("user_jvm_args.txt"),
                format!("# Read by run.sh. One option per line.\n-Xmx{heap}M\n-Xms{heap}M\n"),
            )?;

            // The installer writes a launcher jar and an argument file per platform; running
            // that file directly is what run.sh does, without needing a shell.
            let args = args_file(root, minecraft, version, name)?;
            Ok(vec![
                java.clone(),
                format!("-Xmx{heap}M"),
                format!("-Xms{heap}M"),
                format!("@{args}"),
                "nogui".into(),
            ])
        }
    }
}

/// The `@argfile` Forge and NeoForge write, named the way their own run script names it.
fn args_file(root: &Path, minecraft: &str, version: &str, loader: &str) -> Result<String> {
    let platform = if cfg!(windows) {
        "win_args.txt"
    } else {
        "unix_args.txt"
    };
    let candidates = [
        format!("libraries/net/neoforged/neoforge/{version}/{platform}"),
        format!("libraries/net/minecraftforge/forge/{minecraft}-{version}/{platform}"),
        format!("libraries/net/minecraftforge/forge/{version}/{platform}"),
    ];
    candidates
        .iter()
        .find(|path| root.join(path).is_file())
        .cloned()
        .ok_or_else(|| anyhow!("the {loader} installer wrote no {platform}"))
}

/// Mojang publishes the server jar per version, behind the same manifest the client uses.
async fn vanilla_server_url(client: &reqwest::Client, minecraft: &str) -> Result<String> {
    let manifest: serde_json::Value = client
        .get("https://launchermeta.mojang.com/mc/game/version_manifest_v2.json")
        .send()
        .await?
        .json()
        .await?;

    let entry = manifest
        .get("versions")
        .and_then(|v| v.as_array())
        .and_then(|versions| {
            versions
                .iter()
                .find(|v| v.get("id").and_then(|id| id.as_str()) == Some(minecraft))
        })
        .ok_or_else(|| anyhow!("Mojang lists no Minecraft {minecraft}"))?;

    let url = entry
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow!("Minecraft {minecraft} has no version file"))?;

    let version: serde_json::Value = client.get(url).send().await?.json().await?;
    version
        .pointer("/downloads/server/url")
        .and_then(|u| u.as_str())
        .map(str::to_string)
        // Every version before 1.2.5 predates the dedicated server download.
        .ok_or_else(|| anyhow!("Minecraft {minecraft} publishes no server jar"))
}

async fn fetch_json_field(client: &reqwest::Client, url: &str, field: &str) -> Option<String> {
    let list: serde_json::Value = client.get(url).send().await.ok()?.json().await.ok()?;
    list.as_array()?
        .iter()
        .find(|entry| entry.get("stable").and_then(|s| s.as_bool()) == Some(true))
        .or_else(|| list.as_array()?.first())?
        .get(field)?
        .as_str()
        .map(str::to_string)
}

/* ------------------------------------------------------------------- running. */

/// How much console to keep. Enough to scroll back through a start-up, not enough to grow
/// without bound over a weekend.
const SCROLLBACK: usize = 600;

struct Live {
    child: Child,
    stdin: ChildStdin,
}

/// The servers running right now, and what they have said.
///
/// The console is kept separately from the process so it survives the server exiting: a crash
/// on start-up is exactly the moment the last twenty lines matter.
pub struct Running {
    watcher: std::sync::Arc<dyn Watcher>,
    live: tokio::sync::Mutex<HashMap<String, Live>>,
    console: Mutex<HashMap<String, Vec<String>>>,
    players: Mutex<HashMap<String, Vec<String>>>,
    /// How many are up, readable without awaiting anything. The window-close handler runs on
    /// the UI thread and has to decide whether to wait before it can await to find out.
    count: std::sync::atomic::AtomicUsize,
}

impl Default for Running {
    fn default() -> Self {
        Self::new(std::sync::Arc::new(()))
    }
}

/// One line, and who said it.
#[derive(Clone, Serialize)]
pub struct ConsoleLine {
    pub id: String,
    pub line: String,
}

/// Where a running server's output goes.
///
/// The launcher forwards it to the window; a test collects it. Kept as a trait rather than an
/// `AppHandle` so that starting and stopping a real server is something a test can do - it is
/// the half of this module a unit test cannot otherwise reach.
pub trait Watcher: Send + Sync + 'static {
    fn line(&self, line: ConsoleLine);
    /// A server started or stopped; whatever is drawing the list has to read it again.
    fn changed(&self);
}

/// For tests, and for a `Running` built before anything is listening.
impl Watcher for () {
    fn line(&self, _line: ConsoleLine) {}
    fn changed(&self) {}
}

impl Running {
    pub fn new(watcher: std::sync::Arc<dyn Watcher>) -> Self {
        Self {
            watcher,
            live: Default::default(),
            console: Default::default(),
            players: Default::default(),
            count: Default::default(),
        }
    }

    pub async fn is_running(&self, id: &str) -> bool {
        self.live.lock().await.contains_key(id)
    }

    /// Whether anything is up at all, without waiting for the lock.
    pub fn any(&self) -> bool {
        self.count.load(std::sync::atomic::Ordering::Relaxed) > 0
    }

    /// Called after every change to `live`, and only while it is held.
    fn recount(&self, live: &HashMap<String, Live>) {
        self.count
            .store(live.len(), std::sync::atomic::Ordering::Relaxed);
    }

    pub async fn ids(&self) -> Vec<String> {
        self.live.lock().await.keys().cloned().collect()
    }

    pub fn console(&self, id: &str) -> Vec<String> {
        self.console
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn players(&self, id: &str) -> Vec<String> {
        self.players
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    fn push(&self, id: &str, line: String) {
        {
            let mut console = self.console.lock().unwrap_or_else(|e| e.into_inner());
            let lines = console.entry(id.to_string()).or_default();
            lines.push(line.clone());
            if lines.len() > SCROLLBACK {
                lines.drain(..lines.len() - SCROLLBACK);
            }
        }
        self.watcher.line(ConsoleLine {
            id: id.to_string(),
            line,
        });
    }

    /// Start it. Returns once the process is up - not once the world has loaded, which the
    /// console says in its own time.
    pub async fn start(self: &std::sync::Arc<Self>, server: &Hosted) -> Result<()> {
        let mut live = self.live.lock().await;
        if live.contains_key(&server.id) {
            return Err(anyhow!("error.alreadyRunning"));
        }
        if !server.eula {
            return Err(anyhow!("error.eulaNotAccepted"));
        }
        let root = dir(&server.id);
        if !root.is_dir() {
            return Err(anyhow!("error.serverGone"));
        }
        let (_, args) = server
            .command
            .split_first()
            .ok_or_else(|| anyhow!("error.serverGone"))?;
        // Not command[0]: that path was resolved when the server was made, and the runtime it
        // names can be gone, or the player can have pointed at their own Java since. Asking
        // again is what makes both of those a fix rather than a rebuild.
        let program = java(&server.minecraft)?;

        self.console
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(server.id.clone(), Vec::new());
        self.players
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(server.id.clone(), Vec::new());

        let mut child = Command::new(&program)
            .args(args)
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Without this the server outlives a killed launcher on Windows as an orphan
            // holding the port, and the next start fails with "address already in use".
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("starting {}", program.display()))?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr"))?;

        for stream in [
            Box::new(stdout) as Box<dyn tokio::io::AsyncRead + Send + Unpin>,
            Box::new(stderr),
        ] {
            let this = self.clone();
            let id = server.id.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stream).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    this.note_player(&id, &line);
                    this.push(&id, line);
                }
            });
        }

        // One watcher, on the process rather than on either stream: stdout closing is not the
        // same as the server having exited, and the row must only clear when it really has.
        {
            let this = self.clone();
            let id = server.id.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                    let mut live = this.live.lock().await;
                    let Some(entry) = live.get_mut(&id) else {
                        return; // stopped by hand; whoever did that reported it
                    };
                    if let Ok(Some(status)) = entry.child.try_wait() {
                        live.remove(&id);
                        this.recount(&live);
                        drop(live);
                        this.push(&id, format!("--- {status}"));
                        this.stopped(&id);
                        return;
                    }
                }
            });
        }

        live.insert(server.id.clone(), Live { child, stdin });
        self.recount(&live);
        drop(live);
        self.watcher.changed();
        Ok(())
    }

    /// Type a line at the server's console, as though it had been typed at a terminal.
    pub async fn send(&self, id: &str, line: &str) -> Result<()> {
        let mut live = self.live.lock().await;
        let entry = live
            .get_mut(id)
            .ok_or_else(|| anyhow!("error.notRunning"))?;
        entry.stdin.write_all(line.as_bytes()).await?;
        entry.stdin.write_all(b"\n").await?;
        entry.stdin.flush().await?;
        Ok(())
    }

    /// Ask it to stop. The console command, not a signal: a Minecraft server that is killed
    /// loses whatever chunks it had not written yet.
    pub async fn stop(&self, id: &str) -> Result<()> {
        self.send(id, "stop").await
    }

    /// Ask every running server to stop, and wait for them to finish writing.
    ///
    /// A Minecraft server keeps recent chunk edits in memory and flushes them on `stop`; a
    /// process that is killed instead loses them, and a world can be left mid-write. So the
    /// window closing has to wait rather than take the runtime down with the children still
    /// attached to it.
    pub async fn stop_all(&self, patience: std::time::Duration) {
        let ids: Vec<String> = self.live.lock().await.keys().cloned().collect();
        if ids.is_empty() {
            return;
        }
        for id in &ids {
            let _ = self.stop(id).await;
        }

        let deadline = tokio::time::Instant::now() + patience;
        while tokio::time::Instant::now() < deadline {
            let mut live = self.live.lock().await;
            live.retain(|_, entry| !matches!(entry.child.try_wait(), Ok(Some(_))));
            self.recount(&live);
            if live.is_empty() {
                return;
            }
            drop(live);
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        // Out of patience. Killing beats leaving the process orphaned holding the port, and
        // it has had every chance to save by now.
        let mut live = self.live.lock().await;
        for (_, mut entry) in live.drain() {
            let _ = entry.child.start_kill();
        }
        self.recount(&live);
    }

    /// The last resort, for a server that ignored `stop`. The world may lose recent changes.
    pub async fn kill(&self, id: &str) -> Result<()> {
        let mut live = self.live.lock().await;
        let mut entry = live.remove(id).ok_or_else(|| anyhow!("error.notRunning"))?;
        self.recount(&live);
        let _ = entry.child.kill().await;
        drop(live);
        self.stopped(id);
        Ok(())
    }

    fn stopped(&self, id: &str) {
        self.players
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        self.watcher.changed();
    }

    /// Who is on. Read out of the console, because a server with no query port and no RCON
    /// still says it in plain words.
    fn note_player(&self, id: &str, line: &str) {
        let Some((name, joined)) = join_or_leave(line) else {
            return;
        };
        let mut players = self.players.lock().unwrap_or_else(|e| e.into_inner());
        let list = players.entry(id.to_string()).or_default();
        list.retain(|player| player != name);
        if joined {
            list.push(name.to_string());
        }
    }
}

/// `[12:00:00] [Server thread/INFO]: Steve joined the game` - and the Forge shape, which puts
/// a second bracketed source in before the colon.
fn join_or_leave(line: &str) -> Option<(&str, bool)> {
    let message = line.rsplit_once("]: ").map(|(_, rest)| rest)?.trim();
    // A chat message is `<Steve> ...`, and a player may type any sentence they like.
    if message.starts_with('<') {
        return None;
    }
    let (name, joined) = match message.strip_suffix(" joined the game") {
        Some(name) => (name, true),
        None => (message.strip_suffix(" left the game")?, false),
    };
    // A name is one word. Anything else came out of a plugin quoting a player.
    (!name.is_empty() && !name.contains(' ')).then_some((name, joined))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_is_sorted_into_the_areas_the_publisher_hands_out() {
        let at = |path: &str, side| place(path, side).to_string_lossy().replace('\\', "/");

        // The regression: a resource pack used to land in the config area, which delivered it
        // to players as a config file - into config/resourcepacks/, where it does nothing.
        assert_eq!(
            at("resourcepacks/Fresh.zip", Side::ClientOnly),
            "unifiedmc/client-resourcepacks/Fresh.zip"
        );
        assert_eq!(
            at("shaderpacks/BSL.zip", Side::Both),
            "unifiedmc/client-shaders/BSL.zip",
            "no server loads a shader, whatever the pack says about sides"
        );
        assert_eq!(
            at("datapacks/vanilla-tweaks.zip", Side::Both),
            "world/datapacks/vanilla-tweaks.zip"
        );
        assert_eq!(
            at("world/datapacks/extra.zip", Side::ClientOnly),
            "unifiedmc/client-datapacks/extra.zip"
        );
        assert_eq!(
            at("mods/sodium.jar", Side::ClientOnly),
            "unifiedmc/client/sodium.jar"
        );
        assert_eq!(at("mods/create.jar", Side::Both), "mods/create.jar");
        assert_eq!(
            at("config/sodium.json", Side::ClientOnly),
            "unifiedmc/client-config/sodium.json"
        );
        assert_eq!(at("config/create.toml", Side::Both), "config/create.toml");
        // the prefix is stripped once, not repeatedly
        assert_eq!(
            at("mods/mods/odd.jar", Side::ClientOnly),
            "unifiedmc/client/mods/odd.jar"
        );
    }

    #[test]
    fn a_pack_name_becomes_a_directory_name_not_a_path() {
        assert_eq!(
            sanitise("All of Create Aeronautics"),
            "all-of-create-aeronautics"
        );
        assert_eq!(sanitise("../../etc"), "etc");
        assert_eq!(sanitise("///"), "pack");
    }

    #[test]
    fn a_pack_states_a_hash_so_that_it_gets_checked() {
        let bytes = b"jar bytes";
        use sha1::{Digest, Sha1};
        let real = hex::encode(Sha1::digest(bytes));
        assert!(verify(bytes, Some(&real)).is_ok());
        assert!(verify(bytes, Some(&real.to_uppercase())).is_ok());
        assert!(verify(bytes, None).is_ok(), "no hash is not a failure");
        assert!(verify(bytes, Some("da39a3ee5e6b4b0d3255bfef95601890afd80709")).is_err());
    }

    #[test]
    fn a_start_script_does_not_carry_one_machine_s_java_path() {
        let root = std::env::temp_dir().join("unifiedmc-script-test");
        std::fs::create_dir_all(&root).unwrap();
        write_scripts(
            &root,
            &[
                "/home/someone/.unifiedmc/mc/runtimes/java-runtime-delta/bin/java".into(),
                "-Xmx4096M".into(),
                "-jar".into(),
                "server.jar".into(),
                "nogui".into(),
            ],
        )
        .unwrap();

        let script = std::fs::read_to_string(root.join("start.sh")).unwrap();
        assert!(!script.contains(".unifiedmc"), "{script}");
        assert!(
            script.contains("java -Xmx4096M -jar server.jar nogui"),
            "{script}"
        );
        // and the flags after it survive, which is what the substitution could break
        assert!(std::fs::read_to_string(root.join("start.bat"))
            .unwrap()
            .contains("java -Xmx4096M -jar server.jar nogui"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn java_follows_the_two_versions_mojang_moved_on() {
        assert_eq!(java_needed("1.8.9"), 8);
        assert_eq!(java_needed("1.16.5"), 8);
        assert_eq!(java_needed("1.17.1"), 17);
        assert_eq!(java_needed("1.20.1"), 17);
        assert_eq!(java_needed("1.20.4"), 17);
        // the move nobody remembers: 1.20.5, not 1.21
        assert_eq!(java_needed("1.20.5"), 21);
        assert_eq!(java_needed("1.21.8"), 21);
        assert_eq!(java_needed("1.21"), 21);
    }

    #[test]
    fn a_release_file_names_its_java_in_two_numbering_schemes() {
        let home = std::env::temp_dir().join("unifiedmc-release-test");
        std::fs::create_dir_all(&home).unwrap();

        std::fs::write(
            home.join("release"),
            "JAVA_VERSION=\"21.0.4\"\nOS_ARCH=\"x86_64\"\n",
        )
        .unwrap();
        assert_eq!(release_major(&home), Some(21));

        // Java numbered itself 1.8 until 9, and the runtimes Mojang ships for old versions
        // still say so.
        std::fs::write(home.join("release"), "JAVA_VERSION=\"1.8.0_452\"\n").unwrap();
        assert_eq!(release_major(&home), Some(8));

        std::fs::write(home.join("release"), "OS_NAME=\"Linux\"\n").unwrap();
        assert_eq!(release_major(&home), None);

        let _ = std::fs::remove_dir_all(&home);
    }

    /// The line `java -version` prints, from the three JVMs anyone is likely to have. The
    /// version sits in the first pair of quotes on every one of them.
    #[test]
    fn a_jvm_announces_its_major_in_the_first_quoted_field() {
        let first = |banner: &str| major_of(banner.split('"').nth(1).unwrap());
        assert_eq!(
            first("openjdk version \"21.0.4\" 2024-07-16\nOpenJDK Runtime Environment"),
            Some(21)
        );
        assert_eq!(first("java version \"1.8.0_452\""), Some(8));
        assert_eq!(first("openjdk version \"17.0.11\" 2024-04-16"), Some(17));

        // A binary that is not a JVM says nothing of the sort, and must not read as Java 0.
        assert_eq!("bash: no such thing".split('"').nth(1), None);
    }

    /// The floor is what a wrong-Java crash is: below it the server starts and dies on the
    /// first class it loads, so the check has to happen before anything is spawned.
    #[test]
    fn the_floor_is_what_decides_whether_a_jvm_may_run_it() {
        assert!(major_of("17.0.11").unwrap() < java_needed("1.21.1"));
        assert!(major_of("21.0.4").unwrap() >= java_needed("1.21.1"));
        assert!(major_of("1.8.0_452").unwrap() >= java_needed("1.16.5"));
        assert!(major_of("1.8.0_452").unwrap() < java_needed("1.17.1"));
    }

    /// The three names Mojang gives its runtimes, against the floors this launcher asks for.
    #[test]
    fn every_floor_has_a_runtime_mojang_publishes_for_it() {
        assert_eq!(component_for(java_needed("1.16.5")), "jre-legacy");
        assert_eq!(component_for(java_needed("1.20.4")), "java-runtime-gamma");
        assert_eq!(component_for(java_needed("1.21.8")), "java-runtime-delta");
    }

    /// Mojang drops the arch suffix wherever it only ever built one, and the key has to match
    /// theirs exactly or the runtime is simply not in the index.
    #[test]
    fn the_platform_key_matches_the_one_mojang_files_the_build_under() {
        let key = platform(21).unwrap();
        let published = [
            "linux",
            "linux-i386",
            "mac-os",
            "mac-os-arm64",
            "windows-arm64",
            "windows-x64",
            "windows-x86",
        ];
        assert!(published.contains(&key.as_str()), "{key}");

        // No ARM build of Java 8 was ever made, so an Apple Silicon Mac runs the Intel one.
        if key == "mac-os-arm64" {
            assert_eq!(platform(8).unwrap(), "mac-os");
        }
    }

    #[test]
    fn the_console_says_who_arrived_and_who_left() {
        assert_eq!(
            join_or_leave("[12:00:00] [Server thread/INFO]: Steve joined the game"),
            Some(("Steve", true))
        );
        assert_eq!(
            join_or_leave(
                "[12:00:00] [Server thread/INFO] [minecraft/MinecraftServer]: Alex left the game"
            ),
            Some(("Alex", false))
        );
        // A player can type anything, and their chat line must not add a player named after it
        assert_eq!(
            join_or_leave("[12:00:00] [Server thread/INFO]: <Steve> Notch joined the game"),
            None
        );
        assert_eq!(
            join_or_leave("[12:00:00] [Server thread/INFO]: Done (12.5s)! For help, type \"help\""),
            None
        );
        assert_eq!(join_or_leave("no timestamp at all"), None);
    }

    /// The download, once, against the real internet. Java 17 rather than a floor this machine
    /// is likely to already clear, so the fetch actually happens.
    ///
    /// Ignored by default because it pulls a whole JRE into the launcher's own runtimes
    /// directory - where it then stays, and gets reused, which is the point:
    ///
    ///   cargo test --lib host::tests::a_runtime_this_machine_lacks_is_fetched -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "downloads a JRE from Mojang"]
    async fn a_runtime_this_machine_lacks_is_fetched() {
        let client = reqwest::Client::new();
        let java = download_java(&client, 17, &|phase, detail, done, total| {
            if done == 0 || done == total {
                println!("{phase} {detail} {done}/{total}");
            }
        })
        .await
        .expect("no runtime");

        assert!(java.is_file(), "{java:?}");
        assert_eq!(probe_major(&java), Some(17));
        // The next start has to find it without downloading it again.
        assert!(runtimes().iter().any(|(major, _)| *major == 17));
    }

    /// The whole feature, once, against the real internet: build a vanilla server, start it,
    /// wait for it to say it is up, stop it, and check it exited.
    ///
    /// Ignored by default because it downloads Mojang's server jar and needs a JRE on disk -
    /// neither belongs in `cargo test`. Run it by hand after touching anything in here:
    ///
    ///   cargo test --lib host::tests::a_vanilla_server_is_built_and_runs -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "downloads a server jar and starts a JVM"]
    async fn a_vanilla_server_is_built_and_runs() {
        use std::sync::Arc;

        let root = std::env::temp_dir().join("unifiedmc-host-e2e");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let client = reqwest::Client::builder()
            .user_agent("UnifiedMC-test")
            .build()
            .unwrap();

        let mut server = build(
            &client,
            &root,
            &Spec {
                name: "e2e".into(),
                minecraft: "1.21.1".into(),
                loader: None,
                loader_version: None,
                port: 25599,
                // Small on purpose: this has to start on a build machine, not a games rig.
                memory: 1536,
                eula: true,
                publish: false,
                pack: None,
                cf_key: String::new(),
            },
            &|phase, detail, _, _| println!("  {phase} {detail}"),
        )
        .await
        .expect("building the server");
        server.id = "e2e".into();

        assert!(root.join("server.jar").is_file(), "no server jar");
        assert!(root.join("start.sh").is_file(), "no start script");
        assert!(std::fs::read_to_string(root.join("eula.txt"))
            .unwrap()
            .contains("eula=true"));
        assert!(server.command[0].ends_with("java"), "{:?}", server.command);

        // start() reads the directory the id names, so the record has to point at this one.
        let hosted = dir(&server.id);
        let _ = std::fs::remove_dir_all(&hosted);
        std::fs::create_dir_all(hosted.parent().unwrap()).unwrap();
        std::fs::rename(&root, &hosted).unwrap();

        let running = Arc::new(Running::default());
        running.start(&server).await.expect("starting the server");

        // A cold first start unpacks the world; two minutes is generous and still bounded.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(120);
        let mut up = false;
        while tokio::time::Instant::now() < deadline {
            if running
                .console("e2e")
                .iter()
                .any(|line| line.contains("Done ("))
            {
                up = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        let log = running.console("e2e").join("\n");
        assert!(up, "the server never finished starting:\n{log}");

        running.stop("e2e").await.expect("stopping the server");
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        while running.any() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
        assert!(!running.any(), "the server ignored stop");

        let _ = std::fs::remove_dir_all(&hosted);
    }

    #[test]
    fn a_player_is_listed_once_however_often_they_reconnect() {
        let running = Running::default();
        for line in [
            "[Server thread/INFO]: Steve joined the game",
            "[Server thread/INFO]: Alex joined the game",
            "[Server thread/INFO]: Steve left the game",
            "[Server thread/INFO]: Steve joined the game",
            "[Server thread/INFO]: Steve joined the game",
        ] {
            running.note_player("s1", line);
        }
        assert_eq!(running.players("s1"), vec!["Alex", "Steve"]);
    }
}
