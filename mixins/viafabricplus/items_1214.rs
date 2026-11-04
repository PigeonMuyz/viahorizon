// SPDX-License-Identifier: GPL-3.0-or-later
// Patch-level 1.21.2-1.21.4 state (identical component ids): tooltip bits moved out of values and
// tropical-fish bucket data moved into separate components in 1.21.5.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_mapping::Maps,
    vfp_tables_1215 as next,
};
use horizon_world::nbt::{JavaString, Tag};
use std::collections::BTreeSet;
pub(crate) const EXTRA: &[&str] = &[
    "banner_patterns",
    "bees",
    "block_entity_data",
    "block_state",
    "bundle_contents",
    "charged_projectiles",
    "container",
    "container_loot",
    "firework_explosion",
    "fireworks",
    "instrument",
    "map_id",
    "painting/variant",
    "pot_decorations",
    "potion_contents",
    "tropical_fish/pattern",
    "written_book_content",
];
const SHOW: &[i32] = &[4, 13, 14, 15, 16, 45, 47, 58, 66];
const FISH: &[&str] = &[
    "tropical_fish/pattern",
    "tropical_fish/base_color",
    "tropical_fish/pattern_color",
];
fn source<M: Maps>(name: &str) -> Option<i32> {
    M::COMPONENT_IDENTIFIERS
        .iter()
        .position(|value| *value == name)
        .map(|id| id as i32)
}
fn native(name: &str) -> i32 {
    let id = next::COMPONENT_IDENTIFIERS
        .iter()
        .position(|value| *value == name)
        .expect("exported component identifier");
    next::DATA_COMPONENT_TYPE
        .map(id as i32)
        .expect("exported native component mapping")
}
pub struct Patch<M: Maps> {
    hide: bool,
    hidden: BTreeSet<i32>,
    fish: [Option<i32>; 3],
    marker: std::marker::PhantomData<M>,
}
impl<M: Maps> Default for Patch<M> {
    fn default() -> Self {
        Self {
            hide: false,
            hidden: BTreeSet::new(),
            fish: [None; 3],
            marker: std::marker::PhantomData,
        }
    }
}
impl<M: Maps> Patch<M> {
    pub fn old_special(&mut self, id: i32, r: &mut Reader<'_>, delimited: bool) -> Result<bool> {
        let name = usize::try_from(id)
            .ok()
            .and_then(|id| M::COMPONENT_IDENTIFIERS.get(id))
            .copied();
        if !matches!(name, Some("hide_tooltip" | "hide_additional_tooltip")) {
            return Ok(false);
        }
        if delimited && r.varint()? != 0 {
            return Err(Error::new("Nonempty old tooltip unit component"));
        }
        if name == Some("hide_tooltip") {
            self.hide = true;
        } else {
            self.hidden.extend(EXTRA.iter().map(|name| native(name)));
        }
        Ok(true)
    }
    fn capture(&mut self, id: i32, r: &mut Reader<'_>) -> Result<()> {
        if id == 18 {
            self.hide = r.bool()?;
            for _ in 0..r.count(1024)? {
                let kind = r.varint()?;
                if !(0..crate::vfp_tables::DATA_COMPONENT_TYPE.mapped_size).contains(&kind) {
                    return Err(Error::new("Invalid hidden native component"));
                }
                self.hidden.insert(kind);
            }
        } else {
            let index = FISH
                .iter()
                .position(|name| native(name) == id)
                .ok_or_else(|| Error::new("Unexpected fish component"))?;
            self.fish[index] = Some(r.varint()?);
        }
        Ok(())
    }
    pub fn native_special(&mut self, id: i32, r: &mut Reader<'_>, delimited: bool) -> Result<bool> {
        if id != 18 && !FISH.iter().any(|name| native(name) == id) {
            return Ok(false);
        }
        if delimited {
            let n = r.count(r.remaining())?;
            let mut value = Reader::new(r.bytes(n)?);
            self.capture(id, &mut value)?;
            if value.remaining() != 0 {
                return Err(Error::new("Trailing tooltip/fish component"));
            }
        } else {
            self.capture(id, r)?;
        }
        Ok(true)
    }
    pub fn forward_value(&mut self, native: i32, bytes: &[u8]) {
        if SHOW.contains(&native) && bytes.last() == Some(&0) {
            self.hidden.insert(native);
        }
    }
    pub fn removed(&self, id: i32, forward: bool, removals: &mut Vec<i32>) -> bool {
        if forward
            && matches!(
                M::COMPONENT_IDENTIFIERS.get(id as usize),
                Some(&"hide_tooltip" | &"hide_additional_tooltip")
            )
        {
            removals.push(18);
            return true;
        }
        if !forward && id == 18 {
            for name in ["hide_tooltip", "hide_additional_tooltip"] {
                removals.push(source::<M>(name).expect("old tooltip identifier"));
            }
            return true;
        }
        false
    }
    pub fn finish_forward(
        &self,
        entries: &mut Vec<(i32, Vec<u8>)>,
        removals: &mut Vec<i32>,
    ) -> Result<()> {
        if let Some((_, bytes)) = entries.iter_mut().find(|(id, _)| *id == 61) {
            let mut r = Reader::new(bytes);
            let mut tag = r.nbt()?;
            if r.remaining() != 0 {
                return Err(Error::new("Trailing bucket data"));
            }
            if let Tag::Compound(fields) = &mut tag {
                if let Some(Tag::Int(packed)) = fields.remove(&JavaString::from("BucketVariantTag"))
                {
                    let mut w = Writer::new();
                    w.nbt(&tag);
                    *bytes = w.data;
                    for (name, value) in FISH.iter().zip([
                        packed & 65535,
                        (packed >> 16) & 255,
                        (packed >> 24) & 255,
                    ]) {
                        let mut w = Writer::new();
                        w.varint(value);
                        entries.push((native(name), w.data));
                    }
                }
            }
        }
        if self.hide || !self.hidden.is_empty() {
            let mut w = Writer::new();
            w.bool(self.hide);
            w.varint(self.hidden.len() as i32);
            for &id in &self.hidden {
                w.varint(id);
            }
            entries.push((18, w.data));
            removals.retain(|id| *id != 18);
        }
        Ok(())
    }
    pub fn finish_reverse(
        &self,
        entries: &mut Vec<(i32, Vec<u8>)>,
        removals: &mut Vec<i32>,
    ) -> Result<()> {
        for (id, bytes) in entries.iter_mut() {
            if let Ok(native) = M::DATA_COMPONENT_TYPE.map(*id) {
                if SHOW.contains(&native) {
                    let show = bytes
                        .last_mut()
                        .ok_or_else(|| Error::new("Missing old tooltip flag"))?;
                    *show = u8::from(!self.hidden.contains(&native));
                }
            }
        }
        for (name, present) in [
            ("hide_tooltip", self.hide),
            (
                "hide_additional_tooltip",
                EXTRA
                    .iter()
                    .filter(|name| source::<M>(name).is_some())
                    .all(|name| self.hidden.contains(&native(name))),
            ),
        ] {
            if present {
                let id = source::<M>(name).expect("old tooltip identifier");
                entries.push((id, Vec::new()));
                removals.retain(|removed| *removed != id);
            }
        }
        if let Some(pattern) = self.fish[0] {
            let id = source::<M>("bucket_entity_data").expect("old bucket identifier");
            let position = entries.iter().position(|(kind, _)| *kind == id);
            let mut data = match position {
                Some(index) => Reader::new(&entries[index].1).nbt()?,
                None => Tag::compound([] as [(&str, Tag); 0]),
            };
            let Tag::Compound(fields) = &mut data else {
                return Err(Error::new("Invalid bucket data"));
            };
            let packed = ((self.fish[2].unwrap_or(255) & 255) << 24)
                | ((self.fish[1].unwrap_or(255) & 255) << 16)
                | (pattern & 65535);
            fields.insert(JavaString::from("BucketVariantTag"), Tag::Int(packed));
            let mut w = Writer::new();
            w.nbt(&data);
            if let Some(index) = position {
                entries[index].1 = w.data;
            } else {
                entries.push((id, w.data));
            }
            removals.retain(|removed| *removed != id);
        }
        Ok(())
    }
}
