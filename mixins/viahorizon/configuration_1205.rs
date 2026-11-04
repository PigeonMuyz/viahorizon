// SPDX-License-Identifier: GPL-3.0-or-later
// Built-in registries were not sent by 1.20.5/1.20.6. Their original order
// matters: item enchantment ids and painting metadata refer to that order.
use crate::{
    Error, Result, packets::configuration::RegistryEntry, vfp_registry_1205 as data,
    vfp_registry_access::Registries,
};
use horizon_world::nbt::Tag;
fn resource(bytes: &[u8]) -> Result<Tag> {
    let (root, used) = horizon_world::nbt::read_named(
        bytes,
        horizon_world::nbt::Limits {
            max_bytes: bytes.len(),
            max_nodes: 500000,
            max_depth: 64,
        },
    )
    .map_err(|error| Error::new(error.to_string()))?;
    if used != bytes.len() {
        return Err(Error::new("Trailing legacy registry resource"));
    }
    Ok(root.tag)
}
pub fn defaults(registry: &Registries) -> Result<Vec<(String, Vec<RegistryEntry>)>> {
    let paintings = data::PAINTINGS
        .iter()
        .map(|(width, height, name)| RegistryEntry {
            id: format!("minecraft:{name}"),
            data: Some(Tag::compound([
                ("width", Tag::Int(*width)),
                ("height", Tag::Int(*height)),
                ("asset_id", Tag::String((*name).into())),
            ])),
        })
        .collect();
    let root = resource(data::ENCHANTMENTS)?;
    let Some(Tag::List(enchantments)) = root.get("entries") else {
        return Err(Error::new("Missing legacy enchantment registry"));
    };
    if enchantments.len() != data::ENCHANTMENT_NAMES.len() {
        return Err(Error::new("Legacy enchantment registry order mismatch"));
    }
    let enchantments = data::ENCHANTMENT_NAMES
        .iter()
        .zip(enchantments)
        .map(|(name, tag)| RegistryEntry {
            id: format!("minecraft:{name}"),
            data: Some(tag.clone()),
        })
        .collect();
    let root = resource(data::JUKEBOX_SONGS)?;
    let mut songs = Vec::new();
    for name in data::JUKEBOX_ORDER {
        let tag = root
            .get(name)
            .ok_or_else(|| Error::new("Missing legacy jukebox entry"))?;
        songs.push(RegistryEntry {
            id: if name.contains(':') {
                (*name).to_owned()
            } else {
                format!("minecraft:{name}")
            },
            data: Some(tag.clone()),
        });
    }
    let mut out = vec![
        ("painting_variant".into(), paintings),
        ("enchantment".into(), enchantments),
        ("jukebox_song".into(), songs),
    ];
    if !registry.contains("wolf_variant") {
        out.push((
            "wolf_variant".into(),
            vec![RegistryEntry {
                id: "minecraft:pale".into(),
                data: Some(Tag::compound([(
                    "assets",
                    Tag::compound([
                        ("wild", Tag::String("minecraft:entity/wolf/wolf".into())),
                        (
                            "tame",
                            Tag::String("minecraft:entity/wolf/wolf_tame".into()),
                        ),
                        (
                            "angry",
                            Tag::String("minecraft:entity/wolf/wolf_angry".into()),
                        ),
                    ]),
                )])),
            }],
        ));
    }
    Ok(out)
}
