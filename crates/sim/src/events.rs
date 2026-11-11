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
    // Settling down (M6)
    BuildingStarted { site: crate::buildings::StructureId, person: PersonId, name: String, kind: String },
    BuildingDone { site: crate::buildings::StructureId, kind: String, owner: Option<PersonId> },
    FieldSown { site: crate::buildings::StructureId, person: PersonId, name: String },
    Harvested { site: crate::buildings::StructureId, person: PersonId, name: String, amount: u32 },
    /// Frost killed a crop before it ripened.
    CropFailed { site: crate::buildings::StructureId },
    SpearMade { person: PersonId, name: String },
    Hunted { person: PersonId, name: String, animal: String, kind: u16 },
    /// The day's weather turned (rain, snow, a cold snap).
    Weather { sky: String, snap: bool },
    // Free time (M7)
    /// A free-time activity became someone's hobby.
    HobbyFound { person: PersonId, name: String, hobby: String },
    /// Someone made something in their free time ("planted a flower bed", "carved a statue").
    Made { person: PersonId, name: String, what: String },
    /// A world setting changed.
    SettingChanged { what: String },
    /// You spoke to someone (M8), and how your words were read.
    Spoken { person: PersonId, name: String, words: String, read_as: String },
    /// Someone chose to do what you asked (`what`: the option they chose; `asked`: how your
    /// words were read).
    DidAsked { person: PersonId, name: String, what: String, asked: String },
    /// A gift arrived (M10): what it was, in words ("30 berries", "a house"), and whether it went
    /// into the stores.
    Gifted { what: String, stores: bool },
    /// A gift that couldn't be given, and why (no room for a building).
    GiftRefused { what: String, why: String },
}
