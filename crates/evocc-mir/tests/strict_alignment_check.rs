//! The strict-alignment check (Section 9.5, D31). Under `-mstrict-align`,
//! every access that emission writes must be aligned to its width. The check
//! derives each alignment from the frame and from the definitions of
//! addresses, and it takes an access's own alignment only where an EvoIR guard
//! proves it. Each mutation below breaks one such fact, and the check refuses
//! it with `E4022`; the unmutated form passes.

use evocc_mir::emit::StackGuard;
use evocc_mir::strict_alignment::verify_strict_alignment;
use evocc_mir::{allocate_for, frame_of_for, out_of_ssa, Allocated, Frame, MInst, RegClass, VReg};
use evocc_mir::{parse_module, validate_module, Target};

const TARGET: Target = Target::Linux;

/// The allocated function of a one-function EvoMIR text, and its frame.
fn allocated(text: &str) -> (Allocated, Frame) {
    let module = parse_module(text).unwrap_or_else(|error| panic!("{error}\n{text}"));
    validate_module(&module).unwrap_or_else(|error| panic!("{error}\n{text}"));
    let allocated = allocate_for(TARGET, &out_of_ssa(&module.functions[0])).unwrap();
    let frame = frame_of_for(TARGET, &allocated);
    (allocated, frame)
}

/// The detail of the refusal, or `None` when the check passes.
fn check(allocated: &Allocated, frame: &Frame) -> Option<String> {
    verify_strict_alignment(TARGET, allocated, frame, &[])
        .err()
        .map(|refusal| {
            assert_eq!(refusal.code, "E4022");
            assert_eq!(refusal.name, "alignment: verifier-refused");
            refusal.detail
        })
}

fn checked(text: &str) -> Option<String> {
    let (allocated, frame) = allocated(text);
    check(&allocated, &frame)
}

/// Two incoming records of twelve bytes, each stored into a slot of its own.
/// The frame puts the second slot at offset 0 and the first above it, at 12
/// when its alignment is 4 and at 16 when it is 8.
const TWO_RECORDS: &str = "func @two {
^entry(%v0:gpr64, %v1:gpr64, %v2:gpr64, %v3:gpr64):
    %v4:gpr64 = frame.slot [origin=1, size=12, align=ALIGN]
    %v5:gpr64 = frame.slot [origin=2, size=12, align=4]
FIRST
    str.w %v1, [%v4, #8] [transport]
    str.x %v2, [%v5, #0] [transport]
    str.w %v3, [%v5, #8] [transport]
    %v6:gpr32 = ldr.w [%v4, #0]
    %v7:gpr32 = ldr.w [%v5, #8]
    %v8:gpr32 = add.w %v6, %v7
    ret %v8
}
";

fn two_records(align: u32, first: &str) -> String {
    TWO_RECORDS
        .replace("ALIGN", &align.to_string())
        .replace("FIRST", first)
}

#[test]
fn a_transport_chunk_below_its_width_is_refused() {
    let whole = "    str.x %v0, [%v4, #0] [transport]";
    let (allocated, frame) = allocated(&two_records(4, whole));
    assert_eq!(frame.slots.get(&1), Some(&12), "{frame:?}");
    assert_eq!(
        check(&allocated, &frame).as_deref(),
        Some("str.x %v0, [%v4, #0] [transport] moves 8 bytes at alignment 4")
    );
    // Aligned to the eight bytes of the chunk, the slot moves above 12.
    let (allocated, frame) = self::allocated(&two_records(8, whole));
    assert_eq!(frame.slots.get(&1), Some(&16), "{frame:?}");
    assert_eq!(check(&allocated, &frame), None);
    // Two aligned words move the same eight bytes.
    let words = "    str.w %v0, [%v4, #0] [transport]
    %v9:gpr64 = lsr.x %v0, #32
    str.w %v9, [%v4, #4] [transport]";
    assert_eq!(checked(&two_records(4, words)), None);
    // An alignment below the width makes emission move single bytes.
    let bytes = "    str.x %v0, [%v4, #0] [align=4] [transport]";
    assert_eq!(checked(&two_records(4, bytes)), None);
}

/// An access that EvoIR performs is aligned by its guard, which traps in the
/// EvoIR interpreter when it fails; a transport access has no guard.
#[test]
fn only_an_evoir_access_keeps_the_alignment_of_its_guard() {
    let guarded = "    str.x %v0, [%v4, #0]";
    assert_eq!(checked(&two_records(4, guarded)), None);
    let claimed = "    str.x %v0, [%v4, #0] [align=8] [transport]";
    assert_eq!(
        checked(&two_records(4, claimed)).as_deref(),
        Some("str.x %v0, [%v4, #0] [transport] moves 8 bytes at alignment 4")
    );
}

/// An address keeps the alignment of its object and of every offset added to
/// it, through copies and block arguments.
#[test]
fn derived_addresses_keep_the_alignment_of_their_offsets() {
    let derived = |step: u64, displacement: i64| {
        format!(
            "func @derived {{
^entry(%v0:gpr64):
    %v1:gpr64 = frame.slot [origin=1, size=32, align=16]
    %v2:gpr64 = add.x %v1, #{step}
    str.x %v0, [%v2, #{displacement}] [transport]
    ret
}}
"
        )
    };
    assert_eq!(checked(&derived(8, 0)), None);
    assert_eq!(checked(&derived(4, 4)), None);
    assert_eq!(
        checked(&derived(4, 0)).as_deref(),
        Some("str.x %v0, [%v2, #0] [transport] moves 8 bytes at alignment 4")
    );
    let joined = |other: u64| {
        format!(
            "func @joined {{
^entry(%v0:gpr64, %v1:gpr64):
    %v2:gpr64 = frame.slot [origin=1, size=32, align=16]
    %v3:gpr64 = add.x %v2, #8
    %v4:gpr64 = add.x %v2, #{other}
    cbz %v1, ^b1(%v0, %v3), ^b1(%v0, %v4)

^b1(%v5:gpr64, %v6:gpr64):
    str.x %v5, [%v6, #0] [transport]
    ret
}}
"
        )
    };
    assert_eq!(checked(&joined(16)), None);
    let refused = checked(&joined(12)).expect("one edge gives alignment 4");
    assert!(
        refused.ends_with("moves 8 bytes at alignment 4"),
        "{refused}"
    );
}

/// A pointer that the function loads or receives has no derived alignment.
#[test]
fn a_transport_access_through_an_unknown_pointer_is_refused() {
    let text = "func @unknown {
^entry(%v0:gpr64, %v1:gpr64):
    str.w %v1, [%v0, #0] [transport]
    ret
}
";
    assert_eq!(
        checked(text).as_deref(),
        Some("str.w %v1, [%v0, #0] [transport] moves 4 bytes at alignment 1")
    );
    let bytes = text.replace("str.w", "strb");
    assert_eq!(checked(&bytes), None);
}

/// One function with a slot of each kind of frame fact: an 8-aligned object.
const FRAME: &str = "func @frame {
^entry(%v0:gpr64):
    %v1:gpr64 = frame.slot [origin=1, size=8, align=8]
    str.x %v0, [%v1, #0]
    %v2:gpr64 = ldr.x [%v1, #0]
    ret %v2
}
";

#[test]
fn a_frame_that_misplaces_a_slot_is_refused() {
    let (allocated, frame) = allocated(FRAME);
    assert_eq!(check(&allocated, &frame), None);

    let mut moved = frame.clone();
    moved.slots.insert(1, 12);
    assert_eq!(
        check(&allocated, &moved).as_deref(),
        Some("frame object 1 at frame offset 12 has alignment 4, not its declared 8")
    );

    let mut size = frame.clone();
    size.size += 8;
    let refused = check(&allocated, &size).expect("sp loses its alignment");
    assert!(
        refused.contains("does not keep sp 16-byte aligned"),
        "{refused}"
    );

    let mut saved = frame.clone();
    saved.saved.insert(19, 20);
    assert_eq!(
        check(&allocated, &saved).as_deref(),
        Some("the save slot of x19 at frame offset 20 moves 8 bytes at alignment 4")
    );

    let mut variadic = frame.clone();
    variadic.register_save = Some((0, 8));
    assert_eq!(
        check(&allocated, &variadic).as_deref(),
        Some("the variadic save slot of q0 at frame offset 72 moves 16 bytes at alignment 8")
    );

    let mut guard = frame.clone();
    guard.stack_guard = Some((
        16,
        StackGuard::SystemRegister {
            register: "sp_el0".into(),
            offset: 1236,
        },
    ));
    assert_eq!(
        check(&allocated, &guard).as_deref(),
        Some("the guard at sp_el0 + 1236 moves 8 bytes at alignment 4")
    );
    guard.stack_guard = Some((
        16,
        StackGuard::SystemRegister {
            register: "sp_el0".into(),
            offset: 1240,
        },
    ));
    assert_eq!(check(&allocated, &guard), None);
}

/// Spill slots and stack arguments are frame accesses of the register's width.
#[test]
fn spills_and_stack_arguments_need_the_width_of_their_register() {
    let (allocated, frame) = allocated(FRAME);
    let quad = VReg::new(900, RegClass::Simd128);
    let with = |inst: MInst| {
        let mut changed = allocated.clone();
        changed.blocks[0].insts.insert(0, inst);
        changed
    };

    let spilled = with(MInst::SpillReload { dst: quad, slot: 0 });
    let mut spills = frame.clone();
    spills.spills.insert(0, 8);
    assert_eq!(
        check(&spilled, &spills).as_deref(),
        Some("the simd128 access of spill slot 0 at frame offset 8 moves 16 bytes at alignment 8")
    );
    spills.spills.insert(0, 16);
    assert_eq!(check(&spilled, &spills), None);
    let mut lost = frame.clone();
    lost.spills.clear();
    assert_eq!(
        check(&spilled, &lost).as_deref(),
        Some("spill slot 0 has no offset in the frame")
    );

    let incoming = with(MInst::IncomingArg { dst: quad, slot: 1 });
    assert_eq!(
        check(&incoming, &frame).as_deref(),
        Some("incoming stack argument 1 at x29 + 24 moves 16 bytes at alignment 8")
    );
    let incoming = with(MInst::IncomingArg { dst: quad, slot: 2 });
    assert_eq!(check(&incoming, &frame), None);

    let outgoing = with(MInst::OutgoingArg { src: quad, slot: 3 });
    assert_eq!(
        check(&outgoing, &frame).as_deref(),
        Some("outgoing stack argument 3 at sp + 24 moves 16 bytes at alignment 8")
    );
}
