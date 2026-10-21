//! L05, with the rule decided on 2026-10-13: a record of one to four short
//! vectors (8 or 16 bytes) of one size is an AAPCS64 HVA, whatever its lane
//! types, and travels in vector registers as GCC 14 and Apple clang pass it.
//! Until L05, a record with a floating lane was `E3049` at every call
//! interface, because the general record rule would have passed it by
//! reference or in general registers. Now each interface lowers, with the
//! `hva` mark of the size-only classification. The crossed calls against
//! GCC 14, Clang 19 and Apple clang are in `floating_vectors.rs`.

use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const TYPES: &str = "typedef float f4 __attribute__((vector_size(16)));
typedef float f2 __attribute__((vector_size(8)));
typedef double d2 __attribute__((vector_size(16)));
typedef double d1 __attribute__((vector_size(8)));
typedef int i4 __attribute__((vector_size(16)));
typedef short s8 __attribute__((vector_size(16)));
struct quad { f4 a, b; };
struct pair { f2 a; };
struct wide { d2 a[3]; };
struct mixed { f4 a; i4 b; };
struct lanes { i4 a, b; };
struct shapes { i4 a; s8 b; };
struct halves { f2 a; d1 b; };
struct general { f4 a; int b; };
struct five { f2 a[5]; };
struct padded { f2 a; } __attribute__((aligned(16)));
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
fn records_of_floating_vectors_lower_at_every_call_interface() {
    for (body, mark) in [
        ("float take(struct quad v) { return v.a[0]; }", "hva=2xvec<4, bits<32>>"),
        (
            "struct pair give(void) { struct pair p = {0}; return p; }",
            "hva=1xvec<2, bits<32>>",
        ),
        (
            "extern void use(struct wide); void call(struct wide *p) { use(*p); }",
            "hva=3xvec<2, bits<64>>",
        ),
        (
            "extern struct quad make(void); float call(void) { struct quad q = make(); return q.b[3]; }",
            "hva=2xvec<4, bits<32>>",
        ),
        (
            "extern int take_mixed(struct mixed); int call(struct mixed *p) { return take_mixed(*p); }",
            "hva=2xvec<4, bits<32>>",
        ),
        (
            "int (*pointer)(struct pair); int call(struct pair *p) { return pointer(*p); }",
            "hva=1xvec<2, bits<32>>",
        ),
        (
            "extern int vf(int, ...); int call(struct quad *p) { return vf(1, *p); }",
            "hva=2xvec<4, bits<32>>",
        ),
        (
            "int vf(int n, ...) { __builtin_va_list ap; __builtin_va_start(ap, n); \
             struct pair p = __builtin_va_arg(ap, struct pair); __builtin_va_end(ap); \
             return n + (int)p.a[1]; }",
            "hva=1xvec<2, bits<32>>",
        ),
        // AAPCS64 tests only the short-vector size: two lane shapes of one
        // size make an HVA, integer lanes included.
        (
            "int take(struct shapes v) { return v.a[0] + v.b[7]; }",
            "hva=2xvec<4, bits<32>>",
        ),
        (
            "double take(struct halves v) { return v.a[1] + v.b[0]; }",
            "hva=2xvec<2, bits<32>>",
        ),
    ] {
        for target in TARGETS {
            let output = run(target, body);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{target}: {body}\n{stderr}");
            let ir = String::from_utf8_lossy(&output.stdout);
            assert!(ir.contains(mark), "{target}: {body}: {mark}\n{ir}");
        }
    }
}

#[test]
fn records_that_are_no_hva_keep_the_composite_rules() {
    for body in [
        // Integer lanes of one shape form an HVA, as before.
        "int take(struct lanes v) { return v.a[0]; }",
        // A floating vector beside a scalar makes no HVA: the general record
        // rule is the platform rule.
        "int take(struct general *v) { return v->b; }",
        "extern void use(struct general); void call(struct general *p) { use(*p); }",
        // Five members make no HVA: the record travels by reference.
        "extern void use(struct five); void call(struct five *p) { use(*p); }",
        // Padding makes no HVA, as in GCC 14: the record travels in general
        // registers.
        "extern void use(struct padded); void call(struct padded *p) { use(*p); }",
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
            let ir = String::from_utf8_lossy(&output.stdout);
            if !body.contains("lanes") {
                assert!(!ir.contains("hva="), "{target}: {body}\n{ir}");
            }
        }
    }
}
