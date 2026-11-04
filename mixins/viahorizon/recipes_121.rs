// SPDX-License-Identifier: GPL-3.0-or-later
// Connection-owned legacy recipes. The display ids are stable for one recipe
// declaration; every embedded stack is converted directly to the native wire.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
    vfp_mapping::Maps,
    vfp_registry_access::Registries,
    vfp_tables as native,
    vfp_wire::{self as wire, Packet},
};
use std::collections::{BTreeMap, BTreeSet};
fn key(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}
#[derive(Clone)]
struct Item {
    id: i32,
    value: Vec<u8>,
}
type Ingredient = Vec<Item>;
struct Recipe {
    key: String,
    group: i32,
    category: i32,
    kind: i32,
    width: i32,
    height: i32,
    ingredients: Vec<Ingredient>,
    result: Item,
    notification: bool,
    duration: i32,
    experience: f32,
}
#[derive(Default)]
pub struct Recipes {
    recipes: Vec<Recipe>,
    keys: BTreeMap<String, usize>,
    groups: BTreeMap<String, i32>,
    inputs: BTreeMap<String, BTreeSet<i32>>,
    stone: Vec<Recipe>,
}
fn packet(name: &str, data: Vec<u8>) -> Result<Packet> {
    Ok((
        wire::packet_id(native::NEW_CLIENTBOUND, name).map_err(Error::new)?,
        data,
    ))
}
fn item<M: Maps>(r: &mut Reader<'_>, registry: &Registries, allow_empty: bool) -> Result<Item> {
    if M::NATIVE_ITEMS {
        let mut native = Writer::new();
        crate::vfp_items_1203::forward::<M>(r, &mut native, registry)?;
        let mut r = Reader::new(&native.data);
        let count = r.varint()?;
        if count == 0 {
            if allow_empty {
                return Ok(Item {
                    id: -1,
                    value: Vec::new(),
                });
            }
            let id = M::ITEMS.map(1).map_err(Error::new)?;
            let mut w = Writer::new();
            w.varint(id);
            w.varint(1);
            w.varint(0);
            w.varint(0);
            return Ok(Item { id, value: w.data });
        }
        let id = r.varint()?;
        let mut w = Writer::new();
        w.varint(id);
        w.varint(count);
        w.bytes(r.rest());
        return Ok(Item { id, value: w.data });
    }
    let count = r.varint()?;
    if count <= 0 {
        return Err(Error::new("Empty recipe ingredient/result stack"));
    }
    let source = r.varint()?;
    let id = M::ITEMS.map(source).map_err(Error::new)?;
    let mut w = Writer::new();
    crate::vfp_items::write_template::<M>(source, count, r, &mut w, true, 0, registry)?;
    Ok(Item { id, value: w.data })
}
fn ingredient<M: Maps>(r: &mut Reader<'_>, registry: &Registries) -> Result<Ingredient> {
    let mut items = Vec::new();
    for _ in 0..r.count(4096)? {
        let item = item::<M>(r, registry, true)?;
        if item.id >= 0 {
            items.push(item);
        }
    }
    Ok(items)
}
fn item_display(w: &mut Writer, item: &Item) {
    w.varint(5);
    w.bytes(&item.value);
}
fn ingredient_display(w: &mut Writer, items: &Ingredient) {
    match items.len() {
        0 => w.varint(0),
        1 => item_display(w, &items[0]),
        n => {
            w.varint(10);
            w.varint(n as i32);
            for item in items {
                item_display(w, item);
            }
        }
    }
}
fn holders(w: &mut Writer, items: &Ingredient) {
    w.varint(items.len() as i32 + 1);
    for item in items {
        w.varint(item.id);
    }
}
impl Recipe {
    fn display(&self, w: &mut Writer) {
        w.varint(self.kind);
        match self.kind {
            0 | 1 => {
                if self.kind == 1 {
                    w.varint(self.width);
                    w.varint(self.height);
                }
                w.varint(self.ingredients.len() as i32);
                for items in &self.ingredients {
                    ingredient_display(w, items);
                }
                item_display(w, &self.result);
                w.varint(0);
            }
            2 => {
                ingredient_display(w, &self.ingredients[0]);
                w.varint(1);
                item_display(w, &self.result);
                w.varint(0);
                w.varint(self.duration);
                w.f32(self.experience);
            }
            _ => unreachable!(),
        }
    }
}
impl Recipes {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    fn group(&mut self, r: &mut Reader<'_>) -> Result<i32> {
        let value = r.string(32767)?;
        let value = key(&value);
        if value.is_empty() {
            return Ok(-1);
        }
        let n = self.groups.len() as i32;
        Ok(*self.groups.entry(value.to_owned()).or_insert(n))
    }
    fn input<M: Maps>(
        &mut self,
        name: &str,
        r: &mut Reader<'_>,
        registry: &Registries,
    ) -> Result<Ingredient> {
        let items = ingredient::<M>(r, registry)?;
        self.inputs
            .entry(name.to_owned())
            .or_default()
            .extend(items.iter().map(|item| item.id));
        Ok(items)
    }
    pub fn update<M: Maps>(&mut self, data: &[u8], registry: &Registries) -> Result<Packet> {
        // Build separately so malformed updates cannot leave half a recipe set.
        let mut next = Self::default();
        let mut r = Reader::new(data);
        for _ in 0..r.count(65536)? {
            let legacy_kind = if M::NATIVE_ITEMS {
                Some(r.string(32767)?)
            } else {
                None
            };
            let name = r.string(32767)?;
            let name = key(&name).to_owned();
            if M::PROTOCOL < 763 && legacy_kind.as_deref().map(key) == Some("smithing") {
                ingredient::<M>(&mut r, registry)?;
                ingredient::<M>(&mut r, registry)?;
                item::<M>(&mut r, registry, false)?;
                continue; // This removed recipe type has no native display.
            }
            let serializer = if let Some(kind) = legacy_kind {
                M::RECIPE_IDENTIFIERS
                    .iter()
                    .position(|name| *name == key(&kind))
                    .map(|id| id as i32)
                    .ok_or_else(|| Error::new(format!("Unknown legacy recipe serializer {kind}")))?
            } else {
                r.varint()?
            };
            let kind = usize::try_from(serializer)
                .ok()
                .and_then(|id| M::RECIPE_IDENTIFIERS.get(id))
                .ok_or_else(|| Error::new("Unknown legacy recipe serializer"))?;
            match *kind {
                "crafting_shaped" | "crafting_shapeless" | "smelting" | "blasting" | "smoking"
                | "campfire_cooking" | "stonecutting" => {
                    let old_shape = if M::PROTOCOL < 765 && *kind == "crafting_shaped" {
                        Some((r.varint()?, r.varint()?))
                    } else {
                        None
                    };
                    let group = next.group(&mut r)?;
                    let category = if *kind == "stonecutting" {
                        0
                    } else {
                        r.varint()?
                    };
                    let (display, width, height, ingredients, duration, experience);
                    let mut notification = true;
                    if *kind == "crafting_shaped" {
                        display = 1;
                        (width, height) = if let Some(shape) = old_shape {
                            shape
                        } else {
                            (r.varint()?, r.varint()?)
                        };
                        if !(1..=3).contains(&width) || !(1..=3).contains(&height) {
                            return Err(Error::new("Invalid shaped recipe dimensions"));
                        }
                        let mut values = Vec::new();
                        for _ in 0..width * height {
                            values.push(ingredient::<M>(&mut r, registry)?);
                        }
                        ingredients = values;
                    } else if *kind == "crafting_shapeless" {
                        display = 0;
                        width = 0;
                        height = 0;
                        let mut values = Vec::new();
                        for _ in 0..r.count(9)? {
                            values.push(ingredient::<M>(&mut r, registry)?);
                        }
                        ingredients = values;
                    } else {
                        display = if *kind == "stonecutting" { 3 } else { 2 };
                        width = 0;
                        height = 0;
                        let input = match *kind {
                            "smelting" => "furnace_input",
                            "blasting" => "blast_furnace_input",
                            "smoking" => "smoker_input",
                            "campfire_cooking" => "campfire_input",
                            _ => "",
                        };
                        ingredients = vec![if input.is_empty() {
                            ingredient::<M>(&mut r, registry)?
                        } else {
                            next.input::<M>(input, &mut r, registry)?
                        }];
                    }
                    let result = item::<M>(&mut r, registry, false)?;
                    if display == 1 && M::PROTOCOL >= 762 {
                        notification = r.bool()?;
                    }
                    if display == 2 {
                        experience = r.f32()?;
                        duration = r.varint()?;
                    } else {
                        experience = 0.0;
                        duration = 0;
                    }
                    let recipe = Recipe {
                        key: name.clone(),
                        group,
                        category,
                        kind: display,
                        width,
                        height,
                        ingredients,
                        result,
                        notification,
                        duration,
                        experience,
                    };
                    if display == 3 {
                        next.stone.push(recipe);
                    } else {
                        if next.keys.insert(name, next.recipes.len()).is_some() {
                            return Err(Error::new("Duplicate legacy recipe key"));
                        }
                        next.recipes.push(recipe);
                    }
                }
                "smithing_transform" | "smithing_trim" => {
                    for input in ["smithing_template", "smithing_base", "smithing_addition"] {
                        next.input::<M>(input, &mut r, registry)?;
                    }
                    if *kind == "smithing_transform" {
                        item::<M>(&mut r, registry, false)?;
                    }
                }
                value
                    if value.starts_with("crafting_special_")
                        || value == "crafting_decorated_pot" =>
                {
                    r.varint()?;
                }
                _ => {
                    return Err(Error::new(format!(
                        "Unsupported legacy recipe serializer {kind}"
                    )));
                }
            }
        }
        if r.remaining() != 0 {
            return Err(Error::new("Trailing legacy recipe data"));
        }
        // Native ids preserve the target's identifier order. Sort by the
        // description key exactly as the 1.21.2 recipe bridge does.
        next.stone.sort_by_cached_key(|recipe| {
            let source = M::ITEMS
                .inverse(recipe.result.id)
                .ok()
                .and_then(|id| M::ITEM_IDENTIFIERS.get(id as usize))
                .copied()
                .unwrap_or("");
            (
                u8::from(!M::BLOCK_IDENTIFIERS.contains(&source)),
                source.to_owned(),
            )
        });
        let mut w = Writer::new();
        w.varint(next.inputs.len() as i32 + 2);
        for (name, ids) in [
            ("minecraft:brewing_input", native::BREWING_INPUTS),
            ("minecraft:brewing_reagent", native::BREWING_REAGENTS),
        ] {
            w.string(name);
            w.varint(ids.len() as i32);
            for id in ids {
                w.varint(*id);
            }
        }
        for (name, ids) in &next.inputs {
            w.string(name);
            w.varint(ids.len() as i32);
            for id in ids {
                w.varint(*id);
            }
        }
        w.varint(next.stone.len() as i32);
        for recipe in &next.stone {
            holders(&mut w, &recipe.ingredients[0]);
            item_display(&mut w, &recipe.result);
        }
        *self = next;
        packet("UPDATE_RECIPES", w.data)
    }
    pub fn book(&self, data: &[u8]) -> Result<Vec<Packet>> {
        let mut r = Reader::new(data);
        let state = r.varint()?;
        if !(0..=2).contains(&state) {
            return Err(Error::new("Invalid legacy recipe book state"));
        }
        let mut settings = Writer::new();
        for _ in 0..8 {
            settings.bool(r.bool()?);
        }
        let mut ids = Vec::new();
        for _ in 0..r.count(65536)? {
            let s = r.string(32767)?;
            ids.push(key(&s).to_owned());
        }
        let mut highlight = BTreeSet::new();
        if state == 0 {
            for _ in 0..r.count(65536)? {
                let s = r.string(32767)?;
                highlight.insert(key(&s).to_owned());
            }
        }
        if r.remaining() != 0 {
            return Err(Error::new("Trailing legacy recipe book data"));
        }
        let recipes: Vec<_> = ids
            .iter()
            .filter_map(|key| self.keys.get(key).copied())
            .collect();
        let mut w = Writer::new();
        w.varint(recipes.len() as i32);
        for id in recipes {
            w.varint(id as i32);
            if state == 2 {
                continue;
            }
            let recipe = &self.recipes[id];
            recipe.display(&mut w);
            w.varint(recipe.group + 1);
            w.varint(recipe.category);
            w.bool(true);
            let inputs: Vec<_> = recipe
                .ingredients
                .iter()
                .filter(|input| !input.is_empty())
                .collect();
            w.varint(inputs.len() as i32);
            for input in inputs {
                holders(&mut w, input);
            }
            w.u8(if state == 1 {
                2 | u8::from(recipe.notification)
            } else {
                u8::from(highlight.contains(&recipe.key)) * 2
            });
        }
        if state != 2 {
            w.bool(state == 0);
        }
        Ok(vec![
            packet("RECIPE_BOOK_SETTINGS", settings.data)?,
            packet(
                if state == 2 {
                    "RECIPE_BOOK_REMOVE"
                } else {
                    "RECIPE_BOOK_ADD"
                },
                w.data,
            )?,
        ])
    }
    pub fn ghost(&self, data: &[u8]) -> Result<Option<Packet>> {
        let mut r = Reader::new(data);
        let container = i32::from(r.i8()?);
        let name = r.string(32767)?;
        if r.remaining() != 0 {
            return Err(Error::new("Trailing ghost recipe data"));
        }
        let Some(id) = self.keys.get(key(&name)) else {
            return Ok(None);
        };
        let mut w = Writer::new();
        w.varint(container);
        self.recipes[*id].display(&mut w);
        Ok(Some(packet("PLACE_GHOST_RECIPE", w.data)?))
    }
    pub fn serverbound(&self, name: &str, data: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut r = Reader::new(data);
        let mut w = Writer::new();
        if name == "PLACE_RECIPE" {
            let id = r.varint()?;
            if !(0..=255).contains(&id) {
                return Err(Error::new("Legacy recipe container out of range"));
            }
            w.u8(id as u8);
        }
        let id = r.varint()?;
        let Some(recipe) = usize::try_from(id).ok().and_then(|id| self.recipes.get(id)) else {
            return Ok(None);
        };
        w.string(&recipe.key);
        w.bytes(r.rest());
        Ok(Some(w.data))
    }
}
