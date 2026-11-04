// SPDX-License-Identifier: GPL-3.0-or-later
fn vfp_list(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    mods.push(crate::game::native_hooks::NativeMod {
        id: horizon_protocol::vfp_core::ID.into(),
        title: format!("{} {} (26.3)",horizon_protocol::vfp_core::NAME,horizon_protocol::vfp_core::VERSION),
        description: "Rust Release-only port in development. Horizon 26.3 client with gameplay translation down to Release 1.19.3; earlier Releases pending.".into(),
        keys: Vec::new(),
    });
}
