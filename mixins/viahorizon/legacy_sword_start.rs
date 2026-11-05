// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    game: &mut crate::game::Game<'_>,
    hand: horizon_client::first_person::Hand,
) -> ::core::ops::ControlFlow<bool> {
    use horizon_protocol::packets::play::{Serverbound, UseItem};

    if hand != horizon_client::first_person::Hand::Main
        || !game.held().is_some_and(|stack| {
            horizon_client::net::vfp_capture_session::legacy_sword(&stack.item)
        })
        || !game.remote.as_ref().is_some_and(|remote| {
            matches!(
                horizon_client::net::vfp_capture_session::legacy_protocol(&remote.session),
                Some(4 | 5 | 47)
            )
        })
    {
        return ::core::ops::ControlFlow::Continue(());
    }
    let sequence = game.next_sequence();
    game.send(Serverbound::UseItem(UseItem {
        hand: 0,
        sequence,
        yaw: game.yaw,
        pitch: game.pitch,
    }));
    game.start_using_item();
    ::core::ops::ControlFlow::Break(true)
}
