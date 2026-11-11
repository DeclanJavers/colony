//! Terrain and water meshes from the map. The ground is a height grid per 64x64-tile chunk (corner
//! heights averaged from the tiles around them), with the diorama's soil sides along the map
//! edge. Water is a separate surface mesh (see water_mesh).

#![allow(clippy::needless_range_loop)]

use bytemuck::{Pod, Zeroable};
use sim::world::Map;

pub const CHUNK: u32 = 64;
/// How deep the diorama's sides go, in metres below sea level.
pub const BOTTOM: f32 = -7.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct TerrainVertex {
    pub pos: [f32; 3],
    /// Smooth normal (the ground is shaded softly; models stay faceted).
    pub normal: [f32; 3],
    /// 0 ground, 1 soil side, 2 water side
    pub side: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct WaterVertex {
    pub pos: [f32; 3],
    pub depth: f32,
}

pub struct MeshData<V> {
    pub vertices: Vec<V>,
    pub indices: Vec<u32>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl<V> MeshData<V> {
    fn new() -> Self {
        MeshData { vertices: Vec::new(), indices: Vec::new(), min: [f32::MAX; 3], max: [f32::MIN; 3] }
    }
    fn grow(&mut self, p: [f32; 3]) {
        for k in 0..3 {
            self.min[k] = self.min[k].min(p[k]);
            self.max[k] = self.max[k].max(p[k]);
        }
    }
}

/// Height of the ground (the bed, under water) at each tile corner: (size+1)^2 values. Each
/// corner averages the tiles around it, but land next to water counts as at most a little above
/// that water, so banks always slope down into it (otherwise a river mouth between higher
/// beaches would be closed off by a sandbar of averaged corners).
pub fn corner_heights(map: &Map) -> Vec<f32> {
    let n = map.size as usize;
    let w = n + 1;
    let mut h = vec![0.0; w * w];
    for cy in 0..w {
        for cx in 0..w {
            let tiles: Vec<usize> = [(cx as i32 - 1, cy as i32 - 1), (cx as i32, cy as i32 - 1), (cx as i32 - 1, cy as i32), (cx as i32, cy as i32)]
                .into_iter()
                .filter(|&(tx, ty)| tx >= 0 && ty >= 0 && (tx as usize) < n && (ty as usize) < n)
                .map(|(tx, ty)| ty as usize * n + tx as usize)
                .collect();
            let water_top = tiles.iter().filter(|&&i| map.water[i].is_water()).map(|&i| map.water_level[i].max(0.0)).fold(f32::NEG_INFINITY, f32::max);
            let sum: f32 = tiles
                .iter()
                .map(|&i| if map.water[i].is_water() || water_top == f32::NEG_INFINITY { map.elevation[i] } else { map.elevation[i].min(water_top + 0.05) })
                .sum();
            h[cy * w + cx] = sum / tiles.len() as f32;
        }
    }
    h
}

fn quad_indices(base: u32, flip: bool) -> [u32; 6] {
    // corners: 0 (x0,z0), 1 (x1,z0), 2 (x0,z1), 3 (x1,z1)
    if flip { [base, base + 2, base + 1, base + 1, base + 2, base + 3] } else { [base, base + 2, base + 3, base, base + 3, base + 1] }
}

/// Ground vertices per tile along each side (the mesh follows the smooth surface between corners).
pub const SUB: u32 = 2;

/// The ground mesh for one chunk, plus its share of the diorama sides.
pub fn chunk_mesh(map: &Map, ground: &Ground, cx: u32, cz: u32) -> MeshData<TerrainVertex> {
    let n = map.size;
    let (x0, z0) = (cx * CHUNK, cz * CHUNK);
    let (x1, z1) = ((x0 + CHUNK).min(n), (z0 + CHUNK).min(n));
    let step = 1.0 / SUB as f32;
    let (cols, rows) = ((x1 - x0) * SUB + 1, (z1 - z0) * SUB + 1);
    let mut m = MeshData::new();
    for zi in 0..rows {
        for xi in 0..cols {
            let (x, z) = (x0 as f32 + xi as f32 * step, z0 as f32 + zi as f32 * step);
            let p = [x, ground.height(x, z), z];
            m.grow(p);
            m.vertices.push(TerrainVertex { pos: p, normal: ground.normal(x, z), side: 0.0 });
        }
    }
    for zi in 0..rows - 1 {
        for xi in 0..cols - 1 {
            let a = zi * cols + xi;
            let (b, c, d) = (a + 1, a + cols, a + cols + 1);
            let h = |i: u32| m.vertices[i as usize].pos[1];
            // Split along the flatter diagonal.
            if (h(a) - h(d)).abs() <= (h(b) - h(c)).abs() {
                m.indices.extend_from_slice(&[a, c, d, a, d, b]);
            } else {
                m.indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
    }
    // Diorama sides along the map edge, following the same surface.
    let side = |m: &mut MeshData<TerrainVertex>, a: (f32, f32), b: (f32, f32), out: [f32; 3]| {
        let mid = ((a.0 + b.0) * 0.5 - out[0] * 0.25, (a.1 + b.1) * 0.5 - out[2] * 0.25);
        let tile = map.idx((mid.0 as u32).min(n - 1), (mid.1 as u32).min(n - 1));
        let top = (ground.height(a.0, a.1), ground.height(b.0, b.1));
        let wall = |m: &mut MeshData<TerrainVertex>, hi: (f32, f32), lo: (f32, f32), kind: f32| {
            let base = m.vertices.len() as u32;
            for p in [[a.0, hi.0, a.1], [b.0, hi.1, b.1], [a.0, lo.0, a.1], [b.0, lo.1, b.1]] {
                m.grow(p);
                m.vertices.push(TerrainVertex { pos: p, normal: out, side: kind });
            }
            m.indices.extend_from_slice(&quad_indices(base, false));
        };
        if map.water[tile].is_water() {
            let s = map.water_level[tile].max(0.0);
            wall(m, (s, s), top, 2.0);
        }
        wall(m, top, (BOTTOM, BOTTOM), 1.0);
    };
    let nf = n as f32;
    for k in 0..(x1 - x0) * SUB {
        let (xa, xb) = (x0 as f32 + k as f32 * step, x0 as f32 + (k + 1) as f32 * step);
        if z0 == 0 {
            side(&mut m, (xa, 0.0), (xb, 0.0), [0.0, 0.0, -1.0]);
        }
        if z1 == n {
            side(&mut m, (xa, nf), (xb, nf), [0.0, 0.0, 1.0]);
        }
    }
    for k in 0..(z1 - z0) * SUB {
        let (za, zb) = (z0 as f32 + k as f32 * step, z0 as f32 + (k + 1) as f32 * step);
        if x0 == 0 {
            side(&mut m, (0.0, za), (0.0, zb), [-1.0, 0.0, 0.0]);
        }
        if x1 == n {
            side(&mut m, (nf, za), (nf, zb), [1.0, 0.0, 0.0]);
        }
    }
    m
}

/// The water surface. Corners take the average surface of the water tiles around them, so rivers
/// slope smoothly instead of stepping (steps drew outlines across the water). The surface also
/// runs one tile in under the banks: the sloping ground hides it except where it dips below, so
/// the waterline follows the ground instead of zigzagging along tile edges.
pub fn water_mesh(map: &Map, ground: &[f32]) -> MeshData<WaterVertex> {
    let mut m = MeshData::new();
    let n = map.size as usize;
    let w = n + 1;
    let wet = |tx: i64, tz: i64| -> Option<usize> {
        (tx >= 0 && tz >= 0 && (tx as usize) < n && (tz as usize) < n)
            .then(|| tz as usize * n + tx as usize)
            .filter(|&i| map.water[i].is_water())
    };
    let mut corner = vec![f32::NAN; w * w];
    for cz in 0..w as i64 {
        for cx in 0..w as i64 {
            for r in [1i64, 2, 3] {
                let (mut s, mut k) = (0.0, 0);
                for tz in cz - r..cz + r {
                    for tx in cx - r..cx + r {
                        if let Some(i) = wet(tx, tz) {
                            s += map.water_level[i].max(0.0);
                            k += 1;
                        }
                    }
                }
                if k > 0 {
                    let c = cz as usize * w + cx as usize;
                    // Corners touching water take its level. Outer corners stay just under the
                    // ground, so water never shows beyond where there is water: the visible
                    // waterline is where the smooth ground meets the water inside the first tile.
                    corner[c] = if r == 1 { s / k as f32 } else { (s / k as f32).min(ground[c] - 0.05) };
                    break;
                }
            }
        }
    }
    for z in 0..n {
        for x in 0..n {
            let near_water = (-2..=2).any(|dz| (-2..=2).any(|dx| wet(x as i64 + dx, z as i64 + dz).is_some()));
            if !near_water {
                continue;
            }
            let cs = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dz)| (z + dz) * w + x + dx);
            if cs.iter().any(|&c| corner[c].is_nan()) {
                continue;
            }
            let base = m.vertices.len() as u32;
            for (k, &c) in cs.iter().enumerate() {
                let p = [(x + (k & 1)) as f32, corner[c], (z + (k >> 1)) as f32];
                m.grow(p);
                m.vertices.push(WaterVertex { pos: p, depth: (corner[c] - ground[c]).max(0.0) });
            }
            m.indices.extend_from_slice(&quad_indices(base, false));
        }
    }
    m
}

/// How the water moves, one texel per tile (RGBA8), built once at load and sampled with smooth
/// filtering so it blends across tile edges:
///   R, G: flow direction x, z (0..255 for -1..1), smoothed across neighbouring water
///   B:    speed 0..1: more water draining through, a steeper drop and a narrower channel are
///         faster; banks are slower; lakes barely move; the sea is still
///   A:    closeness to the shore (255 at the bank, 0 from 3 tiles out), for foam
pub fn flow_field(map: &Map) -> Vec<u8> {
    use sim::world::{DIRS, NO_FLOW, Water};
    let n = map.size as usize;
    let wet = |i: usize| map.water[i].is_water();
    let at = |x: i64, z: i64| (x >= 0 && z >= 0 && (x as usize) < n && (z as usize) < n).then(|| z as usize * n + x as usize);
    // Raw direction and speed per tile.
    let max_flow = map.flow.iter().cloned().fold(1.0f32, f32::max).ln().max(1.0);
    let mut dir = vec![[0.0f32; 2]; n * n];
    let mut speed = vec![0.0f32; n * n];
    for i in 0..n * n {
        if !wet(i) || map.water[i] == Water::Sea || map.flow_dir[i] == NO_FLOW {
            continue;
        }
        let (dx, dz) = DIRS[map.flow_dir[i] as usize];
        let len = ((dx * dx + dz * dz) as f32).sqrt();
        dir[i] = [dx as f32 / len, dz as f32 / len];
        let volume = (map.flow[i].max(1.0).ln() / max_flow).clamp(0.0, 1.0);
        speed[i] = match map.water[i] {
            Water::Lake => 0.08 + 0.1 * volume,
            _ => {
                // Drop to the next tile (rapids) and how much of the area around is water (narrow
                // channels run faster).
                let (x, z) = ((i % n) as i64, (i / n) as i64);
                let drop = at(x + dx as i64, z + dz as i64).map_or(0.0, |j| (map.water_level[i] - map.water_level[j]).max(0.0));
                let mut water = 0;
                for oz in -2..=2 {
                    for ox in -2..=2 {
                        water += at(x + ox, z + oz).is_some_and(wet) as u32;
                    }
                }
                let narrow = 1.0 - (water as f32 / 25.0);
                (0.25 + 0.45 * volume + 0.35 * narrow + (drop * 2.5).min(0.35)).clamp(0.0, 1.0)
            }
        };
    }
    // Smooth the field across neighbouring water so it curves instead of zigzagging.
    for _ in 0..3 {
        let (d0, s0) = (dir.clone(), speed.clone());
        for i in 0..n * n {
            if !wet(i) || map.water[i] == Water::Sea {
                continue;
            }
            let (x, z) = ((i % n) as i64, (i / n) as i64);
            let (mut ax, mut az, mut sp, mut k) = (0.0, 0.0, 0.0, 0.0);
            for oz in -1..=1 {
                for ox in -1..=1 {
                    if let Some(j) = at(x + ox, z + oz).filter(|&j| wet(j) && map.water[j] != Water::Sea) {
                        let w = if ox == 0 && oz == 0 { 2.0 } else { 1.0 };
                        ax += d0[j][0] * w;
                        az += d0[j][1] * w;
                        sp += s0[j] * w;
                        k += w;
                    }
                }
            }
            let len = (ax * ax + az * az).sqrt();
            if len > 1e-4 {
                dir[i] = [ax / len, az / len];
            }
            speed[i] = sp / k;
        }
    }
    // River plumes: where a river runs into the sea (or a lake), its current carries on out,
    // spreading and slowing tile by tile instead of stopping dead at the mouth.
    for _ in 0..10 {
        let (d0, s0) = (dir.clone(), speed.clone());
        for i in 0..n * n {
            if map.water[i] != Water::Sea && map.water[i] != Water::Lake {
                continue;
            }
            let (x, z) = ((i % n) as i64, (i / n) as i64);
            let (mut ax, mut az, mut best) = (0.0, 0.0, 0.0f32);
            for oz in -1..=1 {
                for ox in -1..=1 {
                    if let Some(j) = at(x + ox, z + oz).filter(|&j| wet(j)) {
                        ax += d0[j][0] * s0[j];
                        az += d0[j][1] * s0[j];
                        best = best.max(s0[j]);
                    }
                }
            }
            let carried = best * 0.78;
            if carried > speed[i] {
                let len = (ax * ax + az * az).sqrt();
                if len > 1e-4 {
                    dir[i] = [ax / len, az / len];
                }
                speed[i] = carried;
            }
        }
    }
    // Distance to the shore (in tiles, up to 3), through the water.
    let mut shore = vec![3u8; n * n];
    for i in 0..n * n {
        if !wet(i) {
            continue;
        }
        let (x, z) = ((i % n) as i64, (i / n) as i64);
        let mut best = 3;
        for oz in -3i64..=3 {
            for ox in -3i64..=3 {
                if let Some(j) = at(x + ox, z + oz)
                    && !wet(j)
                {
                    best = best.min(ox.abs().max(oz.abs()) as u8);
                }
            }
        }
        shore[i] = best;
    }
    let mut out = Vec::with_capacity(n * n * 4);
    for i in 0..n * n {
        // Water is slower near the banks.
        let bank = 0.35 + 0.65 * ((shore[i] as f32 - 0.5) / 2.0).clamp(0.0, 1.0);
        let v = if wet(i) { speed[i] * bank } else { 0.0 };
        let enc = |c: f32| ((c * 0.5 + 0.5) * 255.0).round() as u8;
        let near = if wet(i) { 255 - (shore[i] as u32 * 85).min(255) as u8 } else { 255 };
        out.extend_from_slice(&[enc(dir[i][0]), enc(dir[i][1]), (v * 255.0) as u8, near]);
    }
    out
}

/// The smooth ground surface: Catmull-Rom interpolation through the corner heights, the same
/// surface the terrain mesh is built from. Everything that stands on the ground (people, trees,
/// the camp) uses it, so nothing floats or sinks.
#[derive(Clone)]
pub struct Ground {
    n: usize,
    corners: Vec<f32>,
}

fn catmull(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t2 * t)
}

impl Ground {
    pub fn new(map: &Map) -> Ground {
        Ground { n: map.size as usize, corners: corner_heights(map) }
    }

    fn at(&self, x: i64, z: i64) -> f32 {
        let w = self.n as i64 + 1;
        self.corners[(z.clamp(0, w - 1) * w + x.clamp(0, w - 1)) as usize]
    }

    /// Height at world point (x, z).
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let (x0, z0) = (x.floor() as i64, z.floor() as i64);
        let (fx, fz) = (x - x0 as f32, z - z0 as f32);
        let row = |zz: i64| catmull(self.at(x0 - 1, zz), self.at(x0, zz), self.at(x0 + 1, zz), self.at(x0 + 2, zz), fx);
        catmull(row(z0 - 1), row(z0), row(z0 + 1), row(z0 + 2), fz)
    }

    pub fn normal(&self, x: f32, z: f32) -> [f32; 3] {
        let e = 0.25;
        let nx = self.height(x - e, z) - self.height(x + e, z);
        let nz = self.height(x, z - e) - self.height(x, z + e);
        let len = (nx * nx + (2.0 * e).powi(2) + nz * nz).sqrt();
        [nx / len, 2.0 * e / len, nz / len]
    }

    pub fn size(&self) -> usize {
        self.n
    }
}

/// Ground surface height at a world point (water surface over water), for the camera.
pub fn surface_at(map: &Map, x: f32, z: f32) -> f32 {
    let n = map.size as i32;
    let (tx, tz) = ((x.floor() as i32).clamp(0, n - 1), (z.floor() as i32).clamp(0, n - 1));
    let i = map.idx(tx as u32, tz as u32);
    if map.water[i].is_water() { map.water_level[i].max(0.0) } else { map.elevation[i] }
}
