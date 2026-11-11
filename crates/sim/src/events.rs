//! Everything that happens is an event. Systems emit them; the chronicle, story feed, memories,
//! stats and traces listen (ARCHITECTURE.md section 1, rule 5).

use crate::people::PersonId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    DayStarted { day: u64 },
    SeasonStarted { season: String, year: u64 },
    NodePlaced { kind: u16, x: u32, y: u32 },
    NodeRemoved { kind: u16, x: u32, y: u32 },
    PersonDied { person: PersonId, name: String, cause: String },
    FireLit { person: PersonId, name: String },
    FireOut,
    /// Someone got better at something.
    SkillUp { person: PersonId, name: String, skill: String, level: u32 },
    /// A body need became desperate (crossed 0.85).
    NeedCritical { person: PersonId, name: String, need: String, word: String },
}
