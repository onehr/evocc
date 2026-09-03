//! Equality optimization over EvoIR.
//!
//! This crate owns the e-graph dependency. The EvoIR crate remains the
//! optimizer-independent authority for IR types, validation, text and
//! interpretation.

mod eligibility;
mod evp1;
pub mod known_bits;
mod persistent;
pub mod policy;
mod scalar_eq;
mod seed_facts;

pub use evp1::*;
pub use persistent::*;
pub use policy::{
    policy_by_name, shipped_policies, CostAxis, CostFunction, CostModel, ExtractionPolicy,
    OperatorWeights, RegistryCeiling, SchedulePolicy,
};
pub use scalar_eq::*;
pub use seed_facts::{seed_fact_agreement, SeedFactAgreement};
