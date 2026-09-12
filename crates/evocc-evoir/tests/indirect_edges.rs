//! Indirect transfer retains ordinary local SSA and alternative effect edges.

use evocc_evoir::{
    interpret_function, interpret_module, parse_section9_module, render_module, validate_module,
    InterpreterLimits, RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

const COUNTER: &str = include_str!("../../../tests/fixtures/computed-counter.evoir");

#[test]
fn indirect_edges_round_trip_and_bind_the_selected_objects_and_tokens() {
    let module = parse_section9_module(COUNTER.as_bytes()).expect("indirect edge syntax");
    validate_module(&module).unwrap();
    assert_eq!(render_module(&module), COUNTER);
    let args = [
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ];
    let limits = InterpreterLimits::new(1_000);
    assert_eq!(
        interpret_module(&module, "computed_counter", &args, limits)
            .unwrap()
            .return_values[0]
            .raw_bits,
        3
    );
    assert_eq!(
        interpret_function(&module.functions[0], &args, limits)
            .unwrap()
            .return_values[0]
            .raw_bits,
        3
    );
}

#[test]
fn indirect_edges_do_not_weaken_local_use_def_or_token_binding() {
    parse_section9_module(COUNTER.as_bytes()).expect("positive control must parse first");
    for (from, to) in [
        ("^b1(%v26, %v27, %v28)", "^b1(%v2, %v27, %v28)"),
        ("^b1(%v26, %v27, %v28)", "^b1(%v26, %v27)"),
        ("^b1(%v26, %v27, %v28)", "^b1(%v27, %v26, %v28)"),
        ("^b1(%v26, %v27, %v28)", "^b1(%v26, %v27, %v27)"),
        (", ^b3(%v26, %v27, %v28)", ""),
        ("^b3(%v26, %v27, %v28)]", "^b1(%v26, %v27, %v28)]"),
        ("br.indirect %v25", "br.indirect %v27"),
    ] {
        let invalid = COUNTER.replace(from, to);
        assert_ne!(invalid, COUNTER, "mutation must change its input");
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn a_null_indirect_target_is_a_branch_error_not_a_call_error() {
    let source = COUNTER.replace(
        "%v24 = block_addr ^b3 : ptr<generic>",
        "%v24 = ptr.null : ptr<generic>",
    );
    // Retain the second address-taken destination independently of the bad
    // runtime target. The failure must not be an incomplete-CFG diagnostic.
    let source = source.replace(
        "    br.indirect %v25",
        "    %v36 = block_addr ^b3 : ptr<generic>\n    br.indirect %v25",
    );
    let module = parse_section9_module(source.as_bytes()).expect("well-formed null branch");
    let args = [
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ];
    assert_eq!(
        interpret_module(
            &module,
            "computed_counter",
            &args,
            InterpreterLimits::new(1_000)
        )
        .unwrap_err()
        .code(),
        "E3048"
    );
}

#[test]
fn a_legacy_explicit_signed_shift_obligation_is_still_executable() {
    let source = "func @legacy(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }\n{\n^entry(%v0: mem<all>, %v1: world):\n    %v2 = const.bits<32> 4294967295\n    %v3 = const.bits<64> 1\n    %v4 = signed_shl_defined.signed %v2, %v3\n    ub.assume.invalid_signed_left_shift %v4\n    %v5 = shl.bits<32> %v2, %v3\n    ret %v5, %v0, %v1\n}\n";
    let module = parse_section9_module(source.as_bytes()).unwrap();
    let args = [
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ];
    assert_eq!(
        interpret_module(&module, "legacy", &args, InterpreterLimits::new(100))
            .unwrap_err()
            .code(),
        "E3006"
    );
}

#[test]
fn relocation_only_destinations_and_current_tokens_are_validated() {
    let mut module = parse_section9_module(COUNTER.as_bytes()).unwrap();
    let f = &mut module.functions[0];
    let label = f.blocks[1]
        .insts
        .iter()
        .find_map(|i| {
            if let evocc_evoir::Inst::BlockAddr { result, block: 3 } = i {
                Some(result.clone())
            } else {
                None
            }
        })
        .unwrap();
    // A runtime null value does not make the second label disappear: it
    // remains address-taken solely by the static table's relocation.
    for i in &mut f.blocks[1].insts {
        if matches!(i, evocc_evoir::Inst::BlockAddr { block: 3, .. }) {
            *i = evocc_evoir::Inst::PtrNull(label.clone());
        }
    }
    let text = format!("global @table origin=100 kind=file visibility=internal size=8 align=8 bytes=0000000000000000 initialized=11111111 relocs=[(0, @computed_counter^b3, 0)]\n\n{}", render_module(&module));
    let mut module = parse_section9_module(text.as_bytes()).unwrap();
    // Retain an ordinary path to b3, so removing its indirect edge tests the
    // relocation-derived set rather than just the reachability rule.
    if let evocc_evoir::Inst::CondBr(_, _, _, target, args) =
        &mut module.functions[0].blocks[1].terminator
    {
        *target = 3;
        args.remove(0);
    }
    validate_module(&module).unwrap();
    if let evocc_evoir::Inst::IndirectBr { edges, .. } =
        &mut module.functions[0].blocks[2].terminator
    {
        edges.pop();
    }
    assert!(validate_module(&module).unwrap_err().contains("successor"));
    let stale = COUNTER.replace(
        "    br.indirect %v25",
        "    %v36 = access_in_bounds %v27, %v26 [size=4, align=4]\n    br.indirect %v25",
    );
    assert!(parse_section9_module(stale.as_bytes())
        .unwrap_err()
        .to_string()
        .contains("token"));
}
