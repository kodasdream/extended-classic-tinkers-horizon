fn ddsstinkers_enderman_teleport(m: &Mob) -> core::ops::ControlFlow<bool> {
    if crate::items::tinkers_items::teleport_blocked(m.id) {
        core::ops::ControlFlow::Break(false)
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
