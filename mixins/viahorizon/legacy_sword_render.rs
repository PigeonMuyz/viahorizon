// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(game: &crate::game::Game<'_>) {
    use horizon_client::first_person::Hand;
    use horizon_client::net::vfp_capture_session::{
        SWORD_BLOCK_RENDER, legacy_protocol, legacy_sword,
    };

    let blocking = game.used_hand() == Some(Hand::Main)
        && game.held().is_some_and(|stack| legacy_sword(&stack.item))
        && game
            .remote
            .as_ref()
            .is_some_and(|remote| matches!(legacy_protocol(&remote.session), Some(4 | 5 | 47)));
    SWORD_BLOCK_RENDER.set(blocking);
}
