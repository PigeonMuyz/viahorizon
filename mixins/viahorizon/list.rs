// SPDX-License-Identifier: GPL-3.0-or-later
fn vfp_list(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    mods.push(crate::game::native_hooks::NativeMod {
        id: horizon_protocol::vfp_core::ID.into(),
        title: format!("{} {} (26.3)",horizon_protocol::vfp_core::NAME,horizon_protocol::vfp_core::VERSION),
        description: "Choose the server version for your next connection.".into(),
        keys: Vec::new(),
    });
}
