//! Map images for judging a seed without starting the game: one PNG per layer, an overview, and a
//! contact sheet of many seeds.

use sim::Sim;
use sim::content::hex_colour;
use sim::world::Water;
use std::path::Path;

pub struct Img {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Img {
    fn new(w: u32, h: u32, bg: [u8; 3]) -> Img {
        let mut px = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..w * h {
            px.extend_from_slice(&bg);
        }
        Img { w, h, px }
    }

    fn set(&mut self, x: u32, y: u32, c: [u8; 3]) {
        if x < self.w && y < self.h {
            let i = ((y * self.w + x) * 3) as usize;
            self.px[i..i + 3].copy_from_slice(&c);
        }
    }

    fn get(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.w + x) * 3) as usize;
        [self.px[i], self.px[i + 1], self.px[i + 2]]
    }
}

pub fn write_png(path: &Path, img: &Img) {
    let file = std::fs::File::create(path).unwrap_or_else(|e| panic!("can't write {}: {e}", path.display()));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), img.w, img.h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("png header");
    w.write_image_data(&img.px).expect("png data");
}

fn lerp(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|k| (a[k] as f32 + (b[k] as f32 - a[k] as f32) * t).round() as u8)
}

fn scale(c: [u8; 3], s: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * s).clamp(0.0, 255.0) as u8)
}

/// Piecewise colour ramp over (value, colour) stops.
fn ramp(stops: &[(f32, &str)], v: f32) -> [u8; 3] {
    if v <= stops[0].0 {
        return hex_colour(stops[0].1);
    }
    for w in stops.windows(2) {
        if v <= w[1].0 {
            return lerp(hex_colour(w[0].1), hex_colour(w[1].1), (v - w[0].0) / (w[1].0 - w[0].0));
        }
    }
    hex_colour(stops[stops.len() - 1].1)
}

fn surface(sim: &Sim, i: usize) -> f32 {
    let m = &sim.map;
    if m.water[i].is_water() { m.water_level[i].max(0.0) } else { m.elevation[i] }
}

/// Light from the top left: slopes facing it brighter, the others darker.
fn hillshade(sim: &Sim, x: u32, y: u32) -> f32 {
    let m = &sim.map;
    let n = m.size;
    let s = |x: u32, y: u32| surface(sim, m.idx(x.min(n - 1), y.min(n - 1)));
    let dx = s(x + 1, y) - s(x.saturating_sub(1), y);
    let dy = s(x, y + 1) - s(x, y.saturating_sub(1));
    (1.0 + (dx + dy) * 0.12).clamp(0.6, 1.35)
}

fn water_colour(sim: &Sim, i: usize) -> Option<[u8; 3]> {
    let m = &sim.map;
    match m.water[i] {
        Water::None => None,
        Water::Sea => Some(lerp(hex_colour("#3C7FC0"), hex_colour("#244F86"), -m.elevation[i] / 8.0)),
        Water::Lake => Some(hex_colour("#4A8ACB")),
        Water::River => Some(hex_colour("#5B9AD6")),
    }
}

/// Layer by layer, each tile one pixel.
fn field(sim: &Sim, f: impl Fn(usize) -> [u8; 3]) -> Img {
    let n = sim.map.size;
    let mut img = Img::new(n, n, [0, 0, 0]);
    for y in 0..n {
        for x in 0..n {
            img.set(x, y, f(sim.map.idx(x, y)));
        }
    }
    img
}

pub fn layers(sim: &Sim) -> Vec<(&'static str, Img)> {
    let m = &sim.map;
    let n = m.size;
    let shade = |i: usize| hillshade(sim, i as u32 % n, i as u32 / n);
    let maxflow = m.flow.iter().cloned().fold(1.0f32, f32::max).ln();
    vec![
        ("overview", overview(sim, 2)),
        (
            "elevation",
            field(sim, |i| {
                water_colour(sim, i).unwrap_or_else(|| {
                    scale(ramp(&[(0.0, "#5C9439"), (8.0, "#86913D"), (16.0, "#8A5A35"), (28.0, "#9A9590"), (42.0, "#F0F4FA")], m.elevation[i]), shade(i))
                })
            }),
        ),
        ("biomes", field(sim, |i| hex_colour(&sim.content.biomes[m.biome[i] as usize].colour))),
        ("temperature", field(sim, |i| ramp(&[(-5.0, "#3B5BA8"), (5.0, "#BFD3EE"), (10.0, "#F3EADB"), (20.0, "#C8402C")], m.temperature[i]))),
        ("rain", field(sim, |i| ramp(&[(0.0, "#E8DDC0"), (1.0, "#2B5BA8")], m.rain[i]))),
        ("moisture", field(sim, |i| ramp(&[(0.0, "#E8DDC0"), (1.0, "#2B5BA8")], m.moisture[i]))),
        (
            "fertility",
            field(sim, |i| if m.water[i].is_water() { hex_colour("#1E2A3A") } else { ramp(&[(0.0, "#8A5A35"), (0.5, "#B9A24A"), (1.0, "#3F8A35")], m.fertility[i]) }),
        ),
        (
            "flow",
            field(sim, |i| {
                if m.water[i] == Water::Sea {
                    return hex_colour("#1E2A3A");
                }
                let v = (m.flow[i].max(1.0).ln() / maxflow).clamp(0.0, 1.0);
                if m.water[i].is_fresh() { hex_colour("#7FB8E6") } else { lerp([20, 20, 20], [230, 230, 230], v) }
            }),
        ),
    ]
}

/// Biomes with hillshade, water, nodes and the start site. `s` pixels per tile.
pub fn overview(sim: &Sim, s: u32) -> Img {
    let m = &sim.map;
    let n = m.size;
    let mut img = Img::new(n * s, n * s, [0, 0, 0]);
    for y in 0..n {
        for x in 0..n {
            let i = m.idx(x, y);
            let c = water_colour(sim, i)
                .unwrap_or_else(|| scale(hex_colour(&sim.content.biomes[m.biome[i] as usize].colour), hillshade(sim, x, y)));
            for dy in 0..s {
                for dx in 0..s {
                    img.set(x * s + dx, y * s + dy, c);
                }
            }
        }
    }
    for (_, nd) in sim.nodes.iter() {
        let def = &sim.content.nodes[nd.kind as usize];
        if s == 1 && !def.blocks && def.id != "wildflowers" {
            continue;
        }
        let c = hex_colour(&def.colour);
        let (x, y) = (nd.x as u32 * s, nd.y as u32 * s);
        if s == 1 {
            img.set(x, y, c);
        } else {
            for dy in 0..s - 1 {
                for dx in 0..s - 1 {
                    img.set(x + dx, y + dy, c);
                }
            }
        }
    }
    // Start site: a gold ring.
    let (sx, sy) = m.start;
    let r = 6i32;
    for d in -r..=r {
        for (px, py) in [(d, -r), (d, r), (-r, d), (r, d)] {
            let (x, y) = (sx as i32 + px, sy as i32 + py);
            if x >= 0 && y >= 0 {
                for dy in 0..s {
                    for dx in 0..s {
                        img.set(x as u32 * s + dx, y as u32 * s + dy, hex_colour("#E8C468"));
                    }
                }
            }
        }
    }
    img
}

/// 3x5 digits for labelling the contact sheet.
const DIGITS: [u16; 10] = [0x7B6F, 0x2C97, 0x73E7, 0x73CF, 0x5BC9, 0x79CF, 0x79EF, 0x7249, 0x7BEF, 0x7BCF];

fn draw_number(img: &mut Img, x0: u32, y0: u32, n: u64, scale_px: u32) {
    let text = n.to_string();
    let w = (text.len() as u32 * 4 + 1) * scale_px;
    for y in 0..7 * scale_px {
        for x in 0..w {
            img.set(x0 + x, y0 + y, [30, 26, 31]);
        }
    }
    for (k, ch) in text.bytes().enumerate() {
        let bits = DIGITS[(ch - b'0') as usize];
        for row in 0..5u32 {
            for col in 0..3u32 {
                if bits & (1 << (14 - (row * 3 + col))) != 0 {
                    for dy in 0..scale_px {
                        for dx in 0..scale_px {
                            img.set(x0 + (1 + k as u32 * 4 + col) * scale_px + dx, y0 + (1 + row) * scale_px + dy, [243, 234, 219]);
                        }
                    }
                }
            }
        }
    }
}

pub fn contact_sheet(tiles: &[(u64, Img)], cols: u32) -> Img {
    let (tw, th) = (tiles[0].1.w, tiles[0].1.h);
    let gap = 8;
    let rows = (tiles.len() as u32).div_ceil(cols);
    let mut sheet = Img::new(cols * (tw + gap) + gap, rows * (th + gap) + gap, [30, 26, 31]);
    for (k, (seed, img)) in tiles.iter().enumerate() {
        let (cx, cy) = (gap + (k as u32 % cols) * (tw + gap), gap + (k as u32 / cols) * (th + gap));
        for y in 0..th {
            for x in 0..tw {
                sheet.set(cx + x, cy + y, img.get(x, y));
            }
        }
        draw_number(&mut sheet, cx + 4, cy + 4, *seed, 3);
    }
    sheet
}
