//! The life course (M5): babies kept close and fed, pregnancy and birth with inheritance, coming
//! of age, ageing and old age, grief, relationships that grow with time together and fade, and
//! skills lost with their last holder. Numbers are in data/base/life.ron.

use crate::actions::Kind;
use crate::content::Content;
use crate::events::Event;
use crate::ids::Table;
use crate::people::genes::{self, Genome};
use crate::people::social::{Family, Memory, Rel, Stage};
use crate::people::{Moments, Person, PersonId, Sex, Tally, other};
use crate::rng::Rng;
use std::collections::{BTreeMap, VecDeque};

/// What one person can see of another this tick (actions read these instead of the table).
#[derive(Clone, Debug)]
pub struct Seen {
    pub id: PersonId,
    pub name: String,
    pub pos: (f32, f32),
    pub alive: bool,
    pub buried: bool,
    pub asleep: bool,
    pub stage: Stage,
    pub age: f32,
    pub sex: Sex,
    pub drawn_to: Sex,
    pub personality: [f32; 5],
    pub mother: Option<PersonId>,
    pub father: Option<PersonId>,
    pub partner: Option<PersonId>,
    pub fertile: bool,
    pub pregnant: bool,
    pub kind: Option<Kind>,
    pub anim: Option<crate::actions::Anim>,
    /// Skill levels.
    pub levels: Vec<u32>,
    pub hunger: f32,
    pub thirst: f32,
    pub carrying_food: u32,
    /// Who they've asked and haven't had an answer from (to avoid asking twice).
    pub asked: Vec<PersonId>,
    pub died: Option<u64>,
}

pub fn seen(people: &Table<Person>, tick: u64, c: &Content) -> Vec<Seen> {
    let berries = c.item("berries");
    let hunger = c.need("hunger");
    let mut asked: BTreeMap<PersonId, Vec<PersonId>> = BTreeMap::new();
    for (id, p) in people.iter() {
        for inv in &p.invites {
            asked.entry(inv.from).or_default().push(id);
        }
    }
    people
        .iter()
        .map(|(id, p)| Seen {
            id,
            name: p.name.clone(),
            pos: p.pos,
            alive: p.alive,
            buried: p.buried,
            asleep: p.asleep,
            stage: p.stage(tick, c),
            age: p.age_years(tick, c),
            sex: p.sex,
            drawn_to: p.drawn_to,
            personality: p.personality,
            mother: p.family.mother,
            father: p.family.father,
            partner: p.family.partner,
            fertile: p.alive && p.fertile(tick, c),
            pregnant: p.pregnant.is_some(),
            kind: p.action.as_ref().map(|a| a.kind.clone()),
            anim: p.action.as_ref().map(|a| a.anim),
            levels: (0..c.skills.skills.len()).map(|k| p.skill_level(k, c)).collect(),
            hunger: p.needs[hunger],
            thirst: p.needs[c.need("thirst")],
            carrying_food: p.carry[berries],
            asked: asked.remove(&id).unwrap_or_default(),
            died: p.died.as_ref().map(|d| d.0),
        })
        .collect()
}

pub fn find(seen: &[Seen], id: PersonId) -> Option<&Seen> {
    seen.iter().find(|s| s.id == id)
}

/// How close two people are as family: 1 partner, parent or child; 0.8 sibling; 0.5
/// grandparent, grandchild, aunt or uncle; 0.25 cousin; 0 otherwise.
pub fn kinship(seen: &[Seen], a: PersonId, b: PersonId) -> f32 {
    let partners = find(seen, a).is_some_and(|x| x.partner == Some(b)) || find(seen, b).is_some_and(|y| y.partner == Some(a));
    if partners { 1.0 } else { blood(seen, a, b) }
}

/// Blood relation only (as `kinship`, without partners).
pub fn blood(seen: &[Seen], a: PersonId, b: PersonId) -> f32 {
    if a == b {
        return 1.0;
    }
    let (Some(x), Some(y)) = (find(seen, a), find(seen, b)) else { return 0.0 };
    let parents = |s: &Seen| [s.mother, s.father].into_iter().flatten().collect::<Vec<_>>();
    let (px, py) = (parents(x), parents(y));
    if px.contains(&b) || py.contains(&a) {
        return 1.0;
    }
    if px.iter().any(|p| py.contains(p)) {
        return 0.8;
    }
    let grand = |ps: &[PersonId]| ps.iter().filter_map(|p| find(seen, *p)).flat_map(parents).collect::<Vec<_>>();
    let (gx, gy) = (grand(&px), grand(&py));
    if gx.contains(&b) || gy.contains(&a) {
        return 0.5;
    }
    // Aunt or uncle: a sibling of a parent.
    let sibling_of = |s: &Seen, ps: &[PersonId]| ps.iter().filter_map(|p| find(seen, *p)).any(|p| parents(p).iter().any(|g| parents(s).contains(g)) && p.id != s.id);
    if sibling_of(x, &py) || sibling_of(y, &px) {
        return 0.5;
    }
    if gx.iter().any(|g| gy.contains(g)) {
        return 0.25;
    }
    0.0
}

/// Too closely related to be partners (anything above cousins).
pub fn close_kin(seen: &[Seen], a: PersonId, b: PersonId) -> bool {
    blood(seen, a, b) >= 0.5
}

fn memory(tick: u64, text: String, mood: String, about: Option<PersonId>, feeling: f32, importance: f32) -> Memory {
    Memory { tick, text, mood, about, feeling, importance, strength: 1.0 }
}

#[allow(clippy::too_many_arguments)]
pub fn remember(p: &mut Person, c: &Content, tick: u64, text: String, mood: String, about: Option<PersonId>, feeling: f32, importance: f32) {
    p.remember(memory(tick, text, mood, about, feeling, importance), c);
}

/// Someone dies: the body stays until buried, their partner is free again, those close to them
/// grieve, and a taught skill nobody living knows any more is lost.
pub fn die(people: &mut Table<Person>, id: PersonId, cause: String, tick: u64, c: &Content, events: &mut Vec<Event>) {
    let Some(p) = people.get_mut(id) else { return };
    if !p.alive {
        return;
    }
    p.alive = false;
    p.asleep = false;
    p.waiting = false;
    p.pregnant = None;
    p.invites.clear();
    p.died = Some((tick, cause.clone()));
    let name = p.name.clone();
    let knew: Vec<usize> = (0..c.skills.skills.len()).filter(|&k| c.skills.skills[k].taught && p.skill_level(k, c) >= 1).collect();
    events.push(Event::PersonDied { person: id, name: name.clone(), cause });
    let all = seen(people, tick, c);
    let ids: Vec<PersonId> = people.iter().filter(|(_, q)| q.alive).map(|(i, _)| i).collect();
    for q in ids {
        let kin = kinship(&all, q, id);
        let p = people.get_mut(q).unwrap();
        if p.family.partner == Some(id) {
            p.family.partner = None;
            remember(p, c, tick, format!("{name} died"), format!("Grieving {name}"), Some(id), -0.9, 0.95);
        } else if kin >= 0.8 {
            remember(p, c, tick, format!("{name} died"), format!("Grieving {name}"), Some(id), -0.75, 0.85);
        } else if kin >= 0.5 || p.rel(id).affection > 0.3 {
            remember(p, c, tick, format!("{name} died"), format!("Misses {name}"), Some(id), -0.4, 0.5);
        } else if p.rel(id).familiarity > 0.25 {
            remember(p, c, tick, format!("{name} died"), format!("Sad about {name}"), Some(id), -0.15, 0.3);
        }
        p.invites.retain(|i| i.from != id);
    }
    for k in knew {
        if !people.iter().any(|(_, q)| q.alive && q.skill_level(k, c) >= 1) {
            events.push(Event::SkillLost { skill: c.skills.skills[k].name.to_lowercase(), last: name.clone() });
        }
    }
}

/// A name nobody living has, from the pool for their sex.
fn child_name(people: &Table<Person>, c: &Content, sex: Sex, rng: &mut Rng) -> String {
    let pool = if sex == Sex::Female { &c.names.female } else { &c.names.male };
    let taken = |n: &str| people.iter().any(|(_, p)| p.alive && p.name == n);
    for _ in 0..40 {
        let n = &pool[rng.below(pool.len() as u32) as usize];
        if !taken(n) {
            return n.clone();
        }
    }
    let base = &pool[rng.below(pool.len() as u32) as usize];
    let mut k = 2;
    while taken(&format!("{base} {k}")) {
        k += 1;
    }
    format!("{base} {k}")
}

/// A baby, from its parents' DNA.
pub fn born(people: &Table<Person>, mother: PersonId, father: PersonId, tick: u64, c: &Content, map_size: u32, rng: &mut Rng) -> Person {
    let m = people.get(mother).unwrap();
    let f = people.get(father).unwrap();
    let sex = if rng.chance(0.5) { Sex::Female } else { Sex::Male };
    let genome = Genome::inherit(&m.genome, &f.genome, rng);
    let pal = &c.people.clothes;
    let clothes = crate::content::hex_colour(&pal[rng.below(pal.len() as u32) as usize]);
    let looks = genes::express(&genome, &c.genes, sex, clothes, rng);
    let personality = genes::personality(&genome, &c.genes, rng);
    let mut rels = BTreeMap::new();
    for parent in [mother, father] {
        rels.insert(parent, Rel { familiarity: 0.5, affection: 0.8, respect: 0.5, romance: 0.0, last: tick });
    }
    let pp = &c.people;
    Person {
        name: child_name(people, c, sex, rng),
        sex,
        born: tick as i64,
        genome,
        looks,
        personality,
        chronotype: (rng.next_f32() * 2.0 - 1.0) * pp.chronotype_spread,
        needs: c.needs.iter().map(|_| 0.1).collect(),
        health: 1.0,
        alive: true,
        died: None,
        practice: vec![0.0; c.skills.skills.len()],
        carry: vec![0; c.items.len()],
        // What a baby will know when it can walk: what it saw carried about by its mother.
        knowledge: { let _ = map_size; m.knowledge.clone() },
        pos: (m.pos.0 + 0.4, m.pos.1 + 0.3),
        facing: m.facing,
        asleep: false,
        waiting: false,
        action: None,
        open_trace: None,
        traces: VecDeque::new(),
        brain: m.brain.clone(),
        thought: String::new(),
        tally: Tally::default(),
        last: Moments { ate: tick, drank: tick, woke: tick, rested: tick },
        family: Family { mother: Some(mother), father: Some(father), partner: None, children: Vec::new(), generation: m.family.generation.max(f.family.generation) + 1 },
        drawn_to: if rng.chance(c.life.drawn_to_other) { other(sex) } else { sex },
        rels,
        memories: Vec::new(),
        invites: Vec::new(),
        pregnant: None,
        last_birth: None,
        buried: false,
        grave: None,
        buried_by: None,
        indoors: None,
        hobbies: Vec::new(),
        heard: None,
        said: None,
    }
}

/// Births due now. Returns the new people's ids.
pub fn births(people: &mut Table<Person>, tick: u64, c: &Content, map_size: u32, rng: &mut Rng, events: &mut Vec<Event>) -> Vec<PersonId> {
    let due: Vec<(PersonId, PersonId)> =
        people.iter().filter(|(_, p)| p.alive && p.pregnant.as_ref().is_some_and(|g| g.due <= tick)).map(|(id, p)| (id, p.pregnant.as_ref().unwrap().father)).collect();
    let mut out = Vec::new();
    for (mother, father) in due {
        // A father who died during the pregnancy is still the father.
        if people.get(father).is_none() {
            people.get_mut(mother).unwrap().pregnant = None;
            continue;
        }
        let baby = born(people, mother, father, tick, c, map_size, rng);
        let name = baby.name.clone();
        let parents = format!("{} and {}", people.get(mother).unwrap().name, people.get(father).unwrap().name);
        let id = people.insert(baby);
        out.push(id);
        for parent in [mother, father] {
            let p = people.get_mut(parent).unwrap();
            p.family.children.push(id);
            p.rels.insert(id, Rel { familiarity: 0.5, affection: 0.9, respect: 0.0, romance: 0.0, last: tick });
            if p.alive {
                remember(p, c, tick, format!("{name} was born"), format!("Joy at {name}'s birth"), Some(id), 0.9, 0.9);
            }
        }
        let m = people.get_mut(mother).unwrap();
        m.pregnant = None;
        m.last_birth = Some(tick);
        m.needs[c.need("rest")] = (m.needs[c.need("rest")] + 0.3).min(1.0);
        events.push(Event::Born { child: id, name, mother, father, parents });
        if rng.chance(c.life.childbirth_death) {
            die(people, mother, "childbirth".into(), tick, c, events);
        }
    }
    out
}

/// Who looks after a baby: their mother, else their father, else the closest grown relative,
/// else any grown person.
pub fn carer(seen: &[Seen], baby: &Seen) -> Option<PersonId> {
    let alive_grown = |id: Option<PersonId>| id.and_then(|i| find(seen, i)).filter(|s| s.alive && s.stage >= Stage::Adult).map(|s| s.id);
    if let Some(m) = alive_grown(baby.mother).or(alive_grown(baby.father)) {
        return Some(m);
    }
    seen.iter()
        .filter(|s| s.alive && s.stage >= Stage::Adult)
        .max_by(|a, b| kinship(seen, baby.id, a.id).total_cmp(&kinship(seen, baby.id, b.id)).then(b.id.cmp(&a.id)))
        .map(|s| s.id)
}

/// Babies stay at their carer's side (asleep when they sleep). Those still nursing feed from
/// their mother when she's close; older ones share their carer's meals and drinks.
pub fn babies(people: &mut Table<Person>, seen: &[Seen], tick: u64, c: &Content) {
    let tm = c.calendar.minutes_per_tick as f32 / 60.0;
    let (hunger, thirst, rest) = (c.need("hunger"), c.need("thirst"), c.need("rest"));
    for b in seen.iter().filter(|s| s.alive && s.stage == Stage::Baby) {
        let Some(cid) = carer(seen, b) else { continue };
        let carer = find(seen, cid).unwrap();
        let (cpos, casleep, canim) = (carer.pos, carer.asleep, carer.anim);
        let nursing = b.age < c.life.nursing_age && b.mother == Some(cid) && carer.hunger < 0.8;
        // A baby sleeps under the same roof as whoever has them.
        let roof = people.get(cid).and_then(|q| q.indoors);
        let baby = people.get_mut(b.id).unwrap();
        baby.indoors = if casleep { roof } else { None };
        let k = (b.id.index % 6) as f32;
        let target = (cpos.0 + 0.5 * (k * 1.1).cos(), cpos.1 + 0.5 * (k * 1.1).sin());
        let d = ((target.0 - baby.pos.0).powi(2) + (target.1 - baby.pos.1).powi(2)).sqrt();
        // Carried: babies keep up with whoever has them.
        if d > 0.05 {
            baby.facing = (target.0 - baby.pos.0).atan2(target.1 - baby.pos.1);
        }
        baby.pos = target;
        baby.asleep = casleep;
        baby.needs[rest] = 0.0;
        let close = ((cpos.0 - baby.pos.0).powi(2) + (cpos.1 - baby.pos.1).powi(2)).sqrt() < 3.0;
        if nursing && close {
            baby.needs[hunger] = (baby.needs[hunger] - 0.4 * tm).max(0.0);
            baby.needs[thirst] = (baby.needs[thirst] - 0.6 * tm).max(0.0);
            if baby.needs[hunger] < 0.05 {
                baby.last.ate = tick;
                baby.last.drank = tick;
            }
            let m = people.get_mut(cid).unwrap();
            m.needs[hunger] = (m.needs[hunger] + 0.02 * tm).min(1.0);
        } else if close {
            // Sharing: a drink takes a few minutes, a meal about a quarter of an hour.
            use crate::actions::Anim;
            let minutes = tm * 60.0;
            let baby = people.get_mut(b.id).unwrap();
            match canim {
                Some(Anim::Drink) | Some(Anim::DrawWater) => {
                    baby.needs[thirst] = (baby.needs[thirst] - 0.15 * minutes).max(0.0);
                    baby.last.drank = tick;
                }
                Some(Anim::Eat) => {
                    baby.needs[hunger] = (baby.needs[hunger] - 0.06 * minutes).max(0.0);
                    baby.last.ate = tick;
                }
                _ => {}
            }
        }
    }
}

/// Every hour: people close together get to know each other.
pub fn hourly(people: &mut Table<Person>, seen: &[Seen], tick: u64, c: &Content) {
    let reach = c.people.company_reach;
    let gain = c.life.familiarity_per_hour;
    let awake: Vec<&Seen> = seen.iter().filter(|s| s.alive && !s.asleep).collect();
    let mut pairs = Vec::new();
    for (i, a) in awake.iter().enumerate() {
        for b in &awake[i + 1..] {
            if ((a.pos.0 - b.pos.0).powi(2) + (a.pos.1 - b.pos.1).powi(2)).sqrt() <= reach {
                pairs.push((a.id, b.id));
            }
        }
    }
    for (a, b) in pairs {
        for (x, y) in [(a, b), (b, a)] {
            let r = people.get_mut(x).unwrap().rel_mut(y);
            r.familiarity = (r.familiarity + gain).min(1.0);
            r.last = tick;
        }
    }
    // Invitations nobody answered lapse.
    let lapse = (c.life.invite_hours * 60.0) as u64;
    let ids: Vec<PersonId> = people.iter().map(|(id, _)| id).collect();
    for id in ids {
        people.get_mut(id).unwrap().invites.retain(|i| tick.saturating_sub(i.tick) < lapse);
    }
}

/// Every morning: memories and feelings fade, people come of age, and old age takes some.
/// Returns those who died of old age.
pub fn daily(people: &mut Table<Person>, tick: u64, c: &Content, rng: &mut Rng, events: &mut Vec<Event>) {
    let l = &c.life;
    let day_years = c.calendar.ticks_per_day() as f64 / c.ticks_per_age_year();
    let ids: Vec<PersonId> = people.iter().filter(|(_, p)| p.alive).map(|(id, _)| id).collect();
    let mut old_age = Vec::new();
    for id in ids {
        let p = people.get_mut(id).unwrap();
        for m in p.memories.iter_mut() {
            m.strength *= 1.0 - l.memory_fade * (1.0 - m.importance);
        }
        p.memories.retain(|m| m.strength >= l.forget);
        let partner = p.family.partner;
        for (other, r) in p.rels.iter_mut() {
            r.affection *= l.affection_fade;
            if partner != Some(*other) {
                r.romance *= l.romance_fade;
            }
            r.familiarity = (r.familiarity - l.familiarity_fade).max(0.0);
        }
        let age = p.age_years(tick, c);
        let was = age - day_years as f32;
        if was < l.adult_age && age >= l.adult_age {
            events.push(Event::GrewUp { person: id, name: p.name.clone() });
            let name = p.name.clone();
            remember(p, c, tick, "Came of age".into(), "Grown up at last".into(), None, 0.3, 0.6);
            let _ = name;
        }
        if age >= l.elder_age {
            let cap = (1.0 - (age - l.elder_age) * l.elder_weakening).max(0.3);
            p.health = p.health.min(cap);
            let hazard = l.old_age_hazard * ((age - l.elder_age) / l.old_age_scale).exp();
            if rng.chance((hazard as f64 * day_years).min(1.0) as f32) {
                old_age.push(id);
            }
        }
    }
    for id in old_age {
        die(people, id, "old age".into(), tick, c, events);
    }
}
