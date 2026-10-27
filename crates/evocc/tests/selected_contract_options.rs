//! N09g: four GCC options on both drivers and both targets. `-fno-common`
//! and a UTF-8 `-finput-charset` select contracts that EvoCC has.
//! `-fno-builtin-NAME` disables the recognition of one name, and
//! `-fno-inline` keeps every optional substitution out (EvoIR 10.1.9).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
static NEXT: AtomicU32 = AtomicU32::new(0);

struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-selected-options-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn source(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc(target: &str, arguments: &[&str], source: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command.arg(format!("--target={target}"));
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command.args(arguments).arg(source).output().unwrap()
}

fn detail(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The output of a run that succeeded with no diagnostic.
fn clean(output: Output, context: &str) -> String {
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "{context}: {}",
        detail(&output)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// The strict driver refuses the option as it does today.
fn refused(output: Output, option: &str, context: &str) {
    assert!(
        output.status.code() == Some(1)
            && output.stdout.is_empty()
            && String::from_utf8_lossy(&output.stderr)
                .contains(&format!("unsupported option `{option}`")),
        "{context}: {}",
        detail(&output)
    );
}

/// The number of direct calls of `name` in the assembly.
fn calls(assembly: &str, target: &str, name: &str) -> usize {
    let symbol = if target == DARWIN {
        format!("_{name}")
    } else {
        name.to_owned()
    };
    assembly
        .lines()
        .filter(|line| line.trim() == format!("bl {symbol}"))
        .count()
}

/// Whether the assembly defines the function `name`.
fn defines(assembly: &str, target: &str, name: &str) -> bool {
    let label = if target == DARWIN {
        format!("_{name}:")
    } else {
        format!("{name}:")
    };
    assembly.lines().any(|line| line == label)
}

/// GCC 14 accepts each spelling: the letter case and the hyphen may differ.
const UTF8_CHARSETS: [&str; 7] = ["utf-8", "UTF-8", "Utf-8", "uTf-8", "utf8", "UTF8", "uTF8"];

#[test]
fn options_that_name_an_existing_contract_change_no_code() {
    let work = Work::new();
    // A tentative definition and a UTF-8 source character.
    let source = work.source(
        "input.c",
        "int tentative;\nconst char *text = \"\u{c4}\";\nint f(void) { return tentative + text[1]; }\n",
    );
    let mut options = vec!["-fno-common".to_owned()];
    options.extend(UTF8_CHARSETS.map(|charset| format!("-finput-charset={charset}")));
    for target in [LINUX, DARWIN] {
        let expected = clean(evocc(target, &["--emit=asm"], &source), target);
        for option in &options {
            let context = format!("{target} {option}");
            let assembly = clean(evocc(target, &["--emit=asm", option], &source), &context);
            assert!(assembly == expected, "{context}: the assembly changed");
            // The cc command accepts it with no diagnostic, also under -Werror.
            clean(
                evocc(target, &["-Werror", option, "-fsyntax-only"], &source),
                &context,
            );
        }
    }
}

#[test]
fn other_common_and_charset_options_keep_their_refusal() {
    let work = Work::new();
    let source = work.source("input.c", "int tentative;\n");
    for target in [LINUX, DARWIN] {
        for option in [
            "-fcommon",
            "-finput-charset=latin1",
            "-finput-charset=ISO-8859-1",
            "-finput-charset=utf_8",
            "-finput-charset=ISO-10646/UTF8/",
            "-finput-charset=",
        ] {
            let context = format!("{target} {option}");
            refused(
                evocc(target, &["--emit=asm", option], &source),
                option,
                &context,
            );
        }
    }
}

#[test]
fn a_named_builtin_is_disabled_alone() {
    let work = Work::new();
    let source = work.source(
        "input.c",
        "extern int abs(int); extern long labs(long);\n\
         int f(int x) { return abs(x) + (int)labs(x) + __builtin_abs(x); }\n",
    );
    for target in [LINUX, DARWIN] {
        let asm = |options: &[&str]| {
            let mut arguments = vec!["--emit=asm"];
            arguments.extend_from_slice(options);
            clean(
                evocc(target, &arguments, &source),
                &format!("{target} {options:?}"),
            )
        };
        let library = |assembly: &str| {
            (
                calls(assembly, target, "abs"),
                calls(assembly, target, "labs"),
            )
        };
        let expected = asm(&[]);
        assert_eq!(library(&expected), (0, 0));
        // One call: the ordinary `abs`. `__builtin_abs` keeps its rule, and
        // `-fbuiltin` does not restore the name, as in GCC 14.
        let disabled: [&[&str]; 3] = [
            &["-fno-builtin-abs"],
            &["-fno-builtin-abs", "-fbuiltin"],
            &["-fbuiltin", "-fno-builtin-abs"],
        ];
        for options in disabled {
            assert_eq!(library(&asm(options)), (1, 0), "{target} {options:?}");
        }
        assert_eq!(
            library(&asm(&["-fno-builtin-abs", "-fno-builtin-labs"])),
            (1, 1)
        );
        // The order rule of `-fbuiltin` and `-fno-builtin` does not change.
        assert_eq!(library(&asm(&["-fno-builtin"])), (1, 1));
        assert!(asm(&["-fno-builtin", "-fbuiltin"]) == expected, "{target}");
        // A name that EvoCC does not recognize has no effect.
        for option in ["-fno-builtin-unknown_name", "-fno-builtin-printf"] {
            assert!(asm(&[option]) == expected, "{target} {option}");
        }
        clean(
            evocc(
                target,
                &["-Werror", "-fno-builtin-abs", "-fsyntax-only"],
                &source,
            ),
            target,
        );
        refused(
            evocc(target, &["--emit=asm", "-fno-builtin-"], &source),
            "-fno-builtin-",
            target,
        );
    }
}

#[test]
fn no_inline_substitutes_only_always_inline_callees() {
    let work = Work::new();
    let source = work.source(
        "input.c",
        "static int helper(int x) { return x + 1; }\n\
         static int seven(void) { return 7; }\n\
         static inline __attribute__((always_inline)) int forced(int x) { return x * 2; }\n\
         int main(void) { return helper(20) + seven() + forced(8) - 44; }\n",
    );
    for target in [LINUX, DARWIN] {
        // Today EvoCC substitutes the two small static helpers too.
        let assembly = clean(evocc(target, &["--emit=asm"], &source), target);
        for name in ["helper", "seven", "forced"] {
            assert_eq!(calls(&assembly, target, name), 0, "{target} {name}");
        }
        let context = format!("{target} -fno-inline");
        let assembly = clean(
            evocc(target, &["--emit=asm", "-fno-inline"], &source),
            &context,
        );
        for name in ["helper", "seven"] {
            assert_eq!(calls(&assembly, target, name), 1, "{context} {name}");
            assert!(defines(&assembly, target, name), "{context} {name}");
        }
        assert_eq!(calls(&assembly, target, "forced"), 0, "{context}");
        assert!(!defines(&assembly, target, "forced"), "{context}");
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = clean(evocc(target, &[mode, "-fno-inline"], &source), &context);
            assert_eq!(output, "0\n", "{context} {mode}");
        }
        let output = clean(
            evocc(target, &["--check", "-fno-inline"], &source),
            &context,
        );
        assert!(output.contains("main returned 0"), "{context}: {output}");
        clean(
            evocc(
                target,
                &["-Werror", "-fno-inline", "-fsyntax-only"],
                &source,
            ),
            &context,
        );
    }
}

/// LLVM test-suite 2919e55176a5, SingleSource/Regression/C/gcc-c-torture/
/// execute. These four old PASS programs stopped at `unsupported option`.
/// The arguments are the original ones; the ladder's LLVM projection drops
/// only the compiler, `-o` and `-c`.
const ORIGINAL_ARGUMENTS: [&str; 9] = [
    "-DNDEBUG",
    "-O3",
    "-DNDEBUG",
    "-w",
    "-Werror=date-time",
    "-Wno-implicit-int",
    "-Wno-int-conversion",
    "-Wno-implicit-function-declaration",
    "-w",
];
const ORIGINALS: [(&str, &str); 4] = [
    ("20040709-3", "-fno-common"),
    ("pr68506", "-fno-builtin-abort"),
    ("stkalign", "-fno-inline"),
    ("wchar_t-1", "-finput-charset=utf-8"),
];

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn the_four_llvm_originals_run_in_the_sealed_linux_image() {
    let tree = root().join(
        ".build/ladder/llvm-test-suite/llvm-test-suite-2919e55176a572ee085cb46b291c47b7092c363e\
         /SingleSource/Regression/C/gcc-c-torture/execute",
    );
    assert!(
        tree.is_dir(),
        "the pinned LLVM test-suite tree is required for this acceptance test"
    );
    let work = Work::new();
    let mut script = String::from("set -eu\n");
    for (name, option) in ORIGINALS {
        let mut arguments = vec!["--emit=asm"];
        arguments.extend(ORIGINAL_ARGUMENTS);
        arguments.push(option);
        let output = evocc(LINUX, &arguments, &tree.join(format!("{name}.c")));
        assert!(output.status.success(), "{name}: {}", detail(&output));
        std::fs::write(work.0.join(format!("{name}.s")), &output.stdout).unwrap();
        // The reference is GCC 14 with the same arguments.
        script.push_str(&format!(
            "cd /tree && gcc-14 {} {option} {name}.c -o /tmp/reference\n\
             gcc-14 /work/{name}.s -o /tmp/subject\n\
             /tmp/reference > /tmp/reference.out\n\
             /tmp/subject > /tmp/subject.out\n\
             cmp /tmp/reference.out /tmp/subject.out\n\
             echo '{name}: PASS'\n",
            ORIGINAL_ARGUMENTS.join(" ")
        ));
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
        .arg(format!("{}:/tree:ro", tree.display()))
        .arg("-v")
        .arg(format!("{}:/work", work.0.display()))
        .args(["evo-nexlab:sealed", "sh", "-c", &script])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", detail(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.matches(": PASS").count(), 4, "{stdout}");
}
