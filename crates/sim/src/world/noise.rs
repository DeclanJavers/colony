//! Gradient noise for world generation. Gradients come from a fixed set of 8 directions picked by
//! hash, and only basic float operations are used, so the same seed gives the same world.

use crate::hash::hash2;

const G: f32 = std::f32::consts::FRAC_1_SQRT_2;
const GRADS: [(f32, f32); 8] = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (G, G), (-G, G), (G, -G), (-G, -G)];

#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Perlin-style gradient noise, roughly in -1..1, with features about 1 unit apart.
pub fn gradient(seed: u64, x: f32, y: f32) -> f32 {
    let x0 = x.floor();
    let y0 = y.floor();
    let (ix, iy) = (x0 as i32, y0 as i32);
    let (fx, fy) = (x - x0, y - y0);
    let g = |i: i32, j: i32, dx: f32, dy: f32| {
        let (gx, gy) = GRADS[(hash2(seed, i, j) & 7) as usize];
        gx * dx + gy * dy
    };
    let n00 = g(ix, iy, fx, fy);
    let n10 = g(ix + 1, iy, fx - 1.0, fy);
    let n01 = g(ix, iy + 1, fx, fy - 1.0);
    let n11 = g(ix + 1, iy + 1, fx - 1.0, fy - 1.0);
    let (u, v) = (fade(fx), fade(fy));
    (lerp(lerp(n00, n10, u), lerp(n01, n11, u), v) * 1.4).clamp(-1.0, 1.0)
}

/// Fractal noise: `octaves` layers, each twice the detail at half the strength. Roughly -1..1.
pub fn fbm(seed: u64, x: f32, y: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut total) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += gradient(seed.wrapping_add(o as u64 * 0x9E37), x * freq, y * freq) * amp;
        total += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / total
}

/// Ridged noise: sharp crests where the noise crosses zero, for mountain ranges. 0..1.
pub fn ridged(seed: u64, x: f32, y: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut total) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        let r = 1.0 - gradient(seed.wrapping_add(o as u64 * 0x7F4A), x * freq, y * freq).abs();
        sum += r * r * amp;
        total += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / total
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
