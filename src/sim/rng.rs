//! Mulberry32, bit-compatible with the web game's `core/Rng.ts`.

#[derive(Debug, Clone)]
pub struct Rng {
    state: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    pub fn next_f32(&mut self) -> f32 {
        self.next_f64() as f32
    }

    pub fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6d2b_79f5);
        let mut t = self.state;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        (t ^ (t >> 14)) as f64 / 4_294_967_296.0
    }

    pub fn next_index(&mut self, len: usize) -> usize {
        ((self.next_f64() * len as f64) as usize).min(len.saturating_sub(1))
    }

    /// Uniform in `-1..1`.
    pub fn signed(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_mulberry32() {
        // Reference values from the JS implementation with seed 1.
        let mut r = Rng::new(1);
        let a = r.next_f64();
        let b = r.next_f64();
        assert!((a - 0.6270739405881613).abs() < 1e-12, "{a}");
        assert!((b - 0.002735721180215478).abs() < 1e-12, "{b}");
    }

    #[test]
    fn index_stays_in_range() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            assert!(r.next_index(5) < 5);
        }
    }
}
