//! The subagent bridge (ARCHITECTURE.md section 8): decisions out as files, answers back as files.
//!
//! - **Batches.** Decisions are collected for a few game minutes (or until there are enough),
//!   then written together as `req-<n>.json`. Meanwhile the people waiting just stand and think.
//! - **Answers.** A driver (Claude Code with Haiku subagents, or `tools/haiku_driver`) writes
//!   `res-<n>.json`: a choice number and a one-line reason for each decision.
//! - **Checking.** An answer must name one of the numbered options. Anything else (missing,
//!   out of range, unreadable) falls back to the rules brain, and the trace says so.
//! - **Stopping.** If the driver hits the plan's usage limit it writes `stop.json`; the brain
//!   reports it and the runner saves and stops cleanly, to resume later from the save.
//!
//! The files are plain JSON so a person (or a subagent) can read and answer them by hand.

use serde::{Deserialize, Serialize};
use sim::brain::{Answer, Brain, DecisionRequest};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a driver is told, at the top of every batch.
pub const INSTRUCTIONS: &str = "You decide what people in a colony simulation do next. Each item is one person: their situation, then numbered options. \
Pick the option that person would most sensibly choose: look after urgent needs first (thirst, hunger, cold, exhaustion), then the group's needs (food and firewood for the stores), then what suits their personality. \
Reply with JSON only, exactly: {\"answers\": [{\"id\": <item id>, \"choice\": <option number>, \"reason\": \"<one short sentence, in their voice>\"}]}, one answer per item.";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchItem {
    pub id: u64,
    pub person: String,
    /// The situation and the numbered options, exactly as a text brain sees them.
    pub text: String,
    /// How many options there are (answers must be 1..=options).
    pub options: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Batch {
    pub batch: u64,
    pub tick: u64,
    pub instructions: String,
    pub items: Vec<BatchItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemAnswer {
    pub id: u64,
    pub choice: i64,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchAnswer {
    pub answers: Vec<ItemAnswer>,
}

/// Why a driver stopped (written as `stop.json`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stop {
    pub reason: String,
}

pub struct SubagentBrain {
    dir: PathBuf,
    /// Collected, not yet written.
    pending: Vec<DecisionRequest>,
    /// Polls since the first pending decision arrived.
    waited: u32,
    /// Write a batch after this many polls (ticks) of collecting...
    pub window: u32,
    /// ...or once this many decisions are collected.
    pub max_items: usize,
    /// Written, waiting for an answer: batch number -> its decisions (item ids in order).
    in_flight: BTreeMap<u64, Vec<(u64, DecisionRequest)>>,
    next_batch: u64,
    next_item: u64,
    stopped: Option<String>,
    /// Answers that came back unusable (counted for the run's report).
    pub bad_answers: u64,
    pub answered: u64,
}

impl SubagentBrain {
    /// A bridge in `dir` (created if needed). Batches already answered there are picked up.
    pub fn new(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        // Carry on numbering after any batches already in the folder (a resumed run).
        let mut next_batch = 0;
        for e in std::fs::read_dir(dir)?.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(n) = name.strip_prefix("req-").and_then(|s| s.strip_suffix(".json")).and_then(|s| s.parse::<u64>().ok()) {
                next_batch = next_batch.max(n + 1);
            }
        }
        Ok(SubagentBrain {
            dir: dir.to_path_buf(),
            pending: Vec::new(),
            waited: 0,
            window: 15,
            max_items: 24,
            in_flight: BTreeMap::new(),
            next_batch,
            next_item: next_batch * 1000,
            stopped: None,
            bad_answers: 0,
            answered: 0,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn write_batch(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let n = self.next_batch;
        self.next_batch += 1;
        let mut items = Vec::new();
        let mut held = Vec::new();
        for req in std::mem::take(&mut self.pending) {
            let id = self.next_item;
            self.next_item += 1;
            let options = req.options.iter().filter(|o| o.possible).count();
            let person = req.text.split_once(',').map_or(String::new(), |(a, _)| a.trim_start_matches("You are ").to_string());
            items.push(BatchItem { id, person, text: req.text.clone(), options });
            held.push((id, req));
        }
        let tick = held.first().map_or(0, |(_, r)| r.tick);
        let batch = Batch { batch: n, tick, instructions: INSTRUCTIONS.into(), items };
        let path = self.dir.join(format!("req-{n}.json"));
        let tmp = self.dir.join(format!("req-{n}.json.tmp"));
        // Written whole, then renamed, so a driver never reads half a batch.
        if std::fs::write(&tmp, serde_json::to_vec_pretty(&batch).expect("batch serialises")).and_then(|_| std::fs::rename(&tmp, &path)).is_err() {
            self.stopped = Some(format!("couldn't write {}", path.display()));
        }
        self.in_flight.insert(n, held);
        self.waited = 0;
    }

    /// Read a finished batch's answers, checking each one.
    fn read_answers(&mut self, n: u64) -> Option<Vec<(DecisionRequest, Answer)>> {
        let path = self.dir.join(format!("res-{n}.json"));
        let bytes = std::fs::read(&path).ok()?;
        let parsed: Option<BatchAnswer> = parse_answer(&bytes);
        let held = self.in_flight.remove(&n)?;
        let by_id: BTreeMap<u64, ItemAnswer> = parsed.map(|p| p.answers.into_iter().map(|a| (a.id, a)).collect()).unwrap_or_default();
        let mut out = Vec::new();
        for (id, req) in held {
            // Option numbers count the possible options only (as the text lists them).
            let possible: Vec<usize> = req.options.iter().enumerate().filter(|(_, o)| o.possible).map(|(i, _)| i).collect();
            let picked = by_id.get(&id).and_then(|a| usize::try_from(a.choice).ok().filter(|k| *k >= 1 && *k <= possible.len()).map(|k| (possible[k - 1], a.reason.clone())));
            let answer = match picked {
                Some((choice, reason)) => {
                    self.answered += 1;
                    let scores = req.options.iter().enumerate().map(|(i, o)| if !o.possible { f32::NEG_INFINITY } else if i == choice { 1.0 } else { 0.0 }).collect();
                    Answer { scores, choice, reason: reason.map(|r| r.trim().to_string()).filter(|r| !r.is_empty()) }
                }
                None => {
                    // Not a usable answer: an impossible choice makes the sim fall back to rules.
                    self.bad_answers += 1;
                    Answer { scores: vec![0.0; req.options.len()], choice: usize::MAX, reason: Some("(the answer couldn't be used)".into()) }
                }
            };
            out.push((req, answer));
        }
        Some(out)
    }
}

/// Answers as JSON, tolerating text around it (a model may add a sentence or a code fence).
pub fn parse_answer(bytes: &[u8]) -> Option<BatchAnswer> {
    if let Ok(a) = serde_json::from_slice::<BatchAnswer>(bytes) {
        return Some(a);
    }
    let s = String::from_utf8_lossy(bytes);
    let (start, end) = (s.find('{')?, s.rfind('}')?);
    serde_json::from_str::<BatchAnswer>(&s[start..=end]).ok()
}

impl Brain for SubagentBrain {
    fn id(&self) -> &str {
        "subagent"
    }

    fn submit(&mut self, req: DecisionRequest) {
        self.pending.push(req);
    }

    fn poll(&mut self) -> Vec<(DecisionRequest, Answer)> {
        if self.stopped.is_none()
            && let Ok(bytes) = std::fs::read(self.dir.join("stop.json"))
        {
            let why = serde_json::from_slice::<Stop>(&bytes).map(|s| s.reason).unwrap_or_else(|_| "the driver stopped".into());
            self.stopped = Some(why);
        }
        if !self.pending.is_empty() {
            self.waited += 1;
            if self.pending.len() >= self.max_items || self.waited >= self.window {
                self.write_batch();
            }
        }
        let ready: Vec<u64> = self.in_flight.keys().copied().filter(|n| self.dir.join(format!("res-{n}.json")).exists()).collect();
        let mut out = Vec::new();
        for n in ready {
            if let Some(a) = self.read_answers(n) {
                out.extend(a);
            }
        }
        out
    }

    fn in_flight(&self) -> usize {
        self.in_flight.values().map(|v| v.len()).sum()
    }

    fn queued(&self) -> usize {
        self.pending.len()
    }

    fn flush(&mut self) {
        self.write_batch();
    }

    fn stopped(&self) -> Option<String> {
        self.stopped.clone()
    }
}
