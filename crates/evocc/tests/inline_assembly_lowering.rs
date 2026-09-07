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
            r#"asm("" : : "Ir"(1));"#,
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
