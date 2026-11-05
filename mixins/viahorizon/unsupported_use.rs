// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    game: &mut crate::game::Game<'_>,
    hand: horizon_client::first_person::Hand,
) -> ::core::ops::ControlFlow<bool> {
    let Some(protocol) = game
        .remote
        .as_ref()
        .and_then(|r| horizon_client::net::vfp_capture_session::protocol(&r.session))
    else {
        return ::core::ops::ControlFlow::Continue(());
    };
    let held = match hand {
        horizon_client::first_person::Hand::Main => game.inventory[game.selected].clone(),
        _ => game.offhand.clone(),
    };
    if !crate::game::remote::vfp_item_support::supported(game, protocol, &held) {
        crate::game::remote::vfp_item_support::sanitize(game);
        return ::core::ops::ControlFlow::Break(false);
    }
    ::core::ops::ControlFlow::Continue(())
}
