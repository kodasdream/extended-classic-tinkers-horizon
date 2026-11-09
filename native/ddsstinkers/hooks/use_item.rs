fn ddsstinkers_use_item(game: &mut crate::game::Game<'_>, stack: &horizon_client::items::ItemStack, claimed: &mut bool) {
    crate::game::tinkers::use_item(game, stack, claimed);
}
