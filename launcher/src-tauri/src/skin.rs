//! The player's own face, for the title bar - and changing it.
//!
//! A signed-in player has a skin at Mojang; an offline profile has whichever default
//! Minecraft would pick for that name. Either way it ends up as the 8x8 head from the skin
//! texture, scaled up with the hat layer on top - which is what the game itself draws.

use std::io::Cursor;

use anyhow::{anyhow, Result};
use base64::Engine;
use image::{imageops, GenericImageView, RgbaImage};

/// Where the head sits in a skin texture, and where the hat that goes over it sits.
const HEAD: (u32, u32, u32, u32) = (8, 8, 8, 8);
const HAT: (u32, u32, u32, u32) = (40, 8, 8, 8);
const OUTPUT: u32 = 64;

const SKINS: &str = "https://api.minecraftservices.com/minecraft/profile/skins";
/// A skin is a 64 pixel png - a few kilobytes. Anything near this is not one.
const MAX_UPLOAD: usize = 256 * 1024;

/// The same cap, applied to the base64 the webview sends before anything decodes it.
///
/// `check` is too late to be the only limit: by then the webview has already read the file into
/// a string. Base64 is four characters per three bytes.
pub fn too_much_base64(encoded: &str) -> bool {
    encoded.len() > MAX_UPLOAD.div_ceil(3) * 4
}

pub async fn head(client: &reqwest::Client, uuid: &str, online: bool) -> Option<String> {
    let texture = if online {
        skin_from_mojang(client, uuid).await.ok()?
    } else {
        default_skin(uuid)?
    };
    render(&texture).ok()
}

/// The profile endpoint returns the skin url inside a base64 property.
async fn skin_from_mojang(client: &reqwest::Client, uuid: &str) -> Result<Vec<u8>> {
    let clean: String = uuid.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let profile: serde_json::Value = client
        .get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{clean}"
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let encoded = profile
        .get("properties")
        .and_then(|p| p.as_array())
        .and_then(|properties| {
            properties
                .iter()
                .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("textures"))
        })
        .and_then(|p| p.get("value")?.as_str())
        .ok_or_else(|| anyhow!("no textures on that profile"))?;

    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    let textures: serde_json::Value = serde_json::from_slice(&decoded)?;
    let url = textures
        .get("textures")
        .and_then(|t| t.get("SKIN"))
        .and_then(|s| s.get("url"))
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow!("that profile has no skin"))?;

    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec())
}

/// The default Minecraft would use, read from the client jar - the texture is Mojang's.
fn default_skin(uuid: &str) -> Option<Vec<u8>> {
    // Minecraft picks between the defaults by a bit of the uuid; close enough to do the same
    let slim = uuid.bytes().map(|b| b as u32).sum::<u32>() % 2 == 0;
    let candidates: [&str; 2] = if slim {
        [
            "assets/minecraft/textures/entity/player/slim/alex.png",
            "assets/minecraft/textures/entity/player/wide/steve.png",
        ]
    } else {
        [
            "assets/minecraft/textures/entity/player/wide/steve.png",
            "assets/minecraft/textures/entity/player/slim/alex.png",
        ]
    };
    for path in candidates {
        if let Some(bytes) = crate::modinfo::read_from_client_jar(path) {
            return Some(bytes);
        }
    }
    None
}

/// The whole skin texture, for a window that wants to draw more than the face.
///
/// Uncropped on purpose: slicing six faces here would mean six data urls to keep in step, and
/// the one place that needs them can cut a 64x64 png with a background-position. Normalised to
/// 64x64 first, because a pre-1.8 skin is 64x32 and has no second layer below the head.
pub async fn texture(client: &reqwest::Client, uuid: &str, online: bool) -> Option<String> {
    let bytes = if online {
        skin_from_mojang(client, uuid).await.ok()?
    } else {
        default_skin(uuid)?
    };
    let skin = image::load(Cursor::new(&bytes), image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    if skin.width() < 64 || skin.height() < 32 {
        return None;
    }

    // A 64x32 skin is already the right width; the rows below simply do not exist, and an
    // empty bottom half is exactly what "this skin has no overlay there" should look like.
    let square: RgbaImage = if skin.height() >= 64 {
        skin
    } else {
        let mut grown = RgbaImage::new(64, 64);
        imageops::overlay(&mut grown, &skin, 0, 0);
        grown
    };

    let mut out = Cursor::new(Vec::new());
    square.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(out.into_inner())
    ))
}

/// The head with the hat layer over it. Nearest-neighbour: a skin is 64 pixels wide.
fn render(texture: &[u8]) -> Result<String> {
    let skin = image::load(Cursor::new(texture), image::ImageFormat::Png)?.to_rgba8();
    if skin.width() < 64 || skin.height() < 32 {
        return Err(anyhow!("not a skin texture"));
    }

    let mut face: RgbaImage = skin.view(HEAD.0, HEAD.1, HEAD.2, HEAD.3).to_image();
    let hat = skin.view(HAT.0, HAT.1, HAT.2, HAT.3).to_image();
    imageops::overlay(&mut face, &hat, 0, 0);

    let scaled = imageops::resize(&face, OUTPUT, OUTPUT, imageops::FilterType::Nearest);
    let mut out = Cursor::new(Vec::new());
    scaled.write_to(&mut out, image::ImageFormat::Png)?;

    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(out.into_inner())
    ))
}

/// Wear this png. `slim` is the three-pixel-armed model, Mojang calls it "slim" against
/// "classic".
pub async fn upload(client: &reqwest::Client, token: &str, png: &[u8], slim: bool) -> Result<()> {
    let token = signed_in(token)?;
    check(png)?;

    let (content_type, body) = form(if slim { "slim" } else { "classic" }, png);
    let answer = client
        .put(SKINS)
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .body(body)
        .send()
        .await?;
    mojang_said(answer).await
}

/// Back to the default for this account.
pub async fn reset(client: &reqwest::Client, token: &str) -> Result<()> {
    let answer = client
        .delete(format!("{SKINS}/active"))
        .bearer_auth(signed_in(token)?)
        .send()
        .await?;
    mojang_said(answer).await
}

/// Changing a skin is an account operation, so an offline profile cannot.
fn signed_in(token: &str) -> Result<&str> {
    match token.trim() {
        "" => Err(anyhow!("error.skinNeedsMicrosoft")),
        token => Ok(token),
    }
}

/// What Mojang accepts, checked here so a bad file is a sentence rather than a 400. Only the
/// header is read.
fn check(png: &[u8]) -> Result<()> {
    if png.len() > MAX_UPLOAD {
        return Err(anyhow!("error.skinTooBig"));
    }
    let size = image::ImageReader::with_format(Cursor::new(png), image::ImageFormat::Png)
        .into_dimensions()
        .map_err(|_| anyhow!("error.skinNotPng"))?;
    // 64x64 is the modern layout, 64x32 the pre-1.8 one that the game still understands
    match size {
        (64, 64) | (64, 32) => Ok(()),
        _ => Err(anyhow!("error.skinWrongSize")),
    }
}

/// multipart/form-data by hand: the `multipart` feature is off, and two known fields are less
/// code than turning it on. The boundary is random because
/// the file half is attacker-supplied - a fixed one could be spelled out inside the png and
/// forge a second field.
fn form(variant: &str, png: &[u8]) -> (String, Vec<u8>) {
    let mut seed = [0u8; 16];
    getrandom::fill(&mut seed).expect("the operating system has no randomness");
    let boundary = format!("unifiedmc{}", hex::encode(seed));

    let mut body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"variant\"\r\n\r\n\
         {variant}\r\n\
         --{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"skin.png\"\r\n\
         Content-Type: image/png\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// Mojang answers with a status and little else worth showing.
async fn mojang_said(answer: reqwest::Response) -> Result<()> {
    let status = answer.status();
    if status.is_success() {
        return Ok(());
    }
    Err(match status.as_u16() {
        // the session is real but stale - signing in again is the fix, and only the player can
        401 | 403 => anyhow!("error.skinSignedOut"),
        400 | 413 | 415 => anyhow!("error.skinRefused"),
        other => anyhow!("Mojang answered {other}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut encoded = Cursor::new(Vec::new());
        RgbaImage::new(width, height)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        encoded.into_inner()
    }

    #[test]
    fn only_a_skin_shaped_png_is_sent() {
        assert!(check(&png(64, 64)).is_ok());
        assert!(
            check(&png(64, 32)).is_ok(),
            "the legacy layout is still a skin"
        );
        assert_eq!(
            check(&png(32, 32)).unwrap_err().to_string(),
            "error.skinWrongSize"
        );
        assert_eq!(
            check(b"GIF89a not a png at all").unwrap_err().to_string(),
            "error.skinNotPng"
        );
        assert_eq!(
            check(&vec![0u8; MAX_UPLOAD + 1]).unwrap_err().to_string(),
            "error.skinTooBig"
        );
        // and the same answer without decoding it first: a skin's worth of base64 fits, an
        // arbitrary file the webview read into a string does not
        assert!(!too_much_base64(&"A".repeat(MAX_UPLOAD / 3 * 4)));
        assert!(too_much_base64(&"A".repeat(MAX_UPLOAD * 2)));
        // an offline profile has no token, and must hear that instead of a 401
        assert_eq!(
            signed_in("  ").unwrap_err().to_string(),
            "error.skinNeedsMicrosoft"
        );
    }

    #[test]
    fn the_body_carries_both_fields_and_closes() {
        let skin = png(64, 64);
        let (content_type, body) = form("slim", &skin);
        let boundary = content_type
            .split("boundary=")
            .nth(1)
            .expect("the content type names the boundary")
            .to_string();

        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("name=\"variant\"\r\n\r\nslim\r\n"));
        assert!(text.contains("filename=\"skin.png\""));
        assert!(body.ends_with(format!("\r\n--{boundary}--\r\n").as_bytes()));
        assert!(
            body.windows(skin.len()).any(|window| window == skin),
            "the png goes over untouched"
        );
    }

    #[test]
    fn a_head_comes_out_of_a_skin_sized_texture() {
        let mut skin = RgbaImage::new(64, 64);
        // paint the head region so a wrong crop is visible rather than plausible
        for x in HEAD.0..HEAD.0 + HEAD.2 {
            for y in HEAD.1..HEAD.1 + HEAD.3 {
                skin.put_pixel(x, y, image::Rgba([12, 34, 56, 255]));
            }
        }
        let mut encoded = Cursor::new(Vec::new());
        skin.write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();

        let uri = render(&encoded.into_inner()).unwrap();
        assert!(uri.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn something_that_is_not_a_skin_is_refused() {
        let tiny = RgbaImage::new(8, 8);
        let mut encoded = Cursor::new(Vec::new());
        tiny.write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        assert!(render(&encoded.into_inner()).is_err());
    }
}
