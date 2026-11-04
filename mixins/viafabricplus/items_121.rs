// SPDX-License-Identifier: GPL-3.0-or-later
// Food was one component through 1.21.1; the later protocol splits its
// nutrition, consumption effects, and replacement stack into separate values.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_mapping::Maps,
    vfp_registry_access::Registries,
};
use horizon_world::nbt::{JavaString, Tag};
#[derive(Default)]
pub struct Patch {
    seconds: Option<f32>,
    remainder: Option<Vec<u8>>,
    effects: Vec<(Vec<u8>, f32)>,
}
fn sound(r: &mut Reader<'_>) -> Result<()> {
    let id = r.varint()?;
    if id < 0 {
        return Err(Error::new("Negative consumption sound"));
    }
    if id == 0 {
        r.string(32767)?;
        if r.bool()? {
            r.f32()?;
        }
    }
    Ok(())
}
impl Patch {
    pub fn food<M: Maps>(
        &mut self,
        r: &mut Reader<'_>,
        w: &mut Writer,
        depth: usize,
        registry: &Registries,
    ) -> Result<()> {
        w.varint(r.varint()?);
        w.f32(r.f32()?);
        w.bool(r.bool()?);
        self.seconds = Some(r.f32()?);
        if M::PROTOCOL >= 767 && r.bool()? {
            let count = r.varint()?;
            if count < 0 {
                return Err(Error::new("Negative old food replacement count"));
            }
            if count != 0 {
                let item = r.varint()?;
                let mut converted = Writer::new();
                crate::vfp_items::write_template::<M>(
                    item,
                    count,
                    r,
                    &mut converted,
                    true,
                    depth + 1,
                    registry,
                )?;
                self.remainder = Some(converted.data);
            }
        }
        for _ in 0..r.count(1024)? {
            let start = r.position();
            r.varint()?;
            crate::vfp_values::effect_details(r, &mut Writer::new(), depth + 1)?;
            let data = r.since(start).to_vec();
            let probability = r.f32()?;
            self.effects.push((data, probability));
        }
        Ok(())
    }
    fn capture<M: Maps>(
        &mut self,
        id: i32,
        r: &mut Reader<'_>,
        depth: usize,
        registry: &Registries,
    ) -> Result<()> {
        if id == 25 {
            let mut old = Writer::new();
            crate::vfp_items::template_using::<M>(r, &mut old, false, registry)?;
            self.remainder = Some(old.data);
            return Ok(());
        }
        self.seconds = Some(r.f32()?);
        r.varint()?;
        sound(r)?;
        r.bool()?;
        for _ in 0..r.count(1024)? {
            match r.varint()? {
                0 => {
                    let mut effects = Vec::new();
                    for _ in 0..r.count(1024)? {
                        let start = r.position();
                        r.varint()?;
                        crate::vfp_values::effect_details(r, &mut Writer::new(), depth + 1)?;
                        effects.push(r.since(start).to_vec());
                    }
                    let probability = r.f32()?;
                    self.effects
                        .extend(effects.into_iter().map(|effect| (effect, probability)));
                }
                1 => {
                    let n = r.count(65537)?;
                    if n == 0 {
                        r.string(32767)?;
                    } else {
                        for _ in 1..n {
                            r.varint()?;
                        }
                    }
                }
                2 => (),
                3 => {
                    r.f32()?;
                    r.bool()?;
                }
                4 => sound(r)?,
                _ => return Err(Error::new("Unknown native consumption effect")),
            }
        }
        Ok(())
    }
    pub fn native_special<M: Maps>(
        &mut self,
        id: i32,
        r: &mut Reader<'_>,
        delimited: bool,
        depth: usize,
        registry: &Registries,
    ) -> Result<bool> {
        if !matches!(id, 24 | 25) {
            return Ok(false);
        }
        if delimited {
            let n = r.count(r.remaining())?;
            let mut value = Reader::new(r.bytes(n)?);
            self.capture::<M>(id, &mut value, depth, registry)?;
            if value.remaining() != 0 {
                return Err(Error::new("Trailing consumption component"));
            }
        } else {
            self.capture::<M>(id, r, depth, registry)?;
        }
        Ok(true)
    }
    pub fn finish_forward(&self, entries: &mut Vec<(i32, Vec<u8>)>, removals: &mut Vec<i32>) {
        if let Some(seconds) = self.seconds {
            let mut w = Writer::new();
            w.f32(seconds);
            w.varint(1);
            w.varint(0);
            w.string("minecraft:entity.generic.eat");
            w.bool(false);
            w.bool(true);
            w.varint(self.effects.len() as i32);
            for (effect, probability) in &self.effects {
                w.varint(0);
                w.varint(1);
                w.bytes(effect);
                w.f32(*probability);
            }
            entries.push((24, w.data));
            removals.retain(|id| *id != 24);
        }
        if let Some(remainder) = &self.remainder {
            entries.push((25, remainder.clone()));
            removals.retain(|id| *id != 25);
        }
    }
    pub fn finish_reverse<M: Maps>(&self, entries: &mut [(i32, Vec<u8>)]) -> Result<()> {
        let Some(food) = M::COMPONENT_IDENTIFIERS
            .iter()
            .position(|name| *name == "food")
        else {
            return Ok(());
        };
        if let Some((_, bytes)) = entries.iter_mut().find(|(id, _)| *id == food as i32) {
            let mut w = Writer::new();
            w.bytes(bytes);
            w.f32(self.seconds.unwrap_or(1.6));
            if M::PROTOCOL >= 767 {
                w.bool(self.remainder.is_some());
                if let Some(remainder) = &self.remainder {
                    w.bytes(remainder);
                }
            }
            w.varint(self.effects.len() as i32);
            for (effect, probability) in &self.effects {
                w.bytes(effect);
                w.f32(*probability);
            }
            *bytes = w.data;
        }
        Ok(())
    }
}
pub fn fire_matches(data: &[u8]) -> Result<bool> {
    let mut r = Reader::new(data);
    let named = r.varint()? == 0;
    let result = if named {
        let name = r.string(32767)?;
        matches!(name.as_str(), "is_fire" | "minecraft:is_fire")
    } else {
        false
    };
    Ok(result)
}
pub fn lock(r: &mut Reader<'_>, w: &mut Writer, forward: bool) -> Result<()> {
    let value = r.nbt()?;
    if forward {
        let Tag::String(value) = value else {
            return Err(Error::new("Old lock is not text"));
        };
        let json = serde_json::json!({"text":value.to_string_lossy()}).to_string();
        w.nbt(&Tag::compound([(
            "components",
            Tag::compound([("minecraft:custom_name", Tag::String(json.as_str().into()))]),
        )]));
    } else {
        let components = value.get("components");
        let name = components.and_then(|tag| {
            tag.get("minecraft:custom_name")
                .or_else(|| tag.get("custom_name"))
        });
        let Some(Tag::String(json)) = name else {
            return Ok(());
        };
        let text = crate::vfp_text_1214::json_text(&json.to_string_lossy(), 0)?;
        fn plain(tag: &Tag, depth: usize, out: &mut String) -> Result<()> {
            if depth > 64 {
                return Err(Error::new("Lock text nests too deeply"));
            }
            match tag {
                Tag::String(text) => out.push_str(&text.to_string_lossy()),
                Tag::List(values) => {
                    for value in values {
                        plain(value, depth + 1, out)?;
                    }
                }
                Tag::Compound(fields) => {
                    if let Some(value) = fields.get(&JavaString::from("text")) {
                        plain(value, depth + 1, out)?;
                    }
                    if let Some(value) = fields.get(&JavaString::from("extra")) {
                        plain(value, depth + 1, out)?;
                    }
                }
                _ => (),
            }
            Ok(())
        }
        let mut value = String::new();
        plain(&text, 0, &mut value)?;
        w.nbt(&Tag::String(value.as_str().into()));
    }
    Ok(())
}
