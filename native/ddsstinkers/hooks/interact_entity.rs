fn ddsstinkers_interact_entity(game: &mut crate::game::Game<'_>, entity: i32, kind: &str, stack: Option<&ItemStack>, claimed: &mut bool) {
    crate::game::tinkers_traits::use_on_mob(game, entity, kind, stack, claimed);
}
