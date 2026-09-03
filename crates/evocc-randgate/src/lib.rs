//! The randomized semantic gate.
//!
//! A seed names one well-typed, validator-clean EvoIR module forever. The gate
//! interprets that module, runs the scalar equality optimizer over it under an
//! explicit budget, interprets the result, and compares the two executions by
//! the specification's equality criterion: ordinary return values, the world's
//! volatile trace, surviving static memory, and trap behaviour. Every EVP1
//! certificate the optimizer produced is replayed by the standalone
//! `evo-proofcheck` binary rather than by any code the optimizer can reach.
//!
//! Equality is modulo undefined behaviour. A program the interpreter reports as
//! undefined is discarded and counted, never compared.

pub mod differential;
pub mod generate;
pub mod report;
pub mod rng;
pub mod shrink;

pub use differential::{
    observe, run_seed, DefinedRun, GateBudget, Mismatch, MismatchKind, Observation, ProofChecker,
    StaticObservation, Verdict,
};
pub use generate::{entry_arguments, generate_module, GeneratorConfig, GeneratorMode, WIDTHS};
pub use report::{run_campaign, CampaignReport};
pub use rng::Rng;
pub use shrink::shrink_module;
