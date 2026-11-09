fn ddsstinkers_screen_click(game: &crate::game::Game<'_>, block: &str, data: &[i32], at: [f32; 2], right: bool, button: &mut Option<i32>) {
    crate::game::tinkers::screen_click(game, block, data, at, right, button);
}
