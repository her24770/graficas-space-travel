//! Generador pseudoaleatorio determinista (xorshift64*): misma semilla, mismo universo.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    /// Número en [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misma_semilla_misma_secuencia() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        assert!((0..10).all(|_| a.next_u32() == b.next_u32()));
    }

    #[test]
    fn f32_queda_en_el_rango() {
        let mut r = Rng::new(1);
        assert!((0..1000).all(|_| (0.0..1.0).contains(&r.f32())));
    }
}
