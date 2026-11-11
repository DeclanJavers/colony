//! Seeded random numbers. Our own small generator (xoshiro256**, seeded through SplitMix64), so a
//! library update can never change a world. Each system draws from its own named stream.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rng {
    s: [u64; 4],
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut st = seed;
        Rng { s: [splitmix(&mut st), splitmix(&mut st), splitmix(&mut st), splitmix(&mut st)] }
    }

    /// The stream for one system: the world seed plus the system's name.
    pub fn stream(seed: u64, name: &str) -> Self {
        Self::new(seed ^ crate::hash::fnv64(name.as_bytes()).rotate_left(17))
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform integer in [0, n).
    pub fn below(&mut self, n: u32) -> u32 {
        (((self.next_u64() >> 32) * n as u64) >> 32) as u32
    }

    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_repeat_and_differ() {
        let a: Vec<u64> = (0..4).map({ let mut r = Rng::stream(7, "a"); move |_| r.next_u64() }).collect();
        let b: Vec<u64> = (0..4).map({ let mut r = Rng::stream(7, "a"); move |_| r.next_u64() }).collect();
        let c: Vec<u64> = (0..4).map({ let mut r = Rng::stream(7, "b"); move |_| r.next_u64() }).collect();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn ranges_stay_in_bounds() {
        let mut r = Rng::new(1);
        for _ in 0..10_000 {
            let f = r.next_f32();
            assert!((0.0..1.0).contains(&f));
            assert!(r.below(7) < 7);
        }
    }
}
