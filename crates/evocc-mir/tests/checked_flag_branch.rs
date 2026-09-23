use evocc_evoir::parse_section9_module;
use evocc_mir::{Cond, MInst, MTerm, MirError, MirLimits, MirValue};

fn text(bits: u16, predicate: &str, before: &str, after: &str) -> String {
    format!(
        "func @main(%arg0: bits<{bits}>, %arg1: bits<{bits}>, %arg2: mem<all>, %arg3: world) -> (bits<{bits}>, mem<all>, world) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }}
{{
^entry(%v0: bits<{bits}>, %v1: bits<{bits}>, %v2: mem<all>, %v3: world):
    %v4 = {predicate} %v0, %v1
{before}    ub.assume.signed_overflow %v4
{after}    ret %v0, %v2, %v3
}}
"
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
    for function in &mir.functions {
        let allocated = evocc_mir::allocate(&evocc_mir::out_of_ssa(function)).unwrap();
        evocc_mir::verify_allocation(&allocated).unwrap();
    }
    mir
}

#[test]
fn a_single_use_check_branches_on_its_flags_without_materializing_a_boolean() {
    for bits in [3, 13, 32, 64, 128] {
        for predicate in [
            "signed_add_defined",
            "signed_sub_defined",
            "signed_mul_defined",
        ] {
            let mir = lower(&text(bits, predicate, "", ""));
            let entry = &mir.functions[0].blocks[0];
            if bits == 128 && predicate == "signed_mul_defined" {
                // The limb predicate ends in a Boolean AND, not one CSET.
                assert!(matches!(entry.term, MTerm::Tb { .. }));
                continue;
            }
            let MTerm::BCond {
                flags,
                taken,
                untaken,
                ..
            } = &entry.term
            else {
                panic!("{bits} {predicate}: a checked branch needlessly materializes its boolean");
            };
            assert!(!matches!(entry.insts.last(), Some(MInst::Cset { .. })));
            assert!(evocc_mir::inst_defs(entry.insts.last().unwrap()).contains(flags));
            let blocks = &mir.functions[0].blocks;
            assert!(matches!(
                blocks.iter().find(|b| b.id == *taken).unwrap().term,
                MTerm::Brk { imm: 3004 }
            ));
            assert!(matches!(
                blocks.iter().find(|b| b.id == *untaken).unwrap().term,
                MTerm::Ret { .. }
            ));
        }
    }
}

#[test]
fn all_comparison_conditions_keep_the_false_trap_and_true_value() {
    for bits in [32, 64] {
        let mask = u64::MAX >> (64 - bits);
        let sign = 1_u64 << (bits - 1);
        for (predicate, trap_condition) in [
            ("eq", Cond::Ne),
            ("ne", Cond::Eq),
            ("slt", Cond::Ge),
            ("sle", Cond::Gt),
            ("ult", Cond::Hs),
            ("ule", Cond::Hi),
        ] {
            let mir = lower(&text(bits, &format!("icmp.{predicate}"), "", ""));
            assert!(matches!(mir.functions[0].blocks[0].term,
                             MTerm::BCond { cond, .. } if cond == trap_condition));
            for a in [0, 1, sign - 1, sign, mask] {
                for b in [0, 1, sign - 1, sign, mask] {
                    let signed = |v: u64| (v << (64 - bits)) as i64;
                    let defined = match predicate {
                        "eq" => a == b,
                        "ne" => a != b,
                        "slt" => signed(a) < signed(b),
                        "sle" => signed(a) <= signed(b),
                        "ult" => a < b,
                        "ule" => a <= b,
                        _ => unreachable!(),
                    };
                    let result = evocc_mir::interpret_module(
                        &mir,
                        "main",
                        &[MirValue::Int(a), MirValue::Int(b)],
                        MirLimits::new(100),
                    );
                    if defined {
                        assert_eq!(result.unwrap().results, [MirValue::Int(a)]);
                    } else {
                        assert!(matches!(result, Err(MirError::Trap { imm: 3004 })));
                    }
                }
            }
        }
    }
}

#[test]
fn narrow_and_full_width_arithmetic_checks_still_trap_on_overflow() {
    for bits in [3, 13, 32, 64] {
        let mask = u64::MAX >> (64 - bits);
        let sign = 1_u64 << (bits - 1);
        let signed = |v: u64| ((v << (64 - bits)) as i64 >> (64 - bits)) as i128;
        for predicate in [
            "signed_add_defined",
            "signed_sub_defined",
            "signed_mul_defined",
        ] {
            let mir = lower(&text(bits, predicate, "", ""));
            for a in [0, 1, sign - 1, sign, mask] {
                for b in [0, 1, sign - 1, sign, mask] {
                    let value = match predicate {
                        "signed_add_defined" => signed(a) + signed(b),
                        "signed_sub_defined" => signed(a) - signed(b),
                        "signed_mul_defined" => signed(a) * signed(b),
                        _ => unreachable!(),
                    };
                    let defined = (-(sign as i128)..sign as i128).contains(&value);
                    let result = evocc_mir::interpret_module(
                        &mir,
                        "main",
                        &[MirValue::Int(a), MirValue::Int(b)],
                        MirLimits::new(100),
                    );
                    if defined {
                        assert_eq!(result.unwrap().results, [MirValue::Int(a)]);
                    } else {
                        assert!(matches!(result, Err(MirError::Trap { imm: 3004 })));
                    }
                }
            }
        }
    }
}

#[test]
fn nonadjacent_or_reused_conditions_keep_their_boolean() {
    for source in [
        text(32, "signed_add_defined", "    %v5 = const.bits<32> 7\n", ""),
        text(32, "signed_add_defined", "", "    %v5 = zext.bits<32> %v4\n"),
        text(64, "signed_sub_defined", "", "    ub.assume.signed_overflow %v4\n"),
        text(32, "icmp.eq", "", "").replace("ret %v0, %v2, %v3", "br ^b1(%v4, %v2, %v3)\n\n^b1(%v5: bits<1>, %v6: mem<all>, %v7: world):\n    %v8 = zext.bits<32> %v5\n    ret %v8, %v6, %v7"),
    ] {
        let mir = lower(&source);
        assert!(matches!(mir.functions[0].blocks[0].term, MTerm::Tb { .. }), "{source}");
        assert!(mir.functions[0].blocks[0].insts.iter().any(|i| matches!(i, MInst::Cset { .. })));
    }
}

#[test]
fn guarded_continuation_bindings_survive_register_pressure() {
    let mut before = String::new();
    let mut after = String::new();
    for i in 0..32 {
        let constant = 10 + 2 * i;
        let value = constant + 1;
        before += &format!("    %v{constant} = const.bits<64> {}\n    %v{value} = xor.bits<64> %v0, %v{constant}\n", i + 1);
        if i != 0 {
            let previous = if i == 1 { 11 } else { 200 + i - 1 };
            after += &format!("    %v{} = add.bits<64> %v{previous}, %v{value}\n", 200 + i);
        }
    }
    let source = text(64, "icmp.slt", "", &after)
        .replace("    %v4 =", &format!("{before}    %v4 ="))
        .replace("ret %v0,", "ret %v231,");
    let mir = lower(&source);
    assert!(matches!(
        mir.functions[0].blocks[0].term,
        MTerm::BCond { .. }
    ));
    let allocated = evocc_mir::allocate(&evocc_mir::out_of_ssa(&mir.functions[0])).unwrap();
    assert!(
        allocated.spilled > 0,
        "the control must actually exercise spills"
    );
    evocc_mir::verify_allocation(&allocated).unwrap();
    let result = evocc_mir::interpret_module(
        &mir,
        "main",
        &[MirValue::Int(7), MirValue::Int(8)],
        MirLimits::new(1000),
    )
    .unwrap();
    assert_eq!(
        result.results,
        [MirValue::Int((1..=32).map(|i| 7 ^ i).sum())]
    );
}
