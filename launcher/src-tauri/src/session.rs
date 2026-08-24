//! Who the game launches as.
//!
//! Two sources, in order:
//!
//! 1. `~/.unifiedmc/session.json`, written by the hub mod when it runs inside a launcher
//!    that is already signed in. A bridge until our own Azure application is approved.
//! 2. An offline profile, which only reaches offline-mode servers.
//!
//! Deliberately not lyceris' own Microsoft flow: it is hardcoded to the official launcher's
//! client id, and presenting ourselves as that application is the one thing the approval we
//! are waiting on rules out.

use std::fs;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub name: String,
    pub uuid: String,
    #[serde(default)]
    pub token: String,
    /// "microsoft" or "offline" - the UI says which, because it changes where you can play.
    #[serde(default)]
    pub kind: String,
}

impl Session {
    pub fn offline(name: &str) -> Self {
        Self {
            name: name.to_string(),
            uuid: offline_uuid(name),
            token: String::new(),
            kind: "offline".into(),
        }
    }

    pub fn is_online(&self) -> bool {
        self.kind == "microsoft" && !self.token.is_empty()
    }
}

pub fn current(offline_name: &str) -> Session {
    borrowed().unwrap_or_else(|| Session::offline(offline_name))
}

/// A session handed over by whatever launcher started the hub, if it is still valid.
fn borrowed() -> Option<Session> {
    let raw = fs::read_to_string(paths::session_file()).ok()?;
    let saved: serde_json::Value = serde_json::from_str(&raw).ok()?;

    let name = saved.get("name")?.as_str()?.to_string();
    let uuid = saved.get("uuid")?.as_str()?.to_string();
    let token = saved.get("token")?.as_str()?.to_string();

    if expired(&token) {
        return None;
    }
    Some(Session {
        name,
        uuid,
        token,
        kind: "microsoft".into(),
    })
}

/// Minecraft access tokens are JWTs. Read `exp` without verifying anything - we are not the
/// one checking the signature, we only want to say "expired" rather than let a server refuse
/// the player for no visible reason.
fn expired(token: &str) -> bool {
    let Some(payload) = token.split('.').nth(1) else {
        return false; // unreadable expiry must not block a session that may well work
    };
    let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else {
        return false;
    };
    let Ok(claims) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    let Some(exp) = claims.get("exp").and_then(|v| v.as_u64()) else {
        return false;
    };
    exp <= now()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The offline uuid Minecraft itself derives, so a world keeps the same player between runs.
fn offline_uuid(name: &str) -> String {
    let digest = md5::compute(format!("OfflinePlayer:{name}"));
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30; // version 3
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_the_shape_minecraft_expects() {
        let id = offline_uuid("Player");
        assert_eq!(id.len(), 36);
        assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
        assert_eq!(&id[14..15], "3", "version nibble");
        assert_eq!(offline_uuid("Player"), id, "same name, same id");
        assert_ne!(offline_uuid("Steve"), id);
    }

    #[test]
    fn an_unreadable_expiry_does_not_reject_the_session() {
        assert!(!expired("not-a-jwt"));
        assert!(!expired(""));
    }
}
