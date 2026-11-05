// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    world: &crate::world::World,
    x: i32,
    z: i32,
) -> ::core::ops::ControlFlow<Option<Box<Region>>> {
    if crate::vfp_render_compat::legacy() {
        return ::core::ops::ControlFlow::Break(crate::vfp_render_compat::region(world, x, z));
    }
    ::core::ops::ControlFlow::Continue(())
}
