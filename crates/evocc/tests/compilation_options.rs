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
fn debug_target_vectors_are_not_described_as_floating_scalars() {
    let debug = successful(run(
        &["--emit=asm", "-g"],
        "typedef __Float32x4_t lanes; typedef __SVBool_t predicates; int anchor;",
    ));
    assert!(debug.contains(".asciz \"__Float32x4_t\""));
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
