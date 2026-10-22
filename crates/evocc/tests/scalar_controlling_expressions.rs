//! C requires a scalar controlling expression. These tests pin the refusal
//! at the operand for records, unions, vectors and void, and keep pointer and
//! complex operands as accepted scalar values.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [DARWIN, LINUX];
const REFUSAL_MODES: [&str; 3] = ["--parse-only", "--emit=evoir", "--emit=asm"];
const RUN_MODES: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
const LEVELS: [&str; 2] = ["-O0", "-O2"];

#[derive(Debug, Clone, Copy)]
struct Operand {
    name: &'static str,
    prelude: &'static str,
    parameter: &'static str,
    expression: &'static str,
    accepted: bool,
    setup: &'static str,
    value: &'static str,
}

const OPERANDS: [Operand; 8] = [
    Operand {
        name: "record",
        prelude: "struct Record { int member; };",
        parameter: "struct Record value",
        expression: "value",
        accepted: false,
        setup: "",
        value: "",
    },
    Operand {
        name: "union",
        prelude: "union Union { int member; };",
        parameter: "union Union value",
        expression: "value",
        accepted: false,
        setup: "",
        value: "",
    },
    Operand {
        name: "pointer",
        prelude: "",
        parameter: "int *value",
        expression: "value",
        accepted: true,
        setup: "int target = 1; int *value = &target;",
        value: "value",
    },
    Operand {
        name: "integer vector",
        prelude: "typedef int IntVector __attribute__((vector_size(16)));",
        parameter: "IntVector value",
        expression: "value",
        accepted: false,
        setup: "",
        value: "",
    },
    Operand {
        name: "floating vector",
        prelude: "typedef float FloatVector __attribute__((vector_size(16)));",
        parameter: "FloatVector value",
        expression: "value",
        accepted: false,
        setup: "",
        value: "",
    },
    Operand {
        name: "floating complex",
        prelude: "",
        parameter: "float _Complex value",
        expression: "value",
        accepted: true,
        setup: "float _Complex value = 1.0f;",
        value: "value",
    },
    Operand {
        name: "integer complex",
        prelude: "",
        parameter: "_Complex int value",
        expression: "value",
        accepted: true,
        setup: "_Complex int value = 1;",
        value: "value",
    },
    Operand {
        name: "void",
        prelude: "void get(void);",
        parameter: "void",
        expression: "get()",
        accepted: false,
        setup: "",
        value: "",
    },
];

#[derive(Debug, Clone, Copy)]
enum Construct {
    If,
    While,
    Do,
    For,
    Ternary,
    Not,
    And,
    Or,
}

const CONSTRUCTS: [Construct; 8] = [
    Construct::If,
    Construct::While,
    Construct::Do,
    Construct::For,
    Construct::Ternary,
    Construct::Not,
    Construct::And,
    Construct::Or,
];

fn body(construct: Construct, expression: &str) -> String {
    match construct {
        Construct::If => format!("if ({expression}) return 1; return 0;"),
        Construct::While => format!("while ({expression}) return 1; return 0;"),
        Construct::Do => format!("do {{ return 1; }} while ({expression}); return 0;"),
        Construct::For => format!("for (; {expression};) return 1; return 0;"),
        Construct::Ternary => format!("return ({expression}) ? 1 : 0;"),
        Construct::Not => format!("return !({expression});"),
        Construct::And => format!("return ({expression}) && 1;"),
        Construct::Or => format!("return ({expression}) || 0;"),
    }
}

fn source(construct: Construct, operand: Operand) -> String {
    let function = if operand.parameter == "void" {
        "int subject(void)".to_owned()
    } else {
        format!("int subject({})", operand.parameter)
    };
    let mut text = String::new();
    if !operand.prelude.is_empty() {
        text.push_str(operand.prelude);
        text.push('\n');
    }
    text.push_str(&function);
    text.push_str(" { ");
    text.push_str(&body(construct, operand.expression));
    text.push_str(" }\n");
    if operand.accepted {
        let expected = match construct {
            Construct::Not => 0,
            Construct::If
            | Construct::While
            | Construct::Do
            | Construct::For
            | Construct::Ternary
            | Construct::And
            | Construct::Or => 1,
        };
        text.push_str(&format!(
            "int main(void) {{ {} return subject({}) == {expected} ? 0 : 1; }}\n",
            operand.setup, operand.value
        ));
    }
    text
}

fn run(source: &str, target: &str, mode: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([
            "--target",
            target,
            mode,
            "--error-format=short",
            "-w",
            "/dev/stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

fn anchor(construct: Construct, operand: Operand) -> &'static str {
    match construct {
        Construct::If | Construct::While | Construct::Do | Construct::For => {
            if operand.parameter == "void" {
                "get()"
            } else {
                operand.expression
            }
        }
        Construct::Ternary | Construct::Not | Construct::And | Construct::Or => {
            if operand.parameter == "void" {
                "(get())"
            } else {
                "(value)"
            }
        }
    }
}

fn expected_code(construct: Construct, operand: Operand) -> Option<&'static str> {
    if operand.accepted {
        return None;
    }
    match construct {
        Construct::If | Construct::While | Construct::Do | Construct::For => Some("E2103"),
        Construct::Not | Construct::And | Construct::Or => {
            if operand.name.contains("vector") {
                Some("E2102")
            } else {
                Some("E2103")
            }
        }
        Construct::Ternary => {
            if operand.name.contains("vector") {
                Some("E2202")
            } else {
                Some("E2103")
            }
        }
    }
}

#[test]
fn non_scalar_controlling_expressions_fail_at_the_operand() {
    for construct in CONSTRUCTS {
        for operand in OPERANDS {
            let Some(code) = expected_code(construct, operand) else {
                continue;
            };
            let text = source(construct, operand);
            let offset = text
                .rfind(anchor(construct, operand))
                .expect("operand anchor");
            let prefix = &text[..offset];
            let line = prefix.bytes().filter(|&byte| byte == b'\n').count() + 1;
            let column = prefix.rsplit('\n').next().unwrap().len() + 1;
            for target in TARGETS {
                for mode in REFUSAL_MODES {
                    let output = run(&text, target, mode);
                    let message = String::from_utf8_lossy(&output.stderr);
                    assert_eq!(
                        output.status.code(),
                        Some(1),
                        "{construct:?} {operand:?} {target} {mode}\n{message}"
                    );
                    assert!(
                        message.starts_with(&format!("/dev/stdin:{line}:{column}: error[{code}]:")),
                        "{construct:?} {operand:?} {target} {mode}\n{message}"
                    );
                    assert!(!message.contains("E3101"), "{message}");
                    assert!(!message.contains("E3007"), "{message}");
                    assert!(output.stdout.is_empty(), "{message}");
                }
            }
        }
    }
}

#[test]
fn scalar_controlling_expressions_still_run() {
    for construct in CONSTRUCTS {
        for operand in OPERANDS {
            if !operand.accepted {
                continue;
            }
            let text = source(construct, operand);
            for target in TARGETS {
                for mode in RUN_MODES {
                    let output = run(&text, target, mode);
                    let message = String::from_utf8_lossy(&output.stderr);
                    assert!(
                        output.status.success(),
                        "{construct:?} {operand:?} {target} {mode}\n{message}"
                    );
                    assert_eq!(
                        output.stdout, b"0\n",
                        "{construct:?} {operand:?} {target} {mode}\n{message}"
                    );
                }
            }
        }
    }
}

/// Native Darwin checks use the host assembler and linker.
#[cfg(target_os = "macos")]
#[test]
fn scalar_controlling_expressions_run_natively_on_darwin() {
    let directory = std::env::temp_dir().join(format!(
        "evocc-scalar-control-darwin-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    for construct in CONSTRUCTS {
        for operand in OPERANDS {
            if !operand.accepted {
                continue;
            }
            for level in LEVELS {
                let stem = format!("{construct:?}-{}", operand.name.replace(' ', "-"));
                let source_file = directory.join(format!("{stem}.c"));
                let assembly = directory.join(format!("{stem}{level}.s"));
                let executable = directory.join(format!("{stem}{level}"));
                std::fs::write(&source_file, source(construct, operand)).unwrap();
                let compiled = Command::new(env!("CARGO_BIN_EXE_evocc"))
                    .args([
                        "--target",
                        DARWIN,
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
                    "{construct:?} {operand:?} {level}: {}",
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
                    "{construct:?} {operand:?} {level}: {}",
                    String::from_utf8_lossy(&linked.stderr)
                );
                let ran = Command::new(&executable).output().unwrap();
                assert!(
                    ran.status.success(),
                    "{construct:?} {operand:?} {level}: {:?}",
                    ran.status
                );
            }
        }
    }
}

/// The Linux native lane runs every accepted case in the sealed image.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn scalar_controlling_expressions_run_natively_on_linux() {
    let directory =
        std::env::temp_dir().join(format!("evocc-scalar-control-linux-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sysroot = root.join("sysroot/aarch64-unknown-linux-gnu");
    let mut script = String::from("cd /work\n");
    let mut cases = Vec::new();
    for construct in CONSTRUCTS {
        for operand in OPERANDS {
            if !operand.accepted {
                continue;
            }
            for level in LEVELS {
                let stem = format!("{construct:?}-{}", operand.name.replace(' ', "-"));
                let source_file = directory.join(format!("{stem}.c"));
                let assembly = directory.join(format!("{stem}{level}.s"));
                std::fs::write(&source_file, source(construct, operand)).unwrap();
                let compiled = Command::new(env!("CARGO_BIN_EXE_evocc"))
                    .args(["--target", LINUX])
                    .arg(format!("--sysroot={}", sysroot.display()))
                    .args([
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
                    "{construct:?} {operand:?} {level}: {}",
                    String::from_utf8_lossy(&compiled.stderr)
                );
                let file = format!("{stem}{level}.s");
                script.push_str(&format!(
                    "gcc-14 {file} -o {stem}{level} && {{ ./{stem}{level}; echo \"{stem} {level} $?\"; }}\n"
                ));
                cases.push((format!("{stem} {level}"), construct, operand));
            }
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
    assert!(output.status.success(), "{}", stdout);
    let mut failures = Vec::new();
    for (key, construct, operand) in cases {
        let line = stdout
            .lines()
            .find(|line| line.rsplit_once(' ').is_some_and(|(head, _)| head == key));
        match line.and_then(|line| line.rsplit_once(' ')) {
            Some((_, "0")) => {}
            other => failures.push(format!("{key} ({construct:?} {operand:?}): {other:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
