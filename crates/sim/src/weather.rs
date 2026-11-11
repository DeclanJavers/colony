//! Weather (M6): each morning rolls the day's sky from the season and the region's rain. It moves
//! the day's temperature, wets or dries the soil, builds or melts snow cover, and chills people
//! caught out in rain or snow. Numbers are in data/base/weather.ron.

use crate::content::Content;
use crate::rng::Rng;
use crate::world::Map;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Sky {
    #[default]
    Clear,
    Cloudy,
    Rain,
    Snow,
}

impl Sky {
    pub fn word(self) -> &'static str {
        match self {
            Sky::Clear => "clear",
            Sky::Cloudy => "cloudy",
            Sky::Rain => "rain",
            Sky::Snow => "snow",
        }
    }
    /// Rain or snow falling.
    pub fn wet(self) -> bool {
        matches!(self, Sky::Rain | Sky::Snow)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Weather {
    pub sky: Sky,
    /// Degrees added to the day's temperature (drift and cold snaps).
    pub offset: f32,
    /// Days left in a cold snap.
    pub snap_left: u32,
    /// Snow lying on the ground, 0..1.
    pub snow: f32,
    /// How wet the region is (mean worldgen rain over land), set once.
    pub region_rain: f32,
}

impl Weather {
    /// Degrees the sky adds at this hour: clear nights are colder, clouds keep warmth in.
    pub fn sky_temp(&self, c: &Content, hour: f32) -> f32 {
        // 1 at 03:00, fading to 0 by noon and by 18:00 (the hours a clear sky loses heat).
        let from_three = (hour - 3.0 + 12.0).rem_euclid(24.0) - 12.0;
        let night = 1.0 - (from_three.abs() / 9.0).min(1.0);
        match self.sky {
            Sky::Clear => c.weather.clear_night * night,
            _ => c.weather.cloudy,
        }
    }
}

/// Mean temperature at the camp for today (before the sky's effect), for deciding rain or snow.
fn day_temp(c: &Content, map: &Map, at: (u32, u32), season: usize, offset: f32) -> f32 {
    map.temperature[map.idx(at.0, at.1)] + c.people.season_temp[season] + offset
}

/// Roll a new day's weather, then wet, dry and snow the land.
pub fn new_day(map: &mut Map, c: &Content, season: usize, at: (u32, u32), rng: &mut Rng) {
    let w = &c.weather;
    if map.weather.region_rain <= 0.0 {
        let land: Vec<f32> = (0..map.rain.len()).filter(|&i| !map.water[i].is_water()).map(|i| map.rain[i]).collect();
        map.weather.region_rain = (land.iter().sum::<f32>() / land.len().max(1) as f32).max(0.1);
    }
    let wet_scale = (map.weather.region_rain / 0.6).clamp(0.5, 1.6);
    // Temperature drift, and winter cold snaps.
    let mut offset = (rng.next_f32() * 2.0 - 1.0) * w.drift;
    if map.weather.snap_left > 0 {
        map.weather.snap_left -= 1;
        offset += w.snap_cold;
    } else if season == 3 && rng.chance(w.snap_chance) {
        map.weather.snap_left = w.snap_days.saturating_sub(1);
        offset += w.snap_cold;
    }
    map.weather.offset = offset;
    let temp = day_temp(c, map, at, season, offset);
    let roll = rng.next_f32();
    let wet = (w.wet_chance[season] * wet_scale).min(0.9);
    map.weather.sky = if roll < wet {
        if temp < 0.5 { Sky::Snow } else { Sky::Rain }
    } else if roll < wet + w.cloud_chance[season] {
        Sky::Cloudy
    } else {
        Sky::Clear
    };
    // Snow cover builds on snowy days and melts on warm ones.
    let melt = (temp - 0.0).max(0.0) * w.melt_per_c;
    let fall = if map.weather.sky == Sky::Snow { w.snow_fall } else { 0.0 };
    map.weather.snow = (map.weather.snow + fall - melt).clamp(0.0, 1.0);
    // Soil: rain wets it; warm days dry it; ground near fresh water stays damp. Snow melting
    // waters it too.
    let rain = if map.weather.sky == Sky::Rain { w.rain_wet } else { 0.0 } + melt.min(map.weather.snow + melt) * 0.3;
    let season_temp = c.people.season_temp[season] + offset;
    for i in 0..map.moisture.len() {
        if map.water[i].is_water() {
            continue;
        }
        let dry = w.dry_base + (map.temperature[i] + season_temp).max(0.0) * w.dry_per_c;
        let mut m = map.moisture[i] + rain * (0.6 + map.rain[i] * 0.8) - dry;
        if map.near_water[i] {
            m = m.max(w.damp_near_water);
        }
        map.moisture[i] = m.clamp(0.02, 1.0);
    }
}
