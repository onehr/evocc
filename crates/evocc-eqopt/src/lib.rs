//! Equality optimization over EvoIR.
//!
//! This crate owns the e-graph dependency. The EvoIR crate remains the
//! optimizer-independent authority for IR types, validation, text and
//! interpretation.

mod scalar_eq;

pub use scalar_eq::*;
