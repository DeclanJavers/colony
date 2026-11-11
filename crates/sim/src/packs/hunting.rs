//! The hunting pack: knapping spears (it takes stonework) and hunting the animals people have
//! seen with them. The first pack, and the proof of the interface: before M10 this lived in the
//! base (settle.rs and the rules brain), and it plays out exactly as it did then
//! (crates/sim/tests/packs.rs).

use crate::pack::api::*;
use crate::pack::{Pack, Target, kind};

pub struct Hunting;

const MAKE_SPEAR: u8 = 0;
const HUNT: u8 = 1;

impl Pack for Hunting {
    fn id(&self) -> &'static str {
        "hunting"
    }

    fn name(&self) -> &'static str {
        "Hunting"
    }

    fn about(&self) -> &'static str {
        "Spears knapped from stone, to hunt the animals people have seen."
    }

    fn acts(&self) -> &'static [&'static str] {
        &["make_spear", "hunt"]
    }

    fn options(&self, p: &Person, _id: PersonId, ctx: &Ctx, walk_minutes: &dyn Fn(f32) -> f32, opts: &mut Vec<Choice>) {
        let c = ctx.c;
        let b = &p.knowledge.stores;
        // Spears (stonework), and hunting with one.
        let (spear, wood, stone) = (c.item("spear"), c.item("wood"), c.item("stone"));
        let sw = &c.actions.spear;
        let spears = p.carry[spear] + b.items[spear];
        if spears < 2 {
            let stonework = c.skill("stonework");
            let to_camp = dist(p.pos, ctx.camp.stores_pos());
            if !p.knows(stonework, c) {
                opts.push(Choice { label: "Make a spear".into(), kind: kind(self, "make_spear", Target::None), features: Vec::new(), possible: false, why_not: Some("doesn't know stonework (to knap a point)".into()) });
            } else if b.items[wood] >= sw.wood && b.items[stone] >= sw.stone {
                opts.push(choice("Make a spear".into(), kind(self, "make_spear", Target::None), c, vec![
                    ("dist", to_camp),
                    ("minutes", walk_minutes(to_camp) + sw.minutes as f32),
                    ("spears", spears as f32),
                    ("game_known", (!p.knowledge.animals.is_empty()) as u8 as f32),
                ]));
            }
        }
        if spears > 0 {
            for (k, def) in c.animals.iter().enumerate() {
                let near = p
                    .knowledge
                    .animals
                    .iter()
                    .filter(|(_, a)| a.kind == k as u16 && ctx.tick.saturating_sub(a.when) < 3 * 1440)
                    .map(|(aid, a)| (*aid, a, dist(p.pos, (a.x, a.y))))
                    .filter(|x| x.2 < 70.0)
                    .min_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
                if let Some((aid, a, d)) = near {
                    let level = p.skill_level(c.skill("hunting"), c) as f32;
                    // Herds keep to their range: a sighting from a day or two ago is still worth
                    // following, though they may have moved on.
                    let stale = ctx.tick.saturating_sub(a.when) as f32 / (3.0 * 1440.0);
                    let chance = (def.hit * (0.7 + 0.06 * level)).min(0.95) * (1.0 - 0.5 * stale);
                    let dir = direction(p.pos, (a.x, a.y));
                    opts.push(choice(format!("Hunt {} {dir}", def.name.to_lowercase()), kind(self, "hunt", Target::Animal(aid)), c, vec![
                        ("dist", d),
                        ("minutes", walk_minutes(d) * 1.3 + c.actions.hunt_minutes as f32 + walk_minutes(dist((a.x, a.y), ctx.camp.stores_pos()))),
                        ("chance", chance),
                        ("gain.meat", def.meat as f32 * chance),
                        ("skill", level / 10.0),
                    ]));
                }
            }
        }
    }

    fn step(&self, act: u8, target: Target, p: &mut Person, a: &mut Action, ctx: &mut Ctx) -> Option<Ending> {
        match (act, target) {
            (MAKE_SPEAR, _) => make_spear(p, a, ctx),
            (HUNT, Target::Animal(animal)) => hunt(p, a, ctx, animal),
            _ => Some(Ending::Done),
        }
    }

    fn rules_score(&self, act: u8, o: &Choice, req: &DecisionRequest) -> f32 {
        let sleepy = req.s("sleepy");
        let curiosity = req.s("trait.curiosity");
        match act {
            MAKE_SPEAR => 0.12 + 0.2 * o.f("game_known") - 0.15 * o.f("spears") - 0.6 * sleepy,
            _ => 0.12 + 0.1 * curiosity + 0.15 * o.f("skill") - 0.8 * sleepy,
        }
    }

    fn thought(&self, act: u8, req: &DecisionRequest) -> String {
        match act {
            MAKE_SPEAR => "A good spear and we could eat meat.".into(),
            _ => if req.s("food_days") < 2.0 { "We need food. I'll see if I can bring down something.".into() } else { "I saw game nearby. Meat would make a change.".into() },
        }
    }

    fn scares_animals(&self, act: u8, phase: u8) -> bool {
        act == HUNT && (1..3).contains(&phase)
    }
}

fn make_spear(p: &mut Person, a: &mut Action, ctx: &mut Ctx) -> Option<Ending> {
    let c = ctx.c;
    let tm = c.calendar.minutes_per_tick as f32;
    let no_path = |what: &str| Some(Ending::Failed(format!("couldn't find a way to {what}")));
    let sw = &c.actions.spear;
    let (spear, wood, stone) = (c.item("spear"), c.item("wood"), c.item("stone"));
    if a.phase == 0 {
        match walk(p, a, ctx, ctx.camp.stores_at, true) {
            Walk::Arrived => {
                if ctx.camp.stores[wood] < sw.wood || ctx.camp.stores[stone] < sw.stone {
                    return Some(Ending::Failed("the stores didn't have the wood and stone".into()));
                }
                ctx.camp.stores[wood] -= sw.wood;
                ctx.camp.stores[stone] -= sw.stone;
                next_phase(a, ctx);
            }
            Walk::NoPath => return no_path("the stores"),
            Walk::Moving => {}
        }
        return None;
    }
    a.anim = Anim::Build;
    if a.work_left <= 0.0 {
        a.work_left = sw.minutes as f32;
    }
    a.work_left -= tm;
    practise(p, "stonework", tm, ctx);
    if a.work_left <= 0.0 {
        ctx.camp.stores[spear] += 1;
        a.stored[spear] += 1;
        ctx.events.push(Event::SpearMade { person: ctx.who, name: p.name.clone() });
        return Some(Ending::Done);
    }
    None
}

fn hunt(p: &mut Person, a: &mut Action, ctx: &mut Ctx, animal: AnimalId) -> Option<Ending> {
    let c = ctx.c;
    let tm = c.calendar.minutes_per_tick as f32;
    let spear = c.item("spear");
    let meat = c.item("meat");
    // Bringing it home is the last phase; the quarry no longer matters then.
    if a.phase >= 3 {
        return match walk(p, a, ctx, ctx.camp.stores_at, true) {
            Walk::Arrived | Walk::NoPath => {
                for (i, n) in p.carry.iter_mut().enumerate() {
                    ctx.camp.stores[i] += *n;
                    a.stored[i] += *n;
                    *n = 0;
                }
                Some(if a.stored[meat] > 0 { Ending::Done } else { Ending::Failed(a.label.split_once(": ").map_or(a.label.clone(), |(_, w)| w.to_string())) })
            }
            Walk::Moving => None,
        };
    }
    let Some(prey) = ctx.animals.get(animal) else { return give_up(a, ctx, "it had gone") };
    let def = &c.animals[prey.kind as usize];
    let (kind, at) = (prey.kind, prey.pos);
    let d = dist(p.pos, at);
    match a.phase {
        0 => {
            if p.carry[spear] > 0 {
                next_phase(a, ctx);
                return None;
            }
            match walk(p, a, ctx, ctx.camp.stores_at, true) {
                Walk::Arrived => {
                    if take(p, ctx, spear, 1) == 0 {
                        return Some(Ending::Failed("there was no spear in the stores".into()));
                    }
                    next_phase(a, ctx);
                }
                Walk::NoPath => return Some(Ending::Failed("couldn't reach the stores".into())),
                Walk::Moving => {}
            }
            None
        }
        1 => {
            // Close in (following it if it moves), quietly.
            if d <= 9.0 {
                next_phase(a, ctx);
                return None;
            }
            if d > c.actions.lost_at * 3.0 || minutes_in(a, ctx) > 240.0 {
                return give_up(a, ctx, &format!("couldn't get near the {}", def.name.to_lowercase()));
            }
            let t = (at.0 as u16, at.1 as u16);
            if a.path_ready && a.path.last().is_none_or(|e| (e.0 as i32 - t.0 as i32).abs() + (e.1 as i32 - t.1 as i32).abs() > 3) {
                a.path_ready = false;
            }
            if let Walk::NoPath = walk(p, a, ctx, t, true) {
                return give_up(a, ctx, "couldn't reach it");
            }
            None
        }
        _ => {
            // Stalk, then throw.
            face(p, at);
            a.anim = Anim::Idle;
            if d > c.actions.lost_at {
                ctx.effects.push(Effect::Scare { at, amount: 0.12 });
                return give_up(a, ctx, &format!("the {} got away", def.name.to_lowercase()));
            }
            if minutes_in(a, ctx) < c.actions.hunt_minutes as f32 {
                if d > 9.0 {
                    // It drifted off: creep closer.
                    let t = (at.0 as u16, at.1 as u16);
                    a.path_ready = false;
                    let _ = walk(p, a, ctx, t, true);
                }
                return None;
            }
            let level = p.skill_level(c.skill("hunting"), c) as f32;
            practise(p, "hunting", c.actions.hunt_minutes as f32, ctx);
            let close = (1.0 - (d - 4.0).max(0.0) / 12.0).clamp(0.3, 1.0);
            let chance = (def.hit * (0.7 + 0.06 * level) * close).min(0.95);
            let hit = ctx.rng.chance(chance);
            if ctx.rng.chance(c.actions.spear.breaks) {
                p.carry[spear] = p.carry[spear].saturating_sub(1);
            }
            if !hit {
                ctx.effects.push(Effect::Scare { at, amount: 0.15 });
                return give_up(a, ctx, &format!("missed the {}", def.name.to_lowercase()));
            }
            ctx.effects.push(Effect::Kill { animal, by: ctx.who });
            ctx.effects.push(Effect::Scare { at, amount: 0.3 });
            let room = c.people.carry.saturating_sub(p.carried());
            p.carry[meat] += def.meat.min(room);
            ctx.events.push(Event::Hunted { person: ctx.who, name: p.name.clone(), animal: def.name.to_lowercase(), kind });
            a.phase = 3;
            a.path_ready = false;
            let _ = tm;
            None
        }
    }
}

/// End a hunt that came to nothing: walk the spear home first.
fn give_up(a: &mut Action, ctx: &Ctx, why: &str) -> Option<Ending> {
    a.label = format!("Bring the spear home: {why}");
    a.phase = 3;
    a.path_ready = false;
    a.mark = ctx.tick;
    None
}
