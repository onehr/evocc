//! Execution-based translation validation, not a universal equivalence proof.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::time::Instant;

use evocc_evoir::{
    interpret_module, render_module, validate_module, InterpreterLimits, Module, RuntimeValue,
};
use evocc_proof::sha256_hex;
use evocc_randgate::{DefinedRun, Observation};

pub mod preflight;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassClass {
    Verified,
    Validated,
    Lowering,
}

impl PassClass {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::Validated => "VALIDATED",
            Self::Lowering => "LOWERING",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Opportunities {
    pub eligible_sites: u64,
    pub taken_sites: u64,
    pub declined_by_reason: BTreeMap<String, u64>,
    pub estimated_bytes: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct PassOutput {
    pub module: Module,
    pub opportunities: Opportunities,
}

pub trait ModulePass {
    fn name(&self) -> &'static str;
    fn version(&self) -> u32;
    fn apply(&self, module: &Module) -> Result<PassOutput, String>;
}

#[derive(Debug, Clone)]
pub struct ExecutionInput {
    pub id: String,
    pub entry: String,
    pub arguments: Vec<RuntimeValue>,
    pub step_budget: u64,
}

impl ExecutionInput {
    pub fn main(id: impl Into<String>, arguments: Vec<RuntimeValue>, step_budget: u64) -> Self {
        Self {
            id: id.into(),
            entry: "main".into(),
            arguments,
            step_budget,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRecord {
    pub input_id: String,
    pub input_sha256: String,
    pub before_steps: Option<u64>,
    pub after_steps: Option<u64>,
    pub before_outcome_sha256: String,
    pub after_outcome_sha256: String,
    pub outcome: String,
}

/// One interpreter observation for measurement consumers such as N05. This is
/// not pass admission: notably a volatile program can be measured while still
/// being opaque to this version of the transformation-validation profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionMeasurement {
    pub input_sha256: String,
    pub outcome_sha256: String,
    pub steps: Option<u64>,
    pub observation: Observation,
}

pub fn measure_execution(
    module: &Module,
    input: &ExecutionInput,
) -> Result<ExecutionMeasurement, ValidationError> {
    validate_module(module).map_err(ValidationError::InvalidInput)?;
    let (observation, steps) = execute(module, input)?;
    Ok(ExecutionMeasurement {
        input_sha256: sha256_hex(
            format!(
                "{}\n{:?}\n{}",
                input.entry, input.arguments, input.step_budget
            )
            .as_bytes(),
        ),
        outcome_sha256: sha256_hex(format!("{observation:?}").as_bytes()),
        steps,
        observation,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassRecord {
    pub name: String,
    pub version: u32,
    pub class: PassClass,
    pub opportunities: Opportunities,
    pub before_ir_sha256: String,
    pub after_ir_sha256: String,
    pub executions: Vec<ExecutionRecord>,
    pub pass_nanos: u128,
    pub validation_nanos: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PipelineReport {
    pub passes: Vec<PassRecord>,
}

#[derive(Debug, Clone)]
pub struct PipelineOutput {
    pub module: Module,
    pub report: PipelineReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    InvalidInput(String),
    InvalidOutput(String),
    InvalidTelemetry(String),
    Ineligible(String),
    NoInputs,
    PassFailed(String),
    Mismatch {
        input_id: String,
        observation: &'static str,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ValidationError {}

/// Stage the entire chain and publish only if every intermediate comparison
/// succeeds. The caller's module is never mutated, including on later failure.
/// Empty pipelines validate structure but perform no executions or transforms.
pub fn run_pipeline(
    module: &Module,
    passes: &[&dyn ModulePass],
    inputs: &[ExecutionInput],
) -> Result<PipelineOutput, ValidationError> {
    validate_module(module).map_err(ValidationError::InvalidInput)?;
    let mut staged = module.clone();
    let mut report = PipelineReport::default();
    if passes.is_empty() {
        return Ok(PipelineOutput {
            module: staged,
            report,
        });
    }
    if inputs.is_empty() {
        return Err(ValidationError::NoInputs);
    }
    let mut ids = BTreeSet::new();
    for input in inputs {
        if input.id.is_empty() || !ids.insert(&input.id) {
            return Err(ValidationError::InvalidInput(
                "empty or duplicate execution input ID".into(),
            ));
        }
    }
    for pass in passes {
        admission(&staged)?;
        if pass.name().is_empty() || pass.version() == 0 {
            return Err(ValidationError::InvalidTelemetry(
                "empty pass name or zero version".into(),
            ));
        }
        let before_ir_sha256 = sha256_hex(render_module(&staged).as_bytes());
        let started = Instant::now();
        let output = pass.apply(&staged).map_err(ValidationError::PassFailed)?;
        let pass_nanos = started.elapsed().as_nanos();
        check_opportunities(&output.opportunities)?;
        validate_module(&output.module).map_err(ValidationError::InvalidOutput)?;
        admission(&output.module)?;
        let after_ir_sha256 = sha256_hex(render_module(&output.module).as_bytes());
        let started = Instant::now();
        let mut executions = Vec::with_capacity(inputs.len());
        for input in inputs {
            let (before, before_steps) = execute(&staged, input)?;
            let (after, after_steps) = execute(&output.module, input)?;
            if let Some(observation) = observation_difference(&before, &after) {
                return Err(ValidationError::Mismatch {
                    input_id: input.id.clone(),
                    observation,
                });
            }
            // These compact hashes are measurement identities, not certificates.
            // Debug formatting belongs to this versioned report format only;
            // equivalence above compares the typed observations, never hashes.
            executions.push(ExecutionRecord {
                input_id: input.id.clone(),
                input_sha256: sha256_hex(
                    format!(
                        "{}\n{:?}\n{}",
                        input.entry, input.arguments, input.step_budget
                    )
                    .as_bytes(),
                ),
                before_steps,
                after_steps,
                before_outcome_sha256: sha256_hex(format!("{before:?}").as_bytes()),
                after_outcome_sha256: sha256_hex(format!("{after:?}").as_bytes()),
                outcome: match before {
                    Observation::Defined(_) => "DEFINED_MATCH".into(),
                    Observation::Undefined { code, .. } => format!("DIAGNOSTIC_MATCH:{code}"),
                },
            });
        }
        let validation_nanos = started.elapsed().as_nanos();
        report.passes.push(PassRecord {
            name: pass.name().into(),
            version: pass.version(),
            class: PassClass::Validated,
            opportunities: output.opportunities,
            before_ir_sha256,
            after_ir_sha256,
            executions,
            pass_nanos,
            validation_nanos,
        });
        staged = output.module;
    }
    Ok(PipelineOutput {
        module: staged,
        report,
    })
}

fn check_opportunities(opportunities: &Opportunities) -> Result<(), ValidationError> {
    let sum = opportunities.declined_by_reason.iter().try_fold(
        opportunities.taken_sites,
        |sum, (reason, count)| {
            if reason.is_empty() || *count == 0 {
                None
            } else {
                sum.checked_add(*count)
            }
        },
    );
    if sum != Some(opportunities.eligible_sites) {
        return Err(ValidationError::InvalidTelemetry(
            "eligible sites must equal taken plus nonempty declined reasons without overflow"
                .into(),
        ));
    }
    Ok(())
}

fn admission(module: &Module) -> Result<(), ValidationError> {
    if !module.assembly.is_empty()
        || !module.lifecycle.is_empty()
        || !module.tls.is_empty()
        || !module.extern_objects.is_empty()
    {
        return Err(ValidationError::Ineligible(
            "assembly, lifecycle, TLS or external object execution is outside this TV profile"
                .into(),
        ));
    }
    for function in &module.functions {
        let effects = function.effects;
        if effects.volatile || effects.atomic || effects.io || effects.fenv || effects.may_unwind {
            return Err(ValidationError::Ineligible(format!(
                "opaque effects in @{}",
                function.name
            )));
        }
    }
    Ok(())
}

fn execute(
    module: &Module,
    input: &ExecutionInput,
) -> Result<(Observation, Option<u64>), ValidationError> {
    match interpret_module(
        module,
        &input.entry,
        &input.arguments,
        InterpreterLimits::new(input.step_budget),
    ) {
        Ok(outcome) => Ok((
            Observation::Defined(DefinedRun::from(&outcome)),
            Some(outcome.steps),
        )),
        Err(error) => {
            let code = error.code();
            // Only named source/runtime diagnostics are comparable. Unknown
            // codes, unsupported execution, external resolution and exhausted
            // host resources must never become matching diagnostic evidence.
            if !matches!(
                code,
                "E3001"
                    | "E3002"
                    | "E3003"
                    | "E3004"
                    | "E3005"
                    | "E3006"
                    | "E3009"
                    | "E3010"
                    | "E3011"
                    | "E3012"
                    | "E3013"
                    | "E3014"
                    | "E3015"
                    | "E3016"
                    | "E3018"
                    | "E3019"
                    | "E3022"
                    | "E3024"
                    | "E3027"
                    | "E3038"
                    | "E3047"
                    | "E3048"
                    | "E3063"
            ) {
                return Err(ValidationError::Ineligible(format!(
                    "{}: {code}: {}",
                    input.id,
                    error.message()
                )));
            }
            Ok((
                Observation::Undefined {
                    code,
                    detail: error.message(),
                },
                None,
            ))
        }
    }
}

/// The existing randomized gate's complete observation projection: return
/// values, surviving memory, ordered volatile events and diagnostics. Opaque
/// operations remain inadmissible to the pipeline even though this comparison
/// intentionally retains their event projection for callers and regressions.
pub fn observation_difference(before: &Observation, after: &Observation) -> Option<&'static str> {
    match (before, after) {
        (Observation::Defined(before), Observation::Defined(after)) => {
            if before.returns != after.returns {
                Some("returns")
            } else if before.statics != after.statics {
                Some("static-memory")
            } else if before.volatile != after.volatile {
                Some("events")
            } else {
                None
            }
        }
        (
            Observation::Undefined {
                code: left,
                detail: a,
            },
            Observation::Undefined {
                code: right,
                detail: b,
            },
        ) if left == right && a == b => None,
        _ => Some("diagnostics"),
    }
}

pub struct IdentityPass;

impl ModulePass for IdentityPass {
    fn name(&self) -> &'static str {
        "identity"
    }
    fn version(&self) -> u32 {
        1
    }
    fn apply(&self, module: &Module) -> Result<PassOutput, String> {
        Ok(PassOutput {
            module: module.clone(),
            opportunities: Opportunities::default(),
        })
    }
}

impl PipelineReport {
    /// Measured assembly bytes are deliberately absent from this interpreter
    /// report; N05 supplies them only after real object measurement.
    pub fn render_json(&self) -> String {
        let passes = self.passes.iter().map(|pass| {
            let declines = pass.opportunities.declined_by_reason.iter().map(|(reason, count)| format!("{}:{count}", json_string(reason))).collect::<Vec<_>>().join(",");
            let executions = pass.executions.iter().map(|run| format!("{{\"input_id\":{},\"input_sha256\":{},\"before_steps\":{},\"after_steps\":{},\"before_outcome_sha256\":{},\"after_outcome_sha256\":{},\"outcome\":{}}}",
                json_string(&run.input_id), json_string(&run.input_sha256), optional(run.before_steps), optional(run.after_steps), json_string(&run.before_outcome_sha256), json_string(&run.after_outcome_sha256), json_string(&run.outcome))).collect::<Vec<_>>().join(",");
            format!("{{\"name\":{},\"version\":{},\"class\":{},\"eligible_sites\":{},\"taken_sites\":{},\"declined_by_reason\":{{{declines}}},\"estimated_bytes\":{},\"actual_bytes\":null,\"before_ir_sha256\":{},\"after_ir_sha256\":{},\"validation_input_count\":{},\"executions\":[{executions}],\"pass_nanos\":{},\"validation_nanos\":{}}}",
                json_string(&pass.name), pass.version, json_string(pass.class.label()), pass.opportunities.eligible_sites, pass.opportunities.taken_sites, optional(pass.opportunities.estimated_bytes), json_string(&pass.before_ir_sha256), json_string(&pass.after_ir_sha256), pass.executions.len(), pass.pass_nanos, pass.validation_nanos)
        }).collect::<Vec<_>>().join(",");
        format!("{{\"schema\":\"evocc-pass-report-v1\",\"native_validation\":\"NOT_RUN\",\"passes\":[{passes}]}}\n")
    }
}

fn optional(value: Option<impl std::fmt::Display>) -> String {
    value.map_or_else(|| "null".into(), |value| value.to_string())
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch < '\u{20}' => {
                let _ = write!(output, "\\u{:04x}", ch as u32);
            }
            ch => output.push(ch),
        }
    }
    output.push('"');
    output
}
