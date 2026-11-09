fn ddsstinkers_mining_speed(items: &crate::items::Items, held: Option<&crate::items::ItemStack>, block: &str) -> core::ops::ControlFlow<f32> {
    match crate::items::tinkers_items::mining_speed(items, held, block) {
        Some(speed) => core::ops::ControlFlow::Break(speed),
        None => core::ops::ControlFlow::Continue(()),
    }
}
