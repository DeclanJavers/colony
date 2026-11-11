//! Colony's UI kit (ARCHITECTURE.md section 12). Immediate mode: each frame the game describes
//! the UI (panels, text, bars, buttons, tooltips) and the kit turns it into triangles and a glyph
//! atlas that the renderer draws on top of everything, sharp, after the pixel pass.
//!
//! Sizes are in logical pixels; `scale` turns them into window pixels, so the UI scales with the
//! window and the display. Two fonts: Pixelify Sans for titles and numbers, Inter for dense text.

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

pub mod style;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct Vertex {
    /// Window pixels.
    pub pos: [f32; 2],
    /// Atlas texture coordinates (text).
    pub uv: [f32; 2],
    /// Linear RGBA (straight alpha).
    pub colour: [f32; 4],
    /// Position relative to the shape's centre, in window pixels (for rounded corners).
    pub local: [f32; 2],
    /// x: mode (0 filled shape, 1 text, 2 outline), y: corner radius, z, w: half size
    pub shape: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn contains(&self, p: (f32, f32)) -> bool {
        p.0 >= self.x && p.1 >= self.y && p.0 < self.x + self.w && p.1 < self.y + self.h
    }
    pub fn inset(&self, d: f32) -> Rect {
        Rect::new(self.x + d, self.y + d, (self.w - 2.0 * d).max(0.0), (self.h - 2.0 * d).max(0.0))
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Font {
    /// Pixelify Sans: titles, numbers, labels.
    Pixel,
    /// Inter: dense text.
    Sans,
}

#[derive(Clone, Copy, Debug)]
pub struct Text {
    pub font: Font,
    /// Logical pixels.
    pub size: f32,
    pub colour: [f32; 4],
}

/// What the player's mouse did this frame (window pixels).
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub cursor: (f32, f32),
    /// The button was released this frame without dragging (a click).
    pub clicked: bool,
    /// Scroll this frame (lines; positive = up).
    pub scroll: f32,
}

#[derive(Clone, Copy)]
struct Glyph {
    /// Atlas pixels.
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    xmin: f32,
    ymin: f32,
    advance: f32,
}

const ATLAS: u32 = 1024;

/// What the renderer draws this frame.
pub struct Output<'a> {
    pub vertices: &'a [Vertex],
    pub indices: &'a [u32],
    /// The glyph atlas (R8, ATLAS x ATLAS) if it changed since last frame.
    pub atlas: Option<&'a [u8]>,
    pub atlas_size: u32,
}

pub struct Ui {
    fonts: [fontdue::Font; 2],
    glyphs: HashMap<(Font, u32, char), Glyph>,
    atlas: Vec<u8>,
    atlas_dirty: bool,
    shelf: (u32, u32, u32),
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    /// Tooltips are drawn last, on top.
    tooltip: Option<(f32, f32, Vec<String>)>,
    blocking: Vec<Rect>,
    pub input: Input,
    pub scale: f32,
    pub size: (f32, f32),
}

fn lin(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// "#RRGGBB" (and alpha 0..1) to linear RGBA.
pub fn hex(s: &str, a: f32) -> [f32; 4] {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0xFF00FF);
    [lin(((v >> 16) & 255) as f32 / 255.0), lin(((v >> 8) & 255) as f32 / 255.0), lin((v & 255) as f32 / 255.0), a]
}

impl Ui {
    pub fn new(pixel_font: &[u8], sans_font: &[u8]) -> Result<Ui, String> {
        let load = |b: &[u8]| fontdue::Font::from_bytes(b, fontdue::FontSettings::default()).map_err(|e| e.to_string());
        Ok(Ui {
            fonts: [load(pixel_font)?, load(sans_font)?],
            glyphs: HashMap::new(),
            atlas: vec![0; (ATLAS * ATLAS) as usize],
            atlas_dirty: true,
            shelf: (1, 1, 0),
            vertices: Vec::new(),
            indices: Vec::new(),
            tooltip: None,
            blocking: Vec::new(),
            input: Input::default(),
            scale: 1.0,
            size: (1.0, 1.0),
        })
    }

    /// Start a frame: window size (window pixels), UI scale, and this frame's input.
    pub fn begin(&mut self, size: (f32, f32), scale: f32, input: Input) {
        self.size = size;
        self.scale = scale;
        self.input = input;
        self.vertices.clear();
        self.indices.clear();
        self.tooltip = None;
        self.blocking.clear();
    }

    /// Logical pixels to window pixels.
    pub fn px(&self, v: f32) -> f32 {
        (v * self.scale).round()
    }

    /// Is the cursor over any panel drawn this frame (so the world shouldn't react to it)?
    pub fn over_ui(&self) -> bool {
        self.blocking.iter().any(|r| r.contains(self.input.cursor))
    }

    pub fn hovered(&self, r: Rect) -> bool {
        r.contains(self.input.cursor)
    }

    pub fn clicked(&self, r: Rect) -> bool {
        self.input.clicked && self.hovered(r)
    }

    /// Mark an area as UI (clicks there don't reach the world).
    pub fn block(&mut self, r: Rect) {
        self.blocking.push(r);
    }

    // -----------------------------------------------------------------------------------------
    // Shapes

    fn quad(&mut self, r: Rect, colour: [f32; 4], mode: f32, radius: f32, uv: [f32; 4]) {
        let base = self.vertices.len() as u32;
        let (hw, hh) = (r.w * 0.5, r.h * 0.5);
        for (k, (cx, cy)) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].into_iter().enumerate() {
            let _ = k;
            self.vertices.push(Vertex {
                pos: [r.x + cx * r.w, r.y + cy * r.h],
                uv: [uv[0] + cx * (uv[2] - uv[0]), uv[1] + cy * (uv[3] - uv[1])],
                colour,
                local: [(cx - 0.5) * r.w, (cy - 0.5) * r.h],
                shape: [mode, radius, hw, hh],
            });
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
    }

    /// A filled rectangle with rounded corners (radius in window pixels).
    pub fn rect(&mut self, r: Rect, colour: [f32; 4], radius: f32) {
        self.quad(r, colour, 0.0, radius, [0.0; 4]);
    }

    /// A one-pixel-wide outline.
    pub fn outline(&mut self, r: Rect, colour: [f32; 4], radius: f32) {
        self.quad(r, colour, 2.0, radius, [0.0; 4]);
    }

    /// The standard panel: dark, translucent, rounded, with a hairline border. Blocks the world.
    pub fn panel(&mut self, r: Rect) {
        let rad = self.px(10.0);
        self.rect(r, style::PANEL, rad);
        self.outline(r, style::BORDER, rad);
        self.block(r);
    }

    /// A bar filled to `frac` (0..1).
    pub fn bar(&mut self, r: Rect, frac: f32, colour: [f32; 4]) {
        let rad = (r.h * 0.5).floor();
        self.rect(r, style::TRACK, rad);
        let w = (r.w * frac.clamp(0.0, 1.0)).round();
        if w >= 1.0 {
            self.rect(Rect::new(r.x, r.y, w.max(r.h.min(r.w)), r.h), colour, rad);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Text

    fn glyph(&mut self, font: Font, px: u32, ch: char) -> Glyph {
        if let Some(g) = self.glyphs.get(&(font, px, ch)) {
            return *g;
        }
        let f = &self.fonts[font as usize];
        let (m, bitmap) = f.rasterize(ch, px as f32);
        let (w, h) = (m.width as u32, m.height as u32);
        // Shelf packing; start over if the atlas fills up.
        if self.shelf.0 + w + 1 >= ATLAS {
            self.shelf = (1, self.shelf.1 + self.shelf.2 + 1, 0);
        }
        if self.shelf.1 + h + 1 >= ATLAS {
            self.glyphs.clear();
            self.atlas.fill(0);
            self.shelf = (1, 1, 0);
        }
        let (x, y) = (self.shelf.0, self.shelf.1);
        for row in 0..h {
            let src = (row * w) as usize;
            let dst = ((y + row) * ATLAS + x) as usize;
            self.atlas[dst..dst + w as usize].copy_from_slice(&bitmap[src..src + w as usize]);
        }
        self.shelf.0 += w + 1;
        self.shelf.2 = self.shelf.2.max(h);
        self.atlas_dirty = true;
        let g = Glyph { x, y, w, h, xmin: m.xmin as f32, ymin: m.ymin as f32, advance: m.advance_width };
        self.glyphs.insert((font, px, ch), g);
        g
    }

    fn font_px(&self, t: &Text) -> u32 {
        // Pixelify is drawn on its pixel grid (multiples of 8 window pixels), Inter at any size.
        match t.font {
            Font::Pixel => ((t.size * self.scale / 8.0).round().max(1.0) * 8.0) as u32,
            Font::Sans => (t.size * self.scale).round().max(6.0) as u32,
        }
    }

    /// Line height in window pixels.
    pub fn line_height(&self, t: Text) -> f32 {
        (self.font_px(&t) as f32 * 1.3).round()
    }

    /// Width of a string in window pixels.
    pub fn measure(&mut self, s: &str, t: Text) -> f32 {
        let px = self.font_px(&t);
        s.chars().map(|ch| self.glyph(t.font, px, ch).advance).sum::<f32>().round()
    }

    /// Draw a line of text with its top-left at (x, y). Returns its width.
    pub fn text(&mut self, x: f32, y: f32, s: &str, t: Text) -> f32 {
        let px = self.font_px(&t);
        let ascent = self.fonts[t.font as usize].horizontal_line_metrics(px as f32).map_or(px as f32 * 0.8, |m| m.ascent);
        let baseline = (y + ascent).round();
        let mut pen = x.round();
        for ch in s.chars() {
            let g = self.glyph(t.font, px, ch);
            if g.w > 0 && g.h > 0 {
                let gx = (pen + g.xmin).round();
                let gy = baseline - g.ymin - g.h as f32;
                let a = ATLAS as f32;
                let uv = [g.x as f32 / a, g.y as f32 / a, (g.x + g.w) as f32 / a, (g.y + g.h) as f32 / a];
                self.quad(Rect::new(gx, gy, g.w as f32, g.h as f32), t.colour, 1.0, 0.0, uv);
            }
            pen += g.advance;
        }
        (pen - x).round()
    }

    /// Text wrapped to `width`; returns the height used.
    pub fn wrapped(&mut self, x: f32, y: f32, width: f32, s: &str, t: Text) -> f32 {
        let lh = self.line_height(t);
        let mut line = String::new();
        let mut yy = y;
        for word in s.split_whitespace() {
            let test = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if self.measure(&test, t) > width && !line.is_empty() {
                self.text(x, yy, &line, t);
                yy += lh;
                line = word.to_string();
            } else {
                line = test;
            }
        }
        if !line.is_empty() {
            self.text(x, yy, &line, t);
            yy += lh;
        }
        yy - y
    }

    /// Height wrapped text would take, without drawing it.
    pub fn wrapped_height(&mut self, width: f32, s: &str, t: Text) -> f32 {
        let lh = self.line_height(t);
        let mut lines = 0;
        let mut line = String::new();
        for word in s.split_whitespace() {
            let test = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if self.measure(&test, t) > width && !line.is_empty() {
                lines += 1;
                line = word.to_string();
            } else {
                line = test;
            }
        }
        if !line.is_empty() {
            lines += 1;
        }
        lines as f32 * lh
    }

    // -----------------------------------------------------------------------------------------
    // Widgets

    /// A button; `on` draws it highlighted. Returns true when clicked.
    pub fn button(&mut self, r: Rect, label: &str, on: bool) -> bool {
        let hover = self.hovered(r);
        let rad = self.px(6.0);
        let (bg, fg) = if on {
            (if hover { style::GOLD_HOVER } else { style::GOLD }, style::ON_GOLD)
        } else {
            (if hover { style::CHIP_HOVER } else { style::CHIP }, style::TEXT)
        };
        self.rect(r, bg, rad);
        if !on {
            self.outline(r, style::BORDER, rad);
        }
        let t = Text { font: Font::Sans, size: 12.5, colour: fg };
        let w = self.measure(label, t);
        let lh = self.line_height(t);
        self.text(r.x + (r.w - w) * 0.5, r.y + (r.h - lh) * 0.5 + self.px(1.0), label, t);
        self.block(r);
        self.clicked(r)
    }

    /// Width a button needs for a label.
    pub fn button_width(&mut self, label: &str) -> f32 {
        let t = Text { font: Font::Sans, size: 12.5, colour: style::TEXT };
        self.measure(label, t) + self.px(22.0)
    }

    /// Show these lines near the cursor, on top of everything (the last call wins).
    pub fn tooltip(&mut self, lines: Vec<String>) {
        let (x, y) = self.input.cursor;
        self.tooltip = Some((x, y, lines));
    }

    /// Finish the frame: draw the tooltip and hand over the triangles (and the atlas if changed).
    pub fn finish(&mut self) -> Output<'_> {
        if let Some((x, y, lines)) = self.tooltip.take() {
            let t = Text { font: Font::Sans, size: 12.0, colour: style::TEXT };
            let pad = self.px(8.0);
            let lh = self.line_height(t);
            let w = lines.iter().map(|l| self.measure(l, t)).fold(0.0, f32::max) + 2.0 * pad;
            let h = lines.len() as f32 * lh + 2.0 * pad;
            let mut bx = x + self.px(16.0);
            let mut by = y + self.px(16.0);
            if bx + w > self.size.0 {
                bx = x - w - self.px(8.0);
            }
            if by + h > self.size.1 {
                by = y - h - self.px(8.0);
            }
            let r = Rect::new(bx.max(0.0), by.max(0.0), w, h);
            let rad = self.px(6.0);
            self.rect(r, style::TOOLTIP, rad);
            self.outline(r, style::BORDER, rad);
            for (k, l) in lines.iter().enumerate() {
                self.text(r.x + pad, r.y + pad + k as f32 * lh, l, t);
            }
        }
        let dirty = std::mem::take(&mut self.atlas_dirty);
        Output { vertices: &self.vertices, indices: &self.indices, atlas: dirty.then_some(&self.atlas[..]), atlas_size: ATLAS }
    }
}
