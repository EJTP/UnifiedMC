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

/// Bring an instance in line with the manifest and start it on the given server.
pub async fn run(
    app: AppHandle,
    client: reqwest::Client,
    address: String,
    manifest: Manifest,
    settings: Settings,
    session: Session,
) -> Result<()> {
    let key = crate::servers::instance_key(&address, &manifest);
    let instance = paths::instance(&key);
    std::fs::create_dir_all(&instance)?;

    report(&app, "Mods werden abgeglichen", "", 0, 0);
    let mods_dir = instance.join("mods");
    let synced = {
        let app = app.clone();
        sync::mods(
            &client,
            &manifest.mods,
            &mods_dir,
            move |done, total, name| {
                report(&app, "Mods werden geladen", name, done as u64, total as u64);
            },
        )
        .await?
    };
    report(
        &app,
        "Mods",
        &format!(
            "{} vorhanden, {} geladen",
            synced.total - synced.downloaded,
            synced.downloaded
        ),
        0,
        0,
    );

    if !manifest.config.is_empty() {
        report(&app, "Konfiguration", "", 0, 0);
        sync::config(&client, &manifest.config, &instance).await?;
    }

    // the player's own jars go in alongside, unless the server already ships them
    for jar in sync::personal(&manifest.mods, &paths::profiles().join(&key)) {
        if let Some(name) = jar.file_name() {
            let _ = std::fs::copy(&jar, mods_dir.join(name));
        }
    }

    let memory = crate::settings::heap_mb(settings.memory, manifest.mods.len());
    report(
        &app,
        &format!("Minecraft {}", manifest.minecraft),
        &format!("{memory} MB"),
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
                    report(&app, &format!("{kind} werden geladen"), &name, done, total);
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
    .profile(Profile {
        name: key.clone(),
        root: paths::instances(),
    })
    .custom_args(vec!["--quickPlayMultiplayer".into(), address.clone()]);

    match loader_for(&manifest)? {
        Some(loader) => {
            let config = builder.loader(loader).build();
            install(&config, Some(&emitter)).await?;
            report(&app, "Minecraft startet", &manifest.minecraft, 0, 0);
            launch(&config, Some(&emitter)).await?;
        }
        None => {
            let config = builder.build();
            install(&config, Some(&emitter)).await?;
            report(&app, "Minecraft startet", &manifest.minecraft, 0, 0);
            launch(&config, Some(&emitter)).await?;
        }
    }

    Ok(())
}
