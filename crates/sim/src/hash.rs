//! Stable hashing: FNV-1a for bytes (world-state hashes, stream names) and a mixer for integer
//! coordinates (noise lattices). Never std's hasher, which may change between Rust versions.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

pub fn fnv64(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

pub fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Hash of a seed and two integer coordinates.
pub fn hash2(seed: u64, x: i32, y: i32) -> u64 {
    mix(mix(seed ^ 0x517C_C1B7_2722_0A95 ^ (x as u32 as u64)) ^ ((y as u32 as u64) << 32))
}
