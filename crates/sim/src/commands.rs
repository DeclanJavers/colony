//! Commands come from outside the sim (the player's god tools, settings, tests). They're queued,
//! applied at the start of the next tick and recorded in the command log, so a seed plus the log
//! replays a world exactly.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// Put a resource node on an empty land tile (a gift, or a test).
    PlaceNode { kind: u16, x: u32, y: u32 },
    /// Take away the node on a tile, if any.
    RemoveNode { x: u32, y: u32 },
    /// The "beauty matters" world setting (M7).
    SetBeautyMatters { on: bool },
    /// Speak to someone (M8, god tool): your words, read into a request.
    Speak { person: crate::people::PersonId, words: String },
}
