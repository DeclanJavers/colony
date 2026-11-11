//! Buildings and fields (M6): shelters, houses and storehouses, built from sites anyone can
//! work on, and fields of grain. Kinds are in data/base/buildings.ron, the crop in farming.ron.

use crate::camp::Camp;
use crate::content::Content;
use crate::ids::{Id, Table};
use crate::people::PersonId;
use crate::world::Map;
use serde::{Deserialize, Serialize};

/// A crop growing in a field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    pub sown: u64,
    /// 0 when sown .. 1 ripe.
    pub growth: f32,
    /// 0 .. 1: weeds cut growth and the harvest; tending pulls them.
    pub weeds: f32,
    /// What's left of a full harvest (frost, neglect, standing too long).
    pub health: f32,
    /// Killed by frost before it ripened.
    pub dead: bool,
    /// Who sowed it.
    pub by: Option<PersonId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Structure {
    pub kind: u16,
    /// Top-left tile of the footprint.
    pub x: u16,
    pub y: u16,
    /// Work done, 0..1 (finished at 1).
    pub progress: f32,
    /// Materials brought so far, per entry of the kind's cost.
    pub paid: Vec<u32>,
    /// The household it belongs to (whoever started a home; nobody for a storehouse or field).
    pub owner: Option<PersonId>,
    pub started: u64,
    pub finished: Option<u64>,
    /// Fields only.
    pub crop: Option<Crop>,
    /// Fields: grain harvested from it in all.
    pub harvested: u32,
    /// Homes: when it was last left empty (an empty shelter falls down after a few days).
    #[serde(default)]
    pub empty_since: Option<u64>,
    /// Homes: decoration steps done (M7: stones to the door, flower boxes, carved posts).
    #[serde(default)]
    pub decor: u8,
}

impl Structure {
    pub fn done(&self) -> bool {
        self.finished.is_some()
    }

    /// Centre of the footprint, in tiles.
    pub fn centre(&self, c: &Content) -> (f32, f32) {
        let d = &c.buildings[self.kind as usize];
        (self.x as f32 + d.w as f32 * 0.5, self.y as f32 + d.h as f32 * 0.5)
    }

    /// The tile in front of it (south side, middle), where people stand to use it.
    pub fn door(&self, c: &Content) -> (u16, u16) {
        let d = &c.buildings[self.kind as usize];
        if d.blocks { (self.x + d.w / 2, self.y + d.h) } else { (self.x + d.w / 2, self.y + d.h / 2) }
    }

    /// How much of the cost has been brought (0..1); work can't get ahead of it.
    pub fn paid_share(&self, c: &Content) -> f32 {
        let d = &c.buildings[self.kind as usize];
        let need: u32 = d.cost.iter().map(|(_, n)| n).sum();
        if need == 0 {
            return 1.0;
        }
        self.paid.iter().sum::<u32>() as f32 / need as f32
    }

    /// Materials still to bring: (item index, count).
    pub fn owed(&self, c: &Content) -> Vec<(usize, u32)> {
        let d = &c.buildings[self.kind as usize];
        d.cost.iter().zip(&self.paid).filter(|((_, n), p)| *n > **p).map(|((id, n), p)| (c.item(id), n - p)).collect()
    }
}

pub type StructureId = Id<Structure>;

/// Put a new site on the map.
pub fn place(structures: &mut Table<Structure>, map: &mut Map, c: &Content, kind: u16, at: (u16, u16), owner: Option<PersonId>, tick: u64) -> StructureId {
    let d = &c.buildings[kind as usize];
    let s = Structure { kind, x: at.0, y: at.1, progress: 0.0, paid: vec![0; d.cost.len()], owner, started: tick, finished: None, crop: None, harvested: 0, empty_since: None, decor: 0 };
    let id = structures.insert(s);
    for y in at.1..at.1 + d.h {
        for x in at.0..at.0 + d.w {
            let i = map.idx(x as u32, y as u32);
            map.built[i] = if d.blocks { 1 } else { 2 };
            map.building_at[i] = id.index + 1;
        }
    }
    map.version += 1;
    id
}

/// Take a structure off the map.
pub fn remove(structures: &mut Table<Structure>, map: &mut Map, c: &Content, id: StructureId) {
    let Some(s) = structures.remove(id) else { return };
    let d = &c.buildings[s.kind as usize];
    for y in s.y..s.y + d.h {
        for x in s.x..s.x + d.w {
            let i = map.idx(x as u32, y as u32);
            map.built[i] = 0;
            map.building_at[i] = 0;
        }
    }
    map.version += 1;
}

/// The structure on a tile, if any.
pub fn at(structures: &Table<Structure>, map: &Map, x: u32, y: u32) -> Option<StructureId> {
    let slot = map.building_at[map.idx(x, y)];
    if slot == 0 { None } else { structures.id_at(slot - 1) }
}

/// A clear spot for a new building near the camp: dry, flat, nothing standing on it or right
/// next to it, outside the ring round the fire, a little apart from other buildings. Searched
/// in rings outwards from the fire (fields further out), so the camp grows in a ring.
pub fn find_site(map: &Map, camp: &Camp, c: &Content, kind: u16) -> Option<(u16, u16)> {
    let d = &c.buildings[kind as usize];
    let (fx, fy) = camp.fire_pos();
    let (w, h) = (d.w as i32, d.h as i32);
    let n = map.size as i32;
    let start = if d.blocks { 6.5 } else { 11.0 };
    let mut r = start;
    while r < 40.0 {
        let steps = ((std::f32::consts::TAU * r) / 1.5).ceil() as usize;
        for k in 0..steps {
            let a = k as f32 / steps as f32 * std::f32::consts::TAU + r * 0.37;
            let (cx, cy) = (fx + a.cos() * r, fy + a.sin() * r);
            let (x0, y0) = ((cx - w as f32 * 0.5).floor() as i32, (cy - h as f32 * 0.5).floor() as i32);
            if x0 < 2 || y0 < 2 || x0 + w + 2 >= n || y0 + h + 2 >= n {
                continue;
            }
            // The footprint and a one-tile margin must be clear; the footprint must be flat.
            let level = map.level[map.idx(x0 as u32, y0 as u32)];
            let mut ok = true;
            'check: for y in y0 - 1..y0 + h + 1 {
                for x in x0 - 1..x0 + w + 1 {
                    let i = map.idx(x as u32, y as u32);
                    let inside = x >= x0 && x < x0 + w && y >= y0 && y < y0 + h;
                    if map.water[i].is_water() || map.built[i] != 0 || (inside && (map.node_at[i] != 0 || map.level[i] != level)) {
                        ok = false;
                        break 'check;
                    }
                    if !inside && map.node_at[i] != 0 && d.blocks {
                        // A tree right against the wall is fine for a field, not for a house door.
                        if y == y0 + h {
                            ok = false;
                            break 'check;
                        }
                    }
                }
            }
            // Keep clear of the stores pile.
            let (sx, sy) = camp.stores_pos();
            if ok && ((cx - sx).powi(2) + (cy - sy).powi(2)).sqrt() < 3.5 {
                ok = false;
            }
            if ok {
                return Some((x0 as u16, y0 as u16));
            }
        }
        r += 1.5;
    }
    None
}

/// Is there a finished storehouse?
pub fn has_storehouse(structures: &Table<Structure>, c: &Content) -> bool {
    let k = c.building("storehouse");
    structures.iter().any(|(_, s)| s.kind == k && s.done())
}

/// Spoil the stores for a day: food rots in the open, barely in a storehouse.
pub fn spoil(camp: &mut Camp, structures: &Table<Structure>, c: &Content, rng_frac: f32) {
    let stored = has_storehouse(structures, c);
    for i in 0..camp.stores.len() {
        let (open, kept) = c.spoil(i);
        let share = if stored { kept } else { open };
        if share <= 0.0 || camp.stores[i] == 0 {
            continue;
        }
        let lost = camp.stores[i] as f32 * share;
        // Round with a fraction carried by the day's random number, so small stores spoil too.
        let whole = lost.floor() as u32 + (lost.fract() > rng_frac) as u32;
        camp.stores[i] = camp.stores[i].saturating_sub(whole);
    }
}

/// A day passes on a field: the crop grows from moisture, fertility, temperature and season,
/// weeds come up, frost can kill it, and a ripe crop left standing slowly spoils.
pub fn grow(s: &mut Structure, map: &Map, c: &Content, temp: f32, season: usize) {
    let f = &c.farming;
    let Some(crop) = s.crop.as_mut() else { return };
    if crop.dead {
        return;
    }
    let d = &c.buildings[s.kind as usize];
    let (mut moist, mut fert) = (0.0, 0.0);
    for y in s.y..s.y + d.h {
        for x in s.x..s.x + d.w {
            let i = map.idx(x as u32, y as u32);
            moist += map.moisture[i];
            fert += map.fertility[i];
        }
    }
    let tiles = (d.w * d.h) as f32;
    let (moist, fert) = (moist / tiles, fert / tiles);
    // Frost: a light one scorches the young grain, a hard one (4 degrees below) kills it.
    if temp < f.frost && crop.growth < 1.0 {
        let hard = ((f.frost - temp) / 4.0).clamp(0.0, 1.0);
        crop.health *= 1.0 - 0.3 - 0.7 * hard;
        if crop.health < 0.2 {
            crop.dead = true;
            return;
        }
    }
    if crop.growth >= 1.0 {
        crop.health *= 1.0 - f.overripe_loss;
        return;
    }
    let g = growth_factors(c, moist, fert, temp, season);
    crop.growth = (crop.growth + g.total / f.days_to_ripe * (1.0 - crop.weeds * 0.6)).min(1.0);
    crop.weeds = (crop.weeds + f.weeds_per_day).min(1.0);
    // Heavy weeds cost some of the harvest.
    if crop.weeds > 0.6 {
        crop.health *= 0.97;
    }
}

/// Why a crop grows as fast as it does (each 0..1), for the field's inspector.
#[derive(Clone, Copy, Debug)]
pub struct Growth {
    pub moisture: f32,
    pub fertility: f32,
    pub warmth: f32,
    pub season: f32,
    pub total: f32,
}

pub fn growth_factors(c: &Content, moist: f32, fert: f32, temp: f32, season: usize) -> Growth {
    let f = &c.farming;
    let band = |v: f32, (lo, hi): (f32, f32), soft: f32| {
        if v < lo {
            (1.0 - (lo - v) / soft).max(0.0)
        } else if v > hi {
            (1.0 - (v - hi) / soft).max(0.0)
        } else {
            1.0
        }
    };
    let moisture = band(moist, f.best_moisture, 0.3);
    let warmth = band(temp, f.best_temp, 10.0);
    let fertility = (0.5 + fert).min(1.0);
    let season = [1.0, 1.1, 0.7, 0.0][season.min(3)];
    Growth { moisture, fertility, warmth, season, total: moisture * fertility * warmth * season }
}

/// Mean daytime temperature at a tile today (for growth).
pub fn day_temp(c: &Content, map: &Map, x: u32, y: u32, season: usize) -> f32 {
    map.temperature[map.idx(x, y)] + c.people.season_temp[season] + map.weather.offset
}
