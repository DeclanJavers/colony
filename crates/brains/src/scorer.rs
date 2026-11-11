//! The scorer (ARCHITECTURE.md section 9): a small model over a decision's named numbers, trained
//! on another brain's choices (the teacher: Haiku through the bridge, or the rules brain) and run
//! natively in well under a millisecond.
//!
//! **The model.** An option's score is a weight per feature of the option (distance, relief,
//! gain), plus a weight per feature of the situation for that option's kind of action. The first
//! part ranks options by what they offer; the second lets the situation pull towards some kinds of
//! action and away from others. Trained by softmax cross-entropy over each decision's possible
//! options, with Adam and a little L2. Weights are a plain JSON file.

use serde::{Deserialize, Serialize};
use sim::brain::{Answer, Brain, Choice, DecisionRequest};
use std::collections::BTreeMap;

/// One decision as training data: the situation, the possible options, and what the teacher chose.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Example {
    pub seed: u64,
    pub teacher: String,
    pub state: Vec<(String, f32)>,
    /// (action kind, the option's features), possible options only.
    pub options: Vec<(String, Vec<(String, f32)>)>,
    pub choice: usize,
}

impl Example {
    /// From a decision trace (None if the choice wasn't a possible option, or there's no choice to
    /// learn from).
    pub fn from_trace(t: &sim::brain::Trace, act_of: &dyn Fn(&Choice) -> String) -> Option<Example> {
        let possible: Vec<usize> = t.request.options.iter().enumerate().filter(|(_, o)| o.possible).map(|(i, _)| i).collect();
        let choice = possible.iter().position(|&i| i == t.choice)?;
        if possible.len() < 2 || t.fallback {
            return None;
        }
        Some(Example {
            seed: t.seed,
            teacher: t.brain.clone(),
            state: t.request.state.clone(),
            options: possible.iter().map(|&i| (act_of(&t.request.options[i]), t.request.options[i].features.clone())).collect(),
            choice,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Scorer {
    pub name: String,
    /// What it was trained on, and how well it agreed with the teacher on held-out seeds.
    pub trained_on: String,
    pub held_out_agreement: f32,
    /// The same, counting a match on the kind of action (chop, forage...) rather than the option.
    #[serde(default)]
    pub held_out_kind_agreement: f32,
    pub option_w: BTreeMap<String, f32>,
    /// act -> situation feature -> weight.
    pub state_w: BTreeMap<String, BTreeMap<String, f32>>,
}

/// Feature values are squashed so no one feature (minutes, distance) swamps the rest: small
/// values pass through, big ones grow with their logarithm.
fn squash(v: f32) -> f32 {
    let a = v.abs();
    if !a.is_finite() {
        0.0
    } else if a <= 1.0 {
        v
    } else {
        v.signum() * (1.0 + a.ln()).min(9.0)
    }
}

/// Products of an option's features with the situation's that matter together: how much a need
/// is relieved times how urgent it is, and how much of an item is gained times what it's worth.
pub fn derived(state: &[(String, f32)], opt: &[(String, f32)]) -> Vec<(String, f32)> {
    let get = |k: &str| state.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
    let mut out = Vec::new();
    for (k, v) in opt {
        if let Some(n) = k.strip_prefix("relief.")
            && let Some(need) = get(&format!("need.{n}"))
        {
            out.push((format!("{k}*urgency"), v * sim::brain::urgency(need)));
        } else if let Some(i) = k.strip_prefix("gain.")
            && let Some(value) = get(&format!("value.{i}"))
        {
            out.push((format!("{k}*value"), v * value * 10.0));
        }
    }
    out
}

impl Scorer {
    pub fn score(&self, act: &str, state: &[(String, f32)], opt: &[(String, f32)]) -> f32 {
        let mut s = 0.0;
        for (k, v) in opt.iter().chain(derived(state, opt).iter()) {
            if let Some(w) = self.option_w.get(k) {
                s += w * squash(*v);
            }
        }
        if let Some(ws) = self.state_w.get(act) {
            for (k, v) in state {
                if let Some(w) = ws.get(k) {
                    s += w * squash(*v);
                }
            }
        }
        s
    }

    pub fn load(path: &std::path::Path) -> Result<Scorer, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self).expect("scorer serialises"))
    }

    /// How often its top option is the teacher's choice.
    pub fn agreement(&self, data: &[Example]) -> f32 {
        if data.is_empty() {
            return 0.0;
        }
        let hits = data.iter().filter(|e| self.pick(e) == e.choice).count();
        hits as f32 / data.len() as f32
    }

    /// How often its top option is the same kind of action as the teacher's.
    pub fn kind_agreement(&self, data: &[Example]) -> f32 {
        if data.is_empty() {
            return 0.0;
        }
        let hits = data.iter().filter(|e| e.options[self.pick(e)].0 == e.options[e.choice].0).count();
        hits as f32 / data.len() as f32
    }

    fn pick(&self, e: &Example) -> usize {
        let scores: Vec<f32> = e.options.iter().map(|(a, f)| self.score(a, &e.state, f)).collect();
        (0..scores.len()).max_by(|&a, &b| scores[a].total_cmp(&scores[b]).then(b.cmp(&a))).unwrap_or(0)
    }
}

/// Train a scorer. Seeds with `seed % 5 == 0` are held out for the agreement figure.
pub fn train(name: &str, data: &[Example], epochs: usize) -> Scorer {
    let (train_set, held): (Vec<&Example>, Vec<&Example>) = data.iter().partition(|e| e.seed % 5 != 0);
    // Parameters: a flat vector, indexed by name.
    let mut index: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut key = |a: &str, k: &str| -> usize {
        let n = index.len();
        *index.entry((a.to_string(), k.to_string())).or_insert(n)
    };
    // Option features have act "" ; situation features are per act.
    // Each decision: its options' (parameter, value) lists, and the teacher's choice.
    type Row = (Vec<Vec<(usize, f32)>>, usize);
    let mut rows: Vec<Row> = Vec::new();
    for e in &train_set {
        let mut opts = Vec::new();
        for (act, f) in &e.options {
            let mut x: Vec<(usize, f32)> = f.iter().chain(derived(&e.state, f).iter()).map(|(k, v)| (key("", k), squash(*v))).collect();
            x.extend(e.state.iter().map(|(k, v)| (key(act, k), squash(*v))));
            opts.push(x);
        }
        rows.push((opts, e.choice));
    }
    let n = index.len();
    let mut w = vec![0.0f32; n];
    let (mut m, mut v) = (vec![0.0f32; n], vec![0.0f32; n]);
    let lr: f32 = 0.001;
    let (b1, b2, l2) = (0.9f32, 0.999f32, 1e-4f32);
    let mut t = 0i32;
    for _ in 0..epochs {
        for (opts, choice) in &rows {
            let scores: Vec<f32> = opts.iter().map(|x| x.iter().map(|(i, val)| w[*i] * val).sum()).collect();
            let mx = scores.iter().cloned().fold(f32::MIN, f32::max);
            let ex: Vec<f32> = scores.iter().map(|s| (s - mx).exp()).collect();
            let z: f32 = ex.iter().sum();
            let mut grad: BTreeMap<usize, f32> = BTreeMap::new();
            for (k, x) in opts.iter().enumerate() {
                let p = ex[k] / z - if k == *choice { 1.0 } else { 0.0 };
                for (i, val) in x {
                    *grad.entry(*i).or_default() += p * val;
                }
            }
            t += 1;
            for (i, g) in grad {
                let g = g + l2 * w[i];
                m[i] = b1 * m[i] + (1.0 - b1) * g;
                v[i] = b2 * v[i] + (1.0 - b2) * g * g;
                let mh = m[i] / (1.0 - b1.powi(t));
                let vh = v[i] / (1.0 - b2.powi(t));
                w[i] -= lr * mh / (vh.sqrt() + 1e-8);
            }
        }
    }
    let mut s = Scorer { name: name.to_string(), ..Default::default() };
    for ((act, k), i) in index {
        if w[i].abs() < 1e-5 {
            continue;
        }
        if act.is_empty() {
            s.option_w.insert(k, w[i]);
        } else {
            s.state_w.entry(act).or_default().insert(k, w[i]);
        }
    }
    let teachers: std::collections::BTreeSet<&str> = data.iter().map(|e| e.teacher.as_str()).collect();
    s.trained_on = format!("{} decisions ({} held out) from {}", train_set.len(), held.len(), teachers.into_iter().collect::<Vec<_>>().join(", "));
    let held: Vec<Example> = held.into_iter().cloned().collect();
    s.held_out_agreement = s.agreement(&held);
    s.held_out_kind_agreement = s.kind_agreement(&held);
    s
}

/// The scorer as a brain: answers at once.
pub struct ScorerBrain {
    pub scorer: Scorer,
    content: sim::content::Content,
    out: Vec<(DecisionRequest, Answer)>,
    /// Time spent scoring (for the per-decision cost).
    pub micros: u128,
    pub decisions: u64,
}

impl ScorerBrain {
    pub fn new(scorer: Scorer) -> Self {
        ScorerBrain { scorer, content: sim::content::Content::base(), out: Vec::new(), micros: 0, decisions: 0 }
    }
}

impl Brain for ScorerBrain {
    fn id(&self) -> &str {
        &self.scorer.name
    }

    fn submit(&mut self, req: DecisionRequest) {
        let t0 = std::time::Instant::now();
        let scores: Vec<f32> = req
            .options
            .iter()
            .map(|o| if o.possible { self.scorer.score(&o.kind.name(&self.content), &req.state, &o.features) } else { f32::NEG_INFINITY })
            .collect();
        let choice = (0..scores.len()).filter(|&i| req.options[i].possible).max_by(|&a, &b| scores[a].total_cmp(&scores[b]).then(b.cmp(&a))).unwrap_or(0);
        self.micros += t0.elapsed().as_micros();
        self.decisions += 1;
        self.out.push((req, Answer { scores, choice, reason: None }));
    }

    fn poll(&mut self) -> Vec<(DecisionRequest, Answer)> {
        std::mem::take(&mut self.out)
    }
}
