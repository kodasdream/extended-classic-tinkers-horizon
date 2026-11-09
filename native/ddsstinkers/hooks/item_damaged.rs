fn ddsstinkers_item_damaged(game: &mut crate::game::Game<'_>, player: u128, name: &str, item: &str, amount: &mut i32, cancel: &mut bool) {
    crate::game::tinkers::item_damaged(game, item, amount, cancel);
}
