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
    let source = "int main(void){static const int value=7; return !__builtin_constant_p(value);}";
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
        "static int f(int x){return __builtin_constant_p(x);}int main(void){return !f(1);}",
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

#[test]
fn nested_numeric_pointer_conditions_keep_their_arithmetic_result() {
    let source = "extern void invalid(void) __attribute__((error(\"sign\")));int main(void){if(!((((char *)-1<(char *)1)?2:1)&(((int *)-1<(int *)1)?2:1)))invalid();return 0;}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
}

#[test]
fn immutable_automatic_constants_resolve_optimization_time_queries() {
    let source = "extern void invalid(void) __attribute__((error(\"value\")));static int f(void){const unsigned long last=41;if(last+1!=42)invalid();return __builtin_constant_p(last);}int main(void){return !f();}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
    assert_constant_program(source);
    for source in [
        "static int f(void){int x=1;++x;return x!=2;}int main(void){return f();}",
        "static int f(void){int x=1;int *p=&x;*p=3;return x!=3;}int main(void){return f();}",
        "static int f(int c){const int a=({if(c)goto end; 1;});int b=2;end:b=4;return b!=4;}int main(void){return f(1);}",
    ] { assert_constant_program(source); }
}

#[test]
fn optional_inline_hints_preserve_address_taken_label_identity() {
    let source = "static inline int f(int n){if(n)return 0;here:return (unsigned long)&&here!=0;}int main(void){return f(1);}";
    assert_constant_program(source);
}

#[test]
fn optimization_constant_queries_resolve_local_values_in_both_interpreters() {
    let source = include_str!("../../../tests/fixtures/optimization-constant-query.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"1\n", "{mode}");
    }
}

#[test]
fn optimization_queries_preserve_effects_and_resolve_kernel_request_values() {
    let source = include_str!("../../../tests/fixtures/optimization-query-effects.c");
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_required"));
}

#[test]
fn query_facts_do_not_initialize_objects_across_incoming_jumps() {
    let source = "int main(void){goto next;{int x=7;next:return __builtin_constant_p(x);}}";
    assert_constant_program(source);
}

#[test]
fn exhaustive_masked_switches_do_not_reference_an_impossible_default() {
    let source = "extern void impossible(void);int f(unsigned x){switch(x&1){case 0:return 7;case 1:return 9;default:impossible();return 3;}}int main(void){return f(0)!=7||f(1)!=9||f(2)!=7||f(~0u)!=9;}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @impossible"));
}

#[test]
fn masked_switch_folding_preserves_effects_fallthrough_and_uncovered_defaults() {
    for source in [
        "int f(unsigned x){switch(x&3){case 0:return 7;case 1:return 9;default:return 11;}}int main(void){return f(2)!=11||f(3)!=11;}",
        "int main(void){int x=2;int y=0;switch(x++&1){case 0:y=1;default:y+=2;break;case 1:y=7;}return x!=3||y!=3;}",
        "int f(unsigned x){switch(x&3){case 0 ... 2:return 7;case 3:return 9;default:return 4;}}int main(void){return f(2)!=7||f(3)!=9;}",
        "int main(void){int y=0;goto in;switch(y&1){case 0:return 3;case 1:return 4;default:in:return 0;}}",
    ] { assert_constant_program(source); }
}

#[test]
fn fresh_compound_literal_representation_reads_expose_layout_checks() {
    let source = include_str!("../../../tests/fixtures/constant-representation-reads.c");
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid_layout"));
}

#[test]
fn representation_facts_do_not_invent_padding_or_ignore_volatile_and_runtime_values() {
    for source in [
        "void observe(void);struct p{char a;int b;};int f(void){if(((unsigned char*)&(struct p){.a=3})[1]==0)observe();return 0;}",
        "void observe(void);struct p{char a;};int f(void){if(((volatile unsigned char*)&(struct p){.a=3})[0]==3)observe();return 0;}",
        "void observe(void);struct p{char a;};int f(int x){if(((unsigned char*)&(struct p){.a=x})[0]==3)observe();return 0;}",
    ] {
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("call @observe"));
    }
    assert_constant_program("struct p{char a;};int main(void){int n=0;int a=((unsigned char*)&(struct p){.a=++n})[0];return n!=1||a!=1;}");
}

#[test]
fn constant_facts_cross_internal_inline_exits_but_not_source_jumps() {
    let source = "void invalid(void);struct s{unsigned n;};static inline struct s*get(struct s*p){return p;}unsigned f(struct s*p){return({__auto_type a=get(p)->n;__auto_type b=2;if(!__builtin_constant_p(b))invalid();a<b?a:b;});}int main(void){struct s s={5};return f(&s)!=2;}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
}

#[test]
fn constant_scalar_literals_and_same_object_address_relations_prune_branches() {
    let source = "void invalid(void);struct list{void*next;void*prev;};static struct list node;int main(void){if((int){0}!=0||(int){1}!=1)invalid();if((struct list*)&node.prev<=&node||(struct list*)&node.prev>=&node+1)invalid();return 0;}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
}

#[test]
fn finite_conditional_results_prove_bounds_without_skipping_calls() {
    let source = "void invalid(void);static int calls;int choose(int n){++calls;return n;}int f(int n){if(58<((choose(n)?52:48)-12))invalid();return 0;}int main(void){f(0);f(1);return calls!=2;}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
}

#[test]
fn finite_facts_preserve_mixed_outcomes_conversions_and_runtime_errors() {
    for source in [
        "int f(int c){return (c?52:48)>50;}int main(void){return f(0)!=0||f(1)!=1;}",
        "int f(int c){return (unsigned char)(c?256:257)>0;}int main(void){return f(0)!=1||f(1)!=0;}",
        "int f(int c){return (c?-1:1)<0;}int main(void){return f(0)!=0||f(1)!=1;}",
    ] {
        assert_constant_program(source);
    }
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(
            "int f(int c){if(1/(c?1:0)>2)return 1;return 0;}int main(void){return f(0);}",
            mode,
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3002"));
    }
    for source in [
        "void observe(void);static int a,b;void f(void){if(&a<&b)observe();}",
        "void observe(void);static int a;void f(void){if((unsigned long)&a+~0ul<(unsigned long)&a)observe();}",
    ] {
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("call @observe"));
    }
}
