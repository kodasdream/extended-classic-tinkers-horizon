// A modular tool's numbers live on the stack (the `ddsstinkers:tool` component), but the game reads mining speed and harvest
// level per item. These two functions answer for such stacks, on the client and the server alike.
use super::{ItemStack, Items};
use horizon_world::nbt::Tag;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const TOOL: &str = "ddsstinkers:tool";

pub fn int(tag: &Tag, key: &str) -> Option<i32> {
    match tag.get(key)? {
        Tag::Int(v) => Some(*v),
        Tag::Short(v) => Some(i32::from(*v)),
        Tag::Byte(v) => Some(i32::from(*v)),
        _ => None,
    }
}

pub fn float(tag: &Tag, key: &str) -> Option<f32> {
    match tag.get(key)? {
        Tag::Float(v) => Some(f32::from_bits(*v)),
        _ => None,
    }
}

pub fn tool_tag(stack: &ItemStack) -> Option<&Tag> {
    stack.data()?.other(TOOL)
}

/// Which mining tags the tool works on: bit 0 pickaxe, 1 axe, 2 shovel, 3 hoe.
fn in_tags(items: &Items, mask: i32, block: &str) -> bool {
    (mask & 1 != 0 && items.block_in_tag("minecraft:mineable/pickaxe", block))
        || (mask & 2 != 0 && items.block_in_tag("minecraft:mineable/axe", block))
        || (mask & 4 != 0 && items.block_in_tag("minecraft:mineable/shovel", block))
        || (mask & 8 != 0 && items.block_in_tag("minecraft:mineable/hoe", block))
}

pub fn broken(stack: &ItemStack, tag: &Tag) -> bool {
    int(tag, "dur").is_some_and(|d| stack.damage as i32 >= d)
}

/// Speed traits that depend only on the stack: tool condition and the like.
fn trait_speed(stack: &ItemStack, tag: &Tag, base: f32) -> f32 {
    let mut speed = base;
    let dur = int(tag, "dur").unwrap_or(1).max(1) as f32;
    let left = (dur - stack.damage as f32).max(0.0) / dur;
    if let Some(Tag::List(traits)) = tag.get("traits") {
        for t in traits {
            let Tag::String(name) = t else { continue };
            match name.to_string_lossy().as_str() {
                // Stonebound: faster the more worn. Jagged and its kin trade the other way, on damage instead.
                "stonebound" => speed += (1.0 - left).powf(1.5) * base * 0.5,
                "lightweight" => speed *= 1.1,
                "heavy" | "superheavy" => speed *= 0.9,
                "fractured" | "fractured2" => speed *= 1.05,
                "dense" => speed *= 0.95 + 0.05 * left,
                "momentum" => speed *= 1.0 + momentum() * 0.25,
                "precipitate" => speed *= 1.0 + (1.0 - health_share()) * 2.0,
                "aquadynamic" if wet() => speed *= 1.6,
                "aridiculous" => speed *= 1.0 + warmth().max(0.0) * 0.3,
                _ => {}
            }
        }
    }
    speed
}

// Momentum: blocks broken in a row. The server and the window each keep their own.
static MOMENTUM: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub fn set_momentum(blocks: u32) {
    MOMENTUM.store(blocks.min(64), std::sync::atomic::Ordering::Relaxed);
}
fn momentum() -> f32 {
    MOMENTUM.load(std::sync::atomic::Ordering::Relaxed) as f32 / 32.0
}

// What speed traits need of the player, which this crate can't see: the game tells it every tick (`set_conditions`).
static HEALTH: AtomicU32 = AtomicU32::new(0x3F80_0000);
static WET: AtomicBool = AtomicBool::new(false);
static WARMTH: AtomicU32 = AtomicU32::new(0);
pub fn set_conditions(health_share: f32, wet: bool, warmth: f32) {
    HEALTH.store(health_share.to_bits(), Ordering::Relaxed);
    WET.store(wet, Ordering::Relaxed);
    WARMTH.store(warmth.to_bits(), Ordering::Relaxed);
}
fn health_share() -> f32 {
    f32::from_bits(HEALTH.load(Ordering::Relaxed)).clamp(0.0, 1.0)
}
fn wet() -> bool {
    WET.load(Ordering::Relaxed)
}
fn warmth() -> f32 {
    f32::from_bits(WARMTH.load(Ordering::Relaxed))
}

// Enderference: mobs that can't teleport yet, by id, with how many of their tries are left.
static NO_TELEPORT: Mutex<Vec<(u32, u32)>> = Mutex::new(Vec::new());
pub fn block_teleport(id: u32, tries: u32) {
    if let Ok(mut l) = NO_TELEPORT.lock() {
        l.retain(|(m, _)| *m != id);
        l.push((id, tries));
    }
}
pub fn teleport_blocked(id: u32) -> bool {
    let Ok(mut l) = NO_TELEPORT.lock() else { return false };
    let Some(i) = l.iter().position(|(m, _)| *m == id) else { return false };
    l[i].1 = l[i].1.saturating_sub(1);
    if l[i].1 == 0 {
        l.swap_remove(i);
    }
    true
}

/// The mining level a block needs, by Long Climb's tags.
fn needed_level(items: &Items, block: &str) -> Option<i32> {
    (0..=50).find(|n| items.block_in_tag(&format!("longclimb:needs_level_{n}"), block))
}

pub fn mining_speed(items: &Items, held: Option<&ItemStack>, block: &str) -> Option<f32> {
    let stack = held?;
    let tag = tool_tag(stack)?;
    if broken(stack, tag) {
        return Some(0.3);
    }
    let has = |name: &str| matches!(tag.get("traits"), Some(Tag::List(l)) if l.iter().any(|t| matches!(t, Tag::String(s) if s.to_string_lossy() == name)));
    if !in_tags(items, int(tag, "mine").unwrap_or(0), block) {
        // Crumbling: soft things that want no tool go quicker.
        return Some(if has("crumbling") { 2.5 } else { 1.0 });
    }
    let mut speed = trait_speed(stack, tag, float(tag, "speed").unwrap_or(1.0));
    if has("crumbling") && items.block_in_tag("minecraft:mineable/shovel", block) {
        speed *= 1.5;
    }
    // Unnatural: the further the tool's level is above what the block asks, the faster.
    if has("unnatural") {
        let above = int(tag, "lvl").unwrap_or(0) - needed_level(items, block).unwrap_or(0);
        speed *= 1.0 + above.clamp(0, 20) as f32 * 0.05;
    }
    Some(speed)
}

pub fn correct_for_drops(items: &Items, held: Option<&ItemStack>, block: &str) -> Option<bool> {
    let stack = held?;
    let tag = tool_tag(stack)?;
    if broken(stack, tag) {
        return Some(false);
    }
    if items.block_in_tag(&format!("longclimb:incorrect_for_level_{}", int(tag, "lvl").unwrap_or(0)), block) {
        return Some(false);
    }
    Some(in_tags(items, int(tag, "mine").unwrap_or(0), block))
}
