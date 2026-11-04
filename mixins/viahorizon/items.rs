use crate::vfp_mapping::{Maps, V262};
// SPDX-License-Identifier: GPL-3.0-or-later
// Item patches with unchanged value codecs. Versioned nested values are rejected
// until their own converters exist, rather than decoded with the wrong schema.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    item::{Component, component as c},
    vfp_tables as t, vfp_wire as wire,
};
use horizon_world::nbt::{JavaString, Tag};

pub(crate) const BACKUP_KEY: &str = "viafabricplus:26_2_backup";
struct BackupValue {
    source: i32,
    mapped_id: i32,
    original: Vec<u8>,
    mapped: Vec<u8>,
    removed: bool,
    synthetic: bool,
}
fn save_backup(
    entries: &mut Vec<(i32, Vec<u8>)>,
    removals: &mut Vec<i32>,
    values: Vec<BackupValue>,
) -> Result<()> {
    if values.is_empty() {
        return Ok(());
    }
    let index = entries.iter().position(|(id, _)| *id == 0);
    let custom = if index.is_some() {
        1
    } else if removals.contains(&0) {
        removals.retain(|id| *id != 0);
        2
    } else {
        0
    };
    let mut data = if let Some(index) = index {
        Reader::new(&entries[index].1).nbt()?
    } else {
        Tag::compound([] as [(&str, Tag); 0])
    };
    let Tag::Compound(map) = &mut data else {
        return Err(Error::new("Item custom_data is not a compound"));
    };
    let key = JavaString::from(BACKUP_KEY);
    if map.contains_key(&key) {
        return Err(Error::new(
            "Item custom_data collides with translator backup key",
        ));
    }
    map.insert(
        key,
        Tag::compound([
            ("schema", Tag::Int(1)),
            ("custom", Tag::Byte(custom)),
            (
                "values",
                Tag::List(
                    values
                        .into_iter()
                        .map(|v| {
                            Tag::compound([
                                ("source", Tag::Int(v.source)),
                                ("mapped_id", Tag::Int(v.mapped_id)),
                                ("original", Tag::ByteArray(v.original)),
                                ("mapped", Tag::ByteArray(v.mapped)),
                                ("removed", Tag::Byte(i8::from(v.removed))),
                                ("synthetic", Tag::Byte(i8::from(v.synthetic))),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]),
    );
    let mut w = Writer::new();
    w.nbt(&data);
    if let Some(index) = index {
        entries[index].1 = w.data;
    } else {
        entries.push((0, w.data));
    }
    Ok(())
}
pub(crate) fn restore_backup_using<M: Maps>(
    entries: &mut Vec<(i32, Vec<u8>)>,
    observed: &[(i32, Vec<u8>)],
    removals: &mut Vec<i32>,

    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    let Some(index) = entries.iter().position(|(id, _)| *id == 0) else {
        return Ok(());
    };
    let mut data = Reader::new(&entries[index].1).nbt()?;
    let Tag::Compound(map) = &mut data else {
        return Ok(());
    };
    let Some(backup) = map.remove(&JavaString::from(BACKUP_KEY)) else {
        return Ok(());
    };
    if backup.get("schema") != Some(&Tag::Int(1)) {
        return Err(Error::new("Invalid item backup schema"));
    }
    let Some(Tag::Byte(custom @ (0..=2))) = backup.get("custom") else {
        return Err(Error::new("Invalid item backup custom-data flag"));
    };
    let Some(Tag::List(values)) = backup.get("values") else {
        return Err(Error::new("Invalid item backup values"));
    };
    if *custom != 1 && map.is_empty() {
        entries.remove(index);
        if *custom == 2 {
            removals.push(0);
        }
    } else {
        let mut w = Writer::new();
        w.nbt(&data);
        entries[index].1 = w.data;
    }
    let mut seen = std::collections::HashSet::new();
    for value in values {
        let (
            Some(Tag::Int(source)),
            Some(Tag::Int(mapped_id)),
            Some(Tag::ByteArray(original)),
            Some(Tag::ByteArray(mapped)),
        ) = (
            value.get("source"),
            value.get("mapped_id"),
            value.get("original"),
            value.get("mapped"),
        )
        else {
            return Err(Error::new("Malformed item backup value"));
        };
        let synthetic = match value.get("synthetic") {
            None | Some(Tag::Byte(0)) => false,
            Some(Tag::Byte(1)) => true,
            _ => return Err(Error::new("Invalid item backup synthesis flag")),
        };
        if synthetic {
            if M::PROTOCOL >= 768
                || !(matches!(*mapped_id, 6 | 21) || (M::PROTOCOL < 767 && *mapped_id == 12))
                || !original.is_empty()
                || value.get("removed") != Some(&Tag::Byte(0))
                || M::DATA_COMPONENT_TYPE.map(*source).map_err(Error::new)? != *mapped_id
                || !seen.insert(*source)
            {
                return Err(Error::new("Invalid synthesized item backup"));
            }
            let mut reader = Reader::new(mapped);
            Component::read(*mapped_id, &mut reader)?;
            if reader.remaining() != 0 {
                return Err(Error::new("Trailing synthesized item backup"));
            }
            if observed
                .iter()
                .any(|(id, bytes)| id == mapped_id && bytes == mapped)
            {
                entries.retain(|(id, _)| id != source);
            }
            continue;
        }
        let expected = if *source == M::MAP_COLOR_OLD {
            -1
        } else {
            M::DATA_COMPONENT_TYPE.map(*source).map_err(Error::new)?
        };
        let legacy_value = (M::PROTOCOL < 775
            && matches!(expected, 8 | 27 | 37 | 63 | 66 | 67 | 104 | 106))
            || (M::PROTOCOL < 773 && matches!(expected, 60 | 79))
            || (M::PROTOCOL < 769 && expected == 17)
            || (M::PROTOCOL < 768 && matches!(expected, 13 | 53 | 81))
            || (M::PROTOCOL < 767 && expected == 16);
        if (*source != M::MAP_COLOR_OLD
            && !matches!(expected, 6 | 9 | 11 | 57 | 58 | 62 | 64 | 72)
            && !legacy_value)
            || !seen.insert(*source)
        {
            return Err(Error::new("Invalid or duplicate item backup component"));
        }
        let obsolete_bed = M::BED_OLD.is_some() && expected == 62 && *mapped_id == -1;
        if *mapped_id != expected && !obsolete_bed {
            return Err(Error::new("Mismatched item backup component id"));
        }
        match value.get("removed") {
            Some(Tag::Byte(1))
                if *source == M::MAP_COLOR_OLD && original.is_empty() && mapped.is_empty() =>
            {
                removals.push(M::MAP_COLOR_OLD);
                continue;
            }
            None | Some(Tag::Byte(0)) => (),
            _ => return Err(Error::new("Invalid item backup removal")),
        }
        let mut reader = Reader::new(original);
        let mut encoded = Writer::new();
        if obsolete_bed {
            if M::PROTOCOL < 773 {
                let data = reader.nbt()?;
                if !matches!(data.get("id"), Some(Tag::String(id)) if matches!(id.to_string_lossy().as_str(), "bed" | "minecraft:bed"))
                {
                    return Err(Error::new("Invalid obsolete bed NBT backup"));
                }
            } else {
                if Some(reader.varint()?) != M::BED_OLD {
                    return Err(Error::new("Invalid obsolete bed backup"));
                }
                reader.nbt()?;
            }
        } else if *source == M::MAP_COLOR_OLD {
            reader.i32()?;
        } else {
            value_using::<M>(expected, &mut reader, &mut encoded, true, 0, _registry)?;
        }
        if reader.remaining() != 0 || encoded.data != *mapped {
            return Err(Error::new("Invalid item backup payload"));
        }
        let mut unchanged = *mapped_id == -1;
        for (id, bytes) in observed {
            if id != mapped_id {
                continue;
            }
            unchanged |= bytes == mapped;
            if *mapped_id == c::WRITTEN_BOOK_CONTENT && !unchanged {
                let mut expected = Reader::new(mapped);
                let mut actual = Reader::new(bytes);
                let expected_value = Component::read(*mapped_id, &mut expected)?;
                let actual_value = Component::read(*mapped_id, &mut actual)?;
                // The pinned native writer drops filtered variants. Restore
                // them when every field the player could edit is unchanged.
                unchanged = expected.remaining() == 0
                    && actual.remaining() == 0
                    && expected_value == actual_value;
            }
        }
        if unchanged {
            entries.retain(|(id, _)| id != source);
            entries.push((*source, original.clone()));
        }
    }
    Ok(())
}

pub(crate) fn stable(kind: i32) -> bool {
    matches!(
        kind,
        c::CUSTOM_DATA
            | c::MAX_STACK_SIZE
            | c::MAX_DAMAGE
            | c::DAMAGE
            | c::UNBREAKABLE
            | c::CUSTOM_NAME
            | c::ITEM_NAME
            | c::ITEM_MODEL
            | c::LORE
            | c::RARITY
            | c::REPAIR_COST
            | c::CREATIVE_SLOT_LOCK
            | c::ENCHANTMENT_GLINT_OVERRIDE
            | c::DYED_COLOR
            | c::MAP_ID
            | c::MAP_POST_PROCESSING
            | c::OMINOUS_BOTTLE_AMPLIFIER
            | c::BASE_COLOR
            | c::ATTACK_RANGE
            | c::ENCHANTMENTS
            | c::STORED_ENCHANTMENTS
            | c::CUSTOM_MODEL_DATA
            | c::POTION_CONTENTS
            | c::SUSPICIOUS_STEW_EFFECTS
            | c::WRITABLE_BOOK_CONTENT
            | c::WRITTEN_BOOK_CONTENT
            | c::LODESTONE_TRACKER
            | c::BANNER_PATTERNS
            | 5
            | 7
            | 8
            | 23
            | 26
            | 29
            | 35
            | 54
            | 59
            | 68
            | 78
            | 81
            | 82
            | 22
            | 27
            | 31
            | 34
            | 61
            | 67
            | 70
            | 71
            | 73
            | 88..=117
    )
}
// This is the count/item/patch codec used by ordinary inventory slots.
// delimited is only used for creative-mode serverbound component payloads.
pub fn stack_using<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    to_client: bool,
    delimited: bool,

    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    if M::NATIVE_ITEMS {
        if !to_client {
            return crate::vfp_items_1203::reverse::<M>(r, w, delimited, _registry);
        }
        use crate::buf::Codec;
        let start = r.position();
        if delimited {
            crate::item::DelimitedSlot::read(r)?;
        } else {
            crate::item::Slot::read(r)?;
        }
        w.bytes(r.since(start));
        return Ok(());
    }
    structured_stack_using::<M>(r, w, to_client, delimited, _registry)
}
pub(crate) fn structured_stack_using<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    to_client: bool,
    delimited: bool,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    let count = r.varint()?;
    if count < 0 {
        return Err(Error::new("Negative item count"));
    }
    w.varint(count);
    if count == 0 {
        return Ok(());
    }
    let item = r.varint()?;
    w.varint(
        if to_client {
            M::ITEMS.map(item)
        } else {
            M::ITEMS.inverse(item)
        }
        .map_err(Error::new)?,
    );
    patch_using::<M>(r, w, to_client, delimited, 0, Some(item), _registry)
}
pub fn template_using<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    to_client: bool,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    if M::NATIVE_ITEMS {
        if !to_client {
            return Err(Error::new(
                "This legacy server has no standalone item-template wire type",
            ));
        }
        use crate::buf::Codec;
        let start = r.position();
        crate::item::Template::read(r)?;
        w.bytes(r.since(start));
        return Ok(());
    }
    template_at::<M>(r, w, to_client, 0, _registry)
}
fn text_value(r: &mut Reader<'_>, w: &mut Writer, protocol: i32) -> Result<()> {
    let mut tag = r.nbt()?;
    crate::vfp_play::text_version(&mut tag, protocol)?;
    w.nbt(&tag);
    Ok(())
}
fn text_component(kind: i32, r: &mut Reader<'_>, w: &mut Writer, protocol: i32) -> Result<()> {
    match kind {
        c::CUSTOM_NAME | c::ITEM_NAME => text_value(r, w, protocol)?,
        c::LORE => {
            let n = r.count(256)?;
            w.varint(n as i32);
            for _ in 0..n {
                text_value(r, w, protocol)?;
            }
        }
        c::WRITTEN_BOOK_CONTENT => {
            // Preserve BOTH filterable variants. Component::WrittenBook drops
            // the filtered title/pages, so its writer cannot round-trip this.
            w.string(&r.string(32)?);
            let filtered = r.bool()?;
            w.bool(filtered);
            if filtered {
                w.string(&r.string(32)?);
            }
            w.string(&r.string(32767)?);
            w.varint(r.varint()?);
            let n = r.count(r.remaining())?;
            w.varint(n as i32);
            for _ in 0..n {
                text_value(r, w, protocol)?;
                let filtered = r.bool()?;
                w.bool(filtered);
                if filtered {
                    text_value(r, w, protocol)?;
                }
            }
            w.bool(r.bool()?);
        }
        _ => unreachable!(),
    }
    Ok(())
}
fn template_at<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    to_client: bool,
    depth: usize,

    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Nested item templates exceed 64 levels"));
    }
    let first = r.varint()?;
    let second = r.varint()?;
    let (item, count) = if to_client && M::PROTOCOL < 775 {
        (second, first)
    } else {
        (first, second)
    };
    write_template::<M>(item, count, r, w, to_client, depth, _registry)
}
pub(crate) fn write_template<M: Maps>(
    item: i32,
    count: i32,
    r: &mut Reader<'_>,
    w: &mut Writer,
    to_client: bool,
    depth: usize,
    registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    if count <= 0 {
        return Err(Error::new("Nonpositive item template count"));
    }
    let source_item = item;
    let item = if to_client {
        M::ITEMS.map(item)
    } else {
        M::ITEMS.inverse(item)
    }
    .map_err(Error::new)?;
    if !to_client && M::PROTOCOL < 775 {
        w.varint(count);
        w.varint(item);
    } else {
        w.varint(item);
        w.varint(count);
    }
    patch_using::<M>(
        r,
        w,
        to_client,
        false,
        depth + 1,
        Some(source_item),
        registry,
    )
}
pub(crate) fn value_using<M: Maps>(
    kind: i32,
    r: &mut Reader<'_>,
    w: &mut Writer,
    to_client: bool,
    depth: usize,

    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Nested item components exceed 64 levels"));
    }
    match kind {
        23 if M::PROTOCOL < 768 && to_client => {
            crate::vfp_items_121::Patch::default().food::<M>(r, w, depth, _registry)
        }
        27 if M::PROTOCOL < 768 => {
            if to_client {
                w.varint(0);
                w.string("minecraft:is_fire");
            } else {
                crate::vfp_values::convert_using::<M>(
                    27,
                    r,
                    &mut Writer::new(),
                    false,
                    depth,
                    _registry,
                )?;
            }
            Ok(())
        }
        81 if M::PROTOCOL < 768 => crate::vfp_items_121::lock(r, w, to_client),
        53 if M::PROTOCOL < 768 => {
            let start = r.position();
            if to_client {
                if r.bool()? {
                    r.varint()?;
                }
                if r.bool()? {
                    r.i32()?;
                }
                for _ in 0..r.count(1024)? {
                    r.varint()?;
                    crate::vfp_values::effect_details(r, &mut Writer::new(), depth + 1)?;
                }
                w.bytes(r.since(start));
                w.bool(false);
            } else {
                Component::read(53, r)?;
                let mut copy = Reader::new(r.since(start));
                if copy.bool()? {
                    copy.varint()?;
                }
                if copy.bool()? {
                    copy.i32()?;
                }
                for _ in 0..copy.count(1024)? {
                    copy.varint()?;
                    crate::vfp_values::effect_details(&mut copy, &mut Writer::new(), depth + 1)?;
                }
                w.bytes(&r.since(start)[..copy.position()]);
            }
            Ok(())
        }
        17 if M::PROTOCOL < 769 => {
            if to_client {
                let value = r.varint()?;
                w.varint(1);
                w.f32(value as f32);
                for _ in 0..3 {
                    w.varint(0);
                }
            } else {
                Component::read(17, r)?;
            }
            Ok(())
        }
        13 if M::PROTOCOL < 768 && to_client => {
            let mut values = Vec::new();
            for _ in 0..r.count(256)? {
                let id = r.varint()?;
                let level = r.varint()?;
                if level != 0 {
                    values.push((id, level));
                }
            }
            w.varint(values.len() as i32);
            for (id, level) in values {
                w.varint(id);
                w.varint(level);
            }
            r.bool()?;
            Ok(())
        }
        4 | 13 | 45 | 47 if M::PROTOCOL < 770 => {
            let start = r.position();
            Component::read(kind, r)?;
            w.bytes(r.since(start));
            if to_client {
                r.bool()?;
            } else {
                w.bool(true);
            }
            Ok(())
        }

        8 | 27 | 67 | 104 | 106 if M::PROTOCOL < 775 => {
            crate::vfp_values::convert_using::<M>(kind, r, w, to_client, depth, _registry)
        }
        c::CUSTOM_NAME | c::ITEM_NAME | c::LORE | c::WRITTEN_BOOK_CONTENT if to_client => {
            text_component(kind, r, w, M::PROTOCOL)
        }
        25 | 80 => template_at::<M>(r, w, to_client, depth + 1, _registry),
        51 | 52 | 77 => {
            let count = r.count(1024)?;
            w.varint(count as i32);
            for _ in 0..count {
                if kind == 77 && M::PROTOCOL < 775 {
                    if to_client {
                        let count = r.varint()?;
                        if count < 0 {
                            return Err(Error::new("Negative container stack count"));
                        }
                        w.bool(count != 0);
                        if count != 0 {
                            let item = r.varint()?;
                            write_template::<M>(item, count, r, w, true, depth + 1, _registry)?;
                        }
                    } else if r.bool()? {
                        template_at::<M>(r, w, false, depth + 1, _registry)?;
                    } else {
                        w.varint(0);
                    }
                    continue;
                }
                if kind == 77 {
                    let present = r.bool()?;
                    w.bool(present);
                    if !present {
                        continue;
                    }
                }
                template_at::<M>(r, w, to_client, depth + 1, _registry)?;
            }
            Ok(())
        }
        76 => {
            if to_client {
                if r.varint()? != 4 {
                    return Err(Error::new("Expected four pot decoration IDs"));
                }
                for _ in 0..4 {
                    w.bool(true);
                    w.varint(M::ITEMS.map(r.varint()?).map_err(Error::new)?);
                    w.varint(1);
                    w.varint(0);
                    w.varint(0);
                }
            } else {
                w.varint(4);
                for _ in 0..4 {
                    if !r.bool()? {
                        w.varint(M::BRICK_OLD);
                        continue;
                    }
                    let item = r.varint()?;
                    if r.varint()? != 1 || r.varint()? != 0 || r.varint()? != 0 {
                        return Err(Error::new(
                            "Patched pot decoration cannot be represented by 26.2",
                        ));
                    }
                    w.varint(M::ITEMS.inverse(item).map_err(Error::new)?);
                }
            }
            Ok(())
        }
        _ if stable(kind) => {
            let start = r.position();
            Component::read(kind, r)?;
            w.bytes(r.since(start));
            Ok(())
        }
        _ => crate::vfp_values::convert_using::<M>(kind, r, w, to_client, depth, _registry),
    }
}
fn patch_using<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    to_client: bool,
    delimited: bool,
    depth: usize,
    item_hint: Option<i32>,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    patch_values::<M>(
        r, w, to_client, delimited, depth, false, item_hint, _registry,
    )
}
pub(crate) fn exact_using<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    forward: bool,
    depth: usize,

    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    patch_values::<M>(r, w, forward, false, depth, true, None, _registry)
}
fn patch_values<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    to_client: bool,
    delimited: bool,
    depth: usize,
    exact: bool,
    item_hint: Option<i32>,
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<()> {
    let added = r.count(1024)?;
    let removed = if exact { 0 } else { r.count(1024)? };
    let mut entries: Vec<(i32, Vec<u8>)> = Vec::with_capacity(added);
    let mut backups = Vec::new();
    let mut observed = Vec::new();
    let mut removals = Vec::with_capacity(removed);
    let mut seen = std::collections::HashSet::new();
    let mut old_patch = crate::vfp_items_1214::Patch::<M>::default();
    let mut food_patch = crate::vfp_items_121::Patch::default();
    for _ in 0..added {
        let source = r.varint()?;
        if !seen.insert(source) {
            return Err(Error::new("Duplicate data component"));
        }
        if !to_client
            && M::PROTOCOL < 768
            && food_patch.native_special::<M>(source, r, delimited, depth, _registry)?
        {
            continue;
        }
        if M::PROTOCOL < 770
            && if to_client {
                old_patch.old_special(source, r, delimited)?
            } else {
                old_patch.native_special(source, r, delimited)?
            }
        {
            continue;
        }
        let unsupported = !to_client
            && if source == 40 || source == 41 {
                M::SWING_OLD < 0
            } else {
                M::DATA_COMPONENT_TYPE.inverse(source).is_err()
            };
        if unsupported {
            if !(0..M::DATA_COMPONENT_TYPE.mapped_size).contains(&source) {
                return Err(Error::new("Unknown native data component"));
            }
            if exact {
                return Err(Error::new(
                    "New-only component cannot be matched by the old server",
                ));
            }
            // Validate and consume native-only data, then leave it out of the
            // old patch. New clients have extra components even on old items.
            if delimited {
                let size = r.count(r.remaining())?;
                let mut value = Reader::new(r.bytes(size)?);
                Component::read(source, &mut value)?;
                if value.remaining() != 0 {
                    return Err(Error::new("Trailing native-only component bytes"));
                }
            } else {
                Component::read(source, r)?;
            }
            continue;
        }
        if to_client && source == M::MAP_COLOR_OLD {
            if exact {
                return Err(Error::new("26.3 has no exact map_color predicate"));
            }
            let value;
            if delimited {
                let size = r.count(r.remaining())?;
                let mut sub = Reader::new(r.bytes(size)?);
                value = sub.i32()?;
                if sub.remaining() != 0 {
                    return Err(Error::new("Trailing map-color bytes"));
                }
            } else {
                value = r.i32()?;
            }
            backups.push(BackupValue {
                source,
                mapped_id: -1,
                original: value.to_be_bytes().to_vec(),
                mapped: Vec::new(),
                removed: false,
                synthetic: false,
            });
            continue;
        }
        let destination = if to_client {
            if source == M::SWING_OLD {
                Ok(40)
            } else {
                M::DATA_COMPONENT_TYPE.map(source)
            }
        } else {
            if source == 40 || source == 41 {
                Ok(M::SWING_OLD)
            } else {
                M::DATA_COMPONENT_TYPE.inverse(source)
            }
        }
        .map_err(Error::new)?;
        let native = if to_client { destination } else { source };
        let mut encoded = Writer::new();
        let original;
        if delimited {
            let len = r.count(r.remaining())?;
            let bytes = r.bytes(len)?;
            original = bytes.to_vec();
            let mut sub = Reader::new(bytes);
            if M::PROTOCOL < 768 && native == 23 && to_client {
                food_patch.food::<M>(&mut sub, &mut encoded, depth + 1, _registry)?;
            } else {
                value_using::<M>(
                    native,
                    &mut sub,
                    &mut encoded,
                    to_client,
                    depth + 1,
                    _registry,
                )?;
            }
            if sub.remaining() != 0 {
                return Err(Error::new("Trailing component value bytes"));
            }
        } else {
            let start = r.position();
            if M::PROTOCOL < 768 && native == 23 && to_client {
                food_patch.food::<M>(r, &mut encoded, depth + 1, _registry)?;
            } else {
                value_using::<M>(native, r, &mut encoded, to_client, depth + 1, _registry)?;
            }
            original = r.since(start).to_vec();
        }
        if !to_client
            && ((M::PROTOCOL < 775 && matches!(native, 27 | 67))
                || (M::PROTOCOL < 773 && matches!(native, 60 | 62))
                || (M::PROTOCOL < 769 && native == 17))
            && encoded.data.is_empty()
            && !(M::PROTOCOL < 768
                && native == 27
                && crate::vfp_items_121::fire_matches(&original)?)
        {
            // These native values have no older representation. Omit the
            // field, retaining its observation for checked backup restoration.
            if exact {
                return Err(Error::new(
                    "A native value cannot be matched by the older component schema",
                ));
            }
            observed.push((source, original));
            continue;
        }
        if !to_client
            && M::PROTOCOL < 768
            && native == 27
            && !crate::vfp_items_121::fire_matches(&original)?
        {
            observed.push((source, original));
            continue;
        }
        if !to_client && M::PROTOCOL < 768 && native == 81 && encoded.data.is_empty() {
            observed.push((source, original));
            continue;
        }
        if to_client && native == 62 && encoded.data.is_empty() {
            if exact {
                return Err(Error::new(
                    "Obsolete block entity cannot be matched exactly by this client",
                ));
            }
            backups.push(BackupValue {
                source,
                mapped_id: -1,
                original,
                mapped: Vec::new(),
                removed: false,
                synthetic: false,
            });
            continue;
        }
        if M::PROTOCOL < 770 && to_client {
            old_patch.forward_value(native, &original);
        }
        let changed_text = matches!(native, 6 | 9 | 11 | 57) && original != encoded.data;
        if to_client && exact && changed_text {
            return Err(Error::new(
                "Changed text cannot be matched exactly by this client",
            ));
        }
        let legacy_value = (M::PROTOCOL < 775
            && matches!(native, 8 | 27 | 37 | 63 | 66 | 67 | 104 | 106))
            || (M::PROTOCOL < 773 && matches!(native, 60 | 79))
            || (M::PROTOCOL < 769 && native == 17)
            || (M::PROTOCOL < 768 && matches!(native, 13 | 53 | 81))
            || (M::PROTOCOL < 767 && native == 16);
        if to_client
            && !exact
            && (matches!(native, 57 | 58 | 62 | 64 | 72) || changed_text || legacy_value)
        {
            backups.push(BackupValue {
                source,
                mapped_id: destination,
                original,
                mapped: encoded.data.clone(),
                removed: false,
                synthetic: false,
            });
        } else if !to_client && !exact {
            observed.push((source, original));
        }
        if let Some((_, previous)) = entries.iter().find(|(id, _)| *id == destination) {
            if *previous != encoded.data {
                return Err(Error::new(
                    "26.2 cannot represent different attack and interact animations",
                ));
            }
        } else {
            entries.push((destination, encoded.data.clone()));
        }
        if to_client && source == M::SWING_OLD {
            entries.push((41, encoded.data));
        }
    }
    for _ in 0..removed {
        let source = r.varint()?;
        if !seen.insert(source) {
            return Err(Error::new("Duplicate or conflicting removed component"));
        }
        if to_client
            && M::PROTOCOL < 768
            && M::COMPONENT_IDENTIFIERS.get(source as usize) == Some(&"food")
        {
            removals.extend([24, 25]);
        }
        if M::PROTOCOL < 770 && old_patch.removed(source, to_client, &mut removals) {
            continue;
        }
        if !to_client && !(0..M::DATA_COMPONENT_TYPE.mapped_size).contains(&source) {
            return Err(Error::new("Unknown removed native component"));
        }
        if !to_client
            && if source == 40 || source == 41 {
                M::SWING_OLD < 0
            } else {
                M::DATA_COMPONENT_TYPE.inverse(source).is_err()
            }
        {
            continue;
        }
        if to_client && source == M::MAP_COLOR_OLD {
            backups.push(BackupValue {
                source,
                mapped_id: -1,
                original: Vec::new(),
                mapped: Vec::new(),
                removed: true,
                synthetic: false,
            });
            continue;
        }
        if to_client && source == M::SWING_OLD {
            removals.extend([40, 41]);
            continue;
        }
        removals.push(if !to_client && (source == 40 || source == 41) {
            M::SWING_OLD
        } else {
            if to_client {
                M::DATA_COMPONENT_TYPE.map(source)
            } else {
                M::DATA_COMPONENT_TYPE.inverse(source)
            }
            .map_err(Error::new)?
        });
    }
    if M::PROTOCOL < 768 && to_client && !exact {
        let mut synthetic = Vec::new();
        if M::PROTOCOL < 767
            && item_hint
                .and_then(|id| usize::try_from(id).ok())
                .and_then(|id| M::ITEM_IDENTIFIERS.get(id))
                .is_some_and(|name| matches!(*name, "trident" | "piglin_banner_pattern"))
            && !entries.iter().any(|(id, _)| *id == 12)
        {
            synthetic.push((12, vec![0]));
        }
        if entries.iter().any(|(id, data)| *id == 13 && data == &[0])
            && backups.iter().any(|backup| {
                backup.mapped_id == 13 && Reader::new(&backup.original).varint().unwrap_or(0) > 0
            })
            && !entries.iter().any(|(id, _)| *id == 21)
            && !removals.contains(&21)
        {
            synthetic.push((21, vec![1]));
        }
        let override_name = item_hint
            .and_then(|id| usize::try_from(id).ok())
            .and_then(|id| M::ITEM_IDENTIFIERS.get(id))
            .is_some_and(|name| {
                matches!(
                    *name,
                    "compass"
                        | "player_head"
                        | "potion"
                        | "shield"
                        | "tipped_arrow"
                        | "splash_potion"
                        | "lingering_potion"
                )
            });
        if override_name && !entries.iter().any(|(id, _)| *id == 6) {
            if let Some((_, name)) = entries.iter().find(|(id, _)| *id == 9) {
                let name = Reader::new(name).nbt()?;
                let mut value = Writer::new();
                value.nbt(&Tag::compound([
                    ("text", Tag::String("".into())),
                    ("italic", Tag::Byte(0)),
                    ("extra", Tag::List(vec![name])),
                ]));
                synthetic.push((6, value.data));
            }
        }
        for (native, value) in synthetic {
            let source = M::DATA_COMPONENT_TYPE.inverse(native).map_err(Error::new)?;
            backups.push(BackupValue {
                source,
                mapped_id: native,
                original: Vec::new(),
                mapped: value.clone(),
                removed: false,
                synthetic: true,
            });
            entries.push((native, value));
            removals.retain(|id| *id != native);
        }
    }
    if !exact {
        if to_client {
            save_backup(&mut entries, &mut removals, backups)?;
        } else {
            restore_backup_using::<M>(&mut entries, &observed, &mut removals, _registry)?;
        }
    }
    if M::PROTOCOL < 770 {
        if to_client {
            old_patch.finish_forward(&mut entries, &mut removals)?;
        } else {
            old_patch.finish_reverse(&mut entries, &mut removals)?;
        }
    }
    if M::PROTOCOL < 768 {
        if to_client {
            food_patch.finish_forward(&mut entries, &mut removals);
        } else {
            food_patch.finish_reverse::<M>(&mut entries)?;
        }
    }
    removals.sort_unstable();
    removals.dedup();
    if entries.iter().any(|(id, _)| removals.contains(id)) {
        return Err(Error::new("Conflicting converted component patch"));
    }
    w.varint(entries.len() as i32);
    if !exact {
        w.varint(removals.len() as i32);
    }
    for (id, data) in entries {
        w.varint(id);
        if delimited && !(M::PROTOCOL < 770 && !to_client) {
            w.byte_array(&data);
        } else {
            w.bytes(&data);
        }
    }
    for id in removals {
        w.varint(id);
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
        .and_then(|id| t::OLD_CLIENTBOUND.get(id))
        .copied()
        .ok_or_else(|| Error::new("Invalid old packet ID"))?;
    if !matches!(
        name,
        "CONTAINER_SET_CONTENT"
            | "CONTAINER_SET_SLOT"
            | "SET_CURSOR_ITEM"
            | "SET_PLAYER_INVENTORY"
            | "SET_EQUIPMENT"
    ) {
        return crate::vfp_world::clientbound_using::<M>(id, payload, _registry);
    }
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    match name {
        "CONTAINER_SET_CONTENT" => {
            w.varint(r.varint()?);
            w.varint(r.varint()?);
            let count = r.count(65536)?;
            w.varint(count as i32);
            for _ in 0..count {
                stack_using::<M>(&mut r, &mut w, true, false, _registry)?;
            }
            stack_using::<M>(&mut r, &mut w, true, false, _registry)?;
        }
        "CONTAINER_SET_SLOT" => {
            w.varint(r.varint()?);
            w.varint(r.varint()?);
            w.i16(r.i16()?);
            stack_using::<M>(&mut r, &mut w, true, false, _registry)?;
        }
        "SET_PLAYER_INVENTORY" => {
            w.varint(r.varint()?);
            stack_using::<M>(&mut r, &mut w, true, false, _registry)?;
        }
        "SET_CURSOR_ITEM" => {
            stack_using::<M>(&mut r, &mut w, true, false, _registry)?;
        }
        "SET_EQUIPMENT" => {
            w.varint(r.varint()?);
            let mut ended = false;
            for _ in 0..16 {
                let slot = r.u8()?;
                w.u8(slot);
                stack_using::<M>(&mut r, &mut w, true, false, _registry)?;
                if slot & 128 == 0 {
                    ended = true;
                    break;
                }
            }
            if !ended {
                return Err(Error::new("Too many equipment entries"));
            }
        }
        _ => unreachable!(),
    }
    if r.remaining() != 0 {
        return Err(Error::new("Trailing inventory packet bytes"));
    }
    Ok(vec![(
        wire::packet_id(t::NEW_CLIENTBOUND, name).map_err(Error::new)?,
        w.data,
    )])
}

pub fn serverbound_using<M: Maps>(
    id: i32,
    payload: &[u8],
    _registry: &crate::vfp_registry_access::Registries,
) -> Result<Vec<wire::Packet>> {
    let name = usize::try_from(id)
        .ok()
        .and_then(|i| t::NEW_SERVERBOUND.get(i))
        .copied()
        .ok_or_else(|| Error::new("Invalid serverbound item packet"))?;
    let mut writer = Writer::new();
    match name {
        "SET_CREATIVE_MODE_SLOT" => {
            let mut reader = Reader::new(payload);
            writer.i16(reader.i16()?);
            let mut item = Reader::new(&payload[reader.position()..]);
            if item.varint()? > 0 {
                let id = item.varint()?;
                if !(0..M::ITEMS.mapped_size).contains(&id) {
                    return Err(Error::new("Unknown native creative item"));
                }
                if M::ITEMS.inverse(id).is_err() {
                    return Ok(Vec::new());
                }
            }
            stack_using::<M>(&mut reader, &mut writer, false, true, _registry)?;
            if reader.remaining() != 0 {
                return Err(Error::new("Trailing creative slot bytes"));
            }
        }
        "CONTAINER_CLICK" => {
            // Session normally supplies its connection-local restoration cache.
            // Standalone callers use the same safe resynchronization fallback.
            return crate::vfp_prediction::Predictions::default().click_with::<M>(id, payload);
        }
        "SPECTATOR_ACTION" => {
            return Ok(vec![(
                wire::packet_id(t::OLD_SERVERBOUND, "SPECTATE_ENTITY").map_err(Error::new)?,
                payload.to_vec(),
            )]);
        }
        _ => return crate::vfp_play::serverbound_262(id, payload),
    }
    Ok(vec![(
        wire::packet_id(t::OLD_SERVERBOUND, name).map_err(Error::new)?,
        writer.data,
    )])
}

pub(crate) fn stable_hash(id: i32) -> bool {
    matches!(id,1..=12|17|19..=23|26|29..=31|34|35|47|48|50|54|65|75)
}

pub fn clientbound_262(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    clientbound_using::<V262>(
        id,
        payload,
        &crate::vfp_registry_access::Registries::default(),
    )
}
pub fn serverbound_262(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    serverbound_using::<V262>(
        id,
        payload,
        &crate::vfp_registry_access::Registries::default(),
    )
}
pub fn stack_262(
    r: &mut Reader<'_>,
    w: &mut Writer,
    to_client: bool,
    delimited: bool,
) -> Result<()> {
    stack_using::<V262>(
        r,
        w,
        to_client,
        delimited,
        &crate::vfp_registry_access::Registries::default(),
    )
}
pub fn template_262(r: &mut Reader<'_>, w: &mut Writer, to_client: bool) -> Result<()> {
    template_using::<V262>(
        r,
        w,
        to_client,
        &crate::vfp_registry_access::Registries::default(),
    )
}

pub fn stack_with<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    to_client: bool,
    delimited: bool,
) -> Result<()> {
    stack_using::<M>(
        r,
        w,
        to_client,
        delimited,
        &crate::vfp_registry_access::Registries::default(),
    )
}

pub fn template_with<M: Maps>(r: &mut Reader<'_>, w: &mut Writer, to_client: bool) -> Result<()> {
    template_using::<M>(
        r,
        w,
        to_client,
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

pub fn serverbound_with<M: Maps>(id: i32, payload: &[u8]) -> Result<Vec<wire::Packet>> {
    serverbound_using::<M>(
        id,
        payload,
        &crate::vfp_registry_access::Registries::default(),
    )
}
