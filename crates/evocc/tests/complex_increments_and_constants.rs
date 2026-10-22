//! GNU integer complex `++`/`--` and imaginary integer constants.
//! GCC 14 and Apple clang agree on these values and types.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [DARWIN, LINUX];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];

const INC_TYPES: [&str; 5] = ["char", "short", "int", "long", "unsigned"];

const IMAG_CASES: [(&str, u8, u8, u8); 10] = [
    ("4i", 8, 4, 0),
    ("4j", 8, 4, 0),
    ("4il", 16, 4, 1),
    ("4li", 16, 4, 1),
    ("4ill", 16, 4, 2),
    ("4lli", 16, 4, 2),
    ("4ui", 8, 4, 3),
    ("4iu", 8, 4, 3),
    ("4uli", 16, 4, 4),
    ("0x10i", 8, 16, 0),
];

const VALID_SUFFIXES: [(&str, u8, u8, u8); 5] = [
    ("4uil", 16, 4, 4),
    ("4iLL", 16, 4, 2),
    ("4Li", 16, 4, 1),
    ("010i", 8, 8, 0),
    ("0b11i", 8, 3, 0),
];

const INVALID_SUFFIXES: [&str; 8] = ["1i0", "0xi1", "4lil", "4lLi", "4lL", "4Ll", "4ulL", "4lLu"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn command(target: &str) -> Command {
    let mut command = Command::new(evocc());
    command.args(["--target", target, "-std=gnu17", "-w"]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command
}

fn run(source: &str, target: &str, mode: &str, level: Option<&str>) -> Output {
    let mut command = command(target);
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

fn increment_source(ty: &str) -> String {
    format!(
        "int main(void) {{\n\
         _Complex {ty} z = 3 + 4i;\n\
         _Complex {ty} a = z++;\n\
         _Complex {ty} b = ++z;\n\
         _Complex {ty} c = z--;\n\
         _Complex {ty} d = --z;\n\
         return (__real__ a == 3 && __imag__ a == 4\n\
              && __real__ b == 5 && __imag__ b == 4\n\
              && __real__ c == 5 && __imag__ c == 4\n\
              && __real__ d == 3 && __imag__ d == 4\n\
              && __real__ z == 3 && __imag__ z == 4) ? 0 : 1;\n\
         }}\n"
    )
}

fn imaginary_source(literal: &str, size: u8, value: u8, type_id: u8) -> String {
    format!(
        "#define TYPE_ID(x) _Generic((x), \\\n\
           _Complex int: 0, _Complex long: 1, _Complex long long: 2, \\\n\
           _Complex unsigned int: 3, _Complex unsigned long: 4, \\\n\
           _Complex unsigned long long: 5, default: 9)\n\
         int main(void) {{\n\
           return sizeof({literal}) == {size} && __imag__({literal}) == {value} \\\n\
             && TYPE_ID({literal}) == {type_id} ? 0 : 1;\n\
         }}\n"
    )
}

#[test]
fn integer_complex_increments_match_the_references() {
    for ty in INC_TYPES {
        let source = increment_source(ty);
        for target in TARGETS {
            for level in LEVELS {
                for mode in INTERPRETERS {
                    let output = run(&source, target, mode, Some(level));
                    assert!(
                        output.status.success(),
                        "{ty} {target} {level} {mode}\n{}",
                        text(&output)
                    );
                    assert_eq!(
                        output.stdout,
                        b"0\n",
                        "{ty} {target} {level} {mode}\n{}",
                        text(&output)
                    );
                }
            }
        }
    }
}

#[test]
fn integer_complex_increment_overflow_keeps_e3004() {
    let source = "int main(void){_Complex int z = 2147483647; z++; return 0;}\n";
    for target in TARGETS {
        for mode in INTERPRETERS {
            let output = run(source, target, mode, None);
            assert!(!output.status.success(), "{target} {mode}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3004"),
                "{target} {mode}\n{}",
                text(&output)
            );
        }
    }
}

#[test]
fn imaginary_integer_constants_match_the_references() {
    for (literal, size, value, type_id) in IMAG_CASES {
        let source = imaginary_source(literal, size, value, type_id);
        for target in TARGETS {
            for level in LEVELS {
                for mode in INTERPRETERS {
                    let output = run(&source, target, mode, Some(level));
                    assert!(
                        output.status.success(),
                        "{literal} {target} {level} {mode}\n{}",
                        text(&output)
                    );
                    assert_eq!(
                        output.stdout,
                        b"0\n",
                        "{literal} {target} {level} {mode}\n{}",
                        text(&output)
                    );
                }
            }
        }
    }
}

#[test]
fn valid_imaginary_suffixes_keep_the_reference_type() {
    for (literal, size, value, type_id) in VALID_SUFFIXES {
        let source = imaginary_source(literal, size, value, type_id);
        for target in TARGETS {
            for level in LEVELS {
                for mode in INTERPRETERS {
                    let output = run(&source, target, mode, Some(level));
                    assert!(
                        output.status.success(),
                        "{literal} {target} {level} {mode}\n{}",
                        text(&output)
                    );
                    assert_eq!(
                        output.stdout,
                        b"0\n",
                        "{literal} {target} {level} {mode}\n{}",
                        text(&output)
                    );
                }
            }
        }
    }
}

#[test]
fn invalid_imaginary_suffixes_are_e0112() {
    for spelling in INVALID_SUFFIXES {
        let source = format!("long x = {spelling};\nint main(void) {{ return 0; }}\n");
        for target in TARGETS {
            let output = run(&source, target, "--parse-only", None);
            assert!(
                !output.status.success(),
                "{spelling} {target}\n{}",
                text(&output)
            );
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("error[E0112]"),
                "{spelling} {target}\n{}",
                text(&output)
            );
        }
    }
}

#[test]
fn imaginary_integer_constants_are_refused_in_if() {
    for (spelling, codes) in [
        ("4i", &["E0111"][..]),
        ("4il", &["E0111"][..]),
        ("1i0", &["E0111", "E0112"][..]),
        ("4lL", &["E0111", "E0112"][..]),
    ] {
        let source = format!(
            "#if {spelling}\nint x=1;\n#else\nint x=0;\n#endif\nint main(void){{return 0;}}\n"
        );
        for target in TARGETS {
            let output = run(&source, target, "-E", None);
            assert!(
                !output.status.success(),
                "{spelling} {target}\n{}",
                text(&output)
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                codes
                    .iter()
                    .any(|code| stderr.contains(&format!("error[{code}]"))),
                "{spelling} {target}\n{}",
                text(&output)
            );
        }
    }
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_matches_gcc() {
    let directory =
        std::env::temp_dir().join(format!("evocc-complex-linux-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let mut script = String::from("cd /work\n");
    let mut expected = Vec::new();
    let mut sources: Vec<(String, String)> = INC_TYPES
        .iter()
        .map(|ty| (format!("inc-{ty}"), increment_source(ty)))
        .collect();
    sources.extend(IMAG_CASES.iter().map(|(literal, size, value, type_id)| {
        (
            format!("imag-{}", literal.replace("0x", "x")),
            imaginary_source(literal, *size, *value, *type_id),
        )
    }));
    sources.extend(
        VALID_SUFFIXES
            .iter()
            .map(|(literal, size, value, type_id)| {
                (
                    format!("suffix-{}", literal.replace("0x", "x")),
                    imaginary_source(literal, *size, *value, *type_id),
                )
            }),
    );
    for (name, source) in sources {
        for (level_index, level) in LEVELS.iter().enumerate() {
            let stem = format!("{name}-{level_index}");
            let source_file = directory.join(format!("{stem}.c"));
            let assembly = directory.join(format!("{stem}.s"));
            std::fs::write(&source_file, &source).unwrap();
            let compiled = command(LINUX)
                .args([level, "-S", "-o", assembly.to_str().unwrap()])
                .arg(&source_file)
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{stem}\n{}", text(&compiled));
            script.push_str(&format!(
                "gcc-14 -std=gnu17 -O2 {stem}.c -o {stem}-ref && ./{stem}-ref; echo \"{stem} ref $?\"\n\
                 gcc-14 {stem}.s -o {stem}-evocc && ./{stem}-evocc; echo \"{stem} evocc $?\"\n"
            ));
            expected.push(format!("{stem} ref 0"));
            expected.push(format!("{stem} evocc 0"));
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
fn darwin_native_matches_apple_clang() {
    let directory =
        std::env::temp_dir().join(format!("evocc-complex-darwin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let mut sources: Vec<(String, String)> = INC_TYPES
        .iter()
        .map(|ty| (format!("inc-{ty}"), increment_source(ty)))
        .collect();
    sources.extend(IMAG_CASES.iter().map(|(literal, size, value, type_id)| {
        (
            format!("imag-{}", literal.replace("0x", "x")),
            imaginary_source(literal, *size, *value, *type_id),
        )
    }));
    sources.extend(
        VALID_SUFFIXES
            .iter()
            .map(|(literal, size, value, type_id)| {
                (
                    format!("suffix-{}", literal.replace("0x", "x")),
                    imaginary_source(literal, *size, *value, *type_id),
                )
            }),
    );
    for (name, source) in sources {
        for (level_index, level) in LEVELS.iter().enumerate() {
            let stem = format!("{name}-{level_index}");
            let source_file = directory.join(format!("{stem}.c"));
            let assembly = directory.join(format!("{stem}.s"));
            std::fs::write(&source_file, &source).unwrap();
            let compiled = command(DARWIN)
                .args([level, "-S", "-o", assembly.to_str().unwrap()])
                .arg(&source_file)
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{stem}\n{}", text(&compiled));
            let program = directory.join(&stem);
            assert!(Command::new("clang")
                .args([assembly.to_str().unwrap(), "-o", program.to_str().unwrap()])
                .status()
                .unwrap()
                .success());
            assert_eq!(
                Command::new(&program).status().unwrap().code(),
                Some(0),
                "{stem}"
            );
            let reference = directory.join(format!("{stem}-ref"));
            assert!(Command::new("clang")
                .args([
                    "-std=gnu17",
                    "-O2",
                    "-w",
                    source_file.to_str().unwrap(),
                    "-o",
                    reference.to_str().unwrap(),
                ])
                .status()
                .unwrap()
                .success());
            assert_eq!(
                Command::new(&reference).status().unwrap().code(),
                Some(0),
                "{stem}"
            );
        }
    }
}
