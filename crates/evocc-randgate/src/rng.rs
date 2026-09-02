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

impl Rng {
    /// Start the stream at `seed`.
    #[must_use]
    pub const fn from_seed(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Draw the next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        todo!("deterministic bit source")
    }

    /// Draw a value in `[0, bound)`, or `0` when `bound` is zero.
    pub fn below(&mut self, bound: u64) -> u64 {
        todo!("bounded draw for {bound}")
    }

    /// Draw `true` with probability `numerator / 100`.
    pub fn chance(&mut self, numerator: u32) -> bool {
        todo!("weighted coin for {numerator}")
    }

    /// Draw one element of a non-empty slice.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        todo!("uniform choice over {} items", items.len())
    }
}
