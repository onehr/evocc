//! Section 5: the lowering table, row by row, and the Section 3 extensions at
//! exactly the points the table names.
//!
//! Each fixture is canonical EvoIR Section 9 text, so the lowering is exercised
//! as what Section 1 says it is: a function of the extracted EvoIR text.

use evocc_evoir::parse_section9_module;
use evocc_mir::{lower_module, render_module, validate_module, MInst, MTerm};

fn lower(text: &str) -> evocc_mir::MModule {
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let lowered = lower_module(&module).expect("every form is in the Section 5 table");
    validate_module(&lowered).expect("the lowering produces verifiable EvoMIR");
    lowered
}

fn mnemonics(module: &evocc_mir::MModule) -> Vec<String> {
    let mut names = Vec::new();
    for function in &module.functions {
        for block in &function.blocks {
            for inst in &block.insts {
                names.push(match inst {
                    MInst::Mov { .. } => "mov".to_owned(),
                    MInst::MovImm { .. } => "mov#".to_owned(),
                    MInst::Adrp { .. } => "adrp".to_owned(),
                    MInst::AddPageOff { .. } => "add@".to_owned(),
                    MInst::Load { kind, .. } => kind.mnemonic().to_owned(),
                    MInst::Store { kind, .. } => kind.mnemonic().to_owned(),
                    MInst::Binary { op, .. } => op.mnemonic().to_owned(),
                    MInst::BinaryImm { op, .. } => format!("{}#", op.mnemonic()),
                    MInst::Msub { .. } => "msub".to_owned(),
                    MInst::Extend { op, .. } => op.mnemonic().to_owned(),
                    MInst::Bfx { signed: true, .. } => "sbfx".to_owned(),
                    MInst::Bfx { signed: false, .. } => "ubfx".to_owned(),
                    MInst::Cmp { .. } => "cmp".to_owned(),
                    MInst::CmpImm { .. } => "cmp#".to_owned(),
                    MInst::Cset { .. } => "cset".to_owned(),
                    MInst::Csel { .. } => "csel".to_owned(),
                    MInst::Bl { .. } => "bl".to_owned(),
                    MInst::Blr { .. } => "blr".to_owned(),
                    MInst::FrameSlot { .. } => "frame.slot".to_owned(),
                });
            }
        }
    }
    names
}

const ADD: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 7
    %v4 = add.bits<32> %v0, %v3
    ret %v4, %v1, %v2
}
";

#[test]
fn a_constant_lowers_to_a_move_immediate_and_add_to_add() {
    assert_eq!(mnemonics(&lower(ADD)), vec!["mov#", "add"]);
}

#[test]
fn the_memory_and_world_arguments_do_not_occupy_registers() {
    // Section 5: "EvoIR's mem and world tokens are consumed here, and the order
    // they imposed becomes program order."
    let lowered = lower(ADD);
    assert_eq!(lowered.functions[0].blocks[0].args.len(), 1);
}

const UNSIGNED_NARROW: &str = "\
func @main(%arg0: bits<13>, %arg1: mem<all>, %arg2: world) -> (bits<13>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<13>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<13> 3
    %v4 = lshr.bits<13> %v0, %v3
    ret %v4, %v1, %v2
}
";

#[test]
fn lshr_on_a_narrow_value_zero_extends_first() {
    // Section 3: lshr needs its operand "zero-extended to the register width",
    // inserted as "and with the mask 2^N - 1".
    assert_eq!(
        mnemonics(&lower(UNSIGNED_NARROW)),
        vec!["mov#", "and#", "lsr"]
    );
}

const SIGNED_NARROW: &str = "\
func @main(%arg0: bits<13>, %arg1: mem<all>, %arg2: world) -> (bits<13>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<13>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<13> 3
    %v4 = ashr.bits<13> %v0, %v3
    ret %v4, %v1, %v2
}
";

#[test]
fn ashr_on_a_narrow_value_sign_extends_first() {
    // Section 3: "sbfx from bit N - 1".
    assert_eq!(
        mnemonics(&lower(SIGNED_NARROW)),
        vec!["mov#", "sbfx", "asr"]
    );
}

const BYTE_NARROW: &str = "\
func @main(%arg0: bits<8>, %arg1: mem<all>, %arg2: world) -> (bits<8>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<8>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<8> 3
    %v4 = ashr.bits<8> %v0, %v3
    %v5 = lshr.bits<8> %v4, %v3
    ret %v5, %v1, %v2
}
";

#[test]
fn a_width_of_eight_uses_the_byte_extension_spellings() {
    // Section 3 names sxtb/uxtb "when N is 8 or 16".
    let names = mnemonics(&lower(BYTE_NARROW));
    assert!(names.contains(&"sxtb".to_owned()), "{names:?}");
    assert!(names.contains(&"uxtb".to_owned()), "{names:?}");
}

const REMAINDER: &str = "\
func @main(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = urem.bits<32> %v0, %v1
    ret %v4, %v2, %v3
}
";

#[test]
fn a_remainder_lowers_to_a_divide_and_an_msub() {
    assert_eq!(mnemonics(&lower(REMAINDER)), vec!["udiv", "msub"]);
}

const COMPARE_THEN_BRANCH: &str = "\
func @main(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = icmp.ult %v0, %v1
    cond_br %v4, ^b1(%v0, %v2, %v3), ^b1(%v1, %v2, %v3)

^b1(%v5: bits<32>, %v6: mem<all>, %v7: world):
    ret %v5, %v6, %v7
}
";

#[test]
fn a_comparison_consumed_only_by_a_branch_fuses_into_b_cond() {
    // Section 5: "when the only consumer is a cond_br in the same block, cmp
    // then b.cond with no cset".
    let lowered = lower(COMPARE_THEN_BRANCH);
    assert_eq!(mnemonics(&lowered), vec!["cmp"]);
    assert!(matches!(
        lowered.functions[0].blocks[0].term,
        MTerm::BCond { .. }
    ));
}

const COMPARE_THEN_RETURN: &str = "\
func @main(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<1>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = icmp.slt %v0, %v1
    ret %v4, %v2, %v3
}
";

#[test]
fn a_comparison_with_another_consumer_lowers_to_cmp_then_cset() {
    assert_eq!(mnemonics(&lower(COMPARE_THEN_RETURN)), vec!["cmp", "cset"]);
}

const BRANCH_ON_A_BIT: &str = "\
func @main(%arg0: bits<1>, %arg1: mem<all>, %arg2: world) -> (bits<1>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<1>, %v1: mem<all>, %v2: world):
    cond_br %v0, ^b1(%v0, %v1, %v2), ^b1(%v0, %v1, %v2)

^b1(%v3: bits<1>, %v4: mem<all>, %v5: world):
    ret %v3, %v4, %v5
}
";

#[test]
fn an_unfused_condition_branches_on_bit_zero() {
    // Section 3: "cond_br on bits<1> ... nothing: tbnz/tbz on bit 0".
    let lowered = lower(BRANCH_ON_A_BIT);
    assert!(matches!(
        lowered.functions[0].blocks[0].term,
        MTerm::Tb {
            nonzero: true,
            bit: 0,
            ..
        }
    ));
}

const MEMORY: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = alloca %v1, %v2 [origin=0, size=4, align=4]
    %v6 = access_in_bounds %v4, %v3 [size=4, align=4]
    (%v7, %v8) = store %v6, %v5, %v3, %v0
    %v9 = access_in_bounds %v7, %v3 [size=4, align=4]
    (%v10, %v11, %v12) = load %v9, %v8, %v3 : bits<32>
    ret %v10, %v11, %v12
}
";

#[test]
fn an_allocation_a_store_and_a_load_lower_to_a_slot_and_a_pair_of_accesses() {
    // Section 5 lowers pointer_valid and access_in_bounds to nothing.
    assert_eq!(
        mnemonics(&lower(MEMORY)),
        vec!["frame.slot", "str.w", "ldr.w"]
    );
}

const GLOBAL: &str = "\
global @counter origin=0 kind=file visibility=external size=4 align=4 bytes=00000000 initialized=1111

func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = global_addr @counter : ptr<generic>
    %v4 = access_in_bounds %v1, %v3 [size=4, align=4]
    (%v5, %v6, %v7) = load %v4, %v2, %v3 : bits<32>
    ret %v5, %v6, %v7
}
";

#[test]
fn a_global_address_lowers_to_the_page_relative_pair() {
    assert_eq!(mnemonics(&lower(GLOBAL)), vec!["adrp", "add@", "ldr.w"]);
}

const UB_ASSUME: &str = "\
func @main(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = icmp.ne %v0, %v1
    ub.assume.division_by_zero %v4
    %v5 = add.bits<32> %v0, %v1
    ret %v5, %v2, %v3
}
";

#[test]
fn an_obligation_becomes_a_checked_branch_to_a_trap() {
    // Section 5: "checked profile: tbz %c, #0, trap_label where the label holds
    // brk #imm for the reason".
    let lowered = lower(UB_ASSUME);
    let function = &lowered.functions[0];
    assert!(matches!(
        function.blocks[0].term,
        MTerm::Tb {
            nonzero: false,
            bit: 0,
            ..
        }
    ));
    assert!(
        function
            .blocks
            .iter()
            .any(|block| matches!(block.term, MTerm::Brk { imm: 3002 })),
        "the trap block carries the division-by-zero reason"
    );
}

const TRAP: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    trap.unreachable_reached
}
";

#[test]
fn a_trap_becomes_a_brk_with_the_reason_number() {
    let lowered = lower(TRAP);
    assert!(matches!(
        lowered.functions[0].blocks[0].term,
        MTerm::Brk { imm: 3024 }
    ));
}

const CALL: &str = "\
func @helper(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    ret %v0, %v1, %v2
}

func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call @helper(%v0, %v1, %v2)
    ret %v3, %v4, %v5
}
";

#[test]
fn a_direct_call_lowers_to_bl_with_explicit_argument_and_result_registers() {
    let lowered = lower(CALL);
    assert_eq!(mnemonics(&lowered), vec!["bl"]);
}

#[test]
fn the_lowering_is_a_function_of_the_evoir_text() {
    // Section 1: "two runs over the same EvoIR text produce the same EvoMIR
    // text, byte for byte."
    for fixture in [
        ADD,
        UNSIGNED_NARROW,
        SIGNED_NARROW,
        REMAINDER,
        COMPARE_THEN_BRANCH,
        MEMORY,
        GLOBAL,
        UB_ASSUME,
        CALL,
    ] {
        let first = render_module(&lower(fixture));
        let second = render_module(&lower(fixture));
        assert_eq!(first, second);
    }
}

#[test]
fn every_lowering_round_trips_through_the_textual_form() {
    for fixture in [ADD, UNSIGNED_NARROW, MEMORY, GLOBAL, UB_ASSUME, CALL] {
        let lowered = lower(fixture);
        let text = render_module(&lowered);
        let parsed = evocc_mir::parse_module(&text).expect("parse");
        assert_eq!(parsed, lowered);
    }
}
