//! Inlined return storage is mutable; source pointee qualifications are not.
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(source: &str, target: &str, level: &str, mode: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target", target, level, mode, "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn qualified_results_keep_their_values_and_execute_each_effect_once() {
    let cases = [
        "struct Pair { const int a; int b; }; static int state = 3;
         static inline __attribute__((always_inline)) const struct Pair value(int step) {
             struct Pair result = { state, step }; state += step; return result;
         }
         int main(void) { struct Pair p = value(2); struct Pair q = value(1);
             return p.a != 3 || p.b != 2 || q.a != 5 || q.b != 1 || state != 6; }",
        "static int state = 3;
         static inline __attribute__((always_inline)) const int value(int step) {
             state += step; if (step == 1) return state; return state + 1;
         }
         int main(void) { return value(1) != 4 || value(2) != 7 || state != 6; }",
        "static int state = 3;
         static inline __attribute__((always_inline)) const volatile int value(void) {
             ++state; return state;
         }
         int main(void) { return value() != 4 || value() != 5 || state != 5; }",
        "static int state = 3;
         static inline __attribute__((always_inline)) int * const value(int step) {
             state += step; return &state;
         }
         int main(void) { int *p = value(1); *p += 2;
             return p != &state || *value(3) != 9 || state != 9; }",
        "static int state = 3;
         static inline __attribute__((always_inline)) const int * const value(void) {
             ++state; return &state;
         }
         int main(void) { const int *p = value();
             return p != &state || *p != 4 || *value() != 5 || state != 5; }",
        "struct Pair { int a; int b; }; static int state = 3;
         static inline __attribute__((always_inline)) const struct Pair value(int step) {
             struct Pair result = { state, step }; state += step; return result;
         }
         int main(void) { struct Pair p = value(2); struct Pair q = value(1);
             return p.a != 3 || p.b != 2 || q.a != 5 || q.b != 1 || state != 6; }",
    ];
    for target in ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        for level in ["-O0", "-O2"] {
            for source in cases {
                for mode in [
                    "--interpret-c-hir",
                    "--interpret-evoir",
                    "--emit=evoir",
                    "--emit=asm",
                ] {
                    let output = run(source, target, level, mode);
                    assert!(
                        output.status.success(),
                        "{target} {level} {mode}: {source}\n{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    if mode.starts_with("--interpret") {
                        assert_eq!(output.stdout, b"0\n", "{target} {level} {mode}: {source}");
                    } else if mode == "--emit=evoir" {
                        let text = String::from_utf8_lossy(&output.stdout);
                        assert!(
                            !text.contains("call @value"),
                            "required inline call remains: {text}"
                        );
                        let module =
                            evocc_evoir::parse_target_module(&output.stdout, target).unwrap();
                        evocc_evoir::validate_module(&module).unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn removing_result_storage_qualifiers_does_not_make_a_const_pointee_writable() {
    let source = "static int state;
        static inline __attribute__((always_inline)) const int * const value(void) { return &state; }
        int main(void) { *value() = 7; return 0; }";
    for target in ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        let output = run(source, target, "-O2", "--emit=evoir");
        assert!(
            !output.status.success(),
            "const pointee became writable: {target}"
        );
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("E3101"),
            "source constraint reached malformed lowering: {output:?}"
        );
    }
}
