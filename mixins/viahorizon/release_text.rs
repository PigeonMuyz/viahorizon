// SPDX-License-Identifier: GPL-3.0-or-later
// Section-sign styling at the old item-text boundaries. Unrecognized format
// codes stay literal; custom item strings are not treated as components.
use serde_json::{Value, json};
const COLORS: &[&str] = &[
    "black",
    "dark_blue",
    "dark_green",
    "dark_aqua",
    "dark_red",
    "dark_purple",
    "gold",
    "gray",
    "dark_gray",
    "blue",
    "green",
    "aqua",
    "red",
    "light_purple",
    "yellow",
    "white",
];
pub fn json(text: &str) -> String {
    let mut style = serde_json::Map::new();
    style.insert("italic".into(), false.into());
    let mut parts = Vec::new();
    let mut run = String::new();
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '§' {
            run.push(character);
            continue;
        }
        let Some(&code) = characters.peek() else {
            run.push(character);
            break;
        };
        let code = code.to_ascii_lowercase();
        if !code.is_ascii_hexdigit() && !matches!(code, 'k' | 'l' | 'm' | 'n' | 'o' | 'r') {
            run.push(character);
            continue;
        }
        characters.next();
        if !run.is_empty() {
            let mut part = style.clone();
            part.insert("text".into(), std::mem::take(&mut run).into());
            parts.push(Value::Object(part));
        }
        if let Some(color) = code.to_digit(16) {
            style.clear();
            style.insert("italic".into(), false.into());
            style.insert("color".into(), COLORS[color as usize].into());
        } else if code == 'r' {
            style.clear();
            style.insert("italic".into(), false.into());
        } else {
            style.insert(
                match code {
                    'k' => "obfuscated",
                    'l' => "bold",
                    'm' => "strikethrough",
                    'n' => "underlined",
                    _ => "italic",
                }
                .into(),
                true.into(),
            );
        }
    }
    if !run.is_empty() || parts.is_empty() {
        style.insert("text".into(), run.into());
        parts.push(Value::Object(style));
    }
    if parts.len() == 1 {
        parts.remove(0).to_string()
    } else {
        json!({"text":"","italic":false,"extra":parts}).to_string()
    }
}
pub fn component(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Null) => r#"{"text":""}"#.to_owned(),
        Ok(_) => text.to_owned(),
        Err(_) => json(text),
    }
}

pub fn legacy(text: &str) -> String {
    fn visit(value: &Value, out: &mut String, depth: usize) {
        if depth > 64 {
            return;
        }
        match value {
            Value::String(value) => out.push_str(value),
            Value::Array(values) => {
                for value in values {
                    visit(value, out, depth + 1);
                }
            }
            Value::Object(fields) => {
                if let Some(index) = fields
                    .get("color")
                    .and_then(Value::as_str)
                    .and_then(|color| COLORS.iter().position(|&name| name == color))
                {
                    out.push('§');
                    out.push(char::from_digit(index as u32, 16).unwrap());
                }
                for (name, code) in [
                    ("obfuscated", 'k'),
                    ("bold", 'l'),
                    ("strikethrough", 'm'),
                    ("underlined", 'n'),
                    ("italic", 'o'),
                ] {
                    if fields.get(name).and_then(Value::as_bool) == Some(true) {
                        out.push('§');
                        out.push(code);
                    }
                }
                if let Some(value) = fields.get("text").and_then(Value::as_str) {
                    out.push_str(value);
                } else if let Some(value) = fields.get("translate").and_then(Value::as_str) {
                    out.push_str(value);
                }
                if let Some(value) = fields.get("extra") {
                    visit(value, out, depth + 1);
                }
            }
            _ => (),
        }
    }
    match serde_json::from_str(text) {
        Ok(value) => {
            let mut result = String::new();
            visit(&value, &mut result, 0);
            result
        }
        Err(_) => text.to_owned(),
    }
}
