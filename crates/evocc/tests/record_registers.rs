//! Records of at most sixteen bytes pass in general registers (EvoMIR 6.4,
//! D29). The EvoMIR interpreter moves their bytes with transport accesses
//! (EvoMIR 4.1): padding and members that were never stored travel with their
//! initializedness, and a pointer member keeps its provenance. Each case runs
//! on both targets in the C-HIR and EvoIR interpreters, through `--check`, and
//! in the EvoMIR interpreter on the lowered EvoIR of `-O0` and `-O2`. The
//! transport marker changes no instruction, and on Darwin the native programs
//! return what Apple clang's programs return.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use evocc_mir::{MInst, MirLimits, MirValue, Target};

const TARGETS: [&str; 2] = ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"];
const MODES: [&str; 3] = ["--interpret-c-hir", "--interpret-evoir", "--check"];
const LEVELS: [&str; 2] = ["-O0", "-O2"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures/record-registers").join(name)
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn evocc(target: &str, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command.arg(format!("--target={target}"));
    if target.contains("linux") {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(target).display()
        ));
    }
    command.args(args).output().unwrap()
}

/// The EvoIR that the driver emits for `path` at `level`.
fn emitted(path: &Path, target: &str, level: &str) -> evocc_evoir::Module {
    let output = evocc(
        target,
        &["-w", level, "--emit=evoir", path.to_str().unwrap()],
    );
    assert!(output.status.success(), "{}", text(&output));
    evocc_evoir::parse_target_module(&output.stdout, target).unwrap()
}

fn machine_target(target: &str) -> Target {
    if target.contains("linux") {
        Target::Linux
    } else {
        Target::Darwin
    }
}

fn lowered(module: &evocc_evoir::Module, target: &str) -> evocc_mir::MModule {
    let lowered = evocc_mir::lower_module_for(machine_target(target), module, &|_| false)
        .unwrap_or_else(|refusals| {
            panic!(
                "lowering refused: {:?}",
                refusals
                    .iter()
                    .map(|refusal| refusal.code)
                    .collect::<Vec<_>>()
            )
        });
    evocc_mir::validate_module(&lowered).unwrap();
    lowered
}

/// The EvoMIR interpreter on the lowered module: `main`'s value, or the code
/// of the diagnostic that stopped it.
fn machine(module: &evocc_evoir::Module, target: &str) -> Result<i64, String> {
    let lowered = lowered(module, target);
    match evocc_mir::interpret_module(&lowered, "main", &[], MirLimits::new(1_000_000)) {
        Ok(outcome) => match outcome.results.first() {
            Some(MirValue::Int(raw)) => Ok(i64::from(*raw as u32 as i32)),
            other => Err(format!("{other:?}")),
        },
        Err(error) => Err(error.code()),
    }
}

/// The number of transport accesses in `module`.
fn transports(module: &evocc_mir::MModule) -> usize {
    module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.insts)
        .filter(|inst| {
            matches!(
                inst,
                MInst::Load {
                    transport: true,
                    ..
                } | MInst::Store {
                    transport: true,
                    ..
                }
            )
        })
        .count()
}

#[test]
fn records_in_general_registers_run_in_every_interpreter() {
    let path = fixture("records.c");
    for target in TARGETS {
        for mode in MODES {
            let output = evocc(
                target,
                &["-w", "--error-format=short", mode, path.to_str().unwrap()],
            );
            let label = format!("{target} {mode}");
            assert!(output.status.success(), "{label}\n{}", text(&output));
            assert!(output.stderr.is_empty(), "{label}\n{}", text(&output));
            if mode != "--check" {
                assert_eq!(
                    String::from_utf8_lossy(&output.stdout).trim(),
                    "0",
                    "{label}\n{}",
                    text(&output)
                );
            }
        }
        for level in LEVELS {
            let module = emitted(&path, target, level);
            assert!(
                transports(&lowered(&module, target)) > 0,
                "{target} {level}"
            );
            assert_eq!(machine(&module, target), Ok(0), "{target} {level} EvoMIR");
        }
    }
}

#[test]
fn an_unstored_member_is_an_uninitialized_read_in_every_interpreter() {
    for (name, line, column) in [("unstored.c", 4, 45), ("unstored_result.c", 8, 12)] {
        let path = fixture(name);
        let expected = format!(
            "{}:{line}:{column}: error[E3001]: read-of-uninitialized-object",
            path.display()
        );
        for target in TARGETS {
            for mode in MODES {
                let output = evocc(
                    target,
                    &["-w", "--error-format=short", mode, path.to_str().unwrap()],
                );
                let label = format!("{name} {target} {mode}");
                assert_eq!(output.status.code(), Some(1), "{label}\n{}", text(&output));
                assert_eq!(
                    String::from_utf8_lossy(&output.stderr).lines().next(),
                    Some(expected.as_str()),
                    "{label}\n{}",
                    text(&output)
                );
            }
            // EvoMIR has no source positions; it stops with the same code.
            for level in LEVELS {
                assert_eq!(
                    machine(&emitted(&path, target, level), target),
                    Err("E3001".to_owned()),
                    "{name} {target} {level} EvoMIR"
                );
            }
        }
    }
}

/// Allocation and emission do not read the transport marker: the module
/// without it gives the same assembly.
#[test]
fn the_transport_marker_changes_no_instruction() {
    let path = fixture("records.c");
    for target in TARGETS {
        for level in LEVELS {
            let module = emitted(&path, target, level);
            let marked = lowered(&module, target);
            let mut plain = marked.clone();
            for inst in plain
                .functions
                .iter_mut()
                .flat_map(|function| &mut function.blocks)
                .flat_map(|block| &mut block.insts)
            {
                if let MInst::Load { transport, .. } | MInst::Store { transport, .. } = inst {
                    *transport = false;
                }
            }
            assert!(transports(&marked) > 0);
            assert_eq!(transports(&plain), 0);
            let assembly = |machine: &evocc_mir::MModule| {
                let functions = machine
                    .functions
                    .iter()
                    .map(|function| {
                        evocc_mir::allocate_for(
                            machine_target(target),
                            &evocc_mir::out_of_ssa(function),
                        )
                        .unwrap()
                    })
                    .collect::<Vec<_>>();
                evocc_mir::emit_module_for(machine_target(target), &functions, &module.globals)
            };
            assert_eq!(assembly(&marked), assembly(&plain), "{target} {level}");
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_programs_return_what_apple_clang_programs_return() {
    let directory =
        std::env::temp_dir().join(format!("evocc-record-registers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let source = fixture("records.c");
    for level in LEVELS {
        let mine = directory.join(format!("records-evocc{level}"));
        let theirs = directory.join(format!("records-clang{level}"));
        let built = evocc(
            "aarch64-apple-darwin",
            &[
                level,
                source.to_str().unwrap(),
                "-o",
                mine.to_str().unwrap(),
            ],
        );
        assert!(built.status.success(), "{}", text(&built));
        let built = Command::new("clang")
            .args(["-w", level])
            .arg(&source)
            .arg("-o")
            .arg(&theirs)
            .output()
            .unwrap();
        assert!(built.status.success(), "{}", text(&built));
        let mine = Command::new(&mine).output().unwrap();
        let theirs = Command::new(&theirs).output().unwrap();
        assert_eq!(mine.status.code(), Some(0), "{level}\n{}", text(&mine));
        assert_eq!(theirs.status.code(), Some(0), "{level}\n{}", text(&theirs));
    }
    let _ = std::fs::remove_dir_all(&directory);
}

/// GCC 14's programs return 0 on Linux, and EvoCC's programs do too.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_programs_return_what_gcc_14_programs_return() {
    let directory = std::env::temp_dir().join(format!(
        "evocc-record-registers-linux-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::copy(fixture("records.c"), directory.join("records.c")).unwrap();
    let mut script = String::from("set -e\ncd /work\n");
    for level in LEVELS {
        let assembly = directory.join(format!("records{level}.s"));
        let output = evocc(
            "aarch64-unknown-linux-gnu",
            &[
                level,
                "-S",
                directory.join("records.c").to_str().unwrap(),
                "-o",
                assembly.to_str().unwrap(),
            ],
        );
        assert!(output.status.success(), "{}", text(&output));
        script.push_str(&format!(
            "gcc-14 records{level}.s -o evocc{level} && ./evocc{level}\n\
             gcc-14 -w {level} records.c -o gcc{level} && ./gcc{level}\n\
             echo {level} ok\n"
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
        .arg(format!("{}:/work", directory.display()))
        .args(["evo-nexlab:sealed", "sh", "-c", &script])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "-O0 ok\n-O2 ok\n");
    let _ = std::fs::remove_dir_all(&directory);
}
