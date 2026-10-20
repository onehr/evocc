//! Machine facts must describe every incoming edge, not just the first one.
use evocc_evoir::{parse_section9_module, validate_module};
use evocc_mir::{interpret_module, lower_module_for, BinOp, MInst, MirLimits, MirValue, Target};

fn module(body: &str) -> evocc_evoir::Module {
    let text = format!(
        "func @word(%arg0: bits<64>) -> (bits<64>) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}
{{
^entry(%v0: bits<64>):
{body}
}}\n"
    );
    let parsed = parse_section9_module(text.as_bytes()).unwrap();
    validate_module(&parsed).unwrap();
    parsed
}

#[test]
fn a_boolean_stays_zero_extended_across_a_join_and_a_loop() {
    let source = module(
        "    %v1 = const.bits<64> 0
    %v2 = icmp.eq %v0, %v1
    %v3 = zext.bits<32> %v2
    br ^b1(%v3)

^b1(%v4: bits<32>):
    %v5 = sext.bits<64> %v4
    %v6 = const.bits<64> 0
    %v7 = icmp.eq %v5, %v6
    %v8 = const.bits<32> 1
    cond_br %v7, ^b1(%v8), ^b2(%v5)

^b2(%v9: bits<64>):
    ret %v9",
    );
    for target in [Target::Darwin, Target::Linux] {
        let machine = lower_module_for(target, &source, &|_| false).unwrap();
        evocc_mir::validate_module(&machine).unwrap();
        for input in [0, 1, u64::MAX] {
            assert_eq!(
                interpret_module(
                    &machine,
                    "word",
                    &[MirValue::Int(input)],
                    MirLimits::new(100)
                )
                .unwrap()
                .results,
                [MirValue::Int(1)]
            );
        }
        assert!(
            !machine.functions[0]
                .blocks
                .iter()
                .flat_map(|b| &b.insts)
                .any(|i| matches!(
                    i,
                    MInst::Extend { .. } | MInst::BinaryImm { op: BinOp::And, .. }
                )),
            "boolean masks and the signed extension of a nonnegative word are redundant"
        );
    }
}

#[test]
fn a_join_with_one_negative_input_keeps_the_sign_extension() {
    let source = module(
        "    %v1 = const.bits<64> 0
    %v2 = icmp.eq %v0, %v1
    %v3 = const.bits<32> 1
    %v4 = const.bits<32> 2147483648
    cond_br %v2, ^b1(%v3), ^b1(%v4)

^b1(%v5: bits<32>):
    %v6 = sext.bits<64> %v5
    ret %v6",
    );
    for target in [Target::Darwin, Target::Linux] {
        let machine = lower_module_for(target, &source, &|_| false).unwrap();
        assert!(machine.functions[0]
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .any(|i| matches!(i, MInst::Extend { .. })));
        for (input, expected) in [(0, 1), (1, (-2147483648_i64) as u64)] {
            assert_eq!(
                interpret_module(
                    &machine,
                    "word",
                    &[MirValue::Int(input)],
                    MirLimits::new(100)
                )
                .unwrap()
                .results,
                [MirValue::Int(expected)]
            );
        }
    }
}

#[test]
fn truncation_and_extension_keep_every_source_sign_boundary() {
    // A Boolean sign-extended directly from bits<1> is -1, not +1.
    // Also exercise arbitrary upper bits and the W/X transition at 32 bits.
    for bits in 1..64 {
        let mask = (1_u64 << bits) - 1;
        let sign = 1_u64 << (bits - 1);
        for signed in [false, true] {
            let op = if signed { "sext" } else { "zext" };
            let source = module(&format!(
                "    %v1 = trunc.bits<{bits}> %v0\n    br ^b1(%v1)\n\n^b1(%v2: bits<{bits}>):\n    %v3 = {op}.bits<64> %v2\n    ret %v3"
            ));
            for target in [Target::Darwin, Target::Linux] {
                let machine = lower_module_for(target, &source, &|_| false).unwrap();
                evocc_mir::validate_module(&machine).unwrap();
                for input in [0, 1, sign - 1, sign, mask, !mask, u64::MAX] {
                    let raw = input & mask;
                    let expected = if signed && raw & sign != 0 {
                        raw | !mask
                    } else {
                        raw
                    };
                    assert_eq!(
                        interpret_module(
                            &machine,
                            "word",
                            &[MirValue::Int(input)],
                            MirLimits::new(100)
                        )
                        .unwrap()
                        .results,
                        [MirValue::Int(expected)],
                        "{bits} {op} {input}"
                    );
                }
            }
        }
    }
}
