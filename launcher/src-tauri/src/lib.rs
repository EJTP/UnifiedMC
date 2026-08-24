pub mod catalogue;
pub mod pack;
pub mod paths;
pub mod play;
pub mod servers;
pub mod session;
pub mod settings;
pub mod sync;

use anyhow::Result;
use serde::Serialize;
use tauri::{AppHandle, State};

use servers::{SavedServer, ServerStatus};
use session::Session;
use settings::Settings;

/// One page of catalogue results. Large enough that the side filter still leaves a screenful.
const PAGE: usize = 40;

struct App {
    client: reqwest::Client,
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
}

#[tauri::command]
fn bootstrap() -> Bootstrap {
    let settings = Settings::load();
    let session = session::current(&settings.offline_name);
    Bootstrap {
        servers: servers::load(),
        settings,
        session,
    }
}

#[tauri::command]
fn add_server(name: String, address: String) -> Result<Vec<SavedServer>, String> {
    let address = address.trim().to_string();
    if address.is_empty() {
        return Err("Keine Adresse angegeben".into());
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
    });
    servers::save(&list).map_err(failed)?;
    Ok(list)
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
    let settings = Settings::load();
    let (host, port) = servers::split_address(&address);

    let status = match servers::ping(&host, port).await {
        Ok(status) => status,
        Err(error) => {
            return Ok(ServerStatus {
                id,
                online: false,
                error: Some(format!("{error}")),
                motd: String::new(),
                players: 0,
                max_players: 0,
                manifest: None,
            })
        }
    };

    let players = status.get("players");
    Ok(ServerStatus {
        id,
        online: true,
        error: None,
        motd: status
            .get("description")
            .map(servers::plain_motd)
            .unwrap_or_default(),
        players: players
            .and_then(|p| p.get("online"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        max_players: players
            .and_then(|p| p.get("max"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        manifest: servers::manifest(&client, &host, port, settings.manifest_port, &status).await,
    })
}

#[tauri::command]
async fn play(app: AppHandle, state: State<'_, App>, address: String) -> Result<(), String> {
    let client = state.client.clone();
    let settings = Settings::load();
    let session = session::current(&settings.offline_name);
    let (host, port) = servers::split_address(&address);

    play::report(&app, "Server wird abgefragt", &address, 0, 0);
    let status = servers::ping(&host, port).await.map_err(failed)?;

    let manifest = servers::manifest(&client, &host, port, settings.manifest_port, &status)
        .await
        .ok_or_else(|| {
            "Der Server veröffentlicht kein Manifest. Läuft dort das UnifiedMC-Server-Mod?"
                .to_string()
        })?;

    play::run(app, client, address, manifest, settings, session)
        .await
        .map_err(failed)
}

/// One list for the browser, whichever tab is open. All three come back the same shape, so
/// the screen renders one kind of row.
#[tauri::command]
async fn mods(
    state: State<'_, App>,
    address: String,
    tab: String,
    query: String,
    offset: usize,
) -> Result<Vec<catalogue::Hit>, String> {
    let client = state.client.clone();
    let settings = Settings::load();
    let (host, port) = servers::split_address(&address);

    let status = servers::ping(&host, port).await.map_err(failed)?;
    let manifest = servers::manifest(&client, &host, port, settings.manifest_port, &status)
        .await
        .ok_or_else(|| "Der Server veröffentlicht kein Manifest.".to_string())?;

    match tab.as_str() {
        "pack" => Ok(manifest
            .mods
            .iter()
            .map(|entry| catalogue::Hit {
                id: entry.sha1.clone(),
                title: entry.name.clone(),
                description: String::new(),
                downloads: 0,
                source: "pack".into(),
                on_server: true,
                icon: None,
            })
            .collect()),
        "installed" => {
            let key = servers::instance_key(&address, &manifest);
            Ok(
                sync::personal(&manifest.mods, &paths::profiles().join(&key))
                    .into_iter()
                    .map(|jar| {
                        let name = jar
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        catalogue::Hit {
                            id: name.clone(),
                            title: name,
                            description: "in deinem Profil".into(),
                            downloads: 0,
                            source: "profile".into(),
                            on_server: false,
                            icon: None,
                        }
                    })
                    .collect(),
            )
        }
        _ => {
            let catalogue = catalogue::Catalogue {
                client: &client,
                cf_key: &settings.curseforge_key,
            };
            catalogue
                .search(&manifest, &query, PAGE, offset)
                .await
                .map_err(failed)
        }
    }
}

#[tauri::command]
async fn install_mods(
    state: State<'_, App>,
    address: String,
    ids: Vec<String>,
) -> Result<Vec<String>, String> {
    let client = state.client.clone();
    let settings = Settings::load();
    let (host, port) = servers::split_address(&address);

    let status = servers::ping(&host, port).await.map_err(failed)?;
    let manifest = servers::manifest(&client, &host, port, settings.manifest_port, &status)
        .await
        .ok_or_else(|| "Der Server veröffentlicht kein Manifest.".to_string())?;

    let catalogue = catalogue::Catalogue {
        client: &client,
        cf_key: &settings.curseforge_key,
    };
    let resolved = catalogue.resolve(&manifest, &ids).await.map_err(failed)?;

    let key = servers::instance_key(&address, &manifest);
    let profile = paths::profiles().join(&key).join("mods");
    sync::mods(&client, &resolved, &profile, |_, _, _| {})
        .await
        .map_err(failed)?;

    Ok(resolved.into_iter().map(|entry| entry.name).collect())
}

#[tauri::command]
async fn remove_mods(
    state: State<'_, App>,
    address: String,
    names: Vec<String>,
) -> Result<Vec<String>, String> {
    let client = state.client.clone();
    let settings = Settings::load();
    let (host, port) = servers::split_address(&address);

    let status = servers::ping(&host, port).await.map_err(failed)?;
    let manifest = servers::manifest(&client, &host, port, settings.manifest_port, &status)
        .await
        .ok_or_else(|| "Der Server veröffentlicht kein Manifest.".to_string())?;

    let key = servers::instance_key(&address, &manifest);
    let profile = paths::profiles().join(&key).join("mods");

    let mut gone = Vec::new();
    for name in names {
        // only ever inside the player's own folder, and only by bare filename
        let Some(file) = std::path::Path::new(&name).file_name() else {
            continue;
        };
        let target = profile.join(file);
        if target.is_file() && std::fs::remove_file(&target).is_ok() {
            gone.push(file.to_string_lossy().into_owned());
        }
    }
    Ok(gone)
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<Settings, String> {
    settings.save().map_err(failed)?;
    Ok(settings)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(App {
            client: reqwest::Client::builder()
                .user_agent(concat!("UnifiedMC/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("http client"),
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            add_server,
            remove_server,
            probe,
            play,
            mods,
            install_mods,
            remove_mods,
            save_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
