//! EvoIR execution refuses bit widths that exceed its 64-bit value model.

use evocc_evoir::{interpret_module, Block, Function, Inst, InterpreterLimits, Module, Value};
use evocc_types::EvoIRType;

#[test]
fn evoir_refuses_executed_bits128_arithmetic_with_e3033() {
    let one = Value {
        id: 0,
        ty: EvoIRType::Bits(128),
    };
    let three = Value {
        id: 1,
        ty: EvoIRType::Bits(128),
    };
    let product = Value {
        id: 2,
        ty: EvoIRType::Bits(128),
    };
    let module = Module {
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![],
        functions: vec![Function {
            variadic: false,
            name: "main".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(128)],
            effects: Default::default(),
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(one.clone(), 1),
                    Inst::Const(three.clone(), 3),
                    Inst::Mul(product.clone(), one, three),
                ],
                terminator: Inst::Ret(vec![product]),
            }],
        }],
    };
    let error = interpret_module(&module, "main", &[], InterpreterLimits::new(100))
        .expect_err("bits<128> execution must fail closed until its value model exists");
    assert_eq!(error.code(), "E3033", "{error:?}");
    assert_eq!(error.message(), "not-executed: wide-integer", "{error:?}");
}
