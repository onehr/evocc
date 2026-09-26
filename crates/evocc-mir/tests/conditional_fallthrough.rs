//! Preserve logical successors; choose the actual adjacent block at emission.
use evocc_mir::{
    allocate_for, emit_module_for, out_of_ssa, verify_allocation, Cond, MBlock, MFunction, MInst,
    MTerm, RegClass, Target, VReg, Width,
};

fn function(term: MTerm, order: [u32; 3]) -> MFunction {
    let x = VReg::new(0, RegClass::Gpr64);
    let flags = VReg::new(1, RegClass::Flags);
    let mut blocks = vec![MBlock {
        id: 0,
        args: vec![x],
        insts: if matches!(term, MTerm::BCond { .. }) {
            vec![MInst::CmpImm {
                dst: flags,
                lhs: x,
                imm: 0,
                width: Width::X,
            }]
        } else {
            vec![]
        },
        term,
    }];
    for id in order {
        let result = VReg::new(10 + id, RegClass::Gpr64);
        blocks.push(MBlock {
            id,
            args: vec![],
            insts: vec![MInst::MovImm {
                dst: result,
                imm: u64::from(id + 10),
            }],
            term: MTerm::Ret {
                values: vec![result],
            },
        });
    }
    MFunction {
        name: "choose".into(),
        blocks,
    }
}

fn terms() -> Vec<(MTerm, String, String)> {
    let mut terms = vec![];
    for cond in [
        Cond::Eq,
        Cond::Ne,
        Cond::Lt,
        Cond::Le,
        Cond::Gt,
        Cond::Ge,
        Cond::Lo,
        Cond::Ls,
        Cond::Hi,
        Cond::Hs,
        Cond::Vs,
        Cond::Vc,
        Cond::Cs,
        Cond::Cc,
    ] {
        terms.push((
            MTerm::BCond {
                flags: VReg::new(1, RegClass::Flags),
                cond,
                taken: 1,
                taken_args: vec![],
                untaken: 2,
                untaken_args: vec![],
            },
            format!("b.{}", cond.mnemonic()),
            format!("b.{}", cond.inverted().mnemonic()),
        ));
    }
    for nonzero in [false, true] {
        terms.push((
            MTerm::Cb {
                nonzero,
                src: VReg::new(0, RegClass::Gpr64),
                taken: 1,
                taken_args: vec![],
                untaken: 2,
                untaken_args: vec![],
            },
            format!("{} x0,", if nonzero { "cbnz" } else { "cbz" }),
            format!("{} x0,", if nonzero { "cbz" } else { "cbnz" }),
        ));
        terms.push((
            MTerm::Tb {
                nonzero,
                src: VReg::new(0, RegClass::Gpr64),
                bit: 63,
                taken: 1,
                taken_args: vec![],
                untaken: 2,
                untaken_args: vec![],
            },
            format!("{} x0, #63,", if nonzero { "tbnz" } else { "tbz" }),
            format!("{} x0, #63,", if nonzero { "tbz" } else { "tbnz" }),
        ));
    }
    terms
}

fn entry(text: &str) -> Vec<&str> {
    text.lines()
        .skip_while(|line| !line.ends_with("choose_block0:"))
        .skip(1)
        .take_while(|line| !line.contains("choose_block") || !line.ends_with(':'))
        .map(str::trim)
        .filter(|line| !line.starts_with("cmp "))
        .collect()
}

#[test]
fn either_adjacent_successor_uses_one_short_conditional_branch() {
    for target in [Target::Linux, Target::Darwin] {
        let prefix = if target == Target::Linux { ".L" } else { "L" };
        for (term, normal, inverse) in terms() {
            for (order, mnemonic, destination, extra) in [
                ([1, 2, 3], inverse.as_str(), 2, None),
                ([2, 1, 3], normal.as_str(), 1, None),
                ([3, 1, 2], normal.as_str(), 1, Some(2)),
            ] {
                let f = function(term.clone(), order);
                let allocated = allocate_for(target, &out_of_ssa(&f)).unwrap();
                verify_allocation(&allocated).unwrap();
                let text = emit_module_for(target, &[allocated], &[]);
                let mut expected = vec![format!("{mnemonic} {prefix}choose_block{destination}")];
                if let Some(id) = extra {
                    expected.push(format!("b {prefix}choose_block{id}"));
                }
                assert_eq!(
                    entry(&text),
                    expected,
                    "{term:?}, order {order:?}, {target:?}"
                );
            }
        }
    }
}

#[test]
fn adjacent_taken_edge_is_preserved_when_the_other_edge_needs_relaxation() {
    for target in [Target::Linux, Target::Darwin] {
        let prefix = if target == Target::Linux { ".L" } else { "L" };
        // Both CB/TB senses plus flag conditions; large filler is deliberately
        // after the fall-through label, so the non-adjacent edge is out of range.
        for (term, normal, _) in terms().into_iter().skip(13) {
            // Filler repeats definitions only to force emitter sizing; allocation
            // is performed on the well-formed one-definition shape first.
            let mut allocated =
                allocate_for(target, &out_of_ssa(&function(term, [1, 2, 3]))).unwrap();
            verify_allocation(&allocated).unwrap();
            let filler = allocated.blocks.iter_mut().find(|b| b.id == 1).unwrap();
            let inst = filler.insts[0].clone();
            filler.insts.splice(0..0, std::iter::repeat_n(inst, 12_000));
            let text = emit_module_for(target, &[allocated], &[]);
            assert_eq!(
                entry(&text),
                [
                    format!("{normal} {prefix}choose_relax0"),
                    format!("b {prefix}choose_block2"),
                    format!("{prefix}choose_relax0:"),
                ]
            );
            assemble(&text, target);
        }
    }
}

fn assemble(text: &str, target: Target) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("SKIP: clang is unavailable for the long-branch assembler check");
        return;
    }
    let mut child = Command::new("clang")
        .args([
            "-target",
            if target == Target::Linux {
                "aarch64-linux-gnu"
            } else {
                "arm64-apple-macos14"
            },
            "-x",
            "assembler",
            "-c",
            "-",
            "-o",
            "/dev/null",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn equal_target_edges_keep_their_distinct_value_copies() {
    use evocc_mir::{MModule, MirLimits, MirValue};
    let g = |id| VReg::new(id, RegClass::Gpr64);
    let function = MFunction {
        name: "choose".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![g(0)],
                insts: vec![
                    MInst::MovImm { dst: g(1), imm: 11 },
                    MInst::MovImm { dst: g(2), imm: 29 },
                ],
                term: MTerm::Cb {
                    nonzero: true,
                    src: g(0),
                    taken: 1,
                    taken_args: vec![g(1)],
                    untaken: 1,
                    untaken_args: vec![g(2)],
                },
            },
            MBlock {
                id: 1,
                args: vec![g(3)],
                insts: vec![],
                term: MTerm::Ret { values: vec![g(3)] },
            },
        ],
    };
    let module = MModule {
        functions: vec![function.clone()],
        ..MModule::default()
    };
    evocc_mir::validate_module(&module).unwrap();
    for (input, expected) in [(0, 29), (1, 11)] {
        assert_eq!(
            evocc_mir::interpret_module(
                &module,
                "choose",
                &[MirValue::Int(input)],
                MirLimits::new(100)
            )
            .unwrap()
            .results,
            [MirValue::Int(expected)]
        );
    }
    for target in [Target::Linux, Target::Darwin] {
        let allocated = allocate_for(target, &out_of_ssa(&function)).unwrap();
        verify_allocation(&allocated).unwrap();
        let MTerm::Cb { taken, untaken, .. } = &allocated.blocks[0].term else {
            panic!("lost conditional");
        };
        assert_ne!(
            taken, untaken,
            "distinct copies require distinct incoming routes"
        );
        assemble(&emit_module_for(target, &[allocated], &[]), target);
    }
}
