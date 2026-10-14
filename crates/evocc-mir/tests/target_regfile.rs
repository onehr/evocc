//! N03a step 1: freeze the current register facts before moving their owner.
//! This is not permission to change allocator policy or verifier admission.

use std::collections::BTreeMap;

use evocc_evoir::{assembly_clobber, assembly_register_binding};
use evocc_mir::isa::aarch64::regfile;
use evocc_mir::{verify_allocation, Allocated, AsmClobber, MBlock, MInst, MTerm, RegClass, VReg};
use evocc_proof::sha256_hex;

const DESCRIPTION: &str = include_str!("../../../targets/aarch64/regfile");

#[test]
fn generic_register_facts_work_for_unrelated_banks_and_widths() {
    use evocc_mir::generic::register::{RegisterBank, RegisterView};
    for bank in [
        RegisterBank {
            first: 200,
            count: 3,
        },
        RegisterBank {
            first: 254,
            count: 2,
        },
    ] {
        let view = RegisterView {
            bank,
            bits: 24,
            prefix: "r",
        };
        for number in 0..=u8::MAX {
            let expected = u16::from(number) >= u16::from(bank.first)
                && u16::from(number) < u16::from(bank.first) + u16::from(bank.count);
            assert_eq!(bank.contains(number), expected);
            if expected {
                assert_eq!(view.spell(number), format!("r{}", number - bank.first));
            }
        }
    }
}

#[test]
fn generic_modules_do_not_import_target_facts_or_instruction_names() {
    let mut pending = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/generic")];
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(
                std::fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = std::fs::read_to_string(&path).unwrap().to_lowercase();
            for forbidden in [
                "aarch64",
                "x86",
                "minst::",
                "mterm::",
                "regclass::gpr",
                "regclass::simd",
                "nzcv",
                "x16",
                "x17",
                "v30",
                "v31",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{} contains {forbidden}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn the_canonical_description_binds_the_generated_register_facts() {
    assert!(DESCRIPTION.starts_with("version regfile.v1\ntarget aarch64\n"));
    assert!(DESCRIPTION.ends_with('\n'));
    assert!(!DESCRIPTION.contains('\r'));
    let digest_lines: Vec<_> = DESCRIPTION
        .lines()
        .filter_map(|line| line.strip_prefix("sha256 "))
        .collect();
    assert_eq!(digest_lines.len(), 1);
    let payload: String = DESCRIPTION
        .split_inclusive('\n')
        .filter(|line| !line.starts_with("sha256 "))
        .collect();
    assert_eq!(digest_lines[0], sha256_hex(payload.as_bytes()));
    assert_eq!(digest_lines[0], regfile::DESCRIPTION_DIGEST);
    for row in [
        "bank gpr 0 31",
        "bank simd 64 32",
        "view gpr32 gpr 32 w",
        "view gpr64 gpr 64 x",
        "view simd32 simd 32 s",
        "view simd64 simd 64 d",
        "view simd128 simd 128 q",
        "virtual flags nzcv 0",
    ] {
        assert!(DESCRIPTION.lines().any(|line| line == row), "missing {row}");
    }
    assert_eq!((regfile::GPR_BANK.first, regfile::GPR_BANK.count), (0, 31));
    assert_eq!(
        (regfile::SIMD_BANK.first, regfile::SIMD_BANK.count),
        (64, 32)
    );
    for number in 0..=u8::MAX {
        assert_eq!(regfile::GPR_BANK.contains(number), number < 31);
        assert_eq!(
            regfile::SIMD_BANK.contains(number),
            (64..=95).contains(&number)
        );
    }
}

#[test]
fn ordered_allocator_pools_are_not_the_call_clobber_sets() {
    assert_eq!(
        regfile::CALLER_SAVED,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
    assert_eq!(
        regfile::CALLEE_SAVED,
        [19, 20, 21, 22, 23, 24, 25, 26, 27, 28]
    );
    assert_eq!(
        regfile::SIMD_CALLER_SAVED,
        [64, 65, 66, 67, 68, 69, 70, 71, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93]
    );
    assert_eq!(
        regfile::GPR_CALL_CLOBBERS,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]
    );
    assert_eq!(
        regfile::SIMD_CALL_CLOBBERS,
        [
            64, 65, 66, 67, 68, 69, 70, 71, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93,
            94, 95
        ]
    );
    assert!(regfile::SIMD_ACROSS_CALL.is_empty());
    assert_eq!(
        regfile::SIMD_LOW64_PRESERVED,
        [72, 73, 74, 75, 76, 77, 78, 79]
    );
    assert_eq!(evocc_mir::CALLER_SAVED, regfile::CALLER_SAVED);
    assert_eq!(evocc_mir::CALLEE_SAVED, regfile::CALLEE_SAVED);
    assert_eq!(
        evocc_mir::linscan::SIMD_CALLER_SAVED,
        regfile::SIMD_CALLER_SAVED
    );
    assert!(!regfile::GPR_CALL_CLOBBERS.contains(&30));
    for number in [16, 17] {
        assert!(regfile::GPR_CALL_CLOBBERS.contains(&number));
        assert!(!regfile::CALLER_SAVED.contains(&number));
    }
    for number in [94, 95] {
        assert!(regfile::SIMD_CALL_CLOBBERS.contains(&number));
        assert!(!regfile::SIMD_CALLER_SAVED.contains(&number));
    }
}

#[test]
fn register_views_alias_within_one_bank_and_preserve_physical_spelling() {
    for (class, view, bits, prefix, bank) in [
        (
            RegClass::Gpr32,
            regfile::GPR32_VIEW,
            32,
            "w",
            regfile::GPR_BANK,
        ),
        (
            RegClass::Gpr64,
            regfile::GPR64_VIEW,
            64,
            "x",
            regfile::GPR_BANK,
        ),
        (
            RegClass::Simd32,
            regfile::SIMD32_VIEW,
            32,
            "s",
            regfile::SIMD_BANK,
        ),
        (
            RegClass::Simd64,
            regfile::SIMD64_VIEW,
            64,
            "d",
            regfile::SIMD_BANK,
        ),
        (
            RegClass::Simd128,
            regfile::SIMD128_VIEW,
            128,
            "q",
            regfile::SIMD_BANK,
        ),
    ] {
        assert_eq!(view.bank, bank);
        assert_eq!(view.bits, bits);
        assert_eq!(view.prefix, prefix);
        for index in 0..bank.count {
            let number = bank.first + index;
            assert_eq!(
                evocc_mir::linscan::physical(class, number),
                format!("{prefix}{index}")
            );
        }
    }
    assert_eq!(regfile::GPR32_VIEW.bank, regfile::GPR64_VIEW.bank);
    assert_eq!(regfile::SIMD32_VIEW.bank, regfile::SIMD128_VIEW.bank);
    assert_ne!(regfile::GPR32_VIEW.bank, regfile::SIMD32_VIEW.bank);
    // This is the allocator's existing bookkeeping tag, not a physical alias.
    assert_eq!(regfile::FLAGS_ASSIGNMENT_TAG, 0);
    assert_eq!(regfile::FLAGS_SPELLING, "nzcv");
    assert_eq!(evocc_mir::linscan::physical(RegClass::Flags, 0), "nzcv");
}

#[test]
fn reserved_pool_exclusions_do_not_remove_the_fixed_link_register_exception() {
    assert_eq!(
        regfile::RESERVED,
        [16, 17, 18, 29, 30, 72, 73, 74, 75, 76, 77, 78, 79, 94, 95]
    );
    let ordinary: Vec<_> = regfile::CALLER_SAVED
        .into_iter()
        .chain(regfile::CALLEE_SAVED)
        .chain(regfile::SIMD_CALLER_SAVED)
        .collect();
    for number in regfile::RESERVED {
        assert!(
            !ordinary.contains(&number),
            "reserved {number} enters a pool"
        );
    }
    let fixed: Vec<_> = regfile::CALLER_SAVED
        .into_iter()
        .chain(regfile::CALLEE_SAVED)
        .chain([30])
        .chain(regfile::SIMD_CALLER_SAVED)
        .collect();
    assert_eq!(regfile::FIXED_ASM.as_slice(), fixed.as_slice());
    for number in [16, 17, 18, 29, 94, 95] {
        assert!(!regfile::FIXED_ASM.contains(&number));
    }
    assert_eq!(regfile::SCRATCH, 16);
    assert_eq!(regfile::SCRATCH_UNIT, 17);
    assert_eq!(regfile::SIMD_SCRATCH, 94);
    assert_eq!(regfile::SIMD_SCRATCH_UNIT, 95);
    assert_eq!(evocc_mir::linscan::SCRATCH, regfile::SCRATCH);
    assert_eq!(evocc_mir::linscan::SIMD_SCRATCH, regfile::SIMD_SCRATCH);
}

#[test]
fn existing_assembly_binding_and_clobber_spelling_boundaries_are_unchanged() {
    for number in 0..=u8::MAX {
        let gpr = format!("r@x{number}");
        let vector = format!("w@v{number}");
        let gpr_allowed = matches!(number, 0..=15 | 19..=28 | 30);
        let vector_allowed = matches!(number, 0..=7 | 16..=29);
        assert_eq!(
            assembly_register_binding(&gpr).is_ok(),
            gpr_allowed,
            "{gpr}"
        );
        assert_eq!(
            assembly_register_binding(&vector).is_ok(),
            vector_allowed,
            "{vector}"
        );
        if gpr_allowed {
            assert_eq!(
                assembly_register_binding(&gpr).unwrap(),
                ("r", Some(number))
            );
        }
        if vector_allowed {
            assert_eq!(
                assembly_register_binding(&vector).unwrap(),
                ("w", Some(64 + number))
            );
        }
    }
    for constraint in ["r@x00", "r@x+0", "r@w0", "w@v00", "w@v+0", "w@q0"] {
        assert!(
            assembly_register_binding(constraint).is_err(),
            "{constraint}"
        );
    }
    for (text, expected) in [
        ("x30", AsmClobber::Register(30)),
        ("w30", AsmClobber::Register(30)),
        ("v31", AsmClobber::VectorRegister(31)),
        ("d31", AsmClobber::VectorRegister(31)),
        ("s31", AsmClobber::VectorRegister(31)),
        ("x00", AsmClobber::Register(0)),
        ("v00", AsmClobber::VectorRegister(0)),
    ] {
        assert_eq!(AsmClobber::parse(text), Some(expected), "{text}");
        assert!(assembly_clobber(text), "{text}");
    }
    for text in ["x31", "w31", "v32", "q0", "b0", "h0", "sp", "xzr"] {
        assert_eq!(AsmClobber::parse(text), None, "{text}");
        assert!(!assembly_clobber(text), "{text}");
    }
}

fn local_copy(class: RegClass, number: u8) -> Allocated {
    let source = VReg::new(0, class);
    let destination = VReg::new(1, class);
    Allocated {
        name: "register_bank_fixture".to_owned(),
        blocks: vec![MBlock {
            id: 0,
            args: Vec::new(),
            insts: vec![
                MInst::MovImm {
                    dst: source,
                    imm: 0,
                },
                MInst::Mov {
                    dst: destination,
                    src: source,
                },
            ],
            term: MTerm::Ret { values: Vec::new() },
        }],
        assignment: BTreeMap::from([(source.id, number), (destination.id, number)]),
        spilled: 0,
        frame_slots: 0,
        // A sound allocation saves each callee-saved register that it writes.
        used_callee_saved: regfile::CALLEE_SAVED
            .contains(&number)
            .then_some(number)
            .into_iter()
            .collect(),
        parameters: 0,
        outgoing: 0,
        indirect_result: false,
        internal: false,
    }
}

#[test]
fn allocation_verifier_admission_remains_bank_membership_not_pool_membership() {
    for class in [
        RegClass::Gpr32,
        RegClass::Gpr64,
        RegClass::Simd32,
        RegClass::Simd64,
        RegClass::Simd128,
    ] {
        for number in 0..=u8::MAX {
            let admitted = if class.is_simd() {
                (64..=95).contains(&number)
            } else {
                number < 31
            };
            let result = verify_allocation(&local_copy(class, number));
            assert_eq!(
                result.is_ok(),
                admitted,
                "{class:?} register {number}: {result:?}"
            );
            if !admitted {
                let error = result.unwrap_err();
                assert_eq!(error.code, "E4020");
                assert!(error.detail.contains("incompatible register bank"));
            }
        }
    }
}
