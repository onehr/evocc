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
    _module: &Module,
    _keep: &dyn Fn(&str) -> bool,
) -> Result<String, NativeStop> {
    todo!("Sections 5 to 8 as one pipeline")
}

/// Lower, allocate, verify and emit every function of one EvoIR module.
pub fn compile_module(module: &Module) -> Result<String, NativeStop> {
    compile_module_with(module, &|_| true)
}

/// Why a module is not a program of the Section 9 native corpus.
///
/// "its `main` takes no arguments and returns one `bits<32>`, and its programs
/// are compared by exit status modulo 256".
pub fn native_corpus_entry(_module: &Module) -> Result<(), &'static str> {
    todo!("the Section 9 native corpus entry shape")
}

/// The host toolchain, located once and recorded by version.
#[derive(Debug, Clone)]
pub struct Toolchain {
    _version: String,
    _workdir: PathBuf,
}

impl Toolchain {
    /// Find `clang`, record its version line, and open a working directory.
    pub fn locate() -> Result<Self, String> {
        todo!("the recorded host toolchain")
    }

    /// The version line the receipt records.
    #[must_use]
    pub fn version(&self) -> &str {
        &self._version
    }

    /// Assemble one emitted text into an object file.
    pub fn assemble(&self, _name: &str, _text: &str) -> Result<PathBuf, NativeStop> {
        todo!("the host assembler")
    }

    /// Link one object file into an executable.
    pub fn link(&self, _name: &str, _object: &Path) -> Result<PathBuf, NativeStop> {
        todo!("the host linker")
    }

    /// Run one executable and report the eight bits its exit status carries.
    pub fn run(&self, _program: &Path) -> Result<u8, NativeStop> {
        todo!("the native run")
    }

    /// Assemble, link and run one emitted text, removing every file it made.
    pub fn exit_status(&self, _name: &str, _text: &str) -> Result<u8, NativeStop> {
        todo!("the native run")
    }
}

/// Run `command`, returning its standard error when it fails.
fn _output(mut command: Command) -> Result<(), String> {
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&output.stderr).into_owned())
}
