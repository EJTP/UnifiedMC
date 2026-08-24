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
    let synced = {
        let app = app.clone();
        sync::into_dir(
            &client,
            &manifest.mods,
            &instance.join("mods"),
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

    // The rest of what the pack ships: the same job in a different directory.
    //
    // Only when the server actually publishes some. mods/ is the launcher's to fill, so an
    // empty list there really does mean "no mods" - but resourcepacks/ and shaderpacks/ are
    // where Minecraft itself puts whatever somebody dragged in, and reconciling an empty list
    // against those would quietly delete every pack the player added by hand.
    for (dir, phase) in [
        ("resourcepacks", "progress.resourcepacks"),
        ("shaderpacks", "progress.shaders"),
        ("datapacks", "progress.datapacks"),
    ] {
        let entries = manifest.entries(dir);
        if entries.is_empty() {
            continue;
        }
        report(&app, phase, "", 0, entries.len() as u64);
        let app = app.clone();
        sync::into_dir(
            &client,
            entries,
            &instance.join(dir),
            move |done, total, name| {
                report(&app, phase, name, done as u64, total as u64);
            },
        )
        .await?;
    }

    if !manifest.config.is_empty() {
        report(&app, "progress.config", "", 0, 0);
        sync::config(&client, &manifest.config, &instance).await?;
    }

    // the player's own files go in alongside, unless the server already ships them
    let profile = paths::profiles().join(&key);
    for kind in crate::catalogue::KINDS {
        let dir = kind.dir();
        let mine = sync::personal(manifest.entries(dir), &profile.join(dir));
        if mine.is_empty() {
            continue;
        }
        let target = instance.join(dir);
        let _ = std::fs::create_dir_all(&target);
        for file in mine {
            if let Some(name) = file.file_name() {
                let _ = std::fs::copy(&file, target.join(name));
            }
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

    // ponytail: lyceris' own downloads are a ceiling we cannot close from here. install() uses
    // each sha1 only to decide WHETHER to fetch, never to check what arrived, so anything
    // downloaded in a run is on the classpath that same run unverified - and loader libraries
    // whose metadata carries no hash are never verified on any run. Everything is https, so the
    // attacker has to be the upstream host. Fixing it means a patched lyceris; the version is
    // pinned in Cargo.toml so the behaviour cannot change without someone choosing it.
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
