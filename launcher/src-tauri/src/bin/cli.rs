//! The server-side companion: turn a modpack into a server directory, and change a running
//! server without SFTP. Shares the launcher's pack reader and its server builder - the
//! launcher's own hosting tab writes the same directory through the same functions.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use launcher_lib::{host, pack};

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
        /// The port the server listens on.
        #[arg(long, default_value_t = 25565)]
        port: u16,
        /// Heap in MB. Sized from the pack when left out.
        #[arg(long, default_value_t = 0)]
        memory: u64,
        /// Leave the publisher mod out: the server runs, but hands nothing to clients.
        #[arg(long)]
        no_publish: bool,
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
    // The library reports a few failures as keys, because the launcher translates them. A
    // terminal cannot, so the handful it can actually hit are spelled out here.
    run()
        .await
        .map_err(|error| match format!("{error:#}").as_str() {
            "error.noJava" => anyhow!(
                "no Java found. A Minecraft server needs a JDK 21 (17 below 1.20.5, 8 below 1.17)."
            ),
            "error.curseforgeKeyNeeded" => anyhow!(
                "this CurseForge pack names its files by id. Pass --cf-key to resolve them."
            ),
            _ => error,
        })
}

async fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Init {
            pack,
            out,
            dry_run,
            eula,
            port,
            memory,
            no_publish,
            cf_key,
        } => init(&pack, out, dry_run, eula, port, memory, !no_publish, cf_key).await,
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

/// The library reports progress as a key and a detail so the launcher can translate it. This
/// is a terminal, so it gets English.
fn say(phase: &str, detail: String, done: u64, total: u64) {
    let what = match phase {
        "host.read" => "read",
        "host.resolved" => "resolved from CurseForge",
        "host.clientOnly" => "turned out to be client-only; Modrinth says so, the pack did not",
        "host.wrote" => "files written",
        "host.files" => {
            // One line per file would be a wall; every tenth is a progress bar you can read.
            if done.is_multiple_of(10) || done == total {
                println!("  {done}/{total} {detail}");
            }
            return;
        }
        "host.publisher" => "the publisher mod, into mods/",
        "host.publisherFailed" => "could not fetch the publisher mod",
        "host.loader" => "loader",
        "host.installer" => "running the installer",
        "host.blocked" => "refuse third-party distribution - download those by hand into mods/",
        "host.failed" => "could not be fetched; get those by hand before starting the server",
        other => other,
    };
    if detail.is_empty() {
        println!("  {what}");
    } else {
        println!("  {detail} {what}");
    }
}

/// 32 bytes from the operating system, hex encoded. Not std's RandomState: that is a hash seed.
fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system has no randomness");
    hex::encode(bytes)
}

#[allow(clippy::too_many_arguments)]
async fn init(
    path: &Path,
    out: Option<PathBuf>,
    dry_run: bool,
    accept_eula: bool,
    port: u16,
    memory: u64,
    publish: bool,
    cf_key: Option<String>,
) -> Result<()> {
    // Read once here for the summary and the default directory name; host::build reads it
    // again to do the work, which is a zip open against saying the same thing in two places.
    let summary = pack::read(path)?;
    println!("{} {}", summary.name, summary.version);
    println!("  minecraft {}", summary.minecraft);
    if let Some((loader, version)) = &summary.loader {
        println!("  {loader} {version}");
    }
    for (area, count) in summary.counts() {
        println!("  {count} {area}");
    }

    let unresolved = summary
        .files
        .iter()
        .filter(|f| f.path.starts_with("cf://"))
        .count();
    if unresolved > 0 && cf_key.as_deref().unwrap_or_default().is_empty() {
        return Err(anyhow!(
            "{unresolved} files are CurseForge ids and need --cf-key to resolve"
        ));
    }
    let root = out.unwrap_or_else(|| PathBuf::from(host::sanitise(&summary.name)));
    if dry_run {
        println!("\nWould write to {}", root.display());
        return Ok(());
    }
    if root
        .read_dir()
        .is_ok_and(|mut entries| entries.next().is_some())
    {
        return Err(anyhow!("{} exists and is not empty", root.display()));
    }
    std::fs::create_dir_all(&root)?;

    let client = reqwest::Client::builder()
        .user_agent(concat!("UnifiedMC/", env!("CARGO_PKG_VERSION")))
        .build()?;

    println!("\nSetting the server up:");
    let server = host::build(
        &client,
        &root,
        &host::Spec {
            name: summary.name.clone(),
            minecraft: String::new(),
            loader: None,
            loader_version: None,
            port,
            memory,
            eula: accept_eula,
            publish,
            pack: Some(path.to_path_buf()),
            // --cf-key beats whatever was compiled in, so a build without the secret can
            // still import a CurseForge manifest.
            cf_key: cf_key
                .filter(|key| !key.is_empty())
                .unwrap_or_else(|| launcher_lib::settings::curseforge_key().to_string()),
        },
        &say,
    )
    .await?;

    println!("\n{} is ready.", root.display());
    println!("  {} MB heap, port {}", server.memory, server.port);
    if !server.publishes {
        println!("  no publisher mod: nothing is handed to clients");
    }
    if !accept_eula {
        println!("  Read https://aka.ms/MinecraftEULA, then set eula=true in eula.txt");
        println!("  (or run this again with --eula, which says you have read it)");
    }
    println!("  Start it with ./start.sh");
    Ok(())
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

/// A bare address becomes a url, never a cleartext one to another machine: the token
/// authenticates a write endpoint, and on http anyone on the path can read it.
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

#[cfg(test)]
mod tests {
    use super::*;

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
