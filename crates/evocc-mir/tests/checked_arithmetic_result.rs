use evocc_evoir::parse_section9_module;
use evocc_mir::{BinOp, MInst, MTerm, MirLimits, MirValue};

fn text(bits: u16, predicate: &str, op: &str, between: &str, operands: &str) -> String {
    let (between, result) = if between.is_empty() {
        (String::new(), 5)
    } else {
        (format!("    %v5 = const.bits<{bits}> {between}\n"), 6)
    };
    format!(
        "func @main(%arg0: bits<{bits}>, %arg1: bits<{bits}>, %arg2: mem<all>, %arg3: world) -> (bits<{bits}>, mem<all>, world) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }}
{{
^entry(%v0: bits<{bits}>, %v1: bits<{bits}>, %v2: mem<all>, %v3: world):
    %v4 = {predicate} %v0, %v1
    ub.assume.signed_overflow %v4
{between}    %v{result} = {op}.bits<{bits}> {operands}
    ret %v{result}, %v2, %v3
}}
"
    )
}

fn lower(source: &str) -> evocc_mir::MModule {
    let ir = parse_section9_module(source.as_bytes()).unwrap();
    let mir = evocc_mir::lower_module(&ir).unwrap();
    evocc_mir::validate_module(&mir).unwrap();
    let roundtrip = evocc_mir::parse_module(&evocc_mir::render_module(&mir)).unwrap();
    assert_eq!(roundtrip, mir);
    mir
}

#[test]
fn checked_add_sub_carry_the_existing_result_over_the_guard() {
    for bits in [32, 64] {
        for (predicate, op) in [("signed_add_defined", "add"), ("signed_sub_defined", "sub")] {
            let mir = lower(&text(bits, predicate, op, "", "%v0, %v1"));
            let function = &mir.functions[0];
            assert!(
                function
                    .blocks
                    .iter()
                    .flat_map(|b| &b.insts)
                    .all(|inst| !matches!(
                        inst,
                        MInst::Binary {
                            op: BinOp::Add | BinOp::Sub,
                            ..
                        }
                    )),
                "{bits} {op}: repeated arithmetic after checking the same operation"
            );
            let MTerm::BCond {
                cond: evocc_mir::Cond::Vs,
                untaken_args,
                ..
            } = &function.blocks[0].term
            else {
                panic!("the checked branch must remain");
            };
            assert_eq!(
                untaken_args.len(),
                1,
                "only the computed value needs transport"
            );
            assert!(function
                .blocks
                .iter()
                .any(|b| matches!(b.term, MTerm::Brk { imm: 3004 })));
        }
    }
}

#[test]
fn checked_result_reuse_preserves_values_and_overflow_traps() {
    for bits in [32, 64] {
        let sign = 1_u64 << (bits - 1);
        let mask = u64::MAX >> (64 - bits);
        for (predicate, op, values) in [
            (
                "signed_add_defined",
                "add",
                vec![
                    (sign - 1, 0, Some(sign - 1)),
                    (sign, 1, Some(sign + 1)),
                    (sign - 1, 1, None),
                    (sign, mask, None),
                ],
            ),
            (
                "signed_sub_defined",
                "sub",
                vec![
                    (sign, 0, Some(sign)),
                    (sign - 1, 1, Some(sign - 2)),
                    (sign, 1, None),
                    (sign - 1, mask, None),
                ],
            ),
        ] {
            let mir = lower(&text(bits, predicate, op, "", "%v0, %v1"));
            for (a, b, expected) in values {
                let outcome = evocc_mir::interpret_module(
                    &mir,
                    "main",
                    &[MirValue::Int(a), MirValue::Int(b)],
                    MirLimits::new(100),
                );
                match expected {
                    Some(value) => assert_eq!(
                        outcome.unwrap().results,
                        [MirValue::Int(value)],
                        "{bits} {op} {a} {b}"
                    ),
                    None => assert!(
                        matches!(
                            outcome.unwrap_err(),
                            evocc_mir::MirError::Trap { imm: 3004 }
                        ),
                        "{bits} {op} {a} {b}"
                    ),
                }
            }
        }
    }
}

#[test]
fn reuse_does_not_match_different_operations_operands_or_nonadjacent_forms() {
    for (bits, predicate, op, between, operands) in [
        (32, "signed_add_defined", "sub", "", "%v0, %v1"),
        (64, "signed_sub_defined", "sub", "", "%v1, %v0"),
        (64, "signed_add_defined", "add", "0", "%v0, %v1"),
        (13, "signed_add_defined", "add", "", "%v0, %v1"),
    ] {
        let mir = lower(&text(bits, predicate, op, between, operands));
        assert!(
            mir.functions[0]
                .blocks
                .iter()
                .skip(1)
                .flat_map(|b| &b.insts)
                .any(|inst| matches!(
                    inst,
                    MInst::Binary {
                        op: BinOp::Add | BinOp::Sub,
                        ..
                    }
                )),
            "{bits} {predicate} {op} {operands}"
        );
    }
}
