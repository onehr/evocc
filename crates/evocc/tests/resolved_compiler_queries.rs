use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(source: &str, mode: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["-O2", mode, "/dev/stdin"])
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

#[test]
fn inline_constant_queries_are_recomputed_after_parameter_substitution() {
    let source = include_str!("../../../tests/fixtures/inline-constant-query.c");
    let output = run(source, "--interpret-evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"0\n");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_required"));
}

#[test]
fn return_address_extraction_is_an_evaluated_identity_not_an_external_call() {
    let source = include_str!("../../../tests/fixtures/extract-return-address.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("__builtin_extract_return_addr"));
}

#[test]
fn constant_inline_returns_do_not_copy_unselected_local_declarations() {
    for expression in [
        "0 && ({ int local = 7; local; })",
        "1 || ({ int local = 7; local; })",
    ] {
        let expected = u8::from(expression.starts_with('1'));
        let source = format!("static inline int config(void){{return {expression};}} int main(void){{return config() != {expected};}}");
        let output = run(&source, "--interpret-evoir");
        assert!(
            output.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
}

#[test]
fn immutable_static_values_prune_false_error_calls_without_becoming_ices() {
    let source = include_str!("../../../tests/fixtures/static-constant-branch.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_failed"));
    let source = "int main(void){static const int value=7; _Static_assert(!__builtin_constant_p(value),\"profile predicate\");return __builtin_constant_p(value);}";
    let output = run(source, "--interpret-evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"0\n");
}

#[test]
fn static_value_propagation_preserves_escapes_volatile_reads_and_updates() {
    for source in [
        "int main(void){static const int value=7;const int *p=&value;return *p!=7;}",
        "int main(void){static const volatile int value=7;return value!=7;}",
        "static int f(void){static int value=5; ++value;return value;}int main(void){static const int expected=7;f();return f()!=expected;}",
        "int main(void){static const int value=7; int a=0; if(a) return 2; static const int later=8; return value+later!=15;}",
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            assert!(output.status.success(), "{source}: {mode}: {}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(output.stdout, b"0\n", "{source}: {mode}");
        }
    }
    let output = run(
        "int main(void){static const volatile int value=7;return value;}",
        "--emit=evoir",
    );
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("load volatile"));
}

#[test]
fn constant_inline_leaf_results_remove_false_error_calls() {
    let source = include_str!("../../../tests/fixtures/constant-call-results.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_failed"));
    let output = run(source, "--interpret-evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"0\n");
}

#[test]
fn common_private_parameter_values_preserve_calls_and_effects() {
    let source = include_str!("../../../tests/fixtures/private-constant-parameter.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_failed"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("call @map"));
    let output = run(source, "--interpret-evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"0\n");
}

#[test]
fn parameter_facts_do_not_escape_their_proven_callers_or_replace_effects() {
    for source in [
        "static int f(int x){return x;}int main(void){return f(1)+f(2)!=3;}",
        "static int f(int x){return x;}int main(void){int (*p)(int)=f;return f(1)+p(2)!=3;}",
        "static int f(int x){int *p=&x;*p=4;return x;}int main(void){return f(1)!=4;}",
        "static int f(int x){++x;return x;}int main(void){return f(1)!=2;}",
        "static int f(int x){return __builtin_constant_p(x);}int main(void){return f(1);}",
        "static __attribute__((always_inline)) inline int f(signed char x){return x;}int main(void){return f(255)!=-1;}",
        "static __attribute__((always_inline)) inline int f(int x){return x+x;}int main(void){int x=1;int y=f(x++);return x!=2||y!=2;}",
    ] {
        let output = run(source, "--interpret-evoir");
        assert!(output.status.success(), "{source}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, b"0\n", "{source}");
    }
    for source in [
        "static __attribute__((always_inline)) inline int f(volatile int x){return x;}int main(void){return f(7);}",
        "static int f(volatile int x){return x;}int main(void){return f(7);}",
    ] {
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("load volatile"));
    }
}

#[test]
fn constant_queries_are_ices_in_static_initializers() {
    assert_constant_program(include_str!(
        "../../../tests/fixtures/constant-query-initializers.c"
    ));
}

#[test]
fn constant_queries_are_ices_in_case_labels() {
    assert_constant_program(
        "int fallback(int x); int main(void){switch(32){case __builtin_constant_p(2)?2<<4:fallback(2):return 0;default:return 1;}}",
    );
}

#[test]
fn constant_queries_select_static_relocations_without_evaluating_the_other_arm() {
    assert_constant_program(
        "int fallback(int x); static int object=7;static int *p=(__builtin_constant_p(2)?2:fallback(2))?&object:0;int main(void){return *p!=7;}",
    );
}

fn assert_constant_program(source: &str) {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
}

#[test]
fn small_inline_helpers_specialize_distinct_constant_arguments() {
    let source = "extern void invalid_size(void) __attribute__((error(\"size\")));static inline void check(unsigned long n){if(n>64)invalid_size();}int main(void){check(16);check(48);return 0;}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid_size"));
    assert_constant_program(source);
}

#[test]
fn numeric_pointer_conditions_fold_with_unsigned_pointer_width() {
    let source = "extern void invalid_sign(void) __attribute__((error(\"sign\")));int main(void){if((unsigned char *)-1 < (unsigned char *)1)invalid_sign();if(!((void *)0<(void *)1))invalid_sign();return 0;}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid_sign"));
}

#[test]
fn optional_inlining_preserves_recursive_and_noinline_calls() {
    for source in [
        "static inline int f(int n){return n?f(n-1)+1:0;}int main(void){return f(3)!=3;}",
        "static __attribute__((noinline)) inline int f(int n){return n+1;}int main(void){return f(3)!=4;}",
        "static inline int f(void){static int n;return ++n;}int main(void){f();return f()!=2;}",
        "static inline int f(int n){return n+n;}int main(void){int n=2;int r=f(n++);return r!=4||n!=3;}",
    ] {
        assert_constant_program(source);
    }
    let output=run("static __attribute__((noinline)) inline int f(int n){return n+1;}int main(void){return f(3);}","--emit=evoir");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("call @f"));
}
