fn ddsstinkers_inventory_tick(game: &mut crate::game::Game<'_>, slot: i32, selected: bool) {
    crate::game::tinkers::inventory_tick(game, slot, selected);
}
