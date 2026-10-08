//! Repairs must preserve source bytes outside their spans and respect review.
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const CASES: [(&str, &str, &str, &str); 5] = [
    (
        "W0001.remove-declaration",
        "safe",
        "int f(void) { int unused = 2; return 0; }\n",
        "int f(void) {  return 0; }\n",
    ),
    (
        "W0002.void-cast",
        "safe",
        "int f(int unused) { return 0; }\n",
        "int f(int unused) { (void)unused; return 0; }\n",
    ),
    (
        "W0003.insert-prototype",
        "review",
        "int f(void) { return missing(1, 2.0); }\n",
        "int missing(int, double);\nint f(void) { return missing(1, 2.0); }\n",
    ),
    (
        "W0004.insert-int",
        "safe",
        "f(void) { return 0; }\n",
        "int f(void) { return 0; }\n",
    ),
    (
        "W0005.return-zero",
        "review",
        "int f(void) { }\n",
        "int f(void) { return 0; }\n",
    ),
];

struct Source(PathBuf);
impl Source {
    fn new(text: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "evocc-repairs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("input.c");
        std::fs::write(&file, text).unwrap();
        Self(file)
    }
    fn fix(&self, options: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_evo"))
            .arg("fix")
            .arg(&self.0)
            .args(options)
            .env("EVOCC", env!("CARGO_BIN_EXE_evocc"))
            .output()
            .unwrap()
    }
    fn check(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([
                "-fsyntax-only",
                "-Wall",
                "-Wextra",
                "-Werror",
                "--color=never",
            ])
            .arg(&self.0)
            .output()
            .unwrap()
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        std::fs::remove_dir_all(self.0.parent().unwrap()).unwrap();
    }
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn json(bytes: &[u8], assertions: &str, args: &[&str]) {
    let mut child = Command::new("python3")
        .args([
            "-c",
            &format!("import json,sys\nr=json.load(sys.stdin)\n{assertions}"),
        ])
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    assert!(
        child.wait().unwrap().success(),
        "{}",
        String::from_utf8_lossy(bytes)
    );
}

#[test]
fn all_five_diagnostics_carry_typed_repairs() {
    for (id, kind, before, _) in CASES {
        let source = Source::new(before);
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([
                "-fsyntax-only",
                "-Wall",
                "--cap-lints=warn",
                "--error-format=json",
            ])
            .arg(&source.0)
            .output()
            .unwrap();
        success(&output);
        json(
            &output.stderr,
            r#"
assert r['code'] == sys.argv[1][:5]
assert len(r['repairs']) == 1
fix = r['repairs'][0]
assert set(fix) == {'repair_id', 'kind', 'file', 'span', 'replacement', 'title'}
assert fix['repair_id'] == sys.argv[1] and fix['kind'] == sys.argv[2]
assert fix['file'] == sys.argv[3] == fix['span']['file']
assert set(fix['span']) == set(r['spans'][0])
assert isinstance(fix['replacement'], str) and fix['title']
if fix['kind'] == 'safe':
    assert 'help:' in r['rendered'] and fix['replacement'] in r['rendered']
"#,
            &[id, kind, source.0.to_str().unwrap()],
        );
    }
}

fn apply(index: usize) {
    let (id, kind, before, after) = CASES[index];
    let source = Source::new(before);
    let plan = source.fix(&["--plan", "--json"]);
    success(&plan);
    json(&plan.stdout, "assert r['exit_code'] == 0\nassert [p['repair_id'] for p in r['repairs']] == [sys.argv[1]]", &[id]);
    assert_eq!(std::fs::read(&source.0).unwrap(), before.as_bytes());
    let options: &[&str] = if kind == "review" {
        &["--apply", "--review", "--json"]
    } else {
        &["--apply", "--json"]
    };
    let output = source.fix(options);
    success(&output);
    json(
        &output.stdout,
        "assert r['exit_code'] == 0\nassert r['applied'] == 1",
        &[],
    );
    assert_eq!(std::fs::read(&source.0).unwrap(), after.as_bytes());
    success(&source.check());
}

#[test]
fn remove_constant_unused_declaration() {
    apply(0);
}

#[test]
fn fix_accepts_file_before_between_or_after_options() {
    for mode in ["--plan", "--apply"] {
        for file_index in 0..=2 {
            let before = CASES[0].2;
            let source = Source::new(before);
            let mut args = vec![std::ffi::OsString::from(mode), "--json".into()];
            args.insert(file_index, source.0.clone().into_os_string());
            let output = Command::new(env!("CARGO_BIN_EXE_evo"))
                .arg("fix")
                .args(args)
                .env("EVOCC", env!("CARGO_BIN_EXE_evocc"))
                .output()
                .unwrap();
            success(&output);
            json(
                &output.stdout,
                "assert r['exit_code'] == 0\nassert r['applied'] == int(sys.argv[1])\nassert [p['repair_id'] for p in r['repairs']] == ['W0001.remove-declaration']",
                &[if mode == "--plan" { "0" } else { "1" }],
            );
            let expected = if mode == "--plan" { before } else { CASES[0].3 };
            assert_eq!(std::fs::read(&source.0).unwrap(), expected.as_bytes());
        }
    }
}

#[test]
fn unused_declaration_removal_consumes_its_otherwise_empty_line() {
    for newline in ["\n", "\r\n"] {
        for declaration in [
            "    int unused = 2;",
            "\tint unused = 2; \t",
            "    int unused =\n        2;",
            "    int unused = 2, also_unused = 3;",
        ] {
            let before = format!("int f(void) {{\n{declaration}\n\n    return 0;\n}}\n")
                .replace('\n', newline);
            let expected = "int f(void) {\n\n    return 0;\n}\n".replace('\n', newline);
            let source = Source::new(&before);
            success(&source.fix(&["--apply", "--json"]));
            assert_eq!(std::fs::read(&source.0).unwrap(), expected.as_bytes());
            success(&source.check());
        }
    }
}

#[test]
fn unused_declaration_removal_preserves_neighboring_code_and_comments() {
    for (before, expected) in [
        (CASES[0].2, CASES[0].3),
        (
            "int f(void) {\n    int unused = 2; // keep this comment\n    return 0;\n}\n",
            "int f(void) {\n     // keep this comment\n    return 0;\n}\n",
        ),
        (
            "int f(void) {\n    /* keep this comment */ int unused = 2;\n    return 0;\n}\n",
            "int f(void) {\n    /* keep this comment */ \n    return 0;\n}\n",
        ),
        (
            "int f(void) {\n    int unused = 2; return 0;\n}\n",
            "int f(void) {\n     return 0;\n}\n",
        ),
    ] {
        let source = Source::new(before);
        success(&source.fix(&["--apply", "--json"]));
        assert_eq!(std::fs::read(&source.0).unwrap(), expected.as_bytes());
        success(&source.check());
    }
}

#[test]
fn unused_declaration_line_removal_preserves_a_following_macro() {
    let source = Source::new("#define RET return 0;\nint f(void) {\n    int unused = 1;\nRET\n}\n");
    let output = source.fix(&["--apply", "--json"]);
    success(&output);
    json(&output.stdout, "assert r['applied'] == 1", &[]);
    assert_eq!(
        std::fs::read_to_string(&source.0).unwrap(),
        "#define RET return 0;\nint f(void) {\nRET\n}\n"
    );
    success(&source.check());
}

#[test]
fn repair_insertion_at_a_macro_boundary_still_requires_caution() {
    let before = "#define RET return 0;\nint f(int unused) {RET}\n";
    let source = Source::new(before);
    let output = source.fix(&["--apply", "--json"]);
    success(&output);
    json(&output.stdout, "assert r['applied'] == 0", &[]);
    assert_eq!(std::fs::read_to_string(&source.0).unwrap(), before);
}

#[test]
fn use_unused_parameter() {
    apply(1);
}
#[test]
fn declare_implicit_function_with_promoted_argument_types() {
    apply(2);
}
#[test]
fn make_implicit_int_explicit() {
    apply(3);
}
#[test]
fn insert_reviewed_return() {
    apply(4);
}

#[test]
fn review_only_plan_leaves_source_untouched() {
    for index in [2, 4] {
        let source = Source::new(CASES[index].2);
        let output = source.fix(&["--apply", "--json"]);
        success(&output);
        json(&output.stdout, "assert r['applied'] == 0", &[]);
        assert_eq!(std::fs::read_to_string(&source.0).unwrap(), CASES[index].2);
    }
}

#[test]
fn syntax_error_prevents_every_edit() {
    let before = "f(void) { int unused = 1; return @; }\n";
    let source = Source::new(before);
    let output = source.fix(&["--apply", "--review", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(&source.0).unwrap(), before);
    json(
        &output.stdout,
        "assert r['exit_code'] == 1 and r['applied'] == 0",
        &[],
    );
}

#[test]
fn side_effecting_initializer_and_volatile_access_are_not_safe_edits() {
    for before in [
        "int g(void); int f(void) { int unused = g(); return 0; }\n",
        "int f(void) { volatile int unused = 1; return 0; }\n",
        "int f(volatile int unused) { return 0; }\n",
        "int f(void) { volatile char unused[] = \"x\"; return 0; }\n",
        "int f(void) { int unused = 1e99; return 0; }\n",
        "struct S { volatile int x; }; int f(void) { struct S unused = {}; return 0; }\n",
    ] {
        let source = Source::new(before);
        let output = source.fix(&["--apply", "--json"]);
        success(&output);
        json(&output.stdout, "assert r['applied'] == 0", &[]);
        assert_eq!(std::fs::read_to_string(&source.0).unwrap(), before);
    }
}

#[test]
fn edits_use_descending_byte_offsets() {
    let source = Source::new("f(int unused) { int dead = 1; return 0; }\n");
    success(&source.fix(&["--apply"]));
    assert_eq!(
        std::fs::read_to_string(&source.0).unwrap(),
        "int f(int unused) { (void)unused;  return 0; }\n"
    );
    success(&source.check());
}

#[test]
fn declaration_removal_preserves_other_declarators() {
    let source = Source::new("int f(void) { int unused = 2, used = 3; return used; }\n");
    success(&source.fix(&["--apply"]));
    assert_eq!(
        std::fs::read_to_string(&source.0).unwrap(),
        "int f(void) { int used = 3; return used; }\n"
    );
    success(&source.check());
}

#[test]
fn macro_expansion_is_not_a_writable_declaration() {
    let before = "#define DECL int unused = 2;\nint f(void) { DECL return 0; }\n";
    let source = Source::new(before);
    success(&source.fix(&["--apply", "--json"]));
    assert_eq!(std::fs::read_to_string(&source.0).unwrap(), before);
}

#[test]
fn all_unused_declarators_can_be_removed_together() {
    let source = Source::new("int f(void) { int a = 1, b = 2; return 0; }\n");
    success(&source.fix(&["--apply"]));
    success(&source.check());
}

#[test]
fn adjacent_unused_declarators_keep_shared_syntax() {
    for declaration in [
        "int used = 3, a = 1, b = 2;",
        "int a = 1, b = 2, used = 3;",
        "int used = 3, a = 1, b = 2, other = 4;",
    ] {
        let result = if declaration.contains("other") {
            "used + other"
        } else {
            "used"
        };
        let source = Source::new(&format!(
            "int f(void) {{ {declaration} return {result}; }}\n"
        ));
        success(&source.fix(&["--apply", "--json"]));
        success(&source.check());
    }
}

#[test]
fn removing_a_loop_declaration_keeps_the_separator() {
    let source = Source::new("int f(void) { for (int unused = 2; 1;) break; return 0; }\n");
    success(&source.fix(&["--apply"]));
    assert_eq!(
        std::fs::read_to_string(&source.0).unwrap(),
        "int f(void) { for (; 1;) break; return 0; }\n"
    );
    success(&source.check());
}

#[test]
fn prototype_precedes_an_implicit_return_type_at_the_same_position() {
    let source = Source::new("f(void) { return missing(); }\n");
    success(&source.fix(&["--apply", "--review", "--json"]));
    assert_eq!(
        std::fs::read_to_string(&source.0).unwrap(),
        "int missing(void);\nint f(void) { return missing(); }\n"
    );
    success(&source.check());
}

#[test]
fn prototypes_use_promoted_scalars_and_pointer_declarators() {
    for source in [
        "int f(char a, float b) { return missing(a, b); }\n",
        "int f(const int *a) { return missing(a); }\n",
        "int f(double (*a)(float)) { return missing(a); }\n",
        "int f(int (*a)[3]) { return missing(a); }\n",
    ] {
        let source = Source::new(source);
        success(&source.fix(&["--apply", "--review", "--json"]));
        success(&source.check());
    }
}
