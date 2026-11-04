use crate::vfp_mapping::{Maps, V262};
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    Error, Result,
    buf::{Codec, Reader, Writer},
    chunk::ChunkData,
    packets::play::BlockEntityData,
    vfp_tables as tables, vfp_wire as wire,
};
use horizon_world::nbt::{JavaString, Tag};
pub(crate) fn obsolete_block_entity<M: Maps>(
    kind: i32,
    _registry: &crate::vfp_registry_access::Registries,
) -> bool {
    M::BED_OLD == Some(kind)
} // Bed block entities were removed in 26.2.

// 26.2 uses an ordered list of sherd item names. 26.3 uses four named
// decoration records. Build the replacement before changing the input tag.
pub fn upgrade_block_entity(tag: &mut Tag) -> Result<()> {
    let Some(sherds) = tag.get("sherds") else {
        return Ok(());
    };
    if matches!(sherds, Tag::Compound(_)) {
        return Ok(());
    }
    let Tag::List(list) = sherds else {
        return Err(Error::new("Invalid decorated-pot sherd list"));
    };
    if list.len() > 4 || list.iter().any(|v| !matches!(v, Tag::String(_))) {
        return Err(Error::new("Invalid decorated-pot sherd entries"));
    }
    let names = ["back", "left", "right", "front"];
    let entries = names.iter().enumerate().map(|(index, &side)| {
        let item = list
            .get(index)
            .cloned()
            .unwrap_or_else(|| Tag::String(JavaString::from("minecraft:brick")));
        (side, Tag::compound([("id", item)]))
    });
    let upgraded = Tag::compound(entries);
    *tag.get_mut("sherds").expect("validated key") = upgraded;
    Ok(())
}
pub fn downgrade_block_entity(tag: &mut Tag) -> Result<()> {
    let Some(Tag::Compound(decorations)) = tag.get("sherds") else {
        return Ok(());
    };
    let mut sherds = Vec::with_capacity(4);
    for side in ["back", "left", "right", "front"] {
        let id = match decorations.get(&JavaString::from(side)) {
            None => Tag::String(JavaString::from("minecraft:brick")),
            Some(Tag::Compound(item)) if item.len() == 1 => match item.get(&JavaString::from("id"))
            {
                Some(Tag::String(id)) => Tag::String(id.clone()),
                _ => return Err(Error::new("Invalid pot decoration item identifier")),
            },
            _ => {
                return Err(Error::new(
                    "26.2 pot decorations cannot contain patched templates",
                ));
            }
        };
        sherds.push(id);
    }
    *tag.get_mut("sherds").expect("validated sherd compound") = Tag::List(sherds);
    Ok(())
}
pub fn legacy_profile(tag: &mut Tag) -> Result<()> {
    let Tag::Compound(fields) = tag else {
        return Ok(());
    };
    let Some(owner) = fields.remove(&JavaString::from("SkullOwner")) else {
        return Ok(());
    };
    let mut profile = std::collections::BTreeMap::new();
    match owner {
        Tag::String(name) => {
            profile.insert("name".into(), Tag::String(name));
        }
        Tag::Compound(mut owner) => {
            if let Some(Tag::String(name)) = owner.remove(&JavaString::from("Name")) {
                let value = name.to_string_lossy();
                if value.len() <= 16 && value.bytes().all(|byte| (b'!'..=b'~').contains(&byte)) {
                    profile.insert("name".into(), Tag::String(name));
                }
            }
            if let Some(Tag::IntArray(id)) = owner.remove(&JavaString::from("Id")) {
                if id.len() == 4 {
                    profile.insert("id".into(), Tag::IntArray(id));
                }
            }
            if let Some(Tag::Compound(properties)) = owner.remove(&JavaString::from("Properties")) {
                let mut values = Vec::new();
                for (name, list) in properties {
                    if let Tag::List(list) = list {
                        for value in list {
                            let Some(Tag::String(text)) = value.get("Value") else {
                                continue;
                            };
                            let mut property = std::collections::BTreeMap::new();
                            property.insert("name".into(), Tag::String(name.clone()));
                            property.insert("value".into(), Tag::String(text.clone()));
                            if let Some(Tag::String(signature)) = value.get("Signature") {
                                property.insert("signature".into(), Tag::String(signature.clone()));
                            }
                            values.push(Tag::Compound(property));
                        }
                    }
                }
                profile.insert("properties".into(), Tag::List(values));
            }
        }
        _ => return Ok(()),
    }
    fields.insert("profile".into(), Tag::Compound(profile));
    Ok(())
}
pub fn upgrade_block_entity_using<M: Maps>(kind: i32, tag: &mut Tag) -> Result<()> {
    let name = M::BLOCK_ENTITY_IDENTIFIERS
        .get(kind as usize)
        .copied()
        .unwrap_or("");
    if let Some(from) = M::DATA_VERSION {
        if M::PROTOCOL < 766 && matches!(tag, Tag::End) {
            *tag = Tag::compound([] as [(&str, Tag); 0]);
        }
        if !name.is_empty() {
            if let Tag::Compound(fields) = tag {
                fields.insert(
                    "id".into(),
                    Tag::String(format!("minecraft:{name}").as_str().into()),
                );
            }
        }
        horizon_world::datafix::update(
            horizon_world::datafix::schema::Ty::BlockEntity,
            tag,
            from,
            horizon_world::datafix::CURRENT,
        );
        return upgrade_block_entity(tag);
    }
    upgrade_block_entity(tag)?;
    if M::PROTOCOL < 766 && name == "skull" {
        legacy_profile(tag)?;
    }
    if M::PROTOCOL >= 770 {
        return Ok(());
    }
    if let Some(Tag::String(value)) = tag.get_mut("CustomName") {
        let mut text = crate::vfp_text_1214::json_text(&value.to_string_lossy(), 0)?;
        crate::vfp_play::text_version(&mut text, M::PROTOCOL)?;
        *tag.get_mut("CustomName").expect("known custom name") = text;
    }
    if !matches!(name, "sign" | "hanging_sign") {
        return Ok(());
    }
    for side in ["front_text", "back_text"] {
        if let Some(text) = tag.get_mut(side) {
            for key in ["messages", "filtered_messages"] {
                if let Some(Tag::List(messages)) = text.get_mut(key) {
                    for message in messages {
                        if let Tag::String(value) = message {
                            let mut component =
                                crate::vfp_text_1214::json_text(&value.to_string_lossy(), 0)?;
                            crate::vfp_play::text_version(&mut component, M::PROTOCOL)?;
                            *message = match component {
                                Tag::Compound(_) => component,
                                Tag::String(value) => Tag::compound([("text", Tag::String(value))]),
                                other => Tag::compound([
                                    ("text", Tag::String("".into())),
                                    ("extra", Tag::List(vec![other])),
                                ]),
                            };
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn clientbound_using<M: Maps>(
    id: i32,
    payload: &[u8],
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<Vec<wire::Packet>> {
    let name = usize::try_from(id)
        .ok()
        .and_then(|i| tables::OLD_CLIENTBOUND.get(i))
        .copied()
        .ok_or_else(|| Error::new("Invalid world packet ID"))?;
    let mut reader = Reader::new(payload);
    let mut writer = Writer::new();
    match name {
        "BLOCK_ENTITY_DATA" => {
            let mut packet = BlockEntityData::read(&mut reader)?;
            if packet.kind < 0 {
                return Err(Error::new("Negative block entity type"));
            }
            if obsolete_block_entity::<M>(packet.kind, _registry) {
                if reader.remaining() != 0 {
                    return Err(Error::new("Trailing obsolete block entity bytes"));
                }
                return Ok(Vec::new());
            }
            upgrade_block_entity_using::<M>(packet.kind, &mut packet.tag)?;
            packet.kind = M::BLOCKENTITIES.map(packet.kind).map_err(Error::new)?;
            BlockEntityData::write(&packet, &mut writer);
        }
        "LEVEL_CHUNK_WITH_LIGHT" => {
            let x = reader.i32()?;
            let z = reader.i32()?;
            let mut chunk = if M::PROTOCOL < 770 {
                crate::vfp_world_1214::chunk_using::<M>(&mut reader)?
            } else {
                ChunkData::read(&mut reader)?
            };
            chunk.buffer = wire::chunk_sections_in_dimension::<M>(
                &chunk.buffer,
                _registry.entry_count("worldgen/biome"),
                _registry.active_sections(),
            )
            .map_err(Error::new)?;
            let mut entities = Vec::with_capacity(chunk.block_entities.len());
            for mut entity in std::mem::take(&mut chunk.block_entities) {
                if entity.kind < 0 {
                    return Err(Error::new("Negative chunk block entity type"));
                }
                if obsolete_block_entity::<M>(entity.kind, _registry) {
                    continue;
                }
                upgrade_block_entity_using::<M>(entity.kind, &mut entity.tag)?;
                entity.kind = M::BLOCKENTITIES.map(entity.kind).map_err(Error::new)?;
                entities.push(entity);
            }
            chunk.block_entities = entities;
            let light = wire::light_data(reader.rest()).map_err(Error::new)?;
            writer.i32(x);
            writer.i32(z);
            ChunkData::write(&chunk, &mut writer);
            writer.bytes(&light);
        }
        _ => {
            return crate::entity_data::vfp_entities::clientbound_using::<M>(
                id, payload, _registry,
            );
        }
    }
    if reader.remaining() != 0 {
        return Err(Error::new("Trailing world packet bytes"));
    }
    Ok(vec![(
        wire::packet_id(tables::NEW_CLIENTBOUND, name).map_err(Error::new)?,
        writer.data,
    )])
}

pub fn clientbound_262(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    clientbound_using::<V262>(
        id,
        payload,
        &crate::vfp_registry_access::Registries::default(),
    )
}

pub fn clientbound_with<M: Maps>(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    clientbound_using::<M>(
        id,
        payload,
        &crate::vfp_registry_access::Registries::default(),
    )
}
