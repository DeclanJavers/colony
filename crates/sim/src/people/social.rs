//! Families and society (M5): family links, relationships, memories and invitations. The numbers
//! are in data/base/life.ron; how they change is in crates/sim/src/family.rs and actions.rs.

use super::PersonId;
use serde::{Deserialize, Serialize};

/// Where someone is in life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Stage {
    /// Makes no decisions; stays with their mother or carer.
    Baby,
    /// Chooses from a short list.
    Child,
    Adult,
    /// Slowly weakens; old age can kill.
    Elder,
}

impl Stage {
    pub fn word(self) -> &'static str {
        match self {
            Stage::Baby => "baby",
            Stage::Child => "child",
            Stage::Adult => "adult",
            Stage::Elder => "elder",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Family {
    pub mother: Option<PersonId>,
    pub father: Option<PersonId>,
    pub partner: Option<PersonId>,
    pub children: Vec<PersonId>,
    /// 0 for founders, then one more than the older parent's.
    pub generation: u32,
}

/// How one person feels about another (kept by each side: it needn't be mutual).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rel {
    /// How well they know them (0..1): grows with time spent together.
    pub familiarity: f32,
    /// Liking (-1..1): talk, help, arguments and gossip move it.
    pub affection: f32,
    /// Regard for their skill and help (0..1).
    pub respect: f32,
    /// Romantic interest (0..1): courting raises it.
    pub romance: f32,
    /// When they last spent time together.
    pub last: u64,
}

impl Rel {
    /// A word for the relationship, from the strongest feeling.
    pub fn word(&self, partner: bool) -> &'static str {
        if partner {
            return "partner";
        }
        if self.affection < -0.3 {
            return "rival";
        }
        if self.romance > 0.4 {
            return "sweetheart";
        }
        match (self.familiarity, self.affection) {
            (f, a) if f > 0.5 && a > 0.4 => "close friend",
            (f, a) if f > 0.25 && a > 0.15 => "friend",
            (f, _) if f > 0.15 => "acquaintance",
            _ => "stranger",
        }
    }
}

/// Something that happened to someone, as they remember it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub tick: u64,
    /// What happened, in their words ("Became partners with Kira").
    pub text: String,
    /// How it counts towards mood while it's fresh ("Happy with Kira", "Grieving Bram").
    pub mood: String,
    pub about: Option<PersonId>,
    /// -1 (awful) .. 1 (wonderful)
    pub feeling: f32,
    /// 0 .. 1: important memories fade slowly.
    pub importance: f32,
    /// 1 when new; fades each day.
    pub strength: f32,
}

/// What someone has been asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ask {
    /// "Will you be my partner?"
    Partner,
    /// "Shall we try for a child?"
    Child,
    /// "I can teach you this skill."
    Teach { skill: u8 },
    /// "Could you spare some food?"
    Food,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Invite {
    pub from: PersonId,
    pub ask: Ask,
    pub tick: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pregnancy {
    pub father: PersonId,
    pub conceived: u64,
    pub due: u64,
}
