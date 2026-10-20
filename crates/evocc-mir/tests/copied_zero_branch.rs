//! A private zero-extending copy must not force a second zero comparison.
use evocc_evoir::{parse_section9_module, validate_module};
use evocc_mir::{interpret_module, lower_module_for, MTerm, MirLimits, MirValue, Target};

fn source(op: &str, cast: &str) -> evocc_evoir::Module {
    let text = format!(
        "func @word(%arg0: bits<64>) -> (bits<64>) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}
{{
^entry(%v0: bits<64>):
    %v1 = trunc.bits<32> %v0
    br ^b1(%v1)

^b1(%v2: bits<32>):
    %v3 = {cast}.bits<64> %v2
    %v4 = const.bits<64> 0
    %v5 = icmp.{op} %v3, %v4
    cond_br %v5, ^b2(), ^b3()

^b2():
    %v6 = const.bits<64> 1
    ret %v6

^b3():
    %v7 = const.bits<64> 0
    ret %v7
}}\n"
    );
    let parsed = parse_section9_module(text.as_bytes()).unwrap();
    validate_module(&parsed).unwrap();
    parsed
}

#[test]
fn zero_extended_word_branches_test_the_original_word() {
    for target in [Target::Darwin, Target::Linux] {
        for op in ["eq", "ne"] {
            let machine = lower_module_for(target, &source(op, "zext"), &|_| false).unwrap();
            evocc_mir::validate_module(&machine).unwrap();
            for input in [0, 1, 0x80000000, u32::MAX as u64, 1 << 32, u64::MAX] {
                let nonzero = input as u32 != 0;
                assert_eq!(
                    interpret_module(
                        &machine,
                        "word",
                        &[MirValue::Int(input)],
                        MirLimits::new(100)
                    )
                    .unwrap()
                    .results,
                    [MirValue::Int(u64::from(if op == "ne" {
                        nonzero
                    } else {
                        !nonzero
                    }))]
                );
            }
            assert!(
                machine.functions[0]
                    .blocks
                    .iter()
                    .any(|b| matches!(b.term, MTerm::Cb { .. })),
                "the private copy and zero comparison should select CBZ/CBNZ: {machine:#?}"
            );
        }
    }
}

#[test]
fn a_signed_ordering_keeps_its_extension_and_flags() {
    for target in [Target::Darwin, Target::Linux] {
        let machine = lower_module_for(target, &source("slt", "sext"), &|_| false).unwrap();
        evocc_mir::validate_module(&machine).unwrap();
        for input in [0, 1, 0x80000000, u32::MAX as u64, 1 << 32, u64::MAX] {
            assert_eq!(
                interpret_module(
                    &machine,
                    "word",
                    &[MirValue::Int(input)],
                    MirLimits::new(100)
                )
                .unwrap()
                .results,
                [MirValue::Int(u64::from((input as i32) < 0))]
            );
        }
        assert!(!machine.functions[0]
            .blocks
            .iter()
            .any(|b| matches!(b.term, MTerm::Cb { .. })));
    }
}
