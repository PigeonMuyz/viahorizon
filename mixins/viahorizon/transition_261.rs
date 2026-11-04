// SPDX-License-Identifier: GPL-3.0-or-later
// Structural 26.1 -> 26.2 changes, around the shared source-version codecs.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    packets::play,
    vfp_tables as t, vfp_tables_261_native as types,
    vfp_wire::{self as wire, Packet},
};
use std::{borrow::Cow, collections::BTreeMap};
pub fn prepare<'a>(name: &str, payload: &'a [u8], encrypted: bool) -> Result<Cow<'a, [u8]>> {
    if name == "LOGIN" {
        let (&secure_chat, body) = payload
            .split_last()
            .ok_or_else(|| Error::new("Missing secure-chat flag"))?;
        if secure_chat > 1 {
            return Err(Error::new("Invalid secure-chat flag"));
        }
        let mut p = body.to_vec();
        p.push(u8::from(encrypted));
        p.push(secure_chat);
        return Ok(Cow::Owned(p));
    }
    if name != "SET_PLAYER_TEAM" {
        return Ok(Cow::Borrowed(payload));
    }
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.string(&r.string(32767)?);
    let mode = r.i8()?;
    w.i8(mode);
    if mode == 0 || mode == 2 {
        let display = r.nbt()?;
        let flags = r.i8()?;
        let visibility = r.varint()?;
        let collision = r.varint()?;
        let color = r.varint()?;
        let prefix = r.nbt()?;
        let suffix = r.nbt()?;
        w.nbt(&display);
        w.nbt(&prefix);
        w.nbt(&suffix);
        w.varint(visibility);
        w.varint(collision);
        w.bool((0..16).contains(&color));
        if (0..16).contains(&color) {
            w.varint(color);
        }
        w.i8(flags);
    }
    if matches!(mode, 0 | 3 | 4) {
        let n = r.count(r.remaining())?;
        w.varint(n as i32);
        for _ in 0..n {
            w.string(&r.string(32767)?);
        }
    }
    if !(0..=4).contains(&mode) || r.remaining() != 0 {
        return Err(Error::new("Invalid 26.1 team packet"));
    }
    Ok(Cow::Owned(w.data))
}
#[derive(Clone, Copy)]
enum Kind {
    Cube,
    Frog,
    Firework,
}
pub struct Entities {
    fake: i32,
    kinds: BTreeMap<i32, Kind>,
}
impl Entities {
    pub fn new() -> Result<Self> {
        let secret = crate::crypto::generate_secret()?;
        let id = u32::from_be_bytes(secret[..4].try_into().unwrap()) % 2147483646;
        Self::with_alias(-2 - (id as i32))
    }
    pub fn with_alias(fake: i32) -> Result<Self> {
        if fake >= -1 {
            return Err(Error::new("Zero-entity alias must be below -1"));
        }
        Ok(Self {
            fake,
            kinds: BTreeMap::new(),
        })
    }
    fn client(&self, id: i32) -> i32 {
        if id == 0 { self.fake } else { id }
    }
    fn server(&self, id: i32) -> i32 {
        if id == self.fake { 0 } else { id }
    }
    fn body(packet: play::Clientbound) -> Result<Packet> {
        let bytes = packet.encode();
        let mut r = Reader::new(&bytes);
        let id = r.varint()?;
        Ok((id, r.rest().to_vec()))
    }
    pub fn clientbound(&mut self, packets: &mut [Packet]) -> Result<()> {
        for (id, data) in packets {
            let name = usize::try_from(*id)
                .ok()
                .and_then(|i| t::NEW_CLIENTBOUND.get(i))
                .copied()
                .ok_or_else(|| Error::new("Invalid translated entity packet"))?;
            match name {
                "LOGIN" => {
                    let play::Clientbound::Login(mut p) = play::Clientbound::decode(*id, data)?
                    else {
                        unreachable!()
                    };
                    self.kinds.clear();
                    p.player_id = self.client(p.player_id);
                    *data = Self::body(play::Clientbound::Login(p))?.1;
                }
                "RESPAWN" => {
                    self.kinds.clear();
                }
                "ADD_ENTITY" => {
                    let play::Clientbound::AddEntity(mut p) = play::Clientbound::decode(*id, data)?
                    else {
                        unreachable!()
                    };
                    if p.id == self.fake {
                        return Err(Error::new(
                            "Server entity id collides with reserved zero alias",
                        ));
                    }
                    p.id = self.client(p.id);
                    if types::PROJECTILES.contains(&p.kind) {
                        p.data = self.client(p.data);
                    }
                    let name = types::ENTITY_NAMES
                        .get(p.kind as usize)
                        .copied()
                        .ok_or_else(|| Error::new("Unknown mapped entity type"))?;
                    let kind = if types::CUBE_MOBS.contains(&p.kind) {
                        Some(Kind::Cube)
                    } else {
                        match name {
                            "frog" => Some(Kind::Frog),
                            "firework_rocket" => Some(Kind::Firework),
                            _ => None,
                        }
                    };
                    self.kinds.remove(&p.id);
                    if let Some(kind) = kind {
                        if self.kinds.len() >= 65536 {
                            return Err(Error::new("Too many tracked metadata types"));
                        }
                        self.kinds.insert(p.id, kind);
                    }
                    *data = Self::body(play::Clientbound::AddEntity(p))?.1;
                }
                "SET_ENTITY_DATA" => {
                    let mut r = Reader::new(data);
                    let entity = self.client(r.varint()?);
                    let kind = self.kinds.get(&entity).copied();
                    *data = crate::entity_data::vfp_entities::metadata_261(
                        entity,
                        r.rest(),
                        matches!(kind, Some(Kind::Cube)),
                        match kind {
                            Some(Kind::Frog) => Some(19),
                            Some(Kind::Firework) => Some(9),
                            _ => None,
                        },
                        self.fake,
                    )?;
                }
                "REMOVE_ENTITIES" => {
                    let play::Clientbound::RemoveEntities(mut p) =
                        play::Clientbound::decode(*id, data)?
                    else {
                        unreachable!()
                    };
                    for entity in &mut p.ids {
                        *entity = self.client(*entity);
                        self.kinds.remove(entity);
                    }
                    *data = Self::body(play::Clientbound::RemoveEntities(p))?.1;
                }
                "SET_PASSENGERS" => {
                    let play::Clientbound::SetPassengers(mut p) =
                        play::Clientbound::decode(*id, data)?
                    else {
                        unreachable!()
                    };
                    p.vehicle = self.client(p.vehicle);
                    for id in &mut p.passengers {
                        *id = self.client(*id);
                    }
                    *data = Self::body(play::Clientbound::SetPassengers(p))?.1;
                }
                _ => {
                    let mut r = Reader::new(data);
                    let mut w = Writer::new();
                    match name {
                        "SET_ENTITY_LINK" | "ENTITY_EVENT" => w.i32(self.client(r.i32()?)),
                        "MOUNT_SCREEN_OPEN" => {
                            w.varint(r.varint()?);
                            w.varint(r.varint()?);
                            w.i32(self.client(r.i32()?));
                        }
                        "DAMAGE_EVENT" => {
                            w.varint(self.client(r.varint()?));
                            w.varint(r.varint()?);
                            for _ in 0..2 {
                                let n = r.varint()?;
                                w.varint(if n == 1 { self.fake + 1 } else { n });
                            }
                        }
                        "PLAYER_LOOK_AT" => {
                            w.varint(r.varint()?);
                            w.bytes(r.bytes(24)?);
                            let entity = r.bool()?;
                            w.bool(entity);
                            if entity {
                                w.varint(self.client(r.varint()?));
                            }
                        }
                        "SOUND_ENTITY" => {
                            let holder = r.varint()?;
                            w.varint(holder);
                            if holder == 0 {
                                w.string(&r.string(32767)?);
                                let range = r.bool()?;
                                w.bool(range);
                                if range {
                                    w.f32(r.f32()?);
                                }
                            }
                            w.varint(r.varint()?);
                            w.varint(self.client(r.varint()?));
                        }
                        "TAKE_ITEM_ENTITY" => {
                            w.varint(self.client(r.varint()?));
                            w.varint(self.client(r.varint()?));
                        }
                        "ANIMATE"
                        | "SWING_ANIMATION"
                        | "ENTITY_POSITION_SYNC"
                        | "HURT_ANIMATION"
                        | "MOVE_ENTITY_POS"
                        | "MOVE_ENTITY_ROT"
                        | "MOVE_ENTITY_POS_ROT"
                        | "MOVE_MINECART_ALONG_TRACK"
                        | "REMOVE_MOB_EFFECT"
                        | "ROTATE_HEAD"
                        | "SET_CAMERA"
                        | "SET_ENTITY_MOTION"
                        | "UPDATE_MOB_EFFECT"
                        | "TELEPORT_ENTITY"
                        | "PROJECTILE_POWER"
                        | "BLOCK_DESTRUCTION"
                        | "SET_EQUIPMENT"
                        | "PLAYER_COMBAT_KILL"
                        | "UPDATE_ATTRIBUTES" => w.varint(self.client(r.varint()?)),
                        _ => continue,
                    }
                    w.bytes(r.rest());
                    *data = w.data;
                }
            }
        }
        Ok(())
    }
    pub fn serverbound(&self, packets: &mut [Packet]) -> Result<()> {
        for (id, data) in packets {
            let name = usize::try_from(*id)
                .ok()
                .and_then(|i| t::OLD_SERVERBOUND.get(i))
                .copied()
                .ok_or_else(|| Error::new("Invalid reverse entity packet"))?;
            let mut r = Reader::new(data);
            let mut w = Writer::new();
            match name {
                "SPECTATE_ENTITY" => {
                    let encoded = r.varint()?;
                    if encoded == 0 {
                        *id = wire::packet_id(t::OLD_SERVERBOUND, "SWING").map_err(Error::new)?;
                        w.varint(0);
                    } else {
                        w.varint(self.server(encoded.wrapping_sub(1)));
                    }
                    if r.remaining() != 0 {
                        return Err(Error::new("Trailing spectator action"));
                    }
                }
                "ENTITY_TAG_QUERY" => {
                    w.varint(r.varint()?);
                    w.varint(self.server(r.varint()?));
                    w.bytes(r.rest());
                }
                "ATTACK"
                | "INTERACT"
                | "PICK_ITEM_FROM_ENTITY"
                | "PLAYER_COMMAND"
                | "SET_COMMAND_MINECART" => {
                    w.varint(self.server(r.varint()?));
                    w.bytes(r.rest());
                }
                _ => continue,
            }
            *data = w.data;
        }
        Ok(())
    }
}
