use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, InterpreterLimits, RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

#[test]
fn builtin_trap_round_trips_and_reports_only_on_its_executed_edge() {
    let text = "\
func @f(%arg0: bits<1>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<1>, %v1: mem<all>, %v2: world):
    cond_br %v0, ^b1(%v1, %v2), ^b2(%v1, %v2)

^b1(%v3: mem<all>, %v4: world):
    trap.builtin_trap

^b2(%v5: mem<all>, %v6: world):
    %v7 = const.bits<32> 7
    ret %v7, %v5, %v6
}
";
    let module = parse_section9_module(text.as_bytes()).expect("explicit typed trap parses");
    evocc_evoir::validate_module(&module).expect("trap remains a valid effectful terminator");
    assert_eq!(render_module(&module), text);
    for reached in [false, true] {
        let result = interpret_module(
            &module,
            "f",
            &[
                RuntimeValue::new(EvoIRType::Bits(1), u64::from(reached)),
                RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                RuntimeValue::new(EvoIRType::World, 0),
            ],
            InterpreterLimits::new(100),
        );
        if reached {
            let error = result.unwrap_err();
            assert_eq!(error.code(), "E3047");
            assert_eq!(error.message(), "builtin-trap-reached");
        } else {
            assert_eq!(result.unwrap().return_values[0].raw_bits, 7);
        }
    }
    assert!(parse_section9_module(
        text.replace("trap.builtin_trap", "trap.builtin_trap %v3")
            .as_bytes()
    )
    .is_err());
    let mut dishonest_effects = module;
    dishonest_effects.functions[0].effects.may_trap = false;
    assert!(evocc_evoir::validate_module(&dishonest_effects).is_err());
}

#[test]
fn module_assembly_round_trips_without_execution() {
    let text = "asm \".globl name\\x0a\"\n";
    let module = parse_section9_module(text.as_bytes()).expect("module assembly is admitted");
    assert_eq!(render_module(&module), text);
}

#[test]
fn bit_instructions_are_total_and_typed() {
    for (op, width, input, expected) in [
        ("ctlz", 32, 0, 32),
        ("cttz", 32, 0, 32),
        ("ctpop", 32, 0, 0),
        ("ctlz", 32, 1, 31),
        ("cttz", 32, 8, 3),
        ("ctpop", 64, u64::MAX, 64),
        ("bswap", 32, 0x11223344, 0x44332211),
        ("bswap", 16, 0x1234, 0x3412),
    ] {
        let text = format!("func @f(%arg0: bits<{width}>, %arg1: mem<all>, %arg2: world) -> (bits<{width}>, mem<all>, world) cc = target_default\n    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}\n{{\n^entry(%v0: bits<{width}>, %v1: mem<all>, %v2: world):\n    %v3 = {op}.bits<{width}> %v0\n    ret %v3, %v1, %v2\n}}\n");
        let module =
            parse_section9_module(text.as_bytes()).expect("total bit operation is admitted");
        assert_eq!(render_module(&module), text);
        let result = interpret_module(
            &module,
            "f",
            &[
                RuntimeValue::new(EvoIRType::Bits(width), input),
                RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                RuntimeValue::new(EvoIRType::World, 0),
            ],
            InterpreterLimits::new(1000),
        )
        .unwrap();
        assert_eq!(result.return_values[0].raw_bits, expected);
        if op == "bswap" {
            assert!(parse_section9_module(
                text.replace(&format!("bits<{width}>"), "bits<8>")
                    .as_bytes()
            )
            .is_err());
        }
    }
}
