fn ddsstinkers_correct_for_drops(items: &crate::items::Items, held: Option<&crate::items::ItemStack>, block: &str) -> core::ops::ControlFlow<bool> {
    match crate::items::tinkers_items::correct_for_drops(items, held, block) {
        Some(ok) => core::ops::ControlFlow::Break(ok),
        None => core::ops::ControlFlow::Continue(()),
    }
}
