//! Speaking to someone (M8, the god tool "speak"). Your words are read into a request: on the
//! Low AI setting by keyword (data/base/speak.ron). What they heard is kept on the person for a
//! few hours: the options it asks for carry an "asked" feature (the rules brain likes them more),
//! the text a text brain sees says what they heard, and the decision's trace shows both.

use crate::content::Content;
use serde::{Deserialize, Serialize};

/// What someone heard, and how it was read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Heard {
    pub words: String,
    /// The actions asked for (names, as in traces), and how they're shown ("fetch wood").
    pub acts: Vec<String>,
    pub read_as: Vec<String>,
    pub at: u64,
    pub until: u64,
}

/// Read words by keyword: every topic with a keyword that starts one of the words said. A word
/// that is exactly some topic's keyword belongs to that topic only ("firewood" is wood, not fire).
pub fn read(words: &str, c: &Content) -> (Vec<String>, Vec<String>) {
    let lower = words.to_lowercase();
    let said: Vec<&str> = lower.split(|ch: char| !ch.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let exact = |w: &str| c.speak.topics.iter().any(|t| t.words.iter().any(|k| k == w));
    let mut acts: Vec<String> = Vec::new();
    let mut read_as: Vec<String> = Vec::new();
    for t in &c.speak.topics {
        let hit = t.words.iter().any(|k| {
            if k.contains(' ') {
                lower.contains(k.as_str())
            } else {
                said.iter().any(|w| *w == k.as_str() || (w.starts_with(k.as_str()) && !exact(w)))
            }
        });
        if hit {
            for a in &t.acts {
                if !acts.contains(a) {
                    acts.push(a.clone());
                }
            }
            if !read_as.contains(&t.read) {
                read_as.push(t.read.clone());
            }
        }
    }
    (acts, read_as)
}

/// What they heard, for someone, as a request (or None if nothing was understood).
pub fn hear(words: &str, c: &Content, tick: u64) -> Heard {
    let (acts, read_as) = read(words, c);
    let ticks = (c.speak.hours * 60.0 / c.calendar.minutes_per_tick as f32) as u64;
    Heard { words: words.trim().to_string(), acts, read_as, at: tick, until: tick + ticks }
}
