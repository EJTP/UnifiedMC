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
    #[serde(default)]
    pub motd: String,
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
pub async fn ping(host: &str, port: u16) -> Result<serde_json::Value> {
    let stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect((host, port)))
        .await
        .context("connection timed out")??;
    let mut stream = stream;

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
    let base = format!("http://{host}:{manifest_port}/");

    if let Some(embedded) = status.get("unifiedmc") {
        if let Ok(mut parsed) = serde_json::from_value::<Manifest>(embedded.clone()) {
            resolve_urls(&mut parsed, &base);
            return Some(parsed);
        }
    }

    let response = client
        .get(format!("{base}unifiedmc.json"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    let mut parsed: Manifest = response.json().await.ok()?;
    resolve_urls(&mut parsed, &base);
    let _ = port;
    Some(parsed)
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
}
