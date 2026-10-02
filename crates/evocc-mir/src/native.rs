//! Section 9, instrument three: the host toolchain in the loop.
//!
//! "The assembled program is run on the host and its output compared with the
//! reference compiler's build of the same C source ... the host toolchain for
//! Darwin, recorded by version."
//!
//! This module is the only place in the crate that runs another program. It
//! keeps the pipeline's stages apart so that a refusal or a disagreement names
//! the component that produced it -- lowering, allocation, emission, the
//! assembler, the linker, or the run -- which is what the native campaign has
//! to report.

use std::path::{Path, PathBuf};
use std::process::Command;

use evocc_evoir::Module;
use evocc_types::EvoIRType;

use crate::isa::aarch64::abi::{ProfileFeature, KERNEL};
use crate::lane::is_register_type;
use crate::{
    allocate_for, emitted_functions, lower_module_for_profile, out_of_ssa, verify_allocation_for,
    RelocationModel, Target,
};

/// Where the native pipeline stopped.
///
/// Each variant names one component, so a campaign row can be read without
/// re-running anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeStop {
    /// Section 5 has no row for some instruction; the codes are Section 10's.
    Lowering(Vec<&'static str>),
    /// Section 7's linear scan did not settle.
    Allocation(String),
    /// Section 9's second instrument refused the allocation (`E4020`).
    Verification(String),
    /// The host assembler refused the emitted text.
    Assembly(String),
    /// The host linker refused the object.
    Link(String),
    /// The program did not exit normally.
    Run(String),
}

impl NativeStop {
    /// The component the campaign names.
    #[must_use]
    pub const fn component(&self) -> &'static str {
        match self {
            Self::Lowering(_) => "lowering",
            Self::Allocation(_) => "allocation",
            Self::Verification(_) => "verification",
            Self::Assembly(_) => "emission",
            Self::Link(_) => "link",
            Self::Run(_) => "run",
        }
    }

    /// What the component said.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::Lowering(codes) => codes.join(","),
            Self::Allocation(text)
            | Self::Verification(text)
            | Self::Assembly(text)
            | Self::Link(text)
            | Self::Run(text) => text.clone(),
        }
    }
}

/// Lower, allocate, verify and emit one EvoIR module as Darwin arm64 assembly.
///
/// Only the functions `keep` admits are emitted, so a module may declare a
/// function the host side of a probe defines.
pub fn compile_module_with(
    module: &Module,
    keep: &dyn Fn(&str) -> bool,
) -> Result<String, NativeStop> {
    compile_module_for(Target::Darwin, module, keep)
}

/// Lower, allocate, verify and emit one EvoIR module for `target`.
pub fn compile_module_for(
    target: Target,
    module: &Module,
    keep: &dyn Fn(&str) -> bool,
) -> Result<String, NativeStop> {
    compile_module_with_relocation(target, module, keep, RelocationModel::PositionIndependent)
}

/// Compile with an explicit address model; allocation and ABI checks are unchanged.
pub fn compile_module_with_relocation(
    target: Target,
    module: &Module,
    keep: &dyn Fn(&str) -> bool,
    relocation: RelocationModel,
) -> Result<String, NativeStop> {
    compile_module_with_options(
        target,
        module,
        keep,
        relocation,
        &crate::emit::CodegenOptions::default(),
    )
}

pub fn compile_module_with_options(
    target: Target,
    module: &Module,
    keep: &dyn Fn(&str) -> bool,
    relocation: RelocationModel,
    options: &crate::emit::CodegenOptions,
) -> Result<String, NativeStop> {
    compile_module_with_guard_control(target, module, keep, relocation, options, None)
}

/// Size-attribution only. Not certificate-admitted, installable or PASS-eligible.
/// Uses the ordinary Linux backend with checked obligations and common DCE.
pub fn compile_module_for_guard_attribution(
    module: &Module,
    true_predicates: &std::collections::BTreeMap<String, std::collections::BTreeSet<u32>>,
) -> Result<String, NativeStop> {
    let assembly = compile_module_with_guard_control(
        Target::Linux,
        module,
        &|_| true,
        RelocationModel::PositionIndependent,
        &crate::emit::CodegenOptions::default(),
        Some(true_predicates),
    )?;
    Ok(format!("// UNVERIFIED_DIAGNOSTIC: NON_INSTALLABLE, NOT_PASS_ELIGIBLE\n{assembly}\n.section .note.evocc.unverified,\"\",%progbits\n.asciz \"UNVERIFIED_DIAGNOSTIC_NON_INSTALLABLE\"\n"))
}

fn compile_module_with_guard_control(
    target: Target,
    module: &Module,
    keep: &dyn Fn(&str) -> bool,
    relocation: RelocationModel,
    options: &crate::emit::CodegenOptions,
    true_predicates: Option<&std::collections::BTreeMap<String, std::collections::BTreeSet<u32>>>,
) -> Result<String, NativeStop> {
    // Linux-only features are the kernel profile's host-only facts.
    let admits = |feature| KERNEL.admits(target, feature);
    if (!admits(ProfileFeature::FunctionSections) && options.function_sections)
        || (!admits(ProfileFeature::DataSections) && options.data_sections)
    {
        return Err(NativeStop::Assembly(
            "individual function/data sections require the Linux ELF target".into(),
        ));
    }
    if !admits(ProfileFeature::ExplicitSections)
        && module.functions.iter().any(|f| f.section.is_some())
    {
        return Err(NativeStop::Assembly(
            "explicit function sections require the Linux ELF target".into(),
        ));
    }
    if !admits(ProfileFeature::Tls) && !module.tls.is_empty() {
        return Err(NativeStop::Assembly(
            "ELF TLS requires the Linux target".into(),
        ));
    }
    if !admits(ProfileFeature::PatchableEntry)
        && module
            .functions
            .iter()
            .any(|f| f.patchable_entry.is_some_and(|(n, _)| n != 0))
    {
        return Err(NativeStop::Assembly(
            "patchable function entries require the Linux ELF target".into(),
        ));
    }
    if !admits(ProfileFeature::ProtectedVisibility)
        && module
            .functions
            .iter()
            .map(|f| f.visibility)
            .chain(module.declares.iter().map(|f| f.visibility))
            .chain(module.globals.iter().map(|g| g.visibility))
            .chain(module.extern_objects.iter().map(|g| g.visibility))
            .chain(module.aliases.iter().map(|a| a.visibility))
            .any(|v| v == evocc_evoir::GlobalVisibility::Protected)
    {
        return Err(NativeStop::Assembly(
            "ELF protected visibility is unsupported by the Darwin target".into(),
        ));
    }
    // Section 8 (revision 0.16) drops an internal function nothing references
    // before emission. It is answered here rather than after lowering because
    // an unreferenced `static inline` stub from a shared header calls
    // functions the configuration does not build, and lowering one would
    // either refuse or leave an undefined symbol in the object.
    let emitted = emitted_functions(module);
    let keep = &move |name: &str| keep(name) && emitted.contains(name);
    let lowered = if let Some(claimed) = true_predicates {
        crate::lower::lower_module_for_guard_attribution(
            target,
            module,
            &|name| !keep(name),
            claimed,
        )
    } else {
        lower_module_for_profile(target, module, &|name| !keep(name), options.ub_checks)
    }
    .map_err(|refusals| {
        let mut codes: Vec<&'static str> = refusals.iter().map(|refusal| refusal.code).collect();
        codes.sort_unstable();
        codes.dedup();
        NativeStop::Lowering(codes)
    })?;
    let mut functions = Vec::new();
    for function in &lowered.functions {
        if !keep(&function.name) {
            continue;
        }
        if options.general_registers_only
            && function.blocks.iter().any(|block| {
                block
                    .args
                    .iter()
                    .copied()
                    .chain(block.insts.iter().flat_map(crate::inst_defs))
                    .chain(block.insts.iter().flat_map(crate::inst_uses))
                    .any(|reg| reg.class.is_simd())
            })
        {
            return Err(NativeStop::Lowering(vec!["E4002"]));
        }
        let destroyed = if options.retain_dead_computations {
            crate::regalloc::out_of_ssa_with_dce(function, false)
        } else {
            out_of_ssa(function)
        };
        let mut allocated = allocate_for(target, &destroyed)
            .map_err(|detail| NativeStop::Allocation(format!("@{}: {detail}", function.name)))?;
        allocated.internal = module.functions.iter().any(|source| {
            source.name == function.name
                && source.visibility == evocc_evoir::GlobalVisibility::Internal
        });
        verify_allocation_for(target, &allocated).map_err(|refusal| {
            NativeStop::Verification(format!(
                "@{}: {} {} in block {}: {}",
                function.name, refusal.code, refusal.name, refusal.block, refusal.detail
            ))
        })?;
        functions.push(allocated);
    }
    let assembly = crate::emit::emit_linked_module(target, &functions, module, relocation, options);
    Ok(format!(
        "// evocc-native-ub-checks={}\n{assembly}",
        options.ub_checks.name()
    ))
}

/// Lower, allocate, verify and emit every function of one EvoIR module.
pub fn compile_module(module: &Module) -> Result<String, NativeStop> {
    compile_module_with(module, &|_| true)
}

/// Why a module is not a program of the Section 9 native corpus.
///
/// "its `main` takes no arguments and returns one `bits<32>`, and its programs
/// are compared by exit status modulo 256".
pub fn native_corpus_entry(module: &Module) -> Result<(), &'static str> {
    let Some(main) = module
        .functions
        .iter()
        .find(|function| function.name == "main")
    else {
        return Err("noEntry");
    };
    if main.params.iter().any(|param| is_register_type(&param.ty)) {
        return Err("entryTakesArguments");
    }
    let results: Vec<&EvoIRType> = main
        .ret_tys
        .iter()
        .filter(|ty| is_register_type(ty))
        .collect();
    match results.as_slice() {
        [EvoIRType::Bits(32)] => Ok(()),
        [_] => Err("entryResultIsNotThirtyTwoBits"),
        _ => Err("entryDoesNotReturnOneResult"),
    }
}

/// The host toolchain, located once and recorded by version.
///
/// Section 9's third instrument compares the assembled program with "the
/// reference compiler's build of the same C source ... the host toolchain for
/// Darwin, recorded by version", so the version line is part of the value and
/// every receipt prints it.
#[derive(Debug, Clone)]
pub struct Toolchain {
    version: String,
    workdir: PathBuf,
}

impl Toolchain {
    /// Find `clang`, record its version line, and open a working directory.
    pub fn locate() -> Result<Self, String> {
        let workdir = std::env::temp_dir().join(format!("evocc-native-{}", std::process::id()));
        Self::locate_in(&workdir)
    }

    /// Use an explicit working directory, for byte-identical object captures
    /// whose assembler input paths must be stable across compiler revisions.
    pub fn locate_in(workdir: &Path) -> Result<Self, String> {
        let output = Command::new("clang")
            .arg("--version")
            .output()
            .map_err(|error| format!("clang is absent: {error}"))?;
        if !output.status.success() {
            return Err("clang did not report a version".to_owned());
        }
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        if !text.contains("arm64") && !text.contains("aarch64") {
            return Err("the host toolchain does not target arm64".to_owned());
        }
        std::fs::create_dir_all(workdir)
            .map_err(|error| format!("no working directory: {error}"))?;
        // A campaign writes and deletes three files per program, and a desktop
        // platform indexes and scans each one as it appears. The marker is
        // advisory and its absence costs only time, so a host that ignores it
        // is not an error here.
        let _ = std::fs::write(workdir.join(".metadata_never_index"), b"");
        Ok(Self {
            version: text.lines().next().unwrap_or_default().to_owned(),
            workdir: workdir.to_owned(),
        })
    }

    /// The version line the receipt records.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Assemble one emitted text into an object file.
    pub fn assemble(&self, name: &str, text: &str) -> Result<PathBuf, NativeStop> {
        let source = self.workdir.join(format!("{name}.s"));
        let object = self.workdir.join(format!("{name}.o"));
        std::fs::write(&source, text)
            .map_err(|error| NativeStop::Assembly(format!("no source file: {error}")))?;
        let mut command = Command::new("clang");
        command.arg("-c").arg("-o").arg(&object).arg(&source);
        let outcome = run(command).map_err(NativeStop::Assembly);
        let _ = std::fs::remove_file(&source);
        outcome?;
        Ok(object)
    }

    /// Link one object file into an executable.
    pub fn link(&self, name: &str, object: &Path) -> Result<PathBuf, NativeStop> {
        let program = self.workdir.join(name);
        let mut command = Command::new("clang");
        command.arg("-o").arg(&program).arg(object);
        let outcome = run(command).map_err(NativeStop::Link);
        let _ = std::fs::remove_file(object);
        outcome?;
        Ok(program)
    }

    /// Run one executable and report the eight bits its exit status carries.
    ///
    /// Section 9: the corpus "programs are compared by exit status modulo 256,
    /// the eight bits a process status carries". A program killed by a signal
    /// has no such status and is a stop, not a status of its own.
    pub fn run(&self, program: &Path) -> Result<u8, NativeStop> {
        let outcome = Command::new(program)
            .output()
            .map_err(|error| NativeStop::Run(error.to_string()));
        let _ = std::fs::remove_file(program);
        let outcome = outcome?;
        let code = outcome.status.code().ok_or_else(|| {
            NativeStop::Run(format!("{} did not exit normally", program.display()))
        })?;
        u8::try_from(code & 0xff).map_err(|error| NativeStop::Run(error.to_string()))
    }

    /// Assemble, link and run one emitted text, removing every file it made.
    pub fn exit_status(&self, name: &str, text: &str) -> Result<u8, NativeStop> {
        let object = self.assemble(name, text)?;
        let program = self.link(name, &object)?;
        self.run(&program)
    }
}

/// Run `command`, returning its standard error when it fails.
fn run(mut command: Command) -> Result<(), String> {
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&output.stderr).into_owned())
}

/// The EvoCC driver binary, which is what a user runs.
///
/// Section 9's third instrument compares an assembled program with "the
/// reference compiler's build of the same C source", so the C source has to
/// reach EvoIR first. The front end is another crate's business; this is the
/// same boundary a user crosses, the command line.
#[derive(Debug, Clone)]
pub struct Driver {
    binary: PathBuf,
    checker_digest: Option<String>,
}

impl Driver {
    /// Find `evocc` next to the running test or binary.
    ///
    /// Prefer the current profile. A retained release executable can belong to
    /// an older compiler/text revision and must not contaminate a debug gate.
    pub fn locate() -> Result<Self, String> {
        let mut directory =
            std::env::current_exe().map_err(|error| format!("current executable: {error}"))?;
        directory.pop();
        if directory.ends_with("deps") {
            directory.pop();
        }
        let profile = directory.clone();
        let root = profile.parent().map(Path::to_path_buf);
        let name = format!("evocc{}", std::env::consts::EXE_SUFFIX);
        let candidates = std::iter::once(profile.join(&name)).chain(
            root.into_iter()
                .map(|root| root.join("release").join(&name)),
        );
        for candidate in candidates {
            if candidate.is_file() {
                return Ok(Self {
                    binary: candidate,
                    checker_digest: None,
                });
            }
        }
        Err(format!(
            "no {name} beside {}; build the workspace so the driver is present",
            directory.display()
        ))
    }

    /// The path the receipt records.
    #[must_use]
    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// Supply a caller-selected trusted build/release checker identity. Never
    /// infer trust from whichever companion happens to be beside the driver.
    #[must_use]
    pub fn with_checker_digest(mut self, digest: String) -> Self {
        self.checker_digest = Some(digest);
        self
    }

    /// Compile one C source to canonical EvoIR text, or report the driver's
    /// first diagnostic.
    pub fn emit_evoir(&self, source: &Path, optimize: bool) -> Result<String, String> {
        let mut command = Command::new(&self.binary);
        command.arg("--emit=evoir");
        if optimize {
            command.arg("-Oeq-scalar");
            if let Some(digest) = &self.checker_digest {
                command.arg(format!("--eq-checker-sha256={digest}"));
            }
        }
        command.arg(source);
        let output = command
            .output()
            .map_err(|error| format!("{}: {error}", self.binary.display()))?;
        if !output.status.success() {
            let complaint = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(if complaint.trim().is_empty() {
                format!("the driver exited {}", output.status)
            } else {
                complaint
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}
