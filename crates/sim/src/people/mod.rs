//! People (ARCHITECTURE.md section 4): identity, DNA and looks, body and needs, personality, skills,
//! knowledge, and what they're doing.

pub mod genes;
pub mod knowledge;
pub mod social;

use crate::actions::Action;
use crate::brain::Trace;
use crate::content::{Content, hex_colour};
use crate::ids::Id;
use crate::rng::Rng;
use genes::{Genome, Looks};
use knowledge::Knowledge;
use serde::{Deserialize, Serialize};
use social::{Family, Invite, Memory, Pregnancy, Rel, Stage};
use std::collections::{BTreeMap, VecDeque};

pub type PersonId = Id<Person>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sex {
    Female,
    Male,
}

pub const TRAITS: [&str; 5] = ["curiosity", "diligence", "sociability", "warmth", "calm"];
/// Decision traces kept in memory per person.
pub const TRACE_KEEP: usize = 200;

/// When things last happened to someone (ticks), for explaining their needs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Moments {
    pub ate: u64,
    pub drank: u64,
    pub woke: u64,
    pub rested: u64,
}

/// Counters for checks and the inspector.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    /// Minutes asleep between 08:00 and 18:00 (should be near zero: sleep follows the body clock).
    pub day_sleep_minutes: u64,
    pub awake_minutes: u64,
    pub decisions: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    pub sex: Sex,
    /// Tick of birth (negative for founders born before the game began).
    pub born: i64,
    pub genome: Genome,
    pub looks: Looks,
    /// curiosity, diligence, sociability, warmth, calm (0..1)
    pub personality: [f32; 5],
    /// Hours this person's body clock runs early (-) or late (+).
    pub chronotype: f32,
    /// One per need in data/base/needs.ron: 0 satisfied .. 1 desperate.
    pub needs: Vec<f32>,
    pub health: f32,
    pub alive: bool,
    /// (tick, cause)
    pub died: Option<(u64, String)>,
    /// Minutes practised, per skill.
    pub practice: Vec<f32>,
    /// What they carry, per item.
    pub carry: Vec<u32>,
    pub knowledge: Knowledge,
    /// Position in tiles (continuous).
    pub pos: (f32, f32),
    /// Facing, radians (0 = +z).
    pub facing: f32,
    pub asleep: bool,
    /// Waiting for a brain's answer.
    pub waiting: bool,
    pub action: Option<Action>,
    /// The trace of the decision that started the current action (finished when it ends).
    pub open_trace: Option<Trace>,
    pub traces: VecDeque<Trace>,
    /// Which brain decides for them.
    pub brain: String,
    /// The latest thought, in words.
    pub thought: String,
    pub tally: Tally,
    pub last: Moments,
    // Families and society (M5)
    pub family: Family,
    /// Which sex they're drawn to.
    pub drawn_to: Sex,
    /// How they feel about each person they've met.
    pub rels: BTreeMap<PersonId, Rel>,
    pub memories: Vec<Memory>,
    /// What others have asked them (answered at their next decision, or it lapses).
    pub invites: Vec<Invite>,
    pub pregnant: Option<Pregnancy>,
    /// Tick of their last child's birth (mothers).
    pub last_birth: Option<u64>,
    /// For the dead: buried yet? (Until then the body lies where they died.)
    pub buried: bool,
    /// Where they're buried, and who buried them.
    pub grave: Option<(u16, u16)>,
    pub buried_by: Option<PersonId>,
    /// Under a roof right now (the building kind): shelters them from cold and rain.
    pub indoors: Option<u16>,
    /// Free-time activities they've enjoyed (M7), and which became hobbies.
    #[serde(default)]
    pub hobbies: Vec<Hobby>,
    /// What they last heard you say (M8), while it still counts.
    #[serde(default)]
    pub heard: Option<crate::speak::Heard>,
    /// What they last said aloud, and when (M8: chat lines, shown as a speech bubble).
    #[serde(default)]
    pub said: Option<(String, u64)>,
}

impl Person {
    /// Age in years (people age `life.age_per_year` years per game year). For the dead, their
    /// age when they died.
    pub fn age_years(&self, tick: u64, c: &Content) -> f32 {
        let at = self.died.as_ref().map_or(tick, |d| d.0.min(tick));
        ((at as i64 - self.born) as f64 / c.ticks_per_age_year()) as f32
    }

    pub fn stage(&self, tick: u64, c: &Content) -> Stage {
        let a = self.age_years(tick, c);
        let l = &c.life;
        if a < l.child_age {
            Stage::Baby
        } else if a < l.adult_age {
            Stage::Child
        } else if a < l.elder_age {
            Stage::Adult
        } else {
            Stage::Elder
        }
    }

    pub fn grown(&self, tick: u64, c: &Content) -> bool {
        self.stage(tick, c) >= Stage::Adult
    }

    /// Do they know a skill? Taught skills need level 1; anyone can try the others.
    pub fn knows(&self, skill: usize, c: &Content) -> bool {
        !c.skills.skills[skill].taught || self.skill_level(skill, c) >= 1
    }

    /// Can they have a child now (age, sex, not already expecting, not too soon after the last)?
    pub fn fertile(&self, tick: u64, c: &Content) -> bool {
        let a = self.age_years(tick, c);
        let l = &c.life;
        match self.sex {
            Sex::Female => {
                let gap = self.last_birth.is_none_or(|b| (tick.saturating_sub(b)) as f64 >= l.birth_gap_years as f64 * c.ticks_per_age_year());
                (l.fertile_female.0..l.fertile_female.1).contains(&a) && self.pregnant.is_none() && gap
            }
            Sex::Male => (l.fertile_male.0..l.fertile_male.1).contains(&a),
        }
    }

    pub fn rel(&self, other: PersonId) -> Rel {
        self.rels.get(&other).cloned().unwrap_or_default()
    }

    pub fn rel_mut(&mut self, other: PersonId) -> &mut Rel {
        self.rels.entry(other).or_default()
    }

    /// Remember something. A memory of the same thing about the same person within a day is
    /// refreshed instead of repeated.
    pub fn remember(&mut self, m: Memory, c: &Content) {
        let day = c.calendar.ticks_per_day();
        if let Some(old) = self.memories.iter_mut().find(|o| o.text == m.text && o.about == m.about && m.tick.saturating_sub(o.tick) < day) {
            old.strength = 1.0;
            old.tick = m.tick;
            return;
        }
        self.memories.push(m);
        if self.memories.len() > c.life.keep_memories {
            // Forget the weakest (least strong, least important).
            let weakest = (0..self.memories.len())
                .min_by(|&a, &b| {
                    let s = |m: &Memory| m.strength * (0.3 + m.importance);
                    s(&self.memories[a]).total_cmp(&s(&self.memories[b]))
                })
                .unwrap();
            self.memories.remove(weakest);
        }
    }

    pub fn skill_level(&self, skill: usize, c: &Content) -> u32 {
        ((self.practice[skill] / c.skills.minutes_per_level_sq).sqrt().floor() as u32).min(10)
    }

    /// Work speed from skill: 0.6 at level 0 to 1.2 at level 10.
    pub fn work_speed(&self, skill: usize, c: &Content) -> f32 {
        0.6 + 0.06 * self.skill_level(skill, c) as f32
    }

    pub fn carried(&self) -> u32 {
        self.carry.iter().sum()
    }

    /// The most pressing body need above 0.5, if any: (index, value).
    pub fn pressing(&self, c: &Content) -> Option<(usize, f32)> {
        self.needs
            .iter()
            .enumerate()
            .filter(|(i, v)| c.needs[*i].kind == crate::content::NeedKind::Body && **v >= 0.5)
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, v)| (i, *v))
    }

    /// How they feel overall, and why: (score, reasons). Each reason is (points, words).
    /// `beauty` is the beauty where they are, as it counts (0 when beauty doesn't matter).
    pub fn mood(&self, c: &Content, company: bool, by_fire: bool, beauty: f32) -> (i32, Vec<(i32, String)>) {
        let mut why: Vec<(i32, String)> = Vec::new();
        for (i, n) in c.needs.iter().enumerate() {
            let v = self.needs[i];
            if v >= 0.5 {
                let weight = if n.kind == crate::content::NeedKind::Body { 8.0 } else { 5.0 };
                why.push((-((v - 0.4) * weight).round() as i32, capitalise(n.word(v))));
            }
        }
        let n = |id: &str| self.needs[c.need(id)];
        if n("hunger") < 0.25 && n("thirst") < 0.25 {
            why.push((2, "Fed and watered".into()));
        }
        if n("rest") < 0.25 {
            why.push((1, "Well rested".into()));
        }
        if company && n("social") < 0.4 {
            why.push((2, "Good company".into()));
        }
        if by_fire {
            why.push((1, "By the fire".into()));
        }
        let lovely = (beauty * c.leisure.beauty.mood).round() as i32;
        if lovely > 0 {
            why.push((lovely, "Lovely here".into()));
        }
        if self.health < 0.7 {
            why.push((-((1.0 - self.health) * 8.0).round() as i32, "Weak and unwell".into()));
        }
        // Fresh memories: grief, joy, quarrels.
        for m in &self.memories {
            let v = (m.feeling * m.strength * 6.0).round() as i32;
            if v != 0 {
                why.push((v, m.mood.clone()));
            }
        }
        if self.pregnant.is_some() {
            why.push((1, "Expecting a child".into()));
        }
        // The same reason twice (two quarrels) counts once, at its strongest.
        let mut seen: Vec<(i32, String)> = Vec::new();
        for (v, r) in why {
            match seen.iter_mut().find(|(_, s)| *s == r) {
                Some(e) if v.abs() > e.0.abs() => e.0 = v,
                Some(_) => {}
                None => seen.push((v, r)),
            }
        }
        let mut why = seen;
        why.retain(|(v, _)| *v != 0);
        why.sort_by(|a, b| a.0.abs().cmp(&b.0.abs()).reverse());
        (why.iter().map(|(v, _)| v).sum(), why)
    }

    pub fn mood_word(score: i32) -> &'static str {
        match score {
            s if s >= 5 => "Happy",
            s if s >= 2 => "Content",
            s if s >= -1 => "Okay",
            s if s >= -4 => "Uneasy",
            s if s >= -8 => "Unhappy",
            _ => "Miserable",
        }
    }

    pub fn push_trace(&mut self, t: Trace) {
        if self.traces.len() >= TRACE_KEEP {
            self.traces.pop_front();
        }
        self.traces.push_back(t);
    }
}

/// How much someone has enjoyed a free-time activity; past a threshold it's their hobby.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hobby {
    /// Which activity (data/base/leisure.ron order).
    pub act: u16,
    pub joy: f32,
    pub times: u32,
    /// When it became a hobby.
    pub since: Option<u64>,
}

impl Person {
    /// Their hobbies (activity indices), in the order they took them up.
    pub fn hobby_acts(&self) -> Vec<u16> {
        let mut h: Vec<&Hobby> = self.hobbies.iter().filter(|h| h.since.is_some()).collect();
        h.sort_by_key(|h| h.since);
        h.into_iter().map(|h| h.act).collect()
    }

    pub fn has_hobby(&self, act: u16) -> bool {
        self.hobbies.iter().any(|h| h.act == act && h.since.is_some())
    }
}

pub fn other(s: Sex) -> Sex {
    if s == Sex::Female { Sex::Male } else { Sex::Female }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// The founding group: adults of mixed ages and sexes, random DNA from the population.
pub fn founders(c: &Content, n: u32, start: (u32, u32), map_size: u32, start_tick: u64, rng: &mut Rng) -> Vec<Person> {
    let p = &c.people;
    let ticks_per_year = c.ticks_per_age_year();
    let mut used = Vec::new();
    let mut out = Vec::new();
    for k in 0..n {
        let sex = if k % 2 == 0 { Sex::Female } else { Sex::Male };
        let pool = if sex == Sex::Female { &c.names.female } else { &c.names.male };
        let mut name = pool[rng.below(pool.len() as u32) as usize].clone();
        while used.contains(&name) {
            name = pool[rng.below(pool.len() as u32) as usize].clone();
        }
        used.push(name.clone());
        let age = p.start_age.0 as f32 + rng.next_f32() * (p.start_age.1 - p.start_age.0) as f32;
        let genome = Genome::random(&c.genes, rng);
        let clothes = hex_colour(&p.clothes[(k as usize) % p.clothes.len()]);
        let looks = genes::express(&genome, &c.genes, sex, clothes, rng);
        let personality = genes::personality(&genome, &c.genes, rng);
        let a = k as f32 / n as f32 * std::f32::consts::TAU;
        let pos = (start.0 as f32 + 0.5 + a.cos() * 2.5, start.1 as f32 + 0.5 + a.sin() * 2.5);
        out.push(Person {
            name,
            sex,
            born: start_tick as i64 - (age as f64 * ticks_per_year) as i64,
            genome,
            looks,
            personality,
            chronotype: (rng.next_f32() * 2.0 - 1.0) * p.chronotype_spread,
            needs: c.needs.iter().map(|_| rng.next_f32() * 0.25).collect(),
            health: 1.0,
            alive: true,
            died: None,
            // Everyone can forage, cut wood and explore; only some know the taught skills (below).
            practice: c.skills.skills.iter().map(|s| if s.taught { 0.0 } else { 40.0 + rng.next_f32() * 360.0 }).collect(),
            carry: vec![0; c.items.len()],
            knowledge: Knowledge::new(map_size, c.items.len()),
            pos,
            facing: a + std::f32::consts::PI,
            asleep: false,
            waiting: false,
            action: None,
            open_trace: None,
            traces: VecDeque::new(),
            brain: "rules".into(),
            thought: "We made it. This looks like a good place to stay.".into(),
            tally: Tally::default(),
            last: Moments { ate: start_tick, drank: start_tick, woke: start_tick, rested: start_tick },
            family: Family::default(),
            drawn_to: if rng.chance(c.life.drawn_to_other) { other(sex) } else { sex },
            rels: BTreeMap::new(),
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
        });
    }
    // A taught skill is known by its share of the group (at least one person).
    for (k, s) in c.skills.skills.iter().enumerate() {
        if !s.taught {
            continue;
        }
        let want = ((s.founders_know * n as f32).round() as usize).clamp(1, out.len());
        let mut order: Vec<usize> = (0..out.len()).collect();
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i as u32 + 1) as usize);
        }
        for &i in &order[..want] {
            out[i].practice[k] = 40.0 + rng.next_f32() * 360.0;
        }
    }
    out
}
