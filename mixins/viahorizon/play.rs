use crate::vfp_mapping::{Maps, V262};
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    packets::play as p,
    vfp_tables as t, vfp_wire as wire,
};
use horizon_world::nbt::{JavaString, Tag};

pub fn serverbound_262(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    let name = usize::try_from(id)
        .ok()
        .and_then(|i| t::NEW_SERVERBOUND.get(i))
        .copied()
        .ok_or_else(|| Error::new("Invalid serverbound play packet"))?;
    if let Some(data) = crate::vfp_auxiliary::serverbound(name, payload)? {
        return Ok(vec![(
            wire::packet_id(t::OLD_SERVERBOUND, name).map_err(Error::new)?,
            data,
        )]);
    }
    match name {
        "BUNDLE_ITEM_SELECTED"
        | "CHANGE_DIFFICULTY"
        | "CHANGE_GAME_MODE"
        | "CHAT_COMMAND_SIGNED"
        | "CHAT_SESSION_UPDATE"
        | "CONTAINER_BUTTON_CLICK"
        | "CONTAINER_CLOSE"
        | "CONTAINER_SLOT_STATE_CHANGED"
        | "CUSTOM_CLICK_ACTION"
        | "EDIT_BOOK"
        | "LOCK_DIFFICULTY"
        | "MOVE_VEHICLE"
        | "PADDLE_BOAT"
        | "PICK_ITEM_FROM_BLOCK"
        | "PICK_ITEM_FROM_ENTITY"
        | "PING_REQUEST"
        | "PLACE_RECIPE"
        | "PLAYER_ABILITIES"
        | "PLAYER_COMMAND"
        | "RECIPE_BOOK_CHANGE_SETTINGS"
        | "RENAME_ITEM"
        | "SELECT_TRADE"
        | "SET_BEACON"
        | "SET_COMMAND_BLOCK"
        | "SET_GAME_RULE"
        | "TELEPORT_TO_ENTITY" => {
            if matches!(
                p::Serverbound::decode(id, payload)?,
                p::Serverbound::Unknown(_, _)
            ) {
                return Err(Error::new(
                    "Horizon has no codec for this serverbound packet",
                ));
            }
            Ok(vec![(
                wire::packet_id(t::OLD_SERVERBOUND, name).map_err(Error::new)?,
                payload.to_vec(),
            )])
        }
        _ => wire::serverbound_262(id, payload).map_err(Error::new),
    }
}

pub fn text(tag: &mut Tag) -> Result<()> {
    text_version(tag, 776)
}
pub fn text_version(tag: &mut Tag, protocol: i32) -> Result<()> {
    let mut converted = tag.clone();
    text_at(&mut converted, 0, protocol)?;
    *tag = converted;
    Ok(())
}
pub(crate) fn text_at(tag: &mut Tag, depth: usize, protocol: i32) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Text components nest too deeply"));
    }
    match tag {
        Tag::List(values) => {
            for value in values {
                text_at(value, depth + 1, protocol)?;
            }
        }
        Tag::Compound(values) => {
            if protocol < 770 {
                let mut legacy = Tag::Compound(std::mem::take(values));
                crate::vfp_text_1214::events(&mut legacy, depth, protocol)?;
                let Tag::Compound(converted) = legacy else {
                    unreachable!()
                };
                *values = converted;
            }
            if protocol < 774 {
                if let Some(Tag::String(sprite)) = values.get(&JavaString::from("sprite")) {
                    let sprite = sprite.to_string_lossy();
                    let sprite = sprite
                        .split_once(':')
                        .map_or(sprite.as_str(), |(_, path)| path);
                    let atlas = if sprite.starts_with("item/") || sprite.starts_with("trims/items/")
                    {
                        Some("minecraft:items")
                    } else if sprite.starts_with("block/") {
                        Some("minecraft:blocks")
                    } else {
                        None
                    };
                    if let Some(atlas) = atlas {
                        values.insert(JavaString::from("atlas"), Tag::String(atlas.into()));
                    }
                }
            }
            if protocol < 775 {
                if let Some(Tag::String(translate)) = values.get_mut(&JavaString::from("translate"))
                {
                    match translate.to_string_lossy().as_str() {
                        "commands.time.set" => *translate = "Set the time to %s".into(),
                        "commands.time.query" => *translate = "The time is %s".into(),
                        _ => (),
                    }
                }
            }
            // Walk text positions, never arbitrary NBT in a hovered item's
            // custom_data. Such data can legitimately contain a hover_event
            // key that has nothing to do with a text component.
            for name in ["extra", "with", "separator"] {
                if let Some(child) = values.get_mut(&JavaString::from(name)) {
                    text_at(child, depth + 1, protocol)?;
                }
            }
            if let Some(hover @ Tag::Compound(_)) = values.get_mut(&JavaString::from("hover_event"))
            {
                let action = match hover.get("action") {
                    Some(Tag::String(s)) => s.to_string_lossy(),
                    _ => String::new(),
                };
                match action.as_str() {
                    "show_item" => hover_item(hover, depth + 1, protocol)?,
                    "show_text" | "show_entity" => {
                        let field = if action == "show_text" {
                            "value"
                        } else {
                            "name"
                        };
                        if let Some(child) = hover.get_mut(field) {
                            text_at(child, depth + 1, protocol)?;
                        }
                    }
                    _ => (),
                }
            }
            if let Some(click) = values.get_mut(&JavaString::from("click_event")) {
                if matches!(click.get("action"), Some(Tag::String(value)) if value.to_string_lossy() == "show_dialog")
                {
                    if let Some(dialog) = click.get_mut("dialog") {
                        crate::vfp_dialog::upgrade_at(dialog, protocol, depth + 1)?;
                    }
                }
            }
        }
        _ => (),
    }
    Ok(())
}
pub(crate) fn hover_item(tag: &mut Tag, depth: usize, protocol: i32) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Hover items nest too deeply"));
    }
    if protocol < 775 {
        if let Tag::Compound(item) = tag {
            let count = match item.get(&JavaString::from("count")) {
                None => Some(1),
                Some(Tag::Byte(value)) => Some(i32::from(*value)),
                Some(Tag::Short(value)) => Some(i32::from(*value)),
                Some(Tag::Int(value)) => Some(*value),
                _ => None,
            };
            if let Some(count) = count {
                item.insert(
                    JavaString::from("count"),
                    Tag::Int(if count == 0 { 1 } else { count }),
                );
            }
        }
    }
    let Some(Tag::Compound(components)) = tag.get_mut("components") else {
        return Ok(());
    };
    if protocol < 770 {
        crate::vfp_text_1214::hover(components, depth + 1, protocol)?;
    }
    for namespace in ["", "minecraft:"] {
        let key = |name: &str| JavaString::from(format!("{namespace}{name}").as_str());
        for name in ["item_name", "custom_name", "lore"] {
            if let Some(child) = components.get_mut(&key(name)) {
                text_at(child, depth + 1, protocol)?;
            }
        }
        if let Some(Tag::Compound(book)) = components.get_mut(&key("written_book_content")) {
            if let Some(Tag::List(pages)) = book.get_mut(&JavaString::from("pages")) {
                for page in pages {
                    if page.get("raw").is_some() {
                        for field in ["raw", "filtered"] {
                            if let Some(child) = page.get_mut(field) {
                                text_at(child, depth + 1, protocol)?;
                            }
                        }
                    } else {
                        text_at(page, depth + 1, protocol)?;
                    }
                }
            }
        }
        for name in ["bundle_contents", "charged_projectiles", "container"] {
            if let Some(Tag::List(items)) = components.get_mut(&key(name)) {
                for item in items {
                    if name == "container" {
                        if let Some(item) = item.get_mut("item") {
                            hover_item(item, depth + 1, protocol)?;
                        }
                    } else {
                        hover_item(item, depth + 1, protocol)?;
                    }
                }
            }
        }
        for name in ["use_remainder", "sulfur_cube_content"] {
            if let Some(item) = components.get_mut(&key(name)) {
                hover_item(item, depth + 1, protocol)?;
            }
        }
        // Same display fallback as ViaVersion: hover items do not need these
        // versioned behaviours, and their old NBT codecs differ from 26.3.
        for name in [
            "provides_trim_material",
            "pot_decorations",
            "map_color",
            "trim",
            "swing_animation",
            "lock",
            "debug_stick_state",
            "equippable",
            "consumable",
            "blocks_attacks",
            "break_sound",
            "note_block_sound",
            "tool",
            "block_transformer",
        ] {
            components.remove(&key(name));
        }
        if protocol < 773 {
            for name in ["entity_data", "block_entity_data", "bees"] {
                components.remove(&key(name));
            }
        }
    }
    Ok(())
}
fn chat_type(v: &mut p::ChatTypeBound, protocol: i32) -> Result<()> {
    text_version(&mut v.name, protocol)?;
    if let Some(t) = &mut v.target {
        text_version(t, protocol)?;
    }
    Ok(())
}
fn number(v: &mut Option<p::NumberFormat>, protocol: i32) -> Result<()> {
    match v {
        Some(p::NumberFormat::Styled(t) | p::NumberFormat::Fixed(t)) => text_version(t, protocol),
        _ => Ok(()),
    }
}
fn typed(id: i32, payload: &[u8], protocol: i32) -> Result<wire::Packet> {
    let mut packet = p::Clientbound::decode(id, payload)?;
    match &mut packet {
        p::Clientbound::Unknown(_, _) => {
            return Err(Error::new(
                "Horizon has no codec for this unchanged play packet",
            ));
        }
        p::Clientbound::Disconnect(v) => text_version(&mut v.reason, protocol)?,
        p::Clientbound::SystemChat(v) => text_version(&mut v.content, protocol)?,
        p::Clientbound::DisguisedChat(v) => {
            text_version(&mut v.message, protocol)?;
            chat_type(&mut v.chat_type, protocol)?;
        }
        p::Clientbound::PlayerChat(v) => {
            if let Some(t) = &mut v.unsigned_content {
                text_version(t, protocol)?;
            }
            chat_type(&mut v.chat_type, protocol)?;
        }
        p::Clientbound::PlayerCombatKill(v) => text_version(&mut v.message, protocol)?,
        p::Clientbound::ServerData(v) => text_version(&mut v.motd, protocol)?,
        p::Clientbound::SetActionBarText(v)
        | p::Clientbound::SetSubtitleText(v)
        | p::Clientbound::SetTitleText(v) => text_version(&mut v.text, protocol)?,
        p::Clientbound::TabList(v) => {
            text_version(&mut v.header, protocol)?;
            text_version(&mut v.footer, protocol)?;
        }
        p::Clientbound::ResourcePackPush(v) => {
            if let Some(t) = &mut v.prompt {
                text_version(t, protocol)?;
            }
        }
        p::Clientbound::OpenScreen(v) => text_version(&mut v.title, protocol)?,
        p::Clientbound::ShowDialog(v) => {
            if let p::DialogHolder::Direct(tag) = &mut v.dialog {
                crate::vfp_dialog::upgrade_at(tag, protocol, 0)?;
            }
        }
        p::Clientbound::PlayerInfoUpdate(v) => {
            for e in &mut v.entries {
                if let Some(Some(t)) = &mut e.display_name {
                    text_version(t, protocol)?;
                }
            }
        }
        p::Clientbound::BossEvent(v) => match &mut v.action {
            p::BossAction::Add { name, .. } | p::BossAction::UpdateName { name } => {
                text_version(name, protocol)?
            }
            _ => (),
        },
        p::Clientbound::SetPlayerTeam(v) => {
            if let Some(t) = &mut v.parameters {
                text_version(&mut t.display_name, protocol)?;
                text_version(&mut t.prefix, protocol)?;
                text_version(&mut t.suffix, protocol)?;
            }
        }
        p::Clientbound::SetObjective(v) => {
            if let Some((t, _, format)) = &mut v.display {
                text_version(t, protocol)?;
                number(format, protocol)?;
            }
        }
        p::Clientbound::SetScore(v) => {
            if let Some(t) = &mut v.display {
                text_version(t, protocol)?;
            }
            number(&mut v.number_format, protocol)?;
        }
        _ => (),
    }
    let bytes = packet.encode();
    let mut r = Reader::new(&bytes);
    let id = r.varint()?;
    Ok((id, r.rest().to_vec()))
}
pub fn clientbound_using<M: Maps>(
    id: i32,
    payload: &[u8],
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<Vec<wire::Packet>> {
    let name = usize::try_from(id)
        .ok()
        .and_then(|i| t::OLD_CLIENTBOUND.get(i))
        .copied()
        .ok_or_else(|| Error::new("Invalid play packet ID"))?;
    let target = wire::packet_id(t::NEW_CLIENTBOUND, name).map_err(Error::new)?;
    if let Some(data) = crate::vfp_auxiliary::clientbound_version(name, payload, M::PROTOCOL)? {
        return Ok(vec![(target, data)]);
    }
    match name {
        "CHUNKS_BIOMES" if M::PROTOCOL < 770 => {
            let data =
                crate::vfp_world_1214::biomes(payload, _registry.entry_count("worldgen/biome"))?;
            p::Clientbound::decode(target, &data)?;
            Ok(vec![(target, data)])
        }
        "UPDATE_ATTRIBUTES" => {
            let p::Clientbound::UpdateAttributes(mut packet) =
                p::Clientbound::decode(target, payload)?
            else {
                return Err(Error::new("Invalid attribute update"));
            };
            for value in &mut packet.values {
                value.attribute = M::ATTRIBUTES.map(value.attribute).map_err(Error::new)?;
            }
            let bytes = p::Clientbound::UpdateAttributes(packet).encode();
            let mut r = Reader::new(&bytes);
            r.varint()?;
            Ok(vec![(target, r.rest().to_vec())])
        }
        "PLAYER_CHAT" => {
            let mut r = Reader::new(payload);
            let mut w = Writer::new();
            let start = r.position();
            r.varint()?;
            r.bytes(16)?;
            r.varint()?;
            if r.bool()? {
                r.bytes(256)?;
            }
            r.string(256)?;
            r.bytes(16)?;
            let n = r.count(20)?;
            for _ in 0..n {
                let index = r.varint()?;
                if index < 0 {
                    return Err(Error::new("Invalid packed chat signature"));
                }
                if index == 0 {
                    r.bytes(256)?;
                }
            }
            if r.bool()? {
                r.nbt()?;
            }
            w.bytes(r.since(start));
            let kind = r.varint()?;
            if !(0..=2).contains(&kind) {
                return Err(Error::new("Invalid old chat filter type"));
            }
            w.varint(kind);
            if kind == 2 {
                let n = r.count(r.remaining() / 8)?;
                let mut bytes = Vec::with_capacity(n * 8);
                for _ in 0..n {
                    bytes.extend_from_slice(&r.i64()?.to_le_bytes());
                }
                while bytes.last() == Some(&0) {
                    bytes.pop();
                }
                w.byte_array(&bytes);
            }
            crate::vfp_chat::bound(&mut r, &mut w, M::PROTOCOL)?;
            Ok(vec![typed(target, &w.data, M::PROTOCOL)?])
        }
        "DISGUISED_CHAT" => {
            let mut r = Reader::new(payload);
            let mut w = Writer::new();
            let message = r.nbt()?;
            w.nbt(&message);
            crate::vfp_chat::bound(&mut r, &mut w, M::PROTOCOL)?;
            Ok(vec![typed(target, &w.data, M::PROTOCOL)?])
        }
        "MERCHANT_OFFERS" => {
            fn cost<M: Maps>(
                r: &mut Reader<'_>,
                w: &mut Writer,
                _registry: &crate::vfp_registry_access::Registries,
            ) -> Result<()> {
                if M::NATIVE_ITEMS {
                    let start = r.position();
                    r.varint()?;
                    if r.varint()? <= 0 {
                        return Err(Error::new("Nonpositive merchant item cost"));
                    }
                    for _ in 0..r.count(65536)? {
                        let kind = r.varint()?;
                        crate::item::Component::read(kind, r)?;
                    }
                    w.bytes(r.since(start));
                    return Ok(());
                }
                w.varint(M::ITEMS.map(r.varint()?).map_err(Error::new)?);
                let count = r.varint()?;
                if count <= 0 {
                    return Err(Error::new("Nonpositive merchant item cost"));
                }
                w.varint(count);
                crate::vfp_items::exact_using::<M>(r, w, true, 0, _registry)
            }
            let mut r = Reader::new(payload);
            let mut w = Writer::new();
            w.varint(r.varint()?);
            let n = r.count(r.remaining())?;
            w.varint(n as i32);
            for _ in 0..n {
                cost::<M>(&mut r, &mut w, _registry)?;
                crate::vfp_items::stack_using::<M>(&mut r, &mut w, true, false, _registry)?;
                let second = r.bool()?;
                w.bool(second);
                if second {
                    cost::<M>(&mut r, &mut w, _registry)?;
                }
                w.bool(r.bool()?);
                w.bytes(r.bytes(24)?);
            }
            w.varint(r.varint()?);
            w.varint(r.varint()?);
            w.bool(r.bool()?);
            w.bool(r.bool()?);
            if r.remaining() != 0 {
                return Err(Error::new("Trailing merchant offer bytes"));
            }
            p::Clientbound::decode(target, &w.data)?;
            Ok(vec![(target, w.data)])
        }
        "EXPLODE" => {
            let mut r = Reader::new(payload);
            let mut w = Writer::new();
            w.bytes(r.bytes(32)?); // Position, radius and destroyed-block count.
            let knockback = r.bool()?;
            w.bool(knockback);
            if knockback {
                w.bytes(r.bytes(24)?);
            }
            crate::entity_data::vfp_entities::particle_using::<M>(&mut r, &mut w, _registry)?;
            crate::vfp_values::sound_using::<M>(&mut r, &mut w, true, _registry)?;
            let n = r.count(r.remaining())?;
            w.varint(n as i32);
            for _ in 0..n {
                crate::entity_data::vfp_entities::particle_using::<M>(&mut r, &mut w, _registry)?;
                w.bytes(r.bytes(8)?);
                w.varint(r.varint()?);
            }
            w.bool(true);
            if r.remaining() != 0 {
                return Err(Error::new("Trailing explosion bytes"));
            }
            p::Clientbound::decode(target, &w.data)?;
            Ok(vec![(target, w.data)])
        }
        "UPDATE_TAGS" => {
            let config_id =
                wire::packet_id(t::OLD_CONFIGURATION, "UPDATE_TAGS").map_err(Error::new)?;
            let (_, data) = if matches!(M::PROTOCOL, 761..=772) {
                (
                    0,
                    crate::vfp_configuration_1217::play_tags(M::PROTOCOL, payload)?,
                )
            } else if M::PROTOCOL == 773 {
                (0, crate::vfp_configuration_1219::play_tags(payload)?)
            } else if M::PROTOCOL == 774 {
                (0, crate::vfp_configuration_12111::play_tags(payload)?)
            } else if M::PROTOCOL == 775 {
                crate::vfp_configuration_261::clientbound(config_id, payload)?
            } else {
                crate::vfp_configuration::clientbound_262(config_id, payload)?
            };
            Ok(vec![(target, data)])
        }
        "OPEN_SCREEN" => {
            let packet = typed(target, payload, M::PROTOCOL)?;
            let mut r = Reader::new(&packet.1);
            let container = r.varint()?;
            let menu = r.varint()?;
            let mut result = vec![packet];
            if menu == 11 {
                for (slot, value) in [(2, 400), (3, 20)] {
                    let mut w = Writer::new();
                    w.varint(container);
                    w.i16(slot);
                    w.i16(value);
                    result.push((
                        wire::packet_id(t::NEW_CLIENTBOUND, "CONTAINER_SET_DATA")
                            .map_err(Error::new)?,
                        w.data,
                    ));
                }
            }
            Ok(result)
        }
        // These layouts have no static registry references or structural
        // changes in this version pair. The pinned native decoder validates
        // complete consumption before any packet is returned to the game.
        "BOSS_EVENT"
        | "CHANGE_DIFFICULTY"
        | "CHUNKS_BIOMES"
        | "COMMAND_SUGGESTIONS"
        | "COOKIE_REQUEST"
        | "COOLDOWN"
        | "CUSTOM_CHAT_COMPLETIONS"
        | "DAMAGE_EVENT"
        | "DELETE_CHAT"
        | "DISCONNECT"
        | "ENTITY_EVENT"
        | "GAME_RULE_VALUES"
        | "MOUNT_SCREEN_OPEN"
        | "MAP_ITEM_DATA"
        | "MOVE_MINECART_ALONG_TRACK"
        | "MOVE_VEHICLE"
        | "OPEN_BOOK"
        | "PONG_RESPONSE"
        | "PLAYER_ABILITIES"
        | "PLAYER_COMBAT_KILL"
        | "PLAYER_INFO_REMOVE"
        | "PLAYER_INFO_UPDATE"
        | "PLAYER_LOOK_AT"
        | "PLAYER_POSITION"
        | "RECIPE_BOOK_REMOVE"
        | "RECIPE_BOOK_SETTINGS"
        | "RESET_SCORE"
        | "RESOURCE_PACK_POP"
        | "RESOURCE_PACK_PUSH"
        | "SELECT_ADVANCEMENTS_TAB"
        | "SERVER_DATA"
        | "SET_ACTION_BAR_TEXT"
        | "SET_DISPLAY_OBJECTIVE"
        | "SET_ENTITY_LINK"
        | "SET_ENTITY_MOTION"
        | "SET_HELD_SLOT"
        | "SET_OBJECTIVE"
        | "SET_PASSENGERS"
        | "SET_PLAYER_TEAM"
        | "SET_SCORE"
        | "SET_SIMULATION_DISTANCE"
        | "SET_SUBTITLE_TEXT"
        | "SET_TIME"
        | "SET_TITLE_TEXT"
        | "SET_TITLES_ANIMATION"
        | "START_CONFIGURATION"
        | "STORE_COOKIE"
        | "SYSTEM_CHAT"
        | "TAB_LIST"
        | "TAG_QUERY"
        | "TAKE_ITEM_ENTITY"
        | "TELEPORT_ENTITY"
        | "TRANSFER"
        | "UPDATE_MOB_EFFECT"
        | "PROJECTILE_POWER"
        | "CUSTOM_REPORT_DETAILS"
        | "SERVER_LINKS"
        | "TRACKED_WAYPOINT"
        | "CLEAR_DIALOG"
        | "SHOW_DIALOG" => Ok(vec![typed(target, payload, M::PROTOCOL)?]),
        _ => wire::clientbound_with::<M>(id, payload).map_err(Error::new),
    }
}

pub fn clientbound_262(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    clientbound_using::<V262>(
        id,
        payload,
        &crate::vfp_registry_access::Registries::default(),
    )
}

pub fn clientbound_with<M: Maps>(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    clientbound_using::<M>(
        id,
        payload,
        &crate::vfp_registry_access::Registries::default(),
    )
}
