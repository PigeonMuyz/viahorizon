// SPDX-License-Identifier: GPL-3.0-or-later
fn vfp_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    if !*ran && crate::game::viahorizon::command(game, command) {
        *ran = true;
    }
}
