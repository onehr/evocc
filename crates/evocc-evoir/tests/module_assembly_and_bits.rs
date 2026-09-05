use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, InterpreterLimits, RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

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
        let text = format!("func @f(%arg0: bits<{width}>, %arg1: mem<all>, %arg2: world) -> (bits<{width}>, mem<all>, world) cc = target_default\n    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }}\n{{\n^entry(%v0: bits<{width}>, %v1: mem<all>, %v2: world):\n    %v3 = {op}.bits<{width}> %v0\n    ret %v3, %v1, %v2\n}}\n");
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
