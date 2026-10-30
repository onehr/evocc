//! Vector comparison result types and the existing long-double vector surface.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [DARWIN, LINUX];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

// lane, vector bytes, lane count, Linux mask id, Darwin mask id.
const CASES: [(&str, usize, usize, u8, u8); 24] = [
    ("char", 8, 8, 1, 3),
    ("char", 16, 16, 1, 3),
    ("short", 8, 4, 4, 4),
    ("short", 16, 8, 4, 4),
    ("int", 8, 2, 6, 6),
    ("int", 16, 4, 6, 6),
    ("long", 8, 1, 8, 10),
    ("long", 16, 2, 8, 10),
    ("long long", 8, 1, 8, 10),
    ("long long", 16, 2, 8, 10),
    ("unsigned char", 8, 8, 1, 3),
    ("unsigned char", 16, 16, 1, 3),
    ("unsigned short", 8, 4, 4, 4),
    ("unsigned short", 16, 8, 4, 4),
    ("unsigned int", 8, 2, 6, 6),
    ("unsigned int", 16, 4, 6, 6),
    ("unsigned long", 8, 1, 8, 10),
    ("unsigned long", 16, 2, 8, 10),
    ("unsigned long long", 8, 1, 8, 10),
    ("unsigned long long", 16, 2, 8, 10),
    ("float", 8, 2, 6, 6),
    ("float", 16, 4, 6, 6),
    ("double", 8, 1, 8, 10),
    ("double", 16, 2, 8, 10),
];

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
    run_with_flags(source, target, mode, level, &[])
}

fn run_with_flags(
    source: &str,
    target: &str,
    mode: &str,
    level: Option<&str>,
    flags: &[&str],
) -> Output {
    let mut command = command(target);
    if let Some(level) = level {
        command.arg(level);
    }
    command.args(flags);
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

fn temporary_directory(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "vector-type-{name}-{}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ))
}

fn compile_assembly(source: &str, target: &str, level: &str) -> (PathBuf, Output) {
    let directory = temporary_directory("assembly");
    std::fs::create_dir_all(&directory).unwrap();
    let source_file = directory.join("source.c");
    let assembly = directory.join("source.s");
    std::fs::write(&source_file, source).unwrap();
    let output = command(target)
        .args([level, "-S", "-o", assembly.to_str().unwrap()])
        .arg(&source_file)
        .output()
        .unwrap();
    (directory, output)
}

fn type_source(lane: &str, bytes: usize, lanes: usize, expected: u8) -> String {
    let initializer = (0..lanes).map(|_| "1").collect::<Vec<_>>().join(", ");
    let initializer_b = (0..lanes).map(|_| "2").collect::<Vec<_>>().join(", ");
    let candidates = [
        ("signed char", 1),
        ("unsigned char", 2),
        ("char", 3),
        ("short", 4),
        ("unsigned short", 5),
        ("int", 6),
        ("unsigned int", 7),
        ("long", 8),
        ("unsigned long", 9),
        ("long long", 10),
        ("unsigned long long", 11),
    ];
    let mut source = String::new();
    source.push_str(&format!(
        "typedef {lane} v __attribute__((vector_size({bytes})));\n"
    ));
    for (ty, _) in candidates {
        source.push_str(&format!(
            "typedef {ty} m_{} __attribute__((vector_size({bytes})));\n",
            ty.replace(' ', "_")
        ));
    }
    source.push_str("#define TID(x) _Generic((x), ");
    source.push_str(
        &candidates
            .iter()
            .map(|(ty, id)| format!("m_{}: {id}", ty.replace(' ', "_")))
            .collect::<Vec<_>>()
            .join(", "),
    );
    source.push_str(")\n");
    source.push_str(&format!(
        "int main(void) {{\n  v a = {{{initializer}}}, b = {{{initializer_b}}};\n  int bad = 0;\n"
    ));
    let operators = [
        ("<", true),
        ("==", false),
        (">", false),
        ("<=", true),
        (">=", false),
        ("!=", true),
    ];
    for (index, (operator, expected_value)) in operators.iter().enumerate() {
        let expected_value = u8::from(*expected_value);
        source.push_str(&format!(
            "  {{ __typeof__(a {operator} b) m = a {operator} b; \
             if (TID(a {operator} b) != {expected}) bad |= 1 << {index}; \
             if ((m[0] != 0) != {expected_value}) bad |= 1 << {}; }}\n",
            index + 6
        ));
    }
    source.push_str("  return bad;\n}\n");
    source
}

fn run_interpreter(source: &str, target: &str, mode: &str, level: &str) -> Output {
    run(source, target, mode, Some(level))
}

fn char_option_source(expected_id: u8, expected_value: i64) -> String {
    format!(
        "typedef unsigned char v __attribute__((vector_size(8)));\n\
         typedef char m_char __attribute__((vector_size(8)));\n\
         typedef signed char m_schar __attribute__((vector_size(8)));\n\
         typedef unsigned char m_uchar __attribute__((vector_size(8)));\n\
         #define TID(x) _Generic((x), m_char: 3, m_schar: 1, m_uchar: 2)\n\
         int main(void) {{\n\
           v a = {{1,1,1,1,1,1,1,1}}, b = {{2,2,2,2,2,2,2,2}};\n\
           __typeof__(a < b) r = a < b;\n\
           return TID(a < b) == {expected_id} && (long long)r[0] == {expected_value} ? 0 : 1;\n\
         }}\n"
    )
}

#[test]
fn char_mask_types_ignore_the_plain_char_option() {
    let cases: &[(&str, &[&str], u8, i64)] = &[
        (DARWIN, &[], 3, -1),
        (DARWIN, &["-fsigned-char"], 3, -1),
        (DARWIN, &["-funsigned-char"], 3, 255),
        (LINUX, &[], 1, -1),
        (LINUX, &["-fsigned-char"], 1, -1),
        (LINUX, &["-funsigned-char"], 1, -1),
    ];
    for (target, flags, expected_id, expected_value) in cases {
        let source = char_option_source(*expected_id, *expected_value);
        for level in LEVELS {
            for mode in INTERPRETERS {
                let output = run_with_flags(&source, target, mode, Some(level), flags);
                assert!(
                    output.status.success() && output.stdout == b"0\n",
                    "{target} {flags:?} {level} {mode}\n{}",
                    text(&output)
                );
            }
        }
    }
}

#[test]
fn the_other_64_bit_mask_assignment_is_accepted() {
    let sources = [
        "typedef long v2l __attribute__((vector_size(16)));\n\
         typedef long long v2ll __attribute__((vector_size(16)));\n\
         int main(void) { v2ll a = {0}, b = {0}; v2l m = a < b; return (int)m[0]; }\n",
        "typedef long v2l __attribute__((vector_size(16)));\n\
         typedef long long v2ll __attribute__((vector_size(16)));\n\
         int main(void) { v2l a = {0}, b = {0}; v2ll m = a < b; return (int)m[0]; }\n",
    ];
    for source in sources {
        for target in TARGETS {
            for level in LEVELS {
                let (directory, output) = compile_assembly(source, target, level);
                assert!(
                    output.status.success(),
                    "{target} {level}\n{}",
                    text(&output)
                );
                let _ = std::fs::remove_dir_all(directory);
            }
        }
    }
}

#[test]
fn vector_comparison_types_and_values_match_the_references() {
    for (lane, bytes, lanes, linux_id, darwin_id) in CASES {
        for target in TARGETS {
            let expected = if target == DARWIN {
                darwin_id
            } else {
                linux_id
            };
            let source = type_source(lane, bytes, lanes, expected);
            for level in LEVELS {
                for mode in INTERPRETERS {
                    let output = run_interpreter(&source, target, mode, level);
                    assert!(
                        output.status.success() && output.stdout == b"0\n",
                        "{lane} {bytes} {target} {level} {mode}\n{}",
                        text(&output)
                    );
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn vector_comparison_types_and_values_match_native_darwin() {
    for (lane, bytes, lanes, _, darwin_id) in CASES {
        let source = type_source(lane, bytes, lanes, darwin_id);
        for level in LEVELS {
            let (directory, compiled) = compile_assembly(&source, DARWIN, level);
            assert!(
                compiled.status.success(),
                "{lane} {bytes} {level}\n{}",
                text(&compiled)
            );
            let program = directory.join("program");
            let status = Command::new("clang")
                .args([
                    directory.join("source.s").to_str().unwrap(),
                    "-o",
                    program.to_str().unwrap(),
                ])
                .status()
                .unwrap();
            assert!(status.success(), "{lane} {bytes} {level}");
            let run = Command::new(&program).status().unwrap();
            assert_eq!(run.code(), Some(0), "{lane} {bytes} {level}");
            let _ = std::fs::remove_dir_all(directory);
        }
    }
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn vector_comparison_types_and_values_match_native_linux() {
    let directory = temporary_directory("linux-native");
    std::fs::create_dir_all(&directory).unwrap();
    let mut script = String::from("cd /work\n");
    let mut expected = Vec::new();
    for (index, (lane, bytes, lanes, linux_id, _)) in CASES.into_iter().enumerate() {
        let source = type_source(lane, bytes, lanes, linux_id);
        for (level_index, level) in LEVELS.iter().enumerate() {
            let stem = format!("case-{index}-{level_index}");
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

#[test]
fn long_double_vector_sizeof_compiles_on_linux() {
    let source = "typedef long double v2ld __attribute__((vector_size(32)));\n\
                  int main(void) { return sizeof(v2ld) == 32 ? 0 : 1; }\n";
    for level in LEVELS {
        let (directory, output) = compile_assembly(source, LINUX, level);
        assert!(output.status.success(), "{level}\n{}", text(&output));
        let _ = std::fs::remove_dir_all(directory);
    }
}

#[test]
fn long_double_vector_value_forms_are_e3050() {
    let header = "typedef long double v2ld __attribute__((vector_size(32)));\n";
    let forms = [
        format!("{header}v2ld pass(v2ld x) {{ return x; }}\n"),
        format!("{header}v2ld make(void) {{ return (v2ld){{1.0L, 2.0L}}; }}\n"),
        format!(
            "{header}int main(void) {{ v2ld a = (v2ld){{1.0L, 2.0L}}; return (int)a[0]; }}\n"
        ),
        format!(
            "{header}int main(void) {{ v2ld a; a = (v2ld){{1.0L, 2.0L}}; return 0; }}\n"
        ),
        format!(
            "{header}int main(void) {{ v2ld a = (v2ld){{1.0L, 2.0L}}, b = (v2ld){{2.0L, 3.0L}}; return a[0] < b[0]; }}\n"
        ),
    ];
    for source in forms {
        for level in LEVELS {
            let (directory, output) = compile_assembly(&source, LINUX, level);
            assert!(!output.status.success(), "{level}\n{}", text(&output));
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3050"),
                "{level}\n{}",
                text(&output)
            );
            let _ = std::fs::remove_dir_all(directory);
        }
    }
}

#[test]
fn long_double_vector_arithmetic_keeps_e3050() {
    let source = "\
typedef long double v2ld __attribute__((vector_size(32)));\n\
v2ld add(v2ld a, v2ld b) { return a + b; }\n";
    for level in LEVELS {
        let (directory, output) = compile_assembly(source, LINUX, level);
        assert!(!output.status.success(), "{level}\n{}", text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3050"),
            "{level}\n{}",
            text(&output)
        );
        let _ = std::fs::remove_dir_all(directory);
    }
}
