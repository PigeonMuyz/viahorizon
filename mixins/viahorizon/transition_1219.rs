// SPDX-License-Identifier: GPL-3.0-or-later
// 1.21.9/1.21.10 -> 1.21.11. Registry ids stay in the original server domain;
// the final typed codec maps them once via V1219.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_mapping::{Maps, V1219},
    vfp_registry_access::Registries,
    vfp_tables_261_native as types, vfp_tables_1219 as old, vfp_tables_12111 as middle,
    vfp_wire::{self as wire, Packet},
};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Transition {
    game_time: i64,
    metadata: BTreeMap<i32, u8>,
    local: Option<i32>,
}
impl Transition {
    pub fn clientbound(&mut self, id: i32, data: &[u8], registry: &Registries) -> Result<Packet> {
        self.clientbound_using::<V1219>(id, data, registry)
    }
    pub fn clientbound_using<M: Maps>(
        &mut self,
        id: i32,
        data: &[u8],
        registry: &Registries,
    ) -> Result<Packet> {
        let name = usize::try_from(id)
            .ok()
            .and_then(|id| old::CLIENTBOUND.get(id))
            .copied()
            .ok_or_else(|| Error::new("Unknown 1.21.9 play packet"))?;
        let mut target = name;
        let mut output = data.to_vec();
        match name {
            "LOGIN" => {
                let entity = Reader::new(data).i32()?;
                self.local = Some(entity);
                self.metadata.clear();
                self.metadata.insert(entity, 4);
            }
            "RESPAWN" => {
                self.metadata.clear();
                if let Some(entity) = self.local {
                    self.metadata.insert(entity, 4);
                }
            }
            "ADD_ENTITY" => {
                let mut r = Reader::new(data);
                let id = r.varint()?;
                r.uuid()?;
                let kind = M::ENTITIES.map(r.varint()?).map_err(Error::new)?;
                let mut flags = match types::ENTITY_NAMES.get(kind as usize) {
                    Some(&"wolf") => 1,
                    Some(&"bee") => 2,
                    _ => 0,
                };
                if types::AVATARS.contains(&kind) {
                    flags |= 4;
                }
                self.metadata.remove(&id);
                if flags != 0 {
                    if self.metadata.len() >= 65536 {
                        return Err(Error::new("Too many tracked 1.21.9 entities"));
                    }
                    self.metadata.insert(id, flags);
                }
            }
            "REMOVE_ENTITIES" => {
                let mut r = Reader::new(data);
                for _ in 0..r.count(65536)? {
                    self.metadata.remove(&r.varint()?);
                }
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing remove-entities data"));
                }
            }
            "SET_ENTITY_DATA" => {
                let entity = Reader::new(data).varint()?;
                output = crate::entity_data::vfp_entities::prepare_metadata_1219_using::<M>(
                    data,
                    self.metadata.get(&entity).copied().unwrap_or(0),
                    self.game_time,
                    registry,
                )?;
            }
            "SET_TIME" => {
                self.game_time = Reader::new(data).i64()?;
            }
            "SET_BORDER_LERP_SIZE" | "INITIALIZE_BORDER" => {
                let mut r = Reader::new(data);
                let mut w = Writer::new();
                w.bytes(r.bytes(if name == "INITIALIZE_BORDER" { 32 } else { 16 })?);
                w.varlong(r.varlong()? / 50);
                w.bytes(r.rest());
                output = w.data;
            }
            "HORSE_SCREEN_OPEN" => target = "MOUNT_SCREEN_OPEN",
            _ => (),
        }
        Ok((
            wire::packet_id(middle::CLIENTBOUND, target).map_err(Error::new)?,
            output,
        ))
    }
    pub fn serverbound(&mut self, packets: Vec<Packet>) -> Result<Vec<Packet>> {
        let mut output = Vec::new();
        for (id, data) in packets {
            let name = usize::try_from(id)
                .ok()
                .and_then(|id| middle::SERVERBOUND.get(id))
                .copied()
                .ok_or_else(|| Error::new("Unknown 1.21.11 serverbound packet"))?;
            if name == "PLAYER_ACTION" && Reader::new(&data).varint()? == 7 {
                continue;
            }
            if name == "CLIENT_TICK_END" {
                if !data.is_empty() {
                    return Err(Error::new("Trailing client-tick-end data"));
                }
                self.game_time = self.game_time.wrapping_add(1);
            }
            output.push((id, data)); // Both versions share ServerboundPackets1_21_6.
        }
        Ok(output)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
