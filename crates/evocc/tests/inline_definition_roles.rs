//! Inline bodies and linkable definitions are different source-level roles.
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(source: &str, mode: &str, flags: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", mode])
        .args(flags)
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

fn ir(source: &str, flags: &[&str]) -> String {
    let out = run(source, "--emit=evoir", flags);
    assert!(
        out.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn declaration_chain_selects_the_definition_not_just_the_body_spelling() {
    let gnu = "__attribute__((gnu_inline))";
    let cases = [
        (
            format!("extern inline {gnu} int f(int x){{return x+1;}}"),
            false,
        ),
        (format!("inline {gnu} int f(int x){{return x+1;}}"), true),
        (
            format!("extern inline {gnu} int f(int x){{return x+1;}} inline {gnu} int f(int);"),
            true,
        ),
        (
            format!("inline {gnu} int f(int); extern inline {gnu} int f(int x){{return x+1;}}"),
            true,
        ),
        (
            format!("extern int f(int); extern inline {gnu} int f(int x){{return x+1;}}"),
            false,
        ),
        ("inline int f(int x){return x+1;}".into(), false),
        ("extern inline int f(int x){return x+1;}".into(), true),
        ("inline int f(int x){return x+1;} int f(int);".into(), true),
        ("int f(int); inline int f(int x){return x+1;}".into(), true),
    ];
    for (source, defined) in cases {
        let text = ir(
            &format!("{source} int (*address(void))(int){{return f;}}"),
            &[],
        );
        assert_eq!(text.contains("func @f("), defined, "{source}\n{text}");
        assert_eq!(text.contains("declare @f("), !defined, "{source}\n{text}");
        assert!(text.contains("func_addr @f"), "{text}");
    }
}

#[test]
fn keyword_macro_expansion_preserves_the_inline_definition_role() {
    let header = "#define __gnu_inline __attribute__((__gnu_inline__))\n#define inline inline __gnu_inline __attribute__((unused))\n";
    for (storage, defined) in [("", true), ("extern ", false), ("static ", true)] {
        let source = format!(
            "{header}{storage}inline int f(int x){{return x+1;}} int (*address(void))(int){{return f;}}"
        );
        let text = ir(&source, &[]);
        assert_eq!(text.contains("func @f("), defined, "{source}\n{text}");
        assert_eq!(text.contains("declare @f("), !defined, "{source}\n{text}");
        assert!(text.contains("func_addr @f"), "{text}");
    }
}

#[test]
fn address_reference_does_not_disable_direct_always_inline_substitution() {
    let source = "extern inline __attribute__((gnu_inline,always_inline)) int f(int x){return x+100;} int direct(int x){return f(x);} int(*address(void))(int){return f;} int main(void){return direct(2)!=102;}";
    let text = ir(source, &[]);
    assert!(!text.contains("func @f("), "{text}");
    assert!(text.contains("declare @f("), "{text}");
    assert!(text.contains("func_addr @f"), "{text}");
    assert!(!text.contains("call @f"), "{text}");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = run(source, mode, &[]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\n");
    }
}

#[test]
fn inline_only_body_cannot_satisfy_an_unexpanded_external_call() {
    let source = "extern inline __attribute__((gnu_inline)) int f(int x){return x+100;} int main(void){int(*p)(int)=f;return p(2);}";
    let text = ir(source, &[]);
    assert!(!text.contains("func @f("), "{text}");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = run(source, mode, &[]);
        assert!(!out.status.success(), "external body was invented: {mode}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("E3039"),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn small_inline_only_helpers_can_supply_direct_expansion_without_a_local_definition() {
    let source = "struct item { int value; struct item *next; }; inline int value(struct item *p){return p->value;} inline int sum(struct item *p){int n=0;while(p){n+=value(p);p=p->next;}return n;} int main(void){struct item b={4,0},a={3,&b};return sum(&a)!=7;}";
    let text = ir(source, &["-O2"]);
    assert!(!text.contains("call @sum"), "{text}");
    assert!(!text.contains("call @value"), "{text}");
    assert!(!text.contains("func @sum("), "{text}");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = run(source, mode, &["-O2"]);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\n");
    }
    // An explicit noinline body remains an external reference, not a hidden
    // substitute for a missing external implementation.
    let text = ir(
        "inline __attribute__((noinline)) int f(int x){return x+1;} int main(void){return f(2);}",
        &["-O2"],
    );
    assert!(text.contains("call @f"), "{text}");
    assert!(!text.contains("func @f("), "{text}");
}

#[test]
fn ordinary_definition_can_coexist_with_a_gnu_inline_body_but_not_another_definition() {
    let source = "extern inline __attribute__((gnu_inline)) int f(int x){return x+100;} int f(int x){return x+7;} int main(void){int(*p)(int)=f;return p(2)!=9;}";
    let text = ir(source, &[]);
    assert_eq!(text.matches("func @f(").count(), 1, "{text}");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = run(source, mode, &[]);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\n");
    }
    for bad in [
        "int f(void){return 1;} int f(void){return 2;}",
        "int f(void){return 1;} extern inline __attribute__((gnu_inline)) int f(void){return 2;}",
        "extern inline __attribute__((gnu_inline)) int f(void){return 1;} extern inline __attribute__((gnu_inline)) int f(void){return 2;}",
    ] {
        let out = run(bad, "--parse-only", &[]);
        assert!(String::from_utf8_lossy(&out.stderr).contains("E2002"));
    }
}

#[test]
fn each_body_keeps_its_own_local_identity_and_reference_closure() {
    let source = include_str!("../../../tests/fixtures/gnu-inline-definition-chain.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = run(source, mode, &[]);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\n");
    }
}

#[test]
fn inline_dialect_flags_and_per_function_attributes_do_not_leak() {
    for flags in [&["-std=gnu89"][..], &["-std=gnu11", "-fgnu89-inline"][..]] {
        let text = ir(
            "extern inline int f(int x){return x;} int(*address(void))(int){return f;}",
            flags,
        );
        assert!(!text.contains("func @f("), "{text}");
        assert!(text.contains("declare @f("), "{text}");
    }
    let text = ir("extern inline __attribute__((gnu_inline)) int f(int x){return x;} extern inline int g(int x){return x;} int(*address(void))(int){return f;}", &[]);
    assert!(!text.contains("func @f("));
    assert!(text.contains("func @g("));
    let text = ir(
        "static inline int f(int x){return x;} int(*address(void))(int){return f;}",
        &[],
    );
    assert!(text.contains("func @f(") && text.contains("visibility=internal"));
    let out = run("#if !defined(__GNUC_GNU_INLINE__) || defined(__GNUC_STDC_INLINE__) || __STDC_VERSION__ != 201112L\n#error wrong inline mode\n#endif\nint main(void){return 0;}", "--parse-only", &["-std=gnu11", "-fgnu89-inline"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // GCC rejects this combination while Clang accepts it; retain the
    // profile's refusal boundary rather than silently choosing ISO semantics.
    let out = run(
        "int x;",
        "--parse-only",
        &["-std=gnu89", "-fno-gnu89-inline"],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("-fno-gnu89-inline"));
    let out = run(
        "int x;",
        "--parse-only",
        &["-std=gnu89", "-fno-gnu89-inline", "-std=gnu11"],
    );
    assert!(out.status.success(), "later language mode must take effect");
}

#[test]
fn explicit_gnu99_keeps_its_version_and_iso_inline_semantics() {
    let source = include_str!("../../../tests/fixtures/gnu99-language-mode.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let out = run(source, mode, &["-std=gnu99"]);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(out.stdout, b"0\n");
        }
    }
    let text = ir("extern inline int f(int x){return x;} inline int g(int x){return x;} int(*address(void))(int){return g;}", &["-std=gnu99"]);
    assert!(text.contains("func @f("));
    assert!(!text.contains("func @g("));
    assert!(text.contains("declare @g("));
    let out = run(
        "int main(void){return undeclared();}",
        "--parse-only",
        &["-std=gnu99"],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("E2001"));
    for flags in [
        &["-std=gnu11", "-std=gnu99"][..],
        &["-std=gnu89", "-fno-gnu89-inline", "-std=gnu99"][..],
    ] {
        let out = run(source, "--parse-only", flags);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let out = run(
        "#if __STDC_VERSION__ != 201112L\n#error wrong final mode\n#endif\nint x;",
        "--parse-only",
        &["-std=gnu99", "-std=gnu11"],
    );
    assert!(out.status.success());
}

#[test]
fn iso_c89_selects_legacy_declarations_and_its_own_keyword_set() {
    let source = include_str!("../../../tests/fixtures/iso-c89-language-mode.c");
    for flag in ["-std=c89", "-std=c90", "-ansi"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let out = run(source, mode, &[flag]);
            assert!(
                out.status.success(),
                "{flag} {mode}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(out.stdout, b"0\n");
            }
        }
        let text = ir("extern __inline__ int f(int x){return x;} __inline__ int g(int x){return x;} int(*address(void))(int){return f;}", &[flag]);
        assert!(!text.contains("func @f("), "{text}");
        assert!(
            text.contains("declare @f(") && text.contains("func @g("),
            "{text}"
        );
        let out = run(
            "int main(void){for(int i=0;i<2;i++){}return 0;}",
            "--parse-only",
            &[flag],
        );
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("E1001"));
    }
    let out = run(
        "int restrict=3; int main(void){return restrict-3;}",
        "--interpret-evoir",
        &["-std=gnu89"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"0\n");
    let out = run("#if defined(__STRICT_ANSI__) || __STDC_VERSION__ != 201112L\n#error stale mode\n#endif\nint x;", "--parse-only", &["-ansi", "-std=gnu11"]);
    assert!(out.status.success());
}

#[test]
fn iso_c99_changes_macros_and_keyword_roles_not_just_the_option_parser() {
    let source = include_str!("../../../tests/fixtures/iso-c99-language-mode.c");
    let out = run(source, "--emit=asm", &["-std=c99"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = ir("extern inline int f(int x){return x;} inline int g(int x){return x;} int(*address(void))(int){return g;}", &["-std=c99"]);
    assert!(text.contains("func @f("));
    assert!(!text.contains("func @g("));
    for source in [
        "typeof(1) x;",
        "asm(\"nop\");",
        "int main(void){return undeclared();}",
    ] {
        let out = run(source, "--parse-only", &["-std=c99"]);
        assert!(!out.status.success(), "{source}");
    }
    let out = run("#if defined(__STRICT_ANSI__) || __STDC_VERSION__ != 199901L\n#error stale ISO mode\n#endif\nint x;", "--parse-only", &["-std=c99", "-std=gnu99"]);
    assert!(out.status.success());
    let out = run("#if __STDC_VERSION__ != 7 || __STRICT_ANSI__ != 2\n#error lost macro override\n#endif\nint x;", "--parse-only", &["-std=c99", "-D__STDC_VERSION__=7", "-D__STRICT_ANSI__=2"]);
    assert!(out.status.success());
}

#[test]
fn c11_and_gnu17_keep_their_keyword_and_version_contracts() {
    let source = include_str!("../../../tests/fixtures/language-mode-versions.c");
    for flags in [
        ["-std=c11", "-DEXPECT_VERSION=201112L", "-DEXPECT_ISO=1"],
        ["-std=gnu17", "-DEXPECT_VERSION=201710L", "-DEXPECT_ISO=0"],
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let out = run(source, mode, &flags);
            assert!(
                out.status.success(),
                "{flags:?} {mode}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(out.stdout, b"0\n");
            }
        }
    }
}

#[test]
fn language_mode_controls_literal_tokens_before_macro_expansion() {
    for mode in ["-std=c89", "-std=gnu89", "-std=gnu99", "-std=c99"] {
        for prefix in ["u8", "u", "U"] {
            let source = format!("int main(void){{return sizeof({prefix}\"x\");}}");
            let out = run(&source, "--parse-only", &[mode]);
            assert!(!out.status.success(), "{mode} admitted {prefix} literal");
            // Before C11, the prefix is an identifier and can be expanded.
            let source =
                format!("#define {prefix}\nint main(void){{return sizeof({prefix}\"x\")!=2;}}");
            let out = run(&source, "--interpret-evoir", &[mode]);
            assert!(
                out.status.success(),
                "{mode}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.stdout, b"0\n");
            let source = format!(
                "#define CAT(a,b) a##b\nint main(void){{return sizeof(CAT({prefix},\"x\"));}}"
            );
            let out = run(&source, "--parse-only", &[mode]);
            assert!(!out.status.success(), "{mode} pasted a non-token");
            assert!(String::from_utf8_lossy(&out.stderr).contains("E0110"));
        }
        // Command-line replacement tokens follow the same lexical mode.
        let out = run(
            "int main(void){return sizeof(S)!=2;}",
            "--interpret-evoir",
            &[mode, "-Du8=", "-DS=u8\"x\""],
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\n");
    }
    for mode in ["-std=c11", "-std=gnu11", "-std=gnu17"] {
        let out = run(
            "#define u8 invalid\nint main(void){return sizeof(u8\"x\")!=2;}",
            "--interpret-evoir",
            &[mode],
        );
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\n");
    }
}

#[test]
fn gnu89_does_not_silently_admit_a_c99_for_declaration() {
    let source = "int main(void){for(int i=0;i<2;i++){}return 0;}";
    let out = run(source, "--parse-only", &["-std=gnu89"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("E1001"));
    for mode in [
        "-std=gnu99",
        "-std=c99",
        "-std=c11",
        "-std=gnu11",
        "-std=gnu17",
    ] {
        let out = run(source, "--interpret-evoir", &[mode]);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\n");
    }
}
