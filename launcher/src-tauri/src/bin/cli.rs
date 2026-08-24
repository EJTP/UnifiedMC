//! The server-side companion.
//!
//! Two jobs: turn a modpack into a server directory, and change a running server without
//! SFTP. Both share the launcher's pack reader, so "which side is this mod for" is answered
//! in exactly one place.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use launcher_lib::pack::{self, Side};

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
        /// CurseForge API key, needed to resolve a CurseForge manifest.
        #[arg(long, env = "UNIFIEDMC_CF_KEY")]
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
        #[arg(long, env = "UNIFIEDMC_ADMIN_TOKEN")]
        token: String,
    },

    /// Remove a client mod from a running server.
    Remove {
        server: String,
        name: String,
        #[arg(long, default_value = "client")]
        area: String,
        #[arg(long, env = "UNIFIEDMC_ADMIN_TOKEN")]
        token: String,
    },

    /// Make the server re-read its directories and rebuild the manifest.
    Rescan {
        server: String,
        #[arg(long, env = "UNIFIEDMC_ADMIN_TOKEN")]
        token: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Init {
            pack,
            out,
            dry_run,
            cf_key,
        } => init(&pack, out, dry_run, cf_key).await,
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
        } => push(&server, &jar, &area, &token).await,
        Command::Remove {
            server,
            name,
            area,
            token,
        } => {
            remote(
                &server,
                "delete",
                &[("name", &name), ("area", &area)],
                &token,
            )
            .await
        }
        Command::Rescan { server, token } => remote(&server, "rescan", &[], &token).await,
    }
}

/// 32 bytes of randomness, hex encoded. Long enough that guessing is not a strategy.
fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher, RandomState};

    let mut bytes = Vec::with_capacity(32);
    while bytes.len() < 32 {
        // RandomState is seeded by the OS and is the one source of entropy in std
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_usize(bytes.len());
        bytes.extend_from_slice(&hasher.finish().to_le_bytes());
    }
    bytes.truncate(32);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn init(
    path: &Path,
    out: Option<PathBuf>,
    dry_run: bool,
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

    let mut written = 0usize;
    let mut failed: Vec<(String, String)> = Vec::new();

    for file in &pack.files {
        // config for the server, mods split by which side loads them
        let target = match (file.side, file.path.starts_with("mods/")) {
            (Side::ClientOnly, true) => root
                .join("unifiedmc/client")
                .join(file.path.trim_start_matches("mods/")),
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
                Ok(bytes) => std::fs::write(&target, &bytes)?,
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

    println!("Upload its contents to your server, then restart it.");
    Ok(())
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

async fn push(server: &str, jar: &Path, area: &str, token: &str) -> Result<()> {
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
        .post(format!("{}/admin/upload", base(server)))
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

async fn remote(server: &str, action: &str, query: &[(&str, &str)], token: &str) -> Result<()> {
    let response = reqwest::Client::new()
        .post(format!("{}/admin/{action}", base(server)))
        .bearer_auth(token)
        .query(query)
        .send()
        .await?;
    report(response).await
}

fn base(server: &str) -> String {
    if server.starts_with("http") {
        server.trim_end_matches('/').to_string()
    } else {
        format!("http://{server}")
    }
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
    fn a_pack_name_becomes_a_directory_name_not_a_path() {
        assert_eq!(
            sanitise("All of Create Aeronautics"),
            "all-of-create-aeronautics"
        );
        assert_eq!(sanitise("../../etc"), "etc");
        assert_eq!(sanitise("///"), "pack");
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
        assert_eq!(base("mc.example.com:25566"), "http://mc.example.com:25566");
        assert_eq!(
            base("http://mc.example.com:25566/"),
            "http://mc.example.com:25566"
        );
    }
}
