// SPDX-License-Identifier: GPL-3.0-or-later
// Structural 1.21.11 -> 26.1 changes. Static references are mapped separately
// by V12111 directly to the native registries, so they are never mapped twice.
use crate::{
    Error, Result,
    buf::{Codec, LpVec3, Reader, Writer},
    vfp_mapping::{Maps, V12111},
    vfp_registry_access::Registries,
    vfp_tables as t, vfp_tables_261_native as types, vfp_tables_12111 as old,
    vfp_wire::{self as wire, Packet},
};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Transition {
    sneaking: bool,
    metadata: BTreeMap<i32, u8>,
}
impl Transition {
    pub fn clientbound_using<M: Maps>(
        &mut self,
        id: i32,
        data: &[u8],
        registry: &Registries,
    ) -> Result<Packet> {
        let name = usize::try_from(id)
            .ok()
            .and_then(|i| old::CLIENTBOUND.get(i))
            .copied()
            .ok_or_else(|| Error::new("Unknown 1.21.11 play packet"))?;
        let mut output = data.to_vec();
        match name {
            "LOGIN" | "RESPAWN" => {
                self.sneaking = false;
                self.metadata.clear();
            }
            "ADD_ENTITY" => {
                let mut r = Reader::new(data);
                let id = r.varint()?;
                r.uuid()?;
                let kind = M::ENTITIES.map(r.varint()?).map_err(Error::new)?;
                let mut flags = u8::from(types::AGEABLES.contains(&kind));
                match types::ENTITY_NAMES.get(kind as usize) {
                    Some(&"villager") => flags |= 2,
                    Some(&"zombie_villager") => flags |= 4,
                    _ => (),
                }
                self.metadata.remove(&id);
                if flags != 0 {
                    if self.metadata.len() >= 65536 {
                        return Err(Error::new("Too many tracked old entity types"));
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
                let id = Reader::new(data).varint()?;
                output = crate::entity_data::vfp_entities::prepare_metadata_12111_using::<M>(
                    data,
                    self.metadata.get(&id).copied().unwrap_or(0),
                    registry,
                )?;
            }
            "SET_TIME" => {
                let mut r = Reader::new(data);
                let time = r.i64()?;
                let day = r.i64()?;
                let ticking = r.bool()?;
                if r.remaining() != 0 {
                    return Err(Error::new("Trailing old time update"));
                }
                let mut w = Writer::new();
                w.i64(time);
                w.varint(2);
                for name in ["overworld", "the_end"] {
                    w.varint(registry.id("world_clock", name).map_err(Error::new)?);
                    w.varlong(day);
                    w.f32(0.0);
                    w.f32(if ticking { 1.0 } else { 0.0 });
                }
                output = w.data;
            }
            _ => (),
        }
        Ok((
            wire::packet_id(t::OLD_CLIENTBOUND, name).map_err(Error::new)?,
            output,
        ))
    }
    pub fn clientbound(&mut self, id: i32, data: &[u8], registry: &Registries) -> Result<Packet> {
        self.clientbound_using::<V12111>(id, data, registry)
    }
    pub fn serverbound(&mut self, packets: Vec<Packet>) -> Result<Vec<Packet>> {
        let mut output = Vec::new();
        for (id, payload) in packets {
            let name = usize::try_from(id)
                .ok()
                .and_then(|i| t::OLD_SERVERBOUND.get(i))
                .copied()
                .ok_or_else(|| Error::new("Unknown canonical serverbound packet"))?;
            let mut r = Reader::new(&payload);
            let mut w = Writer::new();
            let mut target = name;
            match name {
                "SET_GAME_RULE" => continue,
                "CLIENT_COMMAND" => {
                    let action = r.varint()?;
                    if r.remaining() != 0 || !(0..=2).contains(&action) {
                        return Err(Error::new("Invalid client command"));
                    }
                    if action == 2 {
                        continue;
                    }
                    w.varint(action);
                }
                "PLAYER_INPUT" => {
                    let flags = r.u8()?;
                    if r.remaining() != 0 {
                        return Err(Error::new("Trailing player-input data"));
                    }
                    self.sneaking = flags & 32 != 0;
                    w.u8(flags);
                }
                "ATTACK" | "SPECTATE_ENTITY" => {
                    w.varint(r.varint()?);
                    if r.remaining() != 0 {
                        return Err(Error::new("Trailing old attack data"));
                    }
                    w.varint(1);
                    w.bool(self.sneaking);
                    target = "INTERACT";
                }
                "INTERACT" => {
                    let entity = r.varint()?;
                    let hand = r.varint()?;
                    let location = LpVec3::read(&mut r)?;
                    let secondary = r.bool()?;
                    if !(0..=1).contains(&hand) || r.remaining() != 0 {
                        return Err(Error::new("Invalid entity interaction"));
                    }
                    self.sneaking = secondary;
                    let mut at = Writer::new();
                    at.varint(entity);
                    at.varint(2);
                    for coordinate in location {
                        at.f32(coordinate as f32);
                    }
                    at.varint(hand);
                    at.bool(secondary);
                    output.push((
                        wire::packet_id(old::SERVERBOUND, "INTERACT").map_err(Error::new)?,
                        at.data,
                    ));
                    w.varint(entity);
                    w.varint(0);
                    w.varint(hand);
                    w.bool(secondary);
                }
                _ => w.bytes(&payload),
            }
            output.push((
                wire::packet_id(old::SERVERBOUND, target).map_err(Error::new)?,
                w.data,
            ));
        }
        Ok(output)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
