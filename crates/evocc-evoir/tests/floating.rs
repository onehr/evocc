use evocc_evoir::{interpret_module, parse_section9_module, render_module, InterpreterLimits};

const NUMBERS: &str = "func @main() -> (bits<64>) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n{\n^entry():\n    %v0 = const.f64 0x7ff8000000000011\n    %v1 = const.f64 0xfff0000000000022\n    %v2 = fp.add %v0, %v1 : f64\n    %v3 = fp.to_bits %v2 : bits<64>\n    ret %v3\n}\n";

#[test]
fn floating_text_and_interpreter_preserve_nan_operand_order() {
    let module = parse_section9_module(NUMBERS.as_bytes()).unwrap();
    assert_eq!(render_module(&module), NUMBERS);
    let result = interpret_module(&module, "main", &[], InterpreterLimits::new(100)).unwrap();
    assert_eq!(result.return_values[0].value(), 0xfff8000000000022);
}

#[test]
fn malformed_floating_operations_do_not_pass_structural_validation() {
    for text in [
        NUMBERS.replace(
            "const.f64 0x7ff8000000000011",
            "const.f32 0x7ff8000000000011",
        ),
        NUMBERS.replace("const.f64 0x7ff8000000000011", "const.f32 0x7fc00011"),
        NUMBERS.replace("fp.add %v0, %v1 : f64", "fp.add %v0 : f64"),
        NUMBERS.replace("fp.add %v0, %v1 : f64", "fp.oeq %v0, %v1 : f64"),
        NUMBERS.replace("fp.to_bits %v2 : bits<64>", "fp.to_bits %v2 : bits<32>"),
        NUMBERS.replace(
            "fp.to_bits %v2 : bits<64>",
            "fp.to_int_defined.s129 %v2 : bits<1>",
        ),
    ] {
        if let Ok(module) = parse_section9_module(text.as_bytes()) {
            assert!(evocc_evoir::validate_module(&module).is_err(), "{text}");
        }
    }
}

#[test]
fn hfa_abi_alignment_roundtrips_independently_of_object_alignment() {
    let base = NUMBERS
        .replace(
            "@main()",
            "@main(%arg0: ptr<generic> byval(32, 32, hfa=4xf64))",
        )
        .replace("^entry()", "^entry(%v4: ptr<generic>)");
    let ordinary = parse_section9_module(base.as_bytes()).unwrap();
    assert_eq!(render_module(&ordinary), base);
    let aligned = base.replace("hfa=4xf64", "hfa=4xf64, abi_align=16");
    let module = parse_section9_module(aligned.as_bytes()).unwrap();
    assert_ne!(ordinary, module);
    assert_eq!(render_module(&module), aligned);
    for invalid in [0, 4, 24, 32, 256] {
        let text = aligned.replace("abi_align=16", &format!("abi_align={invalid}"));
        assert!(parse_section9_module(text.as_bytes()).is_err());
    }
    assert!(parse_section9_module(aligned.replace("32, 32,", "32, 8,").as_bytes()).is_err());
}
