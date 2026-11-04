// SPDX-License-Identifier: GPL-3.0-or-later
// 1.21.2-1.21.4 text events and the older NBT representation of hover items.
use crate::{Error, Result};
use horizon_world::nbt::{JavaString, Tag};
fn key(s: &str) -> JavaString {
    s.into()
}
fn json_tag(value: serde_json::Value, depth: usize) -> Result<Tag> {
    if depth > 64 {
        return Err(Error::new("JSON text nests too deeply"));
    }
    Ok(match value {
        serde_json::Value::Null => Tag::String("".into()),
        serde_json::Value::Bool(value) => Tag::Byte(i8::from(value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                if let Ok(value) = i32::try_from(value) {
                    Tag::Int(value)
                } else {
                    Tag::Long(value)
                }
            } else {
                Tag::Double(
                    value
                        .as_f64()
                        .ok_or_else(|| Error::new("Invalid JSON number"))?
                        .to_bits(),
                )
            }
        }
        serde_json::Value::String(value) => Tag::String(value.as_str().into()),
        serde_json::Value::Array(values) => {
            let values = values
                .into_iter()
                .map(|value| json_tag(value, depth + 1))
                .collect::<Result<Vec<_>>>()?;
            if let Some(first) = values.first() {
                if values
                    .iter()
                    .any(|value| std::mem::discriminant(value) != std::mem::discriminant(first))
                {
                    return Err(Error::new("Mixed types in JSON NBT data list"));
                }
            }
            Tag::List(values)
        }
        serde_json::Value::Object(values) => Tag::Compound(
            values
                .into_iter()
                .map(|(name, value)| Ok((name.as_str().into(), json_tag(value, depth + 1)?)))
                .collect::<Result<_>>()?,
        ),
    })
}
fn json_hover(value: serde_json::Value, depth: usize) -> Result<Tag> {
    if depth > 64 {
        return Err(Error::new("Hover JSON nests too deeply"));
    }
    let serde_json::Value::Object(values) = value else {
        return json_tag(value, depth + 1);
    };
    let action = values
        .get("action")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut fields = std::collections::BTreeMap::new();
    for (name, value) in values {
        let value = if matches!(name.as_str(), "contents" | "value") && action == "show_text" {
            json_component(value, depth + 1)?
        } else if name == "contents" && action == "show_entity" {
            if let serde_json::Value::Object(values) = value {
                let mut entity = std::collections::BTreeMap::new();
                for (name, value) in values {
                    entity.insert(
                        key(&name),
                        if name == "name" {
                            json_component(value, depth + 1)?
                        } else {
                            json_tag(value, depth + 1)?
                        },
                    );
                }
                Tag::Compound(entity)
            } else {
                json_tag(value, depth + 1)?
            }
        } else {
            json_tag(value, depth + 1)?
        };
        fields.insert(key(&name), value);
    }
    Ok(Tag::Compound(fields))
}
fn json_component(value: serde_json::Value, depth: usize) -> Result<Tag> {
    if depth > 64 {
        return Err(Error::new("Component JSON nests too deeply"));
    }
    match value {
        serde_json::Value::Array(values) => {
            let mut components = Vec::new();
            for value in values {
                let value = json_component(value, depth + 1)?;
                components.push(Tag::compound([
                    ("text", Tag::String("".into())),
                    ("extra", Tag::List(vec![value])),
                ]));
            }
            Ok(Tag::List(components))
        }
        serde_json::Value::Object(values) => {
            let mut fields = std::collections::BTreeMap::new();
            for (name, value) in values {
                let value = match name.as_str() {
                    "extra" | "with" | "separator" => json_component(value, depth + 1)?,
                    "hoverEvent" | "hover_event" => json_hover(value, depth + 1)?,
                    _ => json_tag(value, depth + 1)?,
                };
                fields.insert(key(&name), value);
            }
            Ok(Tag::Compound(fields))
        }
        serde_json::Value::Bool(value) => Ok(Tag::String(value.to_string().as_str().into())),
        serde_json::Value::Number(value) => Ok(Tag::String(value.to_string().as_str().into())),
        value => json_tag(value, depth),
    }
}
pub fn json_text(value: &str, depth: usize) -> Result<Tag> {
    let value = serde_json::from_str(value)
        .ok()
        .and_then(|value| json_component(value, depth).ok())
        .unwrap_or_else(|| {
            Tag::String(
                if matches!(
                    value.trim_start().as_bytes().first(),
                    Some(b'{' | b'[' | b'"')
                ) {
                    "<error>".into()
                } else {
                    value.into()
                },
            )
        });
    Ok(value)
}
fn click(tag: &mut Tag) -> bool {
    let Tag::Compound(fields) = tag else {
        return false;
    };
    let action = match fields.get(&key("action")) {
        Some(Tag::String(value)) => value.to_string_lossy(),
        _ => return false,
    };
    let value = fields.get(&key("value")).cloned();
    match action.as_str() {
        "open_url" => {
            let Some(Tag::String(value)) = value else {
                return false;
            };
            let url = value.to_string_lossy();
            if !url.split_once(':').is_some_and(|(scheme, _)| {
                scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
            }) || url
                .chars()
                .any(|c| c.is_ascii_whitespace() || c.is_control())
            {
                return false;
            }
            fields.insert(key("url"), Tag::String(value));
        }
        "change_page" => {
            let page = match value {
                Some(Tag::String(value)) => value.to_string_lossy().parse::<i32>().ok(),
                Some(Tag::Int(value)) => Some(value),
                _ => None,
            };
            let Some(page) = page.filter(|page| *page > 0) else {
                return false;
            };
            fields.insert(key("page"), Tag::Int(page));
        }
        "run_command" | "suggest_command" => {
            let Some(Tag::String(mut value)) = value else {
                return false;
            };
            value.0.retain(|unit| {
                (*unit >= 32 && *unit != 127 && *unit != 167)
                    || (action == "suggest_command" && *unit == 10)
            });
            fields.insert(key("command"), Tag::String(value));
        }
        _ => (),
    }
    true
}
pub(crate) fn events(tag: &mut Tag, depth: usize, protocol: i32) -> Result<()> {
    let Tag::Compound(fields) = tag else {
        return Ok(());
    };
    if let Some(mut value) = fields.remove(&key("clickEvent")) {
        if click(&mut value) {
            fields.insert(key("click_event"), value);
        }
    }
    if let Some(mut hover) = fields.remove(&key("hoverEvent")) {
        let mut legacy_contents = false;
        let action = match hover.get("action") {
            Some(Tag::String(value)) => value.to_string_lossy(),
            _ => String::new(),
        };
        if let Tag::Compound(data) = &mut hover {
            if !data.contains_key(&key("contents")) && action != "show_text" {
                if let Some(Tag::String(legacy)) = data.remove(&key("value")) {
                    if let Ok((tag, used)) =
                        horizon_world::nbt::parse_snbt(&legacy.to_string_lossy())
                    {
                        if legacy.to_string_lossy()[used..].trim().is_empty() {
                            data.insert(key("contents"), tag);
                            legacy_contents = true;
                        }
                    }
                }
            }
            if action == "show_text" {
                if !data.contains_key(&key("value")) {
                    if let Some(value) = data.remove(&key("contents")) {
                        data.insert(key("value"), value);
                    }
                }
            } else if let Some(contents) = data.remove(&key("contents")) {
                match (action.as_str(), contents) {
                    ("show_item", Tag::String(id)) => {
                        data.insert(key("id"), Tag::String(id));
                    }
                    ("show_item", Tag::Compound(mut item)) => {
                        if protocol < 766 || item.contains_key(&key("tag")) {
                            if let Some(Tag::String(snbt)) = item.get(&key("tag")) {
                                let snbt = snbt.to_string_lossy();
                                let (tag, used) =
                                    horizon_world::nbt::parse_snbt(&snbt).map_err(Error::new)?;
                                if !snbt[used..].trim().is_empty()
                                    || !matches!(tag, Tag::Compound(_))
                                {
                                    return Err(Error::new("Invalid legacy hover item NBT"));
                                }
                                item.insert(key("tag"), tag);
                            }
                            if let Some(count) = item.remove(&key("count")) {
                                item.entry(key("Count")).or_insert(count);
                            }
                            item.entry(key("Count")).or_insert(Tag::Byte(1));
                            let mut stack = Tag::Compound(item);
                            horizon_world::datafix::update(
                                horizon_world::datafix::schema::Ty::ItemStack,
                                &mut stack,
                                crate::vfp_data_versions::for_protocol(protocol.min(765))
                                    .unwrap_or(crate::vfp_data_versions::P765),
                                crate::vfp_data_versions::P766,
                            );
                            let Tag::Compound(converted) = stack else {
                                return Err(Error::new("Invalid upgraded hover item"));
                            };
                            item = converted;
                        }
                        if let Some(count) = item.remove(&key("Count")) {
                            item.entry(key("count")).or_insert(count);
                        }
                        for name in ["id", "count", "components"] {
                            if let Some(value) = item.remove(&key(name)) {
                                data.insert(key(name), value);
                            }
                        }
                    }
                    ("show_entity", Tag::Compound(mut entity)) => {
                        for (before, after) in [("id", "uuid"), ("type", "id"), ("name", "name")] {
                            if let Some(mut value) = entity.remove(&key(before)) {
                                if before == "name" && legacy_contents {
                                    if let Tag::String(json) = &value {
                                        value = json_text(&json.to_string_lossy(), depth + 1)?;
                                    }
                                }
                                data.insert(key(after), value);
                            }
                        }
                    }
                    _ => (),
                }
            }
        }
        fields.insert(key("hover_event"), hover);
    }
    Ok(())
}
fn remove(fields: &mut std::collections::BTreeMap<JavaString, Tag>, name: &str) -> Option<Tag> {
    let bare = fields.remove(&key(name));
    fields.remove(&key(&format!("minecraft:{name}"))).or(bare)
}
pub(crate) fn hover(
    components: &mut std::collections::BTreeMap<JavaString, Tag>,
    depth: usize,
    protocol: i32,
) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Hover component text nests too deeply"));
    }
    if protocol < 769 {
        if let Some(value) = remove(components, "custom_model_data") {
            let value = if let Tag::Int(value) = value {
                Tag::compound([(
                    "floats",
                    Tag::List(vec![Tag::Float((value as f32).to_bits())]),
                )])
            } else {
                value
            };
            components.insert(key("minecraft:custom_model_data"), value);
        }
    }
    let hide = remove(components, "hide_tooltip").is_some();
    let mut hidden = Vec::new();
    if remove(components, "hide_additional_tooltip").is_some() {
        hidden.extend(
            crate::vfp_items_1214::EXTRA
                .iter()
                .map(|name| Tag::String(format!("minecraft:{name}").as_str().into())),
        );
    }
    for name in [
        "unbreakable",
        "can_place_on",
        "can_break",
        "dyed_color",
        "attribute_modifiers",
        "trim",
        "enchantments",
        "stored_enchantments",
        "jukebox_playable",
    ] {
        let Some(mut value) = remove(components, name) else {
            continue;
        };
        if let Tag::Compound(fields) = &mut value {
            if matches!(
                fields.remove(&key("show_in_tooltip")),
                Some(Tag::Byte(0) | Tag::Int(0))
            ) {
                hidden.push(Tag::String(format!("minecraft:{name}").as_str().into()));
            }
            match name {
                "dyed_color" => {
                    if let Some(rgb) = fields.remove(&key("rgb")) {
                        value = rgb;
                    }
                }
                "attribute_modifiers" => {
                    if let Some(modifiers) = fields.remove(&key("modifiers")) {
                        value = modifiers;
                    }
                }
                "can_place_on" | "can_break" => {
                    if let Some(predicates) = fields.remove(&key("predicates")) {
                        value = predicates;
                    }
                }
                "enchantments" | "stored_enchantments" => {
                    if let Some(Tag::Compound(levels)) = fields.remove(&key("levels")) {
                        fields.extend(levels);
                    }
                }
                _ => (),
            }
        }
        components.insert(key(&format!("minecraft:{name}")), value);
    }
    if hide || !hidden.is_empty() {
        components.insert(
            key("minecraft:tooltip_display"),
            Tag::compound([
                ("hide_tooltip", Tag::Byte(i8::from(hide))),
                ("hidden_components", Tag::List(hidden)),
            ]),
        );
    }
    for name in ["item_name", "custom_name"] {
        if let Some(value) = remove(components, name) {
            let value = if let Tag::String(value) = value {
                json_text(&value.to_string_lossy(), depth + 1)?
            } else {
                value
            };
            components.insert(key(&format!("minecraft:{name}")), value);
        }
    }
    if let Some(Tag::List(values)) = remove(components, "lore") {
        let mut lore = Vec::new();
        for value in values {
            let value = if let Tag::String(value) = value {
                json_text(&value.to_string_lossy(), depth + 1)?
            } else {
                value
            };
            lore.push(Tag::compound([
                ("text", Tag::String("".into())),
                ("extra", Tag::List(vec![value])),
            ]));
        }
        components.insert(key("minecraft:lore"), Tag::List(lore));
    }
    if let Some(mut book) = remove(components, "written_book_content") {
        if let Some(Tag::List(pages)) = book.get_mut("pages") {
            for page in pages {
                match page {
                    Tag::String(value) => {
                        *page = json_text(&value.to_string_lossy(), depth + 1)?;
                    }
                    Tag::Compound(fields) => {
                        for name in ["raw", "filtered"] {
                            if let Some(Tag::String(value)) = fields.get(&key(name)) {
                                let value = json_text(&value.to_string_lossy(), depth + 1)?;
                                fields.insert(key(name), value);
                            }
                        }
                    }
                    _ => (),
                }
            }
        }
        components.insert(key("minecraft:written_book_content"), book);
    }
    remove(components, "instrument");
    remove(components, "jukebox_playable");
    Ok(())
}
