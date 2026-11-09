fn ddsstinkers_mob_killed(game: &mut crate::game::Game<'_>, entity: u32, kind: &str) {
    crate::game::tinkers_traits::mob_killed(game, entity, kind);
}
