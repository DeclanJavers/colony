//! What people do (ARCHITECTURE.md section 5). Each kind of action knows how to offer concrete
//! options from what a person believes (with features for the scorer and words for text brains),
//! and how to carry itself out in phases: walk somewhere, work, bring things back.

use crate::brain::{Choice, DecisionRequest, Ending};
use crate::camp::Camp;
use crate::content::Content;
use crate::events::Event;
use crate::ids::{Id, Table};
use crate::path::{Pathfinder, tile_cost, walk_pace};
use crate::people::{Person, PersonId, TRAITS};
use crate::rng::Rng;
use crate::world::{Map, Node, Water};
use serde::{Deserialize, Serialize};

/// Which animation to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anim {
    Idle,
    Walk,
    Gather,
    Chop,
    Mine,
    Drink,
    DrawWater,
    Eat,
    Sleep,
    Sit,
    Build,
}

impl Anim {
    /// The villager model's clip for this animation.
    pub fn clip(self) -> &'static str {
        match self {
            Anim::Idle => "idle",
            Anim::Walk => "walk",
            Anim::Gather => "gather",
            Anim::Chop => "chop",
            Anim::Mine => "mine",
            Anim::Drink => "drink",
            Anim::DrawWater => "draw_water",
            Anim::Eat => "eat",
            Anim::Sleep => "lie_down",
            Anim::Sit => "sit",
            Anim::Build => "build",
        }
    }
}

/// What was chosen (the target is part of the choice).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Gather { def: u16, node: Id<Node> },
    Drink { x: u16, y: u16 },
    DrinkStored,
    FetchWater { x: u16, y: u16 },
    Eat,
    /// Eat berries straight from a bush.
    EatAt { node: Id<Node> },
    Sleep,
    /// Sleep rough wherever they are (no fire).
    SleepHere,
    Rest,
    LightFire,
    TendFire,
    Explore { x: u16, y: u16 },
    CheckStores,
}

impl Kind {
    /// Short name, used for feature names and stats.
    pub fn name(&self, c: &Content) -> String {
        match self {
            Kind::Gather { def, .. } => c.actions.gather[*def as usize].id.clone(),
            Kind::Drink { .. } => "drink".into(),
            Kind::DrinkStored => "drink_stored".into(),
            Kind::FetchWater { .. } => "fetch_water".into(),
            Kind::Eat => "eat".into(),
            Kind::EatAt { .. } => "eat_at_bush".into(),
            Kind::Sleep => "sleep".into(),
            Kind::SleepHere => "sleep_here".into(),
            Kind::Rest => "rest".into(),
            Kind::LightFire => "light_fire".into(),
            Kind::TendFire => "tend_fire".into(),
            Kind::Explore { .. } => "explore".into(),
            Kind::CheckStores => "check_stores".into(),
        }
    }

    /// The need this action is for, if any (an urgent other need may interrupt it).
    fn eases(&self, c: &Content) -> Option<usize> {
        match self {
            Kind::Eat | Kind::EatAt { .. } => Some(c.need("hunger")),
            Kind::Drink { .. } | Kind::DrinkStored => Some(c.need("thirst")),
            Kind::Sleep | Kind::SleepHere => Some(c.need("rest")),
            Kind::Rest => Some(c.need("fun")),
            Kind::LightFire | Kind::TendFire => Some(c.need("warmth")),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub kind: Kind,
    pub label: String,
    pub phase: u8,
    pub path: Vec<(u16, u16)>,
    pub path_i: usize,
    pub path_ready: bool,
    pub work_left: f32,
    pub started: u64,
    pub anim: Anim,
    pub needs_start: Vec<f32>,
    pub stored: Vec<u32>,
    /// Current walking pace (eases towards the ground's pace instead of jumping).
    #[serde(default = "one")]
    pub pace: f32,
}

fn one() -> f32 {
    1.0
}

/// Everything an action touches besides the person.
pub struct Ctx<'a> {
    pub c: &'a Content,
    pub map: &'a mut Map,
    pub nodes: &'a mut Table<Node>,
    pub camp: &'a mut Camp,
    pub path: &'a mut Pathfinder,
    pub rng: &'a mut Rng,
    pub tick: u64,
    pub events: &'a mut Vec<Event>,
    pub alive: usize,
    /// This person's slot among the living (for their place by the fire).
    pub slot: usize,
}

// ---------------------------------------------------------------------------------------------
// Words

fn direction(from: (f32, f32), to: (f32, f32)) -> &'static str {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if dx * dx + dy * dy < 9.0 {
        return "right here";
    }
    // The map's +y is south.
    let a = dy.atan2(dx).to_degrees();
    match a {
        a if (-22.5..22.5).contains(&a) => "to the east",
        a if (22.5..67.5).contains(&a) => "to the south-east",
        a if (67.5..112.5).contains(&a) => "to the south",
        a if (112.5..157.5).contains(&a) => "to the south-west",
        a if (-67.5..-22.5).contains(&a) => "to the north-east",
        a if (-112.5..-67.5).contains(&a) => "to the north",
        a if (-157.5..-112.5).contains(&a) => "to the north-west",
        _ => "to the west",
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn centre(t: (u16, u16)) -> (f32, f32) {
    (t.0 as f32 + 0.5, t.1 as f32 + 0.5)
}

/// Air temperature at a tile now: the map's mean, the season, and the day's swing (warmest at
/// 15:00, coldest at 03:00).
pub fn air_temp(c: &Content, map: &Map, x: u32, y: u32, tick: u64) -> f32 {
    let cal = &c.calendar;
    let i = map.idx(x.min(map.size - 1), y.min(map.size - 1));
    let hour = cal.minute_of_day(tick) as f32 / 60.0;
    let swing = ((hour - 15.0) / 24.0 * std::f32::consts::TAU).cos() * c.people.day_night_swing;
    map.temperature[i] + c.people.season_temp[cal.season_index(tick)] + swing
}

/// How strongly someone's body clock says "sleep" now: 1 at night, 0 by day, with an hour's ramp.
pub fn sleepy(c: &Content, tick: u64, chronotype: f32) -> f32 {
    let hour = c.calendar.minute_of_day(tick) as f32 / 60.0;
    let h = (hour - chronotype).rem_euclid(24.0);
    let (s, w) = (c.people.sleep_hour, c.people.wake_hour);
    let ramp = |x: f32| x.clamp(0.0, 1.0);
    if h >= s - 1.0 {
        ramp(h - (s - 1.0))
    } else if h < w {
        1.0
    } else {
        ramp(1.0 - (h - w))
    }
}

// ---------------------------------------------------------------------------------------------
// Offering options

const ITEMS: [&str; 5] = ["berries", "water", "wood", "sticks", "stone"];

/// Berries one person eats in a day.
fn daily_berries(c: &Content) -> f32 {
    let h = &c.needs[c.need("hunger")];
    (h.awake_per_hour * 16.0 + h.asleep_per_hour * 8.0) / c.items[c.item("berries")].food
}

/// The situation as named numbers: needs, the stores as they believe them, how much each item is
/// worth to the group right now, time, cold, personality, skills.
fn state_features(p: &Person, ctx: &Ctx) -> Vec<(String, f32)> {
    let c = ctx.c;
    let cal = &c.calendar;
    let mut f: Vec<(String, f32)> = Vec::new();
    for (i, n) in c.needs.iter().enumerate() {
        f.push((format!("need.{}", n.id), p.needs[i]));
    }
    f.push(("health".into(), p.health));
    let hour = cal.minute_of_day(ctx.tick) as f32 / 60.0;
    f.push(("hour".into(), hour / 24.0));
    f.push(("sleepy".into(), sleepy(c, ctx.tick, p.chronotype)));
    f.push(("evening".into(), ((hour - 15.0) / 4.0).clamp(0.0, 1.0) * if hour < 22.0 { 1.0 } else { 0.0 }));
    let season = cal.season_index(ctx.tick);
    for (k, s) in cal.seasons.iter().enumerate() {
        f.push((s.clone(), (k == season) as u8 as f32));
    }
    let temp = air_temp(c, ctx.map, p.pos.0 as u32, p.pos.1 as u32, ctx.tick);
    f.push(("temp".into(), temp / 20.0));
    f.push(("cold".into(), ((c.people.comfort_temp - temp) / 15.0).clamp(0.0, 1.0)));
    let b = &p.knowledge.stores;
    let n = ctx.alive.max(1) as f32;
    for it in ITEMS {
        let i = c.item(it);
        f.push((format!("stores.{it}"), b.items[i] as f32 / n));
        f.push((format!("carried.{it}"), p.carry[i] as f32));
    }
    f.push(("fire_lit".into(), b.fire_lit as u8 as f32));
    f.push(("fire_hours".into(), if b.fire_lit { b.fire_hours } else { 0.0 }));
    // What each item is worth to the group now (shortage against a target for the season).
    let food_days = (b.items[c.item("berries")] + p.carry[c.item("berries")]) as f32 / (n * daily_berries(c));
    let food_target = c.actions.food_target_days[season.min(3)];
    let food_short = (1.0 - food_days / food_target).clamp(0.0, 1.0);
    let fuel = b.items[c.item("wood")] as f32 * c.items[c.item("wood")].fuel
        + b.items[c.item("sticks")] as f32 * c.items[c.item("sticks")].fuel
        + if b.fire_lit { b.fire_hours } else { 0.0 };
    // How cold tonight will be at the camp: fuel matters most when nights are cold.
    let (cx, cy) = ctx.camp.stores_at;
    let ci = ctx.map.idx(cx as u32, cy as u32);
    let night_temp = ctx.map.temperature[ci] + c.people.season_temp[season] - c.people.day_night_swing;
    let night_cold = ((c.people.comfort_temp - night_temp) / 10.0).clamp(0.0, 1.5);
    let fuel_target = 6.0 + night_cold * [14.0, 12.0, 30.0, 40.0][season.min(3)];
    let fuel_short = (1.0 - fuel / fuel_target).clamp(0.0, 1.0);
    let water_short = (1.0 - b.items[c.item("water")] as f32 / (1.5 * n)).clamp(0.0, 1.0);
    // Without kindling there's no fire: sticks matter a lot then, more so before cold nights.
    let kindling = if b.items[c.item("sticks")] < 3 { 0.05 + 0.12 * night_cold } else { 0.0 };
    f.push(("value.berries".into(), 0.12 * food_short + 0.01));
    f.push(("value.water".into(), 0.04 * water_short));
    f.push(("value.wood".into(), (0.04 + 0.1 * night_cold) * fuel_short + 0.003));
    f.push(("value.sticks".into(), (0.015 + 0.03 * night_cold) * fuel_short + 0.002 + kindling));
    f.push(("night_cold".into(), night_cold));
    f.push(("value.stone".into(), 0.004));
    f.push(("food_days".into(), food_days.min(20.0)));
    f.push(("fuel_hours".into(), fuel.min(100.0)));
    f.push(("winter".into(), (season == 3) as u8 as f32 + (season == 2) as u8 as f32 * 0.5));
    let known_food = p.knowledge.nodes.values().filter(|k| k.kind == c.node_kind("berry_bush") && k.amount >= 1.0).count();
    f.push(("food_unknown".into(), if known_food < 3 { 1.0 } else { 0.0 }));
    f.push(("explored".into(), p.knowledge.explored_share()));
    f.push(("stores_age_hours".into(), ((ctx.tick.saturating_sub(b.when)) as f32 / 60.0).min(240.0)));
    for (t, v) in TRAITS.iter().zip(p.personality) {
        f.push((format!("trait.{t}"), v));
    }
    for (i, s) in c.skills.skills.iter().enumerate() {
        f.push((format!("skill.{}", s.id), p.skill_level(i, c) as f32 / 10.0));
    }
    f.push(("people".into(), n / 20.0));
    f
}

fn choice(label: String, kind: Kind, c: &Content, features: Vec<(&str, f32)>) -> Choice {
    let mut f: Vec<(String, f32)> = vec![(format!("act.{}", kind.name(c)), 1.0)];
    f.extend(features.into_iter().map(|(k, v)| (k.to_string(), v)));
    Choice { label, kind, features: f, possible: true, why_not: None }
}

fn impossible(label: &str, kind: Kind, why: &str) -> Choice {
    Choice { label: label.into(), kind, features: Vec::new(), possible: false, why_not: Some(why.into()) }
}

/// Everything this person could do now, from what they believe.
pub fn request(p: &Person, id: PersonId, ctx: &Ctx) -> DecisionRequest {
    let c = ctx.c;
    let speed = c.people.walk_speed;
    let state = state_features(p, ctx);
    let get = |name: &str| state.iter().find(|(k, _)| k == name).map_or(0.0, |(_, v)| *v);
    let mut opts: Vec<Choice> = Vec::new();
    let camp = ctx.camp.stores_pos();
    let to_camp = dist(p.pos, camp);
    let b = &p.knowledge.stores;
    let walk = |d: f32| d * 1.3 / speed;
    let need = |n: &str| p.needs[c.need(n)];

    // Gathering
    for (gi, g) in c.actions.gather.iter().enumerate() {
        let kind = c.node_kind(&g.node);
        let skill = c.skill(&g.skill);
        let near = p.knowledge.nearest_nodes(kind, p.pos, 2);
        if near.is_empty() {
            if gi == 0 {
                opts.push(impossible(&g.name, Kind::Gather { def: gi as u16, node: Id::default() }, &format!("no {} known", c.nodes[kind as usize].name.to_lowercase())));
            }
            continue;
        }
        for (nid, kn, d) in near {
            let at = centre((kn.x, kn.y));
            let carry = (c.people.carry - p.carried().min(c.people.carry)) as f32;
            // What they saw may be out of date (others pick too): trust it less as it ages.
            let fresh = (-((ctx.tick.saturating_sub(kn.when)) as f32 / 60.0) / 18.0).exp();
            let expect = carry.min(kn.amount.floor()) * (0.35 + 0.65 * fresh);
            let batches = (expect / g.yield_per as f32).ceil();
            let work = batches * g.minutes as f32 / p.work_speed(skill, c);
            let minutes = walk(d) + work + walk(dist(at, camp));
            let label = format!("{} at the {} {}", g.name, c.nodes[kind as usize].name.to_lowercase(), direction(p.pos, at));
            opts.push(choice(label, Kind::Gather { def: gi as u16, node: nid }, c, vec![
                ("dist", d),
                ("minutes", minutes),
                ("amount", kn.amount),
                ("fresh", fresh),
                (&*format!("gain.{}", g.item), expect),
                ("skill", p.skill_level(skill, c) as f32 / 10.0),
            ]));
        }
    }
    // Water
    for (w, d) in p.knowledge.nearest_water(p.pos, 1) {
        let at = centre((w.x, w.y));
        let dir = direction(p.pos, at);
        opts.push(choice(format!("Drink at the water {dir}"), Kind::Drink { x: w.x, y: w.y }, c, vec![
            ("dist", d),
            ("minutes", walk(d) + c.actions.drink_minutes as f32),
            ("relief.thirst", need("thirst")),
        ]));
        let amount = c.actions.fetch_water.amount.min(c.people.carry) as f32;
        opts.push(choice(format!("Fetch water for the camp (from the water {dir})"), Kind::FetchWater { x: w.x, y: w.y }, c, vec![
            ("dist", d),
            ("minutes", walk(d) + c.actions.fetch_water.minutes as f32 + walk(dist(at, camp))),
            ("gain.water", amount),
            ("relief.thirst", need("thirst") * 0.5),
        ]));
    }
    if p.knowledge.water.is_empty() {
        opts.push(impossible("Drink", Kind::Drink { x: 0, y: 0 }, "no fresh water known"));
    }
    let water = c.item("water");
    if b.items[water] > 0 {
        let have = b.items[water] as f32 * c.items[water].drink;
        opts.push(choice("Drink from the water at camp".into(), Kind::DrinkStored, c, vec![
            ("dist", to_camp),
            ("minutes", walk(to_camp) + c.actions.drink_minutes as f32),
            ("relief.thirst", need("thirst").min(have)),
            ("gain.water", -(need("thirst") / c.items[water].drink).ceil().min(b.items[water] as f32)),
        ]));
    }
    // Food
    let berries = c.item("berries");
    let food = p.carry[berries] + b.items[berries];
    if food > 0 {
        let d = if p.carry[berries] > 0 { 0.0 } else { to_camp };
        opts.push(choice("Eat".into(), Kind::Eat, c, vec![
            ("dist", d),
            ("minutes", walk(d) + c.actions.eat_minutes as f32),
            ("relief.hunger", need("hunger").min(food as f32 * c.items[berries].food)),
            ("gain.berries", -(need("hunger") / c.items[berries].food).ceil().min(b.items[berries] as f32)),
        ]));
    } else {
        opts.push(impossible("Eat", Kind::Eat, "no food in the stores or in hand"));
    }
    // Eat as they pick: berries straight from a bush they know.
    let bush = c.node_kind("berry_bush");
    if let Some((nid, kn, d)) = p.knowledge.nearest_nodes(bush, p.pos, 1).into_iter().next() {
        let at = centre((kn.x, kn.y));
        let fresh = (-((ctx.tick.saturating_sub(kn.when)) as f32 / 60.0) / 18.0).exp();
        opts.push(choice(format!("Eat berries at the bush {}", direction(p.pos, at)), Kind::EatAt { node: nid }, c, vec![
            ("dist", d),
            ("minutes", walk(d) + c.actions.eat_minutes as f32 * 2.0),
            ("relief.hunger", need("hunger").min(kn.amount * c.items[berries].food) * (0.4 + 0.6 * fresh)),
            ("fresh", fresh),
        ]));
    }
    // Sleep and rest (by the fire, which warms if lit)
    let fire_warm = if b.fire_lit { need("warmth") } else { 0.0 };
    let to_fire = dist(p.pos, ctx.camp.fire_pos());
    opts.push(choice("Sleep by the fire".into(), Kind::Sleep, c, vec![
        ("dist", to_fire),
        ("minutes", walk(to_fire)),
        ("relief.rest", need("rest")),
        ("relief.warmth", fire_warm),
    ]));
    let here = ctx.map.idx((p.pos.0 as u32).min(ctx.map.size - 1), (p.pos.1 as u32).min(ctx.map.size - 1));
    if to_fire > 25.0 && !ctx.map.water[here].is_water() {
        // Sleeping rough: no fire, so the cold counts against it.
        let exposure = get("cold") * 0.8 + need("warmth") * 0.5;
        opts.push(choice("Sleep where you are (no fire)".into(), Kind::SleepHere, c, vec![
            ("dist", 0.0),
            ("minutes", 0.0),
            ("relief.rest", need("rest")),
            ("relief.warmth", -exposure),
        ]));
    }
    opts.push(choice("Rest by the fire".into(), Kind::Rest, c, vec![
        ("dist", to_fire),
        ("minutes", walk(to_fire) + c.actions.rest.minutes as f32),
        ("relief.fun", need("fun").min(c.actions.rest.fun_per_hour)),
        ("relief.rest", need("rest").min(c.actions.rest.rest_per_hour)),
        ("relief.social", need("social").min(0.1)),
        ("relief.warmth", fire_warm),
    ]));
    // The fire
    let lf = &c.actions.light_fire;
    let (sticks, wood) = (c.item("sticks"), c.item("wood"));
    if !b.fire_lit {
        let kindling = b.items[sticks] >= lf.sticks && b.items[wood] >= lf.wood;
        // Without sticks, kindling can be shaved from two logs (harder to catch).
        let shaved = !kindling && b.items[wood] >= 2;
        if kindling || shaved {
            let skill = c.skill("firemaking");
            let chance = (lf.chance + lf.chance_per_level * p.skill_level(skill, c) as f32).min(0.98) * if shaved { 0.6 } else { 1.0 };
            opts.push(choice(if shaved { "Light the fire (shaving kindling from wood)".into() } else { "Light the fire".into() }, Kind::LightFire, c, vec![
                ("dist", to_fire),
                ("minutes", walk(to_fire) + lf.minutes as f32 * if shaved { 1.5 } else { 1.0 }),
                ("chance", chance),
                ("relief.warmth", need("warmth") * 0.5),
            ]));
        } else {
            opts.push(impossible("Light the fire", Kind::LightFire, &format!("needs {} sticks (or 2 wood) in the stores", lf.sticks)));
        }
    } else if b.fire_hours < 8.0 && b.items[wood] + b.items[sticks] > 0 {
        opts.push(choice("Add wood to the fire".into(), Kind::TendFire, c, vec![
            ("dist", to_fire),
            ("minutes", walk(to_fire) + c.actions.tend_fire_minutes as f32),
            ("relief.warmth", need("warmth") * 0.3),
        ]));
    }
    // Exploring
    if let Some(t) = explore_target(p, ctx) {
        let at = centre(t);
        let d = dist(p.pos, at);
        opts.push(choice(format!("Explore {}", direction(p.pos, at)), Kind::Explore { x: t.0, y: t.1 }, c, vec![
            ("dist", d),
            ("minutes", walk(d)),
            ("unexplored", 1.0 - p.knowledge.explored_share()),
        ]));
    }
    if get("stores_age_hours") > 6.0 {
        opts.push(choice("Check the stores".into(), Kind::CheckStores, c, vec![
            ("dist", to_camp),
            ("minutes", walk(to_camp) + c.actions.check_stores_minutes as f32),
        ]));
    }
    let text = describe(p, ctx, &state, &opts);
    DecisionRequest { person: id, tick: ctx.tick, state, options: opts, text }
}

/// Nearest unexplored land cell (its centre), by straight-line distance.
fn explore_target(p: &Person, ctx: &Ctx) -> Option<(u16, u16)> {
    let n = ctx.map.size;
    let cells = n.div_ceil(crate::people::knowledge::CELL);
    let mut best: Option<(f32, (u16, u16))> = None;
    for cy in 0..cells {
        for cx in 0..cells {
            if p.knowledge.explored[(cy * cells + cx) as usize] {
                continue;
            }
            let cell = crate::people::knowledge::CELL;
            let (x, y) = ((cx * cell + cell / 2).min(n - 1), (cy * cell + cell / 2).min(n - 1));
            if tile_cost(ctx.map, ctx.map.idx(x, y)).is_none() {
                continue;
            }
            let d = dist(p.pos, (x as f32 + 0.5, y as f32 + 0.5));
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, (x as u16, y as u16)));
            }
        }
    }
    best.map(|(_, t)| t)
}

/// The request in words, as a text brain would see it.
fn describe(p: &Person, ctx: &Ctx, state: &[(String, f32)], opts: &[Choice]) -> String {
    let c = ctx.c;
    let cal = &c.calendar;
    let get = |name: &str| state.iter().find(|(k, _)| k == name).map_or(0.0, |(_, v)| *v);
    let age = p.age_years(ctx.tick, c) as u32;
    let who = if p.sex == crate::people::Sex::Female { "a woman" } else { "a man" };
    let traits: Vec<&str> = TRAITS
        .iter()
        .zip(p.personality)
        .filter_map(|(t, v)| {
            let (hi, lo) = match *t {
                "curiosity" => ("curious", "set in your ways"),
                "diligence" => ("hard-working", "easy-going"),
                "sociability" => ("sociable", "private"),
                "warmth" => ("kind", "blunt"),
                _ => ("calm", "excitable"),
            };
            if v > 0.68 { Some(hi) } else if v < 0.32 { Some(lo) } else { None }
        })
        .collect();
    let m = cal.minute_of_day(ctx.tick);
    let temp = get("temp") * 20.0;
    let feel: Vec<String> = c.needs.iter().enumerate().filter(|(i, _)| p.needs[*i] >= 0.25).map(|(i, n)| n.word(p.needs[i]).to_string()).collect();
    let b = &p.knowledge.stores;
    let list = |v: &[u32]| {
        let parts: Vec<String> = c.items.iter().zip(v).filter(|(_, n)| **n > 0).map(|(it, n)| format!("{n} {}", it.name.to_lowercase())).collect();
        if parts.is_empty() { "nothing".to_string() } else { parts.join(", ") }
    };
    let fire = if b.fire_lit { format!("lit, about {:.0} hours of fuel left", b.fire_hours) } else { "out".to_string() };
    let mut s = format!(
        "You are {}, {age}, {who} in a group of {} foragers who have just settled by fresh water.{}\n",
        p.name,
        ctx.alive,
        if traits.is_empty() { String::new() } else { format!(" You are {}.", traits.join(", ")) }
    );
    s += &format!("It is {}, day {}, {:02}:{:02}, {:.0} C.\n", cal.season(ctx.tick), cal.day_of_season(ctx.tick), m / 60, m % 60, temp);
    s += &format!("You feel: {}.\n", if feel.is_empty() { "fine".to_string() } else { feel.join(", ") });
    s += &format!("You carry: {}.\n", list(&p.carry));
    s += &format!("The camp stores (as you last saw them): {}. The fire is {fire}.\n", list(&b.items));
    s += &format!("Food for about {:.1} days.\n\nWhat do you do next? Answer with a number and one sentence why.\n", get("food_days"));
    for (k, o) in (1..).zip(opts.iter().filter(|o| o.possible)) {
        s += &format!("{k}. {} (about {:.0} min)\n", o.label, o.f("minutes"));
    }
    s
}

/// A first-person line about the choice, from what drove it.
pub fn thought(req: &DecisionRequest, o: &Choice, c: &Content) -> String {
    let need = |n: &str| req.s(&format!("need.{n}"));
    let low_food = req.s("food_days") < 2.0;
    match &o.kind {
        Kind::Gather { def, .. } => {
            let g = &c.actions.gather[*def as usize];
            match g.item.as_str() {
                "berries" if low_food => "We're running low on food. I'll go and pick berries.".into(),
                "berries" => "A few more berries in the stores won't hurt.".into(),
                "wood" | "sticks" if req.s("winter") > 0.0 => "The cold is coming. We'll need wood for the fire.".into(),
                "wood" | "sticks" => "The fire needs feeding. I'll fetch some wood.".into(),
                _ => "Stone might come in handy.".into(),
            }
        }
        Kind::Drink { .. } | Kind::DrinkStored => if need("thirst") > 0.6 { "My throat is dry. I need water.".into() } else { "I'll have a drink.".into() },
        Kind::FetchWater { .. } => "The camp could use more water.".into(),
        Kind::Eat => if need("hunger") > 0.6 { "I'm so hungry. Time to eat.".into() } else { "I'll eat something.".into() },
        Kind::EatAt { .. } => "Those berries look good. I'll eat a few right off the bush.".into(),
        Kind::SleepHere => "Camp is too far. I'll sleep here tonight.".into(),
        Kind::Sleep => if req.s("sleepy") > 0.5 { "It's late. Time to sleep.".into() } else { "I'm worn out. I need to lie down.".into() },
        Kind::Rest => "I'll sit by the fire for a while.".into(),
        Kind::LightFire => if req.s("cold") > 0.3 { "It's cold. We need a fire.".into() } else { "We should get a fire going before dark.".into() },
        Kind::TendFire => "The fire's getting low. I'll add some wood.".into(),
        Kind::Explore { .. } => if req.s("food_unknown") > 0.0 { "We need to find more food. I'll look around.".into() } else { "I wonder what's out there.".into() },
        Kind::CheckStores => "I'll see what's left in the stores.".into(),
    }
}

// ---------------------------------------------------------------------------------------------
// Doing

pub fn start(p: &Person, choice: &Choice, tick: u64) -> Action {
    Action {
        kind: choice.kind.clone(),
        label: choice.label.clone(),
        phase: 0,
        path: Vec::new(),
        path_i: 0,
        path_ready: false,
        work_left: 0.0,
        started: tick,
        anim: Anim::Walk,
        needs_start: p.needs.clone(),
        stored: vec![0; p.carry.len()],
        pace: 1.0,
    }
}

enum Walk {
    Moving,
    Arrived,
    NoPath,
}

/// Walk towards `to` (or next to it). Finds the path on the first call.
fn walk(p: &mut Person, a: &mut Action, ctx: &mut Ctx, to: (u16, u16), beside: bool) -> Walk {
    let here = (p.pos.0 as u32, p.pos.1 as u32);
    if !a.path_ready {
        a.path_ready = true;
        a.path_i = 0;
        if here == (to.0 as u32, to.1 as u32) {
            a.path.clear();
        } else {
            match ctx.path.find(ctx.map, here, (to.0 as u32, to.1 as u32), 60_000) {
                Some(mut path) => {
                    if beside && path.len() > 1 && path.last() == Some(&to) {
                        path.pop();
                    }
                    a.path = path;
                }
                None => return Walk::NoPath,
            }
        }
    }
    a.anim = Anim::Walk;
    // Pace eases towards what the ground allows (wading in, climbing out), over a few minutes.
    let here = ctx.map.idx((p.pos.0 as u32).min(ctx.map.size - 1), (p.pos.1 as u32).min(ctx.map.size - 1));
    let ahead = a.path.get(a.path_i).map_or(here, |t| ctx.map.idx(t.0 as u32, t.1 as u32));
    let want = 0.5 * (walk_pace(ctx.map, here) + walk_pace(ctx.map, ahead));
    a.pace += (want - a.pace) * 0.35;
    let mut budget = ctx.c.people.walk_speed * ctx.c.calendar.minutes_per_tick as f32 * a.pace;
    while budget > 0.0 {
        let Some(&next) = a.path.get(a.path_i) else {
            a.anim = Anim::Idle;
            return Walk::Arrived;
        };
        let target = centre(next);
        let (dx, dy) = (target.0 - p.pos.0, target.1 - p.pos.1);
        let d = (dx * dx + dy * dy).sqrt();
        let step = budget;
        if d > 1e-4 {
            p.facing = dx.atan2(dy);
        }
        if step >= d {
            p.pos = target;
            budget -= d;
            a.path_i += 1;
            let i = ctx.map.idx(next.0 as u32, next.1 as u32);
            ctx.map.footfall[i] += 1.0;
        } else {
            p.pos = (p.pos.0 + dx / d * step, p.pos.1 + dy / d * step);
            budget = 0.0;
        }
    }
    Walk::Moving
}

fn face(p: &mut Person, at: (f32, f32)) {
    let (dx, dy) = (at.0 - p.pos.0, at.1 - p.pos.1);
    if dx * dx + dy * dy > 1e-4 {
        p.facing = dx.atan2(dy);
    }
}

/// Spend this tick on work; true when the work is done.
fn work(a: &mut Action, minutes: f32, anim: Anim, tick_minutes: f32) -> bool {
    if a.work_left <= 0.0 {
        a.work_left = minutes;
    }
    a.anim = anim;
    a.work_left -= tick_minutes;
    if a.work_left <= 0.0 {
        a.work_left = 0.0;
        true
    } else {
        false
    }
}

fn deliver(p: &mut Person, a: &mut Action, camp: &mut Camp) {
    for (i, n) in p.carry.iter_mut().enumerate() {
        camp.stores[i] += *n;
        a.stored[i] += *n;
        *n = 0;
    }
}

/// One tick of the person's current action. Returns how it ended, if it did.
pub fn step(p: &mut Person, a: &mut Action, ctx: &mut Ctx) -> Option<Ending> {
    let c = ctx.c;
    let tm = c.calendar.minutes_per_tick as f32;
    let camp_tile = ctx.camp.stores_at;
    let fire_tile = ctx.camp.fire_at;
    let no_path = |what: &str| Some(Ending::Failed(format!("couldn't find a way to {what}")));
    match a.kind.clone() {
        Kind::Gather { def, node } => {
            let g = &c.actions.gather[def as usize];
            let item = c.item(&g.item);
            let skill = c.skill(&g.skill);
            match a.phase {
                0 => {
                    let Some(n) = ctx.nodes.get(node) else {
                        p.knowledge.forget_node(node);
                        return Some(Ending::Failed("it was gone".into()));
                    };
                    let at = (n.x, n.y);
                    match walk(p, a, ctx, at, true) {
                        Walk::Arrived => {
                            a.phase = 1;
                            face(p, centre(at));
                        }
                        Walk::NoPath => return no_path("it"),
                        Walk::Moving => {}
                    }
                }
                1 => {
                    let Some(n) = ctx.nodes.get(node) else {
                        p.knowledge.forget_node(node);
                        return Some(Ending::Failed("it was gone".into()));
                    };
                    if n.amount < 1.0 {
                        let n = n.clone();
                        p.knowledge.see_node(node, &n, ctx.tick);
                        if p.carried() > 0 {
                            a.phase = 2;
                            a.path_ready = false;
                        } else {
                            return Some(Ending::Failed("there was nothing left".into()));
                        }
                        return None;
                    }
                    let anim = match g.item.as_str() {
                        "wood" => Anim::Chop,
                        "stone" => Anim::Mine,
                        _ => Anim::Gather,
                    };
                    if work(a, g.minutes as f32 / p.work_speed(skill, c), anim, tm) {
                        p.practice[skill] += g.minutes as f32;
                        let room = c.people.carry.saturating_sub(p.carried());
                        let n = ctx.nodes.get_mut(node).unwrap();
                        let k = (g.yield_per.min(room) as f32).min(n.amount.floor()) as u32;
                        n.amount -= k as f32;
                        p.carry[item] += k;
                        let snapshot = n.clone();
                        p.knowledge.see_node(node, &snapshot, ctx.tick);
                        if snapshot.amount < 1.0 && c.nodes[snapshot.kind as usize].regrow <= 0.0 && c.nodes[snapshot.kind as usize].id != "berry_bush" {
                            let i = ctx.map.idx(snapshot.x as u32, snapshot.y as u32);
                            ctx.nodes.remove(node);
                            ctx.map.node_at[i] = 0;
                            ctx.map.version += 1;
                            p.knowledge.forget_node(node);
                            ctx.events.push(Event::NodeRemoved { kind: snapshot.kind, x: snapshot.x as u32, y: snapshot.y as u32 });
                        }
                        if p.carried() >= c.people.carry || snapshot.amount < 1.0 || k == 0 {
                            a.phase = 2;
                            a.path_ready = false;
                        }
                    }
                }
                2 => match walk(p, a, ctx, camp_tile, true) {
                    Walk::Arrived => {
                        deliver(p, a, ctx.camp);
                        return Some(Ending::Done);
                    }
                    Walk::NoPath => return no_path("the camp"),
                    Walk::Moving => {}
                },
                _ => return Some(Ending::Done),
            }
        }
        Kind::Drink { x, y } | Kind::FetchWater { x, y } => {
            let fetching = matches!(a.kind, Kind::FetchWater { .. });
            match a.phase {
                0 => match walk(p, a, ctx, (x, y), true) {
                    Walk::Arrived => {
                        a.phase = 1;
                        face(p, centre((x, y)));
                    }
                    Walk::NoPath => return no_path("the water"),
                    Walk::Moving => {}
                },
                1 => {
                    let minutes = if fetching { c.actions.fetch_water.minutes } else { c.actions.drink_minutes };
                    if work(a, minutes as f32, if fetching { Anim::DrawWater } else { Anim::Drink }, tm) {
                        let t = c.need("thirst");
                        if fetching {
                            p.needs[t] = (p.needs[t] - 0.5).max(0.0);
                            let room = c.people.carry.saturating_sub(p.carried());
                            p.carry[c.item("water")] += c.actions.fetch_water.amount.min(room);
                            a.phase = 2;
                            a.path_ready = false;
                        } else {
                            p.needs[t] = 0.0;
                            return Some(Ending::Done);
                        }
                    }
                }
                2 => match walk(p, a, ctx, camp_tile, true) {
                    Walk::Arrived => {
                        deliver(p, a, ctx.camp);
                        return Some(Ending::Done);
                    }
                    Walk::NoPath => return no_path("the camp"),
                    Walk::Moving => {}
                },
                _ => return Some(Ending::Done),
            }
        }
        Kind::EatAt { node } => {
            let berries = c.item("berries");
            let Some(n) = ctx.nodes.get(node) else {
                p.knowledge.forget_node(node);
                return Some(Ending::Failed("it was gone".into()));
            };
            let at = (n.x, n.y);
            if a.phase == 0 {
                match walk(p, a, ctx, at, true) {
                    Walk::Arrived => {
                        a.phase = 1;
                        face(p, centre(at));
                    }
                    Walk::NoPath => return no_path("it"),
                    Walk::Moving => {}
                }
                return None;
            }
            if work(a, c.actions.eat_minutes as f32 * 2.0, Anim::Gather, tm) {
                let h = c.need("hunger");
                let per = c.items[berries].food;
                let n = ctx.nodes.get_mut(node).unwrap();
                let mut ate = 0;
                while p.needs[h] > 0.08 && n.amount >= 1.0 {
                    n.amount -= 1.0;
                    p.needs[h] = (p.needs[h] - per).max(0.0);
                    ate += 1;
                }
                let snapshot = n.clone();
                p.knowledge.see_node(node, &snapshot, ctx.tick);
                if ate == 0 {
                    return Some(Ending::Failed("there was nothing left".into()));
                }
                return Some(Ending::Done);
            }
        }
        Kind::DrinkStored | Kind::Eat | Kind::CheckStores => {
            let eating = matches!(a.kind, Kind::Eat);
            let berries = c.item("berries");
            if a.phase == 0 {
                if eating && p.carry[berries] > 0 {
                    a.phase = 1;
                } else {
                    match walk(p, a, ctx, camp_tile, true) {
                        Walk::Arrived => {
                            a.phase = 1;
                            face(p, ctx.camp.stores_pos());
                        }
                        Walk::NoPath => return no_path("the camp"),
                        Walk::Moving => {}
                    }
                }
                return None;
            }
            let minutes = match a.kind {
                Kind::Eat => c.actions.eat_minutes,
                Kind::DrinkStored => c.actions.drink_minutes,
                _ => c.actions.check_stores_minutes,
            };
            let anim = match a.kind {
                Kind::Eat => Anim::Eat,
                Kind::DrinkStored => Anim::Drink,
                _ => Anim::Idle,
            };
            if work(a, minutes as f32, anim, tm) {
                match a.kind {
                    Kind::Eat => {
                        let h = c.need("hunger");
                        let per = c.items[berries].food;
                        let mut ate = 0;
                        while p.needs[h] > 0.08 {
                            if p.carry[berries] > 0 {
                                p.carry[berries] -= 1;
                            } else if ctx.camp.stores[berries] > 0 {
                                ctx.camp.stores[berries] -= 1;
                            } else {
                                break;
                            }
                            p.needs[h] = (p.needs[h] - per).max(0.0);
                            ate += 1;
                        }
                        if ate == 0 {
                            return Some(Ending::Failed("there was nothing to eat".into()));
                        }
                    }
                    Kind::DrinkStored => {
                        let t = c.need("thirst");
                        let w = c.item("water");
                        let mut drank = 0;
                        while p.needs[t] > 0.08 && ctx.camp.stores[w] > 0 {
                            ctx.camp.stores[w] -= 1;
                            p.needs[t] = (p.needs[t] - c.items[w].drink).max(0.0);
                            drank += 1;
                        }
                        if drank == 0 {
                            return Some(Ending::Failed("the water had run out".into()));
                        }
                    }
                    _ => {}
                }
                return Some(Ending::Done);
            }
        }
        Kind::Sleep | Kind::SleepHere | Kind::Rest => {
            let sleeping = matches!(a.kind, Kind::Sleep | Kind::SleepHere);
            if a.phase == 0 && matches!(a.kind, Kind::SleepHere) {
                a.phase = 1;
                p.asleep = true;
                return None;
            }
            if a.phase == 0 {
                let spot = ctx.camp.spot(ctx.slot, sleeping, ctx.map);
                match walk(p, a, ctx, spot, false) {
                    Walk::Arrived | Walk::NoPath => {
                        a.phase = 1;
                        face(p, ctx.camp.fire_pos());
                        if sleeping {
                            p.asleep = true;
                        }
                    }
                    Walk::Moving => {}
                }
                return None;
            }
            if sleeping {
                a.anim = Anim::Sleep;
                let rest = p.needs[c.need("rest")];
                let s = sleepy(c, ctx.tick, p.chronotype);
                let hour = c.calendar.minute_of_day(ctx.tick) as f32 / 60.0 - p.chronotype;
                let morning = hour >= c.people.wake_hour && hour < 14.0;
                // The body clock: up once rested in the morning, and up anyway a little later
                // unless truly exhausted (so sleep never drifts into the day).
                let late = hour >= c.people.wake_hour + 1.5 && hour < 14.0;
                if (morning && rest < 0.35) || (late && rest < 0.7) || (s < 0.05 && rest < 0.03) {
                    p.asleep = false;
                    return Some(Ending::Done);
                }
                return None;
            }
            let r = &c.actions.rest;
            let (f, rs) = (c.need("fun"), c.need("rest"));
            p.needs[f] = (p.needs[f] - r.fun_per_hour / 60.0 * tm).max(0.0);
            p.needs[rs] = (p.needs[rs] - r.rest_per_hour / 60.0 * tm).max(0.0);
            if work(a, r.minutes as f32, Anim::Sit, tm) {
                return Some(Ending::Done);
            }
        }
        Kind::LightFire | Kind::TendFire => {
            if a.phase == 0 {
                match walk(p, a, ctx, fire_tile, true) {
                    Walk::Arrived => {
                        a.phase = 1;
                        face(p, ctx.camp.fire_pos());
                    }
                    Walk::NoPath => return no_path("the fire"),
                    Walk::Moving => {}
                }
                return None;
            }
            let (sticks, wood) = (c.item("sticks"), c.item("wood"));
            let lighting = matches!(a.kind, Kind::LightFire);
            let minutes = if lighting { c.actions.light_fire.minutes } else { c.actions.tend_fire_minutes };
            if work(a, minutes as f32, Anim::Build, tm) {
                let camp = &mut *ctx.camp;
                if lighting {
                    let lf = &c.actions.light_fire;
                    if camp.fire_lit {
                        return Some(Ending::Done);
                    }
                    let kindling = camp.stores[sticks] >= lf.sticks && camp.stores[wood] >= lf.wood;
                    let shaved = !kindling && camp.stores[wood] >= 2;
                    if !kindling && !shaved {
                        return Some(Ending::Failed("there wasn't enough kindling".into()));
                    }
                    let skill = c.skill("firemaking");
                    p.practice[skill] += lf.minutes as f32;
                    let chance = (lf.chance + lf.chance_per_level * p.skill_level(skill, c) as f32) * if shaved { 0.6 } else { 1.0 };
                    if !ctx.rng.chance(chance) {
                        if shaved {
                            camp.stores[wood] -= 1;
                        } else {
                            camp.stores[sticks] -= 1;
                        }
                        return Some(Ending::Failed("the fire wouldn't catch".into()));
                    }
                    if shaved {
                        camp.stores[wood] -= 1;
                    } else {
                        camp.stores[sticks] -= lf.sticks;
                    }
                    let logs = lf.wood.max(1).min(camp.stores[wood]);
                    camp.stores[wood] -= logs;
                    camp.fire_lit = true;
                    camp.fire_hours = lf.sticks as f32 * c.items[sticks].fuel + logs as f32 * c.items[wood].fuel;
                    ctx.events.push(Event::FireLit);
                } else {
                    if !camp.fire_lit {
                        return Some(Ending::Failed("the fire had gone out".into()));
                    }
                    let mut added = 0;
                    while added < 2 && camp.stores[wood] > 0 {
                        camp.stores[wood] -= 1;
                        camp.fire_hours += c.items[wood].fuel;
                        added += 1;
                    }
                    while added < 3 && camp.stores[sticks] > 0 && camp.stores[wood] == 0 {
                        camp.stores[sticks] -= 1;
                        camp.fire_hours += c.items[sticks].fuel;
                        added += 1;
                    }
                    if added == 0 {
                        return Some(Ending::Failed("there was nothing to burn".into()));
                    }
                }
                return Some(Ending::Done);
            }
        }
        Kind::Explore { x, y } => match walk(p, a, ctx, (x, y), false) {
            Walk::Arrived => {
                p.practice[c.skill("exploring")] += (ctx.tick - a.started) as f32 * tm;
                return Some(Ending::Done);
            }
            Walk::NoPath => {
                // Mark it explored so they don't keep trying to reach it.
                p.knowledge.mark_explored(ctx.map.size, x as f32, y as f32, 1.0);
                return no_path("there");
            }
            Walk::Moving => {}
        },
    }
    None
}

/// Should an urgent need cut the current action short? Returns the reason.
pub fn should_interrupt(p: &Person, a: &Action, c: &Content, camp: &Camp) -> Option<String> {
    let fire_lit = camp.fire_lit;
    let can_light = camp.stores[c.item("sticks")] >= c.actions.light_fire.sticks && camp.stores[c.item("wood")] >= c.actions.light_fire.wood;
    let eases = a.kind.eases(c);
    for (i, n) in c.needs.iter().enumerate() {
        if n.kind != crate::content::NeedKind::Body || Some(i) == eases {
            continue;
        }
        let v = p.needs[i];
        let limit = if matches!(a.kind, Kind::Sleep | Kind::SleepHere) { 0.92 } else { 0.85 };
        if v >= limit {
            // Cold only sends them back if there's a fire to go to (or one they can light);
            // sleeping or resting by a lit fire already handles it.
            if n.id == "warmth" && (!(fire_lit || can_light) || (matches!(a.kind, Kind::Sleep | Kind::Rest) && fire_lit)) {
                continue;
            }
            // Can't be helped by stopping: rest while asleep.
            if n.id == "rest" && matches!(a.kind, Kind::Sleep | Kind::SleepHere) {
                continue;
            }
            return Some(format!("{} was {}", p.name, n.word(v)));
        }
    }
    None
}

/// Is this tile water someone can drink from?
pub fn is_fresh(map: &Map, i: usize) -> bool {
    matches!(map.water[i], Water::River | Water::Lake)
}
