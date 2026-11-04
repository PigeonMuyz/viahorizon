// SPDX-License-Identifier: GPL-3.0-or-later
// The JSON-text boundary and single resource-pack protocol of 1.20.2.
// NBT items, metadata and recipes keep their original source schema until the
// following source-aware adapters consume them.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_tables_1202 as old, vfp_tables_1203 as next,
    vfp_wire::{self as wire, Packet},
};
fn end(r: &Reader<'_>) -> Result<()> {
    if r.remaining() == 0 {
        Ok(())
    } else {
        Err(Error::new("Trailing 1.20.2 packet bytes"))
    }
}
fn string(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.string(&r.string(32767)?);
    Ok(())
}
fn text(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.nbt(&crate::vfp_text_1214::json_text(&r.string(262144)?, 0)?);
    Ok(())
}
fn optional_text(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if present {
        text(r, w)?;
    }
    Ok(())
}
fn optional_string(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if present {
        string(r, w)?;
    }
    Ok(())
}
fn old_item<M: crate::vfp_mapping::Maps>(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    let start = r.position();
    if r.bool()? {
        r.varint()?;
        r.i8()?;
        if M::PROTOCOL < 764 {
            r.vfp_named_nbt()?;
        } else {
            r.nbt()?;
        }
    }
    w.bytes(r.since(start));
    Ok(())
}
fn resource_pack(data: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    let url = r.string(32767)?;
    let hash = r.string(32767)?;
    let mut uuid = crate::crypto::md5(url.as_bytes());
    uuid[6] = (uuid[6] & 15) | 0x30;
    uuid[8] = (uuid[8] & 63) | 0x80;
    w.bytes(&uuid);
    w.string(&url);
    w.string(&hash);
    w.bool(r.bool()?);
    optional_text(&mut r, &mut w)?;
    end(&r)?;
    Ok(w.data)
}
fn packet(table: &[&str], name: &str, data: Vec<u8>) -> Result<Packet> {
    Ok((wire::packet_id(table, name).map_err(Error::new)?, data))
}
fn pack_status(data: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(data);
    r.uuid()?;
    let action = r.varint()?;
    end(&r)?;
    if action == 4 {
        return Ok(None);
    }
    if action < 0 {
        return Err(Error::new("Negative resource-pack status"));
    }
    let mut w = Writer::new();
    w.varint(if action > 4 { 2 } else { action });
    Ok(Some(w.data))
}
pub fn configuration(id: i32, data: &[u8]) -> Result<Vec<Packet>> {
    let name = usize::try_from(id)
        .ok()
        .and_then(|id| old::CONFIGURATION_CLIENTBOUND.get(id))
        .copied()
        .ok_or_else(|| Error::new("Unknown 1.20.2 configuration packet"))?;
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    match name {
        "DISCONNECT" => {
            text(&mut r, &mut w)?;
            end(&r)?;
        }
        "RESOURCE_PACK" => {
            return Ok(vec![
                packet(
                    next::CONFIGURATION_CLIENTBOUND,
                    "RESOURCE_PACK_POP",
                    vec![0],
                )?,
                packet(
                    next::CONFIGURATION_CLIENTBOUND,
                    "RESOURCE_PACK_PUSH",
                    resource_pack(data)?,
                )?,
            ]);
        }
        _ => w.bytes(data),
    }
    Ok(vec![packet(next::CONFIGURATION_CLIENTBOUND, name, w.data)?])
}
pub fn configuration_reply(body: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut r = Reader::new(body);
    let id = r.varint()?;
    let name = usize::try_from(id)
        .ok()
        .and_then(|id| next::CONFIGURATION_SERVERBOUND.get(id))
        .copied()
        .ok_or_else(|| Error::new("Unknown 1.20.3 configuration reply"))?;
    let payload = if name == "RESOURCE_PACK" {
        let Some(data) = pack_status(r.rest())? else {
            return Ok(Vec::new());
        };
        data
    } else {
        r.rest().to_vec()
    };
    let mut w = Writer::new();
    w.varint(wire::packet_id(old::CONFIGURATION_SERVERBOUND, name).map_err(Error::new)?);
    w.bytes(&payload);
    Ok(vec![w.data])
}
pub fn clientbound<M: crate::vfp_mapping::Maps>(id: i32, data: &[u8]) -> Result<Vec<Packet>> {
    let name = usize::try_from(id)
        .ok()
        .and_then(|id| old::CLIENTBOUND.get(id))
        .copied()
        .ok_or_else(|| Error::new("Unknown 1.20.2 play packet"))?;
    let mut target = name;
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    match name {
        "DISCONNECT" | "SET_ACTION_BAR_TEXT" | "SET_TITLE_TEXT" | "SET_SUBTITLE_TEXT" => {
            text(&mut r, &mut w)?;
            end(&r)?;
        }
        "SYSTEM_CHAT" | "SERVER_DATA" => {
            text(&mut r, &mut w)?;
            w.bytes(r.rest());
        }
        "TAB_LIST" => {
            text(&mut r, &mut w)?;
            text(&mut r, &mut w)?;
            end(&r)?;
        }
        "PLAYER_COMBAT_KILL" => {
            w.varint(r.varint()?);
            text(&mut r, &mut w)?;
            end(&r)?;
        }
        "DISGUISED_CHAT" => {
            text(&mut r, &mut w)?;
            w.varint(r.varint()?);
            text(&mut r, &mut w)?;
            optional_text(&mut r, &mut w)?;
            end(&r)?;
        }
        "BOSS_EVENT" => {
            w.uuid(r.uuid()?);
            let action = r.varint()?;
            w.varint(action);
            if action == 0 || action == 3 {
                text(&mut r, &mut w)?;
            }
            w.bytes(r.rest());
        }
        "OPEN_SCREEN" => {
            w.varint(r.varint()?);
            w.varint(old::STEP_MENUS.map(r.varint()?).map_err(Error::new)?);
            text(&mut r, &mut w)?;
            end(&r)?;
        }
        "SET_SCORE" => {
            string(&mut r, &mut w)?;
            let action = r.varint()?;
            let objective = r.string(32767)?;
            if action == 1 {
                target = "RESET_SCORE";
                w.bool(!objective.is_empty());
                if !objective.is_empty() {
                    w.string(&objective);
                }
            } else {
                w.string(&objective);
                w.varint(r.varint()?);
                w.bool(false);
                w.bool(false);
            }
            end(&r)?;
        }
        "SET_OBJECTIVE" => {
            string(&mut r, &mut w)?;
            let action = r.u8()?;
            w.u8(action);
            if action == 0 || action == 2 {
                text(&mut r, &mut w)?;
                w.varint(r.varint()?);
                w.bool(false);
            }
            end(&r)?;
        }
        "COMMAND_SUGGESTIONS" => {
            for _ in 0..3 {
                w.varint(r.varint()?);
            }
            let count = r.count(65536)?;
            w.varint(count as i32);
            for _ in 0..count {
                string(&mut r, &mut w)?;
                optional_text(&mut r, &mut w)?;
            }
            end(&r)?;
        }
        "MAP_ITEM_DATA" => {
            w.varint(r.varint()?);
            w.u8(r.u8()?);
            w.bool(r.bool()?);
            let icons = r.bool()?;
            w.bool(icons);
            if icons {
                let count = r.count(65536)?;
                w.varint(count as i32);
                for _ in 0..count {
                    w.varint(r.varint()?);
                    w.bytes(r.bytes(3)?);
                    optional_text(&mut r, &mut w)?;
                }
            }
            w.bytes(r.rest());
        }
        "SET_PLAYER_TEAM" => {
            string(&mut r, &mut w)?;
            let action = r.u8()?;
            w.u8(action);
            if action == 0 || action == 2 {
                text(&mut r, &mut w)?;
                w.u8(r.u8()?);
                string(&mut r, &mut w)?;
                string(&mut r, &mut w)?;
                w.varint(r.varint()?);
                text(&mut r, &mut w)?;
                text(&mut r, &mut w)?;
            }
            w.bytes(r.rest());
        }
        "PLAYER_INFO_UPDATE" => {
            let actions = r.u8()?;
            if actions & !63 != 0 {
                return Err(Error::new("Invalid old player-info actions"));
            }
            w.u8(actions);
            let count = r.count(65536)?;
            w.varint(count as i32);
            for _ in 0..count {
                w.uuid(r.uuid()?);
                if actions & 1 != 0 {
                    string(&mut r, &mut w)?;
                    let properties = r.count(1024)?;
                    w.varint(properties as i32);
                    for _ in 0..properties {
                        string(&mut r, &mut w)?;
                        string(&mut r, &mut w)?;
                        optional_string(&mut r, &mut w)?;
                    }
                }
                if actions & 2 != 0 {
                    let session = r.bool()?;
                    w.bool(session);
                    if session {
                        w.uuid(r.uuid()?);
                        w.i64(r.i64()?);
                        for _ in 0..2 {
                            w.byte_array(&r.byte_array(65536)?);
                        }
                    }
                }
                if actions & 4 != 0 {
                    w.varint(r.varint()?);
                }
                if actions & 8 != 0 {
                    w.bool(r.bool()?);
                }
                if actions & 16 != 0 {
                    w.varint(r.varint()?);
                }
                if actions & 32 != 0 {
                    optional_text(&mut r, &mut w)?;
                }
            }
            end(&r)?;
        }
        "PLAYER_CHAT" => {
            w.uuid(r.uuid()?);
            w.varint(r.varint()?);
            let signed = r.bool()?;
            w.bool(signed);
            if signed {
                w.bytes(r.bytes(256)?);
            }
            string(&mut r, &mut w)?;
            w.bytes(r.bytes(16)?);
            let count = r.count(20)?;
            w.varint(count as i32);
            for _ in 0..count {
                let index = r.varint()?;
                w.varint(index);
                if index == 0 {
                    w.bytes(r.bytes(256)?);
                }
            }
            optional_text(&mut r, &mut w)?;
            let filter = r.varint()?;
            w.varint(filter);
            if filter == 2 {
                let longs = r.count(4096)?;
                w.varint(longs as i32);
                w.bytes(r.bytes(longs * 8)?);
            }
            w.varint(r.varint()?);
            text(&mut r, &mut w)?;
            optional_text(&mut r, &mut w)?;
            end(&r)?;
        }
        "UPDATE_ADVANCEMENTS" => {
            w.bool(r.bool()?);
            let count = r.count(65536)?;
            w.varint(count as i32);
            for _ in 0..count {
                string(&mut r, &mut w)?;
                optional_string(&mut r, &mut w)?;
                let display = r.bool()?;
                w.bool(display);
                if display {
                    text(&mut r, &mut w)?;
                    text(&mut r, &mut w)?;
                    old_item::<M>(&mut r, &mut w)?;
                    w.varint(r.varint()?);
                    let flags = r.i32()?;
                    w.i32(flags);
                    if flags & 1 != 0 {
                        string(&mut r, &mut w)?;
                    }
                    w.bytes(r.bytes(8)?);
                }
                let rows = r.count(65536)?;
                w.varint(rows as i32);
                for _ in 0..rows {
                    let count = r.count(65536)?;
                    w.varint(count as i32);
                    for _ in 0..count {
                        string(&mut r, &mut w)?;
                    }
                }
                w.bool(r.bool()?);
            }
            w.bytes(r.rest());
        }
        "EXPLODE" => {
            w.bytes(r.bytes(28)?);
            let count = r.count(1_048_576)?;
            w.varint(count as i32);
            w.bytes(r.bytes(count * 3 + 12)?);
            end(&r)?;
            w.varint(1);
            // Original source ids, consumed and mapped once by the later chain.
            for name in ["explosion", "explosion_emitter"] {
                let native = crate::vfp_tables_261_native::PARTICLE_NAMES
                    .iter()
                    .position(|value| *value == name)
                    .ok_or_else(|| Error::new("Missing native explosion particle"))?;
                w.varint(old::PARTICLES.inverse(native as i32).map_err(Error::new)?);
            }
            w.string("minecraft:entity.generic.explode");
            w.bool(false);
        }
        "RESOURCE_PACK" => {
            return Ok(vec![
                packet(next::CLIENTBOUND, "RESOURCE_PACK_POP", vec![0])?,
                packet(
                    next::CLIENTBOUND,
                    "RESOURCE_PACK_PUSH",
                    resource_pack(data)?,
                )?,
            ]);
        }
        _ => w.bytes(data),
    }
    let mut result = vec![packet(next::CLIENTBOUND, target, w.data)?];
    if matches!(name, "LOGIN" | "RESPAWN" | "INITIALIZE_BORDER") {
        let mut event = Writer::new();
        event.u8(13);
        event.f32(0.0);
        result.push(packet(next::CLIENTBOUND, "GAME_EVENT", event.data)?);
    }
    Ok(result)
}
pub fn serverbound(packets: Vec<Packet>) -> Result<Vec<Packet>> {
    let mut result = Vec::new();
    for (id, data) in packets {
        let name = usize::try_from(id)
            .ok()
            .and_then(|id| next::SERVERBOUND.get(id))
            .copied()
            .ok_or_else(|| Error::new("Unknown 1.20.3 reply"))?;
        if name == "CONTAINER_SLOT_STATE_CHANGED" {
            continue;
        }
        let data = match name {
            "RESOURCE_PACK" => {
                let Some(data) = pack_status(&data)? else {
                    continue;
                };
                data
            }
            "SET_JIGSAW_BLOCK" => {
                let mut r = Reader::new(&data);
                r.bytes(8)?;
                for _ in 0..5 {
                    r.string(32767)?;
                }
                let end = r.position();
                r.varint()?;
                r.varint()?;
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing jigsaw priorities"));
                }
                data[..end].to_vec()
            }
            _ => data,
        };
        result.push(packet(old::SERVERBOUND, name, data)?);
    }
    Ok(result)
}
