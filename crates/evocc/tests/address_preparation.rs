use std::process::Command;

use evocc_mir::{BinOp, ExtOp, MInst, MirLimits, MirValue, Target};

fn run(mode: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", "-O2", mode])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/repeated-pointer-index.c"
        ))
        .output()
        .unwrap()
}

fn machine() -> evocc_mir::MModule {
    let output = run("--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    let machine = evocc_mir::lower_module_for(Target::Linux, &module, &|_| false).unwrap();
    evocc_mir::validate_module(&machine).unwrap();
    machine
}

#[test]
fn repeated_addresses_share_only_their_pure_index_preparation() {
    let machine = machine();
    let instructions = |name: &str| {
        machine
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap()
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .collect::<Vec<_>>()
    };
    let same = instructions("same_stride");
    assert_eq!(
        same.iter()
            .filter(|i| matches!(
                i,
                MInst::Extend {
                    op: ExtOp::Sxtw,
                    ..
                }
            ))
            .count(),
        1,
        "one already executed index extension should serve both bases"
    );
    assert_eq!(
        same.iter()
            .filter(|i| matches!(
                i,
                MInst::BinaryImm {
                    op: BinOp::Lsl,
                    imm: 2,
                    ..
                }
            ))
            .count(),
        1
    );
    assert_eq!(
        same.iter()
            .filter(|i| matches!(i, MInst::Load { .. }))
            .count(),
        2,
        "this is not load forwarding"
    );
    let different = instructions("different_strides");
    let row_index = machine
        .functions
        .iter()
        .find(|f| f.name == "different_strides")
        .unwrap()
        .blocks[0]
        .args[2];
    assert_eq!(
        different
            .iter()
            .filter(|i| matches!(
                i,
                MInst::Extend {
                    op: ExtOp::Sxtw,
                    src,
                    ..
                } if *src == row_index
            ))
            .count(),
        1
    );
    assert_eq!(
        different
            .iter()
            .filter(|i| matches!(i, MInst::Binary { op: BinOp::Mul, .. }))
            .count(),
        2,
        "distinct row strides must not share their scaled delta"
    );
}

#[test]
fn negative_indices_and_guard_continuations_keep_their_values() {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
    let machine = machine();
    assert_eq!(
        evocc_mir::interpret_module(&machine, "main", &[], MirLimits::new(100_000))
            .unwrap()
            .results,
        [MirValue::Int(0)]
    );
    let assembly = run("--emit=asm");
    assert!(
        assembly.status.success(),
        "{}",
        String::from_utf8_lossy(&assembly.stderr)
    );
    assert!(
        String::from_utf8_lossy(&assembly.stdout).contains("brk #3004"),
        "signed overflow obligations must survive"
    );
}
