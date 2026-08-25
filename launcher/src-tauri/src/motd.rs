//! Turning a server description into something drawable with its colours. A server sends it as
//! a chat component tree or as a legacy string with section signs.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    /// A CSS colour, or none for the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underlined: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strikethrough: bool,
    /// `§k`, which Minecraft draws as shifting characters.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub obfuscated: bool,
}

/// Minecraft's sixteen colours, as they are actually rendered.
fn named(code: char) -> Option<&'static str> {
    Some(match code {
        '0' => "#000000",
        '1' => "#0000AA",
        '2' => "#00AA00",
        '3' => "#00AAAA",
        '4' => "#AA0000",
        '5' => "#AA00AA",
        '6' => "#FFAA00",
        '7' => "#AAAAAA",
        '8' => "#555555",
        '9' => "#5555FF",
        'a' => "#55FF55",
        'b' => "#55FFFF",
        'c' => "#FF5555",
        'd' => "#FF55FF",
        'e' => "#FFFF55",
        'f' => "#FFFFFF",
        _ => return None,
    })
}

fn by_name(name: &str) -> Option<String> {
    let code = match name {
        "black" => '0',
        "dark_blue" => '1',
        "dark_green" => '2',
        "dark_aqua" => '3',
        "dark_red" => '4',
        "dark_purple" => '5',
        "gold" => '6',
        "gray" | "grey" => '7',
        "dark_gray" | "dark_grey" => '8',
        "blue" => '9',
        "green" => 'a',
        "aqua" => 'b',
        "red" => 'c',
        "light_purple" => 'd',
        "yellow" => 'e',
        "white" => 'f',
        // a component may also carry a hex colour directly
        other if other.starts_with('#') => return Some(other.to_string()),
        _ => return None,
    };
    named(code).map(str::to_string)
}

pub fn parse(value: &serde_json::Value) -> Vec<Span> {
    let mut spans = Vec::new();
    walk(value, &Span::default(), &mut spans);
    spans.retain(|span| !span.text.is_empty());
    spans
}

fn walk(value: &serde_json::Value, inherited: &Span, out: &mut Vec<Span>) {
    match value {
        // a bare string still carries section codes
        serde_json::Value::String(text) => out.extend(from_codes(text, inherited)),
        serde_json::Value::Array(parts) => {
            for part in parts {
                walk(part, inherited, out);
            }
        }
        serde_json::Value::Object(map) => {
            // formatting is inherited by children unless they say otherwise
            let mut style = inherited.clone();
            if let Some(color) = map.get("color").and_then(|v| v.as_str()) {
                style.color = by_name(color);
            }
            for (key, flag) in [
                ("bold", &mut style.bold),
                ("italic", &mut style.italic),
                ("underlined", &mut style.underlined),
                ("strikethrough", &mut style.strikethrough),
                ("obfuscated", &mut style.obfuscated),
            ] {
                if let Some(set) = map.get(key).and_then(|v| v.as_bool()) {
                    *flag = set;
                }
            }

            if let Some(text) = map.get("text").and_then(|v| v.as_str()) {
                out.extend(from_codes(text, &style));
            }
            if let Some(extra) = map.get("extra") {
                walk(extra, &style, out);
            }
        }
        _ => {}
    }
}

/// Section codes, which appear even inside a component's text.
fn from_codes(text: &str, inherited: &Span) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut current = Span {
        text: String::new(),
        ..inherited.clone()
    };
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '§' {
            current.text.push(ch);
            continue;
        }
        let Some(code) = chars.next() else { break };
        let code = code.to_ascii_lowercase();

        if !current.text.is_empty() {
            spans.push(current.clone());
            current.text.clear();
        }

        match code {
            'r' => {
                current = Span {
                    text: String::new(),
                    ..inherited.clone()
                }
            }
            'l' => current.bold = true,
            'o' => current.italic = true,
            'n' => current.underlined = true,
            'm' => current.strikethrough = true,
            'k' => current.obfuscated = true,
            _ => {
                if let Some(colour) = named(code) {
                    // a colour resets the decorations, the way Minecraft does it
                    current = Span {
                        text: String::new(),
                        color: Some(colour.to_string()),
                        ..Default::default()
                    };
                }
            }
        }
    }

    if !current.text.is_empty() {
        spans.push(current);
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plain_text_is_one_span() {
        let spans = parse(&json!("A Minecraft Server"));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "A Minecraft Server");
        assert!(spans[0].color.is_none());
    }

    #[test]
    fn section_codes_become_colour_and_style() {
        let spans = parse(&json!("§cRed §lBold"));
        assert_eq!(spans[0].text, "Red ");
        assert_eq!(spans[0].color.as_deref(), Some("#FF5555"));
        assert!(spans[1].bold);
        assert_eq!(
            spans[1].color.as_deref(),
            Some("#FF5555"),
            "bold keeps the colour"
        );
    }

    #[test]
    fn a_colour_resets_the_decorations_the_way_minecraft_does() {
        let spans = parse(&json!("§lBold§aGreen"));
        assert!(spans[0].bold);
        assert!(!spans[1].bold, "a colour code clears bold");
        assert_eq!(spans[1].color.as_deref(), Some("#55FF55"));
    }

    #[test]
    fn components_inherit_from_their_parent() {
        let spans = parse(&json!({
            "text": "one ", "color": "gold", "bold": true,
            "extra": [{"text": "two"}, {"text": "three", "color": "red"}]
        }));
        assert_eq!(spans[0].color.as_deref(), Some("#FFAA00"));
        assert!(spans[1].bold, "inherited from the parent");
        assert_eq!(spans[1].color.as_deref(), Some("#FFAA00"));
        assert_eq!(
            spans[2].color.as_deref(),
            Some("#FF5555"),
            "own colour wins"
        );
    }

    #[test]
    fn reset_goes_back_to_what_was_inherited() {
        let spans = parse(&json!({"text": "§lbold§rplain", "color": "aqua"}));
        assert!(spans[0].bold);
        assert!(!spans[1].bold);
        assert_eq!(
            spans[1].color.as_deref(),
            Some("#55FFFF"),
            "reset keeps the component"
        );
    }
}
