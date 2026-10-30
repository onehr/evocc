//! Command-line option parsing and admission.

use super::*;

struct OptionParser {
    mode: Option<DriverMode>,
    lints: lints::Levels,
    repair_mode: Option<String>,
    error_format: Option<diagnostics::Format>,
    color: diagnostics::Color,
    target: Option<DriverTarget>,
    input: Option<String>,
    source_from_stdin: bool,
    include_paths: Vec<SearchPath>,
    system_include_paths: Vec<SearchPath>,
    framework_options: Vec<String>,
    forced_includes: Vec<String>,
    macro_prefix_maps: Vec<(String, String)>,
    macro_actions: Vec<MacroAction>,
    standard_includes: bool,
    core_entries: Vec<String>,
    proof_arguments: proof_required::ProofArguments,
    optimize_scalar_evoir: bool,
    opt_passes: Vec<String>,
    pass_report: Option<std::path::PathBuf>,
    eq_policy: ExtractionPolicy,
    eq_checker: Option<std::path::PathBuf>,
    eq_checker_digest: Option<String>,
    eq_cost_selected: bool,
    eq_proof_dir: Option<std::path::PathBuf>,
    eq_timeout_ms: u64,
    eq_options: bool,
    optimization: u8,
    debug_types: bool,
    signed_wrapping: bool,
    dynamic_rounding: bool,
    trapping_math: bool,
    standard_builtins: bool,
    disabled_builtins: std::collections::BTreeSet<String>,
    optional_inlining: bool,
    hosted: bool,
    pthread: bool,
    automatic_zero: bool,
    tls_model: evocc_types::TlsModel,
    stack_protector: bool,
    patchable_entry: Option<(u16, u16)>,
    minimum_function_alignment: u32,
    codegen: CodegenOptions,
    ub_checks_selected: bool,
    frame_variants_selected: bool,
    selection_variants_selected: bool,
    allocation_variants_selected: bool,
    guard_is_sysreg: bool,
    guard_register: Option<String>,
    guard_offset: Option<i64>,
    language: LanguageMode,
    gnu_inline: Option<bool>,
    short_wchar: bool,
    signed_char: Option<bool>,
    position_independence: Option<PositionIndependence>,
    survey: bool,
    deployment_option: Option<String>,
    system_root: Option<String>,
    ignored_options: Vec<String>,
    index: usize,
    defined_functions: bool,
    program_arguments: Option<Vec<Vec<u8>>>,
}

impl OptionParser {
    fn new() -> Self {
        Self {
            mode: None,
            lints: lints::Levels::default(),
            repair_mode: None,
            error_format: None,
            color: diagnostics::Color::Auto,
            target: None,
            input: None,
            source_from_stdin: false,
            include_paths: Vec::new(),
            system_include_paths: Vec::new(),
            framework_options: Vec::new(),
            forced_includes: Vec::new(),
            macro_prefix_maps: Vec::new(),
            macro_actions: Vec::new(),
            standard_includes: true,
            core_entries: Vec::new(),
            proof_arguments: proof_required::ProofArguments::default(),
            optimize_scalar_evoir: false,
            opt_passes: Vec::new(),
            pass_report: None,
            eq_policy: ExtractionPolicy::default(),
            eq_checker: None,
            eq_checker_digest: None,
            eq_cost_selected: false,
            eq_proof_dir: None,
            eq_timeout_ms: 30_000,
            eq_options: false,
            optimization: 0,
            debug_types: false,
            signed_wrapping: false,
            dynamic_rounding: false,
            trapping_math: true,
            standard_builtins: true,
            disabled_builtins: std::collections::BTreeSet::new(),
            optional_inlining: true,
            hosted: true,
            pthread: false,
            automatic_zero: false,
            tls_model: evocc_types::TlsModel::default(),
            stack_protector: false,
            patchable_entry: None,
            minimum_function_alignment: 4,
            codegen: CodegenOptions::default(),
            ub_checks_selected: false,
            frame_variants_selected: false,
            selection_variants_selected: false,
            allocation_variants_selected: false,
            guard_is_sysreg: false,
            guard_register: None,
            guard_offset: None,
            language: LanguageMode::Gnu11,
            gnu_inline: None,
            short_wchar: false,
            signed_char: None,
            position_independence: None,
            survey: false,
            deployment_option: None,
            system_root: None,
            ignored_options: Vec::new(),
            index: 0,
            defined_functions: false,
            program_arguments: None,
        }
    }

    fn parse_argument(&mut self, args: &[String], lenient: bool) -> Result<(), String> {
        if self.parse_actions(args)? {
            return Ok(());
        }
        if self.parse_pipeline_options(args)? {
            return Ok(());
        }
        if self.parse_equality_options(args)? {
            return Ok(());
        }
        if self.parse_codegen_options(args)? {
            return Ok(());
        }
        if self.parse_runtime_options(args, lenient)? {
            return Ok(());
        }
        if self.parse_machine_options(args)? {
            return Ok(());
        }
        if self.parse_include_options(args)? {
            return Ok(());
        }
        if self.parse_language_options(args)? {
            return Ok(());
        }
        self.parse_remaining(args, lenient)
    }

    fn parse_actions(&mut self, args: &[String]) -> Result<bool, String> {
        let arg = &args[self.index];
        match arg.as_str() {
            "-E" => set_mode(&mut self.mode, DriverMode::Preprocess)?,
            "--parse-only" => set_mode(&mut self.mode, DriverMode::ParseOnly)?,
            "--source-from-stdin" => self.source_from_stdin = true,
            _ if arg.starts_with("--repair=") => {
                let value = &arg[9..];
                if !matches!(value, "plan" | "safe" | "review")
                    || self.repair_mode.replace(value.into()).is_some()
                {
                    return Err("--repair requires one of plan, safe or review, once".into());
                }
            }
            "--interpret-c-hir" => set_mode(&mut self.mode, DriverMode::InterpretCHir)?,
            "--list-defined-functions" => {
                set_mode(&mut self.mode, DriverMode::ParseOnly)?;
                self.defined_functions = true;
            }
            _ if arg.starts_with("--program-arg-hex=") => {
                let hex = &arg[18..];
                if !hex.len().is_multiple_of(2) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("program argument must contain pairs of hexadecimal digits".into());
                }
                let bytes = (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect::<Vec<_>>();
                if bytes.contains(&0) {
                    return Err("program argument cannot contain a null byte".into());
                }
                self.program_arguments
                    .get_or_insert_with(Vec::new)
                    .push(bytes);
            }
            "--emit=c-hir" => set_mode(&mut self.mode, DriverMode::EmitCHir)?,
            "--emit=core-c-hir" => set_mode(&mut self.mode, DriverMode::EmitCoreCHir)?,
            _ if arg.starts_with("--core-entry=") => {
                let name = arg.strip_prefix("--core-entry=").unwrap();
                if name.is_empty() || self.core_entries.iter().any(|entry| entry == name) {
                    return Err(
                        "--core-entry requires a nonempty, nonduplicate function name".into(),
                    );
                }
                self.core_entries.push(name.to_owned());
            }
            _ if arg.starts_with("--proof-") => self.proof_arguments.parse(arg)?,
            "--emit=evoir" => set_mode(&mut self.mode, DriverMode::EmitEvoIr)?,
            "--emit=asm" => set_mode(&mut self.mode, DriverMode::EmitAsm)?,
            "--interpret-evoir" => set_mode(&mut self.mode, DriverMode::InterpretEvoIr)?,
            "--check" => set_mode(&mut self.mode, DriverMode::Check)?,
            _ if arg.starts_with("--color=") => {
                self.color = diagnostics::Color::parse(arg.strip_prefix("--color=").unwrap())?;
            }
            _ if arg.starts_with("--error-format=") => {
                let selected =
                    diagnostics::Format::parse(arg.strip_prefix("--error-format=").unwrap())?;
                if self.error_format.replace(selected).is_some() {
                    return Err("--error-format may appear only once".into());
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn parse_pipeline_options(&mut self, args: &[String]) -> Result<bool, String> {
        let arg = &args[self.index];
        match arg.as_str() {
            "-Oeq-scalar" => self.optimize_scalar_evoir = true,
            _ if arg.starts_with("--opt-pass=") => {
                let name = arg.strip_prefix("--opt-pass=").unwrap();
                if name != "identity" {
                    return Err(format!(
                        "unknown development pass `{name}`; supported: identity"
                    ));
                }
                self.opt_passes.push(name.to_owned());
            }
            "--pass-report" => {
                self.index += 1;
                let path = args
                    .get(self.index)
                    .ok_or("--pass-report requires a path")?;
                if path.is_empty()
                    || self
                        .pass_report
                        .replace(std::path::PathBuf::from(path))
                        .is_some()
                {
                    return Err("--pass-report requires one nonempty path".into());
                }
            }
            _ if arg.starts_with("--pass-report=") => {
                let path = arg.strip_prefix("--pass-report=").unwrap();
                if path.is_empty()
                    || self
                        .pass_report
                        .replace(std::path::PathBuf::from(path))
                        .is_some()
                {
                    return Err("--pass-report requires one nonempty path".into());
                }
            }
            _ if arg.starts_with("--ub-checks=") => {
                if self.ub_checks_selected {
                    return Err("--ub-checks may appear only once".into());
                }
                self.ub_checks_selected = true;
                self.codegen.ub_checks =
                    evocc_mir::UbCheckProfile::parse(arg.strip_prefix("--ub-checks=").unwrap())?;
            }
            // EvoMIR revision 0.56: opt-in, emission-only frame variants.
            _ if arg.starts_with("--frame-opt=") => {
                if self.frame_variants_selected {
                    return Err("--frame-opt may appear only once".into());
                }
                self.frame_variants_selected = true;
                self.codegen.frame = evocc_mir::emit::FrameVariants::parse(
                    arg.strip_prefix("--frame-opt=").unwrap(),
                )?;
            }
            _ if arg.starts_with("--isel-opt=") => {
                if self.selection_variants_selected {
                    return Err("--isel-opt may appear only once".into());
                }
                self.selection_variants_selected = true;
                let selected = evocc_mir::emit::SelectionVariants::parse(
                    arg.strip_prefix("--isel-opt=").unwrap(),
                )?;
                self.codegen.compact_constants = selected.constants;
                self.codegen.local_selection = selected.local;
                self.codegen.reuse_extensions = selected.extensions;
                self.codegen.select_controls = selected.controls;
            }
            _ if arg.starts_with("--regalloc-opt=") => {
                if self.allocation_variants_selected {
                    return Err("--regalloc-opt may appear only once".into());
                }
                self.allocation_variants_selected = true;
                self.codegen.allocation = evocc_mir::generic::allocation::AllocationOptions::parse(
                    arg.strip_prefix("--regalloc-opt=").unwrap(),
                )?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn parse_equality_options(&mut self, args: &[String]) -> Result<bool, String> {
        let arg = &args[self.index];
        match arg.as_str() {
            _ if arg.starts_with("--eq-") => {
                let (key, value) = arg
                    .split_once('=')
                    .ok_or("options for EqSat require --eq-KEY=VALUE")?;
                if value.is_empty() {
                    return Err("empty EqSat option".into());
                }
                self.eq_options |= key != "--eq-proof-dir";
                match key {
                    "--eq-policy" => {
                        if self.eq_cost_selected {
                            return Err("--eq-policy and --eq-weights are exclusive and may appear only once".into());
                        }
                        self.eq_cost_selected = true;
                        let selected =
                            evocc_eqopt::policy_by_name(value).ok_or("unknown EqSat policy")?;
                        self.eq_policy = selected.with_schedule(self.eq_policy.schedule());
                    }
                    "--eq-weights" => {
                        if self.eq_cost_selected {
                            return Err("--eq-policy and --eq-weights are exclusive and may appear only once".into());
                        }
                        self.eq_cost_selected = true;
                        self.eq_policy =
                            self.eq_policy.with_cost(evocc_eqopt::CostFunction::Vector(
                                evocc_eqopt::OperatorWeights::parse(value)?,
                            ))
                    }
                    "--eq-checker" => self.eq_checker = Some(std::path::PathBuf::from(value)),
                    "--eq-checker-sha256" => self.eq_checker_digest = Some(value.to_owned()),
                    "--eq-proof-dir" => self.eq_proof_dir = Some(std::path::PathBuf::from(value)),
                    "--eq-checker-timeout-ms" => {
                        self.eq_timeout_ms = value.parse().map_err(|_| "invalid checker timeout")?
                    }
                    "--eq-max-nodes" | "--eq-max-epochs" | "--eq-max-transactions" => {
                        let number = value.parse().map_err(|_| "invalid EqSat resource limit")?;
                        let mut schedule = self.eq_policy.schedule();
                        match key {
                            "--eq-max-nodes" => schedule.max_nodes = number,
                            "--eq-max-epochs" => schedule.max_epochs = number,
                            _ => schedule.max_transactions = number,
                        }
                        self.eq_policy = self.eq_policy.with_schedule(schedule);
                    }
                    _ => return Err(format!("unknown EqSat option {key}")),
                }
            }
            "-O0" => self.optimization = 0,
            "-O" | "-O1" | "-O2" | "-O3" | "-Og" => self.optimization = 1,
            "-Os" => self.optimization = 2,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn parse_codegen_options(&mut self, args: &[String]) -> Result<bool, String> {
        let arg = &args[self.index];
        match arg.as_str() {
            "-g" | "-gdwarf-4" => self.debug_types = true,
            "-g0" => self.debug_types = false,
            "-mcmodel=tiny" => self.codegen.tiny_addresses = true,
            "-mcmodel=small" => self.codegen.tiny_addresses = false,
            "-ffunction-sections" => self.codegen.function_sections = true,
            "-fno-function-sections" => self.codegen.function_sections = false,
            "-fdata-sections" => self.codegen.data_sections = true,
            "-fno-data-sections" => self.codegen.data_sections = false,
            "-fasynchronous-unwind-tables" | "-funwind-tables" => self.codegen.unwind_tables = true,
            "-fno-asynchronous-unwind-tables" | "-fno-unwind-tables" => {
                self.codegen.unwind_tables = false
            }
            "-fwrapv" | "-fno-strict-overflow" => self.signed_wrapping = true,
            "-fno-wrapv" | "-fstrict-overflow" => self.signed_wrapping = false,
            // No pass derives non-aliasing from the C access type. Existing
            // object/provenance, initializedness and permission checks remain.
            "-fno-strict-aliasing" => {}
            // Omission is permission, not a request to enable a frame variant.
            "-fomit-frame-pointer" => self.codegen.frame_pointer_required = false,
            "-fno-omit-frame-pointer" => self.codegen.frame_pointer_required = true,
            "-mno-omit-leaf-frame-pointer" => self.codegen.leaf_frame_pointer_required = true,
            // No sibling-call transform exists in this backend.
            "-fno-optimize-sibling-calls" => {}
            // EvoMIR 8.2 emits each tentative definition as a zero-filled
            // definition, never as a common symbol. `-fcommon` stays
            // unsupported.
            "-fno-common" => {}
            // No automatic vectorizer exists either, so these disabling
            // options select the existing contract on both targets. Explicit
            // vector operations stay as written.
            "-fno-tree-vectorize" | "-fno-tree-loop-vectorize" | "-fno-tree-slp-vectorize" => {}
            // These options only permit transforms that EvoCC never makes, so
            // its code meets each of them on both targets. `-ftrapping-math`
            // is the default; the last trapping option selects the macro.
            "-fexpensive-optimizations" | "-ftree-vectorize" | "-fno-vect-cost-model" => {}
            "-ftrapping-math" => self.trapping_math = true,
            "-fno-trapping-math" => self.trapping_math = false,
            // x18 is reserved and never assigned on either target (EvoMIR
            // Section 2.5), so generated code already keeps it fixed. Other
            // `-ffixed-` registers stay unsupported.
            "-ffixed-x18" => {}
            // There is no optional function/branch padding, automatic array
            // prefetch, loop-header copying or loop-to-libcall transform.
            // These disabling options select that existing contract; source
            // and minimum function alignment remain mandatory. Do not admit
            // the positive transforms or unrelated GCC tuning options here.
            "-fno-align-functions"
            | "-fno-align-jumps"
            | "-fno-align-labels"
            | "-fno-align-loops"
            | "-fno-prefetch-loop-arrays"
            | "-fno-tree-ch"
            | "-fno-tree-loop-distribute-patterns" => {}
            // Bounded GCC tuning compatibility (EvoMIR Section 6): allocation
            // already spans each whole function; no code-hoisting pass exists;
            // block placement never duplicates blocks for a trace cache. These
            // select existing behavior, not GCC's coloring/layout heuristics.
            // Do not generalize to loop regions, hoisting or STC transforms.
            "-fira-region=one" | "-fira-hoist-pressure" | "-freorder-blocks-algorithm=simple" => {}
            "-ffp-contract=off" | "-fno-fast-math" => {}
            // FLT_EVAL_METHOD=0 already rounds every arithmetic operation,
            // assignment and cast to its semantic source format. Preserve
            // that contract without changing rounding/environment selection.
            "-fexcess-precision=standard" => {}
            "-frounding-math" => self.dynamic_rounding = true,
            "-fno-rounding-math" => self.dynamic_rounding = false,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn parse_runtime_options(&mut self, args: &[String], lenient: bool) -> Result<bool, String> {
        let arg = &args[self.index];
        match arg.as_str() {
            // These options permit transforms that this compiler never makes,
            // so the cc command line can ignore them without a wrong result.
            "-ffast-math" | "-ffp-contract=fast" | "-ffp-contract=on" if lenient => {
                self.ignored_options.push(arg.clone());
            }
            "-ffast-math" | "-ffp-contract=fast" | "-ffp-contract=on" => {
                return Err(format!("unsupported floating-point profile option `{arg}`"));
            }
            "-fno-builtin" => self.standard_builtins = false,
            "-fbuiltin" => self.standard_builtins = true,
            // GCC 14 keeps a named builtin disabled after `-fbuiltin`, and an
            // unrecognized name has no effect. An empty name stays unsupported.
            _ if arg.len() > "-fno-builtin-".len() && arg.starts_with("-fno-builtin-") => {
                self.disabled_builtins
                    .insert(arg["-fno-builtin-".len()..].to_owned());
            }
            // EvoIR 10.1.9: only `always_inline` callees are substituted, as
            // in GCC 14. The positive `-finline` stays unsupported.
            "-fno-inline" => self.optional_inlining = false,
            // The source is read as UTF-8. GCC 14 accepts these spellings in
            // any letter case, with or without the hyphen. Every other
            // charset stays unsupported.
            _ if arg.strip_prefix("-finput-charset=").is_some_and(|charset| {
                charset.eq_ignore_ascii_case("utf-8") || charset.eq_ignore_ascii_case("utf8")
            }) => {}
            "-ffreestanding" => {
                self.hosted = false;
                self.standard_builtins = false;
            }
            "-fhosted" => {
                self.hosted = true;
                self.standard_builtins = true;
            }
            "-pthread" => self.pthread = true,
            "-ftrivial-auto-var-init=zero" => self.automatic_zero = true,
            "-ftrivial-auto-var-init=uninitialized" => self.automatic_zero = false,
            _ if arg.starts_with("-ftls-model=") => {
                self.tls_model = evocc_types::TlsModel::parse(&arg[12..])
                    .ok_or_else(|| format!("unsupported TLS model `{arg}`"))?;
            }
            // The basic spelling selects the existing strong policy. This
            // protects at least the requested functions; it is not a no-op.
            "-fstack-protector" | "-fstack-protector-strong" => self.stack_protector = true,
            "-fno-stack-protector" => self.stack_protector = false,
            _ if arg.starts_with("-fmin-function-alignment=") => {
                let value = &arg["-fmin-function-alignment=".len()..];
                self.minimum_function_alignment = value
                    .bytes()
                    .all(|b| b.is_ascii_digit())
                    .then(|| value.parse::<u32>().ok())
                    .flatten()
                    .filter(|value| *value <= 65536 && (*value == 0 || value.is_power_of_two()))
                    .map(|value| value.max(4))
                    .ok_or_else(|| format!("unsupported minimum function alignment `{value}`"))?;
            }
            _ if arg.starts_with("-fpatchable-function-entry=") => {
                let value = &arg["-fpatchable-function-entry=".len()..];
                let (total, before) = value.split_once(',').unwrap_or((value, "0"));
                let number = |text: &str| {
                    text.bytes()
                        .all(|b| b.is_ascii_digit())
                        .then(|| text.parse::<u16>().ok())
                        .flatten()
                };
                let pair = number(total).zip(number(before)).filter(|(n, m)| m <= n);
                self.patchable_entry = Some(
                    pair.ok_or_else(|| format!("invalid patchable function entry `{value}`"))?,
                );
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn parse_machine_options(&mut self, args: &[String]) -> Result<bool, String> {
        let arg = &args[self.index];
        match arg.as_str() {
            // Share the effective signing scope with native emission and the
            // ACLE macro. BTI and the B key remain outside this profile.
            "-mbranch-protection=pac-ret" => {
                self.codegen.sign_returns = true;
                self.codegen.sign_leaf_returns = false;
            }
            "-mbranch-protection=pac-ret+leaf" => {
                self.codegen.sign_returns = true;
                self.codegen.sign_leaf_returns = true;
            }
            "-mbranch-protection=none" => {
                self.codegen.sign_returns = false;
                self.codegen.sign_leaf_returns = false;
            }
            "-mgeneral-regs-only" => self.codegen.general_registers_only = true,
            "-mno-general-regs-only" => self.codegen.general_registers_only = false,
            "-mstrict-align" => self.codegen.strict_alignment = true,
            "-mno-strict-align" => self.codegen.strict_alignment = false,
            "-mstack-protector-guard=sysreg" => self.guard_is_sysreg = true,
            "-mstack-protector-guard=global" => self.guard_is_sysreg = false,
            _ if arg.starts_with("-mstack-protector-guard-reg=") => {
                let register = arg.trim_start_matches("-mstack-protector-guard-reg=");
                if !matches!(
                    register,
                    "sp_el0" | "tpidr_el0" | "tpidrro_el0" | "tpidr_el1" | "tpidr_el2"
                ) {
                    return Err(format!("unsupported stack guard register `{register}`"));
                }
                self.guard_register = Some(register.to_owned());
            }
            _ if arg.starts_with("-mstack-protector-guard-offset=") => {
                self.guard_offset = Some(
                    arg.trim_start_matches("-mstack-protector-guard-offset=")
                        .parse::<i64>()
                        .map_err(|_| format!("invalid stack guard offset `{arg}`"))?,
                );
            }
            // Apple clang's deployment target options; the last one wins.
            _ if arg.starts_with("-mmacosx-version-min=")
                || arg.starts_with("-mmacos-version-min=") =>
            {
                self.deployment_option = Some(arg.clone());
            }
            "-fshort-wchar" => self.short_wchar = true,
            "-fno-short-wchar" => self.short_wchar = false,
            "-fsigned-char" | "-fno-unsigned-char" => self.signed_char = Some(true),
            "-funsigned-char" | "-fno-signed-char" => self.signed_char = Some(false),
            "-fpic" => self.position_independence = Some(PositionIndependence::Pic(1)),
            "-fPIC" => self.position_independence = Some(PositionIndependence::Pic(2)),
            "-fpie" => self.position_independence = Some(PositionIndependence::Pie(1)),
            "-fPIE" => self.position_independence = Some(PositionIndependence::Pie(2)),
            "-fno-pic" | "-fno-PIC" | "-fno-pie" | "-fno-PIE" => {
                self.position_independence = Some(PositionIndependence::Disabled);
            }
            "--target" => {
                self.index += 1;
                let value = args
                    .get(self.index)
                    .ok_or_else(|| "--target requires an argument".to_string())?;
                set_target(&mut self.target, parse_target(value)?)?;
            }
            _ if arg.starts_with("--target=") => {
                let value = &arg["--target=".len()..];
                set_target(&mut self.target, parse_target(value)?)?;
            }
            "--survey" => self.survey = true,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn parse_include_options(&mut self, args: &[String]) -> Result<bool, String> {
        let arg = &args[self.index];
        match arg.as_str() {
            "--sysroot" | "-isysroot" => {
                self.index += 1;
                let value = args
                    .get(self.index)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("{arg} requires an argument"))?;
                self.system_root = Some(value.clone());
            }
            _ if arg.starts_with("--sysroot=") => {
                let value = &arg["--sysroot=".len()..];
                if value.is_empty() {
                    return Err("--sysroot requires a directory".into());
                }
                self.system_root = Some(value.to_owned());
            }
            "-P" | "-undef" => {}
            "-nostdinc" => self.standard_includes = false,
            "-I" | "-D" | "-U" | "-include" | "-isystem" | "-F" | "-iframework" => {
                self.index += 1;
                let value = args
                    .get(self.index)
                    .ok_or_else(|| format!("{arg} requires an argument"))?
                    .clone();
                match arg.as_str() {
                    "-I" => self.include_paths.push(SearchPath::normal(value)),
                    "-isystem" => self.system_include_paths.push(SearchPath::normal(value)),
                    "-F" => {
                        self.framework_options.push(arg.clone());
                        self.include_paths.push(SearchPath::framework(value));
                    }
                    "-iframework" => {
                        self.framework_options.push(arg.clone());
                        self.system_include_paths.push(SearchPath::framework(value));
                    }
                    "-D" => self.macro_actions.push(MacroAction::Define(value)),
                    "-U" => self.macro_actions.push(MacroAction::Undefine(value)),
                    "-include" => self.forced_includes.push(value),
                    _ => unreachable!("matched driver option"),
                }
            }
            _ if arg.starts_with("-I") && arg.len() > 2 => {
                self.include_paths.push(SearchPath::normal(&arg[2..]));
            }
            _ if arg.starts_with("-isystem") && arg.len() > "-isystem".len() => {
                self.system_include_paths
                    .push(SearchPath::normal(&arg["-isystem".len()..]));
            }
            _ if arg.starts_with("-iframework")
                && arg.len() > "-iframework".len()
                && !arg.starts_with("-iframeworkwithsysroot") =>
            {
                self.framework_options.push(arg.clone());
                self.system_include_paths
                    .push(SearchPath::framework(&arg["-iframework".len()..]));
            }
            _ if arg.starts_with("-F") && arg.len() > 2 => {
                self.framework_options.push(arg.clone());
                self.include_paths.push(SearchPath::framework(&arg[2..]));
            }
            _ if arg.starts_with("-D") && arg.len() > 2 => {
                self.macro_actions
                    .push(MacroAction::Define(arg[2..].to_owned()));
            }
            _ if arg.starts_with("-U") && arg.len() > 2 => {
                self.macro_actions
                    .push(MacroAction::Undefine(arg[2..].to_owned()));
            }
            _ if arg.starts_with("-fmacro-prefix-map=") => {
                let payload = &arg["-fmacro-prefix-map=".len()..];
                let Some((old, new)) = payload.split_once('=') else {
                    return Err(format!(
                        "invalid -fmacro-prefix-map `{payload}`; expected non-empty OLD in OLD=NEW"
                    ));
                };
                if old.is_empty() {
                    return Err(format!(
                        "invalid -fmacro-prefix-map `{payload}`; expected non-empty OLD in OLD=NEW"
                    ));
                }
                self.macro_prefix_maps
                    .push((old.to_owned(), new.to_owned()));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn parse_language_options(&mut self, args: &[String]) -> Result<bool, String> {
        let arg = &args[self.index];
        match arg.as_str() {
            "-std=gnu11" => self.language = LanguageMode::Gnu11,
            "-std=gnu99" | "-std=gnu9x" => self.language = LanguageMode::Gnu99,
            "-std=c89" | "-std=c90" | "-ansi" | "-std=iso9899:1990" => {
                self.language = LanguageMode::C89
            }
            "-std=c99" | "-std=c9x" | "-std=iso9899:1999" => self.language = LanguageMode::C99,
            "-std=c11" | "-std=c1x" | "-std=iso9899:2011" => self.language = LanguageMode::C11,
            "-std=c17" | "-std=c18" | "-std=iso9899:2017" | "-std=iso9899:2018" => {
                self.language = LanguageMode::C17;
            }
            "-std=gnu17" | "-std=gnu18" => self.language = LanguageMode::Gnu17,
            "-std=gnu89" | "-std=gnu90" => self.language = LanguageMode::Gnu89,
            "-fgnu89-inline" => self.gnu_inline = Some(true),
            "-fno-gnu89-inline" => self.gnu_inline = Some(false),
            _ if arg.starts_with("-std=") => {
                return Err(format!("unsupported C language mode `{arg}`"));
            }
            "-pedantic" => {}
            // GCC 14's -fpermissive lowers the error of W0012 to a warning
            // (EvoIR 6.3); it changes no other diagnostic of EvoCC.
            "-fpermissive" => self.lints.set_permissive(true),
            "-fno-permissive" => self.lints.set_permissive(false),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn parse_remaining(&mut self, args: &[String], lenient: bool) -> Result<(), String> {
        let arg = &args[self.index];
        match arg.as_str() {
            _ if arg.starts_with("-W") || arg == "-w" || arg.starts_with("--cap-lints=") => {
                self.lints.option(arg)?
            }
            _ if lenient
                && (arg.starts_with("-f")
                    || arg.starts_with("-m")
                    || matches!(arg.as_str(), "-coverage" | "--coverage")) =>
            {
                self.ignored_options.push(arg.clone());
            }
            _ if arg.starts_with('-') && arg != "/dev/stdin" => {
                return Err(format!("unsupported option `{arg}`"));
            }
            _ => {
                if self.input.replace(arg.clone()).is_some() {
                    return Err("multiple input files are not supported".into());
                }
            }
        }
        Ok(())
    }

    fn validate_source_mode(&self, mode: DriverMode, lenient: bool) -> Result<(), String> {
        if self.repair_mode.is_some() && mode != DriverMode::ParseOnly {
            return Err("--repair requires --parse-only".into());
        }
        // A plan record has a JSON and a text form only. Refuse before any edit,
        // so that no half-concise plan appears.
        if self.repair_mode.is_some() && self.error_format == Some(diagnostics::Format::Concise) {
            return Err("--repair does not support --error-format=concise".into());
        }
        if self.source_from_stdin
            && (!lenient || mode != DriverMode::ParseOnly || self.repair_mode.is_some())
        {
            return Err("--source-from-stdin requires -fsyntax-only without --repair".into());
        }
        Ok(())
    }

    fn admit_proof(
        &mut self,
        mode: DriverMode,
        lenient: bool,
    ) -> Result<Option<proof_required::ProofOptions>, String> {
        let proof_arguments = std::mem::take(&mut self.proof_arguments);
        proof_arguments.admit(proof_required::Admission {
            // The proof command publishes only assembly text and its record.
            // cc actions translate -S/-c/link into EmitAsm internally, but their
            // extra output destinations are not part of that publication contract.
            emit_asm: mode == DriverMode::EmitAsm && !lenient,
            entries: &self.core_entries,
            baseline: self.optimization == 0 && !self.standard_builtins,
            rewrites: self.optimize_scalar_evoir,
            rewrite_checker_pinned: self.eq_checker_digest.is_some(),
            unchanged_backend: self.opt_passes.is_empty()
                && !self.signed_wrapping
                && !self.dynamic_rounding
                && !self.stack_protector
                && !self.automatic_zero
                && self.pass_report.is_none()
                && (self.optimize_scalar_evoir
                    || (self.eq_proof_dir.is_none() && !self.eq_options))
                && !self.frame_variants_selected
                && !self.selection_variants_selected
                && !self.allocation_variants_selected
                && self.codegen.ub_checks == evocc_mir::UbCheckProfile::Checked,
        })
    }

    fn validate_core_and_equality(
        &self,
        mode: DriverMode,
        proof: Option<&proof_required::ProofOptions>,
    ) -> Result<(), String> {
        if mode == DriverMode::EmitCoreCHir {
            if self.core_entries.is_empty() || self.optimization != 0 || self.standard_builtins {
                return Err("source export for Core C-HIR requires --core-entry=NAME, -O0 and -fno-builtin; it is not a compilation proof".into());
            }
        } else if !self.core_entries.is_empty() && proof.is_none() {
            return Err("--core-entry requires --emit=core-c-hir".into());
        }
        if self.optimize_scalar_evoir
            && !matches!(
                mode,
                DriverMode::EmitEvoIr
                    | DriverMode::EmitAsm
                    | DriverMode::InterpretEvoIr
                    | DriverMode::Check
            )
        {
            return Err(
                "-Oeq-scalar requires --emit=evoir, --emit=asm or --interpret-evoir".to_owned(),
            );
        }
        if (!self.opt_passes.is_empty() || self.pass_report.is_some())
            && !matches!(
                mode,
                DriverMode::EmitEvoIr
                    | DriverMode::EmitAsm
                    | DriverMode::InterpretEvoIr
                    | DriverMode::Check
            )
        {
            return Err(
                "development passes and --pass-report require an EvoIR execution or emission mode"
                    .into(),
            );
        }
        let schedule = self.eq_policy.schedule();
        if self.eq_proof_dir.is_some()
            && !matches!(
                mode,
                DriverMode::EmitEvoIr
                    | DriverMode::EmitAsm
                    | DriverMode::InterpretEvoIr
                    | DriverMode::Check
            )
        {
            return Err("--eq-proof-dir requires an EvoIR execution or emission mode".into());
        }
        if (self.eq_options && !self.optimize_scalar_evoir)
            || schedule.max_nodes > 100_000
            || schedule.max_epochs > 1_024
            || !(1..=8).contains(&schedule.max_transactions)
            || !(1..=300_000).contains(&self.eq_timeout_ms)
        {
            return Err(
                "options for EqSat require -Oeq-scalar and admitted resource limits".into(),
            );
        }
        Ok(())
    }

    fn select_stack_guard(&mut self) -> Result<(), String> {
        if self.guard_is_sysreg {
            self.codegen.stack_guard = StackGuard::SystemRegister {
                register: self
                    .guard_register
                    .take()
                    .ok_or("sysreg stack guard requires its register")?,
                offset: self
                    .guard_offset
                    .ok_or("sysreg stack guard requires its byte offset")?,
            };
        }
        Ok(())
    }

    fn validate_native_options(&self, mode: DriverMode) -> Result<(), String> {
        if mode == DriverMode::Check && self.ub_checks_selected {
            return Err("--ub-checks is native-only and cannot weaken --check".into());
        }
        if mode == DriverMode::Check && self.frame_variants_selected {
            return Err("--frame-opt is native-only and has no meaning for --check".into());
        }
        if mode == DriverMode::Check && self.selection_variants_selected {
            return Err("--isel-opt is native-only and has no meaning for --check".into());
        }
        if mode == DriverMode::Check && self.allocation_variants_selected {
            return Err("--regalloc-opt is native-only and has no meaning for --check".into());
        }
        Ok(())
    }

    fn select_deployment(
        &mut self,
        target: DriverTarget,
        lenient: bool,
    ) -> Result<Option<evocc_cpp::availability::Version>, String> {
        let darwin_deployment = if target.is_darwin() {
            Some(darwin::deployment_target(
                self.deployment_option.as_deref(),
            )?)
        } else {
            if let Some(option) = self.deployment_option.take() {
                if !lenient {
                    return Err(format!("unsupported option `{option}`"));
                }
                self.ignored_options.push(option);
            }
            None
        };
        Ok(darwin_deployment)
    }

    fn finish(mut self, lenient: bool) -> Result<DriverOptions, String> {
        if self.language.legacy_declarations() && self.gnu_inline == Some(false) {
            return Err("unsupported inline mode: -fno-gnu89-inline with C89/GNU89".into());
        }

        let mode = self.mode.ok_or_else(|| {
            "exactly one of -E, --parse-only, --interpret-c-hir, --emit=evoir, --emit=asm, or --interpret-evoir is required"
                .to_string()
        })?;

        self.validate_source_mode(mode, lenient)?;
        let proof = self.admit_proof(mode, lenient)?;
        self.validate_core_and_equality(mode, proof.as_ref())?;
        self.select_stack_guard()?;
        self.validate_native_options(mode)?;
        let target = self.target.unwrap_or(DriverTarget::Aarch64UnknownLinuxGnu);
        if let Some(option) = self
            .framework_options
            .first()
            .filter(|_| !target.is_darwin())
        {
            return Err(format!("unsupported option `{option}`"));
        }

        let darwin_deployment = self.select_deployment(target, lenient)?;
        self.codegen.anonymous_on_stack = target.platform().layout().anonymous_arguments_on_stack();
        if self.program_arguments.is_some() && mode != DriverMode::InterpretCHir {
            return Err("program arguments require --interpret-c-hir".into());
        }

        self.into_driver_options(mode, target, proof, darwin_deployment)
    }

    fn into_driver_options(
        self,
        mode: DriverMode,
        target: DriverTarget,
        proof: Option<proof_required::ProofOptions>,
        darwin_deployment: Option<evocc_cpp::availability::Version>,
    ) -> Result<DriverOptions, String> {
        Ok(DriverOptions {
            mode,
            error_format: self.error_format.unwrap_or_default(),
            color: self.color,
            source_bytes: std::cell::RefCell::new(Vec::new()),
            lints: self.lints,
            repair_mode: self.repair_mode,
            included_bytes: std::cell::RefCell::new(std::collections::BTreeMap::new()),
            target,
            input: self
                .input
                .ok_or_else(|| "an input file is required".to_string())?,
            source_from_stdin: self.source_from_stdin,
            include_paths: self.include_paths,
            system_include_paths: self.system_include_paths,
            forced_includes: self.forced_includes,
            macro_prefix_maps: self.macro_prefix_maps,
            macro_actions: self.macro_actions,
            standard_includes: self.standard_includes,
            core_entries: self.core_entries,
            defined_functions: self.defined_functions,
            program_arguments: self.program_arguments,
            proof,
            optimize_scalar_evoir: self.optimize_scalar_evoir,
            opt_passes: self.opt_passes,
            pass_report: self.pass_report,
            eq_policy: self.eq_policy,
            eq_checker: self.eq_checker,
            eq_checker_digest: self.eq_checker_digest,
            eq_proof_dir: self.eq_proof_dir,
            eq_timeout_ms: self.eq_timeout_ms,
            optimization: self.optimization,
            debug_types: self.debug_types,
            signed_wrapping: self.signed_wrapping,
            dynamic_rounding: self.dynamic_rounding,
            trapping_math: self.trapping_math,
            standard_builtins: self.standard_builtins,
            disabled_builtins: self.disabled_builtins,
            optional_inlining: self.optional_inlining,
            hosted: self.hosted,
            pthread: self.pthread,
            automatic_zero: self.automatic_zero,
            tls_model: self.tls_model,
            stack_protector: self.stack_protector,
            patchable_entry: self.patchable_entry,
            minimum_function_alignment: self.minimum_function_alignment,
            codegen: self.codegen,
            language: self.language,
            gnu_inline: self
                .gnu_inline
                .unwrap_or(self.language.legacy_declarations()),
            short_wchar: self.short_wchar,
            // Plain char is signed on Darwin and unsigned on AArch64 Linux.
            signed_char: self.signed_char.unwrap_or(target.is_darwin()),
            // Darwin code is always position independent, and Apple's compiler
            // defines the PIC macros but not the PIE macros.
            position_independence: self.position_independence.unwrap_or(if target.is_darwin() {
                PositionIndependence::Pic(2)
            } else {
                PositionIndependence::Pie(2)
            }),
            survey: self.survey,
            darwin_deployment,
            system_root: self.system_root,
            cc_mode: false,
            ignored_options: self.ignored_options,
            captured: None,
            opened_files: std::cell::RefCell::new(Vec::new()),
        })
    }
}

pub(super) fn parse(args: &[String], lenient: bool) -> Result<DriverOptions, String> {
    let mut parser = OptionParser::new();
    while parser.index < args.len() {
        parser.parse_argument(args, lenient)?;
        parser.index += 1;
    }
    parser.finish(lenient)
}
