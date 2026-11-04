// SPDX-License-Identifier: GPL-3.0-or-later
fn vfp_settings(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id == horizon_protocol::vfp_core::ID {
        crate::game::viahorizon::settings(game, buttons);
    }
}
