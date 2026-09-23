use evocc_evoir::parse_section9_module;
use evocc_mir::{MInst, MirLimits, MirValue};

fn text(bits: u16, cmp: &str, reverse: bool, local_zero: bool) -> String {
    let (a, b) = if reverse { (1, 0) } else { (0, 1) };
    let zero = format!("    %v6 = const.bits<{bits}> 0\n");
    format!(
        "func @main(%arg0: bits<{bits}>, %arg1: bits<{bits}>, %arg2: mem<all>, %arg3: world) -> (bits<{bits}>, mem<all>, world) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }}
{{
^entry(%v0: bits<{bits}>, %v1: bits<{bits}>, %v2: mem<all>, %v3: world):
{early}    %v4 = icmp.{cmp} %v0, %v1
    %v5 = zext.bits<{bits}> %v4
{late}    %v7 = sub.bits<{bits}> %v6, %v5
    %v8 = xor.bits<{bits}> %v{a}, %v{b}
    %v9 = and.bits<{bits}> %v8, %v7
    %v10 = xor.bits<{bits}> %v{b}, %v9
    ret %v10, %v2, %v3
}}
",
        early = if local_zero { "" } else { &zero },
        late = if local_zero { &zero } else { "" },
    )
}

fn lower(source: &str) -> evocc_mir::MModule {
    let ir = parse_section9_module(source.as_bytes()).unwrap();
    evocc_evoir::validate_module(&ir).unwrap();
    let mir = evocc_mir::lower_module(&ir).unwrap();
    evocc_mir::validate_module(&mir).unwrap();
    assert_eq!(
        evocc_mir::parse_module(&evocc_mir::render_module(&mir)).unwrap(),
        mir
    );
    for f in &mir.functions {
        let allocated = evocc_mir::allocate(&evocc_mir::out_of_ssa(f)).unwrap();
        evocc_mir::verify_allocation(&allocated).unwrap();
    }
    mir
}

fn selections(mir: &evocc_mir::MModule) -> usize {
    mir.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter(|i| matches!(i, MInst::Csel { .. }))
        .count()
}

#[test]
fn compared_bit_selections_use_one_adjacent_cmp_csel() {
    for bits in [32, 64] {
        for cmp in ["eq", "ne", "slt", "sle", "ult", "ule"] {
            for reverse in [false, true] {
                for local_zero in [false, true] {
                    let mir = lower(&text(bits, cmp, reverse, local_zero));
                    assert_eq!(selections(&mir), 1, "{bits} {cmp} reverse={reverse}");
                    let body = &mir.functions[0].blocks[0].insts;
                    assert!(body.windows(2).any(|pair| matches!(
                        pair,
                        [MInst::Cmp { dst, .. }, MInst::Csel { flags, .. }] if dst == flags
                    )));
                    assert!(!body.iter().any(|i| matches!(i, MInst::Cset { .. })));
                }
            }
        }
    }
}

#[test]
fn selection_preserves_signed_unsigned_and_full_width_patterns() {
    for bits in [32, 64] {
        let mask = u64::MAX >> (64 - bits);
        let sign = 1_u64 << (bits - 1);
        for cmp in ["eq", "ne", "slt", "sle", "ult", "ule"] {
            for reverse in [false, true] {
                let mir = lower(&text(bits, cmp, reverse, true));
                for a in [0, 1, sign - 1, sign, mask] {
                    for b in [0, 1, sign - 1, sign, mask] {
                        let signed = |n: u64| (n << (64 - bits)) as i64;
                        let condition = match cmp {
                            "eq" => a == b,
                            "ne" => a != b,
                            "slt" => signed(a) < signed(b),
                            "sle" => signed(a) <= signed(b),
                            "ult" => a < b,
                            "ule" => a <= b,
                            _ => unreachable!(),
                        };
                        let expected = if condition != reverse { a } else { b };
                        let outcome = evocc_mir::interpret_module(
                            &mir,
                            "main",
                            &[MirValue::Int(a), MirValue::Int(b)],
                            MirLimits::new(100),
                        )
                        .unwrap();
                        assert_eq!(outcome.results, [MirValue::Int(expected)]);
                    }
                }
            }
        }
    }
}

#[test]
fn fusion_rejects_nonzero_masks_changed_inputs_and_other_consumers() {
    let source = text(32, "slt", false, true);
    for changed in [
        source.replace("const.bits<32> 0", "const.bits<32> 1"),
        source.replace("sub.bits<32> %v6, %v5", "sub.bits<32> %v5, %v6"),
        source.replace("xor.bits<32> %v1, %v9", "xor.bits<32> %v0, %v9"),
        source.replace("ret %v10", "%v11 = add.bits<32> %v10, %v5\n    ret %v11"),
        source.replace("ret %v10", "%v11 = add.bits<32> %v10, %v6\n    ret %v11"),
        source.replace("%v5 = zext", "%v11 = const.bits<1> 1\n    %v5 = zext"),
        text(13, "slt", false, true),
        text(128, "slt", false, true),
    ] {
        assert_eq!(selections(&lower(&changed)), 0, "{changed}");
    }
}
