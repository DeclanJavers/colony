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
    /// Who last lit it, and when (tick).
    pub lit_by: Option<(crate::people::PersonId, u64)>,
}

impl Camp {
    pub fn new(start: (u32, u32), items: usize) -> Camp {
        Camp {
            stores_at: (start.0 as u16, start.1 as u16),
            fire_at: (start.0 as u16 + 3, start.1 as u16 + 2),
            stores: vec![0; items],
            fire_lit: false,
            fire_hours: 0.0,
            lit_by: None,
        }
    }

    pub fn fire_pos(&self) -> (f32, f32) {
        (self.fire_at.0 as f32 + 0.5, self.fire_at.1 as f32 + 0.5)
    }

    pub fn stores_pos(&self) -> (f32, f32) {
        (self.stores_at.0 as f32 + 0.5, self.stores_at.1 as f32 + 0.5)
    }

    /// A place by the fire, one per person (by slot), each their own: where they sit and where
    /// they sleep. Rings around the fire (0.7 m apart, seats about 0.75 m apart round each ring)
    /// on dry land, clear of the stores pile and of anything standing. Four rings stay inside the
    /// fire's warmth (about 84 places within 3.6 m, fewer by water); after that, further out.
    /// The sim hands out slots youngest first, so children get the warmest places. Returns an exact position
    /// (people walk to its tile, then settle onto it).
    pub fn spot(&self, slot: usize, sleeping: bool, map: &Map) -> (f32, f32) {
        let _ = sleeping;
        let (fx, fy) = self.fire_pos();
        let (sx, sy) = self.stores_pos();
        let mut free = 0;
        for ring in 0..8 {
            let r = 1.5 + ring as f32 * 0.7;
            let n = ((std::f32::consts::TAU * r / 0.75).floor() as usize).max(6);
            // Each ring starts half a seat round, so sleepers' heads fall between the feet of
            // the ring inside.
            let start = ring as f32 * std::f32::consts::PI / n as f32;
            for k in 0..n {
                let a = start + k as f32 / n as f32 * std::f32::consts::TAU;
                let (x, y) = (fx + a.cos() * r, fy + a.sin() * r);
                if x < 0.0 || y < 0.0 || x >= map.size as f32 || y >= map.size as f32 {
                    continue;
                }
                let i = map.idx(x as u32, y as u32);
                let clear = (x - sx).powi(2) + (y - sy).powi(2) >= 4.0;
                if !clear || map.water[i].is_water() || map.node_at[i] != 0 {
                    continue;
                }
                if free == slot {
                    return (x, y);
                }
                free += 1;
            }
        }
        self.fire_pos()
    }
}
