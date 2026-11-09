fn ddsstinkers_attack_damage(game: &mut crate::game::Game<'_>) -> core::ops::ControlFlow<f32> {
    match crate::game::tinkers::attack_damage(&*game) {
        Some(damage) => core::ops::ControlFlow::Break(damage),
        None => core::ops::ControlFlow::Continue(()),
    }
}
