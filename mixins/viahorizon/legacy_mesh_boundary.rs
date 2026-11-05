// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(streaming: &mut crate::streaming::Streaming, world: &crate::world::World) {
    if !streaming.remote || !crate::vfp_render_compat::legacy() {
        return;
    }
    // Missing columns on an old server's outer ring are not pending local light jobs.
    // Mark only those mesh dependencies ready; region() leaves their data absent.
    let loaded: Vec<_> = world.chunks.keys().copied().collect();
    streaming.lit.retain(|&(x, z)| {
        world.chunks.contains_key(&(x, z))
            || (-1..=1).any(|dz| (-1..=1).any(|dx| world.chunks.contains_key(&(x + dx, z + dz))))
    });
    for (x, z) in loaded {
        for dz in -1..=1 {
            for dx in -1..=1 {
                let key = (x + dx, z + dz);
                if !world.chunks.contains_key(&key) {
                    streaming.lit.insert(key);
                }
            }
        }
    }
}
