fn ddsstinkers_block_entity_render_bounding_box(game: &crate::game::Game<'_>, pos: [i32; 3], block: &str, state: horizon_world::registry::BlockStateId, entity: &horizon_client::containers::ModBlockEntity, bounds: &mut [f32; 6]) {
    crate::game::tinkers::render_box(game, pos, block, state, entity, bounds);
}
