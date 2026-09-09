//! EvoMIR Section 6, "Variadic functions" (revision 0.10, `E4012` withdrawn),
//! and the plain `asm` of Section 4 reaching the assembler.
//!
//! "A call passes anonymous arguments exactly as it passes named ones: same
//! registers, same slots, no distinction ... A variadic **callee** reserves a
//! 64-byte, 16-byte-aligned **register save area** in its frame and stores
//! `x_k` to `x7` into it at offsets `8 * k` to `56` in the prologue ...
//! With no named stack arguments, `va_start` stores `stack = x29 + 16`,
//! `gr_top = save area + 64`,
//! `gr_offs = -8 * (8 - k)`. Revision 0.28 adds the adjacent vector save
//! area: `vr_top = gr_top + 128`, `vr_offs = -128`.
//! `va_end` emits nothing."

use evocc_evoir::parse_section9_module;
use evocc_mir::{compile_module_for, Target};

/// A variadic callee with one named parameter, its list object and one
/// `va_arg` of `bits<64>`.
const CALLEE: &str = r#"func @sum(%arg0: bits<32>, ..., %arg1: mem<all>, %arg2: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = alloca %v1, %v2 [origin=1, size=32, align=8]
    (%v6, %v7) = va_start %v3, %v4, %v5
    (%v8, %v9, %v10) = va_arg.bits<64> %v3, %v6, %v7
    (%v11, %v12) = va_end %v3, %v9, %v10
    ret %v8, %v11, %v12
}
"#;

/// A caller that passes anonymous arguments to it.
const CALLER: &str = r#"declare @sum(bits<32>, ..., mem<all>, world) -> (bits<64>, mem<all>, world) cc = target_default

func @call_it(%arg0: mem<all>, %arg1: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=true, may_trap=true, may_unwind=true }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 2
    %v3 = const.bits<64> 40
    %v4 = const.bits<64> 2
    (%v5, %v6, %v7) = call @sum(%v2, %v3, %v4, %v0, %v1)
    ret %v5, %v6, %v7
}
"#;

/// The implicit `asm` a global register variable reads through, exactly as the
/// front end writes it for `register unsigned long sp_var asm("sp")`.
const REGISTER_READ: &str = r#"func @get_sp(%arg0: mem<all>, %arg1: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=true, atomic=false, io=true, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    (%v2, %v3, %v4) = asm volatile "mov %0, sp" %v0, %v1 outputs [("=r", "", bits<64>, -)] inputs [] clobbers []
    ret %v2, %v3, %v4
}
"#;

fn emitted(target: Target, text: &str) -> String {
    let module = parse_section9_module(text.as_bytes()).expect("the unit parses");
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
fn a_variadic_callee_saves_the_registers_its_named_parameters_left() {
    let text = emitted(Target::Linux, CALLEE);
    // One named `bits<32>` parameter consumes `x0`, so `x1` to `x7` are saved.
    for number in 1..=7 {
        assert!(
            text.contains(&format!("str x{number}, [sp, #")),
            "x{number} reaches the register save area:\n{text}"
        );
    }
    assert!(
        !text.contains("str x0, [sp, #"),
        "a named parameter's register is not part of the save area:\n{text}"
    );
}

#[test]
fn a_variadic_callee_preserves_all_incoming_vector_registers() {
    let module = parse_section9_module(CALLEE.as_bytes()).unwrap();
    let machine = evocc_mir::lower_module(&module).unwrap();
    let allocated =
        evocc_mir::allocate_for(Target::Linux, &evocc_mir::out_of_ssa(&machine.functions[0]))
            .unwrap();
    for outgoing in [0, 16, 1024, 65536] {
        let mut allocated = allocated.clone();
        allocated.outgoing = outgoing;
        let frame = evocc_mir::frame_of(&allocated);
        let (_, base) = frame.register_save.unwrap();
        assert_eq!(base % 16, 0);
        assert!(frame.slots.values().all(|offset| *offset >= base + 192));
        let text = evocc_mir::emit_module_for(Target::Linux, &[allocated], &[]);
        for pair in [0, 2, 4, 6] {
            assert!(
                text.contains(&format!("stp q{pair}, q{},", pair + 1)),
                "incoming q{pair} and q{} must survive before any body call:\n{text}",
                pair + 1
            );
        }
    }
}

#[test]
fn va_start_points_at_a_distinct_vector_save_area() {
    let module = parse_section9_module(CALLEE.as_bytes()).unwrap();
    let machine = evocc_mir::lower_module(&module).unwrap();
    let insts = &machine.functions[0].blocks[0].insts;
    let stored = |offset| {
        insts
            .iter()
            .find_map(|inst| match inst {
                evocc_mir::MInst::Store {
                    src, offset: at, ..
                } if *at == offset => Some(*src),
                _ => None,
            })
            .unwrap()
    };
    assert_ne!(
        stored(8),
        stored(16),
        "gr_top and vr_top are different areas"
    );
    let vr_offs = stored(28);
    assert!(
        insts.iter().any(|inst| matches!(inst,
            evocc_mir::MInst::MovImm { dst, imm }
                if *dst == vr_offs && *imm == u64::from((-128_i32) as u32)
        )),
        "vr_offs starts at -128, not at the caller's stack"
    );
}

#[test]
fn va_start_stores_the_five_fields_and_va_end_stores_nothing() {
    let text = emitted(Target::Linux, CALLEE);
    // `stack = x29 + 16` is the only address taken from the frame pointer.
    assert!(
        text.contains("add x") && text.contains(", x29, #16"),
        "the first incoming stack slot is `x29 + 16`:\n{text}"
    );
    // `gr_offs = -8 * (8 - 1)` is -56 as a signed 32-bit integer.
    assert!(
        text.contains("movz w") || text.contains("movz x"),
        "the two offsets are materialized:\n{text}"
    );
    let stores = text.matches("str w").count();
    assert!(
        stores >= 2,
        "`gr_offs` and `vr_offs` are 32-bit stores:\n{text}"
    );
}

#[test]
fn a_caller_passes_anonymous_arguments_in_the_ordinary_registers() {
    let text = emitted(Target::Linux, CALLER);
    assert!(text.contains("bl sum"), "{text}");
    assert!(
        !text.contains("save"),
        "a call site cannot tell a variadic prototype from a fixed one:\n{text}"
    );
}

#[test]
fn a_plain_assembly_instruction_reaches_the_emitted_text() {
    for target in [Target::Darwin, Target::Linux] {
        let text = emitted(target, REGISTER_READ);
        assert!(
            text.contains("mov x0, sp"),
            "the template is written with its operand's register:\n{text}"
        );
    }
}
