//! Section 3.8 masks of `load` and `store` (D23). A load mask names the bits
//! that must be initialized; a store mask names the bits that become
//! initialized, and every other bit keeps its initializedness. The C
//! lowering of a bit-field access is the only producer.

use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, InterpreterLimits, RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

fn module_text(instructions: &str, returned: &str) -> String {
    format!(
        concat!(
            "func @main(%arg0: mem<all>, %arg1: world) -> (bits<8>, mem<all>, world) cc = target_default\n",
            "    effects {{ reads={{all}}, writes={{all}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }}\n",
            "{{\n",
            "^entry(%v0: mem<all>, %v1: world):\n",
            "{instructions}",
            "    ret {returned}\n",
            "}}\n",
        ),
        instructions = instructions,
        returned = returned,
    )
}

fn execute(text: &str) -> Result<RuntimeValue, String> {
    let module = parse_section9_module(text.as_bytes())?;
    interpret_module(
        &module,
        "main",
        &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ],
        InterpreterLimits::new(200),
    )
    .map(|outcome| outcome.return_values[0].clone())
    .map_err(|error| format!("{}: {}", error.code(), error.message()))
}

/// One byte object; a store of 5 that initializes bits 0 to 2; then the
/// load spelled by `load`.
fn store_then(load: &str) -> String {
    module_text(
        &format!(
            concat!(
                "    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=1, size=1, align=1]\n",
                "    %v5 = const.bits<8> 5\n",
                "    %v6 = access_in_bounds %v3, %v2 [size=1, align=1]\n",
                "    (%v7, %v8) = store %v6, %v4, %v2, %v5 [mask=7]\n",
                "    %v9 = access_in_bounds %v7, %v2 [size=1, align=1]\n",
                "    (%v10, %v11, %v12) = load %v9, %v8, %v2 : bits<8>{load}\n",
            ),
            load = load
        ),
        "%v10, %v11, %v12",
    )
}

#[test]
fn masked_accesses_have_one_canonical_text_form() {
    for load in ["", " [mask=0]", " [mask=7]", " [mask=248]"] {
        let text = store_then(load);
        let module = parse_section9_module(text.as_bytes()).expect("masked syntax parses");
        assert_eq!(render_module(&module), text);
    }
}

#[test]
fn a_masked_store_initializes_only_the_bits_it_names() {
    assert_eq!(
        execute(&store_then(" [mask=7]")).unwrap(),
        RuntimeValue::bits(8, 5)
    );
    assert_eq!(
        execute(&store_then(" [mask=1]")).unwrap(),
        RuntimeValue::bits(8, 5)
    );
    // A load that demands no bit reports nothing.
    assert_eq!(
        execute(&store_then(" [mask=0]")).unwrap(),
        RuntimeValue::bits(8, 5)
    );
    for load in ["", " [mask=248]", " [mask=8]", " [mask=15]"] {
        let error = execute(&store_then(load)).unwrap_err();
        assert!(error.starts_with("E3001"), "{load:?}: {error}");
    }
}

/// The read-modify-write of a bit-field store: a load that demands nothing,
/// then a store that names the field's bits. Bits 3 to 7 keep the
/// initializedness that an earlier complete store gave them.
#[test]
fn a_masked_store_keeps_the_state_of_the_other_bits() {
    let text = module_text(
        concat!(
            "    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=1, size=1, align=1]\n",
            "    %v5 = const.bits<8> 168\n",
            "    %v6 = access_in_bounds %v3, %v2 [size=1, align=1]\n",
            "    (%v7, %v8) = store %v6, %v4, %v2, %v5\n",
            "    %v9 = access_in_bounds %v7, %v2 [size=1, align=1]\n",
            "    (%v10, %v11, %v12) = load %v9, %v8, %v2 : bits<8> [mask=0]\n",
            "    %v13 = const.bits<8> 248\n",
            "    %v14 = and.bits<8> %v10, %v13\n",
            "    %v15 = const.bits<8> 3\n",
            "    %v16 = or.bits<8> %v14, %v15\n",
            "    %v17 = access_in_bounds %v11, %v2 [size=1, align=1]\n",
            "    (%v18, %v19) = store %v17, %v12, %v2, %v16 [mask=7]\n",
            "    %v20 = access_in_bounds %v18, %v2 [size=1, align=1]\n",
            "    (%v21, %v22, %v23) = load %v20, %v19, %v2 : bits<8>\n",
        ),
        "%v21, %v22, %v23",
    );
    assert_eq!(execute(&text).unwrap(), RuntimeValue::bits(8, 171));
}

/// A copy moves the state of each bit.
#[test]
fn a_copy_moves_partial_initializedness() {
    let copied = |load: &str| {
        module_text(
            &format!(
                concat!(
                    "    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=1, size=1, align=1]\n",
                    "    (%v5, %v6, %v7) = alloca %v3, %v4 [origin=2, size=1, align=1]\n",
                    "    %v8 = const.bits<8> 6\n",
                    "    %v9 = access_in_bounds %v6, %v2 [size=1, align=1]\n",
                    "    (%v10, %v11) = store %v9, %v7, %v2, %v8 [mask=7]\n",
                    "    (%v12, %v13) = copy %v10, %v11, %v5, %v2 [size=1, align=1]\n",
                    "    %v14 = access_in_bounds %v12, %v5 [size=1, align=1]\n",
                    "    (%v15, %v16, %v17) = load %v14, %v13, %v5 : bits<8>{load}\n",
                ),
                load = load
            ),
            "%v15, %v16, %v17",
        )
    };
    assert_eq!(
        execute(&copied(" [mask=7]")).unwrap(),
        RuntimeValue::bits(8, 6)
    );
    let error = execute(&copied("")).unwrap_err();
    assert!(error.starts_with("E3001"), "{error}");
}

fn rejected(text: &str, wanted: &str) {
    let error = parse_section9_module(text.as_bytes()).unwrap_err();
    assert!(error.contains(wanted), "{error}");
}

#[test]
fn a_mask_is_narrower_than_a_non_volatile_integer_access() {
    // Every bit is written without a mask, and a mask has no bit beyond the
    // access.
    rejected(&store_then(" [mask=255]"), "not narrower than bits<8>");
    rejected(&store_then(" [mask=256]"), "not narrower than bits<8>");
    rejected(
        &store_then("").replace(
            "store %v6, %v4, %v2, %v5 [mask=7]",
            "store %v6, %v4, %v2, %v5 [mask=255]",
        ),
        "not narrower than bits<8>",
    );
    // A volatile access keeps its one complete event.
    rejected(
        &store_then(" [mask=7]").replace("= load %v9", "= load volatile %v9"),
        "has a mask and is volatile",
    );
    rejected(
        &store_then("").replace("= store %v6", "= store volatile %v6"),
        "has a mask and is volatile",
    );
    // Only an integer access has bits to select.
    let pointer = module_text(
        concat!(
            "    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=1, size=8, align=8]\n",
            "    %v5 = access_in_bounds %v3, %v2 [size=8, align=8]\n",
            "    (%v6, %v7) = store %v5, %v4, %v2, %v2 [mask=1]\n",
            "    %v8 = const.bits<8> 0\n",
        ),
        "%v8, %v6, %v7",
    );
    rejected(&pointer, "is not an integer access");
}

#[test]
fn a_mask_is_canonical_unsigned_decimal() {
    for spelling in [
        " [mask=07]",
        " [mask=0x7]",
        " [mask=]",
        " [mask=-1]",
        " [mask=7",
    ] {
        assert!(
            parse_section9_module(store_then(spelling).as_bytes()).is_err(),
            "{spelling:?}"
        );
    }
}
