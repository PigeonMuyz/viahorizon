// SPDX-License-Identifier: GPL-3.0-or-later
// Legacy NBT is first encoded in the 1.20.5 component domain. Item, block and
// entity ids retain their original domain and are mapped only by the final
// codecs. This module is not a protocol capability or a configuration adapter.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    item::{Component, FireworkExplosion},
    vfp_mapping::{Maps, V1205},
    vfp_registry_1203 as names,
    vfp_registry_access::Registries,
    vfp_wire::IdMap,
};
use horizon_world::nbt::{JavaString, Tag};
use std::collections::BTreeMap;
pub const ORIGINAL: &str = "viafabricplus:legacy_nbt";
pub struct Legacy<M: Maps>(std::marker::PhantomData<M>);
macro_rules! delegate {
    ($($field:ident),*)=>{$(const $field:IdMap=M::$field;)*};
}
impl<M: Maps> Maps for Legacy<M> {
    const PROTOCOL: i32 = 766;
    const DATA_VERSION: Option<i32> = M::DATA_VERSION;
    delegate!(
        ARGUMENTTYPES,
        BLOCKENTITIES,
        BLOCKS,
        BLOCKSTATES,
        ENTITIES,
        ITEMS,
        PARTICLES,
        SOUNDS,
        STATISTICS
    );
    const ATTRIBUTES: IdMap = V1205::ATTRIBUTES;
    const DATA_COMPONENT_TYPE: IdMap = V1205::DATA_COMPONENT_TYPE;
    const BRICK_OLD: i32 = M::BRICK_OLD;
    const MAP_COLOR_OLD: i32 = V1205::MAP_COLOR_OLD;
    const BED_OLD: Option<i32> = M::BED_OLD;
    const COMPONENT_IDENTIFIERS: &'static [&'static str] = V1205::COMPONENT_IDENTIFIERS;
    const ENTITY_IDENTIFIERS: &'static [&'static str] = M::ENTITY_IDENTIFIERS;
    const BLOCK_ENTITY_IDENTIFIERS: &'static [&'static str] = M::BLOCK_ENTITY_IDENTIFIERS;
    const ITEM_IDENTIFIERS: &'static [&'static str] = M::ITEM_IDENTIFIERS;
    const SOUND_IDENTIFIERS: &'static [&'static str] = M::SOUND_IDENTIFIERS;
}
fn local(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}
fn real(tag: &Tag, key: &str, default: f32) -> f32 {
    match tag.get(key) {
        Some(Tag::Float(value)) => f32::from_bits(*value),
        Some(Tag::Double(value)) => f64::from_bits(*value) as f32,
        Some(value) => number(value).map_or(default, |value| value as f32),
        None => default,
    }
}
fn item_name<M: Maps>(id: i32) -> Result<Tag> {
    let name = usize::try_from(id)
        .ok()
        .and_then(|id| M::ITEM_IDENTIFIERS.get(id))
        .ok_or_else(|| Error::new("Unknown item in legacy inline value"))?;
    Ok(Tag::String(format!("minecraft:{name}").as_str().into()))
}
fn sound_to_wire<M: Maps>(tag: &Tag, w: &mut Writer) -> Result<()> {
    if let Some(name) = text(tag) {
        if let Some(id) = M::SOUND_IDENTIFIERS
            .iter()
            .position(|value| *value == local(&name))
        {
            w.varint(id as i32 + 1);
        } else {
            w.varint(0);
            w.string(&name);
            w.bool(false);
        }
    } else {
        let id = string(tag, "sound_id")
            .ok_or_else(|| Error::new("Inline instrument has no sound id"))?;
        w.varint(0);
        w.string(&id);
        w.bool(tag.get("range").is_some());
        if tag.get("range").is_some() {
            w.f32(real(tag, "range", 16.0));
        }
    }
    Ok(())
}
fn sound_from_wire<M: Maps>(r: &mut Reader<'_>) -> Result<Tag> {
    let holder = r.varint()?;
    if holder < 0 {
        return Err(Error::new("Negative legacy instrument sound holder"));
    }
    if holder == 0 {
        let mut fields = BTreeMap::new();
        fields.insert(
            "sound_id".into(),
            Tag::String(r.string(32767)?.as_str().into()),
        );
        if r.bool()? {
            fields.insert("range".into(), Tag::Float(r.f32()?.to_bits()));
        }
        Ok(Tag::Compound(fields))
    } else {
        let name = usize::try_from(holder - 1)
            .ok()
            .and_then(|id| M::SOUND_IDENTIFIERS.get(id))
            .ok_or_else(|| Error::new("Unknown legacy instrument sound holder"))?;
        Ok(Tag::String(format!("minecraft:{name}").as_str().into()))
    }
}
fn number(tag: &Tag) -> Option<i32> {
    match tag {
        Tag::Byte(n) => Some(i32::from(*n)),
        Tag::Short(n) => Some(i32::from(*n)),
        Tag::Int(n) => Some(*n),
        Tag::Long(n) => Some(*n as i32),
        Tag::Float(n) => Some(f32::from_bits(*n) as i32),
        Tag::Double(n) => Some(f64::from_bits(*n) as i32),
        _ => None,
    }
}
fn int(tag: &Tag, key: &str, default: i32) -> i32 {
    tag.get(key).and_then(number).unwrap_or(default)
}
fn text(tag: &Tag) -> Option<String> {
    if let Tag::String(s) = tag {
        Some(s.to_string_lossy())
    } else {
        None
    }
}
fn string(tag: &Tag, key: &str) -> Option<String> {
    tag.get(key).and_then(text)
}
fn list<'a>(tag: &'a Tag, key: &str) -> &'a [Tag] {
    match tag.get(key) {
        Some(Tag::List(values)) => values,
        _ => &[],
    }
}
fn uuid(tag: &Tag) -> Option<u128> {
    match tag {
        Tag::IntArray(values) if values.len() == 4 => Some(
            values
                .iter()
                .fold(0u128, |uuid, n| (uuid << 32) | u128::from(*n as u32)),
        ),
        Tag::String(value) => {
            u128::from_str_radix(&value.to_string_lossy().replace('-', ""), 16).ok()
        }
        _ => None,
    }
}
fn json(tag: &Tag) -> Result<Tag> {
    crate::vfp_text_1214::json_text(
        &text(tag).ok_or_else(|| Error::new("Legacy JSON text is not a string"))?,
        0,
    )
}
#[derive(Default)]
struct Patch {
    values: BTreeMap<i32, Vec<u8>>,
}
impl Patch {
    fn put(&mut self, name: &str, value: Writer) -> Result<()> {
        let id = V1205::COMPONENT_IDENTIFIERS
            .iter()
            .position(|key| *key == name)
            .ok_or_else(|| Error::new("Missing legacy component identifier"))?;
        self.values.insert(id as i32, value.data);
        Ok(())
    }
    fn value(&mut self, name: &str, value: &Tag) -> Result<()> {
        let mut w = Writer::new();
        w.nbt(value);
        self.put(name, w)
    }
    fn integer(&mut self, name: &str, value: i32) -> Result<()> {
        let mut w = Writer::new();
        w.varint(value);
        self.put(name, w)
    }
    fn finish(self, w: &mut Writer) {
        w.varint(self.values.len() as i32);
        w.varint(0);
        for (id, value) in self.values {
            w.varint(id);
            w.bytes(&value);
        }
    }
}
fn explosion(tag: &Tag) -> FireworkExplosion {
    let colors = |key| match tag.get(key) {
        Some(Tag::IntArray(values)) => values.clone(),
        _ => Vec::new(),
    };
    FireworkExplosion {
        shape: int(tag, "Type", 0).clamp(0, 4),
        colors: colors("Colors"),
        fade: colors("FadeColors"),
        trail: int(tag, "Trail", 0) != 0,
        twinkle: int(tag, "Flicker", 0) != 0,
    }
}
fn block_predicate<M: Maps>(raw: &str) -> Result<Option<Vec<u8>>> {
    let properties = raw.find('[');
    let nbt = raw.find('{');
    let end = properties.into_iter().chain(nbt).min().unwrap_or(raw.len());
    let identifier = raw[..end].trim();
    let mut w = Writer::new();
    w.bool(true);
    if let Some(tag) = identifier.strip_prefix('#') {
        w.varint(0);
        w.string(tag);
    } else {
        let Some(id) = M::BLOCK_IDENTIFIERS
            .iter()
            .position(|name| *name == local(identifier))
        else {
            return Ok(None);
        };
        w.varint(2);
        w.varint(id as i32);
    }
    let mut values = Vec::new();
    if let Some(start) = properties {
        if let Some(end) = raw[start + 1..].find(']') {
            for property in raw[start + 1..start + 1 + end].split(',') {
                if let Some((name, value)) = property.split_once('=') {
                    values.push((name.trim(), value.trim()));
                }
            }
        }
    }
    w.bool(!values.is_empty());
    if !values.is_empty() {
        w.varint(values.len() as i32);
        for (name, value) in values {
            w.string(name);
            w.bool(true);
            w.string(value);
        }
    }
    let nbt = nbt.and_then(|start| {
        horizon_world::nbt::parse_snbt(&raw[start..])
            .ok()
            .map(|(tag, _)| tag)
    });
    w.bool(nbt.is_some());
    if let Some(nbt) = nbt {
        w.nbt(&nbt);
    }
    Ok(Some(w.data))
}
fn effect(tag: &Tag, w: &mut Writer, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Legacy potion effects nest too deeply"));
    }
    w.varint(int(tag, "amplifier", 0));
    w.varint(int(tag, "duration", 0));
    w.bool(int(tag, "ambient", 0) != 0);
    w.bool(int(tag, "show_particles", 1) != 0);
    w.bool(int(tag, "show_icon", 1) != 0);
    let hidden = tag.get("hidden_effect");
    w.bool(hidden.is_some());
    if let Some(hidden) = hidden {
        effect(hidden, w, depth + 1)?;
    }
    Ok(())
}
fn filterable(tag: Option<&Tag>, w: &mut Writer, limit: usize) -> Result<()> {
    let text = tag.and_then(text).unwrap_or_default();
    w.string(&text.chars().take(limit).collect::<String>());
    Ok(())
}
fn nested<M: Maps>(tag: &Tag, depth: usize, registry: &Registries) -> Result<Option<Vec<u8>>> {
    let Some(name) = string(tag, "id") else {
        return Ok(None);
    };
    let Some(id) = M::ITEM_IDENTIFIERS
        .iter()
        .position(|value| *value == local(&name))
    else {
        return Ok(None);
    };
    let count = int(tag, "Count", 0);
    if count <= 0 {
        return Ok(None);
    }
    let mut w = Writer::new();
    w.varint(count);
    w.varint(id as i32);
    patch::<M>(
        id as i32,
        tag.get("tag").unwrap_or(&Tag::End),
        depth + 1,
        registry,
    )?
    .finish(&mut w);
    Ok(Some(w.data))
}
fn patch<M: Maps>(item: i32, tag: &Tag, depth: usize, registry: &Registries) -> Result<Patch> {
    if depth > 64 {
        return Err(Error::new("Legacy item NBT nests too deeply"));
    }
    let mut out = Patch::default();
    if matches!(tag, Tag::End) {
        return Ok(out);
    }
    let Tag::Compound(original) = tag else {
        return Err(Error::new("Legacy item NBT is not a compound"));
    };
    if original.contains_key(&JavaString::from(ORIGINAL)) {
        return Err(Error::new(
            "Legacy item NBT collides with translator marker",
        ));
    }
    let mut custom = original.clone();
    custom.insert(ORIGINAL.into(), Tag::Byte(1));
    out.value("custom_data", &Tag::Compound(custom))?;
    let upgraded;
    let tag = if M::PROTOCOL < 764
        && (tag.get("CustomPotionEffects").is_some()
            || (M::ITEM_IDENTIFIERS.get(item as usize) == Some(&"suspicious_stew")
                && tag.get("Effects").is_some()))
    {
        let name = M::ITEM_IDENTIFIERS
            .get(item as usize)
            .ok_or_else(|| Error::new("Unknown pre-configuration item id"))?;
        // Only normalize this stack's effect fields. Nested item/entity/block
        // NBT has its own typed conversion and must keep its original schema.
        let effect_fields = ["CustomPotionEffects", "Effects"]
            .into_iter()
            .filter_map(|key| {
                tag.get(key)
                    .cloned()
                    .map(|value| (JavaString::from(key), value))
            })
            .collect();
        let mut stack = Tag::compound([
            (
                "id",
                Tag::String(format!("minecraft:{name}").as_str().into()),
            ),
            ("Count", Tag::Byte(1)),
            ("tag", Tag::Compound(effect_fields)),
        ]);
        horizon_world::datafix::update(
            horizon_world::datafix::schema::Ty::ItemStack,
            &mut stack,
            M::DATA_VERSION.ok_or_else(|| Error::new("Missing old item DataVersion"))?,
            crate::vfp_data_versions::P764,
        );
        let mut merged = original.clone();
        if let Some(Tag::Compound(effects)) = stack.get("tag") {
            for (old, new) in [
                ("CustomPotionEffects", "custom_potion_effects"),
                ("Effects", "effects"),
            ] {
                if let Some(value) = effects.get(&JavaString::from(new)) {
                    merged.remove(&JavaString::from(old));
                    merged.insert(new.into(), value.clone());
                }
            }
        }
        upgraded = Tag::Compound(merged);
        &upgraded
    } else {
        tag
    };
    let hide = int(tag, "HideFlags", 0);
    if hide & 32 != 0 {
        out.put("hide_additional_tooltip", Writer::new())?;
    }
    for (old, new, positive) in [
        ("Damage", "damage", true),
        ("RepairCost", "repair_cost", true),
        ("CustomModelData", "custom_model_data", false),
        ("map", "map_id", false),
    ] {
        if let Some(value) = tag.get(old).and_then(number) {
            if !positive || value > 0 {
                out.integer(new, value)?;
            }
        }
    }
    if int(tag, "Unbreakable", 0) != 0 {
        let mut w = Writer::new();
        w.bool(hide & 4 == 0);
        out.put("unbreakable", w)?;
    }
    if let Some(display) = tag.get("display") {
        if let Some(value) = display.get("Name") {
            if let Ok(name) = json(value) {
                out.value("custom_name", &name)?;
            }
        }
        if let Some(Tag::List(lore)) = display.get("Lore") {
            let lore = lore.iter().take(256).map(json).collect::<Result<Vec<_>>>();
            if let Ok(lore) = lore {
                let mut w = Writer::new();
                w.varint(lore.len() as i32);
                for entry in lore {
                    w.nbt(&entry);
                }
                out.put("lore", w)?;
            }
        }
        if let Some(color) = display.get("color").and_then(number) {
            let mut w = Writer::new();
            w.i32(color);
            w.bool(hide & 64 == 0);
            out.put("dyed_color", w)?;
        }
        if let Some(color) = display.get("MapColor").and_then(number) {
            let mut w = Writer::new();
            w.i32(color);
            out.put("map_color", w)?;
        }
    }
    for (old, new, hidden) in [
        ("Enchantments", "enchantments", 1),
        ("StoredEnchantments", "stored_enchantments", 32),
    ] {
        if let Some(Tag::List(enchants)) = tag.get(old) {
            let mut values = BTreeMap::new();
            for enchant in enchants {
                let Some(level) = enchant.get("lvl").and_then(number) else {
                    continue;
                };
                if let Some(id) = string(enchant, "id").and_then(|name| {
                    let name = if local(&name) == "sweeping" {
                        "sweeping_edge"
                    } else {
                        local(&name)
                    };
                    crate::vfp_registry_1205::ENCHANTMENT_NAMES
                        .iter()
                        .position(|key| *key == name)
                }) {
                    values.insert(id as i32, level.clamp(0, 255));
                }
            }
            if !enchants.is_empty() && values.is_empty() {
                let mut w = Writer::new();
                w.bool(true);
                out.put("enchantment_glint_override", w)?;
            }
            let mut w = Writer::new();
            w.varint(values.len() as i32);
            for (id, level) in values {
                w.varint(id);
                w.varint(level);
            }
            w.bool(hide & hidden == 0);
            out.put(new, w)?;
        }
    }
    if let Some(Tag::List(modifiers)) = tag.get("AttributeModifiers") {
        let mut values = Vec::new();
        for modifier in modifiers.iter().take(1024) {
            let Some(attribute) = string(modifier, "AttributeName") else {
                continue;
            };
            let attribute = if local(&attribute) == "horse.jump_strength" {
                "generic.jump_strength"
            } else {
                local(&attribute)
            };
            let Some(attribute) = names::ATTRIBUTES.iter().position(|name| *name == attribute)
            else {
                continue;
            };
            let Some(id) = modifier.get("UUID").and_then(uuid) else {
                continue;
            };
            let operation = int(modifier, "Operation", 0);
            if !(0..=2).contains(&operation) {
                continue;
            }
            let name = string(modifier, "Name").unwrap_or_default();
            let slot = string(modifier, "Slot")
                .and_then(|name| names::SLOTS.iter().position(|value| *value == name))
                .unwrap_or(0);
            let amount = match modifier.get("Amount") {
                Some(Tag::Double(bits)) => f64::from_bits(*bits),
                Some(Tag::Float(bits)) => f64::from(f32::from_bits(*bits)),
                Some(n) => f64::from(number(n).unwrap_or(0)),
                _ => 0.0,
            };
            let mut w = Writer::new();
            w.varint(attribute as i32);
            w.uuid(id);
            w.string(&name);
            w.f64(amount);
            w.varint(operation);
            w.varint(slot as i32);
            values.push(w.data);
        }
        let mut w = Writer::new();
        w.varint(values.len() as i32);
        for value in values {
            w.bytes(&value);
        }
        w.bool(hide & 2 == 0);
        out.put("attribute_modifiers", w)?;
    } else if hide & 2 != 0 {
        let mut w = Writer::new();
        w.varint(0);
        w.bool(false);
        out.put("attribute_modifiers", w)?;
    }
    if let Some(Tag::Compound(state)) = tag.get("BlockStateTag") {
        let values: Vec<_> = state
            .iter()
            .filter_map(|(key, value)| {
                text(value)
                    .or_else(|| number(value).map(|n| n.to_string()))
                    .map(|value| (key.to_string_lossy(), value))
            })
            .collect();
        let mut w = Writer::new();
        w.varint(values.len() as i32);
        for (key, value) in values {
            w.string(&key);
            w.string(&value);
        }
        out.put("block_state", w)?;
    }
    for (old, new) in [
        ("EntityTag", "entity_data"),
        ("DebugProperty", "debug_stick_state"),
    ] {
        if let Some(value) = tag.get(old) {
            if matches!(value, Tag::Compound(_)) {
                if old == "EntityTag" {
                    let mut value = value.clone();
                    if value.get("variant").is_some() {
                        if let Tag::Compound(fields) = &mut value {
                            fields.insert("id".into(), Tag::String("minecraft:painting".into()));
                        }
                    }
                    if value.get("id").is_some() {
                        out.value(new, &value)?;
                    }
                } else {
                    out.value(new, value)?;
                }
            }
        }
    }
    let mut bucket = BTreeMap::new();
    for key in [
        "NoAI",
        "Silent",
        "NoGravity",
        "Glowing",
        "Invulnerable",
        "Health",
        "Age",
        "Variant",
        "HuntingCooldown",
        "BucketVariantTag",
    ] {
        if let Some(value) = tag.get(key) {
            bucket.insert(JavaString::from(key), value.clone());
        }
    }
    if !bucket.is_empty() {
        out.value("bucket_entity_data", &Tag::Compound(bucket))?;
    }
    if let Some(value) = tag.get("Explosion") {
        let mut w = Writer::new();
        Component::FireworkExplosion(explosion(value)).write(70, &mut w);
        out.put("firework_explosion", w)?;
    }
    if let Some(value) = tag.get("Fireworks") {
        let mut w = Writer::new();
        Component::Fireworks {
            flight: int(value, "Flight", 0) & 255,
            explosions: list(value, "Explosions")
                .iter()
                .take(256)
                .map(explosion)
                .collect(),
        }
        .write(71, &mut w);
        out.put("fireworks", w)?;
    }
    if let Some(Tag::List(recipes)) = tag.get("Recipes") {
        out.value("recipes", &Tag::List(recipes.clone()))?;
    }
    if let Some(instrument) = string(tag, "instrument").and_then(|name| {
        names::INSTRUMENTS
            .iter()
            .position(|value| *value == local(&name))
    }) {
        let mut w = Writer::new();
        w.varint(instrument as i32 + 1);
        out.put("instrument", w)?;
    } else if let Some(instrument @ Tag::Compound(_)) = tag.get("instrument") {
        if let Some(sound) = instrument.get("sound_event") {
            let mut w = Writer::new();
            w.varint(0);
            sound_to_wire::<M>(sound, &mut w)?;
            w.varint(int(instrument, "use_duration", 140).max(0));
            w.f32(real(instrument, "range", 256.0));
            out.put("instrument", w)?;
        }
    }
    if tag.get("LodestoneTracked").is_some() {
        let mut w = Writer::new();
        let pos = tag.get("LodestonePos");
        let dimension = string(tag, "LodestoneDimension");
        w.bool(pos.is_some() && dimension.is_some());
        if let (Some(pos), Some(dimension)) = (pos, dimension) {
            w.string(&dimension);
            w.i64(crate::buf::pack_block_pos([
                int(pos, "X", 0),
                int(pos, "Y", 0),
                int(pos, "Z", 0),
            ]));
        }
        w.bool(int(tag, "LodestoneTracked", 0) != 0);
        out.put("lodestone_tracker", w)?;
    }
    if let Some(Tag::List(effects)) = tag.get("effects") {
        let mut values = Vec::new();
        for effect in effects {
            if let Some(id) = string(effect, "id").and_then(|name| {
                names::EFFECTS
                    .iter()
                    .position(|value| *value == local(&name))
            }) {
                values.push((id, int(effect, "duration", 0)));
            }
        }
        let mut w = Writer::new();
        w.varint(values.len() as i32);
        for (id, duration) in values {
            w.varint(id as i32);
            w.varint(duration);
        }
        out.put("suspicious_stew_effects", w)?;
    }
    let potion = string(tag, "Potion").and_then(|name| {
        names::POTIONS
            .iter()
            .position(|value| *value == local(&name))
    });
    let color = tag.get("CustomPotionColor").and_then(number);
    let custom = tag.get("custom_potion_effects");
    if potion.is_some() || color.is_some() || custom.is_some() {
        let mut w = Writer::new();
        w.bool(potion.is_some());
        if let Some(id) = potion {
            w.varint(id as i32);
        }
        w.bool(color.is_some());
        if let Some(color) = color {
            w.i32(color);
        }
        let mut effects = Vec::new();
        for value in list(tag, "custom_potion_effects") {
            if let Some(id) = string(value, "id").and_then(|name| {
                names::EFFECTS
                    .iter()
                    .position(|value| *value == local(&name))
            }) {
                let mut effect_data = Writer::new();
                effect_data.varint(id as i32);
                effect(value, &mut effect_data, 0)?;
                effects.push(effect_data.data);
            }
        }
        w.varint(effects.len() as i32);
        for effect in effects {
            w.bytes(&effect);
        }
        out.put("potion_contents", w)?;
    }
    if let Some(profile) = tag.get("SkullOwner") {
        let name = if let Tag::String(name) = profile {
            Some(name.to_string_lossy())
        } else {
            string(profile, "Name")
        };
        let id = profile.get("Id").and_then(uuid);
        let mut w = Writer::new();
        w.bool(name.is_some());
        if let Some(name) = name {
            w.string(&name.chars().take(16).collect::<String>());
        }
        w.bool(id.is_some());
        if let Some(id) = id {
            w.uuid(id);
        }
        let mut properties = Vec::new();
        if let Some(Tag::Compound(values)) = profile.get("Properties") {
            for (name, values) in values {
                if let Tag::List(values) = values {
                    for value in values {
                        if let Some(text) = string(value, "Value") {
                            properties.push((
                                name.to_string_lossy(),
                                text,
                                string(value, "Signature"),
                            ));
                        }
                    }
                }
            }
        }
        properties.truncate(16);
        w.varint(properties.len() as i32);
        for (name, value, signature) in properties {
            w.string(&name.chars().take(64).collect::<String>());
            w.string(&value);
            w.bool(signature.is_some());
            if let Some(signature) = signature {
                w.string(&signature.chars().take(1024).collect::<String>());
            }
        }
        out.put("profile", w)?;
    }
    if let Some(trim) = tag.get("Trim") {
        if let (Some(material), Some(pattern)) = (trim.get("material"), trim.get("pattern")) {
            let mut w = Writer::new();
            if let Some(material) = text(material) {
                w.varint(
                    registry
                        .id("trim_material", &material)
                        .map_err(Error::new)?
                        + 1,
                );
            } else {
                let asset = string(material, "asset_name")
                    .ok_or_else(|| Error::new("Inline trim material has no asset name"))?;
                let ingredient = string(material, "ingredient")
                    .and_then(|name| {
                        M::ITEM_IDENTIFIERS
                            .iter()
                            .position(|value| *value == local(&name))
                    })
                    .ok_or_else(|| Error::new("Inline trim material has no ingredient"))?;
                w.varint(0);
                w.string(&asset);
                w.varint(ingredient as i32);
                let model = match material.get("item_model_index") {
                    Some(Tag::Float(n)) => f32::from_bits(*n),
                    Some(Tag::Double(n)) => f64::from_bits(*n) as f32,
                    _ => 0.0,
                };
                w.f32(model);
                let overrides =
                    if let Some(Tag::Compound(fields)) = material.get("override_armor_materials") {
                        fields
                            .iter()
                            .filter_map(|(key, value)| {
                                let key = key.to_string_lossy();
                                let id = names::ARMOR_MATERIALS
                                    .iter()
                                    .position(|value| *value == local(&key))
                                    .or_else(|| {
                                        key.parse::<usize>()
                                            .ok()
                                            .filter(|id| *id < names::ARMOR_MATERIALS.len())
                                    })?;
                                Some((id, text(value)?))
                            })
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    };
                w.varint(overrides.len() as i32);
                for (id, name) in overrides {
                    w.varint(id as i32);
                    w.string(&name);
                }
                w.nbt(
                    material
                        .get("description")
                        .unwrap_or(&Tag::String("".into())),
                );
            }
            if let Some(pattern) = text(pattern) {
                w.varint(registry.id("trim_pattern", &pattern).map_err(Error::new)? + 1);
            } else {
                let asset = string(pattern, "assetId")
                    .or_else(|| string(pattern, "asset_id"))
                    .ok_or_else(|| Error::new("Inline trim pattern has no asset"))?;
                let template = string(pattern, "templateItem")
                    .or_else(|| string(pattern, "template_item"))
                    .and_then(|name| {
                        M::ITEM_IDENTIFIERS
                            .iter()
                            .position(|value| *value == local(&name))
                    })
                    .ok_or_else(|| Error::new("Inline trim pattern has no template"))?;
                w.varint(0);
                w.string(&asset);
                w.varint(template as i32);
                w.nbt(
                    pattern
                        .get("description")
                        .unwrap_or(&Tag::String("".into())),
                );
                w.bool(int(pattern, "decal", 0) != 0);
            }
            w.bool(hide & 128 == 0);
            out.put("trim", w)?;
        }
    }
    let item_name = M::ITEM_IDENTIFIERS
        .get(item as usize)
        .copied()
        .unwrap_or("");
    if matches!(item_name, "writable_book" | "written_book") {
        let mut w = Writer::new();
        let written = item_name == "written_book";
        if written {
            filterable(tag.get("title"), &mut w, 32)?;
            w.bool(tag.get("filtered_title").is_some());
            if tag.get("filtered_title").is_some() {
                filterable(tag.get("filtered_title"), &mut w, 32)?;
            }
            w.string(&string(tag, "author").unwrap_or_default());
            w.varint(int(tag, "generation", 0).clamp(0, 3));
        }
        let pages = list(tag, "pages");
        let n = if written {
            pages.len().min(1024)
        } else {
            pages.len().min(100)
        };
        w.varint(n as i32);
        for (index, page) in pages.iter().take(n).enumerate() {
            if written {
                w.nbt(&json(page).unwrap_or_else(|_| Tag::String("* Invalid book tag *".into())));
            } else {
                filterable(Some(page), &mut w, 1024)?;
            }
            let filtered = tag
                .get("filtered_pages")
                .and_then(|pages| pages.get(index.to_string().as_str()));
            w.bool(filtered.is_some());
            if let Some(filtered) = filtered {
                if written {
                    w.nbt(&json(filtered).unwrap_or_else(|_| Tag::String("".into())));
                } else {
                    filterable(Some(filtered), &mut w, 1024)?;
                }
            }
        }
        if written {
            w.bool(int(tag, "resolved", 0) != 0);
        }
        out.put(
            if written {
                "written_book_content"
            } else {
                "writable_book_content"
            },
            w,
        )?;
    }
    for (old, new) in [
        ("ChargedProjectiles", "charged_projectiles"),
        ("Items", "bundle_contents"),
    ] {
        if old == "Items" && item_name != "bundle" {
            continue;
        }
        if let Some(Tag::List(items)) = tag.get(old) {
            let mut values = Vec::new();
            for item in items.iter().take(1024) {
                if let Some(item) = nested::<M>(item, depth, registry)? {
                    values.push(item);
                }
            }
            let mut w = Writer::new();
            w.varint(values.len() as i32);
            for item in values {
                w.bytes(&item);
            }
            out.put(new, w)?;
        }
    }
    for (old, new, hidden) in [
        ("CanPlaceOn", "can_place_on", 16),
        ("CanDestroy", "can_break", 8),
    ] {
        if let Some(Tag::List(predicates)) = tag.get(old) {
            let mut values = Vec::new();
            for predicate in predicates.iter().take(1024) {
                if let Some(value) = text(predicate) {
                    if let Some(predicate) = block_predicate::<M>(&value)? {
                        values.push(predicate);
                    }
                }
            }
            let mut w = Writer::new();
            w.varint(values.len() as i32);
            for predicate in values {
                w.bytes(&predicate);
            }
            w.bool(hide & hidden == 0);
            out.put(new, w)?;
        }
    }
    if let Some(Tag::List(decorations)) = tag.get("Decorations") {
        let mut values = BTreeMap::new();
        for decoration in decorations.iter().take(1024) {
            let Some(id) = string(decoration, "id") else {
                continue;
            };
            let number = |key| match decoration.get(key) {
                Some(Tag::Double(bits)) => f64::from_bits(*bits),
                Some(Tag::Float(bits)) => f64::from(f32::from_bits(*bits)),
                Some(tag) => f64::from(number(tag).unwrap_or(0)),
                _ => 0.0,
            };
            let kind = names::DECORATIONS
                .get(int(decoration, "type", 0) as usize)
                .copied()
                .unwrap_or("player");
            values.insert(
                id.as_str().into(),
                Tag::compound([
                    ("type", Tag::String(kind.into())),
                    ("x", Tag::Double(number("x").to_bits())),
                    ("z", Tag::Double(number("z").to_bits())),
                    ("rotation", Tag::Float((number("rot") as f32).to_bits())),
                ]),
            );
        }
        out.value("map_decorations", &Tag::Compound(values))?;
    }
    if let Some(block) = tag.get("BlockEntityTag") {
        let mut block = block.clone();
        crate::vfp_world::legacy_profile(&mut block)?;
        let Tag::Compound(fields) = &mut block else {
            return Ok(out);
        };
        let mut block_kind = None;
        if let Some(lock) = fields.get(&JavaString::from("Lock")) {
            if matches!(lock, Tag::String(_)) {
                out.value("lock", lock)?;
            }
        }
        if let Some(Tag::List(bees)) = fields.get(&JavaString::from("Bees")) {
            let bees: Vec<_> = bees
                .iter()
                .take(1024)
                .filter_map(|bee| {
                    bee.get("EntityData").map(|data| {
                        (
                            data,
                            int(bee, "TicksInHive", 0),
                            int(bee, "MinOccupationTicks", 0),
                        )
                    })
                })
                .collect();
            let mut w = Writer::new();
            w.varint(bees.len() as i32);
            for (data, ticks, minimum) in bees {
                w.nbt(data);
                w.varint(ticks);
                w.varint(minimum);
            }
            out.put("bees", w)?;
            block_kind = Some("beehive");
        }
        if let Some(Tag::List(sherds)) = fields.get(&JavaString::from("sherds")) {
            if sherds.len() == 4 {
                let mut w = Writer::new();
                w.varint(4);
                for sherd in sherds {
                    let id = text(sherd)
                        .and_then(|name| {
                            M::ITEM_IDENTIFIERS
                                .iter()
                                .position(|key| *key == local(&name))
                        })
                        .map(|id| id as i32)
                        .unwrap_or(M::BRICK_OLD);
                    w.varint(id);
                }
                out.put("pot_decorations", w)?;
                block_kind = Some("decorated_pot");
            }
        }
        if let Some(Tag::String(sound)) = fields.get(&JavaString::from("note_block_sound")) {
            let mut w = Writer::new();
            w.string(&sound.to_string_lossy());
            out.put("note_block_sound", w)?;
            block_kind = Some("skull");
        }
        if let Some(Tag::String(loot)) = fields.get(&JavaString::from("LootTable")) {
            let seed = match fields.get(&JavaString::from("LootTableSeed")) {
                Some(Tag::Long(seed)) => *seed,
                _ => 0,
            };
            out.value(
                "container_loot",
                &Tag::compound([
                    ("loot_table", Tag::String(loot.clone())),
                    ("loot_table_seed", Tag::Long(seed)),
                ]),
            )?;
        }
        if let Some(color) = fields
            .remove(&JavaString::from("Base"))
            .and_then(|tag| number(&tag))
        {
            out.integer("base_color", color.clamp(0, 15))?;
        }
        if let Some(Tag::List(items)) = fields.remove(&JavaString::from("Items")) {
            let mut slots = BTreeMap::new();
            for item in items.iter().take(256) {
                let slot = int(item, "Slot", 0);
                if (0..=255).contains(&slot) {
                    if let Some(value) = nested::<M>(item, depth, registry)? {
                        slots.insert(slot, value);
                    }
                }
            }
            let size = slots.last_key_value().map_or(1, |(slot, _)| slot + 1);
            let mut w = Writer::new();
            w.varint(size);
            for slot in 0..size {
                if let Some(item) = slots.get(&slot) {
                    w.bytes(item);
                } else {
                    w.varint(0);
                }
            }
            out.put("container", w)?;
            block_kind = Some("shulker_box");
        }
        if let Some(Tag::List(patterns)) = fields.remove(&JavaString::from("Patterns")) {
            let mut layers = Vec::new();
            for pattern in patterns.iter().take(1024) {
                let color = int(pattern, "Color", -1);
                if !(0..16).contains(&color) {
                    continue;
                }
                let Some(name) = string(pattern, "Pattern").and_then(|code| {
                    names::PATTERN_CODES
                        .iter()
                        .find(|(key, _)| *key == code)
                        .map(|(_, name)| *name)
                }) else {
                    continue;
                };
                let id = registry.id("banner_pattern", name).ok().or_else(|| {
                    names::PATTERNS
                        .iter()
                        .position(|value| *value == name)
                        .map(|id| id as i32)
                });
                if let Some(id) = id {
                    layers.push((id, color));
                }
            }
            let mut w = Writer::new();
            w.varint(layers.len() as i32);
            for (id, color) in layers {
                w.varint(id + 1);
                w.varint(color);
            }
            out.put("banner_patterns", w)?;
            block_kind = Some("banner");
        }
        if !fields.contains_key(&JavaString::from("id")) {
            if let Some(kind) = block_kind {
                fields.insert(
                    "id".into(),
                    Tag::String(format!("minecraft:{kind}").as_str().into()),
                );
            }
        }
        if fields.contains_key(&JavaString::from("id")) {
            out.value("block_entity_data", &block)?;
        }
    }
    Ok(out)
}
pub fn forward<M: Maps>(r: &mut Reader<'_>, w: &mut Writer, registry: &Registries) -> Result<()> {
    if !r.bool()? {
        w.varint(0);
        return Ok(());
    }
    let item = r.varint()?;
    let count = i32::from(r.i8()?);
    let tag = if M::PROTOCOL < 764 {
        r.vfp_named_nbt()?
    } else {
        r.nbt()?
    };
    if count <= 0 {
        w.varint(0);
        return Ok(());
    }
    let mut source = Writer::new();
    source.varint(count);
    source.varint(item);
    patch::<M>(item, &tag, 0, registry)?.finish(&mut source);
    crate::vfp_items::structured_stack_using::<Legacy<M>>(
        &mut Reader::new(&source.data),
        w,
        true,
        false,
        registry,
    )
}
fn json_value(tag: &Tag, depth: usize) -> Result<serde_json::Value> {
    use serde_json::Value as Json;
    if depth > 64 {
        return Err(Error::new("Legacy text output nests too deeply"));
    }
    Ok(match tag {
        Tag::End => Json::Null,
        Tag::String(s) => Json::String(s.to_string_lossy()),
        Tag::Byte(n) => Json::from(*n),
        Tag::Short(n) => Json::from(*n),
        Tag::Int(n) => Json::from(*n),
        Tag::Long(n) => Json::from(*n),
        Tag::Float(n) => Json::from(f32::from_bits(*n)),
        Tag::Double(n) => Json::from(f64::from_bits(*n)),
        Tag::ByteArray(values) => Json::Array(values.iter().map(|n| Json::from(*n)).collect()),
        Tag::IntArray(values) => Json::Array(values.iter().map(|n| Json::from(*n)).collect()),
        Tag::LongArray(values) => Json::Array(values.iter().map(|n| Json::from(*n)).collect()),
        Tag::List(values) => Json::Array(
            values
                .iter()
                .map(|tag| json_value(tag, depth + 1))
                .collect::<Result<_>>()?,
        ),
        Tag::Compound(values) => {
            let mut map = serde_json::Map::new();
            for (key, value) in values {
                let key = key.to_string_lossy();
                let value = if matches!(
                    key.as_str(),
                    "italic" | "bold" | "underlined" | "strikethrough" | "obfuscated" | "interpret"
                ) {
                    number(value)
                        .map(|value| Json::Bool(value != 0))
                        .map(Ok)
                        .unwrap_or_else(|| json_value(value, depth + 1))?
                } else {
                    json_value(value, depth + 1)?
                };
                map.insert(key, value);
            }
            Json::Object(map)
        }
    })
}
fn json_string(tag: &Tag) -> Result<Tag> {
    Ok(Tag::String(json_value(tag, 0)?.to_string().as_str().into()))
}
fn display_field(tag: &mut BTreeMap<JavaString, Tag>, name: &str, value: Tag) -> Result<()> {
    let display = tag
        .entry("display".into())
        .or_insert_with(|| Tag::compound([] as [(&str, Tag); 0]));
    let Tag::Compound(display) = display else {
        return Err(Error::new("Legacy display is not a compound"));
    };
    display.insert(name.into(), value);
    Ok(())
}
fn block_field(tag: &mut BTreeMap<JavaString, Tag>, name: &str, value: Tag) -> Result<()> {
    let block = tag
        .entry("BlockEntityTag".into())
        .or_insert_with(|| Tag::compound([] as [(&str, Tag); 0]));
    let Tag::Compound(block) = block else {
        return Err(Error::new("Legacy block entity data is not a compound"));
    };
    block.insert(name.into(), value);
    Ok(())
}
fn snbt(tag: &Tag, depth: usize) -> Result<String> {
    if depth > 64 {
        return Err(Error::new("Legacy predicate NBT output nests too deeply"));
    }
    Ok(match tag {
        Tag::End => "{}".into(),
        Tag::Byte(n) => format!("{n}b"),
        Tag::Short(n) => format!("{n}s"),
        Tag::Int(n) => n.to_string(),
        Tag::Long(n) => format!("{n}L"),
        Tag::Float(n) => format!("{}f", f32::from_bits(*n)),
        Tag::Double(n) => format!("{}d", f64::from_bits(*n)),
        Tag::String(s) => serde_json::Value::String(s.to_string_lossy()).to_string(),
        Tag::ByteArray(values) => format!(
            "[B;{}]",
            values
                .iter()
                .map(|n| format!("{n}b"))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Tag::IntArray(values) => format!(
            "[I;{}]",
            values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Tag::LongArray(values) => format!(
            "[L;{}]",
            values
                .iter()
                .map(|n| format!("{n}L"))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Tag::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(|tag| snbt(tag, depth + 1))
                .collect::<Result<Vec<_>>>()?
                .join(",")
        ),
        Tag::Compound(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| Ok(format!(
                    "{}:{}",
                    serde_json::Value::String(key.to_string_lossy()),
                    snbt(value, depth + 1)?
                )))
                .collect::<Result<Vec<_>>>()?
                .join(",")
        ),
    })
}
fn old_predicates<M: Maps>(r: &mut Reader<'_>) -> Result<(Tag, bool)> {
    let mut predicates = Vec::new();
    for _ in 0..r.count(1024)? {
        let mut blocks = Vec::new();
        if r.bool()? {
            let n = r.count(65537)?;
            if n == 0 {
                blocks.push(format!("#{}", r.string(32767)?));
            } else {
                for _ in 1..n {
                    let id = r.varint()?;
                    if let Some(name) = M::BLOCK_IDENTIFIERS.get(id as usize) {
                        blocks.push(format!("minecraft:{name}"));
                    }
                }
            }
        }
        let mut properties = Vec::new();
        if r.bool()? {
            for _ in 0..r.count(1024)? {
                let key = r.string(32767)?;
                if r.bool()? {
                    properties.push(format!("{key}={}", r.string(32767)?));
                } else {
                    for _ in 0..2 {
                        if r.bool()? {
                            r.string(32767)?;
                        }
                    }
                }
            }
        }
        let data = if r.bool()? { Some(r.nbt()?) } else { None };
        let suffix = format!(
            "{}{}",
            if properties.is_empty() {
                String::new()
            } else {
                format!("[{}]", properties.join(","))
            },
            data.as_ref()
                .map(|tag| snbt(tag, 0))
                .transpose()?
                .unwrap_or_default()
        );
        predicates.extend(
            blocks
                .into_iter()
                .map(|name| Tag::String(format!("{name}{suffix}").as_str().into())),
        );
    }
    Ok((Tag::List(predicates), r.bool()?))
}
fn nested_old<M: Maps>(r: &mut Reader<'_>, registry: &Registries) -> Result<Option<Tag>> {
    let mut native = Writer::new();
    crate::vfp_items::structured_stack_using::<Legacy<M>>(r, &mut native, true, false, registry)?;
    let mut old = Writer::new();
    reverse::<M>(&mut Reader::new(&native.data), &mut old, false, registry)?;
    let mut r = Reader::new(&old.data);
    if !r.bool()? {
        return Ok(None);
    }
    let id = r.varint()?;
    let count = r.i8()?;
    let tag = if M::PROTOCOL < 764 {
        r.vfp_named_nbt()?
    } else {
        r.nbt()?
    };
    let name = M::ITEM_IDENTIFIERS
        .get(id as usize)
        .ok_or_else(|| Error::new("Unknown reconstructed legacy item id"))?;
    let mut out = BTreeMap::new();
    out.insert(
        "id".into(),
        Tag::String(format!("minecraft:{name}").as_str().into()),
    );
    out.insert("Count".into(), Tag::Byte(count));
    if !matches!(tag, Tag::End) {
        out.insert("tag".into(), tag);
    }
    Ok(Some(Tag::Compound(out)))
}
fn effect_tag(r: &mut Reader<'_>, depth: usize) -> Result<Tag> {
    if depth > 64 {
        return Err(Error::new("Reconstructed potion effect nests too deeply"));
    }
    let amplifier = r.varint()?;
    let duration = r.varint()?;
    let ambient = r.bool()?;
    let particles = r.bool()?;
    let icon = r.bool()?;
    let mut fields = BTreeMap::new();
    fields.insert("amplifier".into(), Tag::Int(amplifier));
    fields.insert("duration".into(), Tag::Int(duration));
    fields.insert("ambient".into(), Tag::Byte(i8::from(ambient)));
    fields.insert("show_particles".into(), Tag::Byte(i8::from(particles)));
    fields.insert("show_icon".into(), Tag::Byte(i8::from(icon)));
    if r.bool()? {
        fields.insert("hidden_effect".into(), effect_tag(r, depth + 1)?);
    }
    Ok(Tag::Compound(fields))
}
fn explosion_tag(value: &FireworkExplosion) -> Tag {
    Tag::compound([
        ("Type", Tag::Byte(value.shape as i8)),
        ("Colors", Tag::IntArray(value.colors.clone())),
        ("FadeColors", Tag::IntArray(value.fade.clone())),
        ("Trail", Tag::Byte(i8::from(value.trail))),
        ("Flicker", Tag::Byte(i8::from(value.twinkle))),
    ])
}
fn uuid_tag(value: u128) -> Tag {
    Tag::IntArray(
        (0..4)
            .map(|i| ((value >> (96 - i * 32)) as u32) as i32)
            .collect(),
    )
}
fn old_effect_instance(tag: &mut Tag, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::new("Old potion effects nest too deeply"));
    }
    let Tag::Compound(fields) = tag else {
        return Ok(());
    };
    if let Some(Tag::String(name)) = fields.remove(&JavaString::from("id")) {
        let name = name.to_string_lossy();
        if let Some(id) = names::EFFECTS
            .iter()
            .position(|value| *value == local(&name))
        {
            fields.insert("Id".into(), Tag::Int(id as i32 + 1));
        }
    }
    if let Some(hidden) = fields.get_mut(&JavaString::from("hidden_effect")) {
        old_effect_instance(hidden, depth + 1)?;
    }
    for (new, old) in [
        ("amplifier", "Amplifier"),
        ("duration", "Duration"),
        ("ambient", "Ambient"),
        ("show_particles", "ShowParticles"),
        ("show_icon", "ShowIcon"),
        ("hidden_effect", "HiddenEffect"),
        ("factor_calculation_data", "FactorCalculationData"),
    ] {
        if let Some(value) = fields.remove(&JavaString::from(new)) {
            fields.insert(old.into(), value);
        }
    }
    Ok(())
}
fn old_effects<M: Maps>(item: i32, tag: &mut Tag) -> Result<()> {
    let Tag::Compound(fields) = tag else {
        return Ok(());
    };
    if let Some(mut effects) = fields.remove(&JavaString::from("custom_potion_effects")) {
        if let Tag::List(values) = &mut effects {
            for value in values {
                old_effect_instance(value, 0)?;
            }
        }
        fields.insert("CustomPotionEffects".into(), effects);
    }
    if M::ITEM_IDENTIFIERS.get(item as usize) == Some(&"suspicious_stew") {
        if let Some(Tag::List(effects)) = fields.remove(&JavaString::from("effects")) {
            let mut values = Vec::new();
            for effect in effects {
                let Some(name) = string(&effect, "id") else {
                    continue;
                };
                let Some(id) = names::EFFECTS
                    .iter()
                    .position(|value| *value == local(&name))
                else {
                    continue;
                };
                values.push(Tag::compound([
                    ("EffectId", Tag::Int(id as i32 + 1)),
                    ("EffectDuration", Tag::Int(int(&effect, "duration", 160))),
                ]));
            }
            fields.insert("Effects".into(), Tag::List(values));
        }
    }
    Ok(())
}
fn old_header<M: Maps>(
    item: i32,
    count: i32,
    tag: &Tag,
    original: bool,
    w: &mut Writer,
) -> Result<()> {
    if count <= 0 {
        w.bool(false);
        return Ok(());
    }
    if count > 127 {
        return Err(Error::new("Legacy item count exceeds signed byte range"));
    }
    w.bool(true);
    w.varint(item);
    w.i8(count as i8);
    if M::PROTOCOL < 764 {
        let mut tag = tag.clone();
        if !original {
            old_effects::<M>(item, &mut tag)?;
        }
        w.vfp_named_nbt(&tag)?;
    } else {
        w.nbt(tag);
    }
    Ok(())
}
pub fn reverse<M: Maps>(
    r: &mut Reader<'_>,
    w: &mut Writer,
    delimited: bool,
    registry: &Registries,
) -> Result<()> {
    // Decode with the ordinary component translator first, including its
    // checked backups, tooltip flags and native-only component removal.
    let mut source = Writer::new();
    crate::vfp_items::structured_stack_using::<Legacy<M>>(
        r,
        &mut source,
        false,
        delimited,
        registry,
    )?;
    let mut input = Reader::new(&source.data);
    let count = input.varint()?;
    if count == 0 {
        w.bool(false);
        return Ok(());
    }
    let item = input.varint()?;
    let added = input.count(1024)?;
    let removed = input.count(1024)?;
    let mut tag = BTreeMap::new();
    let mut hide = 0i32;
    for _ in 0..added {
        let id = input.varint()?;
        let name = V1205::COMPONENT_IDENTIFIERS
            .get(id as usize)
            .copied()
            .ok_or_else(|| Error::new("Unknown reconstructed legacy component"))?;
        if name == "custom_data" {
            let custom = input.nbt()?;
            let Tag::Compound(mut custom) = custom else {
                return Err(Error::new("Legacy custom data is not a compound"));
            };
            if custom.remove(&JavaString::from(ORIGINAL)) == Some(Tag::Byte(1)) {
                // ViaVersion's older item path restores the complete original
                // NBT when its marker survived, including unknown mod tags.
                return old_header::<M>(item, count, &Tag::Compound(custom), true, w);
            }
            tag.extend(custom);
            continue;
        }
        if name == "map_color" {
            display_field(&mut tag, "MapColor", Tag::Int(input.i32()?))?;
            continue;
        }
        if name == "instrument" {
            let holder = input.varint()?;
            if holder < 0 {
                return Err(Error::new("Negative reconstructed instrument holder"));
            }
            let value = if holder == 0 {
                Tag::compound([
                    ("sound_event", sound_from_wire::<M>(&mut input)?),
                    ("use_duration", Tag::Int(input.varint()?)),
                    ("range", Tag::Float(input.f32()?.to_bits())),
                ])
            } else {
                let name = usize::try_from(holder - 1)
                    .ok()
                    .and_then(|id| names::INSTRUMENTS.get(id))
                    .ok_or_else(|| Error::new("Unknown reconstructed instrument"))?;
                Tag::String(format!("minecraft:{name}").as_str().into())
            };
            tag.insert("instrument".into(), value);
            continue;
        }
        if name == "trim" {
            let holder = input.varint()?;
            if holder < 0 {
                return Err(Error::new("Negative reconstructed trim material holder"));
            }
            let material = if holder == 0 {
                let asset = input.string(32767)?;
                let ingredient = item_name::<M>(input.varint()?)?;
                let model = input.f32()?;
                let mut overrides = BTreeMap::new();
                for _ in 0..input.count(1024)? {
                    let id = input.varint()?;
                    let value = input.string(32767)?;
                    if let Some(name) = usize::try_from(id)
                        .ok()
                        .and_then(|id| names::ARMOR_MATERIALS.get(id))
                    {
                        overrides
                            .insert(JavaString::from(*name), Tag::String(value.as_str().into()));
                    }
                }
                Tag::compound([
                    ("asset_name", Tag::String(asset.as_str().into())),
                    ("ingredient", ingredient),
                    ("item_model_index", Tag::Float(model.to_bits())),
                    ("override_armor_materials", Tag::Compound(overrides)),
                    ("description", input.nbt()?),
                ])
            } else {
                Tag::String(
                    registry
                        .name("trim_material", holder - 1)
                        .map_err(Error::new)?
                        .into(),
                )
            };
            let holder = input.varint()?;
            if holder < 0 {
                return Err(Error::new("Negative reconstructed trim pattern holder"));
            }
            let pattern = if holder == 0 {
                Tag::compound([
                    ("assetId", Tag::String(input.string(32767)?.as_str().into())),
                    ("templateItem", item_name::<M>(input.varint()?)?),
                    ("description", input.nbt()?),
                    ("decal", Tag::Byte(i8::from(input.bool()?))),
                ])
            } else {
                Tag::String(
                    registry
                        .name("trim_pattern", holder - 1)
                        .map_err(Error::new)?
                        .into(),
                )
            };
            if !input.bool()? {
                hide |= 128;
            }
            tag.insert(
                "Trim".into(),
                Tag::compound([("material", material), ("pattern", pattern)]),
            );
            continue;
        }
        if matches!(name, "writable_book_content" | "written_book_content") {
            let written = name == "written_book_content";
            if written {
                tag.insert(
                    "title".into(),
                    Tag::String(input.string(32)?.as_str().into()),
                );
                if input.bool()? {
                    tag.insert(
                        "filtered_title".into(),
                        Tag::String(input.string(32)?.as_str().into()),
                    );
                }
                tag.insert(
                    "author".into(),
                    Tag::String(input.string(32767)?.as_str().into()),
                );
                tag.insert("generation".into(), Tag::Int(input.varint()?));
            }
            let count = input.count(if written { 1024 } else { 100 })?;
            let mut pages = Vec::new();
            let mut filtered = BTreeMap::new();
            for index in 0..count {
                pages.push(if written {
                    json_string(&input.nbt()?)?
                } else {
                    Tag::String(input.string(1024)?.as_str().into())
                });
                if input.bool()? {
                    let value = if written {
                        json_string(&input.nbt()?)?
                    } else {
                        Tag::String(input.string(1024)?.as_str().into())
                    };
                    filtered.insert(index.to_string().as_str().into(), value);
                }
            }
            tag.insert("pages".into(), Tag::List(pages));
            if !filtered.is_empty() {
                tag.insert("filtered_pages".into(), Tag::Compound(filtered));
            }
            if written {
                tag.insert("resolved".into(), Tag::Byte(i8::from(input.bool()?)));
            }
            continue;
        }
        if matches!(name, "hide_additional_tooltip" | "hide_tooltip") {
            hide |= if name == "hide_tooltip" { 255 } else { 32 };
            continue;
        }
        if matches!(name, "can_place_on" | "can_break") {
            let (value, show) = old_predicates::<M>(&mut input)?;
            tag.insert(
                if name == "can_break" {
                    "CanDestroy"
                } else {
                    "CanPlaceOn"
                }
                .into(),
                value,
            );
            if !show {
                hide |= if name == "can_break" { 8 } else { 16 };
            }
            continue;
        }
        if name == "block_state" {
            let mut state = BTreeMap::new();
            for _ in 0..input.count(1024)? {
                state.insert(
                    input.string(32767)?.as_str().into(),
                    Tag::String(input.string(32767)?.as_str().into()),
                );
            }
            tag.insert("BlockStateTag".into(), Tag::Compound(state));
            continue;
        }
        if name == "note_block_sound" {
            block_field(
                &mut tag,
                "note_block_sound",
                Tag::String(input.string(32767)?.as_str().into()),
            )?;
            continue;
        }
        if name == "container_loot" {
            let value = input.nbt()?;
            if let Some(value) = value.get("loot_table") {
                block_field(&mut tag, "LootTable", value.clone())?;
            }
            if let Some(value) = value.get("loot_table_seed") {
                block_field(&mut tag, "LootTableSeed", value.clone())?;
            }
            continue;
        }
        if name == "pot_decorations" {
            let mut sherds = Vec::new();
            for _ in 0..input.count(4)? {
                let id = input.varint()?;
                if let Some(name) = M::ITEM_IDENTIFIERS.get(id as usize) {
                    sherds.push(Tag::String(format!("minecraft:{name}").as_str().into()));
                }
            }
            block_field(&mut tag, "sherds", Tag::List(sherds))?;
            continue;
        }
        if name == "bees" {
            let mut bees = Vec::new();
            for _ in 0..input.count(1024)? {
                let data = input.nbt()?;
                let ticks = input.varint()?;
                let minimum = input.varint()?;
                bees.push(Tag::compound([
                    ("EntityData", data),
                    ("TicksInHive", Tag::Int(ticks)),
                    ("MinOccupationTicks", Tag::Int(minimum)),
                ]));
            }
            block_field(&mut tag, "Bees", Tag::List(bees))?;
            continue;
        }
        if name == "container" {
            let mut items = Vec::new();
            for slot in 0..input.count(256)? {
                if let Some(Tag::Compound(mut value)) = nested_old::<M>(&mut input, registry)? {
                    value.insert("Slot".into(), Tag::Byte(slot as i8));
                    items.push(Tag::Compound(value));
                }
            }
            block_field(&mut tag, "Items", Tag::List(items))?;
            continue;
        }
        if name == "map_decorations" {
            let value = input.nbt()?;
            let mut decorations = Vec::new();
            if let Tag::Compound(values) = value {
                for (id, value) in values {
                    let Some(kind) = string(&value, "type").and_then(|kind| {
                        names::DECORATIONS
                            .iter()
                            .position(|name| *name == local(&kind))
                    }) else {
                        continue;
                    };
                    let mut fields = BTreeMap::new();
                    fields.insert("id".into(), Tag::String(id));
                    fields.insert("type".into(), Tag::Int(kind as i32));
                    for (old, new) in [("x", "x"), ("z", "z"), ("rotation", "rot")] {
                        if let Some(value) = value.get(old) {
                            fields.insert(new.into(), value.clone());
                        }
                    }
                    decorations.push(Tag::Compound(fields));
                }
            }
            tag.insert("Decorations".into(), Tag::List(decorations));
            continue;
        }
        if matches!(
            name,
            "lock"
                | "entity_data"
                | "block_entity_data"
                | "bucket_entity_data"
                | "debug_stick_state"
                | "recipes"
        ) {
            let value = input.nbt()?;
            if name == "bucket_entity_data" {
                if let Tag::Compound(values) = value {
                    tag.extend(values);
                }
            } else if name == "lock" {
                block_field(&mut tag, "Lock", value)?;
            } else {
                let key = match name {
                    "entity_data" => "EntityTag",
                    "block_entity_data" => "BlockEntityTag",
                    "debug_stick_state" => "DebugProperty",
                    _ => "Recipes",
                };
                tag.insert(key.into(), value);
            }
            continue;
        }
        let native = V1205::DATA_COMPONENT_TYPE.map(id).map_err(Error::new)?;
        let mut encoded = Writer::new();
        let start = input.position();
        crate::vfp_items::value_using::<Legacy<M>>(
            native,
            &mut input,
            &mut encoded,
            true,
            0,
            registry,
        )?;
        if input.since(start).last() == Some(&0) {
            hide |= match native {
                4 => 4,
                13 => 1,
                14 => 16,
                15 => 8,
                16 => 2,
                45 => 32,
                47 => 64,
                58 => 128,
                _ => 0,
            };
        }
        let value = Component::read(native, &mut Reader::new(&encoded.data))?;
        match (native, value) {
            (3, Component::Int(n)) => {
                tag.insert("Damage".into(), Tag::Int(n));
            }
            (19, Component::Int(n)) => {
                tag.insert("RepairCost".into(), Tag::Int(n));
            }
            (21, Component::Bool(true)) => {
                let value = tag
                    .entry("Enchantments".into())
                    .or_insert_with(|| Tag::List(Vec::new()));
                if let Tag::List(enchants) = value {
                    if enchants.is_empty() {
                        enchants.push(Tag::compound([("id", Tag::String("".into()))]));
                    }
                }
            }
            (4, Component::Unit) => {
                tag.insert("Unbreakable".into(), Tag::Byte(1));
            }
            (6 | 9, Component::Text(value)) => {
                display_field(&mut tag, "Name", json_string(&value)?)?;
            }
            (11, Component::Lore(values)) => {
                display_field(
                    &mut tag,
                    "Lore",
                    Tag::List(values.iter().map(json_string).collect::<Result<_>>()?),
                )?;
            }
            (13 | 45, Component::Enchantments(values)) => {
                let values = values
                    .into_iter()
                    .filter_map(|(id, level)| {
                        crate::vfp_registry_1205::ENCHANTMENT_NAMES
                            .get(id as usize)
                            .map(|name| {
                                Tag::compound([
                                    (
                                        "id",
                                        Tag::String(format!("minecraft:{name}").as_str().into()),
                                    ),
                                    ("lvl", Tag::Short(level as i16)),
                                ])
                            })
                    })
                    .collect();
                tag.insert(
                    if native == 13 {
                        "Enchantments"
                    } else {
                        "StoredEnchantments"
                    }
                    .into(),
                    Tag::List(values),
                );
            }
            (17, Component::CustomModelData { floats, .. }) => {
                if let Some(value) = floats.first() {
                    tag.insert("CustomModelData".into(), Tag::Int(*value as i32));
                }
            }
            (47, Component::Int(color)) => display_field(&mut tag, "color", Tag::Int(color))?,
            (48, Component::Int(map)) => {
                tag.insert("map".into(), Tag::Int(map));
            }
            (
                53,
                Component::Potion {
                    potion,
                    color,
                    rest,
                },
            ) => {
                if let Some(name) = potion.and_then(|id| names::POTIONS.get(id as usize)) {
                    tag.insert(
                        "Potion".into(),
                        Tag::String(format!("minecraft:{name}").as_str().into()),
                    );
                }
                if let Some(color) = color {
                    tag.insert("CustomPotionColor".into(), Tag::Int(color));
                }
                let mut r = Reader::new(&rest);
                let mut effects = Vec::new();
                for _ in 0..r.count(1024)? {
                    let id = r.varint()?;
                    let mut effect = effect_tag(&mut r, 0)?;
                    if let Some(name) = names::EFFECTS.get(id as usize) {
                        if let Tag::Compound(fields) = &mut effect {
                            fields.insert(
                                "id".into(),
                                Tag::String(format!("minecraft:{name}").as_str().into()),
                            );
                        }
                        effects.push(effect);
                    }
                }
                if !effects.is_empty() {
                    tag.insert("custom_potion_effects".into(), Tag::List(effects));
                }
            }
            (16, Component::Raw(bytes)) => {
                let mut r = Reader::new(&bytes);
                let mut modifiers = Vec::new();
                for _ in 0..r.count(1024)? {
                    let attribute = r.varint()?;
                    let modifier = r.string(32767)?;
                    let amount = r.f64()?;
                    let operation = r.varint()?;
                    let slot = r.varint()?;
                    match r.varint()? {
                        0 | 1 => (),
                        2 => {
                            r.nbt()?;
                        }
                        _ => return Err(Error::new("Unknown reconstructed modifier display")),
                    }
                    let Some(name) = V1205::ATTRIBUTES
                        .inverse(attribute)
                        .ok()
                        .and_then(|id| names::ATTRIBUTES.get(id as usize))
                    else {
                        continue;
                    };
                    let (uuid, name_modifier) = crate::vfp_modifiers_1205::identity(&modifier);
                    let mut value = BTreeMap::new();
                    value.insert(
                        "AttributeName".into(),
                        Tag::String(
                            if *name == "generic.jump_strength" {
                                "horse.jump_strength"
                            } else {
                                *name
                            }
                            .into(),
                        ),
                    );
                    value.insert("UUID".into(), uuid_tag(uuid));
                    value.insert("Name".into(), Tag::String(name_modifier.as_str().into()));
                    value.insert("Amount".into(), Tag::Double(amount.to_bits()));
                    value.insert("Operation".into(), Tag::Int(operation));
                    if let Some(slot) = names::SLOTS.get(slot as usize) {
                        if *slot != "any" {
                            value.insert("Slot".into(), Tag::String((*slot).into()));
                        }
                    }
                    modifiers.push(Tag::Compound(value));
                }
                tag.insert("AttributeModifiers".into(), Tag::List(modifiers));
            }
            (51 | 52, Component::Stacks(stacks)) => {
                let mut items = Vec::new();
                for stack in stacks {
                    let mut encoded = Writer::new();
                    <crate::item::Slot as crate::buf::Codec<Option<crate::item::Stack>>>::write(
                        &Some(stack),
                        &mut encoded,
                    );
                    let mut old = Writer::new();
                    reverse::<M>(&mut Reader::new(&encoded.data), &mut old, false, registry)?;
                    let mut old = Reader::new(&old.data);
                    if !old.bool()? {
                        continue;
                    }
                    let item = old.varint()?;
                    let count = old.i8()?;
                    let data = if M::PROTOCOL < 764 {
                        old.vfp_named_nbt()?
                    } else {
                        old.nbt()?
                    };
                    if let Some(name) = M::ITEM_IDENTIFIERS.get(item as usize) {
                        let mut fields = BTreeMap::new();
                        fields.insert(
                            "id".into(),
                            Tag::String(format!("minecraft:{name}").as_str().into()),
                        );
                        fields.insert("Count".into(), Tag::Byte(count));
                        if !matches!(data, Tag::End) {
                            fields.insert("tag".into(), data);
                        }
                        items.push(Tag::Compound(fields));
                    }
                }
                tag.insert(
                    if native == 51 {
                        "ChargedProjectiles"
                    } else {
                        "Items"
                    }
                    .into(),
                    Tag::List(items),
                );
            }
            (74, Component::BannerPatterns(patterns)) => {
                let mut values = Vec::new();
                for (id, color) in patterns {
                    if let Ok(name) = registry.name("banner_pattern", id) {
                        if let Some((code, _)) = names::PATTERN_CODES
                            .iter()
                            .find(|(_, value)| *value == local(name))
                        {
                            values.push(Tag::compound([
                                ("Pattern", Tag::String((*code).into())),
                                ("Color", Tag::Int(color)),
                            ]));
                        }
                    }
                }
                block_field(&mut tag, "Patterns", Tag::List(values))?;
            }
            (75, Component::Int(color)) => block_field(&mut tag, "Base", Tag::Int(color))?,
            (56, Component::WritableBook { pages, .. }) => {
                tag.insert(
                    "pages".into(),
                    Tag::List(
                        pages
                            .into_iter()
                            .map(|page| Tag::String(page.as_str().into()))
                            .collect(),
                    ),
                );
            }
            (57, Component::WrittenBook(book)) => {
                tag.insert("title".into(), Tag::String(book.title.as_str().into()));
                tag.insert("author".into(), Tag::String(book.author.as_str().into()));
                tag.insert("generation".into(), Tag::Int(book.generation));
                tag.insert("resolved".into(), Tag::Byte(i8::from(book.resolved)));
                tag.insert(
                    "pages".into(),
                    Tag::List(book.pages.iter().map(json_string).collect::<Result<_>>()?),
                );
            }
            (55, Component::StewEffects(effects)) => {
                tag.insert(
                    "effects".into(),
                    Tag::List(
                        effects
                            .into_iter()
                            .filter_map(|(id, duration)| {
                                names::EFFECTS.get(id as usize).map(|name| {
                                    Tag::compound([
                                        (
                                            "id",
                                            Tag::String(
                                                format!("minecraft:{name}").as_str().into(),
                                            ),
                                        ),
                                        ("duration", Tag::Int(duration)),
                                    ])
                                })
                            })
                            .collect(),
                    ),
                );
            }
            (58, Component::Trim { material, pattern }) => {
                let material = registry
                    .name("trim_material", material)
                    .map_err(Error::new)?;
                let pattern = registry.name("trim_pattern", pattern).map_err(Error::new)?;
                tag.insert(
                    "Trim".into(),
                    Tag::compound([
                        ("material", Tag::String(material.into())),
                        ("pattern", Tag::String(pattern.into())),
                    ]),
                );
            }
            (63, Component::Instrument(id)) => {
                if let Some(name) = names::INSTRUMENTS.get(id as usize) {
                    tag.insert(
                        "instrument".into(),
                        Tag::String(format!("minecraft:{name}").as_str().into()),
                    );
                }
            }
            (61, Component::Nbt(Tag::Compound(values))) => {
                tag.extend(values);
            }
            (69, Component::LodestoneTracker { target, tracked }) => {
                tag.insert("LodestoneTracked".into(), Tag::Byte(i8::from(tracked)));
                if let Some((dimension, pos)) = target {
                    tag.insert(
                        "LodestoneDimension".into(),
                        Tag::String(dimension.as_str().into()),
                    );
                    tag.insert(
                        "LodestonePos".into(),
                        Tag::compound([
                            ("X", Tag::Int(pos[0])),
                            ("Y", Tag::Int(pos[1])),
                            ("Z", Tag::Int(pos[2])),
                        ]),
                    );
                }
            }
            (70, Component::FireworkExplosion(value)) => {
                tag.insert("Explosion".into(), explosion_tag(&value));
            }
            (71, Component::Fireworks { flight, explosions }) => {
                tag.insert(
                    "Fireworks".into(),
                    Tag::compound([
                        ("Flight", Tag::Byte(flight as i8)),
                        (
                            "Explosions",
                            Tag::List(explosions.iter().map(explosion_tag).collect()),
                        ),
                    ]),
                );
            }
            (72, Component::Profile(profile)) => {
                let mut value = BTreeMap::new();
                if let Some(name) = profile.name {
                    value.insert("Name".into(), Tag::String(name.as_str().into()));
                }
                if let Some(id) = profile.id {
                    value.insert("Id".into(), uuid_tag(id));
                }
                let mut properties: BTreeMap<JavaString, Vec<Tag>> = BTreeMap::new();
                for (name, text, signature) in profile.properties {
                    let mut property = BTreeMap::new();
                    property.insert("Value".into(), Tag::String(text.as_str().into()));
                    if let Some(signature) = signature {
                        property.insert("Signature".into(), Tag::String(signature.as_str().into()));
                    }
                    properties
                        .entry(name.as_str().into())
                        .or_default()
                        .push(Tag::Compound(property));
                }
                value.insert(
                    "Properties".into(),
                    Tag::Compound(
                        properties
                            .into_iter()
                            .map(|(name, values)| (name, Tag::List(values)))
                            .collect(),
                    ),
                );
                tag.insert("SkullOwner".into(), Tag::Compound(value));
            }
            (60 | 62, Component::Raw(bytes)) => {
                let mut value = Reader::new(&bytes);
                value.varint()?;
                let value = value.nbt()?;
                tag.insert(
                    if native == 60 {
                        "EntityTag"
                    } else {
                        "BlockEntityTag"
                    }
                    .into(),
                    value,
                );
            }
            (68, Component::Nbt(value)) => {
                tag.insert("Recipes".into(), value);
            }
            (81, Component::Nbt(value)) => {
                tag.insert("Lock".into(), value);
            }
            _ => (),
        }
    }
    for _ in 0..removed {
        input.varint()?;
    }
    if input.remaining() != 0 {
        return Err(Error::new("Trailing reconstructed legacy item patch"));
    }
    if hide != 0 {
        tag.insert("HideFlags".into(), Tag::Int(hide));
    }
    old_header::<M>(
        item,
        count,
        &if tag.is_empty() {
            Tag::End
        } else {
            Tag::Compound(tag)
        },
        false,
        w,
    )
}
