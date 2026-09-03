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

use core::fmt;
use core::fmt::Write as _;

use evocc_evoir::CmpOp;

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

/// One coordinate of the cost vector.
///
/// There is one axis per operator the P0 profile can emit or extract, and no
/// axis for the constant: Section 7.5 fixes a constant's cost at zero as part
/// of the profile's shape, so it is not a coordinate a policy may move. Every
/// comparison predicate shares the `ICmp` axis, because the profile prices the
/// instruction rather than the predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CostAxis {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    Shl,
    LShr,
    AShr,
    ZExt,
    SExt,
    Trunc,
    ICmp,
}

impl CostAxis {
    /// Every axis, in the order a rendered vector prints them.
    pub const ALL: [Self; 13] = [
        Self::Add,
        Self::Sub,
        Self::Mul,
        Self::And,
        Self::Or,
        Self::Xor,
        Self::Shl,
        Self::LShr,
        Self::AShr,
        Self::ZExt,
        Self::SExt,
        Self::Trunc,
        Self::ICmp,
    ];

    /// The axis `operator` is priced on, or `None` for the constant.
    #[must_use]
    pub fn of(operator: ScalarOperator) -> Option<Self> {
        Some(match operator {
            ScalarOperator::Const => return None,
            ScalarOperator::Add => Self::Add,
            ScalarOperator::Sub => Self::Sub,
            ScalarOperator::Mul => Self::Mul,
            ScalarOperator::And => Self::And,
            ScalarOperator::Or => Self::Or,
            ScalarOperator::Xor => Self::Xor,
            ScalarOperator::Shl => Self::Shl,
            ScalarOperator::LShr => Self::LShr,
            ScalarOperator::AShr => Self::AShr,
            ScalarOperator::ZExt => Self::ZExt,
            ScalarOperator::SExt => Self::SExt,
            ScalarOperator::Trunc => Self::Trunc,
            ScalarOperator::ICmp(..) => Self::ICmp,
        })
    }

    /// The name this axis carries in a rendered vector.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Shl => "shl",
            Self::LShr => "lshr",
            Self::AShr => "ashr",
            Self::ZExt => "zext",
            Self::SExt => "sext",
            Self::Trunc => "trunc",
            Self::ICmp => "icmp",
        }
    }

    /// One operator this axis prices, used to project a named cost function
    /// on to the vector. Every operator on an axis costs the same under a
    /// vector, so any of them would serve; `cost_vector.rs` pins that the
    /// shipped functions agree with the projection on all of them.
    fn representative(self) -> ScalarOperator {
        match self {
            Self::Add => ScalarOperator::Add,
            Self::Sub => ScalarOperator::Sub,
            Self::Mul => ScalarOperator::Mul,
            Self::And => ScalarOperator::And,
            Self::Or => ScalarOperator::Or,
            Self::Xor => ScalarOperator::Xor,
            Self::Shl => ScalarOperator::Shl,
            Self::LShr => ScalarOperator::LShr,
            Self::AShr => ScalarOperator::AShr,
            Self::ZExt => ScalarOperator::ZExt,
            Self::SExt => ScalarOperator::SExt,
            Self::Trunc => ScalarOperator::Trunc,
            Self::ICmp => ScalarOperator::ICmp(CmpOp::Eq),
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Add => 0,
            Self::Sub => 1,
            Self::Mul => 2,
            Self::And => 3,
            Self::Or => 4,
            Self::Xor => 5,
            Self::Shl => 6,
            Self::LShr => 7,
            Self::AShr => 8,
            Self::ZExt => 9,
            Self::SExt => 10,
            Self::Trunc => 11,
            Self::ICmp => 12,
        }
    }
}

/// A cost function written out as one weight per axis.
///
/// This is the space a search over cost searches. Every shipped cost function
/// is a point in it ([`CostFunction::weights`]), and a point in it is a cost
/// function ([`CostFunction::Vector`]), so a search never reaches past what
/// Section 7.5 already permits a producer to choose.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperatorWeights {
    values: [u64; 13],
}

impl OperatorWeights {
    /// The same weight on every axis.
    #[must_use]
    pub const fn uniform(weight: u64) -> Self {
        Self {
            values: [weight; 13],
        }
    }

    /// A vector from its coordinates in [`CostAxis::ALL`] order.
    #[must_use]
    pub const fn from_values(values: [u64; 13]) -> Self {
        Self { values }
    }

    /// The coordinates, in [`CostAxis::ALL`] order.
    #[must_use]
    pub const fn values(&self) -> [u64; 13] {
        self.values
    }

    /// The weight on one axis.
    #[must_use]
    pub fn get(&self, axis: CostAxis) -> u64 {
        self.values[axis.index()]
    }

    /// The same vector with one axis moved.
    #[must_use]
    pub fn with(mut self, axis: CostAxis, weight: u64) -> Self {
        self.values[axis.index()] = weight;
        self
    }

    /// `add=1,sub=1,...` in axis order: the one form a report and a command
    /// line both use, so a searched weighting replays from either.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = String::new();
        for (position, axis) in CostAxis::ALL.iter().enumerate() {
            if position > 0 {
                text.push(',');
            }
            let _ = write!(text, "{}={}", axis.name(), self.get(*axis));
        }
        text
    }

    /// The inverse of [`Self::render`]. Every axis must appear exactly once
    /// and in order, so a vector that parses is a vector that round-trips.
    pub fn parse(text: &str) -> Result<Self, String> {
        let fields = text.split(',').collect::<Vec<_>>();
        if fields.len() != CostAxis::ALL.len() {
            return Err(format!(
                "a weight vector has {} fields, not {}",
                CostAxis::ALL.len(),
                fields.len()
            ));
        }
        let mut values = [0_u64; 13];
        for (field, axis) in fields.iter().zip(CostAxis::ALL) {
            let (name, value) = field
                .split_once('=')
                .ok_or_else(|| format!("{field} is not NAME=WEIGHT"))?;
            if name != axis.name() {
                return Err(format!("expected {} where {name} appears", axis.name()));
            }
            values[axis.index()] = value
                .parse::<u64>()
                .map_err(|_| format!("{value} is not a non-negative integer weight"))?;
        }
        Ok(Self { values })
    }
}

impl fmt::Debug for OperatorWeights {
    /// The rendered form, so a report that prints a policy's cost function
    /// prints the weights themselves rather than a struct dump.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.render())
    }
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
    /// All positive, and cheapest on an addition: a multiply costs three, a
    /// shift two, everything else one. With the emitting rules of
    /// `p0.scalar.v4` this extracts `add(x, x)` from every spelling of a
    /// doubling.
    PreferAdd,
    /// All positive, and cheapest on a shift: a multiply costs three, an
    /// addition two, everything else one. It extracts `shl(x, 1)` from every
    /// spelling of a doubling, which is the other half of the measurement.
    PreferShift,
    /// A control that charges nothing for a multiply. No region rooted at one
    /// is then a strict improvement, so every such region is declined. It
    /// exists so that "the two shipped policies optimize the same programs"
    /// is a measurement a differing policy could have refuted.
    ZeroMultiply,
    /// One weight per axis, written out. Every variant above is a point in
    /// this space; this variant is what a search over cost produces.
    Vector(OperatorWeights),
}

impl CostFunction {
    /// This function as a point in the weight space.
    ///
    /// The projection reads [`CostModel::operator_cost`] rather than restating
    /// it, so a named function and its vector form cannot drift apart. It is
    /// lossless because no shipped function prices two comparison predicates
    /// differently, which `cost_vector.rs` pins over the whole operator
    /// domain rather than leaving to this comment.
    #[must_use]
    pub fn weights(self) -> OperatorWeights {
        let mut values = [0_u64; 13];
        for axis in CostAxis::ALL {
            values[axis.index()] = self.operator_cost(axis.representative());
        }
        OperatorWeights::from_values(values)
    }
}

impl CostModel for CostFunction {
    fn operator_cost(&self, operator: ScalarOperator) -> u64 {
        if operator == ScalarOperator::Const {
            // Section 7.5's semantic convention, not a policy choice: a
            // constant emits no instruction the cost model can charge for.
            return 0;
        }
        match self {
            Self::InstructionCount => 1,
            Self::WeightedOperators => match operator {
                ScalarOperator::Mul => 3,
                _ => 1,
            },
            Self::PreferAdd => match operator {
                ScalarOperator::Mul => 3,
                ScalarOperator::Shl | ScalarOperator::LShr | ScalarOperator::AShr => 2,
                _ => 1,
            },
            Self::PreferShift => match operator {
                ScalarOperator::Mul => 3,
                ScalarOperator::Add => 2,
                _ => 1,
            },
            Self::ZeroMultiply => match operator {
                ScalarOperator::Mul => 0,
                _ => 1,
            },
            Self::Vector(weights) => CostAxis::of(operator).map_or(0, |axis| weights.get(axis)),
        }
    }
}

/// The widest reviewed registry a policy draws rules from.
///
/// Section 11.6 admits four registries and a certificate commits to one. The
/// producer already names the narrowest registry containing every rule it
/// fired; this names the widest one it is willing to fire from, and a region
/// whose rewrites reach past it is declined under Section 8.7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegistryCeiling {
    V1,
    V2,
    V3,
    V4,
}

impl RegistryCeiling {
    /// The registry name this ceiling permits at its widest.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::V1 => "p0.scalar.v1",
            Self::V2 => "p0.scalar.v2",
            Self::V3 => "p0.scalar.v3",
            Self::V4 => "p0.scalar.v4",
        }
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
        name: &'static str,
        registry: RegistryCeiling,
        cost: CostFunction,
        schedule: SchedulePolicy,
    ) -> Self {
        Self {
            name,
            registry,
            cost,
            schedule,
        }
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
    pub fn with_schedule(self, schedule: SchedulePolicy) -> Self {
        Self { schedule, ..self }
    }

    /// The same policy under a different cost function.
    ///
    /// The name is kept: what changes is how the policy prices an operator,
    /// not which policy it is. A caller that wants a new name builds a new
    /// policy with [`Self::new`].
    #[must_use]
    pub fn with_cost(self, cost: CostFunction) -> Self {
        Self { cost, ..self }
    }
}

impl Default for ExtractionPolicy {
    fn default() -> Self {
        Self::new(
            "instruction-count",
            RegistryCeiling::V4,
            CostFunction::InstructionCount,
            SchedulePolicy::default(),
        )
    }
}

/// Every policy this producer ships, in report order.
#[must_use]
pub fn shipped_policies() -> Vec<ExtractionPolicy> {
    vec![
        ExtractionPolicy::default(),
        ExtractionPolicy::new(
            "weighted-operators",
            RegistryCeiling::V4,
            CostFunction::WeightedOperators,
            SchedulePolicy::default(),
        ),
        ExtractionPolicy::new(
            "zero-multiply",
            RegistryCeiling::V4,
            CostFunction::ZeroMultiply,
            SchedulePolicy::default(),
        ),
        ExtractionPolicy::new(
            "prefer-add",
            RegistryCeiling::V4,
            CostFunction::PreferAdd,
            SchedulePolicy::default(),
        ),
        ExtractionPolicy::new(
            "prefer-shift",
            RegistryCeiling::V4,
            CostFunction::PreferShift,
            SchedulePolicy::default(),
        ),
    ]
}

/// The policy named `name`, if this producer ships one.
#[must_use]
pub fn policy_by_name(name: &str) -> Option<ExtractionPolicy> {
    shipped_policies()
        .into_iter()
        .find(|policy| policy.name == name)
}
