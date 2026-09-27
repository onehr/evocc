//! Section 9, validation, and execution contract for the EvoIR object copy.

use std::panic::{catch_unwind, AssertUnwindSafe};

use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, InterpreterLimits, RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

fn module_text(result_types: &str, instructions: &str, returned: &str) -> String {
    format!(
        concat!(
            "func @main(%arg0: mem<all>, %arg1: world) -> ({result_types}) cc = target_default\n",
            "    effects {{ reads={{all}}, writes={{all}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }}\n",
            "{{\n",
            "^entry(%v0: mem<all>, %v1: world):\n",
            "{instructions}",
            "    ret {returned}\n",
            "}}\n",
        ),
        result_types = result_types,
        instructions = instructions,
        returned = returned,
    )
}

fn entry_tokens() -> Vec<RuntimeValue> {
    vec![
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ]
}

fn execute(text: &str) -> Result<evocc_evoir::InterpreterOutcome, String> {
    let module = parse_section9_module(text.as_bytes())?;
    interpret_module(
        &module,
        "main",
        &entry_tokens(),
        InterpreterLimits::new(200),
    )
    .map_err(|error| format!("{}: {}", error.code(), error.message()))
}

fn initialized_copy_text() -> String {
    module_text(
        "bits<32>, mem<all>, world",
        concat!(
            "    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=1, size=8, align=8]\n",
            "    (%v5, %v6, %v7) = alloca %v3, %v4 [origin=2, size=8, align=8]\n",
            "    %v8 = const.bits<32> 287454020\n",
            "    %v9 = access_in_bounds %v6, %v5 [size=4, align=4]\n",
            "    (%v10, %v11) = store %v9, %v7, %v5, %v8\n",
            "    (%v12, %v13) = copy %v10, %v11, %v2, %v5 [size=8, align=8]\n",
            "    %v14 = access_in_bounds %v12, %v2 [size=4, align=4]\n",
            "    (%v15, %v16, %v17) = load %v14, %v13, %v2 : bits<32>\n",
        ),
        "%v15, %v16, %v17",
    )
}

#[test]
fn copy_has_one_canonical_text_form_and_real_effects() {
    let text = initialized_copy_text();
    let module = parse_section9_module(text.as_bytes()).expect("copy syntax must parse");
    assert_eq!(render_module(&module), text);
    assert!(module.functions[0].effects.reads_all);
    assert!(module.functions[0].effects.writes_all);
    assert!(module.functions[0].effects.may_trap);

    let outcome = execute(&text).expect("disjoint copy must execute");
    assert_eq!(
        outcome.return_values[0],
        RuntimeValue::bits(32, 0x1122_3344)
    );
}

#[test]
fn volatile_representation_copy_is_exact_width_and_round_trips() {
    use evocc_memory::VolatileDirection::{Load, Store};
    for (spelling, directions) in [
        ("copy.volatile.read", vec![Load]),
        ("copy.volatile.write", vec![Store]),
        ("copy.volatile.readwrite", vec![Load, Store]),
    ] {
        let text = initialized_copy_text()
            .replace("= copy ", &format!("= {spelling} "))
            .replace("volatile=false", "volatile=true");
        let module =
            parse_section9_module(text.as_bytes()).expect("exact-width volatile copy parses");
        assert_eq!(render_module(&module), text);
        let outcome = execute(&text).expect("representation padding is not a scalar read");
        assert_eq!(outcome.return_values[0], RuntimeValue::bits(32, 0x11223344));
        assert_eq!(
            outcome
                .world
                .volatile_trace()
                .iter()
                .map(|event| event.direction)
                .collect::<Vec<_>>(),
            directions
        );
        assert!(outcome
            .world
            .volatile_trace()
            .iter()
            .all(|event| event.access.size_bytes == 8));
        for attributes in [
            "size=8, align=4",
            "size=16, align=8",
            "size=0, align=8",
            "size=3, align=8",
        ] {
            assert!(
                parse_section9_module(
                    text.replace("size=8, align=8]", &format!("{attributes}]"))
                        .as_bytes()
                )
                .is_err(),
                "{attributes}"
            );
        }
        assert!(
            parse_section9_module(text.replace("volatile=true", "volatile=false").as_bytes())
                .is_err()
        );
    }
}

#[test]
fn copy_transfers_initializedness_without_reading_padding() {
    let text = module_text(
        "bits<8>, mem<all>, world",
        concat!(
            "    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=1, size=8, align=8]\n",
            "    (%v5, %v6, %v7) = alloca %v3, %v4 [origin=2, size=8, align=8]\n",
            "    %v8 = const.bits<32> 7\n",
            "    %v9 = access_in_bounds %v6, %v5 [size=4, align=4]\n",
            "    (%v10, %v11) = store %v9, %v7, %v5, %v8\n",
            "    (%v12, %v13) = copy %v10, %v11, %v2, %v5 [size=8, align=8]\n",
            "    %v14 = const.bits<64> 4\n",
            "    %v15 = ptr.offset.add.unsigned %v2, %v14 [scale=1]\n",
            "    %v16 = pointer_valid %v12, %v15\n",
            "    %v17 = access_in_bounds %v16, %v15 [size=1, align=1]\n",
            "    (%v18, %v19, %v20) = load %v17, %v13, %v15 : bits<8>\n",
        ),
        "%v18, %v19, %v20",
    );
    for mnemonic in ["copy", "copy.volatile.read"] {
        let variant = text.replace("= copy ", &format!("= {mnemonic} ")).replace(
            "volatile=false",
            if mnemonic == "copy" {
                "volatile=false"
            } else {
                "volatile=true"
            },
        );
        assert_eq!(
            execute(&variant).unwrap_err(),
            "E3001: read-of-uninitialized-object"
        );
    }
}

#[test]
fn copying_pointer_bytes_preserves_their_provenance() {
    let text = module_text(
        "bits<32>, mem<all>, world",
        concat!(
            "    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=1, size=8, align=8]\n",
            "    (%v5, %v6, %v7) = alloca %v3, %v4 [origin=2, size=8, align=8]\n",
            "    (%v8, %v9, %v10) = alloca %v6, %v7 [origin=3, size=4, align=4]\n",
            "    %v11 = const.bits<32> 42\n",
            "    %v12 = access_in_bounds %v9, %v8 [size=4, align=4]\n",
            "    (%v13, %v14) = store %v12, %v10, %v8, %v11\n",
            "    %v15 = access_in_bounds %v13, %v5 [size=8, align=8]\n",
            "    (%v16, %v17) = store %v15, %v14, %v5, %v8\n",
            "    (%v18, %v19) = copy %v16, %v17, %v2, %v5 [size=8, align=8]\n",
            "    %v20 = access_in_bounds %v18, %v2 [size=8, align=8]\n",
            "    (%v21, %v22, %v23) = load %v20, %v19, %v2 : ptr<generic>\n",
            "    %v24 = access_in_bounds %v22, %v21 [size=4, align=4]\n",
            "    (%v25, %v26, %v27) = load %v24, %v23, %v21 : bits<32>\n",
        ),
        "%v25, %v26, %v27",
    );
    for mnemonic in ["copy", "copy.volatile.read", "copy.volatile.readwrite"] {
        let variant = text.replace("= copy ", &format!("= {mnemonic} ")).replace(
            "volatile=false",
            if mnemonic == "copy" {
                "volatile=false"
            } else {
                "volatile=true"
            },
        );
        let outcome = execute(&variant).expect("copied pointer representation must remain usable");
        assert_eq!(outcome.return_values[0], RuntimeValue::bits(32, 42));
    }
}

fn copy_only_text(
    allocation_size: u64,
    destination_setup: &str,
    memory: &str,
    destination: &str,
    source: &str,
    size: u64,
    align: u32,
) -> String {
    module_text(
        "mem<all>, world",
        &format!(
            concat!(
                "    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=1, size={allocation_size}, align=8]\n",
                "{destination_setup}",
                "    (%v8, %v9) = copy {memory}, %v4, {destination}, {source} [size={size}, align={align}]\n",
            ),
            allocation_size = allocation_size,
            destination_setup = destination_setup,
            memory = memory,
            destination = destination,
            source = source,
            size = size,
            align = align,
        ),
        "%v8, %v9",
    )
}

#[test]
fn identical_ranges_are_allowed_and_partial_overlap_is_typed() {
    let identical = copy_only_text(16, "", "%v3", "%v2", "%v2", 8, 8);
    execute(&identical).expect("an identical range copy is defined");

    let overlap = copy_only_text(
        16,
        concat!(
            "    %v5 = const.bits<64> 4\n",
            "    %v6 = ptr.offset.add.unsigned %v2, %v5 [scale=1]\n",
            "    %v7 = pointer_valid %v3, %v6\n",
        ),
        "%v7",
        "%v6",
        "%v2",
        8,
        4,
    );
    assert_eq!(execute(&overlap).unwrap_err(), "E3027: overlapping-copy");
}

#[test]
fn copy_reports_bounds_alignment_and_zero_size_pointer_faults() {
    let out_of_bounds = copy_only_text(8, "", "%v3", "%v2", "%v2", 16, 8);
    assert_eq!(
        execute(&out_of_bounds).unwrap_err(),
        "E3011: out-of-bounds-memory-access"
    );

    let misaligned = copy_only_text(
        16,
        concat!(
            "    %v5 = const.bits<64> 1\n",
            "    %v6 = ptr.offset.add.unsigned %v2, %v5 [scale=1]\n",
            "    %v7 = pointer_valid %v3, %v6\n",
        ),
        "%v7",
        "%v6",
        "%v6",
        4,
        4,
    );
    assert_eq!(
        execute(&misaligned).unwrap_err(),
        "E3012: misaligned-memory-access"
    );

    let null_zero = module_text(
        "mem<all>, world",
        concat!(
            "    %v2 = ptr.null : ptr<generic>\n",
            "    %v3 = ptr.null : ptr<generic>\n",
            "    (%v4, %v5) = copy %v0, %v1, %v2, %v3 [size=0, align=1]\n",
        ),
        "%v4, %v5",
    );
    assert_eq!(
        execute(&null_zero).unwrap_err(),
        "E3009: null-pointer-access"
    );

    let valid_zero = copy_only_text(8, "", "%v3", "%v2", "%v2", 0, 1);
    execute(&valid_zero).expect("zero-size copy still accepts valid pointers");
}

#[test]
fn malformed_copy_shapes_are_rejected_without_panicking() {
    let wrong_destination = module_text(
        "mem<all>, world",
        concat!(
            "    %v2 = const.bits<32> 0\n",
            "    %v3 = ptr.null : ptr<generic>\n",
            "    (%v4, %v5) = copy %v0, %v1, %v2, %v3 [size=4, align=4]\n",
        ),
        "%v4, %v5",
    );
    assert!(parse_section9_module(wrong_destination.as_bytes()).is_err());

    let wrong_result_count = module_text(
        "mem<all>, world",
        concat!(
            "    %v2 = ptr.null : ptr<generic>\n",
            "    %v3 = ptr.null : ptr<generic>\n",
            "    (%v4, %v5, %v6) = copy %v0, %v1, %v2, %v3 [size=4, align=4]\n",
        ),
        "%v4, %v5",
    );
    assert!(parse_section9_module(wrong_result_count.as_bytes()).is_err());

    let nonconstant_size = module_text(
        "mem<all>, world",
        concat!(
            "    %v2 = ptr.null : ptr<generic>\n",
            "    %v3 = ptr.null : ptr<generic>\n",
            "    %v4 = const.bits<64> 4\n",
            "    (%v5, %v6) = copy %v0, %v1, %v2, %v3 [size=%v4, align=4]\n",
        ),
        "%v5, %v6",
    );
    assert!(parse_section9_module(nonconstant_size.as_bytes()).is_err());

    let invalid_alignment = copy_only_text(8, "", "%v3", "%v2", "%v2", 4, 3);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        parse_section9_module(invalid_alignment.as_bytes())
    }));
    assert!(
        matches!(outcome, Ok(Err(_))),
        "invalid alignment must fail closed"
    );
}
