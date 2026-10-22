//! Aggregate GNU statement expressions must copy their value out before the
//! objects declared inside the expression end. The same value kinds run in
//! both interpreters and in native code on both targets.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [DARWIN, LINUX];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];

const CASES: [(&str, &str); 10] = [
    (
        "record",
        "struct S { int a, b; };\n\
         int main(void) { struct S s = ({ struct S t = { 1, 2 }; t; }); return s.a + s.b - 3; }\n",
    ),
    (
        "union",
        "union U { int i; float f; };\n\
         int main(void) { union U u = ({ union U t; t.i = 7; t; }); return u.i - 7; }\n",
    ),
    (
        "record with array",
        "struct A { int v[2]; };\n\
         int main(void) { struct A a = ({ struct A t = {{ 4, 5 }}; t; }); return a.v[0] + a.v[1] - 9; }\n",
    ),
    (
        "integer complex",
        "int main(void) { _Complex int z = ({ _Complex int t = 6; t; }); return __real__ z - 6; }\n",
    ),
    (
        "floating complex",
        "int main(void) { float _Complex z = ({ float _Complex t = 6.0f; t; }); return __real__ z != 6.0f; }\n",
    ),
    (
        "nested",
        "struct S { int a, b; };\n\
         int main(void) { struct S s = ({ struct S t = ({ struct S q = { 1, 2 }; q; }); t; }); return s.a + s.b - 3; }\n",
    ),
    (
        "argument",
        "struct S { int a, b; };\n\
         struct S id(struct S x) { return x; }\n\
         int main(void) { struct S s = id(({ struct S t = { 1, 2 }; t; })); return s.a + s.b - 3; }\n",
    ),
    (
        "return value",
        "struct S { int a, b; };\n\
         struct S make(void) { return ({ struct S t = { 1, 2 }; t; }); }\n\
         int main(void) { struct S s = make(); return s.a + s.b - 3; }\n",
    ),
    (
        "address taken inside",
        "struct S { int a, b; };\n\
         int main(void) { struct S s = ({ struct S t = { 1, 2 }; struct S *p = &t; p->a = 3; t; }); return s.a + s.b - 5; }\n",
    ),
    (
        "two in a row",
        "struct S { int a, b; };\n\
         int main(void) { struct S a = ({ struct S t = { 1, 2 }; t; }); struct S b = ({ struct S t = { 3, 4 }; t; }); return a.a + a.b + b.a + b.b - 10; }\n",
    ),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn run(source: &str, target: &str, mode: &str) -> Output {
    let mut command = Command::new(evocc());
    command.args(["--target", target]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command
        .args([mode, "-w", "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn interpreters() {
    for (name, source) in CASES {
        for target in TARGETS {
            for mode in INTERPRETERS {
                let output = run(source, target, mode);
                assert!(
                    output.status.success(),
                    "{name} {target} {mode}\n{}",
                    text(&output)
                );
                assert_eq!(
                    output.stdout,
                    b"0\n",
                    "{name} {target} {mode}\n{}",
                    text(&output)
                );
            }
        }
    }
}

#[test]
fn aggregate_statement_expressions_run_in_both_interpreters() {
    interpreters();
}

fn function_ir<'a>(ir: &'a str, name: &str) -> &'a str {
    let marker = format!("func @{name}(");
    let start = ir
        .find(&marker)
        .unwrap_or_else(|| panic!("{name}: no function\n{ir}"));
    let rest = &ir[start..];
    let end = rest.find("\n}\n").map_or(rest.len(), |end| end + 3);
    &rest[..end]
}

fn volatile_roots(function: &str) -> Vec<String> {
    let mut offsets = std::collections::BTreeMap::new();
    for line in function.lines() {
        if let Some((left, right)) = line.split_once(" = ptr.offset.add.unsigned ") {
            let result = left.trim().trim_start_matches('(').to_owned();
            let base = right.split(',').next().unwrap().trim().to_owned();
            offsets.insert(result, base);
        }
    }
    let root = |value: &str| {
        let mut current = value.to_owned();
        while let Some(base) = offsets.get(&current) {
            current = base.clone();
        }
        current
    };
    function
        .lines()
        .filter(|line| line.contains("load volatile"))
        .map(|line| {
            let pointer = line
                .rsplit_once(" : ")
                .unwrap()
                .0
                .rsplit(' ')
                .next()
                .unwrap();
            root(pointer)
        })
        .collect()
}

fn temporary_value(function: &str) -> String {
    function
        .lines()
        .find_map(|line| {
            let (left, right) = line.split_once(" = alloca ")?;
            let origin = right
                .split("origin=")
                .nth(1)?
                .split(',')
                .next()?
                .parse::<u64>()
                .ok()?;
            (origin & (1 << 63) != 0).then(|| left.trim().trim_start_matches('(').to_owned())
        })
        .unwrap_or_else(|| panic!("no synthetic alloca\n{function}"))
}

#[test]
fn volatile_accesses_stay_on_the_source_object() {
    let source = "struct S { int a, b; };\n\
                  volatile struct S g;\n\
                  int f1(void) { struct S s = ({ volatile struct S t = { 1, 2 }; t; }); return s.a; }\n\
                  int f2(void) { struct S s = ({ g; }); return s.a; }\n\
                  int f3(volatile struct S *p) { struct S s = ({ *p; }); return s.b; }\n\
                  int f4(void) { struct S s = ({ struct S t = { 3, 4 }; t; }); return s.b; }\n\
                  int f5(volatile struct S *p) { struct S s = ({ struct S u = *p; u; }); return s.a; }\n";
    for target in TARGETS {
        let output = run(source, target, "--emit=evoir");
        assert!(output.status.success(), "{target}\n{}", text(&output));
        let ir = String::from_utf8(output.stdout).unwrap();
        for (name, expected_count, expected_roots) in [
            ("f1", 2, vec!["%v5".to_owned(), "%v5".to_owned()]),
            ("f2", 2, vec!["%v5".to_owned(), "%v5".to_owned()]),
            ("f3", 2, vec!["%v0".to_owned(), "%v0".to_owned()]),
            ("f4", 0, vec![]),
            ("f5", 2, vec!["%v0".to_owned(), "%v0".to_owned()]),
        ] {
            let function = function_ir(&ir, name);
            let roots = volatile_roots(function);
            assert_eq!(
                roots.len(),
                expected_count,
                "{target} {name}: volatile roots {roots:?}\n{function}"
            );
            assert_eq!(
                roots, expected_roots,
                "{target} {name}: volatile roots\n{function}"
            );
            let temporary = temporary_value(function);
            assert!(
                !roots.contains(&temporary),
                "{target} {name}: volatile access to temporary {temporary}\n{function}"
            );
        }
    }
}

#[test]
fn the_value_is_copied_before_the_inner_lifetime_ends() {
    let source = CASES[0].1;
    for target in TARGETS {
        let output = run(source, target, "--emit=evoir");
        assert!(output.status.success(), "{target}\n{}", text(&output));
        let ir = String::from_utf8(output.stdout).unwrap();
        let copy = ir
            .lines()
            .position(|line| line.contains("= copy ") && line.contains("[size=8, align=4]"))
            .unwrap_or_else(|| panic!("{target}: no aggregate copy\n{ir}"));
        let end = ir
            .lines()
            .position(|line| line.contains("= lifetime.end "))
            .unwrap_or_else(|| panic!("{target}: no lifetime.end\n{ir}"));
        assert!(
            copy < end,
            "{target}: copy at line {copy} does not precede lifetime.end at line {end}\n{ir}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn aggregate_statement_expressions_run_natively_on_darwin() {
    let directory = std::env::temp_dir().join(format!(
        "evocc-aggregate-statement-darwin-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    for (name, source) in CASES {
        for level in LEVELS {
            let stem = name.replace(' ', "-");
            let source_file = directory.join(format!("{stem}{level}.c"));
            let assembly = directory.join(format!("{stem}{level}.s"));
            let executable = directory.join(format!("{stem}{level}"));
            std::fs::write(&source_file, source).unwrap();
            let compiled = Command::new(evocc())
                .args([
                    "--target",
                    DARWIN,
                    "-w",
                    level,
                    "-S",
                    "-o",
                    assembly.to_str().unwrap(),
                    source_file.to_str().unwrap(),
                ])
                .output()
                .unwrap();
            assert!(
                compiled.status.success(),
                "{name} {level}: {}",
                String::from_utf8_lossy(&compiled.stderr)
            );
            let linked = Command::new("clang")
                .args([
                    assembly.to_str().unwrap(),
                    "-o",
                    executable.to_str().unwrap(),
                ])
                .output()
                .unwrap();
            assert!(
                linked.status.success(),
                "{name} {level}: {}",
                String::from_utf8_lossy(&linked.stderr)
            );
            let ran = Command::new(&executable).output().unwrap();
            assert!(ran.status.success(), "{name} {level}: {:?}", ran.status);
        }
    }
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn aggregate_statement_expressions_run_natively_on_linux() {
    let directory = std::env::temp_dir().join(format!(
        "evocc-aggregate-statement-linux-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let sysroot = root().join("sysroot").join(LINUX);
    let mut script = String::from("cd /work\n");
    let mut keys = Vec::new();
    for (name, source) in CASES {
        for level in LEVELS {
            let stem = name.replace(' ', "-");
            let source_file = directory.join(format!("{stem}{level}.c"));
            let assembly = directory.join(format!("{stem}{level}.s"));
            std::fs::write(&source_file, source).unwrap();
            let compiled = Command::new(evocc())
                .args(["--target", LINUX])
                .arg(format!("--sysroot={}", sysroot.display()))
                .args([
                    "-w",
                    level,
                    "-S",
                    "-o",
                    assembly.to_str().unwrap(),
                    source_file.to_str().unwrap(),
                ])
                .output()
                .unwrap();
            assert!(
                compiled.status.success(),
                "{name} {level}: {}",
                String::from_utf8_lossy(&compiled.stderr)
            );
            let file = format!("{stem}{level}.s");
            script.push_str(&format!(
                "gcc-14 {file} -o {stem}{level} && ./{stem}{level}; echo \"{stem} {level} $?\"\n"
            ));
            keys.push(format!("{stem} {level}"));
        }
    }
    let output = Command::new("container")
        .args([
            "run",
            "--rm",
            "--platform",
            "linux/arm64",
            "--network",
            "none",
            "-v",
        ])
        .arg(format!("{}:/work", directory.display()))
        .args(["evo-nexlab:sealed", "sh", "-c", &script])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    for key in keys {
        let line = stdout
            .lines()
            .find(|line| line.rsplit_once(' ').is_some_and(|(head, _)| head == key));
        assert_eq!(
            line.and_then(|line| line.rsplit_once(' '))
                .map(|(_, status)| status),
            Some("0"),
            "{key}: {stdout}"
        );
    }
}
