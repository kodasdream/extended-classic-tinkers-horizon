fn ddsstinkers_hurt_enemy(game: &mut crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, entity: u32, kind: &str) {
    crate::game::tinkers::hurt_enemy(game, stack, entity, kind);
}
