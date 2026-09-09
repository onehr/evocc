//! Pointer poison is typed ordinary data until an operation consumes its provenance.

use evocc_evoir::{
    parse_section9_module, render_module, validate_ssa, Block, Function, Inst, Module, Value,
};
use evocc_types::{AddressSpace, EvoIRType, Region};

fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}

#[test]
fn pointer_poison_validates_and_round_trips_in_both_pointer_namespaces() {
    let object_pointer = value(0, EvoIRType::Ptr(AddressSpace::Generic));
    let function_pointer = value(
        1,
        EvoIRType::FnPtr {
            variadic: false,
            params: vec![
                EvoIRType::Bits(32),
                EvoIRType::Mem(Region::All),
                EvoIRType::World,
            ],
            results: vec![
                EvoIRType::Bits(32),
                EvoIRType::Mem(Region::All),
                EvoIRType::World,
            ],
            marks: Vec::new(),
        },
    );
    let function = Function {
        alignment: 4,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
        name: "poison_forms".to_owned(),
        params: vec![],
        ret_tys: vec![],
        effects: Default::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![],
            insts: vec![
                Inst::Uninitialized(object_pointer),
                Inst::Uninitialized(function_pointer),
            ],
            terminator: Inst::Ret(vec![]),
        }],
        param_marks: Vec::new(),
    };
    validate_ssa(&function).expect("pointer poison must be an ordinary typed definition");

    let module = Module {
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![],
        functions: vec![function],
    };
    let text = render_module(&module);
    assert!(text.contains("%v0 = uninitialized.ptr<generic>"));
    assert!(text.contains(concat!(
        "%v1 = uninitialized.fnptr<(bits<32>, mem<all>, world)",
        "->(bits<32>, mem<all>, world)>"
    )));
    let reparsed = parse_section9_module(text.as_bytes()).expect("pointer poison text must parse");
    assert_eq!(render_module(&reparsed), text);
}
