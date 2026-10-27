//! N09i: options that only permit a transform that EvoCC never makes, and
//! the default `-ftrapping-math`. Both drivers accept them on both targets,
//! with no diagnostic and no change of code. On Linux, `-fno-trapping-math`
//! defines `__NO_TRAPPING_MATH__` as GCC 14 does; Darwin defines nothing, as
//! Apple clang does.

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
            "evocc-permission-options-{}-{}",
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
fn clean(output: Output, context: &str) -> Vec<u8> {
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "{context}: {}",
        detail(&output)
    );
    output.stdout
}

/// GCC 14's macro set does not change for the first three; the trapping pair
/// changes only `__NO_TRAPPING_MATH__` (measured with `-dM -E`).
#[cfg(target_os = "macos")]
const PERMISSIONS: [&[&str]; 7] = [
    &["-fexpensive-optimizations"],
    &["-ftree-vectorize"],
    &["-fno-vect-cost-model"],
    &["-fno-trapping-math"],
    &["-ftrapping-math"],
    &["-ftrapping-math", "-fno-trapping-math"],
    &["-fno-trapping-math", "-ftrapping-math"],
];

const LOOPS: &str = "static unsigned short in[128];\n\
    static float values[64];\n\
    int main(void) {\n\
    unsigned sum = 0;\n\
    float total = 0.0f;\n\
    for (int i = 0; i < 128; i++) in[i] = (unsigned short)i;\n\
    for (int i = 0; i < 128; i++) sum += in[i] * 3u;\n\
    for (int i = 0; i < 64; i++) values[i] = (float)i * 0.5f;\n\
    for (int i = 0; i < 64; i++) total += values[i];\n\
    return sum == 24384 && total == 1008.0f ? 0 : 1;\n\
    }\n";

#[cfg(target_os = "macos")]
#[test]
fn each_permission_only_option_changes_no_code() {
    let work = Work::new();
    let source = work.source("loops.c", LOOPS);
    for target in [LINUX, DARWIN] {
        let assembly = clean(evocc(target, &["--emit=asm"], &source), target);
        let object = work.0.join("plain.o");
        let object_path = object.to_str().unwrap();
        clean(evocc(target, &["-c", "-o", object_path], &source), target);
        let expected_object = std::fs::read(&object).unwrap();
        for options in PERMISSIONS {
            let context = format!("{target} {options:?}");
            let mut arguments = vec!["--emit=asm"];
            arguments.extend_from_slice(options);
            let output = clean(evocc(target, &arguments, &source), &context);
            assert!(output == assembly, "{context}: the assembly changed");
            let optioned = work.0.join("optioned.o");
            let optioned_path = optioned.to_str().unwrap();
            let mut arguments = vec!["-c", "-o", optioned_path];
            arguments.extend_from_slice(options);
            clean(evocc(target, &arguments, &source), &context);
            assert!(
                std::fs::read(&optioned).unwrap() == expected_object,
                "{context}: the object changed"
            );
            let mut arguments = vec!["-Werror", "-fsyntax-only"];
            arguments.extend_from_slice(options);
            clean(evocc(target, &arguments, &source), &context);
        }
    }
}

#[test]
fn the_trapping_macro_follows_gcc_14_on_linux_only() {
    let work = Work::new();
    let source = work.source(
        "probe.c",
        "#ifdef __NO_TRAPPING_MATH__\nint no_trapping = __NO_TRAPPING_MATH__;\n#endif\n",
    );
    // The last of the two trapping options wins, as in GCC 14.
    let cases: [(&[&str], bool); 5] = [
        (&[], false),
        (&["-ftrapping-math"], false),
        (&["-fno-trapping-math"], true),
        (&["-ftrapping-math", "-fno-trapping-math"], true),
        (&["-fno-trapping-math", "-ftrapping-math"], false),
    ];
    for (options, gcc_defines) in cases {
        for target in [LINUX, DARWIN] {
            let context = format!("{target} {options:?}");
            let mut arguments = vec!["-E"];
            arguments.extend_from_slice(options);
            let output = clean(evocc(target, &arguments, &source), &context);
            let text = String::from_utf8(output).unwrap();
            let defined = text.contains("no_trapping");
            assert_eq!(defined, gcc_defines && target == LINUX, "{context}: {text}");
            if defined {
                assert!(text.replace(' ', "").contains("no_trapping=1;"), "{text}");
            }
        }
    }
}

#[test]
fn the_fast_math_options_keep_their_rule() {
    let work = Work::new();
    let source = work.source("loops.c", LOOPS);
    for target in [LINUX, DARWIN] {
        for (option, strict) in [
            ("-ffast-math", "unsupported floating-point profile option"),
            ("-ffinite-math-only", "unsupported option"),
            ("-funsafe-math-optimizations", "unsupported option"),
            ("-Ofast", "unsupported option"),
        ] {
            let context = format!("{target} {option}");
            let output = evocc(target, &["--emit=asm", option], &source);
            assert!(
                output.status.code() == Some(1)
                    && output.stdout.is_empty()
                    && String::from_utf8_lossy(&output.stderr)
                        .contains(&format!("{strict} `{option}`")),
                "{context}: {}",
                detail(&output)
            );
            let output = evocc(target, &["-fsyntax-only", option], &source);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if option == "-Ofast" {
                assert_eq!(output.status.code(), Some(1), "{context}: {stderr}");
                assert!(stderr.contains("unsupported option `-Ofast`"), "{stderr}");
            } else {
                assert!(output.status.success(), "{context}: {stderr}");
                assert!(
                    stderr.contains(&format!(
                        "option `{option}` is not supported and has no effect"
                    )),
                    "{context}: {stderr}"
                );
            }
        }
    }
}

/// LLVM test-suite 2919e55176a5, SingleSource/Regression/C/gcc-c-torture/
/// execute. The arguments are the original ones of the old PASS runs; the
/// ladder's LLVM projection drops only the compiler, `-o` and `-c`.
const ORIGINALS: [(&str, &[&str]); 3] = [
    (
        "pr68381.c",
        &[
            "-DNDEBUG",
            "-O3",
            "-DNDEBUG",
            "-w",
            "-Werror=date-time",
            "-Wno-implicit-int",
            "-Wno-int-conversion",
            "-Wno-implicit-function-declaration",
            "-w",
            "-O",
            "-fexpensive-optimizations",
            "-Wno-error=incompatible-pointer-types",
        ],
    ),
    (
        "pr68532.c",
        &[
            "-DNDEBUG",
            "-O3",
            "-DNDEBUG",
            "-w",
            "-Werror=date-time",
            "-Wno-implicit-int",
            "-Wno-int-conversion",
            "-Wno-implicit-function-declaration",
            "-w",
            "-O2",
            "-ftree-vectorize",
            "-fno-vect-cost-model",
            "-Wno-error=incompatible-pointer-types",
        ],
    ),
    (
        "ieee/compare-fp-3.c",
        &[
            "-DNDEBUG",
            "-O3",
            "-DNDEBUG",
            "-w",
            "-Werror=date-time",
            "-Wno-implicit-int",
            "-Wno-implicit-function-declaration",
            "-w",
            "-fno-trapping-math",
            "-Wno-error=incompatible-pointer-types",
        ],
    ),
];

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn the_three_llvm_originals_run_in_the_sealed_linux_image() {
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
    for (index, (source, arguments)) in ORIGINALS.iter().enumerate() {
        let mut options = vec!["--emit=asm"];
        options.extend_from_slice(arguments);
        let output = evocc(LINUX, &options, &tree.join(source));
        assert!(output.status.success(), "{source}: {}", detail(&output));
        std::fs::write(work.0.join(format!("{index}.s")), &output.stdout).unwrap();
        // GCC 14 with the same arguments is the reference. The link uses the
        // original link rule's -no-pie; standard input is /dev/null, as lit.
        script.push_str(&format!(
            "cd /tree && gcc-14 {} -no-pie {source} -o /tmp/reference\n\
             gcc-14 -no-pie /work/{index}.s -o /tmp/subject\n\
             /tmp/reference < /dev/null > /tmp/reference.out 2>&1\n\
             /tmp/subject < /dev/null > /tmp/subject.out 2>&1\n\
             cmp /tmp/reference.out /tmp/subject.out\n\
             echo '{source}: PASS'\n",
            arguments.join(" ")
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
    assert_eq!(stdout.matches(": PASS").count(), 3, "{stdout}");
}
