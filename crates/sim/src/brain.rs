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
                for it in ["berries", "water", "wood", "sticks", "stone"] {
                    gain += o.f(&format!("gain.{it}")) * value(it);
                }
                s += gain * (0.85 + 0.3 * diligence);
                // Time costs.
                s -= o.f("minutes") * 0.0015;
                s += match &o.kind {
                    // The body clock makes them sleep at night, but only if they're tired too.
                    Kind::Sleep | Kind::SleepHere => 2.4 * sleepy * (need("rest") * 2.5).min(1.0) - 1.2 * (1.0 - sleepy),
                    Kind::Rest => 0.15 + 0.15 * (1.0 - diligence) - 0.6 * sleepy,
                    Kind::Explore { .. } => 0.1 + 0.45 * curiosity * (1.0 - req.s("explored")) + 0.5 * req.s("food_unknown") - 0.8 * sleepy,
                    Kind::LightFire => 0.3 + 1.6 * req.s("cold") + 0.9 * req.s("evening") + 0.5 * req.s("winter"),
                    Kind::TendFire => (0.2 + 1.4 * req.s("cold") + 0.8 * req.s("evening") + 0.4 * req.s("winter")) * (1.0 - req.s("fire_hours") / 6.0).max(0.0),
                    Kind::CheckStores => 0.05 * req.s("stores_age_hours").min(24.0) / 24.0,
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
