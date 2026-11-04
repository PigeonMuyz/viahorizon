// SPDX-License-Identifier: GPL-3.0-or-later
// 26.2 -> 26.3 registry schema changes. Namespaced custom values are retained.
use crate::{Error, Result};
use horizon_world::nbt::{Compound, JavaString, Tag};
fn key(s: &str) -> JavaString {
    s.into()
}
fn text(s: &str) -> Tag {
    Tag::String(s.into())
}
fn local(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}
fn typename(map: &Compound) -> String {
    match map.get(&key("type")) {
        Some(Tag::String(s)) => s.to_string_lossy(),
        _ => String::new(),
    }
}
fn depth(n: usize) -> Result<()> {
    if n > 64 {
        Err(Error::new("Registry expression nesting exceeds 64"))
    } else {
        Ok(())
    }
}
fn unary(kind: &str, input: Tag) -> Tag {
    Tag::compound([
        ("type", text(&format!("minecraft:{kind}"))),
        ("input", input),
    ])
}
fn number(tag: &Tag) -> Option<f32> {
    match tag {
        Tag::Byte(v) => Some(*v as f32),
        Tag::Short(v) => Some(*v as f32),
        Tag::Int(v) => Some(*v as f32),
        Tag::Long(v) => Some(*v as f32),
        Tag::Float(v) => Some(f32::from_bits(*v)),
        Tag::Double(v) => Some(f64::from_bits(*v) as f32),
        _ => None,
    }
}
fn replace(
    map: &mut Compound,
    old: &str,
    new: &str,
    f: impl FnOnce(Tag) -> Result<Tag>,
) -> Result<()> {
    if let Some(value) = map.remove(&key(old)) {
        map.insert(key(new), f(value)?);
    }
    Ok(())
}
pub fn int_provider(value: Tag) -> Result<Tag> {
    int_at(value, 0)
}
fn int_at(value: Tag, n: usize) -> Result<Tag> {
    depth(n)?;
    if let Some(value) = number(&value) {
        return Ok(Tag::Int(((value as f64 + 0.5).floor()) as i32));
    }
    if let Tag::Compound(mut map) = value {
        if let Some(scale) = map.get(&key("scale")) {
            if local(&typename(&map)) == "score" && number(scale).is_none() {
                return Err(Error::new("Invalid score provider scale"));
            }
        }
        match local(&typename(&map)) {
            "binomial" => {
                replace(&mut map, "n", "n", |v| int_at(v, n + 1))?;
                replace(&mut map, "p", "p", |v| float_at(v, n + 1))?;
                return Ok(Tag::Compound(map));
            }
            "score" if map.get(&key("scale")).and_then(number).unwrap_or(1.0) == 1.0 => {
                map.remove(&key("scale"));
                return Ok(Tag::Compound(map));
            }
            _ => {
                return Ok(unary(
                    "from_float",
                    unary("round", float_at(Tag::Compound(map), n + 1)?),
                ));
            }
        }
    }
    Err(Error::new("Invalid integer number provider"))
}
pub fn float_provider(value: Tag) -> Result<Tag> {
    float_at(value, 0)
}
fn float_compound(value: Tag, n: usize) -> Result<Tag> {
    let value = float_at(value, n)?;
    if matches!(value, Tag::Compound(_)) {
        Ok(value)
    } else {
        Ok(Tag::compound([
            ("type", text("minecraft:constant")),
            ("value", value),
        ]))
    }
}
fn float_at(value: Tag, n: usize) -> Result<Tag> {
    depth(n)?;
    if number(&value).is_some() {
        return Ok(value);
    }
    let Tag::Compound(mut map) = value else {
        return Err(Error::new("Invalid float number provider"));
    };
    let kind = typename(&map);
    match if kind.is_empty() {
        "uniform"
    } else {
        local(&kind)
    } {
        "uniform" => {
            map.insert(key("type"), text("minecraft:uniform"));
            replace(&mut map, "min", "min", |v| float_at(v, n + 1))?;
            replace(&mut map, "max", "max", |v| float_at(v, n + 1))?;
        }
        "sum" => {
            map.insert(key("type"), text("minecraft:add"));
            replace(&mut map, "summands", "inputs", |v| {
                let Tag::List(values) = v else {
                    return Err(Error::new("Invalid sum provider inputs"));
                };
                Ok(Tag::List(
                    values
                        .into_iter()
                        .map(|v| float_compound(v, n + 1))
                        .collect::<Result<_>>()?,
                ))
            })?;
        }
        "binomial" => return Ok(unary("from_int", int_at(Tag::Compound(map), n + 1)?)),
        "score" => {
            let scale = map.remove(&key("scale"));
            let score = unary("from_int", Tag::Compound(map));
            return match scale {
                None => Ok(score),
                Some(scale) => Ok(Tag::compound([
                    ("type", text("minecraft:mul")),
                    (
                        "inputs",
                        Tag::List(vec![score, float_compound(scale, n + 1)?]),
                    ),
                ])),
            };
        }
        _ => (),
    }
    Ok(Tag::Compound(map))
}
fn int_range(value: Tag, n: usize) -> Result<Tag> {
    if let Tag::Compound(mut map) = value {
        replace(&mut map, "min", "min", |v| int_at(v, n + 1))?;
        replace(&mut map, "max", "max", |v| int_at(v, n + 1))?;
        Ok(Tag::Compound(map))
    } else {
        Ok(value)
    }
}
pub fn predicate(tag: &mut Tag) -> Result<()> {
    let mut upgraded = tag.clone();
    predicate_at(&mut upgraded, 0)?;
    *tag = upgraded;
    Ok(())
}
fn predicate_at(tag: &mut Tag, n: usize) -> Result<()> {
    depth(n)?;
    let Tag::Compound(map) = tag else {
        return Err(Error::new("Enchantment predicate is not a compound"));
    };
    if let Some(condition) = map.remove(&key("condition")) {
        if !matches!(condition, Tag::String(_)) {
            return Err(Error::new("Invalid predicate condition type"));
        }
        map.insert(key("type"), condition);
    }
    if map
        .get(&key("type"))
        .is_some_and(|v| !matches!(v, Tag::String(_)))
    {
        return Err(Error::new("Invalid predicate type"));
    }
    let kind = typename(map);
    match local(&kind) {
        "value_check" => {
            map.insert(key("type"), text("minecraft:int_value_check"));
            replace(map, "value", "value", |v| int_at(v, n + 1))?;
            replace(map, "range", "test", |v| int_range(v, n + 1))?;
        }
        "time_check" => replace(map, "value", "value", |v| int_range(v, n + 1))?,
        "random_chance" => replace(map, "chance", "chance", |v| float_at(v, n + 1))?,
        "block_state_property" => {
            map.insert(key("type"), text("minecraft:match_block"));
            if let Some(v) = map.remove(&key("block")) {
                map.insert(key("blocks"), v);
            }
            if let Some(v) = map.remove(&key("properties")) {
                map.insert(key("state"), v);
            }
        }
        "damage_source_properties" => {
            if let Some(Tag::Compound(p)) = map.get_mut(&key("predicate")) {
                if let Some(Tag::List(tags)) = p.get_mut(&key("tags")) {
                    for tag in tags {
                        if let Some(Tag::String(id)) = tag.get_mut("id") {
                            let value = id.to_string_lossy();
                            if !value.starts_with('#') {
                                *id = format!("#{value}").as_str().into();
                            }
                        }
                    }
                }
            }
        }
        _ => (),
    }
    if let Some(child) = map.get_mut(&key("term")) {
        predicate_at(child, n + 1)?;
    }
    if let Some(Tag::List(children)) = map.get_mut(&key("terms")) {
        for child in children {
            predicate_at(child, n + 1)?;
        }
    }
    Ok(())
}
fn state(tag: &mut Tag) {
    if let Tag::Compound(map) = tag {
        if let Some(v) = map.remove(&key("Name")) {
            map.insert(key("id"), v);
        }
        if let Some(v) = map.remove(&key("Properties")) {
            map.insert(key("properties"), v);
        }
    }
}
pub fn block_provider(tag: &mut Tag) -> Result<()> {
    let mut upgraded = tag.clone();
    block_at(&mut upgraded, 0)?;
    *tag = upgraded;
    Ok(())
}
fn block_at(tag: &mut Tag, n: usize) -> Result<()> {
    depth(n)?;
    let Tag::Compound(map) = tag else {
        return Err(Error::new("Block state provider is not a compound"));
    };
    let kind = typename(map);
    if kind.is_empty() {
        state(tag);
        return Ok(());
    }
    let kind = local(&kind);
    let renamed = match kind {
        "simple_state_provider" => "simple",
        "weighted_state_provider" => "weighted",
        "randomized_int_state_provider" => "randomized_int",
        "rotated_block_provider" => "rotated",
        "rule_based_state_provider" => "rule_based",
        s => s.strip_suffix("_provider").unwrap_or(s),
    };
    if !renamed.contains(':') {
        map.insert(key("type"), text(&format!("minecraft:{renamed}")));
    }
    for k in ["state", "default_state"] {
        if let Some(v) = map.get_mut(&key(k)) {
            state(v);
        }
    }
    for k in ["states", "low_states", "high_states"] {
        if let Some(Tag::List(values)) = map.get_mut(&key(k)) {
            for v in values {
                state(v);
            }
        }
    }
    if let Some(Tag::List(entries)) = map.get_mut(&key("entries")) {
        for entry in entries {
            if let Some(data) = entry.get_mut("data") {
                state(data);
            }
        }
    }
    for k in ["source", "fallback", "source_block_state_provider"] {
        if let Some(v) = map.get_mut(&key(k)) {
            block_at(v, n + 1)?;
        }
    }
    if let Some(Tag::List(rules)) = map.get_mut(&key("rules")) {
        for rule in rules {
            if let Some(v) = rule.get_mut("then") {
                block_at(v, n + 1)?;
            }
        }
    }
    Ok(())
}
fn effects(tag: &mut Tag, n: usize) -> Result<()> {
    depth(n)?;
    match tag {
        Tag::List(list) => {
            for value in list {
                effects(value, n + 1)?;
            }
        }
        Tag::Compound(map) => {
            if let Some(requirements) = map.get_mut(&key("requirements")) {
                predicate_at(requirements, n + 1)?;
            }
            let kind = typename(map);
            if matches!(local(&kind), "replace_disk" | "replace_block") {
                if let Some(provider) = map.get_mut(&key("block_state")) {
                    block_at(provider, n + 1)?;
                }
            }
            if local(&kind) == "spawn_particles" {
                if let Some(particle) = map.get_mut(&key("particle")) {
                    if let Some(block) = particle.get_mut("block_state") {
                        state(block);
                    }
                }
            }
            for (k, v) in map {
                if k != &key("requirements") && k != &key("block_state") {
                    effects(v, n + 1)?;
                }
            }
        }
        _ => (),
    }
    Ok(())
}
pub fn upgrade_registry(name: &str, tag: &mut Tag) -> Result<()> {
    match local(name) {
        "enchantment" => {
            if let Some(v) = tag.get_mut("effects") {
                let mut converted = v.clone();
                effects(&mut converted, 0)?;
                *v = converted;
            }
        }
        "worldgen/block_state_provider" => block_provider(tag)?,
        _ => (),
    }
    Ok(())
}
