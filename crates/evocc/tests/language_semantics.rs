//! Small source reproductions of measured benchmark failures.
use std::path::Path;
use std::process::Command;

fn check(fixture: &str, modes: &[&str]) {
    check_with_args(fixture, modes, &[]);
}

fn check_with_args(fixture: &str, modes: &[&str], options: &[&str]) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(fixture);
    for mode in modes {
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args(["--target=aarch64-unknown-linux-gnu", mode])
            .args(options)
            .arg(&source)
            .output()
            .expect("start compiler");
        assert!(
            output.status.success(),
            "{fixture} {mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n", "{fixture} {mode}");
        }
    }
}

const EXECUTION: &[&str] = &["--interpret-c-hir", "--interpret-evoir", "--emit=asm"];

#[test]
fn unused_call_parameters_and_returns_still_require_typed_conversions() {
    for case in 0..7 {
        check_with_args(
            "unused-typed-call.c",
            EXECUTION,
            &[&format!("-DCASE={case}")],
        );
    }
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/unused-typed-call.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([mode, "-DCASE=7"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("E3001"));
    }
}

#[test]
fn benchmark_frontier_floating_absolute_calls() {
    check_with_args("floating-absolute-calls.c", EXECUTION, &["-O2"]);
    let module = lower_source("extern double fabs(double); extern void impossible(void); void f(double x){double y=fabs(x); if(y<0.0)impossible();}").unwrap();
    let text = evocc_evoir::render_module(&module);
    assert!(
        text.contains("fp.olt"),
        "the original comparison is still evaluated"
    );
    assert!(!text.contains("call @fabs"));
    for source in [
        // Preserve the call under test; ordinary private leaves may inline.
        "static __attribute__((noinline)) double fabs(double x){return x;} double f(double x){return fabs(x);}",
        "extern double fabs(double) __asm__(\"alternate\"); double f(double x){return fabs(x);}",
        "extern double (*fabs)(double); double f(double x){return fabs(x);}",
        "extern int fabs(int); int f(int x){return fabs(x);}",
    ] {
        let text = evocc_evoir::render_module(&lower_source(source).unwrap());
        assert!(!text.contains("fp.to_bits"), "{source}");
        assert!(text.contains("call"), "{source}");
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/floating-absolute-calls.c");
    let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--emit=asm", "-O2"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("bl impossible_absolute_relation"));
    for option in ["-fno-builtin", "-ffreestanding"] {
        let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args(["--emit=evoir", "-O2", option])
            .arg(&source)
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("call @fabs"));
    }
}

#[test]
fn benchmark_frontier_floating_local_values() {
    check("floating-local-values.c", EXECUTION);
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/floating-local-values.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([mode, "-DUNINITIALIZED"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("E3001"));
        check_with_args(
            "floating-local-values.c",
            &[mode],
            &["-DUNINITIALIZED", "-ftrivial-auto-var-init=zero"],
        );
    }
    let module = lower_source("double f(double x){double y=x+1.0; return y;}").unwrap();
    assert!(!evocc_evoir::render_module(&module).contains(" = alloca "));
    // Address escape and volatile storage retain real memory objects.
    for src in [
        "extern void use(double*); double f(double x){double y=x; use(&y); return y;}",
        "double f(double x){volatile double y=x; return y;}",
    ] {
        assert!(evocc_evoir::render_module(&lower_source(src).unwrap()).contains(" = alloca "));
    }
}

#[test]
fn benchmark_frontier_floating_partitions() {
    check_with_args("floating-comparison-partitions.c", EXECUTION, &["-O2"]);
    check("floating-comparison-partition-controls.c", EXECUTION);
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/floating-comparison-partitions.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([
                "--target=aarch64-unknown-linux-gnu",
                mode,
                "-O2",
                "-DUNINITIALIZED",
            ])
            .arg(&source)
            .output()
            .unwrap();
        assert!(!out.status.success(), "{mode}: consumed poison was erased");
        assert!(String::from_utf8_lossy(&out.stderr).contains("E3001"));
    }
    let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", "--emit=asm", "-O2"])
        .arg(source)
        .output()
        .unwrap();
    assert!(out.status.success());
    let assembly = String::from_utf8(out.stdout).unwrap();
    assert!(
        !assembly.contains("bl impossible_float_relation"),
        "impossible floating partitions still reference an undefined function"
    );
    let module = lower_source(
        "extern void observed(void); void f(volatile float x, volatile float y) { if ((x<y)&&(x>y)) observed(); }",
    ).unwrap();
    assert!(evocc_evoir::render_module(&module).contains("call @observed"));
}

#[test]
fn benchmark_frontier_floating_comparisons() {
    check("floating-comparison-builtins.c", EXECUTION);
    for (source, code) in [
        ("int f(void){return __builtin_isless(1.0);}", "E2909"),
        ("int f(void){return __builtin_isless(1, 2);}", "E2951"),
        ("int f(int *p){return __builtin_isless(p, 1.0);}", "E2951"),
    ] {
        let file = evocc_common::FileId::new(0);
        let arena = evocc_common::Arena::default();
        let mut lexer = evocc_lex::Lexer::new(file, source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            tokens.push(token);
        }
        let mut parser = evocc_parser::Parser::new(tokens, file, source);
        let _unit = parser.parse_translation_unit_in(&arena);
        assert_eq!(parser.diagnostic().map(|d| d.code), Some(code), "{source}");
    }
    let module = lower_source("int f(long double x){return __builtin_isless(x, 0.0L);}").unwrap();
    assert!(evocc_evoir::render_module(&module).contains("f128"));
}

#[test]
fn benchmark_frontier_polynomial_places() {
    check_with_args("polynomial-scalar-place.c", EXECUTION, &["-DTEST_PRIVATE"]);
}

#[test]
fn benchmark_frontier_static_member_offsets() {
    check("static-narrow-member-offset.c", EXECUTION);
}

#[test]
fn benchmark_frontier_fpcr_admission_dominance() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/floating-environment-dominance.c");
    let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", "--emit=asm"])
        .arg(source)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let assembly = String::from_utf8(out.stdout).unwrap();
    for (name, expected) in [
        ("chain", 1),
        ("loop", 1),
        ("both", 2),
        ("one", 2),
        ("after_call", 2),
        ("after_asm", 2),
        ("after_asm_goto", 2),
        ("transport", 0),
    ] {
        let body = assembly
            .split_once(&format!("\n{name}:\n"))
            .unwrap()
            .1
            .split_once(&format!(".size {name},"))
            .unwrap()
            .0;
        assert_eq!(body.matches(", fpcr").count(), expected, "{name}: {body}");
    }
}

#[test]
fn switch_default_keeps_values_live_through_every_case_dispatch() {
    check("switch-default-pointer-environment.c", EXECUTION);
}

#[test]
fn floating_classification_preserves_bits_and_evaluates_each_operand_once() {
    check("floating-classification.c", EXECUTION);
}

#[test]
fn floating_integer_conversions_check_the_truncated_value_at_full_width() {
    check("floating-integer-boundaries.c", EXECUTION);
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/floating-integer-boundaries.c");
    for case in 1..=11 {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
                .args(["--target=aarch64-unknown-linux-gnu", mode])
                .arg(format!("-DCASE={case}"))
                .arg(&source)
                .output()
                .unwrap();
            let expected = if case == 11 { "E3001" } else { "E3063" };
            assert!(!out.status.success(), "{mode} case {case}");
            assert!(
                String::from_utf8_lossy(&out.stderr).contains(expected),
                "{mode} case {case}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
fn scalar_floating_execution_preserves_formats_and_conversions() {
    check("floating-runtime-scalar.c", EXECUTION);
}

#[test]
fn floating_call_interfaces_keep_the_aapcs_scalar_and_hfa_banks() {
    check("floating-abi.c", EXECUTION);
}

#[test]
fn floating_hfa_classification_uses_completed_record_layout() {
    check("floating-hfa-alignment.c", EXECUTION);
}

#[test]
fn floating_hfa_varargs_preserve_member_alignment_separately_from_object_alignment() {
    check("floating-hfa-varargs-alignment.c", EXECUTION);
    let module = lower_source(
        "struct __attribute__((aligned(32))) H { double a __attribute__((aligned(16))); double b,c,d; }; struct H identity(struct H h) { return h; }",
    )
    .unwrap();
    assert!(evocc_evoir::render_module(&module).contains("hfa=4xf64, abi_align=16"));
}

#[test]
fn floating_target_header_constants_narrow_their_exact_source_representation() {
    check("floating-header-constants.c", EXECUTION);
    let module = lower_source("double f(void){return (double)(1.0L + 0x1p-100L);}").unwrap();
    let result = evocc_evoir::interpret_module(
        &module,
        "f",
        &[
            evocc_evoir::RuntimeValue::new(
                evocc_types::EvoIRType::Mem(evocc_types::Region::All),
                0,
            ),
            evocc_evoir::RuntimeValue::new(evocc_types::EvoIRType::World, 0),
        ],
        evocc_evoir::InterpreterLimits::new(100),
    )
    .unwrap();
    assert_eq!(result.return_values[0].value(), 0x3ff0000000000000);
}

/// L04 admits floating complex values: the comparison that was the
/// refused form lowers to two part comparisons.
#[test]
fn floating_complex_values_lower_as_two_parts() {
    for kind in ["float", "double", "long double"] {
        let source = format!(
            "int main(void){{ volatile _Complex {kind} a = 0.0, b = -0.0; return a != b; }}"
        );
        let module = lower_source(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
        let text = evocc_evoir::render_module(&module);
        assert_eq!(text.matches("fp.oeq").count(), 2, "{source}\n{text}");
    }
}

#[test]
fn target_floating_call_refusals_survive_the_cli_without_affecting_storage() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/floating-target-call-refusal.c");
    for kind in ["__fp16", "__bf16"] {
        for case in 0..5 {
            for mode in ["--emit=evoir", "--emit=asm", "--interpret-evoir"] {
                for opt in ["-O0", "-O2"] {
                    let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
                        .args(["--target=aarch64-unknown-linux-gnu", mode, opt])
                        .arg(format!("-DFLOAT_TYPE={kind}"))
                        .arg(format!("-DCASE={case}"))
                        .arg(&source)
                        .output()
                        .unwrap();
                    let diagnostic = String::from_utf8_lossy(&out.stderr);
                    assert!(!out.status.success(), "{kind} {case} {mode} {opt}");
                    assert!(diagnostic.contains("E3049"), "{diagnostic}");
                    assert!(!diagnostic.contains("E3101"), "{diagnostic}");
                }
            }
        }
        check_with_args(
            "floating-target-call-refusal.c",
            EXECUTION,
            &[&format!("-DFLOAT_TYPE={kind}"), "-DCASE=5"],
        );
    }
}

#[test]
fn variable_array_cast_captures_on_the_executed_expression_path() {
    check("variable-array-cast.c", EXECUTION);
    check("variable-array-cast-entry.c", EXECUTION);
}

#[test]
fn variable_array_cast_is_a_memory_assembly_operand() {
    check_with_args("variable-array-cast.c", &["--emit=asm"], &["-DWITH_ASM"]);
}

#[test]
fn returned_array_members_decay_without_repeating_the_record_expression() {
    check("returned-array-members.c", EXECUTION);
}

#[test]
fn returned_arrays_use_the_same_pointer_projection_rules() {
    check("returned-array-projections.c", EXECUTION);
}

#[test]
fn readonly_static_objects_reject_stores_after_initialization() {
    check("static-readonly-storage.c", EXECUTION);
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/static-readonly-storage.c");
    for case in 1..=7 {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
                .args(["--target=aarch64-unknown-linux-gnu", mode])
                .arg(format!("-DCASE={case}"))
                .arg(&source)
                .output()
                .unwrap();
            assert!(
                !out.status.success(),
                "{mode}, case {case} allowed the store"
            );
            assert!(
                String::from_utf8_lossy(&out.stderr).contains("E3014"),
                "{mode}, case {case}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
fn readonly_global_metadata_survives_ir_and_machine_roundtrips() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/static-readonly-storage.c");
    let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", "--emit=evoir"])
        .arg(source)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    for name in [
        "scalar",
        "zero",
        "array",
        "pointers",
        "device",
        "explicit_data",
    ] {
        let line = text
            .lines()
            .find(|l| l.starts_with(&format!("global @{name} ")))
            .unwrap();
        assert!(line.contains(" readonly=true"), "{line}");
    }
    for name in ["object", "pointee", "mixed", "literal", "mutable_literal"] {
        let line = text
            .lines()
            .find(|l| l.starts_with(&format!("global @{name} ")))
            .unwrap();
        assert!(!line.contains(" readonly=true"), "{line}");
    }
    let module = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    let changed = text.replacen(" readonly=true", "", 1);
    let changed = evocc_evoir::parse_section9_module(changed.as_bytes()).unwrap();
    assert_ne!(
        module, changed,
        "dropping permissions changes module semantics"
    );
    let machine = evocc_mir::lower_module(&module).unwrap();
    let bytes = evocc_mir::render_module(&machine);
    let reparsed = evocc_mir::parse_module(&bytes).unwrap();
    assert_eq!(machine.globals, reparsed.globals);
}

#[test]
fn readonly_native_placement_distinguishes_pic_relocations_and_explicit_sections() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/static-readonly-storage.c");
    for model in ["-fno-pic", "-fPIC"] {
        let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args(["--target=aarch64-unknown-linux-gnu", "--emit=asm", model])
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        let section_of = |symbol: &str| {
            text.split_once(&format!("\n{symbol}:\n"))
                .unwrap()
                .0
                .lines()
                .rev()
                .find(|line| {
                    line.starts_with(".section ") || matches!(*line, ".data" | ".bss" | ".rodata")
                })
                .unwrap()
        };
        for symbol in ["scalar", "zero", "array", "device"] {
            assert!(section_of(symbol).contains(".rodata"), "{model}: {symbol}");
        }
        let pointers = section_of("pointers");
        assert!(
            pointers.contains(if model == "-fPIC" {
                ".data.rel.ro"
            } else {
                ".rodata"
            }),
            "{pointers}"
        );
        assert_eq!(
            section_of("explicit_data"),
            ".section \".data.explicit\",\"aw\",%progbits"
        );
        for symbol in ["object", "pointee", "mixed"] {
            assert_eq!(section_of(symbol), ".data", "{symbol}");
        }
    }
    let module =
        lower_source("const int a[2]={1,2}; int main(void){((int*)a)[1]=3; return 0;}").unwrap();
    let machine = evocc_mir::lower_module(&module).unwrap();
    let error =
        evocc_mir::interpret_module(&machine, "main", &[], evocc_mir::MirLimits::new(100_000))
            .unwrap_err();
    assert_eq!(error.code(), "E3014", "{error:?}");
}

#[test]
fn pointer_conversions_follow_array_element_qualification() {
    for case in 1..=6 {
        check_with_args(
            "array-pointer-qualification.c",
            EXECUTION,
            &[&format!("-DCASE={case}")],
        );
    }
}

#[test]
fn array_pointer_conversion_does_not_erase_bounds_or_nested_pointer_types() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/array-pointer-qualification.c");
    for case in 7..=9 {
        let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args(["--target=aarch64-unknown-linux-gnu", "--parse-only"])
            .arg(format!("-DCASE={case}"))
            .arg(&source)
            .output()
            .unwrap();
        assert!(!out.status.success(), "case {case}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("E2202"),
            "case {case}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn static_initializers_use_initialized_const_integers_without_changing_ice() {
    for case in [1, 2, 3, 7, 11, 12, 13, 14, 15, 17] {
        check_with_args(
            "static-const-initializers.c",
            EXECUTION,
            &[&format!("-DCASE={case}")],
        );
    }
}

#[test]
fn static_const_extension_keeps_unknown_values_and_ice_queries_separate() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/static-const-initializers.c");
    for (case, code) in [
        (4, "E2208"),
        (5, "E2208"),
        (6, "E2208"),
        (8, "E2912"),
        (9, "E2208"),
        (10, "E2208"),
        (16, "E2208"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args(["--target=aarch64-unknown-linux-gnu", "--emit=evoir"])
            .arg(format!("-DCASE={case}"))
            .arg(&source)
            .output()
            .unwrap();
        assert!(!output.status.success(), "case {case}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "case {case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn inline_vector_parameters_keep_complete_object_initialization() {
    check("inline-vector-objects.c", EXECUTION);
}

#[test]
fn vector_scalar_conversions_preserve_admitted_unsigned_representations() {
    check("vector-scalar-conversions.c", EXECUTION);
}

#[test]
fn vector_scalar_conversions_keep_narrowing_and_conflicts_refused() {
    for source in [
        "typedef int V __attribute__((vector_size(16))); V f(V v,unsigned x){return v+x;}",
        "typedef unsigned V __attribute__((vector_size(16))); V f(V v,long x){return v+x;}",
        "typedef unsigned char V __attribute__((vector_size(16))); V f(V v){return v+65535;}",
        "typedef unsigned char V __attribute__((vector_size(16))); V f(V v){return v+-129;}",
        "typedef signed char V __attribute__((vector_size(16))); V f(V v){return v+255;}",
    ] {
        let arena = evocc_common::Arena::new(4096);
        let file = evocc_common::FileId::new(0);
        let mut lexer = evocc_lex::Lexer::new(file, source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            tokens.push(token);
        }
        let mut parser = evocc_parser::Parser::new(tokens, file, source);
        let _unit = parser.parse_translation_unit_in(&arena);
        assert_eq!(
            parser.diagnostic().map(|d| d.code),
            Some("E2102"),
            "{source}"
        );
    }
}

#[test]
fn direct_vector_attributes_keep_the_declarator_type_identity() {
    check("vector-declaration-attributes.c", EXECUTION);
}

#[test]
fn wide_vector_values_survive_parameter_copies_and_control_flow() {
    check("wide-vector-control-flow.c", EXECUTION);
}

#[test]
fn variable_arrays_reenter_labels_and_sibling_scopes() {
    check("variable-array-label-scopes.c", EXECUTION);
}

#[test]
fn unsupported_function_options_do_not_silently_change_representation_accesses() {
    // Preserve this original reproducer. Its per-function no-strict-aliasing
    // request used to be silently ignored; it is now an explicit refusal.
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/representation-only-array-places.c");
    for mode in EXECUTION {
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args(["--target=aarch64-unknown-linux-gnu", mode])
            .arg(&source)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("E2952") && stderr.contains("optimize"),
            "{stderr}"
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn assembly_tied_pointer_supplies_the_narrow_register_representation() {
    check("assembly-tied-pointer-width.c", &["--emit=asm"]);
}

#[test]
fn constant_floating_representations_do_not_require_floating_execution() {
    check("constant-floating-representation.c", EXECUTION);
}

#[test]
fn floating_literals_round_directly_to_the_source_format() {
    check("floating-literal-rounding.c", EXECUTION);
}

#[test]
fn hexadecimal_floating_literals_preserve_boundary_representations() {
    check("floating-hex-representation.c", EXECUTION);
}

#[test]
fn finite_floating_constants_share_source_width_semantics() {
    check("floating-constant-arithmetic.c", EXECUTION);
}

#[test]
fn quiet_nan_payloads_are_exact_object_representations() {
    check("nan-constant-representation.c", EXECUTION);
}

#[test]
fn scalar_nan_values_keep_their_type_and_dynamic_library_boundary() {
    for source in [
        "double f(void){return __builtin_nan(\"1\");}",
        "void f(double*p,const char*s){*p=__builtin_nan(s);}",
        "void f(double*p){*p=__builtin_nan(\"1\")+1.;}",
        "void f(float*p){*p=__builtin_nan(\"1\");}",
    ] {
        let module = lower_source(source).expect("scalar floating execution is supported");
        let text = evocc_evoir::render_module(&module);
        if source.contains("*p=__builtin_nan(\"1\")+1.") {
            assert!(text.contains("9221120237041090561"), "{text}");
        } else if source.contains("float*p") {
            assert!(
                text.contains("2143289344") || text.contains("f32"),
                "{text}"
            );
        } else {
            assert!(text.contains("f64"), "{text}");
        }
        if source.contains("__builtin_nan(s)") {
            assert!(module.declares.iter().any(|f| f.name == "nan"));
            assert!(!module.functions.iter().any(|f| f.name == "nan"));
        }
    }
}

#[test]
fn nan_payload_conflicts_and_invalid_builtin_calls_remain_refused() {
    for (source, code) in [
        ("double x=__builtin_nan(\"-1\");", "E2208"),
        ("double x=__builtin_nan(\"+1\");", "E2208"),
        ("double x=__builtin_nan(\" 1\");", "E2208"),
        ("double x=__builtin_nan(\"xyz\");", "E2208"),
        ("double x=__builtin_nan(\"1x\");", "E2208"),
        ("double x=__builtin_nan(\"08\");", "E2208"),
        ("double x=__builtin_nan(\"0x\");", "E2208"),
        ("double x=__builtin_nan(\"1\\0z\");", "E2208"),
        ("double x=__builtin_nan();", "E2909"),
        ("double x=__builtin_nan(1);", "E2913"),
        ("double nan(int); double x=__builtin_nan(\"1\");", "E2927"),
    ] {
        let file = evocc_common::FileId::new(0);
        let arena = evocc_common::Arena::new(4096);
        let mut lexer = evocc_lex::Lexer::new(file, source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            tokens.push(token);
        }
        let mut parser = evocc_parser::Parser::new(tokens, file, source);
        let _unit = parser.parse_translation_unit_in(&arena);
        assert_eq!(parser.diagnostic().map(|d| d.code), Some(code), "{source}");
    }
}

#[test]
fn constant_representation_stores_do_not_turn_floating_values_into_integers() {
    for source in [
        "int f(void){union{double d;long l;}v;return (int)(v.d=10);}",
        "int f(double*p){*p=10;return (int)*p;}",
        "void f(double*p,int n){*p=n;}",
        "void f(double*p){*p=(-2147483647-1)/-1;}",
        "void f(double*p,int n){*p=n+1;}",
    ] {
        let module =
            lower_source(source).expect("scalar conversion lowers to FP, not integer storage");
        let text = evocc_evoir::render_module(&module);
        assert!(text.contains("f64"), "{text}");
        if source.contains("/ -1") || source.contains("/-1") {
            assert!(text.contains("ub.assume.signed_overflow"), "{text}");
        }
    }
    // An atomic floating store converts to binary64 and moves its bits.
    let module = lower_source("void f(_Atomic double*p){*p=10;}").expect("atomic store lowers");
    let text = evocc_evoir::render_module(&module);
    assert!(
        text.contains("f64") && text.contains("atomic.store seq_cst"),
        "{text}"
    );
    // A floating compound assignment on an atomic object stays refused.
    let source = "void f(_Atomic double*p){*p+=10;}";
    let arena = evocc_common::Arena::new(4096);
    let file = evocc_common::FileId::new(0);
    let mut lexer = evocc_lex::Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    let mut parser = evocc_parser::Parser::new(tokens, file, source);
    let _unit = parser.parse_translation_unit_in(&arena);
    assert_eq!(parser.diagnostic().unwrap().code, "E2702");
}

fn lower_source(source: &str) -> Result<evocc_evoir::Module, evocc_lower::LowerError> {
    let arena = evocc_common::Arena::new(64 * 1024);
    let file = evocc_common::FileId::new(0);
    let mut lexer = evocc_lex::Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    let mut parser = evocc_parser::Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{source}: {:?}", parser.diagnostic());
    evocc_lower::lower_unit(
        &unit,
        &evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu(),
    )
}

#[test]
fn relative_label_relocations_do_not_erase_intermediate_narrowing() {
    let source = "int main(void){ static long x=(signed char)(&&b-&&a); a: if(x) goto b; return 0; b:return 1;}";
    assert_eq!(
        lower_source(source)
            .expect_err("cast cannot become a 64-bit difference relocation")
            .code(),
        "E3026"
    );
}

#[test]
fn floating_homogeneous_aggregates_are_not_misclassified_as_integer_records() {
    for source in [
        "struct S{float a[2];}; int f(struct S x){return sizeof x;}",
        "union U{float a[4];float b;}; union U f(union U x){return x;}",
    ] {
        let module = lower_source(source).expect("supported HFA signature");
        let marks = &module.functions[0].param_marks;
        assert!(marks
            .iter()
            .filter_map(|mark| mark.shape())
            .all(|shape| shape.floating.is_some() && shape.vector.is_none()));
        assert!(evocc_evoir::render_module(&module).contains("hfa="));
    }
}

#[test]
fn variable_array_objects_capture_and_release_storage() {
    // The native main retains 4096 repetitions to detect stack growth. The
    // interpreter checks 64 fresh identities/lifetimes, without paying for
    // thousands of historical immutable-memory snapshots.
    let source = include_str!("../../../tests/fixtures/variable-array-objects.c");
    let arena = evocc_common::Arena::new(64 * 1024);
    let file = evocc_common::FileId::new(0);
    let mut lexer = evocc_lex::Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    let mut parser = evocc_parser::Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let main = unit
        .iter()
        .find_map(|decl| match decl {
            evocc_chir::CHirDecl::Function(function) if function.name.spelling == "short_run" => {
                Some(function.symbol)
            }
            _ => None,
        })
        .unwrap();
    let chir = evocc_chir::interp::interpret_scalar_function(
        &unit,
        main,
        evocc_chir::interp::InterpreterLimits::new(5_000_000),
    )
    .unwrap();
    assert_eq!(
        chir.return_value
            .unwrap()
            .mathematical_value(unit.types())
            .unwrap(),
        0
    );
    let module = evocc_lower::lower_unit(
        &unit,
        &evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu(),
    )
    .unwrap();
    let arguments = module
        .functions
        .iter()
        .find(|f| f.name == "short_run")
        .unwrap()
        .params
        .iter()
        .map(|v| evocc_evoir::RuntimeValue::new(v.ty.clone(), 0))
        .collect::<Vec<_>>();
    let result = evocc_evoir::interpret_module(
        &module,
        "short_run",
        &arguments,
        evocc_evoir::InterpreterLimits::new(500_000),
    )
    .unwrap();
    assert_eq!(
        result.return_values.first(),
        Some(&evocc_evoir::RuntimeValue::bits(32, 0))
    );
    check("variable-array-objects.c", &["--emit=asm"]);
    check("variable-array-alloca-lifetime.c", EXECUTION);
}

#[test]
fn ended_variable_array_keeps_its_lifetime_error() {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([mode, "--target=aarch64-unknown-linux-gnu"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/variable-array-ended-lifetime.c"),
            )
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3010"),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
#[test]
fn variable_size_type_query_evaluates_its_bound_once() {
    check("variable-size-type-query.c", EXECUTION);
    check("variable-array-capability.c", EXECUTION);
}

#[test]
fn variably_modified_typeof_captures_each_new_bound() {
    check("variable-array-typeof.c", EXECUTION);
    check("variable-array-typeof-array.c", EXECUTION);
}

#[test]
fn variably_modified_pointer_typeof_preserves_evaluation_contexts() {
    for optimization in ["-O0", "-O2"] {
        check_with_args(
            "variable-array-typeof-effects.c",
            EXECUTION,
            &[optimization],
        );
        check_with_args("typeof-evaluation-contexts.c", EXECUTION, &[optimization]);
        check_with_args("typeof-parameter-evaluation.c", EXECUTION, &[optimization]);
    }
}

#[test]
fn variable_arrays_obey_the_automatic_initialization_option() {
    check_with_args(
        "variable-array-zero.c",
        EXECUTION,
        &["-ftrivial-auto-var-init=zero"],
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([mode, "--target=aarch64-unknown-linux-gnu"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/variable-array-zero.c"),
            )
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3001"),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
#[test]
fn assembly_general_readwrite_operand_uses_a_real_register() {
    check("assembly-general-update.c", &["--emit=asm"]);
}
#[test]
fn assembly_tied_integer_width_preserves_the_low_part() {
    check("assembly-tied-width.c", &["--emit=asm"]);
}
#[test]
fn null_conditional_pointer_conversion_is_shared() {
    check("null-conditional-qualification.c", EXECUTION);
    check("immediate-pointer-qualifiers.c", EXECUTION);
}
#[test]
fn mixed_record_varargs_copy_unused_representation() {
    check("mixed-record-varargs.c", EXECUTION);
    check_with_args("mixed-record-varargs-driver.c", EXECUTION, &["-DCOMBINED"]);
}

#[test]
fn integer_complex_varargs_use_the_composite_abi() {
    check("integer-complex-varargs.c", EXECUTION);
    // L04: floating complex anonymous arguments use the HFA rules.
    for source in [
        "void f(int n,...){__builtin_va_list ap;__builtin_va_start(ap,n);__builtin_va_arg(ap,_Complex double);}",
        "struct S{int n;_Complex double value;};void f(int n,...){__builtin_va_list ap;__builtin_va_start(ap,n);__builtin_va_arg(ap,struct S);}",
    ] {
        assert!(lower_source(source).is_ok(), "{source}");
    }
    for (source, code) in [
        ("typedef float v2f __attribute__((vector_size(8)));struct H{v2f a,b;};void f(int n,...){__builtin_va_list ap;__builtin_va_start(ap,n);__builtin_va_arg(ap,struct H);}", "E3049"),
        ("struct S{int n;__fp16 value;};void f(int n,...){__builtin_va_list ap;__builtin_va_start(ap,n);__builtin_va_arg(ap,struct S);}", "E3030"),
    ] {
        assert_eq!(lower_source(source).unwrap_err().code(), code);
    }
}
#[test]
fn relative_label_table_dispatches_to_the_same_function() {
    check("relative-label-table.c", EXECUTION);
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", "--emit=evoir"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/relative-label-table.c"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    let machine = evocc_mir::lower_module(&module).unwrap();
    let outcome =
        evocc_mir::interpret_module(&machine, "main", &[], evocc_mir::MirLimits::new(100_000))
            .unwrap();
    assert_eq!(outcome.results, [evocc_mir::MirValue::Int(0)]);
}
#[test]
fn atomic_read_modify_write_returns_old_and_new_values() {
    check("atomic-read-modify-write.c", EXECUTION);
    check("atomic-rmw-widths.c", EXECUTION);
}
#[test]
fn ancestor_addresses_use_the_reviewed_null_rule() {
    for optimization in ["-O0", "-O2"] {
        check_with_args("activation-address-depths.c", EXECUTION, &[optimization]);
    }
    // Keep the original contrary-oracle fixture unchanged: it requires a real
    // ancestor and therefore returns one under the approved null rule.
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/ancestor-return-address.c");
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", "--interpret-evoir"])
        .arg(source)
        .output()
        .expect("start compiler");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"1\n");
}
#[test]
fn integer_complex_arithmetic_keeps_both_components() {
    check("integer-complex-values.c", EXECUTION);
    check("integer-complex-operations.c", EXECUTION);
    check("integer-complex-initialization.c", EXECUTION);
    check("integer-complex-lvalues.c", EXECUTION);
}

#[test]
fn aggregate_parameters_keep_storage_addresses_across_edges() {
    check("aggregate-parameter-edges.c", EXECUTION);
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/aggregate-parameter-edges.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([mode, "-DREAD_UNINITIALIZED=1"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "{mode} read an uninitialized component"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3001"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn integer_complex_division_parses_without_the_withdrawn_refusal() {
    // L07 replaced E2948 with GCC's wide algorithm (EvoIR 10.1.14). The
    // values are tested in integer_complex_division.rs.
    for source in [
        "int main(void){_Complex int z=(9+7i)/(2+1i);return __real__ z;}",
        "int main(void){_Complex int z=9+7i;z/=2+1i;return __real__ z;}",
        "int main(void){int x=9;x/=2+1i;return x;}",
    ] {
        let arena = evocc_common::Arena::new(64 * 1024);
        let file = evocc_common::FileId::new(0);
        let mut lexer = evocc_lex::Lexer::new(file, source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            tokens.push(token);
        }
        let mut parser = evocc_parser::Parser::new(tokens, file, source);
        let _unit = parser.parse_translation_unit_in(&arena);
        assert!(
            !parser.had_error(),
            "{source}: {:?}",
            parser.diagnostic().map(|diagnostic| diagnostic.code)
        );
    }
}

#[test]
fn integer_complex_updates_refuse_pointer_targets() {
    // L04: a floating target computes in the floating complex type.
    for source in [
        "int main(void){double d=3;d*=1+2i;return d!=3;}",
        "int main(void){double d=3;d/=1+2i;return d!=0.6;}",
    ] {
        assert!(lower_source(source).is_ok(), "{source}");
    }
    for source in ["int main(void){int *p=0;p+=1+2i;return 0;}"] {
        let arena = evocc_common::Arena::new(64 * 1024);
        let file = evocc_common::FileId::new(0);
        let mut lexer = evocc_lex::Lexer::new(file, source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            tokens.push(token);
        }
        let mut parser = evocc_parser::Parser::new(tokens, file, source);
        let _unit = parser.parse_translation_unit_in(&arena);
        assert!(parser.had_error(), "accepted invalid update: {source}");
        assert_eq!(parser.diagnostic().unwrap().code, "E2102", "{source}");
    }
}

#[test]
fn complex_components_do_not_bypass_cast_or_const_constraints() {
    for source in [
        "int f(void*p){_Complex int z=(_Complex int)p;return 0;}",
        "void *f(_Complex int z){return (void*)z;}",
        "int f(void){const _Complex int z=1+2i;__real__ z=3;return 0;}",
    ] {
        let arena = evocc_common::Arena::new(64 * 1024);
        let file = evocc_common::FileId::new(0);
        let mut lexer = evocc_lex::Lexer::new(file, source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            tokens.push(token);
        }
        let mut parser = evocc_parser::Parser::new(tokens, file, source);
        let _unit = parser.parse_translation_unit_in(&arena);
        assert!(
            parser.had_error(),
            "accepted invalid complex form: {source}"
        );
    }
}
