pub mod auth;
pub mod catalogue;
pub mod host;
pub mod instances;
pub mod loaders;
pub mod modinfo;
pub mod motd;
pub mod options;
pub mod pack;
pub mod paths;
pub mod play;
pub mod playtime;
pub mod servers;
pub mod session;
pub mod settings;
pub mod skin;
pub mod sync;
pub mod update;

use anyhow::Result;
use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use catalogue::Kind;
use servers::{Manifest, SavedServer, ServerStatus};
use session::Session;
use settings::Settings;

/// One page of catalogue results. Large enough that the side filter still leaves a screenful.
const PAGE: usize = 40;

/// What a player may add on top of what this server ships.
///
/// A pack owns the world's data, so datapacks are the one category that cannot be piled onto
/// it. The rest is client-side: a pack or shader cannot reach the server, and the catalogue
/// already drops a mod whose server side is required. No pack, no pack to break - all four.
fn allowed(manifest: &Manifest) -> Vec<Kind> {
    catalogue::KINDS
        .into_iter()
        .filter(|kind| manifest.mods.is_empty() || *kind != Kind::Datapack)
        // Without a loader nothing here loads a mod, so offering the whole catalogue offers
        // jars that cannot run. Resource packs, shaders and datapacks are the game's own and
        // need no loader at all.
        .filter(|kind| manifest.loader.is_some() || *kind != Kind::Mod)
        .collect()
}

/// What a running server's output is forwarded to: the window, as an event per line.
struct Window(AppHandle);

impl host::Watcher for Window {
    fn line(&self, line: host::ConsoleLine) {
        let _ = self.0.emit("unifiedmc://console", line);
    }

    fn changed(&self) {
        let _ = self.0.emit("unifiedmc://hosts", ());
    }
}

struct App {
    client: reqwest::Client,
    /// The servers this launcher started. Owned here because the readers that follow their
    /// console outlive the command that started them.
    running: std::sync::Arc<host::Running>,
}

/// Tauri wants an error it can serialise; anyhow carries the context we actually want to show.
fn failed(error: anyhow::Error) -> String {
    format!("{error:#}")
}

#[derive(Serialize)]
struct Bootstrap {
    servers: Vec<SavedServer>,
    settings: Settings,
    session: Session,
    /// Minecraft's own placeholder. None until a version is installed.
    unknown_server_icon: Option<String>,
}

/// The player's face. Its own command so the window need not wait on Mojang.
#[tauri::command]
async fn player_head(state: State<'_, App>) -> Result<Option<String>, String> {
    let settings = Settings::load();
    let session = session::current(&state.client, &settings.offline_name).await;
    Ok(skin::head(&state.client, &session.uuid, session.is_online()).await)
}

#[tauri::command]
async fn bootstrap(state: State<'_, App>) -> Result<Bootstrap, String> {
    let settings = Settings::load();
    let session = session::current(&state.client, &settings.offline_name).await;
    Ok(Bootstrap {
        servers: servers::load(),
        settings,
        session,
        unknown_server_icon: modinfo::unknown_server_icon(),
    })
}

#[tauri::command]
fn add_server(name: String, address: String) -> Result<Vec<SavedServer>, String> {
    let address = address.trim().to_string();
    if address.is_empty() {
        return Err("error.noAddress".into());
    }
    let mut list = servers::load();
    let name = if name.trim().is_empty() {
        address.clone()
    } else {
        name
    };
    list.push(SavedServer {
        id: format!("{}-{}", address, list.len()),
        name,
        address,
        loader: None,
        minecraft: None,
    });
    servers::save(&list).map_err(failed)?;
    Ok(list)
}

/// What the player wants this instance to be. Both are corrections to detection: a proxy
/// answers with the oldest protocol it accepts, and a Paper server announces no loader.
#[tauri::command]
async fn configure(
    state: State<'_, App>,
    id: String,
    minecraft: Option<String>,
    loader: Option<String>,
) -> Result<Vec<SavedServer>, String> {
    let minecraft = minecraft.filter(|v| !v.is_empty());
    let loader = loader.filter(|l| !l.is_empty() && l != "vanilla");

    // Resolved before storing, so an impossible combination fails here and not mid-launch.
    if let Some(name) = loader.as_deref() {
        let kind = loaders::Kind::parse(name).ok_or_else(|| "error.unknownLoader".to_string())?;
        let version = match minecraft.clone() {
            Some(chosen) => chosen,
            None => {
                let list = servers::load();
                let server = list
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| "error.noSuchServer".to_string())?;
                let (host, port) = servers::split_address(&server.address);
                let status = servers::ping(&host, port).await.map_err(failed)?;
                servers::minecraft_version(&state.client, &status)
                    .await
                    .map_err(failed)?
            }
        };
        loaders::latest(&state.client, kind, &version)
            .await
            .map_err(|error| format!("{name} {version}: {error}"))?;
    }

    let mut list = servers::load();
    for entry in &mut list {
        if entry.id == id {
            entry.minecraft = minecraft.clone();
            entry.loader = loader.clone();
        }
    }
    servers::save(&list).map_err(failed)?;
    Ok(list)
}

#[tauri::command]
fn instances() -> Vec<instances::Instance> {
    instances::load()
}

#[tauri::command]
async fn add_instance(
    state: State<'_, App>,
    name: String,
    minecraft: String,
    loader: Option<String>,
    loader_version: Option<String>,
) -> Result<Vec<instances::Instance>, String> {
    if minecraft.trim().is_empty() {
        return Err("error.noVersionChosen".into());
    }
    let loader = loader.filter(|l| !l.is_empty() && l != "vanilla");
    // empty means "whatever is newest at launch", which is not the same as a pinned build
    let loader_version = loader_version.filter(|v| !v.is_empty());

    // Checked before writing it down, so an impossible combination fails here.
    if let Some(name) = loader.as_deref() {
        let kind = loaders::Kind::parse(name).ok_or_else(|| "error.unknownLoader".to_string())?;
        loaders::latest(&state.client, kind, &minecraft)
            .await
            .map_err(|error| format!("{name} {minecraft}: {error}"))?;
    }

    let mut list = instances::load();
    list.push(instances::Instance {
        id: format!(
            "{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ),
        name: if name.trim().is_empty() {
            format!("Minecraft {minecraft}")
        } else {
            name
        },
        minecraft,
        loader,
        loader_version,
        source: None,
    });
    instances::save(&list).map_err(failed)?;
    Ok(list)
}

#[tauri::command]
fn remove_instance(id: String) -> Result<Vec<instances::Instance>, String> {
    let mut list = instances::load();
    list.retain(|instance| instance.id != id);
    instances::save(&list).map_err(failed)?;
    Ok(list)
}

/// Start an instance: into Minecraft's own menu, where the player picks a world.
#[tauri::command]
async fn play_instance(app: AppHandle, state: State<'_, App>, id: String) -> Result<(), String> {
    let settings = Settings::load();
    let session = session::current(&state.client, &settings.offline_name).await;

    let list = instances::load();
    let instance = list
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| "error.noSuchInstance".to_string())?;

    let mut manifest = servers::manifest_for_choice(
        instance.minecraft.clone(),
        instance.loader.as_deref().and_then(loader_of),
    );

    // A pinned build wins; otherwise resolve now, so a choice made months ago still installs
    if let Some(loader) = manifest.loader.as_mut() {
        loader.version = instance.loader_version.clone();
        if loader.version.is_none() {
            let kind = loaders::Kind::parse(&loader.kind)
                .ok_or_else(|| "error.unknownLoader".to_string())?;
            loader.version = Some(
                loaders::latest(&state.client, kind, &instance.minecraft)
                    .await
                    .map_err(failed)?,
            );
        }
    }

    playtime::timed(
        playtime::instance_key(&instance.id),
        play::run(
            app,
            state.client.clone(),
            instances::key(instance),
            // no address: the player picks a world, or a server, from Minecraft's own menu
            None,
            manifest,
            settings,
            session,
        ),
    )
    .await
    .map_err(failed)
}

/// Every Minecraft release, newest first. What the version picker offers.
#[tauri::command]
async fn versions(state: State<'_, App>) -> Result<Vec<String>, String> {
    let manifest: serde_json::Value = state
        .client
        .get("https://launchermeta.mojang.com/mc/game/version_manifest_v2.json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    Ok(manifest
        .get("versions")
        .and_then(|v| v.as_array())
        .map(|versions| {
            versions
                .iter()
                .filter(|v| v.get("type").and_then(|t| t.as_str()) == Some("release"))
                .filter_map(|v| v.get("id")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

/// Every loader build for this Minecraft version, newest first. Empty for Vanilla, which has none.
#[tauri::command]
async fn loader_versions(
    state: State<'_, App>,
    loader: String,
    minecraft: String,
) -> Result<Vec<String>, String> {
    let kind = loaders::Kind::parse(&loader).ok_or_else(|| "error.unknownLoader".to_string())?;
    loaders::versions(&state.client, kind, &minecraft)
        .await
        .map_err(failed)
}

#[tauri::command]
fn remove_server(id: String) -> Result<Vec<SavedServer>, String> {
    let mut list = servers::load();
    list.retain(|server| server.id != id);
    servers::save(&list).map_err(failed)?;
    Ok(list)
}

/// Everything the list row wants to show, for one server.
#[tauri::command]
async fn probe(state: State<'_, App>, id: String, address: String) -> Result<ServerStatus, String> {
    let client = state.client.clone();

    let (host, port) = servers::split_address(&address);

    let status = match servers::ping(&host, port).await {
        Ok(status) => status,
        Err(error) => {
            return Ok(ServerStatus {
                id,
                online: false,
                error: Some(format!("{error}")),
                motd: Vec::new(),
                icon: None,
                players: 0,
                max_players: 0,
                manifest: None,
            })
        }
    };

    let players = status.get("players");
    // the struct takes ownership of the id, and the lookup below still needs it
    let lookup = id.clone();
    Ok(ServerStatus {
        id,
        online: true,
        error: None,
        motd: status
            .get("description")
            .map(motd::parse)
            .unwrap_or_default(),
        // Minecraft already sends the favicon as a data: URI, so it goes straight through
        icon: status
            .get("favicon")
            .and_then(|v| v.as_str())
            .filter(|uri| uri.starts_with("data:image/png;base64,"))
            .map(str::to_string),
        players: players
            .and_then(|p| p.get("online"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        max_players: players
            .and_then(|p| p.get("max"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        manifest: match servers::manifest(&client, &host, port, &status).await {
            Some(published) => Some(published),
            // Nothing published: show what the player chose, so the row reads "1.21.1 fabric".
            None => chosen_manifest(&state, &lookup, &status).await,
        },
    })
}

/// Ping a server, read its manifest, and name the instance and profile that belong to it.
/// Every command the mod browser has starts here.
fn instance_id(address: &str) -> Option<&str> {
    address.strip_prefix("instance-")
}

fn host_id(address: &str) -> Option<&str> {
    address.strip_prefix("host-")
}

/// Neither of these is on the other side of a network, so the catalogue's client-side filter
/// would only hide most of itself for no reason - and on a server the player runs, a
/// server-side mod is the point rather than the thing to leave out.
fn own_setup(address: &str) -> bool {
    instance_id(address).is_some() || host_id(address).is_some()
}

/// Where the files a player adds for this address actually go.
///
/// A profile for anything reached over the network: the launcher copies those in beside what
/// the server sent. A server hosted here loads its own directory, so there is no second copy
/// to make and no profile to keep.
fn content_dir(address: &str, key: &str, dir: &str) -> std::path::PathBuf {
    match host_id(address) {
        Some(id) => host::dir(id).join(dir),
        None => paths::profiles().join(key).join(dir),
    }
}

/// Every folder that counts as "installed here".
///
/// One, except for mods on a hosted server: those are split across what the server loads and
/// what it only hands out, and a player who added Sodium wants to see it listed and be able to
/// take it away again - not have it vanish because it landed on the other side.
fn content_dirs(address: &str, key: &str, dir: &str) -> Vec<std::path::PathBuf> {
    let mut folders = vec![content_dir(address, key, dir)];
    if let Some(id) = host_id(address) {
        if dir == "mods" {
            folders.push(host::dir(id).join("unifiedmc/client"));
        }
    }
    folders
}

async fn served(state: &State<'_, App>, address: &str) -> Result<(Manifest, String), String> {
    // An instance is not an address. "instance-<id>" resolved as a hostname is how this failed:
    // a DNS error about a profile on the local disk.
    if let Some(id) = host_id(address) {
        let server = host::find(id).ok_or_else(|| "error.serverGone".to_string())?;
        let manifest = servers::manifest_for_choice(
            server.minecraft.clone(),
            server.loader.as_deref().and_then(loader_of),
        );
        return Ok((manifest, address.to_string()));
    }

    if let Some(id) = instance_id(address) {
        let list = instances::load();
        let instance = list
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| "error.noSuchInstance".to_string())?;
        let manifest = servers::manifest_for_choice(
            instance.minecraft.clone(),
            instance.loader.as_deref().and_then(loader_of),
        );
        return Ok((manifest, instances::key(instance)));
    }

    let (host, port) = servers::split_address(address);
    let status = servers::ping(&host, port).await.map_err(failed)?;

    let manifest = match servers::manifest(&state.client, &host, port, &status).await {
        Some(published) => published,
        // A server that publishes no pack is exactly where this screen is wanted - nothing to
        // conflict with. Falling back to the player's own choice is what makes it reachable.
        None => {
            let id = servers::load()
                .into_iter()
                .find(|saved| saved.address == address)
                .map(|saved| saved.id)
                .unwrap_or_default();
            chosen_manifest(state, &id, &status)
                .await
                .ok_or_else(|| "error.noManifest".to_string())?
        }
    };

    let key = servers::instance_key(address, &manifest);
    Ok((manifest, key))
}

/// The mod ids already sitting in a folder, for telling the catalogue what not to offer.
fn installed_ids(folder: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jar"))
        .filter_map(|path| modinfo::read(&path))
        .map(|info| info.id.to_lowercase())
        .filter(|id| !id.is_empty())
        .collect()
}

/// One catalogue row for a jar on disk.
///
/// A mod jar carries a display name, a description, a version and usually an icon. Listing it
/// as its filename throws that away and makes the player's own mods look worse than anything
/// the catalogue offers.
fn hit_from_jar(
    jar: &std::path::Path,
    source: &str,
    on_server: bool,
    installed: bool,
    removable: bool,
) -> catalogue::Hit {
    let filename = jar
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let info = modinfo::read(jar);

    catalogue::Hit {
        id: filename.clone(),
        title: info
            .as_ref()
            .map(|i| i.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or(filename),
        // The version moved out of the description and into its own field, so the line under
        // a title is a description and nothing else.
        description: info
            .as_ref()
            .map(|i| i.description.clone())
            .unwrap_or_default(),
        downloads: 0,
        source: source.into(),
        on_server,
        installed,
        author: String::new(),
        version: info.as_ref().map(|i| i.version.clone()).unwrap_or_default(),
        // A jar on this disk has no catalogue page to point at.
        url: String::new(),
        removable,
        icon: info.and_then(|i| i.icon),
    }
}

/// A stored loader name as something the launcher can install. The version is resolved at
/// launch, so a stale choice picks up newer builds on its own.
fn loader_of(name: &str) -> Option<servers::Loader> {
    let kind = loaders::Kind::parse(name)?;
    (kind != loaders::Kind::Vanilla).then(|| servers::Loader {
        kind: kind.as_str().to_string(),
        version: None,
    })
}

/// The manifest a server without one gets: the player's own loader choice.
async fn chosen_manifest(
    state: &State<'_, App>,
    id: &str,
    status: &serde_json::Value,
) -> Option<servers::Manifest> {
    let saved = servers::load();
    let server = saved.iter().find(|s| s.id == id)?;
    let (chosen, choice) = (server.minecraft.clone(), server.loader.clone());

    // A proxy answers the ping with the OLDEST protocol it accepts, so an explicit choice has
    // to outrank detection - and it is the only thing that rescues a server whose protocol
    // maps to no known release at all.
    let minecraft = match chosen {
        Some(version) => version,
        None => servers::minecraft_version(&state.client, status)
            .await
            .ok()?,
    };

    let Some(name) = choice else {
        return Some(servers::manifest_for_choice(minecraft, None));
    };
    let kind = loaders::Kind::parse(&name)?;
    if kind == loaders::Kind::Vanilla {
        return Some(servers::manifest_for_choice(minecraft, None));
    }

    let version = loaders::latest(&state.client, kind, &minecraft)
        .await
        .ok()?;
    Some(servers::manifest_for_choice(
        minecraft,
        Some(servers::Loader {
            kind: kind.as_str().to_string(),
            version: Some(version),
        }),
    ))
}

#[tauri::command]
async fn play(
    app: AppHandle,
    state: State<'_, App>,
    address: String,
    // an instance to join with instead of the server's own setup: the player's mods come along
    instance: Option<String>,
) -> Result<(), String> {
    let client = state.client.clone();
    let settings = Settings::load();
    let session = session::current(&state.client, &settings.offline_name).await;

    // The instance decides what runs; the server only decides where to connect.
    if let Some(id) = instance.filter(|id| !id.is_empty()) {
        let list = instances::load();
        let chosen = list
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| "error.noSuchInstance".to_string())?;

        let mut manifest = servers::manifest_for_choice(
            chosen.minecraft.clone(),
            chosen.loader.as_deref().and_then(loader_of),
        );
        if let Some(loader) = manifest.loader.as_mut() {
            loader.version = chosen.loader_version.clone();
            if loader.version.is_none() {
                let kind = loaders::Kind::parse(&loader.kind)
                    .ok_or_else(|| "error.unknownLoader".to_string())?;
                loader.version = Some(
                    loaders::latest(&state.client, kind, &chosen.minecraft)
                        .await
                        .map_err(failed)?,
                );
            }
        }

        // the instance owns the directory and the personal mods in it; the server only says
        // where to connect
        let key = instances::key(chosen);
        // Filed under the server, not the instance the player brought: they pressed play on
        // a server, and counting both would count the evening twice.
        return playtime::timed(
            playtime::server_key(&address),
            play::run(app, client, key, Some(address), manifest, settings, session),
        )
        .await
        .map_err(failed);
    }

    let (host, port) = servers::split_address(&address);

    play::report(&app, "progress.probe", &address, 0, 0);
    let status = servers::ping(&host, port).await.map_err(failed)?;

    let manifest = match servers::manifest(&client, &host, port, &status).await {
        Some(published) => published,
        None => {
            // A Vanilla or Paper server publishes nothing; the mods come from the player's profile.
            let id = servers::load()
                .into_iter()
                .find(|s| s.address == address)
                .map(|s| s.id)
                .unwrap_or_default();
            chosen_manifest(&state, &id, &status)
                .await
                .ok_or_else(|| "error.serverNoVersion".to_string())?
        }
    };

    let key = servers::instance_key(&address, &manifest);
    playtime::timed(
        playtime::server_key(&address),
        play::run(app, client, key, Some(address), manifest, settings, session),
    )
    .await
    .map_err(failed)
}

/// What a player is allowed to add here, as the names the command surface uses. The browser
/// asks before it draws its tabs; `install_mods` asks again, because a tab is not a boundary.
#[tauri::command]
async fn allowed_kinds(state: State<'_, App>, address: String) -> Result<Vec<String>, String> {
    let (manifest, _) = served(&state, &address).await?;
    Ok(allowed(&manifest)
        .into_iter()
        .map(|kind| kind.as_str().to_string())
        .collect())
}

/// One list for the browser, whichever tab is open. All three come back the same shape, so
/// the screen renders one kind of row.
#[tauri::command]
async fn mods(
    state: State<'_, App>,
    address: String,
    tab: String,
    kind: Kind,
    query: String,
    offset: usize,
    sort: catalogue::Sort,
) -> Result<Vec<catalogue::Hit>, String> {
    let client = state.client.clone();
    let (manifest, key) = served(&state, &address).await?;
    let dir = kind.dir();

    match tab.as_str() {
        // One list of what is actually here, whoever put it there: what the server ships
        // first, then the player's own. Two tabs for that was two places to look for one
        // answer, and the difference that matters - whether it can be taken away again - is
        // a property of the row, not of which tab it was found on.
        "installed" => {
            let mut here: Vec<catalogue::Hit> = manifest
                .entries(dir)
                .iter()
                .map(|entry| {
                    // The jar knows its own name and icon; the filename is a last resort. A sha1 out of the
                    // manifest names a blob or nothing at all.
                    let blob = if sync::is_hash(&entry.sha1) {
                        paths::blobs().join(&entry.sha1)
                    } else {
                        std::path::PathBuf::new()
                    };
                    // The jar knows its own name and icon; the filename is a last resort. A
                    // sha1 out of the manifest names a blob or nothing at all.
                    let mut hit = hit_from_jar(&blob, "pack", true, false, false);
                    if hit.title.is_empty() || hit.title == entry.sha1 {
                        hit.title = entry.name.clone();
                    }
                    hit.id = entry.sha1.clone();
                    hit
                })
                .collect();

            here.extend(
                content_dirs(&address, &key, dir)
                    .iter()
                    .flat_map(|folder| sync::personal(manifest.entries(dir), folder))
                    .map(|file| hit_from_jar(&file, "profile", false, true, true)),
            );
            Ok(here)
        }
        _ => {
            let catalogue = catalogue::Catalogue {
                client: &client,
                cf_key: settings::curseforge_key(),
            };
            let mut hits = catalogue
                .search(
                    &manifest,
                    kind,
                    catalogue::Search {
                        query: &query,
                        limit: PAGE,
                        offset,
                        sort,
                        own_setup: own_setup(&address),
                    },
                )
                .await
                .map_err(failed)?;

            // Mark what the player already has. Mods only: a jar declares an id, a pack zip does not.
            let mine: Vec<String> = match dir {
                "mods" => content_dirs(&address, &key, dir)
                    .iter()
                    .flat_map(|folder| installed_ids(folder))
                    .collect(),
                _ => Vec::new(),
            };
            for hit in &mut hits {
                let declared = hit.title.to_lowercase();
                hit.installed = mine.iter().any(|id| {
                    // a jar declares a mod id, a catalogue entry a slug and a title; both are worth checking
                    hit.id.trim_start_matches("cf:") == id
                        || hit.id == *id
                        || declared.replace(' ', "") == id.replace('-', "")
                });
            }
            Ok(hits)
        }
    }
}

#[tauri::command]
async fn install_mods(
    state: State<'_, App>,
    address: String,
    kind: Kind,
    ids: Vec<String>,
) -> Result<Vec<String>, String> {
    let client = state.client.clone();
    let (manifest, key) = served(&state, &address).await?;

    // Enforced here, not just by hiding a tab: a tab is a hint, this answers the command.
    if !allowed(&manifest).contains(&kind) {
        return Err("error.kindNotAllowed".into());
    }

    let catalogue = catalogue::Catalogue {
        client: &client,
        cf_key: settings::curseforge_key(),
    };
    let resolved = catalogue
        .resolve(&manifest, kind, &ids)
        .await
        .map_err(failed)?;

    let names: Vec<String> = resolved.iter().map(|entry| entry.name.clone()).collect();

    // add, not sync: reconciling the player's own folder against one answer would delete the
    // rest of what they installed
    for (dir, entries) in sorted_by_side(&client, &address, kind, &manifest, resolved).await {
        sync::add(&client, &entries, &content_dir(&address, &key, &dir))
            .await
            .map_err(failed)?;
    }

    Ok(names)
}

/// Which folder each pick belongs in, once.
///
/// Everywhere but a hosted server that is the folder the tab named. On a server the player
/// runs, it is not: a client-only mod in `mods/` takes the start down on the first class it
/// loads, so Modrinth is asked the same question the pack importer asks, and anything it calls
/// client-only goes to the area the publisher hands to players instead.
async fn sorted_by_side(
    client: &reqwest::Client,
    address: &str,
    kind: Kind,
    manifest: &Manifest,
    resolved: Vec<servers::ModEntry>,
) -> Vec<(String, Vec<servers::ModEntry>)> {
    let here = kind.dir().to_string();
    if host_id(address).is_none() || kind != Kind::Mod || resolved.is_empty() {
        return vec![(here, resolved)];
    }

    let mut probe = pack::Pack {
        name: String::new(),
        version: String::new(),
        minecraft: manifest.minecraft.clone(),
        loader: None,
        files: resolved
            .iter()
            .map(|entry| pack::PackFile {
                path: format!("mods/{}", entry.name),
                side: pack::Side::Both,
                sha1: Some(entry.sha1.clone()),
                url: None,
                bytes: None,
            })
            .collect(),
        blocked: Vec::new(),
    };
    // No network, no answer, and everything stays where the tab put it - which is the same
    // guess the pack importer falls back to.
    if pack::resolve_unknown_sides(client, &mut probe).await == 0 {
        return vec![(here, resolved)];
    }

    let client_only: std::collections::HashSet<&str> = probe
        .files
        .iter()
        .filter(|file| file.side == pack::Side::ClientOnly)
        .filter_map(|file| file.path.strip_prefix("mods/"))
        .collect();

    let (mine, theirs): (Vec<_>, Vec<_>) = resolved
        .into_iter()
        .partition(|entry| client_only.contains(entry.name.as_str()));

    vec![(here, theirs), ("unifiedmc/client".into(), mine)]
}

#[tauri::command]
async fn remove_mods(
    state: State<'_, App>,
    address: String,
    kind: Kind,
    names: Vec<String>,
) -> Result<Vec<String>, String> {
    let (_, key) = served(&state, &address).await?;
    let folders = content_dirs(&address, &key, kind.dir());

    let mut gone = Vec::new();
    for name in names {
        // only ever inside those folders, and only by bare filename
        let Some(file) = std::path::Path::new(&name).file_name() else {
            continue;
        };
        for folder in &folders {
            let target = folder.join(file);
            if target.is_file() && std::fs::remove_file(&target).is_ok() {
                gone.push(file.to_string_lossy().into_owned());
                break;
            }
        }
    }
    Ok(gone)
}

/// Put a new skin on the player's Minecraft profile.
///
/// Base64 because `<input type="file">` is the whole file picker. Needs a Microsoft session,
/// so an offline profile is told rather than watching the request fail.
#[tauri::command]
async fn set_skin(state: State<'_, App>, png_base64: String, slim: bool) -> Result<(), String> {
    let session = session::current(&state.client, &Settings::load().offline_name).await;
    if !session.is_online() {
        return Err("error.skinNeedsMicrosoft".into());
    }
    // Checked before the decode: the webview already passed a string, and decoding half a
    // gigabyte only to refuse it is the expensive half of the same answer.
    let png_base64 = png_base64.trim();
    if skin::too_much_base64(png_base64) {
        return Err("error.skinTooBig".into());
    }
    let png = base64::engine::general_purpose::STANDARD
        .decode(png_base64)
        .map_err(|_| "error.skinNotAnImage".to_string())?;

    skin::upload(&state.client, &session.token, &png, slim)
        .await
        .map_err(failed)
}

/// Back to whichever default Minecraft picks for this account.
#[tauri::command]
async fn reset_skin(state: State<'_, App>) -> Result<(), String> {
    let session = session::current(&state.client, &Settings::load().offline_name).await;
    if !session.is_online() {
        return Err("error.skinNeedsMicrosoft".into());
    }
    skin::reset(&state.client, &session.token)
        .await
        .map_err(failed)
}

/// What this machine has, so the memory picker cannot offer more than exists.
#[tauri::command]
fn data_dir() -> String {
    paths::data().to_string_lossy().into_owned()
}

/// Sign in with the device code flow. One command, not a start/poll pair: the pair would have
/// to park the device code between calls, and that code is what an attacker would want.
#[tauri::command]
async fn sign_in(app: AppHandle, state: State<'_, App>) -> Result<Session, String> {
    let (prompt, device_code) = auth::begin(&state.client).await.map_err(failed)?;

    // Emitted before the wait, because the wait is the player reading this off their screen.
    let _ = app.emit("unifiedmc://signin", prompt.clone());

    auth::finish(&state.client, &device_code, prompt.expires_in)
        .await
        .map_err(failed)
}

/// Forget the account. The offline profile is left, which still reaches offline-mode servers.
#[tauri::command]
fn sign_out() -> Session {
    auth::forget();
    Session::offline(&Settings::load().offline_name)
}

#[tauri::command]
fn machine_memory() -> u64 {
    settings::machine_memory_mb()
}

/// Exactly the flags a launch with these settings would pass, so the settings screen cannot
/// describe a JVM the launcher never starts.
#[tauri::command]
fn jvm_preview(settings: Settings, mods: usize) -> Vec<String> {
    let heap = settings::heap_mb(settings.memory, mods);
    // lyceris only ever emits -Xmx; showing an -Xms here would be a flag that is not passed
    let mut flags = vec![format!("-Xmx{heap}M")];
    flags.extend(settings::jvm_args(&settings, heap));
    flags
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<Settings, String> {
    settings.save().map_err(failed)?;
    Ok(settings)
}

/* ---------------------------------------------------------------- hosting. */

/// A hosted server as the window draws it: what was written down, plus what it is doing now.
#[derive(Serialize)]
struct HostView {
    #[serde(flatten)]
    server: host::Hosted,
    running: bool,
    players: Vec<String>,
    /// What to type into Minecraft's own "add server" box on this machine.
    address: String,
    /// Where its files are, for the button that opens the folder.
    directory: String,
}

async fn host_views(running: &host::Running) -> Vec<HostView> {
    let live = running.ids().await;
    host::load()
        .into_iter()
        .map(|server| HostView {
            running: live.contains(&server.id),
            players: running.players(&server.id),
            address: format!("localhost:{}", server.port),
            directory: host::dir(&server.id).to_string_lossy().into_owned(),
            server,
        })
        .collect()
}

#[tauri::command]
async fn hosts(state: State<'_, App>) -> Result<Vec<HostView>, String> {
    Ok(host_views(&state.running).await)
}

/// Build a server. Everything slow is in here, so it reports through the same overlay a
/// launch does - a two hundred mod pack is a long download either way.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn create_host(
    app: AppHandle,
    state: State<'_, App>,
    name: String,
    minecraft: String,
    loader: Option<String>,
    loader_version: Option<String>,
    port: u16,
    memory: u64,
    eula: bool,
    publish: bool,
    pack: Option<String>,
) -> Result<Vec<HostView>, String> {
    // Said before anything is downloaded: a server directory that cannot legally start is a
    // waste of the next four hundred megabytes.
    if !eula {
        return Err("error.eulaNotAccepted".into());
    }
    if port < 1024 {
        return Err("error.portTooLow".into());
    }
    if host::load().iter().any(|server| server.port == port) {
        return Err("error.portTaken".into());
    }

    let reporter = app.clone();
    host::create(
        &state.client,
        host::Spec {
            name,
            minecraft,
            loader,
            loader_version,
            port,
            memory,
            eula,
            publish,
            pack: pack.filter(|p| !p.is_empty()).map(std::path::PathBuf::from),
            cf_key: settings::curseforge_key().to_string(),
        },
        &move |phase, detail, done, total| play::report(&reporter, phase, &detail, done, total),
    )
    .await
    .map_err(failed)?;

    Ok(host_views(&state.running).await)
}

#[tauri::command]
async fn start_host(state: State<'_, App>, id: String) -> Result<(), String> {
    let server = host::find(&id).ok_or_else(|| "error.serverGone".to_string())?;
    state.running.start(&server).await.map_err(failed)
}

#[tauri::command]
async fn stop_host(state: State<'_, App>, id: String) -> Result<(), String> {
    state.running.stop(&id).await.map_err(failed)
}

/// For a server that ignored `stop`. Said out loud in the interface, because a killed server
/// loses whatever it had not written to disk.
#[tauri::command]
async fn kill_host(state: State<'_, App>, id: String) -> Result<(), String> {
    state.running.kill(&id).await.map_err(failed)
}

/// One line at the server's console, exactly as if it had been typed at a terminal.
#[tauri::command]
async fn host_command(state: State<'_, App>, id: String, line: String) -> Result<(), String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(());
    }
    state.running.send(&id, line).await.map_err(failed)
}

/// What it has said so far. The console keeps it, so opening the drawer late still shows the
/// start-up - which is exactly when something went wrong.
#[tauri::command]
fn host_console(state: State<'_, App>, id: String) -> Vec<String> {
    state.running.console(&id)
}

/// Forget a server, and optionally delete what it wrote. Refused while it is running: the
/// files are open, and Windows will not let go of them.
#[tauri::command]
async fn remove_host(
    state: State<'_, App>,
    id: String,
    delete_files: bool,
) -> Result<Vec<HostView>, String> {
    if state.running.is_running(&id).await {
        return Err("error.stopItFirst".into());
    }
    if delete_files {
        let directory = host::dir(&id);
        // Only ever inside the launcher's own hosted directory, whatever the id turned out
        // to be: the id is generated, but it arrives here from the webview.
        if directory.starts_with(host::root()) && directory != host::root() {
            std::fs::remove_dir_all(&directory).map_err(|error| format!("{error}"))?;
        }
    }
    let mut list = host::load();
    list.retain(|server| server.id != id);
    host::save(&list).map_err(failed)?;
    Ok(host_views(&state.running).await)
}

/// The player's own file manager, on the server's directory. Where anyone goes to edit
/// server.properties, drop in a world, or read the crash report.
#[tauri::command]
fn open_host_dir(app: AppHandle, id: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let directory = host::dir(&id);
    if !directory.is_dir() {
        return Err("error.serverGone".into());
    }
    app.opener()
        .open_path(directory.to_string_lossy(), None::<&str>)
        .map_err(|error| format!("{error}"))
}

/// Everything played, keyed the way `playtime` files it. Read whole: it is a few dozen
/// entries, and the window wants all of them at once to sort a list by them.
#[tauri::command]
fn playtime() -> playtime::Book {
    playtime::load()
}

/// Which day it is where the player is, so the trend can end on today rather than on
/// whatever the last session happened to be.
#[tauri::command]
fn today() -> playtime::Day {
    playtime::today()
}

/// A catalogue project's own page, in the player's browser.
///
/// Only the two catalogues we search. The url arrives back through the webview, and "open
/// anything in the browser" is a capability worth keeping pointed somewhere: a hit's link
/// comes from a third-party API, not from us.
#[tauri::command]
fn open_catalogue_page(app: AppHandle, url: String) -> Result<(), String> {
    const ALLOWED: [&str; 4] = [
        "https://modrinth.com/",
        "https://www.modrinth.com/",
        "https://www.curseforge.com/",
        "https://curseforge.com/",
    ];
    if !ALLOWED.iter().any(|prefix| url.starts_with(prefix)) {
        return Err("error.notACataloguePage".into());
    }
    update::open(&app, &url).map_err(failed)
}

/* ----------------------------------------------------------------- updates. */

/// Whether GitHub has a newer release than this build. None when it has not, or when it could
/// not be asked - neither is worth a message.
#[tauri::command]
async fn check_update(state: State<'_, App>) -> Result<Option<update::Release>, String> {
    Ok(update::check(&state.client).await)
}

#[tauri::command]
fn open_release(app: AppHandle, url: String) -> Result<(), String> {
    // Only ever our own releases page: the url comes back through the webview, and "open this
    // in the player's browser" is a capability worth keeping pointed somewhere.
    if !url.starts_with("https://github.com/EJTP/UnifiedMC/") {
        return Err("error.notOurRelease".into());
    }
    update::open(&app, &url).map_err(failed)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // In setup rather than manage(): the console watcher needs a handle to emit through,
        // and that only exists once the app has been built.
        .setup(|app| {
            app.manage(App {
                client: reqwest::Client::builder()
                    .user_agent(concat!("UnifiedMC/", env!("CARGO_PKG_VERSION")))
                    // reqwest has no default timeout, and a launch has no cancel button.
                    // Generous enough for a 400 MB pack on a bad line.
                    .timeout(std::time::Duration::from_secs(300))
                    .connect_timeout(std::time::Duration::from_secs(10))
                    .build()
                    .expect("http client"),
                running: std::sync::Arc::new(host::Running::new(std::sync::Arc::new(Window(
                    app.handle().clone(),
                )))),
            });
            Ok(())
        })
        // A hosted server is a child of this process, and dropping the runtime kills it where
        // it stands. Closing the window therefore asks each one to stop first and waits.
        .on_window_event(|window, event| {
            let tauri::WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            let running = window.state::<App>().running.clone();
            // Nothing to wait for, so the window shuts the way it always did.
            if !running.any() {
                return;
            }
            api.prevent_close();

            let window = window.clone();
            tauri::async_runtime::spawn(async move {
                // The window stays up while this runs, so it can say what it is waiting for.
                let _ = window.emit("unifiedmc://closing", ());
                running.stop_all(std::time::Duration::from_secs(25)).await;
                // destroy, not close: close would come back through this handler and wait again
                let _ = window.destroy();
            });
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            sign_in,
            sign_out,
            player_head,
            add_server,
            remove_server,
            configure,
            instances,
            add_instance,
            remove_instance,
            versions,
            loader_versions,
            probe,
            play,
            play_instance,
            mods,
            install_mods,
            remove_mods,
            allowed_kinds,
            set_skin,
            reset_skin,
            machine_memory,
            data_dir,
            jvm_preview,
            save_settings,
            hosts,
            create_host,
            start_host,
            stop_host,
            kill_host,
            host_command,
            host_console,
            remove_host,
            open_host_dir,
            check_update,
            open_release,
            playtime,
            today,
            open_catalogue_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_with_its_own_pack_keeps_its_world_data_to_itself() {
        let packed = Manifest {
            mods: vec![servers::ModEntry::default()],
            ..Default::default()
        };
        let names: Vec<&str> = allowed(&packed).iter().map(|k| k.as_str()).collect();
        // No loader in this manifest, so no mod can load either - the pack rule and the
        // loader rule both apply, and datapacks are the world's own.
        assert_eq!(names, vec!["resourcepack", "shader"]);
        // the one install_mods actually asks
        assert!(!allowed(&packed).contains(&Kind::Datapack));
    }

    #[test]
    fn a_server_with_no_pack_has_no_pack_to_break() {
        // vanilla, Paper, a proxy: nothing is published, so all four are the player's
        let names: Vec<&str> = allowed(&Manifest::default())
            .iter()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["resourcepack", "shader", "datapack"],
            "the command surface and rule 4 disagree"
        );
        // With a loader the fourth comes back: that is the half the filter is about.
        let with_loader = Manifest {
            loader: Some(servers::Loader {
                kind: "fabric".into(),
                version: None,
            }),
            ..Default::default()
        };
        let names: Vec<&str> = allowed(&with_loader).iter().map(|k| k.as_str()).collect();
        assert_eq!(names, vec!["mod", "resourcepack", "shader", "datapack"]);
    }
}
