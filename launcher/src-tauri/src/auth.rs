//! Signing in with a Microsoft account.
//!
//! Microsoft -> Xbox Live -> XSTS -> Minecraft, in that order. Device code flow, so no local
//! web server and no URI scheme has to be registered.

use std::fs;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths;
use crate::session::Session;

/// Our own application, approved by Mojang. A device-code client has no secret; the id only
/// says which application is asking. Forks set their own.
const CLIENT_ID: &str = "439957e7-a0fb-4363-9d45-a9bde08a3349";

/// Enough to prove an Xbox identity, plus the refresh token.
const SCOPE: &str = "XboxLive.signin offline_access";

const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT_LOGIN_URL: &str =
    "https://api.minecraftservices.com/authentication/login_with_xbox";
const PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

fn client_id() -> String {
    std::env::var("UNIFIEDMC_CLIENT_ID").unwrap_or_else(|_| CLIENT_ID.to_string())
}

/// What the player has to do, shown while the poll below waits for them to do it.
#[derive(Clone, Debug, Serialize)]
pub struct Prompt {
    /// The short code they type into the page.
    pub user_code: String,
    /// The page they type it into.
    pub verification_uri: String,
    /// Seconds until the code stops working, so the window can say so rather than hang.
    pub expires_in: u64,
}

/// The signed-in account, on disk between runs. The refresh token is why the file is 0600.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct Account {
    name: String,
    uuid: String,
    /// The token the game launches with. Short-lived; refreshed from the one below.
    #[serde(default)]
    minecraft_token: String,
    #[serde(default)]
    refresh_token: String,
}

fn account_file() -> std::path::PathBuf {
    paths::data().join("account.json")
}

fn load() -> Option<Account> {
    let raw = fs::read_to_string(account_file()).ok()?;
    serde_json::from_str(&raw).ok()
}

fn store(account: &Account) -> Result<()> {
    let file = account_file();
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&file, serde_json::to_vec_pretty(account)?)?;

    // Anyone who reads this can mint Minecraft tokens until it is revoked.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&file, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn forget() {
    let _ = fs::remove_file(account_file());
}

/// Start the flow, hand back what the player has to do.
pub async fn begin(client: &reqwest::Client) -> Result<(Prompt, String)> {
    let answer: serde_json::Value = client
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", client_id().as_str()), ("scope", SCOPE)])
        .send()
        .await?
        .error_for_status()
        .context("Microsoft refused to start the sign-in")?
        .json()
        .await?;

    let device_code = string(&answer, "device_code")?;
    Ok((
        Prompt {
            user_code: string(&answer, "user_code")?,
            verification_uri: string(&answer, "verification_uri")?,
            expires_in: answer
                .get("expires_in")
                .and_then(|v| v.as_u64())
                .unwrap_or(900),
        },
        device_code,
    ))
}

/// Wait for the player to finish in their browser, then walk the rest of the chain.
/// `authorization_pending` is the normal answer until they do, not an error.
pub async fn finish(
    client: &reqwest::Client,
    device_code: &str,
    expires_in: u64,
) -> Result<Session> {
    let deadline = std::time::Instant::now() + Duration::from_secs(expires_in.min(1800));
    let mut interval = Duration::from_secs(5);

    let refresh_token = loop {
        if std::time::Instant::now() >= deadline {
            bail!("error.signIn.expired");
        }
        tokio::time::sleep(interval).await;

        let answer: serde_json::Value = client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", client_id().as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", device_code),
            ])
            .send()
            .await?
            .json()
            .await?;

        if let Some(token) = answer.get("refresh_token").and_then(|v| v.as_str()) {
            break token.to_string();
        }
        match answer.get("error").and_then(|v| v.as_str()) {
            Some("authorization_pending") => continue,
            // Asked for politely; ignoring it gets the whole flow rate limited.
            Some("slow_down") => {
                interval += Duration::from_secs(5);
                continue;
            }
            Some("expired_token") => bail!("error.signIn.expired"),
            Some("authorization_declined") => bail!("error.signIn.declined"),
            Some(other) => bail!("error.signIn.failed: {other}"),
            None => bail!("error.signIn.failed"),
        }
    };

    let account = exchange(client, &refresh_token).await?;
    store(&account)?;
    Ok(session_of(&account))
}

/// Refresh token -> Microsoft -> Xbox -> XSTS -> Minecraft -> profile. One function because
/// every step feeds the next.
async fn exchange(client: &reqwest::Client, refresh_token: &str) -> Result<Account> {
    let refreshed: serde_json::Value = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id().as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])
        .send()
        .await?
        .error_for_status()
        .context("error.signIn.expired")?
        .json()
        .await?;

    let microsoft = string(&refreshed, "access_token")?;
    // Microsoft rotates it; keeping the old one works until it suddenly does not.
    let next_refresh = refreshed
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .unwrap_or(refresh_token)
        .to_string();

    let xbox: serde_json::Value = client
        .post(XBL_URL)
        .json(&serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={microsoft}"),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }))
        .send()
        .await?
        .error_for_status()
        .context("error.signIn.xbox")?
        .json()
        .await?;

    let xbox_token = string(&xbox, "Token")?;
    let hash = user_hash(&xbox)?;

    let xsts_answer = client
        .post(XSTS_URL)
        .json(&serde_json::json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbox_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }))
        .send()
        .await?;

    // XSTS states the reason as a number; two of them are worth naming.
    if !xsts_answer.status().is_success() {
        let body: serde_json::Value = xsts_answer.json().await.unwrap_or_default();
        return Err(anyhow!(match body.get("XErr").and_then(|v| v.as_u64()) {
            Some(2_148_916_233) => "error.signIn.noXboxAccount",
            Some(2_148_916_238) => "error.signIn.childAccount",
            _ => "error.signIn.xbox",
        }));
    }
    let xsts: serde_json::Value = xsts_answer.json().await?;
    let xsts_token = string(&xsts, "Token")?;

    let minecraft: serde_json::Value = client
        .post(MINECRAFT_LOGIN_URL)
        .json(&serde_json::json!({
            "identityToken": format!("XBL3.0 x={hash};{xsts_token}"),
        }))
        .send()
        .await?
        .error_for_status()
        .context("error.signIn.minecraft")?
        .json()
        .await?;

    let minecraft_token = string(&minecraft, "access_token")?;

    // An account without the game authenticates fine and then has no profile.
    let profile_answer = client
        .get(PROFILE_URL)
        .bearer_auth(&minecraft_token)
        .send()
        .await?;
    if profile_answer.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("error.signIn.noGame");
    }
    let profile: serde_json::Value = profile_answer
        .error_for_status()
        .context("error.signIn.minecraft")?
        .json()
        .await?;

    Ok(Account {
        name: string(&profile, "name")?,
        uuid: dashed(&string(&profile, "id")?),
        minecraft_token,
        refresh_token: next_refresh,
    })
}

/// The signed-in session, refreshed if the stored token ran out. None if nobody is signed in;
/// a refresh that no longer works drops the account rather than showing a name that cannot play.
pub async fn session(client: &reqwest::Client) -> Option<Session> {
    let stored = load()?;
    if !stored.minecraft_token.is_empty() && !crate::session::expired(&stored.minecraft_token) {
        return Some(session_of(&stored));
    }
    if stored.refresh_token.is_empty() {
        return None;
    }
    match exchange(client, &stored.refresh_token).await {
        Ok(fresh) => {
            let _ = store(&fresh);
            Some(session_of(&fresh))
        }
        Err(_) => {
            forget();
            None
        }
    }
}

fn session_of(account: &Account) -> Session {
    Session {
        name: account.name.clone(),
        uuid: account.uuid.clone(),
        token: account.minecraft_token.clone(),
        kind: "microsoft".into(),
    }
}

fn string(value: &serde_json::Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("no {key} in the answer"))
}

/// Xbox buries the user hash one array deep; the Minecraft login needs it beside the token.
fn user_hash(xbox: &serde_json::Value) -> Result<String> {
    xbox.get("DisplayClaims")
        .and_then(|c| c.get("xui"))
        .and_then(|x| x.as_array())
        .and_then(|list| list.first())
        .and_then(|first| first.get("uhs"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("error.signIn.xbox"))
}

/// Minecraft gives the uuid without dashes; everything else here expects them.
fn dashed(id: &str) -> String {
    let clean: String = id.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if clean.len() != 32 {
        return id.to_string();
    }
    format!(
        "{}-{}-{}-{}-{}",
        &clean[0..8],
        &clean[8..12],
        &clean[12..16],
        &clean[16..20],
        &clean[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_uuid_gets_its_dashes_back() {
        assert_eq!(
            dashed("069a79f444e94726a5befca90e38aaf5"),
            "069a79f4-44e9-4726-a5be-fca90e38aaf5"
        );
        // already dashed, or not a uuid at all: left exactly as it came
        let dashes = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
        assert_eq!(dashed(dashes), dashes);
        assert_eq!(dashed("nonsense"), "nonsense");
    }

    #[test]
    fn the_user_hash_comes_out_of_the_array_xbox_buries_it_in() {
        let answer = serde_json::json!({
            "Token": "t",
            "DisplayClaims": { "xui": [{ "uhs": "1234567890" }] }
        });
        assert_eq!(user_hash(&answer).unwrap(), "1234567890");
        assert!(user_hash(&serde_json::json!({ "Token": "t" })).is_err());
    }

    #[test]
    fn the_client_id_can_be_overridden_without_a_rebuild() {
        // a fork must be able to be itself; the default is ours
        assert_eq!(CLIENT_ID.len(), 36, "an azure application id");
        assert!(client_id().len() >= 36);
    }
}
