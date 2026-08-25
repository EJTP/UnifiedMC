//! The server-side companion.
//!
//! Two jobs: turn a modpack into a server directory, and change a running server without
//! SFTP. Both share the launcher's pack reader, so "which side is this mod for" is answered
//! in exactly one place.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use launcher_lib::pack::{self, Side};
// the same predicate the launcher writes manifest files through: a path out of somebody else's
// archive has to stay inside the directory we are writing, on either side of the connection
use launcher_lib::sync::plain_relative;

#[derive(Parser)]
#[command(
    name = "unifiedmc-server-cli",
    about = "Set up and steer a UnifiedMC server"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Turn a modpack into a server directory.
    Init {
        /// A .mrpack, a CurseForge zip, or a server pack zip.
        pack: PathBuf,
        /// Where to write. Defaults to the pack's name.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Say what would happen, write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Say you have read https://aka.ms/MinecraftEULA. Without it the server will not start.
        #[arg(long)]
        eula: bool,
        /// CurseForge API key, needed to resolve a CurseForge manifest.
        #[arg(long, env = "UNIFIEDMC_CF_KEY", hide_env_values = true)]
        cf_key: Option<String>,
    },

    /// Generate a token for remote control. Put it in config/unifiedmc.properties.
    Token,

    /// Send a client mod to a running server.
    Push {
        /// Server address, e.g. mc.example.com:25566
        server: String,
        /// The jar to send.
        jar: PathBuf,
        /// "client" (served, never loaded) or "shared" (staged for the next restart).
        #[arg(long, default_value = "client")]
        area: String,
        // hide_env_values: the doc tells you to export this, and --help is exactly what ends
        // up in a paste, a screenshot, a CI log or tmux scrollback.
        #[arg(long, env = "UNIFIEDMC_ADMIN_TOKEN", hide_env_values = true)]
        token: String,
        /// Send the token over plain http to a remote host anyway.
        #[arg(long)]
        insecure: bool,
    },

    /// Remove a client mod from a running server.
    Remove {
        server: String,
        name: String,
        #[arg(long, default_value = "client")]
        area: String,
        #[arg(long, env = "UNIFIEDMC_ADMIN_TOKEN", hide_env_values = true)]
        token: String,
        /// Send the token over plain http to a remote host anyway.
        #[arg(long)]
        insecure: bool,
    },

    /// Make the server re-read its directories and rebuild the manifest.
    Rescan {
        server: String,
        #[arg(long, env = "UNIFIEDMC_ADMIN_TOKEN", hide_env_values = true)]
        token: String,
        /// Send the token over plain http to a remote host anyway.
        #[arg(long)]
        insecure: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Init {
            pack,
            out,
            dry_run,
            eula,
            cf_key,
        } => init(&pack, out, dry_run, eula, cf_key).await,
        Command::Token => {
            println!("{}", new_token());
            eprintln!("\nPut this in config/unifiedmc.properties as:");
            eprintln!("  admin-token=<the line above>");
            eprintln!("Remote control stays off until you do, and off is the safe default.");
            Ok(())
        }
        Command::Push {
            server,
            jar,
            area,
            token,
            insecure,
        } => push(&base(&server, insecure)?, &jar, &area, &token).await,
        Command::Remove {
            server,
            name,
            area,
            token,
            insecure,
        } => {
            remote(
                &base(&server, insecure)?,
                "delete",
                &[("name", &name), ("area", &area)],
                &token,
            )
            .await
        }
        Command::Rescan {
            server,
            token,
            insecure,
        } => remote(&base(&server, insecure)?, "rescan", &[], &token).await,
    }
}

/// 32 bytes from the operating system, hex encoded.
///
/// Not std's RandomState: that is a hash seed, and four SipHash outputs of the messages 0, 8,
/// 16, 24 under one thread-local key is not a sentence anyone should have to reason about when
/// the answer is a credential.
fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system has no randomness");
    hex::encode(bytes)
}

async fn init(
    path: &Path,
    out: Option<PathBuf>,
    dry_run: bool,
    accept_eula: bool,
    cf_key: Option<String>,
) -> Result<()> {
    let pack = pack::read(path)?;
    println!("{} {}", pack.name, pack.version);
    println!("  minecraft {}", pack.minecraft);
    if let Some((loader, version)) = &pack.loader {
        println!("  {loader} {version}");
    }

    let unresolved = pack
        .files
        .iter()
        .filter(|f| f.path.starts_with("cf://"))
        .count();
    if unresolved > 0 && cf_key.is_none() {
        return Err(anyhow!(
            "{unresolved} files are CurseForge ids and need --cf-key to resolve"
        ));
    }

    for (area, count) in pack.counts() {
        println!("  {count} {area}");
    }
    if !pack.blocked.is_empty() {
        println!(
            "\n  {} refuse third-party distribution:",
            pack.blocked.len()
        );
        for name in &pack.blocked {
            println!("    {name}");
        }
        println!("  Download those by hand and drop them in mods/.");
    }

    let root = out.unwrap_or_else(|| PathBuf::from(sanitise(&pack.name)));
    if dry_run {
        println!("\nWould write to {}", root.display());
        return Ok(());
    }

    let client = reqwest::Client::builder()
        .user_agent(concat!("UnifiedMC/", env!("CARGO_PKG_VERSION")))
        .build()?;

    // Before anything is written: a pack that leaves a mod's sides undeclared would otherwise
    // put a client-only jar in mods/, and a dedicated server dies on the first client class it
    // touches. Modrinth is asked once, in bulk, and a failure here changes nothing.
    let mut pack = pack;
    let moved = pack::resolve_unknown_sides(&client, &mut pack).await;
    if moved > 0 {
        println!("  {moved} more turned out to be client-only; Modrinth says so, the pack did not");
    }

    let mut written = 0usize;
    let mut failed: Vec<(String, String)> = Vec::new();

    for file in &pack.files {
        // A pack is a zip somebody else wrote, and both a member name and an mrpack index
        // entry are attacker text: "overrides/../../../../.ssh/authorized_keys" survives every
        // strip_prefix below and lands wherever it likes, as whoever ran this.
        if !plain_relative(&file.path) {
            println!("  skipping {} - not a path inside the pack", file.path);
            continue;
        }

        // config for the server, mods split by which side loads them
        let target = match (file.side, file.path.starts_with("mods/")) {
            // strip_prefix, not trim_start_matches: that one strips repeatedly, so
            // "mods/mods/x.jar" would come out as "x.jar"
            (Side::ClientOnly, true) => root
                .join("unifiedmc/client")
                .join(file.path.strip_prefix("mods/").unwrap_or(&file.path)),
            // client-config/ mirrors config/, so the prefix must not be repeated inside it
            (Side::ClientOnly, false) => root
                .join("unifiedmc/client-config")
                .join(file.path.strip_prefix("config/").unwrap_or(&file.path)),
            (_, _) => root.join(&file.path),
        };

        if pack::is_private(&file.path) {
            continue; // backups and per-world state are nobody else's business
        }

        std::fs::create_dir_all(
            target
                .parent()
                .ok_or_else(|| anyhow!("odd path: {}", file.path))?,
        )?;

        if let Some(bytes) = &file.bytes {
            std::fs::write(&target, bytes)?;
        } else if let Some(url) = &file.url {
            // One dead link must not end a two hundred file import. Collect and report at the
            // end, so the admin sees the whole list rather than one at a time.
            match fetch(&client, url).await {
                Ok(bytes) => match verify(&bytes, file.sha1.as_deref()) {
                    // The pack states a hash for a reason: a repointed link or a swapped CDN
                    // object would otherwise become a jar in mods/, and from there a jar in
                    // every player's mods/ by way of the manifest.
                    Ok(()) => std::fs::write(&target, &bytes)?,
                    Err(error) => {
                        failed.push((file.path.clone(), format!("{error}")));
                        continue;
                    }
                },
                Err(error) => {
                    failed.push((file.path.clone(), format!("{error}")));
                    continue;
                }
            }
        } else {
            continue;
        }
        written += 1;
        if written.is_multiple_of(25) {
            println!("  {written} files");
        }
    }

    write_config(&root, &pack)?;
    println!("\nWrote {written} files to {}", root.display());

    if !failed.is_empty() {
        println!("\n{} could not be fetched:", failed.len());
        for (path, error) in &failed {
            println!("  {path}\n    {error}");
        }
        println!("\nGet those by hand before starting the server; it will be missing them.");
    }

    provision(&client, &root, &pack, accept_eula).await?;
    Ok(())
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

fn write_config(root: &Path, pack: &pack::Pack) -> Result<()> {
    let (loader, version) = pack
        .loader
        .clone()
        .unwrap_or_else(|| ("neoforge".into(), String::new()));

    let config = format!(
        "#UnifiedMC - what this server tells clients to install\n\
         port=25566\n\
         loader={loader}\n\
         loader-version={version}\n\
         minecraft={}\n\
         \n\
         # Remote control is off without a token. Generate one with:\n\
         #   unifiedmc-server-cli token\n\
         #admin-token=\n",
        pack.minecraft
    );
    std::fs::create_dir_all(root.join("config"))?;
    std::fs::write(root.join("config/unifiedmc.properties"), config)?;

    std::fs::create_dir_all(root.join("unifiedmc/client"))?;
    std::fs::create_dir_all(root.join("unifiedmc/client-config"))?;
    std::fs::write(
        root.join("unifiedmc/server-only.txt"),
        "# One jar filename per line: mods this server loads but clients must NOT get.\n\
         # Client-only mods do not belong here - those live in unifiedmc/client/.\n",
    )?;
    Ok(())
}

/// A pack name becomes a directory name, so it stops being a path.
fn sanitise(name: &str) -> String {
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

async fn push(base: &str, jar: &Path, area: &str, token: &str) -> Result<()> {
    let bytes = std::fs::read(jar).with_context(|| format!("reading {}", jar.display()))?;
    let name = jar
        .file_name()
        .ok_or_else(|| anyhow!("no filename"))?
        .to_string_lossy()
        .into_owned();

    use sha1::{Digest, Sha1};
    let sha1 = hex::encode(Sha1::digest(&bytes));

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/admin/upload"))
        .bearer_auth(token)
        .query(&[
            ("name", name.as_str()),
            ("sha1", sha1.as_str()),
            ("area", area),
        ])
        .body(bytes)
        .send()
        .await?;

    report(response).await
}

async fn remote(base: &str, action: &str, query: &[(&str, &str)], token: &str) -> Result<()> {
    let response = reqwest::Client::new()
        .post(format!("{base}/admin/{action}"))
        .bearer_auth(token)
        .query(query)
        .send()
        .await?;
    report(response).await
}

/// A bare address becomes a url - but never a cleartext one to another machine.
///
/// The token authenticates a write endpoint that hard-links a jar into every player's mods
/// directory, so anyone on the path between here and the host who reads it owns the group. The
/// mod speaks no TLS, so the answer is a tunnel rather than a scheme:
///     ssh -L 25566:localhost:25566 you@host
/// and then push to localhost:25566.
fn base(server: &str, insecure: bool) -> Result<String> {
    let url = if server.starts_with("http://") || server.starts_with("https://") {
        server.trim_end_matches('/').to_string()
    } else {
        format!("http://{server}")
    };
    if insecure || !url.starts_with("http://") || is_this_machine(host_of(&url)) {
        return Ok(url);
    }
    Err(anyhow!(
        "{url} would send the admin token in the clear.\n\
         Tunnel it instead:  ssh -L 25566:localhost:25566 <host>\n\
         then use localhost:25566 - or pass --insecure if the network really is trusted."
    ))
}

fn host_of(url: &str) -> &str {
    let authority = url
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("");
    match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""), // [::1]:25566
        None => authority
            .rsplit_once(':')
            .map(|(host, _)| host)
            .unwrap_or(authority),
    }
}

fn is_this_machine(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

async fn report(response: reqwest::Response) -> Result<()> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status.is_success() {
        println!("{body}");
        return Ok(());
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(anyhow!(
            "rejected. Is admin-token set in config/unifiedmc.properties, and does it match?"
        ));
    }
    Err(anyhow!("{status}: {body}"))
}

/// The pieces a directory of mods is not: a loader, a server jar, the two files Minecraft
/// refuses to start without, and something to double-click.
///
/// Without this, `init` produced a folder that looked complete and could not be started, and
/// every person who tried had to go and find the same four things by hand.
async fn provision(
    client: &reqwest::Client,
    root: &Path,
    pack: &pack::Pack,
    accept_eula: bool,
) -> Result<()> {
    println!("\nSetting the server up:");

    let (loader, mut version) = pack
        .loader
        .clone()
        .unwrap_or_else(|| ("neoforge".into(), String::new()));

    // 1. The mod that makes this a UnifiedMC server at all - where it can load. It is written
    //    against NeoForge, and Fabric answers a jar it does not understand with "found 1
    //    non-fabric mod" and carries on: a server that runs perfectly and provisions nobody.
    //    Saying so here beats letting somebody find out from a launcher that sees no pack.
    let publisher = matches!(loader.as_str(), "neoforge" | "forge");
    if publisher {
        let mods = root.join("mods");
        std::fs::create_dir_all(&mods)?;
        match fetch(client, PUBLISHER_URL).await {
            Ok(bytes) => {
                std::fs::write(mods.join("unifiedmc-server.jar"), &bytes)?;
                println!("  the publisher mod, into mods/");
            }
            // Not fatal: everything else here still produces a working server, only one that
            // hands nothing to clients.
            Err(error) => println!(
                "  could not fetch the publisher mod: {error}\n    get it from {PUBLISHER_URL}"
            ),
        }
    } else {
        println!("  no publisher mod: it is a NeoForge mod and this pack is {loader}");
        println!("    the server will run; clients just cannot install from it yet");
        println!("    https://github.com/EJTP/UnifiedMC-Server#platforms");
    }

    // 2. The loader's own server. Fabric hands out a launchable jar; NeoForge and Forge hand
    //    out an installer that has to be run once, which needs a JVM on this machine.
    if version.is_empty() {
        if let Some(kind) = launcher_lib::loaders::Kind::parse(&loader) {
            version = launcher_lib::loaders::latest(client, kind, &pack.minecraft)
                .await
                .unwrap_or_default();
        }
    }
    // Sized like the launcher sizes a client: a pack this size does not fit in the JVM's
    // default quarter of the machine, and a server that dies at 40 players is not a mystery.
    let heap = 2048 + 12 * pack.files.len().min(500);
    let start = install_loader(client, root, &loader, &version, &pack.minecraft, heap).await?;

    // 3. Minecraft will not start without being told the licence was read, and that is the
    //    admin's statement to make, not ours.
    std::fs::write(
        root.join("eula.txt"),
        if accept_eula {
            "# Accepted by whoever ran unifiedmc-server-cli init --eula\neula=true\n"
        } else {
            "eula=false\n"
        },
    )?;

    // 4. A server.properties that matches the pack rather than the vanilla defaults.
    let properties = root.join("server.properties");
    if !properties.exists() {
        std::fs::write(
            &properties,
            format!(
                "motd={} {}\nserver-port=25565\nonline-mode=true\nmax-players=20\n\
                 view-distance=10\nsimulation-distance=8\nlevel-name=world\nmotd-escaped=false\n",
                pack.name, pack.version
            ),
        )?;
        println!("  server.properties");
    }

    // 5. Something to run. The heap is already inside `start` - a JVM option after -jar is an
    //    argument to the program, not to the JVM, so appending it here would set nothing.
    std::fs::write(
        root.join("start.sh"),
        format!("#!/bin/sh\ncd \"$(dirname \"$0\")\"\nexec {start}\n"),
    )?;
    std::fs::write(
        root.join("start.bat"),
        format!("@echo off\r\ncd /d \"%~dp0\"\r\n{start}\r\npause\r\n"),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            root.join("start.sh"),
            std::fs::Permissions::from_mode(0o755),
        )?;
    }
    println!("  start.sh and start.bat, {heap} MB heap");

    // Said here rather than discovered at the first start: a server directory is usually built
    // on one machine and run on another, so this is a note, not a failure.
    if std::process::Command::new("java")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_err()
    {
        println!("  no java on this machine - the server needs a JDK 21 wherever it runs");
    }

    println!("\n{} is ready.", root.display());
    if !accept_eula {
        println!("  Read https://aka.ms/MinecraftEULA, then set eula=true in eula.txt");
        println!("  (or run this again with --eula, which says you have read it)");
    }
    println!("  Start it with ./start.sh");
    Ok(())
}

/// Where the publisher mod is published. The launcher and the server mod version separately,
/// so this follows whatever the server repository last released rather than our own number.
const PUBLISHER_URL: &str =
    "https://github.com/EJTP/UnifiedMC-Server/releases/latest/download/unifiedmc-server-0.1.0.jar";

/// Returns the java command line that starts the server, without the heap flags.
async fn install_loader(
    client: &reqwest::Client,
    root: &Path,
    loader: &str,
    version: &str,
    minecraft: &str,
    heap: usize,
) -> Result<String> {
    match loader {
        "fabric" | "quilt" => {
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
                .with_context(|| format!("fetching the {loader} server from {url}"))?;
            std::fs::write(root.join("server.jar"), &bytes)?;
            println!("  {loader} {version} server jar");
            Ok(format!(
                "java -Xmx{heap}M -Xms{heap}M -jar server.jar nogui"
            ))
        }
        _ => {
            let url = if loader == "forge" {
                format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{version}/forge-{version}-installer.jar")
            } else {
                format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar")
            };
            let bytes = fetch(client, &url)
                .await
                .with_context(|| format!("fetching the {loader} installer from {url}"))?;
            let installer = root.join("installer.jar");
            std::fs::write(&installer, &bytes)?;
            println!("  {loader} {version} installer");

            // The installer writes the libraries and a run script; it needs a JVM here. When
            // there is none, leave it in place with the one command to run - that is still a
            // long way short of "go and work out which four things to download".
            match std::process::Command::new("java")
                .arg("-jar")
                .arg("installer.jar")
                .arg("--install-server")
                .arg(".")
                .current_dir(root)
                .status()
            {
                Ok(status) if status.success() => {
                    std::fs::write(
                        root.join("user_jvm_args.txt"),
                        format!(
                            "# Read by run.sh. One option per line.\n-Xmx{heap}M\n-Xms{heap}M\n"
                        ),
                    )?;
                    let _ = std::fs::remove_file(&installer);
                    let _ = std::fs::remove_file(root.join("installer.jar.log"));
                    println!("  installed it");
                    Ok("sh run.sh nogui".into())
                }
                Ok(status) => {
                    println!("  the installer exited with {status}; run it yourself:");
                    println!(
                        "    cd {} && java -jar installer.jar --install-server .",
                        root.display()
                    );
                    Ok("sh run.sh nogui".into())
                }
                Err(_) => {
                    println!("  no java here, so run this on the server before starting it:");
                    println!("    java -jar installer.jar --install-server .");
                    Ok("sh run.sh nogui".into())
                }
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_pack_member_cannot_write_outside_the_directory_we_make() {
        assert!(plain_relative("mods/sodium.jar"));
        assert!(plain_relative("config/create/common.toml"));
        for hostile in [
            "../../../../.ssh/authorized_keys",
            "/etc/cron.d/x",
            "mods/../../../x",
            "..",
            "",
        ] {
            assert!(!plain_relative(hostile), "accepted {hostile:?}");
        }
    }

    #[test]
    fn a_pack_states_a_hash_so_that_it_gets_checked() {
        // sha1 of "jar bytes"
        let bytes = b"jar bytes";
        use sha1::{Digest, Sha1};
        let real = hex::encode(Sha1::digest(bytes));
        assert!(verify(bytes, Some(&real)).is_ok());
        assert!(verify(bytes, Some(&real.to_uppercase())).is_ok());
        assert!(verify(bytes, None).is_ok(), "no hash is not a failure");
        assert!(verify(bytes, Some("da39a3ee5e6b4b0d3255bfef95601890afd80709")).is_err());
    }

    #[test]
    fn tokens_are_long_and_not_repeated() {
        let token = new_token();
        assert_eq!(token.len(), 64, "32 bytes, hex encoded");
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(new_token(), token);
    }

    #[test]
    fn a_bare_address_becomes_a_url_and_a_url_is_left_alone() {
        assert_eq!(
            base("localhost:25566", false).unwrap(),
            "http://localhost:25566"
        );
        assert_eq!(
            base("http://127.0.0.1:25566/", false).unwrap(),
            "http://127.0.0.1:25566"
        );
        assert_eq!(
            base("https://mc.example.com/", false).unwrap(),
            "https://mc.example.com"
        );
    }

    #[test]
    fn the_admin_token_does_not_go_over_the_wire_in_the_clear() {
        // the exact line the doc used to teach
        assert!(base("mc.example.com:25566", false).is_err());
        assert!(base("http://mc.example.com:25566", false).is_err());
        // said out loud, it is allowed - and a tunnel needs no flag at all
        assert!(base("mc.example.com:25566", true).is_ok());
        assert!(base("[::1]:25566", false).is_ok());
        // "http" is a prefix of more than one thing; only a scheme is a scheme
        assert_eq!(
            host_of("http://httpserver.example.com:1/x"),
            "httpserver.example.com"
        );
        assert!(base("httpserver.example.com:25566", false).is_err());
    }
}
