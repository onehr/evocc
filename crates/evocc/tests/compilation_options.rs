use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(arguments: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(arguments)
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn successful(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn check_preprocessed_program(source: &str) -> String {
    successful(run(&["--parse-only"], source));
    let text = successful(run(&["-E"], source));
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        assert_eq!(successful(run(&[mode], &text)).trim(), "0");
    }
    successful(run(&["--emit=asm"], &text));
    text
}

#[test]
fn conservative_codegen_controls_match_the_existing_native_contract() {
    let source = include_str!("../../../tests/fixtures/conservative-codegen-options.c");
    let flags = [
        "-fomit-frame-pointer",
        "-fno-omit-frame-pointer",
        "-fno-optimize-sibling-calls",
        "-fno-align-functions",
        "-fno-align-jumps",
        "-fno-align-labels",
        "-fno-align-loops",
        "-fno-prefetch-loop-arrays",
        "-fno-tree-ch",
        "-fno-tree-loop-distribute-patterns",
    ];
    for optimization in ["-O0", "-O2"] {
        for mode in [
            "--interpret-c-hir",
            "--interpret-evoir",
            "--emit=evoir",
            "--emit=asm",
        ] {
            let baseline = successful(run(&[mode, optimization], source));
            for flag in flags {
                assert_eq!(
                    successful(run(&[mode, optimization, flag], source)),
                    baseline,
                    "{flag}"
                );
            }
            let mut arguments = vec![mode, optimization];
            arguments.extend(flags);
            arguments.extend(flags.iter().rev());
            assert_eq!(successful(run(&arguments, source)), baseline);
            if mode.starts_with("--interpret-") {
                assert_eq!(baseline.trim(), "0");
            }
            if mode == "--emit=asm" {
                assert_eq!(baseline.matches("stp x29, x30, [sp, #-16]!").count(), 4);
                assert_eq!(baseline.matches("mov x29, sp").count(), 4);
                assert!(baseline
                    .contains(".p2align 6\n.type source_aligned, %function\nsource_aligned:"));
                assert_eq!(
                    baseline
                        .lines()
                        .filter(|line| line.starts_with(".p2align "))
                        .count(),
                    4
                );
                assert!(!baseline.contains("bl memset"));
                assert!(!baseline.contains("prfm "));
            }
        }
    }
    let base = ["--emit=asm", "-fmin-function-alignment=32"];
    let expected = successful(run(&base, source));
    let mut arguments = base.to_vec();
    arguments.extend(flags);
    assert_eq!(successful(run(&arguments, source)), expected);
    assert!(expected.contains(".p2align 5\n.type plain_leaf, %function\nplain_leaf:"));
    assert!(expected.contains(".p2align 6\n.type source_aligned, %function\nsource_aligned:"));
    // Disabling absent transforms is not approval for new positive transforms
    // or arbitrary GCC allocator/layout algorithms.
    for flag in [
        "-falign-functions=32",
        "-falign-loops",
        "-fprefetch-loop-arrays",
        "-ftree-ch",
        "-ftree-loop-distribute-patterns",
        "-foptimize-sibling-calls",
        "-fira-region=all",
        "-fira-region=mixed",
        "-freorder-blocks-algorithm=stc",
    ] {
        assert!(
            !run(&["--emit=asm", flag], source).status.success(),
            "{flag}"
        );
    }
}

#[test]
fn bounded_gcc_tuning_selects_existing_function_allocation_and_layout() {
    let source = include_str!("../../../tests/fixtures/driver-gcc-tuning.c");
    let flags = [
        "-fira-region=one",
        "-fira-hoist-pressure",
        "-freorder-blocks-algorithm=simple",
    ];
    for optimization in ["-O0", "-O2", "-Os"] {
        for mode in [
            "--interpret-c-hir",
            "--interpret-evoir",
            "--emit=evoir",
            "--emit=asm",
        ] {
            let baseline = successful(run(&[mode, optimization], source));
            for flag in flags {
                assert_eq!(
                    successful(run(&[mode, optimization, flag], source)),
                    baseline
                );
            }
            let mut arguments = vec![mode, optimization];
            arguments.extend(flags);
            arguments.extend(flags.iter().rev());
            assert_eq!(successful(run(&arguments, source)), baseline);
            if mode.starts_with("--interpret-") {
                assert_eq!(baseline.trim(), "0");
            }
        }
    }
    for flag in [
        "-fira-region=all",
        "-fira-region=mixed",
        "-fira-region=ONE",
        "-fira-algorithm=CB",
        "-fira-loop-pressure",
        "-fcode-hoisting",
        "-freorder-blocks-algorithm=stc",
        "-freorder-blocks-and-partition",
        "-fpack-struct=1",
        "-mabi=ilp32",
        "-Ofast",
    ] {
        let output = run(&["--emit=asm", flag], source);
        assert!(!output.status.success(), "{flag}");
        assert!(output.stdout.is_empty(), "{flag}");
    }
}

#[test]
fn individual_symbol_sections_preserve_storage_kinds_and_explicit_placement() {
    let source = include_str!("../../../tests/fixtures/driver-symbol-sections.c");
    for relocation in ["-fno-pic", "-fPIC"] {
        let base = ["--emit=asm", relocation];
        let baseline = successful(run(&base, source));
        let mut arguments = base.to_vec();
        arguments.extend(["-ffunction-sections", "-fdata-sections"]);
        let text = successful(run(&arguments, source));
        for (name, kind, flags, ty) in [
            ("live_function", "text", "ax", "progbits"),
            ("dead_function", "text", "ax", "progbits"),
            ("initialized", "data", "aw", "progbits"),
            ("zeroed", "bss", "aw", "nobits"),
            ("readonly", "rodata", "a", "progbits"),
            ("readonly_zero", "rodata", "a", "progbits"),
            ("tls_data", "tdata", "awT", "progbits"),
            ("tls_zero", "tbss", "awT", "nobits"),
            ("dead_data", "data", "aw", "progbits"),
            ("dead_zero", "bss", "aw", "nobits"),
        ] {
            let prefix = text.split_once(&format!("\n{name}:\n")).unwrap().0;
            let section = prefix
                .lines()
                .rev()
                .find(|l| l.starts_with(".section "))
                .unwrap();
            assert_eq!(
                section,
                format!(".section \".{kind}.{name}\",\"{flags}\",%{ty}")
            );
        }
        let kind = if relocation == "-fPIC" {
            "data.rel.ro"
        } else {
            "rodata"
        };
        assert!(text.contains(&format!(".section \".{kind}.relocation\"")));
        for name in [
            "text.selected",
            "data.selected",
            "bss.selected",
            "tdata.selected",
        ] {
            assert!(text.contains(&format!(".section \".{name}\"")));
        }
        assert!(!text.contains(".text.selected_function"));
        assert!(!text.contains(".data.explicit_data"));
        assert!(!text.contains(".text.live_alias"));
        assert!(text.contains(".set live_alias, live_function"));
        assert!(text.contains(".init_array.00201"));
        assert!(text.contains(".fini_array.00201"));
        arguments.extend(["-fno-function-sections", "-fno-data-sections"]);
        assert_eq!(successful(run(&arguments, source)), baseline);
        arguments.extend(["-ffunction-sections", "-fdata-sections"]);
        assert_eq!(successful(run(&arguments, source)), text);
        for mode in ["-E", "--emit=evoir"] {
            assert_eq!(
                successful(run(&[mode, relocation], source)),
                successful(run(
                    &[mode, relocation, "-ffunction-sections", "-fdata-sections"],
                    source
                ))
            );
        }
    }
    // Neither switch enables the other class of placement.
    let only_functions = successful(run(&["--emit=asm", "-ffunction-sections"], source));
    assert!(only_functions.contains(".text.live_function"));
    assert!(!only_functions.contains(".data.initialized"));
    let only_data = successful(run(&["--emit=asm", "-fdata-sections"], source));
    assert!(!only_data.contains(".text.live_function"));
    assert!(only_data.contains(".data.initialized"));
}

#[test]
fn individual_symbol_sections_are_not_silently_ignored_on_darwin() {
    let text = successful(run(&["--emit=evoir"], "int main(void) { return 0; }"));
    let module = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    for (function_sections, data_sections) in [(true, false), (false, true)] {
        let error = evocc_mir::compile_module_with_options(
            evocc_mir::Target::Darwin,
            &module,
            &|_| true,
            evocc_mir::emit::RelocationModel::PositionIndependent,
            &evocc_mir::emit::CodegenOptions {
                function_sections,
                data_sections,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(matches!(error, evocc_mir::NativeStop::Assembly(message)
            if message == "individual function/data sections require the Linux ELF target"));
    }
}

#[test]
fn disabling_strict_aliasing_keeps_the_existing_conservative_access_semantics() {
    let source = include_str!("../../../tests/fixtures/no-strict-aliasing.c");
    for optimization in ["-O0", "-O2"] {
        for mode in [
            "--interpret-c-hir",
            "--interpret-evoir",
            "--emit=evoir",
            "--emit=asm",
        ] {
            let baseline = successful(run(&[mode, optimization], source));
            assert_eq!(
                successful(run(&[mode, optimization, "-fno-strict-aliasing"], source)),
                baseline
            );
            if mode.starts_with("--interpret-") {
                assert_eq!(baseline.trim(), "0");
            }
        }
    }
    // This option removes no object/provenance/lifetime/permission guard.
    let invalid = "const int x = 7; int main(void) { *(int *)&x = 9; return x; }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let result = run(&[mode, "-fno-strict-aliasing"], invalid);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("E3014"));
    }
}

#[test]
fn standard_excess_precision_is_the_existing_source_format_profile() {
    let source = include_str!("../../../tests/fixtures/floating-standard-evaluation.c");
    for optimization in ["-O0", "-O2"] {
        for mode in [
            "-E",
            "--interpret-c-hir",
            "--interpret-evoir",
            "--emit=evoir",
            "--emit=asm",
        ] {
            for rounding in [None, Some("-frounding-math")] {
                let mut args = vec!["--target=aarch64-unknown-linux-gnu", mode, optimization];
                if let Some(rounding) = rounding {
                    args.push(rounding);
                }
                let baseline = successful(run(&args, source));
                args.push("-fexcess-precision=standard");
                assert_eq!(successful(run(&args, source)), baseline);
                args.push("-fexcess-precision=standard");
                assert_eq!(successful(run(&args, source)), baseline);
                if mode.starts_with("--interpret-") {
                    assert_eq!(baseline.trim(), "0");
                }
            }
        }
    }
    for flag in [
        "-fexcess-precision=fast",
        "-fexcess-precision=16",
        "-fexcess-precision=unknown",
    ] {
        assert!(!run(&["--emit=asm", flag], source).status.success());
    }
    // Standard precision does not authorize a separately refused fast-math mode.
    assert!(!run(
        &["--emit=asm", "-fexcess-precision=standard", "-ffast-math"],
        source
    )
    .status
    .success());
}

#[test]
fn pthread_predefinitions_precede_explicit_macro_actions_in_every_mode() {
    let source = include_str!("../../../tests/fixtures/driver-pthread.c");
    let cases: &[&[&str]] = &[
        &["-DEXPECT_REENTRANT=0"],
        &["-pthread"],
        &["-pthread", "-pthread"],
        &["-pthread", "-U_REENTRANT", "-DEXPECT_REENTRANT=0"],
        &["-U_REENTRANT", "-pthread", "-DEXPECT_REENTRANT=0"],
        &["-pthread", "-D_REENTRANT=7", "-DEXPECT_REENTRANT=7"],
        &["-D_REENTRANT=7", "-pthread", "-DEXPECT_REENTRANT=7"],
    ];
    for case in cases {
        for optimization in ["-O0", "-O2"] {
            for mode in ["-E", "--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
                let mut arguments = vec![mode, optimization];
                arguments.extend_from_slice(case);
                let text = successful(run(&arguments, source));
                if mode.starts_with("--interpret-") {
                    assert_eq!(text.trim(), "0");
                }
            }
        }
    }
}

#[test]
fn baseline_floating_profile_does_not_advertise_strict_environment_or_fma() {
    let source = "#if defined(__STDC_IEC_559__) || defined(__STDC_IEC_60559_BFP__) || defined(__STDC_IEC_559_COMPLEX__) || defined(__FP_FAST_FMA) || defined(__FP_FAST_FMAF)\n#error unsupported_floating_contract_advertised\n#endif\n#if __FLT_EVAL_METHOD__ != 0 || __GCC_IEC_559 != 0 || __GCC_IEC_559_COMPLEX != 0\n#error incorrect_numerical_profile\n#endif\nint main(void){return 0;}\n";
    successful(run(&["--parse-only"], source));
}

#[test]
fn preprocessed_text_preserves_nested_pack_actions() {
    let text = check_preprocessed_program(include_str!(
        "../../../tests/fixtures/preprocessed-packing.c"
    ));
    assert!(text.contains("#pragma pack(push, 1)"));
    assert!(text.contains("#pragma pack(pop)"));
    assert!(text.contains("#pragma pack()"));
}

#[test]
fn preprocessed_text_preserves_target_header_declarations() {
    let source = "int before;\n#pragma GCC aarch64 \"arm_neon.h\"\n\
        _Static_assert(sizeof(uint8x8x3_t) == 24, \"tuple\");\n\
        int main(void) { return 0; }";
    let text = check_preprocessed_program(source);
    assert!(text.contains("#pragma GCC aarch64 \"arm_neon.h\"\n"));
    assert!(!text.contains('\u{1e}'));
}

#[test]
fn preprocessed_text_preserves_visibility_scope() {
    let source = "#pragma GCC visibility push(hidden)\n\
        int hidden;\n#pragma GCC visibility pop\n\
        int visible; int main(void) { return 0; }";
    let text = check_preprocessed_program(source);
    let assembly = successful(run(&["--emit=asm"], &text));
    assert!(assembly.contains(".hidden hidden\n"));
    assert!(!assembly.contains(".hidden visible\n"));
}

#[test]
fn debug_types_are_opt_in_and_do_not_change_executable_assembly() {
    let source = include_str!("../../../tests/fixtures/debug-types.c");
    let plain = successful(run(&["--emit=asm"], source));
    let debug = successful(run(&["--emit=asm", "-g"], source));
    assert!(debug.starts_with(&plain));
    assert!(debug.contains(".section .debug_info"));
    assert!(debug.contains(".section .debug_abbrev"));
    for name in [
        "node",
        "packet",
        "state",
        "count",
        "mode",
        "tail",
        "inspect_node",
        "callback_slot",
        "index",
    ] {
        assert!(debug.contains(&format!(".asciz {name:?}")), "{name}");
    }
    assert_eq!(successful(run(&["--emit=asm", "-g", "-g0"], source)), plain);
    assert!(successful(run(&["--emit=asm", "-g0", "-gdwarf-4"], source))
        .contains(".section .debug_info"));
}

#[test]
fn debug_members_keep_their_written_typedef_identity() {
    let debug = successful(run(
        &["--emit=asm", "-g"],
        "typedef struct {int counter;} atomic_t;
         typedef struct refcount_struct {atomic_t refs;} refcount_t;
         struct common {refcount_t refcnt; const refcount_t *ptr;};
         struct common object;",
    ));
    let alias_label = |spelling: &str| {
        let prefix = debug
            .split_once(&format!(".asciz {spelling:?}\n"))
            .unwrap()
            .0;
        prefix
            .lines()
            .rev()
            .find_map(|line| line.strip_suffix(':'))
            .unwrap()
            .to_owned()
    };
    for (member, alias) in [("refs", "atomic_t"), ("refcnt", "refcount_t")] {
        let prefix = debug.split_once(&format!(".asciz {member:?}\n")).unwrap().0;
        assert_eq!(
            prefix.lines().last().unwrap(),
            format!(".4byte {} - .L_evocc_debug_cu", alias_label(alias)),
            "{member} must reference its source typedef, not only its canonical record"
        );
    }
}

#[test]
fn debug_integer_names_match_the_sealed_btf_type_vocabulary() {
    let debug = successful(run(&["--emit=asm", "-g"],
        "struct R { short a; unsigned short b; long c; unsigned long d; long long e; unsigned long long f; unsigned __int128 g; }; struct R object;"));
    for spelling in [
        "short int",
        "short unsigned int",
        "long int",
        "long unsigned int",
        "long long int",
        "long long unsigned int",
        "__int128 unsigned",
    ] {
        assert!(
            debug.contains(&format!(".asciz {spelling:?}")),
            "missing BTF-compatible base type {spelling}"
        );
    }
}

#[test]
fn debug_padding_bitfields_do_not_create_member_dies() {
    let debug = successful(run(
        &["--emit=asm", "-g"],
        "struct P {unsigned first:3; unsigned :5; unsigned last:1;}; struct P object;",
    ));
    let abbreviations = debug.split_once(".L_evocc_debug_abbrev:\n").unwrap().1;
    assert_eq!(abbreviations.matches("\n.uleb128 13\n.byte 0\n").count(), 1,
        "only named bitfields have member DIEs; padding must not add a nameless-member abbreviation");
}

#[test]
fn debug_callback_parameters_keep_their_written_typedefs() {
    let debug = successful(run(
        &["--emit=asm", "-g"],
        "typedef unsigned long argument_size;
         struct callbacks { int (*send)(argument_size count); };
         struct callbacks object;",
    ));
    let prefix = debug.split_once(".asciz \"argument_size\"\n").unwrap().0;
    let label = prefix
        .lines()
        .rev()
        .find_map(|line| line.strip_suffix(':'))
        .unwrap();
    assert!(
        debug.contains(&format!(".4byte {label} - .L_evocc_debug_cu\n")),
        "the callback parameter must reference argument_size, not its canonical integer"
    );
    assert!(!debug.contains(".asciz \"count\"\n"));
}

#[test]
fn debug_multidimensional_arrays_use_one_die_with_all_subranges() {
    for source in [
        "unsigned long object[4][5];",
        "typedef unsigned long row[5]; row object[4];",
    ] {
        let debug = successful(run(&["--emit=asm", "-g"], source));
        let label = debug
            .split_once(".asciz \"object\"\n.4byte ")
            .unwrap()
            .1
            .split_once(" - ")
            .unwrap()
            .0;
        // Delimit by label lines, not by the labels used inside references.
        let array = debug
            .split_once(&format!("{label}:\n"))
            .unwrap()
            .1
            .split("\n.L_evocc_")
            .next()
            .unwrap();
        assert_eq!(
            array.matches(".4byte .L_evocc_index_type - ").count(),
            2,
            "a source matrix must have two subranges, not nested BTF array elements"
        );
    }
}

#[test]
fn debug_target_vectors_are_not_described_as_floating_scalars() {
    let debug = successful(run(
        &["--emit=asm", "-g"],
        "typedef __Float32x4_t lanes; typedef __SVBool_t predicates; int anchor;",
    ));
    // A real NEON vector is an array DIE over its scalar element, not the
    // former opaque builtin DIE. Follow the typedef instead of requiring an
    // invented name on that canonical vector type.
    let vector_label = debug
        .split_once(".asciz \"lanes\"\n.4byte ")
        .unwrap()
        .1
        .split_once(" - ")
        .unwrap()
        .0;
    let vector = debug.split_once(&format!("\n{vector_label}:\n")).unwrap().1;
    let element_label = vector
        .lines()
        .nth(1)
        .unwrap()
        .strip_prefix(".4byte ")
        .unwrap()
        .split_once(" - ")
        .unwrap()
        .0;
    assert!(
        vector.contains(".4byte .L_evocc_index_type - .L_evocc_debug_cu\n.uleb128 4\n.byte 0\n")
    );
    let element = debug
        .split_once(&format!("\n{element_label}:\n"))
        .unwrap()
        .1;
    assert_eq!(element.lines().nth(1), Some(".asciz \"float\""));
    assert_eq!(element.lines().nth(2), Some(".uleb128 4")); // element size
    assert_eq!(element.lines().nth(3), Some(".uleb128 4")); // DW_ATE_float
    assert!(debug.contains(".asciz \"__SVBool_t\""));
    assert!(debug.contains(".uleb128 8455")); // DW_AT_GNU_vector
    assert!(!debug.contains("TargetBuiltin("));
}

#[test]
fn debug_information_does_not_resurrect_an_unemitted_internal_function() {
    let debug = successful(run(
        &["--emit=asm", "-g"],
        "static int unused(void) { return 7; } int main(void) { return 0; }",
    ));
    assert!(!debug.contains(".8byte \"unused\""));
    assert!(debug.contains(".8byte \"main\""));
}

#[test]
fn debug_external_attributes_follow_c_linkage_and_omit_false_flags() {
    // This check needs an out-of-line internal definition, not an eliminated
    // private leaf. Retention is independent of the linkage encoded by DWARF.
    let source = format!(
        "static int inherited_function(int) __attribute__((noinline));\n{}",
        include_str!("../../../tests/fixtures/debug-linkage.c")
    );
    let debug = successful(run(&["--emit=asm", "-g"], &source));
    let entry = |name: &str| {
        debug
            .split_once(&format!(".asciz {name:?}\n"))
            .unwrap()
            .1
            .lines()
            .take(6)
            .collect::<Vec<_>>()
    };
    // The normal DWARF-to-BTF consumer treats external-attribute presence
    // as external linkage, so a false flag must be absent, not encoded zero.
    for name in ["local_counter", "inherited_counter"] {
        assert_eq!(
            entry(name)[1],
            ".uleb128 9",
            "{name} has C internal linkage"
        );
    }
    for name in ["public_counter", "hidden_counter", "protected_counter"] {
        assert_eq!(entry(name)[1], ".byte 1", "{name} has C external linkage");
        assert_eq!(entry(name)[2], ".uleb128 9");
    }
    for name in ["local_function", "inherited_function"] {
        let local = entry(name);
        assert_eq!(local[2], ".byte 1"); // DW_AT_prototyped, no DW_AT_external.
        assert!(local[3].starts_with(".uleb128 "));
    }
    for name in [
        "local_address",
        "inspect_counters",
        "hidden_function",
        "protected_function",
    ] {
        let external = entry(name);
        assert_eq!(external[2], ".byte 1"); // DW_AT_external.
        assert_eq!(external[3], ".byte 1"); // DW_AT_prototyped.
    }
}

#[test]
fn debug_objects_use_the_same_post_inline_inventory_as_lowering() {
    let source = "static __attribute__((always_inline)) inline int next(void) { \
        struct local_record { int value; }; \
        static struct local_record counter = { 5 }; \
        return ++counter.value; \
        } int main(void) { next(); return next() != 7; }";
    let plain = successful(run(&["-O2", "--emit=asm"], source));
    let debug = successful(run(&["-O2", "--emit=asm", "-g"], source));
    assert!(debug.starts_with(&plain));
    assert!(debug.contains(".asciz \"local_record\""));
    assert!(debug.contains(".asciz \"value\""));
    assert!(!debug.contains(".8byte \"next\""));
}

#[test]
fn zero_initialization_is_explicit_and_runs_at_each_declaration() {
    let source = include_str!("../../../tests/fixtures/automatic-zero.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        assert_eq!(
            successful(run(&[mode, "-ftrivial-auto-var-init=zero"], source)).trim(),
            "0"
        );
    }
    successful(run(&["--emit=asm", "-ftrivial-auto-var-init=zero"], source));
}

#[test]
fn automatic_zero_does_not_repeat_nonpointer_representation_stores() {
    for (source, expected_stores, expected_nulls) in [
        ("extern void observe(void *); void f(void) { struct { int x[4]; } r; observe(&r); }", 2, 0),
        ("extern void observe(void *); void f(void) { struct { int x[2]; void *p; } r; observe(&r); }", 3, 1),
        ("extern void observe(void *); void f(unsigned n) { long r[n]; observe(r); }", 8, 0),
    ] {
        let text = successful(run(&["--emit=evoir", "-ftrivial-auto-var-init=zero"], source));
        assert_eq!(text.lines().filter(|line| line.contains(" = store ")).count(), expected_stores, "{text}");
        assert_eq!(text.matches(" = ptr.null ").count(), expected_nulls, "{text}");
    }
}

#[test]
fn automatic_zero_single_pass_keeps_padding_values_and_pointer_provenance() {
    let source = include_str!("../../../tests/fixtures/automatic-zero-single-pass.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        assert_eq!(
            successful(run(&[mode, "-ftrivial-auto-var-init=zero"], source)).trim(),
            "0"
        );
        for options in [
            vec![mode],
            vec![
                mode,
                "-ftrivial-auto-var-init=zero",
                "-ftrivial-auto-var-init=uninitialized",
            ],
        ] {
            let output = run(&options, source);
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("E3001"));
        }
    }
    successful(run(&["--emit=asm", "-ftrivial-auto-var-init=zero"], source));
}

#[test]
fn prezeroed_bytes_do_not_admit_deferred_value_types() {
    for ty in ["_Complex long double", "_Complex double"] {
        let source =
            format!("extern void observe(void *); void f(void) {{ {ty} r; observe(&r); }}");
        let output = run(&["--emit=evoir", "-ftrivial-auto-var-init=zero"], &source);
        assert!(!output.status.success());
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(diagnostic.contains("not-lowered:"), "{diagnostic}");
    }
}

#[test]
fn automatic_zero_initializes_constructor_padding_without_changing_members() {
    let source = include_str!("../../../tests/fixtures/automatic-initializer-padding.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        assert_eq!(
            successful(run(&[mode, "-ftrivial-auto-var-init=zero"], source)).trim(),
            "0"
        );
    }
    successful(run(&["--emit=asm", "-ftrivial-auto-var-init=zero"], source));
}

#[test]
fn automatic_zero_covers_evaluated_compound_literal_padding() {
    let source = include_str!("../../../tests/fixtures/automatic-compound-padding.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        assert_eq!(
            successful(run(&[mode, "-ftrivial-auto-var-init=zero"], source)).trim(),
            "0"
        );
        let output = run(&[mode], source);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3001"));
    }
    successful(run(&["--emit=asm", "-ftrivial-auto-var-init=zero"], source));
}

#[test]
fn constructor_padding_zero_does_not_initialize_a_self_reference() {
    let source =
        "struct S { int n; long m; }; int main(void) { struct S s = {.n=s.n, .m=3}; return s.n; }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(&[mode, "-ftrivial-auto-var-init=zero"], source);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3001"));
    }
}

#[test]
fn constructor_padding_zero_keeps_copy_initializers_and_exemptions_unchanged() {
    let copy = "struct S { char n; long m; }; int main(void) { struct S s; __builtin_memset(&s, 0x39, sizeof(s)); struct S t = s; return ((unsigned char *)&t)[1]; }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        assert_eq!(
            successful(run(&[mode, "-ftrivial-auto-var-init=zero"], copy)).trim(),
            "57"
        );
        for (options, source) in [
            (vec![mode], "struct S { char n; long m; }; int main(void) { struct S s={1,2}; return ((unsigned char *)&s)[1]; }"),
            (vec![mode, "-ftrivial-auto-var-init=zero"], "struct S { char n; long m; }; int main(void) { struct S s __attribute__((uninitialized))={1,2}; return ((unsigned char *)&s)[1]; }"),
        ] {
            let output = run(&options, source);
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("E3001"));
        }
    }
}

#[test]
fn zero_initialization_does_not_erase_poison_without_the_option_or_for_exempt_objects() {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        for (flags, source) in [
            (vec![mode], "int main(void) { int x; return x; }"),
            (
                vec![
                    mode,
                    "-ftrivial-auto-var-init=zero",
                    "-ftrivial-auto-var-init=uninitialized",
                ],
                "int main(void) { int x; return x; }",
            ),
            (
                vec![mode, "-ftrivial-auto-var-init=zero"],
                "int main(void) { int x __attribute__((uninitialized)); return x; }",
            ),
        ] {
            let output = run(&flags, source);
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3001"),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn hosted_mode_and_automatic_initialization_follow_the_last_option() {
    for (flags, expected) in [
        (vec!["-E", "-ffreestanding"], "0"),
        (vec!["-E", "-ffreestanding", "-fhosted"], "1"),
        (vec!["-E", "-fhosted", "-ffreestanding"], "0"),
    ] {
        assert_eq!(
            successful(run(&flags, "__STDC_HOSTED__\n")).trim(),
            expected
        );
    }
    assert_eq!(
        successful(run(
            &[
                "--interpret-evoir",
                "-ftrivial-auto-var-init=uninitialized",
                "-ftrivial-auto-var-init=zero"
            ],
            "int main(void) { int x; return x; }"
        ))
        .trim(),
        "0"
    );
}

#[test]
fn freestanding_disables_only_ordinary_library_name_substitution() {
    let source = "int abs(int x){return x+100;} int main(void){volatile int x=-7; return abs(x)+__builtin_abs(x);}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        for (options, expected) in [
            (vec![mode, "-ffreestanding"], "100"),
            (vec![mode, "-ffreestanding", "-fbuiltin"], "14"),
            (vec![mode, "-fbuiltin", "-ffreestanding"], "100"),
        ] {
            assert_eq!(successful(run(&options, source)).trim(), expected);
        }
    }
}

#[test]
fn stack_protection_selects_source_arrays_and_frame_addresses() {
    let source = "extern void use(void *); int plain(int a) { int n=a; return n; } void array(void) { char a[2]; use(a); } void scalar(void) { int n=1; use(&n); } void heap(int *p) { use(&*p); } void parameter(int a) { use(&a); } __attribute__((no_stack_protector)) void exempt(void) { char a[20]; use(a); } void persistent(void) { static char a[20]; use(a); } void literal(void) { use(&(int){3}); } void literal_array(void) { use((int[]){1,2}); }";
    let ir = successful(run(&["--emit=evoir", "-fstack-protector-strong"], source));
    let protected = ir
        .lines()
        .filter(|line| line.starts_with("func @") && line.contains(" stack_protect"))
        .map(|line| line.split('(').next().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        protected,
        [
            "func @array",
            "func @scalar",
            "func @parameter",
            "func @literal",
            "func @literal_array"
        ]
    );
    let disabled = successful(run(
        &[
            "--emit=evoir",
            "-fstack-protector-strong",
            "-fno-stack-protector",
        ],
        source,
    ));
    assert!(!disabled.contains(" stack_protect"));
    evocc_evoir::parse_section9_module(ir.as_bytes())
        .expect("the protection marker survives textual interchange");
}

#[test]
fn stack_guard_uses_the_requested_register_offset_and_checks_each_return() {
    let source =
        "extern void use(void *); int f(int c) { char a[16]; use(a); if(c) return 1; return 2; }";
    let asm = successful(run(
        &[
            "--emit=asm",
            "-fstack-protector-strong",
            "-mstack-protector-guard=sysreg",
            "-mstack-protector-guard-reg=sp_el0",
            "-mstack-protector-guard-offset=64",
        ],
        source,
    ));
    assert!(asm.contains("mrs x16, sp_el0"), "{asm}");
    assert!(asm.contains("[x16, #64]"), "{asm}");
    assert!(asm.contains("__stack_chk_fail"), "{asm}");
    assert!(
        asm.matches("cmp x16, x17").count()
            >= asm.lines().filter(|line| line.trim() == "ret").count(),
        "{asm}"
    );
}

#[test]
fn return_signing_matches_frame_lifetime_and_final_override() {
    let source = "extern int g(int); int f(int n) { return g(n) + 1; } void *address(void) { return __builtin_return_address(0); }";
    let asm = successful(run(&["--emit=asm", "-mbranch-protection=pac-ret"], source));
    assert!(asm.contains("paciasp\n    stp x29, x30"), "{asm}");
    assert!(
        asm.contains("ldp x29, x30, [sp], #16\n    autiasp\n    ret"),
        "{asm}"
    );
    assert!(
        asm.contains("xpaclri"),
        "return-address builtin must remove signing bits: {asm}"
    );
    let disabled = successful(run(
        &[
            "--emit=asm",
            "-mbranch-protection=pac-ret",
            "-mbranch-protection=none",
        ],
        source,
    ));
    assert!(!disabled.contains("paciasp"));
    assert!(!disabled.contains("autiasp"));
}

#[test]
fn general_register_only_varargs_never_save_vector_registers() {
    let source = "long f(int n, ...) { __builtin_va_list a; __builtin_va_start(a,n); long x=__builtin_va_arg(a,long); __builtin_va_end(a); return x; }";
    let asm = successful(run(&["--emit=asm", "-mgeneral-regs-only"], source));
    assert!(!asm.contains("stp q"), "{asm}");
}

#[test]
fn effective_codegen_modes_control_the_frozen_predefined_set() {
    let source = "__SSP_STRONG__ __ARM_FEATURE_PAC_DEFAULT\n#ifdef __ARM_FP\nBAD_FLOAT\n#endif\n#ifdef __ARM_NEON\nBAD_VECTOR\n#endif\n";
    assert_eq!(
        successful(run(
            &[
                "-E",
                "-fstack-protector-strong",
                "-mbranch-protection=pac-ret",
                "-mgeneral-regs-only"
            ],
            source
        ))
        .trim(),
        "3 1"
    );
}

#[test]
fn strict_alignment_expands_unaligned_values_but_retains_native_results() {
    let source = include_str!("../../../tests/fixtures/strict-alignment.c");
    let asm = successful(run(&["--emit=asm", "-mstrict-align"], source));
    let read = asm
        .split("read_value:\n")
        .nth(1)
        .unwrap()
        .split(".size read_value")
        .next()
        .unwrap();
    assert_eq!(
        read.lines()
            .filter(|line| line.trim_start().starts_with("ldrb "))
            .count(),
        8,
        "{read}"
    );
    let write = asm
        .split("write_value:\n")
        .nth(1)
        .unwrap()
        .split(".size write_value")
        .next()
        .unwrap();
    assert_eq!(
        write
            .lines()
            .filter(|line| line.trim_start().starts_with("strb "))
            .count(),
        8,
        "{write}"
    );
    let ordinary = successful(run(
        &["--emit=asm", "-mstrict-align", "-mno-strict-align"],
        source,
    ));
    assert!(!ordinary.contains("ldrb w17"));
    let ir = successful(run(&["--emit=evoir"], source));
    let module = evocc_evoir::parse_section9_module(ir.as_bytes()).unwrap();
    let mir = evocc_mir::lower_module_for(evocc_mir::Target::Linux, &module, &|_| false).unwrap();
    let text = evocc_mir::render_module(&mir);
    assert_eq!(evocc_mir::parse_module(&text).unwrap(), mir);
    assert!(evocc_mir::parse_module(&text.replace("[align=1]", "[align=3]")).is_err());
    assert_eq!(
        evocc_mir::interpret_module(&mir, "main", &[], evocc_mir::MirLimits::new(100_000))
            .unwrap_or_else(|error| panic!("{error:?}\n{text}"))
            .results,
        vec![evocc_mir::MirValue::Int(0)]
    );
}

#[test]
fn automatic_initialization_and_protection_cover_vararg_storage() {
    let source = "long v(int n,...) { __builtin_va_list ap; __builtin_va_start(ap,n); long x=__builtin_va_arg(ap,long); __builtin_va_end(ap); return x; } int main(void) { return v(0,7L)!=7; }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        assert_eq!(
            successful(run(&[mode, "-ftrivial-auto-var-init=zero"], source)).trim(),
            "0"
        );
    }
    let ir = successful(run(&["--emit=evoir", "-fstack-protector-strong"], source));
    assert!(
        ir.lines()
            .any(|line| line.starts_with("func @v(") && line.ends_with(" stack_protect")),
        "{ir}"
    );
}

#[test]
fn optimization_options_select_the_measured_macro_state() {
    let source = "#ifdef __OPTIMIZE__\nOPT\n#endif\n#ifdef __OPTIMIZE_SIZE__\nSIZE\n#endif\n#ifdef __NO_INLINE__\nNOINLINE\n#endif\n";
    for (flags, expected) in [
        (vec!["-E", "-O2"], "OPT"),
        (vec!["-E", "-O2", "-O0"], "NOINLINE"),
        (vec!["-E", "-Os"], "OPT SIZE"),
        (vec!["-E", "-Os", "-O3"], "OPT"),
        (vec!["-E", "-O2", "-U__OPTIMIZE__"], ""),
    ] {
        assert_eq!(successful(run(&flags, source)).trim(), expected);
    }
}

#[test]
fn tiny_addresses_and_unwind_metadata_follow_effective_options() {
    let source = "static int x; int *f(void){ return &x; }";
    let asm = successful(run(
        &[
            "--emit=asm",
            "-mcmodel=tiny",
            "-fasynchronous-unwind-tables",
        ],
        source,
    ));
    assert!(
        asm.contains("    adr ") && !asm.contains("    adrp "),
        "{asm}"
    );
    assert!(
        asm.contains(".cfi_startproc") && asm.contains(".cfi_endproc"),
        "{asm}"
    );
    assert!(asm.contains(".cfi_def_cfa 29, 16"), "{asm}");
    let off = successful(run(
        &[
            "--emit=asm",
            "-fasynchronous-unwind-tables",
            "-fno-asynchronous-unwind-tables",
        ],
        source,
    ));
    assert!(!off.contains(".cfi_startproc"));
}
