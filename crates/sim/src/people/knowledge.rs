//! What a person knows about the world: places they've seen or been told about, each with its
//! source, when, and how much they trust it. Options offered to a brain come from here, never
//! from the true map (ARCHITECTURE.md section 16).

use crate::ids::Id;
use crate::world::Node;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::PersonId;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    Seen,
    Told(PersonId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KnownNode {
    pub kind: u16,
    pub x: u16,
    pub y: u16,
    /// How much they think is there.
    pub amount: f32,
    pub when: u64,
    pub source: Source,
    pub trust: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KnownWater {
    pub x: u16,
    pub y: u16,
    pub when: u64,
    pub source: Source,
}

/// What they think is in the camp's stores, and when they last looked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoresBelief {
    pub items: Vec<u32>,
    pub fire_lit: bool,
    pub fire_hours: f32,
    pub when: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Knowledge {
    pub nodes: BTreeMap<Id<Node>, KnownNode>,
    pub water: Vec<KnownWater>,
    pub stores: StoresBelief,
    /// Coarse map of where they've been: one bit per 16x16-tile cell.
    pub explored: Vec<bool>,
}

/// Cell size of the explored map.
pub const CELL: u32 = 16;
/// Fresh water spots kept in mind (the nearest few, spread out).
const MAX_WATER: usize = 12;

impl Knowledge {
    pub fn new(map_size: u32, items: usize) -> Self {
        let c = map_size.div_ceil(CELL) as usize;
        Knowledge {
            nodes: BTreeMap::new(),
            water: Vec::new(),
            stores: StoresBelief { items: vec![0; items], fire_lit: false, fire_hours: 0.0, when: 0 },
            explored: vec![false; c * c],
        }
    }

    pub fn see_node(&mut self, id: Id<Node>, n: &Node, now: u64) {
        self.nodes.insert(id, KnownNode { kind: n.kind, x: n.x, y: n.y, amount: n.amount, when: now, source: Source::Seen, trust: 1.0 });
    }

    pub fn forget_node(&mut self, id: Id<Node>) {
        self.nodes.remove(&id);
    }

    /// Remember a fresh water spot unless one is already known close by. When full, the
    /// farthest from `home` is dropped.
    pub fn see_water(&mut self, x: u16, y: u16, now: u64, home: (u32, u32)) {
        if let Some(w) = self.water.iter_mut().find(|w| (w.x as i32 - x as i32).abs() + (w.y as i32 - y as i32).abs() < 10) {
            w.when = now;
            return;
        }
        self.water.push(KnownWater { x, y, when: now, source: Source::Seen });
        if self.water.len() > MAX_WATER {
            let d = |w: &KnownWater| (w.x as i32 - home.0 as i32).pow(2) + (w.y as i32 - home.1 as i32).pow(2);
            let far = (0..self.water.len()).max_by_key(|&i| (d(&self.water[i]), i)).unwrap();
            self.water.remove(far);
        }
    }

    pub fn mark_explored(&mut self, map_size: u32, x: f32, y: f32, radius: f32) {
        let c = map_size.div_ceil(CELL) as i32;
        let r = (radius / CELL as f32).ceil() as i32;
        let (cx, cy) = ((x / CELL as f32) as i32, (y / CELL as f32) as i32);
        for dy in -r..=r {
            for dx in -r..=r {
                let (ex, ey) = (cx + dx, cy + dy);
                if ex >= 0 && ey >= 0 && ex < c && ey < c {
                    let mid = ((ex as f32 + 0.5) * CELL as f32, (ey as f32 + 0.5) * CELL as f32);
                    if (mid.0 - x).powi(2) + (mid.1 - y).powi(2) <= (radius + CELL as f32 * 0.5).powi(2) {
                        self.explored[(ey * c + ex) as usize] = true;
                    }
                }
            }
        }
    }

    pub fn explored_share(&self) -> f32 {
        self.explored.iter().filter(|&&e| e).count() as f32 / self.explored.len().max(1) as f32
    }

    /// Known nodes of a kind with something left, nearest first (up to `k`).
    pub fn nearest_nodes(&self, kind: u16, from: (f32, f32), k: usize) -> Vec<(Id<Node>, &KnownNode, f32)> {
        self.nearest_by_kind(from, k, kind as usize + 1).swap_remove(kind as usize)
    }

    /// For every kind (below `kinds`) at once: known nodes with something left, nearest first
    /// (up to `k` each). One pass, because people know hundreds of places and k is small.
    pub fn nearest_by_kind(&self, from: (f32, f32), k: usize, kinds: usize) -> Vec<Vec<(Id<Node>, &KnownNode, f32)>> {
        let mut out: Vec<Vec<(Id<Node>, &KnownNode, f32)>> = (0..kinds).map(|_| Vec::with_capacity(k + 1)).collect();
        for (id, n) in &self.nodes {
            if n.amount < 1.0 || n.kind as usize >= kinds {
                continue;
            }
            let v = &mut out[n.kind as usize];
            let d2 = (n.x as f32 + 0.5 - from.0).powi(2) + (n.y as f32 + 0.5 - from.1).powi(2);
            if v.len() == k && v.last().is_some_and(|l| (d2, *id) >= (l.2, l.0)) {
                continue;
            }
            let at = v.partition_point(|e| (e.2, e.0) < (d2, *id));
            v.insert(at, (*id, n, d2));
            v.truncate(k);
        }
        for v in out.iter_mut() {
            for e in v.iter_mut() {
                e.2 = e.2.sqrt();
            }
        }
        out
    }

    pub fn nearest_water(&self, from: (f32, f32), k: usize) -> Vec<(&KnownWater, f32)> {
        let mut v: Vec<(&KnownWater, f32)> =
            self.water.iter().map(|w| (w, ((w.x as f32 + 0.5 - from.0).powi(2) + (w.y as f32 + 0.5 - from.1).powi(2)).sqrt())).collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1).then((a.0.x, a.0.y).cmp(&(b.0.x, b.0.y))));
        v.truncate(k);
        v
    }
}
