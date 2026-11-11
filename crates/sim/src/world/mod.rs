//! The world: a square grid of 1 m tiles, each with the fields that systems read and expansions
//! later change (ARCHITECTURE.md section 3), plus the resource nodes on it.

pub mod worldgen;
pub mod noise;
mod stages;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Water {
    None,
    Sea,
    Lake,
    River,
}

impl Water {
    pub fn is_water(self) -> bool {
        self != Water::None
    }
    pub fn is_fresh(self) -> bool {
        matches!(self, Water::Lake | Water::River)
    }
}

/// A resource node: a tree, rock, bush... One per tile at most.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub kind: u16,
    pub x: u16,
    pub y: u16,
    pub variant: u8,
    /// Quarter turns, for drawing.
    pub turn: u8,
    pub amount: f32,
}

/// Flow directions: index 0..8, as (dx, dy). 255 = no flow (sea, or the lowest point).
pub const DIRS: [(i32, i32); 8] = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
pub const NO_FLOW: u8 = 255;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Map {
    pub size: u32,
    /// Ground height in metres (sea level 0). Under water: the bed.
    pub elevation: Vec<f32>,
    /// Whole-metre terrace level of the surface, for building and drawing.
    pub level: Vec<u8>,
    pub water: Vec<Water>,
    /// Water surface height where there is water.
    pub water_level: Vec<f32>,
    /// Which way water runs off each tile (index into DIRS).
    pub flow_dir: Vec<u8>,
    /// How much rain drains through each tile (sum of upstream rain).
    pub flow: Vec<f32>,
    pub biome: Vec<u8>,
    /// Mean yearly temperature, degrees C.
    pub temperature: Vec<f32>,
    /// How wet the climate is, 0..1.
    pub rain: Vec<f32>,
    /// Soil moisture, 0..1. Seasons and weather will move it; irrigation (an expansion) raises it.
    pub moisture: Vec<f32>,
    /// Soil fertility, 0..1. Farming uses it up; manure and rotation (an expansion) restore it.
    pub fertility: Vec<f32>,
    /// Footsteps worn into each tile. Busy routes become trails.
    pub footfall: Vec<f32>,
    /// Beauty of the surroundings (flowers, carvings...).
    pub beauty: Vec<f32>,
    /// Node table slot index + 1 on each tile, 0 for none.
    pub node_at: Vec<u32>,
    /// What's built on each tile: 0 nothing, 1 a building (blocks walking), 2 a field.
    pub built: Vec<u8>,
    /// Structure table slot index + 1 on each tile of a building or field, 0 for none.
    pub building_at: Vec<u32>,
    /// Within 2 tiles of a river or lake (the soil there stays damp).
    pub near_water: Vec<bool>,
    /// Today's weather and the snow on the ground.
    pub weather: crate::weather::Weather,
    /// Where the first group arrives.
    pub start: (u32, u32),
    /// Bumped on every change to tiles or nodes (caches key on it).
    pub version: u64,
}

impl Map {
    pub fn new(size: u32) -> Self {
        let n = (size * size) as usize;
        Map {
            size,
            elevation: vec![0.0; n],
            level: vec![0; n],
            water: vec![Water::None; n],
            water_level: vec![0.0; n],
            flow_dir: vec![NO_FLOW; n],
            flow: vec![0.0; n],
            biome: vec![0; n],
            temperature: vec![0.0; n],
            rain: vec![0.0; n],
            moisture: vec![0.0; n],
            fertility: vec![0.0; n],
            footfall: vec![0.0; n],
            beauty: vec![0.0; n],
            node_at: vec![0; n],
            built: vec![0; n],
            building_at: vec![0; n],
            near_water: vec![false; n],
            weather: Default::default(),
            start: (size / 2, size / 2),
            version: 0,
        }
    }

    #[inline]
    pub fn idx(&self, x: u32, y: u32) -> usize {
        (y * self.size + x) as usize
    }

    #[inline]
    pub fn xy(&self, i: usize) -> (u32, u32) {
        (i as u32 % self.size, i as u32 / self.size)
    }

    #[inline]
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.size && (y as u32) < self.size
    }

    /// The tile a tile's water flows into, if any.
    pub fn downstream(&self, i: usize) -> Option<usize> {
        let d = self.flow_dir[i];
        if d == NO_FLOW {
            return None;
        }
        let (x, y) = self.xy(i);
        let (dx, dy) = DIRS[d as usize];
        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
        self.in_bounds(nx, ny).then(|| self.idx(nx as u32, ny as u32))
    }
}
