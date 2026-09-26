use std::io::Write;
use std::process::{Command, Stdio};

use evocc_mir::{MInst, MirLimits, MirValue, Target};

fn run(level: &str, mode: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([
            "--target=aarch64-unknown-linux-gnu",
            level,
            mode,
            "/dev/stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(include_bytes!(
            "../../../tests/fixtures/memory-address-selection.c"
        ))
        .unwrap();
    child.wait_with_output().unwrap()
}

fn machine(level: &str) -> evocc_mir::MModule {
    let output = run(level, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ir = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    evocc_evoir::validate_module(&ir).unwrap();
    let mir = evocc_mir::lower_module_for(Target::Linux, &ir, &|_| false).unwrap();
    evocc_mir::validate_module(&mir).unwrap();
    mir
}

#[test]
fn constant_field_accesses_select_the_existing_memory_offset_form() {
    for level in ["-O0", "-O2"] {
        let mir = machine(level);
        for (name, expected) in [("read_third", 16), ("write_second", 8), ("previous", -8)] {
            // At O0 the unary minus is still a guarded subtraction, not a
            // constant definition. This selection does not evaluate it anew.
            let expected = if level == "-O0" && name == "previous" {
                0
            } else {
                expected
            };
            let function = mir.functions.iter().find(|f| f.name == name).unwrap();
            let accesses: Vec<_> = function
                .blocks
                .iter()
                .flat_map(|b| &b.insts)
                .filter_map(|inst| match inst {
                    MInst::Load {
                        offset, volatile, ..
                    }
                    | MInst::Store {
                        offset, volatile, ..
                    } => Some((*offset, *volatile)),
                    _ => None,
                })
                .collect();
            assert_eq!(accesses, [(expected, true)], "{level} {name}");
        }
    }
}

#[test]
fn field_values_negative_offsets_and_packed_bytes_keep_their_semantics() {
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(level, mode);
            assert!(
                output.status.success(),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, b"0\n");
        }
        let mir = machine(level);
        assert_eq!(
            evocc_mir::interpret_module(&mir, "main", &[], MirLimits::new(100_000))
                .unwrap()
                .results,
            [MirValue::Int(0)]
        );
        // The MIR interpreter executes block-local pre-allocation SSA, not
        // its destroyed form. Allocation verification and native execution
        // separately exercise the latter representation.
        for function in &mir.functions {
            evocc_mir::verify_allocation(
                &evocc_mir::allocate_for(Target::Linux, &evocc_mir::out_of_ssa(function)).unwrap(),
            )
            .unwrap();
        }
        let output = run(level, "--emit=asm");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
