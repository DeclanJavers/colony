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

/// Height of the ground (the bed, under water) at each tile corner: (size+1)^2 values.
pub fn corner_heights(map: &Map) -> Vec<f32> {
    let n = map.size as usize;
    let w = n + 1;
    let mut h = vec![0.0; w * w];
    for cy in 0..w {
        for cx in 0..w {
            let (mut sum, mut k) = (0.0, 0);
            for (tx, ty) in [(cx as i32 - 1, cy as i32 - 1), (cx as i32, cy as i32 - 1), (cx as i32 - 1, cy as i32), (cx as i32, cy as i32)] {
                if tx >= 0 && ty >= 0 && (tx as usize) < n && (ty as usize) < n {
                    sum += map.elevation[ty as usize * n + tx as usize];
                    k += 1;
                }
            }
            h[cy * w + cx] = sum / k as f32;
        }
    }
    h
}

fn quad_indices(base: u32, flip: bool) -> [u32; 6] {
    // corners: 0 (x0,z0), 1 (x1,z0), 2 (x0,z1), 3 (x1,z1)
    if flip { [base, base + 2, base + 1, base + 1, base + 2, base + 3] } else { [base, base + 2, base + 3, base, base + 3, base + 1] }
}

/// The ground mesh for one chunk, plus its share of the diorama sides.
pub fn chunk_mesh(map: &Map, corners: &[f32], cx: u32, cz: u32) -> MeshData<TerrainVertex> {
    let n = map.size;
    let w = (n + 1) as usize;
    let (x0, z0) = (cx * CHUNK, cz * CHUNK);
    let (x1, z1) = ((x0 + CHUNK).min(n), (z0 + CHUNK).min(n));
    let mut m = MeshData::new();
    let cw = (x1 - x0 + 1) as usize;
    let h = |x: i64, z: i64| corners[z.clamp(0, n as i64) as usize * w + x.clamp(0, n as i64) as usize];
    for z in z0..=z1 {
        for x in x0..=x1 {
            let (xi, zi) = (x as i64, z as i64);
            let p = [x as f32, h(xi, zi), z as f32];
            let nx = h(xi - 1, zi) - h(xi + 1, zi);
            let nz = h(xi, zi - 1) - h(xi, zi + 1);
            let len = (nx * nx + 4.0 + nz * nz).sqrt();
            m.grow(p);
            m.vertices.push(TerrainVertex { pos: p, normal: [nx / len, 2.0 / len, nz / len], side: 0.0 });
        }
    }
    for z in z0..z1 {
        for x in x0..x1 {
            let lx = (x - x0) as usize;
            let lz = (z - z0) as usize;
            let a = (lz * cw + lx) as u32;
            let (b, c, d) = (a + 1, a + cw as u32, a + cw as u32 + 1);
            let h = |i: u32| m.vertices[i as usize].pos[1];
            // Split along the flatter diagonal.
            if (h(a) - h(d)).abs() <= (h(b) - h(c)).abs() {
                m.indices.extend_from_slice(&[a, c, d, a, d, b]);
            } else {
                m.indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
    }
    // Diorama sides along the map edge.
    let side = |m: &mut MeshData<TerrainVertex>, ax: f32, az: f32, bx: f32, bz: f32, ha: f32, hb: f32, tile: usize, out: [f32; 3]| {
        let water = map.water[tile].is_water();
        let ground = (ha, hb);
        let wall = |m: &mut MeshData<TerrainVertex>, top: (f32, f32), bottom: (f32, f32), kind: f32| {
            let base = m.vertices.len() as u32;
            for p in [[ax, top.0, az], [bx, top.1, bz], [ax, bottom.0, az], [bx, bottom.1, bz]] {
                m.grow(p);
                m.vertices.push(TerrainVertex { pos: p, normal: out, side: kind });
            }
            m.indices.extend_from_slice(&quad_indices(base, false));
        };
        if water {
            let s = map.water_level[tile].max(0.0);
            wall(m, (s, s), ground, 2.0);
        }
        wall(m, ground, (BOTTOM, BOTTOM), 1.0);
    };
    let hc = |x: u32, z: u32| corners[z as usize * w + x as usize];
    if z0 == 0 {
        for x in x0..x1 {
            side(&mut m, x as f32, 0.0, x as f32 + 1.0, 0.0, hc(x, 0), hc(x + 1, 0), map.idx(x, 0), [0.0, 0.0, -1.0]);
        }
    }
    if z1 == n {
        for x in x0..x1 {
            side(&mut m, x as f32, n as f32, x as f32 + 1.0, n as f32, hc(x, n), hc(x + 1, n), map.idx(x, n - 1), [0.0, 0.0, 1.0]);
        }
    }
    if x0 == 0 {
        for z in z0..z1 {
            side(&mut m, 0.0, z as f32, 0.0, z as f32 + 1.0, hc(0, z), hc(0, z + 1), map.idx(0, z), [-1.0, 0.0, 0.0]);
        }
    }
    if x1 == n {
        for z in z0..z1 {
            side(&mut m, n as f32, z as f32, n as f32, z as f32 + 1.0, hc(n, z), hc(n, z + 1), map.idx(n - 1, z), [1.0, 0.0, 0.0]);
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
            for r in [1i64, 2] {
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
                    corner[cz as usize * w + cx as usize] = s / k as f32;
                    break;
                }
            }
        }
    }
    for z in 0..n {
        for x in 0..n {
            let near_water = (-1..=1).any(|dz| (-1..=1).any(|dx| wet(x as i64 + dx, z as i64 + dz).is_some()));
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

/// Ground surface height at a world point (water surface over water), for the camera.
pub fn surface_at(map: &Map, x: f32, z: f32) -> f32 {
    let n = map.size as i32;
    let (tx, tz) = ((x.floor() as i32).clamp(0, n - 1), (z.floor() as i32).clamp(0, n - 1));
    let i = map.idx(tx as u32, tz as u32);
    if map.water[i].is_water() { map.water_level[i].max(0.0) } else { map.elevation[i] }
}
