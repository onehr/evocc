use std::collections::BTreeMap;

use evocc_evoir::{parse_section9_module, render_module, Module, RuntimeValue};
use evocc_passes::preflight::{inspect, SourcePermission};
use evocc_passes::ExecutionInput;
use evocc_types::EvoIRType;

const EFFECTS: &str = "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }";

#[path = "../examples/support/source_inventory.rs"]
mod source_inventory;

fn edge_module(second_edge_wrong: bool) -> Module {
    let second = if second_edge_wrong {
        "%v1, %v1, %v3"
    } else {
        "%v0, %v1, %v3"
    };
    let text = format!("func @main(%arg0: bits<32>, %arg1: bits<32>, %arg2: bits<1>) -> (bits<32>) cc = target_default\n{EFFECTS}\n{{\n^entry(%v0: bits<32>, %v1: bits<32>, %v2: bits<1>):\n    %v3 = mul.bits<32> %v0, %v1\n    cond_br %v2, ^b1(%v0, %v1, %v3), ^b1({second})\n\n^b1(%v4: bits<32>, %v5: bits<32>, %v6: bits<32>):\n    %v7 = mul.bits<32> %v4, %v5\n    %v8 = add.bits<32> %v6, %v7\n    ret %v8\n}}\n");
    parse_section9_module(text.as_bytes()).unwrap()
}

fn inline_module() -> Module {
    let text = format!("func @helper(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default visibility=internal\n{EFFECTS}\n{{\n^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):\n    %v3 = add.bits<32> %v0, %v0\n    ret %v3, %v1, %v2\n}}\n\nfunc @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default\n{EFFECTS}\n{{\n^entry(%v0: mem<all>, %v1: world):\n    %v2 = const.bits<32> 7\n    %v3 = const.bits<32> 9\n    (%v4, %v5, %v6) = call @helper(%v2, %v0, %v1)\n    ret %v4, %v5, %v6\n}}\n");
    parse_section9_module(text.as_bytes()).unwrap()
}

#[test]
fn read_only_preflight_finds_residual_edge_reuse_without_changing_bytes() {
    let module = edge_module(false);
    let before = render_module(&module);
    let inputs = [ExecutionInput::main(
        "edge",
        vec![
            RuntimeValue::new(EvoIRType::Bits(32), 3),
            RuntimeValue::new(EvoIRType::Bits(32), 5),
            RuntimeValue::new(EvoIRType::Bits(1), 1),
        ],
        100,
    )];
    let report = inspect(&module, &BTreeMap::new(), &inputs).unwrap();
    assert_eq!(report.edge_reuse.structural_sites, 1);
    assert_eq!(report.edge_reuse.admitted_sites, 1);
    assert_eq!(report.executed_inputs, 1);
    assert_eq!(before, render_module(&module));
}

#[test]
fn each_edge_occurrence_must_match_even_with_one_predecessor_block() {
    let report = inspect(&edge_module(true), &BTreeMap::new(), &[]).unwrap();
    assert_eq!(report.edge_reuse.structural_sites, 0);
    assert!(report
        .edge_reuse
        .declined
        .get("incoming-edge-mismatch")
        .is_some_and(|n| *n > 0));
}

#[test]
fn inline_shape_does_not_invent_source_permission_or_execution_inputs() {
    let module = inline_module();
    let missing = inspect(&module, &BTreeMap::new(), &[]).unwrap();
    assert_eq!(missing.inline.candidates, 1);
    assert_eq!(
        missing.inline.declined.get("source-policy-unknown"),
        Some(&1)
    );
    let allowed = BTreeMap::from([("helper".into(), SourcePermission::Allowed)]);
    let report = inspect(&module, &allowed, &[]).unwrap();
    assert_eq!(report.inline.structural_sites, 1);
    assert_eq!(report.inline.admitted_sites, 0);
    assert_eq!(report.module_admission, "NO_EXECUTION_INPUTS");
    let barrier = BTreeMap::from([("helper".into(), SourcePermission::Barrier)]);
    let report = inspect(&module, &barrier, &[]).unwrap();
    assert_eq!(report.inline.declined.get("source-barrier"), Some(&1));
}

#[test]
fn opaque_module_counts_structure_but_never_tv_admission() {
    let mut module = edge_module(false);
    module.assembly.push("nop".into());
    let report = inspect(&module, &BTreeMap::new(), &[]).unwrap();
    assert_eq!(report.edge_reuse.structural_sites, 1);
    assert_eq!(report.edge_reuse.admitted_sites, 0);
    assert_eq!(report.module_admission, "OPAQUE");
}

#[test]
fn source_barriers_on_earlier_or_later_prototypes_are_preserved() {
    for barrier in ["noinline", "noipa", "noclone"] {
        for late in [false, true] {
            let prototype = format!("static int helper(int) __attribute__(({barrier}));");
            let definition =
                "static int helper(int x){return x+x;} int main(void){return helper(7);}";
            let source = if late {
                format!("{definition}{prototype}")
            } else {
                format!("{prototype}{definition}")
            };
            let (_, permissions) = source_inventory::lower_preprocessed(&source).unwrap();
            assert_eq!(
                permissions.get("helper"),
                Some(&SourcePermission::Barrier),
                "{source}"
            );
        }
    }
}

#[test]
fn explicit_retention_on_any_prototype_blocks_new_inline_permission() {
    for retention in ["used", "retain"] {
        for late in [false, true] {
            let prototype = format!("static int helper(int) __attribute__(({retention}));");
            let definition =
                "static int helper(int x){return x+x;} int main(void){return helper(7);}";
            let source = if late {
                format!("{definition}{prototype}")
            } else {
                format!("{prototype}{definition}")
            };
            let (_, permissions) = source_inventory::lower_preprocessed(&source).unwrap();
            assert_eq!(
                permissions.get("helper"),
                Some(&SourcePermission::Barrier),
                "{source}"
            );
        }
    }
}

#[test]
fn poisoned_modules_do_not_erase_call_consumption_boundaries() {
    let mut module = inline_module();
    let evocc_evoir::Inst::Const(value, _) = module.functions[1].blocks[0].insts[0].clone() else {
        panic!()
    };
    module.functions[1].blocks[0].insts[0] = evocc_evoir::Inst::Uninitialized(value);
    let report = inspect(
        &module,
        &BTreeMap::from([("helper".into(), SourcePermission::Allowed)]),
        &[],
    )
    .unwrap();
    assert_eq!(report.inline.structural_sites, 0);
    assert_eq!(report.inline.declined.get("module-poison-source"), Some(&1));
}

#[test]
fn edge_value_must_be_a_producer_not_poison_or_opaque_parameter() {
    let mut module = edge_module(false);
    let evocc_evoir::Inst::Mul(value, _, _) = module.functions[0].blocks[0].insts[0].clone() else {
        panic!()
    };
    module.functions[0].blocks[0].insts[0] = evocc_evoir::Inst::Uninitialized(value);
    let report = inspect(&module, &BTreeMap::new(), &[]).unwrap();
    assert_eq!(report.edge_reuse.structural_sites, 0);
    assert_eq!(
        report.edge_reuse.declined.get("incoming-edge-mismatch"),
        Some(&1)
    );
}

#[test]
fn self_edge_is_not_an_acyclic_opportunity() {
    let mut module = edge_module(false);
    let block = &mut module.functions[0].blocks[1];
    block.terminator = evocc_evoir::Inst::Br(
        block.id,
        block.args.iter().map(|p| p.value.clone()).collect(),
    );
    let report = inspect(&module, &BTreeMap::new(), &[]).unwrap();
    assert_eq!(report.edge_reuse.structural_sites, 0);
    assert_eq!(
        report.edge_reuse.declined.get("cyclic-destination"),
        Some(&2)
    );
}

#[test]
fn census_denominators_and_machine_cost_absence_are_explicit() {
    let report = inspect(&edge_module(false), &BTreeMap::new(), &[]).unwrap();
    for counts in [&report.inline, &report.edge_reuse] {
        assert_eq!(
            counts.candidates,
            counts.structural_sites + counts.declined.values().sum::<u64>()
        );
    }
    let json = report.render_json();
    assert!(json.contains("\"transforms\":0"));
    assert!(json.contains("NO_EXECUTION_INPUTS"));
    assert!(!json.contains("actual_bytes"));
}

#[test]
fn malformed_duplicate_numeric_value_ids_are_rejected_before_census() {
    let mut module = edge_module(false);
    let block = &mut module.functions[0].blocks[1];
    let evocc_evoir::Inst::Mul(root, _, _) = &mut block.insts[0] else {
        panic!()
    };
    // Local operand scope does not relax the existing function-wide numeric
    // ID uniqueness rule. Do not turn this malformed input into a census row.
    root.id = 3;
    let evocc_evoir::Inst::Add(_, _, reused) = &mut block.insts[1] else {
        panic!()
    };
    reused.id = 3;
    let error = inspect(&module, &BTreeMap::new(), &[]).unwrap_err();
    assert!(
        matches!(error, evocc_passes::ValidationError::InvalidInput(message) if message.contains("defined more than once"))
    );
}
