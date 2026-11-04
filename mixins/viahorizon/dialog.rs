// SPDX-License-Identifier: GPL-3.0-or-later
// Walk only dialog/text positions defined by RegistryDataRewriter. In
// particular, custom click payloads and item custom_data remain opaque.
use crate::{Error, Result, buf::Reader, packets::configuration as cfg, vfp_wire::Packet};
use horizon_world::nbt::Tag;
fn local(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}
fn kind(tag: &Tag) -> String {
    match tag.get("type") {
        Some(Tag::String(value)) => value.to_string_lossy(),
        _ => String::new(),
    }
}
fn text(tag: &mut Tag, field: &str, protocol: i32, depth: usize) -> Result<()> {
    if let Some(value) = tag.get_mut(field) {
        crate::vfp_play::text_at(value, depth + 1, protocol)?;
    }
    Ok(())
}
fn action(tag: &mut Tag, protocol: i32, depth: usize) -> Result<()> {
    text(tag, "label", protocol, depth)?;
    text(tag, "tooltip", protocol, depth)
}
fn body(tag: &mut Tag, protocol: i32, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Dialog bodies nest too deeply"));
    }
    if let Tag::List(bodies) = tag {
        for value in bodies {
            body(value, protocol, depth + 1)?;
        }
        return Ok(());
    }
    match local(&kind(tag)) {
        "plain_message" => text(tag, "contents", protocol, depth)?,
        "item" => {
            text(tag, "description", protocol, depth)?;
            if let Some(description) = tag.get_mut("description") {
                text(description, "contents", protocol, depth + 1)?;
            }
            if let Some(item @ Tag::Compound(_)) = tag.get_mut("item") {
                crate::vfp_play::hover_item(item, depth + 1, protocol)?;
            }
        }
        _ => (),
    }
    Ok(())
}
pub(crate) fn upgrade_at(tag: &mut Tag, protocol: i32, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Dialogs nest too deeply"));
    }
    match tag {
        Tag::String(_) => return Ok(()), // Registry reference.
        Tag::List(values) => {
            for value in values {
                upgrade_at(value, protocol, depth + 1)?;
            }
            return Ok(());
        }
        Tag::Compound(_) => (),
        _ => return Err(Error::new("Invalid dialog definition")),
    }
    text(tag, "title", protocol, depth)?;
    text(tag, "external_title", protocol, depth)?;
    if let Some(Tag::List(inputs)) = tag.get_mut("inputs") {
        for input in inputs {
            text(input, "label", protocol, depth + 1)?;
            if local(&kind(input)) == "single_option" {
                if let Some(Tag::List(options)) = input.get_mut("options") {
                    for option in options {
                        text(option, "display", protocol, depth + 2)?;
                    }
                }
            }
        }
    }
    let kind = kind(tag);
    let fields: &[&str] = match local(&kind) {
        "confirmation" => &["yes", "no"],
        "dialog_list" | "multi_action" | "server_links" => &["exit_action"],
        "notice" => &["action"],
        _ => &[],
    };
    for field in fields {
        if let Some(value) = tag.get_mut(field) {
            action(value, protocol, depth + 1)?;
        }
    }
    if local(&kind) == "dialog_list" {
        if let Some(value) = tag.get_mut("dialogs") {
            upgrade_at(value, protocol, depth + 1)?;
        }
    } else if local(&kind) == "multi_action" {
        if let Some(Tag::List(actions)) = tag.get_mut("actions") {
            for value in actions {
                action(value, protocol, depth + 1)?;
            }
        }
    }
    if let Some(value) = tag.get_mut("body") {
        body(value, protocol, depth + 1)?;
    }
    Ok(())
}
pub fn configuration(packet: &mut Packet, protocol: i32) -> Result<()> {
    let name = usize::try_from(packet.0)
        .ok()
        .and_then(|id| crate::vfp_tables::NEW_CONFIGURATION.get(id));
    if !matches!(name, Some(&"REGISTRY_DATA" | &"SHOW_DIALOG")) {
        return Ok(());
    }
    let mut decoded = cfg::Clientbound::decode(packet.0, &packet.1)?;
    match &mut decoded {
        cfg::Clientbound::RegistryData(registry) => {
            for entry in &mut registry.entries {
                let Some(tag) = &mut entry.data else {
                    continue;
                };
                match local(&registry.registry) {
                    "dialog" => upgrade_at(tag, protocol, 0)?,
                    "enchantment" | "trim_material" | "trim_pattern" | "jukebox_song"
                    | "instrument" => text(tag, "description", protocol, 0)?,
                    "painting_variant" => {
                        text(tag, "title", protocol, 0)?;
                        text(tag, "author", protocol, 0)?;
                    }
                    _ => (),
                }
            }
        }
        cfg::Clientbound::ShowDialog(dialog) => upgrade_at(&mut dialog.dialog, protocol, 0)?,
        _ => (),
    }
    let encoded = decoded.encode();
    let mut r = Reader::new(&encoded);
    r.varint()?;
    packet.1 = r.rest().to_vec();
    Ok(())
}
