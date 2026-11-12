//! The text model (ARCHITECTURE.md section 9), run natively through ONNX Runtime.
//!
//! It reads a decision as words, the way Claude Haiku does: the situation (everything before
//! "What do you do next?"), then the numbered options. A small encoder (bge-small, 33M
//! parameters) turns the situation and each option line into vectors, and a small head scores
//! each (situation, option) pair. Trained on Haiku's choices on Colab
//! (`brain/text_model/train_colab.py`); the files live in `brain/models/<name>/`:
//! `encoder.int8.onnx`, `head.onnx` and `tokenizer.json`.
//!
//! Option lines repeat a lot ("Forage at the berry bush to the north-east (about 192 min)"), so
//! their vectors are cached: a decision usually costs one encoder pass on the situation.

use sim::brain::{Answer, Brain, DecisionRequest};
use std::collections::HashMap;
use std::path::Path;

/// How many tokens of the situation and of an option line the model reads (as in training).
const SITUATION_TOKENS: usize = 256;
const OPTION_TOKENS: usize = 48;
/// Option vectors kept (each is a few hundred floats).
const CACHE: usize = 20_000;

pub struct TextModel {
    encoder: ort::session::Session,
    head: ort::session::Session,
    tok: tokenizers::Tokenizer,
    cache: HashMap<String, Vec<f32>>,
}

/// The situation and the option lines, as the model was trained on them.
pub fn split(text: &str) -> (String, Vec<String>) {
    let (head, tail) = text.split_once("What do you do next?").unwrap_or((text, ""));
    let options = tail
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let (n, rest) = l.split_once(". ")?;
            n.chars().all(|c| c.is_ascii_digit()).then(|| rest.trim().to_string())
        })
        .collect();
    (head.trim().to_string(), options)
}

/// CPU threads for the model: COLONY_TEXT_THREADS, default 4 (an M3 Pro has 6 performance cores;
/// the game's own threads need some).
fn threads() -> usize {
    std::env::var("COLONY_TEXT_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(4)
}

/// ONNX Runtime allows one environment per process: made once, shared by every model loaded.
fn environment() -> Result<&'static ort::environment::Environment, String> {
    static ENV: std::sync::OnceLock<Result<ort::environment::Environment, String>> = std::sync::OnceLock::new();
    ENV.get_or_init(|| ort::init().with_name("colony").build().map_err(err)).as_ref().map_err(|e| e.clone())
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl TextModel {
    pub fn load(dir: &Path) -> Result<TextModel, String> {
        let env = environment()?;
        let session = |file: &str| -> Result<ort::session::Session, String> {
            let path = dir.join(file);
            ort::session::Session::builder(env)
                .map_err(err)?
                .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
                .map_err(err)?
                .with_intra_threads(threads())
                .map_err(err)?
                .commit_from_file(&path)
                .map_err(|e| format!("{}: {e}", path.display()))
        };
        let encoder = session("encoder.int8.onnx")?;
        let head = session("head.onnx")?;
        let mut tok = tokenizers::Tokenizer::from_file(dir.join("tokenizer.json")).map_err(err)?;
        // The file keeps whatever padding and truncation training last used (truncation at the
        // options' 48 tokens): lengths are set here instead, per kind of text.
        tok.with_padding(None);
        tok.with_truncation(None).map_err(err)?;
        Ok(TextModel { encoder, head, tok, cache: HashMap::new() })
    }

    /// Vectors for some texts (one encoder pass, padded to the longest).
    fn encode(&mut self, texts: &[String], max_tokens: usize) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut rows: Vec<Vec<u32>> = Vec::with_capacity(texts.len());
        for t in texts {
            let e = self.tok.encode(t.as_str(), true).map_err(err)?;
            let mut ids = e.get_ids().to_vec();
            if ids.len() > max_tokens {
                // Keep the end marker, as truncation in training did.
                let last = *ids.last().unwrap_or(&0);
                ids.truncate(max_tokens);
                ids[max_tokens - 1] = last;
            }
            rows.push(ids);
        }
        let width = rows.iter().map(|r| r.len()).max().unwrap_or(1);
        let mut ids = Vec::with_capacity(rows.len() * width);
        let mut mask = Vec::with_capacity(rows.len() * width);
        for r in &rows {
            for k in 0..width {
                ids.push(*r.get(k).unwrap_or(&0) as i64);
                mask.push((k < r.len()) as i64);
            }
        }
        let shape = [rows.len() as i64, width as i64];
        let ids = ort::value::Tensor::from_array((shape, ids)).map_err(err)?;
        let mask = ort::value::Tensor::from_array((shape, mask)).map_err(err)?;
        let out = self.encoder.run(ort::inputs!["input_ids" => ids, "attention_mask" => mask]).map_err(err)?;
        let (shape, data) = out["vector"].try_extract_tensor::<f32>().map_err(err)?;
        let d = *shape.last().unwrap_or(&1) as usize;
        Ok(data.chunks(d).map(|c| c.to_vec()).collect())
    }

    /// A score for each option line.
    pub fn score(&mut self, situation: &str, options: &[String]) -> Result<Vec<f32>, String> {
        let s = self.encode(&[situation.to_string()], SITUATION_TOKENS)?.remove(0);
        let missing: Vec<String> = options.iter().filter(|o| !self.cache.contains_key(*o)).cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        if !missing.is_empty() {
            if self.cache.len() + missing.len() > CACHE {
                self.cache.clear();
            }
            let vs = self.encode(&missing, OPTION_TOKENS)?;
            for (o, v) in missing.into_iter().zip(vs) {
                self.cache.insert(o, v);
            }
        }
        let d = s.len();
        let mut o = Vec::with_capacity(options.len() * d);
        for line in options {
            o.extend_from_slice(&self.cache[line]);
        }
        let situation = ort::value::Tensor::from_array(([1i64, d as i64], s)).map_err(err)?;
        let opts = ort::value::Tensor::from_array(([options.len() as i64, d as i64], o)).map_err(err)?;
        let out = self.head.run(ort::inputs!["situation" => situation, "options" => opts]).map_err(err)?;
        let (_, scores) = out["scores"].try_extract_tensor::<f32>().map_err(err)?;
        Ok(scores.to_vec())
    }
}

/// The text model as a brain: answers at once (a few milliseconds a decision).
pub struct TextBrain {
    pub name: String,
    model: TextModel,
    out: Vec<(DecisionRequest, Answer)>,
    /// Time spent deciding, and how many decisions (for the per-decision cost).
    pub micros: u128,
    pub decisions: u64,
    /// Decisions it couldn't score (left to the rules brain's fallback).
    pub failed: u64,
}

impl TextBrain {
    pub fn load(name: &str, dir: &Path) -> Result<TextBrain, String> {
        Ok(TextBrain { name: name.to_string(), model: TextModel::load(dir)?, out: Vec::new(), micros: 0, decisions: 0, failed: 0 })
    }
}

impl Brain for TextBrain {
    fn id(&self) -> &str {
        &self.name
    }

    fn submit(&mut self, req: DecisionRequest) {
        let t0 = std::time::Instant::now();
        let possible: Vec<usize> = req.options.iter().enumerate().filter(|(_, o)| o.possible).map(|(i, _)| i).collect();
        let (situation, lines) = split(&req.text);
        let mut scores = vec![f32::NEG_INFINITY; req.options.len()];
        // The text lists the possible options in order; if it doesn't match, don't guess.
        let choice = match (lines.len() == possible.len()).then(|| self.model.score(&situation, &lines)) {
            Some(Ok(s)) => {
                for (k, &i) in possible.iter().enumerate() {
                    scores[i] = s[k];
                }
                possible.iter().copied().max_by(|&a, &b| scores[a].total_cmp(&scores[b]).then(b.cmp(&a)))
            }
            _ => None,
        };
        self.micros += t0.elapsed().as_micros();
        self.decisions += 1;
        // An impossible choice makes the sim fall back to the rules brain (and say so in the trace).
        let choice = choice.unwrap_or_else(|| {
            self.failed += 1;
            usize::MAX
        });
        self.out.push((req, Answer { scores, choice, reason: None }));
    }

    fn poll(&mut self) -> Vec<(DecisionRequest, Answer)> {
        std::mem::take(&mut self.out)
    }
}
