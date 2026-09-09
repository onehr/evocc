use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn run(source: &str, mode: &str) -> Output {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("evocc-assembly-{}-{serial}.c", std::process::id()));
    fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", mode])
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    output
}

const REGISTER_ALTERNATIVE: &str = include_str!("fixtures/assembly_register_alternative.c");
const FIXED_REGISTERS: &str = include_str!("fixtures/assembly_fixed_registers.c");
const MEMORY_ALTERNATIVES: &str =
    include_str!("../../../tests/fixtures/assembly-memory-alternatives.c");
const LOCAL_ADDRESS: &str = include_str!("../../../tests/fixtures/inline-local-address.c");
const INLINE_MEMORY: &str = r#"
static inline __attribute__((always_inline)) int read_memory(int value) {
    asm volatile("" : : "Q"(value) : "memory");
    return value;
}
int main(void) { return read_memory(7) != 7; }
"#;

#[test]
fn address_modifiers_select_registers_and_execute_operands_once() {
    let source = include_str!("../../../tests/fixtures/assembly-address-operands.c");
    let selected = source.replace("\"p\"", "\"r\"");
    for mode in ["--emit=evoir", "--emit=asm"] {
        let actual = run(source, mode);
        assert!(
            actual.status.success(),
            "{}",
            String::from_utf8_lossy(&actual.stderr)
        );
        let expected = run(&selected, mode);
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        assert_eq!(actual.stdout, expected.stdout, "{mode}: p selects r");
    }
    #[cfg(target_arch = "aarch64")]
    execute_native_assembly(source, "address-modifiers");
}

#[test]
fn address_modifiers_refuse_non_address_operands_and_outputs() {
    for operands in [
        "::\"r\"(1)",
        "::\"p\"(1)",
        "::\"i\"(1UL)",
        ":\"=r\"(x)",
        ":\"+r\"(x)",
        "::\"Q\"(x)",
        "::\"rZ\"(0UL)",
    ] {
        let source =
            format!("int f(void){{unsigned long x=0; asm(\"// %a0\" {operands}); return 0;}}");
        let output = run(&source, "--emit=evoir");
        assert!(!output.status.success(), "{operands}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3021"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn single_trailing_assembly_spelling_uses_the_existing_contextual_path() {
    for source in [
        "__asm(\".text\"); int value;",
        "extern int value __asm(\"renamed_value\");",
        "int f(void) { __asm volatile(\"\" : : : \"memory\"); return 0; }",
    ] {
        let output = run(source, "--parse-only");
        assert!(
            output.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let source = "int main(void) { int value; __asm __volatile__(\"mov %w0, #7\" : \"=r\"(value)); return value != 7; }";
    for mode in ["--emit=evoir", "--emit=asm"] {
        let output = run(source, mode);
        let canonical = run(&source.replace("__asm ", "__asm__ "), mode);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            canonical.status.success(),
            "{}",
            String::from_utf8_lossy(&canonical.stderr)
        );
        assert_eq!(output.stdout, canonical.stdout, "{mode}");
    }
    #[cfg(target_arch = "aarch64")]
    execute_native_assembly(source, "single-trailing-spelling");
}

#[test]
fn register_memory_alternatives_select_the_existing_register_representation() {
    for source in [
        MEMORY_ALTERNATIVES.to_owned(),
        MEMORY_ALTERNATIVES
            .replace("rm", "mr")
            .replace("=&mr", "=&rm"),
    ] {
        let selected = source.replace("rm", "r").replace("mr", "r");
        for mode in ["--emit=evoir", "--emit=asm"] {
            let output = run(&source, mode);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let expected = run(&selected, mode);
            assert!(
                expected.status.success(),
                "{}",
                String::from_utf8_lossy(&expected.stderr)
            );
            assert_eq!(output.stdout, expected.stdout, "{mode}");
        }
    }
}

#[test]
fn inline_local_address_bindings_reach_symbolic_assembly_operands() {
    let output = run(LOCAL_ADDRESS, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    for target in [evocc_mir::Target::Linux, evocc_mir::Target::Darwin] {
        let assembly = evocc_mir::compile_module_for(target, &module, &|_| true).unwrap();
        assert!(!assembly.contains("%c0"));
    }
}

#[test]
fn inline_memory_operands_keep_the_parameter_object() {
    let output = run(INLINE_MEMORY, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    evocc_evoir::parse_section9_module(&output.stdout).unwrap();
}

#[test]
#[cfg(target_arch = "aarch64")]
fn register_memory_alternatives_execute_places_effects_and_goto_edges() {
    execute_native_assembly(MEMORY_ALTERNATIVES, "register-memory");
}

#[test]
#[cfg(target_arch = "aarch64")]
fn inline_local_addresses_and_memory_operands_execute_on_the_machine() {
    execute_native_assembly(LOCAL_ADDRESS, "inline-local-address");
    execute_native_assembly(INLINE_MEMORY, "inline-memory");
}

#[test]
fn inline_local_bindings_do_not_replay_mutable_or_effectful_values() {
    for body in [
        r#"int k=1; k+=arg; asm(""::"i"(k)); return k;"#,
        r#"volatile int k=1; asm(""::"i"(k)); return k;"#,
        r#"int k=++calls; asm(""::"i"(k)); return k;"#,
        r#"int k=1; asm("" : "+r"(k)); asm(""::"i"(k)); return k;"#,
        r#"int k=1; int *p=&k; *p=arg; asm(""::"i"(k)); return k;"#,
    ] {
        let source = format!("static int calls; static inline __attribute__((always_inline)) int f(int arg){{{body}}} int entry(int arg){{return f(arg);}}");
        let output = run(&source, "--emit=evoir");
        assert!(!output.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3021"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for body in [
        "goto done; int k=5; done:return k;",
        "int first=({if(skip)goto done; 0;}); int k=5; done:return k;",
    ] {
        let source = format!("static inline __attribute__((always_inline)) int f(int skip){{{body}}} int main(void){{int flag=1; return f(flag);}}");
        let output = run(&source, "--interpret-evoir");
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3001"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn register_aliases_preserve_the_physical_operand_binding() {
    let source = FIXED_REGISTERS
        .replace("__asm__(\"x8\")", "__asm__(\"r8\")")
        .replace("__asm__(\"w8\")", "__asm__(\"r8\")");
    let output = run(&source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("@r8"), "{text}");
    assert!(text.contains("@x8"), "{text}");
    evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
}

#[test]
#[cfg(target_arch = "aarch64")]
fn register_aliases_execute_inputs_outputs_and_both_goto_edges() {
    let source = FIXED_REGISTERS
        .replace("__asm__(\"x8\")", "__asm__(\"r8\")")
        .replace("__asm__(\"w8\")", "__asm__(\"r8\")");
    execute_native_assembly(&source, "register-aliases");
}

#[test]
#[cfg(target_arch = "aarch64")]
fn digit_builtin_needs_no_libc_definition_on_the_machine() {
    execute_native_assembly(
        include_str!("../../../tests/fixtures/builtin-digit.c"),
        "builtin-digit",
    );
}

#[test]
#[cfg(target_arch = "aarch64")]
fn integer_modes_and_packed_storage_execute_on_the_machine() {
    execute_native_assembly(
        include_str!("../../../tests/fixtures/integer-mode-memory.c"),
        "integer-mode-memory",
    );
}

#[test]
fn symbolic_immediates_preserve_symbols_offsets_and_ir_round_trips() {
    let source = include_str!("../../../tests/fixtures/symbolic-assembly-immediates.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    assert_eq!(
        evocc_evoir::render_module(&module).as_bytes(),
        output.stdout
    );
    for target in [evocc_mir::Target::Linux, evocc_mir::Target::Darwin] {
        let mir = evocc_mir::lower_module_for(target, &module, &|_| true).unwrap();
        let text = evocc_mir::render_module(&mir);
        assert_eq!(evocc_mir::parse_module(&text).unwrap(), mir);
        assert!(text.contains("@words+8"), "{text}");
        for invalid in [
            text.replace("@words+8", "@words+08"),
            text.replace("@words+8", "@words"),
            text.replace("@words+8", "@words;other+8"),
            text.replace("\"i\"", "\"n\""),
        ] {
            assert!(evocc_mir::parse_module(&invalid).is_err(), "{invalid}");
        }
        let assembly = evocc_mir::compile_module_for(target, &module, &|_| true).unwrap();
        let prefix = if target == evocc_mir::Target::Darwin {
            "_"
        } else {
            ""
        };
        for symbol in ["words+8", "external_word", "target"] {
            assert!(
                assembly.contains(&format!("{prefix}{symbol}")),
                "{assembly}"
            );
        }
        assert!(!assembly.contains("%c"), "{assembly}");
    }
}

#[test]
#[cfg(target_arch = "aarch64")]
fn symbolic_inline_arguments_preserve_one_evaluation_on_the_machine() {
    execute_native_assembly(
        r#"
        static int object, effects;
        static inline __attribute__((always_inline)) int f(int *key, int branch) {
            asm volatile("// %c0 %w1" : : "i"(key), "r"(branch ? ++effects : --effects));
            asm volatile("// %c0" : : "i"(&&done));
        done: return effects;
        }
        int main(void) { int a = f(&object, 1); int b = f(&object, 0); return a != 1 || b != 0 || effects != 0; }
        "#,
        "symbolic-single-evaluation",
    );
}

#[test]
#[cfg(target_arch = "aarch64")]
fn symbolic_immediates_do_not_create_runtime_symbol_references() {
    execute_native_assembly(
        r#"
        extern long absent_object;
        extern void absent_function(void);
        int main(void) {
            asm volatile("// %c0 %c1" : : "i"(&absent_object), "i"(absent_function));
            return 0;
        }
        "#,
        "symbolic-no-runtime-reference",
    );
}

#[test]
fn assembly_immediates_rematerialize_after_effectful_operand_control_flow() {
    for source in [
        r#"static int object; int effects; int f(int x){asm volatile("// %c0 %w1"::"i"(&object),"r"(x?++effects:--effects));return effects;}"#,
        r#"int effects; int f(int x){asm volatile("// %c0 %w1"::"i"(7),"r"(x?++effects:--effects));return effects;}"#,
        r#"static int object; int f(void){asm goto("// %c0 %l[done]"::"i"(&object): :done);return 0;done:return 1;}"#,
        r#"int f(void){asm volatile("// %c0"::"i"(&&done));done:return 0;}"#,
        r#"static int key; static inline __attribute__((always_inline)) int test(int *p){asm goto("// %c0 %l[done]"::"i"(p): :done);return 0;done:return 1;} int f(void){return test(&key);}"#,
    ] {
        let output = run(source, "--emit=asm");
        assert!(
            output.status.success(),
            "{source}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("%c"));
    }
}

#[test]
fn symbolic_immediates_do_not_admit_dynamic_addresses_or_numeric_only_constraints() {
    for source in [
        r#"void f(long *p){asm volatile(""::"i"(p));}"#,
        r#"extern long object; void f(void){asm volatile(""::"n"(&object));}"#,
        r#"void f(long x){asm volatile("// %c0"::"r"(x));}"#,
        r#"static inline __attribute__((always_inline)) void f(long *p){asm volatile(""::"i"(p));} void g(long *p){f(p);}"#,
    ] {
        let output = run(source, "--emit=asm");
        assert!(!output.status.success(), "{source}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3021"));
    }
}

#[test]
fn local_register_bindings_survive_ir_round_trip() {
    let output = run(FIXED_REGISTERS, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    for constraint in ["r@x8", "=r@x8", "+r@x8", "+&r@x8"] {
        assert!(text.contains(constraint), "missing {constraint}: {text}");
    }
    let module = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    assert_eq!(evocc_evoir::render_module(&module), text);
}

#[test]
#[cfg(target_arch = "aarch64")]
fn local_register_bindings_execute_inputs_outputs_and_both_goto_edges() {
    // Sealed GCC-14 and Clang-19, at both -O0 and -O2: exit 0.
    execute_native_assembly(FIXED_REGISTERS, "fixed-registers");
}

#[test]
fn local_register_bindings_refuse_conflicts_instead_of_dropping_them() {
    for source in [
        r#"void f(long n){register long a asm("x8")=n;asm(""::"r"(a):"x8");}"#,
        r#"void f(long n){register long a asm("r8")=n;asm(""::"r"(a):"x8");}"#,
        r#"void f(long n){register long a asm("x16")=n;asm(""::"r"(a));}"#,
        r#"void f(long n){register long a asm("r16")=n;asm(""::"r"(a));}"#,
        r#"long f(long n){register long a asm("x8");register long b asm("x9")=n;asm("":"=r"(a):"0"(b));return a;}"#,
    ] {
        let output = run(source, "--emit=asm");
        assert!(!output.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("binding"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn lowered_register_metadata_is_not_a_source_constraint_spelling() {
    let output = run(r#"void f(long n){asm(""::"r@x8"(n));}"#, "--emit=evoir");
    assert!(
        !output.status.success(),
        "lowered metadata must not extend the source vocabulary"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("E3021"));
}

const REGISTER_ALTERNATIVES: [&str; 12] = [
    "Ir", "Jr", "Kr", "Lr", "ir", "nr", "rI", "rJ", "rK", "rL", "ri", "rn",
];

#[test]
fn register_alternative_selects_the_existing_ir_and_machine_path() {
    let selected = REGISTER_ALTERNATIVE.replace("\"Ir\"", "\"r\"");
    for constraint in REGISTER_ALTERNATIVES {
        let source = REGISTER_ALTERNATIVE.replace("\"Ir\"", &format!("\"{constraint}\""));
        for mode in ["--emit=evoir", "--emit=asm"] {
            let actual = run(&source, mode);
            let expected = run(&selected, mode);
            assert!(
                actual.status.success(),
                "{constraint} {mode}: {}",
                String::from_utf8_lossy(&actual.stderr)
            );
            assert!(
                expected.status.success(),
                "{}",
                String::from_utf8_lossy(&expected.stderr)
            );
            assert_eq!(actual.stdout, expected.stdout, "{mode}");
            if mode == "--emit=evoir" {
                let module = evocc_evoir::parse_section9_module(&actual.stdout).unwrap();
                assert_eq!(
                    evocc_evoir::render_module(&module).as_bytes(),
                    actual.stdout
                );
                for target in [evocc_mir::Target::Linux, evocc_mir::Target::Darwin] {
                    evocc_mir::lower_module_for(target, &module, &|_| false)
                        .expect("machine lowering diagnostics");
                    evocc_mir::compile_module_for(target, &module, &|_| true).expect(
                        "selected constraint reaches allocation, verification and emission",
                    );
                }
            }
        }
    }
}

#[test]
#[cfg(target_arch = "aarch64")]
fn register_alternative_executes_after_native_assembly_and_linking() {
    for constraint in REGISTER_ALTERNATIVES {
        let source = REGISTER_ALTERNATIVE.replace("\"Ir\"", &format!("\"{constraint}\""));
        execute_native_assembly(&source, &format!("goto-{constraint}"));
    }
}

#[test]
#[cfg(target_arch = "aarch64")]
fn ordinary_register_alternative_executes_after_native_assembly_and_linking() {
    execute_native_assembly(
        &format!("#define ORDINARY_ASSEMBLY_ONLY\n{REGISTER_ALTERNATIVE}"),
        "ordinary",
    );
}

#[test]
#[cfg(target_arch = "aarch64")]
fn immediate_register_alternatives_match_the_measured_bit_operations() {
    // The six values are the sealed GCC-14/Clang-19 kr.c observations.
    execute_native_assembly(
        r#"
        static unsigned long andnot(unsigned long v, unsigned long i) {
            unsigned long r; asm("bic %0, %1, %2" : "=r"(r) : "r"(v), "Kr"(i)); return r;
        }
        static unsigned long either(unsigned long v, unsigned long i) {
            unsigned long r; asm("orr %0, %1, %2" : "=r"(r) : "r"(v), "Lr"(i)); return r;
        }
        static unsigned long add(unsigned long v, unsigned long i) {
            unsigned long r; asm("add %0, %1, %2" : "=r"(r) : "r"(v), "ir"(i)); return r;
        }
        static unsigned long sub(unsigned long v, unsigned long i) {
            unsigned long r; asm("sub %0, %1, %2" : "=r"(r) : "r"(v), "Jr"(i)); return r;
        }
        volatile unsigned long dyn = 0xf0f0;
        int main(void) {
            return andnot(0xffff, 0xff) != 0xff00 || andnot(0xffff, dyn) != 0xf0f
                || either(1, 0xff00) != 0xff01 || add(5, 100) != 0x69
                || sub(5, 3) != 2 || add(5, dyn) != 0xf0f5;
        }
        "#,
        "logical-values",
    );
}

#[test]
fn register_alternatives_do_not_admit_outputs_or_wide_operands() {
    for constraint in REGISTER_ALTERNATIVES {
        for statement in [
            format!("asm(\"\" : \"={constraint}\"(x));"),
            format!("asm(\"\" : \"+{constraint}\"(x));"),
            format!("asm(\"\" : : \"{constraint}\"(wide));"),
        ] {
            let source =
                format!("void f(void){{unsigned long x=0;__uint128_t wide=1;{statement}}}");
            let output = run(&source, "--emit=evoir");
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{constraint}: {source}");
            assert!(
                error.contains("E3021") && error.contains(constraint),
                "{error}"
            );
        }
    }
}

#[cfg(target_arch = "aarch64")]
fn execute_native_assembly(source_text: &str, name: &str) {
    let target = if cfg!(target_os = "macos") {
        evocc_mir::Target::Darwin
    } else {
        evocc_mir::Target::Linux
    };
    let output = run(source_text, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let directory = std::env::temp_dir().join(format!(
        "evocc-register-alternative-{}-{name}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let source = directory.join("program.s");
    let program = directory.join("program");
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    let assembly = evocc_mir::compile_module_for(target, &module, &|_| true).unwrap();
    fs::write(&source, assembly).unwrap();
    let assembled = Command::new("clang")
        .arg(&source)
        .arg("-o")
        .arg(&program)
        .output()
        .unwrap();
    assert!(
        assembled.status.success(),
        "{}",
        String::from_utf8_lossy(&assembled.stderr)
    );
    let execution = Command::new(&program).output().unwrap();
    assert_eq!(
        execution.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&execution.stderr)
    );
    fs::remove_file(program).unwrap();
    fs::remove_file(source).unwrap();
    fs::remove_dir(directory).unwrap();
}

fn emits(source: &str) -> String {
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    evocc_evoir::parse_section9_module(text.as_bytes()).expect("emitted assembly is valid SSA");
    assert!(text.contains(" = asm"));
    text
}

#[test]
fn always_inline_substitutes_constant_arguments_before_assembly_validation() {
    let text = emits(
        r#"static __attribute__((always_inline)) inline int check(int n) {
            asm goto("" : : "i"(n) : : hit);
            return 0;
        hit:
            return 1;
        }
        int entry(void) { return check(3); }"#,
    );
    assert!(
        !text.contains("call @check"),
        "callee must be substituted: {text}"
    );
    emits(
        r#"static __attribute__((always_inline)) int f(int n){asm(""::"i"(n));return n;}int entry(void){(void)sizeof(f);return f(3);}"#,
    );
    emits(
        r#"static __attribute__((always_inline)) int f(int n){asm goto("b %l[hit]"::"i"(n)::hit);return 0;hit:return 1;}int entry(void){return f(3)+f(4);}"#,
    );
}

#[test]
fn inline_refusals_keep_the_immediate_and_cycle_boundaries() {
    for (source, code) in [
        (r#"static __attribute__((always_inline)) int f(int n){asm("":"+r"(n));asm(""::"i"(n));return n;}int entry(void){return f(3);}"#, "E3021"),
        (r#"static __attribute__((always_inline)) int f(int n){asm(""::"i"(n));return n;} int g(int n){return f(n);}"#, "E3021"),
        (r#"static __attribute__((always_inline)) int f(int n){asm(""::"i"(n));return n;} int (*p)(int)=f; int g(void){return f(3);}"#, "E3021"),
        ("static __attribute__((always_inline)) int f(int n){return n?f(n-1):0;} int main(void){return f(3);}", "E2932"),
    ] {
        let output = run(source, "--emit=evoir");
        assert!(!output.status.success(), "unexpected acceptance: {source}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(code), "{}", String::from_utf8_lossy(&output.stderr));
    }
}

#[test]
fn assembly_constraints_and_refusals() {
    let cases: &[(&str, &str, Option<&str>)] = &[
        (
            "register_input_output",
            r#"asm volatile("add %0, %1, %2" : "=r"(y) : "r"(x), "r"(x));"#,
            None,
        ),
        (
            "early_clobber_output",
            r#"asm("mov %0, %1" : "=&r"(y) : "r"(x));"#,
            None,
        ),
        (
            "readwrite_register",
            r#"asm("lsl %0, %0, #1" : "+r"(x));"#,
            None,
        ),
        (
            "readwrite_early_clobber",
            r#"asm("add %0, %0, %1" : "+&r"(x) : "r"(y));"#,
            None,
        ),
        (
            "tied_register",
            r#"asm("add %0, %0, #1" : "=r"(x) : "0"(x));"#,
            None,
        ),
        (
            "register_or_zero",
            r#"asm("orr %0, %1, %x2" : "=r"(y) : "r"(x), "rZ"(0UL));"#,
            None,
        ),
        (
            "memory_input",
            r#"asm("ldr %0, %1" : "=r"(y) : "Q"(*p));"#,
            None,
        ),
        (
            "memory_output",
            r#"asm("str %1, %0" : "=Q"(*p) : "r"(y));"#,
            None,
        ),
        (
            "memory_readwrite",
            r#"asm volatile("ldr x9, %0\n\tstr x9, %0" : "+Q"(*p) : : "x9");"#,
            None,
        ),
        (
            "offsettable_memory_input",
            r#"asm("ldr %0, %1" : "=r"(y) : "Qo"(*p));"#,
            None,
        ),
        (
            "add_immediate",
            r#"asm("add %0, %1, %2" : "=r"(y) : "r"(x), "I"(4095UL));"#,
            None,
        ),
        (
            "negated_add_immediate",
            r#"asm("add %0, %1, %2" : "=r"(y) : "r"(x), "J"(-4095L));"#,
            None,
        ),
        (
            "logical_immediate_word",
            r#"asm("and %0, %1, %2" : "=r"(y) : "r"(x), "K"(0xffU));"#,
            None,
        ),
        (
            "logical_immediate_doubleword",
            r#"asm("and %0, %1, %2" : "=r"(y) : "r"(x), "L"(0xffffffffUL));"#,
            None,
        ),
        (
            "constant_expression_immediate",
            r#"asm("mov %0, %1" : "=r"(y) : "i"(1 << 31));"#,
            None,
        ),
        (
            "integer_immediate",
            r#"asm("mov %0, %1" : "=r"(y) : "n"(7));"#,
            None,
        ),
        (
            "address_input",
            r#"asm("mov %0, %1" : "=r"(y) : "p"(p));"#,
            None,
        ),
        (
            "empty_volatile_memory_and_flags",
            r#"asm volatile("" : : : "memory", "cc");"#,
            None,
        ),
        (
            "unknown_register_constraint",
            r#"asm("" : "=q"(x));"#,
            Some("=q"),
        ),
        (
            "unlisted_constraint_combination",
            r#"asm("" : : "Jrr"(1));"#,
            Some("Jrr"),
        ),
        (
            "register_alternative_constant",
            r#"asm("cmp %0, %1" : : "r"(x), "Ir"(4097UL) : "cc");"#,
            None,
        ),
        (
            "register_alternative_dynamic",
            r#"asm("cmp %0, %1" : : "r"(x), "Ir"(y) : "cc");"#,
            None,
        ),
        (
            "register_alternative_output",
            r#"asm("" : "=Ir"(x));"#,
            Some("=Ir"),
        ),
        (
            "register_alternative_readwrite",
            r#"asm("" : "+Ir"(x));"#,
            Some("+Ir"),
        ),
        (
            "reversed_unlisted_combination",
            r#"asm("" : : "rII"(x));"#,
            Some("rII"),
        ),
        (
            "register_alternative_wide_input",
            r#"__uint128_t wide=1;asm("" : : "Ir"(wide));"#,
            Some("Ir"),
        ),
        (
            "unlisted_memory_output_combination",
            r#"asm("" : "=Qo"(*p));"#,
            Some("=Qo"),
        ),
        ("nonconstant_immediate", r#"asm("" : : "i"(x));"#, Some("i")),
        (
            "missing_tied_output",
            r#"asm("" : "=r"(x) : "9"(x));"#,
            Some("9"),
        ),
        (
            "zero_register_without_width",
            r#"asm("mov %0, %1" : "=r"(x) : "rZ"(0));"#,
            Some("rZ"),
        ),
        (
            "unknown_template_modifier",
            r#"asm("mov %q0, %q0" : "+r"(x));"#,
            Some("%q0"),
        ),
        (
            "unknown_physical_clobber",
            r#"asm("" : : : "notareg");"#,
            Some("notareg"),
        ),
    ];
    let mut failures = Vec::new();
    for (name, assembly, refused) in cases {
        let source = format!("int main(void) {{ unsigned long x=5, y=0; unsigned long *p=&x; {assembly} return 0; }}");
        let output = run(&source, "--emit=evoir");
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        let accepted = match refused {
            Some(constraint) => {
                !output.status.success()
                    && diagnostic.contains("E3021")
                    && diagnostic.contains(constraint)
            }
            None => {
                output.status.success()
                    && evocc_evoir::parse_section9_module(&output.stdout).is_ok()
                    && (*name != "constant_expression_immediate"
                        || !String::from_utf8_lossy(&output.stdout).contains("ub.assume"))
            }
        };
        if !accepted {
            failures.push(format!("{name}: {diagnostic}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn memory_constraints_do_not_add_scalar_load_or_writeback() {
    let text = emits(r#"int main(void) { int x=5; asm volatile("" : "+Q"(x)); return 0; }"#);
    assert_eq!(text.matches(" = load").count(), 0, "{text}");
    assert_eq!(
        text.matches(" = store").count(),
        1,
        "only initialize x: {text}"
    );
}

#[test]
fn named_assembly_outputs_and_goto_labels_survive_lowering() {
    let source = r#"int main(void) {
        int x=7, c=1;
        asm goto("cbnz %w[branch], %l[taken]\n\tadd %w[value], %w[value], #5"
                 : [value] "+r"(x) : [branch] "r"(c) : : taken);
        return x;
    taken: return x;
    }"#;
    let text = emits(source);
    assert!(text.contains(" = asm.goto "), "{text}");
    for retained in ["\"value\"", "\"branch\"", "\"taken\"", " fallthrough "] {
        assert!(text.contains(retained), "{retained}: {text}");
    }
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3021"));
    }
}

#[test]
fn interpreters_refuse_only_assembly_that_is_reached() {
    for condition in [0, 1] {
        let source = format!("int f(int c) {{ if(c) asm(\"nop\"); return 7; }} int main(void) {{return f({condition});}}");
        emits(&source);
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(&source, mode);
            if condition == 0 {
                assert!(
                    output.status.success(),
                    "{mode}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(output.stdout, b"7\n");
            } else {
                assert!(!output.status.success());
                assert!(String::from_utf8_lossy(&output.stderr).contains("E3021"));
            }
        }
    }
}
