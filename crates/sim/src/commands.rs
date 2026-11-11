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
    /// A gift (M10, god tool).
    Gift(Gift),
}

/// What a gift is. Ids are content ids ("berries", "house"), so the log reads plainly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Gift {
    /// Food or materials, put straight into the camp's stores.
    Items { item: String, amount: u32 },
    /// A finished building, on the next free spot near the camp. A home goes to whoever has
    /// none at the next morning's rehousing; nobody owns a storehouse.
    Building { kind: String },
}
