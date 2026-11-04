// SPDX-License-Identifier: GPL-3.0-or-later
// 1.19.3 -> 1.19.4 packet structure, without remapping static registry ids.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_tables_1193 as old, vfp_tables_1194 as next,
    vfp_wire::{self as wire, Packet},
};
use horizon_world::nbt::{JavaString, Tag};
use std::collections::BTreeSet;
fn packet(name: &str, data: Vec<u8>) -> Result<Packet> {
    Ok((
        wire::packet_id(next::CLIENTBOUND, name).map_err(Error::new)?,
        data,
    ))
}
fn end(r: &Reader<'_>) -> Result<()> {
    if r.remaining() == 0 {
        Ok(())
    } else {
        Err(Error::new("Trailing 1.19.3 packet bytes"))
    }
}
pub fn registries(tag: &mut Tag) -> Result<()> {
    let Tag::Compound(registries) = tag else {
        return Err(Error::new("Old registries are not a compound"));
    };
    let data = crate::vfp_registry_1193::DAMAGE_TYPES;
    let (root, used) = horizon_world::nbt::read_named(
        data,
        horizon_world::nbt::Limits {
            max_bytes: data.len(),
            max_nodes: 100000,
            max_depth: 64,
        },
    )
    .map_err(|error| Error::new(format!("Bundled damage types: {error}")))?;
    if used != data.len() {
        return Err(Error::new("Trailing bundled damage types"));
    }
    registries.insert("minecraft:damage_type".into(), root.tag);
    for kind in ["trim_material", "trim_pattern"] {
        registries
            .entry(format!("minecraft:{kind}").as_str().into())
            .or_insert_with(|| {
                Tag::compound([
                    (
                        "type",
                        Tag::String(format!("minecraft:{kind}").as_str().into()),
                    ),
                    ("value", Tag::List(Vec::new())),
                ])
            });
    }
    if let Some(Tag::List(biomes)) = registries
        .get_mut(&JavaString::from("minecraft:worldgen/biome"))
        .and_then(|registry| registry.get_mut("value"))
    {
        for biome in biomes {
            if let Some(Tag::Compound(element)) = biome.get_mut("element") {
                let rain = !matches!(element.get(&JavaString::from("precipitation")), Some(Tag::String(name)) if name.to_string_lossy() == "none");
                element.insert("has_precipitation".into(), Tag::Byte(i8::from(rain)));
            }
        }
    }
    Ok(())
}
fn base64(text: &str) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut bits = 0u32;
    let mut count = 0u32;
    let mut padding = 0usize;
    let mut symbols = 0usize;
    for byte in text.bytes() {
        if byte == b'=' {
            padding += 1;
            continue;
        }
        if padding != 0 {
            return Err(Error::new("Invalid server-icon base64 padding"));
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(Error::new("Invalid server-icon base64")),
        };
        symbols += 1;
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            output.push((bits >> count) as u8);
        }
    }
    if symbols % 4 == 1 || padding > 2 || (padding != 0 && (symbols + padding) % 4 != 0) {
        return Err(Error::new("Invalid server-icon base64 length"));
    }
    Ok(output)
}
#[derive(Default)]
pub struct Transition {
    player: i32,
    vehicle: Option<i32>,
    fake_teleports: BTreeSet<i32>,
    next_fake: i32,
}
impl Transition {
    pub fn clientbound(&mut self, id: i32, data: &[u8]) -> Result<Vec<Packet>> {
        let name = usize::try_from(id)
            .ok()
            .and_then(|id| old::CLIENTBOUND.get(id))
            .copied()
            .ok_or_else(|| Error::new("Unknown 1.19.3 packet"))?;
        let mut target = name;
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        match name {
            "LOGIN" => {
                self.player = r.i32()?;
                self.vehicle = None;
                self.fake_teleports.clear();
                w.bytes(data);
            }
            "RESPAWN" => {
                self.vehicle = None;
                self.fake_teleports.clear();
                w.bytes(data);
            }
            "SERVER_DATA" => {
                w.string(&if r.bool()? {
                    r.string(262144)?
                } else {
                    "\"\"".into()
                });
                let icon = if r.bool()? {
                    let value = r.string(1048576)?;
                    value
                        .strip_prefix("data:image/png;base64,")
                        .map(base64)
                        .transpose()?
                } else {
                    None
                };
                w.bool(icon.is_some());
                if let Some(icon) = icon {
                    w.byte_array(&icon);
                }
                w.bytes(r.rest());
            }
            "OPEN_SCREEN" => {
                w.varint(r.varint()?);
                let menu = r.varint()?;
                w.varint(if menu >= 21 { menu + 1 } else { menu });
                w.bytes(r.rest());
            }
            "LEVEL_EVENT" => {
                let mut event = r.i32()?;
                let position = r.bytes(8)?.to_vec();
                let mut value = r.i32()?;
                if event == 1010
                    && !usize::try_from(value)
                        .ok()
                        .and_then(|id| old::ITEM_IDENTIFIERS.get(id))
                        .is_some_and(|name| name.starts_with("music_disc_"))
                {
                    event = 1011;
                    value = 0;
                }
                w.i32(event);
                w.bytes(&position);
                w.i32(value);
                w.bytes(r.rest());
            }
            "UPDATE_MOB_EFFECT" => {
                w.varint(r.varint()?);
                w.varint(r.varint()?);
                w.i8(r.i8()?);
                let duration = r.varint()?;
                w.varint(if duration >= 32767 { -1 } else { duration });
                w.bytes(r.rest());
            }
            "ANIMATE" => {
                w.varint(r.varint()?);
                let action = r.u8()?;
                end(&r)?;
                if action == 1 {
                    target = "HURT_ANIMATION";
                    w.f32(0.0);
                } else {
                    w.u8(action);
                }
            }
            "ENTITY_EVENT" => {
                let entity = r.i32()?;
                let event = r.i8()?;
                end(&r)?;
                let damage = match event {
                    33 => Some(36),
                    36 => Some(5),
                    37 => Some(27),
                    57 => Some(15),
                    44 | 2 => Some(16),
                    _ => None,
                };
                if let Some(damage) = damage {
                    target = "DAMAGE_EVENT";
                    w.varint(entity);
                    w.varint(damage);
                    w.varint(0);
                    w.varint(0);
                    w.bool(false);
                } else {
                    w.i32(entity);
                    w.i8(event);
                }
            }
            "SET_PASSENGERS" => {
                let vehicle = r.varint()?;
                let count = r.count(65536)?;
                let mut contains = false;
                for _ in 0..count {
                    contains |= r.varint()? == self.player;
                }
                end(&r)?;
                if self.vehicle == Some(vehicle) {
                    self.vehicle = None;
                }
                if contains {
                    self.vehicle = Some(vehicle);
                }
                w.bytes(data);
            }
            "REMOVE_ENTITIES" => {
                for _ in 0..r.count(65536)? {
                    if self.vehicle == Some(r.varint()?) {
                        self.vehicle = None;
                    }
                }
                end(&r)?;
                w.bytes(data);
            }
            "PLAYER_POSITION" => {
                r.bytes(33)?;
                r.varint()?;
                let end_pos = r.position();
                let dismount = r.bool()?;
                end(&r)?;
                w.bytes(&data[..end_pos]);
                if dismount {
                    if let Some(vehicle) = self.vehicle.take() {
                        let mut passengers = Writer::new();
                        passengers.varint(vehicle);
                        passengers.varint(0);
                        return Ok(vec![
                            packet("BUNDLE_DELIMITER", Vec::new())?,
                            packet("SET_PASSENGERS", passengers.data)?,
                            packet(target, w.data)?,
                            packet("BUNDLE_DELIMITER", Vec::new())?,
                        ]);
                    }
                }
            }
            "TELEPORT_ENTITY" => {
                let entity = r.varint()?;
                if entity != self.player {
                    w.bytes(data);
                } else {
                    target = "PLAYER_POSITION";
                    w.bytes(r.bytes(24)?);
                    w.f32(f32::from(r.i8()?) * 360.0 / 256.0);
                    w.f32(f32::from(r.i8()?) * 360.0 / 256.0);
                    r.bool()?;
                    end(&r)?;
                    w.u8(0);
                    self.next_fake = self
                        .next_fake
                        .checked_sub(1)
                        .filter(|id| *id > i32::MIN + 65536)
                        .ok_or_else(|| Error::new("Too many old self-teleports"))?;
                    self.fake_teleports.insert(self.next_fake);
                    w.varint(self.next_fake);
                }
            }
            _ => w.bytes(data),
        }
        Ok(vec![packet(target, w.data)?])
    }
    pub fn serverbound(&mut self, packets: Vec<Packet>) -> Result<Vec<Packet>> {
        let mut result = Vec::new();
        for (id, data) in packets {
            let name = usize::try_from(id)
                .ok()
                .and_then(|id| next::SERVERBOUND.get(id))
                .copied()
                .ok_or_else(|| Error::new("Unknown 1.19.4 reply"))?;
            if name == "ACCEPT_TELEPORTATION" {
                let mut r = Reader::new(&data);
                let teleport = r.varint()?;
                end(&r)?;
                if self.fake_teleports.remove(&teleport) {
                    continue;
                }
            }
            result.push((
                wire::packet_id(old::SERVERBOUND, name).map_err(Error::new)?,
                data,
            ));
        }
        Ok(result)
    }
}
