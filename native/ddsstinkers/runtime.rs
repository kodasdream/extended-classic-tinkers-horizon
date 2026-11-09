// Tinkers' Construct 1.12 on Horizon: the smeltery, casting, the crafting tables and modular tools.
// Every number that Tinkers takes from its registry comes from data/tinkers.json (the DDSS dump).
use super::Game;
use super::tinkers_traits as traits;
use horizon_client::containers::{ModBlockEntity, ModContainer};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_client::items::tinkers_items as ti;
use horizon_client::mod_api::NativeScreen;
use horizon_client::text::Text;
use horizon_core::geometry::Direction;
use horizon_world::nbt::{Compound, JavaString, Tag};
use horizon_world::registry::BlockStateId;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub const M: &str = "ddsstinkers:";
const INGOT: i32 = 144;
const TANK_CAPACITY: i32 = 4000;

// --- The table -----------------------------------------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct Head {
    #[serde(default)]
    pub durability: f32,
    #[serde(default, rename = "miningspeed")]
    pub speed: f32,
    #[serde(default)]
    pub attack: f32,
}
#[derive(Deserialize, Default)]
pub struct Handle {
    #[serde(default)]
    pub modifier: f32,
    #[serde(default)]
    pub durability: f32,
}
#[derive(Deserialize, Default)]
pub struct Extra {
    #[serde(default, rename = "extraDurability")]
    pub durability: f32,
}
#[derive(Deserialize)]
pub struct CraftItem {
    pub item: String,
    pub value: i32,
}
#[derive(Deserialize)]
pub struct Material {
    pub name: String,
    pub color: u32,
    pub level: i32,
    pub fluid: Option<String>,
    pub item: Option<String>,
    pub head: Option<Head>,
    pub handle: Option<Handle>,
    pub extra: Option<Extra>,
    #[serde(default)]
    pub traits: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub craft: Vec<CraftItem>,
    #[serde(default)]
    pub castable: bool,
}
#[derive(Deserialize)]
pub struct Part {
    pub cost: i32,
    pub name: String,
}
#[derive(Deserialize)]
pub struct ToolPart {
    pub part: String,
    pub role: String,
}
#[derive(Deserialize)]
pub struct ToolDef {
    pub aps: f32,
    pub damage: f32,
    pub cutoff: f32,
    pub knockback: f32,
    pub speed: f32,
    pub durability: f32,
    pub attack_add: f32,
    pub attack_mul: f32,
    pub mining: Vec<String>,
    pub big: bool,
    pub parts: Vec<ToolPart>,
    pub layers: usize,
    pub name: String,
}
#[derive(Deserialize)]
pub struct Melt {
    pub fluid: String,
    pub amount: i32,
}
#[derive(Deserialize)]
pub struct AlloyIn {
    pub fluid: String,
    pub amount: i32,
}
#[derive(Deserialize)]
pub struct Alloy {
    pub result: String,
    pub amount: i32,
    pub inputs: Vec<AlloyIn>,
}
#[derive(Deserialize)]
pub struct Fluid {
    pub name: String,
    pub color: u32,
    pub temp: i32,
}
#[derive(Deserialize)]
pub struct Table {
    pub materials: HashMap<String, Material>,
    pub parts: HashMap<String, Part>,
    pub tools: HashMap<String, ToolDef>,
    pub melting: HashMap<String, Melt>,
    pub alloys: Vec<Alloy>,
    pub fluids: HashMap<String, Fluid>,
    #[serde(skip)]
    pub fluid_names: Vec<String>,
    #[serde(skip)]
    pub tool_names: Vec<String>,
    #[serde(skip)]
    pub part_names: Vec<String>,
}

pub fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t: Table = serde_json::from_slice(horizon_client::native_data!("ddsstinkers", "tinkers.json")).expect("data/tinkers.json");
        t.fluid_names = t.fluids.keys().cloned().collect();
        t.fluid_names.sort();
        t.tool_names = t.tools.keys().cloned().collect();
        t.tool_names.sort();
        t.part_names = t.parts.keys().cloned().collect();
        t.part_names.sort();
        t
    })
}

fn fluid_index(name: &str) -> i32 {
    table().fluid_names.binary_search_by(|n| n.as_str().cmp(name)).map_or(-1, |i| i as i32)
}

// --- NBT helpers ---------------------------------------------------------------------------------------------------------

fn nget<'a>(c: &'a Compound, key: &str) -> Option<&'a Tag> {
    c.get(&JavaString::from(key))
}
fn nint(c: &Compound, key: &str) -> i32 {
    match nget(c, key) {
        Some(Tag::Int(v)) => *v,
        Some(Tag::Byte(v)) => i32::from(*v),
        Some(Tag::Short(v)) => i32::from(*v),
        _ => 0,
    }
}
fn nset(c: &mut Compound, key: &str, value: i32) {
    c.insert(JavaString::from(key), Tag::Int(value));
}
fn nstr(c: &Compound, key: &str) -> String {
    match nget(c, key) {
        Some(Tag::String(s)) => s.to_string_lossy(),
        _ => String::new(),
    }
}
fn nsetstr(c: &mut Compound, key: &str, value: &str) {
    c.insert(JavaString::from(key), Tag::String(JavaString::from(value)));
}
fn jstr(value: &str) -> Tag {
    Tag::String(JavaString::from(value))
}
fn float_tag(v: f32) -> Tag {
    Tag::Float(v.to_bits())
}

/// A list of (fluid, amount) as the smeltery keeps it, bottom first.
fn fluids_of(c: &Compound) -> Vec<(String, i32)> {
    let mut out = Vec::new();
    if let Some(Tag::List(list)) = nget(c, "liquids") {
        for e in list {
            if let (Some(Tag::String(f)), Some(Tag::Int(a))) = (e.get("f"), e.get("a")) {
                out.push((f.to_string_lossy(), *a));
            }
        }
    }
    out
}
fn set_fluids(c: &mut Compound, fluids: &[(String, i32)]) {
    let list = fluids.iter().filter(|(_, a)| *a > 0).map(|(f, a)| Tag::compound([("f", jstr(f)), ("a", Tag::Int(*a))])).collect();
    c.insert(JavaString::from("liquids"), Tag::List(list));
}

fn side(pos: [i32; 3], d: [i32; 3]) -> [i32; 3] {
    [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]]
}
fn facing_vec(name: &str) -> [i32; 3] {
    match name {
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "east" => [1, 0, 0],
        "west" => [-1, 0, 0],
        "up" => [0, 1, 0],
        _ => [0, -1, 0],
    }
}
fn serving(game: &Game<'_>) -> bool {
    game.serving.is_some() || game.options.dedicated.is_some()
}
fn short(block: &str) -> Option<&str> {
    block.strip_prefix(M)
}
fn is_wall(block: &str) -> bool {
    short(block).is_some_and(|b| b.starts_with("seared_") || b == "smeltery_controller")
}

// --- Block entities ------------------------------------------------------------------------------------------------------

const ENTITIES: [&str; 17] = ["pattern_chest", "part_chest", "tinker_tank_controller", "smeltery_controller", "seared_furnace_controller", "seared_tank", "seared_gauge", "seared_window", "seared_drain", "seared_faucet", "casting_table", "casting_basin",
                              "part_builder", "stencil_table", "tool_station", "tool_forge", "crafting_station"];

fn is_entity(block: &str) -> bool {
    short(block).is_some_and(|b| ENTITIES.contains(&b))
}

pub fn block_entity(_game: &mut Game<'_>, block: &str, entity: &mut Option<(String, Compound)>) {
    if entity.is_none() && is_entity(block) {
        *entity = Some((block.to_owned(), Compound::new()));
    }
}

pub fn block_entity_ticks(_game: &Game<'_>, block: &str, ticks: &mut bool) {
    if matches!(short(block), Some("smeltery_controller" | "tinker_tank_controller" | "seared_furnace_controller" | "seared_faucet" | "casting_table" | "casting_basin" | "tool_station" | "tool_forge")) {
        *ticks = true;
    }
}

pub fn container(_game: &Game<'_>, block: &str, out: &mut Option<ModContainer>) {
    let (slots, menu, outputs): (usize, Option<&str>, Vec<usize>) = match short(block) {
        Some("smeltery_controller") => (45, Some("ddsstinkers:smeltery"), vec![]),
        Some("casting_table" | "casting_basin") => (2, None, vec![1]),
        Some("pattern_chest" | "part_chest" | "seared_furnace_controller") => (27, Some("minecraft:generic_9x3"), vec![]),
        Some("part_builder") => (3, Some("ddsstinkers:part_builder"), vec![2]),
        Some("stencil_table") => (2, Some("ddsstinkers:stencil_table"), vec![1]),
        Some("tool_station") => (7, Some("ddsstinkers:tool_station"), vec![6]),
        Some("tool_forge") => (7, Some("ddsstinkers:tool_forge"), vec![6]),
        _ => return,
    };
    let all: Vec<usize> = (0..slots).collect();
    *out = Some(ModContainer {
        slots,
        faces: std::array::from_fn(|_| all.clone()),
        inputs: all.iter().copied().filter(|s| !outputs.contains(s)).collect(),
        outputs,
        menu: menu.map(str::to_owned),
    });
}

pub fn update_tag(_pos: [i32; 3], entity: &ModBlockEntity, tag: &mut Option<Compound>) {
    let Some(block) = short(&entity.id) else { return };
    let mut c = Compound::new();
    match block {
        "seared_tank" | "seared_gauge" | "seared_window" | "seared_faucet" | "casting_table" | "casting_basin" | "smeltery_controller" | "tinker_tank_controller" => {
            for key in ["fluid", "amount", "need", "pouring", "x0", "y0", "z0", "x1", "y1", "z1", "valid", "pour_color"] {
                if let Some(v) = nget(&entity.nbt, key) {
                    c.insert(JavaString::from(key), v.clone());
                }
            }
            if let Some(v) = nget(&entity.nbt, "liquids") {
                c.insert(JavaString::from("liquids"), v.clone());
            }
            // What sits on a table: its cast, or what was cast.
            if matches!(block, "casting_table" | "casting_basin") {
                for (i, key) in [(0, "item0"), (1, "item1")] {
                    if let Some(Some(s)) = entity.items.get(i) {
                        nsetstr(&mut c, key, &s.item);
                        if let Some(color) = stack_color(s) {
                            nset(&mut c, &format!("{key}_c"), color as i32);
                        }
                    } else {
                        nsetstr(&mut c, key, "");
                    }
                }
            }
        }
        _ => return,
    }
    *tag = Some(c);
}

fn stack_color(s: &ItemStack) -> Option<u32> {
    let d = s.data()?.other("minecraft:custom_model_data")?;
    match d.get("colors") {
        Some(Tag::IntArray(c)) => c.first().map(|v| *v as u32),
        Some(Tag::List(c)) => match c.first() {
            Some(Tag::Int(v)) => Some(*v as u32),
            _ => None,
        },
        _ => None,
    }
}

pub fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if !serving(game) {
        return;
    }
    let Some(block) = short(&entity.id).map(str::to_owned) else { return };
    match block.as_str() {
        "smeltery_controller" => smeltery_tick(game, pos, entity),
        "tool_station" | "tool_forge" => {
            if entity.data.is_empty() {
                entity.data.resize(8, 0);
            }
            entity.data[0] = if nstr(&entity.nbt, "tool").is_empty() { -1 } else { nint(&entity.nbt, "tool_index") };
        }
        "seared_furnace_controller" => furnace_tick(game, pos, entity),
        "tinker_tank_controller" => tinker_tank_tick(game, pos, entity),
        "seared_faucet" => faucet_tick(game, pos, entity),
        "casting_table" | "casting_basin" => casting_tick(game, pos, entity),
        _ => {}
    }
}

// --- Tanks and pouring ---------------------------------------------------------------------------------------------------

fn tank_room(entity: &ModBlockEntity, fluid: &str) -> i32 {
    let have = nstr(&entity.nbt, "fluid");
    if nint(&entity.nbt, "amount") > 0 && have != fluid {
        return 0;
    }
    TANK_CAPACITY - nint(&entity.nbt, "amount")
}
fn tank_fill(entity: &mut ModBlockEntity, fluid: &str, amount: i32) {
    let now = nint(&entity.nbt, "amount") + amount;
    nsetstr(&mut entity.nbt, "fluid", fluid);
    nset(&mut entity.nbt, "amount", now);
}

/// What a faucet may pull from the block at `pos`: its fluid. Drains stand for their smeltery.
fn source_fluid(game: &mut Game<'_>, pos: [i32; 3]) -> Option<(String, i32)> {
    let block = game.native_block(pos);
    match short(&block)? {
        "seared_tank" | "seared_gauge" | "seared_window" => game.native_with_mod_block_entity(pos, |e| {
            let n = nint(&e.nbt, "amount");
            (n > 0).then(|| (nstr(&e.nbt, "fluid"), n))
        })?,
        "seared_drain" => {
            let c = game.native_with_mod_block_entity(pos, |e| [nint(&e.nbt, "cx"), nint(&e.nbt, "cy"), nint(&e.nbt, "cz")])?;
            game.native_with_mod_block_entity(c, |e| fluids_of(&e.nbt).into_iter().next())?
        }
        _ => None,
    }
}
fn source_take(game: &mut Game<'_>, pos: [i32; 3], amount: i32) {
    let block = game.native_block(pos);
    match short(&block) {
        Some("seared_tank" | "seared_gauge" | "seared_window") => {
            game.native_with_mod_block_entity(pos, |e| {
                let n = (nint(&e.nbt, "amount") - amount).max(0);
                nset(&mut e.nbt, "amount", n);
            });
            game.native_block_entity_sync(pos);
        }
        Some("seared_drain") => {
            let Some(c) = game.native_with_mod_block_entity(pos, |e| [nint(&e.nbt, "cx"), nint(&e.nbt, "cy"), nint(&e.nbt, "cz")]) else { return };
            game.native_with_mod_block_entity(c, |e| {
                let mut f = fluids_of(&e.nbt);
                if let Some(first) = f.first_mut() {
                    first.1 -= amount;
                }
                f.retain(|(_, a)| *a > 0);
                set_fluids(&mut e.nbt, &f);
            });
            game.native_block_entity_sync(c);
        }
        _ => {}
    }
}

/// How much of `fluid` the block at `pos` will take.
fn target_room(game: &mut Game<'_>, pos: [i32; 3], fluid: &str) -> i32 {
    let block = game.native_block(pos);
    match short(&block) {
        Some("seared_tank" | "seared_gauge" | "seared_window") => game.native_with_mod_block_entity(pos, |e| tank_room(e, fluid)).unwrap_or(0),
        Some("seared_drain") => {
            let Some(c) = game.native_with_mod_block_entity(pos, |e| [nint(&e.nbt, "cx"), nint(&e.nbt, "cy"), nint(&e.nbt, "cz")]) else { return 0 };
            game.native_with_mod_block_entity(c, |e| {
                let per = if e.id == "ddsstinkers:tinker_tank_controller" { 4000 } else { 2880 };
                nint(&e.nbt, "volume") * per - used(&fluids_of(&e.nbt))
            })
            .unwrap_or(0)
            .max(0)
        }
        Some("casting_table") => game.native_with_mod_block_entity(pos, |e| casting_room(e, true, fluid)).unwrap_or(0),
        Some("casting_basin") => game.native_with_mod_block_entity(pos, |e| casting_room(e, false, fluid)).unwrap_or(0),
        _ => 0,
    }
}
fn target_fill(game: &mut Game<'_>, pos: [i32; 3], fluid: &str, amount: i32) {
    let block = game.native_block(pos);
    match short(&block) {
        Some("seared_tank" | "seared_gauge" | "seared_window") => {
            game.native_with_mod_block_entity(pos, |e| tank_fill(e, fluid, amount));
        }
        Some("seared_drain") => {
            let Some(c) = game.native_with_mod_block_entity(pos, |e| [nint(&e.nbt, "cx"), nint(&e.nbt, "cy"), nint(&e.nbt, "cz")]) else { return };
            game.native_with_mod_block_entity(c, |e| {
                let mut f = fluids_of(&e.nbt);
                add_fluid(&mut f, fluid, amount);
                set_fluids(&mut e.nbt, &f);
            });
            game.native_block_entity_sync(c);
        }
        Some("casting_table" | "casting_basin") => {
            game.native_with_mod_block_entity(pos, |e| {
                let now = nint(&e.nbt, "amount") + amount;
                nsetstr(&mut e.nbt, "fluid", fluid);
                nset(&mut e.nbt, "amount", now);
            });
        }
        _ => return,
    }
    game.native_block_entity_sync(pos);
}

const FAUCET_RATE: i32 = 6;

fn faucet_tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if nint(&entity.nbt, "on") == 0 {
        return;
    }
    let facing = game.native_block_property(pos, "facing").unwrap_or_else(|| "south".into());
    // The model faces away from what it pulls from.
    let behind = side(pos, facing_vec(&facing));
    let below = side(pos, [0, -1, 0]);
    let moved = (|| {
        let (fluid, have) = source_fluid(game, behind)?;
        let room = target_room(game, below, &fluid);
        let n = FAUCET_RATE.min(have).min(room);
        if n <= 0 {
            return None;
        }
        source_take(game, behind, n);
        target_fill(game, below, &fluid, n);
        Some(fluid)
    })();
    match moved {
        Some(fluid) => {
            if nint(&entity.nbt, "pouring") == 0 {
                nset(&mut entity.nbt, "pouring", 1);
                nset(&mut entity.nbt, "pour_color", table().fluids.get(&fluid).map_or(0xFFFFFF, |f| f.color) as i32);
                game.native_block_entity_sync(pos);
            }
        }
        None => {
            nset(&mut entity.nbt, "on", 0);
            nset(&mut entity.nbt, "pouring", 0);
            game.native_block_entity_sync(pos);
        }
    }
}

// --- Casting -------------------------------------------------------------------------------------------------------------

/// The cast in a table: which item it pours, for how much of the fluid, and what it holds in its data.
struct Cast {
    need: i32,
    result: ItemStack,
    consume: bool,
}

fn plain(item: &str, count: u32) -> ItemStack {
    ItemStack { item: Arc::from(item), count, damage: 0, extra: None }
}

fn material_of_fluid(fluid: &str) -> Option<(&'static str, &'static Material)> {
    table().materials.iter().find(|(_, m)| m.fluid.as_deref() == Some(fluid)).map(|(k, m)| (k.as_str(), m))
}

fn casting_recipe(cast: Option<&ItemStack>, table_top: bool, fluid: &str) -> Option<Cast> {
    let t = table();
    if !table_top {
        // A basin: a block of the metal, or seared stone.
        if fluid == "stone" {
            return Some(Cast { need: INGOT * 2, result: plain("ddsstinkers:seared_stone", 1), consume: false });
        }
        let (sid, _) = material_of_fluid(fluid)?;
        let block = [format!("longclimb:{sid}_block"), format!("ddsstinkers:{sid}_block")];
        for b in block {
            if t.melting.contains_key(&b) {
                return Some(Cast { need: 1296, result: plain(&b, 1), consume: false });
            }
        }
        return None;
    }
    let cast = cast?;
    let id = short(&cast.item).unwrap_or("");
    // Gold or aluminum brass over a part makes a cast of it; the part is used up.
    if fluid == "gold" || fluid == "alubrass" {
        if t.parts.contains_key(id) {
            return Some(Cast { need: 288, result: plain(&format!("ddsstinkers:cast_{id}"), 1), consume: true });
        }
        let ingot = t.materials.values().any(|m| m.item.as_deref() == Some(&*cast.item));
        if ingot {
            return Some(Cast { need: 288, result: plain("ddsstinkers:cast_ingot", 1), consume: true });
        }
        if cast.item.as_ref() == "minecraft:gold_nugget" || cast.item.as_ref() == "minecraft:iron_nugget" {
            return Some(Cast { need: 288, result: plain("ddsstinkers:cast_nugget", 1), consume: true });
        }
    }
    let (sid, mat) = material_of_fluid(fluid)?;
    if id == "cast_ingot" {
        let item = mat.item.as_deref()?;
        return Some(Cast { need: INGOT, result: plain(item, 1), consume: false });
    }
    if id == "cast_nugget" && matches!(fluid, "iron" | "gold") {
        return Some(Cast { need: 16, result: plain(&format!("minecraft:{fluid}_nugget"), 1), consume: false });
    }
    let part = id.strip_prefix("clay_cast_").map(|p| (p, true)).or_else(|| id.strip_prefix("cast_").map(|p| (p, false)))?;
    let p = t.parts.get(part.0)?;
    Some(Cast { need: p.cost, result: make_part(part.0, sid), consume: part.1 })
}

fn casting_room(entity: &ModBlockEntity, table_top: bool, fluid: &str) -> i32 {
    if entity.items.get(1).is_some_and(Option::is_some) {
        return 0;
    }
    let have = nint(&entity.nbt, "amount");
    if have > 0 && nstr(&entity.nbt, "fluid") != fluid {
        return 0;
    }
    let cast = entity.items.first().and_then(Option::as_ref);
    casting_recipe(cast, table_top, fluid).map_or(0, |r| (r.need - have).max(0))
}

fn casting_tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let amount = nint(&entity.nbt, "amount");
    if amount <= 0 {
        return;
    }
    let table_top = entity.id == "ddsstinkers:casting_table";
    let fluid = nstr(&entity.nbt, "fluid");
    let cast = entity.items.first().and_then(Option::as_ref);
    let Some(recipe) = casting_recipe(cast, table_top, &fluid) else { return };
    if nint(&entity.nbt, "need") != recipe.need {
        nset(&mut entity.nbt, "need", recipe.need);
        game.native_block_entity_sync(pos);
    }
    if amount < recipe.need {
        return;
    }
    let temp = table().fluids.get(&fluid).map_or(300, |f| f.temp);
    let time = 12 + (f64::from(temp) * f64::from(recipe.need) / 144.0).sqrt() as i32 * 2;
    let t = nint(&entity.nbt, "timer") + 1;
    if t < time {
        nset(&mut entity.nbt, "timer", t);
        return;
    }
    nset(&mut entity.nbt, "timer", 0);
    nset(&mut entity.nbt, "amount", 0);
    nset(&mut entity.nbt, "need", 0);
    nsetstr(&mut entity.nbt, "fluid", "");
    if recipe.consume {
        entity.items[0] = None;
    }
    entity.items[1] = Some(recipe.result);
    game.native_block_entity_sync(pos);
    traits::sound("minecraft:block.lava.extinguish", pos, 0.4, 1.4);
}

// --- Right clicks --------------------------------------------------------------------------------------------------------

fn give_hand(game: &mut Game<'_>, stack: ItemStack) {
    let selected = game.selected;
    if game.inventory[selected].is_none() {
        game.inventory[selected] = Some(stack);
    } else if let Some(p) = game.player.as_ref().map(|p| p.position) {
        game.spawn_item(stack, [p.x, p.y + 1.0, p.z], [0.0, 0.0, 0.0], 0);
    }
}

pub fn right_click(game: &mut Game<'_>, pos: [i32; 3], _face: [i32; 3], canceled: &mut bool) {
    if !serving(game) {
        return;
    }
    let block = game.native_block(pos);
    if std::env::var_os("DDSSTINKERS_DEBUG").is_some() {
        eprintln!("[tinkers] right click {block} at {pos:?} selected {} held {:?} sneaking {}", game.selected, game.inventory[game.selected].as_ref().map(|h| h.item.to_string()), game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft));
    }
    let Some(id) = short(&block) else { return };
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    // The client sends a click once per hand when the first does nothing it can see. Only the main hand works these blocks.
    if matches!(id, "seared_faucet" | "seared_tank" | "seared_gauge" | "seared_window" | "casting_table" | "casting_basin")
        && matches!(game.guest_use, Some(super::GuestUse::OnBlock(horizon_client::first_person::Hand::Off)))
    {
        if !sneaking {
            *canceled = true;
        }
        return;
    }
    match id {
        "seared_faucet" => {
            game.native_with_mod_block_entity(pos, |e| {
                let on = nint(&e.nbt, "on");
                nset(&mut e.nbt, "on", 1 - on);
            });
            *canceled = true;
        }
        "seared_tank" | "seared_gauge" | "seared_window" => {
            if sneaking {
                return;
            }
            let held = game.inventory[game.selected].as_ref().map(|s| s.item.to_string()).unwrap_or_default();
            if held == "minecraft:lava_bucket" || held == "minecraft:water_bucket" {
                let fluid = if held == "minecraft:lava_bucket" { "lava" } else { "water" };
                let ok = game.native_with_mod_block_entity(pos, |e| {
                    if tank_room(e, fluid) >= 1000 {
                        tank_fill(e, fluid, 1000);
                        true
                    } else {
                        false
                    }
                });
                if ok == Some(true) {
                    game.inventory[game.selected] = Some(plain("minecraft:bucket", 1));
                    game.native_block_entity_sync(pos);
                }
                *canceled = true;
            } else if held == "minecraft:bucket" {
                let got = game.native_with_mod_block_entity(pos, |e| {
                    let f = nstr(&e.nbt, "fluid");
                    let n = nint(&e.nbt, "amount");
                    if n >= 1000 && (f == "lava" || f == "water") {
                        nset(&mut e.nbt, "amount", n - 1000);
                        Some(f)
                    } else {
                        None
                    }
                });
                if let Some(Some(f)) = got {
                    game.inventory[game.selected] = Some(plain(&format!("minecraft:{f}_bucket"), 1));
                    game.native_block_entity_sync(pos);
                }
                *canceled = true;
            }
        }
        "casting_table" | "casting_basin" => {
            if sneaking {
                return;
            }
            let held = game.inventory[game.selected].clone();
            if std::env::var_os("DDSSTINKERS_DEBUG").is_some() {
                let items = game.native_with_mod_block_entity(pos, |e| e.items.len());
                eprintln!("[tinkers] table: slots {items:?}");
            }
            let taken = game.native_with_mod_block_entity(pos, |e| {
                if nint(&e.nbt, "amount") > 0 {
                    return None;
                }
                if let Some(out) = e.items[1].take() {
                    return Some(out);
                }
                if held.is_none() {
                    return e.items[0].take();
                }
                None
            });
            match taken {
                Some(Some(stack)) => {
                    give_hand(game, stack);
                    game.native_block_entity_sync(pos);
                }
                Some(None) => {
                    if let Some(h) = held {
                        let placed = game.native_with_mod_block_entity(pos, |e| {
                            if e.items[0].is_none() && e.id == "ddsstinkers:casting_table" {
                                e.items[0] = Some(ItemStack { count: 1, ..h.clone() });
                                true
                            } else {
                                false
                            }
                        });
                        if placed == Some(true) {
                            let selected = game.selected;
                            if let Some(s) = game.inventory[selected].as_mut() {
                                s.count -= 1;
                                if s.count == 0 {
                                    game.inventory[selected] = None;
                                }
                            }
                            game.native_block_entity_sync(pos);
                        }
                    }
                }
                None => {}
            }
            *canceled = true;
        }
        _ => {}
    }
}

/// An AIOT works as a shovel, or a hoe when sneaking, as well as the axe it starts from. So do Mattocks and Kamas.
pub fn use_item_on(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3], face: [i32; 3], claimed: &mut bool) {
    if !serving(game) {
        return;
    }
    if traits::use_on_block(game, stack, pos, face) {
        *claimed = true;
        return;
    }
    if short(&stack.item) == Some("slimesling") {
        use_item(game, stack, claimed);
        return;
    }
    let item = &*stack.item;
    let (shovel, hoe) = match item {
        "ddsstinkers:mattock" => (true, false),
        "ddsstinkers:kama" | "ddsstinkers:scythe" => (false, true),
        _ => (false, false),
    };
    if !shovel && !hoe {
        return;
    }
    let sneaking = game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
    let block = game.native_block(pos);
    if game.native_block(side(pos, [0, 1, 0])) != "minecraft:air" {
        return;
    }
    let dim = game.native_dimension().to_owned();
    let items = game.res.items();
    let new = if hoe && (sneaking || !shovel) && items.block_in_tag("minecraft:turns_into_farmland", &block) {
        Some(("minecraft:farmland", "minecraft:item.hoe.till"))
    } else if shovel && !sneaking && items.block_in_tag("minecraft:turns_into_dirt_path", &block) {
        Some(("minecraft:dirt_path", "minecraft:item.shovel.flatten"))
    } else {
        None
    };
    let Some((target, sound)) = new else { return };
    let Some(state) = game.pack.assets.registry.default_state(target) else { return };
    if game.native_set_block(&dim, pos, state, 3) {
        traits::sound(sound, pos, 1.0, 1.0);
        game.damage_held(1);
        *claimed = true;
    }
}

// --- The smeltery --------------------------------------------------------------------------------------------------------

struct Shape {
    min: [i32; 3],
    max: [i32; 3],
    tanks: Vec<[i32; 3]>,
    drains: Vec<[i32; 3]>,
}

const MAX_WIDTH: i32 = 9; // 11 across counting its walls, as the manual says
const MAX_HEIGHT: i32 = 32;

/// The room behind the controller: rectangular, floored and walled in seared blocks.
fn scan(game: &Game<'_>, pos: [i32; 3], facing: [i32; 3]) -> Option<Shape> {
    let air = |p: [i32; 3]| game.native_block(p) == "minecraft:air";
    let start = side(pos, [-facing[0], 0, -facing[2]]);
    if !air(start) {
        return None;
    }
    // Walk out from the first air block to the walls on this layer.
    let walk = |d: [i32; 3]| -> Option<i32> {
        let mut p = start;
        for n in 0..=MAX_WIDTH {
            let next = side(p, d);
            if !air(next) {
                return Some(n);
            }
            p = next;
        }
        None
    };
    let (w, e, n, s) = (walk([-1, 0, 0])?, walk([1, 0, 0])?, walk([0, 0, -1])?, walk([0, 0, 1])?);
    let (x0, x1, z0, z1) = (start[0] - w, start[0] + e, start[2] - n, start[2] + s);
    if x1 - x0 + 1 > MAX_WIDTH || z1 - z0 + 1 > MAX_WIDTH {
        return None;
    }
    let mut y0 = pos[1];
    for _ in 0..MAX_HEIGHT {
        if air([start[0], y0 - 1, start[2]]) {
            y0 -= 1;
        } else {
            break;
        }
    }
    let mut tanks = Vec::new();
    let mut drains = Vec::new();
    let mut note = |p: [i32; 3], b: &str| match short(b) {
        Some("seared_tank" | "seared_gauge" | "seared_window") => tanks.push(p),
        Some("seared_drain") => drains.push(p),
        _ => {}
    };
    // The floor.
    for x in x0..=x1 {
        for z in z0..=z1 {
            let p = [x, y0 - 1, z];
            let b = game.native_block(p);
            if !is_wall(&b) {
                return None;
            }
            note(p, &b);
        }
    }
    // Layers while their walls hold and their middle is air.
    let mut y1 = y0 - 1;
    'layers: for y in y0..y0 + MAX_HEIGHT {
        let mut found = Vec::new();
        for x in x0 - 1..=x1 + 1 {
            for z in z0 - 1..=z1 + 1 {
                let inside = x >= x0 && x <= x1 && z >= z0 && z <= z1;
                let corner = (x == x0 - 1 || x == x1 + 1) && (z == z0 - 1 || z == z1 + 1);
                if corner {
                    continue;
                }
                let p = [x, y, z];
                let b = game.native_block(p);
                if inside {
                    if b != "minecraft:air" {
                        break 'layers;
                    }
                } else if is_wall(&b) {
                    found.push((p, b));
                } else {
                    break 'layers;
                }
            }
        }
        for (p, b) in found {
            note(p, &b);
        }
        y1 = y;
    }
    if y1 < pos[1] {
        return None; // the controller must be in a wall of the room
    }
    Some(Shape { min: [x0, y0, z0], max: [x1, y1, z1], tanks, drains })
}

fn used(fluids: &[(String, i32)]) -> i32 {
    fluids.iter().map(|(_, a)| *a).sum()
}

fn add_fluid(fluids: &mut Vec<(String, i32)>, fluid: &str, amount: i32) {
    if let Some(f) = fluids.iter_mut().find(|(n, _)| n == fluid) {
        f.1 += amount;
    } else {
        fluids.push((fluid.to_owned(), amount));
    }
}

fn smeltery_tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let age = nint(&entity.nbt, "age").wrapping_add(1);
    nset(&mut entity.nbt, "age", age);
    if entity.data.len() < 96 {
        entity.data.resize(96, 0);
    }
    // Look at the structure now and then, as Tinkers does.
    let valid_before = nint(&entity.nbt, "valid");
    if age % 40 == 1 {
        let facing = game.native_block_property(pos, "facing").unwrap_or_else(|| "north".into());
        match scan(game, pos, facing_vec(&facing)) {
            Some(shape) => {
                let volume = (shape.max[0] - shape.min[0] + 1) * (shape.max[1] - shape.min[1] + 1) * (shape.max[2] - shape.min[2] + 1);
                for (k, v) in [("x0", shape.min[0]), ("y0", shape.min[1]), ("z0", shape.min[2]), ("x1", shape.max[0]), ("y1", shape.max[1]), ("z1", shape.max[2]), ("volume", volume), ("valid", 1)] {
                    nset(&mut entity.nbt, k, v);
                }
                let list = shape.tanks.iter().flat_map(|p| p.iter().copied()).collect();
                entity.nbt.insert(JavaString::from("tanks"), Tag::IntArray(list));
                for d in &shape.drains {
                    game.native_with_mod_block_entity(*d, |e| {
                        nset(&mut e.nbt, "cx", pos[0]);
                        nset(&mut e.nbt, "cy", pos[1]);
                        nset(&mut e.nbt, "cz", pos[2]);
                    });
                }
            }
            None => nset(&mut entity.nbt, "valid", 0),
        }
        if nint(&entity.nbt, "valid") != valid_before {
            game.native_block_entity_sync(pos);
        }
    }
    let valid = nint(&entity.nbt, "valid") == 1;
    let volume = nint(&entity.nbt, "volume");
    let slots = volume.clamp(0, 45) as usize;
    let capacity = volume * 2880;
    let mut fluids = fluids_of(&entity.nbt);
    let mut changed = false;
    // Thrown-in items.
    if valid && age % 10 == 0 {
        let min = [nint(&entity.nbt, "x0"), nint(&entity.nbt, "y0"), nint(&entity.nbt, "z0")];
        let max = [nint(&entity.nbt, "x1"), nint(&entity.nbt, "y1"), nint(&entity.nbt, "z1")];
        let dim = game.native_dimension();
        let found = game.native_item_entities(dim, min.map(f64::from), [f64::from(max[0] + 1), f64::from(max[1] + 1), f64::from(max[2] + 1)]);
        for e in found {
            let mut stack = e.stack.clone();
            for s in entity.items.iter_mut().take(slots) {
                if stack.count == 0 {
                    break;
                }
                if s.is_none() {
                    *s = Some(ItemStack { count: 1, ..stack.clone() });
                    stack.count -= 1;
                }
            }
            game.native_set_item_entity(dim, e.id, stack);
        }
    }
    // Fuel: lava from a tank in the walls.
    let mut fuel = nint(&entity.nbt, "fuel");
    let mut temp = nint(&entity.nbt, "temp");
    let wants_heat = valid && entity.items.iter().take(slots).any(Option::is_some);
    if wants_heat && fuel <= 0 {
        let tanks: Vec<[i32; 3]> = match nget(&entity.nbt, "tanks") {
            Some(Tag::IntArray(v)) => v.chunks(3).filter(|c| c.len() == 3).map(|c| [c[0], c[1], c[2]]).collect(),
            _ => Vec::new(),
        };
        for t in tanks {
            let took = game.native_with_mod_block_entity(t, |e| {
                let f = nstr(&e.nbt, "fluid");
                let n = nint(&e.nbt, "amount");
                if f == "lava" && n >= 50 {
                    nset(&mut e.nbt, "amount", n - 50);
                    true
                } else {
                    false
                }
            });
            if took == Some(true) {
                fuel = 100;
                temp = 1000;
                game.native_block_entity_sync(t);
                break;
            }
        }
    }
    let burning = fuel > 0 && wants_heat;
    if burning {
        fuel -= 1;
    }
    if !burning {
        temp = 0;
    }
    nset(&mut entity.nbt, "fuel", fuel);
    nset(&mut entity.nbt, "temp", temp);
    // Melting: each slot warms up until it reaches its fluid's temperature, then runs out if there is room.
    let mut heats = match nget(&entity.nbt, "heat") {
        Some(Tag::IntArray(v)) if v.len() == 45 => v.clone(),
        _ => vec![0; 45],
    };
    let t = table();
    let mut data_heat = [0i32; 45];
    if burning {
        let mut total = used(&fluids);
        for i in 0..slots {
            let Some(stack) = entity.items[i].as_ref() else {
                heats[i] = 0;
                continue;
            };
            let Some(melt) = melt_of(stack) else {
                data_heat[i] = -1;
                continue;
            };
            let need_temp = t.fluids.get(&melt.0).map_or(300, |f| f.temp);
            let need = 60 + need_temp / 3;
            if need_temp > temp {
                data_heat[i] = -1;
                continue;
            }
            heats[i] += 1 + (temp - need_temp) / 100;
            data_heat[i] = (heats[i] * 100 / need).clamp(0, 100);
            if heats[i] >= need && total + melt.1 <= capacity {
                add_fluid(&mut fluids, &melt.0, melt.1);
                total += melt.1;
                entity.items[i] = None;
                heats[i] = 0;
                changed = true;
            }
        }
        // Alloys.
        if age % 5 == 0 {
            for a in &t.alloys {
                if a.inputs.iter().all(|i| fluids.iter().any(|(n, v)| n == &i.fluid && *v >= i.amount)) {
                    for i in &a.inputs {
                        if let Some(f) = fluids.iter_mut().find(|(n, _)| n == &i.fluid) {
                            f.1 -= i.amount;
                        }
                    }
                    add_fluid(&mut fluids, &a.result, a.amount);
                    fluids.retain(|(_, v)| *v > 0);
                    changed = true;
                }
            }
        }
    }
    entity.nbt.insert(JavaString::from("heat"), Tag::IntArray(heats));
    set_fluids(&mut entity.nbt, &fluids);
    // The screen's data.
    entity.data[0] = capacity / 16;
    entity.data[1] = used(&fluids) / 16;
    entity.data[2] = temp;
    entity.data[3] = fuel;
    entity.data[4] = i32::from(valid);
    entity.data[5] = slots as i32;
    for i in 0..16 {
        let (idx, amount) = fluids.get(i).map_or((-1, 0), |(f, a)| (fluid_index(f), *a));
        entity.data[6 + i * 2] = idx;
        entity.data[7 + i * 2] = amount / 16;
    }
    for i in 0..45 {
        entity.data[38 + i] = data_heat[i];
    }
    let lit = burning;
    if nint(&entity.nbt, "lit") != i32::from(lit) {
        nset(&mut entity.nbt, "lit", i32::from(lit));
        game.native_set_block_property_with(pos, "lit", if lit { "true" } else { "false" }, 3);
    }
    if changed {
        game.native_block_entity_sync(pos);
    }
}

/// Scans a multiblock (shared with the smeltery) and keeps its numbers in the controller.
fn keep_shape(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> bool {
    let age = nint(&entity.nbt, "age").wrapping_add(1);
    nset(&mut entity.nbt, "age", age);
    if age % 40 == 1 {
        let before = nint(&entity.nbt, "valid");
        let facing = game.native_block_property(pos, "facing").unwrap_or_else(|| "north".into());
        match scan(game, pos, facing_vec(&facing)) {
            Some(shape) => {
                let volume = (shape.max[0] - shape.min[0] + 1) * (shape.max[1] - shape.min[1] + 1) * (shape.max[2] - shape.min[2] + 1);
                for (k, v) in [("x0", shape.min[0]), ("y0", shape.min[1]), ("z0", shape.min[2]), ("x1", shape.max[0]), ("y1", shape.max[1]), ("z1", shape.max[2]), ("volume", volume), ("valid", 1)] {
                    nset(&mut entity.nbt, k, v);
                }
                let list = shape.tanks.iter().flat_map(|p| p.iter().copied()).collect();
                entity.nbt.insert(JavaString::from("tanks"), Tag::IntArray(list));
                for d in &shape.drains {
                    game.native_with_mod_block_entity(*d, |e| {
                        nset(&mut e.nbt, "cx", pos[0]);
                        nset(&mut e.nbt, "cy", pos[1]);
                        nset(&mut e.nbt, "cz", pos[2]);
                    });
                }
            }
            None => nset(&mut entity.nbt, "valid", 0),
        }
        if nint(&entity.nbt, "valid") != before {
            game.native_block_entity_sync(pos);
        }
    }
    nint(&entity.nbt, "valid") == 1
}

/// A Tinker Tank holds fluids through its drains and does nothing else.
fn tinker_tank_tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    keep_shape(game, pos, entity);
}

/// A Seared Furnace smelts everything in it at once, as a furnace does, on lava from its tanks: 2 ticks an item per hearth.
fn furnace_tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let valid = keep_shape(game, pos, entity);
    let lit_before = nint(&entity.nbt, "lit");
    let mut fuel = nint(&entity.nbt, "fuel");
    let recipes = game.res.recipes();
    let results: Vec<Option<ItemStack>> = entity.items.iter().map(|s| s.as_ref().and_then(|i| recipes.cook("minecraft:smelting", i)).map(|c| c.result.clone())).collect();
    let wants = valid && results.iter().any(Option::is_some);
    if wants && fuel <= 0 {
        let tanks: Vec<[i32; 3]> = match nget(&entity.nbt, "tanks") {
            Some(Tag::IntArray(v)) => v.chunks(3).filter(|c| c.len() == 3).map(|c| [c[0], c[1], c[2]]).collect(),
            _ => Vec::new(),
        };
        for t in tanks {
            let took = game.native_with_mod_block_entity(t, |e| {
                let n = nint(&e.nbt, "amount");
                if nstr(&e.nbt, "fluid") == "lava" && n >= 50 {
                    nset(&mut e.nbt, "amount", n - 50);
                    true
                } else {
                    false
                }
            });
            if took == Some(true) {
                fuel = 200;
                game.native_block_entity_sync(t);
                break;
            }
        }
    }
    let burning = wants && fuel > 0;
    if burning {
        fuel -= 1;
        let mut clock = nint(&entity.nbt, "clock") + 1;
        if clock >= 40 {
            clock = 0;
            for (slot, r) in entity.items.iter_mut().zip(results) {
                if let (Some(s), Some(r)) = (slot.as_mut(), r) {
                    if s.count <= 1 {
                        *slot = Some(r);
                    } else if r.item == s.item {
                        continue;
                    } else {
                        s.count -= 1;
                        let dim = game.native_dimension().to_owned();
                        let _ = dim;
                        game.spawn_item(r, [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 1.1, f64::from(pos[2]) + 0.5], [0.0, 0.1, 0.0], 10);
                    }
                }
            }
        }
        nset(&mut entity.nbt, "clock", clock);
    }
    nset(&mut entity.nbt, "fuel", fuel);
    if lit_before != i32::from(burning) {
        nset(&mut entity.nbt, "lit", i32::from(burning));
        game.native_set_block_property_with(pos, "lit", if burning { "true" } else { "false" }, 3);
    }
}

fn melt_of(stack: &ItemStack) -> Option<(String, i32)> {
    let t = table();
    let item = &*stack.item;
    if let Some(m) = t.melting.get(item) {
        return Some((m.fluid.clone(), m.amount));
    }
    // A tool part or a whole tool melts back to its material, at a loss as in Tinkers (only its head's worth counts for tools).
    let tag = stack.data()?.other("ddsstinkers:part")?;
    let Tag::String(sid) = tag.get("mat")? else { return None };
    let mat = t.materials.get(&sid.to_string_lossy())?;
    let fluid = mat.fluid.clone()?;
    let part = t.parts.get(short(item)?)?;
    Some((fluid, part.cost))
}

// --- Parts and tools -----------------------------------------------------------------------------------------------------

pub fn make_part(part: &str, material: &str) -> ItemStack {
    let t = table();
    let mut stack = plain(&format!("{M}{part}"), 1);
    let m = t.materials.get(material);
    let d = stack.data_mut();
    d.set_other("ddsstinkers:part", Some(Tag::compound([("mat", jstr(material))])));
    if let Some(m) = m {
        d.set_other("minecraft:custom_model_data", Some(Tag::compound([("colors", Tag::IntArray(vec![m.color as i32]))])));
        let name = format!("{} {}", m.name, t.parts.get(part).map_or(part, |p| p.name.as_str()));
        d.set_other("minecraft:item_name", Some(jstr(&name)));
    }
    stack
}

fn part_material(stack: &ItemStack) -> Option<String> {
    match stack.data()?.other("ddsstinkers:part")?.get("mat")? {
        Tag::String(s) => Some(s.to_string_lossy()),
        _ => None,
    }
}

/// `ToolNBT`: heads average, extras add, then handles multiply and add. `materials` go with the tool's part list.
pub fn make_tool(kind: &str, materials: &[String]) -> Option<ItemStack> {
    let t = table();
    let def = t.tools.get(kind)?;
    if materials.len() != def.parts.len() {
        return None;
    }
    let mut heads = Vec::new();
    let mut handles = Vec::new();
    let mut extras = Vec::new();
    let mut level = 0;
    let mut traits: Vec<String> = Vec::new();
    let mut extra_slots = 0;
    for (p, mid) in def.parts.iter().zip(materials) {
        let m = t.materials.get(mid)?;
        let role = p.role.as_str();
        match role {
            "head" | "bow" | "projectile" | "shaft" => {
                if let Some(h) = &m.head {
                    heads.push(h);
                    level = level.max(m.level);
                }
            }
            "handle" => {
                if let Some(h) = &m.handle {
                    handles.push(h);
                }
            }
            "extra" => {
                if let Some(e) = &m.extra {
                    extras.push(e);
                }
            }
            _ => {}
        }
        for tr in m.traits.get(role).into_iter().flatten() {
            if !traits.contains(tr) {
                traits.push(tr.clone());
            }
        }
    }
    let n = heads.len().max(1) as f32;
    let mut durability = heads.iter().map(|h| h.durability).sum::<f32>() / n;
    let mut speed = heads.iter().map(|h| h.speed).sum::<f32>() / n;
    let mut attack = heads.iter().map(|h| h.attack).sum::<f32>() / n;
    if !extras.is_empty() {
        durability += extras.iter().map(|e| e.durability).sum::<f32>() / extras.len() as f32;
    }
    if !handles.is_empty() {
        let k = handles.len() as f32;
        let modifier = handles.iter().map(|h| h.modifier).sum::<f32>() / k;
        durability = (durability * modifier).round() + (handles.iter().map(|h| h.durability).sum::<f32>() / k).round();
    }
    durability = (durability * def.durability).max(1.0);
    // Traits that set a tool's numbers once, when it is made.
    if traits.iter().any(|t| t == "cheapskate") {
        durability *= 0.8;
    }
    // Traits that are just more modifier slots.
    for t in &traits {
        extra_slots += match t.as_str() {
            "writable1" | "moldable1" | "botanical1" => 1,
            "writable2" | "moldable2" | "botanical2" => 2,
            "magical_modifier" => 3,
            _ => 0,
        };
    }
    speed *= def.speed;
    attack = attack * def.attack_mul + def.attack_add;
    let mine = def.mining.iter().fold(0, |m, name| m | match name.as_str() {
        "pickaxe" => 1,
        "axe" => 2,
        "shovel" => 4,
        "hoe" => 8,
        _ => 0,
    });
    // Bows and arrows are vanilla's own items wearing Tinkers' model: the game draws, fires and picks up only those, by id.
    let carrier = match kind {
        "shortbow" | "longbow" => Some("minecraft:bow"),
        "crossbow" => Some("minecraft:crossbow"),
        "arrow" | "bolt" => Some("minecraft:arrow"),
        _ => None,
    };
    let mut stack = plain(carrier.map_or_else(|| format!("{M}{kind}"), str::to_owned).as_str(), 1);
    let colors: Vec<i32> = materials.iter().take(def.layers.max(1)).map(|m| t.materials.get(m).map_or(-1, |x| x.color as i32)).collect();
    let d = stack.data_mut();
    if carrier.is_some() {
        d.set_other("minecraft:item_model", Some(jstr(&format!("{M}{kind}"))));
    }
    d.set_other("minecraft:custom_model_data", Some(Tag::compound([("colors", Tag::IntArray(colors))])));
    let head_mat = def.parts.iter().zip(materials).find(|(p, _)| p.role == "head" || p.role == "bow").and_then(|(_, m)| t.materials.get(m));
    if let Some(h) = head_mat {
        d.set_other("minecraft:item_name", Some(jstr(&format!("{} {}", h.name, def.name))));
    }
    d.set_other(ti::TOOL, Some(Tag::compound([
        ("kind", jstr(kind)),
        ("mats", Tag::List(materials.iter().map(|m| jstr(m)).collect())),
        ("dur", Tag::Int(durability as i32)),
        ("speed", float_tag(speed)),
        ("atk", float_tag(attack)),
        ("lvl", Tag::Int(level)),
        ("mine", Tag::Int(mine)),
        ("free", Tag::Int(3 + extra_slots)),
        ("xp_bonus", Tag::Int(i32::from(traits.iter().any(|t| t == "established" || t == "xu_xp_boost")))),
        ("traits", Tag::List(traits.iter().map(|m| jstr(m)).collect())),
        ("xp", Tag::Int(0)),
        ("tl", Tag::Int(1)),
    ])));
    Some(stack)
}

pub fn component_types(types: &mut Vec<String>) {
    types.push(ti::TOOL.to_owned());
    types.push("ddsstinkers:part".to_owned());
}

pub fn item_bar(stack: &ItemStack, bar: &mut Option<(i32, u32)>) {
    let Some(tag) = ti::tool_tag(stack) else { return };
    let dur = ti::int(tag, "dur").unwrap_or(1).max(1);
    let left = (dur - stack.damage as i32).max(0);
    if left == dur {
        *bar = None;
        return;
    }
    let f = left as f32 / dur as f32;
    let r = ((1.0 - f) * 2.0).min(1.0);
    let g = (f * 2.0).min(1.0);
    *bar = Some(((f * 13.0).round() as i32, ((r * 255.0) as u32) << 16 | ((g * 255.0) as u32) << 8));
}

/// Tooltip lines take their colour as `rgba` packs it; the lines below are written as 0xRRGGBB.
pub fn tooltip(game: &Game<'_>, stack: &ItemStack, lines: &mut Vec<(Text, u32)>) {
    let start = lines.len();
    tooltip_lines(game, stack, lines);
    for line in lines.iter_mut().skip(start) {
        let c = line.1;
        line.1 = horizon_client::hud::rgba((c >> 16 & 255) as u8, (c >> 8 & 255) as u8, (c & 255) as u8, 255);
    }
}

fn tooltip_lines(_game: &Game<'_>, stack: &ItemStack, lines: &mut Vec<(Text, u32)>) {
    let t = table();
    if let Some(mid) = part_material(stack) {
        if let Some(m) = t.materials.get(&mid) {
            if let Some(h) = &m.head {
                lines.push((Text::literal(format!("Durability {}  Speed {:.2}  Attack {:.1}", h.durability, h.speed, h.attack)), 0xAAAAAA));
            }
            lines.push((Text::literal(format!("Mining level {}", m.level)), 0xAAAAAA));
        }
        return;
    }
    let Some(tag) = ti::tool_tag(stack) else { return };
    let dur = ti::int(tag, "dur").unwrap_or(0);
    let left = dur - stack.damage as i32;
    if left <= 0 {
        lines.push((Text::literal("Broken"), 0xFF5555));
    }
    lines.push((Text::literal(format!("Durability {}/{}", left.max(0), dur)), 0xAAAAAA));
    lines.push((Text::literal(format!("Mining level {}", ti::int(tag, "lvl").unwrap_or(0))), 0xAAAAAA));
    lines.push((Text::literal(format!("Mining speed {:.2}", ti::float(tag, "speed").unwrap_or(0.0))), 0xAAAAAA));
    lines.push((Text::literal(format!("Attack {:.2}", ti::float(tag, "atk").unwrap_or(0.0))), 0xAAAAAA));
    lines.push((Text::literal(format!("Modifiers left {}", ti::int(tag, "free").unwrap_or(0))), 0xAAAAAA));
    lines.push((Text::literal(format!("Tool level {} ({} XP)", ti::int(tag, "tl").unwrap_or(1), ti::int(tag, "xp").unwrap_or(0))), 0x55FF55));
    if let Some(Tag::List(list)) = tag.get("traits") {
        for tr in list {
            if let Tag::String(s) = tr {
                lines.push((Text::literal(s.to_string_lossy()), 0xFFAA00));
            }
        }
    }
    for m in traits::tooltip_mods(stack) {
        lines.push((Text::literal(m), 0x55FFFF));
    }
}

pub fn item_damaged(game: &mut Game<'_>, item: &str, amount: &mut i32, cancel: &mut bool) {
    if !item.starts_with(M) {
        return;
    }
    let selected = game.selected;
    let Some(stack) = game.inventory[selected].as_ref() else { return };
    let Some(tag) = ti::tool_tag(stack) else { return };
    let dur = ti::int(tag, "dur").unwrap_or(1);
    // A tool at no durability is broken, not gone: it keeps its parts and waits for repair.
    if stack.damage as i32 >= dur {
        *cancel = true;
        return;
    }
    let room = dur - stack.damage as i32;
    traits::wear(game, amount);
    *amount = (*amount).clamp(0, room);
}

pub fn is_server(game: &Game<'_>) -> bool {
    serving(game)
}
pub fn destroy_block(game: &mut Game<'_>, pos: [i32; 3], _state: BlockStateId, canceled: &mut bool) {
    traits::on_break(game, pos, canceled);
}
pub fn hurt_enemy(game: &mut Game<'_>, stack: &ItemStack, entity: u32, kind: &str) {
    if serving(game) {
        traits::after_hit(game, stack, entity, kind);
    }
}
pub fn inventory_tick(game: &mut Game<'_>, slot: i32, selected: bool) {
    if serving(game) {
        traits::tool_tick(game, slot, selected);
    }
}
pub fn attack_damage(game: &Game<'_>) -> Option<f32> {
    traits::attack_value(game)
}
pub fn melee_target(kind: &str) {
    traits::set_target(kind);
}
pub fn loot_drops(game: &mut Game<'_>, context: &horizon_client::mod_api::NativeLootDrops<'_>, drops: &mut Vec<ItemStack>) {
    traits::loot(game, context.block.is_some() && context.player.is_some(), drops);
}
pub fn server_tick(game: &mut Game<'_>) {
    traits::flush(game);
}
pub fn player_tick(game: &mut Game<'_>) {
    if serving(game) {
        traits::player_tick(game);
    }
}

// --- Crafting tables -----------------------------------------------------------------------------------------------------

pub fn menu_button(game: &mut Game<'_>, _pos: [i32; 3], entity: &mut ModBlockEntity, id: i32) {
    let Some(block) = short(&entity.id).map(str::to_owned) else { return };
    match block.as_str() {
        "stencil_table" => {
            // Pick a part: one blank pattern becomes that part's pattern.
            let t = table();
            if let Some(name) = t.part_names.get(id as usize) {
                let blank = entity.items[0].as_ref().is_some_and(|s| &*s.item == "ddsstinkers:pattern");
                if blank && entity.items[1].is_none() {
                    take(&mut entity.items[0], 1);
                    entity.items[1] = Some(plain(&format!("{M}pattern_{name}"), 1));
                }
            }
        }
        "part_builder" => part_builder_craft(game, entity),
        "tool_station" | "tool_forge" => tool_button(entity, id, block == "tool_forge"),
        "smeltery_controller" => {
            // A press on a fluid sends it to the bottom, where drains pull from.
            let i = (id - 100) as usize;
            let mut f = fluids_of(&entity.nbt);
            if i < f.len() {
                let chosen = f.remove(i);
                f.insert(0, chosen);
                set_fluids(&mut entity.nbt, &f);
            }
        }
        _ => {}
    }
}

fn take(slot: &mut Option<ItemStack>, n: u32) {
    if let Some(s) = slot {
        if s.count <= n {
            *slot = None;
        } else {
            s.count -= n;
        }
    }
}

fn part_builder_craft(game: &Game<'_>, entity: &mut ModBlockEntity) {
    let t = table();
    let Some(pattern) = entity.items[0].as_ref() else { return };
    let Some(part) = short(&pattern.item).and_then(|p| p.strip_prefix("pattern_")).map(str::to_owned) else { return };
    let Some(p) = t.parts.get(&part) else { return };
    let Some(input) = entity.items[1].as_ref() else { return };
    if entity.items[2].is_some() {
        return;
    }
    let items = game.res.items();
    let found = t.materials.iter().find_map(|(sid, m)| {
        m.craft.iter().find_map(|c| {
            let hit = match c.item.strip_prefix('#') {
                Some(tag) => items.item_in_tag(tag, &input.item),
                None => c.item == *input.item,
            };
            hit.then_some((sid.clone(), c.value))
        })
    });
    let Some((sid, value)) = found else { return };
    let need = ((p.cost + value - 1) / value.max(1)) as u32;
    if input.count < need {
        return;
    }
    take(&mut entity.items[1], need);
    entity.items[2] = Some(make_part(&part, &sid));
}

/// Tool Station and Tool Forge. Buttons 0.. pick the tool; 1000 forges with what lies in the slots.
fn tool_button(entity: &mut ModBlockEntity, id: i32, forge: bool) {
    let t = table();
    if id < 1000 {
        if let Some(kind) = t.tool_names.get(id as usize)
            && (forge || !t.tools[kind].big)
        {
            nsetstr(&mut entity.nbt, "tool", kind);
            nset(&mut entity.nbt, "tool_index", id);
        }
        return;
    }
    if entity.items[6].is_some() {
        return;
    }
    // A tool in the first slot: repair and modify it, and it comes out finished.
    if entity.items[0].as_ref().is_some_and(|s| ti::tool_tag(s).is_some()) {
        let mut tool = entity.items[0].take().unwrap_or_else(|| plain("minecraft:air", 0));
        let (_, rest) = entity.items.split_at_mut(1);
        let changed = traits::apply(&mut tool, &mut rest[..5]);
        if changed {
            entity.items[6] = Some(tool);
        } else {
            entity.items[0] = Some(tool);
        }
        return;
    }
    let kind = nstr(&entity.nbt, "tool");
    let Some(def) = t.tools.get(&kind) else { return };
    // Slot 0 is the tool itself when modifying; slots 1 to 5 hold the parts in the tool's order.
    let mut mats = Vec::new();
    for (i, p) in def.parts.iter().enumerate() {
        let Some(s) = entity.items[i + 1].as_ref() else { return };
        if short(&s.item) != Some(p.part.as_str()) {
            return;
        }
        let Some(m) = part_material(s) else { return };
        mats.push(m);
    }
    let Some(result) = make_tool(&kind, &mats) else { return };
    for i in 0..def.parts.len() {
        take(&mut entity.items[i + 1], 1);
    }
    entity.items[6] = Some(result);
}

// --- Screens -------------------------------------------------------------------------------------------------------------

fn grid(x: f32, y: f32, cols: usize, rows: usize) -> Vec<Option<[f32; 2]>> {
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            out.push(Some([x + c as f32 * 18.0, y + r as f32 * 18.0]));
        }
    }
    out
}

pub fn screen(_game: &Game<'_>, block: &str, out: &mut Option<NativeScreen>) {
    let s = match short(block) {
        Some("smeltery_controller") => NativeScreen { width: 234.0, height: 200.0, slots: grid(64.0, 18.0, 9, 5), inventory_y: 118.0, inventory_x: 37.0, own_labels: false, own_background: false },
        Some("part_builder") => NativeScreen { width: 176.0, height: 166.0, slots: vec![Some([26.0, 35.0]), Some([62.0, 35.0]), Some([134.0, 35.0])], inventory_y: 84.0, inventory_x: 8.0, own_labels: false, own_background: false },
        Some("stencil_table") => NativeScreen { width: 240.0, height: 200.0, slots: vec![Some([12.0, 30.0]), Some([12.0, 72.0])], inventory_y: 118.0, inventory_x: 40.0, own_labels: false, own_background: false },
        Some("tool_station" | "tool_forge") => {
            let slots = vec![Some([162.0, 20.0]), Some([108.0, 20.0]), Some([108.0, 44.0]), Some([108.0, 68.0]), Some([132.0, 32.0]), Some([132.0, 56.0]), Some([190.0, 44.0])];
            NativeScreen { width: 240.0, height: 200.0, slots, inventory_y: 118.0, inventory_x: 40.0, own_labels: false, own_background: false }
        }
        _ => return,
    };
    *out = Some(s);
}

const ROW: f32 = 18.0;
const TOOL_COLS: usize = 5;

pub fn screen_draw(game: &Game<'_>, block: &str, data: &[i32], batch: &mut HudBatch, origin: [f32; 2]) {
    let t = table();
    let at = |x: f32, y: f32| [origin[0] + x, origin[1] + y];
    match short(block) {
        Some("smeltery_controller") => {
            let d = |i: usize| data.get(i).copied().unwrap_or(0);
            // The fluid column.
            let (x, y, w, h) = (origin[0] + 8.0, origin[1] + 18.0, 48.0, 90.0);
            game.native_fill(batch, [x - 1.0, y - 1.0, w + 2.0, h + 2.0], 0xFF20_1C1C);
            let capacity = d(0).max(1);
            let mut bottom = y + h;
            for i in 0..16 {
                let idx = d(6 + i * 2);
                let amount = d(7 + i * 2);
                if idx < 0 || amount <= 0 {
                    continue;
                }
                let color = t.fluid_names.get(idx as usize).and_then(|n| t.fluids.get(n)).map_or(0xFFFFFF, |f| f.color);
                let bar = (amount as f32 / capacity as f32 * h).max(2.0);
                game.native_fill(batch, [x, bottom - bar, w, bar], 0xFF00_0000 | color);
                bottom -= bar;
            }
            if d(4) == 0 {
                game.native_text(batch, "Not complete", at(140.0, 6.0), 0xFF5555, false);
            } else {
                game.native_text(batch, &format!("{} C   {} slots", d(2), d(5)), at(140.0, 6.0), 0x404040, false);
            }
            // Heat bars beside the items.
            for i in 0..45usize {
                let v = d(38 + i);
                let (sx, sy) = (origin[0] + 64.0 + (i % 9) as f32 * 18.0, origin[1] + 18.0 + (i / 9) as f32 * 18.0);
                if v < 0 {
                    game.native_fill(batch, [sx, sy + 15.0, 16.0, 1.0], 0xFFFF_3030);
                } else if v > 0 {
                    game.native_fill(batch, [sx, sy + 15.0, 16.0 * v as f32 / 100.0, 1.0], 0xFFFF_A020);
                }
            }
        }
        Some("part_builder") => {
            game.native_text(batch, "Pattern", at(8.0, 22.0), 0x404040, false);
            game.native_text(batch, "Material", at(50.0, 22.0), 0x404040, false);
            game.native_fill(batch, [origin[0] + 88.0, origin[1] + 33.0, 36.0, 20.0], 0xFF70_7070);
            game.native_text(batch, "Craft", at(92.0, 39.0), 0xFFFFFF, true);
        }
        Some("stencil_table") => {
            game.native_text(batch, "Blank", at(8.0, 20.0), 0x404040, false);
            for (i, name) in t.part_names.iter().enumerate() {
                let (cx, cy) = (origin[0] + 40.0 + (i % 10) as f32 * ROW, origin[1] + 18.0 + (i / 10) as f32 * ROW);
                game.native_fill(batch, [cx, cy, 16.0, 16.0], 0xFF8B_8B8B);
                game.native_item(batch, &format!("{M}pattern_{name}"), [cx, cy], 16.0);
            }
        }
        Some("tool_station" | "tool_forge") => {
            let forge = block.ends_with("tool_forge");
            let shown = t.tool_names.iter().filter(|k| forge || !t.tools[*k].big);
            for (i, kind) in shown.enumerate() {
                let (cx, cy) = (origin[0] + 8.0 + (i % TOOL_COLS) as f32 * ROW, origin[1] + 18.0 + (i / TOOL_COLS) as f32 * ROW);
                let idx = t.tool_names.iter().position(|k| k == kind).unwrap_or(0) as i32;
                let chosen = data.get(0).copied().unwrap_or(-1) == idx;
                game.native_fill(batch, [cx, cy, 16.0, 16.0], if chosen { 0xFF55_A055 } else { 0xFF8B_8B8B });
                game.native_item(batch, &format!("{M}{kind}"), [cx, cy], 16.0);
            }
            if let Some(kind) = data.get(0).and_then(|i| t.tool_names.get(*i as usize))
                && let Some(def) = t.tools.get(kind)
            {
                game.native_text(batch, &def.name, at(108.0, 6.0), 0x404040, false);
                for (i, p) in def.parts.iter().enumerate() {
                    let [sx, sy] = [[108.0, 20.0], [108.0, 44.0], [108.0, 68.0], [132.0, 32.0], [132.0, 56.0]][i.min(4)];
                    game.native_item(batch, &format!("{M}{}", p.part), at(sx, sy), 16.0);
                }
            }
            game.native_fill(batch, [origin[0] + 160.0, origin[1] + 44.0, 24.0, 16.0], 0xFF70_7070);
            game.native_text(batch, "Go", at(166.0, 48.0), 0xFFFFFF, true);
        }
        _ => {}
    }
}

pub fn screen_click(_game: &Game<'_>, block: &str, data: &[i32], at: [f32; 2], _right: bool, button: &mut Option<i32>) {
    let t = table();
    let [x, y] = at;
    match short(block) {
        Some("smeltery_controller") => {
            if (8.0..56.0).contains(&x) && (18.0..108.0).contains(&y) {
                // The fluids stack from the bottom, so the press picks the one under the mouse.
                let capacity = data.first().copied().unwrap_or(1).max(1) as f32;
                let mut bottom = 108.0;
                for i in 0..16usize {
                    let amount = data.get(7 + i * 2).copied().unwrap_or(0);
                    if amount <= 0 {
                        continue;
                    }
                    let bar = (amount as f32 / capacity * 90.0).max(2.0);
                    if y >= bottom - bar && y < bottom {
                        *button = Some(100 + i as i32);
                        return;
                    }
                    bottom -= bar;
                }
            }
        }
        Some("part_builder") => {
            if (88.0..124.0).contains(&x) && (33.0..53.0).contains(&y) {
                *button = Some(0);
            }
        }
        Some("stencil_table") => {
            if x >= 40.0 && y >= 18.0 {
                let (c, r) = (((x - 40.0) / ROW) as usize, ((y - 18.0) / ROW) as usize);
                let i = c + r * 10;
                if c < 10 && i < t.part_names.len() {
                    *button = Some(i as i32);
                }
            }
        }
        Some("tool_station" | "tool_forge") => {
            if (160.0..184.0).contains(&x) && (44.0..60.0).contains(&y) {
                *button = Some(1000);
                return;
            }
            let forge = block.ends_with("tool_forge");
            if x >= 8.0 && x < 8.0 + TOOL_COLS as f32 * ROW && y >= 18.0 {
                let (c, r) = (((x - 8.0) / ROW) as usize, ((y - 18.0) / ROW) as usize);
                let i = c + r * TOOL_COLS;
                let shown: Vec<_> = t.tool_names.iter().filter(|k| forge || !t.tools[*k].big).collect();
                if let Some(kind) = shown.get(i) {
                    *button = t.tool_names.iter().position(|k| k == *kind).map(|p| p as i32);
                }
            }
        }
        _ => {}
    }
}

pub fn screen_tooltip(_game: &Game<'_>, block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) {
    let t = table();
    let [x, y] = at;
    if short(block) == Some("smeltery_controller") && (8.0..56.0).contains(&x) && (18.0..108.0).contains(&y) {
        let capacity = data.first().copied().unwrap_or(1).max(1) as f32;
        let mut bottom = 108.0;
        for i in 0..16usize {
            let idx = data.get(6 + i * 2).copied().unwrap_or(-1);
            let amount = data.get(7 + i * 2).copied().unwrap_or(0);
            if idx < 0 || amount <= 0 {
                continue;
            }
            let bar = (amount as f32 / capacity * 90.0).max(2.0);
            if y >= bottom - bar && y < bottom {
                let f = t.fluid_names.get(idx as usize).and_then(|n| t.fluids.get(n));
                lines.push(format!("Molten {}", f.map_or("?", |f| f.name.as_str())));
                let mb = amount * 16;
                lines.push(format!("{} ingots  ({} mB)", mb / INGOT, mb));
                return;
            }
            bottom -= bar;
        }
    }
    if short(block) == Some("stencil_table") && x >= 40.0 && y >= 18.0 {
        let (c, r) = (((x - 40.0) / ROW) as usize, ((y - 18.0) / ROW) as usize);
        if c < 10 && let Some(name) = t.part_names.get(c + r * 10) {
            lines.push(t.parts[name].name.clone());
            lines.push(format!("{} ingots", t.parts[name].cost as f32 / 144.0));
        }
    }
}

// --- Drawing what's inside blocks ----------------------------------------------------------------------------------------

use super::native_block_entity_draw::{BlockEntityDraw, CubeFace};

fn molten_cube(game: &Game<'_>, draw: &mut BlockEntityDraw, from: [f32; 3], to: [f32; 3], color: u32) {
    let sprite = "ddsstinkers:block/fluids/molten_metal";
    let face = || Some(CubeFace { sprite, uv: None });
    game.native_draw_cube(draw, from, to, [face(), face(), face(), face(), face(), face()], color);
}

pub fn submit(game: &Game<'_>, _pos: [i32; 3], block: &str, _state: BlockStateId, entity: &ModBlockEntity, draw: &mut BlockEntityDraw) {
    let t = table();
    let Some(id) = short(block) else { return };
    let color_of = |f: &str| t.fluids.get(f).map_or(0xFFFFFF, |x| x.color);
    match id {
        "seared_tank" | "seared_gauge" | "seared_window" => {
            let n = nint(&entity.nbt, "amount");
            if n > 0 {
                let h = 0.1 + 15.8 * (n as f32 / TANK_CAPACITY as f32).min(1.0);
                molten_cube(game, draw, [0.1, 0.1, 0.1], [15.9, h, 15.9], color_of(&nstr(&entity.nbt, "fluid")));
            }
            draw.keep();
        }
        "seared_faucet" => {
            if nint(&entity.nbt, "pouring") == 1 {
                let c = nint(&entity.nbt, "pour_color") as u32;
                molten_cube(game, draw, [6.0, -16.0, 6.0], [10.0, 5.0, 10.0], c);
            }
            draw.keep();
        }
        "casting_table" | "casting_basin" => {
            let table_top = id == "casting_table";
            let n = nint(&entity.nbt, "amount");
            let need = nint(&entity.nbt, "need").max(1);
            if n > 0 {
                let (lo, hi) = if table_top { (15.0, 15.95) } else { (4.0, 15.5) };
                let h = lo + (hi - lo) * (n as f32 / need as f32).min(1.0);
                molten_cube(game, draw, [1.0, lo, 1.0], [15.0, h, 15.0], color_of(&nstr(&entity.nbt, "fluid")));
            }
            for key in ["item0", "item1"] {
                let item = nstr(&entity.nbt, key);
                if item.is_empty() {
                    continue;
                }
                let mut stack = plain(&item, 1);
                if let Some(Tag::Int(c)) = nget(&entity.nbt, &format!("{key}_c")) {
                    stack.data_mut().set_other("minecraft:custom_model_data", Some(Tag::compound([("colors", Tag::IntArray(vec![*c]))])));
                }
                draw.push();
                if table_top {
                    draw.translate([0.5, 1.01, 0.5]);
                    draw.rotate_about([1.0, 0.0, 0.0], 90.0, [0.0, 0.0, 0.0]);
                    draw.scale([0.8, 0.8, 0.8]);
                } else {
                    draw.translate([0.5, 0.75, 0.5]);
                    draw.scale([1.2, 1.2, 1.2]);
                }
                game.native_draw_item(draw, &stack, horizon_assets::item_model::DisplayContext::Fixed, None, 0);
                draw.pop();
            }
        }
        "smeltery_controller" => {
            if nint(&entity.nbt, "valid") != 1 {
                return;
            }
            let fluids = fluids_of(&entity.nbt);
            if fluids.is_empty() {
                return;
            }
            let min = [nint(&entity.nbt, "x0") - _pos[0], nint(&entity.nbt, "y0") - _pos[1], nint(&entity.nbt, "z0") - _pos[2]];
            let max = [nint(&entity.nbt, "x1") - _pos[0] + 1, nint(&entity.nbt, "y1") - _pos[1] + 1, nint(&entity.nbt, "z1") - _pos[2] + 1];
            let volume = (max[0] - min[0]) * (max[1] - min[1]) * (max[2] - min[2]);
            let capacity = (volume * 2880).max(1) as f32;
            let height = (max[1] - min[1]) as f32;
            let mut y = min[1] as f32;
            let total: i32 = fluids.iter().map(|(_, a)| *a).sum();
            let _ = total;
            for (f, a) in &fluids {
                let h = height * *a as f32 / capacity;
                molten_cube(game, draw, [min[0] as f32 * 16.0, y * 16.0, min[2] as f32 * 16.0], [max[0] as f32 * 16.0, (y + h) * 16.0, max[2] as f32 * 16.0], color_of(f));
                y += h;
            }
        }
        _ => {}
    }
}

pub fn render_box(_game: &Game<'_>, pos: [i32; 3], block: &str, _state: BlockStateId, entity: &ModBlockEntity, bounds: &mut [f32; 6]) {
    if short(block) == Some("smeltery_controller") && nint(&entity.nbt, "valid") == 1 {
        *bounds = [
            (nint(&entity.nbt, "x0") - pos[0] - 1) as f32,
            (nint(&entity.nbt, "y0") - pos[1] - 1) as f32,
            (nint(&entity.nbt, "z0") - pos[2] - 1) as f32,
            (nint(&entity.nbt, "x1") - pos[0] + 2) as f32,
            (nint(&entity.nbt, "y1") - pos[1] + 2) as f32,
            (nint(&entity.nbt, "z1") - pos[2] + 2) as f32,
        ];
    } else if short(block) == Some("seared_faucet") {
        bounds[1] = -2.0;
    }
}

#[allow(dead_code)]
fn unused(_: Direction) {}

// --- /tinkers ------------------------------------------------------------------------------------------------------------

pub fn register_commands(commands: &mut Vec<horizon_client::mod_api::NativeCommand>) {
    commands.push(horizon_client::mod_api::NativeCommand::new("tinkers <action:word> <what:word> [a:word] [b:word] [c:word] [d:word]"));
}

/// `tinkers part pick_head iron`, `tinkers tool pickaxe wood iron iron`, `tinkers info X,Y,Z`.
pub fn perform_command(game: &mut Game<'_>, command: &str, ran: &mut bool) {
    let mut w = command.split_whitespace();
    if w.next() != Some("tinkers") {
        return;
    }
    *ran = true;
    let action = w.next().unwrap_or("");
    let what = w.next().unwrap_or("");
    let rest: Vec<String> = w.map(str::to_owned).collect();
    let made = match action {
        "part" => rest.first().filter(|m| table().materials.contains_key(*m) && table().parts.contains_key(what)).map(|m| make_part(what, m)),
        "tool" => make_tool(what, &rest),
        "info" => {
            let p: Vec<i32> = what.split(',').filter_map(|v| v.parse().ok()).collect();
            if p.len() == 3 {
                let pos = [p[0], p[1], p[2]];
                let text = game.native_with_mod_block_entity(pos, |e| {
                    format!("{} fluids={:?} amount={} fluid={} valid={} temp={} on={} items={:?}", e.id, fluids_of(&e.nbt), nint(&e.nbt, "amount"), nstr(&e.nbt, "fluid"), nint(&e.nbt, "valid"), nint(&e.nbt, "temp"), nint(&e.nbt, "on"),
                            e.items.iter().flatten().map(|s| (s.item.to_string(), s.count)).collect::<Vec<_>>())
                });
                game.native_feedback(Text::literal(text.unwrap_or_else(|| "no Tinkers block there".to_owned())));
            }
            return;
        }
        _ => None,
    };
    match made {
        Some(stack) => {
            give_hand(game, stack);
            game.native_feedback(Text::literal("Given"));
        }
        None => game.native_feedback(Text::literal("Usage: /tinkers part <part> <material> | /tinkers tool <kind> <materials...>")),
    }
}

// --- Use: the special tools ----------------------------------------------------------------------------------------------

/// Right-click in the air. Rapier: a leap back. Laser gun: a beam at the first mob ahead. Slime sling: a leap the way you look.
pub fn use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if std::env::var_os("DDSSTINKERS_DEBUG").is_some() {
        eprintln!("[tinkers] use_item {} serving={}", stack.item, serving(game));
    }
    if !serving(game) {
        return;
    }
    if traits::use_in_air(game, stack) {
        *claimed = true;
        return;
    }
    let Some(id) = short(&stack.item) else { return };
    let Some(p) = game.player.as_ref() else { return };
    let (yaw, pitch) = (p.yaw.to_radians(), p.pitch.to_radians());
    let look = [-(yaw.sin() * pitch.cos()) as f64, -pitch.sin() as f64, (yaw.cos() * pitch.cos()) as f64];
    let eye = [p.position.x, p.position.y + 1.6, p.position.z];
    match id {
        "rapier" | "slimesling" => {
            let k = if id == "rapier" { -1.2 } else { 1.8 };
            let name = game.native_acting_player().map(|a| a.name).unwrap_or_default();
            traits::queue_pub(format!("execute as {name} run tp @s ~ ~0.2 ~"));
            let dir = [look[0] * k, (look[1] * k).max(0.3), look[2] * k];
            if let Some(pl) = game.player.as_mut() {
                pl.velocity.x += dir[0];
                pl.velocity.y += dir[1];
                pl.velocity.z += dir[2];
            }
            if id == "rapier" {
                game.damage_held(1);
            }
            *claimed = true;
        }
        "laser_gun" => {
            let atk = ti::tool_tag(stack).and_then(|t| ti::float(t, "atk")).unwrap_or(1.0);
            let mut best: Option<(u32, f64)> = None;
            for m in game.mobs.iter().filter(|m| !m.removed && m.alive()) {
                let v = [m.position.x - eye[0], m.position.y + 1.0 - eye[1], m.position.z - eye[2]];
                let along = v[0] * look[0] + v[1] * look[1] + v[2] * look[2];
                if !(0.0..64.0).contains(&along) {
                    continue;
                }
                let off = [v[0] - look[0] * along, v[1] - look[1] * along, v[2] - look[2] * along];
                if off[0] * off[0] + off[1] * off[1] + off[2] * off[2] < 1.5 && best.is_none_or(|b| along < b.1) {
                    best = Some((m.id, along));
                }
            }
            if let Some((target, _)) = best
                && let Some(m) = game.mobs.iter_mut().find(|m| m.id == target)
            {
                let hp = m.health - (atk * 1.5 + 2.0);
                if hp > 0.0 {
                    m.health = hp;
                    m.needs_sync = true;
                } else {
                    let dim = game.native_dimension().to_owned();
                    game.native_kill_mob(&dim, target);
                }
            }
            game.damage_held(1);
            *claimed = true;
        }
        _ => {}
    }
}
