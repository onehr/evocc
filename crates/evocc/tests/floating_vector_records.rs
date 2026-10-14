//! The 2026-10-13 decision on L05: a record that AAPCS64 makes an HVA and
//! that holds a vector with floating lanes travels in vector registers under
//! GCC and Apple clang. EvoCC has no floating vector values yet, and its
//! general record rule would pass such a record by reference or in general
//! registers, a silent ABI mismatch. Until L05 lands, every call interface
//! with such a record (parameter, result, anonymous argument or `va_arg`) is
//! `E3049` (`not-lowered: floating-call-interface`) on both targets.

use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const TYPES: &str = "typedef float f4 __attribute__((vector_size(16)));
typedef float f2 __attribute__((vector_size(8)));
typedef double d2 __attribute__((vector_size(16)));
typedef int i4 __attribute__((vector_size(16)));
struct quad { f4 a, b; };
struct pair { f2 a; };
struct wide { d2 a[3]; };
struct mixed { f4 a; i4 b; };
struct lanes { i4 a, b; };
struct general { f4 a; int b; };
";

fn run(target: &str, body: &str) -> Output {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "evocc-floating-vector-records-{}-{}.c",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, format!("{TYPES}{body}\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg(format!("--target={target}"))
        .arg("--emit=evoir")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    output
}

const TARGETS: [&str; 2] = ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"];

#[test]
fn records_of_floating_vectors_are_refused_at_every_call_interface() {
    for body in [
        "float take(struct quad v) { return 0; }",
        "struct pair give(void) { struct pair p = {0}; return p; }",
        "extern void use(struct wide); void call(struct wide *p) { use(*p); }",
        "extern struct quad make(void); void call(void) { struct quad q = make(); (void)&q; }",
        "extern int take_mixed(struct mixed); int call(struct mixed *p) { return take_mixed(*p); }",
        "int (*pointer)(struct pair); int call(struct pair *p) { return pointer(*p); }",
        "extern int vf(int, ...); int call(struct quad *p) { return vf(1, *p); }",
        "int vf(int n, ...) { __builtin_va_list ap; __builtin_va_start(ap, n); \
         struct pair p = __builtin_va_arg(ap, struct pair); __builtin_va_end(ap); \
         return n + (int)sizeof p; }",
    ] {
        for target in TARGETS {
            let output = run(target, body);
            assert_eq!(output.status.code(), Some(1), "{target}: {body}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("E3049") && stderr.contains("floating-call-interface"),
                "{target}: {body}\n{stderr}"
            );
            assert!(output.stdout.is_empty(), "{target}: {body}");
        }
    }
}

#[test]
fn records_that_are_no_floating_hva_keep_their_rules() {
    for body in [
        // Integer lanes form an HVA that the backend already passes.
        "int take(struct lanes v) { return v.a[0]; }",
        // A floating vector beside a scalar makes no HVA: the general record
        // rule is the platform rule.
        "int take(struct general *v) { return v->b; }",
        "extern void use(struct general); void call(struct general *p) { use(*p); }",
        // A record of floating vectors that stays in memory crosses nothing.
        "int local(void) { struct quad q = {0}; struct quad r = q; return (int)sizeof r; }",
    ] {
        for target in TARGETS {
            let output = run(target, body);
            assert!(
                output.status.success(),
                "{target}: {body}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
