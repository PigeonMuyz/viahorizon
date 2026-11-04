// SPDX-License-Identifier: GPL-3.0-or-later
// Older servers do not synchronize the equipment-dependent attributes added
// in 1.21. Observe real item stacks, retaining no global equipment state.
use crate::{
    Error, Result,
    buf::{Codec, Reader, Writer},
    item::{Component, Slot},
    vfp_mapping::Maps,
    vfp_registry_access::Registries,
    vfp_tables as native, vfp_tables_121 as next,
    vfp_wire::{self as wire, Packet},
};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Equipment {
    player: Option<i32>,
    selected: i32,
    slots: BTreeMap<i32, [i32; 5]>,
    sent: Option<[i32; 5]>,
    soul: bool,
}
fn stack<M: Maps>(r: &mut Reader<'_>, registry: &Registries) -> Result<[i32; 5]> {
    let mut w = Writer::new();
    crate::vfp_items::stack_using::<M>(r, &mut w, true, false, registry)?;
    let mut levels = [0; 5];
    if let Some(stack) = Slot::read(&mut Reader::new(&w.data))? {
        if let Some(Component::Enchantments(values)) = stack.component(13) {
            for (id, level) in values {
                if let Some(index) = [20, 11, 12, 6, 8].iter().position(|value| value == id) {
                    levels[index] = (*level).max(0);
                }
            }
        }
    }
    Ok(levels)
}
impl Equipment {
    pub fn soul(&mut self, active: bool) {
        self.soul = active;
    }
    fn set(&mut self, slot: i32, levels: [i32; 5]) {
        self.slots.insert(slot, levels);
    }
    pub fn observe<M: Maps>(
        &mut self,
        name: &str,
        data: &[u8],
        registry: &Registries,
    ) -> Result<Option<Packet>> {
        let mut r = Reader::new(data);
        let mut force = false;
        match name {
            "LOGIN" => {
                self.player = Some(r.i32()?);
                self.sent = None;
                self.soul = false;
            }
            "RESPAWN" => {
                force = true;
                self.soul = false;
            }
            "CONTAINER_SET_CONTENT" => {
                let container = r.u8()?;
                r.varint()?;
                for slot in 0..r.count(65536)? {
                    let levels = stack::<M>(&mut r, registry)?;
                    if container == 0 {
                        self.set(slot as i32, levels);
                    }
                }
                stack::<M>(&mut r, registry)?;
            }
            "CONTAINER_SET_SLOT" => {
                let container = r.i8()?;
                r.varint()?;
                let slot = i32::from(r.i16()?);
                let levels = stack::<M>(&mut r, registry)?;
                if container == 0 {
                    self.set(slot, levels);
                } else if container == -2 {
                    let slot = match slot {
                        0..=8 => 36 + slot,
                        36 => 8,
                        37 => 7,
                        38 => 6,
                        39 => 5,
                        _ => slot,
                    };
                    self.set(slot, levels);
                }
            }
            "SET_CARRIED_ITEM" => self.selected = i32::from(r.i8()?),
            _ => (),
        }
        let head = self.slots.get(&5).copied().unwrap_or([0; 5]);
        let legs = self.slots.get(&7).copied().unwrap_or([0; 5]);
        let feet = self.slots.get(&8).copied().unwrap_or([0; 5]);
        let hand = self
            .slots
            .get(&(36 + self.selected))
            .copied()
            .unwrap_or([0; 5]);
        let levels = [hand[0], i32::from(self.soul), legs[2], head[3], feet[4]];
        let Some(player) = self.player else {
            return Ok(None);
        };
        if !force && self.sent == Some(levels) {
            return Ok(None);
        }
        let previous = self.sent;
        self.sent = Some(levels);
        let definitions = [
            (
                19,
                0.0,
                "enchantment.efficiency/mainhand",
                if levels[0] > 0 {
                    f64::from(levels[0]).powi(2) + 1.0
                } else {
                    0.0
                },
                0u8,
            ),
            (20, 0.0, "enchantment.soul_speed", 1.0, 0),
            (
                25,
                0.3,
                "enchantment.swift_sneak/legs",
                f64::from(levels[2]) * 0.15,
                0,
            ),
            (
                28,
                0.2,
                "enchantment.aqua_affinity/head",
                f64::from(levels[3]) * 4.0,
                2,
            ),
            (
                30,
                0.0,
                "enchantment.depth_strider/feet",
                f64::from(levels[4]) / 3.0,
                0,
            ),
        ];
        let changed: Vec<_> = (0..5)
            .filter(|i| force || previous.is_none_or(|before| before[*i] != levels[*i]))
            .collect();
        let mut w = Writer::new();
        w.varint(player);
        w.varint(changed.len() as i32);
        for i in changed {
            let (id, base, name, amount, operation) = definitions[i];
            w.varint(next::ATTRIBUTES.map(id).map_err(Error::new)?);
            w.f64(base);
            w.varint(i32::from(levels[i] > 0));
            if levels[i] > 0 {
                w.string(&format!("minecraft:{name}"));
                w.f64(amount);
                w.u8(operation);
            }
        }
        Ok(Some((
            wire::packet_id(native::NEW_CLIENTBOUND, "UPDATE_ATTRIBUTES").map_err(Error::new)?,
            w.data,
        )))
    }
}
