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
    // Families and society (M5)
    Born { child: PersonId, name: String, mother: PersonId, father: PersonId, parents: String },
    Expecting { mother: PersonId, name: String, father: String },
    Partnered { a: PersonId, b: PersonId, names: String },
    GrewUp { person: PersonId, name: String },
    Argued { a: PersonId, b: PersonId, names: String },
    /// Someone learned a taught skill (reached level 1) from a teacher.
    Learned { learner: PersonId, name: String, skill: String, teacher: String },
    /// Nobody living knows this skill any more.
    SkillLost { skill: String, last: String },
    Buried { person: PersonId, name: String, by: String },
    /// Someone shared food with someone who asked.
    Shared { from: PersonId, to: PersonId, names: String, what: String },
}
