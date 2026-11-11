//! Social actions (M5): talking (with news and gossip), courting, asking (partnership, a child,
//! a lesson, food) and answering, learning by watching, playing, feeding children and burying
//! the dead. Options come from who a person can see; anything that changes someone else is an
//! `Effect`, applied after everyone's turn so the order never depends on who moved first.

use crate::actions::{Anim, Ctx, Kind, Walk, choice, dist, walk};
use crate::brain::{Choice, Ending};
use crate::content::Content;
use crate::events::Event;
use crate::family::{self, Seen, find, kinship};
use crate::ids::{Id, Table};
use crate::people::knowledge::{KnownNode, KnownWater, Source};
use crate::people::social::{Ask, Invite, Memory, Stage};
use crate::people::{Person, PersonId, Sex};
use crate::rng::Rng;
use crate::world::{Map, Node};

/// A change to someone other than the person acting.
#[derive(Clone, Debug)]
pub enum Effect {
    /// Change how `from` feels about `to`.
    Rel { from: PersonId, to: PersonId, familiarity: f32, affection: f32, respect: f32 },
    /// Courting: `to` warms to `from` if they're drawn to them.
    Woo { from: PersonId, to: PersonId, amount: f32 },
    Need { who: PersonId, need: usize, delta: f32 },
    Invite { to: PersonId, invite: Invite },
    Remember { who: PersonId, memory: Memory },
    Partner { a: PersonId, b: PersonId },
    Conceive { mother: PersonId, father: PersonId },
    Give { from: PersonId, to: PersonId, item: usize, n: u32 },
    Fed { child: PersonId, food: f32, drink: f32, by: PersonId },
    Bury { body: PersonId, at: (u16, u16), by: PersonId },
    /// News: places `from` knows, for `to` to learn if they're new to them.
    Tell { from: PersonId, to: PersonId, nodes: Vec<(Id<Node>, KnownNode)>, water: Vec<KnownWater> },
    /// Gossip: `who`'s feeling about `about` moves towards `toward`.
    Opinion { who: PersonId, about: PersonId, toward: f32 },
}

fn mem(tick: u64, text: String, mood: String, about: Option<PersonId>, feeling: f32, importance: f32) -> Memory {
    Memory { tick, text, mood, about, feeling, importance, strength: 1.0 }
}

/// How well two people get on: similar sociability and warmth help, and kindness helps most.
fn compatibility(a: &[f32; 5], b: &[f32; 5]) -> f32 {
    1.0 - (a[2] - b[2]).abs() * 0.5 - (a[3] - b[3]).abs() * 0.3 + (a[3] + b[3]) * 0.25 - 0.25
}

// ---------------------------------------------------------------------------------------------
// Options

/// People they can see (awake, within sight), nearest first.
fn visible<'a>(p: &Person, id: PersonId, seen: &'a [Seen], range: f32) -> Vec<(&'a Seen, f32)> {
    let mut v: Vec<(&Seen, f32)> =
        seen.iter().filter(|s| s.id != id && s.alive && !s.asleep && s.stage != Stage::Baby).map(|s| (s, dist(p.pos, s.pos))).filter(|(_, d)| *d <= range).collect();
    v.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.id.cmp(&b.0.id)));
    v
}

fn taught_knowers(seen: &[Seen], skill: usize) -> f32 {
    let grown = seen.iter().filter(|s| s.alive && s.stage >= Stage::Adult).count().max(1);
    seen.iter().filter(|s| s.alive && s.levels[skill] >= 1).count() as f32 / grown as f32
}

/// The skill an action uses, if any.
pub fn skill_of(kind: &Kind, c: &Content) -> Option<usize> {
    match kind {
        Kind::Gather { def, .. } => Some(c.skill(&c.actions.gather[*def as usize].skill)),
        Kind::LightFire | Kind::TendFire => Some(c.skill("firemaking")),
        Kind::Explore { .. } => Some(c.skill("exploring")),
        _ => None,
    }
}

/// How useful a skill is to the group right now (0..1), from what its work is worth: what the
/// items it gathers are worth, and fire before cold nights.
fn usefulness(skill: usize, c: &Content, state: &[(String, f32)]) -> f32 {
    let get = |name: &str| state.iter().find(|(k, _)| k == name).map_or(0.0, |(_, v)| *v);
    let id = c.skills.skills[skill].id.as_str();
    if id == "firemaking" {
        return (0.5 + 0.5 * get("night_cold") / 1.5).min(1.0);
    }
    c.actions.gather.iter().filter(|g| g.skill == id).map(|g| (get(&format!("value.{}", g.item)) * 8.0).min(1.0)).fold(0.0, f32::max)
}

/// Everything social this person could do now.
pub fn options(p: &Person, id: PersonId, ctx: &Ctx, state: &[(String, f32)], walk_minutes: &dyn Fn(f32) -> f32) -> Vec<Choice> {
    let c = ctx.c;
    let l = &c.life;
    let seen = ctx.seen;
    let me = find(seen, id).unwrap();
    let stage = me.stage;
    let need = |n: &str| p.needs[c.need(n)];
    let mut opts = Vec::new();
    let near = visible(p, id, seen, c.people.sight as f32 + 8.0);

    // Answers to what others asked.
    for inv in &p.invites {
        let Some(from) = find(seen, inv.from).filter(|s| s.alive) else { continue };
        let r = p.rel(inv.from);
        let d = dist(p.pos, from.pos);
        let base = vec![("dist", d), ("affection", r.affection), ("familiarity", r.familiarity), ("romance", r.romance), ("respect", r.respect)];
        let (label, mut f) = match inv.ask {
            Ask::Partner => (format!("Say yes: become {}'s partner", from.name), base),
            Ask::Child => {
                let mut f = base;
                f.push(("food_days", ctx_food_days(ctx)));
                f.push(("young_children", young_children(seen, id) as f32));
                (format!("Say yes to {}: try for a child", from.name), f)
            }
            Ask::Teach { skill } => {
                let s = &c.skills.skills[skill as usize];
                let mut f = base;
                f.push(("knowers", taught_knowers(seen, skill as usize)));
                f.push(("teacher_level", from.levels[skill as usize] as f32 / 10.0));
                f.push(("my_level", p.skill_level(skill as usize, c) as f32 / 10.0));
                f.push(("taught", s.taught as u8 as f32));
                f.push(("useful", usefulness(skill as usize, c, state)));
                (format!("Learn {} from {}", s.name.to_lowercase(), from.name), f)
            }
            Ask::Food if p.carry[c.item("berries")] == 0 => continue,
            Ask::Food => {
                let mut f = base;
                f.push(("their_hunger", from.hunger));
                f.push(("carried.berries", p.carry[c.item("berries")] as f32));
                (format!("Give {} some of your berries", from.name), f)
            }
        };
        f.push(("minutes", walk_minutes(d) + 30.0));
        opts.push(choice(label, Kind::Answer { from: inv.from, ask: inv.ask }, c, f));
    }

    // Talking: the few nearest they're on speaking terms with.
    let berry = c.node_kind("berry_bush");
    let news = if near.is_empty() { 0 } else { p.knowledge.nodes.values().filter(|k| k.kind == berry && k.amount >= 3.0).take(l.news_places as usize).count() };
    for (s, d) in near.iter().take(3) {
        let r = p.rel(s.id);
        let label = if *d < 4.0 { format!("Talk with {}", s.name) } else { format!("Go and talk with {}", s.name) };
        opts.push(choice(label, Kind::Talk { with: s.id }, c, vec![
            ("dist", *d),
            ("minutes", walk_minutes(*d) + l.talk_minutes as f32),
            ("relief.social", need("social").min(0.25)),
            ("relief.fun", need("fun").min(0.05)),
            ("affection", r.affection),
            ("familiarity", r.familiarity),
            ("kin", kinship(seen, id, s.id)),
            ("news", news as f32),
        ]));
    }

    if stage >= Stage::Adult {
        let mine = p.family.partner;
        // Courting and asking: unpartnered grown people they're drawn to (and who are drawn to
        // them), close in age, not close kin.
        if mine.is_none() {
            let mut best: Option<(&Seen, f32, f32)> = None;
            for (s, d) in &near {
                let ok = s.stage >= Stage::Adult
                    && s.partner.is_none()
                    && s.sex == p.drawn_to
                    && s.drawn_to == p.sex
                    && (s.age - me.age).abs() <= l.court_age_gap
                    && !family::close_kin(seen, id, s.id);
                if !ok {
                    continue;
                }
                let r = p.rel(s.id);
                let pull = r.romance * 2.0 + r.affection + r.familiarity * 0.5 - d / 60.0;
                if best.is_none_or(|b| pull > b.2) {
                    best = Some((s, *d, pull));
                }
            }
            if let Some((s, d, _)) = best {
                let r = p.rel(s.id);
                if r.romance >= l.propose_romance && r.familiarity >= 0.3 && !s.asked.contains(&id) {
                    opts.push(choice(format!("Ask {} to be your partner", s.name), Kind::Ask { with: s.id, ask: Ask::Partner }, c, vec![
                        ("dist", d),
                        ("minutes", walk_minutes(d) + l.ask_minutes as f32),
                        ("romance", r.romance),
                        ("affection", r.affection),
                    ]));
                } else {
                    opts.push(choice(format!("Court {}", s.name), Kind::Court { with: s.id }, c, vec![
                        ("dist", d),
                        ("minutes", walk_minutes(d) + l.court_minutes as f32),
                        ("relief.social", need("social").min(0.25)),
                        ("romance", r.romance),
                        ("affection", r.affection),
                        ("familiarity", r.familiarity),
                    ]));
                }
            }
        }
        // A child with their partner.
        if let Some(pid) = mine
            && let Some(s) = find(seen, pid).filter(|s| s.alive && !s.asleep)
        {
            let (mother_fertile, father_fertile) = if p.sex == Sex::Female { (me.fertile, s.fertile) } else { (s.fertile, me.fertile) };
            if p.sex != s.sex && !s.asked.contains(&id) {
                let d = dist(p.pos, s.pos);
                if mother_fertile && father_fertile {
                    opts.push(choice(format!("Ask {} to try for a child", s.name), Kind::Ask { with: pid, ask: Ask::Child }, c, vec![
                        ("dist", d),
                        ("minutes", walk_minutes(d) + l.together_minutes as f32 + 30.0),
                        ("food_days", ctx_food_days(ctx)),
                        ("young_children", young_children(seen, id) as f32),
                        ("romance", p.rel(pid).romance),
                    ]));
                }
            }
        }
        // Teaching a taught skill to someone young who doesn't know it.
        for (k, sk) in c.skills.skills.iter().enumerate() {
            if !sk.taught || p.skill_level(k, c) < 1 {
                continue;
            }
            let learner = near
                .iter()
                .filter(|(s, _)| s.levels[k] == 0 && s.stage >= Stage::Child && s.age >= l.help_age && s.age < l.learn_until && !s.asked.contains(&id))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((s, d)) = learner {
                opts.push(choice(format!("Offer to teach {} {}", s.name, sk.name.to_lowercase()), Kind::Ask { with: s.id, ask: Ask::Teach { skill: k as u8 } }, c, vec![
                    ("dist", *d),
                    ("minutes", walk_minutes(*d) + l.lesson_minutes as f32 + 30.0),
                    ("knowers", taught_knowers(seen, k)),
                    ("useful", usefulness(k, c, state)),
                    ("kin", kinship(seen, id, s.id)),
                    ("my_level", p.skill_level(k, c) as f32 / 10.0),
                ]));
            }
        }
        // Feeding hungry or thirsty little ones (their own first): babies past nursing, and
        // children too young to fend for themselves.
        let berries = c.item("berries");
        let water = c.item("water");
        let have_food = p.carry[berries] + p.knowledge.stores.items[berries] > 0;
        let have_water = p.carry[water] + p.knowledge.stores.items[water] > 0;
        let want = |s: &Seen| s.hunger.max(s.thirst);
        let mut hungry: Vec<(&Seen, f32)> = seen
            .iter()
            .filter(|s| s.alive && s.stage <= Stage::Child && want(s) >= 0.4 && (s.age >= l.nursing_age || s.mother.is_none_or(|m| find(seen, m).is_none_or(|m| !m.alive))) && s.age < l.help_age)
            .map(|s| (s, dist(p.pos, s.pos)))
            .filter(|(_, d)| *d <= 40.0)
            .collect();
        hungry.sort_by(|a, b| (kinship(seen, id, b.0.id) + want(b.0)).total_cmp(&(kinship(seen, id, a.0.id) + want(a.0))).then(a.0.id.cmp(&b.0.id)));
        if let Some((s, d)) = hungry.first() {
            if (s.hunger >= 0.4 && have_food) || (s.thirst >= 0.4 && have_water) {
                let fetch = if p.carry[berries] > 0 { 0.0 } else { dist(p.pos, ctx.camp.stores_pos()) };
                opts.push(choice(format!("Feed {}", s.name), Kind::Feed { child: s.id }, c, vec![
                    ("dist", *d),
                    ("minutes", walk_minutes(fetch + d) + l.feed_minutes as f32),
                    ("child_hunger", want(s)),
                    ("kin", kinship(seen, id, s.id)),
                ]));
            } else {
                opts.push(Choice { label: format!("Feed {}", s.name), kind: Kind::Feed { child: s.id }, features: Vec::new(), possible: false, why_not: Some(if s.thirst >= 0.4 { "no water in the stores or in hand".into() } else { "no food in the stores or in hand".into() }) });
            }
        }
        // Burying the dead.
        if let Some((s, d)) = seen.iter().filter(|s| !s.alive && !s.buried).map(|s| (s, dist(p.pos, s.pos))).filter(|(_, d)| *d <= 60.0).min_by(|a, b| a.1.total_cmp(&b.1)) {
            let days = s.died.map_or(0.0, |t| ctx.tick.saturating_sub(t) as f32 / c.calendar.ticks_per_day() as f32);
            opts.push(choice(format!("Bury {}", s.name), Kind::Bury { body: s.id }, c, vec![
                ("dist", d),
                ("minutes", walk_minutes(d) + l.bury_minutes as f32),
                ("kin", kinship(seen, id, s.id)),
                ("affection", p.rel(s.id).affection),
                ("days_dead", days),
            ]));
        }
    }

    // Asking for food when there's none in the stores or in hand.
    let berries = c.item("berries");
    if need("hunger") >= 0.5 && p.carry[berries] == 0 && p.knowledge.stores.items[berries] == 0
        && let Some((s, d)) = near.iter().filter(|(s, _)| s.carrying_food >= 2 && !s.asked.contains(&id)).min_by(|a, b| a.1.total_cmp(&b.1)) {
            opts.push(choice(format!("Ask {} for some berries", s.name), Kind::Ask { with: s.id, ask: Ask::Food }, c, vec![
                ("dist", *d),
                ("minutes", walk_minutes(*d) + l.ask_minutes as f32 + 15.0),
                ("relief.hunger", need("hunger").min(0.4) * 0.6),
                ("affection", p.rel(s.id).affection),
            ]));
        }

    // Children: learn by watching, and play.
    if stage == Stage::Child || (stage >= Stage::Adult && me.age < l.adult_age + 6.0) {
        let mut watch: Option<(&Seen, f32, usize)> = None;
        for (s, d) in &near {
            if s.stage < Stage::Adult {
                continue;
            }
            let Some(k) = s.kind.as_ref().and_then(|k| skill_of(k, c)) else { continue };
            if s.levels[k] < 1 || s.levels[k] <= p.skill_level(k, c) {
                continue;
            }
            // Taught skills wait until they're old enough to help.
            if c.skills.skills[k].taught && me.age < l.help_age {
                continue;
            }
            if watch.is_none_or(|w| *d < w.1) {
                watch = Some((s, *d, k));
            }
        }
        if let Some((s, d, k)) = watch {
            let sk = &c.skills.skills[k];
            opts.push(choice(format!("Watch {} and learn ({})", s.name, sk.name.to_lowercase()), Kind::Watch { who: s.id, skill: k as u8 }, c, vec![
                ("dist", d),
                ("minutes", walk_minutes(d) + l.watch_minutes as f32),
                ("taught", sk.taught as u8 as f32),
                ("knowers", taught_knowers(seen, k)),
                ("useful", usefulness(k, c, state)),
                ("my_level", p.skill_level(k, c) as f32 / 10.0),
                ("relief.fun", need("fun").min(0.05)),
            ]));
        }
    }
    if stage == Stage::Child {
        opts.push(choice("Play".into(), Kind::Play, c, vec![
            ("dist", 0.0),
            ("minutes", l.play_minutes as f32),
            ("relief.fun", need("fun").min(0.35)),
            ("relief.social", need("social").min(0.1)),
        ]));
    }
    opts
}

fn ctx_food_days(ctx: &Ctx) -> f32 {
    let c = ctx.c;
    let berries = c.item("berries");
    let daily = {
        let h = &c.needs[c.need("hunger")];
        (h.awake_per_hour * 16.0 + h.asleep_per_hour * 8.0) / c.items[berries].food
    };
    (ctx.camp.stores[berries] as f32 / (ctx.alive.max(1) as f32 * daily)).min(20.0)
}

fn young_children(seen: &[Seen], id: PersonId) -> usize {
    seen.iter().filter(|s| s.alive && s.stage <= Stage::Child && (s.mother == Some(id) || s.father == Some(id))).count()
}

// ---------------------------------------------------------------------------------------------
// Doing

/// Walk to someone (following them if they move). Arrived when within reach.
fn approach(p: &mut Person, a: &mut crate::actions::Action, ctx: &mut Ctx, target: (f32, f32)) -> Walk {
    let t = (target.0.max(0.0) as u16, target.1.max(0.0) as u16);
    if dist(p.pos, target) <= 1.8 {
        a.anim = Anim::Idle;
        return Walk::Arrived;
    }
    if a.path_ready && a.path.last().is_none_or(|e| (e.0 as i32 - t.0 as i32).abs() + (e.1 as i32 - t.1 as i32).abs() > 2) {
        a.path_ready = false;
    }
    walk(p, a, ctx, t, true)
}

fn face_to(p: &mut Person, at: (f32, f32)) {
    let (dx, dy) = (at.0 - p.pos.0, at.1 - p.pos.1);
    if dx * dx + dy * dy > 1e-4 {
        p.facing = dx.atan2(dy);
    }
}

/// Minutes since the action's current phase began.
fn phase_minutes(a: &crate::actions::Action, ctx: &Ctx) -> f32 {
    (ctx.tick.saturating_sub(a.mark)) as f32 * ctx.c.calendar.minutes_per_tick as f32
}

fn next_phase(a: &mut crate::actions::Action, ctx: &Ctx) {
    a.phase += 1;
    a.mark = ctx.tick;
    a.work_left = 0.0;
}

/// One tick of a social action.
pub fn step(p: &mut Person, a: &mut crate::actions::Action, ctx: &mut Ctx) -> Option<Ending> {
    let c = ctx.c;
    let l = &c.life;
    let tm = c.calendar.minutes_per_tick as f32;
    let me = ctx.who;
    let other_id = match &a.kind {
        Kind::Talk { with } | Kind::Court { with } | Kind::Ask { with, .. } => *with,
        Kind::Answer { from, .. } => *from,
        Kind::Watch { who, .. } => *who,
        Kind::Feed { child } => *child,
        Kind::Bury { body } => *body,
        Kind::Play => me,
        _ => return Some(Ending::Done),
    };
    let Some(o) = find(ctx.seen, other_id).cloned() else { return Some(Ending::Failed("they were gone".into())) };
    if !matches!(a.kind, Kind::Bury { .. } | Kind::Play) && !o.alive {
        return Some(Ending::Interrupted(format!("{} died", o.name)));
    }
    let social = c.need("social");
    match a.kind.clone() {
        Kind::Play => {
            // Run about near the camp.
            if a.phase == 0 {
                a.mark = ctx.tick;
                a.phase = 1;
            }
            let t = phase_minutes(a, ctx);
            let f = c.need("fun");
            p.needs[f] = (p.needs[f] - 0.5 / 60.0 * tm).max(0.0);
            a.anim = if (t as u32 / 6).is_multiple_of(2) { Anim::Walk } else { Anim::Idle };
            if a.anim == Anim::Walk {
                let (fx, fy) = ctx.camp.fire_pos();
                let ang = (t / 6.0) + (me.index as f32);
                let target = (fx + ang.cos() * 4.5, fy + ang.sin() * 4.5);
                face_to(p, target);
                let step = 0.4 * tm;
                let d = dist(p.pos, target).max(1e-3);
                let k = (step / d).min(1.0);
                let next = (p.pos.0 + (target.0 - p.pos.0) * k, p.pos.1 + (target.1 - p.pos.1) * k);
                let i = ctx.map.idx((next.0 as u32).min(ctx.map.size - 1), (next.1 as u32).min(ctx.map.size - 1));
                if !ctx.map.water[i].is_water() && ctx.map.node_at[i] == 0 {
                    p.pos = next;
                }
            }
            if t >= l.play_minutes as f32 {
                return Some(Ending::Done);
            }
        }
        Kind::Talk { with } | Kind::Court { with } => {
            let courting = matches!(a.kind, Kind::Court { .. });
            if a.phase == 0 {
                if o.asleep {
                    return Some(Ending::Failed(format!("{} was asleep", o.name)));
                }
                if phase_minutes(a, ctx) > 60.0 {
                    return Some(Ending::Failed(format!("couldn't catch up with {}", o.name)));
                }
                match approach(p, a, ctx, o.pos) {
                    Walk::Arrived => next_phase(a, ctx),
                    Walk::NoPath => return Some(Ending::Failed(format!("couldn't reach {}", o.name))),
                    Walk::Moving => {}
                }
                return None;
            }
            let d = dist(p.pos, o.pos);
            if d > 3.5 {
                // They moved on: follow a little, then give up.
                if d > 10.0 || o.asleep {
                    return Some(Ending::Interrupted(format!("{} left", o.name)));
                }
                let _ = approach(p, a, ctx, o.pos);
                return None;
            }
            face_to(p, o.pos);
            a.anim = Anim::Idle;
            let rate = 0.75 / 60.0 * tm;
            p.needs[social] = (p.needs[social] - rate).max(0.0);
            ctx.effects.push(Effect::Need { who: with, need: social, delta: -rate });
            let minutes = if courting { l.court_minutes } else { l.talk_minutes } as f32;
            if phase_minutes(a, ctx) >= minutes {
                finish_talk(p, with, &o, courting, ctx);
                return Some(Ending::Done);
            }
        }
        Kind::Ask { with, ask } => {
            if a.phase == 0 {
                if o.asleep {
                    return Some(Ending::Failed(format!("{} was asleep", o.name)));
                }
                if phase_minutes(a, ctx) > 60.0 {
                    return Some(Ending::Failed(format!("couldn't catch up with {}", o.name)));
                }
                match approach(p, a, ctx, o.pos) {
                    Walk::Arrived => next_phase(a, ctx),
                    Walk::NoPath => return Some(Ending::Failed(format!("couldn't reach {}", o.name))),
                    Walk::Moving => {}
                }
                return None;
            }
            if a.phase == 1 {
                face_to(p, o.pos);
                a.anim = Anim::Idle;
                if phase_minutes(a, ctx) >= l.ask_minutes as f32 {
                    ctx.effects.push(Effect::Invite { to: with, invite: Invite { from: me, ask, tick: ctx.tick } });
                    if ask == Ask::Partner {
                        return Some(Ending::Done);
                    }
                    next_phase(a, ctx);
                }
                return None;
            }
            // Waiting for an answer (a lesson, a child, food), then doing it together.
            let answering = matches!(&o.kind, Some(Kind::Answer { from, ask: k }) if *from == me && *k == ask);
            let d = dist(p.pos, o.pos);
            if a.phase == 2 {
                a.anim = Anim::Idle;
                if answering {
                    next_phase(a, ctx);
                } else if phase_minutes(a, ctx) > 90.0 {
                    return Some(Ending::Failed(format!("{} didn't answer", o.name)));
                } else if d > 3.0 && !o.asleep {
                    let _ = approach(p, a, ctx, o.pos);
                }
                return None;
            }
            // Together: stay while they're answering.
            a.anim = if matches!(ask, Ask::Teach { .. }) { Anim::Build } else { Anim::Sit };
            face_to(p, o.pos);
            if !answering {
                return Some(Ending::Done);
            }
            if phase_minutes(a, ctx) > 180.0 {
                return Some(Ending::Done);
            }
        }
        Kind::Answer { from, ask } => {
            // The asker should be waiting for this answer (except partnership, said at once).
            let waiting = matches!(&o.kind, Some(Kind::Ask { with, ask: k }) if *with == me && *k == ask);
            if ask == Ask::Partner {
                if a.phase == 0 {
                    ctx.effects.push(Effect::Partner { a: me, b: from });
                    next_phase(a, ctx);
                }
                a.anim = Anim::Idle;
                face_to(p, o.pos);
                if phase_minutes(a, ctx) >= l.ask_minutes as f32 {
                    return Some(Ending::Done);
                }
                return None;
            }
            if !waiting && a.phase < 2 {
                return Some(Ending::Failed(format!("{} had stopped waiting", o.name)));
            }
            if a.phase == 0 {
                if phase_minutes(a, ctx) > 60.0 {
                    return Some(Ending::Failed(format!("couldn't reach {}", o.name)));
                }
                match approach(p, a, ctx, o.pos) {
                    Walk::Arrived => next_phase(a, ctx),
                    Walk::NoPath => return Some(Ending::Failed(format!("couldn't reach {}", o.name))),
                    Walk::Moving => {}
                }
                return None;
            }
            face_to(p, o.pos);
            let d = dist(p.pos, o.pos);
            match ask {
                Ask::Food => {
                    let berries = c.item("berries");
                    let n = p.carry[berries].min(3);
                    if n == 0 {
                        return Some(Ending::Failed("had no berries left to give".into()));
                    }
                    p.carry[berries] -= n;
                    ctx.effects.push(Effect::Give { from: me, to: from, item: berries, n });
                    return Some(Ending::Done);
                }
                Ask::Teach { skill } => {
                    a.anim = Anim::Idle;
                    if a.phase == 1 {
                        a.phase = 2;
                    }
                    if !waiting || d > 4.0 {
                        return Some(Ending::Interrupted(format!("{} stopped teaching", o.name)));
                    }
                    let k = skill as usize;
                    let teacher = (o.levels[k] as f32 / 5.0).clamp(0.4, 2.0);
                    learn(p, k, tm * l.lesson_rate * teacher, &o.name, ctx);
                    if phase_minutes(a, ctx) >= l.lesson_minutes as f32 {
                        ctx.effects.push(Effect::Rel { from, to: me, familiarity: 0.05, affection: 0.05, respect: 0.0 });
                        let r = p.rel_mut(from);
                        r.respect = (r.respect + 0.15).min(1.0);
                        r.affection = (r.affection + 0.05).min(1.0);
                        let sk = c.skills.skills[k].name.to_lowercase();
                        let tick = ctx.tick;
                        p.remember(mem(tick, format!("{} taught me {sk}", o.name), format!("Grateful to {}", o.name), Some(from), 0.3, 0.5), c);
                        ctx.effects.push(Effect::Remember { who: from, memory: mem(tick, format!("Taught {} {sk}", p.name), format!("Proud of teaching {}", p.name), Some(me), 0.3, 0.4) });
                        return Some(Ending::Done);
                    }
                }
                Ask::Child => {
                    a.anim = Anim::Sit;
                    if a.phase == 1 {
                        a.phase = 2;
                    }
                    if !waiting || d > 4.0 {
                        return Some(Ending::Interrupted(format!("{} left", o.name)));
                    }
                    if phase_minutes(a, ctx) >= l.together_minutes as f32 {
                        let (mother, father) = if p.sex == Sex::Female { (me, from) } else { (from, me) };
                        ctx.effects.push(Effect::Conceive { mother, father });
                        let r = p.rel_mut(from);
                        r.romance = (r.romance + 0.05).min(1.0);
                        r.affection = (r.affection + 0.05).min(1.0);
                        return Some(Ending::Done);
                    }
                }
                Ask::Partner => {}
            }
        }
        Kind::Watch { who, skill } => {
            if phase_minutes(a, ctx) > l.watch_minutes as f32 + 30.0 {
                return Some(Ending::Done);
            }
            let d = dist(p.pos, o.pos);
            if d > 2.5 {
                if d > 40.0 {
                    return Some(Ending::Interrupted(format!("lost sight of {}", o.name)));
                }
                let _ = approach(p, a, ctx, o.pos);
                return None;
            }
            face_to(p, o.pos);
            a.anim = Anim::Idle;
            let k = skill as usize;
            if o.levels[k] >= 1 {
                // Watching the work itself teaches most.
                let doing = o.kind.as_ref().and_then(|kd| skill_of(kd, c)) == Some(k);
                let rate = l.watch_rate * if doing { 1.0 } else { 0.25 };
                learn(p, k, tm * rate, &o.name, ctx);
            }
            if a.work_left <= 0.0 {
                a.work_left = l.watch_minutes as f32;
            }
            a.work_left -= tm;
            if a.work_left <= 0.0 {
                ctx.effects.push(Effect::Rel { from: who, to: me, familiarity: 0.03, affection: 0.02, respect: 0.0 });
                let r = p.rel_mut(who);
                r.respect = (r.respect + 0.05).min(1.0);
                return Some(Ending::Done);
            }
        }
        Kind::Feed { child } => {
            let berries = c.item("berries");
            let water = c.item("water");
            if a.phase == 0 {
                if p.carry[berries] > 0 && (p.carry[water] > 0 || o.thirst < 0.4) {
                    next_phase(a, ctx);
                    return None;
                }
                match walk(p, a, ctx, ctx.camp.stores_at, true) {
                    Walk::Arrived => {
                        let take = if o.hunger >= 0.25 { ctx.camp.stores[berries].min(3) } else { 0 };
                        let sip = if o.thirst >= 0.25 { ctx.camp.stores[water].min(2) } else { 0 };
                        if take + sip == 0 && p.carry[berries] == 0 {
                            return Some(Ending::Failed("there was no food or water in the stores".into()));
                        }
                        ctx.camp.stores[berries] -= take;
                        ctx.camp.stores[water] -= sip;
                        p.carry[berries] += take;
                        p.carry[water] += sip;
                        a.path_ready = false;
                        next_phase(a, ctx);
                    }
                    Walk::NoPath => return Some(Ending::Failed("couldn't reach the stores".into())),
                    Walk::Moving => {}
                }
                return None;
            }
            if a.phase == 1 {
                if phase_minutes(a, ctx) > 60.0 {
                    return Some(Ending::Failed(format!("couldn't reach {}", o.name)));
                }
                match approach(p, a, ctx, o.pos) {
                    Walk::Arrived => next_phase(a, ctx),
                    Walk::NoPath => return Some(Ending::Failed(format!("couldn't reach {}", o.name))),
                    Walk::Moving => {}
                }
                return None;
            }
            face_to(p, o.pos);
            a.anim = Anim::Gather;
            if phase_minutes(a, ctx) >= l.feed_minutes as f32 {
                let n = p.carry[berries].min(3);
                let w = p.carry[water].min(2);
                p.carry[berries] -= n;
                p.carry[water] -= w;
                let food = n as f32 * c.items[berries].food;
                let drink = n as f32 * c.items[berries].drink + w as f32 * c.items[water].drink;
                ctx.effects.push(Effect::Fed { child, food, drink, by: me });
                return Some(Ending::Done);
            }
        }
        Kind::Bury { body } => {
            if o.buried {
                return Some(Ending::Done);
            }
            if a.phase == 0 {
                match walk(p, a, ctx, (o.pos.0 as u16, o.pos.1 as u16), true) {
                    Walk::Arrived => next_phase(a, ctx),
                    Walk::NoPath => return Some(Ending::Failed(format!("couldn't reach {}'s body", o.name))),
                    Walk::Moving => {}
                }
                return None;
            }
            face_to(p, o.pos);
            a.anim = Anim::Build;
            if phase_minutes(a, ctx) >= l.bury_minutes as f32 {
                ctx.effects.push(Effect::Bury { body, at: (o.pos.0 as u16, o.pos.1 as u16), by: me });
                return Some(Ending::Done);
            }
        }
        _ => {}
    }
    None
}

/// Practice from a teacher or from watching; a taught skill reaching level 1 is news.
fn learn(p: &mut Person, k: usize, minutes: f32, teacher: &str, ctx: &mut Ctx) {
    let c = ctx.c;
    let before = p.skill_level(k, c);
    p.practice[k] += minutes;
    let after = p.skill_level(k, c);
    if after > before {
        let skill = c.skills.skills[k].name.to_lowercase();
        if before == 0 && c.skills.skills[k].taught {
            ctx.events.push(Event::Learned { learner: ctx.who, name: p.name.clone(), skill, teacher: teacher.to_string() });
        } else {
            ctx.events.push(Event::SkillUp { person: ctx.who, name: p.name.clone(), skill, level: after });
        }
    }
}

/// The end of a talk: they know each other better, like each other more or less, maybe quarrel,
/// and pass on news and gossip.
fn finish_talk(p: &mut Person, with: PersonId, o: &Seen, courting: bool, ctx: &mut Ctx) {
    let c = ctx.c;
    let l = &c.life;
    let me = ctx.who;
    let tick = ctx.tick;
    let their = o.personality;
    let fit = compatibility(&p.personality, &their);
    let excitable = (1.0 - p.personality[4]) * (1.0 - their[4]);
    let sour = (-p.rel(with).affection).max(0.0);
    // Quarrels come from temper and strain: excitable people, bad feeling, and being hungry,
    // tired or cold.
    let strain = ["hunger", "rest", "warmth"].iter().map(|n| p.needs[c.need(n)]).fold(0.0, f32::max);
    let argue = ctx.rng.chance(((l.argue_chance * excitable * 4.0 + sour * 0.08) * (0.3 + 1.4 * strain)).min(0.5));
    if argue {
        let names = format!("{} and {}", p.name, o.name);
        let r = p.rel_mut(with);
        r.affection = (r.affection - 0.15).max(-1.0);
        r.familiarity = (r.familiarity + 0.03).min(1.0);
        p.remember(mem(tick, format!("Argued with {}", o.name), format!("Upset with {}", o.name), Some(with), -0.4, 0.3), c);
        ctx.effects.push(Effect::Rel { from: with, to: me, familiarity: 0.03, affection: -0.15, respect: 0.0 });
        ctx.effects.push(Effect::Remember { who: with, memory: mem(tick, format!("Argued with {}", p.name), format!("Upset with {}", p.name), Some(me), -0.4, 0.3) });
        ctx.events.push(Event::Argued { a: me, b: with, names });
        return;
    }
    // Liking grows less the more they already like each other.
    let warm = (0.05 * fit + 0.02) * (1.0 - p.rel(with).affection.max(0.0));
    let r = p.rel_mut(with);
    r.familiarity = (r.familiarity + 0.05 * (1.0 - r.familiarity)).min(1.0);
    r.affection = (r.affection + warm).clamp(-1.0, 1.0);
    r.last = tick;
    ctx.effects.push(Effect::Rel { from: with, to: me, familiarity: 0.05, affection: warm, respect: 0.0 });
    if courting {
        let r = p.rel_mut(with);
        r.romance = (r.romance + 0.08 + 0.08 * fit.max(0.0)).min(1.0);
        ctx.effects.push(Effect::Woo { from: me, to: with, amount: 0.08 + 0.08 * fit.max(0.0) });
        if r.romance > 0.5 {
            p.remember(mem(tick, format!("Spent time with {}", o.name), format!("Sweet on {}", o.name), Some(with), 0.25, 0.3), c);
        }
    }
    // News: the best berry bushes they know, and fresh water.
    let berry = c.node_kind("berry_bush");
    let mut nodes: Vec<(Id<Node>, KnownNode)> = p.knowledge.nodes.iter().filter(|(_, k)| k.kind == berry && k.amount >= 3.0).map(|(i, k)| (*i, k.clone())).collect();
    nodes.sort_by(|a, b| b.1.amount.total_cmp(&a.1.amount).then(a.0.cmp(&b.0)));
    nodes.truncate(l.news_places as usize);
    let water: Vec<KnownWater> = p.knowledge.water.iter().take(3).cloned().collect();
    ctx.effects.push(Effect::Tell { from: me, to: with, nodes, water });
    // Gossip: what they think of someone else, if they feel strongly.
    if let Some((about, r)) = p.rels.iter().filter(|(id, r)| **id != with && **id != me && r.affection.abs() > 0.3).max_by(|a, b| a.1.affection.abs().total_cmp(&b.1.affection.abs()).then(b.0.cmp(a.0))) {
        ctx.effects.push(Effect::Opinion { who: with, about: *about, toward: r.affection });
    }
}

// ---------------------------------------------------------------------------------------------
// Applying effects

/// Apply this tick's effects, in order.
#[allow(clippy::too_many_arguments)]
pub fn apply(effects: Vec<Effect>, people: &mut Table<Person>, nodes: &mut Table<Node>, map: &mut Map, camp_fire: (f32, f32), c: &Content, tick: u64, rng: &mut Rng, events: &mut Vec<Event>) {
    let name = |people: &Table<Person>, id: PersonId| people.get(id).map_or("someone".to_string(), |p| p.name.clone());
    for e in effects {
        match e {
            Effect::Rel { from, to, familiarity, affection, respect } => {
                if let Some(p) = people.get_mut(from) {
                    let r = p.rel_mut(to);
                    r.familiarity = (r.familiarity + familiarity).clamp(0.0, 1.0);
                    r.affection = (r.affection + affection).clamp(-1.0, 1.0);
                    r.respect = (r.respect + respect).clamp(0.0, 1.0);
                    r.last = tick;
                }
            }
            Effect::Woo { from, to, amount } => {
                let from_sex = people.get(from).map(|p| p.sex);
                if let Some(p) = people.get_mut(to)
                    && Some(p.drawn_to) == from_sex
                    && p.family.partner.is_none()
                {
                    let r = p.rel_mut(from);
                    let warmth = (0.5 + r.affection).clamp(0.2, 1.2);
                    r.romance = (r.romance + amount * warmth).min(1.0);
                }
            }
            Effect::Need { who, need, delta } => {
                if let Some(p) = people.get_mut(who) {
                    p.needs[need] = (p.needs[need] + delta).clamp(0.0, 1.0);
                }
            }
            Effect::Invite { to, invite } => {
                if let Some(p) = people.get_mut(to)
                    && p.alive
                    && !p.invites.iter().any(|i| i.from == invite.from && i.ask == invite.ask)
                {
                    p.invites.push(invite);
                }
            }
            Effect::Remember { who, memory } => {
                if let Some(p) = people.get_mut(who)
                    && p.alive
                {
                    p.remember(memory, c);
                }
            }
            Effect::Partner { a, b } => {
                let free = |p: Option<&Person>| p.is_some_and(|p| p.alive && p.family.partner.is_none());
                if !(free(people.get(a)) && free(people.get(b))) {
                    continue;
                }
                let (na, nb) = (name(people, a), name(people, b));
                for (x, y, ny) in [(a, b, &nb), (b, a, &na)] {
                    let p = people.get_mut(x).unwrap();
                    p.family.partner = Some(y);
                    let r = p.rel_mut(y);
                    r.romance = r.romance.max(0.7);
                    r.affection = (r.affection + 0.2).min(1.0);
                    p.remember(mem(tick, format!("Became partners with {ny}"), format!("Happy with {ny}"), Some(y), 0.8, 0.9), c);
                    p.invites.retain(|i| i.ask != Ask::Partner);
                }
                events.push(Event::Partnered { a, b, names: format!("{na} and {nb}") });
            }
            Effect::Conceive { mother, father } => {
                let ok = people.get(mother).is_some_and(|m| m.alive && m.fertile(tick, c)) && people.get(father).is_some_and(|f| f.alive && f.fertile(tick, c));
                if ok && rng.chance(c.life.conceive_chance) {
                    let due = tick + (c.life.pregnancy_years as f64 * c.ticks_per_age_year()) as u64;
                    let fname = name(people, father);
                    let m = people.get_mut(mother).unwrap();
                    m.pregnant = Some(crate::people::social::Pregnancy { father, conceived: tick, due });
                    events.push(Event::Expecting { mother, name: m.name.clone(), father: fname });
                }
            }
            Effect::Give { from, to, item, n } => {
                let (nf, nt) = (name(people, from), name(people, to));
                if let Some(p) = people.get_mut(to) {
                    let room = c.people.carry.saturating_sub(p.carried());
                    p.carry[item] += n.min(room);
                    let r = p.rel_mut(from);
                    r.affection = (r.affection + 0.15).min(1.0);
                    r.respect = (r.respect + 0.05).min(1.0);
                    p.remember(mem(tick, format!("{nf} shared food with me"), format!("Grateful to {nf}"), Some(from), 0.4, 0.5), c);
                }
                if let Some(p) = people.get_mut(from) {
                    let r = p.rel_mut(to);
                    r.affection = (r.affection + 0.05).min(1.0);
                }
                events.push(Event::Shared { from, to, names: format!("{nf} gave {nt}"), what: format!("{n} {}", c.items[item].name.to_lowercase()) });
            }
            Effect::Fed { child, food, drink, by } => {
                if let Some(p) = people.get_mut(child)
                    && p.alive
                {
                    let (h, t) = (c.need("hunger"), c.need("thirst"));
                    p.needs[h] = (p.needs[h] - food).max(0.0);
                    p.needs[t] = (p.needs[t] - drink).max(0.0);
                    p.last.ate = tick;
                    p.last.drank = tick;
                    let r = p.rel_mut(by);
                    r.affection = (r.affection + 0.05).min(1.0);
                }
            }
            Effect::Bury { body, at, by } => {
                let by_name = name(people, by);
                let Some(p) = people.get_mut(body) else { continue };
                if p.buried {
                    continue;
                }
                p.buried = true;
                p.buried_by = Some(by);
                let dead = p.name.clone();
                let grave = c.node_kind("grave");
                // On the spot, or the nearest free land; but not among the living by the fire:
                // a death there is buried at least 8 m out, on the side where the body lay.
                let (fx, fy) = camp_fire;
                let (dx, dy) = (at.0 as f32 + 0.5 - fx, at.1 as f32 + 0.5 - fy);
                let d = (dx * dx + dy * dy).sqrt();
                let at = if d < 8.0 {
                    let (ux, uy) = if d > 0.1 { (dx / d, dy / d) } else { (0.0, 1.0) };
                    let (x, y) = (fx + ux * 9.0, fy + uy * 9.0);
                    (x.clamp(0.0, map.size as f32 - 1.0) as u16, y.clamp(0.0, map.size as f32 - 1.0) as u16)
                } else {
                    at
                };
                let mut placed = false;
                'find: for r in 0..4i32 {
                    for dy in -r..=r {
                        for dx in -r..=r {
                            let (x, y) = (at.0 as i32 + dx, at.1 as i32 + dy);
                            if x < 0 || y < 0 || x >= map.size as i32 || y >= map.size as i32 {
                                continue;
                            }
                            let i = map.idx(x as u32, y as u32);
                            if map.node_at[i] == 0 && !map.water[i].is_water() {
                                let id = nodes.insert(Node { kind: grave, x: x as u16, y: y as u16, variant: 0, turn: (body.index % 4) as u8, amount: 1.0 });
                                map.node_at[i] = id.index + 1;
                                map.version += 1;
                                placed = true;
                                if let Some(p) = people.get_mut(body) {
                                    p.grave = Some((x as u16, y as u16));
                                }
                                break 'find;
                            }
                        }
                    }
                }
                let _ = placed;
                events.push(Event::Buried { person: body, name: dead, by: by_name });
            }
            Effect::Tell { from, to, nodes: told, water } => {
                let Some(p) = people.get_mut(to) else { continue };
                for (id, k) in told {
                    let newer = p.knowledge.nodes.get(&id).is_none_or(|have| have.when < k.when);
                    if newer {
                        p.knowledge.nodes.insert(id, KnownNode { source: Source::Told(from), trust: 0.8, ..k });
                    }
                }
                for w in water {
                    if !p.knowledge.water.iter().any(|h| (h.x as i32 - w.x as i32).abs() + (h.y as i32 - w.y as i32).abs() < 10) {
                        p.knowledge.water.push(KnownWater { source: Source::Told(from), ..w });
                    }
                }
            }
            Effect::Opinion { who, about, toward } => {
                if let Some(p) = people.get_mut(who)
                    && who != about
                {
                    let r = p.rel_mut(about);
                    r.affection += (toward - r.affection) * 0.2;
                }
            }
        }
    }
}
