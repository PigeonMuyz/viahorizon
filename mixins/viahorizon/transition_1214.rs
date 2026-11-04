// SPDX-License-Identifier: GPL-3.0-or-later
// 1.21.2-1.21.4 structural packets, before the 1.21.5 -> native chain. Registry
// references retain their original server ids until the typed codecs run.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_mapping::Maps,
    vfp_registry_access::Registries,
    vfp_tables_261_native as types, vfp_tables_1214 as old, vfp_tables_1215 as next,
    vfp_wire::{self as wire, Packet},
};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(1);
pub fn random_color() -> i32 {
    let mut value = SERIAL
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    (value ^ (value >> 31)) as i32
}
pub fn trail_duration() -> i32 {
    (SERIAL
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_mul(6364136223846793005)
        % 40) as i32
        + 10
}
#[derive(Default)]
pub struct Transition {
    entities: BTreeMap<i32, &'static str>,
    message_index: i32,
}
impl Transition {
    pub fn clientbound_using<M: Maps>(
        &mut self,
        id: i32,
        data: &[u8],
        registry: &Registries,
    ) -> Result<Vec<Packet>> {
        let name = usize::try_from(id)
            .ok()
            .and_then(|id| old::CLIENTBOUND.get(id))
            .copied()
            .ok_or_else(|| Error::new("Unknown 1.21.4 play packet"))?;
        let mut target = name;
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        let mut extra = Vec::new();
        match name {
            "SET_HELD_SLOT" if M::PROTOCOL < 769 => {
                w.varint(i32::from(r.i8()?));
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old held-slot packet"));
                }
            }
            "LEVEL_PARTICLES" if M::PROTOCOL < 769 => {
                w.bool(r.bool()?);
                w.bool(false);
                w.bytes(r.rest());
            }
            "PLAYER_INFO_UPDATE" if M::PROTOCOL < 769 => {
                if r.u8()? & 128 != 0 {
                    return Err(Error::new("Invalid old player-info actions"));
                }
                w.bytes(data);
            }
            "LOGIN" => {
                self.entities.clear();
                self.message_index = 0;
                self.entities.insert(r.i32()?, "player");
                w.bytes(data);
            }
            "RESPAWN" => {
                self.entities.clear();
                w.bytes(data);
            }
            "ADD_ENTITY" => {
                let entity = r.varint()?;
                r.uuid()?;
                let original_kind = r.varint()?;
                let kind = if let Some(kind) = registry.entity_override(entity) {
                    kind
                } else {
                    M::ENTITIES.map(original_kind).map_err(Error::new)?
                };
                let kind = types::ENTITY_NAMES
                    .get(kind as usize)
                    .copied()
                    .ok_or_else(|| Error::new("Invalid old entity type"))?;
                if self.entities.len() >= 65536 && !self.entities.contains_key(&entity) {
                    return Err(Error::new("Too many tracked 1.21.4 entities"));
                }
                self.entities.insert(entity, kind);
                w.bytes(data);
            }
            "ADD_EXPERIENCE_ORB" => {
                let entity = r.varint()?;
                w.varint(entity);
                w.uuid(
                    0x76667000000040008000000000000000u128
                        | u128::from(SERIAL.fetch_add(1, Ordering::Relaxed)),
                );
                let kind = M::ENTITY_IDENTIFIERS
                    .iter()
                    .position(|kind| *kind == "experience_orb")
                    .expect("old experience orb identifier");
                w.varint(kind as i32);
                w.bytes(r.bytes(24)?);
                w.bytes(&[0; 3]);
                w.varint(i32::from(r.i16()?));
                w.bytes(&[0; 6]);
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old experience orb"));
                }
                target = "ADD_ENTITY";
            }
            "SET_ENTITY_DATA" => {
                let entity = Reader::new(data).varint()?;
                let kind = self.entities.get(&entity).copied().unwrap_or("");
                let (metadata, saddle) = crate::entity_data::vfp_entities::prepare_metadata_1214::<
                    M,
                >(data, kind, registry)?;
                if let Some(saddled) = saddle {
                    let mut equipment = Writer::new();
                    equipment.varint(entity);
                    equipment.u8(7);
                    equipment.varint(i32::from(saddled));
                    if saddled {
                        equipment.varint(if M::NATIVE_ITEMS {
                            M::ITEMS.map(M::SADDLE_OLD).map_err(Error::new)?
                        } else {
                            M::SADDLE_OLD
                        });
                        equipment.varint(0);
                        equipment.varint(0);
                    }
                    extra.push((
                        wire::packet_id(next::CLIENTBOUND, "SET_EQUIPMENT").map_err(Error::new)?,
                        equipment.data,
                    ));
                }
                w.bytes(&metadata);
            }
            "REMOVE_ENTITIES" => {
                for _ in 0..r.count(65536)? {
                    self.entities.remove(&r.varint()?);
                }
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old entity removals"));
                }
                w.bytes(data);
            }
            "PLAYER_CHAT" => {
                w.varint(self.message_index);
                self.message_index = self.message_index.wrapping_add(1);
                w.bytes(data);
            }
            "UPDATE_MOB_EFFECT" => {
                w.varint(r.varint()?);
                let effect = r.varint()?;
                w.varint(effect);
                w.varint(r.varint()?);
                w.varint(r.varint()?);
                let flags = r.u8()?;
                w.u8(if effect == 8 { flags | 8 } else { flags });
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old mob effect"));
                }
            }
            "SET_PLAYER_TEAM" => {
                w.string(&r.string(32767)?);
                let action = r.u8()?;
                w.u8(action);
                if action == 0 || action == 2 {
                    w.nbt(&r.nbt()?);
                    w.u8(r.u8()?);
                    let visibility = r.string(32767)?;
                    let collision = r.string(32767)?;
                    w.varint(match visibility.as_str() {
                        "never" => 1,
                        "hideForOtherTeams" => 2,
                        "hideForOwnTeam" => 3,
                        _ => 0,
                    });
                    w.varint(match collision.as_str() {
                        "never" => 1,
                        "pushOtherTeams" => 2,
                        "pushOwnTeam" => 3,
                        _ => 0,
                    });
                    w.varint(r.varint()?);
                    w.nbt(&r.nbt()?);
                    w.nbt(&r.nbt()?);
                }
                w.bytes(r.rest());
            }
            "ENTITY_EVENT" => {
                let entity = r.i32()?;
                let event = r.u8()?;
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old entity event"));
                }
                if matches!(event, 29 | 30) {
                    target = "SOUND_ENTITY";
                    w.varint(
                        if event == 29 {
                            M::SHIELD_BLOCK_OLD
                        } else {
                            M::SHIELD_BREAK_OLD
                        } + 1,
                    );
                    w.varint(7);
                    w.varint(entity);
                    w.f32(if event == 29 { 1.0 } else { 0.8 });
                    let seed = SERIAL
                        .fetch_add(1, Ordering::Relaxed)
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(entity as u64);
                    w.f32(0.8 + (seed & 65535) as f32 / 65535.0 * 0.4);
                    w.i64(seed as i64);
                } else {
                    w.bytes(data);
                }
            }
            _ => w.bytes(data),
        }
        extra.push((
            wire::packet_id(next::CLIENTBOUND, target).map_err(Error::new)?,
            w.data,
        ));
        Ok(extra)
    }
    pub fn serverbound(&mut self, protocol: i32, packets: Vec<Packet>) -> Result<Vec<Packet>> {
        let mut result = Vec::new();
        for (id, data) in packets {
            let name = usize::try_from(id)
                .ok()
                .and_then(|id| next::SERVERBOUND.get(id))
                .copied()
                .ok_or_else(|| Error::new("Unknown intermediate 1.21.5 reply"))?;
            if matches!(name, "TEST_INSTANCE_BLOCK_ACTION" | "SET_TEST_BLOCK") {
                continue;
            }
            if protocol < 769
                && matches!(
                    name,
                    "PLAYER_LOADED" | "PICK_ITEM_FROM_BLOCK" | "PICK_ITEM_FROM_ENTITY"
                )
            {
                continue;
            }
            let mut payload = data;
            if protocol < 769 && name == "MOVE_VEHICLE" {
                let mut r = Reader::new(&payload);
                r.bytes(32)?;
                r.bool()?;
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old vehicle movement"));
                }
                payload.pop();
            }
            if matches!(name, "CHAT" | "CHAT_COMMAND_SIGNED") {
                let mut r = Reader::new(&payload);
                r.string(if name == "CHAT" { 256 } else { 32767 })?;
                r.bytes(16)?;
                if name == "CHAT" {
                    if r.bool()? {
                        r.bytes(256)?;
                    }
                } else {
                    for _ in 0..r.count(8)? {
                        r.string(32767)?;
                        r.bytes(256)?;
                    }
                }
                r.varint()?;
                r.bytes(3)?;
                r.u8()?;
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old chat reply"));
                }
                payload.pop();
            }
            result.push((
                wire::packet_id(
                    if protocol < 769 {
                        crate::vfp_tables_1212::SERVERBOUND
                    } else {
                        old::SERVERBOUND
                    },
                    name,
                )
                .map_err(Error::new)?,
                payload,
            ));
        }
        Ok(result)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
