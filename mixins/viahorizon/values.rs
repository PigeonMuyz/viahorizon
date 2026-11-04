use crate::vfp_mapping::{Maps, V262};
// SPDX-License-Identifier: GPL-3.0-or-later
// Composite item values. All static registry references pass through the same
// forward/reverse maps; dynamic registry ids retain the server's order.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_wire::IdMap,
};
use horizon_world::nbt::{JavaString, Tag};
fn identifier(names: &[&str], tag: &Tag, default: &str) -> usize {
    let name = match tag.get("id") {
        Some(Tag::String(name)) => name.to_string_lossy(),
        _ => default.to_owned(),
    };
    let name = name.strip_prefix("minecraft:").unwrap_or(&name);
    names
        .iter()
        .position(|candidate| *candidate == name)
        .unwrap_or(0)
}
fn typed_entity<M: Maps>(
    kind: i32,
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
) -> Result<()> {
    let names = if kind == 60 {
        M::ENTITY_IDENTIFIERS
    } else {
        M::BLOCK_ENTITY_IDENTIFIERS
    };
    let map = if kind == 60 {
        M::ENTITIES
    } else {
        M::BLOCKENTITIES
    };
    if forward {
        let mut tag = r.nbt()?;
        let Tag::Compound(_) = tag else {
            return Err(Error::new("Old entity data is not a compound"));
        };
        let id = identifier(names, &tag, if kind == 60 { "pig" } else { "furnace" }) as i32;
        if kind == 62 && M::BED_OLD == Some(id) {
            return Ok(());
        }
        w.varint(map.map(id).map_err(Error::new)?);
        if kind == 62 {
            crate::vfp_world::upgrade_block_entity_using::<M>(id, &mut tag)?;
        } else if let Some(from) = M::DATA_VERSION {
            if let Tag::Compound(fields) = &mut tag {
                fields.entry(JavaString::from("id")).or_insert_with(|| {
                    Tag::String(format!("minecraft:{}", names[id as usize]).as_str().into())
                });
            }
            horizon_world::datafix::update(
                horizon_world::datafix::schema::Ty::Entity,
                &mut tag,
                from,
                horizon_world::datafix::CURRENT,
            );
        }
        w.nbt(&tag);
    } else {
        let id = r.varint()?;
        let mut tag = r.nbt()?;
        let Some(old) = map.inverse(id).ok().and_then(|old| names.get(old as usize)) else {
            return Ok(());
        };
        if kind == 62 {
            crate::vfp_world::downgrade_block_entity(&mut tag)?;
        }
        let Tag::Compound(data) = &mut tag else {
            return Err(Error::new("Entity data is not a compound"));
        };
        data.insert(
            JavaString::from("id"),
            Tag::String(format!("minecraft:{old}").as_str().into()),
        );
        w.nbt(&tag);
    }
    Ok(())
}
fn map(id: i32, table: IdMap, forward: bool) -> Result<i32> {
    if forward {
        table.map(id)
    } else {
        table.inverse(id)
    }
    .map_err(Error::new)
}
fn var(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.varint(r.varint()?);
    Ok(())
}
fn string(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.string(&r.string(32767)?);
    Ok(())
}
fn float(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.f32(r.f32()?);
    Ok(())
}
fn boolean(r: &mut Reader<'_>, w: &mut Writer) -> Result<bool> {
    let v = r.bool()?;
    w.bool(v);
    Ok(v)
}
fn nbt(r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.nbt(&r.nbt()?);
    Ok(())
}
fn text_nbt<M: Maps>(r: &mut Reader<'_>, w: &mut Writer, forward: bool) -> Result<()> {
    let mut tag = r.nbt()?;
    if forward {
        crate::vfp_play::text_version(&mut tag, M::PROTOCOL)?;
    }
    w.nbt(&tag);
    Ok(())
}
fn count(r: &mut Reader<'_>, w: &mut Writer) -> Result<usize> {
    let n = r.count(r.remaining())?;
    w.varint(n as i32);
    Ok(n)
}
fn holder(r: &mut Reader<'_>, w: &mut Writer) -> Result<bool> {
    let id = r.varint()?;
    if id < 0 {
        return Err(Error::new("Negative holder reference"));
    }
    w.varint(id);
    Ok(id == 0)
}
fn either_holder<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
    registry: &crate::vfp_registry_access::Registries,
    name: &str,
) -> Result<bool> {
    if M::PROTOCOL < 770 && name == "instrument" {
        return holder(r, w);
    }
    if M::PROTOCOL < 775 {
        if forward {
            if !r.bool()? {
                let key = r.string(32767)?;
                let id = registry.id(name, &key).map_err(Error::new)?;
                w.varint(id + 1);
                return Ok(false);
            }
        } else {
            w.bool(true);
        }
    }
    holder(r, w)
}
fn synchronized(
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
    registry: &crate::vfp_registry_access::Registries,
    name: &str,
) -> Result<()> {
    let id = if forward {
        if r.bool()? {
            r.varint()?
        } else {
            registry.id(name, &r.string(32767)?).map_err(Error::new)?
        }
    } else {
        r.varint()?
    };
    if id < 0 {
        return Err(Error::new("Negative synchronized registry id"));
    }
    if registry.contains(name) {
        registry.name(name, id).map_err(Error::new)?;
    }
    if !forward {
        w.bool(true);
    }
    w.varint(id);
    Ok(())
}
fn tag_component(
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
    registry: &crate::vfp_registry_access::Registries,
    name: &str,
) -> Result<()> {
    if forward {
        let key = r.string(32767)?;
        if let Ok(id) = registry.id(name, &key) {
            w.varint(2);
            w.varint(id);
        } else {
            w.varint(0);
            w.string(&key);
        }
    } else {
        let count = r.varint()?;
        if count == 0 {
            w.string(&r.string(32767)?);
        } else if count > 0 && count as usize <= r.remaining() + 1 {
            for _ in 1..count {
                r.varint()?;
            }
        } else {
            return Err(Error::new("Invalid tag-only holder set"));
        }
    }
    Ok(())
}
pub fn sound_using<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    let id = r.varint()?;
    if id < 0 {
        return Err(Error::new("Negative sound holder"));
    }
    if id == 0 {
        w.varint(0);
        string(r, w)?;
        if boolean(r, w)? {
            float(r, w)?;
        }
    } else {
        w.varint(map(id - 1, M::SOUNDS, forward)? + 1);
    }
    Ok(())
}
fn optional_sound<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    if boolean(r, w)? {
        sound_using::<M>(r, w, forward, _registry)?;
    }
    Ok(())
}
fn holders(r: &mut Reader<'_>, w: &mut Writer, table: Option<IdMap>, forward: bool) -> Result<()> {
    let n = r.varint()?;
    if n < 0 || n as usize > r.remaining() + 1 {
        return Err(Error::new("Invalid holder set size"));
    }
    w.varint(n);
    if n == 0 {
        string(r, w)?;
    } else {
        for _ in 1..n {
            let id = r.varint()?;
            w.varint(if let Some(t) = table {
                map(id, t, forward)?
            } else {
                id
            });
        }
    }
    Ok(())
}
fn optional_holders(
    r: &mut Reader<'_>,
    w: &mut Writer,
    table: Option<IdMap>,
    forward: bool,
) -> Result<()> {
    if boolean(r, w)? {
        holders(r, w, table, forward)?;
    }
    Ok(())
}
pub(crate) fn effect_details(r: &mut Reader<'_>, w: &mut Writer, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Mob effect nesting exceeds 64"));
    }
    var(r, w)?;
    var(r, w)?;
    for _ in 0..3 {
        boolean(r, w)?;
    }
    if boolean(r, w)? {
        effect_details(r, w, depth + 1)?;
    }
    Ok(())
}
fn consume_effects<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    for _ in 0..count(r, w)? {
        let kind = r.varint()?;
        w.varint(kind);
        match kind {
            0 => {
                for _ in 0..count(r, w)? {
                    var(r, w)?;
                    effect_details(r, w, 0)?;
                }
                float(r, w)?;
            }
            1 => holders(r, w, None, forward)?,
            2 => (),
            3 => {
                float(r, w)?;
                if forward {
                    w.bool(false);
                } else {
                    r.bool()?;
                }
            }
            4 => sound_using::<M>(r, w, forward, _registry)?,
            _ => return Err(Error::new("Unknown consume effect")),
        }
    }
    Ok(())
}
fn trim_material<M: Maps>(r: &mut Reader<'_>, w: &mut Writer, forward: bool) -> Result<()> {
    if holder(r, w)? {
        let name = r.string(32767)?;
        if forward {
            w.string(&format!("minecraft:trim/{name}"));
            if M::PROTOCOL < 770 {
                r.varint()?;
                if M::PROTOCOL < 769 {
                    r.f32()?;
                }
            } // Removed ingredient item id.
            let n = r.count(r.remaining())?;
            for _ in 0..n {
                if M::PROTOCOL < 768 {
                    r.varint()?;
                } else {
                    r.string(32767)?;
                }
                r.string(32767)?;
            }
        } else {
            let path = name.split_once(':').map_or(name.as_str(), |(_, p)| p);
            w.string(path.strip_prefix("trim/").unwrap_or(path));
            if M::PROTOCOL < 770 {
                w.varint(0);
                if M::PROTOCOL < 769 {
                    w.f32(0.0);
                }
            }
            w.varint(0);
        }
        text_nbt::<M>(r, w, forward)?;
    }
    Ok(())
}
fn predicate<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
    depth: usize,

    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    optional_holders(r, w, Some(M::BLOCKS), forward)?;
    if boolean(r, w)? {
        for _ in 0..count(r, w)? {
            string(r, w)?;
            if boolean(r, w)? {
                string(r, w)?;
            } else {
                for _ in 0..2 {
                    if boolean(r, w)? {
                        string(r, w)?;
                    }
                }
            }
        }
    }
    if boolean(r, w)? {
        nbt(r, w)?;
    }
    if M::PROTOCOL < 770 {
        if forward {
            w.varint(0);
            w.varint(0);
        } else {
            for _ in 0..r.count(1024)? {
                let kind = r.varint()?;
                if !(0..M::DATA_COMPONENT_TYPE.mapped_size).contains(&kind) {
                    return Err(Error::new("Invalid predicate component"));
                }
                crate::item::Component::read(kind, r)?;
            }
            for _ in 0..r.count(64)? {
                r.bool()?;
                r.varint()?;
                r.nbt()?;
            }
        }
        return Ok(());
    }
    crate::vfp_items::exact_using::<M>(r, w, forward, depth + 1, _registry)?;
    // DataComponentPredicate since 1.21.11 is an Either: predicate-registry
    // type or data-component type, followed by registry-aware NBT. It is not a
    // binary component value. In particular, component type ids must use the
    // source version's map even though the predicate registry is unchanged.
    let n = r.count(64)?;
    w.varint(n as i32);
    for _ in 0..n {
        let predicate_type = if forward && M::PROTOCOL < 774 {
            true
        } else {
            r.bool()?
        };
        let source = r.varint()?;
        if source < 0 {
            return Err(Error::new("Negative partial predicate type"));
        }
        let target = if predicate_type {
            source
        } else {
            let target = map(source, M::DATA_COMPONENT_TYPE, forward)?;
            let native = if forward { target } else { source };
            if !crate::vfp_items::stable(native) {
                return Err(Error::new(
                    "Partial predicate uses a versioned component NBT schema",
                ));
            }
            target
        };
        if !forward && M::PROTOCOL < 774 && !predicate_type {
            return Err(Error::new(
                "Old partial predicates cannot use a component-type selector",
            ));
        }
        if forward || M::PROTOCOL >= 774 {
            w.bool(predicate_type);
        }
        w.varint(target);
        let start = r.position();
        r.nbt()?;
        w.bytes(r.since(start));
    }
    Ok(())
}
pub fn convert_using<M: Maps>(
    kind: i32,
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
    depth: usize,

    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    match kind {
        8 | 104 | 106 if M::PROTOCOL < 775 => {
            synchronized(
                r,
                w,
                forward,
                _registry,
                match kind {
                    8 => "damage_type",
                    104 => "chicken_variant",
                    _ => "zombie_nautilus_variant",
                },
            )?;
        }
        27 | 67 if M::PROTOCOL < 775 => {
            tag_component(
                r,
                w,
                forward,
                _registry,
                if kind == 27 {
                    "damage_type"
                } else {
                    "banner_pattern"
                },
            )?;
        }
        16 => {
            for _ in 0..count(r, w)? {
                w.varint(map(r.varint()?, M::ATTRIBUTES, forward)?);
                if M::PROTOCOL < 767 {
                    if forward {
                        let uuid = r.uuid()?;
                        let old = r.string(32767)?;
                        w.string(&crate::vfp_modifiers_1205::name(uuid, Some(&old)));
                    } else {
                        let id = r.string(32767)?;
                        let (uuid, name) = crate::vfp_modifiers_1205::identity(&id);
                        w.uuid(uuid);
                        w.string(&name);
                    }
                } else {
                    string(r, w)?;
                }
                w.f64(r.f64()?);
                var(r, w)?;
                var(r, w)?;
                if forward && M::PROTOCOL < 771 {
                    w.varint(0); // Default display was implicit before 1.21.6.
                } else {
                    let display = r.varint()?;
                    if forward || M::PROTOCOL >= 771 {
                        w.varint(display);
                    }
                    match display {
                        0 | 1 => (),
                        2 => {
                            let tag = r.nbt()?;
                            if forward || M::PROTOCOL >= 771 {
                                w.nbt(&tag);
                            }
                        }
                        _ => return Err(Error::new("Unknown attribute modifier display")),
                    }
                }
            }
            if M::PROTOCOL < 770 {
                if forward {
                    r.bool()?;
                } else {
                    w.bool(true);
                }
            }
        }
        14 | 15 => {
            for _ in 0..count(r, w)? {
                predicate::<M>(r, w, forward, depth + 1, _registry)?;
            }
            if M::PROTOCOL < 770 {
                if forward {
                    r.bool()?;
                } else {
                    w.bool(true);
                }
            }
        }
        18 => {
            boolean(r, w)?;
            let n = r.count(r.remaining())?;
            let mut ids = Vec::with_capacity(n);
            for _ in 0..n {
                let id = r.varint()?;
                if forward && id == M::SWING_OLD {
                    ids.extend([40, 41]);
                } else if forward && id == M::MAP_COLOR_OLD { /* Removed map_color. */
                } else if !forward && (id == 40 || id == 41) {
                    if M::SWING_OLD >= 0 {
                        ids.push(M::SWING_OLD);
                    }
                } else if !forward {
                    if !(0..M::DATA_COMPONENT_TYPE.mapped_size).contains(&id) {
                        return Err(Error::new("Unknown tooltip component type"));
                    }
                    if let Ok(source) = M::DATA_COMPONENT_TYPE.inverse(id) {
                        ids.push(source);
                    }
                } else {
                    ids.push(map(id, M::DATA_COMPONENT_TYPE, forward)?);
                }
            }
            ids.sort_unstable();
            ids.dedup();
            w.varint(ids.len() as i32);
            for id in ids {
                w.varint(id);
            }
        }
        24 => {
            float(r, w)?;
            var(r, w)?;
            sound_using::<M>(r, w, forward, _registry)?;
            boolean(r, w)?;
            consume_effects::<M>(r, w, forward, _registry)?;
        }
        28 => {
            for _ in 0..count(r, w)? {
                holders(r, w, Some(M::BLOCKS), forward)?;
                if boolean(r, w)? {
                    float(r, w)?;
                }
                if boolean(r, w)? {
                    boolean(r, w)?;
                }
            }
            float(r, w)?;
            var(r, w)?;
            if M::PROTOCOL < 770 {
                if forward {
                    w.bool(true);
                } else {
                    r.bool()?;
                }
            } else {
                boolean(r, w)?;
            }
        }
        32 => {
            var(r, w)?;
            sound_using::<M>(r, w, forward, _registry)?;
            for _ in 0..2 {
                if boolean(r, w)? {
                    string(r, w)?;
                }
            }
            optional_holders(r, w, Some(M::ENTITIES), forward)?;
            for _ in 0..if M::PROTOCOL < 770 { 3 } else { 4 } {
                boolean(r, w)?;
            }
            if M::PROTOCOL < 770 {
                if forward {
                    w.bool(true);
                } else {
                    r.bool()?;
                }
            }
            if forward && M::PROTOCOL < 771 {
                w.bool(false); // can_be_sheared
                w.varint(M::SOUNDS.map(0).map_err(Error::new)? + 1);
            } else if !forward && M::PROTOCOL < 771 {
                r.bool()?;
                if holder(r, &mut Writer::new())? {
                    r.string(32767)?;
                    if r.bool()? {
                        r.f32()?;
                    }
                }
            } else {
                boolean(r, w)?;
                sound_using::<M>(r, w, forward, _registry)?;
            }
        }
        33 => holders(r, w, Some(M::ITEMS), forward)?,
        36 => consume_effects::<M>(r, w, forward, _registry)?,
        37 => {
            float(r, w)?;
            float(r, w)?;
            for _ in 0..count(r, w)? {
                float(r, w)?;
                optional_holders(r, w, None, forward)?;
                float(r, w)?;
                float(r, w)?;
            }
            for _ in 0..3 {
                float(r, w)?;
            }
            if M::PROTOCOL < 775 {
                let present = r.bool()?;
                if forward {
                    w.bool(present);
                    if present {
                        tag_component(r, w, true, _registry, "damage_type")?;
                    }
                } else if present {
                    let mut tag = Writer::new();
                    tag_component(r, &mut tag, false, _registry, "damage_type")?;
                    w.bool(!tag.data.is_empty());
                    w.bytes(&tag.data);
                } else {
                    w.bool(false);
                }
            } else {
                optional_holders(r, w, None, forward)?;
            }
            optional_sound::<M>(r, w, forward, _registry)?;
            optional_sound::<M>(r, w, forward, _registry)?;
        }
        38 => {
            boolean(r, w)?;
            boolean(r, w)?;
            optional_sound::<M>(r, w, forward, _registry)?;
            optional_sound::<M>(r, w, forward, _registry)?;
        }
        39 => {
            var(r, w)?;
            var(r, w)?;
            for _ in 0..3 {
                if boolean(r, w)? {
                    var(r, w)?;
                    float(r, w)?;
                    float(r, w)?;
                }
            }
            float(r, w)?;
            float(r, w)?;
            optional_sound::<M>(r, w, forward, _registry)?;
            optional_sound::<M>(r, w, forward, _registry)?;
        }
        40 | 41 => {
            var(r, w)?;
            var(r, w)?;
        }
        58 => {
            trim_material::<M>(r, w, forward)?;
            if holder(r, w)? {
                string(r, w)?;
                if M::PROTOCOL < 768 {
                    if forward {
                        r.varint()?;
                    } else {
                        w.varint(0);
                    }
                }
                text_nbt::<M>(r, w, forward)?;
                boolean(r, w)?;
            }
            if M::PROTOCOL < 770 {
                if forward {
                    r.bool()?;
                } else {
                    w.bool(true);
                }
            }
        }
        60 | 62 => {
            if M::PROTOCOL < 773 {
                return typed_entity::<M>(kind, r, w, forward);
            }
            let id = r.varint()?;
            let mut tag = r.nbt()?;
            if kind == 62 && forward && crate::vfp_world::obsolete_block_entity::<M>(id, _registry)
            {
                return Ok(());
            }
            w.varint(map(
                id,
                if kind == 60 {
                    M::ENTITIES
                } else {
                    M::BLOCKENTITIES
                },
                forward,
            )?);
            if kind == 62 {
                if forward {
                    crate::vfp_world::upgrade_block_entity_using::<M>(id, &mut tag)?;
                } else {
                    crate::vfp_world::downgrade_block_entity(&mut tag)?;
                }
            }
            w.nbt(&tag);
        }
        63 => {
            if either_holder::<M>(r, w, forward, _registry, "instrument")? {
                sound_using::<M>(r, w, forward, _registry)?;
                if M::PROTOCOL < 768 {
                    if forward {
                        w.f32(r.varint()? as f32 / 20.0);
                    } else {
                        w.varint((r.f32()? * 20.0) as i32);
                    }
                } else {
                    float(r, w)?;
                }
                float(r, w)?;
                if forward {
                    w.varint(0);
                } else {
                    r.varint()?;
                }
                if M::PROTOCOL < 768 {
                    if forward {
                        w.nbt(&Tag::String("".into()));
                    } else {
                        r.nbt()?;
                    }
                } else {
                    text_nbt::<M>(r, w, forward)?;
                }
            }
        }
        64 => {
            if M::PROTOCOL < 775 {
                if forward && !r.bool()? {
                    let key = r.string(32767)?;
                    w.varint(_registry.id("trim_material", &key).map_err(Error::new)? + 1);
                    return Ok(());
                }
                if !forward {
                    w.bool(true);
                }
            }
            trim_material::<M>(r, w, forward)?;
        }
        66 => {
            if either_holder::<M>(r, w, forward, _registry, "jukebox_song")? {
                sound_using::<M>(r, w, forward, _registry)?;
                text_nbt::<M>(r, w, forward)?;
                float(r, w)?;
                var(r, w)?;
            }
            if M::PROTOCOL < 770 {
                if forward {
                    r.bool()?;
                } else {
                    w.bool(true);
                }
            }
        }
        72 => {
            if M::PROTOCOL < 773 {
                crate::entity_data::vfp_entities::item_profile_legacy(r, w, forward)?;
            } else {
                crate::entity_data::vfp_entities::item_profile_262(r, w, forward)?;
            }
        }
        79 => {
            for _ in 0..count(r, w)? {
                if M::PROTOCOL < 773 {
                    if forward {
                        let bee = M::ENTITY_IDENTIFIERS
                            .iter()
                            .position(|name| *name == "bee")
                            .ok_or_else(|| Error::new("Missing old bee entity type"))?
                            as i32;
                        w.varint(M::ENTITIES.map(bee).map_err(Error::new)?);
                        nbt(r, w)?;
                    } else {
                        r.varint()?;
                        let mut data = r.nbt()?;
                        let Tag::Compound(tag) = &mut data else {
                            return Err(Error::new("Bee data is not a compound"));
                        };
                        tag.insert(JavaString::from("id"), Tag::String("minecraft:bee".into()));
                        w.nbt(&data);
                    }
                } else {
                    w.varint(map(r.varint()?, M::ENTITIES, forward)?);
                    nbt(r, w)?;
                }
                var(r, w)?;
                var(r, w)?;
            }
        }
        83 => sound_using::<M>(r, w, forward, _registry)?,
        _ => {
            return Err(Error::new(format!(
                "Component {kind} requires a versioned value converter"
            )));
        }
    }
    Ok(())
}

pub fn sound(r: &mut Reader<'_>, w: &mut Writer, forward: bool) -> Result<()> {
    sound_using::<V262>(
        r,
        w,
        forward,
        &crate::vfp_registry_access::Registries::default(),
    )
}
pub fn convert(
    kind: i32,
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
    depth: usize,
) -> Result<()> {
    convert_using::<V262>(
        kind,
        r,
        w,
        forward,
        depth,
        &crate::vfp_registry_access::Registries::default(),
    )
}

pub fn sound_with<M: Maps>(r: &mut Reader<'_>, w: &mut Writer, forward: bool) -> Result<()> {
    sound_using::<M>(
        r,
        w,
        forward,
        &crate::vfp_registry_access::Registries::default(),
    )
}

pub fn convert_with<M: Maps>(
    kind: i32,
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
    depth: usize,
) -> Result<()> {
    convert_using::<M>(
        kind,
        r,
        w,
        forward,
        depth,
        &crate::vfp_registry_access::Registries::default(),
    )
}
