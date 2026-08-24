//! What a server is, and how we find that out.

use std::fs;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::paths;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedServer {
    pub id: String,
    pub name: String,
    /// As the player typed it. Resolved to host and port only when we connect.
    pub address: String,

    /// What the player chose to run here.
    ///
    /// A Paper or Vanilla server announces no loader, so without this the instance would be
    /// plain Minecraft and could load nothing. A client-side mod does not need the server to
    /// know about it, so the choice is the player's.
    #[serde(default)]
    pub loader: Option<String>,

    /// Which Minecraft to run, when the player wants something other than what was detected.
    ///
    /// A proxy answers with the oldest protocol it accepts - Hypixel says 1.8.9 while happily
    /// taking 1.21 - so detection alone would pin every player to the oldest option.
    #[serde(default)]
    pub minecraft: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ModEntry {
    pub name: String,
    pub sha1: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ConfigEntry {
    pub path: String,
    pub sha1: String,
    #[serde(default)]
    pub url: String,
    /// Set by the pack for files that must win over whatever is on the client.
    #[serde(default)]
    pub force: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub minecraft: String,
    #[serde(default)]
    pub loader: Option<Loader>,
    #[serde(default)]
    pub mods: Vec<ModEntry>,
    #[serde(default)]
    pub config: Vec<ConfigEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Loader {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerStatus {
    pub id: String,
    pub online: bool,
    #[serde(default)]
    pub error: Option<String>,
    /// Already split into styled runs, so the window draws it the way the game would.
    #[serde(default)]
    pub motd: Vec<crate::motd::Span>,
    /// The server's own icon, already a data: URI - Minecraft sends it as one.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub players: u32,
    #[serde(default)]
    pub max_players: u32,
    #[serde(default)]
    pub manifest: Option<Manifest>,
}

pub fn load() -> Vec<SavedServer> {
    fs::read_to_string(paths::servers_file())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(servers: &[SavedServer]) -> Result<()> {
    let file = paths::servers_file();
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file, serde_json::to_vec_pretty(servers)?)?;
    Ok(())
}

/// "host", "host:port" or a bare address. Minecraft's default is 25565.
pub fn split_address(address: &str) -> (String, u16) {
    match address.rsplit_once(':') {
        Some((host, port)) => match port.parse::<u16>() {
            Ok(port) => (host.to_string(), port),
            Err(_) => (address.to_string(), 25565),
        },
        None => (address.to_string(), 25565),
    }
}

fn write_varint(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

async fn read_varint(stream: &mut TcpStream) -> Result<u32> {
    let mut value: u32 = 0;
    for shift in (0..5).map(|i| i * 7) {
        let byte = stream.read_u8().await?;
        value |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(anyhow!("varint too long"))
}

fn packet(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut body = vec![id];
    body.extend_from_slice(payload);
    let mut out = Vec::with_capacity(body.len() + 5);
    write_varint(&mut out, body.len() as u32);
    out.extend_from_slice(&body);
    out
}

/// The vanilla status ping. Returns the server's own JSON.
/// Where a server actually listens.
///
/// Most public addresses are a SRV record pointing elsewhere: hypixel.net answers nothing on
/// 25565, its record sends you to mc.hypixel.net. Minecraft resolves these, so anyone typing
/// an address expects it to work. Only for the default port - naming a port means that port.
async fn resolve_srv(host: &str, port: u16) -> Option<(String, u16)> {
    if port != 25565 {
        return None;
    }
    // both the builder and build() can fail on a system with no usable resolver config
    let resolver = hickory_resolver::TokioResolver::builder_tokio()
        .ok()?
        .build()
        .ok()?;
    let answer = resolver
        .srv_lookup(format!("_minecraft._tcp.{host}."))
        .await
        .ok()?;

    // lowest priority wins, highest weight breaks the tie - RFC 2782
    let best = answer
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            hickory_resolver::proto::rr::RData::SRV(srv) => Some(srv),
            _ => None,
        })
        .min_by_key(|srv| (srv.priority, std::cmp::Reverse(srv.weight)))?;
    Some((
        best.target.to_string().trim_end_matches('.').to_string(),
        best.port,
    ))
}

pub async fn ping(host: &str, port: u16) -> Result<serde_json::Value> {
    let (connect_host, connect_port) = match resolve_srv(host, port).await {
        Some(target) => target,
        None => (host.to_string(), port),
    };

    let stream = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect((connect_host.as_str(), connect_port)),
    )
    .await
    .context("connection timed out")??;
    let mut stream = stream;

    // the handshake carries the name the player typed, not where SRV sent us: servers use it
    // for virtual hosting, and rewriting it would reach the wrong one
    let mut handshake = Vec::new();
    write_varint(&mut handshake, 0); // protocol 0: we are only asking
    write_varint(&mut handshake, host.len() as u32);
    handshake.extend_from_slice(host.as_bytes());
    handshake.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut handshake, 1); // next state: status

    stream.write_all(&packet(0x00, &handshake)).await?;
    stream.write_all(&packet(0x00, &[])).await?;
    stream.flush().await?;

    let read = async {
        let _length = read_varint(&mut stream).await?;
        if read_varint(&mut stream).await? != 0 {
            return Err(anyhow!("unexpected status packet"));
        }
        let json_length = read_varint(&mut stream).await? as usize;
        let mut body = vec![0u8; json_length];
        stream.read_exact(&mut body).await?;
        Ok(serde_json::from_slice(&body)?)
    };
    tokio::time::timeout(Duration::from_secs(5), read)
        .await
        .context("server stopped answering")?
}

/// Server descriptions are a chat component tree; a list row only wants the words.
pub fn plain_motd(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Object(map) => {
            let mut text = map
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            if let Some(extra) = map.get("extra").and_then(|e| e.as_array()) {
                for part in extra {
                    text.push_str(&plain_motd(part));
                }
            }
            text
        }
        serde_json::Value::Array(parts) => parts.iter().map(plain_motd).collect(),
        _ => String::new(),
    }
}

/// What the server needs client-side, cheapest source first.
pub async fn manifest(
    client: &reqwest::Client,
    host: &str,
    port: u16,
    manifest_port: u16,
    status: &serde_json::Value,
) -> Option<Manifest> {
    // The game port first: the server mod answers HTTP there too, so a player needs to know
    // nothing but the address they already typed. Hosting that hands out one port is the
    // normal case, not the exception, and a second port was never something to ask for.
    let mut candidates = vec![port];
    if manifest_port != 0 && manifest_port != port {
        candidates.push(manifest_port);
    }

    for candidate in candidates {
        let base = format!("http://{host}:{candidate}/");

        if let Some(embedded) = status.get("unifiedmc") {
            if let Ok(mut parsed) = serde_json::from_value::<Manifest>(embedded.clone()) {
                resolve_urls(&mut parsed, &base);
                return Some(parsed);
            }
        }

        let Ok(response) = client
            .get(format!("{base}unifiedmc.json"))
            .timeout(Duration::from_secs(5))
            .send()
            .await
        else {
            continue;
        };
        if let Ok(mut parsed) = response.json::<Manifest>().await {
            resolve_urls(&mut parsed, &base);
            return Some(parsed);
        }
    }
    None
}

/// The server publishes paths, not urls: it has no idea which hostname, port forward or
/// proxy the client reached it through, and it does not need to.
fn resolve_urls(manifest: &mut Manifest, base: &str) {
    for entry in &mut manifest.mods {
        if entry.url.starts_with('/') {
            entry.url = format!("{}{}", base.trim_end_matches('/'), entry.url);
        }
    }
    for entry in &mut manifest.config {
        if entry.url.starts_with('/') {
            entry.url = format!("{}{}", base.trim_end_matches('/'), entry.url);
        }
    }
}

/// What Minecraft the server runs, from the ping.
///
/// version.name is free text - "We support: 1.20-1.21" is a real answer - so only the protocol
/// number is worth reading. A modded server states its version in the manifest instead.
pub async fn minecraft_version(
    client: &reqwest::Client,
    status: &serde_json::Value,
) -> anyhow::Result<String> {
    let version = status
        .get("version")
        .ok_or_else(|| anyhow!("the server sent no version block"))?;

    // version.name is free text - Hypixel answers "Requires MC 1.8 / 1.21" - so parsing it
    // produces nonsense. The protocol number is the only part that means something.
    let protocol = version
        .get("protocol")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow!("the server sent no protocol number"))?;

    let table = protocol_table(client).await?;
    table
        .get(&protocol)
        .cloned()
        .ok_or_else(|| anyhow!("no Minecraft release uses protocol {protocol}"))
}

const PROTOCOL_URL: &str = "https://raw.githubusercontent.com/PrismarineJS/minecraft-data/\
                            master/data/pc/common/protocolVersions.json";

/// protocol number -> release version, cached on disk.
///
/// A proxy reports the oldest protocol it accepts, which it accepts by definition - so a
/// ViaVersion server resolves to something old and still connects.
async fn protocol_table(
    client: &reqwest::Client,
) -> anyhow::Result<std::collections::HashMap<u64, String>> {
    let cache = paths::data().join("protocolVersions.json");
    let raw = match std::fs::read_to_string(&cache) {
        Ok(cached) => cached,
        Err(_) => {
            let fetched = client.get(PROTOCOL_URL).send().await?.text().await?;
            if let Some(parent) = cache.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let _ = std::fs::write(&cache, &fetched);
            fetched
        }
    };

    let entries: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
    let mut table = std::collections::HashMap::new();
    for entry in entries {
        // the file is newest first; keep the first release seen so 772 is 1.21.8, not 1.21.7
        let is_release = entry
            .get("releaseType")
            .and_then(|v| v.as_str())
            .map(|kind| kind == "release")
            .unwrap_or(true);
        if !is_release {
            continue;
        }
        let (Some(protocol), Some(name)) = (
            entry.get("version").and_then(|v| v.as_u64()),
            entry.get("minecraftVersion").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        table.entry(protocol).or_insert_with(|| name.to_string());
    }
    Ok(table)
}

/// A manifest for a server that publishes none: whatever the player chose to run against it.
///
/// Nothing is downloaded from the server here, because there is nothing to download - the
/// mods come from the player's own profile.
pub fn manifest_for_choice(minecraft: String, loader: Option<Loader>) -> Manifest {
    Manifest {
        minecraft,
        loader,
        mods: Vec::new(),
        config: Vec::new(),
    }
}

pub fn instance_key(address: &str, manifest: &Manifest) -> String {
    let loader = manifest
        .loader
        .as_ref()
        .map(|l| format!("-{}", l.kind))
        .unwrap_or_default();
    format!(
        "{}-{}{}",
        address.replace(':', "_"),
        manifest.minecraft,
        loader
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_default_to_the_minecraft_port() {
        assert_eq!(
            split_address("mc.example.com"),
            ("mc.example.com".into(), 25565)
        );
        assert_eq!(
            split_address("mc.example.com:25577"),
            ("mc.example.com".into(), 25577)
        );
        assert_eq!(split_address("1.2.3.4"), ("1.2.3.4".into(), 25565));
    }

    #[test]
    fn varints_round_trip() {
        for value in [0u32, 1, 127, 128, 255, 2_097_151, 25565] {
            let mut buffer = Vec::new();
            write_varint(&mut buffer, value);
            let mut decoded: u32 = 0;
            for (i, byte) in buffer.iter().enumerate() {
                decoded |= ((byte & 0x7F) as u32) << (i * 7);
            }
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn motd_comes_out_of_the_component_tree() {
        assert_eq!(plain_motd(&serde_json::json!("A Server")), "A Server");
        assert_eq!(
            plain_motd(&serde_json::json!({"text": "Hello ", "extra": [{"text": "World"}]})),
            "Hello World"
        );
        assert_eq!(plain_motd(&serde_json::json!(null)), "");
    }

    #[test]
    fn relative_urls_resolve_against_wherever_we_reached_the_server() {
        let mut manifest = Manifest {
            minecraft: "1.21.1".into(),
            mods: vec![ModEntry {
                name: "a.jar".into(),
                sha1: "x".into(),
                url: "/mods/x".into(),
            }],
            ..Default::default()
        };
        resolve_urls(&mut manifest, "http://1.2.3.4:25673/");
        assert_eq!(manifest.mods[0].url, "http://1.2.3.4:25673/mods/x");
    }

    #[tokio::test]
    async fn an_explicit_port_is_never_second_guessed() {
        // naming a port means that port; a SRV record answers "wherever this domain plays"
        assert!(resolve_srv("hypixel.net", 25577).await.is_none());
    }

    #[tokio::test]
    async fn a_public_address_resolves_to_where_it_actually_listens() {
        // hypixel.net answers nothing on 25565; its record points at mc.hypixel.net
        if let Some((host, port)) = resolve_srv("hypixel.net", 25565).await {
            assert!(host.contains("hypixel"), "unexpected target: {host}");
            assert!(port > 0);
        } // no network in CI is not a failure
    }
}
