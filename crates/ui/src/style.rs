//! The UI's colours, from the M0 mockup (ARCHITECTURE.md section 12). One meaning per colour,
//! used everywhere.

use crate::hex;

macro_rules! c {
    ($name:ident, $hex:expr, $a:expr) => {
        pub const $name: [f32; 4] = hex_const($hex, $a);
    };
}

/// Compile-time "#RRGGBB" to linear RGBA (through a close polynomial fit of the sRGB curve).
const fn hex_const(s: &str, a: f32) -> [f32; 4] {
    let b = s.as_bytes();
    const fn nib(c: u8) -> u32 {
        match c {
            b'0'..=b'9' => (c - b'0') as u32,
            b'a'..=b'f' => (c - b'a' + 10) as u32,
            _ => (c - b'A' + 10) as u32,
        }
    }
    const fn srgb(b: &[u8], i: usize) -> f32 {
        let x = (nib(b[i]) * 16 + nib(b[i + 1])) as f32 / 255.0;
        x * (x * (x * 0.305_306 + 0.682_171) + 0.012_522_9)
    }
    [srgb(b, 1), srgb(b, 3), srgb(b, 5), a]
}

c!(PANEL, "#1E1A1F", 0.93);
c!(TOOLTIP, "#1E1A1F", 0.97);
c!(BORDER, "#4A3F46", 1.0);
c!(TRACK, "#3A3138", 1.0);
c!(CHIP, "#2C252B", 1.0);
c!(CHIP_HOVER, "#3A3138", 1.0);
c!(TEXT, "#F3EADB", 1.0);
c!(MUTED, "#B9AD9F", 1.0);
c!(FAINT, "#9A8F83", 1.0);
c!(GOLD, "#E8C468", 1.0);
c!(GOLD_HOVER, "#F0D383", 1.0);
c!(ON_GOLD, "#2A1E1A", 1.0);
c!(THOUGHT, "#E9DCC6", 1.0);
c!(WARN, "#F5C27A", 1.0);
c!(GOOD, "#9FD58A", 1.0);
c!(BAD, "#F0997B", 1.0);
c!(DANGER_BG, "#4A2A22", 1.0);
c!(DANGER, "#F5B8A3", 1.0);
c!(BODY_NEED, "#6CC4A1", 1.0);
c!(MIND_NEED, "#B49CF0", 1.0);
c!(SKILL, "#E8C468", 1.0);
c!(TRAIT, "#B49CF0", 1.0);
c!(BRAIN, "#E8C468", 1.0);
c!(RULES, "#7FB8E6", 1.0);
c!(AFFECTION, "#F0997B", 1.0);
// Event kinds (feed, history, filters)
c!(K_SOCIAL, "#7FB8E6", 1.0);
c!(K_CONFLICT, "#F0997B", 1.0);
c!(K_KNOWLEDGE, "#C3A6F0", 1.0);
c!(K_WORLD, "#8CC56A", 1.0);
c!(K_LIFE, "#E8C468", 1.0);
c!(K_YOU, "#F2A7C9", 1.0);

/// A colour from hex at runtime (same as the constants).
pub fn colour(s: &str) -> [f32; 4] {
    hex(s, 1.0)
}
