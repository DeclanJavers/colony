//! Animals (M6): deer and rabbits in herds. They wander a home range, graze and rest, and flee
//! from people. Populations grow in spring, thin out with age and in winter when there are more
//! than the land holds, and new herds wander in from the map's edges when there are too few.
//! Hunted ground gets a fear that fades; herds move their range away from it. Numbers are in
//! data/base/animals.ron.

use crate::content::Content;
use crate::ids::{Id, Table};
use crate::rng::Rng;
use crate::world::Map;
use serde::{Deserialize, Serialize};

pub type AnimalId = Id<Animal>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Graze,
    Walk,
    Rest,
    Flee,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Animal {
    pub kind: u16,
    pub herd: u32,
    pub pos: (f32, f32),
    pub facing: f32,
    pub born: u64,
    pub female: bool,
    pub state: State,
    pub target: (f32, f32),
    /// Tick the current state ends (graze, rest, flee).
    pub until: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Herd {
    pub id: u32,
    pub kind: u16,
    /// Middle of the home range.
    pub centre: (f32, f32),
}

/// Cell size of the fear map (tiles).
pub const FEAR_CELL: u32 = 16;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Wildlife {
    pub herds: Vec<Herd>,
    pub next_herd: u32,
    /// How dangerous each 16x16 cell feels (hunting raises it; it fades by the day).
    pub fear: Vec<f32>,
    /// How many of each kind the land holds.
    pub capacity: Vec<u32>,
    /// Hunting kills, per kind, all time.
    pub hunted: Vec<u32>,
}

impl Wildlife {
    pub fn fear_at(&self, map: &Map, x: f32, y: f32) -> f32 {
        let c = map.size.div_ceil(FEAR_CELL);
        let (cx, cy) = ((x.max(0.0) as u32 / FEAR_CELL).min(c - 1), (y.max(0.0) as u32 / FEAR_CELL).min(c - 1));
        self.fear.get((cy * c + cx) as usize).copied().unwrap_or(0.0)
    }

    pub fn scare(&mut self, map: &Map, x: f32, y: f32, amount: f32) {
        let c = map.size.div_ceil(FEAR_CELL);
        let (cx, cy) = ((x.max(0.0) as u32 / FEAR_CELL).min(c - 1), (y.max(0.0) as u32 / FEAR_CELL).min(c - 1));
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let (ex, ey) = (cx as i32 + dx, cy as i32 + dy);
                if ex >= 0 && ey >= 0 && (ex as u32) < c && (ey as u32) < c {
                    let k = (ey as u32 * c + ex as u32) as usize;
                    let w = if dx == 0 && dy == 0 { 1.0 } else { 0.35 };
                    self.fear[k] = (self.fear[k] + amount * w).min(1.0);
                }
            }
        }
    }
}

fn habitat(c: &Content, kind: u16, map: &Map, i: usize) -> bool {
    !map.water[i].is_water() && map.built[i] == 0 && c.animals[kind as usize].habitats.iter().any(|h| c.biome(h) == map.biome[i])
}

/// Set up the region's wildlife: how many of each kind it holds, and the first herds, away
/// from where the group arrives.
pub fn populate(animals: &mut Table<Animal>, wild: &mut Wildlife, map: &Map, c: &Content, tick: u64, rng: &mut Rng) {
    let cells = map.size.div_ceil(FEAR_CELL);
    wild.fear = vec![0.0; (cells * cells) as usize];
    wild.hunted = vec![0; c.animals.len()];
    wild.capacity = (0..c.animals.len())
        .map(|k| {
            let tiles = (0..map.biome.len()).filter(|&i| habitat(c, k as u16, map, i)).count();
            (tiles as f32 / 1000.0 * c.animals[k].per_1000_tiles).round() as u32
        })
        .collect();
    for k in 0..c.animals.len() {
        let def = &c.animals[k];
        let avg = (def.herd.0 + def.herd.1) as f32 * 0.5;
        let herds = ((wild.capacity[k] as f32 * 0.75) / avg).round() as u32;
        for _ in 0..herds {
            if let Some(at) = random_habitat(map, c, k as u16, rng, Some((map.start, 28.0))) {
                new_herd(animals, wild, c, k as u16, at, tick, rng, true);
            }
        }
    }
}

/// A random habitat tile, optionally at least `away.1` tiles from `away.0`.
fn random_habitat(map: &Map, c: &Content, kind: u16, rng: &mut Rng, away: Option<((u32, u32), f32)>) -> Option<(f32, f32)> {
    for _ in 0..400 {
        let (x, y) = (rng.below(map.size), rng.below(map.size));
        if let Some(((ax, ay), d)) = away
            && ((x as f32 - ax as f32).powi(2) + (y as f32 - ay as f32).powi(2)).sqrt() < d
        {
            continue;
        }
        if habitat(c, kind, map, map.idx(x, y)) {
            return Some((x as f32 + 0.5, y as f32 + 0.5));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn new_herd(animals: &mut Table<Animal>, wild: &mut Wildlife, c: &Content, kind: u16, at: (f32, f32), tick: u64, rng: &mut Rng, mixed_ages: bool) {
    let def = &c.animals[kind as usize];
    let id = wild.next_herd;
    wild.next_herd += 1;
    wild.herds.push(Herd { id, kind, centre: at });
    let n = def.herd.0 + rng.below(def.herd.1 - def.herd.0 + 1);
    let life = def.lifespan_days as u64 * 1440;
    for k in 0..n {
        let age = if mixed_ages { (rng.next_f32() * 0.7 * life as f32) as u64 } else { (life as f32 * 0.2) as u64 };
        let pos = (at.0 + rng.next_f32() * 3.0 - 1.5, at.1 + rng.next_f32() * 3.0 - 1.5);
        animals.insert(Animal { kind, herd: id, pos, facing: rng.next_f32() * std::f32::consts::TAU, born: tick.saturating_sub(age), female: k % 2 == 0 || rng.chance(0.4), state: State::Graze, target: pos, until: tick + rng.below(60) as u64 });
    }
}

/// One minute for every animal. `people` are awake people's positions; stalking hunters count as
/// harder to spot.
pub fn tick(animals: &mut Table<Animal>, wild: &mut Wildlife, map: &Map, c: &Content, people: &[((f32, f32), bool)], tick: u64, rng: &mut Rng) {
    let tm = c.calendar.minutes_per_tick as f32;
    let ids: Vec<AnimalId> = animals.iter().map(|(id, _)| id).collect();
    for id in ids {
        let a = animals.get_mut(id).unwrap();
        let def = &c.animals[a.kind as usize];
        // Danger: the nearest person close enough to notice.
        let fear = wild.fear_at(map, a.pos.0, a.pos.1);
        let wary = def.wary * (1.0 + fear * 0.8);
        let mut threat: Option<((f32, f32), f32)> = None;
        for &(p, stalking) in people {
            let reach = if stalking { wary * 0.4 } else { wary };
            let d = ((p.0 - a.pos.0).powi(2) + (p.1 - a.pos.1).powi(2)).sqrt();
            if d < reach && threat.is_none_or(|t| d < t.1) {
                threat = Some((p, d));
            }
        }
        if let Some((p, d)) = threat {
            let (dx, dy) = (a.pos.0 - p.0, a.pos.1 - p.1);
            let len = d.max(0.1);
            a.target = (a.pos.0 + dx / len * 12.0, a.pos.1 + dy / len * 12.0);
            a.state = State::Flee;
            a.until = tick + 10;
        }
        let speed = match a.state {
            State::Flee => def.flee,
            State::Walk => def.speed,
            _ => 0.0,
        };
        if speed > 0.0 {
            let (dx, dy) = (a.target.0 - a.pos.0, a.target.1 - a.pos.1);
            let d = (dx * dx + dy * dy).sqrt();
            let step = (speed * tm).min(d);
            if d > 0.05 {
                let next = (a.pos.0 + dx / d * step, a.pos.1 + dy / d * step);
                let (nx, ny) = (next.0 as i32, next.1 as i32);
                let ok = map.in_bounds(nx, ny) && {
                    let i = map.idx(nx as u32, ny as u32);
                    !map.water[i].is_water() && map.built[i] != 1
                };
                if ok {
                    a.facing = dx.atan2(dy);
                    a.pos = next;
                } else {
                    // Blocked (water, a wall): try somewhere else.
                    a.until = tick;
                    a.state = State::Graze;
                }
            }
            if d <= 0.3 && a.state == State::Walk {
                a.state = State::Graze;
                a.until = tick + 20 + rng.below(70) as u64;
            }
        }
        if tick >= a.until && a.state != State::Walk {
            // Next: wander somewhere in the herd's range, or rest a while.
            let centre = wild.herds.iter().find(|h| h.id == a.herd).map_or(a.pos, |h| h.centre);
            if rng.chance(0.25) {
                a.state = State::Rest;
                a.until = tick + 30 + rng.below(90) as u64;
            } else {
                let ang = rng.next_f32() * std::f32::consts::TAU;
                let r = rng.next_f32().sqrt() * def.range;
                a.target = (centre.0 + ang.cos() * r, centre.1 + ang.sin() * r);
                a.state = State::Walk;
                a.until = tick + 240;
            }
        }
    }
}

/// Once a day: fear fades, herds leave frightening ground, animals age and die, winter thins
/// crowded herds, spring brings young, and new herds wander in from the edges.
#[allow(clippy::too_many_arguments)]
pub fn daily(animals: &mut Table<Animal>, wild: &mut Wildlife, map: &Map, c: &Content, tick: u64, season: usize, first_of_season: bool, rng: &mut Rng) {
    for f in wild.fear.iter_mut() {
        *f *= 0.93;
    }
    // Herds move their range off frightening ground (away from it, towards calmer cells).
    let fears: Vec<f32> = wild.herds.iter().map(|h| wild.fear_at(map, h.centre.0, h.centre.1)).collect();
    for (h, &fear) in wild.herds.iter_mut().zip(&fears) {
        if fear > 0.35 {
            let (sx, sy) = (map.start.0 as f32, map.start.1 as f32);
            let (dx, dy) = (h.centre.0 - sx, h.centre.1 - sy);
            let d = (dx * dx + dy * dy).sqrt().max(1.0);
            let to = (h.centre.0 + dx / d * 20.0, h.centre.1 + dy / d * 20.0);
            let (tx, ty) = (to.0 as i32, to.1 as i32);
            if map.in_bounds(tx, ty) && habitat(c, h.kind, map, map.idx(tx as u32, ty as u32)) {
                h.centre = to;
            }
        } else if fear < 0.1 && rng.chance(0.2) {
            // A calm herd's range drifts, a few tiles at a time, over good ground.
            let to = (h.centre.0 + (rng.next_f32() - 0.5) * 24.0, h.centre.1 + (rng.next_f32() - 0.5) * 24.0);
            let (tx, ty) = (to.0 as i32, to.1 as i32);
            if map.in_bounds(tx, ty) && habitat(c, h.kind, map, map.idx(tx as u32, ty as u32)) && map.built[map.idx(tx as u32, ty as u32)] == 0 {
                h.centre = to;
            }
        }
    }
    let ids: Vec<AnimalId> = animals.iter().map(|(id, _)| id).collect();
    let count = |animals: &Table<Animal>, k: u16| animals.iter().filter(|(_, a)| a.kind == k).count() as u32;
    let mut dead = Vec::new();
    for &id in &ids {
        let a = animals.get(id).unwrap();
        let def = &c.animals[a.kind as usize];
        let age_days = (tick.saturating_sub(a.born)) / 1440;
        if age_days > def.lifespan_days as u64 && rng.chance(0.2) {
            dead.push(id);
        }
    }
    // Winter: more than the land holds, and the weakest go.
    if season == 3 {
        for k in 0..c.animals.len() as u16 {
            let n = count(animals, k);
            let cap = wild.capacity[k as usize];
            if n > cap {
                let over = (n - cap) as f32 / n as f32;
                for &id in &ids {
                    if animals.get(id).unwrap().kind == k && rng.chance(over * 0.3) && !dead.contains(&id) {
                        dead.push(id);
                    }
                }
            }
        }
    }
    for id in dead {
        animals.remove(id);
    }
    // Spring: young, fewer when the land is full.
    if season == 0 && first_of_season {
        for k in 0..c.animals.len() as u16 {
            let def = &c.animals[k as usize];
            let n = count(animals, k);
            let room = (1.0 - n as f32 / wild.capacity[k as usize].max(1) as f32).clamp(0.0, 1.0);
            let mothers: Vec<(u32, (f32, f32))> = animals
                .iter()
                .filter(|(_, a)| a.kind == k && a.female && (tick.saturating_sub(a.born)) / 1440 > (def.lifespan_days / 6) as u64)
                .map(|(_, a)| (a.herd, a.pos))
                .collect();
            for (herd, pos) in mothers {
                let mut young = def.births * room;
                while young > 0.0 {
                    if rng.chance(young.min(1.0)) {
                        animals.insert(Animal { kind: k, herd, pos, facing: rng.next_f32() * std::f32::consts::TAU, born: tick, female: rng.chance(0.5), state: State::Graze, target: pos, until: tick + 30 });
                    }
                    young -= 1.0;
                }
            }
        }
    }
    // Herds that have died out are forgotten; new ones wander in from the edges.
    wild.herds.retain(|h| animals.iter().any(|(_, a)| a.herd == h.id));
    for k in 0..c.animals.len() as u16 {
        let n = count(animals, k);
        if (n as f32) < wild.capacity[k as usize] as f32 * 0.6 && rng.chance(0.25) {
            // Enter at a random edge, with a range a little way inland.
            let s = map.size as f32;
            let t = rng.next_f32() * s;
            let (ex, ey) = match rng.below(4) {
                0 => (t, 2.0),
                1 => (t, s - 3.0),
                2 => (2.0, t),
                _ => (s - 3.0, t),
            };
            let i = map.idx(ex as u32, ey as u32);
            if !habitat(c, k, map, i) {
                continue;
            }
            new_herd(animals, wild, c, k, (ex, ey), tick, rng, false);
            // Their range is inland from where they came in.
            let (cx, cy) = (s * 0.5, s * 0.5);
            let (dx, dy) = (cx - ex, cy - ey);
            let d = (dx * dx + dy * dy).sqrt().max(1.0);
            let inland = (ex + dx / d * 30.0, ey + dy / d * 30.0);
            if let Some(h) = wild.herds.last_mut() {
                h.centre = inland;
            }
        }
    }
}
