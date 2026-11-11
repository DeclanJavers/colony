//! People (ARCHITECTURE.md section 4): identity, DNA and looks, body and needs, personality, skills,
//! knowledge, and what they're doing.

pub mod genes;
pub mod knowledge;

use crate::actions::Action;
use crate::brain::Trace;
use crate::content::{Content, hex_colour};
use crate::ids::Id;
use crate::rng::Rng;
use genes::{Genome, Looks};
use knowledge::Knowledge;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub type PersonId = Id<Person>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sex {
    Female,
    Male,
}

pub const TRAITS: [&str; 5] = ["curiosity", "diligence", "sociability", "warmth", "calm"];
/// Decision traces kept in memory per person.
pub const TRACE_KEEP: usize = 200;

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
}

impl Person {
    pub fn age_years(&self, tick: u64, c: &Content) -> f32 {
        let ticks_per_year = (c.calendar.ticks_per_day() * c.calendar.days_per_year()) as f64;
        ((tick as i64 - self.born) as f64 / ticks_per_year) as f32
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

    pub fn push_trace(&mut self, t: Trace) {
        if self.traces.len() >= TRACE_KEEP {
            self.traces.pop_front();
        }
        self.traces.push_back(t);
    }
}

/// The founding group: adults of mixed ages and sexes, random DNA from the population.
pub fn founders(c: &Content, n: u32, start: (u32, u32), map_size: u32, start_tick: u64, rng: &mut Rng) -> Vec<Person> {
    let p = &c.people;
    let ticks_per_year = (c.calendar.ticks_per_day() * c.calendar.days_per_year()) as i64;
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
            born: start_tick as i64 - (age * ticks_per_year as f32) as i64,
            genome,
            looks,
            personality,
            chronotype: (rng.next_f32() * 2.0 - 1.0) * p.chronotype_spread,
            needs: c.needs.iter().map(|_| rng.next_f32() * 0.25).collect(),
            health: 1.0,
            alive: true,
            died: None,
            practice: c.skills.skills.iter().map(|_| rng.next_f32() * 400.0).collect(),
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
        });
    }
    out
}
