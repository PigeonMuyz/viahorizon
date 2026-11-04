// SPDX-License-Identifier: GPL-3.0-or-later
// Connection-local registry identity. Static numeric maps cannot resolve the
// named EitherHolder values sent before 26.1, especially with custom data packs.
use crate::vfp_core::Result;
use std::collections::{BTreeMap, BTreeSet};

fn key(name: &str) -> String {
    if let Some(path) = name.strip_prefix(':') {
        format!("minecraft:{path}")
    } else if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

#[derive(Clone, Debug, Default)]
pub struct Registries {
    entries: BTreeMap<String, Vec<String>>,
    indices: BTreeMap<String, BTreeMap<String, i32>>,
    tags: BTreeMap<String, BTreeMap<String, Vec<i32>>>,
    dimension_scales: Vec<f64>,
    dimension_sections: Vec<Option<usize>>,
    active_dimension: Option<i32>,
    entity_overrides: BTreeMap<i32, i32>,
    explosion: Option<(f32, i32)>,
}
impl Registries {
    pub fn dimension_heights(&mut self, heights: Vec<Option<i32>>) -> Result<()> {
        if heights.len() > 65536
            || heights
                .iter()
                .flatten()
                .any(|height| *height <= 0 || *height > 4096 || *height % 16 != 0)
        {
            return Err("Invalid dimension height".into());
        }
        self.dimension_sections = heights
            .into_iter()
            .map(|height| height.map(|h| h as usize / 16))
            .collect();
        Ok(())
    }
    pub fn select_dimension(&mut self, id: i32) -> Result<()> {
        if id < 0 {
            return Err("Negative dimension type id".into());
        }
        self.active_dimension = Some(id);
        Ok(())
    }
    pub fn active_sections(&self) -> Option<usize> {
        self.active_dimension
            .and_then(|id| self.dimension_sections.get(id as usize).copied().flatten())
    }
    pub fn entity_override(&self, entity: i32) -> Option<i32> {
        self.entity_overrides.get(&entity).copied()
    }
    pub fn set_entity_override(&mut self, entity: i32, kind: Option<i32>) {
        if let Some(kind) = kind {
            self.entity_overrides.insert(entity, kind);
        } else {
            self.entity_overrides.remove(&entity);
        }
    }
    pub fn clear_entity_overrides(&mut self) {
        self.entity_overrides.clear();
    }
    pub fn set_explosion(&mut self, power: f32, affected: i32) {
        self.explosion = Some((power, affected));
    }
    pub fn explosion(&self) -> Option<(f32, i32)> {
        self.explosion
    }
    pub fn replace(
        &mut self,
        registry: &str,
        entries: impl IntoIterator<Item = String>,
    ) -> Result<()> {
        let registry = key(registry);
        let mut ordered = Vec::new();
        let mut indices = BTreeMap::new();
        for name in entries {
            if ordered.len() >= 65536 {
                return Err("Dynamic registry exceeds 65536 entries".into());
            }
            let name = key(&name);
            if indices.insert(name.clone(), ordered.len() as i32).is_some() {
                return Err("Duplicate dynamic registry identifier".into());
            }
            ordered.push(name);
        }
        self.entries.insert(registry.clone(), ordered);
        self.indices.insert(registry, indices);
        Ok(())
    }
    pub fn id(&self, registry: &str, name: &str) -> Result<i32> {
        self.indices
            .get(&key(registry))
            .and_then(|ids| ids.get(&key(name)))
            .copied()
            .ok_or_else(|| format!("Unknown {registry} registry key {name}"))
    }
    pub fn name(&self, registry: &str, id: i32) -> Result<&str> {
        usize::try_from(id)
            .ok()
            .and_then(|id| {
                self.entries
                    .get(&key(registry))
                    .and_then(|names| names.get(id))
            })
            .map(String::as_str)
            .ok_or_else(|| format!("Unknown {registry} registry id {id}"))
    }
    pub fn contains(&self, registry: &str) -> bool {
        self.entries.contains_key(&key(registry))
    }
    pub fn entry_count(&self, registry: &str) -> usize {
        self.entries.get(&key(registry)).map_or(0, Vec::len)
    }
    pub fn replace_tags(
        &mut self,
        registry: &str,
        tags: impl IntoIterator<Item = (String, Vec<i32>)>,
    ) -> Result<()> {
        let mut converted = BTreeMap::new();
        for (name, mut ids) in tags {
            if ids.iter().any(|id| *id < 0) || ids.len() > 65536 {
                return Err("Invalid dynamic registry tag ids".into());
            }
            let mut seen = BTreeSet::new();
            ids.retain(|id| seen.insert(*id));
            if converted.insert(key(&name), ids).is_some() {
                return Err("Duplicate dynamic registry tag".into());
            }
        }
        self.tags.insert(key(registry), converted);
        Ok(())
    }
    pub fn tag(&self, registry: &str, name: &str) -> Option<&[i32]> {
        self.tags
            .get(&key(registry))?
            .get(&key(name))
            .map(Vec::as_slice)
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn dimension_scales(&mut self, scales: Vec<f64>) -> Result<()> {
        if scales.len() > 65536
            || scales
                .iter()
                .any(|scale| !scale.is_finite() || *scale <= 0.0)
        {
            return Err("Invalid dimension coordinate scale".into());
        }
        self.dimension_scales = scales;
        Ok(())
    }
    pub fn dimension_scale(&self, id: i32) -> f64 {
        usize::try_from(id)
            .ok()
            .and_then(|id| self.dimension_scales.get(id))
            .copied()
            .unwrap_or(1.0)
    }
}
