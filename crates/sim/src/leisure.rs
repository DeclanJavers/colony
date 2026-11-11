//! Free time, hobbies and beauty (M7): what people do once their needs are met (stroll, sing,
//! garden, carve, decorate), how a hobby emerges from what someone enjoys, and the beauty field
//! made by flowers, statues and decorated homes. Numbers are in data/base/leisure.ron.
//!
//! Beauty only counts when the "beauty matters" setting is on: then `Ctx::beauty` is the field;
//! off, it's empty, and nothing here (or anywhere) reads it.

use crate::actions::{Action, Anim, Ctx, Kind, Walk, choice, dist, walk};
use crate::brain::{Choice, Ending};
use crate::buildings::Structure;
use crate::content::Content;
use crate::events::Event;
use crate::family::find;
use crate::ids::Table;
use crate::people::social::Stage;
use crate::people::{Hobby, Person, PersonId, TRAITS};
use crate::world::{Map, Node};

/// The beauty of every tile, 0..1, from what grows and what people made.
pub fn beauty_field(map: &Map, nodes: &Table<Node>, structures: &Table<Structure>, c: &Content, season: usize) -> Vec<f32> {
    let b = &c.leisure.beauty;
    let n = map.size as i32;
    let mut field = vec![0.0f32; (map.size * map.size) as usize];
    let mut add = |x: f32, y: f32, value: f32, radius: f32| {
        if value <= 0.0 {
            return;
        }
        let r = radius.ceil() as i32;
        let (cx, cy) = (x.floor() as i32, y.floor() as i32);
        for ty in (cy - r).max(0)..(cy + r + 1).min(n) {
            for tx in (cx - r).max(0)..(cx + r + 1).min(n) {
                let d = ((tx as f32 + 0.5 - x).powi(2) + (ty as f32 + 0.5 - y).powi(2)).sqrt();
                if d < radius {
                    field[(ty * n + tx) as usize] += value * (1.0 - d / radius);
                }
            }
        }
    };
    let carving = c.node_kind("carving");
    for (_, nd) in nodes.iter() {
        let id = &c.nodes[nd.kind as usize].id;
        let Some(&(_, value, radius, winter)) = b.sources.iter().find(|s| &s.0 == id) else { continue };
        // A statue's beauty is set when it's made (its carver's skill), and lasts all year. A
        // flower bed's amount is how well it's blooming (it fades unless tended).
        let v = if nd.kind == carving {
            nd.amount
        } else if season == 3 {
            winter
        } else if id == "flower_bed" {
            value * (0.3 + 0.7 * nd.amount)
        } else {
            value
        };
        add(nd.x as f32 + 0.5, nd.y as f32 + 0.5, v, radius);
    }
    for (_, s) in structures.iter() {
        if s.decor > 0 && s.done() {
            let (x, y) = s.centre(c);
            add(x, y, b.decor[(s.decor as usize - 1).min(b.decor.len() - 1)], b.decor_radius);
        }
    }
    for v in field.iter_mut() {
        *v = v.min(1.0);
    }
    field
}

/// Beauty at a spot (0 when beauty doesn't matter: the field is empty then).
pub fn beauty_at(beauty: &[f32], map: &Map, pos: (f32, f32)) -> f32 {
    if beauty.is_empty() {
        return 0.0;
    }
    let (x, y) = ((pos.0.max(0.0) as u32).min(map.size - 1), (pos.1.max(0.0) as u32).min(map.size - 1));
    beauty[map.idx(x, y)]
}

/// How much an activity suits someone's personality, 0..1.
pub fn appeal(p: &Person, c: &Content, act: u16) -> f32 {
    let def = &c.leisure.acts[act as usize];
    let total: f32 = def.traits.iter().map(|(_, w)| w).sum();
    if total <= 0.0 {
        return 0.5;
    }
    def.traits.iter().map(|(t, w)| w * TRAITS.iter().position(|x| x == t).map_or(0.5, |i| p.personality[i])).sum::<f32>() / total
}

/// A free tile near `at` for something new (a flower bed, a statue): land, nothing standing or
/// built there, off the busy trails, and not in the way of a door or the fire's ring.
fn free_spot(ctx: &Ctx, at: (f32, f32), r0: f32, r1: f32, salt: u64) -> Option<(u16, u16)> {
    let map = &*ctx.map;
    let (fx, fy) = ctx.camp.fire_pos();
    let n = map.size as i32;
    let mut best: Option<(u64, (u16, u16))> = None;
    let r = r1.ceil() as i32;
    let (cx, cy) = (at.0.floor() as i32, at.1.floor() as i32);
    for ty in (cy - r).max(1)..(cy + r + 1).min(n - 1) {
        for tx in (cx - r).max(1)..(cx + r + 1).min(n - 1) {
            let d = ((tx as f32 + 0.5 - at.0).powi(2) + (ty as f32 + 0.5 - at.1).powi(2)).sqrt();
            if d < r0 || d > r1 {
                continue;
            }
            let i = map.idx(tx as u32, ty as u32);
            if map.water[i].is_water() || map.node_at[i] != 0 || map.built[i] != 0 || map.footfall[i] > crate::path::TRAIL_STEPS * 0.3 {
                continue;
            }
            if ((tx as f32 + 0.5 - fx).powi(2) + (ty as f32 + 0.5 - fy).powi(2)).sqrt() < 3.0 {
                continue;
            }
            // Never right next to a building (doors stay clear).
            let beside = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| map.built[map.idx((tx + dx) as u32, (ty + dy) as u32)] == 1);
            if beside {
                continue;
            }
            let h = crate::hash::hash2(salt, tx, ty);
            if best.is_none_or(|b| h < b.0) {
                best = Some((h, (tx as u16, ty as u16)));
            }
        }
    }
    best.map(|b| b.1)
}

/// The household's finished home, if any.
fn finished_home(p: &Person, id: PersonId, ctx: &Ctx) -> Option<crate::buildings::StructureId> {
    crate::settle::home_of(p, id, true, ctx.structures, ctx.c).filter(|h| ctx.structures.get(*h).is_some_and(|s| s.done()))
}

/// Where someone makes things: by their home's door, or by the fire.
fn base_of(p: &Person, id: PersonId, ctx: &Ctx) -> (f32, f32) {
    match finished_home(p, id, ctx).and_then(|h| ctx.structures.get(h)) {
        Some(s) => {
            let d = s.door(ctx.c);
            (d.0 as f32 + 0.5, d.1 as f32 + 0.5)
        }
        None => ctx.camp.fire_pos(),
    }
}

/// The flower bed near `at` that most needs tending, and how well it's blooming (0..1).
fn wilting_bed(ctx: &Ctx, bed: u16, at: (f32, f32), r: f32) -> Option<((u16, u16), f32)> {
    let n = ctx.map.size as i32;
    let ri = r.ceil() as i32;
    let (cx, cy) = (at.0 as i32, at.1 as i32);
    let mut best: Option<((u16, u16), f32)> = None;
    for ty in (cy - ri).max(0)..(cy + ri + 1).min(n) {
        for tx in (cx - ri).max(0)..(cx + ri + 1).min(n) {
            let slot = ctx.map.node_at[ctx.map.idx(tx as u32, ty as u32)];
            if let Some(nd) = (slot > 0).then(|| ctx.nodes.id_at(slot - 1).and_then(|id| ctx.nodes.get(id))).flatten()
                && nd.kind == bed
                && best.is_none_or(|b| nd.amount < b.1)
            {
                best = Some(((tx as u16, ty as u16), nd.amount));
            }
        }
    }
    best
}

fn count_near(ctx: &Ctx, kind: u16, at: (f32, f32), r: f32) -> usize {
    let n = ctx.map.size as i32;
    let ri = r.ceil() as i32;
    let (cx, cy) = (at.0 as i32, at.1 as i32);
    let mut k = 0;
    for ty in (cy - ri).max(0)..(cy + ri + 1).min(n) {
        for tx in (cx - ri).max(0)..(cx + ri + 1).min(n) {
            let slot = ctx.map.node_at[ctx.map.idx(tx as u32, ty as u32)];
            if slot > 0 && ctx.nodes.id_at(slot - 1).and_then(|id| ctx.nodes.get(id)).is_some_and(|nd| nd.kind == kind) {
                k += 1;
            }
        }
    }
    k
}

/// The next step of decorating a home (1 stones to the door, 2 flower boxes, 3 carved posts).
pub fn decor_word(step: u8) -> &'static str {
    match step {
        1 => "lay stones to the door",
        2 => "put up flower boxes",
        _ => "carve the door posts",
    }
}

/// Free-time choices for someone with time to spare.
pub fn options(p: &Person, id: PersonId, ctx: &Ctx, walk_minutes: &dyn Fn(f32) -> f32) -> Vec<Choice> {
    let c = ctx.c;
    let l = &c.leisure;
    let mut opts = Vec::new();
    let Some(me) = find(ctx.seen, id) else { return opts };
    if me.stage < Stage::Adult {
        return opts;
    }
    let season = c.calendar.season_index(ctx.tick);
    let fun = p.needs[c.need("fun")];
    let social = p.needs[c.need("social")];
    let b = &p.knowledge.stores;
    let (wood, stone) = (c.item("wood"), c.item("stone"));
    let base = base_of(p, id, ctx);
    let fire = ctx.camp.fire_pos();
    let salt = (ctx.tick / 240) ^ (id.index as u64 * 7919);
    for (k, def) in l.acts.iter().enumerate() {
        let act = k as u16;
        if !def.seasons[season.min(3)] {
            continue;
        }
        let hobby = p.has_hobby(act);
        let fun_gain = def.fun * if hobby { l.hobby_fun } else { 1.0 };
        let mut features: Vec<(&str, f32)> = vec![
            ("appeal", appeal(p, c, act)),
            ("hobby", hobby as u8 as f32),
            ("relief.fun", fun.min(fun_gain)),
            ("relief.social", social.min(def.social)),
        ];
        let (label, target) = match def.id.as_str() {
            "stroll" => {
                // Somewhere pleasant within a short walk: the most beautiful of a few spots (when
                // beauty matters), else just a spot out along the way.
                let mut best: Option<(f32, (u16, u16))> = None;
                for j in 0..10 {
                    let a = j as f32 / 10.0 * std::f32::consts::TAU + (salt % 97) as f32 * 0.07;
                    let r = 8.0 + (crate::hash::hash2(salt, j, 3) % 9) as f32;
                    let (x, y) = (p.pos.0 + a.cos() * r, p.pos.1 + a.sin() * r);
                    if x < 1.0 || y < 1.0 || x >= ctx.map.size as f32 - 1.0 || y >= ctx.map.size as f32 - 1.0 {
                        continue;
                    }
                    let i = ctx.map.idx(x as u32, y as u32);
                    if ctx.map.water[i].is_water() || ctx.map.built[i] == 1 || ctx.map.node_at[i] != 0 {
                        continue;
                    }
                    let beauty = crate::leisure::beauty_at(ctx.beauty, ctx.map, (x, y));
                    let score = beauty + (crate::hash::hash2(salt, j, 5) % 100) as f32 * 0.0005;
                    if best.is_none_or(|bb| score > bb.0) {
                        best = Some((score, (x as u16, y as u16)));
                    }
                }
                let Some((_, t)) = best else { continue };
                let beauty = beauty_at(ctx.beauty, ctx.map, (t.0 as f32 + 0.5, t.1 as f32 + 0.5));
                features.push(("beauty", beauty));
                let dir = crate::actions::direction(p.pos, (t.0 as f32, t.1 as f32));
                let label = if beauty > 0.25 { format!("Go for a stroll somewhere lovely {dir}") } else { format!("Go for a stroll {dir}") };
                (label, t)
            }
            "sing" => {
                let others = ctx.seen.iter().filter(|q| q.id != id && !q.asleep && dist(q.pos, fire) < c.people.fire_reach + 1.0).count();
                features.push(("company", (others as f32 / 3.0).min(1.0)));
                features.push(("evening", ((c.calendar.minute_of_day(ctx.tick) as f32 / 60.0 - 16.0) / 4.0).clamp(0.0, 1.0)));
                ("Sing by the fire".to_string(), (fire.0 as u16, fire.1 as u16 + 1))
            }
            "garden" => {
                // Plant a new bed (up to three by a home), or tend the one that needs it most.
                let bed = c.node_kind("flower_bed");
                let at_home = finished_home(p, id, ctx).is_some();
                let place = if at_home { "by home" } else { "by the fire" };
                let wilting = wilting_bed(ctx, bed, base, l.place_radius as f32 + 1.0);
                let full = count_near(ctx, bed, base, l.place_radius as f32 + 1.0) >= 3;
                // A wilting bed comes first; with three beds, any that isn't in full bloom.
                if let Some((t, bloom)) = wilting.filter(|w| w.1 < 0.6 || (full && w.1 < 0.85)) {
                    features.push(("makes", 0.6 + 0.4 * (1.0 - bloom)));
                    (format!("Tend the flower beds {place}"), t)
                } else if full {
                    continue;
                } else {
                    let Some(t) = free_spot(ctx, base, 1.5, l.place_radius as f32, salt) else { continue };
                    features.push(("makes", 1.0));
                    (format!("Plant flowers {place}"), t)
                }
            }
            "carve" => {
                if b.items[wood] < def.wood {
                    continue;
                }
                let level = p.skill_level(c.skill("carving"), c);
                features.push(("level", level as f32 / 10.0));
                let statue = level >= l.statue_level;
                features.push(("makes", if statue { 1.0 } else { 0.5 }));
                let label = if statue { "Carve a statue" } else { "Carve some wood" };
                (label.to_string(), (fire.0 as u16 + 1, fire.1 as u16))
            }
            "decorate" => {
                let Some(h) = finished_home(p, id, ctx) else { continue };
                let s = ctx.structures.get(h).unwrap();
                if c.buildings[s.kind as usize].id != "house" || s.decor >= 3 {
                    continue;
                }
                let step = s.decor + 1;
                let ok = if step == 1 { b.items[stone] >= def.stone } else { b.items[wood] >= def.wood };
                if !ok {
                    continue;
                }
                let d = s.door(c);
                features.push(("makes", 1.0));
                (format!("Decorate home: {}", decor_word(step)), d)
            }
            _ => continue,
        };
        let d = dist(p.pos, (target.0 as f32 + 0.5, target.1 as f32 + 0.5));
        features.push(("dist", d));
        features.push(("minutes", walk_minutes(d) + def.minutes as f32));
        opts.push(choice(label, Kind::Leisure { act, x: target.0, y: target.1 }, c, features));
    }
    opts
}

fn practise(p: &mut Person, skill: usize, minutes: f32, ctx: &mut Ctx) {
    let before = p.skill_level(skill, ctx.c);
    p.practice[skill] += minutes;
    let after = p.skill_level(skill, ctx.c);
    if after > before {
        ctx.events.push(Event::SkillUp { person: ctx.who, name: p.name.clone(), skill: ctx.c.skills.skills[skill].name.to_lowercase(), level: after });
    }
}

/// Put a made thing in the world (a flower bed, a statue).
fn place(ctx: &mut Ctx, kind: u16, at: (u16, u16), amount: f32) -> Option<crate::ids::Id<Node>> {
    let i = ctx.map.idx(at.0 as u32, at.1 as u32);
    if ctx.map.node_at[i] != 0 || ctx.map.built[i] != 0 || ctx.map.water[i].is_water() {
        return None;
    }
    let variants = ctx.c.nodes[kind as usize].variants.max(1) as u64;
    let variant = (crate::hash::hash2(0xA27, at.0 as i32, at.1 as i32) % variants) as u8;
    let turn = (crate::hash::hash2(0xA28, at.0 as i32, at.1 as i32) % 4) as u8;
    let id = ctx.nodes.insert(Node { kind, x: at.0, y: at.1, variant, turn, amount });
    ctx.map.node_at[i] = id.index + 1;
    ctx.map.version += 1;
    Some(id)
}

/// One tick of a free-time activity: fetch materials if it needs them, go there, do it (easing
/// fun as it goes), then leave what was made and count the enjoyment towards a hobby.
pub fn step(p: &mut Person, a: &mut Action, ctx: &mut Ctx) -> Option<Ending> {
    let c = ctx.c;
    let l = &c.leisure;
    let tm = c.calendar.minutes_per_tick as f32;
    let Kind::Leisure { act, x, y } = a.kind.clone() else { return Some(Ending::Failed("not a free-time activity".into())) };
    let def = &l.acts[act as usize];
    let (wood, stone) = (c.item("wood"), c.item("stone"));
    // What to take from the stores.
    let need: Vec<(usize, u32)> = match def.id.as_str() {
        "carve" => vec![(wood, def.wood)],
        "decorate" => {
            let step = crate::settle::home_of(p, ctx.who, true, ctx.structures, c).and_then(|h| ctx.structures.get(h)).map_or(1, |s| s.decor + 1);
            if step == 1 { vec![(stone, def.stone)] } else { vec![(wood, def.wood)] }
        }
        _ => Vec::new(),
    };
    match a.phase {
        0 => {
            if need.iter().all(|(i, n)| p.carry[*i] >= *n) {
                a.phase = 1;
                a.path_ready = false;
                return None;
            }
            match walk(p, a, ctx, ctx.camp.stores_at, true) {
                Walk::Arrived => {
                    for (i, n) in &need {
                        let got = (*n - p.carry[*i].min(*n)).min(ctx.camp.stores[*i]);
                        ctx.camp.stores[*i] -= got;
                        p.carry[*i] += got;
                        if p.carry[*i] < *n {
                            return Some(Ending::Failed(format!("the stores had no {}", c.items[*i].name.to_lowercase())));
                        }
                    }
                    a.phase = 1;
                    a.path_ready = false;
                }
                Walk::NoPath => return Some(Ending::Failed("couldn't reach the stores".into())),
                Walk::Moving => {}
            }
            None
        }
        1 => {
            let beside = def.id != "stroll";
            match walk(p, a, ctx, (x, y), beside) {
                Walk::Arrived => {
                    a.phase = 2;
                    a.mark = ctx.tick;
                    a.work_left = 0.0;
                    a.fun_start = p.needs[c.need("fun")];
                    if def.id == "sing" {
                        p.said = Some((crate::chat::song(ctx.who, ctx.tick), ctx.tick));
                    }
                    let (tx, ty) = if def.id == "sing" || def.id == "carve" { ctx.camp.fire_pos() } else { (x as f32 + 0.5, y as f32 + 0.5) };
                    let (dx, dy) = (tx - p.pos.0, ty - p.pos.1);
                    if dx * dx + dy * dy > 1e-4 {
                        p.facing = dx.atan2(dy);
                    }
                }
                Walk::NoPath => return Some(Ending::Failed("couldn't find a way there".into())),
                Walk::Moving => {}
            }
            None
        }
        _ => {
            let hobby = p.has_hobby(act);
            let mult = if hobby { l.hobby_fun } else { 1.0 };
            let (f, s) = (c.need("fun"), c.need("social"));
            p.needs[f] = (p.needs[f] - def.fun * mult / def.minutes as f32 * tm).max(0.0);
            p.needs[s] = (p.needs[s] - def.social / def.minutes as f32 * tm).max(0.0);
            if let Some(sk) = &def.skill {
                practise(p, c.skill(sk), tm, ctx);
            }
            let anim = match def.id.as_str() {
                "stroll" => Anim::Idle,
                "sing" | "carve" => Anim::Sit,
                "garden" => Anim::Gather,
                _ => Anim::Build,
            };
            a.anim = anim;
            if minutes_in(a, ctx) < def.minutes as f32 {
                return None;
            }
            finish(p, a.fun_start, act, x, y, ctx);
            Some(Ending::Done)
        }
    }
}

fn minutes_in(a: &Action, ctx: &Ctx) -> f32 {
    (ctx.tick.saturating_sub(a.mark)) as f32 * ctx.c.calendar.minutes_per_tick as f32
}

/// What was made, and the enjoyment counted towards a hobby.
fn finish(p: &mut Person, fun_start: f32, act: u16, x: u16, y: u16, ctx: &mut Ctx) {
    let c = ctx.c;
    let l = &c.leisure;
    let def = &l.acts[act as usize];
    let (wood, stone) = (c.item("wood"), c.item("stone"));
    let made: Option<String> = match def.id.as_str() {
        "garden" => {
            let bed = c.node_kind("flower_bed");
            let i = ctx.map.idx(x as u32, y as u32);
            let here = ctx.map.node_at[i];
            match (here > 0).then(|| ctx.nodes.id_at(here - 1)).flatten() {
                // Tending: the bed blooms fully again.
                Some(nid) if ctx.nodes.get(nid).is_some_and(|nd| nd.kind == bed) => {
                    ctx.nodes.get_mut(nid).unwrap().amount = 1.0;
                    ctx.map.version += 1;
                    None
                }
                _ => place(ctx, bed, (x, y), 1.0).map(|node| {
                    ctx.works.push(crate::sim::Work { node, by: ctx.who, name: "flower bed".into(), when: ctx.tick });
                    "planted a flower bed".to_string()
                }),
            }
        }
        "carve" => {
            p.carry[wood] = p.carry[wood].saturating_sub(def.wood);
            let level = p.skill_level(c.skill("carving"), c);
            let mut out = Some("carved a little figure".to_string());
            if level >= l.statue_level {
                // A statue, set up by their home (or the fire).
                let at = base_of(p, ctx.who, ctx);
                let carving = c.node_kind("carving");
                let crowd = count_near(ctx, carving, at, l.place_radius as f32 + 2.0);
                let salt = ctx.tick ^ ctx.who.index as u64;
                if crowd < 2
                    && let Some(spot) = free_spot(ctx, at, 2.0, l.place_radius as f32 + 1.0, salt)
                {
                    let beauty = c.leisure.beauty.sources.iter().find(|s| s.0 == "carving").map_or(0.4, |s| s.1) + l.beauty.statue_per_level * level as f32;
                    if let Some(node) = place(ctx, carving, spot, beauty) {
                        ctx.works.push(crate::sim::Work { node, by: ctx.who, name: "statue".into(), when: ctx.tick });
                        out = Some("carved a statue".to_string());
                    }
                }
            }
            out
        }
        "decorate" => {
            let home = crate::settle::home_of(p, ctx.who, true, ctx.structures, c);
            match home.and_then(|h| ctx.structures.get_mut(h)) {
                Some(s) if s.decor < 3 => {
                    s.decor += 1;
                    let step = s.decor;
                    if step == 1 {
                        p.carry[stone] = p.carry[stone].saturating_sub(def.stone);
                    } else {
                        p.carry[wood] = p.carry[wood].saturating_sub(def.wood);
                    }
                    ctx.map.version += 1;
                    Some(match step {
                        1 => "laid stones to their door".to_string(),
                        2 => "put up flower boxes at home".to_string(),
                        _ => "carved posts for their door".to_string(),
                    })
                }
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(what) = made {
        ctx.events.push(Event::Made { person: ctx.who, name: p.name.clone(), what });
    }
    // Enjoyment: more when it suits them, and when they needed the fun.
    let suits = appeal(p, c, act);
    let joy = def.fun * (0.2 + 2.0 * suits * suits) * (0.6 + 0.8 * fun_start);
    let hobbies = p.hobbies.iter().filter(|h| h.since.is_some()).count();
    let e = match p.hobbies.iter_mut().position(|h| h.act == act) {
        Some(i) => &mut p.hobbies[i],
        None => {
            p.hobbies.push(Hobby { act, joy: 0.0, times: 0, since: None });
            p.hobbies.last_mut().unwrap()
        }
    };
    e.joy += joy;
    e.times += 1;
    if e.since.is_none() && e.joy >= def.hobby_joy && hobbies < l.max_hobbies {
        e.since = Some(ctx.tick);
        ctx.events.push(Event::HobbyFound { person: ctx.who, name: p.name.clone(), hobby: hobby_name(c, act).to_string() });
    }
}

/// A hobby's name ("singing", "gardening", "carving", "walking", "decorating").
pub fn hobby_name(c: &Content, act: u16) -> &'static str {
    match c.leisure.acts[act as usize].id.as_str() {
        "stroll" => "walking",
        "sing" => "singing",
        "garden" => "gardening",
        "carve" => "carving",
        "decorate" => "making a home lovely",
        _ => "pastimes",
    }
}
