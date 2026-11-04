// SPDX-License-Identifier: GPL-3.0-or-later
fn vfp_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    if mod_id == horizon_protocol::vfp_core::ID {
        crate::game::viahorizon::click(game, button);
    }
}
