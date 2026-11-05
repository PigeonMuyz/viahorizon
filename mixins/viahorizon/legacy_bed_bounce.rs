// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    world: &crate::motion::MotionWorld<'_, '_, '_>,
    state: horizon_world::registry::BlockStateId,
) -> ::core::ops::ControlFlow<Result<f32>> {
    if crate::net::vfp_capture_session::active_protocol().is_some_and(|p| p < 335)
        && world
            .assets
            .registry
            .state(state)
            .is_some_and(|s| s.name.ends_with("_bed"))
    {
        return ::core::ops::ControlFlow::Break(Ok(0.0));
    }
    ::core::ops::ControlFlow::Continue(())
}
