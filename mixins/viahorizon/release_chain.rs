// SPDX-License-Identifier: GPL-3.0-or-later
// Pre-1.19.3 conversion has an explicit canonical boundary at protocol 761.
// Catalog/data coverage is independent of the public gameplay capability gate.
use crate::{
    Error, Result,
    vfp_wire::{self as wire, IdMap, Packet},
};
use std::sync::Arc;

pub struct Registry {
    pub name: &'static str,
    pub map: IdMap,
}
pub struct RegistryNames {
    pub name: &'static str,
    pub values: &'static [&'static str],
}
pub struct Step {
    pub source: i32,
    pub target: i32,
    pub version: &'static str,
    pub clientbound: &'static [&'static str],
    pub next_clientbound: &'static [&'static str],
    pub serverbound: &'static [&'static str],
    pub next_serverbound: &'static [&'static str],
    pub registries: &'static [Registry],
}
impl Step {
    pub fn registry(&self, name: &str) -> Option<IdMap> {
        self.registries
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.map)
    }
    pub fn map(&self, registry: &str, id: i32) -> Result<i32> {
        self.registry(registry)
            .ok_or_else(|| {
                Error::new(format!(
                    "{} needs an explicit {registry} conversion",
                    self.version
                ))
            })?
            .map(id)
            .map_err(Error::new)
    }
    pub fn inverse(&self, registry: &str, id: i32) -> Result<i32> {
        self.registry(registry)
            .ok_or_else(|| {
                Error::new(format!(
                    "{} needs an explicit inverse {registry} conversion",
                    self.version
                ))
            })?
            .inverse(id)
            .map_err(Error::new)
    }
    // Used only by a codec whose registry schema is known to be unchanged at
    // this edge. A missing mapping is not a blanket forwarding permission.
    pub fn same_or_map(&self, registry: &str, id: i32, inverse: bool) -> Result<i32> {
        if id < 0 {
            return Err(Error::new("Negative legacy registry ID"));
        }
        match self.registry(registry) {
            Some(map) => (if inverse {
                map.inverse(id)
            } else {
                map.map(id)
            })
            .map_err(Error::new),
            None => {
                let before = identifiers(self.source, registry);
                let after = identifiers(self.target, registry);
                match (before, after) {
                    (Some(before), Some(after)) if before == after => {
                        if id as usize >= before.len() {
                            return Err(Error::new("Legacy registry ID exceeds identifier table"));
                        }
                        Ok(id)
                    }
                    _ => Err(Error::new(format!(
                        "Unresolved {} registry at {} -> {}",
                        registry, self.source, self.target
                    ))),
                }
            }
        }
    }
    pub fn incoming(&self, name: &str, data: Vec<u8>) -> Result<Packet> {
        Ok((
            wire::packet_id(self.next_clientbound, name).map_err(Error::new)?,
            data,
        ))
    }
    pub fn outgoing(&self, name: &str, data: Vec<u8>) -> Result<Packet> {
        Ok((
            wire::packet_id(self.serverbound, name).map_err(Error::new)?,
            data,
        ))
    }
    pub fn incoming_name(&self, id: i32) -> Result<&'static str> {
        packet_name(self.clientbound, id)
    }
    pub fn outgoing_name(&self, id: i32) -> Result<&'static str> {
        packet_name(self.next_serverbound, id)
    }
}
pub fn packet_name(table: &'static [&'static str], id: i32) -> Result<&'static str> {
    usize::try_from(id)
        .ok()
        .and_then(|id| table.get(id))
        .copied()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| Error::new(format!("Unknown legacy Release packet {id}")))
}
pub fn step(protocol: i32) -> Option<&'static Step> {
    crate::vfp_release_chain_data::STEPS
        .iter()
        .find(|step| step.source == protocol)
}
pub fn identifiers(protocol: i32, registry: &str) -> Option<&'static [&'static str]> {
    crate::vfp_release_chain_data::IDENTIFIERS
        .iter()
        .find(|(id, _)| *id == protocol)?
        .1
        .iter()
        .find(|names| names.name == registry)
        .map(|names| names.values)
}
pub fn identifier(protocol: i32, registry: &str, id: i32) -> Result<&'static str> {
    usize::try_from(id)
        .ok()
        .and_then(|id| identifiers(protocol, registry)?.get(id))
        .copied()
        .ok_or_else(|| Error::new(format!("Unknown {registry} ID {id} in protocol {protocol}")))
}
pub fn named_id(protocol: i32, registry: &str, name: &str) -> Result<i32> {
    identifiers(protocol, registry)
        .and_then(|names| names.iter().position(|&value| value == name))
        .map(|id| id as i32)
        .ok_or_else(|| {
            Error::new(format!(
                "Unknown {registry} name {name} in protocol {protocol}"
            ))
        })
}
pub fn air_states(protocol: i32) -> &'static [i32] {
    crate::vfp_release_chain_data::AIR_STATES
        .iter()
        .find(|(id, _)| *id == protocol)
        .map_or(&[0], |(_, states)| *states)
}

pub struct Chain {
    states: Vec<crate::vfp_release_older::Adapter>,
}
impl Chain {
    pub fn username(&mut self, name: &str) {
        for state in &mut self.states {
            state.username(name);
        }
    }
    pub fn new(
        protocol: i32,
        identity: Option<Arc<crate::vfp_chat_signing::Identity>>,
    ) -> Result<Self> {
        let data = crate::vfp_release_chain_data::STEPS;
        let first = data
            .iter()
            .position(|step| step.source == protocol)
            .ok_or_else(|| Error::new("No Netty Release chain for this protocol"))?;
        let mut states = Vec::new();
        let mut expected = protocol;
        for step in &data[first..] {
            if step.source != expected {
                return Err(Error::new("Discontinuous legacy Release chain"));
            }
            states.push(crate::vfp_release_older::Adapter::new(
                step,
                protocol,
                identity.clone(),
            ));
            expected = step.target;
        }
        if expected != 761 {
            return Err(Error::new("Legacy chain does not terminate at 1.19.3"));
        }
        Ok(Self { states })
    }
    pub fn clientbound(&mut self, id: i32, payload: &[u8]) -> Result<Vec<Packet>> {
        let mut packets = vec![(id, payload.to_vec())];
        for state in &mut self.states {
            let mut next = Vec::new();
            for (id, data) in packets {
                next.extend(Self::translate_clientbound(state, id, &data)?);
            }
            packets = next;
        }
        Ok(packets)
    }
    pub fn serverbound(&mut self, mut packets: Vec<Packet>) -> Result<Vec<Packet>> {
        let needs_held = self
            .states
            .first()
            .is_some_and(|state| state.step().source <= 47);
        let mut held = None;
        for state in self.states.iter_mut().rev() {
            let step = state.step();
            // The transaction bridge can resolve a selected stack that has
            // moved in a click the server has yet to acknowledge. Carry that
            // value down the same item registry boundaries as the use packet.
            if let Some(stack) = &mut held {
                if (step.source >= 393 || matches!(step.source, 340 | 316 | 210))
                    && crate::vfp_release_rewrite::stack(step, true, stack).is_err()
                {
                    held = None;
                }
            }
            packets = state.serverbound(packets, held.as_ref()).map_err(|error| {
                Error::new(format!(
                    "Release serverbound {} to {}: {error}",
                    step.target, step.source
                ))
            })?;
            if needs_held
                && step.source == 754
                && packets.iter().any(|(id, _)| {
                    matches!(
                        packet_name(step.serverbound, *id),
                        Ok("USE_ITEM" | "USE_ITEM_ON")
                    )
                })
            {
                held = state.held();
            }
            if step.source == 47 {
                held = None;
            }
        }
        Ok(packets)
    }
    pub fn take_local(&mut self) -> Result<Vec<Packet>> {
        let mut result = Vec::new();
        for index in 0..self.states.len() {
            let mut packets = self.states[index].take_local();
            for state in &mut self.states[index + 1..] {
                let mut next = Vec::new();
                for (id, data) in packets {
                    next.extend(Self::translate_clientbound(state, id, &data)?);
                }
                packets = next;
            }
            result.extend(packets);
        }
        Ok(result)
    }

    fn translate_clientbound(
        state: &mut crate::vfp_release_older::Adapter,
        id: i32,
        data: &[u8],
    ) -> Result<Vec<Packet>> {
        let step = state.step();
        state.clientbound(id, data).map_err(|error| {
            let name = packet_name(step.clientbound, id).unwrap_or("unknown");
            Error::new(format!(
                "Release clientbound {} to {} packet {name} ({id:#x}): {error}",
                step.source, step.target
            ))
        })
    }
}

pub fn native_item_supported(protocol: i32, native: i32) -> bool {
    use std::sync::{Arc, Mutex, OnceLock};
    static TABLES: OnceLock<Mutex<std::collections::BTreeMap<i32, Arc<Vec<bool>>>>> =
        OnceLock::new();
    let tables = TABLES.get_or_init(Default::default);
    let cached = tables
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&protocol)
        .cloned();
    let table = cached.unwrap_or_else(|| {
        let values = Arc::new(
            (0..crate::vfp_tables::ITEMS.mapped_size)
                .map(|id| native_item_supported_uncached(protocol, id))
                .collect::<Vec<_>>(),
        );
        tables
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(protocol)
            .or_insert(values)
            .clone()
    });
    usize::try_from(native)
        .ok()
        .and_then(|id| table.get(id))
        .copied()
        .unwrap_or(false)
}
fn native_item_supported_uncached(protocol: i32, native: i32) -> bool {
    use crate::vfp_mapping::*;
    let map = match protocol {
        4..=761 => V1193::ITEMS,
        762 => V1194::ITEMS,
        763 => V120::ITEMS,
        764 => V1202::ITEMS,
        765 => V1203::ITEMS,
        766 => V1205::ITEMS,
        767 => V121::ITEMS,
        768 => V1212::ITEMS,
        769 => V1214::ITEMS,
        770 => V1215::ITEMS,
        771 => V1216::ITEMS,
        772 => V1217::ITEMS,
        773 => V1219::ITEMS,
        774 => V12111::ITEMS,
        775 => V261::ITEMS,
        776 => V262::ITEMS,
        777 => return (0..V262::ITEMS.mapped_size).contains(&native),
        _ => return false,
    };
    let Ok(mut item) = map.inverse(native) else {
        return false;
    };
    // Before flattening, many item tables were numerically identical even when
    // a release added IDs. Exact inverse maps alone cannot prove availability.
    if protocol < 393 {
        let Ok(source) = V1193::ITEMS.inverse(native) else {
            return false;
        };
        let Some(name) = crate::vfp_tables_1193::ITEM_IDENTIFIERS.get(source as usize) else {
            return false;
        };
        if let Some(entity) = name.strip_suffix("_spawn_egg") {
            let since = match entity {
                "bat" | "blaze" | "cave_spider" | "chicken" | "cow" | "creeper" | "enderman"
                | "ghast" | "horse" | "magma_cube" | "mooshroom" | "ocelot" | "pig" | "sheep"
                | "silverfish" | "skeleton" | "slime" | "spider" | "squid" | "villager"
                | "witch" | "wolf" | "zombie" | "zombified_piglin" => 4,
                "endermite" | "guardian" | "rabbit" => 47,
                "shulker" => 107,
                "polar_bear" | "husk" | "stray" => 210,
                "donkey" | "mule" | "elder_guardian" | "evoker" | "llama" | "skeleton_horse"
                | "vex" | "vindicator" | "wither_skeleton" | "zombie_horse" | "zombie_villager" => {
                    315
                }
                "parrot" => 335,
                _ => return false,
            };
            if protocol < since {
                return false;
            }
        }
    }
    let mut damage = 0;
    if protocol < 761 {
        for step in crate::vfp_release_chain_data::STEPS.iter().rev() {
            if step.source < protocol {
                break;
            }
            if let Some(map) = step.registry("items") {
                let Ok(previous) = map.inverse(item) else {
                    return false;
                };
                item = previous;
            }
            if step.source == 340 {
                if item >> 16 == 383 {
                    damage = (item & 65535) as i16;
                    item = 383;
                } else {
                    damage = (item & 15) as i16;
                    item >>= 4;
                }
            }
        }
    }
    if protocol < 393
        && (protocol < 107 && matches!(item, 198..=212 | 426 | 432..=448)
            || protocol < 210 && matches!(item, 213..=217 | 255)
            || protocol < 315 && matches!(item, 218..=234 | 449 | 450)
            || protocol < 316 && item == 452
            || protocol < 335 && matches!(item, 235..=252 | 453))
    {
        return false;
    }
    protocol > 5 || !crate::vfp_release_17::unsupported_item(item, damage)
}
