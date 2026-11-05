// SPDX-License-Identifier: GPL-3.0-or-later
// Registry-sensitive fields are converted at each old Release edge before
// entering the established 1.19.3 -> 26.3 chain. No mixed numeric domains.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_release_chain::{self as chain, Step},
    vfp_release_codec::{self as codec, PaletteFormat, Stack},
};
use horizon_world::nbt::Tag;

pub fn chunk_blocks(step: &Step, r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    w.bytes(r.bytes(8)?);
    let count = r.count(65536)?;
    w.varint(count as i32);
    for _ in 0..count {
        w.i16(r.i16()?);
        w.varint(step.same_or_map("blockstates", r.varint()?, false)?);
    }
    Ok(())
}

pub fn item(step: &Step, inverse: bool, r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    let (source, target) = if inverse {
        (step.target, step.source)
    } else {
        (step.source, step.target)
    };
    let mut value = Stack::read(r, source)?;
    stack(step, inverse, &mut value)?;
    Stack::write(value.as_ref(), w, target)
}
pub fn stack(step: &Step, inverse: bool, value: &mut Option<Stack>) -> Result<()> {
    if let Some(stack) = value {
        if step.source == 340 {
            flatten_item(step, inverse, stack)?;
        } else if step.source == 210 {
            crate::vfp_release_110::stack(stack, inverse);
        } else if step.source == 316 {
            if stack.id == 355 {
                stack.damage = if inverse { 0 } else { 14 };
            }
        } else {
            stack.id = step.same_or_map("items", stack.id, inverse)?;
        }
        if step.source == 404 {
            lore_114(stack, inverse)?;
        }
    }
    Ok(())
}
fn lore_114(stack: &mut Stack, inverse: bool) -> Result<()> {
    const SAVED: &str = "viafabricplus:1_13_lore";
    let Some(Tag::Compound(display)) = stack.tag.get_mut("display") else {
        return Ok(());
    };
    if inverse {
        let original = display.remove(&SAVED.into());
        let Some(Tag::List(lore)) = display.get_mut(&"Lore".into()) else {
            return Ok(());
        };
        if let Some(Tag::List(original)) = original {
            let unchanged = original.len() == lore.len() && original.iter().zip(lore.iter()).all(|(old, new)| {
                matches!((old,new), (Tag::String(old),Tag::String(new)) if crate::vfp_release_text::json(&old.to_string_lossy()) == new.to_string_lossy())
            });
            if unchanged {
                *lore = original;
                return Ok(());
            }
        }
        for line in lore {
            if let Tag::String(line) = line {
                *line = crate::vfp_release_text::legacy(&line.to_string_lossy())
                    .as_str()
                    .into();
            }
        }
    } else {
        let Some(Tag::List(lore)) = display.get(&"Lore".into()).cloned() else {
            return Ok(());
        };
        display.insert(SAVED.into(), Tag::List(lore.clone()));
        let converted = lore
            .into_iter()
            .map(|value| match value {
                Tag::String(line) => Tag::String(
                    crate::vfp_release_text::json(&line.to_string_lossy())
                        .as_str()
                        .into(),
                ),
                value => value,
            })
            .collect();
        display.insert("Lore".into(), Tag::List(converted));
    }
    Ok(())
}
pub fn creative(step: &Step, data: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    let slot = r.i16()?;
    let mut stack = Stack::read(&mut r, step.target)?;
    codec::end(&r)?;
    if let Some(stack) = &stack {
        if let Some(map) = step.registry("items") {
            match map.inverse(stack.id) {
                Ok(_) => (),
                Err(_) if (0..map.mapped_size).contains(&stack.id) => return Ok(None),
                Err(error) => return Err(Error::new(error)),
            }
        }
    }
    self::stack(step, true, &mut stack)?;
    w.i16(slot);
    Stack::write(stack.as_ref(), &mut w, step.source)?;
    Ok(Some(w.data))
}
fn damageable(id: i32) -> bool {
    matches!(id, 256..=259 | 261 | 267..=279 | 283..=286 | 290..=294 | 298..=317 | 346 | 359 | 398 | 442 | 443)
}
fn flatten_item(step: &Step, inverse: bool, stack: &mut Stack) -> Result<()> {
    use crate::vfp_release_chain_data::EGGS_112;
    const ORIGINAL: &str = "viafabricplus:pre_flattening";
    const SAVED_NAME: &str = "viafabricplus:1_12_name";
    if matches!(stack.tag, Tag::End) {
        stack.tag = Tag::Compound(Default::default());
    }
    let Tag::Compound(fields) = &mut stack.tag else {
        return Err(Error::new("Legacy item tag is not a compound"));
    };

    if let Some(Tag::Compound(display)) = fields.get_mut(&"display".into()) {
        if inverse {
            let original = display.remove(&SAVED_NAME.into());
            if let Some(Tag::String(name)) = display.get_mut(&"Name".into()) {
                let current = name.to_string_lossy();
                *name = match original {
                    Some(Tag::String(original))
                        if crate::vfp_release_text::json(&original.to_string_lossy())
                            == current =>
                    {
                        original
                    }
                    _ => crate::vfp_release_text::legacy(&current).as_str().into(),
                };
            }
        } else if let Some(Tag::String(name)) = display.get_mut(&"Name".into()) {
            let original = name.clone();
            *name = crate::vfp_release_text::json(&original.to_string_lossy())
                .as_str()
                .into();
            display.insert(SAVED_NAME.into(), Tag::String(original));
        }
    }
    let enchant_key = if inverse { "Enchantments" } else { "ench" };
    if let Some(Tag::List(mut enchants)) = fields.remove(&enchant_key.into()) {
        flatten_enchants(&mut enchants, inverse);
        fields.insert(
            if inverse {
                "ench".into()
            } else {
                "Enchantments".into()
            },
            Tag::List(enchants),
        );
    }
    if let Some(Tag::List(enchants)) = fields.get_mut(&"StoredEnchantments".into()) {
        flatten_enchants(enchants, inverse);
    }

    if inverse {
        let flat_id = stack.id;
        let key = step.inverse("items", flat_id)?;
        if key >> 16 == 383 {
            stack.id = 383;
            stack.damage = 0;
            if let Some(&name) = EGGS_112.get((key & 0xffff) as usize) {
                let entity = fields
                    .entry("EntityTag".into())
                    .or_insert_with(|| Tag::Compound(Default::default()));
                if let Tag::Compound(entity) = entity {
                    entity.insert("id".into(), Tag::String(name.into()));
                }
            }
        } else {
            stack.id = key >> 4;
            stack.damage = (key & 15) as i16;
        }
        if let Some(Tag::IntArray(original)) = fields.remove(&ORIGINAL.into()) {
            if original.len() == 3
                && (0..=i16::MAX as i32).contains(&original[0])
                && i16::try_from(original[1]).is_ok()
                && step.map("items", original[2]).ok() == Some(flat_id)
            {
                stack.id = original[0];
                stack.damage = original[1] as i16;
            }
        }
        let damage_key = if damageable(stack.id) {
            Some("Damage")
        } else if stack.id == 358 {
            Some("map")
        } else {
            None
        };
        if let Some(damage_key) = damage_key {
            if let Some(Tag::Int(damage)) = fields.remove(&damage_key.into()) {
                stack.damage = i16::try_from(damage)
                    .map_err(|_| Error::new("Legacy item damage exceeds short"))?;
            }
        }
    } else {
        let mut key = (stack.id << 4) | (i32::from(stack.damage) & 15);
        let damage_key = if damageable(stack.id) {
            Some("Damage")
        } else if stack.id == 358 {
            Some("map")
        } else {
            None
        };
        if let Some(damage_key) = damage_key {
            key = stack.id << 4;
            fields.insert(damage_key.into(), Tag::Int(i32::from(stack.damage)));
        }
        if stack.id == 383 {
            let mut egg = None;
            if let Some(Tag::Compound(entity)) = fields.get_mut(&"EntityTag".into()) {
                if let Some(Tag::String(name)) = entity.get(&"id".into()) {
                    let name = name.to_string_lossy();
                    egg = EGGS_112.iter().position(|&candidate| candidate == name);
                    if egg.is_some() {
                        entity.remove(&"id".into());
                    }
                }
                if entity.is_empty() {
                    fields.remove(&"EntityTag".into());
                }
            }
            key = (383 << 16) | egg.unwrap_or(0) as i32;
        }
        let mapped = step
            .map("items", key)
            .or_else(|_| {
                key = stack.id << 4;
                step.map("items", key)
            })
            .or_else(|_| {
                key = 16;
                step.map("items", key)
            })?;
        fields.insert(
            ORIGINAL.into(),
            Tag::IntArray(vec![stack.id, i32::from(stack.damage), key]),
        );
        stack.id = mapped;
        stack.damage = 0;
    }
    if fields.is_empty() {
        stack.tag = Tag::End;
    }
    Ok(())
}

fn flatten_enchants(enchants: &mut [Tag], inverse: bool) {
    use crate::vfp_release_chain_data::ENCHANTS_112;
    const LEGACY: &str = "viaversion:legacy/";
    for enchantment in enchants {
        let Some(id) = enchantment.get_mut("id") else {
            continue;
        };
        if inverse {
            if let Tag::String(name) = id {
                let name = name.to_string_lossy();
                if let Some(value) = ENCHANTS_112
                    .iter()
                    .find_map(|&(value, key)| (key == name).then_some(value))
                    .or_else(|| {
                        name.strip_prefix(LEGACY)
                            .and_then(|value| value.parse::<i16>().ok())
                    })
                {
                    *id = Tag::Short(value);
                }
            }
        } else if let Tag::Short(value) = id {
            let name = ENCHANTS_112
                .iter()
                .find_map(|&(number, name)| (number == *value).then_some(name))
                .map_or_else(|| format!("{LEGACY}{value}"), str::to_owned);
            *id = Tag::String(name.as_str().into());
        }
    }
}
pub fn ingredient(step: &Step, inverse: bool, r: &mut Reader<'_>, w: &mut Writer) -> Result<()> {
    let count = r.count(4096)?;
    w.varint(count as i32);
    for _ in 0..count {
        item(step, inverse, r, w)?;
    }
    Ok(())
}
pub fn recipes(step: &Step, data: &[u8], add_category: bool) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    let count = r.count(65536)?;
    w.varint(count as i32);
    for _ in 0..count {
        let kind = r.string(32767)?;
        w.string(&kind);
        w.string(&r.string(32767)?);
        match kind.strip_prefix("minecraft:").unwrap_or(&kind) {
            "crafting_shaped" => {
                let width = r.varint()?;
                let height = r.varint()?;
                if !(1..=3).contains(&width) || !(1..=3).contains(&height) {
                    return Err(Error::new("Invalid legacy shaped recipe size"));
                }
                w.varint(width);
                w.varint(height);
                w.string(&r.string(32767)?);
                if add_category {
                    w.varint(0);
                }
                for _ in 0..width * height {
                    ingredient(step, false, &mut r, &mut w)?;
                }
                item(step, false, &mut r, &mut w)?;
            }
            "crafting_shapeless" => {
                w.string(&r.string(32767)?);
                if add_category {
                    w.varint(0);
                }
                let ingredients = r.count(9)?;
                w.varint(ingredients as i32);
                for _ in 0..ingredients {
                    ingredient(step, false, &mut r, &mut w)?;
                }
                item(step, false, &mut r, &mut w)?;
            }
            "smelting" | "blasting" | "smoking" | "campfire_cooking" => {
                w.string(&r.string(32767)?);
                if add_category {
                    w.varint(0);
                }
                ingredient(step, false, &mut r, &mut w)?;
                item(step, false, &mut r, &mut w)?;
                w.f32(r.f32()?);
                w.varint(r.varint()?);
            }
            "stonecutting" => {
                w.string(&r.string(32767)?);
                ingredient(step, false, &mut r, &mut w)?;
                item(step, false, &mut r, &mut w)?;
            }
            "smithing" => {
                ingredient(step, false, &mut r, &mut w)?;
                ingredient(step, false, &mut r, &mut w)?;
                item(step, false, &mut r, &mut w)?;
            }
            "crafting_special_armordye"
            | "crafting_special_bookcloning"
            | "crafting_special_mapcloning"
            | "crafting_special_mapextending"
            | "crafting_special_firework_rocket"
            | "crafting_special_firework_star"
            | "crafting_special_firework_star_fade"
            | "crafting_special_tippedarrow"
            | "crafting_special_bannerduplicate"
            | "crafting_special_shielddecoration"
            | "crafting_special_shulkerboxcoloring"
            | "crafting_special_suspiciousstew"
            | "crafting_special_repairitem" => {
                if add_category {
                    w.varint(0);
                }
            }
            _ => return Err(Error::new(format!("Unconverted legacy recipe {kind}"))),
        }
    }
    codec::end(&r)?;
    Ok(w.data)
}

pub fn particle(step: &Step, r: &mut Reader<'_>, w: &mut Writer, header: bool) -> Result<()> {
    let id = if header && step.source < 759 {
        r.i32()?
    } else {
        r.varint()?
    };
    let name = chain::identifier(step.source, "particles", id)?;
    let marker =
        (step.source == 756 && (id == 2 || id == 3)).then_some(if id == 2 { 7754 } else { 7786 });
    let mapped = if marker.is_some() {
        3
    } else {
        step.same_or_map("particles", id, false)?
    };
    if header && step.target < 759 {
        w.i32(mapped);
    } else {
        w.varint(mapped);
    }
    if header {
        w.bool(codec::boolean(r)?);
        for _ in 0..3 {
            let value = if step.source < 573 {
                f64::from(r.f32()?)
            } else {
                r.f64()?
            };
            if step.target < 573 {
                w.f32(value as f32);
            } else {
                w.f64(value);
            }
        }
        w.bytes(r.bytes(12 + 4 + 4)?);
    }
    if let Some(state) = marker {
        w.varint(state);
        return Ok(());
    }
    match name {
        "block" | "block_marker" | "falling_dust" => {
            w.varint(step.same_or_map("blockstates", r.varint()?, false)?);
        }
        "dust" => {
            w.bytes(r.bytes(16)?);
        }
        "dust_color_transition" => {
            w.bytes(r.bytes(28)?);
        }
        "item" => {
            item(step, false, r, w)?;
        }
        "vibration" => {
            if step.source < 759 {
                let origin = r.i64()?;
                if step.target < 759 {
                    w.i64(origin);
                }
            }
            let destination = r.string(32767)?;
            w.string(&destination);
            match destination
                .strip_prefix("minecraft:")
                .unwrap_or(&destination)
            {
                "block" => {
                    w.i64(r.i64()?);
                }
                "entity" => {
                    w.varint(r.varint()?);
                    let offset = if step.source < 759 { 0.0 } else { r.f32()? };
                    if step.target >= 759 {
                        w.f32(offset);
                    }
                }
                _ => return Err(Error::new("Unknown legacy vibration destination")),
            }
            w.varint(r.varint()?);
        }
        "sculk_charge" => {
            w.f32(r.f32()?);
        }
        "shriek" => {
            w.varint(r.varint()?);
        }
        _ => (),
    }
    Ok(())
}
pub fn advancements(step: &Step, data: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    w.bool(codec::boolean(&mut r)?);
    let count = r.count(65536)?;
    w.varint(count as i32);
    for _ in 0..count {
        w.string(&r.string(32767)?);
        let parent = codec::boolean(&mut r)?;
        w.bool(parent);
        if parent {
            w.string(&r.string(32767)?);
        }
        let display = codec::boolean(&mut r)?;
        w.bool(display);
        if display {
            w.string(&r.string(262144)?);
            w.string(&r.string(262144)?);
            item(step, false, &mut r, &mut w)?;
            w.varint(r.varint()?);
            let flags = r.i32()?;
            w.i32(flags);
            if flags & 1 != 0 {
                w.string(&r.string(32767)?);
            }
            w.f32(r.f32()?);
            w.f32(r.f32()?);
        }
        let criteria = r.count(65536)?;
        w.varint(criteria as i32);
        for _ in 0..criteria {
            w.string(&r.string(32767)?);
        }
        let requirements = r.count(65536)?;
        w.varint(requirements as i32);
        for _ in 0..requirements {
            let group = r.count(65536)?;
            w.varint(group as i32);
            for _ in 0..group {
                w.string(&r.string(32767)?);
            }
        }
    }
    w.bytes(r.rest());
    Ok(w.data)
}
pub fn metadata_119(step: &Step, data: &[u8], entity: &str) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let mut output = Writer::new();
    output.varint(r.varint()?);
    let mut entries = 0usize;
    loop {
        let index = r.u8()?;
        if index == 255 {
            output.u8(255);
            break;
        }
        let mut mapped_index = if step.source == 754 && index >= 7 {
            index
                .checked_add(1)
                .ok_or_else(|| Error::new("Old metadata index overflow"))?
        } else {
            index
        };
        if step.source == 736 && entity == "piglin" {
            mapped_index = match mapped_index {
                15 => 16,
                16 => 15,
                value => value,
            };
        }
        let mut removed = step.source == 754 && entity == "shulker" && mapped_index == 17;
        if step.source == 498 {
            if crate::vfp_release_chain_data::LIVING_114.contains(&entity) && mapped_index >= 12 {
                mapped_index = mapped_index
                    .checked_add(1)
                    .ok_or_else(|| Error::new("Old living metadata index overflow"))?;
            }
            if entity == "wolf" {
                if mapped_index == 18 {
                    removed = true;
                } else if mapped_index > 18 {
                    mapped_index -= 1;
                }
            }
        }
        if step.source == 477
            && matches!(entity, "villager" | "wandering_trader")
            && mapped_index >= 15
        {
            mapped_index = mapped_index
                .checked_add(1)
                .ok_or_else(|| Error::new("Old villager metadata index overflow"))?;
        }
        if step.source == 578 && matches!(entity, "arrow" | "spectral_arrow" | "trident") {
            if index == 8 {
                removed = true;
            } else if index > 8 {
                mapped_index -= 1;
            }
        }
        if step.source == 754 && entity == "shulker" && mapped_index > 17 {
            mapped_index -= 1;
        }
        if mapped_index == 255 {
            return Err(Error::new(
                "Old metadata index collides with list terminator",
            ));
        }
        let mut w = Writer::new();
        w.u8(mapped_index);
        entries += 1;
        if entries > 255 {
            return Err(Error::new("Too many legacy entity data entries"));
        }
        let serializer = r.varint()?;
        if !(0..=22).contains(&serializer) {
            return Err(Error::new("Invalid old metadata serializer"));
        }
        let mut wolf_anger = None;
        w.varint(if step.target == 761 && serializer >= 2 {
            serializer + 1
        } else if step.source == 758 && entity == "cat" && index == 19 {
            19
        } else {
            serializer
        });
        match serializer {
            0 => {
                let value = r.i8()?;
                w.i8(value);
                if step.source == 578 && entity == "wolf" && index == 16 {
                    wolf_anger = Some(if value & 2 != 0 { i32::MAX } else { 0 });
                }
            }
            1 => {
                let value = r.varint()?;
                w.varint(
                    if entity.contains("minecart")
                        && index == if step.source < 755 { 10 } else { 11 }
                    {
                        step.same_or_map("blockstates", value, false)?
                    } else {
                        value
                    },
                );
            }
            2 => {
                w.f32(r.f32()?);
            }
            3 | 4 => {
                w.string(&r.string(262144)?);
            }
            5 => {
                codec::optional_text(&mut r, &mut w)?;
            }
            6 => {
                item(step, false, &mut r, &mut w)?;
            }
            7 => {
                w.bool(codec::boolean(&mut r)?);
            }
            8 => {
                w.bytes(r.bytes(12)?);
            }
            9 => {
                w.i64(r.i64()?);
            }
            10 => {
                let present = codec::boolean(&mut r)?;
                w.bool(present);
                if present {
                    w.i64(r.i64()?);
                }
            }
            11 | 17 | 19 | 20 | 22 => {
                w.varint(r.varint()?);
            }
            12 => {
                let present = codec::boolean(&mut r)?;
                w.bool(present);
                if present {
                    w.bytes(r.bytes(16)?);
                }
            }
            13 => {
                let state = r.varint()?;
                w.varint(if state == 0 {
                    0
                } else {
                    step.same_or_map("blockstates", state, false)?
                });
            }
            14 => {
                w.vfp_named_nbt(&r.vfp_named_nbt()?)?;
            }
            15 => {
                particle(step, &mut r, &mut w, false)?;
            }
            16 => {
                for _ in 0..3 {
                    w.varint(r.varint()?);
                }
            }
            18 => {
                let pose = r.varint()?;
                w.varint(
                    if (step.target == 761 && pose >= 10) || (step.source == 754 && pose >= 6) {
                        pose.checked_add(1)
                            .ok_or_else(|| Error::new("Old entity pose overflow"))?
                    } else {
                        pose
                    },
                );
            }
            21 => {
                let present = codec::boolean(&mut r)?;
                w.bool(present);
                if present {
                    w.string(&r.string(32767)?);
                    w.i64(r.i64()?);
                }
            }
            _ => {
                return Err(Error::new(format!(
                    "Unconverted legacy metadata serializer {serializer}"
                )));
            }
        }
        if !removed {
            output.bytes(&w.data);
        }
        if let Some(anger) = wolf_anger {
            output.u8(20);
            output.varint(1);
            output.varint(anger);
        }
    }
    codec::end(&r)?;
    Ok(output.data)
}
pub fn chunk_118(step: &Step, data: &[u8], sections: usize, biome_bits: u8) -> Result<Vec<u8>> {
    if !(1..=256).contains(&sections) {
        return Err(Error::new("Invalid legacy dimension section count"));
    }
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    w.bytes(r.bytes(8)?);
    w.vfp_named_nbt(&r.vfp_named_nbt()?)?;
    let body = r.byte_array(crate::vfp_core::MAX_PACKET)?;
    let mut section_reader = Reader::new(&body);
    let mut out = Writer::new();
    let map = step.registry("blockstates");
    let source_bits = map.map_or(15, |map| (32 - (map.size - 1).leading_zeros()) as u8);
    let target_bits = map.map_or(15, |map| (32 - (map.mapped_size - 1).leading_zeros()) as u8);
    for _ in 0..sections {
        let old_count = section_reader.i16()?;
        let mut blocks =
            PaletteFormat::blocks(step.source, source_bits).read(&mut section_reader)?;
        for state in &mut blocks {
            *state = step.same_or_map("blockstates", *state, false)?;
        }
        out.i16(old_count);
        PaletteFormat::blocks(step.target, target_bits).write(&mut out, &blocks)?;
        let biomes = PaletteFormat::biomes(biome_bits).read(&mut section_reader)?;
        PaletteFormat::biomes(biome_bits).write(&mut out, &biomes)?;
    }
    crate::vfp_wire::section_padding(&body, section_reader.position(), Some(sections))
        .map_err(Error::new)?;
    w.byte_array(&out.data);
    let count = r.count(65536)?;
    w.varint(count as i32);
    for _ in 0..count {
        w.u8(r.u8()?);
        w.i16(r.i16()?);
        w.varint(step.same_or_map("blockentities", r.varint()?, false)?);
        w.vfp_named_nbt(&r.vfp_named_nbt()?)?;
    }
    // Light packet format and its dynamic dimension masks are identical here.
    w.bytes(r.rest());
    Ok(w.data)
}

pub fn tags(step: &Step, data: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    let count = r.count(1024)?;
    w.varint(count as i32);
    for _ in 0..count {
        let registry = r.string(32767)?;
        w.string(&registry);
        let mapping = match registry.as_str() {
            "minecraft:block" => Some("blocks"),
            "minecraft:item" => Some("items"),
            "minecraft:entity_type" => Some("entities"),
            // Other registries, including datapack registries such as banner
            // patterns, retain their server-side IDs at these edges.
            _ => None,
        };
        let tags = r.count(65536)?;
        w.varint(tags as i32);
        for _ in 0..tags {
            w.string(&r.string(32767)?);
            let entries = r.count(65536)?;
            w.varint(entries as i32);
            for _ in 0..entries {
                let id = r.varint()?;
                w.varint(if let Some(registry) = mapping {
                    step.same_or_map(registry, id, false)?
                } else {
                    id
                });
            }
        }
    }
    codec::end(&r)?;
    Ok(w.data)
}
pub fn tags_1164(step: &Step, data: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    if step.target >= 755 {
        w.varint(5);
    }
    for (name, mapping) in [
        ("minecraft:block", Some("blocks")),
        ("minecraft:item", Some("items")),
        ("minecraft:fluid", None),
        ("minecraft:entity_type", Some("entities")),
    ] {
        if step.target >= 755 {
            w.string(name);
        }
        let count = r.count(65536)?;
        w.varint(count as i32);
        for _ in 0..count {
            w.string(&r.string(32767)?);
            let entries = r.count(65536)?;
            w.varint(entries as i32);
            for _ in 0..entries {
                let id = r.varint()?;
                w.varint(if let Some(mapping) = mapping {
                    step.same_or_map(mapping, id, false)?
                } else {
                    id
                });
            }
        }
    }
    if step.target >= 755 {
        w.string("minecraft:game_event");
        w.varint(0);
    }
    codec::end(&r)?;
    Ok(w.data)
}
pub fn commands(step: &Step, data: &[u8]) -> Result<Vec<u8>> {
    if step.target < 759 && step.source != 404 {
        return Ok(data.to_vec());
    }
    let mut r = Reader::new(data);
    let mut w = Writer::new();
    let count = r.count(65536)?;
    w.varint(count as i32);
    for _ in 0..count {
        let flags = r.u8()?;
        w.u8(flags);
        let children = r.count(count)?;
        w.varint(children as i32);
        for _ in 0..children {
            let node = r.count(count.saturating_sub(1))?;
            w.varint(node as i32);
        }
        if flags & 8 != 0 {
            w.varint(r.count(count.saturating_sub(1))? as i32);
        }
        match flags & 3 {
            0 => (),
            1 => {
                w.string(&r.string(32767)?);
            }
            2 => {
                w.string(&r.string(32767)?);
                let (name, mapped) = if step.source >= 759 {
                    let id = r.varint()?;
                    (
                        chain::identifier(step.source, "argumenttypes", id)?.to_owned(),
                        step.same_or_map("argumenttypes", id, false)?,
                    )
                } else {
                    let name = r.string(32767)?;
                    let mapped = if step.target >= 759 {
                        chain::named_id(
                            step.target,
                            "argumenttypes",
                            name.strip_prefix("minecraft:").unwrap_or(&name),
                        )?
                    } else {
                        0
                    };
                    (name, mapped)
                };
                if step.target >= 759 {
                    w.varint(mapped);
                } else {
                    w.string(if step.source == 404 && name == "minecraft:nbt" {
                        "minecraft:nbt_compound_tag"
                    } else {
                        &name
                    });
                }
                match name.strip_prefix("minecraft:").unwrap_or(&name) {
                    "brigadier:double" | "brigadier:long" | "brigadier:float"
                    | "brigadier:integer" => {
                        let properties = r.u8()?;
                        w.u8(properties);
                        let width =
                            if matches!(name.as_str(), "brigadier:double" | "brigadier:long") {
                                8
                            } else {
                                4
                            };
                        if properties & 1 != 0 {
                            w.bytes(r.bytes(width)?);
                        }
                        if properties & 2 != 0 {
                            w.bytes(r.bytes(width)?);
                        }
                    }
                    "brigadier:string" => {
                        w.varint(r.varint()?);
                    }
                    "entity" | "score_holder" => {
                        w.u8(r.u8()?);
                    }
                    "resource" | "resource_or_tag" | "resource_key" | "resource_or_tag_key" => {
                        w.string(&r.string(32767)?);
                    }
                    "item_enchantment" if step.target == 761 => {
                        w.string("minecraft:enchantment");
                    }
                    "mob_effect" if step.target == 761 => {
                        w.string("minecraft:mob_effect");
                    }
                    "entity_summon" if step.target == 761 => {
                        w.string("minecraft:entity_type");
                    }
                    _ => (),
                }
            }
            _ => return Err(Error::new("Invalid old command node kind")),
        }
        if flags & 16 != 0 {
            w.string(&r.string(32767)?);
        }
    }
    w.varint(r.count(count.saturating_sub(1))? as i32);
    codec::end(&r)?;
    Ok(w.data)
}
