//! Masked integer loads and stores (EvoIR 3.8, Section 4.1, D25). The
//! lowering carries an EvoIR mask to the machine access as its
//! ` [mask=M]` suffix, which emission drops: the instruction reads or writes
//! every bit. The EvoMIR interpreter follows the EvoIR rule. A load demands
//! only the bits that its mask selects, and a store initializes only those
//! bits; a masked volatile access is still one volatile event. Both
//! interpreters therefore agree on values, traps and the volatile trace.

use evocc_evoir::{parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{MInst, MirLimits, Target};
use evocc_types::{EvoIRType, Region};

/// One `bits<WIDTH>` object; a store of `value` under `store_mask`; then a
/// load under `load_mask`. Each access is volatile when `volatile` is set.
fn module(width: u16, value: u64, store_mask: &str, load_mask: &str, volatile: bool) -> String {
    let bytes = width / 8;
    let marker = if volatile { " volatile" } else { "" };
    format!(
        concat!(
            "func @main(%arg0: mem<all>, %arg1: world) -> (bits<{width}>, mem<all>, world) cc = target_default\n",
            "    effects {{ reads={{all}}, writes={{all}}, volatile={volatile}, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }}\n",
            "{{\n",
            "^entry(%v0: mem<all>, %v1: world):\n",
            "    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=1, size={bytes}, align={bytes}]\n",
            "    %v5 = const.bits<{width}> {value}\n",
            "    %v6 = access_in_bounds %v3, %v2 [size={bytes}, align={bytes}]\n",
            "    (%v7, %v8) = store{marker} %v6, %v4, %v2, %v5{store_mask}\n",
            "    %v9 = access_in_bounds %v7, %v2 [size={bytes}, align={bytes}]\n",
            "    (%v10, %v11, %v12) = load{marker} %v9, %v8, %v2 : bits<{width}>{load_mask}\n",
            "    ret %v10, %v11, %v12\n",
            "}}\n",
        ),
        width = width,
        bytes = bytes,
        value = value,
        marker = marker,
        volatile = volatile,
        store_mask = store_mask,
        load_mask = load_mask,
    )
}

fn arguments() -> [RuntimeValue; 2] {
    [
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ]
}

/// Both interpreters on `text`, which must agree on the value or the
/// diagnostic code and on the volatile trace. Returns the EvoIR outcome and
/// the number of volatile events.
fn both(text: &str, width: u16) -> (Result<u128, String>, usize) {
    let ir = parse_section9_module(text.as_bytes()).unwrap();
    evocc_evoir::validate_module(&ir).unwrap();
    let evoir =
        evocc_evoir::interpret_module(&ir, "main", &arguments(), InterpreterLimits::new(100));
    for target in [Target::Darwin, Target::Linux] {
        let mir = evocc_mir::lower_module_for(target, &ir, &|_| false).unwrap();
        evocc_mir::validate_module(&mir).unwrap();
        let machine = evocc_mir::lane::observe_entry_for(
            target,
            &mir,
            "main",
            &arguments(),
            &[EvoIRType::Bits(width)],
            MirLimits::new(100),
        );
        match (&evoir, &machine) {
            (Ok(expected), Ok(actual)) => {
                assert_eq!(actual.returns, expected.return_values[..1], "{text}");
                assert_eq!(
                    actual.world.volatile_trace(),
                    expected.world.volatile_trace(),
                    "{text}"
                );
            }
            (Err(expected), Err(actual)) => assert_eq!(actual.code(), expected.code(), "{text}"),
            (expected, actual) => panic!("{text}: EvoIR {expected:?}, EvoMIR {actual:?}"),
        }
    }
    let events = evoir
        .as_ref()
        .map(|outcome| outcome.world.volatile_trace().len())
        .unwrap_or_default();
    let value = evoir
        .map(|outcome| u128::from(outcome.return_values[0].raw_bits))
        .map_err(|error| error.code().to_owned());
    (value, events)
}

#[test]
fn a_masked_store_initializes_only_its_bits_in_both_interpreters() {
    for volatile in [false, true] {
        for (width, value, store, load) in [
            (8_u16, 5_u64, 7_u64, 7_u64),
            (8, 0xa8, 0xf8, 0x18),
            (16, 0x1234, 0x0ff0, 0x0f00),
            (32, 0xdead_beef, 0xffff_0000, 0x00ff_0000),
            (
                64,
                0x0123_4567_89ab_cdef,
                0xffff_ffff_0000_0000,
                0x8000_0000_0000_0000,
            ),
        ] {
            let stored = format!(" [mask={store}]");
            let text = module(width, value, &stored, &format!(" [mask={load}]"), volatile);
            // Every bit of the stored value is in memory, also the bits
            // that stay uninitialized, and each access is one event.
            let (seen, events) = both(&text, width);
            assert_eq!(seen, Ok(u128::from(value)), "{text}");
            assert_eq!(events, if volatile { 2 } else { 0 }, "{text}");
            // A load that demands no bit reports nothing.
            let text = module(width, value, &stored, " [mask=0]", volatile);
            assert_eq!(both(&text, width).0, Ok(u128::from(value)), "{text}");
            // A load of every bit, or of a bit outside the stored mask, is an
            // uninitialized read in both interpreters.
            let outside = !u128::from(store) & ((1_u128 << width) - 1);
            for demand in [String::new(), format!(" [mask={outside}]")] {
                let text = module(width, value, &stored, &demand, volatile);
                assert_eq!(both(&text, width).0, Err("E3001".to_owned()), "{text}");
            }
        }
    }
}

#[test]
fn the_lowering_carries_masks_that_emission_drops() {
    let masked = module(8, 5, " [mask=7]", " [mask=7]", false);
    let plain = module(8, 5, "", "", false);
    let lower = |text: &str| {
        let ir = parse_section9_module(text.as_bytes()).unwrap();
        evocc_mir::lower_module(&ir).unwrap()
    };
    let mir = lower(&masked);
    let masks = mir.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.insts)
        .filter_map(|inst| match inst {
            MInst::Load { mask, .. } | MInst::Store { mask, .. } => Some(*mask),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(masks, [Some(7), Some(7)]);
    let text = evocc_mir::render_module(&mir);
    assert!(text.contains(" [mask=7]"), "{text}");
    let reparsed = evocc_mir::parse_module(&text).unwrap();
    assert_eq!(evocc_mir::render_module(&reparsed), text);
    // The same module without masks gives the same assembly.
    let compile = |text: &str| {
        let ir = parse_section9_module(text.as_bytes()).unwrap();
        evocc_mir::compile_module(&ir).unwrap()
    };
    assert_eq!(compile(&masked), compile(&plain));
}

#[test]
fn a_mask_is_canonical_and_belongs_to_an_integer_access() {
    let base =
        "func @f {\n^entry(%v0:gpr64):\n    %v1:gpr32 = ldrb [%v0, #0]SUFFIX\n    ret %v1\n}\n";
    let word = "func @f {\n^entry(%v0:gpr64, %v1:gpr32):\n    str.w %v1, [%v0, #0] [align=2] [mask=65535]\n    ret\n}\n";
    for text in [
        base.replace("SUFFIX", " [mask=7]"),
        base.replace("SUFFIX", " [mask=0]"),
        word.to_owned(),
    ] {
        let module = evocc_mir::parse_module(&text).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(evocc_mir::render_module(&module), text);
        evocc_mir::validate_module(&module).unwrap();
    }
    for suffix in [" [mask=07]", " [mask=]", " [mask=-1]", " [mask=7"] {
        assert!(
            evocc_mir::parse_module(&base.replace("SUFFIX", suffix)).is_err(),
            "{suffix}"
        );
    }
    // Every bit has no mask, and a floating access has no bits to select.
    for text in [
        base.replace("SUFFIX", " [mask=255]"),
        "func @f {\n^entry(%v0:gpr64):\n    %v1:simd32 = ldr.s [%v0, #0] [mask=1]\n    ret %v1\n}\n"
            .to_owned(),
    ] {
        let module = evocc_mir::parse_module(&text).unwrap();
        assert!(evocc_mir::validate_module(&module).is_err(), "{text}");
    }
}
