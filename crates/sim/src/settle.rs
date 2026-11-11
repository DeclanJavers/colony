//! Settling down (M6): building (start a home, a storehouse or a field; work on any site),
//! sleeping at home, and the field cycle (sow, tend, harvest). (Spears and hunting moved to the
//! hunting pack in M10: packs/hunting.rs.) Options
//! come from what a person believes and sees; anything that changes someone or something else is
//! done here on the structure itself (structures are the sim's, not a person's) or as an `Effect`.

use crate::actions::{Action, Anim, Ctx, Kind, Walk, choice, dist, walk};
use crate::brain::{Choice, Ending};
use crate::buildings::{self, Crop, Structure, StructureId};
use crate::content::Content;
use crate::events::Event;
use crate::family::find;
use crate::people::social::Stage;
use crate::people::{Person, PersonId};

/// The household's home (finished or not): one they started, or their partner did, or (for the
/// young) their parents.
pub fn home_of(p: &Person, id: PersonId, adult: bool, structures: &crate::ids::Table<Structure>, c: &Content) -> Option<StructureId> {
    let mut heads = vec![id];
    heads.extend(p.family.partner);
    // Children live with their parents, and so do grown children until they have a family of
    // their own.
    let single = p.family.partner.is_none() && p.family.children.is_empty();
    if !adult || single {
        heads.extend(p.family.mother);
        heads.extend(p.family.father);
    }
    let homes = [c.building("house"), c.building("shelter")];
    // A house beats a shelter; a finished one beats a site.
    let mut best: Option<(u8, StructureId)> = None;
    for (sid, s) in structures.iter() {
        if !homes.contains(&s.kind) || !s.owner.is_some_and(|o| heads.contains(&o)) {
            continue;
        }
        let rank = (s.kind == c.building("house")) as u8 * 2 + s.done() as u8;
        if best.is_none_or(|b| rank > b.0) {
            best = Some((rank, sid));
        }
    }
    best.map(|b| b.1)
}

/// Once a day, homes change hands: a dead owner's home goes to their partner or a child, a
/// household that moved into a house gives up its shelter, and an empty home goes to an adult
/// who has none (rather than their building another).
pub fn rehouse(structures: &mut crate::ids::Table<Structure>, map: &mut crate::world::Map, people: &crate::ids::Table<Person>, tick: u64, c: &Content) -> Vec<StructureId> {
    let (house, shelter) = (c.building("house"), c.building("shelter"));
    let rank = |s: &Structure| (s.kind == house) as u8 * 2 + s.done() as u8;
    let ids: Vec<StructureId> = structures.iter().filter(|(_, s)| s.kind == house || s.kind == shelter).map(|(id, _)| id).collect();
    for &sid in &ids {
        let Some(owner) = structures.get(sid).and_then(|s| s.owner) else { continue };
        let Some(o) = people.get(owner) else {
            structures.get_mut(sid).unwrap().owner = None;
            continue;
        };
        let mut heir = Some(owner);
        if !o.alive {
            heir = o.family.partner.filter(|q| people.get(*q).is_some_and(|q| q.alive));
            if heir.is_none() {
                // The eldest living child who has no home of their own.
                heir = o.family.children.iter().copied().filter(|k| people.get(*k).is_some_and(|k| k.alive)).find(|k| {
                    !structures.iter().any(|(_, s)| (s.kind == house || s.kind == shelter) && s.owner == Some(*k))
                });
            }
        } else if let Some(here) = structures.get(sid) {
            // A household with a better home lets this one go.
            let household = [Some(owner), o.family.partner];
            let r = rank(here);
            if here.done() && structures.iter().any(|(other, s)| other != sid && (s.kind == house || s.kind == shelter) && household.contains(&s.owner) && s.done() && rank(s) > r) {
                heir = None;
            }
        }
        structures.get_mut(sid).unwrap().owner = heir;
    }
    // Empty finished homes go to adults without one: families get houses first.
    let mut free: Vec<StructureId> = ids.iter().copied().filter(|sid| structures.get(*sid).is_some_and(|s| s.owner.is_none() && s.done())).collect();
    let mut fallen = Vec::new();
    if free.is_empty() {
        return fallen;
    }
    let mut want: Vec<(bool, PersonId)> = people
        .iter()
        .filter(|(id, p)| p.alive && p.stage(tick, c) >= Stage::Adult && home_of(p, *id, true, structures, c).is_none())
        .map(|(id, p)| (p.family.partner.is_some() || !p.family.children.is_empty(), id))
        .collect();
    want.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (family, id) in want {
        // Someone earlier in the list may have housed this person's partner already.
        let p = people.get(id).unwrap();
        if home_of(p, id, true, structures, c).is_some() {
            continue;
        }
        free.sort_by_key(|sid| {
            let s = structures.get(*sid).unwrap();
            (family != (s.kind == house), sid.index)
        });
        let Some(sid) = free.first().copied() else { break };
        free.remove(0);
        let s = structures.get_mut(sid).unwrap();
        s.owner = Some(id);
        s.empty_since = None;
    }
    // A shelter nobody wants falls down after a few days (a house stands empty for longer).
    for sid in free {
        let s = structures.get_mut(sid).unwrap();
        let since = *s.empty_since.get_or_insert(tick);
        let days = if s.kind == shelter { 4 } else { 24 };
        if tick.saturating_sub(since) >= days * c.calendar.ticks_per_day() {
            crate::buildings::remove(structures, map, c, sid);
            fallen.push(sid);
        }
    }
    fallen
}

fn door_f(s: &Structure, c: &Content) -> (f32, f32) {
    let d = s.door(c);
    (d.0 as f32 + 0.5, d.1 as f32 + 0.5)
}

fn share_available(owed: &[(usize, u32)], stores: &[u32]) -> f32 {
    let need: u32 = owed.iter().map(|(_, n)| n).sum();
    if need == 0 {
        return 1.0;
    }
    owed.iter().map(|(i, n)| stores[*i].min(*n)).sum::<u32>() as f32 / need as f32
}

/// Everything about building, farming and hunting this person could do now.
pub fn options(p: &Person, id: PersonId, ctx: &Ctx, state: &[(String, f32)], walk_minutes: &dyn Fn(f32) -> f32) -> Vec<Choice> {
    let c = ctx.c;
    let get = |name: &str| state.iter().find(|(k, _)| k == name).map_or(0.0, |(_, v)| *v);
    let Some(me) = find(ctx.seen, id) else { return Vec::new() };
    let grown = me.stage >= Stage::Adult;
    let b = &p.knowledge.stores;
    let mut opts = Vec::new();
    let structures = &*ctx.structures;
    let season = c.calendar.season_index(ctx.tick);
    let walk_to = |at: (f32, f32)| walk_minutes(dist(p.pos, at));
    let home = home_of(p, id, me.stage >= Stage::Adult, structures, c);
    // Work someone else is already doing on a field (sowing, harvesting) is theirs.
    let taken = |sid: StructureId| {
        ctx.seen.iter().any(|q| q.id != id && matches!(&q.kind, Some(Kind::Sow { field }) | Some(Kind::Harvest { field }) | Some(Kind::Tend { field }) if *field == sid))
    };
    let founding = |def: u16| ctx.seen.iter().any(|q| q.id != id && matches!(&q.kind, Some(Kind::Found { def: d, .. }) if *d == def));

    if grown {
        // A home: start one, if the household has none (a house for a family, a shelter alone).
        let family = p.family.partner.is_some() || !p.family.children.is_empty();
        // A family in a shelter wants a house.
        let in_shelter = home.and_then(|h| structures.get(h)).is_some_and(|h| h.kind == c.building("shelter") && h.done());
        let house_k = c.building("house");
        let building_house = structures.iter().any(|(_, s)| s.kind == house_k && s.owner.is_some_and(|o| o == id || Some(o) == p.family.partner));
        // A grown child starting a family moves out of their parents' home.
        let at_parents = home.and_then(|h| structures.get(h)).is_some_and(|h| h.owner.is_some_and(|o| o != id && Some(o) != p.family.partner));
        if (home.is_none() || ((in_shelter || at_parents) && family && !building_house)) && !founding(c.building("house")) {
            let kind = c.building(if family { "house" } else { "shelter" });
            let def = &c.buildings[kind as usize];
            if let Some(at) = ctx.sites[kind as usize] {
                let owed: Vec<(usize, u32)> = def.cost.iter().map(|(i, n)| (c.item(i), *n)).collect();
                let door = (at.0 as f32 + def.w as f32 * 0.5, at.1 as f32 + def.h as f32 + 0.5);
                opts.push(choice(format!("Start building a {} for {}", def.name.to_lowercase(), if family { "the family" } else { "yourself" }), Kind::Found { def: kind, x: at.0, y: at.1 }, c, vec![
                    ("dist", dist(p.pos, door)),
                    ("minutes", walk_to(door) + 90.0),
                    ("homeless", 1.0),
                    ("family", (p.family.children.len() as f32 + p.family.partner.is_some() as u8 as f32) / 3.0),
                    ("materials", share_available(&owed, &b.items)),
                ]));
            }
        }
        // A storehouse, once the camp is big enough for food to spoil in quantity.
        let store_k = c.building("storehouse");
        if ctx.alive >= 8 && !structures.iter().any(|(_, s)| s.kind == store_k) && !founding(store_k)
            && let Some(at) = ctx.sites[store_k as usize]
        {
            let def = &c.buildings[store_k as usize];
            let owed: Vec<(usize, u32)> = def.cost.iter().map(|(i, n)| (c.item(i), *n)).collect();
            let spoiling: f32 = (0..b.items.len()).map(|i| b.items[i] as f32 * c.spoil(i).0 * c.items[i].food).sum();
            let door = (at.0 as f32 + def.w as f32 * 0.5, at.1 as f32 + def.h as f32 + 0.5);
            opts.push(choice("Start building a storehouse".into(), Kind::Found { def: store_k, x: at.0, y: at.1 }, c, vec![
                ("dist", dist(p.pos, door)),
                ("minutes", walk_to(door) + 90.0),
                ("spoiling", spoiling),
                ("materials", share_available(&owed, &b.items)),
            ]));
        }
        // A field, if there's seed to sow and not enough fields for everyone.
        let field_k = c.building("field");
        let fields = structures.iter().filter(|(_, s)| s.kind == field_k).count();
        let idle = structures.iter().any(|(_, s)| s.kind == field_k && (!s.done() || s.crop.is_none()));
        let grain = c.item(&c.farming.crop);
        let want = ctx.alive.div_ceil(4).max(1);
        // People settle before they farm: the first field waits for the first finished home.
        let settled = structures.iter().any(|(_, s)| s.done() && c.buildings[s.kind as usize].sleeps > 0);
        if settled && season != 3 && !idle && fields < want && b.items[grain] >= c.farming.seed && !founding(field_k)
            && let Some(at) = ctx.sites[field_k as usize]
        {
            let centre = (at.0 as f32 + 1.5, at.1 as f32 + 1.5);
            opts.push(choice("Clear a field for grain".into(), Kind::Found { def: field_k, x: at.0, y: at.1 }, c, vec![
                ("dist", dist(p.pos, centre)),
                ("minutes", walk_to(centre) + c.buildings[field_k as usize].minutes as f32),
                ("seed", (b.items[grain] as f32 / c.farming.seed as f32).min(3.0)),
                ("fields", fields as f32),
            ]));
        }
        // Work on any site that isn't finished (the nearest two).
        let mut sites: Vec<(StructureId, &Structure, f32)> = structures.iter().filter(|(_, s)| !s.done()).map(|(sid, s)| (sid, s, dist(p.pos, door_f(s, c)))).collect();
        sites.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
        for (sid, s, d) in sites.into_iter().take(2) {
            let def = &c.buildings[s.kind as usize];
            let owed = s.owed(c);
            let can = share_available(&owed, &b.items);
            let blocked = s.progress >= s.paid_share(c) - 0.001 && can <= 0.0 && !owed.is_empty();
            let whose = match s.owner.and_then(|o| ctx.seen.iter().find(|q| q.id == o)) {
                Some(o) if o.id == id => " (your own)".to_string(),
                Some(o) => format!(" ({}'s)", o.name),
                None => String::new(),
            };
            let label = if def.id == "field" { "Clear the field".to_string() } else { format!("Work on the {}{whose}, {:.0}% built", def.name.to_lowercase(), s.progress * 100.0) };
            if blocked {
                let what: Vec<String> = owed.iter().map(|(i, n)| format!("{n} {}", c.items[*i].name.to_lowercase())).collect();
                opts.push(Choice { label, kind: Kind::Build { site: sid }, features: Vec::new(), possible: false, why_not: Some(format!("the stores don't have the {} it still needs", what.join(", "))) });
                continue;
            }
            let mine = s.owner.is_some_and(|o| o == id || Some(o) == p.family.partner || Some(o) == p.family.mother || Some(o) == p.family.father);
            opts.push(choice(label, Kind::Build { site: sid }, c, vec![
                ("dist", d),
                ("minutes", walk_minutes(d) + 90.0),
                ("progress", s.progress),
                ("mine", mine as u8 as f32),
                ("home", (def.sleeps > 0) as u8 as f32),
                ("field", (def.id == "field") as u8 as f32),
                ("materials", can),
            ]));
        }
        // Fields: sow, tend, harvest.
        for (sid, s) in structures.iter().filter(|(sid, s)| s.kind == field_k && s.done() && !taken(*sid)) {
            let centre = s.centre(c);
            let d = dist(p.pos, centre);
            match &s.crop {
                None => {
                    if c.farming.sow[season] && b.items[grain] >= c.farming.seed {
                        opts.push(choice("Sow the field with grain".into(), Kind::Sow { field: sid }, c, vec![
                            ("dist", d),
                            ("minutes", walk_minutes(d) + c.farming.sow_minutes as f32),
                            ("seed", (b.items[grain] as f32 / c.farming.seed as f32).min(3.0)),
                        ]));
                    }
                }
                Some(crop) if crop.dead => {
                    // Clear the dead crop by sowing again (in season) or leave it.
                    if c.farming.sow[season] && b.items[grain] >= c.farming.seed {
                        opts.push(choice("Sow the field again (the last crop died)".into(), Kind::Sow { field: sid }, c, vec![
                            ("dist", d),
                            ("minutes", walk_minutes(d) + c.farming.sow_minutes as f32),
                            ("seed", (b.items[grain] as f32 / c.farming.seed as f32).min(3.0)),
                        ]));
                    }
                }
                Some(crop) if crop.growth >= 1.0 || (season >= 2 && crop.growth >= c.farming.early_harvest) => {
                    let yield_ = c.farming.harvest as f32 * crop.health * crop.growth.min(1.0);
                    let label = if crop.growth >= 1.0 { format!("Harvest the grain (about {yield_:.0})") } else { format!("Cut the grain early, before the frost (about {yield_:.0})") };
                    opts.push(choice(label, Kind::Harvest { field: sid }, c, vec![
                        ("dist", d),
                        ("minutes", walk_minutes(d) + c.farming.harvest_minutes as f32),
                        ("gain.grain", yield_),
                        ("ripe", crop.growth.min(1.0)),
                    ]));
                }
                Some(crop) if crop.weeds >= 0.25 => {
                    opts.push(choice("Weed the field".into(), Kind::Tend { field: sid }, c, vec![
                        ("dist", d),
                        ("minutes", walk_minutes(d) + c.farming.tend_minutes as f32),
                        ("weeds", crop.weeds),
                        ("growth", crop.growth),
                    ]));
                }
                _ => {}
            }
        }
        // What the packs on in this world add (pack.rs): spears and hunting, in the base.
        for &k in ctx.packs {
            crate::pack::get(k).options(p, id, ctx, walk_minutes, &mut opts);
        }
    }
    // Sleeping at home (anyone whose household has a finished home).
    if let Some(hid) = home
        && let Some(h) = structures.get(hid).filter(|h| h.done())
    {
        let def = &c.buildings[h.kind as usize];
        let door = door_f(h, c);
        let need = |n: &str| p.needs[c.need(n)];
        opts.push(choice(format!("Sleep at home (the {})", def.name.to_lowercase()), Kind::SleepHome { home: hid }, c, vec![
            ("dist", dist(p.pos, door)),
            ("minutes", walk_to(door)),
            ("relief.rest", need("rest")),
            ("relief.warmth", need("warmth") * (1.0 - def.chill)),
            ("shelter", 1.0 - def.chill),
            ("cold", get("cold")),
        ]));
    }
    opts
}

pub(crate) fn practise(p: &mut Person, skill: &str, minutes: f32, ctx: &mut Ctx) {
    let k = ctx.c.skill(skill);
    let before = p.skill_level(k, ctx.c);
    p.practice[k] += minutes;
    let after = p.skill_level(k, ctx.c);
    if after > before {
        ctx.events.push(Event::SkillUp { person: ctx.who, name: p.name.clone(), skill: ctx.c.skills.skills[k].name.to_lowercase(), level: after });
    }
}

pub(crate) fn next_phase(a: &mut Action, ctx: &Ctx) {
    a.phase += 1;
    a.mark = ctx.tick;
    a.work_left = 0.0;
    a.path_ready = false;
}

pub(crate) fn minutes_in(a: &Action, ctx: &Ctx) -> f32 {
    (ctx.tick.saturating_sub(a.mark)) as f32 * ctx.c.calendar.minutes_per_tick as f32
}

/// Take up to `n` of an item from the stores into their hands (as much as they can carry).
pub(crate) fn take(p: &mut Person, ctx: &mut Ctx, item: usize, n: u32) -> u32 {
    let room = ctx.c.people.carry.saturating_sub(p.carried());
    let got = n.min(room).min(ctx.camp.stores[item]);
    ctx.camp.stores[item] -= got;
    p.carry[item] += got;
    got
}

pub(crate) fn face(p: &mut Person, at: (f32, f32)) {
    let (dx, dy) = (at.0 - p.pos.0, at.1 - p.pos.1);
    if dx * dx + dy * dy > 1e-4 {
        p.facing = dx.atan2(dy);
    }
}

/// One tick of a building, farming or hunting action.
pub fn step(p: &mut Person, a: &mut Action, ctx: &mut Ctx) -> Option<Ending> {
    let c = ctx.c;
    let tm = c.calendar.minutes_per_tick as f32;
    let no_path = |what: &str| Some(Ending::Failed(format!("couldn't find a way to {what}")));
    match a.kind.clone() {
        Kind::Found { def, x, y } => {
            let d = &c.buildings[def as usize];
            let door = if d.blocks { (x + d.w / 2, y + d.h) } else { (x + d.w / 2, y + d.h / 2) };
            match walk(p, a, ctx, door, false) {
                Walk::Arrived | Walk::NoPath => {}
                Walk::Moving => return None,
            }
            // Still free?
            for ty in y..y + d.h {
                for tx in x..x + d.w {
                    let i = ctx.map.idx(tx as u32, ty as u32);
                    if ctx.map.built[i] != 0 || ctx.map.node_at[i] != 0 || ctx.map.water[i].is_water() {
                        return Some(Ending::Failed("the spot was taken".into()));
                    }
                }
            }
            let owner = (d.sleeps > 0).then_some(ctx.who);
            let site = buildings::place(ctx.structures, ctx.map, c, def, (x, y), owner, ctx.tick);
            ctx.events.push(Event::BuildingStarted { site, person: ctx.who, name: p.name.clone(), kind: d.name.to_lowercase() });
            // Carry straight on as the first builder.
            a.kind = Kind::Build { site };
            a.phase = 0;
            a.mark = ctx.tick;
            a.path_ready = false;
            None
        }
        Kind::Build { site } => {
            let Some(s) = ctx.structures.get(site) else { return Some(Ending::Failed("it was gone".into())) };
            if s.done() {
                return Some(Ending::Done);
            }
            let d = &c.buildings[s.kind as usize];
            let owed = s.owed(c);
            let door = s.door(c);
            let centre = s.centre(c);
            match a.phase {
                // Fetch what it still needs (if the stores have it).
                0 => {
                    let carrying: u32 = owed.iter().map(|(i, _)| p.carry[*i]).sum();
                    let stores_have = owed.iter().any(|(i, _)| ctx.camp.stores[*i] > 0);
                    if owed.is_empty() || carrying > 0 || !stores_have {
                        next_phase(a, ctx);
                        return None;
                    }
                    match walk(p, a, ctx, ctx.camp.stores_at, true) {
                        Walk::Arrived => {
                            for (i, n) in &owed {
                                take(p, ctx, *i, *n);
                            }
                            next_phase(a, ctx);
                        }
                        Walk::NoPath => return no_path("the stores"),
                        Walk::Moving => {}
                    }
                    None
                }
                1 => {
                    match walk(p, a, ctx, door, false) {
                        Walk::Arrived | Walk::NoPath => next_phase(a, ctx),
                        Walk::Moving => {}
                    }
                    None
                }
                _ => {
                    face(p, centre);
                    a.anim = Anim::Build;
                    // Hand over what they brought.
                    let s = ctx.structures.get_mut(site).unwrap();
                    for (k, (id, need)) in d.cost.iter().enumerate() {
                        let i = c.item(id);
                        let give = p.carry[i].min(need - s.paid[k].min(*need));
                        p.carry[i] -= give;
                        s.paid[k] += give;
                    }
                    let cap = s.paid_share(c);
                    let skill = c.skill("building");
                    let speed = p.work_speed(skill, c);
                    if s.progress >= cap - 1e-4 {
                        // Waiting on materials: stop for now.
                        return Some(if s.progress > 0.0 { Ending::Done } else { Ending::Failed("there was nothing to build with".into()) });
                    }
                    s.progress = (s.progress + tm * speed / d.minutes as f32).min(cap);
                    practise(p, "building", tm, ctx);
                    let s = ctx.structures.get_mut(site).unwrap();
                    if s.progress >= 1.0 - 1e-4 {
                        s.progress = 1.0;
                        s.finished = Some(ctx.tick);
                        let owner = s.owner;
                        ctx.events.push(Event::BuildingDone { site, kind: d.name.to_lowercase(), owner });
                        return Some(Ending::Done);
                    }
                    if minutes_in(a, ctx) >= 90.0 {
                        return Some(Ending::Done);
                    }
                    None
                }
            }
        }
        Kind::Sow { field } | Kind::Tend { field } | Kind::Harvest { field } => {
            let Some(s) = ctx.structures.get(field) else { return Some(Ending::Failed("the field was gone".into())) };
            let centre = s.centre(c);
            let tile = (centre.0 as u16, centre.1 as u16);
            let grain = c.item(&c.farming.crop);
            let sowing = matches!(a.kind, Kind::Sow { .. });
            let harvesting = matches!(a.kind, Kind::Harvest { .. });
            match a.phase {
                0 => {
                    if sowing && p.carry[grain] < c.farming.seed {
                        match walk(p, a, ctx, ctx.camp.stores_at, true) {
                            Walk::Arrived => {
                                take(p, ctx, grain, c.farming.seed - p.carry[grain]);
                                if p.carry[grain] < c.farming.seed {
                                    return Some(Ending::Failed("there wasn't enough seed grain".into()));
                                }
                                next_phase(a, ctx);
                            }
                            Walk::NoPath => return no_path("the stores"),
                            Walk::Moving => {}
                        }
                        return None;
                    }
                    next_phase(a, ctx);
                    None
                }
                1 => {
                    match walk(p, a, ctx, tile, false) {
                        Walk::Arrived | Walk::NoPath => next_phase(a, ctx),
                        Walk::Moving => {}
                    }
                    None
                }
                2 => {
                    a.anim = if harvesting { Anim::Gather } else { Anim::Build };
                    let minutes = if sowing { c.farming.sow_minutes } else if harvesting { c.farming.harvest_minutes } else { c.farming.tend_minutes } as f32;
                    let speed = p.work_speed(c.skill("farming"), c);
                    if a.work_left <= 0.0 {
                        a.work_left = minutes;
                    }
                    a.work_left -= tm * speed;
                    practise(p, "farming", tm, ctx);
                    if a.work_left > 0.0 {
                        return None;
                    }
                    let s = ctx.structures.get_mut(field).unwrap();
                    if sowing {
                        p.carry[grain] -= c.farming.seed;
                        s.crop = Some(Crop { sown: ctx.tick, growth: 0.0, weeds: 0.0, health: 1.0, dead: false, by: Some(ctx.who) });
                        ctx.events.push(Event::FieldSown { site: field, person: ctx.who, name: p.name.clone() });
                        return Some(Ending::Done);
                    }
                    if !harvesting {
                        if let Some(crop) = s.crop.as_mut() {
                            crop.weeds = 0.0;
                        }
                        return Some(Ending::Done);
                    }
                    // The harvest: better farmers get more of it.
                    let Some(crop) = s.crop.take() else { return Some(Ending::Failed("there was nothing to harvest".into())) };
                    let level = p.skill_level(c.skill("farming"), c) as f32;
                    let got = (c.farming.harvest as f32 * crop.health * crop.growth.min(1.0) * (0.8 + 0.03 * level)).round() as u32;
                    s.harvested += got;
                    // What they can't carry goes straight to the stores (carried over in loads).
                    let room = c.people.carry.saturating_sub(p.carried());
                    let carry = got.min(room);
                    p.carry[grain] += carry;
                    ctx.camp.stores[grain] += got - carry;
                    a.stored[grain] += got - carry;
                    ctx.events.push(Event::Harvested { site: field, person: ctx.who, name: p.name.clone(), amount: got });
                    next_phase(a, ctx);
                    None
                }
                _ => {
                    // Bring the last load home.
                    match walk(p, a, ctx, ctx.camp.stores_at, true) {
                        Walk::Arrived | Walk::NoPath => {
                            for (i, n) in p.carry.iter_mut().enumerate() {
                                ctx.camp.stores[i] += *n;
                                a.stored[i] += *n;
                                *n = 0;
                            }
                            Some(Ending::Done)
                        }
                        Walk::Moving => None,
                    }
                }
            }
        }
        _ => Some(Ending::Done),
    }
}

