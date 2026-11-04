// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    Error, Result, buf::Reader, packets::configuration as cfg, vfp_tables as t, vfp_wire as wire,
};
use horizon_world::nbt::{JavaString, Tag};

pub fn clientbound_262(id: i32, payload: &[u8]) -> Result<wire::Packet> {
    let name = usize::try_from(id)
        .ok()
        .and_then(|i| t::OLD_CONFIGURATION.get(i))
        .copied()
        .ok_or_else(|| Error::new("Unknown 26.2 configuration packet"))?;
    if name == "FINISH_CONFIGURATION" {
        if !payload.is_empty() {
            return Err(Error::new("Trailing finish-configuration data"));
        }
        return Err(Error::new(
            "ViaHorizon: 26.2 configuration preview reached the world-entry boundary. Remaining registry schemas and gameplay conversions are not validated; world entry is disabled.",
        ));
    }
    let mapped = wire::packet_id(t::NEW_CONFIGURATION, name).map_err(Error::new)?;
    let mut packet = cfg::Clientbound::decode(mapped, payload)?;
    match &mut packet {
        cfg::Clientbound::UpdateTags(update) => {
            for registry in &mut update.registries {
                let name = registry
                    .registry
                    .strip_prefix("minecraft:")
                    .unwrap_or(&registry.registry);
                let mapping = match name {
                    "block" => Some(t::BLOCKS),
                    "item" => Some(t::ITEMS),
                    "entity_type" => Some(t::ENTITIES),
                    _ => None,
                };
                for tag in &mut registry.tags {
                    if name == "block" && tag.name == "minecraft:convertable_to_mud" {
                        tag.name = "minecraft:convertible_to_mud".into();
                    }
                    for id in &mut tag.ids {
                        if *id < 0 {
                            return Err(Error::new("Negative tag registry ID"));
                        }
                        if let Some(mapping) = mapping {
                            *id = mapping.map(*id).map_err(Error::new)?;
                        }
                    }
                }
            }
        }
        cfg::Clientbound::RegistryData(registry) => {
            for entry in &mut registry.entries {
                if let Some(data) = &mut entry.data {
                    crate::vfp_registry::upgrade_registry(&registry.registry, data)?;
                }
            }
            // Horizon stores unconsumed dynamic records as opaque NBT. Their
            // arrival order/IDs must remain unchanged; no known-pack data from
            // 26.3 may be substituted for the old server's records.
            if registry
                .registry
                .strip_prefix("minecraft:")
                .unwrap_or(&registry.registry)
                == "trim_material"
            {
                for entry in &mut registry.entries {
                    if let Some(Tag::Compound(data)) = &mut entry.data {
                        if let Some(asset) = data.get(&JavaString::from("asset_name")) {
                            let Tag::String(asset) = asset else {
                                return Err(Error::new("Invalid trim material asset_name"));
                            };
                            let asset = asset.to_string_lossy();
                            data.insert(
                                JavaString::from("palette_id"),
                                Tag::String(JavaString::from(
                                    format!("minecraft:trim/{asset}").as_str(),
                                )),
                            );
                            data.remove(&JavaString::from("asset_name"));
                        }
                    }
                }
            }
        }
        _ => (),
    }
    let encoded = packet.encode();
    let mut reader = Reader::new(&encoded);
    let id = reader.varint()?;
    Ok((id, reader.rest().to_vec()))
}

pub fn missing_registries_262() -> Result<Vec<wire::Packet>> {
    use crate::packets::configuration::{RegistryData, RegistryEntry};
    static CACHE: std::sync::OnceLock<Result<Vec<wire::Packet>>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let patterns = [
                "angler",
                "archer",
                "arms_up",
                "blade",
                "brewer",
                "burn",
                "danger",
                "explorer",
                "flow",
                "friend",
                "guster",
                "heart",
                "heartbreak",
                "howl",
                "miner",
                "mourner",
                "plenty",
                "prize",
                "scrape",
                "sheaf",
                "shelter",
                "skull",
                "snort",
            ];
            let pots = RegistryData {
                registry: "minecraft:decorated_pot_pattern".into(),
                entries: patterns
                    .into_iter()
                    .map(|name| RegistryEntry {
                        id: format!("minecraft:{name}"),
                        data: Some(Tag::compound([(
                            "asset_id",
                            Tag::String(
                                format!("minecraft:{name}_pottery_pattern").as_str().into(),
                            ),
                        )])),
                    })
                    .collect(),
            };
            let bytes = crate::vfp_registry_data::BLOCK_TRANSFORMERS;
            let (root, used) = horizon_world::nbt::read_named(
                bytes,
                horizon_world::nbt::Limits {
                    max_bytes: bytes.len(),
                    max_nodes: 100000,
                    max_depth: 64,
                },
            )
            .map_err(|e| Error::new(e.to_string()))?;
            if used != bytes.len() {
                return Err(Error::new("Trailing bundled registry data"));
            }
            let Tag::Compound(entries) = root.tag else {
                return Err(Error::new("Invalid block transformer registry"));
            };
            let transforms = RegistryData {
                registry: "minecraft:block_transformer".into(),
                entries: entries
                    .into_iter()
                    .map(|(name, data)| RegistryEntry {
                        id: name.to_string_lossy(),
                        data: Some(data),
                    })
                    .collect(),
            };
            let mut packets = Vec::new();
            for data in [pots, transforms] {
                let encoded = cfg::Clientbound::RegistryData(data).encode();
                let mut r = Reader::new(&encoded);
                let id = r.varint()?;
                packets.push((id, r.rest().to_vec()));
            }
            Ok(packets)
        })
        .clone()
}

pub fn serverbound_262(body: &[u8]) -> Result<Vec<u8>> {
    let mut reader = Reader::new(body);
    let id = reader.varint()?;
    let packet = cfg::Serverbound::decode(id, reader.rest())?;
    match packet {
        cfg::Serverbound::FinishConfiguration(_) => Err(Error::new(
            "World entry is disabled until the complete 26.2 translator is validated",
        )),
        cfg::Serverbound::SelectKnownPacks(_) => Ok(cfg::Serverbound::SelectKnownPacks(
            cfg::SelectKnownPacks { packs: Vec::new() },
        )
        .encode()),
        other => Ok(other.encode()),
    }
}
