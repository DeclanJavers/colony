//! The base game's world generation stages, in pipeline order (see worldgen.rs).

// Grid code reads more clearly with tile indices.
#![allow(clippy::needless_range_loop)]

use super::worldgen::GenCtx;
use super::noise::{fbm, ridged, smoothstep};
use super::{DIRS, NO_FLOW, Node, Water};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

const SQ: f32 = std::f32::consts::FRAC_1_SQRT_2;
const COAST_DIRS: [(f32, f32); 8] = [(1.0, 0.0), (SQ, SQ), (0.0, 1.0), (-SQ, SQ), (-1.0, 0.0), (-SQ, -SQ), (0.0, -1.0), (SQ, -SQ)];

// ---------------------------------------------------------------------------------------------
// Helpers

/// Position across the map toward the sea: about -1.4 far inland to 1.4 far out to sea.
fn along(n: usize, x: usize, y: usize, d: (f32, f32)) -> f32 {
    let u = (x as f32 + 0.5) / n as f32 * 2.0 - 1.0;
    let v = (y as f32 + 0.5) / n as f32 * 2.0 - 1.0;
    u * d.0 + v * d.1
}

fn for_n4(n: usize, i: usize, mut f: impl FnMut(usize)) {
    let (x, y) = (i % n, i / n);
    if x > 0 {
        f(i - 1);
    }
    if x + 1 < n {
        f(i + 1);
    }
    if y > 0 {
        f(i - n);
    }
    if y + 1 < n {
        f(i + n);
    }
}

/// Each of the 8 neighbours with its direction index into DIRS.
fn for_n8(n: usize, i: usize, mut f: impl FnMut(usize, usize)) {
    let (x, y) = ((i % n) as i32, (i / n) as i32);
    for (d, (dx, dy)) in DIRS.iter().enumerate() {
        let (nx, ny) = (x + dx, y + dy);
        if nx >= 0 && ny >= 0 && (nx as usize) < n && (ny as usize) < n {
            f(ny as usize * n + nx as usize, d);
        }
    }
}

/// Steps (4-neighbour) from the nearest source to every tile; u32::MAX where unreachable.
fn bfs(n: usize, sources: &[usize]) -> Vec<u32> {
    let mut dist = vec![u32::MAX; n * n];
    let mut q = VecDeque::with_capacity(sources.len());
    for &s in sources {
        dist[s] = 0;
        q.push_back(s);
    }
    while let Some(i) = q.pop_front() {
        let d = dist[i] + 1;
        for_n4(n, i, |j| {
            if dist[j] == u32::MAX {
                dist[j] = d;
                q.push_back(j);
            }
        });
    }
    dist
}

/// Summed-area table of a per-tile count, for fast "how many within this square" questions.
fn sat(n: usize, f: impl Fn(usize) -> u32) -> Vec<u32> {
    let w = n + 1;
    let mut s = vec![0u32; w * w];
    for y in 0..n {
        let mut row = 0;
        for x in 0..n {
            row += f(y * n + x);
            s[(y + 1) * w + x + 1] = s[y * w + x + 1] + row;
        }
    }
    s
}

/// Sum over the square of half-width `r` around (x, y), clipped to the map.
fn sat_square(s: &[u32], n: usize, x: usize, y: usize, r: usize) -> u32 {
    let w = n + 1;
    let (x0, y0) = (x.saturating_sub(r), y.saturating_sub(r));
    let (x1, y1) = ((x + r + 1).min(n), (y + r + 1).min(n));
    s[y1 * w + x1] + s[y0 * w + x0] - s[y0 * w + x1] - s[y1 * w + x0]
}

/// Min-heap key for the priority flood: lowest height first, then lowest index (deterministic).
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

// ---------------------------------------------------------------------------------------------
// Stages

/// Land and sea: a coast on one side of the map, rising inland, roughened by noise. The sea level
/// is set so the seed's chosen share of the map is below it.
pub fn shape(ctx: &mut GenCtx) {
    let (n, p) = (ctx.n, ctx.p);
    let mut r = ctx.rng("gen.shape");
    let d = COAST_DIRS[r.below(8) as usize];
    ctx.coast_dir = d;
    let coast_pos = r.range_f32(0.15, 0.55);
    let sea_frac = r.range_f32(p.sea_fraction.0, p.sea_fraction.1);
    let (s1, s2, s3) = (r.next_u64(), r.next_u64(), r.next_u64());
    let e = &mut ctx.map.elevation;
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32, y as f32);
            let ramp = (coast_pos - along(n, x, y, d)) * p.inland_rise;
            let warp = fbm(s1, fx / p.coast_noise_scale, fy / p.coast_noise_scale, 4) * p.inland_rise * p.coast_roughness;
            let hills = fbm(s2, fx / p.hills_scale, fy / p.hills_scale, 4) * p.hills_height;
            let detail = fbm(s3, fx / p.detail_scale, fy / p.detail_scale, 3) * p.detail_height;
            e[y * n + x] = ramp + warp + hills + detail;
        }
    }
    let mut sorted = e.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let sea_level = sorted[((sea_frac * sorted.len() as f32) as usize).min(sorted.len() - 1)];
    for v in e.iter_mut() {
        *v -= sea_level;
    }
    ctx.report.notes.push(format!("sea about {:.0}% of the map", sea_frac * 100.0));
}

/// Mountains: ridged ranges inland, away from the coast. Some seeds have none.
pub fn mountains(ctx: &mut GenCtx) {
    let (n, p, d) = (ctx.n, ctx.p, ctx.coast_dir);
    let mut r = ctx.rng("gen.mountains");
    if !r.chance(p.mountain_chance) {
        ctx.report.notes.push("no mountains".into());
        return;
    }
    let h = r.range_f32(p.mountain_height.0, p.mountain_height.1);
    let reach = r.range_f32(0.3, 0.9);
    let (s1, s2) = (r.next_u64(), r.next_u64());
    let start = 1.0 - reach * 1.5;
    let e = &mut ctx.map.elevation;
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32, y as f32);
            let inland = -along(n, x, y, d);
            let gaps = (0.6 + fbm(s2, fx / 90.0, fy / 90.0, 2) * 1.2).clamp(0.0, 1.0);
            let mask = smoothstep(start, start + 0.8, inland) * gaps;
            if mask > 0.0 {
                e[y * n + x] += mask * ridged(s1, fx / p.mountain_scale, fy / p.mountain_scale, 5) * h;
            }
        }
    }
    ctx.report.notes.push(format!("mountains up to about {h:.0} m"));
}

/// Light thermal erosion: slopes steeper than the talus angle shed material downhill.
pub fn erosion(ctx: &mut GenCtx) {
    let (n, p) = (ctx.n, ctx.p);
    let e = &mut ctx.map.elevation;
    let mut delta = vec![0.0f32; n * n];
    for _ in 0..p.erosion_passes {
        delta.fill(0.0);
        for i in 0..n * n {
            let h = e[i];
            let (mut low, mut drop) = (i, 0.0f32);
            for_n8(n, i, |j, d| {
                let dd = (h - e[j]) * if d % 2 == 1 { SQ } else { 1.0 };
                if dd > drop {
                    drop = dd;
                    low = j;
                }
            });
            if drop > p.talus {
                let m = (drop - p.talus) * 0.25;
                delta[i] -= m;
                delta[low] += m;
            }
        }
        for (v, dv) in e.iter_mut().zip(&delta) {
            *v += dv;
        }
    }
}

/// The sea: tiles below sea level connected to the map edge. Low hollows inland become lakes later.
pub fn coast(ctx: &mut GenCtx) {
    let n = ctx.n;
    let map = &mut ctx.map;
    let mut sea = vec![false; n * n];
    let mut q = VecDeque::new();
    for i in 0..n * n {
        let (x, y) = (i % n, i / n);
        if (x == 0 || y == 0 || x == n - 1 || y == n - 1) && map.elevation[i] < 0.0 {
            sea[i] = true;
            q.push_back(i);
        }
    }
    while let Some(i) = q.pop_front() {
        for_n4(n, i, |j| {
            if !sea[j] && map.elevation[j] < 0.0 {
                sea[j] = true;
                q.push_back(j);
            }
        });
    }
    for i in 0..n * n {
        map.water[i] = if sea[i] { Water::Sea } else { Water::None };
        map.water_level[i] = 0.0;
        // The sea floor drops away at the coast: no near-dry flats that wall off river mouths.
        if sea[i] {
            map.elevation[i] = map.elevation[i].min(-0.35);
        }
    }
}

/// Temperature (cooler with height) and rain (wetter near the sea and on slopes facing the wind,
/// drier in the shadow of mountains). The wind blows in from the sea.
pub fn climate(ctx: &mut GenCtx) {
    let (n, p, d) = (ctx.n, ctx.p, ctx.coast_dir);
    let mut r = ctx.rng("gen.climate");
    let base_t = r.range_f32(p.base_temperature.0, p.base_temperature.1);
    let base_rain = r.range_f32(p.base_rain.0, p.base_rain.1);
    let (s1, s2) = (r.next_u64(), r.next_u64());
    let sea: Vec<usize> = (0..n * n).filter(|&i| ctx.map.water[i] == Water::Sea).collect();
    ctx.dist_sea = bfs(n, &sea);
    let map = &mut ctx.map;
    let e = &map.elevation;
    let at = |x: f32, y: f32| -> Option<f32> {
        (x >= 0.0 && y >= 0.0 && (x as usize) < n && (y as usize) < n).then(|| e[y as usize * n + x as usize].max(0.0))
    };
    let mut temperature = vec![0.0; n * n];
    let mut rain = vec![0.0; n * n];
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let (fx, fy) = (x as f32, y as f32);
            let ei = e[i].max(0.0);
            temperature[i] = base_t - p.lapse_per_m * ei + fbm(s1, fx / 60.0, fy / 60.0, 2) * 0.8;
            let mut upwind_max = ei;
            for k in 1..=10 {
                match at(fx + d.0 * 4.0 * k as f32, fy + d.1 * 4.0 * k as f32) {
                    Some(h) => upwind_max = upwind_max.max(h),
                    None => break,
                }
            }
            let shadow = ((upwind_max - ei) / 25.0).clamp(0.0, 1.0) * 0.35;
            let lift = at(fx + d.0 * 4.0, fy + d.1 * 4.0).map_or(0.0, |h| ((ei - h) / 8.0).clamp(0.0, 1.0) * 0.15);
            let coastal = 0.2 * (1.0 - (ctx.dist_sea[i] as f32 / (n as f32 * 0.6)).min(1.0));
            rain[i] = (base_rain + coastal - shadow + lift + fbm(s2, fx / 50.0, fy / 50.0, 3) * 0.08).clamp(0.05, 1.0);
        }
    }
    map.temperature = temperature;
    map.rain = rain;
    ctx.report.notes.push(format!("mean temperature {base_t:.1} C at sea level, rain {base_rain:.2}"));
}

const EPS: f32 = 5e-4;

/// Water on land. A priority flood from the sea gives every tile a way downhill to the sea, filling
/// hollows. Routing runs over the ground plus a little noise, so rivers wander instead of running
/// in straight lines. The biggest deep hollows become lakes; the rest are filled in. Rain is then
/// routed downhill, and tiles that drain enough of it become rivers, which always end in a lake or
/// the sea.
pub fn hydrology(ctx: &mut GenCtx) {
    let (n, p) = (ctx.n, ctx.p);
    let nn = n * n;
    let wander = ctx.rng("gen.hydrology").next_u64();
    let map = &mut ctx.map;
    let e0 = map.elevation.clone();
    let route: Vec<f32> = (0..nn)
        .map(|i| {
            let (fx, fy) = ((i % n) as f32, (i / n) as f32);
            e0[i] + (fbm(wander, fx / 7.0, fy / 7.0, 2) + 1.0) * p.river_wander
        })
        .collect();
    let mut filled = route.clone();
    let mut seen = vec![false; nn];
    let mut heap = BinaryHeap::new();
    for i in 0..nn {
        if map.water[i] == Water::Sea {
            seen[i] = true;
            heap.push(Key(filled[i], i as u32));
        }
    }
    if heap.is_empty() {
        for i in 0..nn {
            let (x, y) = (i % n, i / n);
            if x == 0 || y == 0 || x == n - 1 || y == n - 1 {
                seen[i] = true;
                heap.push(Key(filled[i], i as u32));
            }
        }
    }
    while let Some(Key(h, i)) = heap.pop() {
        for_n8(n, i as usize, |j, d| {
            if !seen[j] {
                seen[j] = true;
                filled[j] = route[j].max(h + EPS);
                map.flow_dir[j] = ((d + 4) % 8) as u8;
                heap.push(Key(filled[j], j as u32));
            }
        });
    }

    // Lakes: the biggest deep hollows. Everything else is filled in.
    let mut comp = vec![false; nn];
    let mut hollows: Vec<(usize, Vec<usize>)> = Vec::new();
    for s in 0..nn {
        if comp[s] || map.water[s] != Water::None || filled[s] - route[s] <= 0.02 {
            continue;
        }
        comp[s] = true;
        let mut cells = vec![s];
        let mut k = 0;
        while k < cells.len() {
            let c = cells[k];
            k += 1;
            for_n4(n, c, |j| {
                if !comp[j] && map.water[j] == Water::None && filled[j] - route[j] > 0.02 {
                    comp[j] = true;
                    cells.push(j);
                }
            });
        }
        let deep = cells.iter().filter(|&&c| filled[c] - route[c] > p.lake_min_depth).count();
        if deep as u32 >= p.lake_min_area {
            hollows.push((deep, cells));
        }
    }
    hollows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1[0].cmp(&b.1[0])));
    hollows.truncate(p.max_lakes as usize);
    let lakes = hollows.len();
    for (_, cells) in &hollows {
        let surface = cells.iter().map(|&c| filled[c] - (route[c] - e0[c])).fold(f32::MIN, f32::max);
        for &c in cells {
            map.water[c] = Water::Lake;
            map.water_level[c] = surface;
        }
    }
    for i in 0..nn {
        if map.water[i] == Water::None {
            // Raise filled hollows; elsewhere this is the ground as it was.
            map.elevation[i] = e0[i] + (filled[i] - route[i]);
        }
    }

    // Route the rain downhill, highest tiles first.
    let mut order: Vec<u32> = (0..nn as u32).collect();
    order.sort_by(|&a, &b| filled[b as usize].total_cmp(&filled[a as usize]).then(a.cmp(&b)));
    let mut acc = map.rain.clone();
    for &i in &order {
        if let Some(j) = map.downstream(i as usize) {
            let a = acc[i as usize];
            acc[j] += a;
        }
    }

    // Rivers: pick a threshold that gives a sensible amount of river for this map.
    let land = (0..nn).filter(|&i| map.water[i] == Water::None).count().max(1);
    let mut thr = p.river_threshold * land as f32;
    for _ in 0..12 {
        let count = (0..nn).filter(|&i| map.water[i] == Water::None && acc[i] > thr).count();
        if count < n * 2 {
            thr *= 0.75;
        } else if count > land / p.river_share_max as usize {
            thr *= 1.3;
        } else {
            break;
        }
    }
    let mut river = vec![false; nn];
    let land_at = |map: &crate::world::Map, x: i32, y: i32| -> Option<usize> {
        (x >= 0 && y >= 0 && (x as usize) < n && (y as usize) < n)
            .then(|| y as usize * n + x as usize)
            .filter(|&j| map.water[j] == Water::None)
    };
    for i in 0..nn {
        if map.water[i] != Water::None || acc[i] <= thr {
            continue;
        }
        river[i] = true;
        let d = map.flow_dir[i];
        if d == NO_FLOW {
            continue;
        }
        let (x, y) = ((i % n) as i32, (i / n) as i32);
        let (dx, dy) = DIRS[d as usize];
        // A diagonal step also takes the lower of the two side tiles, so the river stays joined up.
        if dx != 0 && dy != 0 {
            let a = land_at(map, x + dx, y);
            let b = land_at(map, x, y + dy);
            let pick = match (a, b) {
                (Some(a), Some(b)) => Some(if map.elevation[a] <= map.elevation[b] { a } else { b }),
                (a, b) => a.or(b),
            };
            if let Some(j) = pick {
                river[j] = true;
            }
        }
        // Big rivers are wider.
        if acc[i] > thr * 6.0 {
            for (ox, oy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if let Some(j) = land_at(map, x + ox, y + oy) {
                    river[j] = true;
                }
            }
        }
    }
    // Single land tiles left inside a channel become river too.
    for _ in 0..2 {
        for i in 0..nn {
            if map.water[i] != Water::None || river[i] {
                continue;
            }
            let mut wet = 0;
            for_n4(n, i, |j| wet += (river[j] || map.water[j].is_fresh()) as u32);
            if wet >= 3 {
                river[i] = true;
            }
        }
    }
    let mut river_tiles = 0;
    for i in 0..nn {
        if river[i] {
            river_tiles += 1;
            map.water[i] = Water::River;
            map.water_level[i] = map.elevation[i] - 0.15;
            map.elevation[i] -= 0.6;
        }
    }
    map.flow = acc;
    ctx.report.river_threshold = thr;
    ctx.report.notes.push(format!("{lakes} lakes, {river_tiles} river tiles"));
}

/// Slope, and distance to fresh water.
pub fn terrain(ctx: &mut GenCtx) {
    let n = ctx.n;
    let map = &ctx.map;
    let surface = |i: usize| if map.water[i].is_water() { map.water_level[i] } else { map.elevation[i] };
    for i in 0..n * n {
        let s = surface(i);
        let mut m = 0.0f32;
        for_n4(n, i, |j| m = m.max((s - surface(j)).abs()));
        ctx.slope[i] = m;
    }
    let fresh: Vec<usize> = (0..n * n).filter(|&i| map.water[i].is_fresh()).collect();
    ctx.dist_fresh = bfs(n, &fresh);
}

/// Biomes from water, height, slope, temperature and rain.
pub fn biomes(ctx: &mut GenCtx) {
    let (n, p, c) = (ctx.n, ctx.p, ctx.content);
    let [sea, lake, river, beach, marsh, meadow, forest, pine, hills, mountain, snow] =
        ["sea", "lake", "river", "beach", "marsh", "meadow", "forest", "pine_forest", "hills", "mountain", "snow"].map(|id| c.biome(id));
    let mut r = ctx.rng("gen.biomes");
    let (s1, s2) = (r.next_u64(), r.next_u64());
    for i in 0..n * n {
        let (fx, fy) = ((i % n) as f32, (i / n) as f32);
        let m = &ctx.map;
        let (e, t, rain, slope) = (m.elevation[i], m.temperature[i], m.rain[i], ctx.slope[i]);
        let b = match m.water[i] {
            Water::Sea => sea,
            Water::Lake => lake,
            Water::River => river,
            Water::None => {
                if t < -1.0 {
                    snow
                } else if e > p.mountain_elevation || slope > 2.2 {
                    mountain
                } else if ctx.dist_sea[i] <= 2 && e < 1.6 {
                    beach
                } else if e > 11.0 || slope > 1.1 {
                    hills
                } else if ctx.dist_fresh[i] <= 3 && e < 3.0 && slope < 0.35 && rain + fbm(s2, fx / 30.0, fy / 30.0, 2) * 0.25 > 0.74 {
                    marsh
                } else {
                    let f = fbm(s1, fx / 38.0, fy / 38.0, 3) + (rain - 0.55) * 0.8;
                    if f > 0.08 {
                        if t < 6.5 || e > 7.0 { pine } else { forest }
                    } else {
                        meadow
                    }
                }
            }
        };
        ctx.map.biome[i] = b;
    }
}

/// Soil moisture and fertility: from rain, closeness to fresh water (river plains are rich) and the
/// biome. These are the quantities farming and its expansions work with.
pub fn soil(ctx: &mut GenCtx) {
    let (n, c) = (ctx.n, ctx.content);
    let marsh = c.biome("marsh");
    let s = ctx.rng("gen.soil").next_u64();
    for i in 0..n * n {
        let m = &mut ctx.map;
        if m.water[i].is_water() {
            m.moisture[i] = 1.0;
            m.fertility[i] = 0.0;
            continue;
        }
        let (fx, fy) = ((i % n) as f32, (i / n) as f32);
        let df = ctx.dist_fresh[i] as f32;
        let base = c.biomes[m.biome[i] as usize].fertility;
        let wet = if m.biome[i] == marsh { 0.2 } else { 0.0 };
        m.moisture[i] = (m.rain[i] * 0.65 + 0.35 / (1.0 + df / 5.0) + wet).clamp(0.0, 1.0);
        let alluvial = 0.25 / (1.0 + df / 6.0) * (base / 0.6).min(1.0);
        m.fertility[i] = (base + alluvial + fbm(s, fx / 24.0, fy / 24.0, 2) * 0.1).clamp(0.0, 1.0);
    }
}

/// Resource nodes (trees, rocks, bushes...) from each biome's densities in data/base/nodes.ron,
/// in groves and gaps rather than evenly.
pub fn vegetation(ctx: &mut GenCtx) {
    let (n, c) = (ctx.n, ctx.content);
    let nb = c.biomes.len();
    let dens: Vec<Vec<f32>> = c
        .nodes
        .iter()
        .map(|d| {
            let mut v = vec![0.0; nb];
            for (b, p) in &d.densities {
                v[c.biome(b) as usize] = *p;
            }
            v
        })
        .collect();
    let mut r = ctx.rng("gen.vegetation");
    let seeds: Vec<u64> = (0..c.nodes.len()).map(|_| r.next_u64()).collect();
    for i in 0..n * n {
        if ctx.map.water[i].is_water() {
            continue;
        }
        let (x, y) = (i % n, i / n);
        let b = ctx.map.biome[i] as usize;
        for (k, def) in c.nodes.iter().enumerate() {
            let mut pr = dens[k][b];
            if def.near_water > 0.0 && ctx.dist_fresh[i] <= 2 {
                pr += def.near_water;
            }
            if pr <= 0.0 {
                continue;
            }
            let clump = (1.0f32 + def.clump * fbm(seeds[k], x as f32 / 13.0, y as f32 / 13.0, 2) * 2.0).max(0.0);
            if r.next_f32() < (pr * clump).min(0.9) {
                ctx.nodes.push(Some(Node {
                    kind: k as u16,
                    x: x as u16,
                    y: y as u16,
                    variant: r.below(def.variants.max(1) as u32) as u8,
                    turn: r.below(4) as u8,
                    amount: def.amount,
                }));
                ctx.map.node_at[i] = ctx.nodes.len() as u32;
                break;
            }
        }
    }
}

/// Where the first group arrives: flat open ground near fresh water, with berries and trees in
/// reach and good soil, away from the map edge. Targets relax if a seed can't meet them.
pub fn start_site(ctx: &mut GenCtx) {
    let (n, c) = (ctx.n, ctx.content);
    let margin = (ctx.p.start_margin as usize).min(n / 4);
    let berry = c.node_kind("berry_bush") as u32 + 1;
    let (tree, pine) = (c.node_kind("tree") as u32 + 1, c.node_kind("pine") as u32 + 1);
    let kind_at = |i: usize| -> u32 {
        let a = ctx.map.node_at[i];
        if a == 0 { 0 } else { ctx.nodes[a as usize - 1].as_ref().map_or(0, |nd| nd.kind as u32 + 1) }
    };
    let berries = sat(n, |i| (kind_at(i) == berry) as u32);
    let trees = sat(n, |i| (kind_at(i) == tree || kind_at(i) == pine) as u32);
    let ok_biomes = ["meadow", "forest", "pine_forest", "hills"].map(|b| c.biome(b));
    let centre = n as f32 / 2.0;
    // (max steps to fresh water, min berry bushes, min trees) within 40 tiles
    let tiers = [(35u32, 6u32, 12u32), (60, 3, 6), (u32::MAX, 0, 0)];
    let mut chosen = None;
    for (tier, &(max_water, min_berries, min_trees)) in tiers.iter().enumerate() {
        let mut best: Option<(f32, usize)> = None;
        for y in (margin..n - margin).step_by(2) {
            for x in (margin..n - margin).step_by(2) {
                let i = y * n + x;
                let m = &ctx.map;
                if m.water[i].is_water() || !ok_biomes.contains(&m.biome[i]) {
                    continue;
                }
                let df = ctx.dist_fresh[i];
                if df < 3 || df > max_water {
                    continue;
                }
                let b = sat_square(&berries, n, x, y, 40);
                let t = sat_square(&trees, n, x, y, 40);
                if b < min_berries || t < min_trees {
                    continue;
                }
                let (mut lo, mut hi, mut wet) = (f32::MAX, f32::MIN, false);
                for yy in y - 3..=y + 3 {
                    for xx in x - 3..=x + 3 {
                        let j = yy * n + xx;
                        lo = lo.min(m.elevation[j]);
                        hi = hi.max(m.elevation[j]);
                        wet |= m.water[j].is_water();
                    }
                }
                if wet || hi - lo > 2.0 {
                    continue;
                }
                let dc = ((x as f32 - centre).powi(2) + (y as f32 - centre).powi(2)).sqrt();
                let score = b.min(25) as f32 + 0.25 * t.min(60) as f32 + 8.0 * m.fertility[i]
                    - 0.3 * df.min(100) as f32
                    - 4.0 * (hi - lo)
                    - 0.04 * dc;
                if best.is_none_or(|(s, _)| score > s) {
                    best = Some((score, i));
                }
            }
        }
        if let Some((_, i)) = best {
            chosen = Some((i, tier as u8 + 1));
            break;
        }
    }
    let (i, tier) = chosen.unwrap_or_else(|| {
        ctx.report.notes.push("no start site found; using the centre".into());
        ((n / 2) * n + n / 2, 3)
    });
    let (sx, sy) = (i % n, i / n);
    ctx.map.start = (sx as u32, sy as u32);
    ctx.report.start_tier = tier;
    // Clear room to settle.
    for y in sy.saturating_sub(4)..(sy + 5).min(n) {
        for x in sx.saturating_sub(4)..(sx + 5).min(n) {
            let j = y * n + x;
            let a = ctx.map.node_at[j];
            if a > 0 {
                ctx.nodes[a as usize - 1] = None;
                ctx.map.node_at[j] = 0;
            }
        }
    }
}

/// Terrace levels for building and drawing.
pub fn finish(ctx: &mut GenCtx) {
    let m = &mut ctx.map;
    for i in 0..m.level.len() {
        let s = if m.water[i].is_water() { m.water_level[i] } else { m.elevation[i] };
        m.level[i] = s.max(0.0).floor().min(255.0) as u8;
    }
    m.version = 1;
}
