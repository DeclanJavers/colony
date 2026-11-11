//! The brain plug (ARCHITECTURE.md section 6) and decision traces (section 7).
//!
//! When someone is free, the sim builds a `DecisionRequest`: their situation as named numbers
//! (features) and as text, plus concrete options (each with its own features, or why it isn't
//! possible). A brain answers with a score per option and a choice. Every decision becomes a
//! `Trace`: what the brain saw, every score, the rules brain's second opinion, the choice, the
//! reason, and later what happened. Traces are also the training data.
//!
//! `RulesBrain` (hand-written scores over the features) lives here because the sim always needs
//! it: it's the fallback and the second opinion. Other brains live in their own crate.

use crate::actions::Kind;
use crate::people::social::Ask;
use crate::people::PersonId;
use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub label: String,
    pub kind: Kind,
    /// Named numbers for this option (distance, minutes, how much each need it relieves, what
    /// it gains for the stores...).
    pub features: Vec<(String, f32)>,
    pub possible: bool,
    pub why_not: Option<String>,
}

impl Choice {
    pub fn f(&self, name: &str) -> f32 {
        self.features.iter().find(|(k, _)| k == name).map_or(0.0, |(_, v)| *v)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub person: PersonId,
    pub tick: u64,
    /// Named numbers for the person's situation (needs, stores, hour, traits...).
    pub state: Vec<(String, f32)>,
    pub options: Vec<Choice>,
    /// The same situation and options in words, exactly as a text brain would be sent it.
    pub text: String,
}

impl DecisionRequest {
    pub fn s(&self, name: &str) -> f32 {
        self.state.iter().find(|(k, _)| k == name).map_or(0.0, |(_, v)| *v)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    /// One score per option (any scale; impossible options are ignored).
    pub scores: Vec<f32>,
    pub choice: usize,
    pub reason: Option<String>,
}

/// A brain decides for people. Answers may come back later (a subagent thinking): `submit` hands
/// over requests and `poll` returns whatever is finished.
pub trait Brain: Send {
    fn id(&self) -> &str;
    fn submit(&mut self, req: DecisionRequest);
    fn poll(&mut self) -> Vec<(DecisionRequest, Answer)>;
    /// Decisions handed over and still being thought about (the sim pauses for these in
    /// lockstep, and the UI says so).
    fn in_flight(&self) -> usize {
        0
    }
    /// Decisions collected but not yet handed over.
    fn queued(&self) -> usize {
        0
    }
    /// Hand over whatever is collected now (everyone is waiting: no point holding on).
    fn flush(&mut self) {}
    /// Why this brain has stopped answering (a usage limit), if it has.
    fn stopped(&self) -> Option<String> {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Ending {
    Done,
    Failed(String),
    Interrupted(String),
}

/// What happened after a decision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub result: Ending,
    pub minutes: u64,
    /// Change in each need from start to end (negative = relieved).
    pub need_change: Vec<f32>,
    /// Items added to the stores.
    pub stored: Vec<(String, u32)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    pub seed: u64,
    pub tick: u64,
    pub when: String,
    pub person: PersonId,
    pub name: String,
    pub brain: String,
    pub mode: String,
    pub request: DecisionRequest,
    pub scores: Vec<f32>,
    /// The rules brain's scores for the same request (the second opinion).
    pub rules_scores: Vec<f32>,
    pub choice: usize,
    pub rules_choice: usize,
    pub reason: Option<String>,
    pub latency_us: u64,
    pub fallback: bool,
    pub outcome: Option<Outcome>,
}

// ---------------------------------------------------------------------------------------------
// The rules brain

/// How urgent a need feels: gentle at first, steep near the top.
pub fn urgency(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * 0.3 + x * x * 2.2
}

/// Hand-written scores over the features. A little noise keeps people from moving in lockstep; it
/// comes from the seed, the person and the tick, so the brain keeps no state (nothing to save,
/// and the same request always gets the same scores).
pub struct RulesBrain {
    seed: u64,
    out: Vec<(DecisionRequest, Answer)>,
}

impl RulesBrain {
    pub fn new(seed: u64) -> Self {
        RulesBrain { seed, out: Vec::new() }
    }

    /// Score every option (impossible ones get -inf).
    pub fn score(req: &DecisionRequest, seed: u64) -> Vec<f32> {
        let noise = &mut Rng::new(crate::hash::hash2(seed ^ 0x5EED, req.person.index as i32, req.tick as i32));
        let need = |n: &str| req.s(&format!("need.{n}"));
        let diligence = req.s("trait.diligence");
        let curiosity = req.s("trait.curiosity");
        let sociability = req.s("trait.sociability");
        let sleepy = req.s("sleepy");
        let value = |item: &str| req.s(&format!("value.{item}"));
        req.options
            .iter()
            .map(|o| {
                if !o.possible {
                    return f32::NEG_INFINITY;
                }
                let relief = |n: &str| o.f(&format!("relief.{n}"));
                let mut s = 0.0;
                // Needs it eases, weighted by how urgent each is.
                // Needs that can kill (hunger, thirst, cold) weigh most.
                for (n, w) in [("hunger", 1.6), ("thirst", 1.6), ("warmth", 1.6), ("rest", 0.9), ("social", 0.8), ("fun", 0.8)] {
                    s += urgency(need(n)) * relief(n) * w;
                }
                // What it brings back for everyone.
                let mut gain = 0.0;
                for it in ["berries", "water", "wood", "sticks", "stone", "grain", "meat"] {
                    gain += o.f(&format!("gain.{it}")) * value(it);
                }
                s += gain * (0.85 + 0.3 * diligence);
                // Something the voice from the sky asked for (M8, god tool "speak").
                s += 1.0 * o.f("asked");
                // Time costs.
                s -= o.f("minutes") * 0.0015;
                s += match &o.kind {
                    // The body clock makes them sleep at night, but only if they're tired too.
                    Kind::Sleep | Kind::SleepHere => 2.4 * sleepy * (need("rest") * 2.5).min(1.0) - 1.2 * (1.0 - sleepy),
                    // Sitting by the fire: more when tired (free time can go on other things, M7).
                    Kind::Rest => 0.1 + 0.12 * (1.0 - diligence) + 0.25 * need("rest") - 0.15 * (1.0 - req.s("food_days") / 3.0).clamp(0.0, 1.0) - 0.6 * sleepy,
                    // Exploring in the evening strands them far from the fire at bedtime.
                    Kind::Explore { .. } => 0.1 + 0.45 * curiosity * (1.0 - req.s("explored")) + 0.5 * req.s("food_unknown") - 0.6 * req.s("evening") - 0.8 * sleepy,
                    Kind::LightFire => 0.3 + 1.6 * req.s("cold") + 0.9 * req.s("evening") + 0.5 * req.s("winter"),
                    Kind::TendFire => (0.2 + 1.4 * req.s("cold") + 0.8 * req.s("evening") + 0.4 * req.s("winter")) * (1.0 - req.s("fire_hours") / 6.0).max(0.0),
                    Kind::CheckStores => 0.05 * req.s("stores_age_hours").min(24.0) / 24.0,
                    // Social (M5)
                    Kind::Talk { .. } => 0.0 + 0.1 * sociability + 0.02 * o.f("news") + 0.05 * o.f("affection").max(0.0) - 0.9 * sleepy,
                    Kind::Court { .. } => 0.1 + 0.15 * sociability + 0.3 * o.f("romance") + 0.1 * o.f("affection").max(0.0) - 0.9 * sleepy,
                    Kind::Ask { ask: Ask::Partner, .. } => 0.5 + 0.8 * o.f("romance") - 0.6 * sleepy,
                    Kind::Answer { ask: Ask::Partner, .. } => -0.3 + 1.3 * o.f("romance") + 0.4 * o.f("affection"),
                    Kind::Ask { ask: Ask::Child, .. } | Kind::Answer { ask: Ask::Child, .. } => {
                        let food = (o.f("food_days") / 5.0).min(1.0);
                        let answer = matches!(o.kind, Kind::Answer { .. }) as u8 as f32;
                        0.15 + 0.1 * answer + 0.2 * o.f("romance") + 0.35 * food - 0.3 * o.f("young_children") - if o.f("food_days") < 2.0 { 0.6 } else { 0.0 } - 0.5 * sleepy
                    }
                    Kind::Ask { ask: Ask::Teach { .. }, .. } => {
                        let useful = o.f("useful");
                        0.02 + 0.35 * (1.0 - o.f("knowers")).powi(2) * useful + 0.1 * o.f("kin") * useful + 0.05 * diligence - 0.25 * (1.0 - useful) - 0.7 * sleepy
                    }
                    Kind::Answer { ask: Ask::Teach { .. }, .. } => 0.1 + 0.35 * (1.0 - o.f("knowers")) * o.f("taught") * o.f("useful") + 0.15 * curiosity - 0.6 * sleepy,
                    Kind::Ask { ask: Ask::Food, .. } => 0.1,
                    Kind::Answer { ask: Ask::Food, .. } => 0.2 + 0.3 * o.f("affection") + 0.3 * req.s("trait.warmth") + 0.6 * o.f("their_hunger") - urgency(need("hunger")),
                    Kind::Watch { .. } => 0.06 + 0.15 * curiosity + 0.1 * o.f("taught") * (1.0 - o.f("my_level")) * o.f("useful") - 0.7 * sleepy,
                    Kind::Play => 0.15 - 0.9 * sleepy,
                    Kind::Feed { .. } => urgency(o.f("child_hunger")) * 1.5 * (0.4 + 0.6 * o.f("kin")) + 0.1,
                    Kind::Bury { .. } => 0.2 + 0.4 * o.f("kin") + 0.1 * o.f("days_dead").min(3.0) - 0.9 * sleepy,
                    // Settling down (M6)
                    Kind::Found { .. } if o.f("homeless") > 0.0 => {
                        (0.22 + 0.25 * o.f("family") + 0.3 * req.s("night_cold").min(1.0) + 0.15 * diligence) * (0.4 + 0.6 * o.f("materials")) - 0.9 * sleepy
                    }
                    Kind::Found { .. } if o.f("seed") > 0.0 => {
                        let worry = (1.0 - req.s("food_days") / 6.0).clamp(0.0, 1.0);
                        0.12 + 0.35 * worry + 0.1 * o.f("seed").min(3.0) / 3.0 + 0.1 * diligence - 0.1 * o.f("fields") - 0.8 * sleepy
                    }
                    Kind::Found { .. } => (0.05 + 0.08 * o.f("spoiling").min(6.0)) * (0.4 + 0.6 * o.f("materials")) + 0.05 * diligence - 0.9 * sleepy,
                    Kind::Build { .. } => {
                        let mine = o.f("mine") * o.f("home");
                        0.12 + 0.25 * o.f("progress") + 0.35 * mine + 0.2 * mine * req.s("night_cold").min(1.0) + 0.2 * o.f("field") + 0.15 * diligence + 0.1 * o.f("materials") - 0.9 * sleepy
                    }
                    Kind::SleepHome { .. } => 2.4 * sleepy * (need("rest") * 2.5).min(1.0) - 1.2 * (1.0 - sleepy) + 0.3 * o.f("shelter") + 0.3 * o.f("cold") * o.f("shelter"),
                    Kind::Sow { .. } => 0.45 + 0.3 * (1.0 - req.s("food_days") / 6.0).clamp(0.0, 1.0) - 0.6 * sleepy,
                    Kind::Tend { .. } => 0.15 + 0.8 * o.f("weeds") * (1.0 - o.f("growth")).max(0.2) - 0.6 * sleepy,
                    Kind::Harvest { .. } => 0.6 - 0.6 * sleepy,
                    // What packs add (pack.rs): the pack weighs its own actions.
                    Kind::Pack { pack, act, .. } => crate::pack::get(*pack).rules_score(*act, o, req),
                    // Free time (M7): what suits them, their hobbies, and (when beauty matters)
                    // somewhere lovely. Not while food is short.
                    Kind::Leisure { .. } => {
                        let worry = (1.0 - req.s("food_days") / 1.5).clamp(0.0, 1.0);
                        // The more bored, the more it draws them; what suits them draws them most.
                        let drive = (0.5 + 1.2 * need("fun")).min(1.0);
                        let taste = (o.f("appeal") - 0.3).max(0.0) * 0.7;
                        // Free time spent is no loss: most of its length isn't counted against it.
                        drive * (0.05 + taste + 0.2 * o.f("hobby") + 0.3 * o.f("beauty") + 0.1 * o.f("makes")) + 0.06 * o.f("company") + 0.06 * o.f("evening")
                            + 0.0009 * o.f("minutes")
                            - 0.35 * worry
                            - 0.4 * req.s("cold")
                            // Not while hungry, thirsty or cold themselves.
                            - 1.2 * (need("hunger").max(need("thirst")).max(need("warmth")) - 0.4).max(0.0)
                            - 0.8 * sleepy
                    }
                    _ => -0.6 * sleepy,
                };
                s + (noise.next_f32() - 0.5) * 0.06
            })
            .collect()
    }

    pub fn pick(scores: &[f32]) -> usize {
        let mut best = 0;
        for (i, s) in scores.iter().enumerate() {
            if *s > scores[best] {
                best = i;
            }
        }
        best
    }
}

impl Brain for RulesBrain {
    fn id(&self) -> &str {
        "rules"
    }
    fn submit(&mut self, req: DecisionRequest) {
        let scores = Self::score(&req, self.seed);
        let choice = Self::pick(&scores);
        self.out.push((req, Answer { scores, choice, reason: None }));
    }
    fn poll(&mut self) -> Vec<(DecisionRequest, Answer)> {
        std::mem::take(&mut self.out)
    }
}

/// The brains available to a world, by id.
pub struct Brains {
    pub map: BTreeMap<String, Box<dyn Brain>>,
}

impl Brains {
    pub fn base(seed: u64) -> Self {
        let mut map: BTreeMap<String, Box<dyn Brain>> = BTreeMap::new();
        map.insert("rules".into(), Box::new(RulesBrain::new(seed)));
        Brains { map }
    }
}

/// Scores to probabilities, for showing how sure a brain was.
pub fn softmax(scores: &[f32], temperature: f32) -> Vec<f32> {
    let m = scores.iter().cloned().filter(|s| s.is_finite()).fold(f32::MIN, f32::max);
    let e: Vec<f32> = scores.iter().map(|s| if s.is_finite() { ((s - m) / temperature).exp() } else { 0.0 }).collect();
    let t: f32 = e.iter().sum::<f32>().max(1e-9);
    e.iter().map(|v| v / t).collect()
}
