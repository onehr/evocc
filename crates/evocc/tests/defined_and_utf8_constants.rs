//! `defined` produced by macro expansion and UTF-8 plain character constants.
//! GCC 14 evaluates the expansion; Apple clang refuses a plain UTF-8 constant.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [DARWIN, LINUX];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];

const DEFINED_CASES: [(&str, &str, u8); 7] = [
    (
        "object",
        "#define D defined(FOO)\n#define FOO 1\n#if D\nint v=1;\n#else\nint v=0;\n#endif\n",
        1,
    ),
    (
        "object undefined",
        "#define D defined(BAR)\n#if D\nint v=1;\n#else\nint v=0;\n#endif\n",
        0,
    ),
    (
        "empty argument",
        "#define HAS() defined(FOO)\n#define FOO 1\n#if HAS()\nint v=1;\n#else\nint v=0;\n#endif\n",
        1,
    ),
    (
        "empty argument undefined",
        "#define HAS() defined(BAR)\n#if HAS()\nint v=1;\n#else\nint v=0;\n#endif\n",
        0,
    ),
    (
        "no parentheses",
        "#define D defined FOO\n#define FOO 1\n#if D\nint v=1;\n#else\nint v=0;\n#endif\n",
        1,
    ),
    (
        "nested",
        "#define D defined(FOO)\n#define A D\n#define FOO 1\n#if A\nint v=1;\n#else\nint v=0;\n#endif\n",
        1,
    ),
    (
        "elif",
        "#define D defined(FOO)\n#define FOO 1\n#if 0\nint v=2;\n#elif D\nint v=1;\n#else\nint v=0;\n#endif\n",
        1,
    ),
];

#[derive(Debug, Clone, Copy)]
struct Utf8Case {
    literal: &'static str,
    value: &'static str,
    if_value: &'static str,
    if_selected: u8,
}

const PLAIN_UTF8: [Utf8Case; 4] = [
    Utf8Case {
        literal: "'é'",
        value: "0xC3A9ULL",
        if_value: "0xC3A9",
        if_selected: 1,
    },
    Utf8Case {
        literal: "'€'",
        value: "0xE282ACULL",
        if_value: "0xE282AC",
        if_selected: 1,
    },
    Utf8Case {
        literal: "'😀'",
        value: "0xfffffffff09f9880ULL",
        if_value: "0xF09F9880",
        if_selected: 0,
    },
    Utf8Case {
        literal: "'aé'",
        value: "0x61C3A9ULL",
        if_value: "0x61C3A9",
        if_selected: 1,
    },
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn run(source: &str, target: &str, mode: &str, level: Option<&str>) -> Output {
    let mut command = Command::new(evocc());
    command.args(["--target", target, "-std=gnu17", "-w"]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    if let Some(level) = level {
        command.arg(level);
    }
    command
        .args([mode, "/dev/stdin"])
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

fn selected(output: &Output) -> Option<u8> {
    String::from_utf8_lossy(&output.stdout)
        .split("v=")
        .nth(1)
        .and_then(|tail| tail.chars().next())
        .and_then(|digit| digit.to_digit(10))
        .map(|digit| digit as u8)
}

fn refused(output: &Output) {
    assert!(!output.status.success(), "{}", text(output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("error[E1006]"),
        "{}",
        text(output)
    );
}

#[test]
fn defined_from_macro_expansion_is_evaluated() {
    for (name, source, expected) in DEFINED_CASES {
        let source = format!("{source}int main(void) {{ return v == 1 ? 0 : 1; }}\n");
        for target in TARGETS {
            let output = run(&source, target, "-E", None);
            assert!(
                output.status.success(),
                "{name} {target}\n{}",
                text(&output)
            );
            assert_eq!(selected(&output), Some(expected), "{name} {target}");
            let expected_exit = if expected == 1 { 0 } else { 1 };
            for level in LEVELS {
                for mode in INTERPRETERS {
                    let output = run(&source, target, mode, Some(level));
                    assert!(
                        output.status.success(),
                        "{name} {target} {level} {mode}\n{}",
                        text(&output)
                    );
                    assert_eq!(
                        output.stdout,
                        format!("{expected_exit}\n").into_bytes(),
                        "{name} {target} {level} {mode}"
                    );
                }
            }
        }
    }
}

#[test]
fn plain_utf8_constants_follow_gcc_and_darwin_refuses() {
    for case in PLAIN_UTF8 {
        for target in TARGETS {
            let source = format!(
                "int main(void) {{ unsigned long long v = {}; return v == {} ? 0 : 1; }}\n",
                case.literal, case.value
            );
            if target == DARWIN {
                refused(&run(&source, target, "--parse-only", None));
                continue;
            }
            for level in LEVELS {
                for mode in INTERPRETERS {
                    let output = run(&source, target, mode, Some(level));
                    assert!(
                        output.status.success(),
                        "{case:?} {target} {level} {mode}\n{}",
                        text(&output)
                    );
                    assert_eq!(output.stdout, b"0\n", "{case:?} {target} {level} {mode}");
                }
            }
            let directive = format!(
                "#if {} == {}\nint v=1;\n#else\nint v=0;\n#endif\nint main(void){{return v?0:1;}}\n",
                case.literal, case.if_value
            );
            let output = run(&directive, target, "-E", None);
            assert!(output.status.success(), "{case:?}\n{}", text(&output));
            assert_eq!(selected(&output), Some(case.if_selected), "{case:?}");
        }
    }
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_matches_gcc() {
    let directory =
        std::env::temp_dir().join(format!("evocc-defined-utf8-linux-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let sysroot = root().join("sysroot").join(LINUX);
    let mut script = String::from("cd /work\n");
    let cases = [
        (
            "defined",
            format!("{}int main(void){{return v==1?0:1;}}\n", DEFINED_CASES[0].1),
            0,
        ),
        (
            "plain-eacute",
            "int main(void){unsigned long long v='é';return v==0xC3A9ULL?0:1;}\n".to_owned(),
            0,
        ),
        (
            "plain-grin",
            "int main(void){unsigned long long v='😀';return v==0xfffffffff09f9880ULL?0:1;}\n"
                .to_owned(),
            0,
        ),
        (
            "L-grin",
            "int main(void){unsigned long long v=L'😀';return v==0x1F600ULL?0:1;}\n".to_owned(),
            0,
        ),
        (
            "U-grin",
            "int main(void){unsigned long long v=U'😀';return v==0x1F600ULL?0:1;}\n".to_owned(),
            0,
        ),
    ];
    let mut expected = Vec::new();
    for (name, source, expected_exit) in cases {
        let source_file = directory.join(format!("{name}.c"));
        let assembly = directory.join(format!("{name}.s"));
        std::fs::write(&source_file, source).unwrap();
        let compiled = Command::new(evocc())
            .args(["--target", LINUX, "-std=gnu17", "-w"])
            .arg(format!("--sysroot={}", sysroot.display()))
            .args(["-O0", "-S", "-o", assembly.to_str().unwrap()])
            .arg(&source_file)
            .output()
            .unwrap();
        assert!(compiled.status.success(), "{name}\n{}", text(&compiled));
        script.push_str(&format!(
            "gcc-14 -std=gnu17 -O2 {name}.c -o {name}-ref && ./{name}-ref; echo \"{name} ref $?\"\n\
             gcc-14 {name}.s -o {name}-evocc && ./{name}-evocc; echo \"{name} evocc $?\"\n"
        ));
        expected.push(format!("{name} ref {expected_exit}"));
        expected.push(format!("{name} evocc {expected_exit}"));
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
    for line in expected {
        assert!(
            stdout.lines().any(|actual| actual == line),
            "missing {line}\n{stdout}"
        );
    }
}

#[test]
fn prefixed_utf8_constants_keep_their_types() {
    for target in TARGETS {
        for (literal, value, width) in [
            ("L'é'", "0xE9ULL", 4u8),
            ("L'€'", "0x20ACULL", 4),
            ("L'😀'", "0x1F600ULL", 4),
            ("u'é'", "0xE9ULL", 2),
            ("u'€'", "0x20ACULL", 2),
            ("U'é'", "0xE9ULL", 4),
            ("U'€'", "0x20ACULL", 4),
            ("U'😀'", "0x1F600ULL", 4),
        ] {
            let source = format!(
                "int main(void) {{ unsigned long long v = {literal}; return v == {value} && sizeof({literal}) == {width} ? 0 : 1; }}\n"
            );
            for level in LEVELS {
                for mode in INTERPRETERS {
                    let output = run(&source, target, mode, Some(level));
                    assert!(
                        output.status.success(),
                        "{literal} {target} {level} {mode}\n{}",
                        text(&output)
                    );
                    assert_eq!(output.stdout, b"0\n", "{literal} {target} {level} {mode}");
                }
            }
        }
    }
}

#[test]
fn unsigned_short_utf8_grin_follows_the_target() {
    for target in TARGETS {
        if target == DARWIN {
            refused(&run(
                "int main(void) { return (int)u'😀'; }\n",
                target,
                "--parse-only",
                None,
            ));
            continue;
        }
        let source = "int main(void) { unsigned long long v = u'😀'; return v == 0xDE00ULL && sizeof(u'😀') == 2 ? 0 : 1; }\n";
        for level in LEVELS {
            for mode in INTERPRETERS {
                let output = run(source, target, mode, Some(level));
                assert!(
                    output.status.success(),
                    "{target} {level} {mode}\n{}",
                    text(&output)
                );
                assert_eq!(output.stdout, b"0\n", "{target} {level} {mode}");
            }
        }
    }
}

#[test]
fn defined_requires_an_identifier_operand() {
    for source in [
        "#if defined(1)\nint v=1;\n#else\nint v=0;\n#endif\n",
        "#define HAS(x) defined(x)\n#define FOO 1\n#if HAS(FOO)\nint v=1;\n#else\nint v=0;\n#endif\n",
    ] {
        for target in TARGETS {
            let output = run(source, target, "-E", None);
            assert!(!output.status.success(), "{target}\n{}", text(&output));
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("error[E0111]"),
                "{target}\n{}",
                text(&output)
            );
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_trace_header_takes_the_clang_branch() {
    let source = "#include <os/trace_base.h>\n#if OS_LOG_TARGET_HAS_10_15_FEATURES\nint v=1;\n#else\nint v=0;\n#endif\nint main(void){return v?0:1;}\n";
    let directory = std::env::temp_dir().join(format!("evocc-trace-header-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("trace.c");
    std::fs::write(&path, source).unwrap();
    let ours = Command::new(evocc())
        .args([
            "--target",
            DARWIN,
            "-std=gnu17",
            "-w",
            "-mmacosx-version-min=11.0",
            "-E",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(ours.status.success(), "{}", text(&ours));
    let expected = Command::new("clang")
        .args(["-std=gnu17", "-mmacosx-version-min=11.0", "-E"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(expected.status.success(), "{}", text(&expected));
    let ours = String::from_utf8_lossy(&ours.stdout);
    let expected = String::from_utf8_lossy(&expected.stdout);
    let branch = |text: &str| text.contains("int v=1;");
    assert_eq!(branch(&ours), branch(&expected), "{ours}\n---\n{expected}");
}
