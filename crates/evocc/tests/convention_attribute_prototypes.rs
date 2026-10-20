//! Execution and calling-convention attributes on prototypes (EvoIR 10.1.9,
//! D17). glibc's `<math.h>` declares its vector functions with
//! `aarch64_vector_pcs`, and Clang's `bounds_safety_soft_traps.h` declares two
//! functions with `preserve_all`. A prototype therefore records an attribute
//! of the list. A definition of a function that it marks stays E2952, and so
//! does each use of its designator, except for `aarch64_vector_pcs`,
//! `preserve_all` and `preserve_most`: such a callee takes base AAPCS64
//! arguments and preserves a superset of the base callee-saved registers, so
//! a call of it is an ordinary base-PCS call. Every other placement stays
//! E2952 at the attribute. `returns_twice` follows D16 (returns_twice_calls.rs).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];

/// Every attribute of the list except `returns_twice`, with arguments.
const ATTRIBUTES: [&str; 27] = [
    "naked",
    "interrupt",
    "interrupt_handler",
    "pcs(\"aapcs\")",
    "aarch64_vector_pcs",
    "aarch64_sve_pcs",
    "preserve_all",
    "preserve_most",
    "preserve_none",
    "no_callee_saved_registers",
    "no_caller_saved_registers",
    "ms_abi",
    "sysv_abi",
    "stdcall",
    "fastcall",
    "thiscall",
    "vectorcall",
    "regcall",
    "swiftcall",
    "swiftasynccall",
    "regparm(3)",
    "sseregparm",
    "callee_pop_aggregate_return(1)",
    "target(\"+sve\")",
    "target_clones(\"default\")",
    "target_version(\"sve\")",
    "optimize(\"-fwrapv\")",
];

/// The three conventions whose callers keep ordinary calls.
const BASE_CALLS: [&str; 3] = ["aarch64_vector_pcs", "preserve_all", "preserve_most"];

/// Every driver path: native assembly at two levels, the syntax check,
/// `evo check` and both interpreters.
const MODES: [&[&str]; 6] = [
    &["-O0", "--emit=asm"],
    &["-O2", "--emit=asm"],
    &["-fsyntax-only"],
    &["--check"],
    &["--interpret-c-hir"],
    &["--interpret-evoir"],
];
const COMPILE: [&[&str]; 3] = [
    &["-O0", "--emit=asm"],
    &["-O2", "--emit=asm"],
    &["-fsyntax-only"],
];
const EXECUTE: [&[&str]; 3] = [&["--check"], &["--interpret-c-hir"], &["--interpret-evoir"]];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-convention-prototypes-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn source(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, source).unwrap();
        path
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed convention probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn evocc(source: &Path, target: &str, mode: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command
        .arg(format!("--target={target}"))
        .args(["-w", "--error-format=short"]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command.args(mode).arg(source).output().unwrap()
}

/// One-based line and column of the first occurrence of `needle`.
fn position(source: &str, needle: &str) -> (usize, usize) {
    let offset = source.find(needle).expect("the needle is in the source");
    let before = &source[..offset];
    let line = before.matches('\n').count() + 1;
    let column = offset - before.rfind('\n').map_or(0, |newline| newline + 1) + 1;
    (line, column)
}

fn accepted(modes: &[&[&str]], path: &Path, target: &str) {
    for &mode in modes {
        let output = evocc(path, target, mode);
        let label = format!("{} {target} {mode:?}", path.display());
        assert!(output.status.success(), "{label}\n{}", text(&output));
        if mode[0].starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n", "{label}\n{}", text(&output));
        }
    }
}

fn refused(modes: &[&[&str]], path: &Path, target: &str, at: (usize, usize), message: &str) {
    let expected = format!("{}:{}:{}: {message}", path.display(), at.0, at.1);
    for &mode in modes {
        let output = evocc(path, target, mode);
        let label = format!("{} {target} {mode:?}", path.display());
        assert_eq!(output.status.code(), Some(1), "{label}\n{}", text(&output));
        assert!(output.stdout.is_empty(), "{label}\n{}", text(&output));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr.lines().next(),
            Some(expected.as_str()),
            "{label}\n{stderr}"
        );
    }
}

/// The E2952 message of a list attribute.
fn message(attribute: &str) -> String {
    let name = attribute.split('(').next().unwrap();
    format!("error[E2952]: unsupported-function-attribute: {name}")
}

#[test]
fn a_prototype_records_every_attribute_of_the_list() {
    let work = Work::new("prototypes");
    for attribute in ATTRIBUTES {
        let name = attribute.split('(').next().unwrap();
        let arguments = &attribute[name.len()..];
        for (index, declaration) in [
            format!("__attribute__(({attribute})) int f(void);"),
            format!("int f(void) __attribute__(({attribute}));"),
            format!("__attribute__((__{name}__{arguments})) int f(void);"),
            format!("[[gnu::{attribute}]] int f(void);"),
            format!("int g(void), __attribute__(({attribute})) f(void);"),
        ]
        .iter()
        .enumerate()
        {
            let source = format!("{declaration}\nint main(void) {{ return 0; }}\n");
            let path = work.source(&format!("prototype-{index}.c"), &source);
            for target in TARGETS {
                accepted(&MODES, &path, target);
            }
        }
    }
}

#[test]
fn a_definition_of_a_marked_function_stays_refused() {
    let work = Work::new("definitions");
    for attribute in ATTRIBUTES {
        let name = attribute.split('(').next().unwrap();
        // The attribute on the definition is refused at its name.
        let source = format!(
            "__attribute__(({attribute})) int f(void) {{ return 0; }}\nint main(void) {{ return 0; }}\n"
        );
        let path = work.source("carried.c", &source);
        let at = position(&source, name);
        for target in TARGETS {
            refused(&MODES, &path, target, at, &message(attribute));
        }
        // A prototype marks a later definition, in any order.
        for (index, source) in [
            format!(
                "int f(void) __attribute__(({attribute}));\nint f(void) {{ return 0; }}\nint main(void) {{ return 0; }}\n"
            ),
            format!(
                "int f(void) {{ return 0; }}\nint f(void) __attribute__(({attribute}));\nint main(void) {{ return 0; }}\n"
            ),
        ]
        .iter()
        .enumerate()
        {
            let path = work.source(&format!("marked-{index}.c"), source);
            let at = position(source, "f(void) { return");
            for target in TARGETS {
                refused(&MODES, &path, target, at, &message(attribute));
            }
        }
    }
}

#[test]
fn a_use_stays_refused_except_for_the_three_base_pcs_conventions() {
    let work = Work::new("uses");
    for attribute in ATTRIBUTES {
        let name = attribute.split('(').next().unwrap();
        let base = BASE_CALLS.contains(&name);
        let prototype = format!("int f(void) __attribute__(({attribute}));\n");
        let call = format!("{prototype}int main(void) {{ return f(); }}\n");
        let path = work.source("call.c", &call);
        let at = position(&call, "f(); }");
        let address =
            format!("{prototype}int main(void) {{ int (*p)(void) = f; return p == 0; }}\n");
        let taken = work.source("address.c", &address);
        for target in TARGETS {
            if base {
                // An ordinary base-PCS call; the interpreters refuse the
                // external callee as for every external call.
                accepted(&COMPILE, &path, target);
                refused(
                    &EXECUTE,
                    &path,
                    target,
                    at,
                    "error[E3039]: external-call-not-executable",
                );
                accepted(&MODES, &taken, target);
            } else {
                refused(&MODES, &path, target, at, &message(attribute));
                refused(
                    &MODES,
                    &taken,
                    target,
                    position(&address, "f; return"),
                    &message(attribute),
                );
            }
        }
    }
    // A function with two marks is refused by the one that refuses the use.
    let source =
        "int f(void) __attribute__((preserve_all, ms_abi));\nint main(void) { return f(); }\n";
    let path = work.source("both.c", source);
    for target in TARGETS {
        refused(
            &MODES,
            &path,
            target,
            position(source, "f(); }"),
            &message("ms_abi"),
        );
    }
}

#[test]
fn every_other_placement_stays_refused_at_the_attribute() {
    let work = Work::new("placements");
    for attribute in [
        "aarch64_vector_pcs",
        "preserve_all",
        "preserve_none",
        "ms_abi",
    ] {
        for (index, declarations) in [
            format!("typedef int F(void) __attribute__(({attribute}));"),
            format!("typedef __attribute__(({attribute})) int F(void);"),
            format!("int (*p)(void) __attribute__(({attribute}));"),
            format!("int (* __attribute__(({attribute})) p)(void);"),
            format!("__attribute__(({attribute})) int x;"),
            format!("__attribute__(({attribute})) int f(void), x;"),
            format!("int g(int h(void) __attribute__(({attribute})));"),
            format!("struct s {{ int (*p)(void) __attribute__(({attribute})); }};"),
            format!("int g(void) {{ return sizeof(__attribute__(({attribute})) int); }}"),
            format!("int g(void) {{ return (int (*)(void) __attribute__(({attribute})))0 != 0; }}"),
        ]
        .iter()
        .enumerate()
        {
            let source = format!("{declarations}\nint main(void) {{ return 0; }}\n");
            let path = work.source(&format!("placement-{index}.c"), &source);
            let at = position(&source, attribute);
            for target in TARGETS {
                refused(&MODES, &path, target, at, &message(attribute));
            }
        }
    }
}

#[test]
fn linux_math_headers_compile() {
    let work = Work::new("math");
    for (header, options, expected) in [
        ("math.h", &[][..], None),
        ("math.h", &["-D_GNU_SOURCE"][..], None),
        ("tgmath.h", &[][..], None),
        // L04: `_Float32 _Complex` is a separate gap, not this rule.
        ("tgmath.h", &["-D_GNU_SOURCE"][..], Some("error[E1007]")),
    ] {
        let path = work.source(
            "unit.c",
            &format!("#include <{header}>\nint main(void) {{ return 0; }}\n"),
        );
        for level in ["-O0", "-O2"] {
            let mut mode = vec![level, "-S", "-o", "/dev/null"];
            mode.extend(options);
            let output = evocc(&path, LINUX, &mode);
            match expected {
                None => assert!(
                    output.status.success(),
                    "{header} {mode:?}\n{}",
                    text(&output)
                ),
                Some(code) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    assert_eq!(output.status.code(), Some(1), "{header} {mode:?}\n{stderr}");
                    assert!(
                        stderr
                            .lines()
                            .next()
                            .is_some_and(|line| line.contains(code)),
                        "{header} {mode:?}\n{stderr}"
                    );
                }
            }
        }
    }
}

/// Darwin: an EvoCC caller of `preserve_all` and `preserve_most` functions
/// that Apple clang compiled. The callees clobber no register that the
/// caller relies on, and the values that live across the calls survive.
#[cfg(target_os = "macos")]
#[test]
fn darwin_calls_of_preserving_functions_that_apple_clang_compiled() {
    let work = Work::new("darwin");
    let callee = work.source(
        "callee.c",
        "__attribute__((preserve_all)) long keep_all(long a, long b) { return a * 3 + b; }\n\
         __attribute__((preserve_most)) long keep_most(long a, long b) { return a * 5 - b; }\n",
    );
    let caller = work.source(
        "caller.c",
        "#include <bounds_safety_soft_traps.h>\n\
         __attribute__((preserve_all)) long keep_all(long a, long b);\n\
         __attribute__((preserve_most)) long keep_most(long a, long b);\n\
         volatile long sink;\n\
         int main(void) {\n\
             long total = 0;\n\
             for (long i = 0; i < 100; i++) {\n\
                 long a = keep_all(i, total), b = keep_most(i, a);\n\
                 total += a + b;\n\
                 sink = total;\n\
             }\n\
             long expected = 0;\n\
             for (long i = 0; i < 100; i++) {\n\
                 long a = i * 3 + expected, b = i * 5 - a;\n\
                 expected += a + b;\n\
             }\n\
             return total == expected ? 0 : 1;\n\
         }\n",
    );
    let callee_object = work.0.join("callee.o");
    let output = Command::new("clang")
        .args(["-O2", "-c"])
        .arg(&callee)
        .arg("-o")
        .arg(&callee_object)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    for level in ["-O0", "-O2"] {
        let object = work.0.join(format!("caller{level}.o"));
        let output = evocc(
            &caller,
            DARWIN,
            &[level, "-c", "-o", object.to_str().unwrap()],
        );
        assert!(output.status.success(), "{level}\n{}", text(&output));
        let program = work.0.join(format!("program{level}"));
        let output = Command::new("clang")
            .arg(&object)
            .arg(&callee_object)
            .arg("-o")
            .arg(&program)
            .output()
            .unwrap();
        assert!(output.status.success(), "{level}\n{}", text(&output));
        let status = Command::new(&program).status().unwrap();
        assert_eq!(status.code(), Some(0), "{level}");
    }
}

/// Linux: a `<math.h>` program, and an EvoCC caller of a vector-PCS function
/// that GCC 14 compiled, agree with GCC 14 in the sealed image. EvoCC lowers
/// no floating vector value (E3050), so the vector-PCS callee takes integers.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_math_and_vector_pcs_calls_agree_with_gcc_14() {
    let work = Work::new("linux");
    work.source(
        "math.c",
        "#include <math.h>\n\
         volatile double input = 2.0;\n\
         int main(void) {\n\
             double root = sqrt(input * 8.0), wave = sin(input - 2.0);\n\
             return root == 4.0 && wave == 0.0 ? 0 : 1;\n\
         }\n",
    );
    work.source(
        "vpcs-callee.c",
        "__attribute__((aarch64_vector_pcs)) long vpcs_mix(long a, long b) { return a * 7 - b; }\n",
    );
    work.source(
        "vpcs-caller.c",
        "#include <math.h>\n\
         __attribute__((aarch64_vector_pcs)) long vpcs_mix(long a, long b);\n\
         volatile long sink;\n\
         int main(void) {\n\
             long total = 0;\n\
             for (long i = 0; i < 50; i++) {\n\
                 total += vpcs_mix(i, total & 7);\n\
                 sink = total;\n\
             }\n\
             long expected = 0;\n\
             for (long i = 0; i < 50; i++)\n\
                 expected += i * 7 - (expected & 7);\n\
             return total == expected ? 0 : 1;\n\
         }\n",
    );
    let mut script = String::from("set -u\ncd /work\nfailed=0\n");
    let mut lines = 0;
    for level in ["-O0", "-O2"] {
        for program in ["math", "vpcs-caller"] {
            let assembly = work.0.join(format!("{program}{level}.s"));
            let output = evocc(
                &work.0.join(format!("{program}.c")),
                LINUX,
                &[level, "-S", "-o", assembly.to_str().unwrap()],
            );
            assert!(
                output.status.success(),
                "{program} {level}\n{}",
                text(&output)
            );
        }
        script.push_str(&format!(
            "gcc-14 -std=gnu11 -w {level} math.c -lm -o math-gcc{level} && ./math-gcc{level}; s=$?; echo \"math {level} gcc-14 $s\"; [ $s -eq 0 ] || failed=1\n\
             gcc-14 math{level}.s -lm -o math-evocc{level} && ./math-evocc{level}; s=$?; echo \"math {level} evocc $s\"; [ $s -eq 0 ] || failed=1\n\
             gcc-14 -std=gnu11 -w {level} -c vpcs-callee.c -o vpcs-callee{level}.o\n\
             gcc-14 -std=gnu11 -w {level} vpcs-caller.c vpcs-callee{level}.o -o vpcs-gcc{level} && ./vpcs-gcc{level}; s=$?; echo \"vpcs {level} gcc-14 $s\"; [ $s -eq 0 ] || failed=1\n\
             gcc-14 vpcs-caller{level}.s vpcs-callee{level}.o -o vpcs-evocc{level} && ./vpcs-evocc{level}; s=$?; echo \"vpcs {level} evocc $s\"; [ $s -eq 0 ] || failed=1\n"
        ));
        lines += 4;
    }
    script.push_str("readelf -s vpcs-callee-O2.o | grep vpcs_mix\nexit $failed\n");
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
        .arg(format!("{}:/work", work.0.display()))
        .args(["evo-nexlab:sealed", "sh", "-c", &script])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(
        stdout.lines().filter(|line| line.ends_with(" 0")).count(),
        lines,
        "{stdout}"
    );
    // GCC 14 marks the callee's symbol as a variant PCS function.
    assert!(stdout.contains("[VARIANT_PCS]"), "{stdout}");
}
