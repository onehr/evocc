use evocc_evoir::{parse_section9_module, FloatingOp};
use evocc_mir::{
    allocate, interpret_module, lower_module, out_of_ssa, parse_module, render_module,
    verify_allocation, MInst, MirLimits, MirValue, RegClass,
};

const CYCLE: &str = "func @cycle() -> (bits<64>) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n{\n^entry():\n    %v0 = const.f64 0x3ff0000000000000\n    %v1 = const.f64 0x4000000000000000\n    %v2 = const.bits<32> 5\n    br ^b1(%v0, %v1, %v2)\n\n^b1(%v3: f64, %v4: f64, %v5: bits<32>):\n    %v6 = const.bits<32> 0\n    %v7 = const.bits<32> 1\n    %v8 = sub.bits<32> %v5, %v7\n    %v9 = icmp.ne %v5, %v6\n    cond_br %v9, ^b1(%v4, %v3, %v8), ^b2(%v3, %v4)\n\n^b2(%v10: f64, %v11: f64):\n    %v12 = fp.sub %v10, %v11 : f64\n    %v13 = fp.to_bits %v12 : bits<64>\n    ret %v13\n}\n";

#[test]
fn floating_copies_text_and_interpreter_preserve_cycles_and_register_classes() {
    let ir = parse_section9_module(CYCLE.as_bytes()).unwrap();
    let machine = lower_module(&ir).unwrap();
    let text = render_module(&machine);
    assert_eq!(parse_module(&text).unwrap(), machine);
    let out = interpret_module(&machine, "cycle", &[], MirLimits::new(1000)).unwrap();
    assert_eq!(out.results, vec![MirValue::Int(0x3ff0000000000000)]);
    let function = out_of_ssa(&machine.functions[0]);
    let allocated = allocate(&function).unwrap();
    verify_allocation(&allocated).unwrap();
    let mut forged = machine.clone();
    for block in &mut forged.functions[0].blocks {
        for inst in &mut block.insts {
            if let MInst::Floating {
                op: FloatingOp::Sub,
                dst,
                ..
            } = inst
            {
                dst.class = RegClass::Gpr64;
            }
        }
    }
    assert!(evocc_mir::validate_module(&forged).is_err());
}
