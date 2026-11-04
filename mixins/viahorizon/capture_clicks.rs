// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    game: &mut crate::game::Game<'_>,
    container: i32,
    clicks: &[crate::game::remote::Predicted],
) -> ::core::ops::ControlFlow<()> {
    if !game
        .remote
        .as_ref()
        .is_some_and(|remote| horizon_client::net::vfp_capture_session::full_items(&remote.session))
    {
        return ::core::ops::ControlFlow::Continue(());
    }
    let state = game.remote.as_ref().map_or(0, |remote| remote.state_id);
    for (slot, button, input, changed, carried) in clicks {
        let changed: Vec<_> = changed
            .iter()
            .map(|(slot, value)| (*slot, game.to_network(value)))
            .collect();
        let carried = game.to_network(carried);
        let data = horizon_protocol::vfp_full_click::encode(
            container,
            state,
            *slot,
            *button as i8,
            crate::game::remote::input_ordinal(*input),
            &changed,
            &carried,
        );
        game.send(horizon_protocol::packets::play::Serverbound::Unknown(
            horizon_protocol::vfp_full_click::INTERNAL_ID,
            data,
        ));
    }
    ::core::ops::ControlFlow::Break(())
}
