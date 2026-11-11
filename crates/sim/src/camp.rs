//! The camp: where the group keeps its stores and its fire. (Buildings come with M6.)

use crate::world::Map;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camp {
    /// Tile of the stores pile.
    pub stores_at: (u16, u16),
    /// Tile of the fire pit.
    pub fire_at: (u16, u16),
    /// Count per item.
    pub stores: Vec<u32>,
    pub fire_lit: bool,
    /// Hours of fuel left in the fire.
    pub fire_hours: f32,
}

impl Camp {
    pub fn new(start: (u32, u32), items: usize) -> Camp {
        Camp {
            stores_at: (start.0 as u16, start.1 as u16),
            fire_at: (start.0 as u16 + 3, start.1 as u16 + 2),
            stores: vec![0; items],
            fire_lit: false,
            fire_hours: 0.0,
        }
    }

    pub fn fire_pos(&self) -> (f32, f32) {
        (self.fire_at.0 as f32 + 0.5, self.fire_at.1 as f32 + 0.5)
    }

    pub fn stores_pos(&self) -> (f32, f32) {
        (self.stores_at.0 as f32 + 0.5, self.stores_at.1 as f32 + 0.5)
    }

    /// A place to sit or sleep around the fire, one per person (by slot): on dry land, clear of
    /// the stores pile.
    pub fn spot(&self, slot: usize, sleeping: bool, map: &Map) -> (u16, u16) {
        let (fx, fy) = self.fire_pos();
        let (sx, sy) = self.stores_pos();
        let start = slot as f32 * 2.399 + if sleeping { 0.4 } else { 0.0 };
        for ring in 0..4 {
            let r = if sleeping { 3.0 } else { 2.2 } + ring as f32 * 0.8;
            for k in 0..16 {
                let a = start + k as f32 * 0.39;
                let (x, y) = (fx + a.cos() * r, fy + a.sin() * r);
                if x < 0.0 || y < 0.0 || x >= map.size as f32 || y >= map.size as f32 {
                    continue;
                }
                let i = map.idx(x as u32, y as u32);
                let clear = (x - sx).powi(2) + (y - sy).powi(2) >= 4.0;
                if clear && !map.water[i].is_water() && map.node_at[i] == 0 {
                    return (x as u16, y as u16);
                }
            }
        }
        self.fire_at
    }
}
