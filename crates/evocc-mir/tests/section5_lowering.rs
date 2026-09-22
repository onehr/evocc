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

/// The mnemonics of a lowered module, without the result extension.
///
/// Section 6 has a function extend a result narrower than 32 bits before it
/// returns, which the narrow-result probe measured. That extension belongs to
/// the boundary rather than to the Section 5 row under test, so a row's
/// expectation names the row; `mnemonics_with_boundary` is what sees both.
fn mnemonics(module: &evocc_mir::MModule) -> Vec<String> {
    let mut names = mnemonics_with_boundary(module);
    for function in &module.functions {
        for block in &function.blocks {
            let MTerm::Ret { .. } = block.term else {
                continue;
            };
            // The extension is the last instruction before the return, and
            // only when the lowering inserted one.
            if let Some(last) = block.insts.last() {
                let extension = matches!(
                    last,
                    MInst::BinaryImm {
                        op: evocc_mir::BinOp::And,
                        ..
                    } | MInst::Extend { .. }
                );
                if extension && names.last().is_some() {
                    names.pop();
                }
            }
        }
    }
    names
}

fn mnemonics_with_boundary(module: &evocc_mir::MModule) -> Vec<String> {
    let mut names = Vec::new();
    for function in &module.functions {
        for block in &function.blocks {
            for inst in &block.insts {
                names.push(match inst {
                    MInst::AtomicRmw { operation, .. } => {
                        format!("atomic.rmw.{}", operation.mnemonic())
                    }
                    MInst::StackRelease { .. } => "stack.release".to_owned(),
                    MInst::Vector { extract, .. } => {
                        if *extract {
                            "vec.extract".into()
                        } else {
                            "vec.pack".into()
                        }
                    }
                    MInst::Atomic { load, .. } => {
                        if *load {
                            "atomic.load".to_owned()
                        } else {
                            "atomic.store".to_owned()
                        }
                    }
                    MInst::Asm(_) => "asm".to_owned(),
                    MInst::Mov { .. } => "mov".to_owned(),
                    MInst::MovImm { .. } => "mov#".to_owned(),
                    MInst::Adrp { .. } => "adrp".to_owned(),
                    MInst::Adr { .. } => "adr".to_owned(),
                    MInst::AddPageOff { .. } => "add@".to_owned(),
                    MInst::Load { kind, .. } => kind.mnemonic().to_owned(),
                    MInst::Store { kind, .. } => kind.mnemonic().to_owned(),
                    MInst::Binary { op, .. } => op.mnemonic().to_owned(),
                    MInst::BinaryImm { op, .. } => format!("{}#", op.mnemonic()),
                    MInst::Msub { .. } => "msub".to_owned(),
                    MInst::Madd { .. } => "madd".to_owned(),
                    MInst::IncomingArg { .. } => "arg.incoming".to_owned(),
                    MInst::OutgoingArg { .. } => "arg.outgoing".to_owned(),
                    MInst::CarryBinary { op, .. } => op.mnemonic().to_owned(),
                    MInst::SubCarryFlags { .. } => "sbcs".to_owned(),
                    MInst::AddCarryFlags { .. } => "adcs".to_owned(),
                    MInst::TstImm { .. } => "tst".to_owned(),
                    MInst::Extend { op, .. } => op.mnemonic().to_owned(),
                    MInst::Unary { op, .. } => op.mnemonic().to_owned(),
                    MInst::Bfx { signed: true, .. } => "sbfx".to_owned(),
                    MInst::Bfx { signed: false, .. } => "ubfx".to_owned(),
                    MInst::Cmp { .. } => "cmp".to_owned(),
                    MInst::CmpImm { .. } => "cmp#".to_owned(),
                    MInst::Cset { .. } => "cset".to_owned(),
                    MInst::Csel { .. } => "csel".to_owned(),
                    MInst::Bl { .. } => "bl".to_owned(),
                    MInst::Blr { .. } => "blr".to_owned(),
                    MInst::FlagBinary { op, .. } => op.mnemonic().to_owned(),
                    MInst::WideMultiply { op, .. } => op.mnemonic().to_owned(),
                    MInst::MemCopy { .. } => "mem.copy".to_owned(),
                    MInst::Ldp { .. } => "ldp".to_owned(),
                    MInst::Stp { .. } => "stp".to_owned(),
                    MInst::SpillStore { .. } => "spill.store".to_owned(),
                    MInst::SpillReload { .. } => "spill.reload".to_owned(),
                    MInst::FrameSlot { .. } => "frame.slot".to_owned(),
                    MInst::StackAlloc { .. } => "stack.alloc".to_owned(),
                    MInst::FrameAddress { place, .. } => {
                        format!("frame.address.{}", place.mnemonic())
                    }
                    MInst::VaSaveArea { .. } => "va.save".to_owned(),
                    MInst::IndirectResult => "indirect.result".to_owned(),
                    MInst::Aes { op, .. } => format!("crypto.{}", op.mnemonic()),
                    MInst::Floating { op, .. } => format!("fp.{}", op.mnemonic()),
                    MInst::FpEnvCheck => "fp.env.check".to_owned(),
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
fn vector_representation_and_lane_size_survive_machine_text() {
    let source=ADD.replace("may_trap=false","may_trap=true")
        .replace("    ret %v4, %v1, %v2", "    %v5 = vec.pack %v0, %v3, %v4, %v0 : vec<4, bits<32>>\n    %v6 = const.bits<32> 2\n    %v7 = vec.extract %v5, %v6 : bits<32>\n    ret %v7, %v1, %v2");
    let module = lower(&source);
    let text = render_module(&module);
    let parsed = evocc_mir::parse_module(&text).unwrap();
    assert_eq!(parsed, module);
    let changed = text.replace("vec.pack.32.16", "vec.pack.32.8");
    assert_ne!(changed, text);
    assert!(evocc_mir::parse_module(&changed)
        .and_then(|m| validate_module(&m))
        .is_err());
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
    // The shift amount is itself a narrow value, so Section 3's fourth row
    // masks it too: "and with 2^M - 1 when M < 32".
    assert_eq!(
        mnemonics(&lower(UNSIGNED_NARROW)),
        vec!["mov#", "and#", "and#", "lsr"]
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
        vec!["mov#", "sbfx", "and#", "asr"]
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

/// A function whose only instruction is the named predicate, so the mnemonic
/// list is the row of Section 5 and nothing else.
fn predicate(width: u16, body: &str, effects_trap: bool) -> Vec<String> {
    let trap = if effects_trap { "true" } else { "false" };
    let text = [
        format!("func @main(%arg0: bits<{width}>, %arg1: bits<{width}>, %arg2: mem<all>, %arg3: world) -> (bits<1>, mem<all>, world) cc = target_default"),
        format!("    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, may_trap={trap}, may_unwind=false }}"),
        "{".to_owned(),
        format!("^entry(%v0: bits<{width}>, %v1: bits<{width}>, %v2: mem<all>, %v3: world):"),
        format!("    {body}"),
        "    ret %v4, %v2, %v3".to_owned(),
        "}".to_owned(),
        String::new(),
    ]
    .join("\n");
    mnemonics(&lower(&text))
}

#[test]
fn a_divisor_check_lowers_to_a_compare_and_a_cset() {
    // Section 5: "`divisor_nonzero %d` | `cmp %d, #0` then `cset ne`".
    assert_eq!(
        predicate(32, "%v4 = divisor_nonzero %v1", false),
        vec!["cmp#", "cset"]
    );
    // A narrow divisor is normalized first, because `cmp` reads every bit of
    // the register.
    assert_eq!(
        predicate(13, "%v4 = divisor_nonzero %v1", false),
        vec!["and#", "cmp#", "cset"]
    );
}

#[test]
fn the_division_overflow_check_lowers_to_two_comparisons_and_an_or() {
    // Section 5: "`cmp %a, #INT_MIN` (materialized), `cset ne` into `%p`;
    // `cmp %b, #-1`, `cset ne` into `%q`; `orr %p, %q`".
    assert_eq!(
        predicate(32, "%v4 = signed_div_rem_no_overflow %v0, %v1", false),
        vec!["mov#", "cmp", "cset", "mov#", "cmp", "cset", "orr"]
    );
}

#[test]
fn the_additive_overflow_checks_use_the_flag_setting_forms_at_the_register_width() {
    // Section 5: "`adds`/`subs` into a scratch, then `cset vc`".
    assert_eq!(
        predicate(32, "%v4 = signed_add_defined %v0, %v1", false),
        vec!["adds", "cset"]
    );
    assert_eq!(
        predicate(64, "%v4 = signed_sub_defined %v0, %v1", false),
        vec!["subs", "cset"]
    );
}

#[test]
fn the_additive_overflow_checks_test_the_source_width_when_it_is_narrower() {
    // The `v` flag reports overflow of the register. A narrow value needs the
    // bitfield comparison the multiply row already uses.
    assert_eq!(
        predicate(13, "%v4 = signed_add_defined %v0, %v1", false),
        vec!["sbfx", "sbfx", "add", "sbfx", "cmp", "cset"]
    );
}

#[test]
fn the_multiply_overflow_check_uses_the_wide_products() {
    // Section 5: 32-bit uses `smull` and compares with the sign extension of
    // its low bits; 64-bit uses `mul` and `smulh` against `asr` by 63.
    assert_eq!(
        predicate(32, "%v4 = signed_mul_defined %v0, %v1", false),
        vec!["smull", "sbfx", "cmp", "cset"]
    );
    assert_eq!(
        predicate(64, "%v4 = signed_mul_defined %v0, %v1", false),
        vec!["mul", "smulh", "asr#", "cmp", "cset"]
    );
}

#[test]
fn the_shift_count_check_compares_the_amount_with_the_shifted_width() {
    // Section 5: "zero-extend `%amt` per Section 3, `cmp` with `#N`, `cset lo`".
    assert_eq!(
        predicate(32, "%v4 = shift_count_defined.unsigned %v0, %v1", false),
        vec!["cmp#", "cset"]
    );
}

#[test]
fn the_signed_left_shift_check_adds_the_non_negative_operand_condition() {
    // EvoIR Section 3.3.1 makes the predicate false for a negative left
    // operand; Section 5's row states only the count and the round trip.
    let names = predicate(32, "%v4 = signed_shl_defined.unsigned %v0, %v1", false);
    assert!(names.contains(&"lsl".to_owned()), "{names:?}");
    assert!(names.contains(&"asr".to_owned()), "{names:?}");
    assert_eq!(
        names.iter().filter(|name| *name == "cset").count(),
        3,
        "the count, the sign and the round trip: {names:?}"
    );
    assert_eq!(
        names.iter().filter(|name| *name == "and").count(),
        2,
        "their conjunction: {names:?}"
    );
}

const OBJECT_COPY: &str = "\
func @main(%arg0: bits<64>, %arg1: mem<all>, %arg2: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<64>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = alloca %v1, %v2 [origin=0, size=16, align=8]
    (%v6, %v7, %v8) = alloca %v4, %v5 [origin=1, size=16, align=8]
    (%v9, %v10) = copy %v7, %v8, %v6, %v3 [size=16, align=8]
    ret %v0, %v9, %v10
}
";

#[test]
fn an_object_copy_lowers_to_the_mem_copy_pseudo_form() {
    // Section 5: "`copy` with static size | `mem.copy`".
    assert_eq!(
        mnemonics(&lower(OBJECT_COPY)),
        vec!["frame.slot", "frame.slot", "mem.copy"]
    );
}

const THREE_RESULTS: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, bits<32>, bits<1>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = add.bits<32> %v0, %v0
    %v4 = icmp.ult %v0, %v3
    ret %v0, %v3, %v4, %v1, %v2
}
";

#[test]
fn three_ordinary_results_lower_to_a_three_register_return() {
    // Section 6: "an EvoMIR function returns up to eight integer or pointer
    // results in x0 to x7 in order."
    let lowered = lower(THREE_RESULTS);
    let evocc_mir::MTerm::Ret { values } = &lowered.functions[0].blocks[0].term else {
        panic!("the entry block returns");
    };
    assert_eq!(values.len(), 3);
}
