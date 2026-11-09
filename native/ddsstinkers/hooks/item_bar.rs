fn ddsstinkers_item_bar(game: &crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, bar: &mut Option<(i32, u32)>) {
    crate::game::tinkers::item_bar(stack, bar);
}
