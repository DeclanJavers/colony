//! Content: the data files in data/base/. They're built into the binary, so a world always gets
//! the same content as the code it was made with. Their hash goes into every save.

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Calendar {
    pub minutes_per_tick: u32,
    pub days_per_season: u32,
    pub seasons: Vec<String>,
    pub start_hour: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BiomeDef {
    pub id: String,
    pub name: String,
    pub colour: String,
    pub fertility: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NodeDef {
    pub id: String,
    pub name: String,
    pub amount: f32,
    pub variants: u8,
    pub blocks: bool,
    pub clump: f32,
    pub near_water: f32,
    pub colour: String,
    pub densities: Vec<(String, f32)>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GenParams {
    pub sea_fraction: (f32, f32),
    pub inland_rise: f32,
    pub coast_noise_scale: f32,
    pub coast_roughness: f32,
    pub hills_scale: f32,
    pub hills_height: f32,
    pub detail_scale: f32,
    pub detail_height: f32,
    pub mountain_chance: f32,
    pub mountain_height: (f32, f32),
    pub mountain_scale: f32,
    pub mountain_elevation: f32,
    pub erosion_passes: u32,
    pub talus: f32,
    pub river_threshold: f32,
    pub river_share_max: u32,
    pub river_wander: f32,
    pub lake_min_depth: f32,
    pub lake_min_area: u32,
    pub max_lakes: u32,
    pub base_temperature: (f32, f32),
    pub lapse_per_m: f32,
    pub base_rain: (f32, f32),
    pub start_margin: u32,
}

#[derive(Clone, Debug)]
pub struct Content {
    pub calendar: Calendar,
    pub biomes: Vec<BiomeDef>,
    pub nodes: Vec<NodeDef>,
    pub worldgen: GenParams,
    pub hash: u64,
}

const CALENDAR: &str = include_str!("../../../data/base/calendar.ron");
const BIOMES: &str = include_str!("../../../data/base/biomes.ron");
const NODES: &str = include_str!("../../../data/base/nodes.ron");
const WORLDGEN: &str = include_str!("../../../data/base/worldgen.ron");

fn parse<T: for<'de> Deserialize<'de>>(name: &str, text: &str) -> T {
    ron::from_str(text).unwrap_or_else(|e| panic!("data/base/{name}: {e}"))
}

impl Content {
    /// The base game's content.
    pub fn base() -> Self {
        let mut all = Vec::new();
        for t in [CALENDAR, BIOMES, NODES, WORLDGEN] {
            all.extend_from_slice(t.as_bytes());
        }
        Content {
            calendar: parse("calendar.ron", CALENDAR),
            biomes: parse("biomes.ron", BIOMES),
            nodes: parse("nodes.ron", NODES),
            worldgen: parse("worldgen.ron", WORLDGEN),
            hash: crate::hash::fnv64(&all),
        }
    }

    pub fn biome(&self, id: &str) -> u8 {
        self.biomes.iter().position(|b| b.id == id).unwrap_or_else(|| panic!("no biome '{id}'")) as u8
    }

    pub fn node_kind(&self, id: &str) -> u16 {
        self.nodes.iter().position(|n| n.id == id).unwrap_or_else(|| panic!("no node kind '{id}'")) as u16
    }
}

/// "#RRGGBB" to bytes.
pub fn hex_colour(s: &str) -> [u8; 3] {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0xFF00FF);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

#[cfg(test)]
mod tests {
    #[test]
    fn base_content_parses() {
        let c = super::Content::base();
        assert!(c.biomes.len() >= 10);
        assert!(c.nodes.iter().all(|n| n.densities.iter().all(|(b, _)| c.biomes.iter().any(|x| &x.id == b))));
    }
}
