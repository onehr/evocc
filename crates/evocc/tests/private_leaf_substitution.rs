use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(source: &str, level: &str, mode: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([
            "--target=aarch64-unknown-linux-gnu",
            level,
            mode,
            "/dev/stdin",
        ])
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

fn ir(source: &str) -> String {
    let output = run(source, "-O2", "--emit=evoir");
    assert!(
        output.status.success(),
        "source: {source}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    evocc_evoir::validate_module(&module).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

const SOURCE: &str = include_str!("../../../tests/fixtures/private-leaf-substitution.c");

#[test]
fn single_site_inline_visitor_can_call_its_out_of_line_caller() {
    let source = include_str!("../../../tests/fixtures/recursive-inline-visitor.c");
    let text = ir(source);
    assert!(
        !text.contains("call @visit"),
        "retained single-site visitor"
    );
    assert!(!text.contains("func @visit("), "unused visitor definition");
    assert!(text.contains("call @walk"), "recursive walk was lost");
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let output = run(source, level, mode);
            assert!(
                output.status.success(),
                "{level} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(output.stdout, b"0\n", "{level} {mode}");
            }
        }
    }
}

#[test]
fn optional_inline_hints_honor_barriers_across_the_declaration_chain() {
    for attribute in ["noinline", "noipa", "noclone"] {
        for before in [true, false] {
            let declaration = format!("static int f(int) __attribute__(({attribute}));");
            let definition = "static inline int f(int x){return x+1;}";
            let source = if before {
                format!("{declaration}{definition} int g(int x){{return f(x);}}")
            } else {
                format!("{definition}{declaration} int g(int x){{return f(x);}}")
            };
            let text = ir(&source);
            assert!(
                text.contains("call @f"),
                "ignored {attribute} before={before}"
            );
        }
    }
}

#[test]
fn recursive_single_site_admission_stays_bounded() {
    let prefix = "static unsigned walk(unsigned);";
    for (visitor, caller, extra) in [
        ("static inline unsigned visit(unsigned n){return n?walk(n-1):0;}",
         "static unsigned walk(unsigned n){return visit(n)+visit(n);}", ""),
        ("static inline unsigned visit(unsigned n){return n?walk(n-1):0;}",
         "static inline unsigned walk(unsigned n){return visit(n);}", ""),
        ("static inline unsigned visit(unsigned n){return n?walk(n-1):0;}",
         "static unsigned walk(unsigned n){return visit(n);}",
         "unsigned (*address(void))(unsigned){return visit;}"),
        ("static inline unsigned visit(unsigned n){static unsigned calls;++calls;return n?walk(n-1):calls;}",
         "static unsigned walk(unsigned n){return visit(n);}", ""),
        ("static inline unsigned visit(unsigned n){volatile unsigned char a[257];a[0]=n;return n?walk(n-1):a[0];}",
         "static unsigned walk(unsigned n){return visit(n);}", ""),
        ("static inline unsigned visit(unsigned n){int a[n+1];a[0]=n;return n?walk(n-1):a[0];}",
         "static unsigned walk(unsigned n){return visit(n);}", ""),
        ("static inline unsigned visit(unsigned n){int*p=__builtin_alloca(4);*p=n;return n?walk(n-1):*p;}",
         "static unsigned walk(unsigned n){return visit(n);}", ""),
        ("static inline unsigned visit(unsigned n){return n?walk(n-1):(unsigned long)__builtin_frame_address(0);}",
         "static unsigned walk(unsigned n){return visit(n);}", ""),
    ] {
        let source = format!("{prefix}{visitor}{caller}{extra} int main(void){{return walk(3);}}");
        assert!(ir(&source).contains("call @visit"), "unexpected admission: {source}");
    }
    let expression = std::iter::repeat_n("n += 1;", 350).collect::<String>();
    let source = format!("{prefix} static inline unsigned visit(unsigned n){{{expression} return n?walk(n-1):0;}} static unsigned walk(unsigned n){{return visit(n);}} int main(void){{return walk(1);}}");
    assert!(
        ir(&source).contains("call @visit"),
        "unbounded single-site body"
    );
}

#[test]
fn single_site_objects_and_reads_keep_their_dynamic_lifetimes() {
    for (source, code) in [
        ("static int *saved; static int walk(int); static inline int visit(int n){int x=n;saved=&x;return n?walk(n-1):0;} static int walk(int n){return visit(n);} int main(void){walk(0);return *saved;}", "E3010"),
        ("static int walk(int); static inline int visit(int n){int x;return n?walk(n-1):x;} static int walk(int n){return visit(n);} int main(void){return walk(1);}", "E3001"),
        ("static int walk(int); static inline int visit(int n){return n?walk(n-1):1/n;} static int walk(int n){return visit(n);} int main(void){return walk(1);}", "E3002"),
    ] {
        assert!(!ir(source).contains("call @visit"));
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, "-O2", mode);
            assert!(!output.status.success(), "{mode}: {source}");
            assert!(String::from_utf8_lossy(&output.stderr).contains(code), "{mode}: {}",
                    String::from_utf8_lossy(&output.stderr));
        }
    }
}

#[test]
fn explicit_inline_values_keep_complete_initializers_and_local_ssa() {
    for source in [
        include_str!("../../../tests/fixtures/inline-value-transport.c"),
        include_str!("../../../tests/fixtures/inline-complex-parameter.c"),
    ] {
        ir(source);
        for level in ["-O0", "-O2"] {
            for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
                let output = run(source, level, mode);
                assert!(
                    output.status.success(),
                    "{level} {mode}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                if mode.starts_with("--interpret") {
                    assert_eq!(output.stdout, b"0\n");
                }
            }
        }
    }
}

#[test]
fn short_private_leaves_reuse_explicit_inline_substitution() {
    let text = ir(SOURCE);
    for name in ["continue_walk", "leaf_mask", "read_once", "checked_add"] {
        assert!(
            !text.contains(&format!("call @{name}")),
            "retained leaf call: {name}"
        );
        assert!(
            !text.contains(&format!("func @{name}(")),
            "unused leaf body: {name}"
        );
    }
    assert!(
        text.contains("ub.assume.signed_overflow"),
        "arithmetic guard was lost"
    );
}

#[test]
fn leaf_parameter_conversions_reads_and_argument_effects_remain_once() {
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let output = run(SOURCE, level, mode);
            assert!(
                output.status.success(),
                "{level} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(output.stdout, b"0\n", "{level} {mode}");
            }
        }
    }
}

#[test]
fn private_leaf_substitution_does_not_erase_execution_errors() {
    for (source, code) in [
        ("static int add(int x,int y){return x+y;} int main(void){volatile int x=2147483647;return add(x,1);}", "E3004"),
        ("static int read(int *p){return *p;} int main(void){int x;return read(&x);}", "E3001"),
        ("static int div(int x,int y){return x/y;} int main(void){volatile int x=0;return div(1,x);}", "E3002"),
        ("static int read(int *p){return *p;} int main(void){return read(0);}", "E3009"),
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, "-O2", mode);
            assert!(!output.status.success(), "{mode} {source}");
            assert!(String::from_utf8_lossy(&output.stderr).contains(code), "{mode} {source}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

#[test]
fn private_leaf_admission_is_bounded_and_does_not_remove_frame_identity() {
    for source in [
        "int f(int x){return x+1;} int g(int x){return f(x);}",
        "static int f(int x){return x+1;} int(*address(void))(int){return f;} int g(int x){return f(x);}",
        "static __attribute__((noinline)) int f(int x){return x+1;} int g(int x){return f(x);}",
        "static int f(int) __attribute__((noinline)); static int f(int x){return x+1;} int g(int x){return f(x);}",
        "static int f(int x){return x+1;} static int f(int) __attribute__((noinline)); int g(int x){return f(x);}",
        "static __attribute__((noipa)) int f(int x){return x+1;} int g(int x){return f(x);}",
        "static __attribute__((noclone)) int f(int x){return x+1;} int g(int x){return f(x);}",
        "static void *f(void){return __builtin_frame_address(0);} void *g(void){return f();}",
        "static void *f(void){return __builtin_return_address(0);} void *g(void){return f();}",
        "static int *f(int x){return &x;} int *g(int x){return f(x);}",
        "static int f(int x){asm volatile(\"\" : \"+r\"(x));return x;} int g(int x){return f(x);}",
        "static int f(int x){while(x>3)x--;return x;} int g(int x){return f(x);}",
        "static int f(int x){static int n;n+=x;return n;} int g(int x){return f(x);}",
        "static int f(int x){int n=x;return n;} int g(int x){return f(x);}",
        "static int f(int x){return x?f(x-1):0;} int g(int x){return f(x);}",
        "static int f(int x){return x+1;} int a(int) __attribute__((alias(\"f\"))); int g(int x){return f(x);}",
    ] {
        let text = ir(source);
        assert!(text.contains("call @f"), "unsafe leaf admission: {source}\n{text}");
        assert!(text.contains("func @f("), "lost definition: {source}\n{text}");
    }
    // A leaf is not automatically cheap: keep the initial cost bound finite.
    let expression = std::iter::repeat_n("x", 80).collect::<Vec<_>>().join("+");
    let source = format!(
        "static unsigned f(unsigned x){{return {expression};}} unsigned g(unsigned x){{return f(x);}}"
    );
    assert!(ir(&source).contains("call @f"), "unbounded leaf admission");
}
