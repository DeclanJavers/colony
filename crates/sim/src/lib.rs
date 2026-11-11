//! Colony's simulation: pure data on a fixed tick, deterministic from a seed, with no graphics.
//! See docs/ARCHITECTURE.md (sections 2 and 3).

pub mod actions;
pub mod animals;
pub mod brain;
pub mod buildings;
pub mod camp;
pub mod chat;
pub mod chronicle;
pub mod commands;
pub mod content;
pub mod events;
pub mod family;
pub mod hash;
pub mod ids;
pub mod leisure;
pub mod life;
pub mod path;
pub mod people;
pub mod rng;
pub mod save;
pub mod settle;
pub mod sim;
pub mod social;
pub mod speak;
pub mod time;
pub mod weather;
pub mod world;

pub use commands::Command;
pub use content::Content;
pub use events::Event;
pub use sim::Sim;
pub use people::{Person, PersonId};
pub use world::{Map, Node, Water};
