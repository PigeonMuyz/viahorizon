// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{mesher::Region, world::World};
std::thread_local! {
    static LEGACY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub fn set_legacy(value: bool) {
    LEGACY.set(value);
}
pub fn legacy() -> bool {
    LEGACY.get()
}
pub fn region(world: &World, x: i32, z: i32) -> Option<Box<Region>> {
    world.chunks.get(&(x, z))?;
    Some(Box::new(std::array::from_fn(|dz| {
        std::array::from_fn(|dx| {
            world
                .chunks
                .get(&(x + dx as i32 - 1, z + dz as i32 - 1))
                .cloned()
        })
    })))
}
#[cfg(test)]
mod tests {
    #[test]
    fn legacy_mesh_region_keeps_loaded_center_without_inventing_neighbor_columns() {
        let mut world = crate::world::World::new(horizon_world::registry::BlockStateId(0), 0);
        world.chunks.insert(
            (0, 0),
            std::sync::Arc::new(crate::chunk::Chunk::empty(0, 0)),
        );
        let region = super::region(&world, 0, 0).unwrap();
        assert!(region[1][1].is_some());
        assert!(region[0][0].is_none());
        assert_eq!(world.chunks.len(), 1);
        assert!(super::region(&world, 1, 0).is_none());
    }
}
