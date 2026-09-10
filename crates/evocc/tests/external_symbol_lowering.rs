use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn run(source: &str, mode: &str) -> Output {
    run_with_options(source, mode, &[])
}

fn run_with_options(source: &str, mode: &str, options: &[&str]) -> Output {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "evocc-external-{}-{}.c",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", mode])
        .args(options)
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    output
}

#[test]
fn hidden_symbol_metadata_reaches_declarations_definitions_and_addressing() {
    let source = include_str!("../../../tests/fixtures/symbol-sections.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    for prefix in [
        "extern @hidden_value ",
        "declare @hidden_function(",
        "global @local_definition ",
    ] {
        let row = text.lines().find(|line| line.starts_with(prefix)).unwrap();
        assert!(row.contains("visibility=hidden"), "{row}");
    }
    let output = run(source, "--emit=asm");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    for symbol in ["hidden_value", "hidden_function", "local_definition"] {
        assert!(text.contains(&format!(".hidden {symbol}")), "{text}");
    }
    assert!(!text.contains(":got:hidden_value"), "{text}");
    assert!(text.contains(":got:default_value"), "{text}");
}

#[test]
fn object_section_metadata_reaches_zero_bytes_and_symbol_relocations() {
    let source = include_str!("../../../tests/fixtures/symbol-sections.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let module = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    let machine =
        evocc_mir::lower_module_for(evocc_mir::Target::Linux, &module, &|_| false).unwrap();
    let machine_text = evocc_mir::render_module(&machine);
    let reread = evocc_mir::parse_module(&machine_text).unwrap();
    assert_eq!(
        reread.globals, module.globals,
        "machine text must not lose object metadata or relocations"
    );
    for (symbol, section) in [
        ("counter", ".data..percpu"),
        ("reference", ".data.rel.ro.refs"),
    ] {
        let row = text
            .lines()
            .find(|line| line.starts_with(&format!("global @{symbol} ")))
            .unwrap();
        assert!(row.contains(&format!(" section=\"{section}\"")), "{row}");
    }
    let output = run(source, "--emit=asm");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    for section in [".data..percpu", ".data.rel.ro.refs"] {
        assert!(
            text.contains(&format!(".section \"{section}\",\"aw\",%progbits")),
            "{text}"
        );
    }
    assert!(text.contains(".p2align 6"), "{text}");
    assert!(text.contains(".xword counter"), "{text}");
}

#[test]
fn conflicting_symbol_metadata_is_refused_instead_of_silently_selected() {
    let source = include_str!("../../../tests/fixtures/symbol-attribute-errors.c");
    for case in [1, 2, 3, 4, 5, 7] {
        let option = format!("-DCASE={case}");
        let output = run_with_options(source, "--emit=evoir", &[&option]);
        assert!(!output.status.success(), "case {case} must be refused");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E2943"),
            "case {case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn weak_hidden_symbol_metadata_retains_nullable_got_addressing() {
    let output = run_with_options(
        include_str!("../../../tests/fixtures/symbol-attribute-errors.c"),
        "--emit=asm",
        &["-DCASE=6"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains(".weak value") && text.contains(".hidden value"),
        "{text}"
    );
    assert!(text.contains(":got:value"), "{text}");
}

#[test]
fn block_external_storage_is_not_inferred_from_lexical_scope_or_spelling() {
    for source in [
        "char *f(void){extern char a[];return a;} int main(void){return f()==0;}",
        "int main(void){extern char a[];static char *p=a;return p!=a;}",
        "int main(void){extern int x;return &x==0;}",
        "void update(void){} void(*p)(void)=update;int f(int update){int*q=&update;return*q;}int main(void){return f(3)!=3;}",
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(output.stdout, b"0\n", "{mode}: {source}");
        }
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
        assert!(module.extern_objects.iter().all(|object| object.symbol != "update"));
    }
}

#[test]
fn extension_expression_prefixes_preserve_values_and_side_effects() {
    let source = r#"
        #define USE(x) __extension__ (x)
        int calls;
        int once(void) { return ++calls; }
        int main(void) {
            int x = __extension__ __extension__ once();
            __extension__ x++;
            int y = 2 * __extension__ (x + 1);
            for (int i = 0; i < 2; ++i) y++;
            for (__extension__ x = 0; x < 2; ++x) y++;
            _Static_assert(__extension__ 3 == 3, "constant");
            return USE(calls != 1 || x != 2 || y != 10);
        }
    "#;
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
    for source in [
        "int f(void){for(__extension__ int i=0;i<2;++i){}return 0;}",
        "int f(void){return __extension__ ;}",
    ] {
        assert!(!run(source, "--parse-only").status.success(), "{source}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "extension-expressions");
}

#[test]
fn compatible_record_casts_preserve_the_value_and_operand_effects() {
    let source = r#"
        struct S { int a; };
        union U { int a; unsigned b; };
        struct S once(int *calls) { ++*calls; return (struct S){42}; }
        int main(void) {
            int calls = 0;
            const struct S a = {42};
            struct S b = (struct S)a;
            struct S c = (struct S)once(&calls);
            union U u = {17};
            union U v = (union U)u;
            return b.a != 42 || c.a != 42 || calls != 1
                || ((struct S)b).a != 42 || v.a != 17;
        }
    "#;
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
        let output = run(
            "struct A{int x;}; struct B{int x;}; int main(void){struct A a={1};struct B b=(struct B)a;return b.x;}",
            mode,
        );
        assert!(!output.status.success(), "{mode}: unrelated record tags");
    }
    let output = run(
        "struct S{int x;}; int main(void){volatile struct S a={42};struct S b=(struct S)a;return b.x!=42;}",
        "--emit=evoir",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .matches("load volatile")
            .count(),
        1
    );
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "record-casts");
}

#[test]
fn flexible_record_elements_share_brace_elision_and_full_element_storage() {
    let declarations = r#"
        struct S { unsigned char a, b, c[2]; };
        struct W { int head; struct S s[]; };
        struct W w = {1, {1, 2, 3, 4, 5}};
        struct W z = {.s = {[1].c[1] = 9, 10}};
    "#;
    let source = format!(
        "{declarations}int main(void){{
            if(sizeof(struct W)!=4 || w.head!=1) return 1;
            if(w.s[0].a!=1 || w.s[0].c[1]!=4 || w.s[1].a!=5) return 2;
            if(w.s[1].b || w.s[1].c[0] || w.s[1].c[1]) return 3;
            if(z.s[0].a || z.s[1].c[1]!=9 || z.s[2].a!=10 || z.s[2].c[1]) return 4;
            return 0;
        }}"
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(&source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    let output = run(&source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    assert_eq!(
        module
            .globals
            .iter()
            .find(|g| g.symbol == "w")
            .unwrap()
            .layout
            .size_bytes,
        12
    );
    assert_eq!(
        module
            .globals
            .iter()
            .find(|g| g.symbol == "z")
            .unwrap()
            .layout
            .size_bytes,
        16
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(
            &format!("{declarations}int main(void){{return w.s[2].a;}}"),
            mode,
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3011"));
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(&source, "flexible-records");
}

#[test]
fn dereferenced_arrays_decay_without_loading_the_array_object() {
    let source = r#"
        typedef char Row[4];
        int evaluations;
        Row *once(Row *p) { evaluations++; return p; }
        char *first(Row *p) { return *p; }
        int main(void) {
            Row rows[2] = {{1, 2, 3, 4}, {5, 6, 7, 8}};
            Row *p = rows;
            char *a = *once(p++);
            if (evaluations != 1 || a[2] != 3 || p != &rows[1]) return 1;
            char *b = *(evaluations ? p : p - 1);
            b[3] = 42;
            if (first(p)[3] != 42) return 2;
            int cube[1][2][3] = {{{9, 10, 11}, {12, 13, 14}}};
            int (*q)[1][2][3] = &cube;
            return (*q)[0][1][2] != 14;
        }
    "#;
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {} {:?}",
                String::from_utf8_lossy(&output.stderr),
                output.stdout
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "dereferenced-arrays");
}

#[test]
fn unspecified_declarations_use_the_composite_call_interface() {
    let mut failures = Vec::new();
    for declarations in [
        "long f();long f(long);",
        "long f(long);long f();",
        "long f();extern long f(long);long f();",
    ] {
        let source =
            format!("{declarations}long entry(void){{long(*p)(long)=f;return p(1)+f(2);}}");
        let output = run(&source, "--emit=evoir");
        if !output.status.success() {
            failures.push(format!(
                "{source}: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
            continue;
        }
        let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
        let declaration = module.declares.iter().find(|f| f.name == "f").unwrap();
        // One source parameter, followed by the existing memory/world tokens.
        assert_eq!(declaration.params.len(), 3, "{source}");
    }
    let alias = "long f();static long impl(long n){return n+1;}extern __typeof(impl) f __attribute__((weak,alias(\"impl\")));int main(void){return f(41)!=42;}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(alias, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {} {:?}",
                String::from_utf8_lossy(&output.stderr),
                output.stdout
            ));
        }
    }
    for source in [
        "long f(int);long f(long);",
        "long f(void);long entry(void){return f(1);}",
    ] {
        assert!(!run(source, "--emit=evoir").status.success(), "{source}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn incomplete_array_alignment_preserves_member_attributes_without_evaluation() {
    let source = r#"
        struct Object { int tag; unsigned char bytes[] __attribute__((aligned(8))); };
        unsigned long query(void) { struct Object *pointer; return __alignof__(pointer->bytes); }
        int main(void) {
            struct Object *pointer = 0;
            if (_Alignof(int[]) != 4) return 1;
            if (__alignof__((pointer++)->bytes) != 8 || pointer != 0) return 2;
            return query() != 8;
        }
    "#;
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let output = run("int main(void){return sizeof(int[]);}", "--emit=evoir");
    assert!(
        !output.status.success(),
        "alignment must not invent a complete array layout"
    );
}

#[test]
fn globals_first_declared_in_a_block_keep_their_external_storage() {
    let source = include_str!("../../../tests/fixtures/block-extern-global.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "block-extern-global");
}

#[test]
fn prefetch_accepts_qualified_addresses_without_loading_the_object() {
    let source = include_str!("../../../tests/fixtures/qualified-prefetch.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let output = run(source, "--emit=evoir");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    // An unused declaration is not an executed call or a pointee load.
    assert!(!text.contains("call @__builtin_prefetch"), "{text}");
    assert_eq!(text.matches("load volatile").count(), 2, "{text}");
    assert_eq!(text.matches("store volatile").count(), 1, "{text}");
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "qualified-prefetch");

    for (source, code) in [
        ("void f(void) { __builtin_prefetch(); }", "E2909"),
        ("void f(void) { __builtin_prefetch(0, 0, 0, 0); }", "E2909"),
        ("void f(void) { __builtin_prefetch(1); }", "E2913"),
        (
            "void f(void) { struct S { int x; } s; __builtin_prefetch(s); }",
            "E2913",
        ),
        (
            "void f(void) { struct S { int x; } s; __builtin_prefetch(0, s); }",
            "E2913",
        ),
    ] {
        let output = run(source, "--parse-only");
        assert!(!output.status.success(), "accepted {source}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn deallocation_builtin_keeps_the_ordinary_external_call_contract() {
    let source = "static int calls; static void *argument(void) { ++calls; return (void *)0; } int main(void) { __builtin_free(argument()); return calls != 1; }";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("@free"));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "deallocation-builtin");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3039"));
    }
    for (source, code) in [
        ("void f(void) { __builtin_free(); }", "E2909"),
        ("void f(void) { __builtin_free(0, 0); }", "E2909"),
        ("void f(void) { __builtin_free(1); }", "E2913"),
        (
            "int free(void *); void f(void *p) { __builtin_free(p); }",
            "E2927",
        ),
    ] {
        let output = run(source, "--parse-only");
        assert!(!output.status.success(), "accepted {source}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = run("_Static_assert(__builtin_types_compatible_p(__typeof__(__builtin_free(0)), void), \"free returns void\");", "--parse-only");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn abrupt_value_expressions_preserve_only_executed_effects() {
    let source = include_str!("../../../tests/fixtures/abrupt-expression-values.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "abrupt-expression-values");
}

#[test]
fn variadic_record_results_preserve_promoted_arguments_on_the_machine() {
    let source = include_str!("../../../tests/fixtures/variadic-record-results.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "variadic-record-results");
}

#[test]
fn composite_function_declarations_preserve_calls_and_reject_conflicts() {
    let source = include_str!("../../../tests/fixtures/composite-function-declarations.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "composite-function-declarations");
    for invalid in [
        "int f(int); long f(int);",
        "int f(int); int f() { return 0; }",
        "int f(float); int f();",
    ] {
        let output = run(invalid, "--parse-only");
        assert!(!output.status.success(), "accepted {invalid}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("E2002"));
    }
}

#[test]
fn legacy_integer_declarations_require_the_explicit_language_mode() {
    let source = include_str!("../../../tests/fixtures/legacy-integer-declarations.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run_with_options(source, mode, &["-std=gnu89"]);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution_with_options(source, "legacy-integer-declarations", &["-std=gnu89"]);
    for options in [
        &[][..],
        &["-std=gnu11"][..],
        &["-std=gnu89", "-std=gnu11"][..],
    ] {
        let output = run_with_options(source, "--parse-only", options);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E1001"));
    }
}

#[test]
fn legacy_language_macros_follow_mode_before_user_overrides() {
    let source = "__STDC_VERSION__ __GNUC_STDC_INLINE__ __GNUC_GNU_INLINE__\n";
    for (options, expected) in [
        (
            vec!["-std=gnu89"],
            "__STDC_VERSION__ __GNUC_STDC_INLINE__ 1",
        ),
        (vec!["-std=gnu11"], "201112L 1 __GNUC_GNU_INLINE__"),
        (
            vec!["-std=gnu11", "-std=gnu89"],
            "__STDC_VERSION__ __GNUC_STDC_INLINE__ 1",
        ),
        (
            vec![
                "-std=gnu89",
                "-D__STDC_VERSION__=17",
                "-U__GNUC_GNU_INLINE__",
            ],
            "17 __GNUC_STDC_INLINE__ __GNUC_GNU_INLINE__",
        ),
    ] {
        let output = run_with_options(source, "-E", &options);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
    }
}

#[test]
fn legacy_declarations_preserve_missing_names_and_type_conflicts() {
    for (source, code) in [
        ("int main(void) { return missing_object; }", "E2001"),
        (
            "main() { { if (0) missing(); } return sizeof(missing); }",
            "E2001",
        ),
        (
            "int main(void) { return later(1); } long later(int x) { return x; }",
            "E2002",
        ),
        ("int f(a) int a; int a; { return a; }", "E2002"),
        ("int f(a) int absent; { return a; }", "E1001"),
        ("int main(void) { int later; return later(1); }", "E2937"),
        (
            "static const fixed = 3; int main(void) { fixed = 9; return fixed; }",
            "E2101",
        ),
    ] {
        let output = run_with_options(source, "--parse-only", &["-std=gnu89"]);
        assert!(!output.status.success(), "accepted {source}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn legacy_integer_specifiers_preserve_qualifiers_and_parameter_storage() {
    let source = "static const fixed = 3; int f(a, b) register a; volatile b; { return a + b + fixed; } int main(void) { return f(2, 4) != 9; }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run_with_options(source, mode, &["-std=gnu89"]);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution_with_options(source, "legacy-qualified-declarations", &["-std=gnu89"]);
}

#[test]
fn a_visible_object_is_not_an_implicit_function() {
    for options in [&[][..], &["-std=gnu89"][..]] {
        for source in [
            "int main(void) { int later; return later(1); }",
            "int main(void) { int *p; p(); return 0; }",
        ] {
            let output = run_with_options(source, "--parse-only", options);
            assert!(!output.status.success(), "accepted {source}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("E2937"));
        }
    }
}

#[test]
fn legacy_external_calls_keep_their_block_declarations_in_the_body() {
    for source in [
        "main() { if (0) abort(); return 0; }",
        "main() { { if (0) abort(); } return 0; }",
        "main() { return 0 ? abort() : 0; }",
        "main() { if (0) { exit(3); } return 0; }",
        "main() { typeof(({ missing(); 1; })) x; int missing = 5; return missing - 5; }",
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run_with_options(source, mode, &["-std=gnu89"]);
            assert!(
                output.status.success(),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, b"0\n");
        }
        #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
        assert_native_execution_with_options(source, "legacy-external-calls", &["-std=gnu89"]);
    }
}

#[test]
fn integer_sign_and_parity_counts_share_constant_and_runtime_semantics() {
    let source = include_str!("../../../tests/fixtures/integer-bit-counts.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "integer-bit-counts");
    let output = run(source, "--emit=evoir");
    assert!(output.status.success());
    let module = String::from_utf8(output.stdout).unwrap();
    assert!(module.contains("ctlz.bits<32>"));
    assert!(module.contains("ctpop.bits<64>"));
    assert!(!module.contains("bit_builtin_zero_operand"));
}

#[test]
fn integer_sign_and_parity_counts_keep_the_builtin_operand_contract() {
    for (source, code) in [
        ("int f(void) { return __builtin_clrsb(); }", "E2909"),
        ("int f(void) { return __builtin_parityl(1, 2); }", "E2909"),
        ("int f(void) { return __builtin_clrsbll; }", "E2910"),
        ("int f(int *p) { return __builtin_parity(p); }", "E2913"),
    ] {
        let output = run(source, "--parse-only");
        assert!(!output.status.success(), "accepted {source}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{code}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn flexible_string_initializers_execute_without_completing_the_record_type() {
    let source = include_str!("../../../tests/fixtures/flexible-string-initializers.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "flexible-string-initializers");
    for invalid in [
        "struct S { char head; char data[]; }; int main(void) { struct S s = { 'o', \"wx\" }; return s.data[1]; }",
        "struct S { int head; unsigned int data[]; }; int main(void) { struct S s = { 7, L\"wx\" }; return s.data[1]; }",
    ] {
        let output = run(invalid, "--parse-only");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E2919"));
    }
}

#[test]
fn obsolete_designators_use_the_existing_subobject_initializer_plan() {
    let source = include_str!("../../../tests/fixtures/obsolete-designators.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "obsolete-designators");
    for source in [
        "struct S { int a; }; struct S s = { absent: 1 };",
        "struct S { int a; }; int values[2] = { a: 1 };",
        "int values[2] = { [3] 1 };",
    ] {
        let output = run(source, "--parse-only");
        assert!(!output.status.success(), "accepted {source}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("E2906"));
    }
}

#[test]
fn nested_designator_continuation_does_not_skip_the_inner_sibling() {
    let source = "struct Inner { int a, b; }; struct Outer { int head; struct Inner inner; int tail; }; int main(void) { struct Outer value = { .inner.a = 11, 22 }; return value.head || value.inner.a != 11 || value.inner.b != 22 || value.tail; }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.stdout, b"0\n",
            "{mode}: inner.b must be 22, tail must be zero"
        );
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "nested-designator-continuation");
}

#[test]
fn designator_continuation_tracks_nested_paths_and_inferred_bounds() {
    let source = include_str!("../../../tests/fixtures/designator-continuation.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "designator-continuation");
}

#[test]
fn designated_aggregates_follow_ordinary_brace_elision() {
    let source = include_str!("../../../tests/fixtures/designator-brace-elision.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "designator-brace-elision");
}

#[test]
fn nested_initializer_continuation_retains_object_boundaries() {
    for (source, code) in [
        (
            "struct S { int a, b; }; struct S s = { .b = 1, 2 };",
            "E2201",
        ),
        (
            "struct S { int a[2]; }; struct S s = { .a[2] = 1 };",
            "E2906",
        ),
        (
            "struct S { int a[2]; }; struct S s = { .a[1] = 1, 2 };",
            "E2201",
        ),
        (
            "struct S { int h; int a[]; }; void f(void) { struct S s = { .h = 1, 2 }; }",
            "E2201",
        ),
    ] {
        let output = run(source, "--parse-only");
        assert!(!output.status.success(), "accepted {source}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn alignment_specifiers_share_object_and_member_layout() {
    let source = include_str!("../../../tests/fixtures/alignment-specifiers.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "alignment-specifiers");
    let queries = "_Alignas(16) int raised; _Alignas(0) int unchanged; _Alignas(int) int natural; struct S { _Alignas(16) int member; } record; int main(void) { return !__builtin_has_attribute(raised, aligned) || __builtin_has_attribute(unchanged, aligned) || !__builtin_has_attribute(natural, aligned) || !__builtin_has_attribute(record.member, aligned); }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(queries, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
}

#[test]
fn non_object_alignment_queries_use_the_target_without_allocating_objects() {
    let source = "int f(void) { return 7; } _Static_assert(_Alignof(void) == 1, \"void\"); _Static_assert(__alignof__(int(void)) == 4, \"function type\"); _Static_assert(__alignof__(f) == 4, \"function designator\"); _Static_assert(sizeof(void) == 1 && sizeof(f) == 1, \"distinct sizeof rule\"); int main(void) { int side = 0; if (__alignof__(side++) != 4 || side) return 1; return _Alignof(void) != 1 || _Alignof(int(void)) != 4; }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "non-object-alignment");
    let output = run("void object; int main(void) { return 0; }", "--emit=evoir");
    assert!(
        !output.status.success(),
        "void is still not an allocatable object"
    );
}

#[test]
fn alignment_specifiers_reject_invalid_requests_and_placements() {
    let mut failures = Vec::new();
    for source in [
        "_Alignas(3) int object;",
        "_Alignas(1) int object;",
        "int function(_Alignas(16) int parameter);",
        "_Alignas(16) int function(void);",
        "_Alignas(16) typedef int integer;",
        "void function(void) { _Alignas(16) register int object; }",
        "struct S { _Alignas(16) unsigned bit : 1; };",
        "int alignment; _Alignas(alignment) int object;",
        "_Alignas(-1) int object;",
        "_Alignas(1) int object __attribute__((aligned(16)));",
        "int main(void) { return sizeof(_Alignas(16) int); }",
        "struct S { _Alignas(1) int field; };",
        "_Alignas(1) extern int values[];",
        "_Alignas(536870912) char object;",
        "#pragma pack(1)\nstruct S { char head; _Alignas(1) int member; };",
    ] {
        let output = run(source, "--parse-only");
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        if output.status.success() || !diagnostic.contains("E2938") {
            failures.push(format!("{source}: {diagnostic}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn alignment_specifiers_preserve_incomplete_declarations_and_pack_caps() {
    for source in [
        "_Alignas(16) extern struct S object; struct S { char member; };",
        "_Alignas(16) extern struct S object; struct S { char member __attribute__((aligned(32))); };",
        "#pragma pack(1)\nstruct S { char head; _Alignas(16) int member; }; _Static_assert(sizeof(struct S) == 5 && _Alignof(struct S) == 1, \"pack cap\");",
        "_Alignas(void) char object; _Static_assert(__alignof__(object) == 1, \"void alignment\");",
    ] {
        let output = run(source, "--parse-only");
        assert!(output.status.success(), "{source}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

#[test]
fn integer_absolute_builtins_preserve_types_values_and_single_evaluation() {
    let source = include_str!("../../../tests/fixtures/integer-absolute.c");
    for options in [vec![], vec!["-fno-builtin"]] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run_with_options(source, mode, &options);
            assert!(
                output.status.success(),
                "{mode} {options:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, b"0\n");
        }
        #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
        assert_native_execution_with_options(source, "integer-absolute", &options);
    }
}

#[test]
fn nested_flexible_member_initializers_remain_outside_the_profile() {
    for source in [
        "struct S { int n; char d[]; }; union U { struct S s; char space[36]; } u = {{1, \"abc\"}};",
        "struct S { int n; char d[]; }; union U { struct S s; char space[36]; } u = {{1, {'a', 'b', 'c', 0}}};",
        "struct S { int n; char d[]; }; struct O { int n; struct S s; } o = {1, {2, \"abc\"}};",
    ] {
        let output = run(source, "--parse-only");
        assert!(!output.status.success(), "accepted {source}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("E2202"));
    }
    let empty = "struct S { int n; char d[]; }; union U { struct S s; char space[36]; } u = {{3, {}}}; struct O { int n; struct S s; } o = {1, {2, {}}}; int main(void) { return u.s.n != 3 || o.s.n != 2 || sizeof(o) != 8; }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(empty, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
}

#[test]
fn ordinary_absolute_calls_respect_builtin_options_and_symbol_kind() {
    let source = "int abs(int x){return x+100;} long labs(long x){return x+200;} long long llabs(long long x){return x+300;} int main(void){volatile int x=-7; return abs(x)+labs(x)+llabs(x);}";
    for (options, expected) in [
        (vec![], b"21\n".as_slice()),
        (vec!["-fno-builtin"], b"579\n".as_slice()),
        (vec!["-fno-builtin", "-fbuiltin"], b"21\n".as_slice()),
        (vec!["-fbuiltin", "-fno-builtin"], b"579\n".as_slice()),
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run_with_options(source, mode, &options);
            assert!(
                output.status.success(),
                "{mode} {options:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected);
        }
    }
    for source in [
        "static int abs(int x){return x+100;} int main(void){return abs(7)-107;}",
        "int f(int x){return x+100;} int main(void){int (*abs)(int)=f; return abs(7)-107;}",
        "extern int abs(int) __asm__(\"other\"); int other(int x){return x+100;} int main(void){return abs(7)-107;}",
        "extern int renamed(int) __asm__(\"other\"); int other(int x){return x+100;} static int (*p)(int)=renamed; int main(void){return p(7)-107;}",
        "extern int renamed(int) __asm__(\"other\"); int renamed(int x){return x+100;} extern int other(int); int main(void){return other(7)-107;}",
        "unsigned long length(const char *) __asm__(\"strlen\"); int main(void){return length(\"seven\")-5;}",
        "long long a=-1;long long llabs(long long);void abort(void);int main(void){if(llabs(a)!=1)abort();return 0;}long long llabs(long long b){abort();}",
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(output.stdout, b"0\n");
        }
        #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
        assert_native_execution(source, "absolute-call-boundary");
    }
}

#[test]
fn absolute_builtins_preserve_constant_expression_and_overflow_refusals() {
    for (source, code) in [
        ("_Static_assert(__builtin_abs(-9)==9,\"\");", "E2912"),
        ("int f(void){return __builtin_abs();}", "E2909"),
        ("int f(void){return __builtin_labs(1,2);}", "E2909"),
        ("int f(void){return __builtin_abs((int *)0);}", "E2913"),
        (
            "long abs(long);int f(void){return __builtin_abs(-9);}",
            "E2927",
        ),
    ] {
        let output = run(source, "--parse-only");
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{code}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for expression in [
        "__builtin_abs(-2147483647-1)",
        "__builtin_labs(-9223372036854775807L-1)",
        "__builtin_llabs(-9223372036854775807LL-1)",
    ] {
        let source = format!("int main(void){{return {expression};}}");
        for options in [vec![], vec!["-fwrapv"]] {
            for mode in ["--interpret-c-hir", "--interpret-evoir"] {
                let output = run_with_options(&source, mode, &options);
                assert!(!output.status.success());
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains("E3004"),
                    "{mode}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

#[test]
fn signed_wrapping_is_explicit_and_shared_by_execution_paths() {
    let source = include_str!("../../../tests/fixtures/signed-wrapping.c");
    for options in [
        vec!["-fwrapv"],
        vec!["-fno-strict-overflow"],
        vec!["-fstrict-overflow", "-fwrapv"],
        vec!["-fno-wrapv", "-fno-strict-overflow"],
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run_with_options(source, mode, &options);
            assert!(
                output.status.success(),
                "{mode} {options:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, b"0\n", "{mode} {options:?}");
        }
    }
    let output = run_with_options(source, "--interpret-evoir", &["-fwrapv", "-Oeq-scalar"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"0\n");
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution_with_options(source, "signed-wrapping", &["-fwrapv"]);
}

#[test]
fn signed_wrapping_can_be_disabled_without_changing_the_default() {
    let source = include_str!("../../../tests/fixtures/signed-wrapping.c");
    for options in [
        vec![],
        vec!["-fno-wrapv"],
        vec!["-fwrapv", "-fno-wrapv"],
        vec!["-fno-strict-overflow", "-fstrict-overflow"],
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run_with_options(source, mode, &options);
            assert_eq!(output.status.code(), Some(1), "{mode} {options:?}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3004"),
                "{mode} {options:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn signed_wrapping_does_not_waive_other_runtime_obligations() {
    for (source, code) in [
        ("int main(void){int x=1;return x<<32;}", "E3005"),
        ("int main(void){int x=1;return x<<-1;}", "E3005"),
        ("int main(void){int x=1,y=0;return x/y;}", "E3002"),
        ("int main(void){int x=1,y=0;return x%y;}", "E3003"),
        (
            "int main(void){int x=(-2147483647-1),y=-1;return x/y;}",
            "E3004",
        ),
        ("int main(void){int a[2]={0};return *(a+3);}", "E3011"),
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run_with_options(source, mode, &["-fwrapv"]);
            assert_eq!(output.status.code(), Some(1), "{mode}: {source}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(code),
                "{mode}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn digit_builtin_executes_without_an_external_implementation() {
    let source = include_str!("../../../tests/fixtures/builtin-digit.c");
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
fn external_function_addresses_are_position_independent() {
    let source = include_str!("../../../tests/fixtures/external-function-pie.c");
    let output = run(source, "--emit=asm");
    assert!(output.status.success());
    let assembly = String::from_utf8(output.stdout).unwrap();
    assert!(assembly.contains(":got:puts"), "{assembly}");
    assert!(assembly.contains("#:got_lo12:puts"), "{assembly}");
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "external-function-address");
}

#[test]
fn external_object_addresses_are_position_independent() {
    let source = include_str!("../../../tests/fixtures/external-object-pie.c");
    let output = run(source, "--emit=asm");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = String::from_utf8(output.stdout).unwrap();
    assert!(assembly.contains(":got:environ"));
    assert!(assembly.contains("#:got_lo12:environ"));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "external-object-address");
}

#[test]
fn unnamed_definition_parameters_preserve_call_positions_and_argument_effects() {
    let source = include_str!("../../../tests/fixtures/unnamed-definition-parameters.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = run(source, mode);
        if !output.status.success() {
            failures.push(format!(
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        } else if mode != "--emit=asm" && output.stdout != b"0\n" {
            failures.push(format!("{mode}: unexpected result {:?}", output.stdout));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn jumps_preserve_automatic_objects_and_skip_initializer_effects() {
    let source = include_str!("../../../tests/fixtures/goto-object-storage.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "goto-object-storage");
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn unnamed_definition_parameters_execute_with_the_positional_abi() {
    assert_native_execution(
        include_str!("../../../tests/fixtures/unnamed-definition-parameters.c"),
        "unnamed-parameters",
    );
}

#[test]
fn function_pointer_casts_preserve_identity_and_argument_exits() {
    let source = include_str!("../../../tests/fixtures/function-pointer-cast-roundtrip.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let output = run(source, "--emit=evoir");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("ptr.to_int"), "{text}");
    assert!(text.contains("ptr.from_int.fnptr"), "{text}");
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "function-pointer-cast-roundtrip");
}

#[test]
fn function_pointer_casts_check_signatures_only_at_a_reached_call() {
    let source = "static void f(void) {} int main(void) { return ((int (*)(int))f)(7); }";
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if output.status.success() || !String::from_utf8_lossy(&output.stderr).contains("E3019") {
            failures.push(format!(
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn function_and_void_pointer_equality_preserves_addresses_and_effects() {
    let source = include_str!("../../../tests/fixtures/function-void-pointer-equality.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "mixed-pointer-equality");
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn assert_native_execution(source: &str, case: &str) {
    assert_native_execution_with_options(source, case, &[]);
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn assert_native_execution_with_options(source: &str, case: &str, options: &[&str]) {
    let output = run_with_options(source, "--emit=evoir", options);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    let assembly =
        evocc_mir::compile_module_for(evocc_mir::Target::Darwin, &module, &|_| true).unwrap();
    let directory = std::env::temp_dir().join(format!("evocc-{case}-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let asm = directory.join("probe.s");
    let binary = directory.join("probe");
    fs::write(&asm, assembly).unwrap();
    let linked = Command::new("clang")
        .arg(&asm)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let executed = Command::new(&binary).output().unwrap();
    assert_eq!(
        executed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    fs::remove_file(binary).unwrap();
    fs::remove_file(asm).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn unprototyped_function_pointers_recover_the_promoted_call_interface() {
    let source = include_str!("../../../tests/fixtures/unprototyped-function-pointers.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "unprototyped-function-pointers");
}

#[test]
fn unprototyped_calls_still_reject_a_mismatched_dynamic_interface() {
    for arguments in ["7L", ""] {
        let source = format!(
            "int f(int x){{return x;}} int main(void){{int (*p)()=f; return p({arguments});}}"
        );
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(&source, mode);
            assert!(!output.status.success(), "{mode} accepted {source}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3019"),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn nested_switch_entry_executes_without_bypassed_conditions() {
    let source = include_str!("../../../tests/fixtures/nested-switch-labels.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "nested-switch-entry");
}

#[test]
fn discarded_expression_exits_execute_without_invented_fallthrough() {
    let source = include_str!("../../../tests/fixtures/discarded-expression-exits.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "discarded-expression-exits");
}

#[test]
fn constant_case_and_executed_signed_shift_agree_on_the_bit_pattern() {
    let source = "int main(void) { int x = -2147483647-1; switch (x) {
        case 1 << 31: return 0; default: return 1; } }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
        let runtime = source.replace("case 1 << 31: return 0", "case 1 << 31: return 1 << 31");
        let output = run(&runtime, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"-2147483648\n");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "case-constant-folding");
}

#[test]
fn signed_shifts_use_the_measured_bit_pattern_at_every_integer_width() {
    let source = include_str!("../../../tests/fixtures/signed-shift-bit-patterns.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains("signed_shl_defined"), "{text}");
    assert!(
        !text.contains("ub.assume.invalid_signed_left_shift"),
        "{text}"
    );
    assert!(text.contains("ub.assume.invalid_shift_count"), "{text}");
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "signed-shift-bit-patterns");
}

#[test]
fn overaligned_stack_objects_keep_their_addresses_and_live_values_across_calls() {
    let source = include_str!("../../../tests/fixtures/overaligned-stack.c");
    // Test actual addresses, recursion and live bytes, not just __alignof__.
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "overaligned-stack");
}

#[test]
fn aligned_function_addresses_preserve_identity_aliases_and_round_trips() {
    let source = include_str!("../../../tests/fixtures/function-alignment.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "function-alignment");
}

#[test]
fn function_alignment_is_emitted_for_both_targets() {
    let source = include_str!("../../../tests/fixtures/function-alignment.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    let header = text
        .lines()
        .find(|line| line.starts_with("func @first("))
        .unwrap();
    assert!(
        header.ends_with("visibility=internal align=256"),
        "{header}"
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    for target in [evocc_mir::Target::Linux, evocc_mir::Target::Darwin] {
        let assembly = evocc_mir::compile_module_for(target, &module, &|_| true).unwrap();
        let label = if target == evocc_mir::Target::Linux {
            ".type first, %function\nfirst:"
        } else {
            "_first:"
        };
        assert!(
            assembly.contains(&format!(".p2align 8\n{label}")),
            "{target:?}: {assembly}"
        );
    }
}

#[test]
fn both_targets_realign_overaligned_frames_using_the_reserved_scratch_register() {
    let source = include_str!("../../../tests/fixtures/overaligned-stack.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    for target in [evocc_mir::Target::Linux, evocc_mir::Target::Darwin] {
        let lowered = evocc_mir::lower_module_for(target, &module, &|_| true).unwrap();
        let function = lowered
            .functions
            .iter()
            .find(|f| f.name == "check")
            .unwrap();
        let allocated = evocc_mir::allocate(&evocc_mir::out_of_ssa(function)).unwrap();
        evocc_mir::verify_allocation(&allocated).unwrap();
        assert!(
            allocated.frame_slots > 0,
            "the fixture must exercise spills"
        );
        let assembly = evocc_mir::compile_module_for(target, &module, &|_| true).unwrap();
        assert!(
            assembly.contains("mov x16, sp\n    and x16, x16, #-256\n    mov sp, x16"),
            "{target:?}: {assembly}"
        );
        assert!(
            assembly.contains("mov sp, x29\n    ldp x29, x30, [sp], #16"),
            "{target:?}: {assembly}"
        );
        assert!(!assembly.contains("and sp, sp"), "{target:?}: {assembly}");
    }
}

#[test]
fn substituted_member_macro_keeps_its_single_object_projection() {
    let source = "struct S { int slot; };\n\
        #define slot state->slot\n\
        #define SET(s, value) ((s) = (value))\n\
        int main(void) { struct S object = {0}; struct S *state = &object;
          SET(slot, 7); return slot != 7; }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "substituted-member-macro");
}

#[test]
fn enum_pointer_composites_preserve_the_signed_integer_representation() {
    let source = include_str!("../../../tests/fixtures/enum-compatible-pointers.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "enum-compatible-pointers");
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn undefined_weak_object_and_function_addresses_are_zero_natively() {
    let source = "extern int value __attribute__((weak)); extern int optional(void) __attribute__((weak)); int main(void){return optional != 0 || &value != 0;}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    let assembly =
        evocc_mir::compile_module_for(evocc_mir::Target::Darwin, &module, &|_| true).unwrap();
    let directory =
        std::env::temp_dir().join(format!("evocc-weak-addresses-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let asm = directory.join("probe.s");
    let binary = directory.join("probe");
    let reference_source = directory.join("reference.c");
    let reference = directory.join("reference");
    fs::write(&reference_source, source).unwrap();
    // Darwin's linker requires an explicit list of unresolved symbols even
    // for Clang's weak references. Exercise identical policy for both objects;
    // this does not turn a strong reference into a weak one.
    let unresolved = ["-Wl,-U,_optional,-U,_value"];
    let reference_link = Command::new("clang")
        .arg(&reference_source)
        .args(unresolved)
        .arg("-o")
        .arg(&reference)
        .output()
        .unwrap();
    assert!(
        reference_link.status.success(),
        "{}",
        String::from_utf8_lossy(&reference_link.stderr)
    );
    assert_eq!(Command::new(&reference).status().unwrap().code(), Some(0));
    fs::write(&asm, assembly).unwrap();
    let linked = Command::new("clang")
        .arg(&asm)
        .args(unresolved)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let executed = Command::new(&binary).output().unwrap();
    assert_eq!(
        executed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    fs::remove_file(binary).unwrap();
    fs::remove_file(asm).unwrap();
    fs::remove_file(reference).unwrap();
    fs::remove_file(reference_source).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn referenced_external_symbols_lower_without_invented_bodies() {
    for (source, expected) in [
        (
            "extern int g(); int f(void){g((char)1);return g(2);}",
            "declare @g(bits<32>,",
        ),
        (
            "extern int g(int); int f(int x){return g(x);}",
            "declare @g(",
        ),
        (
            "extern int x; int f(void){return x;}",
            "extern @x size=4 align=4",
        ),
        (
            "void f(void*p){__builtin_memset(p,0,1);}",
            "declare @memset(",
        ),
        (
            "extern int g(int); int (*f(void))(int){return g;}",
            "declare @g(",
        ),
    ] {
        let output = run(source, "--emit=evoir");
        assert!(
            output.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(expected), "{text}");
        assert!(!text.contains("func @g("));
        evocc_evoir::parse_section9_module(text.as_bytes()).expect("valid external references");
    }
}

#[test]
fn incomplete_external_objects_have_addresses_without_invented_layouts() {
    for (source, alignment) in [
        ("struct FILE; extern struct FILE input; void *f(void){return &input;}", 1),
        ("union U; extern union U input; void *f(void){return &input;}", 1),
        ("struct FILE; extern struct FILE input __attribute__((aligned(32))); void *f(void){return &input;}", 32),
    ] {
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{source}: {}", String::from_utf8_lossy(&output.stderr));
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(&format!("extern @input size=unknown align={alignment}")), "{text}");
        assert!(text.contains("global_addr @input"), "{text}");
        let module = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
        assert!(module.globals.is_empty());
        assert_eq!(module.extern_objects.len(), 1);
        assert_eq!(module.extern_objects[0].size, None);
    }
    let output = run(
        "struct FILE; extern struct FILE input; int f(void){return sizeof input;}",
        "--emit=evoir",
    );
    assert!(
        !output.status.success(),
        "an address does not make the type complete"
    );
}

#[test]
fn incomplete_external_addresses_execute_without_claiming_object_storage() {
    for declaration in [
        "extern const void input;",
        "extern const void input __attribute__((aligned(32)));",
        "struct O; extern struct O input;",
        "union O; extern union O input;",
    ] {
        let source = format!("{declaration} const void *f(void){{return &input;}} int main(void){{return f()!=&input;}}");
        let output = run(&source, "--emit=evoir");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
        assert!(module.globals.is_empty());
        assert_eq!(module.extern_objects.len(), 1);
        let external = &module.extern_objects[0];
        assert_eq!(external.size, None);
        assert_eq!(
            external.align,
            if declaration.contains("aligned") {
                32
            } else {
                1
            }
        );
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(&source, mode);
            assert!(
                output.status.success(),
                "{declaration}: {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, b"0\n");
        }
        let output = run(
            &format!("{declaration} int main(void){{return *(const char*)&input;}}"),
            "--interpret-c-hir",
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3040"));
    }
}

#[test]
fn unresolved_runtime_symbols_fail_only_when_reached() {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        for (source, code) in [
            ("extern int g(void); int main(void){return g();}", "E3039"),
            ("extern int x; int main(void){return x;}", "E3040"),
            (
                "extern int isdigit(int); int main(void){return isdigit(48);}",
                "E3039",
            ),
            (
                "extern int g(void); int main(void){int (*p)(void)=g;return p();}",
                "E3039",
            ),
        ] {
            let output = run(source, mode);
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(code),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = run(
            "extern int g(void); extern int x; int main(void){if(0)return g()+x;return 0;}",
            mode,
        );
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = run(
        "_Static_assert(sizeof(int[]) == 4, \"incomplete array\");",
        "--parse-only",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("E2912"));
}

#[test]
fn external_memory_library_executes_real_bytes() {
    let source = "int main(void){char a[8]; char b[8]; __builtin_memset(a,0,8); __builtin_memcpy(b,a,8); __builtin_memmove(b+1,b,4); return __builtin_memcmp(a,b,8) + __builtin_strcmp(a,b) + __builtin_strncmp(a,b,8) + __builtin_strlen(a);}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert_eq!(
            output.stdout,
            b"0\n",
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        for (source, expected) in [
            ("int main(void){char a[8]=\"abc\"; __builtin_memmove(a+1,a,4); return __builtin_strlen(a) + 10*__builtin_strcmp(a,\"aabc\");}", "4\n"),
            ("int main(void){char a[4],b[4];a[0]=7;__builtin_memcpy(b,a,4);return b[0];}", "7\n"),
            ("int main(void){char a[4];return (__builtin_memset(a,42,4)==a)+a[3];}", "43\n"),
        ] {
            let output = run(source, mode);
            assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(output.stdout, expected.as_bytes());
        }
        let output = run(
            "int main(void){char a[4],b[4];a[0]=7;__builtin_memcpy(b,a,4);return b[1];}",
            mode,
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3001"));
    }
}

#[test]
fn unprototyped_external_calls_require_one_promoted_signature() {
    let output = run(
        "extern int g(); int f(void){g(1);return g(1L);}",
        "--emit=evoir",
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("E2931"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn external_variadic_calls_promote_anonymous_arguments() {
    let source =
        "extern int log_value(int, ...); int main(void){return log_value(1, (char)2, (short)3);}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("declare @log_value(bits<32>, ..., mem<all>, world)"),
        "{text}"
    );
    evocc_evoir::parse_section9_module(text.as_bytes()).expect("promoted variadic call");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3039"),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn external_provenance_has_the_same_volatile_observation_in_both_interpreters() {
    use evocc_chir::CHirDecl;
    use evocc_common::{Arena, FileId};
    use evocc_lex::Lexer;
    use evocc_parser::Parser;
    use evocc_types::TargetDataLayout;

    for source in [
        "extern int unused; extern int z; extern int x; int *volatile p; int main(void){p=&z;p=&x;return p==&x;}",
        "extern int unused(void); extern int z(void); extern int x(void); int (*volatile p)(void); int main(void){p=z;p=x;return p==x;}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45_001);
        let mut lexer = Lexer::new(file, source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            assert_ne!(token.kind, evocc_lex::TokenKind::Error);
            tokens.push(token);
        }
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let symbol = unit.iter().find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == "main" => Some(function.symbol),
            _ => None,
        }).unwrap();
        let chir = evocc_chir::interp::interpret_scalar_function(&unit, symbol, evocc_chir::interp::InterpreterLimits::new(100_000)).unwrap();
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let function = module.functions.iter().find(|function| function.name == "main").unwrap();
        let arguments = function.params.iter().map(|parameter| evocc_evoir::RuntimeValue::new(parameter.ty.clone(), 0)).collect::<Vec<_>>();
        let evoir = evocc_evoir::interpret_module(&module, "main", &arguments, evocc_evoir::InterpreterLimits::new(100_000)).unwrap();
        assert_eq!(chir.world.volatile_trace(), evoir.world.volatile_trace(), "{source}");
    }
}

#[test]
fn library_arguments_are_converted_to_the_declared_parameter_type() {
    let source = "int main(void){char a[300];__builtin_memset(a,0,300);return __builtin_memcmp(a,a,(signed char)-1);}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            !output.status.success(),
            "{mode}: a negative signed count must convert to size_t, not 255"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3011"),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
