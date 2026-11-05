// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(game: &mut crate::game::Game<'_>) {
    crate::game::remote::vfp_item_support::sanitize(game);
}
