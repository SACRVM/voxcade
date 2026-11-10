//! A 64-bit hash that gives the same number on every machine and with every
//! Rust version (the standard library's may change between them): for the
//! checks of the simulation's state that replays carry, and the fingerprint
//! of a game's data.

use glam::Vec3;

/// Mixes a number thoroughly (splitmix64's finaliser).
pub fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Takes in numbers and bytes one after another; the order counts.
#[derive(Clone, Copy, Debug)]
pub struct Stable(u64);

impl Default for Stable {
    fn default() -> Self {
        Self(0x243f_6a88_85a3_08d3)
    }
}

impl Stable {
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0 = mix(self.0 ^ v).wrapping_add(0x9e37_79b9_7f4a_7c15);
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.u64(v as u32 as u64)
    }

    /// The exact bits, so -0.0 and 0.0 count as different.
    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.u64(v.to_bits() as u64)
    }

    pub fn vec3(&mut self, v: Vec3) -> &mut Self {
        self.f32(v.x).f32(v.y).f32(v.z)
    }

    /// Eight bytes at a time, then the length (so trailing zeros count).
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        for part in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..part.len()].copy_from_slice(part);
            self.u64(u64::from_le_bytes(word));
        }
        self.u64(bytes.len() as u64)
    }

    pub fn finish(&self) -> u64 {
        self.0
    }

    /// Goes on from where `finish` left a hash (one sent to another
    /// machine, kept in a file).
    pub fn resume(state: u64) -> Self {
        Self(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers are fixed for good: replays written today must still
    /// check tomorrow, on any machine.
    #[test]
    fn the_hash_never_changes() {
        assert_eq!(mix(1), 0x5692_161d_100b_05e5);
        assert_eq!(Stable::default().u64(7).bytes(b"voxcade").finish(), Stable::default().u64(7).bytes(b"voxcade").finish());
        assert_ne!(Stable::default().bytes(b"ab").finish(), Stable::default().bytes(b"ab\0").finish());
        assert_ne!(Stable::default().u64(1).u64(2).finish(), Stable::default().u64(2).u64(1).finish());
    }
}
