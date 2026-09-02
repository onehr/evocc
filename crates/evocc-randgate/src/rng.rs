//! The deterministic pseudo-random source.

/// A seeded, reproducible bit source.
///
/// The gate's whole value rests on a seed naming exactly one program forever,
/// so the algorithm is written out here rather than taken from a dependency
/// whose version could change the stream under a recorded seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

impl Rng {
    /// Start the stream at `seed`.
    #[must_use]
    pub const fn from_seed(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Draw the next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Draw a value in `[0, bound)`, or `0` when `bound` is zero.
    ///
    /// Lemire's multiply-shift rejection keeps the draw uniform; the retry loop
    /// is what makes the mapping independent of `bound`'s relation to `2^64`,
    /// so a configuration change never silently reshapes an old seed's stream
    /// into a biased one.
    pub fn below(&mut self, bound: u64) -> u64 {
        if bound <= 1 {
            return 0;
        }
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let draw = self.next_u64();
            let product = u128::from(draw) * u128::from(bound);
            #[allow(clippy::cast_possible_truncation)]
            let low = product as u64;
            if low >= threshold {
                return (product >> 64) as u64;
            }
        }
    }

    /// Draw `true` with probability `numerator / 100`.
    pub fn chance(&mut self, numerator: u32) -> bool {
        self.below(100) < u64::from(numerator)
    }

    /// Draw one element of a non-empty slice.
    ///
    /// # Panics
    /// Panics when `items` is empty.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        assert!(!items.is_empty(), "cannot pick from an empty slice");
        let index = self.below(items.len() as u64);
        #[allow(clippy::cast_possible_truncation)]
        let index = index as usize;
        &items[index]
    }
}
