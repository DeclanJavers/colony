//! Pathfinding: A* over the tile grid, 8 directions. Sea and lakes block; rivers can be waded
//! slowly (fords and bridges are an expansion); slopes cost more; walking between trees and rocks
//! is slower; worn trails are faster. Scratch buffers are reused between searches.

use crate::world::{DIRS, Map, Water};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Walking pace off the trails, by biome (data/base/biomes.ron order): sea, lake, river,
/// beach, marsh, meadow, forest, pine forest, hills, mountain, snow.
const ROUGH: [f32; 11] = [1.0, 1.0, 1.0, 0.85, 0.65, 0.85, 0.7, 0.75, 0.8, 0.65, 0.6];

/// How many footsteps make a tile a full trail.
pub const TRAIL_STEPS: f32 = 120.0;

/// Cost to step onto a tile (1 = open ground), or None if it can't be entered.
pub fn tile_cost(map: &Map, i: usize) -> Option<f32> {
    let mut c = match map.water[i] {
        Water::Sea | Water::Lake => return None,
        Water::River => 6.0,
        Water::None => 1.0,
    };
    if map.node_at[i] != 0 {
        c += 1.5;
    }
    match map.built[i] {
        1 => return None,
        2 => c += 0.6,
        _ => {}
    }
    // Trails pull people onto them early, so traffic gathers into paths.
    c *= 1.0 - 0.45 * (map.footfall[i] / (TRAIL_STEPS * 0.25)).min(1.0);
    Some(c)
}

/// How fast someone walks on a tile, relative to open ground: wading is slow, squeezing between
/// trees a little slow, trails a little quick. (Route costs above are separate: people avoid
/// wading more than its speed alone would say.)
pub fn walk_pace(map: &Map, i: usize) -> f32 {
    let mut p = match map.water[i] {
        Water::River => 0.4,
        Water::None => 1.0,
        _ => 0.3,
    };
    if map.node_at[i] != 0 {
        p *= 0.8;
    }
    if map.built[i] == 2 {
        p *= 0.85;
    }
    // Rough ground (undergrowth, tussocks, mud) is slow going; a worn trail is quick.
    let rough = ROUGH.get(map.biome[i] as usize).copied().unwrap_or(1.0);
    let trail = (map.footfall[i] / TRAIL_STEPS).min(1.0);
    p * (rough + (1.15 - rough) * trail)
}

/// The same tile's pace if it had never been walked (for the trip-time stat: how much the trails
/// save on the trips people actually make).
pub fn walk_pace_untrodden(map: &Map, i: usize) -> f32 {
    let mut p = match map.water[i] {
        Water::River => 0.4,
        Water::None => 1.0,
        _ => 0.3,
    };
    if map.node_at[i] != 0 {
        p *= 0.8;
    }
    if map.built[i] == 2 {
        p *= 0.85;
    }
    p * ROUGH.get(map.biome[i] as usize).copied().unwrap_or(1.0)
}

#[derive(PartialEq)]
struct Key(f32, u32);
impl Eq for Key {}
impl Ord for Key {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
    }
}
impl PartialOrd for Key {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

#[derive(Default)]
pub struct Pathfinder {
    g: Vec<f32>,
    from: Vec<u32>,
    stamp: Vec<u32>,
    round: u32,
    heap: BinaryHeap<Key>,
}

impl Pathfinder {
    /// The tiles to walk through from `start` to `goal` (excluding start), or None. Gives up after
    /// `max_expand` tiles. If the goal itself can't be entered, the path ends next to it.
    pub fn find(&mut self, map: &Map, start: (u32, u32), goal: (u32, u32), max_expand: usize) -> Option<Vec<(u16, u16)>> {
        let n = map.size as usize;
        if self.g.len() != n * n {
            self.g = vec![0.0; n * n];
            self.from = vec![0; n * n];
            self.stamp = vec![0; n * n];
        }
        self.round = self.round.wrapping_add(1);
        if self.round == 0 {
            self.stamp.fill(0);
            self.round = 1;
        }
        let (s, t) = (start.1 as usize * n + start.0 as usize, goal.1 as usize * n + goal.0 as usize);
        let goal_open = tile_cost(map, t).is_some();
        let h = |i: usize| {
            let (dx, dy) = (((i % n) as i32 - goal.0 as i32).abs() as f32, ((i / n) as i32 - goal.1 as i32).abs() as f32);
            (dx.max(dy) - dx.min(dy)) + dx.min(dy) * std::f32::consts::SQRT_2
        };
        self.heap.clear();
        self.stamp[s] = self.round;
        self.g[s] = 0.0;
        self.from[s] = s as u32;
        self.heap.push(Key(h(s), s as u32));
        let mut expanded = 0;
        let mut reached = None;
        while let Some(Key(_, i)) = self.heap.pop() {
            let i = i as usize;
            if i == t || (!goal_open && h(i) < 1.5) {
                reached = Some(i);
                break;
            }
            expanded += 1;
            if expanded > max_expand {
                return None;
            }
            let (x, y) = ((i % n) as i32, (i / n) as i32);
            let gi = self.g[i];
            for (d, (dx, dy)) in DIRS.iter().enumerate() {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx as usize >= n || ny as usize >= n {
                    continue;
                }
                let j = ny as usize * n + nx as usize;
                let Some(c) = tile_cost(map, j) else { continue };
                if d % 2 == 1 {
                    // No cutting corners past water.
                    let a = y as usize * n + nx as usize;
                    let b = ny as usize * n + x as usize;
                    if tile_cost(map, a).is_none() || tile_cost(map, b).is_none() {
                        continue;
                    }
                }
                let climb = (map.elevation[j] - map.elevation[i]).abs() * 1.5;
                let step = c * if d % 2 == 1 { std::f32::consts::SQRT_2 } else { 1.0 } + climb;
                let ng = gi + step;
                if self.stamp[j] != self.round || ng < self.g[j] {
                    self.stamp[j] = self.round;
                    self.g[j] = ng;
                    self.from[j] = i as u32;
                    self.heap.push(Key(ng + h(j), j as u32));
                }
            }
        }
        let end = reached?;
        let mut path = Vec::new();
        let mut i = end;
        while i != s {
            path.push(((i % n) as u16, (i / n) as u16));
            i = self.from[i] as usize;
        }
        path.reverse();
        Some(path)
    }
}
