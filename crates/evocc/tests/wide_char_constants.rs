//! Wide character constants in `#if` and ordinary expressions. The target
//! `wchar_t` decides their value, width and signedness; `-fshort-wchar`
//! selects the 16-bit form.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [DARWIN, LINUX];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];

#[derive(Debug, Clone, Copy)]
struct IfCase {
    name: &'static str,
    condition: &'static str,
    /// `[target][default, short]`; `None` means the reference compiler refuses.
    selected: [[Option<u8>; 2]; 2],
}

const IF_CASES: [IfCase; 3] = [
    IfCase {
        name: "signedness",
        condition: "L'\\0' - 1 < 0",
        selected: [[Some(1), Some(0)], [Some(0), Some(0)]],
    },
    IfCase {
        name: "hex equality",
        condition: "L'\\xffffffff' == 0xffffffff",
        selected: [[Some(0), None], [Some(1), Some(0)]],
    },
    IfCase {
        name: "hex less than zero",
        condition: "L'\\xffffffff' < 0",
        selected: [[Some(1), None], [Some(0), Some(0)]],
    },
];

#[derive(Debug, Clone, Copy)]
struct ValueCase {
    name: &'static str,
    escape: &'static str,
    /// `[target][default, short]`; `None` means the reference compiler refuses.
    expected: [[Option<&'static str>; 2]; 2],
}

const VALUE_CASES: [ValueCase; 3] = [
    ValueCase {
        name: "ffffffff",
        escape: "ffffffff",
        expected: [
            [Some("0xffffffffffffffffULL"), None],
            [Some("0xffffffffULL"), Some("0xffffULL")],
        ],
    },
    ValueCase {
        name: "80000000",
        escape: "80000000",
        expected: [
            [Some("0xffffffff80000000ULL"), None],
            [Some("0x80000000ULL"), Some("0x0ULL")],
        ],
    },
    ValueCase {
        name: "7fffffff",
        escape: "7fffffff",
        expected: [
            [Some("0x7fffffffULL"), None],
            [Some("0x7fffffffULL"), Some("0xffffULL")],
        ],
    },
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn run(source: &str, target: &str, mode: &str, short: bool, level: Option<&str>) -> Output {
    let mut command = Command::new(evocc());
    command.args(["--target", target, "-std=gnu17", "-w"]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    if short {
        command.arg("-fshort-wchar");
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

fn if_source(case: IfCase) -> String {
    format!(
        "#if {}\nint selected = 1;\n#else\nint selected = 0;\n#endif\n\
         int main(void) {{ return selected ? 0 : 1; }}\n",
        case.condition
    )
}

fn value_source(case: ValueCase, expected: &str) -> String {
    format!(
        "int main(void) {{ unsigned long long value = L'\\x{}'; return value == {} ? 0 : 1; }}\n",
        case.escape, expected
    )
}

fn selected(output: &Output) -> Option<u8> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split("selected=")
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
fn wide_if_directives_follow_the_reference() {
    for case in IF_CASES {
        let source = if_source(case);
        for (target_index, target) in TARGETS.iter().enumerate() {
            for (flag_index, short) in [false, true].into_iter().enumerate() {
                let output = run(&source, target, "-E", short, None);
                match case.selected[target_index][flag_index] {
                    None => refused(&output),
                    Some(expected) => {
                        assert!(
                            output.status.success(),
                            "{case:?} {target} {short}\n{}",
                            text(&output)
                        );
                        assert_eq!(
                            selected(&output),
                            Some(expected),
                            "{case:?} {target} {short}\n{}",
                            text(&output)
                        );
                        for level in LEVELS {
                            let expected_exit = if expected == 1 { 0 } else { 1 };
                            for mode in INTERPRETERS {
                                let output = run(&source, target, mode, short, Some(level));
                                assert!(
                                    output.status.success(),
                                    "{case:?} {target} {short} {level} {mode}\n{}",
                                    text(&output)
                                );
                                assert_eq!(
                                    output.stdout,
                                    format!("{expected_exit}\n").into_bytes(),
                                    "{case:?} {target} {short} {level} {mode}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn wide_value_escapes_follow_the_reference() {
    for case in VALUE_CASES {
        for (target_index, target) in TARGETS.iter().enumerate() {
            for (flag_index, short) in [false, true].into_iter().enumerate() {
                match case.expected[target_index][flag_index] {
                    None => {
                        let source = format!(
                            "int main(void) {{ unsigned long long value = L'\\x{}'; return (int)value; }}\n",
                            case.escape
                        );
                        refused(&run(&source, target, "--parse-only", short, None));
                    }
                    Some(expected) => {
                        let source = value_source(case, expected);
                        for level in LEVELS {
                            for mode in INTERPRETERS {
                                let output = run(&source, target, mode, short, Some(level));
                                assert!(
                                    output.status.success(),
                                    "{case:?} {target} {short} {level} {mode}\n{}",
                                    text(&output)
                                );
                                assert_eq!(
                                    output.stdout,
                                    b"0\n",
                                    "{case:?} {target} {short} {level} {mode}\n{}",
                                    text(&output)
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn macro_noise(source: &str, target: &str) -> String {
    let noise = if target == LINUX {
        "#define __APPLE__ 1\n#undef __WCHAR_MAX__\n#undef __SIZEOF_WCHAR_T__\n"
    } else {
        "#undef __APPLE__\n#undef __WCHAR_MAX__\n#undef __SIZEOF_WCHAR_T__\n"
    };
    format!("{noise}{source}")
}

#[test]
fn user_macros_do_not_change_wide_character_rules() {
    for target in TARGETS {
        for short in [false, true] {
            for case in IF_CASES {
                let source = macro_noise(&if_source(case), target);
                let output = run(&source, target, "-E", short, None);
                match case.selected[usize::from(target == LINUX)][usize::from(short)] {
                    None => refused(&output),
                    Some(expected) => {
                        assert!(
                            output.status.success(),
                            "{case:?} {target} {short}\n{}",
                            text(&output)
                        );
                        assert_eq!(
                            selected(&output),
                            Some(expected),
                            "{case:?} {target} {short}\n{}",
                            text(&output)
                        );
                    }
                }
            }
            for case in VALUE_CASES {
                match case.expected[usize::from(target == LINUX)][usize::from(short)] {
                    None => {
                        let source = macro_noise(
                            &format!(
                                "int main(void) {{ unsigned long long value = L'\\x{}'; return (int)value; }}\n",
                                case.escape
                            ),
                            target,
                        );
                        refused(&run(&source, target, "--parse-only", short, None));
                    }
                    Some(expected) => {
                        let source = macro_noise(&value_source(case, expected), target);
                        let output = run(&source, target, "--interpret-c-hir", short, Some("-O0"));
                        assert!(
                            output.status.success(),
                            "{case:?} {target} {short}\n{}",
                            text(&output)
                        );
                        assert_eq!(output.stdout, b"0\n", "{case:?} {target} {short}");
                    }
                }
            }
        }
    }
}

#[test]
fn other_character_prefixes_are_unchanged() {
    for target in TARGETS {
        for short in [false, true] {
            let width = if short { 2 } else { 4 };
            let source = format!(
                "#if 'A' == 0x41 && u'A' == 0x41 && U'A' == 0x41\n\
                 int selected = 1;\n#else\nint selected = 0;\n#endif\n\
                 int main(void) {{ return selected ? 0 : 1; }}\n\
                 _Static_assert(sizeof(L'A') == {width}, \"wide width\");\n"
            );
            for level in LEVELS {
                for mode in INTERPRETERS {
                    let output = run(&source, target, mode, short, Some(level));
                    assert!(
                        output.status.success(),
                        "{target} {short} {level} {mode}\n{}",
                        text(&output)
                    );
                    assert_eq!(output.stdout, b"0\n", "{target} {short} {level} {mode}");
                }
            }
        }
    }
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_matches_gcc() {
    let directory =
        std::env::temp_dir().join(format!("evocc-wide-char-linux-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let sysroot = root().join("sysroot").join(LINUX);
    let mut script = String::from("cd /work\n");
    let mut expected = Vec::new();
    for short in [false, true] {
        let flag = if short { "-fshort-wchar" } else { "" };
        let flags: &[&str] = if short { &["-fshort-wchar"] } else { &[] };
        for level in LEVELS {
            for case in IF_CASES {
                let Some(selected) = case.selected[1][usize::from(short)] else {
                    continue;
                };
                let stem = format!("{}{short}{level}", case.name.replace(' ', "-"));
                let source = directory.join(format!("{stem}.c"));
                let assembly = directory.join(format!("{stem}.s"));
                std::fs::write(&source, if_source(case)).unwrap();
                let compiled = Command::new(evocc())
                    .args(["--target", LINUX, "-std=gnu17", "-w"])
                    .arg(format!("--sysroot={}", sysroot.display()))
                    .args(flags)
                    .args([level, "-S", "-o", assembly.to_str().unwrap()])
                    .arg(&source)
                    .output()
                    .unwrap();
                assert!(compiled.status.success(), "{stem}\n{}", text(&compiled));
                let expected_exit = if selected == 1 { 0 } else { 1 };
                script.push_str(&format!(
                    "gcc-14 -std=gnu17 -O2 {flag} {stem}.c -o {stem}-ref && ./{stem}-ref; echo \"{stem} ref $?\"\n\
                     gcc-14 {stem}.s -o {stem}-evocc && ./{stem}-evocc; echo \"{stem} evocc $?\"\n"
                ));
                expected.push(format!("{stem} ref {expected_exit}"));
                expected.push(format!("{stem} evocc {expected_exit}"));
            }
            for case in VALUE_CASES {
                let Some(value) = case.expected[1][usize::from(short)] else {
                    continue;
                };
                let stem = format!("{}{short}{level}", case.name);
                let source = directory.join(format!("{stem}.c"));
                let assembly = directory.join(format!("{stem}.s"));
                std::fs::write(&source, value_source(case, value)).unwrap();
                let compiled = Command::new(evocc())
                    .args(["--target", LINUX, "-std=gnu17", "-w"])
                    .arg(format!("--sysroot={}", sysroot.display()))
                    .args(flags)
                    .args([level, "-S", "-o", assembly.to_str().unwrap()])
                    .arg(&source)
                    .output()
                    .unwrap();
                assert!(compiled.status.success(), "{stem}\n{}", text(&compiled));
                script.push_str(&format!(
                    "gcc-14 -std=gnu17 -O2 {flag} {stem}.c -o {stem}-ref && ./{stem}-ref; echo \"{stem} ref $?\"\n\
                     gcc-14 {stem}.s -o {stem}-evocc && ./{stem}-evocc; echo \"{stem} evocc $?\"\n"
                ));
                expected.push(format!("{stem} ref 0"));
                expected.push(format!("{stem} evocc 0"));
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
    assert!(output.status.success(), "{stdout}");
    for line in expected {
        assert!(
            stdout.lines().any(|actual| actual == line),
            "missing {line}\n{stdout}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_native_and_apple_clang_agree() {
    let directory =
        std::env::temp_dir().join(format!("evocc-wide-char-darwin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let check = |name: &str, source: &str, expected: Option<i32>, short: bool, level: &str| {
        let flags: &[&str] = if short { &["-fshort-wchar"] } else { &[] };
        let suffix = format!("{}{short}{level}", name.replace(' ', "-"));
        let source_file = directory.join(format!("{suffix}.c"));
        std::fs::write(&source_file, source).unwrap();
        match expected {
            None => {
                let output = Command::new(evocc())
                    .args(["--target", DARWIN, "-std=gnu17", "-w"])
                    .args(flags)
                    .args([level, "-S", "-o", "/dev/null"])
                    .arg(&source_file)
                    .output()
                    .unwrap();
                refused(&output);
                let reference = Command::new("clang")
                    .args(["-std=gnu17", "-O2"])
                    .args(flags)
                    .args(["-S", "-o", "/dev/null", source_file.to_str().unwrap()])
                    .output()
                    .unwrap();
                assert!(!reference.status.success(), "{name}");
            }
            Some(expected) => {
                let assembly = directory.join(format!("{suffix}.s"));
                let compiled = Command::new(evocc())
                    .args(["--target", DARWIN, "-std=gnu17", "-w"])
                    .args(flags)
                    .args([level, "-S", "-o", assembly.to_str().unwrap()])
                    .arg(&source_file)
                    .output()
                    .unwrap();
                assert!(compiled.status.success(), "{name}\n{}", text(&compiled));
                let program = directory.join(&suffix);
                assert!(Command::new("clang")
                    .args([assembly.to_str().unwrap(), "-o", program.to_str().unwrap()])
                    .status()
                    .unwrap()
                    .success());
                assert_eq!(
                    Command::new(&program).status().unwrap().code(),
                    Some(expected)
                );
                let reference = directory.join(format!("{suffix}-clang"));
                let status = Command::new("clang")
                    .args(["-std=gnu17", "-O2", "-w"])
                    .args(flags)
                    .args([
                        source_file.to_str().unwrap(),
                        "-o",
                        reference.to_str().unwrap(),
                    ])
                    .output()
                    .unwrap();
                assert!(
                    status.status.success(),
                    "{name}\n{}",
                    String::from_utf8_lossy(&status.stderr)
                );
                assert_eq!(
                    Command::new(&reference).status().unwrap().code(),
                    Some(expected)
                );
            }
        }
    };
    for short in [false, true] {
        for level in LEVELS {
            for case in IF_CASES {
                let expected =
                    case.selected[0][usize::from(short)]
                        .map(|selected| if selected == 1 { 0 } else { 1 });
                check(case.name, &if_source(case), expected, short, level);
            }
            for case in VALUE_CASES {
                let expected = case.expected[0][usize::from(short)].map(|_| 0);
                let source = case
                    .expected[0][usize::from(short)]
                    .map(|expected| value_source(case, expected))
                    .unwrap_or_else(|| {
                        format!(
                            "int main(void) {{ unsigned long long value = L'\\x{}'; return (int)value; }}\n",
                            case.escape
                        )
                    });
                check(case.name, &source, expected, short, level);
            }
        }
    }
}
