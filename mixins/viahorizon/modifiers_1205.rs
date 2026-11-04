// SPDX-License-Identifier: GPL-3.0-or-later
// UUID/name modifiers became resource identifiers in 1.21. Known vanilla
// identities retain their names, custom identities have deterministic UUIDs.
use crate::vfp_registry_1205 as data;
pub fn name(uuid: u128, old: Option<&str>) -> String {
    if let Some((_, name)) = data::MODIFIER_UUIDS.iter().find(|(id, _)| *id == uuid) {
        return (*name).to_owned();
    }
    if let Some(old) = old {
        if let Some((_, name)) = data::MODIFIER_NAMES.iter().find(|(name, _)| *name == old) {
            return (*name).to_owned();
        }
    }
    let value = format!("{uuid:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &value[..8],
        &value[8..12],
        &value[12..16],
        &value[16..20],
        &value[20..]
    )
}
pub fn identity(name: &str) -> (u128, String) {
    let local = name.strip_prefix("minecraft:").unwrap_or(name);
    // The upstream inverse maps keep the last UUID when historical aliases
    // share one identifier.
    let known = data::MODIFIER_UUIDS
        .iter()
        .rev()
        .find(|(_, value)| *value == local)
        .map(|(id, _)| *id);
    let parsed = name
        .rsplit_once(':')
        .map_or(name, |(_, path)| path)
        .replace('-', "");
    let uuid = known
        .or_else(|| {
            if parsed.len() == 32 {
                u128::from_str_radix(&parsed, 16).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            let mut bytes = crate::crypto::md5(name.as_bytes());
            bytes[6] = (bytes[6] & 15) | 48;
            bytes[8] = (bytes[8] & 63) | 128;
            u128::from_be_bytes(bytes)
        });
    let old = data::MODIFIER_NAMES
        .iter()
        .find(|(_, value)| *value == local)
        .map(|(name, _)| *name)
        .unwrap_or(name);
    (uuid, old.to_owned())
}
