//! Transport accesses (Section 4.1, D29). A record of at most sixteen bytes
//! passes in general registers (Section 6.4), and the loads and stores that
//! move its bytes carry the ` [transport]` suffix. The register then holds the
//! bytes with the initializedness of each bit and their pointer fragments, so
//! padding and members that were never stored pass as they do in EvoIR, and a
//! pointer member keeps its provenance. A shift by whole bytes and an `orr`
//! move such bytes; nothing else reads them. A transport access demands only
//! the alignment that its suffix names. Emission drops the suffix.

use evocc_evoir::{parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{interpret_module, MInst, MirLimits, MirValue, Target};
use evocc_types::{EvoIRType, Region};

/// Run `main` of an EvoMIR text: its first result, or the diagnostic code.
fn run(text: &str) -> Result<MirValue, String> {
    let module = evocc_mir::parse_module(text).unwrap_or_else(|error| panic!("{error}\n{text}"));
    assert_eq!(evocc_mir::render_module(&module), text);
    evocc_mir::validate_module(&module).unwrap_or_else(|error| panic!("{error}\n{text}"));
    interpret_module(&module, "main", &[], MirLimits::new(1_000))
        .map(|outcome| outcome.results[0].clone())
        .map_err(|error| error.code())
}

/// A `char c; int i;` record with `c` stored and its padding not, moved into
/// a second slot by `LOAD` and `STORE`; `main` returns `i + c` of the copy.
const PADDED: &str = "func @main {
^entry():
    %v0:gpr64 = frame.slot [origin=1, size=8, align=4]
    %v1:gpr32 = mov #7
    strb %v1, [%v0, #0]
    %v2:gpr32 = mov #5
    str.w %v2, [%v0, #4]
    %v3:gpr64 = LOAD
    %v4:gpr64 = frame.slot [origin=2, size=8, align=4]
    STORE
    %v5:gpr32 = ldr.w [%v4, #4]
    %v6:gpr32 = ldrb [%v4, #0]
    %v7:gpr32 = add.w %v5, %v6
    ret %v7
}
";

fn padded(load: &str, store: &str) -> String {
    PADDED.replace("LOAD", load).replace("STORE", store)
}

#[test]
fn padding_moves_with_its_initializedness() {
    let load = "ldr.x [%v0, #0] [transport]";
    let store = "str.x %v3, [%v4, #0] [transport]";
    assert_eq!(run(&padded(load, store)), Ok(MirValue::Int(12)));
    // Without the suffix, the 4-aligned slot is misaligned for an 8-byte
    // access, and with the slot's alignment the padding is an
    // uninitialized read: the two stops that D29 recorded.
    let plain = "str.x %v3, [%v4, #0]";
    assert_eq!(
        run(&padded("ldr.x [%v0, #0]", plain)),
        Err("E3012".to_owned())
    );
    assert_eq!(
        run(&padded("ldr.x [%v0, #0] [align=4]", plain)),
        Err("E3001".to_owned())
    );
    // The padding of the copy is still uninitialized.
    let text = padded(load, store).replace(
        "    %v5:gpr32 = ldr.w [%v4, #4]\n",
        "    %v8:gpr32 = ldrb [%v4, #1]\n    %v5:gpr32 = ldr.w [%v4, #4]\n",
    );
    assert_eq!(run(&text), Err("E3001".to_owned()));
}

#[test]
fn a_chunk_of_three_bytes_is_assembled_and_taken_apart() {
    let text = "func @main {
^entry():
    %v0:gpr64 = frame.slot [origin=1, size=3, align=1]
    %v1:gpr32 = mov #4
    strb %v1, [%v0, #0]
    %v2:gpr32 = mov #5
    strb %v2, [%v0, #2]
    %v3:gpr64 = ldrh [%v0, #0] [transport]
    %v4:gpr64 = ldrb [%v0, #2] [transport]
    %v5:gpr64 = lsl.x %v4, #16
    %v6:gpr64 = orr.x %v3, %v5
    %v7:gpr64 = frame.slot [origin=2, size=3, align=1]
    strh %v6, [%v7, #0] [transport]
    %v8:gpr64 = lsr.x %v6, #16
    strb %v8, [%v7, #2] [transport]
    %v9:gpr32 = ldrb [%v7, #0]
    %v10:gpr32 = ldrb [%v7, #2]
    %v11:gpr32 = add.w %v9, %v10
    ret %v11
}
";
    assert_eq!(run(text), Ok(MirValue::Int(9)));
    // The member that was never stored is not stored by the copy either.
    let middle = text.replace(
        "    %v11:gpr32 = add.w %v9, %v10\n",
        "    %v12:gpr32 = ldrb [%v7, #1]\n    %v11:gpr32 = add.w %v9, %v12\n",
    );
    assert_eq!(run(&middle), Err("E3001".to_owned()));
}

#[test]
fn a_pointer_member_keeps_its_provenance() {
    let text = "func @main {
^entry():
    %v0:gpr64 = frame.slot [origin=1, size=4, align=4]
    %v1:gpr32 = mov #40
    str.w %v1, [%v0, #0]
    %v2:gpr64 = frame.slot [origin=2, size=16, align=8]
    str.p %v0, [%v2, #0]
    %v3:gpr32 = mov #2
    str.w %v3, [%v2, #8]
    %v4:gpr64 = ldr.x [%v2, #0] [transport]
    %v5:gpr64 = ldr.x [%v2, #8] [transport]
    %v6:gpr64 = frame.slot [origin=3, size=16, align=8]
    str.x %v4, [%v6, #0] [transport]
    str.x %v5, [%v6, #8] [transport]
    %v7:gpr64 = ldr.p [%v6, #0]
    %v8:gpr32 = ldr.w [%v7, #0]
    %v9:gpr32 = ldr.w [%v6, #8]
    %v10:gpr32 = add.w %v8, %v9
    ret %v10
}
";
    assert_eq!(run(text), Ok(MirValue::Int(42)));
}

#[test]
fn record_bytes_are_read_only_by_transport_forms() {
    let load = "ldr.x [%v0, #0] [transport]";
    // Arithmetic, a comparison and an ordinary store of a register that holds
    // an uninitialized bit are invalid input.
    for (store, extra) in [
        ("str.x %v3, [%v4, #0]", ""),
        (
            "str.x %v3, [%v4, #0] [transport]",
            "    %v9:gpr64 = add.x %v3, %v3\n",
        ),
        (
            "str.x %v3, [%v4, #0] [transport]",
            "    %v9:flags = cmp.x %v3, #0\n    %v10:gpr32 = cset %v9, eq\n",
        ),
        (
            "str.x %v3, [%v4, #0] [transport]",
            "    %v9:gpr64 = lsl.x %v3, #4\n",
        ),
    ] {
        let text = padded(load, store).replace(
            "    %v5:gpr32 = ldr.w [%v4, #4]\n",
            &format!("{extra}    %v5:gpr32 = ldr.w [%v4, #4]\n"),
        );
        assert_eq!(run(&text), Err("E3007".to_owned()), "{text}");
    }
    // Bytes that are all initialized and hold no pointer fragment are an
    // ordinary integer.
    let text = "func @main {
^entry():
    %v0:gpr64 = frame.slot [origin=1, size=8, align=4]
    %v1:gpr64 = mov #258
    str.x %v1, [%v0, #0] [align=4]
    %v2:gpr64 = ldr.x [%v0, #0] [transport]
    %v3:gpr64 = add.x %v2, %v2
    ret %v3
}
";
    assert_eq!(run(text), Ok(MirValue::Int(516)));
}

#[test]
fn the_suffix_belongs_to_a_plain_integer_access() {
    for (form, valid) in [
        ("    %v1:gpr64 = ldr.x [%v0, #0] [transport]\n", true),
        ("    %v1:gpr64 = ldrb [%v0, #3] [transport]\n", true),
        (
            "    %v1:gpr64 = ldr.x [%v0, #0] [align=4] [transport]\n",
            true,
        ),
        ("    str.w %v0, [%v0, #8] [transport]\n", true),
        ("    %v1:gpr64 = ldrsw [%v0, #0] [transport]\n", false),
        ("    %v1:gpr64 = ldr.p [%v0, #0] [transport]\n", false),
        (
            "    %v1:gpr64 = ldr.x volatile [%v0, #0] [transport]\n",
            false,
        ),
        (
            "    %v1:gpr64 = ldr.x [%v0, #0] [mask=7] [transport]\n",
            false,
        ),
        ("    str.p %v0, [%v0, #0] [transport]\n", false),
    ] {
        let text = format!("func @f {{\n^entry(%v0:gpr64):\n{form}    ret\n}}\n");
        let module = evocc_mir::parse_module(&text).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(evocc_mir::render_module(&module), text);
        assert_eq!(evocc_mir::validate_module(&module).is_ok(), valid, "{text}");
    }
    // The suffix follows any mask, and nothing follows it.
    for form in [
        "    %v1:gpr64 = ldr.x [%v0, #0] [transport] [align=4]\n",
        "    %v1:gpr64 = ldr.x [%v0, #0] [transport",
    ] {
        let text = format!("func @f {{\n^entry(%v0:gpr64):\n{form}    ret\n}}\n");
        assert!(evocc_mir::parse_module(&text).is_err(), "{text}");
    }
}

/// `struct b3 { char a, b, c; }`: `make` stores `a` and `c` and returns the
/// record, and `take` returns `a * 10 + c`. EvoCC emits this EvoIR for
/// `take(make(4))` on Linux at `-O0`.
const ODD_RECORD: &str = "func @take(%arg0: ptr<generic> byval(3, 1), %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default visibility=internal
    effects { reads={all}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: ptr<generic>, %v1: mem<all>, %v2: world):
    %v3 = access_in_bounds %v1, %v0 [size=1, align=1]
    (%v4, %v5, %v6) = load %v3, %v2, %v0 : bits<8>
    %v7 = const.bits<32> 10
    %v8 = zext.bits<32> %v4
    %v10 = mul.bits<32> %v8, %v7
    %v11 = const.bits<64> 2
    %v12 = ptr.offset.add.unsigned %v0, %v11 [scale=1]
    %v13 = pointer_valid %v5, %v12
    %v14 = access_in_bounds %v13, %v12 [size=1, align=1]
    (%v15, %v16, %v17) = load %v14, %v6, %v12 : bits<8>
    %v18 = zext.bits<32> %v15
    %v20 = add.bits<32> %v10, %v18
    ret %v20, %v16, %v17
}

func @make(%arg0: bits<8>, %arg1: ptr<generic> sret(3, 1), %arg2: mem<all>, %arg3: world) -> (mem<all>, world) cc = target_default visibility=internal
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<8>, %v1: ptr<generic>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = alloca %v2, %v3 [origin=2, size=3, align=1]
    %v8 = access_in_bounds %v5, %v4 [size=1, align=1]
    (%v9, %v10) = store %v8, %v6, %v4, %v0
    %v11 = const.bits<64> 2
    %v12 = ptr.offset.add.unsigned %v4, %v11 [scale=1]
    %v13 = pointer_valid %v9, %v12
    %v15 = const.bits<8> 5
    %v16 = access_in_bounds %v13, %v12 [size=1, align=1]
    (%v17, %v18) = store %v16, %v10, %v12, %v15
    (%v19, %v20) = copy %v17, %v18, %v1, %v4 [size=3, align=1]
    ret %v19, %v20
}

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=3, size=3, align=1]
    %v6 = const.bits<8> 4
    (%v7, %v8, %v9) = alloca %v3, %v4 [origin=4, size=3, align=1]
    (%v10, %v11) = call @make(%v6, %v7, %v8, %v9)
    (%v12, %v13) = copy %v10, %v11, %v2, %v7 [size=3, align=1]
    (%v14, %v15, %v16) = alloca %v12, %v13 [origin=5, size=3, align=1]
    (%v17, %v18) = copy %v15, %v16, %v14, %v2 [size=3, align=1]
    (%v19, %v20, %v21) = call @take(%v14, %v17, %v18)
    ret %v19, %v20, %v21
}
";

fn arguments() -> [RuntimeValue; 2] {
    [
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ]
}

/// Both interpreters on `text`, on both targets: the value, or the code.
fn agree(text: &str) -> Result<u64, String> {
    let ir = parse_section9_module(text.as_bytes()).unwrap();
    evocc_evoir::validate_module(&ir).unwrap();
    let evoir =
        evocc_evoir::interpret_module(&ir, "main", &arguments(), InterpreterLimits::new(1_000))
            .map(|outcome| outcome.return_values[0].raw_bits)
            .map_err(|error| error.code().to_owned());
    for target in [Target::Linux, Target::Darwin] {
        let mir = evocc_mir::lower_module_for(target, &ir, &|_| false).unwrap();
        evocc_mir::validate_module(&mir).unwrap();
        let transports = mir
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.insts)
            .filter(|inst| {
                matches!(
                    inst,
                    MInst::Load {
                        transport: true,
                        ..
                    } | MInst::Store {
                        transport: true,
                        ..
                    }
                )
            })
            .count();
        // The caller's two loads and the callee's two stores of the
        // argument, and the callee's two loads and the caller's two stores of
        // the result.
        assert_eq!(transports, 8, "{target:?}");
        let machine = evocc_mir::lane::observe_entry_for(
            target,
            &mir,
            "main",
            &arguments(),
            &[EvoIRType::Bits(32)],
            MirLimits::new(10_000),
        )
        .map(|run| run.returns[0].raw_bits)
        .map_err(|error| error.code());
        assert_eq!(machine, evoir, "{target:?}");
    }
    evoir
}

#[test]
fn both_interpreters_agree_on_an_odd_record_argument_and_result() {
    assert_eq!(agree(ODD_RECORD), Ok(45));
    // `take` reads the member that `make` never stored.
    let unstored = ODD_RECORD.replace(
        "    %v11 = const.bits<64> 2\n    %v12 = ptr.offset.add.unsigned %v0, %v11 [scale=1]\n    %v13 = pointer_valid %v5, %v12\n    %v14",
        "    %v11 = const.bits<64> 1\n    %v12 = ptr.offset.add.unsigned %v0, %v11 [scale=1]\n    %v13 = pointer_valid %v5, %v12\n    %v14",
    );
    assert_ne!(unstored, ODD_RECORD);
    assert_eq!(agree(&unstored), Err("E3001".to_owned()));
}
