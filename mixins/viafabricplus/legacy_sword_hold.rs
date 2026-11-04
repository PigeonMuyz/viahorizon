// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
) -> ::core::ops::ControlFlow<bool> {
    if horizon_client::net::vfp_capture_session::legacy_sword(&stack.item)
        && game.remote.as_ref().is_some_and(|remote| {
            matches!(
                horizon_client::net::vfp_capture_session::legacy_protocol(&remote.session),
                Some(4 | 5 | 47)
            )
        })
    {
        ::core::ops::ControlFlow::Break(true)
    } else {
        ::core::ops::ControlFlow::Continue(())
    }
}
