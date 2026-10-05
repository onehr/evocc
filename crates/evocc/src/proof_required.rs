//! Proof-required compilation: work package V06.
//!
//! This mode exports the typed Core source, the actual EvoIR, the actual SSA
//! EvoMIR and the actual post-SSA EvoMIR of one compilation into a new
//! directory. Then the compiled Lean checker checks each integrated stage for
//! each selected entry. The driver writes the assembly and the record only if
//! every integrated stage accepts every entry. A refusal, a missing input, a
//! missing or slow checker, or a construct outside `core-v1` gives a failure
//! exit code and no output. There is no fallback to ordinary compilation.
//!
//! With `-Oeq-scalar`, the directory also holds each rewrite step of the same
//! compilation: the EvoIR before the step, the EvoIR after the step and the
//! certificate. V01 reads the EvoIR before the first step, V05 checks each
//! step, and V02 reads the EvoIR after the last step. Adjacent stages must read
//! the same bytes. A rejected step gives a failure and no output. There is no
//! fallback to the program without the rewrite.
//!
//! The directory also holds the allocation export of the post-SSA EvoMIR. V03
//! reads the same post-SSA file as V02, and the allocated program and the
//! hints from the export.
//!
//! The directory also holds the assembly AST with the frame hints, and the
//! assembly text that the driver publishes. V04 reads the allocation export
//! that V03 reads, the AST and this text. The driver publishes `output.s` only
//! with the bytes that V04 checked.
//!
//! The compiled Lean checker is the one result-checking path. This module does
//! not check a compilation result. It collects the inputs, runs the checker and
//! records its answers.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use evocc_chir::CHirUnit;
use evocc_eqopt::ProofBundle;
use evocc_evoir::Module;
use evocc_mir::core_machine_export::{export_core_machine, CoreMachineMode};

use crate::DriverOptions;

/// The failure exit codes. Exit 0 means that every integrated stage accepted
/// every entry. Exit 1 keeps its ordinary meaning: an incorrect command line or
/// a compilation error before the proof inputs exist.
#[derive(Clone, Copy)]
enum Failure {
    /// A stage checker rejected an entry.
    Rejected = 3,
    /// A proof input is missing, changed, not readable or not writable.
    Input = 4,
    /// The checker is missing, has a different digest, timed out or failed.
    Checker = 5,
    /// The source or the output is outside the `core-v1` profile.
    Unsupported = 6,
}

type Refusal = (Failure, String);

/// The role names are the role names of the Lean command `--check-stage`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Source,
    Ir,
    Ssa,
    PostSsa,
    /// An untrusted loop or call proposal. This role is optional.
    Proposal,
    /// The EvoIR before one rewrite step.
    Before,
    /// The EvoIR after one rewrite step.
    After,
    /// The `evp2` certificate of one rewrite step.
    Certificate,
    /// The allocation export: the allocated program and the hints.
    Allocation,
    /// The assembly AST and the frame hints.
    Asm,
    /// The assembly text that the driver publishes.
    Text,
}

impl Role {
    fn name(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Ir => "ir",
            Self::Ssa => "ssa",
            Self::PostSsa => "postssa",
            Self::Proposal => "proposal",
            Self::Before => "before",
            Self::After => "after",
            Self::Certificate => "certificate",
            Self::Allocation => "allocation",
            Self::Asm => "asm",
            Self::Text => "text",
        }
    }
}

/// The EvoIR program that a stage reads in the rewrite chain.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    /// The EvoIR before the first rewrite step.
    First,
    /// The stage runs once for each rewrite step.
    EachStep,
    /// The EvoIR after the last rewrite step.
    Last,
}

struct Stage {
    id: &'static str,
    name: &'static str,
    /// The last program of the stage. The record says "checked to" it.
    reaches: &'static str,
    place: Place,
    /// The checker input roles, or `None` if the stage is not integrated.
    roles: Option<&'static [Role]>,
    /// The Lean result that an acceptance applies.
    theorem: &'static str,
}

/// All stages in pipeline order. The Lean table is `stages` in
/// `lean/EvoIR/Eval.lean`. To integrate a stage, set its `roles` here, add
/// the same row to the Lean table, and export each new role in `export`.
const STAGES: &[Stage] = &[
    Stage {
        id: "V01",
        name: "typed Core C-HIR to EvoIR",
        reaches: "EvoIR",
        place: Place::First,
        roles: Some(&[Role::Source, Role::Ir, Role::Proposal]),
        theorem: "EvoIR.Core.Symbolic.check_source_target_refines",
    },
    Stage {
        id: "V05",
        name: "EvoIR rewrites",
        reaches: "rewritten EvoIR",
        place: Place::EachStep,
        roles: Some(&[Role::Source, Role::Before, Role::After, Role::Certificate]),
        theorem: "EvoIR.Rewrite.checkRewrite_refines; composition EvoIR.Rewrite.check_source_rewrite_refines",
    },
    Stage {
        id: "V02",
        name: "EvoIR to SSA EvoMIR and post-SSA EvoMIR",
        reaches: "post-SSA EvoMIR",
        place: Place::Last,
        roles: Some(&[Role::Source, Role::Ir, Role::Ssa, Role::PostSsa]),
        theorem: "EvoIR.Machine.checkLowering; composition EvoIR.Machine.source_machine_refines",
    },
    Stage {
        id: "V03",
        name: "post-SSA EvoMIR to allocated EvoMIR",
        reaches: "allocated EvoMIR",
        place: Place::Last,
        roles: Some(&[Role::Source, Role::PostSsa, Role::Allocation]),
        theorem: "EvoIR.Alloc.checkAllocation_sound; composition EvoIR.Alloc.source_allocated_refines",
    },
    Stage {
        id: "V04",
        name: "allocated EvoMIR to final assembly text",
        reaches: "final assembly",
        place: Place::Last,
        roles: Some(&[Role::Source, Role::Allocation, Role::Asm, Role::Text]),
        theorem: "EvoIR.Asm.Proof.checkAsm_sound; composition EvoIR.Asm.Proof.source_asm_refines",
    },
];

const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// The scope of a result in which every stage is integrated.
const COMPLETE_SCOPE: &str = "checked from typed Core source to final assembly text";

const OUTPUT_NOTE: &str = "V04 checked these bytes: the printed assembly AST is equal to them.";

/// The trusted boundaries of every proof-required result. Each exported
/// snapshot is an untrusted input that a stage checks. The chain binds the
/// stages to each other and the last stage to the published text.
const TRUSTED: &[&str] = &[
    "preprocessing, parsing, elaboration and typed source export",
    "the Lean kernel, which checks the theorems, and the compiled Lean checker with the Lean compiler, its runtime and the host toolchain",
    "the readers of the input files",
    "the printer of the assembly AST (EvoIR.Asm.File.print)",
    "the Lean semantics of typed Core C and of the AArch64 subset",
    "the ABI entry assumption (EvoIR.Asm.Proof.AbiEntry): the caller obeys the AArch64 procedure call standard and gives sufficient stack",
    "the assembler, the linker, the runtime libraries and the hardware",
];

fn sha256(bytes: impl AsRef<[u8]>) -> String {
    evocc_proof::sha256_hex(bytes.as_ref())
}

pub(crate) struct ProofOptions {
    directory: PathBuf,
    checker: Option<PathBuf>,
    checker_sha256: Option<String>,
    timeout_ms: u64,
    proposals: Vec<(String, PathBuf)>,
}

/// The `--proof-*` arguments before admission.
#[derive(Default)]
pub(crate) struct ProofArguments {
    directory: Option<PathBuf>,
    checker: Option<PathBuf>,
    checker_sha256: Option<String>,
    timeout_ms: Option<u64>,
    proposals: Vec<(String, PathBuf)>,
}

/// The other driver options that the proof-required mode depends on.
pub(crate) struct Admission<'a> {
    pub emit_asm: bool,
    pub entries: &'a [String],
    /// `-O0` and `-fno-builtin`.
    pub baseline: bool,
    /// No development pass or backend variant, checked guards, and no EqSat
    /// option without `-Oeq-scalar`.
    pub unchanged_backend: bool,
    /// `-Oeq-scalar`: the rewrite steps go to stage V05.
    pub rewrites: bool,
    /// `--eq-checker-sha256` is on the command line.
    pub rewrite_checker_pinned: bool,
}

impl ProofArguments {
    pub(crate) fn parse(&mut self, arg: &str) -> Result<(), String> {
        let (key, value) = arg
            .split_once('=')
            .filter(|(_, value)| !value.is_empty())
            .ok_or_else(|| format!("{arg} requires a nonempty value"))?;
        let duplicate = match key {
            "--proof-required" => self.directory.replace(value.into()).is_some(),
            "--proof-checker" => self.checker.replace(value.into()).is_some(),
            "--proof-checker-sha256" => self.checker_sha256.replace(value.into()).is_some(),
            "--proof-timeout-ms" => {
                let value = value.parse().map_err(|_| "invalid --proof-timeout-ms")?;
                self.timeout_ms.replace(value).is_some()
            }
            "--proof-proposal" => {
                let (entry, path) = value
                    .split_once('=')
                    .filter(|(entry, path)| !entry.is_empty() && !path.is_empty())
                    .ok_or("--proof-proposal requires NAME=PATH")?;
                let duplicate = self.proposals.iter().any(|(name, _)| name == entry);
                self.proposals.push((entry.to_owned(), path.into()));
                duplicate
            }
            _ => return Err(format!("unsupported option `{arg}`")),
        };
        if duplicate {
            return Err(format!("{key} may appear only once for each value"));
        }
        Ok(())
    }

    pub(crate) fn admit(self, admission: Admission<'_>) -> Result<Option<ProofOptions>, String> {
        let Some(directory) = self.directory else {
            if self.checker.is_some()
                || self.checker_sha256.is_some()
                || self.timeout_ms.is_some()
                || !self.proposals.is_empty()
            {
                return Err("--proof-* options require --proof-required=NEW_DIR".into());
            }
            return Ok(None);
        };
        if !admission.emit_asm
            || admission.entries.is_empty()
            || !admission.baseline
            || !admission.unchanged_backend
        {
            return Err("--proof-required requires --emit=asm, --core-entry=NAME, -O0 and -fno-builtin, and permits no development pass, backend variant, unchecked guard or EqSat option without -Oeq-scalar".into());
        }
        if admission.rewrites && !admission.rewrite_checker_pinned {
            return Err(
                "-Oeq-scalar in proof-required mode requires --eq-checker-sha256=SHA256".into(),
            );
        }
        if !admission
            .entries
            .iter()
            .all(|entry| crate::is_identifier(entry))
        {
            return Err("--core-entry requires a C identifier in proof-required mode".into());
        }
        if !self
            .proposals
            .iter()
            .all(|(entry, _)| admission.entries.contains(entry))
        {
            return Err("--proof-proposal names a function that is not a --core-entry".into());
        }
        let timeout_ms = self.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
        if !(1..=3_600_000).contains(&timeout_ms) {
            return Err("--proof-timeout-ms must be from 1 to 3600000".into());
        }
        Ok(Some(ProofOptions {
            directory,
            checker: self.checker,
            checker_sha256: self.checker_sha256,
            timeout_ms,
            proposals: self.proposals,
        }))
    }
}

/// One checker input file, below the proof directory.
struct Artifact {
    role: Role,
    /// `None` for a file of the whole module.
    entry: Option<String>,
    /// For the EvoIR and the rewrite roles: the rewrite step. An EvoIR file at
    /// step K comes after K rewrite steps. `None` for a file of every step.
    step: Option<usize>,
    path: String,
    bytes: Vec<u8>,
    sha256: String,
}

impl Artifact {
    fn new(
        role: Role,
        entry: Option<&str>,
        step: Option<usize>,
        path: String,
        bytes: Vec<u8>,
    ) -> Self {
        let sha256 = sha256(&bytes);
        Self {
            role,
            entry: entry.map(str::to_owned),
            step,
            path,
            bytes,
            sha256,
        }
    }

    fn json(&self) -> Json {
        Json::Object(vec![
            ("role", Json::text(self.role.name())),
            ("path", Json::text(&self.path)),
            ("sha256", Json::text(&self.sha256)),
        ])
    }
}

/// The rewrite chain of one `-Oeq-scalar` compilation.
pub(crate) struct Rewrites {
    /// The module before the first rewrite step.
    pub initial: Module,
    /// The proof bundle of each transaction, in chain order. `--eq-proof-dir`
    /// writes the same data.
    pub steps: Vec<ProofBundle>,
    /// The Rust certificate checker that accepted each step.
    pub checker: PathBuf,
    /// The SHA-256 digest of that checker.
    pub checker_sha256: String,
}

/// Compile in proof-required mode. `module` is the module that the backend
/// compiles. `rewrites` is `None` without `-Oeq-scalar`. On success, the
/// assembly goes to standard output after the record exists. On failure,
/// nothing goes to standard output.
pub(crate) fn run(
    options: &DriverOptions,
    proof: &ProofOptions,
    source: &[u8],
    unit: &CHirUnit<'_>,
    module: &Module,
    rewrites: Option<&Rewrites>,
    resolved: &CHirUnit<'_>,
) -> ExitCode {
    let refuse = |(failure, message): Refusal| {
        options.error_format.usage(&format!(
            "proof-required compilation refused: {message}; no output was written"
        ));
        ExitCode::from(failure as u8)
    };
    let (mut artifacts, closures) = match export(options, proof, unit, module, rewrites) {
        Ok(exported) => exported,
        Err(refusal) => return refuse(refusal),
    };
    let assembly = match crate::assemble(options, module, resolved) {
        Ok(assembly) => assembly,
        Err(code) => return code,
    };
    // V04 checks the text that the driver publishes.
    let path = "inputs/assembly.s".into();
    let bytes = assembly.clone().into_bytes();
    artifacts.push(Artifact::new(Role::Text, None, None, path, bytes));
    match check_and_record(
        options, proof, source, &artifacts, closures, rewrites, &assembly,
    ) {
        Ok(record) => {
            if options.error_format == crate::diagnostics::Format::Text {
                eprintln!("evocc: proof-required result: {record}");
            }
            let mut stdout = std::io::stdout().lock();
            if write!(stdout, "{assembly}").is_err() {
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(refusal) => refuse(refusal),
    }
}

/// Export the checker inputs of one compilation, and the call closure of each
/// entry. The output must contain only checked entries.
fn export(
    options: &DriverOptions,
    proof: &ProofOptions,
    unit: &CHirUnit<'_>,
    module: &Module,
    rewrites: Option<&Rewrites>,
) -> Result<(Vec<Artifact>, Vec<Json>), Refusal> {
    let unsupported = |message: String| (Failure::Unsupported, message);
    let target = options.target_layout();
    let mut artifacts = Vec::new();
    let mut closures = Vec::new();
    for entry in &options.core_entries {
        let refused = |error: evocc_chir::core_profile::CoreProfileError| {
            unsupported(format!(
                "entry `{entry}` is outside core-v1: {:?}",
                error.reason
            ))
        };
        let text = crate::core_export::export_core(unit, &target, &[entry]).map_err(refused)?;
        let inventory =
            evocc_chir::core_profile::inspect_core(unit, &target, &[entry]).map_err(refused)?;
        let closure = inventory.functions.iter().map(|f| Json::text(&f.name));
        closures.push(Json::Object(vec![
            ("entry", Json::text(entry)),
            ("closure", Json::List(closure.collect())),
        ]));
        let path = format!("inputs/source-{entry}.json");
        artifacts.push(Artifact::new(
            Role::Source,
            Some(entry),
            None,
            path,
            text.into(),
        ));
    }

    // Every function and object in the output must be a checked entry.
    if !(module.globals.is_empty()
        && module.extern_objects.is_empty()
        && module.tls.is_empty()
        && module.aliases.is_empty()
        && module.lifecycle.is_empty()
        && module.assembly.is_empty())
    {
        return Err(unsupported(
            "the output has a data object, alias, constructor or file-scope assembly outside core-v1".into(),
        ));
    }
    let emitted = evocc_mir::emitted_functions(module);
    if let Some(name) = emitted
        .iter()
        .find(|name| !options.core_entries.contains(name))
    {
        return Err(unsupported(format!(
            "function `{name}` is in the output but is not a checked entry; add --core-entry={name}"
        )));
    }

    // V01 reads the EvoIR before the first rewrite step. Each step is the
    // triple that `--eq-proof-dir` also writes. V02 reads the EvoIR of the
    // module that the backend compiles.
    let (initial, steps) = rewrites.map_or((module, &[][..]), |r| (&r.initial, &r.steps));
    if rewrites.is_some() && steps.is_empty() {
        return Err((Failure::Input, "-Oeq-scalar gave no rewrite step".into()));
    }
    let ir = evocc_evoir::render_module(initial).into_bytes();
    let path = "inputs/evoir.evo".into();
    artifacts.push(Artifact::new(Role::Ir, None, Some(0), path, ir));
    for (step, bundle) in steps.iter().enumerate() {
        for (role, text, suffix) in [
            (Role::Before, &bundle.before, "before.evo"),
            (Role::After, &bundle.after, "after.evo"),
            (Role::Certificate, &bundle.proof, "proof.evp"),
        ] {
            let path = format!("inputs/rewrite-{step:03}.{suffix}");
            let bytes = text.clone().into_bytes();
            artifacts.push(Artifact::new(role, None, Some(step), path, bytes));
        }
    }
    if !steps.is_empty() {
        let ir = evocc_evoir::render_module(module).into_bytes();
        let path = "inputs/evoir-rewritten.evo".into();
        artifacts.push(Artifact::new(Role::Ir, None, Some(steps.len()), path, ir));
    }

    // The same lowering and SSA removal as the ordinary backend, which
    // `admit` restricts to checked guards and no backend variant.
    let ssa = evocc_mir::lower_module_for_profile(
        evocc_mir::Target::Linux,
        module,
        &|name| !emitted.contains(name),
        evocc_mir::UbCheckProfile::Checked,
    )
    .map_err(|refusals| {
        let codes = refusals.iter().map(|r| r.code).collect::<Vec<_>>();
        unsupported(format!("EvoMIR lowering refused: {}", codes.join(",")))
    })?;
    let mut post = ssa.clone();
    for function in &mut post.functions {
        *function = evocc_mir::out_of_ssa(function);
    }
    for (role, program, mode, path) in [
        (
            Role::Ssa,
            &ssa,
            CoreMachineMode::Ssa,
            "inputs/ssa-evomir.json",
        ),
        (
            Role::PostSsa,
            &post,
            CoreMachineMode::PostSsa,
            "inputs/post-ssa-evomir.json",
        ),
    ] {
        let text = export_core_machine(program, mode)
            .map_err(|error| unsupported(format!("machine snapshot refused: {error}")))?;
        artifacts.push(Artifact::new(role, None, None, path.into(), text.into()));
    }
    // The same register allocator as the ordinary backend, with the default
    // options that `admit` requires, on the same post-SSA module.
    let allocation = evocc_mir::verify_alloc::export_core_allocation(&post)
        .map_err(|error| unsupported(format!("allocation snapshot refused: {error}")))?;
    let path = "inputs/allocation.json".into();
    let bytes = allocation.into_bytes();
    artifacts.push(Artifact::new(Role::Allocation, None, None, path, bytes));
    // The assembly AST and the frame hints of the same module, from the same
    // lowering, allocation and frame layout.
    let (ast, _) = evocc_mir::asm_ast::export_core_asm(module)
        .map_err(|error| unsupported(format!("assembly AST snapshot refused: {error}")))?;
    let path = "inputs/asm-ast.json".into();
    artifacts.push(Artifact::new(Role::Asm, None, None, path, ast.into_bytes()));
    for (entry, path) in &proof.proposals {
        let bytes = fs::read(path).map_err(|error| {
            let path = path.display();
            (
                Failure::Input,
                format!("proposal `{path}` is not readable: {error}"),
            )
        })?;
        let path = format!("inputs/proposal-{entry}.json");
        artifacts.push(Artifact::new(
            Role::Proposal,
            Some(entry),
            None,
            path,
            bytes,
        ));
    }
    Ok((artifacts, closures))
}

/// Run every integrated stage for every entry, then write the output and the
/// record. The result is the scope line of the record.
fn check_and_record(
    options: &DriverOptions,
    proof: &ProofOptions,
    source: &[u8],
    artifacts: &[Artifact],
    closures: Vec<Json>,
    rewrites: Option<&Rewrites>,
    assembly: &str,
) -> Result<String, Refusal> {
    let (checker, checker_sha256) = locate_checker(proof)?;
    let steps = artifacts
        .iter()
        .filter(|a| a.role == Role::Certificate)
        .count();
    // The published text. Its digest must be the digest of the text that V04
    // checks; `chain` compares them.
    let published = Artifact::new(
        Role::Text,
        None,
        None,
        "output.s".into(),
        assembly.as_bytes().to_vec(),
    );
    let chain = chain(artifacts, steps, &published)?;
    write_new(&proof.directory, artifacts)?;
    let mut stages = Vec::new();
    for stage in STAGES {
        let runs = match stage.place {
            Place::First => 0..1,
            Place::EachStep => 0..steps,
            Place::Last => steps..steps + 1,
        };
        let (Some(roles), false) = (stage.roles, runs.is_empty()) else {
            let reason = match stage.roles {
                None => "not integrated",
                Some(_) => "no rewrite ran; the compilation has no -Oeq-scalar",
            };
            stages.push(Json::Object(vec![
                ("id", Json::text(stage.id)),
                ("name", Json::text(stage.name)),
                ("status", Json::text("NOT_CHECKED")),
                ("reason", Json::text(reason)),
            ]));
            continue;
        };
        let each = stage.place == Place::EachStep;
        let mut results = Vec::new();
        for entry in &options.core_entries {
            for step in runs.clone() {
                let inputs = roles
                    .iter()
                    .filter_map(|role| {
                        artifacts.iter().find(|a| {
                            a.role == *role
                                && a.entry.as_ref().is_none_or(|name| name == entry)
                                && a.step.is_none_or(|at| at == step)
                        })
                    })
                    .collect::<Vec<_>>();
                let missing = |role: &&Role| !inputs.iter().any(|a| a.role == **role);
                if let Some(role) = roles.iter().filter(|r| **r != Role::Proposal).find(missing) {
                    let role = role.name();
                    return Err((
                        Failure::Input,
                        format!("stage {} has no {role} input", stage.id),
                    ));
                }
                let report = check(&checker, proof, stage, entry, each.then_some(step), &inputs)?;
                let mut result = vec![("entry", Json::text(entry))];
                if each {
                    result.push(("step", Json::Number(step as u64)));
                }
                result.extend([
                    ("result", Json::text("ACCEPTED")),
                    (
                        "inputs",
                        Json::List(inputs.iter().map(|a| a.json()).collect()),
                    ),
                    ("report", Json::text(&report)),
                ]);
                results.push(Json::Object(result));
            }
        }
        stages.push(Json::Object(vec![
            ("id", Json::text(stage.id)),
            ("name", Json::text(stage.name)),
            ("status", Json::text("CHECKED")),
            ("theorem", Json::text(stage.theorem)),
            ("results", Json::List(results)),
        ]));
    }

    // The checker must not change an input that the record names.
    for artifact in artifacts {
        let path = proof.directory.join(&artifact.path);
        if fs::read(&path).map(|bytes| sha256(&bytes)).ok() != Some(artifact.sha256.clone()) {
            let path = path.display();
            return Err((
                Failure::Input,
                format!("proof input `{path}` changed or is missing after the check"),
            ));
        }
    }

    let checked = STAGES.iter().take_while(|stage| stage.roles.is_some());
    let reaches = checked.last().map_or("nothing", |stage| stage.reaches);
    let complete = STAGES.iter().all(|stage| stage.roles.is_some());
    let label = reaches.to_ascii_uppercase().replace(['-', ' '], "_");
    let result = format!("CHECKED_TO_{label}");
    let scope = if complete {
        COMPLETE_SCOPE.to_owned()
    } else {
        format!("checked to {reaches}, not to final assembly")
    };
    let compiler = std::env::current_exe().unwrap_or_default();
    let compiler_sha256 = fs::read(&compiler).map_or_else(|_| String::new(), sha256);
    let file = |path: &str, digest: &str| {
        Json::Object(vec![
            ("path", Json::text(path)),
            ("sha256", Json::text(digest)),
        ])
    };
    let output = vec![
        ("path", Json::text("output.s")),
        ("sha256", Json::text(&published.sha256)),
        ("checked", Json::Flag(complete)),
        ("note", Json::text(OUTPUT_NOTE)),
    ];
    let mut record = vec![
        ("schema", Json::text("evocc-proof-required-record-v1")),
        ("result", Json::text(&result)),
        ("scope", Json::text(&scope)),
        ("all_stages_checked", Json::Flag(complete)),
        ("profile", Json::text("core-v1")),
        ("target", Json::text(crate::SUPPORTED_TARGET)),
        ("source", file(&options.input, &sha256(source))),
        (
            "compiler",
            file(&compiler.display().to_string(), &compiler_sha256),
        ),
        (
            "checker",
            file(&checker.display().to_string(), &checker_sha256),
        ),
        (
            "checker_digest_pinned",
            Json::Flag(proof.checker_sha256.is_some()),
        ),
        ("checker_timeout_ms", Json::Number(proof.timeout_ms)),
    ];
    // The Rust certificate checker that accepted each rewrite step.
    if let Some(rewrites) = rewrites {
        let path = rewrites.checker.display().to_string();
        record.push(("rewrite_checker", file(&path, &rewrites.checker_sha256)));
    }
    record.extend([
        ("entries", Json::List(closures)),
        ("stages", Json::List(stages)),
        ("chain", chain),
        ("output", Json::Object(output)),
        (
            "trusted",
            Json::List(TRUSTED.iter().map(|t| Json::text(t)).collect()),
        ),
    ]);
    let record = Json::Object(record);
    let mut text = String::new();
    record.render(0, &mut text);
    text.push('\n');
    create_new(&proof.directory.join("output.s"), &published.bytes)?;
    create_new(&proof.directory.join("record.json"), text.as_bytes())?;
    let record = proof.directory.join("record.json");
    Ok(format!("{result}: {scope}; record {}", record.display()))
}

/// The files that adjacent stages read, in stage order. V01 reads the EvoIR
/// before step 0. Step K of V05 reads the EvoIR before step K and the EvoIR
/// after step K. V02 reads the EvoIR after the last step and the post-SSA file.
/// V03 reads the same post-SSA file and the allocation export. V04 reads the
/// same allocation export and the assembly text. `published` is the text that
/// the driver publishes as `output.s`; it must be the text that V04 reads. Two
/// adjacent readers must read the same bytes. A different digest is a gap and
/// stops the compilation. The result is one link for each pair of readers.
fn chain(artifacts: &[Artifact], steps: usize, published: &Artifact) -> Result<Json, Refusal> {
    let file = |role: Role, step: Option<usize>, reader: String| match artifacts
        .iter()
        .find(|a| a.role == role && a.step == step)
    {
        Some(artifact) => Ok((artifact, reader)),
        None => {
            let role = role.name();
            Err((
                Failure::Input,
                format!("stage {reader} has no {role} input"),
            ))
        }
    };
    let mut pairs = Vec::new();
    for step in 0..=steps {
        let left = if step == 0 {
            file(Role::Ir, Some(0), "V01".into())?
        } else {
            let reader = format!("V05 step {}", step - 1);
            file(Role::After, Some(step - 1), reader)?
        };
        let right = if step == steps {
            file(Role::Ir, Some(steps), "V02".into())?
        } else {
            file(Role::Before, Some(step), format!("V05 step {step}"))?
        };
        pairs.push((left, right));
    }
    let post_ssa = |reader: &str| file(Role::PostSsa, None, reader.into());
    pairs.push((post_ssa("V02")?, post_ssa("V03")?));
    let allocation = |reader: &str| file(Role::Allocation, None, reader.into());
    pairs.push((allocation("V03")?, allocation("V04")?));
    let checked = file(Role::Text, None, "V04".into())?;
    pairs.push((checked, (published, "publication".into())));
    let mut links = Vec::new();
    for (left, right) in pairs {
        if left.0.sha256 != right.0.sha256 {
            return Err((
                Failure::Input,
                format!(
                    "the stage chain has a gap: `{}` of {} and `{}` of {} have different digests",
                    left.0.path, left.1, right.0.path, right.1
                ),
            ));
        }
        let reader = |(artifact, reader): (&Artifact, String)| {
            Json::Object(vec![
                ("stage", Json::text(&reader)),
                ("role", Json::text(artifact.role.name())),
                ("path", Json::text(&artifact.path)),
            ])
        };
        links.push(Json::Object(vec![
            ("sha256", Json::text(&left.0.sha256)),
            ("read_by", Json::List(vec![reader(left), reader(right)])),
        ]));
    }
    Ok(Json::List(links))
}

/// The checker path and its digest. A pinned digest must match.
fn locate_checker(proof: &ProofOptions) -> Result<(PathBuf, String), Refusal> {
    let missing = |message: String| (Failure::Checker, message);
    let checker = proof
        .checker
        .clone()
        .or_else(|| std::env::var_os("EVOCC_PROOF_CHECKER").map(PathBuf::from))
        .ok_or_else(|| {
            missing("no checker: give --proof-checker=PATH or EVOCC_PROOF_CHECKER".into())
        })?;
    let checker = fs::canonicalize(&checker).unwrap_or(checker);
    let bytes = fs::read(&checker).map_err(|error| {
        missing(format!(
            "checker `{}` is not readable: {error}",
            checker.display()
        ))
    })?;
    let digest = sha256(&bytes);
    if let Some(expected) = &proof.checker_sha256 {
        if !expected.eq_ignore_ascii_case(&digest) {
            return Err(missing(format!(
                "checker digest {digest} is not the pinned digest {expected}"
            )));
        }
    }
    Ok((checker, digest))
}

/// Create the new proof directory and write each input once.
fn write_new(directory: &Path, artifacts: &[Artifact]) -> Result<(), Refusal> {
    let failed = |path: &Path, error: std::io::Error| {
        (
            Failure::Input,
            format!("cannot create `{}`: {error}", path.display()),
        )
    };
    fs::create_dir(directory).map_err(|error| failed(directory, error))?;
    for child in ["inputs", "checks"] {
        let path = directory.join(child);
        fs::create_dir(&path).map_err(|error| failed(&path, error))?;
    }
    for artifact in artifacts {
        create_new(&directory.join(&artifact.path), &artifact.bytes)?;
    }
    Ok(())
}

fn create_new(path: &Path, bytes: &[u8]) -> Result<(), Refusal> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|error| {
            (
                Failure::Input,
                format!("cannot write `{}`: {error}", path.display()),
            )
        })
}

/// Run one stage for one entry, and for one rewrite step of V05, with the time
/// limit. The result is the acceptance line of the checker.
fn check(
    checker: &Path,
    proof: &ProofOptions,
    stage: &Stage,
    entry: &str,
    step: Option<usize>,
    inputs: &[&Artifact],
) -> Result<String, Refusal> {
    let failed = |message: String| (Failure::Checker, message);
    let step_name = step.map_or_else(String::new, |step| format!("-{step:03}"));
    let log = |suffix: &str| {
        proof
            .directory
            .join(format!("checks/{}-{entry}{step_name}.{suffix}", stage.id))
    };
    let open = |path: &Path| {
        fs::File::create(path).map_err(|error| {
            (
                Failure::Input,
                format!("cannot create `{}`: {error}", path.display()),
            )
        })
    };
    let mut command = Command::new(checker);
    command.arg("--check-stage").arg(stage.id);
    for artifact in inputs {
        let mut binding = OsString::from(format!("{}=", artifact.role.name()));
        binding.push(proof.directory.join(&artifact.path));
        command.arg(binding);
    }
    command
        .stdin(Stdio::null())
        .stdout(open(&log("out"))?)
        .stderr(open(&log("err"))?);
    let mut child = command.spawn().map_err(|error| {
        failed(format!(
            "cannot start checker `{}`: {error}",
            checker.display()
        ))
    })?;
    let deadline = Instant::now() + Duration::from_millis(proof.timeout_ms);
    let status: ExitStatus = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failed(match result {
                    Err(error) => format!("cannot wait for the checker: {error}"),
                    _ => format!(
                        "checker timed out after {} ms in stage {} for entry `{entry}`",
                        proof.timeout_ms, stage.id
                    ),
                }));
            }
        }
    };
    let stdout = fs::read_to_string(log("out")).unwrap_or_default();
    let stderr = fs::read_to_string(log("err")).unwrap_or_default();
    let reason = stderr.lines().next().unwrap_or("no reason").to_owned();
    let step_text = step.map_or_else(String::new, |step| format!(" step {step}"));
    let what = format!(
        "stage {} ({}) for entry `{entry}`{step_text}",
        stage.id, stage.name
    );
    match status.code() {
        Some(0) => {
            let expected = format!("STAGE_ACCEPTED stage={} entry={entry}", stage.id);
            match stdout.lines().collect::<Vec<_>>()[..] {
                [line] if line == expected || line.starts_with(&format!("{expected} ")) => {
                    Ok(line.to_owned())
                }
                _ => Err(failed(format!(
                    "checker gave no acceptance line for {what}"
                ))),
            }
        }
        Some(1) => Err((Failure::Rejected, format!("{what} rejected: {reason}"))),
        Some(2) => Err((
            Failure::Input,
            format!("{what} has an input that is not supported or not readable: {reason}"),
        )),
        _ => Err(failed(format!(
            "checker failed in {what} with {status}: {reason}"
        ))),
    }
}

/// A small JSON value for the record. The record has no external dependency.
enum Json {
    Text(String),
    Flag(bool),
    Number(u64),
    List(Vec<Json>),
    Object(Vec<(&'static str, Json)>),
}

impl Json {
    fn text(value: &str) -> Self {
        Self::Text(value.to_owned())
    }

    fn render(&self, depth: usize, out: &mut String) {
        let indent = |out: &mut String, depth: usize| {
            out.push('\n');
            out.push_str(&"  ".repeat(depth));
        };
        match self {
            Self::Text(value) => {
                out.push('"');
                for character in value.chars() {
                    match character {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\u{0}'..='\u{1f}' => {
                            out.push_str(&format!("\\u{:04x}", u32::from(character)))
                        }
                        other => out.push(other),
                    }
                }
                out.push('"');
            }
            Self::Flag(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Number(value) => out.push_str(&value.to_string()),
            Self::List(values) if values.is_empty() => out.push_str("[]"),
            Self::List(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    out.push_str(if index == 0 { "" } else { "," });
                    indent(out, depth + 1);
                    value.render(depth + 1, out);
                }
                indent(out, depth);
                out.push(']');
            }
            Self::Object(fields) => {
                out.push('{');
                for (index, (key, value)) in fields.iter().enumerate() {
                    out.push_str(if index == 0 { "" } else { "," });
                    indent(out, depth + 1);
                    Self::text(key).render(depth + 1, out);
                    out.push_str(": ");
                    value.render(depth + 1, out);
                }
                indent(out, depth);
                out.push('}');
            }
        }
    }
}
