// SPDX-License-Identifier: GPL-3.0-or-later
// 1.20/1.20.1 has no configuration state. The bridge holds play packets until
// the native client acknowledges registries extracted from join-game.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_mapping::Maps,
    vfp_tables_120 as old, vfp_tables_1202 as next,
    vfp_wire::{self as wire, Packet},
};
use horizon_world::nbt::Tag;
use std::collections::VecDeque;
fn end(r: &Reader<'_>) -> Result<()> {
    if r.remaining() == 0 {
        Ok(())
    } else {
        Err(Error::new("Trailing 1.20 packet bytes"))
    }
}
fn string(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.string(&r.string(32767)?);
    Ok(())
}
fn packet(table: &[&str], name: &str, data: Vec<u8>) -> Result<Packet> {
    Ok((wire::packet_id(table, name).map_err(Error::new)?, data))
}
fn name(id: i32) -> Result<&'static str> {
    usize::try_from(id)
        .ok()
        .and_then(|id| old::CLIENTBOUND.get(id))
        .copied()
        .ok_or_else(|| Error::new("Unknown pre-configuration play packet"))
}
fn sanitize(data: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let channel = r.string(32767)?;
    match channel.as_str() {
        "minecraft:debug/game_test_add_marker" => {
            r.bytes(8 + 4)?;
            r.string(32767)?;
            r.i32()?;
            Ok(data[..r.position()].to_vec())
        }
        "minecraft:debug/game_test_clear" => Ok(data[..r.position()].to_vec()),
        _ => Ok(data.to_vec()),
    }
}
pub fn join<M: Maps>(data: &[u8]) -> Result<(Tag, Packet)> {
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    w.i32(r.i32()?);
    w.bool(r.bool()?);
    let mode = r.u8()?;
    let previous = r.i8()?;
    let worlds = r.count(65536)?;
    w.varint(worlds as i32);
    for _ in 0..worlds {
        string(&mut r, &mut w)?;
    }
    let mut registry = r.vfp_named_nbt()?;
    if M::PROTOCOL < 762 {
        crate::vfp_transition_1193::registries(&mut registry)?;
    }
    let dimension = r.string(32767)?;
    let world = r.string(32767)?;
    let seed = r.i64()?;
    for _ in 0..3 {
        w.varint(r.varint()?);
    }
    w.bool(r.bool()?);
    w.bool(r.bool()?);
    w.bool(false);
    w.string(&dimension);
    w.string(&world);
    w.i64(seed);
    w.u8(mode);
    w.i8(previous);
    w.bytes(r.rest());
    if M::PROTOCOL < 763 {
        w.varint(0);
    }
    Ok((registry, packet(next::CLIENTBOUND, "LOGIN", w.data)?))
}
pub enum Held {
    Ready(Packet),
    Original(Packet),
}
#[derive(Default)]
pub struct Bridge {
    registry: Option<Tag>,
    join: Option<Packet>,
    held: VecDeque<Packet>,
    held_bytes: usize,
    configuration: Option<Vec<Packet>>,
    information: Option<Vec<u8>>,
    outgoing: Vec<Packet>,
}
impl Bridge {
    pub fn hold(&mut self, id: i32, data: &[u8]) -> Result<()> {
        let size = self
            .held_bytes
            .checked_add(data.len())
            .ok_or_else(|| Error::new("Configuration queue overflows"))?;
        if size > 64 * 1024 * 1024 || self.held.len() >= 4096 {
            return Err(Error::new("Server sent too much data during configuration"));
        }
        self.held.push_back((id, data.to_vec()));
        self.held_bytes = size;
        Ok(())
    }
    pub fn take_early(&mut self) -> Vec<Packet> {
        self.held_bytes = 0;
        self.held.drain(..).collect()
    }
    fn registry_packets(registry: &Tag) -> Result<Vec<Packet>> {
        let mut w = Writer::new();
        w.nbt(registry);
        Ok(vec![
            packet(next::CONFIGURATION_CLIENTBOUND, "REGISTRY_DATA", w.data)?,
            packet(next::CONFIGURATION_CLIENTBOUND, "UPDATE_TAGS", vec![0])?,
            packet(
                next::CONFIGURATION_CLIENTBOUND,
                "FINISH_CONFIGURATION",
                Vec::new(),
            )?,
        ])
    }
    pub fn configuration<M: Maps>(&mut self, id: i32, data: &[u8]) -> Result<Vec<Packet>> {
        let name = name(id)?;
        if self.join.is_some() {
            self.hold(id, data)?;
            return Ok(Vec::new());
        }
        if name == "LOGIN" {
            let (registry, join) = join::<M>(data)?;
            let packets = Self::registry_packets(&registry)?;
            self.registry = Some(registry);
            self.join = Some(join);
            return Ok(packets);
        }
        if matches!(
            name,
            "CUSTOM_PAYLOAD"
                | "DISCONNECT"
                | "KEEP_ALIVE"
                | "PING"
                | "UPDATE_ENABLED_FEATURES"
                | "UPDATE_TAGS"
        ) {
            let data = if name == "CUSTOM_PAYLOAD" {
                sanitize(data)?
            } else {
                data.to_vec()
            };
            return Ok(vec![packet(next::CONFIGURATION_CLIENTBOUND, name, data)?]);
        }
        self.hold(id, data)?;
        Ok(Vec::new())
    }
    pub fn waiting_reconfiguration(&self) -> bool {
        self.configuration.is_some()
    }
    pub fn take_configuration(&mut self) -> Result<Vec<Packet>> {
        self.configuration
            .take()
            .ok_or_else(|| Error::new("No pending registry change"))
    }
    pub fn play<M: Maps>(&mut self, id: i32, data: &[u8]) -> Result<Option<Vec<Packet>>> {
        if self.waiting_reconfiguration() {
            self.hold(id, data)?;
            return Ok(Some(Vec::new()));
        }
        if name(id)? != "LOGIN" {
            return Ok(Some(clientbound::<M>(id, data)?));
        }
        let (registry, join) = join::<M>(data)?;
        if self.registry.as_ref() == Some(&registry) {
            self.resend_information()?;
            return Ok(Some(vec![join]));
        }
        self.configuration = Some(Self::registry_packets(&registry)?);
        self.registry = Some(registry);
        self.join = Some(join);
        Ok(None) // Session sends START_CONFIGURATION, then waits for its ack.
    }
    fn resend_information(&mut self) -> Result<()> {
        if let Some(data) = &self.information {
            self.outgoing.push(packet(
                old::SERVERBOUND,
                "CLIENT_INFORMATION",
                data.clone(),
            )?);
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(Vec<Held>, Vec<Packet>)> {
        let join = self
            .join
            .take()
            .ok_or_else(|| Error::new("Configuration ended before join-game"))?;
        let mut packets = vec![Held::Ready(join)];
        packets.extend(self.held.drain(..).map(Held::Original));
        self.held_bytes = 0;
        self.resend_information()?;
        Ok((packets, std::mem::take(&mut self.outgoing)))
    }
    pub fn configuration_reply(&mut self, body: &[u8]) -> Result<Vec<Packet>> {
        let mut r = Reader::new(body);
        let id = r.varint()?;
        let name = usize::try_from(id)
            .ok()
            .and_then(|id| next::CONFIGURATION_SERVERBOUND.get(id))
            .copied()
            .ok_or_else(|| Error::new("Unknown bridged configuration reply"))?;
        match name {
            "CLIENT_INFORMATION" => {
                self.information = Some(r.rest().to_vec());
                Ok(Vec::new())
            }
            "RESOURCE_PACK" => Ok(Vec::new()),
            "CUSTOM_PAYLOAD" | "KEEP_ALIVE" | "PONG" => {
                let data = if name == "CUSTOM_PAYLOAD" {
                    sanitize(r.rest())?
                } else {
                    r.rest().to_vec()
                };
                let packet = packet(old::SERVERBOUND, name, data)?;
                if self.join.is_some() {
                    Ok(vec![packet])
                } else {
                    self.outgoing.push(packet);
                    Ok(Vec::new())
                }
            }
            _ => Err(Error::new("Unexpected bridged configuration reply")),
        }
    }
    pub fn serverbound(&mut self, protocol: i32, packets: Vec<Packet>) -> Result<Vec<Packet>> {
        let mut result = std::mem::take(&mut self.outgoing);
        for (id, data) in packets {
            let name = usize::try_from(id)
                .ok()
                .and_then(|id| next::SERVERBOUND.get(id))
                .copied()
                .ok_or_else(|| Error::new("Unknown 1.20.2 reply"))?;
            if matches!(name, "CHUNK_BATCH_RECEIVED" | "CONFIGURATION_ACKNOWLEDGED") {
                continue;
            }
            let mut r = Reader::new(&data);
            let mut w = Writer::new();
            match name {
                "SIGN_UPDATE" if protocol < 763 => {
                    w.bytes(r.bytes(8)?);
                    if !r.bool()? {
                        continue;
                    }
                    for _ in 0..4 {
                        string(&mut r, &mut w)?;
                    }
                    end(&r)?;
                }
                "CLIENT_INFORMATION" => {
                    self.information = Some(data.clone());
                    w.bytes(&data);
                }
                "CUSTOM_PAYLOAD" => w.bytes(&sanitize(&data)?),
                "SET_BEACON" => {
                    for _ in 0..2 {
                        let present = r.bool()?;
                        w.bool(present);
                        if present {
                            w.varint(
                                r.varint()?
                                    .checked_add(1)
                                    .ok_or_else(|| Error::new("Beacon effect overflows"))?,
                            );
                        }
                    }
                    end(&r)?;
                }
                _ => w.bytes(&data),
            }
            result.push(packet(old::SERVERBOUND, name, w.data)?);
        }
        Ok(result)
    }
}
pub fn clientbound<M: Maps>(id: i32, data: &[u8]) -> Result<Vec<Packet>> {
    let name = name(id)?;
    let mut target = name;
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    match name {
        "LOGIN" => return Ok(vec![join::<M>(data)?.1]),
        "UPDATE_ENABLED_FEATURES" => return Ok(Vec::new()),
        "ADD_PLAYER" => {
            target = "ADD_ENTITY";
            w.varint(r.varint()?);
            w.uuid(r.uuid()?);
            let kind = M::ENTITY_IDENTIFIERS
                .iter()
                .position(|value| *value == "player")
                .ok_or_else(|| Error::new("Missing old player type"))?;
            w.varint(kind as i32);
            w.bytes(r.bytes(24)?);
            let yaw = r.u8()?;
            w.u8(r.u8()?);
            w.u8(yaw);
            w.u8(yaw);
            w.varint(0);
            w.bytes(&[0; 6]);
            end(&r)?;
        }
        "RESPAWN" => {
            string(&mut r, &mut w)?;
            string(&mut r, &mut w)?;
            w.bytes(r.bytes(12)?);
            let keep = r.u8()?;
            let death = r.bool()?;
            w.bool(death);
            if death {
                string(&mut r, &mut w)?;
                w.bytes(r.bytes(8)?);
            }
            w.varint(if M::PROTOCOL < 763 { 0 } else { r.varint()? });
            w.u8(keep);
            end(&r)?;
        }
        "FORGET_LEVEL_CHUNK" => {
            let x = r.i32()?;
            let z = r.i32()?;
            end(&r)?;
            w.i32(z);
            w.i32(x);
        }
        "SET_DISPLAY_OBJECTIVE" => {
            w.varint(i32::from(r.i8()?));
            string(&mut r, &mut w)?;
            end(&r)?;
        }
        "UPDATE_MOB_EFFECT" | "REMOVE_MOB_EFFECT" => {
            w.varint(r.varint()?);
            let id = r.varint()?;
            if id <= 0 {
                return Err(Error::new("Invalid old mob effect id"));
            }
            w.varint(id - 1);
            w.bytes(r.rest());
        }
        "TAG_QUERY" => {
            w.varint(r.varint()?);
            w.nbt(&r.vfp_named_nbt()?);
            end(&r)?;
        }
        "BLOCK_ENTITY_DATA" => {
            w.bytes(r.bytes(8)?);
            w.varint(r.varint()?);
            w.nbt(&r.vfp_named_nbt()?);
            end(&r)?;
        }
        "OPEN_SIGN_EDITOR" if M::PROTOCOL < 763 => {
            w.bytes(r.bytes(8)?);
            end(&r)?;
            w.bool(true);
        }
        "PLAYER_COMBAT_END" if M::PROTOCOL < 763 => {
            w.varint(r.varint()?);
            r.i32()?;
            end(&r)?;
        }
        "PLAYER_COMBAT_KILL" if M::PROTOCOL < 763 => {
            w.varint(r.varint()?);
            r.i32()?;
            string(&mut r, &mut w)?;
            end(&r)?;
        }
        "SECTION_BLOCKS_UPDATE" if M::PROTOCOL < 763 => {
            w.bytes(r.bytes(8)?);
            r.bool()?;
            w.bytes(r.rest());
        }
        "LIGHT_UPDATE" if M::PROTOCOL < 763 => {
            w.varint(r.varint()?);
            w.varint(r.varint()?);
            r.bool()?;
            w.bytes(r.rest());
        }
        "LEVEL_CHUNK_WITH_LIGHT" if M::PROTOCOL < 763 => {
            r.bytes(8)?;
            r.vfp_named_nbt()?;
            r.byte_array(2_097_152)?;
            for _ in 0..r.count(65536)? {
                r.bytes(3)?;
                r.varint()?;
                r.vfp_named_nbt()?;
            }
            w.bytes(&data[..r.position()]);
            r.bool()?;
            w.bytes(r.rest());
        }
        "CUSTOM_PAYLOAD" => w.bytes(&sanitize(data)?),
        "UPDATE_ADVANCEMENTS" => {
            w.bool(r.bool()?);
            let count = r.count(65536)?;
            w.varint(count as i32);
            for _ in 0..count {
                string(&mut r, &mut w)?;
                let parent = r.bool()?;
                w.bool(parent);
                if parent {
                    string(&mut r, &mut w)?;
                }
                let display = r.bool()?;
                w.bool(display);
                if display {
                    string(&mut r, &mut w)?;
                    string(&mut r, &mut w)?;
                    let start = r.position();
                    if r.bool()? {
                        r.varint()?;
                        r.i8()?;
                        r.vfp_named_nbt()?;
                    }
                    w.bytes(r.since(start));
                    w.varint(r.varint()?);
                    let flags = r.i32()?;
                    w.i32(flags);
                    if flags & 1 != 0 {
                        string(&mut r, &mut w)?;
                    }
                    w.bytes(r.bytes(8)?);
                }
                for _ in 0..r.count(65536)? {
                    r.string(32767)?;
                } // removed criteria
                let requirements = r.count(65536)?;
                w.varint(requirements as i32);
                for _ in 0..requirements {
                    let names = r.count(65536)?;
                    w.varint(names as i32);
                    for _ in 0..names {
                        string(&mut r, &mut w)?;
                    }
                }
                w.bool(if M::PROTOCOL < 763 { false } else { r.bool()? });
            }
            w.bytes(r.rest());
        }
        _ => w.bytes(data),
    }
    Ok(vec![packet(next::CLIENTBOUND, target, w.data)?])
}
