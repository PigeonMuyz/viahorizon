// SPDX-License-Identifier: GPL-3.0-or-later
// Structural 1.21.5-1.21.8 -> 1.21.9 packets. Registry ids stay in the original
// domain until the final Maps codec; only layouts and units change here.
use crate::{
    Error, Result,
    buf::{Codec, LpVec3, Reader, Writer},
    vfp_mapping::Maps,
    vfp_registry_access::Registries,
    vfp_tables_261_native as types, vfp_tables_1217 as old, vfp_tables_1219 as next,
    vfp_tables_12111 as server,
    vfp_wire::{self as wire, Packet},
};
use std::collections::BTreeSet;
pub struct Transition {
    players: BTreeSet<i32>,
    hanging: BTreeSet<i32>,
    sneaking: bool,
    local: Option<i32>,
    dimension: i32,
    world: Option<String>,
}
impl Default for Transition {
    fn default() -> Self {
        Self {
            players: BTreeSet::new(),
            hanging: BTreeSet::new(),
            sneaking: false,
            local: None,
            dimension: -1,
            world: None,
        }
    }
}
impl Transition {
    fn spawn(&mut self, r: &mut Reader<'_>) -> Result<()> {
        self.dimension = r.varint()?;
        self.world = Some(r.string(32767)?);
        Ok(())
    }
    pub fn clientbound<M: Maps>(
        &mut self,
        id: i32,
        data: &[u8],
        registry: &Registries,
    ) -> Result<Packet> {
        let source = if M::PROTOCOL <= 770 {
            crate::vfp_tables_1215::CLIENTBOUND
        } else {
            old::CLIENTBOUND
        };
        let name = usize::try_from(id)
            .ok()
            .and_then(|id| source.get(id))
            .copied()
            .ok_or_else(|| Error::new("Unknown 1.21.5-1.21.8 play packet"))?;
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        match name {
            "LOGIN" => {
                let player = r.i32()?;
                self.local = Some(player);
                self.players.clear();
                self.players.insert(player);
                self.hanging.clear();
                self.sneaking = false;
                r.bool()?;
                for _ in 0..r.count(1024)? {
                    r.string(32767)?;
                }
                for _ in 0..3 {
                    r.varint()?;
                }
                for _ in 0..3 {
                    r.bool()?;
                }
                self.spawn(&mut r)?;
                w.bytes(data);
            }
            "RESPAWN" => {
                self.spawn(&mut r)?;
                self.players.clear();
                self.hanging.clear();
                self.sneaking = false;
                if let Some(player) = self.local {
                    self.players.insert(player);
                }
                w.bytes(data);
            }
            "ADD_ENTITY" => {
                let entity = r.varint()?;
                w.varint(entity);
                w.uuid(r.uuid()?);
                let source = r.varint()?;
                w.varint(source);
                let native = M::ENTITIES.map(source).map_err(Error::new)?;
                self.players.remove(&entity);
                self.hanging.remove(&entity);
                if M::PROTOCOL <= 770
                    && matches!(
                        types::ENTITY_NAMES.get(native as usize),
                        Some(&"painting" | &"leash_knot" | &"item_frame" | &"glow_item_frame")
                    )
                {
                    if self.hanging.len() >= 65536 {
                        return Err(Error::new("Too many tracked hanging entities"));
                    }
                    self.hanging.insert(entity);
                }
                if types::ENTITY_NAMES.get(native as usize) == Some(&"player") {
                    if self.players.len() >= 65536 {
                        return Err(Error::new("Too many tracked old players"));
                    }
                    self.players.insert(entity);
                }
                w.bytes(r.bytes(24)?);
                let angles = r.bytes(3)?.to_vec();
                let object = r.varint()?;
                let velocity = [
                    f64::from(r.i16()?) / 8000.0,
                    f64::from(r.i16()?) / 8000.0,
                    f64::from(r.i16()?) / 8000.0,
                ];
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old spawn packet"));
                }
                LpVec3::write(&velocity, &mut w);
                w.bytes(&angles);
                w.varint(object);
            }
            "SET_ENTITY_MOTION" => {
                w.varint(r.varint()?);
                let velocity = [
                    f64::from(r.i16()?) / 8000.0,
                    f64::from(r.i16()?) / 8000.0,
                    f64::from(r.i16()?) / 8000.0,
                ];
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old velocity packet"));
                }
                LpVec3::write(&velocity, &mut w);
            }
            "PLAYER_ROTATION" => {
                w.f32(r.f32()?);
                w.bool(false);
                w.f32(r.f32()?);
                w.bool(false);
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old rotation"));
                }
            }
            "SET_DEFAULT_SPAWN_POSITION" => {
                let position = r.bytes(8)?.to_vec();
                let yaw = r.f32()?;
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old default spawn"));
                }
                w.string(
                    self.world
                        .as_deref()
                        .ok_or_else(|| Error::new("Default spawn before login"))?,
                );
                w.bytes(&position);
                w.f32(yaw);
                w.f32(0.0);
            }
            "INITIALIZE_BORDER" | "SET_BORDER_CENTER" => {
                let scale = registry.dimension_scale(self.dimension);
                w.f64(r.f64()? / scale);
                w.f64(r.f64()? / scale);
                w.bytes(r.rest());
            }
            "EXPLODE" => {
                w.bytes(r.bytes(24)?);
                let (power, affected) = if M::PROTOCOL < 768 {
                    registry.explosion().unwrap_or((0.0, 0))
                } else {
                    (0.0, 0)
                };
                w.f32(power);
                w.i32(affected);
                w.bytes(r.rest());
                w.varint(0);
            }
            "SET_ENTITY_DATA" => {
                let entity = Reader::new(data).varint()?;
                w.bytes(
                    &crate::entity_data::vfp_entities::prepare_metadata_1217::<M>(
                        data,
                        self.players.contains(&entity),
                        self.hanging.contains(&entity),
                        registry,
                    )?,
                );
            }
            "REMOVE_ENTITIES" => {
                for _ in 0..r.count(65536)? {
                    let entity = r.varint()?;
                    self.players.remove(&entity);
                    self.hanging.remove(&entity);
                }
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing remove-entities data"));
                }
                w.bytes(data);
            }
            "CHANGE_DIFFICULTY" if M::PROTOCOL <= 770 => {
                let difficulty = r.u8()?;
                if difficulty > 3 {
                    return Err(Error::new("Invalid old difficulty"));
                }
                w.varint(i32::from(difficulty));
                w.bool(r.bool()?);
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old difficulty data"));
                }
            }
            _ => w.bytes(data),
        }
        Ok((
            wire::packet_id(next::CLIENTBOUND, name).map_err(Error::new)?,
            w.data,
        ))
    }
    pub fn serverbound(&mut self, protocol: i32, packets: Vec<Packet>) -> Result<Vec<Packet>> {
        let mut output = Vec::new();
        for (id, payload) in packets {
            let name = usize::try_from(id)
                .ok()
                .and_then(|id| server::SERVERBOUND.get(id))
                .copied()
                .ok_or_else(|| Error::new("Unknown intermediate serverbound packet"))?;
            let mut data = payload;
            let mut target = name;
            if protocol <= 770 {
                let mut r = Reader::new(&data);
                let mut w = Writer::new();
                match name {
                    "CUSTOM_CLICK_ACTION" => continue,
                    "PLAYER_COMMAND" => {
                        w.varint(r.varint()?);
                        let action = r.varint()?;
                        if !(0..=6).contains(&action) {
                            return Err(Error::new("Invalid old player command"));
                        }
                        w.varint(action + 2);
                        w.varint(r.varint()?);
                    }
                    "PLAYER_INPUT" => {
                        let flags = r.u8()?;
                        let sneaking = flags & 32 != 0;
                        if self.sneaking != sneaking {
                            let mut command = Writer::new();
                            command.varint(
                                self.local
                                    .ok_or_else(|| Error::new("Player input before login"))?,
                            );
                            command.varint(if sneaking { 0 } else { 1 });
                            command.varint(0);
                            output.push((
                                wire::packet_id(
                                    crate::vfp_tables_1215::SERVERBOUND,
                                    "PLAYER_COMMAND",
                                )
                                .map_err(Error::new)?,
                                command.data,
                            ));
                            self.sneaking = sneaking;
                        }
                        w.u8(flags);
                    }
                    "CHANGE_DIFFICULTY" => {
                        let difficulty = r.varint()?;
                        if !(0..=3).contains(&difficulty) {
                            return Err(Error::new("Invalid old difficulty request"));
                        }
                        w.u8(difficulty as u8);
                    }
                    "CHANGE_GAME_MODE" => {
                        let mode = match r.varint()? {
                            0 => "survival",
                            1 => "creative",
                            2 => "adventure",
                            3 => "spectator",
                            _ => return Err(Error::new("Invalid old game mode request")),
                        };
                        w.string(&format!("gamemode {mode}"));
                        target = "CHAT_COMMAND";
                    }
                    _ => w.bytes(r.rest()),
                }
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing 1.21.5 request data"));
                }
                data = w.data;
            }
            if name == "DEBUG_SUBSCRIPTION_REQUEST" {
                let mut r = Reader::new(&data);
                let mut ticks = false;
                for _ in 0..r.count(32)? {
                    ticks |= r.varint()? == 0;
                }
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing debug subscription request"));
                }
                if !ticks {
                    continue;
                }
                data = vec![0];
            }
            let source = if protocol <= 770 {
                crate::vfp_tables_1215::SERVERBOUND
            } else {
                old::SERVERBOUND
            };
            output.push((wire::packet_id(source, target).map_err(Error::new)?, data));
        }
        Ok(output)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
