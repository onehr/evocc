//! EvoMIR Section 5, revision 0.17: the two activation-address rows.
//!
//! "`return_address` | `ldr rd, [x29, #8]`: the saved link register of this
//! frame record, valid anywhere in the body because every function keeps a
//! frame pointer (revision 0.17)" and "`frame_address` | `mov rd, x29`
//! (revision 0.17)".
//!
//! EvoIR 1.9.72 gives the two forms their meaning, and Section 10 withdraws
//! `E4007` with them: level zero of both builtins now has a machine form, so
//! neither is a refusal any more.

use evocc_evoir::{interpret_module, parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{
    compile_module_for, interpret_module as interpret_machine_module, lower_module_for, FramePlace,
    MInst, MirLimits, MirValue, Target,
};
use evocc_types::{EvoIRType, Region};

/// One activation reads each address twice, compares the readings, and reports
/// the two facts a program may learn about them: neither is null, and two
/// readings inside one activation agree. The addresses themselves never leave.
const UNIT: &str = "\
func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = return_address : ptr<generic>
    %v3 = return_address : ptr<generic>
    %v4 = frame_address : ptr<generic>
    %v5 = frame_address : ptr<generic>
    %v6 = ptr.to_int %v2
    %v7 = ptr.to_int %v3
    %v8 = ptr.to_int %v4
    %v9 = ptr.to_int %v5
    %v10 = const.bits<64> 0
    %v11 = icmp.ne %v6, %v10
    %v12 = icmp.ne %v8, %v10
    %v13 = icmp.eq %v6, %v7
    %v14 = icmp.eq %v8, %v9
    %v15 = zext.bits<32> %v11
    %v16 = zext.bits<32> %v12
    %v17 = zext.bits<32> %v13
    %v18 = zext.bits<32> %v14
    %v19 = add.bits<32> %v15, %v16
    %v20 = add.bits<32> %v19, %v17
    %v21 = add.bits<32> %v20, %v18
    ret %v21, %v0, %v1
}
";

fn emitted(target: Target) -> String {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    match compile_module_for(target, &module, &|_| true) {
        Ok(assembly) => assembly,
        Err(stop) => panic!(
            "the unit stopped at {}: {}",
            stop.component(),
            stop.detail()
        ),
    }
}

#[test]
fn each_form_lowers_to_one_frame_place_and_nothing_else() {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    let lowered = lower_module_for(Target::Darwin, &module, &|_| false)
        .expect("both forms are in the Section 5 table");
    let insts = &lowered.functions[0].blocks[0].insts;
    for (place, expected) in [
        (FramePlace::SavedLinkRegister, 2),
        (FramePlace::FramePointer, 2),
    ] {
        let count = insts
            .iter()
            .filter(|inst| matches!(inst, MInst::FrameAddress { place: p, .. } if *p == place))
            .count();
        assert_eq!(
            count,
            expected,
            "each reading is one {} place: {insts:?}",
            place.mnemonic()
        );
    }
}

/// The lines of one emitted function body.
fn body(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

#[test]
fn the_return_address_is_the_saved_link_register_of_this_frame_record() {
    for target in [Target::Darwin, Target::Linux] {
        let text = emitted(target);
        let lines = body(&text);
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.starts_with("ldr ") && line.ends_with(", [x29, #8]"))
                .count(),
            2,
            "each return_address is one ldr from the frame record:\n{text}"
        );
        assert!(
            lines.iter().any(|line| line == "stp x29, x30, [sp, #-16]!"),
            "and the prologue is what puts it there:\n{text}"
        );
    }
}

#[test]
fn the_frame_address_is_the_frame_pointer() {
    for target in [Target::Darwin, Target::Linux] {
        let text = emitted(target);
        let moves = body(&text)
            .iter()
            .filter(|line| line.starts_with("mov ") && line.ends_with(", x29"))
            .count();
        assert_eq!(moves, 2, "each frame_address is one mov from x29:\n{text}");
    }
}

#[test]
fn neither_form_is_refused_any_more() {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    let lowered = lower_module_for(Target::Darwin, &module, &|_| false)
        .expect("E4007 is withdrawn at revision 0.17");
    assert_eq!(lowered.functions.len(), 1, "the whole module lowered");
}

#[test]
fn both_interpreters_answer_the_four_facts() {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    let arguments = vec![
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ];
    let outcome = interpret_module(&module, "main", &arguments, InterpreterLimits::new(100_000))
        .expect("the execution is defined");
    let evoir = outcome
        .return_values
        .iter()
        .find(|value| matches!(value.ty, EvoIRType::Bits(_)))
        .expect("one ordinary result")
        .raw_bits;
    assert_eq!(
        evoir, 4,
        "neither address is null and each pair of readings agrees"
    );

    let lowered = lower_module_for(Target::Darwin, &module, &|_| false).expect("lowering");
    let machine = interpret_machine_module(&lowered, "main", &[], MirLimits::new(100_000))
        .expect("the machine interpreter executes both rows");
    assert_eq!(
        machine.results.first(),
        Some(&MirValue::Int(4)),
        "and the machine interpreter says the same"
    );
}
