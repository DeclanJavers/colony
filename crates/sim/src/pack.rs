//! Expansions (packs), ARCHITECTURE.md section 10. A pack adds new ways for people to change the
//! base's quantities: actions they can choose (with the features brains see and the words they
//! say), how those actions play out, and how the rules brain weighs them. Packs are compiled in
//! (`PACKS`) and switched on per world when it's made (`Settings::packs`, saved with the world).
//!
//! The base's first pack is **hunting** (`packs/hunting.rs`): making spears and hunting with them.
//! The animals themselves (herds, numbers, how they graze, breed and flee) stay in the base:
//! wildlife is a quantity of the world, and hunting is one way to change it.
//!
//! **The hooks** are the `Pack` trait's methods. A pack reaches the world only through them and
//! through `api` (the helpers actions are built from). A hook a pack needs that doesn't exist is
//! added here on purpose and documented, not reached around.
//!
//! **Not hooks yet** (the hunting pack uses the base's for now): content of its own (its numbers
//! live in the base's actions.ron, and animals in animals.ron), event kinds (it uses the base's
//! SpearMade and Hunted events), per-person tables, and UI pages.

use crate::actions::{Action, Ctx};
use crate::brain::{Choice, DecisionRequest, Ending};
use crate::people::{Person, PersonId};
use serde::{Deserialize, Serialize};

/// What a pack action is aimed at (part of the choice, like the base's own kinds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    None,
    Animal(crate::animals::AnimalId),
}

pub trait Pack: Sync {
    /// Stable id (saved in worlds): "hunting".
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    /// A line for the new-world menu.
    fn about(&self) -> &'static str;
    /// The actions it adds, by id (the names brains and traces see: "hunt"). Their order is
    /// saved in worlds, so new ones go at the end.
    fn acts(&self) -> &'static [&'static str];
    /// Options for a grown person deciding what to do, added in order (with their features).
    fn options(&self, p: &Person, id: PersonId, ctx: &Ctx, walk_minutes: &dyn Fn(f32) -> f32, opts: &mut Vec<Choice>);
    /// One tick of one of its actions; Some when it's over.
    fn step(&self, act: u8, target: Target, p: &mut Person, a: &mut Action, ctx: &mut Ctx) -> Option<Ending>;
    /// The rules brain's weight for one of its actions, on top of what the base adds for every
    /// option (needs relieved, things gained, time taken, being asked).
    fn rules_score(&self, act: u8, o: &Choice, req: &DecisionRequest) -> f32;
    /// What someone thinks when they choose it.
    fn thought(&self, act: u8, req: &DecisionRequest) -> String;
    /// Can children choose it?
    fn for_children(&self, _act: u8) -> bool {
        false
    }
    /// Does someone doing it (at this phase) put animals nearby on edge?
    fn scares_animals(&self, _act: u8, _phase: u8) -> bool {
        false
    }
}

/// Every pack compiled in. Their order is saved in worlds (as the pack in a pack action), so
/// new packs go at the end.
pub static PACKS: &[&dyn Pack] = &[&crate::packs::hunting::Hunting];

pub fn get(pack: u8) -> &'static dyn Pack {
    PACKS[pack as usize]
}

/// The packs on in a world, by index, from their ids.
pub fn enabled(ids: &[String]) -> Vec<u8> {
    PACKS.iter().enumerate().filter(|(_, p)| ids.iter().any(|i| i == p.id())).map(|(k, _)| k as u8).collect()
}

/// The ids of the packs a new world has unless told otherwise (all of the base's).
pub fn default_ids() -> Vec<String> {
    PACKS.iter().map(|p| p.id().to_string()).collect()
}

/// A pack action's kind: which pack, which of its acts, at what.
pub fn kind(pack: &dyn Pack, act: &str, target: Target) -> crate::actions::Kind {
    let p = PACKS.iter().position(|q| q.id() == pack.id()).expect("a compiled-in pack") as u8;
    let a = pack.acts().iter().position(|x| *x == act).unwrap_or_else(|| panic!("pack {} has no act {act}", pack.id())) as u8;
    crate::actions::Kind::Pack { pack: p, act: a, target }
}

/// What packs build actions from: the same helpers the base's own actions use.
pub(crate) mod api {
    pub(crate) use crate::actions::{Action, Anim, Ctx, Walk, choice, direction, dist, walk};
    pub(crate) use crate::animals::AnimalId;
    pub(crate) use crate::brain::{Choice, DecisionRequest, Ending};
    pub(crate) use crate::events::Event;
    pub(crate) use crate::people::{Person, PersonId};
    pub(crate) use crate::settle::{face, minutes_in, next_phase, practise, take};
    pub(crate) use crate::social::Effect;
}
