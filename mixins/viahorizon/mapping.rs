// SPDX-License-Identifier: GPL-3.0-or-later
// Explicit source-version maps shared by the typed codecs. No thread/global
// selection can change a connection's mapping set halfway through a packet.
use crate::{vfp_tables as current, vfp_tables_261_native as previous, vfp_wire::IdMap};
pub trait Maps {
    const PROTOCOL: i32;
    // Pre-component packet adapters normalize complete item bodies before
    // their structural chain. Other static registries retain source ids.
    const NATIVE_ITEMS: bool = Self::PROTOCOL < 766;
    const DATA_VERSION: Option<i32> = match Self::PROTOCOL {
        761 => Some(crate::vfp_data_versions::P761),
        762 => Some(crate::vfp_data_versions::P762),
        763 => Some(crate::vfp_data_versions::P763),
        764 => Some(crate::vfp_data_versions::P764),
        765 => Some(crate::vfp_data_versions::P765),
        766 => Some(crate::vfp_data_versions::P766),
        767 => Some(crate::vfp_data_versions::P767),
        768 => Some(crate::vfp_data_versions::P768),
        769 => Some(crate::vfp_data_versions::P769),
        770 => Some(crate::vfp_data_versions::P770),
        771 => Some(crate::vfp_data_versions::P771),
        772 => Some(crate::vfp_data_versions::P772),
        773 => Some(crate::vfp_data_versions::P773),
        774 => Some(crate::vfp_data_versions::P774),
        775 => Some(crate::vfp_data_versions::P775),
        776 => Some(crate::vfp_data_versions::P776),
        777 => Some(crate::vfp_data_versions::P777),
        _ => None,
    };
    const ARGUMENTTYPES: IdMap;
    const ATTRIBUTES: IdMap;
    const BLOCKENTITIES: IdMap;
    const BLOCKS: IdMap;
    const BLOCKSTATES: IdMap;
    const DATA_COMPONENT_TYPE: IdMap;
    const ENTITIES: IdMap;
    const ITEMS: IdMap;
    const PARTICLES: IdMap;
    const SOUNDS: IdMap;
    const STATISTICS: IdMap;
    const BRICK_OLD: i32;
    const MAP_COLOR_OLD: i32 = 45;
    const SWING_OLD: i32 = 40;
    const BED_OLD: Option<i32> = None;
    const SLOT_DISPLAYS: IdMap = IdMap {
        size: 11,
        mapped_size: 11,
        runs: &[(0, 0)],
    };
    const ENTITY_IDENTIFIERS: &'static [&'static str] = &[];
    const ENTITY_FAMILIES: &'static [u8] = &[];
    const BLOCK_ENTITY_IDENTIFIERS: &'static [&'static str] = &[];
    const SOUND_IDENTIFIERS: &'static [&'static str] = &[];
    const COMPONENT_IDENTIFIERS: &'static [&'static str] = &[];
    const ITEM_IDENTIFIERS: &'static [&'static str] = &[];
    const BLOCK_IDENTIFIERS: &'static [&'static str] = &[];
    const RECIPE_IDENTIFIERS: &'static [&'static str] = &[];
    const ATTRIBUTE_IDENTIFIERS: &'static [&'static str] = &[];
    const SADDLE_OLD: i32 = 0;
    const SHIELD_BLOCK_OLD: i32 = 0;
    const SHIELD_BREAK_OLD: i32 = 0;
}
pub struct V262;
impl Maps for V262 {
    const PROTOCOL: i32 = 776;
    const ARGUMENTTYPES: IdMap = current::ARGUMENTTYPES;
    const ATTRIBUTES: IdMap = current::ATTRIBUTES;
    const BLOCKENTITIES: IdMap = current::BLOCKENTITIES;
    const BLOCKS: IdMap = current::BLOCKS;
    const BLOCKSTATES: IdMap = current::BLOCKSTATES;
    const DATA_COMPONENT_TYPE: IdMap = current::DATA_COMPONENT_TYPE;
    const ENTITIES: IdMap = current::ENTITIES;
    const ITEMS: IdMap = current::ITEMS;
    const PARTICLES: IdMap = current::PARTICLES;
    const SOUNDS: IdMap = current::SOUNDS;
    const STATISTICS: IdMap = current::STATISTICS;
    const BRICK_OLD: i32 = current::BRICK_262;
    const BLOCK_ENTITY_IDENTIFIERS: &'static [&'static str] = current::BLOCK_ENTITY_IDENTIFIERS;
}
pub struct V261;
impl Maps for V261 {
    const PROTOCOL: i32 = 775;
    const ARGUMENTTYPES: IdMap = previous::ARGUMENTTYPES;
    const ATTRIBUTES: IdMap = previous::ATTRIBUTES;
    const BLOCKENTITIES: IdMap = previous::BLOCKENTITIES;
    const BLOCKS: IdMap = previous::BLOCKS;
    const BLOCKSTATES: IdMap = previous::BLOCKSTATES;
    const DATA_COMPONENT_TYPE: IdMap = previous::DATA_COMPONENT_TYPE;
    const ENTITIES: IdMap = previous::ENTITIES;
    const ITEMS: IdMap = previous::ITEMS;
    const PARTICLES: IdMap = previous::PARTICLES;
    const SOUNDS: IdMap = previous::SOUNDS;
    const STATISTICS: IdMap = previous::STATISTICS;
    const BRICK_OLD: i32 = previous::BRICK_OLD;
    const BED_OLD: Option<i32> = Some(25);
    const BLOCK_ENTITY_IDENTIFIERS: &'static [&'static str] = previous::BLOCK_ENTITY_IDENTIFIERS;
}
pub struct V12111;
impl Maps for V12111 {
    const PROTOCOL: i32 = 774;
    const ARGUMENTTYPES: IdMap = crate::vfp_tables_12111::ARGUMENTTYPES;
    const ATTRIBUTES: IdMap = crate::vfp_tables_12111::ATTRIBUTES;
    const BLOCKENTITIES: IdMap = crate::vfp_tables_12111::BLOCKENTITIES;
    const BLOCKS: IdMap = crate::vfp_tables_12111::BLOCKS;
    const BLOCKSTATES: IdMap = crate::vfp_tables_12111::BLOCKSTATES;
    const DATA_COMPONENT_TYPE: IdMap = crate::vfp_tables_12111::DATA_COMPONENT_TYPE;
    const ENTITIES: IdMap = crate::vfp_tables_12111::ENTITIES;
    const ITEMS: IdMap = crate::vfp_tables_12111::ITEMS;
    const PARTICLES: IdMap = crate::vfp_tables_12111::PARTICLES;
    const SOUNDS: IdMap = crate::vfp_tables_12111::SOUNDS;
    const STATISTICS: IdMap = crate::vfp_tables_12111::STATISTICS;
    const BRICK_OLD: i32 = crate::vfp_tables_12111::BRICK_OLD;
    const MAP_COLOR_OLD: i32 = crate::vfp_tables_12111::MAP_COLOR_OLD;
    const SWING_OLD: i32 = crate::vfp_tables_12111::SWING_ANIMATION_OLD;
    const BED_OLD: Option<i32> = Some(crate::vfp_tables_12111::BED_OLD);
    const SLOT_DISPLAYS: IdMap = crate::vfp_tables_12111::SLOT_DISPLAYS;
    const BLOCK_ENTITY_IDENTIFIERS: &'static [&'static str] =
        crate::vfp_tables_12111::BLOCK_ENTITY_IDENTIFIERS;
}
pub struct V1219;
impl Maps for V1219 {
    const PROTOCOL: i32 = 773;
    const ARGUMENTTYPES: IdMap = crate::vfp_tables_1219::ARGUMENTTYPES;
    const ATTRIBUTES: IdMap = crate::vfp_tables_1219::ATTRIBUTES;
    const BLOCKENTITIES: IdMap = crate::vfp_tables_1219::BLOCKENTITIES;
    const BLOCKS: IdMap = crate::vfp_tables_1219::BLOCKS;
    const BLOCKSTATES: IdMap = crate::vfp_tables_1219::BLOCKSTATES;
    const DATA_COMPONENT_TYPE: IdMap = crate::vfp_tables_1219::DATA_COMPONENT_TYPE;
    const ENTITIES: IdMap = crate::vfp_tables_1219::ENTITIES;
    const ITEMS: IdMap = crate::vfp_tables_1219::ITEMS;
    const PARTICLES: IdMap = crate::vfp_tables_1219::PARTICLES;
    const SOUNDS: IdMap = crate::vfp_tables_1219::SOUNDS;
    const STATISTICS: IdMap = crate::vfp_tables_1219::STATISTICS;
    const BRICK_OLD: i32 = crate::vfp_tables_1219::BRICK_OLD;
    const MAP_COLOR_OLD: i32 = crate::vfp_tables_1219::MAP_COLOR_OLD;
    const SWING_OLD: i32 = -1;
    const BED_OLD: Option<i32> = Some(crate::vfp_tables_1219::BED_OLD);
    const SLOT_DISPLAYS: IdMap = crate::vfp_tables_1219::SLOT_DISPLAYS;
    const BLOCK_ENTITY_IDENTIFIERS: &'static [&'static str] =
        crate::vfp_tables_1219::BLOCK_ENTITY_IDENTIFIERS;
}
macro_rules! older_maps {
    ($name:ident, $protocol:literal, $table:ident) => {
        pub struct $name;
        impl Maps for $name {
            const PROTOCOL: i32 = $protocol;
            const ARGUMENTTYPES: IdMap = crate::$table::ARGUMENTTYPES;
            const ATTRIBUTES: IdMap = crate::$table::ATTRIBUTES;
            const BLOCKENTITIES: IdMap = crate::$table::BLOCKENTITIES;
            const BLOCKS: IdMap = crate::$table::BLOCKS;
            const BLOCKSTATES: IdMap = crate::$table::BLOCKSTATES;
            const DATA_COMPONENT_TYPE: IdMap = crate::$table::DATA_COMPONENT_TYPE;
            const ENTITIES: IdMap = crate::$table::ENTITIES;
            const ITEMS: IdMap = crate::$table::ITEMS;
            const PARTICLES: IdMap = crate::$table::PARTICLES;
            const SOUNDS: IdMap = crate::$table::SOUNDS;
            const STATISTICS: IdMap = crate::$table::STATISTICS;
            const BRICK_OLD: i32 = crate::$table::BRICK_OLD;
            const MAP_COLOR_OLD: i32 = crate::$table::MAP_COLOR_OLD;
            const SWING_OLD: i32 = -1;
            const SADDLE_OLD: i32 = crate::$table::SADDLE_OLD;
            const SHIELD_BLOCK_OLD: i32 = crate::$table::SHIELD_BLOCK_OLD;
            const SHIELD_BREAK_OLD: i32 = crate::$table::SHIELD_BREAK_OLD;
            const BED_OLD: Option<i32> = Some(crate::$table::BED_OLD);
            const SLOT_DISPLAYS: IdMap = crate::$table::SLOT_DISPLAYS;
            const COMPONENT_IDENTIFIERS: &'static [&'static str] =
                crate::$table::COMPONENT_IDENTIFIERS;
            const ENTITY_IDENTIFIERS: &'static [&'static str] = crate::$table::ENTITY_IDENTIFIERS;
            const ENTITY_FAMILIES: &'static [u8] = crate::$table::ENTITY_FAMILIES;
            const ITEM_IDENTIFIERS: &'static [&'static str] = crate::$table::ITEM_IDENTIFIERS;
            const BLOCK_IDENTIFIERS: &'static [&'static str] = crate::$table::BLOCK_IDENTIFIERS;
            const RECIPE_IDENTIFIERS: &'static [&'static str] = crate::$table::RECIPE_IDENTIFIERS;
            const ATTRIBUTE_IDENTIFIERS: &'static [&'static str] =
                crate::$table::ATTRIBUTE_IDENTIFIERS;
            const SOUND_IDENTIFIERS: &'static [&'static str] = crate::$table::SOUND_IDENTIFIERS;
            const BLOCK_ENTITY_IDENTIFIERS: &'static [&'static str] =
                crate::$table::BLOCK_ENTITY_IDENTIFIERS;
        }
    };
}
older_maps!(V1217, 772, vfp_tables_1217);
older_maps!(V1216, 771, vfp_tables_1216);
older_maps!(V1215, 770, vfp_tables_1215);
older_maps!(V1214, 769, vfp_tables_1214);
older_maps!(V1212, 768, vfp_tables_1212);
older_maps!(V121, 767, vfp_tables_121);
older_maps!(V1205, 766, vfp_tables_1205);
older_maps!(V1203, 765, vfp_tables_1203);
older_maps!(V1202, 764, vfp_tables_1202);
older_maps!(V120, 763, vfp_tables_120);
older_maps!(V1194, 762, vfp_tables_1194);
older_maps!(V1193, 761, vfp_tables_1193);
