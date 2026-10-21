//! Floating vector execution (L05), as the 2026-10-13 rule decides.
//!
//! GNU `float` and `double` vectors of 8 and 16 bytes are values; a wider
//! vector is an object. Each lane operation is the scalar operation of
//! EvoIR 3.13 on that lane, with no implicit contraction, and a comparison
//! gives a mask of signed integer lanes. A conversion is the C cast of each
//! lane, so an out-of-range lane is `E3063`. Shuffles, lane reads and
//! writes, copies and equal-size casts move bits. A short vector passes in
//! a SIMD register, and a record of one to four short vectors of one size
//! is an AAPCS64 HVA, whatever its lane types. `v++` and `&v[i]` follow
//! GCC. Floating vectors are refused under `-mgeneral-regs-only` (`E4002`),
//! a 4-byte floating vector at a call boundary stays `E4002`, and the
//! floating `arm_neon.h` types and builtins keep their refusals (L05b).
//!
//! The tests marked as the sealed lane need `evo-nexlab:sealed` and run
//! explicitly and serially. The Darwin tests run on this host.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
const LEVELS: [&str; 2] = ["-O0", "-O2"];
const INTERPRETERS: [&str; 2] = ["--interpret-c-hir", "--interpret-evoir"];
/// The options of the packet's flag probe and of the strict grid.
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
            "evocc-floating-vectors-{label}-{}",
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
                "retained failed floating-vector probe: {}",
                self.0.display()
            );
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn evocc(target: &str, args: &[&str], source: &Path) -> Output {
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

/// Run `job` on each item, on a few threads: the interpreters are slow in a
/// debug build, and each run is independent.
fn parallel<T: Sync, R: Send>(items: &[T], job: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(item) = items.get(index) else {
                    break;
                };
                let result = job(item);
                results.lock().unwrap().push((index, result));
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    /// Main returns this value.
    Status(i64),
    /// The front end refuses the source with this code.
    Refused(&'static str),
    /// Execution stops with this code.
    Stops(&'static str),
}

struct Probe {
    file: PathBuf,
    name: String,
    defines: Vec<String>,
    strict: bool,
    expect: Expect,
}

impl Probe {
    fn new(file: &str, define: &str, value: u32, expect: Expect) -> Self {
        let path = if file.starts_with("l05-") {
            packet(file)
        } else {
            fixture(file)
        };
        Self {
            file: path,
            name: format!("{file} {define}={value}"),
            defines: vec![format!("-D{define}={value}")],
            strict: false,
            expect,
        }
    }

    fn args<'a>(&'a self, extra: &[&'a str]) -> Vec<&'a str> {
        let mut args = if self.strict {
            STRICT.to_vec()
        } else {
            vec!["-std=gnu11"]
        };
        args.extend(self.defines.iter().map(String::as_str));
        args.extend(extra);
        args
    }
}

/// The table F values of the packet's flag probe (INVALID=1, DIVBYZERO=2,
/// OVERFLOW=4, UNDERFLOW=8, INEXACT=16) under GCC 14, Clang 19 and Apple
/// clang. Case 7 converts a NaN lane, which C leaves undefined.
const FLAG_STATUS: [Option<i64>; 12] = [
    Some(19),
    Some(20),
    Some(1),
    Some(0),
    Some(1),
    Some(0),
    Some(24),
    None,
    Some(2),
    Some(0),
    Some(0),
    Some(1),
];

/// The packet's lane identity probe, every kind. The interpreters run it in
/// slices (`lane_identity_holds_in_both_interpreters`).
fn lane_identity_probes() -> Vec<Probe> {
    [32, 64, 322, 328]
        .into_iter()
        .map(|kind| Probe::new("l05-lane-identity.c", "KIND", kind, Expect::Status(0)))
        .collect()
}

/// The packet's other probes (Section 5.3) and what EvoCC gives for each.
fn probes() -> Vec<Probe> {
    use Expect::*;
    let mut probes = Vec::new();
    for case in 0..=6 {
        probes.push(Probe::new("l05-convert.c", "CASE", case, Status(0)));
    }
    probes.push(Probe::new("l05-convert.c", "CASE", 7, Stops("E3063")));
    for case in 0..=3 {
        probes.push(Probe::new("l05-shuffle.c", "CASE", case, Status(0)));
        probes.push(Probe::new(
            "floating-vector-boundaries.c",
            "CASE",
            case,
            Status(0),
        ));
    }
    // No implicit contraction, whatever the option.
    for case in 0..=2 {
        probes.push(Probe::new("l05-contract.c", "CASE", case, Status(0)));
        let mut off = Probe::new("l05-contract.c", "CASE", case, Status(0));
        off.defines.push("-ffp-contract=off".into());
        off.name.push_str(" -ffp-contract=off");
        probes.push(off);
    }
    for (case, status) in FLAG_STATUS.iter().enumerate() {
        let mut probe = Probe::new(
            "l05-flags.c",
            "CASE",
            case as u32,
            status.map_or(Stops("E3063"), Status),
        );
        probe.strict = true;
        probes.push(probe);
    }
    for case in 0..=7 {
        probes.push(Probe::new(
            "l05-storage-boundary.c",
            "CASE",
            case,
            Status(0),
        ));
    }
    // Table D: the scalar operands that GCC 14, Clang 19 and Apple clang
    // broadcast, an exact long double constant included.
    for case in 0..=13 {
        let expect = if [1, 2, 3, 4, 7, 9, 12].contains(&case) {
            Refused("E2102")
        } else {
            Status(0)
        };
        probes.push(Probe::new("l05-broadcast.c", "CASE", case, expect));
    }
    // `++`, `--` and `&v[i]` as GCC 14 has them, a conditional of vectors,
    // a static vector, a shuffle with an unspecified lane and the mask lane
    // type of each target's reference compiler.
    probes.push(Probe::new("floating-vector-forms.c", "FORMS", 1, Status(0)));
    // Table J. `v++` and `&v[1]` follow GCC; `!v` is refused as GCC and
    // Clang refuse it in C; long double lanes keep their refusal.
    for (case, expect) in [
        (0, Refused("E2102")),
        (1, Refused("E2102")),
        (2, Refused("E2102")),
        (3, Refused("E2102")),
        (4, Refused("E2102")),
        (5, Status(0)),
        (6, Status(0)),
        (7, Refused("E2202")),
        (9, Refused("E2102")),
        (10, Refused("E2102")),
        (11, Status(0)),
        (12, Status(0)),
    ] {
        probes.push(Probe::new("l05-operators.c", "CASE", case, expect));
    }
    probes
}

/// The observed result of one interpreter run, in the terms of `Expect`.
fn observed(output: &Output, expect: Expect) -> bool {
    match expect {
        Expect::Status(status) => interpreted(output) == Some(status),
        Expect::Refused(code) | Expect::Stops(code) => {
            !output.status.success() && first_code(output).as_deref() == Some(code)
        }
    }
}

fn interpreter_failures(target: &str) -> Vec<String> {
    let probes = probes();
    let mut runs = Vec::new();
    for probe in &probes {
        for interpreter in INTERPRETERS {
            for level in LEVELS {
                runs.push((probe, interpreter, level));
            }
        }
    }
    parallel(&runs, |(probe, interpreter, level)| {
        let output = evocc(target, &probe.args(&[level, interpreter]), &probe.file);
        (!observed(&output, probe.expect)).then(|| {
            format!(
                "{target} {} {interpreter} {level}: expected {:?}\n{}",
                probe.name,
                probe.expect,
                text(&output)
            )
        })
    })
    .into_iter()
    .flatten()
    .collect()
}

#[test]
fn packet_probes_pass_in_both_linux_interpreters() {
    let failures = interpreter_failures(LINUX);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn packet_probes_pass_in_both_darwin_interpreters() {
    let failures = interpreter_failures(DARWIN);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The slices of the lane identity probe that one interpreter run takes:
/// (kind, first row, last row, first column, last column). The EvoIR
/// interpreter takes every operand pair; the C-HIR interpreter, which is
/// slower, takes row 11, whose lanes hold infinities and NaNs. Each slice
/// fits the interpreters' budget of 100,000 steps with room to spare.
fn lane_identity_slices(interpreter: &str) -> Vec<(u32, u32, u32, u32, u32)> {
    let mut slices = Vec::new();
    if interpreter == "--interpret-evoir" {
        for row in 0..13 {
            slices.push((32, row, row + 1, 0, 13));
            slices.push((328, row, row + 1, 0, 7));
            slices.push((328, row, row + 1, 7, 13));
        }
        for row in (0..13).step_by(2) {
            slices.push((64, row, row + 2, 0, 13));
            slices.push((322, row, row + 2, 0, 13));
        }
    } else {
        for kind in [32, 64, 322] {
            slices.push((kind, 11, 12, 0, 13));
        }
        slices.push((328, 11, 12, 0, 7));
        slices.push((328, 11, 12, 7, 13));
    }
    slices
}

/// Every lane of l05-lane-identity.c equals the scalar operation in both
/// interpreters, on both targets: the probe's own functions, run in slices
/// by floating-vector-lane-identity-interpreted.c. The C-HIR interpreter
/// ignores the optimization level, so it runs at -O0 alone.
#[test]
fn lane_identity_holds_in_both_interpreters() {
    let wrapper = fixture("floating-vector-lane-identity-interpreted.c");
    let mut runs = Vec::new();
    for target in TARGETS {
        for interpreter in INTERPRETERS {
            let levels: &[&str] = if interpreter == "--interpret-evoir" {
                &LEVELS
            } else {
                &["-O0"]
            };
            for level in levels {
                for slice in lane_identity_slices(interpreter) {
                    runs.push((target, interpreter, *level, slice));
                }
            }
        }
    }
    let failures = parallel(&runs, |(target, interpreter, level, slice)| {
        let (kind, first, last, column_first, column_last) = slice;
        let defines = [
            format!("-DKIND={kind}"),
            format!("-DFIRST={first}"),
            format!("-DLAST={last}"),
            format!("-DCOLUMN_FIRST={column_first}"),
            format!("-DCOLUMN_LAST={column_last}"),
        ];
        let mut args = vec!["-std=gnu11", level, interpreter];
        args.extend(defines.iter().map(String::as_str));
        let output = evocc(target, &args, &wrapper);
        (interpreted(&output) != Some(0)).then(|| {
            format!(
                "{target} {interpreter} {level} {defines:?}: {}",
                text(&output)
            )
        })
    });
    let failures = failures.into_iter().flatten().collect::<Vec<_>>();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A native run that stops at a runtime obligation traps with `brk`. The
/// immediate is the diagnostic number; the process ends with SIGTRAP.
fn native_expectation(expect: Expect) -> Option<i64> {
    match expect {
        Expect::Status(status) => Some(status),
        Expect::Stops(_) => Some(128 + 5),
        Expect::Refused(_) => None,
    }
}

/// Linux native code gives every probe's value at -O0 and -O2, and GCC 14
/// with -ffp-contract=off gives the same value for each probe that both
/// accept, except table J's accepted-by-GCC forms that EvoCC refuses.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn packet_probes_pass_natively_on_linux() {
    let work = Work::new("native-linux");
    let mut script = String::from("cd /work\n");
    let mut expected = Vec::new();
    let mut all = lane_identity_probes();
    all.extend(probes());
    for (index, probe) in all.iter().enumerate() {
        let Some(status) = native_expectation(probe.expect) else {
            let output = evocc(LINUX, &probe.args(&["-fsyntax-only"]), &probe.file);
            assert_eq!(
                first_code(&output).as_deref(),
                match probe.expect {
                    Expect::Refused(code) => Some(code),
                    _ => unreachable!(),
                },
                "{}\n{}",
                probe.name,
                text(&output)
            );
            continue;
        };
        let source = work.copy(&probe.file);
        let name = source.file_name().unwrap().to_str().unwrap().to_owned();
        for level in LEVELS {
            let assembly = work.0.join(format!("p{index}{level}.s"));
            let output = evocc(
                LINUX,
                &probe.args(&[level, "-S", "-o", assembly.to_str().unwrap()]),
                &probe.file,
            );
            assert!(
                output.status.success(),
                "{} {level}\n{}",
                probe.name,
                text(&output)
            );
            script.push_str(&format!(
                "gcc-14 p{index}{level}.s -o t -lm && {{ ./t; echo \"evocc {index} {level} $?\"; }}\n"
            ));
            expected.push((format!("evocc {index} {level}"), status, probe.name.clone()));
            if let Expect::Status(status) = probe.expect {
                let defines = probe.defines.join(" ");
                let strict = if probe.strict { "-frounding-math" } else { "" };
                script.push_str(&format!(
                    "gcc-14 -std=gnu11 {level} -w -ffp-contract=off {strict} {defines} {name} -o r -lm && {{ ./r; echo \"gcc-14 {index} {level} $?\"; }}\n"
                ));
                // Packet table A: at -O2 GCC 14 commutes the 32-byte vector
                // addition, so eight lanes with two quiet NaN operands carry
                // the second payload. IEEE 754 allows it; the scalar rule
                // does not.
                let status = if probe.name == "l05-lane-identity.c KIND=328" && level == "-O2" {
                    8
                } else {
                    status
                };
                expected.push((
                    format!("gcc-14 {index} {level}"),
                    status,
                    probe.name.clone(),
                ));
            }
        }
    }
    let output = sealed(&work, &script);
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    let mut failures = Vec::new();
    for (key, status, name) in &expected {
        let line = stdout
            .lines()
            .find(|line| line.rsplit_once(' ').is_some_and(|(head, _)| head == key));
        match line.and_then(|line| line.rsplit_once(' ')) {
            Some((_, value)) if value == status.to_string() => {}
            other => failures.push(format!("{key} ({name}): expected {status}, got {other:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Darwin native code gives every probe's value at -O0 and -O2, and so does
/// Apple clang with -ffp-contract=off for each probe that both accept.
#[cfg(target_os = "macos")]
#[test]
fn packet_probes_agree_with_apple_clang_natively() {
    let work = Work::new("native-darwin");
    let mut all = lane_identity_probes();
    all.extend(probes());
    let mut runs = Vec::new();
    for (index, probe) in all.iter().enumerate() {
        if let Some(status) = native_expectation(probe.expect) {
            for level in LEVELS {
                runs.push((index, probe, status, level));
            }
        }
    }
    let failures = parallel(&runs, |(index, probe, status, level)| {
        let mut failures = Vec::new();
        let program = work.0.join(format!("p{index}{level}"));
        let output = evocc(
            DARWIN,
            &probe.args(&[level, "-o", program.to_str().unwrap()]),
            &probe.file,
        );
        if !output.status.success() {
            return vec![format!("{} {level}\n{}", probe.name, text(&output))];
        }
        let run = Command::new(&program).output().unwrap();
        let code = run.status.code().map(i64::from).or_else(|| {
            use std::os::unix::process::ExitStatusExt;
            run.status.signal().map(|signal| 128 + i64::from(signal))
        });
        if code != Some(*status) {
            failures.push(format!(
                "{} {level}: EvoCC {code:?}, expected {status}",
                probe.name
            ));
        }
        if let Expect::Status(status) = probe.expect {
            let reference = work.0.join(format!("r{index}{level}"));
            let mut clang = Command::new("clang");
            clang.args(["-std=gnu11", "-w", "-ffp-contract=off", level]);
            if probe.strict {
                clang.arg("-frounding-math");
            }
            let built = clang
                .args(&probe.defines)
                .arg(&probe.file)
                .arg("-o")
                .arg(&reference)
                .output()
                .unwrap();
            // Apple clang refuses the GNU-only forms of the packet:
            // `__builtin_shuffle`, `v++` and `&v[1]`.
            if built.status.success() {
                let observed = Command::new(&reference).output().unwrap().status.code();
                if observed.map(i64::from) != Some(status) {
                    failures.push(format!(
                        "{} {level}: Apple clang {observed:?}, expected {status}",
                        probe.name
                    ));
                }
            }
        }
        failures
    });
    let failures = failures.into_iter().flatten().collect::<Vec<_>>();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// One recorded line of floating-vector-lanes.out.
struct GridLine {
    sample: bool,
    strict: bool,
    kind: u32,
    mode: u32,
    name: String,
    line: String,
    hash: String,
}

fn grid_lines() -> Vec<GridLine> {
    records("floating-vector-lanes.out")
        .into_iter()
        .map(|record| {
            let (form, line) = record.split_once(' ').unwrap();
            let words = line.split(' ').collect::<Vec<_>>();
            GridLine {
                sample: form.starts_with("sample-"),
                strict: form.ends_with("strict"),
                kind: words[0].strip_prefix("KIND=").unwrap().parse().unwrap(),
                mode: words[1].strip_prefix("MODE=").unwrap().parse().unwrap(),
                name: words[2].to_owned(),
                hash: words[3].to_owned(),
                line: line.to_owned(),
            }
        })
        .collect()
}

/// The builds of the grid: (strict, kind, mode).
fn grid_builds() -> Vec<(bool, u32, u32)> {
    let mut builds = Vec::new();
    for kind in [32, 64, 322] {
        builds.push((false, kind, 0));
        for mode in 0..4 {
            builds.push((true, kind, mode));
        }
    }
    builds
}

fn grid_args(strict: bool, kind: u32, mode: u32) -> Vec<String> {
    let mut args = if strict {
        STRICT.iter().map(|arg| (*arg).to_owned()).collect()
    } else {
        vec!["-std=gnu11".to_owned()]
    };
    args.push(format!("-DKIND={kind}"));
    if strict {
        args.push("-DFLAGS".into());
        args.push(format!("-DMODE={mode}"));
    }
    args
}

fn grid_expected(sample: bool, strict: bool, kind: u32, mode: u32) -> Vec<GridLine> {
    let lines = grid_lines()
        .into_iter()
        .filter(|line| {
            line.sample == sample && line.strict == strict && line.kind == kind && line.mode == mode
        })
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 20, "KIND={kind} MODE={mode}");
    lines
}

/// The interpreter runs of one build of the sample: the part, the
/// operation and the names of the lines that the run checks.
fn sample_runs() -> Vec<(u32, Option<u32>, Vec<&'static str>)> {
    let mut runs = Vec::new();
    for (op, name) in ["add", "sub", "mul", "div", "neg", "plus", "shuffle"]
        .into_iter()
        .enumerate()
    {
        // nan2 counts the two-NaN lanes of add and mul.
        let mut names = vec![name];
        if matches!(op, 0 | 2) {
            names.push("nan2");
        }
        runs.push((1, Some(op as u32), names));
    }
    for (op, name) in ["eq", "ne", "lt", "le", "gt", "ge"].into_iter().enumerate() {
        runs.push((2, Some(op as u32), vec![name]));
    }
    runs.push((
        3,
        None,
        vec![
            "to_signed",
            "from_signed",
            "from_unsigned",
            "to_unsigned",
            "widen",
            "narrow",
        ],
    ));
    runs
}

/// Every lane of every operation, bit for bit and, in the strict builds,
/// flag for flag in each rounding mode, as GCC 14, Clang 19 and Apple clang
/// give them, in both interpreters. The interpreters run the sample of the
/// grid, which keeps every pair of operand values, one operation per run to
/// stay within their budget of 100,000 steps. The EvoIR interpreter runs
/// every build at -O0 and -O2 on both targets. The C-HIR interpreter, which
/// ignores the optimization level and is slower, runs every Linux build, and
/// on Darwin the plain build and the strict build of rounding mode 1. The
/// native tests take the whole grid.
#[test]
fn special_value_grid_matches_the_references_in_both_interpreters() {
    let grid = fixture("floating-vector-lanes.c");
    let mut runs = Vec::new();
    for target in TARGETS {
        for (strict, kind, mode) in grid_builds() {
            for interpreter in INTERPRETERS {
                let levels: &[&str] = if interpreter == "--interpret-evoir" {
                    &LEVELS
                } else if target == DARWIN && strict && mode != 1 {
                    &[]
                } else {
                    &["-O0"]
                };
                for level in levels {
                    for (part, op, names) in sample_runs() {
                        runs.push((
                            target,
                            strict,
                            kind,
                            mode,
                            interpreter,
                            *level,
                            part,
                            op,
                            names,
                        ));
                    }
                }
            }
        }
    }
    let failures = parallel(
        &runs,
        |(target, strict, kind, mode, interpreter, level, part, op, names)| {
            let expected = grid_expected(true, *strict, *kind, *mode)
                .into_iter()
                .filter(|line| names.contains(&line.name.as_str()))
                .map(|line| format!("0x{}ull", line.hash))
                .collect::<Vec<_>>();
            assert_eq!(expected.len(), names.len(), "{names:?}");
            let mut args = grid_args(*strict, *kind, *mode);
            args.extend([
                "-DSAMPLE".to_owned(),
                format!("-DPART={part}"),
                "-DCHECK".to_owned(),
                format!("-DEXPECT={}", expected.join(",")),
            ]);
            if let Some(op) = op {
                args.push(format!("-DOP={op}"));
            }
            let mut run = args.iter().map(String::as_str).collect::<Vec<_>>();
            run.extend([*level, *interpreter]);
            let output = evocc(target, &run, &grid);
            (interpreted(&output) != Some(0)).then(|| {
                format!(
                    "{target} strict={strict} KIND={kind} MODE={mode} {interpreter} {level} {names:?}: {}",
                    text(&output)
                )
            })
        },
    );
    let failures = failures.into_iter().flatten().collect::<Vec<_>>();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Darwin native code prints the recorded lines at -O0 and -O2, and so does
/// Apple clang now.
#[cfg(target_os = "macos")]
#[test]
fn special_value_grid_matches_apple_clang_natively() {
    let work = Work::new("grid-darwin");
    let grid = fixture("floating-vector-lanes.c");
    let mut failures = Vec::new();
    for (strict, kind, mode) in grid_builds() {
        let expected = grid_expected(false, strict, kind, mode)
            .into_iter()
            .map(|line| line.line)
            .collect::<Vec<_>>();
        let args = grid_args(strict, kind, mode);
        for level in LEVELS {
            let program = work.0.join(format!("evocc-{strict}-{kind}-{mode}{level}"));
            let mut build = args.iter().map(String::as_str).collect::<Vec<_>>();
            build.extend([level, "-o", program.to_str().unwrap()]);
            let output = evocc(DARWIN, &build, &grid);
            assert!(output.status.success(), "{}", text(&output));
            let run = Command::new(&program).output().unwrap();
            let lines = String::from_utf8_lossy(&run.stdout)
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if !run.status.success() || lines != expected {
                failures.push(format!(
                    "EvoCC strict={strict} KIND={kind} MODE={mode} {level}: {}",
                    text(&run)
                ));
            }
            let reference = work.0.join(format!("clang-{strict}-{kind}-{mode}{level}"));
            let built = Command::new("clang")
                .args(&args)
                .args(["-w", "-ffp-contract=off", level])
                .arg(&grid)
                .arg("-o")
                .arg(&reference)
                .output()
                .unwrap();
            assert!(built.status.success(), "{}", text(&built));
            let run = Command::new(&reference).output().unwrap();
            let lines = String::from_utf8_lossy(&run.stdout)
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert_eq!(
                lines, expected,
                "Apple clang {strict} {kind} {mode} {level}"
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Linux native code prints the recorded lines at -O0 and -O2, and so do
/// GCC 14 and Clang 19 in the sealed image.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn special_value_grid_matches_gcc_14_natively() {
    let work = Work::new("grid-linux");
    let grid = work.copy(&fixture("floating-vector-lanes.c"));
    let mut script = String::from("cd /work\n");
    let mut expected = Vec::new();
    for (strict, kind, mode) in grid_builds() {
        let lines = grid_expected(false, strict, kind, mode)
            .into_iter()
            .map(|line| line.line)
            .collect::<Vec<_>>();
        let args = grid_args(strict, kind, mode);
        for level in LEVELS {
            let name = format!("{}-{kind}-{mode}{level}", u8::from(strict));
            let assembly = work.0.join(format!("evocc-{name}.s"));
            let mut build = args.iter().map(String::as_str).collect::<Vec<_>>();
            build.extend([level, "-S", "-o", assembly.to_str().unwrap()]);
            let output = evocc(LINUX, &build, &grid);
            assert!(output.status.success(), "{name}\n{}", text(&output));
            script.push_str(&format!(
                "gcc-14 evocc-{name}.s -o t -lm && ./t > evocc-{name}.txt; echo \"evocc {name} $?\"\n"
            ));
            for compiler in ["gcc-14", "clang-19"] {
                script.push_str(&format!(
                    "{compiler} {} {level} -w floating-vector-lanes.c -o r -lm && ./r > {compiler}-{name}.txt; echo \"{compiler} {name} $?\"\n",
                    args.join(" ")
                ));
            }
            expected.push((name, lines.clone()));
        }
    }
    let output = sealed(&work, &script);
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(stdout.lines().count(), expected.len() * 3, "{stdout}");
    assert!(stdout.lines().all(|line| line.ends_with(" 0")), "{stdout}");
    let mut failures = Vec::new();
    for (name, lines) in &expected {
        for subject in ["evocc", "gcc-14", "clang-19"] {
            let printed = std::fs::read_to_string(work.0.join(format!("{subject}-{name}.txt")))
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if &printed != lines {
                failures.push(format!("{subject} {name}: {printed:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The crossed-ABI builds: the file, its defines and the two sides.
fn crossed_pairs() -> Vec<(String, PathBuf, Vec<String>, Vec<String>)> {
    let mut pairs = vec![(
        "abi".to_owned(),
        fixture("floating-vector-abi-worker.c"),
        Vec::new(),
        Vec::new(),
    )];
    pairs.push((
        "crossed".to_owned(),
        packet("l05-abi-crossed.c"),
        vec!["-DCASE=0".into(), "-DPART=1".into()],
        vec!["-DCASE=0".into(), "-DPART=2".into()],
    ));
    for case in 0..4 {
        pairs.push((
            format!("hva{case}"),
            packet("l05-hva-mixed-lanes.c"),
            vec![format!("-DCASE={case}"), "-DPART=1".into()],
            vec![format!("-DCASE={case}"), "-DPART=2".into()],
        ));
    }
    pairs
}

/// Crossed calls between EvoCC and GCC 14 or Clang 19, in both directions,
/// at -O0 and -O2: float and double vectors of 8 and 16 bytes as argument,
/// result, variadic argument and HVA record member, mixed-lane HVAs
/// included. GCC 14 at -O2 miscompiles its own `va_arg` of an HVA (strict
/// aliasing; tests/fixtures/hva-varargs-reference.out), so a GCC -O2
/// callee of floating-vector-abi gives exactly its self-control's status,
/// and the same callee built with -fno-strict-aliasing passes.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn crossed_calls_agree_with_gcc_14_and_clang_19() {
    let work = Work::new("crossed-linux");
    for name in [
        "floating-vector-abi.h",
        "floating-vector-abi-worker.c",
        "floating-vector-abi-driver.c",
    ] {
        work.copy(&fixture(name));
    }
    work.copy(&packet("l05-abi-crossed.c"));
    work.copy(&packet("l05-hva-mixed-lanes.c"));
    let mut script = String::from("cd /work\n");
    let mut rows = 0;
    for level in LEVELS {
        for (name, file, callee, caller) in crossed_pairs() {
            let file_name = file.file_name().unwrap().to_str().unwrap().to_owned();
            for (side, defines, source) in [
                ("callee", &callee, file.clone()),
                (
                    "caller",
                    &caller,
                    if name == "abi" {
                        fixture("floating-vector-abi-driver.c")
                    } else {
                        file.clone()
                    },
                ),
            ] {
                let source_name = source.file_name().unwrap().to_str().unwrap().to_owned();
                let assembly = work.0.join(format!("evocc-{name}-{side}{level}.s"));
                let mut args = vec!["-std=gnu11", level];
                args.extend(defines.iter().map(String::as_str));
                args.extend(["-S", "-o", assembly.to_str().unwrap()]);
                let output = evocc(LINUX, &args, &work.0.join(&source_name));
                assert!(
                    output.status.success(),
                    "{name} {side} {level}\n{}",
                    text(&output)
                );
                script.push_str(&format!(
                    "gcc-14 -c evocc-{name}-{side}{level}.s -o evocc-{name}-{side}{level}.o\n"
                ));
                for compiler in ["gcc-14", "clang-19"] {
                    script.push_str(&format!(
                        "{compiler} -std=gnu11 -w {level} {} -c {source_name} -o {compiler}-{name}-{side}{level}.o\n",
                        defines.join(" ")
                    ));
                }
                if name == "abi" && side == "callee" {
                    script.push_str(&format!(
                        "gcc-14 -std=gnu11 -w {level} -fno-strict-aliasing -c {source_name} -o gccnsa-{name}-{side}{level}.o\n"
                    ));
                }
                let _ = &file_name;
            }
            let mut pairs = vec![
                ("evocc", "evocc"),
                ("evocc", "gcc-14"),
                ("gcc-14", "evocc"),
                ("gcc-14", "gcc-14"),
                ("evocc", "clang-19"),
                ("clang-19", "evocc"),
            ];
            if name == "abi" {
                pairs.push(("gccnsa", "evocc"));
            }
            for (callee, caller) in pairs {
                script.push_str(&format!(
                    "gcc-14 {caller}-{name}-caller{level}.o {callee}-{name}-callee{level}.o -o t && {{ ./t; echo \"{name} callee={callee} caller={caller} {level} $?\"; }}\n"
                ));
                rows += 1;
            }
        }
    }
    let output = sealed(&work, &script);
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    assert_eq!(stdout.lines().count(), rows, "{}", text(&output));
    let self_control = |name: &str, level: &str| {
        stdout
            .lines()
            .find(|line| line.starts_with(&format!("{name} callee=gcc-14 caller=gcc-14 {level} ")))
            .and_then(|line| line.rsplit_once(' '))
            .map(|(_, status)| status.to_owned())
    };
    let mut failures = Vec::new();
    for line in stdout.lines() {
        let (head, status) = line.rsplit_once(' ').unwrap();
        let words = head.split(' ').collect::<Vec<_>>();
        let (name, callee, level) = (words[0], words[1], words[3]);
        let expected = if name == "abi" && callee == "callee=gcc-14" && level == "-O2" {
            // GCC's own defect: the self-control names the first variadic
            // HVA check, and an EvoCC caller sees the same status.
            let control = self_control(name, level);
            assert_eq!(control.as_deref(), Some("16"), "{stdout}");
            "16".to_owned()
        } else {
            "0".to_owned()
        };
        if status != expected {
            failures.push(format!("{line}: expected {expected}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Crossed calls between EvoCC and Apple clang, in both directions, at -O0
/// and -O2.
#[cfg(target_os = "macos")]
#[test]
fn crossed_calls_agree_with_apple_clang() {
    let work = Work::new("crossed-darwin");
    for name in [
        "floating-vector-abi.h",
        "floating-vector-abi-worker.c",
        "floating-vector-abi-driver.c",
    ] {
        work.copy(&fixture(name));
    }
    let mut failures = Vec::new();
    for level in LEVELS {
        for (name, file, callee, caller) in crossed_pairs() {
            for (side, defines, source) in [
                ("callee", &callee, work.copy(&file)),
                (
                    "caller",
                    &caller,
                    if name == "abi" {
                        work.0.join("floating-vector-abi-driver.c")
                    } else {
                        work.copy(&file)
                    },
                ),
            ] {
                let object = work.0.join(format!("evocc-{name}-{side}{level}.o"));
                let mut args = vec!["-std=gnu11", level];
                args.extend(defines.iter().map(String::as_str));
                args.extend(["-c", "-o", object.to_str().unwrap()]);
                let output = evocc(DARWIN, &args, &source);
                assert!(
                    output.status.success(),
                    "{name} {side} {level}\n{}",
                    text(&output)
                );
                let clang = Command::new("clang")
                    .args(["-std=gnu11", "-w", level, "-c"])
                    .args(defines)
                    .arg(&source)
                    .arg("-o")
                    .arg(work.0.join(format!("clang-{name}-{side}{level}.o")))
                    .output()
                    .unwrap();
                assert!(clang.status.success(), "{}", text(&clang));
            }
            for (callee, caller) in [
                ("evocc", "evocc"),
                ("evocc", "clang"),
                ("clang", "evocc"),
                ("clang", "clang"),
            ] {
                let program = work.0.join(format!("{name}-{callee}-{caller}{level}"));
                let link = Command::new("clang")
                    .arg(work.0.join(format!("{caller}-{name}-caller{level}.o")))
                    .arg(work.0.join(format!("{callee}-{name}-callee{level}.o")))
                    .arg("-o")
                    .arg(&program)
                    .output()
                    .unwrap();
                assert!(link.status.success(), "{}", text(&link));
                let run = Command::new(&program).output().unwrap();
                if run.status.code() != Some(0) {
                    failures.push(format!(
                        "{name} callee={callee} caller={caller} {level}: {:?}",
                        run.status
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Under -mgeneral-regs-only every floating vector use is refused with the
/// SIMD-class diagnostic, an object copy and a 4-byte vector included, on
/// both targets; the integer and scalar controls keep it too.
#[test]
fn general_registers_only_refuses_floating_vectors() {
    let work = Work::new("general-registers");
    let tiny = work.source(
        "tiny.c",
        "typedef float f1 __attribute__((vector_size(4)));\nf1 g, h;\nvoid copy(void) { g = h; }\n",
    );
    let double = work.source(
        "double.c",
        "typedef double d1 __attribute__((vector_size(8)));\nd1 g, h;\nvoid copy(void) { g = h; }\n",
    );
    let mut failures = Vec::new();
    for target in TARGETS {
        for level in LEVELS {
            let mut inputs = (0..=4)
                .map(|case| (packet("l05-general-regs.c"), format!("-DCASE={case}")))
                .collect::<Vec<_>>();
            inputs.push((tiny.clone(), "-DTINY".into()));
            inputs.push((double.clone(), "-DDOUBLE".into()));
            for (file, define) in inputs {
                let assembly = work.0.join("out.s");
                let output = evocc(
                    target,
                    &[
                        "-std=gnu11",
                        level,
                        "-mgeneral-regs-only",
                        &define,
                        "-S",
                        "-o",
                        assembly.to_str().unwrap(),
                    ],
                    &file,
                );
                if output.status.success() || first_code(&output).as_deref() != Some("E4002") {
                    failures.push(format!(
                        "{target} {level} {} {define}: {}",
                        file.display(),
                        text(&output)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A 4-byte floating vector is no AAPCS64 short vector, and GCC 14 and Clang
/// 19 pass it differently (table G), so it stays E4002 at a call boundary;
/// inside a function it is a value.
#[test]
fn tiny_floating_vectors_keep_their_boundary_refusal() {
    let work = Work::new("tiny");
    let local = work.source(
        "local.c",
        "typedef float f1 __attribute__((vector_size(4)));\n\
         static volatile float x = 1.5f;\n\
         int main(void) { f1 a = {x}; f1 b = a + a; return b[0] != 3.0f; }\n",
    );
    let mut failures = Vec::new();
    for target in TARGETS {
        for part in [1, 2] {
            let define = format!("-DPART={part}");
            let assembly = work.0.join("out.s");
            let output = evocc(
                target,
                &[
                    "-std=gnu11",
                    "-DCASE=1",
                    &define,
                    "-S",
                    "-o",
                    assembly.to_str().unwrap(),
                ],
                &packet("l05-abi-crossed.c"),
            );
            if output.status.success() || first_code(&output).as_deref() != Some("E4002") {
                failures.push(format!("{target} PART={part}: {}", text(&output)));
            }
        }
        for interpreter in INTERPRETERS {
            let output = evocc(target, &["-std=gnu11", interpreter], &local);
            if interpreted(&output) != Some(0) {
                failures.push(format!("{target} {interpreter}: {}", text(&output)));
            }
        }
        let assembly = work.0.join("local.s");
        let output = evocc(
            target,
            &["-std=gnu11", "-S", "-o", assembly.to_str().unwrap()],
            &local,
        );
        if !output.status.success() {
            failures.push(format!("{target} -S: {}", text(&output)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Each lane keeps its own obligations: a read of an uninitialized lane is
/// E3001, a runtime index outside the vector is E3011, and the conversion
/// of a NaN lane to an integer is E3063, in both interpreters.
#[test]
fn floating_lane_obligations_keep_their_diagnostics() {
    let work = Work::new("obligations");
    let cases = [
        (
            "typedef float f4 __attribute__((vector_size(16)));\n\
             int main(void) { f4 v; v[1] = 2; f4 w = v + v; return w[1] != 4; }\n",
            "E3001",
        ),
        (
            "typedef double d2 __attribute__((vector_size(16)));\n\
             static volatile int i = 2;\n\
             int main(void) { d2 v = {1, 2}; return v[i] != 0; }\n",
            "E3011",
        ),
        (
            "typedef double d2 __attribute__((vector_size(16)));\n\
             typedef long l2 __attribute__((vector_size(16)));\n\
             static volatile double z = 0;\n\
             int main(void) { d2 v = {1, z / z}; l2 r = __builtin_convertvector(v, l2); return (int)r[0]; }\n",
            "E3063",
        ),
        (
            "typedef float f2 __attribute__((vector_size(8)));\n\
             typedef unsigned u2 __attribute__((vector_size(8)));\n\
             static volatile float m = -1;\n\
             int main(void) { f2 v = {m, 1}; u2 r = __builtin_convertvector(v, u2); return (int)r[1]; }\n",
            "E3063",
        ),
    ];
    let mut failures = Vec::new();
    for (index, (source, code)) in cases.iter().enumerate() {
        let file = work.source(&format!("case{index}.c"), source);
        for target in TARGETS {
            for interpreter in INTERPRETERS {
                let output = evocc(target, &["-std=gnu11", interpreter], &file);
                if output.status.success() || first_code(&output).as_deref() != Some(*code) {
                    failures.push(format!(
                        "{target} {interpreter} case {index}: expected {code}\n{}",
                        text(&output)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The floating `arm_neon.h` intrinsics belong to L05b. Their nominal
/// vector types and builtins keep the refusals they had before L05.
#[test]
fn floating_neon_intrinsics_keep_their_refusals() {
    let probe = packet("l05-neon-subset.c");
    let mut runs = Vec::new();
    for case in 0..=9 {
        for (target, mode, code) in [
            (LINUX, "--interpret-c-hir", "E3007"),
            (LINUX, "--interpret-evoir", "E3050"),
            (LINUX, "-fsyntax-only", ""),
            (DARWIN, "-fsyntax-only", "E1004"),
        ] {
            runs.push((case, target, mode, code));
        }
    }
    let failures = parallel(&runs, |(case, target, mode, code)| {
        let define = format!("-DCASE={case}");
        let output = evocc(target, &["-std=gnu11", &define, mode], &probe);
        let expected = (!code.is_empty()).then_some(*code);
        let observed = if output.status.success() {
            None
        } else {
            first_code(&output)
        };
        (observed.as_deref() != expected).then(|| {
            format!(
                "{target} CASE={case} {mode}: expected {expected:?}\n{}",
                text(&output)
            )
        })
    });
    let failures = failures.into_iter().flatten().collect::<Vec<_>>();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Floating vectors keep their lane types in EvoIR: a vector of `float`
/// lanes is `vec<4, f32>`, a lane is an `f32` value, and a record of
/// floating vectors carries its HVA mark.
#[test]
fn floating_vectors_keep_their_lane_types_in_evoir() {
    let work = Work::new("evoir");
    let source = work.source(
        "types.c",
        "typedef float f4 __attribute__((vector_size(16)));\n\
         typedef double d1 __attribute__((vector_size(8)));\n\
         struct pair { f4 a, b; };\n\
         f4 add(f4 a, f4 b) { return a + b; }\n\
         d1 neg(d1 a) { return -a; }\n\
         struct pair swap(struct pair p) { struct pair r = {p.b, p.a}; return r; }\n",
    );
    for target in TARGETS {
        let output = evocc(target, &["-std=gnu11", "--emit=evoir"], &source);
        assert!(output.status.success(), "{target}\n{}", text(&output));
        let ir = String::from_utf8_lossy(&output.stdout);
        for expected in [
            "vec<4, f32>",
            "vec<1, f64>",
            "vec.extract",
            "fp.add",
            "fp.neg",
            "vec.pack",
            "hva=2xvec<4, bits<32>>",
        ] {
            assert!(ir.contains(expected), "{target}: {expected}\n{ir}");
        }
    }
}

/// The EvoMIR interpreter runs the lowered machine code of the lane-move,
/// shuffle and boundary probes to the same value as the EvoIR interpreter:
/// a floating lane moves through a general register as its exact bits.
#[test]
fn machine_lane_moves_keep_every_bit() {
    let mut runs = Vec::new();
    for case in 0..=3 {
        runs.push(("floating-vector-boundaries.c", case));
        runs.push(("l05-shuffle.c", case));
    }
    for (name, case) in runs {
        let file = if name.starts_with("l05-") {
            packet(name)
        } else {
            fixture(name)
        };
        let define = format!("-DCASE={case}");
        let output = evocc(LINUX, &["-std=gnu11", &define, "--emit=evoir"], &file);
        assert!(output.status.success(), "{name} {case}\n{}", text(&output));
        let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
        let machine = evocc_mir::lower_module(&module).unwrap();
        let outcome = evocc_mir::interpret_module(
            &machine,
            "main",
            &[],
            evocc_mir::MirLimits::new(1_000_000),
        )
        .unwrap();
        assert_eq!(
            outcome.results,
            [evocc_mir::MirValue::Int(0)],
            "{name} CASE={case}"
        );
    }
}

/// The crossed ABI probe as one unit runs in both interpreters on both
/// targets: every call, variadic argument and HVA record of the native
/// crossed tests keeps its lanes.
#[test]
fn crossed_abi_probe_runs_in_both_interpreters() {
    let unit = fixture("floating-vector-abi-interpreted.c");
    let mut runs = Vec::new();
    for target in TARGETS {
        for interpreter in INTERPRETERS {
            for level in LEVELS {
                runs.push((target, interpreter, level));
            }
        }
    }
    let failures = parallel(&runs, |(target, interpreter, level)| {
        let output = evocc(target, &["-std=gnu11", level, interpreter], &unit);
        (interpreted(&output) != Some(0))
            .then(|| format!("{target} {interpreter} {level}: {}", text(&output)))
    });
    let failures = failures.into_iter().flatten().collect::<Vec<_>>();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
