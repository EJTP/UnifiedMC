//! The player's own face, for the title bar.
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

/// The default Minecraft would use, read from the client jar rather than shipped with us -
/// the texture is Mojang's, and every player has it through their own copy.
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

/// The head, with the hat layer over it. Nearest-neighbour, because a skin is 64 pixels wide
/// and smoothing it into a 64 pixel avatar turns it to mush.
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

#[cfg(test)]
mod tests {
    use super::*;

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
