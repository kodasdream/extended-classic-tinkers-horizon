// What Tinkers' traits, modifiers and tool leveling do. Numbers follow each trait's class or description in the 1.12 jars;
// traits tied to other mods' systems (mana, Thaumcraft research, armor sets) act through the closest vanilla thing.
use super::Game;
use super::tinkers::{table, M};
use horizon_client::items::ItemStack;
use horizon_client::items::tinkers_items as ti;
use horizon_world::nbt::{JavaString, Tag};

fn names(tag: &Tag, key: &str) -> Vec<String> {
    match tag.get(key) {
        Some(Tag::List(l)) => l.iter().filter_map(|t| if let Tag::String(s) = t { Some(s.to_string_lossy()) } else { None }).collect(),
        _ => Vec::new(),
    }
}
pub fn traits_of(stack: &ItemStack) -> Vec<String> {
    ti::tool_tag(stack).map_or_else(Vec::new, |t| names(t, "traits"))
}
pub fn mod_level(stack: &ItemStack, id: &str) -> i32 {
    ti::tool_tag(stack).and_then(|t| t.get("mods")).and_then(|m| m.get(id)).map_or(0, |v| if let Tag::Int(i) = v { *i } else { 0 })
}
fn set_int(tag: &mut Tag, key: &str, v: i32) {
    if let Tag::Compound(c) = tag {
        c.insert(JavaString::from(key), Tag::Int(v));
    }
}
fn set_float(tag: &mut Tag, key: &str, v: f32) {
    if let Tag::Compound(c) = tag {
        c.insert(JavaString::from(key), Tag::Float(v.to_bits()));
    }
}
/// Change the held tool's component.
fn edit_tool(stack: &mut ItemStack, change: impl FnOnce(&mut Tag)) {
    let d = stack.data_mut();
    if let Some(mut tag) = d.other(ti::TOOL).cloned() {
        change(&mut tag);
        d.set_other(ti::TOOL, Some(tag));
    }
}
// The traits' own dice: they leave the world's random sequence to vanilla, so a trait never shifts what vanilla rolls next.
static DICE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn dice() -> u64 {
    use std::sync::atomic::Ordering;
    let mut x = DICE.load(Ordering::Relaxed);
    if x == 0 {
        x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0x9E37_79B9, |d| d.as_nanos() as u64) | 1;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    DICE.store(x, Ordering::Relaxed);
    x
}
fn roll(_game: &mut Game<'_>, one_in: u32) -> bool {
    dice() % u64::from(one_in.max(1)) == 0
}
fn heal_tool(stack: &mut ItemStack, amount: u32) {
    stack.damage = stack.damage.saturating_sub(amount);
}
fn is_undead(kind: &str) -> bool {
    matches!(kind.strip_prefix("minecraft:").unwrap_or(kind), "zombie" | "husk" | "drowned" | "skeleton" | "stray" | "bogged" | "wither_skeleton" | "wither" | "phantom" | "zoglin" | "zombie_villager" | "zombified_piglin" | "skeleton_horse" | "zombie_horse")
}
fn is_nether_mob(kind: &str) -> bool {
    matches!(kind.strip_prefix("minecraft:").unwrap_or(kind), "blaze" | "ghast" | "magma_cube" | "piglin" | "piglin_brute" | "hoglin" | "strider" | "wither_skeleton" | "zombified_piglin")
}
fn is_end_mob(kind: &str) -> bool {
    matches!(kind.strip_prefix("minecraft:").unwrap_or(kind), "enderman" | "endermite" | "shulker" | "ender_dragon")
}
fn is_arthropod(kind: &str) -> bool {
    matches!(kind.strip_prefix("minecraft:").unwrap_or(kind), "spider" | "cave_spider" | "silverfish" | "endermite" | "bee")
}
fn is_night(game: &Game<'_>) -> bool {
    let t = game.native_day_time().rem_euclid(24000);
    (13000..23000).contains(&t)
}

/// `ToolHelper.calcCutoffDamage`: above a weapon's cutoff, damage counts for less and less.
fn cutoff(damage: f32, limit: f32) -> f32 {
    if limit <= 0.0 || damage <= limit {
        return damage;
    }
    limit + (damage - limit).sqrt() * limit.sqrt().max(1.0) * 0.5
}

thread_local! {
    /// What the blow being worked out is aimed at: `melee_damage` names it just before it asks for the attack damage.
    static TARGET: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}
pub fn set_target(kind: &str) {
    TARGET.with_borrow_mut(|t| {
        t.clear();
        t.push_str(kind);
    });
}

/// `Player.attack`'s attack damage with a modular weapon in hand: the tool's numbers, its traits against what it is aimed at.
/// The game then applies the cooldown, critical hits and enchantments as it does for any weapon.
pub fn attack_value(game: &Game<'_>) -> Option<f32> {
    let weapon = game.held()?;
    let tag = ti::tool_tag(weapon)?;
    if ti::broken(weapon, tag) {
        return Some(1.0);
    }
    let t = table();
    let def = t.tools.get(&names_kind(tag))?;
    let kind = TARGET.with_borrow(|k| k.clone());
    let mut atk = ti::float(tag, "atk").unwrap_or(0.0);
    atk += mod_level(weapon, "sharpness") as f32 * 0.072;
    if mod_level(weapon, "smite") > 0 && is_undead(&kind) {
        atk += mod_level(weapon, "smite") as f32 * 0.14;
    }
    if mod_level(weapon, "bane_of_arthopods") > 0 && is_arthropod(&kind) {
        atk += mod_level(weapon, "bane_of_arthopods") as f32 * 0.14;
    }
    let dur = ti::int(tag, "dur").unwrap_or(1).max(1) as f32;
    let worn = weapon.damage as f32;
    // The nearest mob of that kind in reach stands for the target: the numbers below only need its health.
    let at = game.player.as_ref().map(|p| p.position);
    let hp = at.and_then(|c| {
        game.mobs.iter().filter(|m| *m.kind.id == *kind && !m.removed && m.position.distance(c) < 8.0).min_by(|a, b| a.position.distance(c).total_cmp(&b.position.distance(c))).map(|m| (m.health, m.attrs.max_health, m.fire_ticks > 0))
    });
    let (health, max, on_fire) = hp.unwrap_or((20.0, 20.0, false));
    let mine_hp = game.player.as_ref().map_or(20.0, |p| p.health.health());
    let night = is_night(game);
    let mut mul = 1.0f32;
    for tr in names(tag, "traits") {
        match tr.as_str() {
            "fractured" => atk += 1.5,
            "fractured2" => atk += 2.5,
            "crude1" => atk += 0.5,
            "crude2" => atk += 1.0,
            "jagged" => atk += (worn + 1.0).log10() * 1.5,
            "stonebound" => atk -= (worn / dur) * atk * 0.3,
            "coldblooded" if health >= max => mul *= 1.5,
            "holy" if is_undead(&kind) => atk += 5.0,
            "hellish" if !is_nether_mob(&kind) => atk += 2.0,
            "devilsstrength" if is_nether_mob(&kind) || is_end_mob(&kind) => atk += 3.0,
            "superheat" if on_fire => mul *= 1.35,
            "dark" if night => mul *= 1.4,
            "bright" if !night => mul *= 1.2,
            "blind" if night => mul *= 0.85,
            "heroic" if mine_hp <= 6.0 => mul *= 1.0 + (6.0 - mine_hp) / 4.0,
            "sassy" if kind == "minecraft:player" || kind == "minecraft:ender_dragon" || kind == "minecraft:wither" => mul *= 1.5,
            "bloodymary" => atk += (max - health).max(0.0) * 0.2,
            "rudeawakening" => atk += 2.0,
            "vindictive" if kind == "minecraft:player" => mul *= 1.4,
            "insatiable" => atk += tick_stack(tag, "insatiable"),
            "splintering" => atk += tick_stack(tag, "splintering") * 0.5,
            "splintering2" => atk += tick_stack(tag, "splintering") * 0.8,
            "congenial" => atk += ti::int(tag, "kills").unwrap_or(0).min(100) as f32 * 0.03,
            "souleater" => atk += ti::int(tag, "kills").unwrap_or(0).min(500) as f32 * 0.02,
            "squeaky" => return Some(0.0),
            "lightweight" => atk *= 1.1,
            "aridiculous" => mul *= 1.0 + heat(game).max(0.0) * 0.25,
            "defiled" => atk += 2.0, // every mob here is "not from the Defiled Lands"
            "twilit" => atk += 2.0,  // there is no Twilight Forest here, so always the "outside" half
            "in_the_garage" if !in_sunlight(game) => atk += 5.0,
            "surf_wax_america" if riding(game) => atk += 5.0,
            "stopbeingselfish" => atk += closeness(game),
            "shocking" if ti::int(tag, "s_charge").unwrap_or(0) >= 100 => atk += 3.0,
            _ => {}
        }
    }
    // A katana builds a combo: each hit in a row adds to the next, as PlusTiC's does.
    if names_kind(tag) == "katana" {
        atk += ti::int(tag, "combo").unwrap_or(0) as f32 * 0.5;
    }
    Some(cutoff(atk.max(0.0) * def.damage * mul + 1.0, def.cutoff))
}

fn names_kind(tag: &Tag) -> String {
    match tag.get("kind") {
        Some(Tag::String(s)) => s.to_string_lossy(),
        _ => String::new(),
    }
}
fn tick_stack(tag: &Tag, key: &str) -> f32 {
    ti::int(tag, &format!("s_{key}")).unwrap_or(0) as f32
}

/// After the blow lands: effects on the victim and the wielder, stacks, XP.
pub fn after_hit(game: &mut Game<'_>, stack: &ItemStack, entity: u32, kind: &str) {
    if std::env::var_os("DDSSTINKERS_DEBUG").is_some() {
        eprintln!("[tinkers] after_hit {} on {kind} traits={:?}", stack.item, traits_of(stack));
    }
    if ti::tool_tag(stack).is_none() {
        return;
    }
    let traits = traits_of(stack);
    let fiery = mod_level(stack, "fiery");
    for tr in &traits {
        let (mob_effect, ticks, amp): (Option<&'static str>, i32, i32) = match tr.as_str() {
            "poisonous" | "hive_defender" => (Some("minecraft:poison"), 100, 0),
            "withering" | "xu_withering" | "apocalypse" => (Some("minecraft:wither"), 100, 0),
            "freezing" | "frost2" => (Some("minecraft:slowness"), 60, 1),
            "heavy_metal" => (Some("minecraft:slowness"), 40, 0),
            "hollow" => (Some("minecraft:weakness"), 100, 0),
            "sharp" => (Some("minecraft:instant_damage"), 1, 0),
            "blind" => (Some("minecraft:blindness"), 40, 0),
            _ => (None, 0, 0),
        };
        if let Some(effect) = mob_effect
            && let Some(m) = game.mobs.iter_mut().find(|m| m.id == entity)
        {
            m.add_effect(effect, ticks, amp);
            m.needs_sync = true;
        }
        match tr.as_str() {
            "flame2" | "garishly" => {
                if let Some(m) = game.mobs.iter_mut().find(|m| m.id == entity) {
                    m.fire_ticks = m.fire_ticks.max(100);
                }
            }
            "natureswrath" if roll(game, 3) => {
                if let Some(m) = game.mobs.iter_mut().find(|m| m.id == entity) {
                    m.fire_ticks = m.fire_ticks.max(100);
                }
                game.give_player_effect("minecraft:instant_health", 1, 0);
            }
            "heavy_metal" => {
                let dim = game.native_dimension().to_owned();
                let dir = game.player.as_ref().map_or([0.0, 0.0], |p| [-(p.yaw.to_radians().sin()) as f64, p.yaw.to_radians().cos() as f64]);
                game.native_push_mob(&dim, entity, [dir[0] * 0.6, 0.3, dir[1] * 0.6]);
            }
            "chadthunder" if roll(game, 6) => {
                let at = game.mobs.iter().find(|m| m.id == entity).map(|m| [m.position.x, m.position.y, m.position.z]);
                if let Some(at) = at {
                    let dim = game.native_dimension().to_owned();
                    game.native_summon(&dim, "minecraft:lightning_bolt", at, Default::default());
                }
            }
            "explosive" | "unstable" => {
                let at = game.mobs.iter().find(|m| m.id == entity).map(|m| [m.position.x, m.position.y, m.position.z]);
                if let Some(at) = at
                    && (tr == "explosive" || roll(game, 20))
                {
                    queue(format!("execute positioned {} {} {} run summon minecraft:tnt ~ ~ ~ {{fuse:0}}", at[0], at[1], at[2]));
                }
            }
            "hailhydra" if roll(game, 10) => {
                let at = game.mobs.iter().find(|m| m.id == entity).map(|m| [m.position.x, m.position.y, m.position.z]);
                if let Some(at) = at {
                    queue(format!("execute positioned {} {} {} run summon minecraft:tnt ~ ~ ~ {{fuse:0}}", at[0], at[1], at[2]));
                }
            }
            "thermalinversion" => {
                let warm = heat(game);
                if let Some(m) = game.mobs.iter_mut().find(|m| m.id == entity) {
                    if warm > 0.4 {
                        m.add_effect("minecraft:slowness", 60, 1);
                        m.needs_sync = true;
                    } else if warm < -0.4 {
                        m.fire_ticks = m.fire_ticks.max(60);
                    }
                }
            }
            "elemental" if roll(game, 8) => {
                const GIFTS: [&str; 5] = ["minecraft:regeneration", "minecraft:speed", "minecraft:strength", "minecraft:resistance", "minecraft:haste"];
                let i = pick(game, GIFTS.len());
                game.give_player_effect(GIFTS[i], 100, 0);
            }
            "prickly" => {
                hurt_mob(game, entity, 2.0);
                player_pain("minecraft:cactus", 1.0);
            }
            "uplifting" if roll(game, 16) => game.give_player_effect("minecraft:regeneration", 60, 0),
            "curvature" if roll(game, 30) => {
                let at = game.mobs.iter().find(|m| m.id == entity).map(|m| [m.position.x, m.position.y, m.position.z]);
                if let Some(at) = at {
                    let (dx, dz) = (pick(game, 13) as f64 - 6.0, pick(game, 13) as f64 - 6.0);
                    let dim = game.native_dimension().to_owned();
                    game.native_teleport_mob(&dim, entity, [at[0] + dx, at[1] + 1.0, at[2] + dz]);
                }
            }
            "cursed" => {
                let kills = ti::tool_tag(stack).and_then(|t| ti::int(t, "kills")).unwrap_or(0);
                if roll(game, (200 - kills).max(4) as u32) {
                    player_pain("minecraft:magic", 1.0 + pick(game, 3) as f32);
                }
            }
            "catcher" if roll(game, 12) && !is_boss(kind) => {
                let dim = game.native_dimension().to_owned();
                let gone = game.native_remove_mob(&dim, entity);
                if std::env::var_os("DDSSTINKERS_DEBUG").is_some() {
                    eprintln!("[tinkers] catcher {kind} removed={gone}");
                }
                if gone {
                    caught(game, kind);
                }
            }
            "blindbandit" if roll(game, 20) => bandit(game),
            "enderference" if kind == "minecraft:enderman" => ti::block_teleport(entity, 300),
            "sweater_song" if critical_now(game) && roll(game, 3) => strip_armor(game, entity),
            "shocking" => discharge_hit(game, entity),
            "resonance" if roll(game, 4) => {
                let dim = game.native_dimension().to_owned();
                game.native_push_mob(&dim, entity, [0.0, 1.5, 0.0]);
            }
            "vindictive" | "natureheal" | "lifesteal" => {
                game.give_player_effect("minecraft:instant_health", 1, 0);
            }
            "baconlicious" if roll(game, 12) => drop_at(game, entity, "ddsstinkers:edible_bacon"),
            "naturesblessing" if roll(game, 10) => drop_at(game, entity, "minecraft:bread"),
            _ => {}
        }
    }
    if fiery > 0
        && let Some(m) = game.mobs.iter_mut().find(|m| m.id == entity)
    {
        m.fire_ticks = m.fire_ticks.max(fiery * 20);
    }
    let tag = ti::tool_tag(stack);
    let tool = tag.map_or_else(String::new, names_kind);
    let base = table().tools.get(&tool).map_or(1.0, |d| d.knockback);
    let knock = mod_level(stack, "knockback") as f32 / 10.0 + (base - 1.0).max(0.0);
    if knock > 0.0 {
        let dim = game.native_dimension().to_owned();
        let dir = game.player.as_ref().map_or([0.0, 0.0], |p| [-(p.yaw.to_radians().sin()) as f64, p.yaw.to_radians().cos() as f64]);
        game.native_push_mob(&dim, entity, [dir[0] * 0.4 * f64::from(knock), 0.2, dir[1] * 0.4 * f64::from(knock)]);
    }
    // A scythe's and a cleaver's blow sweeps what stands around the target.
    if matches!(tool.as_str(), "scythe" | "cleaver" | "broadsword") {
        let centre = game.mobs.iter().find(|m| m.id == entity).map(|m| m.position);
        if let Some(c) = centre {
            let atk = tag.and_then(|t| ti::float(t, "atk")).unwrap_or(1.0) * 0.5;
            for m in game.mobs.iter_mut().filter(|m| m.id != entity && !m.removed && m.alive()) {
                if m.position.distance(c) < 2.5 {
                    m.health = (m.health - atk).max(0.5);
                    m.needs_sync = true;
                }
            }
        }
    }
    // A cleaver takes heads: a skull now and then, more with Beheading.
    let beheading = mod_level(stack, "beheading") + i32::from(tool == "cleaver");
    if beheading > 0 && roll(game, (12 - beheading * 2).max(2) as u32) {
        let head = match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "zombie" => Some("minecraft:zombie_head"),
            "skeleton" => Some("minecraft:skeleton_skull"),
            "creeper" => Some("minecraft:creeper_head"),
            "wither_skeleton" => Some("minecraft:wither_skeleton_skull"),
            "piglin" => Some("minecraft:piglin_head"),
            _ => None,
        };
        if let Some(h) = head {
            drop_at(game, entity, h);
        }
    }
    let selected = game.selected;
    let fully = kind != "minecraft:armor_stand";
    if let Some(s) = game.inventory[selected].as_mut() {
        edit_tool(s, |t| {
            // Stacks that build up in combat, and Tool Leveling's XP: a point per hit.
            for tr in ["insatiable", "splintering", "combo"] {
                let key = if tr == "combo" { "combo".to_owned() } else { format!("s_{tr}") };
                let v = ti::int(t, &key).unwrap_or(0);
                set_int(t, &key, (v + 1).min(10));
            }
            if fully {
                add_xp(t, 1 + ti::int(t, "xp_bonus").unwrap_or(0));
            }
        });
    }
}

fn drop_at(game: &mut Game<'_>, entity: u32, item: &str) {
    let at = game.mobs.iter().find(|m| m.id == entity).map(|m| [m.position.x, m.position.y + 0.5, m.position.z]);
    if let Some(at) = at {
        game.spawn_item(ItemStack { item: std::sync::Arc::from(item), count: 1, damage: 0, extra: None }, at, [0.0, 0.1, 0.0], 10);
    }
}

// Commands wait for the next server tick, where no player's state is swapped in.
static QUEUE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
fn queue(line: String) {
    if let Ok(mut q) = QUEUE.lock() {
        q.push(line);
    }
}
pub fn queue_pub(line: String) {
    queue(line);
}
pub fn sound(sound: &str, pos: [i32; 3], volume: f32, pitch: f32) {
    queue(format!("playsound {sound} block @a {} {} {} {volume} {pitch}", pos[0], pos[1], pos[2]));
}
pub fn flush(game: &mut Game<'_>) {
    let lines = QUEUE.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default();
    for l in lines {
        game.native_run_command(&l);
    }
}

// --- Tool Leveling -------------------------------------------------------------------------------------------------------

/// `TinkerToolLeveling`: 500 XP to the second level (4500 for the big tools), each next level 2.0 times the last,
/// and every level gives one more modifier slot.
fn xp_for_level(tag: &Tag, level: i32) -> i32 {
    let big = table().tools.get(&names_kind(tag)).is_some_and(|d| d.big && !d.mining.is_empty() || names_kind(tag) == "scythe");
    let base: f32 = if big { 4500.0 } else { 500.0 };
    (base * 2f32.powi(level - 1)).min(i32::MAX as f32 / 2.0) as i32
}
pub fn add_xp(t: &mut Tag, amount: i32) {
    let mut xp = ti::int(t, "xp").unwrap_or(0) + amount;
    let mut level = ti::int(t, "tl").unwrap_or(1);
    let mut free = ti::int(t, "free").unwrap_or(0);
    while xp >= xp_for_level(t, level) {
        xp -= xp_for_level(t, level);
        level += 1;
        free += 1;
    }
    set_int(t, "xp", xp);
    set_int(t, "tl", level);
    set_int(t, "free", free);
}

// --- Mining --------------------------------------------------------------------------------------------------------------

static AREA_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CHAIN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Blocks an area tool breaks along with the one it was aimed at.
fn area(game: &Game<'_>, kind: &str, pos: [i32; 3], name: &str, width_mod: i32, height_mod: i32) -> Vec<[i32; 3]> {
    let items = game.res.items();
    let (yaw, pitch) = game.player.as_ref().map_or((0.0, 0.0), |p| (p.yaw, p.pitch));
    let radius = match kind {
        "hammer" | "excavator" => 1,
        "scythe" => 1,
        _ => 0,
    } + width_mod.max(height_mod);
    if radius == 0 {
        return Vec::new();
    }
    let _ = (name, items);
    let mut out = Vec::new();
    // The plane across the player's view: flat when looking down or up, else upright across their facing.
    let vertical = !(-45.0..=45.0).contains(&pitch);
    let along_x = {
        let a = yaw.rem_euclid(360.0);
        !((45.0..135.0).contains(&a) || (225.0..315.0).contains(&a))
    };
    for a in -radius..=radius {
        for b in -radius..=radius {
            if a == 0 && b == 0 {
                continue;
            }
            let p = if vertical {
                [pos[0] + a, pos[1], pos[2] + b]
            } else if along_x {
                [pos[0] + a, pos[1] + b, pos[2]]
            } else {
                [pos[0], pos[1] + b, pos[2] + a]
            };
            out.push(p);
        }
    }
    out
}

/// A tree above the block, for the Lumber Axe.
fn tree(game: &Game<'_>, pos: [i32; 3]) -> Vec<[i32; 3]> {
    let items = game.res.items();
    let mut found = Vec::new();
    let mut open = vec![pos];
    let mut seen = std::collections::HashSet::new();
    seen.insert(pos);
    while let Some(p) = open.pop() {
        if found.len() >= 128 {
            break;
        }
        for dx in -1..=1 {
            for dy in 0..=1 {
                for dz in -1..=1 {
                    let q = [p[0] + dx, p[1] + dy, p[2] + dz];
                    if seen.insert(q) && items.block_in_tag("minecraft:logs", &game.native_block(q)) {
                        found.push(q);
                        open.push(q);
                    }
                }
            }
        }
    }
    found
}

static PENDING: std::sync::Mutex<Vec<[i32; 3]>> = std::sync::Mutex::new(Vec::new());

/// Each tick of a player: the area tool's other blocks, broken now that the first has gone.
pub fn player_tick(game: &mut Game<'_>) {
    boots_tick(game);
    held_tick(game);
    let list = PENDING.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default();
    if list.is_empty() {
        return;
    }
    AREA_BUSY.store(true, std::sync::atomic::Ordering::Relaxed);
    for p in list {
        // The tool may have worn out partway: the rest would otherwise go by bare hand.
        if game.inventory[game.selected].is_none() {
            break;
        }
        let _ = game.break_block(p);
    }
    AREA_BUSY.store(false, std::sync::atomic::Ordering::Relaxed);
}

pub fn on_break(game: &mut Game<'_>, pos: [i32; 3], _canceled: &mut bool) {
    if !super::tinkers::is_server(game) || AREA_BUSY.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let selected = game.selected;
    let Some(stack) = game.inventory[selected].clone() else { return };
    let Some(tag) = ti::tool_tag(&stack) else { return };
    if ti::broken(&stack, tag) {
        return;
    }
    let kind = names_kind(tag);
    let block = game.native_block(pos);
    let items = game.res.items();
    let traits = names(tag, "traits");
    let mut extra: Vec<[i32; 3]> = match kind.as_str() {
        "lumberaxe" if items.block_in_tag("minecraft:logs", &block) => tree(game, pos),
        "scythe" if items.block_in_tag("minecraft:crops", &block) || items.block_in_tag("minecraft:replaceable_by_trees", &block) => {
            let mut v = Vec::new();
            for dx in -3i32..=3 {
                for dz in -3i32..=3 {
                    if dx != 0 || dz != 0 {
                        v.push([pos[0] + dx, pos[1], pos[2] + dz]);
                    }
                }
            }
            v
        }
        _ => area(game, &kind, pos, &block, mod_level(&stack, "harvestwidth"), mod_level(&stack, "harvestheight")),
    };
    // Fracture digs a line, Cascade a wandering path.
    let forward = game.player.as_ref().map_or([0, 0, 1], |p| {
        let a = p.yaw.rem_euclid(360.0);
        if !(45.0..315.0).contains(&a) { [0, 0, 1] } else if a < 135.0 { [-1, 0, 0] } else if a < 225.0 { [0, 0, -1] } else { [1, 0, 0] }
    });
    if traits.iter().any(|t| t == "fracture") {
        extra.extend((1..=4).map(|i| [pos[0] + forward[0] * i, pos[1], pos[2] + forward[2] * i]));
    }
    let mut stack_changes = 0u32;
    let mut melt: Vec<[i32; 3]> = Vec::new();
    let mut charge_change: Option<i32> = None;
    // Traits on the block itself.
    let stone = items.block_in_tag("minecraft:base_stone_overworld", &block) || block == "minecraft:cobblestone";
    let dirt = items.block_in_tag("minecraft:dirt", &block);
    let plant = items.block_in_tag("minecraft:logs", &block) || items.block_in_tag("minecraft:leaves", &block);
    let mut heal = 0u32;
    for tr in &traits {
        match tr.as_str() {
            "petramor" if stone => heal += 1,
            "softy" if dirt => heal += 1,
            "naturebound" if dirt || plant => heal += 1,
            "superheavy" => game.give_player_effect("minecraft:mining_fatigue", 60, 0),
            "fragile" => stack_changes += 1,
            "slimey_green" | "slimey_blue" | "slimey_pink" if roll(game, 1000) => {
                let dim = game.native_dimension().to_owned();
                game.native_spawn_mob(&dim, "minecraft:slime", [f64::from(pos[0]) + 0.5, f64::from(pos[1]), f64::from(pos[2]) + 0.5], &Default::default());
            }
            "uplifting" if roll(game, 16) => game.give_player_effect("minecraft:regeneration", 60, 0),
            "shocking" => {
                let charged = ti::int(tag, "s_charge").unwrap_or(0) >= 100;
                if charged {
                    game.give_player_effect("minecraft:haste", 100, 1);
                }
                charge_change = Some(if charged { -100 } else { 2 });
            }
            "curvature" if roll(game, 200) => {
                let (dx, dz) = (pick(game, 9) as i32 - 4, pick(game, 9) as i32 - 4);
                queue(format!("tp @p ~{dx} ~ ~{dz}"));
            }
            "dissolving" if roll(game, 40) => {
                queue(format!("summon minecraft:experience_orb {} {} {} {{Value:{}}}", pos[0], pos[1], pos[2], 20 + pick(game, 40)));
            }
            "melting" if stone && roll(game, 20) => melt.push(pos),
            "elemental" if falls(&block) => {
                for dir in [-1i32, 1] {
                    for k in 1..=16 {
                        let q = [pos[0], pos[1] + dir * k, pos[2]];
                        if game.native_block(q) != block {
                            break;
                        }
                        extra.push(q);
                    }
                }
            }
            "cascade" => {
                // A wandering earthquake: each step leaves from the last one.
                let mut q = pos;
                for _ in 0..(4 + pick(game, 8)) {
                    let (axis, way) = (pick(game, 3), if roll(game, 2) { 1 } else { -1 });
                    q[axis] += way;
                    extra.push(q);
                }
            }
            "momentum" => {
                let c = CHAIN.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                ti::set_momentum(c);
            }
            _ => {}
        }
    }
    // The extra blocks break as the player's own hits, once this one is done: same drops, same wear.
    for p in extra {
        let b = game.native_block(p);
        if b == "minecraft:air" || b == "minecraft:bedrock" || !items_ok(game, &stack, &b, &kind) {
            continue;
        }
        if matches!(kind.as_str(), "hammer" | "excavator") && !not_much_harder(game, pos, p) {
            continue;
        }
        if let Ok(mut q) = PENDING.lock() {
            q.push(p);
        }
    }
    if let Some(s) = game.inventory[selected].as_mut() {
        if heal > 0 {
            heal_tool(s, heal);
        }
        s.damage += stack_changes;
        edit_tool(s, |t| {
            add_xp(t, 1 + ti::int(t, "xp_bonus").unwrap_or(0));
            if let Some(by) = charge_change {
                let v = ti::int(t, "s_charge").unwrap_or(0);
                set_int(t, "s_charge", (v + by).clamp(0, 100));
            }
        });
    }
    if let Ok(mut q) = LAVA.lock() {
        q.extend(melt);
    }
}

/// Tinkers' rule for extra blocks: one more than ten times harder than the block being mined stays put.
fn not_much_harder(game: &Game<'_>, center: [i32; 3], pos: [i32; 3]) -> bool {
    let hardness = |p: [i32; 3]| game.res.facts().get(game.world.block(p[0], p[1], p[2])).hardness;
    let h = hardness(pos);
    h >= 0.0 && h <= hardness(center) * 10.0
}

fn items_ok(game: &Game<'_>, stack: &ItemStack, block: &str, kind: &str) -> bool {
    let items = game.res.items();
    if kind == "lumberaxe" || kind == "scythe" {
        return true;
    }
    items.mining_speed(Some(stack), block) > 1.0 && items.correct_for_drops(Some(stack), block)
}

// --- Every tick ----------------------------------------------------------------------------------------------------------

/// A carried tool's slow effects. Held only, unless the trait says otherwise.
pub fn tool_tick(game: &mut Game<'_>, slot: i32, selected: bool) {
    let age = game.native_day_time();
    if age % 20 != 0 {
        return;
    }
    let night = is_night(game);
    let (food, low_health) = game.player.as_ref().map_or((20, false), |p| (p.health.food.food, p.health.health() <= 4.0));
    let hotbar_steeleaf = game.inventory[..9].iter().flatten().any(|s| &*s.item == "ddsstinkers:steeleaf_ingot");
    let Some(stack) = game.native_carried_stack(slot) else { return };
    if ti::tool_tag(stack).is_none() {
        return;
    }
    let traits = traits_of(stack);
    let mut acts: Vec<Act> = Vec::new();
    let moss = mod_level(stack, "mending_moss");
    if moss > 0 {
        heal_tool(stack, moss as u32);
    }
    let mut effect: Vec<(&'static str, i32, i32)> = Vec::new();
    let mut decay = 0;
    for tr in &traits {
        match tr.as_str() {
            "ecological" => {
                if age % 400 == 0 {
                    heal_tool(stack, 1);
                }
            }
            "arcane" if night => heal_tool(stack, 1),
            "decay" => decay = 1,
            "anticorrosion" if selected && age % 100 == 0 => acts.push(Act::Cure),
            "alien" if age % 1200 == 0 => acts.push(Act::Drift(slot)),
            "synergy" if age % 100 == 0 && hotbar_steeleaf => heal_tool(stack, 1),
            "living1" if age % 400 == 0 && stack.damage > 0 => {
                heal_tool(stack, 1);
                acts.push(Act::Tire(0.5));
            }
            "living2" if age % 200 == 0 && stack.damage > 0 => {
                heal_tool(stack, 1);
                acts.push(Act::Tire(0.5));
            }
            "mana" => {
                edit_tool(stack, |t| {
                    let v = ti::int(t, "s_mana").unwrap_or(0);
                    set_int(t, "s_mana", (v + 1).min(200));
                });
                if age % 600 == 0 {
                    heal_tool(stack, 1);
                }
            }
            "tasty" if selected && food <= 6 && age % 60 == 0 => {
                stack.damage += 10;
                acts.push(Act::Feed(2, 0.2));
            }
            "glowing" if selected && age % 40 == 0 => acts.push(Act::Torch),
            "mirabile" if selected && age % 240 == 0 => acts.push(Act::Vein),
            "darktraveler" if selected && age % 100 == 0 => acts.push(Act::Darkness),
            "im_a_superstar" if selected && age % 100 == 0 => acts.push(Act::Attuned),
            "xu_whispering" if selected && age % 6000 == 0 => acts.push(Act::Whisper),
            "fruitsalad" if selected && food <= 14 && age % 100 == 0 => {
                let mut fed = None;
                edit_tool(stack, |t| fed = eat_fruit(t));
                if let Some((n, sat)) = fed {
                    acts.push(Act::Feed(n, sat));
                }
            }
            "nickoftime" if selected && low_health => {
                let mut to = None;
                edit_tool(stack, |t| to = portal_of(t));
                if let Some(to) = to {
                    acts.push(Act::Rescue(to));
                }
            }
            "glimmer" if selected && night && age % 200 == 0 => effect.push(("minecraft:night_vision", 300, 0)),
            "illuminati" if selected => {
                effect.push(("minecraft:invisibility", 40, 0));
                acts.push(Act::Glow);
            }
            "berserk" if selected => {
                effect.push(("minecraft:speed", 40, 2));
                effect.push(("minecraft:strength", 40, 2));
            }
            _ => {}
        }
    }
    stack.damage += decay;
    let magnet = if !selected { 0.0 } else if traits.iter().any(|t| t == "magnetic2") { 6.0 } else if traits.iter().any(|t| t == "magnetic1") { 3.0 } else { 0.0 };
    for (e, t, a) in effect {
        game.give_player_effect(e, t, a);
    }
    for act in acts {
        run_act(game, act);
    }
    if magnet > 0.0
        && let Some(p) = game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z])
    {
        let dim = game.native_dimension();
        let found = game.native_item_entities(dim, [p[0] - magnet, p[1] - magnet, p[2] - magnet], [p[0] + magnet, p[1] + magnet, p[2] + magnet]);
        if let Some(player) = game.native_acting_player() {
            for e in found {
                let left = game.native_give_stack(player.uuid, e.stack.clone());
                let rest = left.unwrap_or_else(|| ItemStack { count: 0, ..e.stack.clone() });
                game.native_set_item_entity(dim, e.id, rest);
            }
        }
    }
    // Stacks fade when out of combat, and a chain of blocks ends.
    CHAIN.store(0, std::sync::atomic::Ordering::Relaxed);
    ti::set_momentum(0);
}

/// Wear: some traits save durability, some spend it faster.
pub fn wear(game: &mut Game<'_>, amount: &mut i32) {
    let selected = game.selected;
    let Some(stack) = game.inventory[selected].clone() else { return };
    let Some(tag) = ti::tool_tag(&stack) else { return };
    let dur = ti::int(tag, "dur").unwrap_or(1).max(1) as f32;
    let left = (dur - stack.damage as f32) / dur;
    for tr in names(tag, "traits") {
        match tr.as_str() {
            "dense" if roll(game, ((1.0 - left).max(0.05) * 100.0) as u32 / 10 + 1) => {}
            "dense" => *amount = 0,
            "duritos" if roll(game, 10) => *amount *= 2,
            "duritos" if roll(game, 2) => *amount = 0,
            "stiff" | "stalwart" if roll(game, 4) => *amount = 0,
            "brittle" if roll(game, 500) => *amount += dur as i32,
            "mana" if ti::int(tag, "s_mana").unwrap_or(0) >= 10 => {
                *amount = 0;
                if let Some(s) = game.inventory[selected].as_mut() {
                    edit_tool(s, |t| {
                        let v = ti::int(t, "s_mana").unwrap_or(0);
                        set_int(t, "s_mana", v - 10);
                    });
                }
            }
            "insatiable" => *amount += ti::int(tag, "s_insatiable").unwrap_or(0) / 3,
            _ => {}
        }
    }
    let reinforced = mod_level(&stack, "reinforced");
    if reinforced > 0 && roll(game, (6 - reinforced).max(1) as u32) {
        *amount = 0;
    }
}

// --- Modifiers -----------------------------------------------------------------------------------------------------------

/// What an item does on a Tool Station: (modifier id, most levels, how many items a level takes).
fn modifier_item(item: &str) -> Option<(&'static str, i32, i32)> {
    Some(match item {
        "minecraft:diamond" => ("diamond", 1, 1),
        "minecraft:emerald" => ("emerald", 1, 1),
        "minecraft:redstone" | "minecraft:redstone_block" => ("haste", 5, 50),
        "minecraft:quartz" | "minecraft:quartz_block" => ("sharpness", 5, 72),
        "minecraft:lapis_lazuli" | "minecraft:lapis_block" => ("luck", 3, 90),
        "minecraft:obsidian" => ("reinforced", 5, 1),
        "minecraft:piston" | "minecraft:sticky_piston" => ("knockback", 3, 10),
        "minecraft:blaze_powder" => ("fiery", 5, 25),
        "minecraft:spider_eye" | "minecraft:fermented_spider_eye" => ("bane_of_arthopods", 5, 24),
        "minecraft:ghast_tear" => ("necrotic", 3, 1),
        "minecraft:bone" => ("smite", 5, 24),
        "minecraft:wither_skeleton_skull" => ("beheading", 3, 1),
        "minecraft:moss_block" => ("mending_moss", 2, 1),
        "minecraft:nether_star" => ("soulbound", 1, 1),
        "minecraft:string" | "minecraft:cobweb" => ("webbed", 5, 16),
        "minecraft:tnt" => ("blasting", 1, 1),
        "minecraft:shulker_shell" => ("shulking", 5, 8),
        "ddsstinkers:silky_jewel" => ("silktouch", 1, 1),
        "ddsstinkers:hammer_head" | "ddsstinkers:large_plate" => ("harvestwidth", 2, 1),
        "ddsstinkers:tough_binding" => ("harvestheight", 2, 1),
        _ => return None,
    })
}

/// Tool Station with a tool in its slot: repair from the head's material, then modifiers. True when anything changed.
pub fn apply(tool: &mut ItemStack, slots: &mut [Option<ItemStack>]) -> bool {
    let t = table();
    let Some(tag) = ti::tool_tag(tool).cloned() else { return false };
    let mats = names(&tag, "mats");
    let def = t.tools.get(&names_kind(&tag));
    let head_mat = def.and_then(|d| d.parts.iter().zip(&mats).find(|(p, _)| p.role == "head" || p.role == "bow").map(|(_, m)| m.clone()));
    let mut changed = false;
    for slot in slots.iter_mut() {
        let Some(s) = slot.as_ref() else { continue };
        // Repair: each ingot of the head's material gives back a share of a head's durability.
        if let Some(mid) = &head_mat
            && let Some(m) = t.materials.get(mid)
            && m.item.as_deref() == Some(&*s.item)
            && tool.damage > 0
        {
            let per = m.head.as_ref().map_or(10.0, |h| h.durability) as u32 / 2 + 10;
            let cheap = if names(&tag, "traits").iter().any(|x| x == "cheap") { 2 } else { 1 };
            let need = tool.damage.div_ceil(per * cheap).min(s.count);
            tool.damage = tool.damage.saturating_sub(need * per * cheap);
            take_count(slot, need);
            changed = true;
            continue;
        }
        // A sharpening kit of a stronger material raises the tool's mining level to that material's.
        if &*s.item == "ddsstinkers:sharpening_kit"
            && let Some(Tag::String(mid)) = s.data().and_then(|d| d.other("ddsstinkers:part")).and_then(|p| p.get("mat"))
            && let Some(m) = t.materials.get(&mid.to_string_lossy())
            && m.level > ti::int(&tag, "lvl").unwrap_or(0)
        {
            let level = m.level;
            edit_tool(tool, |tag| set_int(tag, "lvl", level));
            take_count(slot, 1);
            changed = true;
            continue;
        }
        let Some((id, most, per_level)) = modifier_item(&s.item) else { continue };
        let free = ti::int(&tag, "free").unwrap_or(0);
        let have = mod_level(tool, id);
        if have >= most * per_level {
            continue;
        }
        // A new modifier takes a slot; more of one it already has only adds to its level.
        if have == 0 && free <= 0 {
            continue;
        }
        let worth = if s.item.ends_with("_block") { 9 } else { 1 };
        let add = (s.count as i32 * worth).min(most * per_level - have);
        let used = (add + worth - 1) / worth;
        take_count(slot, used as u32);
        let id_owned = id.to_owned();
        let level_after = have + add;
        edit_tool(tool, |tag| {
            if have == 0 {
                let f = ti::int(tag, "free").unwrap_or(0);
                set_int(tag, "free", f - 1);
            }
            if tag.get("mods").is_none()
                && let Tag::Compound(c) = tag
            {
                c.insert(JavaString::from("mods"), Tag::Compound(Default::default()));
            }
            if let Some(m) = tag.get_mut("mods")
                && let Tag::Compound(c) = m
            {
                c.insert(JavaString::from(id_owned.as_str()), Tag::Int(level_after));
            }
            // One-time bonuses.
            match id_owned.as_str() {
                "luck" | "silktouch" => {}
                "diamond" => {
                    let d = ti::int(tag, "dur").unwrap_or(0);
                    set_int(tag, "dur", d + 500);
                    let s = ti::float(tag, "speed").unwrap_or(0.0);
                    set_float(tag, "speed", s + 0.5);
                    let a = ti::float(tag, "atk").unwrap_or(0.0);
                    set_float(tag, "atk", a + 1.0);
                    let l = ti::int(tag, "lvl").unwrap_or(0);
                    if l < 20 {
                        set_int(tag, "lvl", 20); // Diamond's mining level, as Tinkers' diamond modifier gave
                    }
                }
                "emerald" => {
                    let d = ti::int(tag, "dur").unwrap_or(0);
                    set_int(tag, "dur", d + d / 2);
                    let l = ti::int(tag, "lvl").unwrap_or(0);
                    set_int(tag, "lvl", l.max(14));
                }
                "haste" => {
                    let s = ti::float(tag, "speed").unwrap_or(0.0);
                    set_float(tag, "speed", s + add as f32 * 0.024);
                }
                _ => {}
            }
        });
        // Luck and Silky do their work through the game's own loot tables.
        let level = mod_level(tool, id);
        let enchants: &[(&str, i32)] = match id {
            "luck" => &[("minecraft:fortune", 1), ("minecraft:looting", 1)],
            "silktouch" => &[("minecraft:silk_touch", 1)],
            _ => &[],
        };
        for (e, per) in enchants {
            let lv = (level * per / (per_level.max(1) * if id == "silktouch" { 1 } else { 1 })).max(1) as u32;
            let d = tool.data_mut();
            d.enchantments.retain(|(n, _)| &**n != *e);
            d.enchantments.push((std::sync::Arc::from(*e), lv));
        }
        changed = true;
    }
    changed
}

fn take_count(slot: &mut Option<ItemStack>, n: u32) {
    if let Some(s) = slot {
        if s.count <= n {
            *slot = None;
        } else {
            s.count -= n;
        }
    }
}

pub fn tooltip_mods(stack: &ItemStack) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(Tag::Compound(c)) = ti::tool_tag(stack).and_then(|t| t.get("mods")) {
        for (k, v) in c {
            if let Tag::Int(i) = v {
                out.push(format!("{} ({})", k.to_string_lossy().replace('_', " "), i));
            }
        }
    }
    let _ = M;
    out
}

/// Drops of a block a tool broke: Autosmelt, Prosperous, Pulverizing, Nature's Blessing.
pub fn loot(game: &mut Game<'_>, broke_block: bool, drops: &mut Vec<ItemStack>) {
    if !super::tinkers::is_server(game) {
        return;
    }
    let selected = game.selected;
    let Some(stack) = game.inventory[selected].clone() else { return };
    let Some(tag) = ti::tool_tag(&stack) else { return };
    // Global Traveler: whatever a marked tool frees goes straight into the marked container, block or mob alike.
    if let Some(at) = marked_box(tag) {
        let mut left = Vec::new();
        for d in drops.drain(..) {
            if let Some(rest) = game.native_insert_item(at, horizon_core::geometry::Direction::Up, d) {
                left.push(rest);
            }
        }
        *drops = left;
    }
    if !broke_block {
        return;
    }
    let mut extra = Vec::new();
    for tr in traits_of(&stack) {
        match tr.as_str() {
            "mutate" if roll(game, 10) => {
                const INTO: [&str; 9] = ["stone", "cobblestone", "dirt", "sand", "grass_block", "clay", "netherrack", "ice", "snow_block"];
                let i = pick(game, INTO.len());
                for d in drops.iter_mut() {
                    *d = ItemStack { item: std::sync::Arc::from(format!("minecraft:{}", INTO[i])), count: d.count, damage: 0, extra: None };
                }
            }
            "autosmelt" => {
                let recipes = game.res.recipes();
                for d in drops.iter_mut() {
                    if let Some(c) = recipes.cook("minecraft:smelting", d) {
                        let count = d.count * c.result.count;
                        *d = ItemStack { count, ..c.result.clone() };
                    }
                }
            }
            "prosperous" if roll(game, 8) => {
                if let Some(first) = drops.first() {
                    extra.push(ItemStack { count: 1, ..first.clone() });
                }
            }
            "pulverizing" if roll(game, 3) => drops.clear(),
            "naturesblessing" if roll(game, 20) => extra.push(ItemStack { item: std::sync::Arc::from("minecraft:bread"), count: 1, damage: 0, extra: None }),
            "garishly" if roll(game, 12) => extra.push(ItemStack { item: std::sync::Arc::from("minecraft:blaze_powder"), count: 1, damage: 0, extra: None }),
            _ => {}
        }
    }
    drops.extend(extra);
}

/// Slime Boots: falling never hurts.
pub fn boots_tick(game: &mut Game<'_>) {
    let worn = game.armor.iter().flatten().any(|s| &*s.item == "ddsstinkers:slime_boots");
    if worn
        && let Some(p) = game.player.as_mut()
    {
        p.fall_distance = 0.0;
    }
}

// --- What the rest of the traits share ------------------------------------------------------------------------------------

/// Another way of `roll`: a number below `n`.
fn pick(_game: &mut Game<'_>, n: usize) -> usize {
    (dice() % n.max(1) as u64) as usize
}

fn hurt_mob(game: &mut Game<'_>, id: u32, amount: f32) {
    let Some(health) = game.mobs.iter().find(|m| m.id == id).map(|m| m.health) else { return };
    let dim = game.native_dimension().to_owned();
    if health <= amount {
        game.native_kill_mob(&dim, id);
    } else {
        game.native_set_mob_health(&dim, id, health - amount);
    }
}

fn is_boss(kind: &str) -> bool {
    matches!(kind, "minecraft:ender_dragon" | "minecraft:wither" | "minecraft:elder_guardian" | "minecraft:warden" | "minecraft:player")
}

fn falls(block: &str) -> bool {
    block == "minecraft:sand" || block == "minecraft:red_sand" || block == "minecraft:gravel" || block.ends_with("_concrete_powder") || block.ends_with("suspicious_sand") || block.ends_with("suspicious_gravel")
}

/// How warm the biome underfoot is: above 0 hot, below 0 cold, from its name as there is no temperature table to hand here.
fn heat(game: &Game<'_>) -> f32 {
    let Some(p) = game.player.as_ref().map(|p| p.position) else { return 0.0 };
    let Some(name) = game.biome_name_at(p) else { return 0.0 };
    const HOT: [&str; 6] = ["desert", "badlands", "savanna", "nether", "basalt", "crimson"];
    const COLD: [&str; 7] = ["snowy", "frozen", "ice", "grove", "peaks", "cold", "taiga"];
    if HOT.iter().any(|w| name.contains(w)) {
        1.0
    } else if COLD.iter().any(|w| name.contains(w)) {
        -1.0
    } else {
        0.0
    }
}

fn in_sunlight(game: &Game<'_>) -> bool {
    let Some(p) = game.player.as_ref().map(|p| p.position) else { return false };
    let (_, sky) = game.native_light([p.x.floor() as i32, (p.y + 1.0).floor() as i32, p.z.floor() as i32]);
    sky >= 15 && !is_night(game)
}

fn riding(game: &Game<'_>) -> bool {
    game.riding.is_some()
}

/// Stop Being Selfish: the nearer other players stand, the more it adds, up to 4.
fn closeness(game: &Game<'_>) -> f32 {
    let Some(p) = game.player.as_ref().map(|p| p.position) else { return 0.0 };
    let Some(lan) = game.lan.as_ref() else { return 0.0 };
    let mut bonus = 0.0;
    for g in &lan.hub.guests {
        let d = ((g.position[0] - p.x).powi(2) + (g.position[1] - p.y).powi(2) + (g.position[2] - p.z).powi(2)).sqrt() as f32;
        if d < 16.0 {
            bonus += (16.0 - d) / 8.0;
        }
    }
    bonus.min(4.0)
}

fn critical_now(game: &Game<'_>) -> bool {
    game.player.as_ref().is_some_and(|p| p.fall_distance > 0.0 && !p.ground.on_ground)
}

fn strip_armor(game: &mut Game<'_>, entity: u32) {
    let piece = game.mobs.iter_mut().find(|m| m.id == entity).and_then(|m| {
        m.needs_sync = true;
        m.extra.armor.iter_mut().find(|a| a.is_some())?.take()
    });
    if let Some(piece) = piece {
        drop_at(game, entity, &piece.item);
    }
}

/// Shocking: a full charge goes into the blow (`attack_value` added it) and into the wielder's legs.
fn discharge_hit(game: &mut Game<'_>, _entity: u32) {
    let selected = game.selected;
    let mut full = false;
    if let Some(s) = game.inventory[selected].as_mut() {
        edit_tool(s, |t| {
            let v = ti::int(t, "s_charge").unwrap_or(0);
            full = v >= 100;
            set_int(t, "s_charge", if full { 0 } else { (v + 5).min(100) });
        });
    }
    if full {
        game.give_player_effect("minecraft:speed", 100, 1);
    }
}

// Pain and lava wait for the player's own tick: inside an attack or a break the player isn't ready to be hurt or the block to go.
static PAIN: std::sync::Mutex<Vec<(&'static str, f32)>> = std::sync::Mutex::new(Vec::new());
static LAVA: std::sync::Mutex<Vec<[i32; 3]>> = std::sync::Mutex::new(Vec::new());
fn player_pain(kind: &'static str, amount: f32) {
    if let Ok(mut q) = PAIN.lock() {
        q.push((kind, amount));
    }
}

// --- Things a tool holds -------------------------------------------------------------------------------------------------

fn set_str(tag: &mut Tag, key: &str, v: &str) {
    if let Tag::Compound(c) = tag {
        c.insert(JavaString::from(key), Tag::String(JavaString::from(v)));
    }
}
fn get_str(tag: &Tag, key: &str) -> Option<String> {
    match tag.get(key) {
        Some(Tag::String(s)) if !s.to_string_lossy().is_empty() => Some(s.to_string_lossy()),
        _ => None,
    }
}

fn caught(game: &mut Game<'_>, kind: &str) {
    let selected = game.selected;
    if let Some(s) = game.inventory[selected].as_mut() {
        edit_tool(s, |t| set_str(t, "caught", kind));
    }
}

/// Global Traveler's container, while it is in this level.
fn marked_box(tag: &Tag) -> Option<[i32; 3]> {
    Some([ti::int(tag, "box_x")?, ti::int(tag, "box_y")?, ti::int(tag, "box_z")?])
}
fn portal_of(tag: &Tag) -> Option<[i32; 3]> {
    Some([ti::int(tag, "pt_x")?, ti::int(tag, "pt_y")?, ti::int(tag, "pt_z")?])
}

const FRUITS: [(&str, i32, f32); 6] = [("minecraft:apple", 4, 0.3), ("minecraft:melon_slice", 2, 0.3), ("minecraft:sweet_berries", 2, 0.1), ("minecraft:glow_berries", 2, 0.1), ("minecraft:chorus_fruit", 4, 0.3), ("minecraft:golden_apple", 4, 1.2)];

fn eat_fruit(tag: &mut Tag) -> Option<(i32, f32)> {
    for (item, food, saturation) in FRUITS {
        let key = format!("f_{}", item.trim_start_matches("minecraft:"));
        let n = ti::int(tag, &key).unwrap_or(0);
        if n > 0 {
            set_int(tag, &key, n - 1);
            return Some((food, saturation));
        }
    }
    None
}

enum Act {
    Cure,
    Drift(i32),
    Tire(f32),
    Feed(i32, f32),
    Torch,
    Vein,
    Darkness,
    Attuned,
    Whisper,
    Rescue([i32; 3]),
    Glow,
}

fn run_act(game: &mut Game<'_>, act: Act) {
    match act {
        Act::Cure => queue("effect clear @p minecraft:poison".to_owned()),
        // Alien: the stats wander a little, and never far from where they started.
        Act::Drift(slot) => {
            let (a, b) = (pick(game, 5) as f32 - 2.0, pick(game, 5) as f32 - 2.0);
            if let Some(s) = game.native_carried_stack(slot) {
                edit_tool(s, |t| {
                    let base_s = ti::float(t, "alien_s").or_else(|| ti::float(t, "speed")).unwrap_or(1.0);
                    let base_a = ti::float(t, "alien_a").or_else(|| ti::float(t, "atk")).unwrap_or(1.0);
                    set_float(t, "alien_s", base_s);
                    set_float(t, "alien_a", base_a);
                    set_float(t, "speed", base_s * (1.0 + a * 0.025));
                    set_float(t, "atk", base_a * (1.0 + b * 0.025));
                });
            }
        }
        Act::Tire(amount) => {
            if let Some(p) = game.player.as_mut() {
                p.cause_food_exhaustion(amount);
            }
        }
        Act::Feed(food, saturation) => {
            if let Some(p) = game.player.as_mut() {
                p.health.food.eat(food, saturation);
            }
        }
        // Glowing: a torch underfoot once it gets dark, paid for in durability.
        Act::Torch => {
            let Some(p) = game.player.as_ref().map(|p| p.position) else { return };
            let at = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
            let (block, sky) = game.native_light(at);
            let solid = |b: &str| b != "minecraft:air" && !b.contains("water") && !b.contains("lava");
            if block.max(sky) > 7 || game.native_block(at) != "minecraft:air" || !solid(&game.native_block([at[0], at[1] - 1, at[2]])) {
                return;
            }
            let dim = game.native_dimension().to_owned();
            if let Some(state) = game.pack.assets.registry.default_state("minecraft:torch")
                && game.native_set_block(&dim, at, state, 3)
            {
                game.damage_held(5);
            }
        }
        // Mirabile Visu: stone right by you turns to gold ore, now and then diamond.
        Act::Vein => {
            if !roll(game, 8) {
                return;
            }
            let Some(p) = game.player.as_ref().map(|p| p.position) else { return };
            let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
            for _ in 0..16 {
                let at = [x + pick(game, 5) as i32 - 2, y + pick(game, 4) as i32 - 1, z + pick(game, 5) as i32 - 2];
                if game.native_block(at) == "minecraft:stone" {
                    let ore = if roll(game, 5) { "minecraft:diamond_ore" } else { "minecraft:gold_ore" };
                    let dim = game.native_dimension().to_owned();
                    if let Some(state) = game.pack.assets.registry.default_state(ore)
                        && game.native_set_block(&dim, at, state, 3)
                    {
                        game.damage_held(20);
                    }
                    return;
                }
            }
        }
        // Dark Traveler: whatever stands near suffers.
        Act::Darkness => {
            let Some(p) = game.player.as_ref().map(|p| p.position) else { return };
            let near: Vec<u32> = game.mobs.iter().filter(|m| !m.removed && m.alive() && m.position.distance(p) < 8.0).map(|m| m.id).collect();
            for id in near {
                if roll(game, 3) {
                    let amount = 1.0 + pick(game, 3) as f32;
                    hurt_mob(game, id, amount);
                }
            }
        }
        // I'm a Superstar: no constellations here, so a player's own attunement stands in: one lasting blessing, fixed for them.
        Act::Attuned => {
            const SIGNS: [&str; 5] = ["minecraft:strength", "minecraft:resistance", "minecraft:speed", "minecraft:regeneration", "minecraft:haste"];
            let who = game.native_acting_id();
            game.give_player_effect(SIGNS[(who % SIGNS.len() as u128) as usize], 120, 0);
        }
        Act::Whisper => {
            const LINES: [&str; 6] = ["The tool whispers: more.", "The tool whispers: dig deeper.", "The tool whispers: they are not your friends.", "The tool whispers: feed me.", "The tool whispers: you hear it too, don't you?", "The tool whispers: not that one. The next one."];
            let line = LINES[pick(game, LINES.len())];
            game.native_chat(line);
        }
        // Nick of Time: two pearls and the marked spot, when death is near.
        Act::Rescue(to) => {
            let pearls: u32 = game.inventory.iter().flatten().filter(|s| &*s.item == "minecraft:ender_pearl").map(|s| s.count).sum();
            if pearls < 2 || game.native_block([to[0], to[1] + 1, to[2]]) != "minecraft:air" || game.native_block([to[0], to[1] + 2, to[2]]) != "minecraft:air" {
                return;
            }
            let mut left = 2;
            for slot in game.inventory.iter_mut() {
                if left == 0 {
                    break;
                }
                if let Some(s) = slot
                    && &*s.item == "minecraft:ender_pearl"
                {
                    let take = s.count.min(left);
                    left -= take;
                    s.count -= take;
                    if s.count == 0 {
                        *slot = None;
                    }
                }
            }
            queue(format!("tp @p {} {} {}", to[0], to[1] + 1, to[2]));
            game.give_player_effect("minecraft:regeneration", 100, 2);
        }
        Act::Glow => {
            let Some(p) = game.player.as_ref().map(|p| p.position) else { return };
            for m in game.mobs.iter_mut().filter(|m| m.position.distance(p) < 16.0) {
                m.add_effect("minecraft:glowing", 40, 0);
                m.needs_sync = true;
            }
        }
    }
}

// --- The held tool, every tick --------------------------------------------------------------------------------------------

/// Blind Bandit: an ally for a little while. Iron golems stand in for PlusTiC's own mob.
fn bandit(game: &mut Game<'_>) {
    let Some(p) = game.player.as_ref().map(|p| p.position) else { return };
    let dim = game.native_dimension().to_owned();
    let spawn = crate::game::native_mobs::NativeSpawn { name: Some("Blind Bandit".to_owned()), ..Default::default() };
    if let Some(id) = game.native_spawn_mob(&dim, "minecraft:iron_golem", [p.x + 1.5, p.y, p.z + 1.5], &spawn)
        && let Ok(mut b) = BANDITS.lock()
    {
        b.push((id, game.world.game_time + 600));
    }
}
static BANDITS: std::sync::Mutex<Vec<(u32, i64)>> = std::sync::Mutex::new(Vec::new());
static LAST_HURT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(i64::MIN);
static KEYS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn held_tick(game: &mut Game<'_>) {
    use std::sync::atomic::Ordering;
    // Queued for this tick: pain and the lava of Melting.
    for (kind, amount) in PAIN.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default() {
        game.hurt_player_by(kind, amount);
    }
    let lava = LAVA.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default();
    if !lava.is_empty() {
        let dim = game.native_dimension().to_owned();
        if let Some(state) = game.pack.assets.registry.default_state("minecraft:lava") {
            for at in lava {
                game.native_set_block(&dim, at, state, 3);
            }
        }
    }
    let now = game.world.game_time;
    let gone: Vec<u32> = BANDITS.lock().map(|mut b| {
        let out = b.iter().filter(|(_, until)| *until <= now).map(|(id, _)| *id).collect();
        b.retain(|(_, until)| *until > now);
        out
    }).unwrap_or_default();
    if !gone.is_empty() {
        let dim = game.native_dimension().to_owned();
        for id in gone {
            game.native_remove_mob(&dim, id);
        }
    }

    let Some(stack) = game.held().cloned() else { return };
    let Some(tag) = ti::tool_tag(&stack) else {
        return;
    };
    let traits = names(tag, "traits");
    if traits.is_empty() {
        return;
    }
    // What the speed traits need to know and can't see from where they are asked.
    if let Some((share, at)) = game.player.as_ref().map(|p| (p.health.health() / 20.0, [p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32])) {
        let wet = game.native_block(at) == "minecraft:water" || game.raining_at(horizon_core::position::BlockPos::new(at[0], at[1] + 1, at[2]));
        ti::set_conditions(share, wet, heat(game));
    }
    // Hit by a mob since last time.
    if let Some((who, at)) = game.player_hurt_by()
        && LAST_HURT.swap(at, Ordering::Relaxed) != at
        && game.world.game_time - at < 5
        && let horizon_client::mobs::Who::Mob(attacker) = who
    {
        for tr in &traits {
            match tr.as_str() {
                "spiky" => hurt_mob(game, attacker, 2.0),
                "flammable" => {
                    if let Some(m) = game.mobs.iter_mut().find(|m| m.id == attacker) {
                        m.fire_ticks = m.fire_ticks.max(100);
                    }
                }
                "hailhydra" if roll(game, 4) => game.give_player_effect("minecraft:absorption", 200, 0),
                "blindbandit" if roll(game, 10) => bandit(game),
                "heavy" => {
                    if let Some(p) = game.player.as_mut() {
                        p.velocity.x = 0.0;
                        p.velocity.z = 0.0;
                    }
                }
                _ => {}
            }
        }
    }
    if traits.iter().any(|t| t == "flammable") {
        game.give_player_effect("minecraft:fire_resistance", 40, 0);
    }
    // Moving charges Shocking.
    if traits.iter().any(|t| t == "shocking") && now % 20 == 0 {
        let moved = game.known_player_movement();
        if moved.x.abs() + moved.z.abs() > 0.05 {
            let selected = game.selected;
            if let Some(s) = game.inventory[selected].as_mut() {
                edit_tool(s, |t| {
                    let v = ti::int(t, "s_charge").unwrap_or(0);
                    set_int(t, "s_charge", (v + 2).min(100));
                });
            }
        }
    }
    // Keys, on their press only, and never while typing.
    if game.screen != super::Screen::Playing {
        return;
    }
    use winit::keyboard::KeyCode;
    let pressed = u8::from(game.keys.contains(&KeyCode::KeyN)) | (u8::from(game.keys.contains(&KeyCode::KeyY)) << 1) | (u8::from(game.keys.contains(&KeyCode::Digit0)) << 2);
    let before = KEYS.swap(pressed, Ordering::Relaxed);
    let fresh = pressed & !before;
    if fresh == 0 {
        return;
    }
    let selected = game.selected;
    if fresh & 1 != 0 && traits.iter().any(|t| t == "brownmagic" || t == "nickoftime") {
        if let Some(hit) = game.pick() {
            if let Some(s) = game.inventory[selected].as_mut() {
                edit_tool(s, |t| {
                    set_int(t, "pt_x", hit.pos[0]);
                    set_int(t, "pt_y", hit.pos[1]);
                    set_int(t, "pt_z", hit.pos[2]);
                });
            }
            game.native_chat("Portal set.");
        }
    }
    if fresh & 2 != 0 && traits.iter().any(|t| t == "brownmagic") {
        if let Some(to) = portal_of(tag) {
            if game.native_block([to[0], to[1] + 1, to[2]]) == "minecraft:air" && game.native_block([to[0], to[1] + 2, to[2]]) == "minecraft:air" {
                queue(format!("tp @p {} {} {}", to[0], to[1] + 1, to[2]));
            } else {
                game.native_chat("The portal has no room.");
            }
        }
    }
    if fresh & 4 != 0 && traits.iter().any(|t| t == "portly")
        && let Some(kind) = get_str(tag, "portly")
    {
        let Some(p) = game.player.as_ref().map(|p| p.position) else { return };
        let dim = game.native_dimension().to_owned();
        let a = game.player.as_ref().map_or(0.0, |p| p.yaw.to_radians());
        if game.native_spawn_mob(&dim, &kind, [p.x - f64::from(a.sin()) * 2.0, p.y, p.z + f64::from(a.cos()) * 2.0], &Default::default()).is_some()
            && let Some(s) = game.inventory[selected].as_mut()
        {
            edit_tool(s, |t| set_str(t, "portly", ""));
        }
    }
}

// --- Right clicks --------------------------------------------------------------------------------------------------------

/// A tool used on a block. True when a trait took the click.
pub fn use_on_block(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3], face: [i32; 3]) -> bool {
    let Some(tag) = ti::tool_tag(stack) else { return false };
    let traits = names(tag, "traits");
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    let selected = game.selected;
    // Catcher: what it holds comes out where you click.
    if traits.iter().any(|t| t == "catcher")
        && let Some(kind) = get_str(tag, "caught")
    {
        let dim = game.native_dimension().to_owned();
        let at = [f64::from(pos[0] + face[0]) + 0.5, f64::from(pos[1] + face[1]), f64::from(pos[2] + face[2]) + 0.5];
        if game.native_spawn_mob(&dim, &kind, at, &Default::default()).is_some()
            && let Some(s) = game.inventory[selected].as_mut()
        {
            edit_tool(s, |t| set_str(t, "caught", ""));
        }
        return true;
    }
    // Global Traveler: sneak-click a container to mark it.
    if sneaking && traits.iter().any(|t| t == "global") {
        let key = (pos[0], pos[1], pos[2]);
        if game.world.block_entities.get(&key).is_some() {
            if let Some(s) = game.inventory[selected].as_mut() {
                edit_tool(s, |t| {
                    set_int(t, "box_x", pos[0]);
                    set_int(t, "box_y", pos[1]);
                    set_int(t, "box_z", pos[2]);
                });
            }
            game.native_chat("Drops now go to that container.");
            return true;
        }
    }
    false
}

/// A tool used in the air (or on a block nothing else wanted). True when a trait took the click.
pub fn use_in_air(game: &mut Game<'_>, stack: &ItemStack) -> bool {
    let Some(tag) = ti::tool_tag(stack) else { return false };
    if std::env::var_os("DDSSTINKERS_DEBUG").is_some() {
        eprintln!("[tinkers] use_in_air {} traits={:?}", stack.item, names(tag, "traits"));
    }
    let traits = names(tag, "traits");
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    let selected = game.selected;
    let mut took = false;
    for tr in &traits {
        match tr.as_str() {
            // Ported: several tries to get out of a cave.
            "ported" => {
                took = true;
                if !roll(game, 3) {
                    game.damage_held(1);
                    continue;
                }
                let Some(p) = game.player.as_ref().map(|p| p.position) else { continue };
                let (x, z) = (p.x.floor() as i32, p.z.floor() as i32);
                let mut top = None;
                for y in (p.y.floor() as i32 + 1..320).rev() {
                    if game.native_block([x, y, z]) != "minecraft:air" {
                        top = Some(y + 1);
                        break;
                    }
                }
                if let Some(y) = top
                    && y > p.y.floor() as i32 + 1
                {
                    queue(format!("tp @p {} {} {}", f64::from(x) + 0.5, y, f64::from(z) + 0.5));
                    game.damage_held(5);
                }
            }
            // Whirl: drinks the water around you, further the more it has seen.
            "whirl" => {
                took = true;
                let radius = (2 + ti::int(tag, "tl").unwrap_or(1)).min(10);
                let Some(p) = game.player.as_ref().map(|p| p.position) else { continue };
                let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
                let dim = game.native_dimension().to_owned();
                let Some(air) = game.pack.assets.registry.default_state("minecraft:air") else { continue };
                let mut drunk = 0u32;
                for dx in -radius..=radius {
                    for dy in -radius..=radius {
                        for dz in -radius..=radius {
                            let at = [x + dx, y + dy, z + dz];
                            if game.native_block(at) == "minecraft:water" && game.native_set_block(&dim, at, air, 2) {
                                drunk += 1;
                            }
                        }
                    }
                }
                if drunk > 0 {
                    game.damage_held(1 + drunk / 16);
                }
                if std::env::var_os("DDSSTINKERS_DEBUG").is_some() {
                    eprintln!("[tinkers] whirl r={radius} at={:?} drunk={drunk} feet={}", [x, y, z], game.native_block([x, y, z]));
                }
            }
            "fruitsalad" if sneaking => {
                took = true;
                let found = FRUITS.iter().find_map(|(item, _, _)| {
                    let slot = game.inventory.iter().position(|s| s.as_ref().is_some_and(|s| &*s.item == *item))?;
                    Some((*item, slot))
                });
                if let Some((item, slot)) = found {
                    if let Some(s) = game.inventory[slot].as_mut() {
                        s.count -= 1;
                        if s.count == 0 {
                            game.inventory[slot] = None;
                        }
                    }
                    if let Some(s) = game.inventory[selected].as_mut() {
                        edit_tool(s, |t| {
                            let key = format!("f_{}", item.trim_start_matches("minecraft:"));
                            let n = ti::int(t, &key).unwrap_or(0);
                            set_int(t, &key, n + 1);
                        });
                    }
                }
            }
            // Music of the Spheres: a disc in the bag plays from wherever you stand.
            "musicofthespheres" if sneaking => {
                took = true;
                let disc = game.inventory.iter().flatten().find_map(|s| s.item.strip_prefix("minecraft:music_disc_").map(str::to_owned));
                queue("stopsound @a record".to_owned());
                if let Some(disc) = disc {
                    queue(format!("playsound minecraft:music_disc.{disc} record @a ~ ~ ~ 1 1"));
                }
            }
            _ => {}
        }
    }
    took
}

/// Sneak-right-click on a mob. Portly Gentleman takes it along, at a price in durability.
pub fn use_on_mob(game: &mut Game<'_>, entity: i32, kind: &str, stack: Option<&ItemStack>, claimed: &mut bool) {
    let Some(stack) = stack else { return };
    let Some(tag) = ti::tool_tag(stack) else { return };
    if !names(tag, "traits").iter().any(|t| t == "portly") || !game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) || get_str(tag, "portly").is_some() || is_boss(kind) {
        return;
    }
    let Some(health) = game.mobs.iter().find(|m| m.id == entity as u32).map(|m| m.health) else { return };
    let dim = game.native_dimension().to_owned();
    if game.native_remove_mob(&dim, entity as u32) {
        let selected = game.selected;
        let owned = kind.to_owned();
        if let Some(s) = game.inventory[selected].as_mut() {
            edit_tool(s, |t| set_str(t, "portly", &owned));
        }
        game.damage_held(health as u32);
        *claimed = true;
    }
}

// --- Kills ---------------------------------------------------------------------------------------------------------------

pub fn mob_killed(game: &mut Game<'_>, entity: u32, kind: &str) {
    if std::env::var_os("DDSSTINKERS_DEBUG").is_some() {
        eprintln!("[tinkers] mob_killed {kind} held={:?}", game.inventory[game.selected].as_ref().map(|s| s.item.clone()));
    }
    let selected = game.selected;
    let Some(stack) = game.inventory[selected].clone() else { return };
    let Some(_) = ti::tool_tag(&stack) else { return };
    if let Some(s) = game.inventory[selected].as_mut() {
        edit_tool(s, |t| {
            let k = ti::int(t, "kills").unwrap_or(0);
            set_int(t, "kills", k + 1);
        });
    }
    let at = game.mobs.iter().find(|m| m.id == entity).map(|m| [m.position.x, m.position.y, m.position.z]);
    for tr in traits_of(&stack) {
        match tr.as_str() {
            // Chunky: Mystical Agriculture's mob chunks aren't here, so a spare piece of what the mob drops is.
            "chunky" if roll(game, 3) => {
                let chunk = match kind.strip_prefix("minecraft:").unwrap_or(kind) {
                    "zombie" | "husk" | "drowned" => "minecraft:rotten_flesh",
                    "skeleton" | "stray" | "bogged" => "minecraft:bone",
                    "spider" | "cave_spider" => "minecraft:string",
                    "creeper" => "minecraft:gunpowder",
                    "enderman" => "minecraft:ender_pearl",
                    "blaze" => "minecraft:blaze_rod",
                    "ghast" => "minecraft:ghast_tear",
                    "slime" => "minecraft:slime_ball",
                    "witch" => "minecraft:redstone",
                    "cow" | "mooshroom" => "minecraft:leather",
                    "chicken" => "minecraft:feather",
                    "sheep" => "minecraft:white_wool",
                    "pig" => "minecraft:porkchop",
                    _ => continue,
                };
                drop_at(game, entity, chunk);
            }
            // Dissolving: often nothing, then a lot.
            "dissolving" if roll(game, 4) => {
                if let Some(at) = at {
                    queue(format!("summon minecraft:experience_orb {} {} {} {{Value:{}}}", at[0], at[1], at[2], 30 + pick(game, 70)));
                }
            }
            // Reviving: it gets back up, weaker.
            "reviving" if roll(game, 8) => {
                if let Some(at) = at {
                    let dim = game.native_dimension().to_owned();
                    let spawn = crate::game::native_mobs::NativeSpawn { health: Some(6.0), ..Default::default() };
                    game.native_spawn_mob(&dim, kind, at, &spawn);
                }
            }
            _ => {}
        }
    }
}

/// Slashing: critical hits land harder.
pub fn critical_hit(game: &Game<'_>, multiplier: &mut f32) {
    if let Some(stack) = game.held()
        && traits_of(stack).iter().any(|t| t == "slashing")
    {
        *multiplier *= 1.2;
    }
}

// --- Bows and arrows ------------------------------------------------------------------------------------------------------

/// A shot, once the arrow is in the air: the bow's traits and the arrow's own act on it, and a modular arrow's material sets its damage.
pub fn shot(game: &mut Game<'_>, projectile: u32) {
    if std::env::var_os("DDSSTINKERS_DEBUG").is_some() {
        eprintln!("[tinkers] shot {projectile} found={}", game.creatures.projectiles.iter().any(|p| p.id == projectile));
    }
    let Some(i) = game.creatures.projectiles.iter().position(|p| p.id == projectile) else { return };
    let bow = game.held().cloned();
    let ammo = game.creatures.projectiles[i].arrow.as_ref().map(|a| a.pickup_item.clone());
    let mut traits = bow.as_ref().map_or_else(Vec::new, traits_of);
    traits.extend(ammo.as_ref().map_or_else(Vec::new, traits_of));
    if traits.is_empty() && ammo.as_ref().and_then(|a| ti::tool_tag(a)).is_none() {
        return;
    }
    let arrow_attack = ammo.as_ref().and_then(|a| ti::tool_tag(a)).and_then(|t| ti::float(t, "atk"));
    let mut copies = 0;
    {
        let p = &mut game.creatures.projectiles[i];
        if let Some(atk) = arrow_attack {
            p.bonus.damage += atk;
        }
        for tr in &traits {
            match tr.as_str() {
                "antigravity" => p.no_gravity = true,
                "endspeed" => {
                    p.no_gravity = true;
                    p.velocity = horizon_core::geometry::Vec3::new(p.velocity.x * 3.0, p.velocity.y * 3.0, p.velocity.z * 3.0);
                }
                "hovering" => {
                    p.no_gravity = true;
                    p.velocity = horizon_core::geometry::Vec3::new(p.velocity.x * 0.6, p.velocity.y * 0.6, p.velocity.z * 0.6);
                }
                "arrow_knockback" => p.bonus.punch += 3,
                "naphtha" | "flame2" => {
                    p.bonus.flame = true;
                    if let Some(a) = p.arrow.as_mut() {
                        a.fire_ticks = a.fire_ticks.max(2000);
                    }
                }
                "splitting" => copies = copies.max(1),
                "splitting2" => copies = copies.max(2),
                // Veiled: the renderer can't be told to skip one arrow, so the closest thing: no trail to follow.
                "veiled" => {
                    if let horizon_client::projectiles::ProjectileKind::Arrow { critical, .. } = &mut p.kind {
                        *critical = false;
                    }
                }
                _ => {}
            }
        }
        p.needs_sync = true;
    }
    let spread = |k: i32| f64::from(k) * 0.08;
    for k in 0..copies {
        let mut twin = game.creatures.projectiles[i].clone();
        twin.id = 0;
        let v = twin.velocity;
        let side = if k == 0 { 1 } else { -1 };
        twin.velocity = horizon_core::geometry::Vec3::new(v.x + v.z * spread(side), v.y, v.z - v.x * spread(side));
        game.add_projectile(twin);
    }
}

pub fn hears_impacts(hears: &mut bool) {
    *hears = true;
}

/// Breakable: an arrow that lands may be good for nothing afterwards.
pub fn impact(game: &mut Game<'_>, projectile: u32) {
    let Some(p) = game.creatures.projectiles.iter_mut().find(|p| p.id == projectile) else { return };
    let breakable = p.arrow.as_ref().is_some_and(|a| traits_of(&a.pickup_item).iter().any(|t| t == "breakable"));
    if breakable && NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 2 == 0
        && let Some(a) = p.arrow.as_mut()
    {
        a.pickup = horizon_client::arrows::Pickup::Disallowed;
    }
}
static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
