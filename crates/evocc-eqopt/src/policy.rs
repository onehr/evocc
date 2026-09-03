//! The producer's extraction policy.
//!
//! Section 7.5 ("Cost is a policy", revision 1.9.50) says the cost function is
//! the default policy of the P0 profile and not a property the checker
//! verifies: a certificate proves that every replacement is equivalent, never
//! that it was the cheapest, and Section 8.7 already lets a producer decline
//! any eligible region. A producer may therefore extract under any other cost
//! function provided it is deterministic for a given module, budget and policy
//! name, applies the same strict-improvement rule relative to its own costs,
//! and is named in the producer's report rather than in the certificate.
//!
//! This module is that interface. A policy is three things the specification
//! lets a producer choose -- the rule registry it draws from, the cost function
//! it extracts under, and the schedule parameters it runs within -- behind one
//! deterministic value. Nothing here changes the certificate format.

use crate::scalar_eq::ScalarOperator;
use crate::ScalarEqBudget;

/// What one emitted instruction costs.
///
/// The three structural zero costs of Section 7.5 -- a seed, a `const` node,
/// and a `source=before` witness other than the replacement root -- belong to
/// the profile's shape rather than to any policy, so a cost function chooses
/// only what an operator the producer would newly emit is worth. A cost of
/// zero for an operator is a legal policy: it makes no region rooted at that
/// operator a strict improvement, and Section 8.7 lets the producer decline it.
pub trait CostModel {
    /// The cost of emitting one instance of `operator`.
    fn operator_cost(&self, operator: ScalarOperator) -> u64;
}

/// The cost functions this producer ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CostFunction {
    /// Section 7.5's default: instruction count. Every admitted operator costs
    /// one and a constant costs zero.
    InstructionCount,
    /// A weighted variant, shipped to prove that the cost function is a
    /// parameter: a multiply costs three, a shift or a width change costs one,
    /// every other admitted operator costs one, and a constant costs zero.
    WeightedOperators,
    /// A control that charges nothing for a multiply. No region rooted at one
    /// is then a strict improvement, so every such region is declined. It
    /// exists so that "the two shipped policies optimize the same programs"
    /// is a measurement a differing policy could have refuted.
    ZeroMultiply,
}

impl CostModel for CostFunction {
    fn operator_cost(&self, _operator: ScalarOperator) -> u64 {
        todo!("the extraction policy has no cost function behind it yet")
    }
}

/// The widest reviewed registry a policy draws rules from.
///
/// Section 11.6 admits three registries and a certificate commits to one. The
/// producer already names the narrowest registry containing every rule it
/// fired; this names the widest one it is willing to fire from, and a region
/// whose rewrites reach past it is declined under Section 8.7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegistryCeiling {
    V1,
    V2,
    V3,
}

impl RegistryCeiling {
    /// The registry name this ceiling permits at its widest.
    #[must_use]
    pub fn name(self) -> &'static str {
        todo!("the extraction policy has no registry ceiling behind it yet")
    }
}

/// Section 8.11's bounds, as parameters rather than constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SchedulePolicy {
    pub max_nodes: usize,
    pub max_epochs: usize,
    /// Section 8.11: transactions one run may chain over a single module.
    pub max_transactions: usize,
}

impl SchedulePolicy {
    /// The scheduler bounds as the transient optimizer takes them.
    #[must_use]
    pub fn budget(self) -> ScalarEqBudget {
        ScalarEqBudget::new(self.max_nodes, self.max_epochs)
    }
}

impl Default for SchedulePolicy {
    fn default() -> Self {
        Self {
            max_nodes: 100_000,
            max_epochs: 1_024,
            max_transactions: 8,
        }
    }
}

/// One deterministic producer policy: a name, a registry, a cost function and
/// schedule parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExtractionPolicy {
    name: &'static str,
    registry: RegistryCeiling,
    cost: CostFunction,
    schedule: SchedulePolicy,
}

impl ExtractionPolicy {
    /// A policy assembled from its three parameters.
    #[must_use]
    pub fn new(
        _name: &'static str,
        _registry: RegistryCeiling,
        _cost: CostFunction,
        _schedule: SchedulePolicy,
    ) -> Self {
        todo!("the extraction policy is not assembled yet")
    }

    /// The name that goes into the producer's report.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The widest registry this policy fires from.
    #[must_use]
    pub fn registry(&self) -> RegistryCeiling {
        self.registry
    }

    /// The cost function extraction is judged against.
    #[must_use]
    pub fn cost(&self) -> CostFunction {
        self.cost
    }

    /// The schedule parameters this policy runs within.
    #[must_use]
    pub fn schedule(&self) -> SchedulePolicy {
        self.schedule
    }

    /// The same policy under different scheduler bounds.
    #[must_use]
    pub fn with_schedule(self, _schedule: SchedulePolicy) -> Self {
        todo!("the extraction policy is not assembled yet")
    }
}

impl Default for ExtractionPolicy {
    fn default() -> Self {
        todo!("the extraction policy has no default yet")
    }
}

/// Every policy this producer ships, in report order.
#[must_use]
pub fn shipped_policies() -> Vec<ExtractionPolicy> {
    todo!("no policy is registered yet")
}

/// The policy named `name`, if this producer ships one.
#[must_use]
pub fn policy_by_name(_name: &str) -> Option<ExtractionPolicy> {
    todo!("no policy is registered yet")
}
