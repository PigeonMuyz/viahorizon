use crate::vfp_mapping::{Maps, V262};
// SPDX-License-Identifier: GPL-3.0-or-later
// Per-connection inventory prediction restoration. Unknown hashes use the
// native client's server-resynchronization fallback instead of blocking clicks.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    item::{Component, Hashed, Stack},
    packets::play,
    vfp_hash as hash, vfp_items as items, vfp_tables as t,
    vfp_wire::{self as wire, Packet},
};
use horizon_world::nbt::Tag;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
pub fn nbt(tag: &Tag) -> Result<i32> {
    nbt_at(tag, 0)
}
fn nbt_at(tag: &Tag, depth: usize) -> Result<i32> {
    if depth > 64 {
        return Err(Error::new("Hash NBT nesting exceeds 64"));
    }
    Ok(match tag {
        Tag::End => return Err(Error::new("Cannot hash an absent NBT value")),
        Tag::Byte(v) => hash::byte(*v),
        Tag::Short(v) => hash::short(*v),
        Tag::Int(v) => hash::int(*v),
        Tag::Long(v) => hash::long(*v),
        Tag::Float(v) => hash::float(f32::from_bits(*v)),
        Tag::Double(v) => hash::double(f64::from_bits(*v)),
        Tag::String(v) => hash::utf16(&v.0),
        Tag::ByteArray(v) => hash::bytes(v),
        Tag::IntArray(v) => hash::ints(v),
        Tag::LongArray(v) => hash::longs(v),
        Tag::List(values) => hash::list(
            values
                .iter()
                .map(|v| nbt_at(v, depth + 1))
                .collect::<Result<Vec<_>>>()?,
        ),
        Tag::Compound(values) => hash::map(
            values
                .iter()
                .map(|(k, v)| Ok((hash::utf16(&k.0), nbt_at(v, depth + 1)?)))
                .collect::<Result<Vec<_>>>()?,
        ),
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Patch {
    added: Vec<(i32, i32)>,
    removed: Vec<i32>,
}
pub struct Predictions {
    limit: usize,
    entries: BTreeMap<(i32, i32), Option<Patch>>,
    order: VecDeque<(i32, i32)>,
}
impl Default for Predictions {
    fn default() -> Self {
        Self::new(1024)
    }
}
impl Predictions {
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.clamp(1, 4096),
            entries: BTreeMap::new(),
            order: VecDeque::new(),
        }
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }
    fn remember(&mut self, key: (i32, i32), patch: Patch) {
        if let Some(previous) = self.entries.get_mut(&key) {
            if previous.as_ref() != Some(&patch) {
                *previous = None;
            }
            return;
        }
        while self.entries.len() >= self.limit {
            if let Some(key) = self.order.pop_front() {
                self.entries.remove(&key);
            }
        }
        self.entries.insert(key, Some(patch));
        self.order.push_back(key);
    }
    fn observe_stack_with<M: Maps>(
        &mut self,
        stack: &Stack,
        registry: &crate::vfp_registry_access::Registries,
    ) -> Result<()> {
        let Some(Component::Nbt(tag)) = stack.component(0) else {
            return Ok(());
        };
        if tag.get(items::BACKUP_KEY).is_none() {
            return Ok(());
        }
        let mut w = Writer::new();
        w.nbt(tag);
        let mut entries = vec![(0, w.data)];
        let mut removed = Vec::new();
        items::restore_backup_using::<M>(&mut entries, &[], &mut removed, registry)?;
        let mut added = Vec::new();
        for (id, bytes) in entries {
            let mut r = Reader::new(&bytes);
            let value = match id {
                0 => nbt(&r.nbt()?)?,
                id if id == M::MAP_COLOR_OLD => hash::int(r.i32()?),
                _ => continue,
            };
            if r.remaining() != 0 {
                return Err(Error::new("Trailing restored prediction bytes"));
            }
            added.push((id, value));
        }
        added.sort_unstable();
        removed.sort_unstable();
        removed.dedup();
        self.remember((stack.item, nbt(tag)?), Patch { added, removed });
        Ok(())
    }
    pub fn observe_with<M: Maps>(&mut self, packets: &[Packet]) -> Result<()> {
        self.observe_using::<M>(packets, &crate::vfp_registry_access::Registries::default())
    }
    pub fn observe_using<M: Maps>(
        &mut self,
        packets: &[Packet],
        registry: &crate::vfp_registry_access::Registries,
    ) -> Result<()> {
        for (id, data) in packets {
            let name = usize::try_from(*id)
                .ok()
                .and_then(|i| t::NEW_CLIENTBOUND.get(i))
                .copied();
            if !matches!(
                name,
                Some(
                    "CONTAINER_SET_CONTENT"
                        | "CONTAINER_SET_SLOT"
                        | "SET_CURSOR_ITEM"
                        | "SET_PLAYER_INVENTORY"
                        | "SET_EQUIPMENT"
                        | "MERCHANT_OFFERS"
                )
            ) {
                continue;
            }
            match play::Clientbound::decode(*id, data)? {
                play::Clientbound::ContainerSetContent(p) => {
                    for s in p.items.iter().chain([&p.carried]).flatten() {
                        self.observe_stack_with::<M>(s, registry)?;
                    }
                }
                play::Clientbound::ContainerSetSlot(p) => {
                    if let Some(s) = &p.item {
                        self.observe_stack_with::<M>(s, registry)?;
                    }
                }
                play::Clientbound::SetCursorItem(p) => {
                    if let Some(s) = &p.item {
                        self.observe_stack_with::<M>(s, registry)?;
                    }
                }
                play::Clientbound::SetPlayerInventory(p) => {
                    if let Some(s) = &p.item {
                        self.observe_stack_with::<M>(s, registry)?;
                    }
                }
                play::Clientbound::SetEquipment(p) => {
                    for (_, s) in &p.slots {
                        if let Some(s) = s {
                            self.observe_stack_with::<M>(s, registry)?;
                        }
                    }
                }
                play::Clientbound::MerchantOffers(p) => {
                    for offer in &p.offers {
                        self.observe_stack_with::<M>(&offer.result, registry)?;
                    }
                }
                _ => (),
            }
        }
        Ok(())
    }
    pub fn convert_with<M: Maps>(&self, stack: &Hashed) -> Result<Hashed> {
        if stack.count <= 0 {
            return Err(Error::new("Nonpositive predicted item count"));
        }
        let old_item = M::ITEMS.inverse(stack.item).map_err(Error::new)?;
        let custom_hash = stack
            .added
            .iter()
            .find_map(|&(id, h)| (id == 0).then_some(h));
        let cached = custom_hash.and_then(|h| self.entries.get(&(stack.item, h)));
        let restore = cached.and_then(Option::as_ref);
        let mut seen = BTreeSet::new();
        let mut added = BTreeMap::new();
        let mut ambiguous = BTreeSet::new();
        for &(id, value) in &stack.added {
            if !(0..M::DATA_COMPONENT_TYPE.mapped_size).contains(&id) || !seen.insert(id) {
                return Err(Error::new("Invalid or duplicate predicted component"));
            }
            if id == 0 {
                if cached.is_none() {
                    added.insert(0, value);
                }
                continue;
            }
            let old = if id == 40 || id == 41 {
                if M::SWING_OLD < 0 {
                    continue;
                }
                M::SWING_OLD
            } else if items::stable_hash(id) {
                match M::DATA_COMPONENT_TYPE.inverse(id) {
                    Ok(old) => old,
                    Err(_) => continue,
                }
            } else {
                continue;
            };
            if let Some(previous) = added.insert(old, value) {
                if previous != value {
                    ambiguous.insert(old);
                }
            }
        }
        let mut removed = BTreeSet::new();
        for &id in &stack.removed {
            if !(0..M::DATA_COMPONENT_TYPE.mapped_size).contains(&id) || !seen.insert(id) {
                return Err(Error::new("Conflicting predicted component removal"));
            }
            if M::PROTOCOL < 770 && id == 18 {
                for name in ["hide_tooltip", "hide_additional_tooltip"] {
                    if let Some(id) = M::COMPONENT_IDENTIFIERS
                        .iter()
                        .position(|value| *value == name)
                    {
                        removed.insert(id as i32);
                    }
                }
            } else if id == 40 || id == 41 {
                if M::SWING_OLD >= 0 {
                    removed.insert(M::SWING_OLD);
                }
            } else if let Ok(old) = M::DATA_COMPONENT_TYPE.inverse(id) {
                removed.insert(old);
            }
        }
        if let Some(restore) = restore {
            added.extend(restore.added.iter().copied());
            removed.extend(restore.removed.iter().copied());
        }
        for id in &removed {
            if added.contains_key(id) {
                ambiguous.insert(*id);
            }
        }
        for id in ambiguous {
            added.remove(&id);
            removed.remove(&id);
        }
        Ok(Hashed {
            item: old_item,
            count: stack.count,
            added: added.into_iter().collect(),
            removed: removed.into_iter().collect(),
        })
    }
    pub fn click_with<M: Maps>(&self, id: i32, payload: &[u8]) -> Result<Vec<Packet>> {
        let play::Serverbound::ContainerClick(mut packet) = play::Serverbound::decode(id, payload)?
        else {
            return Err(Error::new("Expected inventory click"));
        };
        if M::PROTOCOL < 770 {
            // The pinned Horizon client deliberately sends no added-value
            // hashes. Missing component values must request ordinary server
            // resynchronization, never be written as if a hash were a value.
            fn full<M: Maps>(
                predictions: &Predictions,
                value: &Option<Hashed>,
                w: &mut Writer,
            ) -> Result<()> {
                let Some(value) = value else {
                    if M::NATIVE_ITEMS {
                        w.bool(false);
                    } else {
                        w.varint(0);
                    }
                    return Ok(());
                };
                if value.count <= 0 {
                    return Err(Error::new("Nonpositive old prediction count"));
                }
                if M::ITEMS.inverse(value.item).is_err() {
                    if M::NATIVE_ITEMS {
                        w.bool(false);
                    } else {
                        w.varint(0);
                    }
                    return Ok(());
                }
                let converted = predictions.convert_with::<M>(value)?;
                if M::NATIVE_ITEMS {
                    if converted.count > 127 {
                        return Err(Error::new(
                            "Legacy prediction count exceeds signed byte range",
                        ));
                    }
                    w.bool(true);
                    w.varint(converted.item);
                    w.i8(converted.count as i8);
                    w.nbt(&Tag::End);
                    return Ok(());
                }
                w.varint(converted.count);
                w.varint(converted.item);
                w.varint(0);
                w.varint(converted.removed.len() as i32);
                for id in converted.removed {
                    w.varint(id);
                }
                Ok(())
            }
            if packet.changed.len() > 128 {
                return Err(Error::new("Too many old changed slots"));
            }
            let mut w = Writer::new();
            w.varint(packet.container);
            w.varint(packet.state);
            w.i16(packet.slot);
            w.i8(packet.button);
            w.varint(packet.input);
            w.varint(packet.changed.len() as i32);
            for (slot, value) in &packet.changed {
                w.i16(*slot);
                full::<M>(self, value, &mut w)?;
            }
            full::<M>(self, &packet.carried, &mut w)?;
            return Ok(vec![(
                wire::packet_id(t::OLD_SERVERBOUND, "CONTAINER_CLICK").map_err(Error::new)?,
                w.data,
            )]);
        }
        for stack in packet
            .changed
            .iter_mut()
            .map(|(_, s)| s)
            .chain([&mut packet.carried])
            .flatten()
        {
            *stack = self.convert_with::<M>(stack)?;
        }
        let bytes = play::Serverbound::ContainerClick(packet).encode();
        let mut r = Reader::new(&bytes);
        r.varint()?;
        Ok(vec![(
            wire::packet_id(t::OLD_SERVERBOUND, "CONTAINER_CLICK").map_err(Error::new)?,
            r.rest().to_vec(),
        )])
    }
    pub fn observe(&mut self, packets: &[Packet]) -> Result<()> {
        self.observe_with::<V262>(packets)
    }
    pub fn convert(&self, stack: &Hashed) -> Result<Hashed> {
        self.convert_with::<V262>(stack)
    }
    pub fn click(&self, id: i32, payload: &[u8]) -> Result<Vec<Packet>> {
        self.click_with::<V262>(id, payload)
    }
}
