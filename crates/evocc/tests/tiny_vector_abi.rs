//! L06: GNU integer vectors of 1, 2 and 4 bytes at a call boundary (EvoMIR
//! 6.2). Linux follows GCC 14: they pass and return as integers in general
//! registers. Darwin follows Apple clang: arguments are the 4-byte integer
//! image, and a result returns in `v0` with each lane widened. Calls that
//! return a tiny vector between EvoCC and Clang code on Linux stay a recorded
//! incompatibility; the negative controls below hold that line.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
#[cfg(target_os = "macos")]
const DARWIN: &str = "aarch64-apple-darwin";
const LEVELS: [&str; 2] = ["-O0", "-O2"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The probe pairs: the packet's probe and the per-shape probe. The last
/// field is the exit status of a crossed call with Clang on Linux, where
/// every result direction fails and nothing else does.
fn probes() -> [(&'static str, PathBuf, PathBuf, i32); 2] {
    let fixtures = root().join("tests/fixtures");
    let packet = fixtures.join("rules-20261013");
    [
        (
            "l06-tiny-vector",
            packet.join("l06-tiny-vector-driver.c"),
            packet.join("l06-tiny-vector-worker.c"),
            30,
        ),
        (
            "tiny-vector-abi-shapes",
            fixtures.join("tiny-vector-abi-shapes-driver.c"),
            fixtures.join("tiny-vector-abi-shapes-worker.c"),
            63,
        ),
    ]
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
        let path = std::env::temp_dir().join(format!("evocc-l06-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed L06 probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// Compile one translation unit to an object (Darwin) or to assembly (Linux).
fn evocc(source: &Path, target: &str, level: &str, output: &Path, options: &[&str]) {
    let action = if target == LINUX { "-S" } else { "-c" };
    let result = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg(format!("--target={target}"))
        .args(["-std=gnu11", level, action])
        .args(options)
        .arg(source)
        .arg("-o")
        .arg(output)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", text(&result));
}

#[cfg(target_os = "macos")]
#[test]
fn tiny_vector_signatures_lower_on_both_targets() {
    let work = Work::new("lower");
    for target in [LINUX, DARWIN] {
        for (name, driver, worker, _) in probes() {
            for source in [driver, worker] {
                evocc(
                    &source,
                    target,
                    "-O2",
                    &work.0.join(format!("{name}.out")),
                    &[],
                );
            }
        }
    }
}

#[test]
fn linux_general_registers_only_keeps_the_integer_convention() {
    let work = Work::new("general");
    for (name, driver, worker, _) in probes() {
        for source in [driver, worker] {
            evocc(
                &source,
                LINUX,
                "-O2",
                &work.0.join(format!("{name}.s")),
                &["-mgeneral-regs-only"],
            );
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_general_registers_only_refuses_a_widened_result() {
    // The Apple result needs v0; without SIMD registers it has no placement.
    let work = Work::new("darwin-general");
    let (_, _, worker, _) = &probes()[0];
    let result = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg(format!("--target={DARWIN}"))
        .args(["-std=gnu11", "-O2", "-c", "-mgeneral-regs-only"])
        .arg(worker)
        .arg("-o")
        .arg(work.0.join("worker.o"))
        .output()
        .unwrap();
    assert!(!result.status.success(), "{}", text(&result));
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("E4002"),
        "{}",
        text(&result)
    );
}

/// Darwin: EvoCC and Apple clang in both directions, and EvoCC alone.
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn darwin_calls_agree_with_apple_clang_in_both_directions() {
    let work = Work::new("darwin");
    for level in LEVELS {
        for (name, driver, worker, _) in probes() {
            let objects = |label: &str| work.0.join(format!("{name}-{label}.o"));
            evocc(&driver, DARWIN, level, &objects("evocc-driver"), &[]);
            evocc(&worker, DARWIN, level, &objects("evocc-worker"), &[]);
            for (source, label) in [(&driver, "clang-driver"), (&worker, "clang-worker")] {
                let result = Command::new("clang")
                    .args(["-std=gnu11", level, "-w", "-c"])
                    .arg(source)
                    .arg("-o")
                    .arg(objects(label))
                    .output()
                    .unwrap();
                assert!(result.status.success(), "{}", text(&result));
            }
            for (driver, worker) in [
                ("evocc-driver", "clang-worker"),
                ("clang-driver", "evocc-worker"),
                ("evocc-driver", "evocc-worker"),
            ] {
                let program = work.0.join(format!("{name}-{driver}-{worker}"));
                let link = Command::new("clang")
                    .arg(objects(driver))
                    .arg(objects(worker))
                    .arg("-o")
                    .arg(&program)
                    .output()
                    .unwrap();
                assert!(link.status.success(), "{}", text(&link));
                let run = Command::new(&program).output().unwrap();
                assert_eq!(
                    run.status.code(),
                    Some(0),
                    "{name} {level} {driver} {worker}"
                );
            }
        }
    }
}

/// Linux: EvoCC and GCC 14 in both directions, EvoCC alone, the
/// `-mgeneral-regs-only` pair, and the Clang 19 negative controls.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_calls_agree_with_gcc_14_and_keep_the_clang_incompatibility() {
    let work = Work::new("linux");
    let mut script = String::from("set -eu\ncd /work\n");
    let mut lines = 0;
    let check = |script: &mut String, label: &str, driver: &str, worker: &str, expected: i32| {
        script.push_str(&format!(
            "gcc-14 {driver} {worker} -o x\nset +e\n./x\nstatus=$?\nset -e\n\
             test \"$status\" -eq {expected}\necho '{label}: {expected}'\n"
        ));
    };
    for level in LEVELS {
        for (name, driver, worker, crossed) in probes() {
            let stem = format!("{name}{level}");
            for source in [&driver, &worker] {
                let file = source.file_name().unwrap().to_string_lossy().into_owned();
                std::fs::copy(source, work.0.join(&file)).unwrap();
            }
            let header = if name == "l06-tiny-vector" {
                driver.with_file_name("l06-tiny-vector-abi.h")
            } else {
                driver.with_file_name("tiny-vector-abi-shapes.h")
            };
            std::fs::copy(&header, work.0.join(header.file_name().unwrap())).unwrap();
            let driver_file = driver.file_name().unwrap().to_string_lossy().into_owned();
            let worker_file = worker.file_name().unwrap().to_string_lossy().into_owned();
            evocc(
                &driver,
                LINUX,
                level,
                &work.0.join(format!("{stem}-d.s")),
                &[],
            );
            evocc(
                &worker,
                LINUX,
                level,
                &work.0.join(format!("{stem}-w.s")),
                &[],
            );
            evocc(
                &worker,
                LINUX,
                level,
                &work.0.join(format!("{stem}-wg.s")),
                &["-mgeneral-regs-only"],
            );
            for (cc, suffix, extra) in [
                ("gcc-14", "gcc", ""),
                ("clang-19", "clang", ""),
                ("gcc-14", "gccg", " -mgeneral-regs-only"),
            ] {
                script.push_str(&format!(
                    "{cc} -std=gnu11 {level} -w{extra} -c {driver_file} -o {stem}-d{suffix}.o\n\
                     {cc} -std=gnu11 {level} -w{extra} -c {worker_file} -o {stem}-w{suffix}.o\n"
                ));
            }
            let pairs = [
                ("evocc driver, gcc-14 worker", "-d.s", "-wgcc.o", 0),
                ("gcc-14 driver, evocc worker", "-dgcc.o", "-w.s", 0),
                ("evocc driver, evocc worker", "-d.s", "-w.s", 0),
                (
                    "gcc-14 general-regs-only driver, evocc worker",
                    "-dgccg.o",
                    "-wg.s",
                    0,
                ),
                (
                    "evocc driver, clang-19 worker",
                    "-d.s",
                    "-wclang.o",
                    crossed,
                ),
                (
                    "clang-19 driver, evocc worker",
                    "-dclang.o",
                    "-w.s",
                    crossed,
                ),
            ];
            for (label, driver_suffix, worker_suffix, expected) in pairs {
                check(
                    &mut script,
                    &format!("{name} {level} {label}"),
                    &format!("{stem}{driver_suffix}"),
                    &format!("{stem}{worker_suffix}"),
                    expected,
                );
                lines += 1;
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
        .arg(format!("{}:/work", work.0.display()))
        .args(["evo-nexlab:sealed", "sh", "-c", &script])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), lines, "{stdout}");
    println!("{stdout}");
}
