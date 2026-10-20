//! Constant address selection must decode the index before scaling it.
use evocc_mir::{BinOp, MInst, MirLimits, MirValue, Target, Width};

fn source(bits: u16, value: u128, sign: &str, direction: &str, scale: u64) -> String {
    format!(
        "func @offset(%arg0: ptr<generic>, %arg1: mem<all>, %arg2: world) -> (ptr<generic>, mem<all>, world) cc = target_default
    effects {{ reads={{all}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }}
{{
^entry(%v0: ptr<generic>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<{bits}> {value}
    %v4 = ptr.offset.{direction}.{sign} %v0, %v3 [scale={scale}]
    %v5 = pointer_valid %v1, %v4
    ret %v4, %v5, %v2
}}
"
    )
}

fn lower(text: &str) -> evocc_mir::MModule {
    let ir = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    evocc_evoir::validate_module(&ir).unwrap();
    let mir = evocc_mir::lower_module_for(Target::Linux, &ir, &|_| false).unwrap();
    evocc_mir::validate_module(&mir).unwrap();
    mir
}

#[test]
fn small_displacements_use_one_immediate_or_alias_the_base() {
    for (bits, value, sign, direction, scale, expected) in [
        (32, 7, "signed", "add", 8, 56),
        (32, 0xffff_ffff, "signed", "add", 8, -8),
        (8, 255, "signed", "sub", 3, 3),
        (8, 255, "unsigned", "add", 3, 765),
        (64, 4095, "unsigned", "add", 1, 4095),
        (64, 4095, "unsigned", "sub", 1, -4095),
        (64, 0, "signed", "sub", u64::MAX, 0),
        (1, 1, "signed", "add", 3, -3),
    ] {
        let text = source(bits, value, sign, direction, scale);
        let mir = lower(&text);
        let function = evocc_mir::regalloc::checked_out_of_ssa(&mir.functions[0], true).unwrap();
        evocc_mir::verify_allocation(&evocc_mir::allocate_for(Target::Linux, &function).unwrap())
            .unwrap();
        let insts = &mir.functions[0].blocks[0].insts;
        let arithmetic: Vec<_> = insts
            .iter()
            .filter(|inst| !matches!(inst, MInst::MovImm { .. }))
            .collect();
        if expected == 0 {
            assert!(arithmetic.is_empty(), "{text}: {arithmetic:?}");
        } else {
            assert!(
                matches!(arithmetic.as_slice(), [MInst::BinaryImm {op, imm, width: Width::X, ..}]
                if *op == if expected < 0 { BinOp::Sub } else { BinOp::Add }
                && *imm == i64::unsigned_abs(expected)),
                "{text}: {arithmetic:?}"
            );
        }
    }
}

#[test]
fn all_index_widths_keep_machine_address_arithmetic() {
    // This is the machine address contract, not an EvoIR provenance claim.
    // These raw addresses are never dereferenced. Out-of-object C pointer
    // formation is not made defined by this test or by immediate selection.
    for bits in 1..=64 {
        let sign_bit = 1_u64 << (bits - 1);
        let mask = u64::MAX >> (64 - bits);
        for raw in [0, 1, sign_bit - 1, sign_bit, mask] {
            for signed in [false, true] {
                let decoded = if signed && raw & sign_bit != 0 {
                    i128::from(raw) - (1_i128 << bits)
                } else {
                    i128::from(raw)
                };
                for subtract in [false, true] {
                    for scale in [1, 3, 8, u64::MAX] {
                        let text = source(
                            bits,
                            raw.into(),
                            if signed { "signed" } else { "unsigned" },
                            if subtract { "sub" } else { "add" },
                            scale,
                        );
                        let mir = lower(&text);
                        let delta = (decoded as u64).wrapping_mul(scale);
                        for base in [8192_u64, u64::MAX - 1] {
                            let expected = if subtract {
                                base.wrapping_sub(delta)
                            } else {
                                base.wrapping_add(delta)
                            };
                            let actual = evocc_mir::interpret_module(
                                &mir,
                                "offset",
                                &[MirValue::Int(base)],
                                MirLimits::new(100),
                            )
                            .unwrap();
                            assert_eq!(
                                actual.results,
                                [MirValue::Int(expected)],
                                "{text}; base={base}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn large_wide_and_dynamic_offsets_keep_the_general_path() {
    for text in [
        source(64, 4096, "unsigned", "add", 1),
        source(64, 4096, "unsigned", "sub", 1),
        source(64, u64::MAX.into(), "unsigned", "add", u64::MAX),
        source(64, 1 << 63, "signed", "sub", 1),
        source(128, 8, "unsigned", "add", 1),
        source(32, 8, "signed", "add", 1)
            .replace(
                "%arg1: mem<all>, %arg2: world",
                "%arg1: bits<32>, %arg2: mem<all>, %arg3: world",
            )
            .replace("%v0: ptr<generic>,", "%v0: ptr<generic>, %v3: bits<32>,")
            .replace("    %v3 = const.bits<32> 8\n", ""),
    ] {
        let mir = lower(&text);
        assert!(
            mir.functions[0].blocks[0].insts.iter().any(|inst| matches!(
                inst,
                MInst::Binary {
                    op: BinOp::Add | BinOp::Sub,
                    width: Width::X,
                    ..
                }
            )),
            "{text}"
        );
    }
}
