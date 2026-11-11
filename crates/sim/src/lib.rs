//! Colony's simulation: pure data on a fixed tick, deterministic from a seed, with no graphics.
//! See docs/ARCHITECTURE.md (sections 2 and 3).

pub mod actions;
pub mod brain;
pub mod camp;
pub mod chronicle;
pub mod commands;
pub mod content;
pub mod events;
pub mod family;
pub mod hash;
pub mod ids;
pub mod life;
pub mod path;
pub mod people;
pub mod rng;
pub mod save;
pub mod sim;
pub mod social;
pub mod time;
pub mod world;

pub use commands::Command;
pub use content::Content;
pub use events::Event;
pub use sim::Sim;
pub use people::{Person, PersonId};
pub use world::{Map, Node, Water};
