//! Constant conversions use the operand's bit width, not its register width.
use evocc_evoir::{parse_section9_module, validate_module};
use evocc_mir::{interpret_module, lower_module_for, MInst, MirLimits, MirValue, Target};

fn check(body: &str, expected: u64) {
    let text = format!(
        "func @word() -> (bits<64>) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}
{{
^entry():
{body}
}}
"
    );
    let source = parse_section9_module(text.as_bytes()).unwrap();
    validate_module(&source).unwrap();
    for target in [Target::Linux, Target::Darwin] {
        let machine = lower_module_for(target, &source, &|_| false).unwrap();
        evocc_mir::validate_module(&machine).unwrap();
        let result = interpret_module(&machine, "word", &[], MirLimits::new(100)).unwrap();
        assert_eq!(result.results, [MirValue::Int(expected)], "{text}");
        assert!(
            machine.functions[0].blocks[0]
                .insts
                .iter()
                .all(|inst| matches!(inst, MInst::MovImm { .. })),
            "a literal conversion needs only constant materialization: {text}"
        );
    }
}

#[test]
fn constant_extensions_cover_every_narrow_width_and_sign_boundary() {
    for width in 1..64 {
        let sign = 1_u64 << (width - 1);
        let mask = (1_u64 << width) - 1;
        for raw in [0, 1, sign - 1, sign, mask] {
            for signed in [false, true] {
                let op = if signed { "sext" } else { "zext" };
                let expected = if signed && raw & sign != 0 {
                    raw | !mask
                } else {
                    raw
                };
                check(
                    &format!("    %v0 = const.bits<{width}> {raw}\n    %v1 = {op}.bits<64> %v0\n    ret %v1"),
                    expected,
                );
            }
        }
    }
}

#[test]
fn constant_truncations_discard_high_bits_before_a_later_extension() {
    for width in 1..64 {
        let mask = (1_u64 << width) - 1;
        for raw in [0, u64::MAX, 1_u64 << width, 0x8000_0000_0000_0001] {
            check(
                &format!("    %v0 = const.bits<64> {raw}\n    %v1 = trunc.bits<{width}> %v0\n    %v2 = zext.bits<64> %v1\n    ret %v2"),
                raw & mask,
            );
        }
    }
}

#[test]
fn dynamic_parameters_are_not_literals_even_with_a_constant_predecessor() {
    let source = parse_section9_module(b"func @word() -> (bits<64>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry():
    %v0 = const.bits<8> 128
    br ^b1(%v0)

^b1(%v1: bits<8>):
    %v2 = sext.bits<64> %v1
    ret %v2
}
").unwrap();
    let machine = lower_module_for(Target::Linux, &source, &|_| false).unwrap();
    assert!(machine.functions[0].blocks[1]
        .insts
        .iter()
        .any(|inst| matches!(inst, MInst::Extend { .. })));
    assert_eq!(
        interpret_module(&machine, "word", &[], MirLimits::new(100))
            .unwrap()
            .results,
        [MirValue::Int((-128_i64) as u64)]
    );
}
