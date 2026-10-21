//! Floating complex execution (L04), as the 2026-10-13 rule decides.
//!
//! A complex value is a pair of parts, real part first. It passes as an
//! AAPCS64 HFA of its two parts. Addition, subtraction, negation,
//! conjugation, `__real__`, `__imag__`, conversions and comparisons work per
//! part with the scalar rules. Multiplication and division call the target's
//! runtime helper in native code: libgcc's `__mul?c3` and `__div?c3` on Linux,
//! compiler-rt's on Darwin. Both interpreters and constant folding share one
//! model of the Linux helpers, and they refuse Darwin multiplication and
//! division, whose compiler-rt sequence is not modeled.
//!
//! The tests marked as the sealed lane need `evo-nexlab:sealed` and run
//! explicitly and serially. The Darwin tests run on this host.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const LEVELS: [&str; 2] = ["-O0", "-O2"];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
/// The floating environment options of the probes.
const STRICT: [&str; 3] = ["-std=gnu11", "-frounding-math", "-ffp-contract=off"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures").join(name)
}

fn packet(name: &str) -> PathBuf {
    fixture("rules-20261013").join(name)
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The machine-read lines of a recorded fixture, after its `records:` line.
fn records(name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(fixture(name)).unwrap();
    let (_, body) = text.split_once("\nrecords:\n").expect("records marker");
    body.lines().map(str::to_owned).collect()
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-floating-complex-{label}-{}",
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

    fn copy(&self, source: &Path) -> PathBuf {
        let path = self.0.join(source.file_name().unwrap());
        std::fs::copy(source, &path).unwrap();
        path
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "retained failed floating-complex probe: {}",
                self.0.display()
            );
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn evocc(target: &str, args: &[&str], source: &Path) -> Output {
    evocc_with(target, &["-w"], args, source)
}

fn evocc_with(target: &str, options: &[&str], args: &[&str], source: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command
        .arg(format!("--target={target}"))
        .args(options)
        .arg("--error-format=short");
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command.args(args).arg(source).output().unwrap()
}

/// Run `script` in the sealed Linux image with `work` mounted at `/work`.
fn sealed(work: &Work, script: &str) -> Output {
    Command::new("container")
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
        .args(["evo-nexlab:sealed", "sh", "-c", script])
        .output()
        .unwrap()
}

/// Main's value as an interpreter reports it.
fn interpreted(output: &Output) -> Option<i64> {
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

/// The first diagnostic code of a failed command.
fn first_code(output: &Output) -> Option<String> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr.split(['[', ']', ' ', ':']).find_map(|word| {
        (word.len() == 5
            && word.starts_with(['E', 'W'])
            && word[1..].bytes().all(|b| b.is_ascii_digit()))
        .then(|| word.to_owned())
    })
}

fn boundary_build(line: &str) -> (String, String, String) {
    let mut words = line.splitn(3, ' ');
    let kind = words.next().unwrap().strip_prefix("KIND=").unwrap();
    let case = words.next().unwrap().strip_prefix("CASE=").unwrap();
    (
        kind.to_owned(),
        case.to_owned(),
        words.next().unwrap().to_owned(),
    )
}

/// The batch probe of floating-complex-boundaries.c runs unchanged in both
/// interpreters, and each of its 15 builds prints the recorded reference
/// line: the 24 finite controls and the 36 bit and status observations.
#[test]
fn boundary_probe_prints_the_reference_lines_in_both_interpreters() {
    let wrapper = fixture("floating-complex-boundaries-interpreted.c");
    let mut failures = Vec::new();
    for line in records("floating-complex-boundaries.out") {
        let (kind, case, expected) = boundary_build(&line);
        for interpreter in INTERPRETERS {
            for level in LEVELS {
                let mut args = STRICT.to_vec();
                let defines = [
                    format!("-DKIND={kind}"),
                    format!("-DCASE={case}"),
                    format!("-DEXPECTED=\"{expected}\""),
                ];
                args.extend(defines.iter().map(String::as_str));
                args.extend([level, interpreter]);
                let output = evocc(LINUX, &args, &wrapper);
                if interpreted(&output) != Some(0) {
                    failures.push(format!(
                        "KIND={kind} CASE={case} {interpreter} {level}: {}",
                        text(&output)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Native Linux code prints the same 15 lines at -O0 and -O2, and the 60
/// reference executions of GCC 14 and Clang 19 still print the recorded
/// lines in the sealed image.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn boundary_probe_prints_the_reference_lines_natively() {
    let work = Work::new("boundaries");
    let probe = work.copy(&fixture("floating-complex-boundaries.c"));
    let mut script = String::from("cd /work\n");
    let lines = records("floating-complex-boundaries.out");
    for line in &lines {
        let (kind, case, _) = boundary_build(line);
        for level in LEVELS {
            let assembly = work.0.join(format!("evocc-{kind}-{case}{level}.s"));
            let mut args = STRICT.to_vec();
            let defines = [format!("-DKIND={kind}"), format!("-DCASE={case}")];
            args.extend(defines.iter().map(String::as_str));
            args.extend([level, "-S", "-o", assembly.to_str().unwrap()]);
            let output = evocc(LINUX, &args, &probe);
            assert!(output.status.success(), "{line} {level}\n{}", text(&output));
            script.push_str(&format!(
                "gcc-14 evocc-{kind}-{case}{level}.s -o t -lm && echo \"evocc {level} KIND={kind} CASE={case} $(./t) exit=$?\"\n"
            ));
            for compiler in ["gcc-14", "clang-19"] {
                script.push_str(&format!(
                    "{compiler} -std=gnu11 {level} -frounding-math -ffp-contract=off -DKIND={kind} -DCASE={case} floating-complex-boundaries.c -o r -lm && echo \"{compiler} {level} KIND={kind} CASE={case} $(./r) exit=$?\"\n"
                ));
            }
        }
    }
    let output = sealed(&work, &script);
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    let mut seen = 0;
    for observed in stdout.lines() {
        let (subject, rest) = observed.split_once(' ').unwrap();
        let (level, rest) = rest.split_once(' ').unwrap();
        let rest = rest.strip_suffix(" exit=0").unwrap_or_else(|| {
            panic!("{subject} {level}: nonzero exit: {observed}");
        });
        assert!(lines.iter().any(|line| line == rest), "{observed}");
        seen += 1;
    }
    // 15 builds at two levels: 30 EvoCC and 60 reference executions.
    assert_eq!(seen, 90, "{stdout}");
}

/// The packet's operation matrix (l04-complex-ops.c): 30 builds. Main's
/// value names the first failed check. GCC folds a static inexact quotient
/// with MPC, but constant folding here shares the helper model, as Clang's
/// folding differs too: KIND=64 CASE=7 fails exactly its check 5.
fn expected_operation_status(kind: u32, case: u32) -> i64 {
    if kind == 64 && case == 7 {
        5
    } else {
        0
    }
}

#[test]
fn operation_probes_pass_in_both_interpreters() {
    let probe = packet("l04-complex-ops.c");
    let mut failures = Vec::new();
    for kind in [32, 64, 128] {
        for case in 0..=9 {
            for interpreter in INTERPRETERS {
                for level in LEVELS {
                    let defines = [format!("-DKIND={kind}"), format!("-DCASE={case}")];
                    let mut args = STRICT.to_vec();
                    args.extend(defines.iter().map(String::as_str));
                    args.extend([level, interpreter]);
                    let output = evocc(LINUX, &args, &probe);
                    if interpreted(&output) != Some(expected_operation_status(kind, case)) {
                        failures.push(format!(
                            "KIND={kind} CASE={case} {interpreter} {level}: {}",
                            text(&output)
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn operation_probes_pass_natively_on_linux() {
    let work = Work::new("operations");
    work.copy(&packet("l04-complex-ops.c"));
    let mut script = String::from("cd /work\n");
    for kind in [32, 64, 128] {
        for case in 0..=9 {
            for level in LEVELS {
                let name = format!("ops-{kind}-{case}{level}.s");
                let defines = [format!("-DKIND={kind}"), format!("-DCASE={case}")];
                let mut args = STRICT.to_vec();
                args.extend(defines.iter().map(String::as_str));
                let assembly = work.0.join(&name);
                args.extend([level, "-S", "-o", assembly.to_str().unwrap()]);
                let output = evocc(LINUX, &args, &packet("l04-complex-ops.c"));
                assert!(
                    output.status.success(),
                    "KIND={kind} CASE={case} {level}\n{}",
                    text(&output)
                );
                script.push_str(&format!(
                    "gcc-14 {name} -o t -lm && ./t; echo \"evocc {kind} {case} {level} $?\"\n\
                     gcc-14 -std=gnu11 {level} -frounding-math -ffp-contract=off -DKIND={kind} -DCASE={case} l04-complex-ops.c -o r -lm && ./r; echo \"gcc-14 {kind} {case} {level} $?\"\n"
                ));
            }
        }
    }
    let output = sealed(&work, &script);
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    let mut seen = 0;
    for line in stdout.lines() {
        let words = line.split(' ').collect::<Vec<_>>();
        let [subject, kind, case, level, status] = words[..] else {
            panic!("{line}");
        };
        let (kind, case): (u32, u32) = (kind.parse().unwrap(), case.parse().unwrap());
        let expected = if subject == "gcc-14" {
            0
        } else {
            expected_operation_status(kind, case)
        };
        assert_eq!(status, expected.to_string(), "{subject} {level}: {line}");
        seen += 1;
    }
    assert_eq!(seen, 120, "{stdout}");
}

/// The first result of a Darwin build of l04-complex-ops.c that differs
/// from Apple clang. The SDK's <complex.h> defines CMPLX only for clang, so
/// CASE 1 calls an undeclared function, which GCC 14 and EvoCC refuse
/// (W0003, an error by default). Constant folding refuses the static
/// products and quotients of CASE 7 (E2960).
fn darwin_refusal(case: u32) -> Option<&'static str> {
    match case {
        1 => Some("W0003"),
        7 => Some("E2960"),
        _ => None,
    }
}

/// The first code of a Darwin build of l04-complex-ops.c with the default
/// diagnostics, not `-w`.
fn darwin_first_code(kind: u32, case: u32, mode: &str, probe: &Path) -> Option<String> {
    let defines = [format!("-DKIND={kind}"), format!("-DCASE={case}")];
    let mut args = STRICT.to_vec();
    args.extend(defines.iter().map(String::as_str));
    args.push(mode);
    first_code(&evocc_with(DARWIN, &[], &args, probe))
}

/// Darwin: the interpreters refuse a reached multiplication or division
/// (E3067); the other cases run.
#[test]
fn darwin_interpreters_refuse_multiplication_and_division() {
    let probe = packet("l04-complex-ops.c");
    let mut failures = Vec::new();
    for kind in [32, 64, 128] {
        for case in 0..=9 {
            for interpreter in INTERPRETERS {
                let defines = [format!("-DKIND={kind}"), format!("-DCASE={case}")];
                let mut args = STRICT.to_vec();
                args.extend(defines.iter().map(String::as_str));
                args.push(interpreter);
                let output = evocc(DARWIN, &args, &probe);
                let expected = darwin_refusal(case).unwrap_or(match case {
                    4 | 5 | 6 | 8 => "E3067",
                    _ => "0",
                });
                let observed = if expected == "0" {
                    interpreted(&output).map(|value| value.to_string())
                } else if darwin_refusal(case).is_some() {
                    darwin_first_code(kind, case, interpreter, &probe)
                } else {
                    first_code(&output)
                };
                if observed.as_deref() != Some(expected) {
                    failures.push(format!(
                        "KIND={kind} CASE={case} {interpreter}: expected {expected}\n{}",
                        text(&output)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Darwin: native code calls compiler-rt and agrees with Apple clang in
/// every build except the refusals of `darwin_refusal`.
#[cfg(target_os = "macos")]
#[test]
fn darwin_native_operations_agree_with_apple_clang() {
    let work = Work::new("darwin-operations");
    let probe = packet("l04-complex-ops.c");
    let mut failures = Vec::new();
    for kind in [32, 64, 128] {
        for case in 0..=9 {
            for level in LEVELS {
                let defines = [format!("-DKIND={kind}"), format!("-DCASE={case}")];
                let clang_program = work.0.join(format!("clang-{kind}-{case}{level}"));
                let clang = Command::new("clang")
                    .args(STRICT)
                    .args(&defines)
                    .arg(level)
                    .arg(&probe)
                    .arg("-o")
                    .arg(&clang_program)
                    .output()
                    .unwrap();
                assert!(clang.status.success(), "{}", text(&clang));
                let expected = Command::new(&clang_program).output().unwrap().status.code();
                let program = work.0.join(format!("evocc-{kind}-{case}{level}"));
                let mut args = STRICT.to_vec();
                args.extend(defines.iter().map(String::as_str));
                args.extend([level, "-o", program.to_str().unwrap()]);
                if let Some(code) = darwin_refusal(case) {
                    let observed = darwin_first_code(kind, case, "-fsyntax-only", &probe);
                    if observed.as_deref() != Some(code) {
                        failures.push(format!("KIND={kind} CASE={case} {level}: {observed:?}"));
                    }
                    continue;
                }
                let output = evocc(DARWIN, &args, &probe);
                if !output.status.success() {
                    failures.push(format!(
                        "KIND={kind} CASE={case} {level}\n{}",
                        text(&output)
                    ));
                    continue;
                }
                let observed = Command::new(&program).output().unwrap().status.code();
                if observed != expected {
                    failures.push(format!(
                        "KIND={kind} CASE={case} {level}: EvoCC {observed:?}, clang {expected:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// One recorded hash line of floating-complex-grid.out: the grid (K=) or
/// its sample (S=).
struct GridRecord {
    sample: bool,
    kind: u32,
    mode: u32,
    hashes: [String; 4],
}

fn grid_records() -> Vec<GridRecord> {
    records("floating-complex-grid.out")
        .iter()
        .map(|line| {
            let words = line.split(' ').collect::<Vec<_>>();
            let (selection, kind) = words[0].split_once('=').unwrap();
            GridRecord {
                sample: selection == "S",
                kind: kind.parse().unwrap(),
                mode: words[2].parse().unwrap(),
                hashes: [words[4], words[5], words[7], words[8]].map(str::to_owned),
            }
        })
        .collect()
}

/// The `-D` options that give a grid or sample build its reference hashes.
fn grid_defines(kind: u32, sample: bool) -> Vec<String> {
    let mut defines = vec![format!("-DKIND={kind}"), "-DCHECK".to_owned()];
    if sample {
        defines.push("-DSAMPLE".to_owned());
    }
    for record in grid_records()
        .iter()
        .filter(|record| record.kind == kind && record.sample == sample)
    {
        let [mul, mul_flags, div, div_flags] = &record.hashes;
        let mode = record.mode;
        defines.push(format!("-DEXPECT_MUL_{mode}=0x{mul}ull"));
        defines.push(format!("-DFLAGS_MUL_{mode}=0x{mul_flags}ull"));
        defines.push(format!("-DEXPECT_DIV_{mode}=0x{div}ull"));
        defines.push(format!("-DFLAGS_DIV_{mode}=0x{div_flags}ull"));
    }
    defines
}

/// Both interpreters reproduce the libgcc helpers bit for bit and flag for
/// flag on the 50 operand sets of the grid's sample, in each rounding mode,
/// for float, double and binary128 long double. An interpreter keeps every
/// object that a complex operation creates, so one run takes one rounding
/// mode of the sample. The model's unit test and the native test take the
/// whole grid.
#[test]
fn special_value_sample_matches_the_libgcc_helpers_in_both_interpreters() {
    let grid = fixture("floating-complex-grid.c");
    let mut failures = Vec::new();
    for kind in [32, 64, 128] {
        let defines = grid_defines(kind, true);
        for mode in 0..4 {
            for interpreter in INTERPRETERS {
                let mode_define = format!("-DMODE={mode}");
                let mut args = STRICT.to_vec();
                args.extend(defines.iter().map(String::as_str));
                args.extend(["-DFORM=0", mode_define.as_str(), interpreter]);
                let output = evocc(LINUX, &args, &grid);
                if interpreted(&output) != Some(0) {
                    failures.push(format!(
                        "KIND={kind} MODE={mode} {interpreter}: {}",
                        text(&output)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Static initializers fold the sample's products and quotients to the bits
/// of the helpers in round-to-nearest; both interpreters read the folded
/// table.
#[test]
fn special_value_sample_folds_to_the_libgcc_helpers() {
    let grid = fixture("floating-complex-grid.c");
    let mut failures = Vec::new();
    for kind in [32, 64, 128] {
        for interpreter in INTERPRETERS {
            let defines = grid_defines(kind, true);
            let mut args = STRICT.to_vec();
            args.extend(defines.iter().map(String::as_str));
            args.extend(["-DFORM=2", interpreter]);
            let output = evocc(LINUX, &args, &grid);
            if interpreted(&output) != Some(0) {
                failures.push(format!("KIND={kind} {interpreter}: {}", text(&output)));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Native code calls the libgcc helpers at -O0 and -O2 on the whole grid
/// in the four rounding modes. A static table of the whole grid folds to the
/// helpers' bits (at -O0; folding does not depend on the level), and so does
/// the sample's table at -O2. The image's helpers still give the recorded
/// hashes.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn special_value_grid_matches_the_libgcc_helpers_natively() {
    let work = Work::new("grid");
    let grid = work.copy(&fixture("floating-complex-grid.c"));
    let mut script = String::from("set -e\ncd /work\n");
    for kind in [32, 64, 128] {
        for (form, sample, level) in [
            ("0", false, "-O0"),
            ("0", false, "-O2"),
            ("2", false, "-O0"),
            ("2", true, "-O2"),
        ] {
            let defines = grid_defines(kind, sample);
            let selection = if sample { "sample" } else { "grid" };
            let name = format!("{selection}-{kind}-{form}{level}.s");
            let assembly = work.0.join(&name);
            let mut args = STRICT.to_vec();
            args.extend(defines.iter().map(String::as_str));
            let form_define = format!("-DFORM={form}");
            let modes = if form == "0" {
                "-DMODES=4"
            } else {
                "-DMODES=1"
            };
            args.extend([
                form_define.as_str(),
                modes,
                level,
                "-S",
                "-o",
                assembly.to_str().unwrap(),
            ]);
            let output = evocc(LINUX, &args, &grid);
            assert!(
                output.status.success(),
                "KIND={kind} FORM={form} {selection} {level}\n{}",
                text(&output)
            );
            script.push_str(&format!(
                "gcc-14 {name} -o t -lm\nif ./t; then echo \"evocc {kind} {form} {selection} {level} ok\"; else echo \"evocc {kind} {form} {selection} {level} mask $?\"; fi\n"
            ));
        }
        script.push_str(&format!(
            "gcc-14 -std=gnu11 -O0 -frounding-math -ffp-contract=off -DKIND={kind} -DFORM=1 -DMODES=4 floating-complex-grid.c -o h -lm\n./h | sed 's/^/K={kind} /'\n"
        ));
    }
    let output = sealed(&work, &script);
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    assert!(output.status.success(), "{}", text(&output));
    let recorded = records("floating-complex-grid.out");
    for line in stdout.lines() {
        if line.starts_with("evocc ") {
            assert!(line.ends_with(" ok"), "{line}");
        } else {
            assert!(recorded.iter().any(|record| record == line), "{line}");
        }
    }
    assert_eq!(stdout.lines().count(), 3 * 4 + 3 * 4, "{stdout}");
}

/// Darwin: native multiplication and division agree with Apple clang's
/// operators bit for bit and flag for flag on the whole grid, at -O0 and
/// -O2, and a static table is refused (E2960).
#[cfg(target_os = "macos")]
#[test]
fn darwin_grid_agrees_with_apple_clang() {
    let work = Work::new("darwin-grid");
    let grid = fixture("floating-complex-grid.c");
    for kind in [32, 64, 128] {
        let defines = [
            format!("-DKIND={kind}"),
            "-DFORM=0".into(),
            "-DMODES=4".into(),
        ];
        let reference = work.0.join(format!("clang-{kind}"));
        let clang = Command::new("clang")
            .args(STRICT)
            .args(&defines)
            .arg("-O2")
            .arg(&grid)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        assert!(clang.status.success(), "{}", text(&clang));
        let expected = Command::new(&reference).output().unwrap();
        assert!(expected.status.success(), "{}", text(&expected));
        for level in LEVELS {
            let program = work.0.join(format!("evocc-{kind}{level}"));
            let mut args = STRICT.to_vec();
            args.extend(defines.iter().map(String::as_str));
            args.extend([level, "-o", program.to_str().unwrap()]);
            let output = evocc(DARWIN, &args, &grid);
            assert!(
                output.status.success(),
                "KIND={kind} {level}\n{}",
                text(&output)
            );
            let observed = Command::new(&program).output().unwrap();
            assert_eq!(
                String::from_utf8_lossy(&observed.stdout),
                String::from_utf8_lossy(&expected.stdout),
                "KIND={kind} {level}"
            );
        }
        let mut args = STRICT.to_vec();
        let kind_define = format!("-DKIND={kind}");
        args.extend([
            kind_define.as_str(),
            "-DFORM=2",
            "-DSAMPLE",
            "-fsyntax-only",
        ]);
        let output = evocc(DARWIN, &args, &grid);
        assert_eq!(
            first_code(&output).as_deref(),
            Some("E2960"),
            "{}",
            text(&output)
        );
    }
}

/// Crossed calls between EvoCC and GCC 14, in both directions, at -O0 and
/// -O2: every complex type as argument, result, variadic argument and
/// record member.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn crossed_calls_agree_with_gcc_14() {
    let work = Work::new("crossed-linux");
    for name in [
        "floating-complex-abi.h",
        "floating-complex-abi-worker.c",
        "floating-complex-abi-driver.c",
    ] {
        work.copy(&fixture(name));
    }
    let mut script = String::from("set -e\ncd /work\n");
    for level in LEVELS {
        for side in ["worker", "driver"] {
            let assembly = work.0.join(format!("{side}{level}.s"));
            let output = evocc(
                LINUX,
                &["-std=gnu11", level, "-S", "-o", assembly.to_str().unwrap()],
                &work.0.join(format!("floating-complex-abi-{side}.c")),
            );
            assert!(output.status.success(), "{side} {level}\n{}", text(&output));
            script.push_str(&format!(
                "gcc-14 -c {side}{level}.s -o evocc-{side}{level}.o\n\
                 gcc-14 -std=gnu11 {level} -c floating-complex-abi-{side}.c -o gcc-{side}{level}.o\n"
            ));
        }
        for (worker, driver) in [("evocc", "gcc"), ("gcc", "evocc"), ("evocc", "evocc")] {
            script.push_str(&format!(
                "gcc-14 {driver}-driver{level}.o {worker}-worker{level}.o -o t\n\
                 if ./t; then echo \"worker={worker} driver={driver} {level} 0\"; else echo \"worker={worker} driver={driver} {level} $?\"; fi\n"
            ));
        }
    }
    let output = sealed(&work, &script);
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(stdout.lines().count(), 6, "{stdout}");
    assert!(stdout.lines().all(|line| line.ends_with(" 0")), "{stdout}");
}

#[cfg(target_os = "macos")]
#[test]
fn crossed_calls_agree_with_apple_clang() {
    let work = Work::new("crossed-darwin");
    for name in [
        "floating-complex-abi.h",
        "floating-complex-abi-worker.c",
        "floating-complex-abi-driver.c",
    ] {
        work.copy(&fixture(name));
    }
    for level in LEVELS {
        for side in ["worker", "driver"] {
            let source = work.0.join(format!("floating-complex-abi-{side}.c"));
            let object = work.0.join(format!("evocc-{side}{level}.o"));
            let output = evocc(
                DARWIN,
                &["-std=gnu11", level, "-c", "-o", object.to_str().unwrap()],
                &source,
            );
            assert!(output.status.success(), "{side} {level}\n{}", text(&output));
            let clang = Command::new("clang")
                .args(["-std=gnu11", level, "-c"])
                .arg(&source)
                .arg("-o")
                .arg(work.0.join(format!("clang-{side}{level}.o")))
                .output()
                .unwrap();
            assert!(clang.status.success(), "{}", text(&clang));
        }
        for (worker, driver) in [("evocc", "clang"), ("clang", "evocc"), ("evocc", "evocc")] {
            let program = work.0.join(format!("{worker}-{driver}{level}"));
            let link = Command::new("clang")
                .arg(work.0.join(format!("{driver}-driver{level}.o")))
                .arg(work.0.join(format!("{worker}-worker{level}.o")))
                .arg("-o")
                .arg(&program)
                .output()
                .unwrap();
            assert!(link.status.success(), "{}", text(&link));
            let run = Command::new(&program).output().unwrap();
            assert_eq!(
                run.status.code(),
                Some(0),
                "worker={worker} driver={driver} {level}"
            );
        }
    }
}

const HEADERS: &str = "#include <complex.h>\n#include <tgmath.h>\n\
    int main(void) { double _Complex z = CMPLX(1.0, 2.0) * I; return creal(z) != -2.0; }\n";

/// glibc's <complex.h> and <tgmath.h> compile with and without _GNU_SOURCE,
/// which declares the _FloatN and _FloatNx complex functions.
#[test]
fn linux_complex_and_tgmath_headers_compile() {
    let work = Work::new("headers");
    let plain = work.source("plain.c", HEADERS);
    let gnu = work.source("gnu.c", &format!("#define _GNU_SOURCE\n{HEADERS}"));
    let tgmath = fixture("floating-complex-tgmath.c");
    for source in [&plain, &gnu, &tgmath] {
        for extra in [&[][..], &["-D_GNU_SOURCE"][..]] {
            for level in LEVELS {
                let mut args = vec!["-std=gnu11", level, "-S", "-o", "/dev/null"];
                args.extend_from_slice(extra);
                let output = evocc(LINUX, &args, source);
                assert!(
                    output.status.success(),
                    "{} {extra:?} {level}\n{}",
                    source.display(),
                    text(&output)
                );
            }
        }
    }
}

/// Complex <tgmath.h> calls select the functions that GCC 14 selects, with
/// and without _GNU_SOURCE, and print GCC 14's 38 lines in the sealed image.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn complex_tgmath_calls_agree_with_gcc_14() {
    let work = Work::new("tgmath");
    let source = work.copy(&fixture("floating-complex-tgmath.c"));
    let mut script = String::from("set -eu\ncd /work\n");
    for (label, extra) in [("plain", &[][..]), ("gnu", &["-D_GNU_SOURCE"][..])] {
        for level in LEVELS {
            let assembly = work.0.join(format!("tgmath-{label}{level}.s"));
            let mut args = vec!["-std=gnu11", level, "-S", "-o", assembly.to_str().unwrap()];
            args.extend_from_slice(extra);
            let output = evocc(LINUX, &args, &source);
            assert!(
                output.status.success(),
                "{label} {level}\n{}",
                text(&output)
            );
            let define = extra.first().copied().unwrap_or("");
            script.push_str(&format!(
                "gcc-14 -std=gnu11 -w {level} {define} floating-complex-tgmath.c -lm -o gcc-{label}{level}\n\
                 gcc-14 tgmath-{label}{level}.s -lm -o evocc-{label}{level}\n\
                 ./gcc-{label}{level} > gcc-{label}{level}.txt\n\
                 ./evocc-{label}{level} > evocc-{label}{level}.txt\n\
                 cmp gcc-{label}{level}.txt evocc-{label}{level}.txt\n\
                 echo \"{label} {level} identical $(wc -l < gcc-{label}{level}.txt) lines\"\n"
            ));
        }
    }
    let output = sealed(&work, &script);
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    assert!(output.status.success(), "{}", text(&output));
    for label in ["plain", "gnu"] {
        for level in LEVELS {
            assert!(
                stdout.contains(&format!("{label} {level} identical 38 lines")),
                "{stdout}"
            );
        }
    }
}

/// Two identical divisions around a rounding change give two results: a
/// strict division is never merged or hoisted across a world change.
const ROUNDING_CHANGE: &str = r#"#include <fenv.h>
static volatile double _Complex a = 1.0 + 1.0i, b = 3.0;
int main(void)
{
    fesetround(FE_UPWARD);
    double _Complex up = a / b;
    fesetround(FE_TONEAREST);
    double _Complex near = a / b;
    return __real__ up == __real__ near || __real__ up != 0x1.5555555555556p-2
        || __real__ near != 0x1.5555555555555p-2;
}
"#;

#[test]
fn divisions_around_a_rounding_change_give_two_results() {
    let work = Work::new("rounding");
    let source = work.source("rounding.c", ROUNDING_CHANGE);
    for interpreter in INTERPRETERS {
        for level in LEVELS {
            let mut args = STRICT.to_vec();
            args.extend([level, interpreter]);
            let output = evocc(LINUX, &args, &source);
            assert_eq!(
                interpreted(&output),
                Some(0),
                "{interpreter} {level}\n{}",
                text(&output)
            );
        }
    }
}

/// Forms that stay refused keep a named diagnostic, on both targets unless
/// noted. Integer complex keeps its own rules.
#[test]
fn refused_forms_keep_named_diagnostics() {
    let work = Work::new("refusals");
    let cases: &[(&str, &str, &str, &[&str])] = &[
        ("imaginary", "double _Imaginary x;", "E1001", &[]),
        ("half-storage", "__fp16 _Complex x;", "E1007", &[]),
        ("bfloat", "__bf16 _Complex x;", "E1007", &[]),
        (
            "mismatched-construction",
            "double _Complex f(void) { return __builtin_complex(1.0f, 2.0); }",
            "E2951",
            &[],
        ),
        (
            "integer-construction",
            "double _Complex f(void) { return __builtin_complex(1, 2); }",
            "E2951",
            &[],
        ),
        (
            "construction-arity",
            "double _Complex f(void) { return __builtin_complex(1.0); }",
            "E2909",
            &[],
        ),
        (
            "ordered-comparison",
            "int f(double _Complex a, double _Complex b) { return a < b; }",
            "E2102",
            &[],
        ),
        // A value of an interchange complex type is refused as a value of
        // the real interchange type is.
        (
            "interchange-value",
            "_Float32 _Complex z; void f(void) { z = 0; }",
            "E2102",
            &[],
        ),
        (
            "half-value",
            "_Float16 _Complex z; void f(void) { z = 0; }",
            "E2102",
            &[],
        ),
        (
            "real-interchange-value",
            "_Float32 z; void f(void) { z = 0; }",
            "E2102",
            &[],
        ),
    ];
    for (name, body, code, extra) in cases {
        let source = work.source(
            &format!("{name}.c"),
            &format!("{body}\nint main(void) {{ return 0; }}\n"),
        );
        for target in [LINUX, DARWIN] {
            let mut args = vec!["-std=gnu11", "-fsyntax-only"];
            args.extend_from_slice(extra);
            let output = evocc(target, &args, &source);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{name} {target}\n{}",
                text(&output)
            );
            assert_eq!(
                first_code(&output).as_deref(),
                Some(*code),
                "{name} {target}\n{}",
                text(&output)
            );
        }
    }
    // The limited-range and fast options stay refused in the checked modes.
    let plain = work.source("plain.c", "int main(void) { return 0; }\n");
    for option in [
        "-fcx-limited-range",
        "-fcx-fortran-rules",
        "-fcomplex-arithmetic=basic",
        "-ffast-math",
    ] {
        let output = evocc(LINUX, &[option, "--interpret-c-hir"], &plain);
        assert_eq!(output.status.code(), Some(1), "{option}\n{}", text(&output));
    }
}

/// `_FloatN _Complex` and `_FloatNx _Complex` are the types that GCC 14
/// admits on aarch64 Linux, with its layouts; their values stay refused.
/// Expression forms that carry a complex value: GNU increments whose value
/// is used, conditionals, comma, `_Generic`, indirect calls, compound
/// literals, record members, array elements and conversions. `main`
/// returns the number of wrong results.
const EXPRESSION_FORMS: &str = r#"typedef double _Complex Z;
static Z id(Z z) { return z; }
static Z (*fp)(Z) = id;
static int count;
static Z bump(void) { count++; return 1.0 + 1.0i; }
struct S { int tag; Z z; float _Complex f; };
int main(void) {
    int wrong = 0;
    volatile int one = 1;
    Z z = 3 + 4i, w = 0;
    w = z++;              wrong += !(__real__ w == 3 && __real__ z == 4);
    w = ++z;              wrong += !(__real__ w == 5 && __real__ z == 5);
    w = z--;              wrong += !(__real__ w == 5 && __real__ z == 4);
    wrong += (int)(z++ + 0.5) != 4;
    w = one ? z : w;      wrong += !(w == z);
    w = (count++, z);     wrong += !(w == z && count == 1);
    w = _Generic(z, Z: z + 1, default: 0); wrong += !(w == 6 + 4i);
    w = fp(z);            wrong += !(w == z);
    w = (Z){2.0};         wrong += !(w == 2);
    w = bump() * bump();  wrong += !(w == 2i && count == 3);
    struct S s = { 1, 1 + 2i, 3 - 1.0if };
    struct S t = s;       wrong += !(t.z == 1 + 2i && t.f == 3 - 1.0if);
    struct S *p = &t;     p->z *= p->f;  wrong += !(p->z == 5 + 5i);
    Z a[3] = { 1, 2i, 3 };
    a[1] += a[2];         wrong += !(a[1] == 3 + 2i);
    Z *q = &a[0];         *q = q[1] / q[2]; wrong += !(a[0] == 1 + 0x1.5555555555555p-1i);
    const Z c = 7 - 1i;   wrong += !(__real__ c == 7 && __imag__ c == -1);
    double d = 3;         d /= 1 + 2i; wrong += d != 0.6;
    float _Complex f = z; wrong += !(f == 5 + 4.0if);
    long double _Complex l = f; wrong += !(l == 5 + 4.0il);
    double r = z;         wrong += r != 5;
    return wrong;
}
"#;

/// The expression forms run in both interpreters on Linux and natively on
/// Darwin; Apple clang gives the same result.
#[test]
fn complex_expression_forms_run_in_both_interpreters() {
    let work = Work::new("forms");
    let source = work.source("forms.c", EXPRESSION_FORMS);
    for interpreter in INTERPRETERS {
        let output = evocc(LINUX, &["-std=gnu11", interpreter], &source);
        assert_eq!(
            interpreted(&output),
            Some(0),
            "{interpreter}\n{}",
            text(&output)
        );
    }
    #[cfg(target_os = "macos")]
    for level in LEVELS {
        let program = work.0.join(format!("forms{level}"));
        let output = evocc(
            DARWIN,
            &["-std=gnu11", level, "-o", program.to_str().unwrap()],
            &source,
        );
        assert!(output.status.success(), "{level}\n{}", text(&output));
        let status = Command::new(&program).status().unwrap();
        assert_eq!(status.code(), Some(0), "{level}");
    }
}

const INTERCHANGE_TYPES: &str = r#"
_Static_assert(sizeof(_Float16 _Complex) == 4 && _Alignof(_Float16 _Complex) == 2, "16");
_Static_assert(sizeof(_Float32 _Complex) == 8 && _Alignof(_Float32 _Complex) == 4, "32");
_Static_assert(sizeof(_Complex _Float64) == 16 && _Alignof(_Complex _Float64) == 8, "64");
_Static_assert(sizeof(_Float128 _Complex) == 32 && _Alignof(_Float128 _Complex) == 16, "128");
_Static_assert(sizeof(_Float32x _Complex) == 16 && _Alignof(_Float32x _Complex) == 8, "32x");
_Static_assert(sizeof(_Float64x _Complex) == 32 && _Alignof(_Float64x _Complex) == 16, "64x");
_Static_assert(_Generic((_Float32 _Complex *)0, float _Complex *: 0, _Float32 _Complex *: 1), "distinct");
_Static_assert(_Generic((_Complex *)0, double _Complex *: 1, default: 0), "plain _Complex");
_Float32 _Complex cacosf32(_Float32 _Complex z);
_Float64x _Complex cpowf64x(_Float64x _Complex x, _Float64x _Complex y);
int main(void) { return 0; }
"#;

#[test]
fn interchange_complex_types_have_the_gcc_layouts() {
    let work = Work::new("interchange");
    let source = work.source("interchange.c", INTERCHANGE_TYPES);
    for interpreter in INTERPRETERS {
        let output = evocc(LINUX, &[interpreter], &source);
        assert_eq!(
            interpreted(&output),
            Some(0),
            "{interpreter}\n{}",
            text(&output)
        );
    }
    let output = evocc(LINUX, &["-S", "-o", "/dev/null"], &source);
    assert!(output.status.success(), "{}", text(&output));
}
