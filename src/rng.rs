//! Small deterministic random generator (xorshift64*). The simulation owns
//! one, so the same inputs always produce the same game.

#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// A generator standing where `state` says (a save's).
    pub fn from_state(state: u64) -> Self {
        // Any state but 0 is one it can stand at.
        Self(if state == 0 { 1 } else { state })
    }

    /// Where the generator stands: what replays check (`Game::state_hash`).
    pub fn state(&self) -> u64 {
        self.0
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A generator brought back from its state draws what the one it was
    /// taken from draws next, whatever that state is.
    #[test]
    fn a_generator_comes_back_where_it_stood() {
        let mut rng = Rng::new(7);
        for _ in 0..100 {
            let mut back = Rng::from_state(rng.state());
            assert_eq!(back.next_u32(), rng.clone().next_u32());
            rng.next_u32();
        }
    }
}
