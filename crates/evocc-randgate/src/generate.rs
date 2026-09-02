//! The deterministic generator of well-typed, validator-clean EvoIR modules.

use evocc_evoir::{Module, RuntimeValue};

/// Bit widths the generator draws from. Every width the Day 7 profile executes
/// is representable; this list keeps the boundary widths and a spread of
/// non-byte widths so masking and extension defects cannot hide in a corner.
pub const WIDTHS: [u16; 14] = [1, 2, 3, 5, 7, 8, 13, 16, 17, 31, 32, 33, 47, 64];

/// The shape ceilings one generated module stays within.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorConfig {
    /// Callable helper functions defined before `@main`.
    pub max_helper_functions: u32,
    /// Module globals available to `global_addr`.
    pub max_globals: u32,
    /// Straight-line instructions emitted into one segment of one block.
    pub max_segment_instructions: u32,
    /// Nesting budget for branch and loop regions.
    pub max_region_depth: u32,
    /// Blocks one function may hold.
    pub max_blocks: u32,
    /// The largest trip count a generated loop may run.
    pub max_loop_trips: u64,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            max_helper_functions: 2,
            max_globals: 2,
            max_segment_instructions: 6,
            max_region_depth: 2,
            max_blocks: 16,
            max_loop_trips: 4,
        }
    }
}

/// Produce the one module this seed names.
///
/// The same seed and configuration yield the same module forever; the result
/// passes `validate_module` and renders to canonical Section 9 bytes.
#[must_use]
pub fn generate_module(seed: u64, config: GeneratorConfig) -> Module {
    let _ = (seed, config);
    todo!("deterministic module generation")
}

/// Deterministic entry arguments for `@main` in `module`, derived from `seed`.
#[must_use]
pub fn entry_arguments(module: &Module, seed: u64) -> Vec<RuntimeValue> {
    let _ = (module, seed);
    todo!("deterministic entry arguments")
}
