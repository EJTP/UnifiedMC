//! Provisioning an instance and starting the game.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use lyceris::auth::AuthMethod;
use lyceris::minecraft::config::{ConfigBuilder, Memory, Profile};
use lyceris::minecraft::emitter::{Emitter, Event};
use lyceris::minecraft::install::install;
use lyceris::minecraft::launch::launch;
use lyceris::minecraft::loader::{
    fabric::Fabric, forge::Forge, neoforge::NeoForge, quilt::Quilt, Loader,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter as TauriEmitter};

use crate::servers::Manifest;
use crate::session::Session;
use crate::{paths, settings::Settings, sync};

/// What the window shows while the player waits. The work happens elsewhere; without this
/// they would watch a spinner through a four hundred megabyte download.
#[derive(Clone, Serialize)]
pub struct Progress {
    pub phase: String,
    pub detail: String,
    pub done: u64,
    pub total: u64,
}

pub fn report(app: &AppHandle, phase: &str, detail: &str, done: u64, total: u64) {
    let _ = app.emit(
        "unifiedmc://progress",
        Progress {
            phase: phase.into(),
            detail: detail.into(),
            done,
            total,
        },
    );
}

fn loader_for(manifest: &Manifest) -> Result<Option<Box<dyn Loader>>> {
    let Some(loader) = manifest.loader.as_ref() else {
        return Ok(None);
    };
    let version = loader.version.clone().ok_or_else(|| {
        anyhow!(
            "the server did not say which {} version it runs",
            loader.kind
        )
    })?;

    Ok(Some(match loader.kind.to_lowercase().as_str() {
        "fabric" => Box::new(Fabric(version)) as Box<dyn Loader>,
        "neoforge" => Box::new(NeoForge(version)),
        "forge" => Box::new(Forge(version)),
        "quilt" => Box::new(Quilt(version)),
        other => return Err(anyhow!("unsupported loader: {other}")),
    }))
}

fn auth_for(session: &Session) -> AuthMethod {
    if session.is_online() {
        AuthMethod::Microsoft {
            username: session.name.clone(),
            xuid: String::new(),
            uuid: session.uuid.clone(),
            access_token: session.token.clone(),
            refresh_token: String::new(),
        }
    } else {
        AuthMethod::Offline {
            username: session.name.clone(),
            uuid: Some(session.uuid.clone()),
        }
    }
}

/// The arguments that send the game straight into a server.
///
/// Only an address belongs here. An instance key is a directory name, and handing that to
/// --quickPlayMultiplayer asks the client to connect to a folder.
fn quick_play(join: Option<&str>) -> Vec<String> {
    match join {
        Some(address) => vec!["--quickPlayMultiplayer".into(), address.into()],
        None => Vec::new(),
    }
}

/// Bring an instance in line with the manifest and start it.
///
/// `key` names the instance directory and the profile the player's own mods live in; `join` is
/// the server to connect to on start, or none to land in Minecraft's own menu. They are not the
/// same string: joining a server with a personal instance uses that instance's key and the
/// server's address at once.
pub async fn run(
    app: AppHandle,
    client: reqwest::Client,
    key: String,
    join: Option<String>,
    manifest: Manifest,
    settings: Settings,
    session: Session,
) -> Result<()> {
    let instance = paths::instance(&key);
    std::fs::create_dir_all(&instance)?;

    // Nobody wants to set their keybindings up again because they joined a different server.
    if let Err(error) = crate::options::apply(&instance) {
        eprintln!("could not apply shared settings: {error}");
    }

    report(&app, "progress.mods.sync", "", 0, 0);
    let mods_dir = instance.join("mods");
    let synced = {
        let app = app.clone();
        sync::mods(
            &client,
            &manifest.mods,
            &mods_dir,
            move |done, total, name| {
                report(
                    &app,
                    "progress.mods.download",
                    name,
                    done as u64,
                    total as u64,
                );
            },
        )
        .await?
    };
    report(
        &app,
        "progress.mods.summary",
        &format!("{} / {}", synced.downloaded, synced.total),
        synced.downloaded as u64,
        synced.total as u64,
    );

    if !manifest.config.is_empty() {
        report(&app, "progress.config", "", 0, 0);
        sync::config(&client, &manifest.config, &instance).await?;
    }

    // the player's own jars go in alongside, unless the server already ships them
    for jar in sync::personal(&manifest.mods, &paths::profiles().join(&key)) {
        if let Some(name) = jar.file_name() {
            let _ = std::fs::copy(&jar, mods_dir.join(name));
        }
    }

    let memory = crate::settings::heap_mb(settings.memory, manifest.mods.len());
    let jvm = crate::settings::jvm_args(&settings, memory);
    report(
        &app,
        "progress.java",
        &format!("{} · {memory} MB", manifest.minecraft),
        0,
        0,
    );

    let emitter = Emitter::default();
    {
        let app = app.clone();
        emitter
            .on(
                Event::MultipleDownloadProgress,
                move |(path, done, total, kind): (String, u64, u64, String)| {
                    let name = PathBuf::from(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or(path);
                    // the kind comes out of lyceris in English; it belongs in the detail, which
                    // the window shows as it is
                    report(
                        &app,
                        "progress.download",
                        &format!("{kind}: {name}"),
                        done,
                        total,
                    );
                },
            )
            .await;
    }

    let builder = ConfigBuilder::new(
        paths::minecraft(),
        manifest.minecraft.clone(),
        auth_for(&session),
    )
    .memory(Memory::Megabyte(memory))
    .custom_java_args(jvm)
    .profile(Profile {
        name: key.clone(),
        root: paths::instances(),
    })
    .custom_args(quick_play(join.as_deref()));

    match loader_for(&manifest)? {
        Some(loader) => {
            let config = builder.loader(loader).build();
            install(&config, Some(&emitter)).await?;
            report(&app, "progress.launch", &manifest.minecraft, 0, 0);
            let mut child = launch(&config, Some(&emitter)).await?;
            child.wait().await?;
        }
        None => {
            let config = builder.build();
            install(&config, Some(&emitter)).await?;
            report(&app, "progress.launch", &manifest.minecraft, 0, 0);
            let mut child = launch(&config, Some(&emitter)).await?;
            child.wait().await?;
        }
    }

    // Whatever the player changed in game goes back to the shared file, so the next instance
    // starts where this one left off.
    if let Err(error) = crate::options::collect(&instance) {
        eprintln!("could not save shared settings: {error}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_server_address_is_ever_joined() {
        // an instance started on its own lands in the menu, not on a server named after a folder
        assert!(quick_play(None).is_empty());
        assert_eq!(
            quick_play(Some("mc.example.com:25565")),
            vec!["--quickPlayMultiplayer", "mc.example.com:25565"]
        );
    }
}
