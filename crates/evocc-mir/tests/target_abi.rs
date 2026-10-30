//! N03a step 3: freeze the current Darwin, Linux and kernel-profile facts
//! before moving their owner. This is not permission to change placement,
//! refusal boundaries or frame layout; the measured Darwin variadic and
//! stack-argument rules stay exactly as currently applied.

use evocc_mir::generic::abi::AbiSpec;
use evocc_mir::generic::register::RegisterList;
use evocc_mir::isa::aarch64::abi::{
    self, Aarch64Abi, ProfileFeature, VaListLayout, DARWIN, KERNEL, LINUX,
};
use evocc_mir::isa::aarch64::regfile;
use evocc_mir::Target;
use evocc_proof::sha256_hex;

const DARWIN_DESCRIPTION: &str = include_str!("../../../targets/aarch64/abi-darwin");
const LINUX_DESCRIPTION: &str = include_str!("../../../targets/aarch64/abi-linux");
const KERNEL_DESCRIPTION: &str = include_str!("../../../targets/aarch64/profile-kernel");

fn stated_digest(description: &str) -> &str {
    assert!(description.ends_with('\n'));
    assert!(!description.contains('\r'));
    let digests: Vec<_> = description
        .lines()
        .filter_map(|line| line.strip_prefix("sha256 "))
        .collect();
    assert_eq!(digests.len(), 1);
    let payload: String = description
        .split_inclusive('\n')
        .filter(|line| !line.starts_with("sha256 "))
        .collect();
    assert_eq!(digests[0], sha256_hex(payload.as_bytes()));
    digests[0]
}

#[test]
fn the_canonical_descriptions_bind_the_generated_tables() {
    for (description, version, generated) in [
        (DARWIN_DESCRIPTION, "abi.v1", abi::DARWIN_DIGEST),
        (LINUX_DESCRIPTION, "abi.v1", abi::LINUX_DIGEST),
        (KERNEL_DESCRIPTION, "profile.v1", abi::KERNEL_DIGEST),
    ] {
        assert!(description.starts_with(&format!("version {version}\ntarget aarch64\nsha256 ")));
        assert_eq!(stated_digest(description), generated);
    }
    // ABI files name the register description they reference, and the
    // profile names the ABI revision that hosts it.
    for description in [DARWIN_DESCRIPTION, LINUX_DESCRIPTION] {
        assert!(description
            .lines()
            .any(|line| line == format!("regfile {}", regfile::DESCRIPTION_DIGEST)));
        assert!(description.contains("\ncallee-saved @CALLEE_SAVED\n"));
        assert!(description.contains("\ncall-clobbers @GPR_CALL_CLOBBERS @SIMD_CALL_CLOBBERS\n"));
        assert!(description.contains("\npartially-preserved @SIMD_LOW64_PRESERVED 8\n"));
    }
    assert!(KERNEL_DESCRIPTION
        .lines()
        .any(|line| line == format!("abi linux {}", abi::LINUX_DIGEST)));
}

fn frozen(name: &'static str, stack_arguments: bool, pair_rounding: bool, abi: &Aarch64Abi) {
    let facts = abi.facts();
    assert_eq!(facts.name, name);
    let banks = [
        RegisterList(&[0, 1, 2, 3, 4, 5, 6, 7]),
        RegisterList(&[64, 65, 66, 67, 68, 69, 70, 71]),
    ];
    assert_eq!(facts.argument_banks, banks);
    assert_eq!(facts.result_banks, banks);
    assert_eq!(facts.indirect_result, Some(8));
    assert_eq!(facts.result_overflow, Some(8));
    assert_eq!(facts.stack_slot_bytes, 8);
    assert_eq!(facts.stack_alignment_bytes, 16);
    assert_eq!(facts.stack_arguments, stack_arguments);
    assert_eq!(facts.callee_saved, regfile::CALLEE_SAVED);
    assert_eq!(facts.call_clobbers, regfile::CALL_CLOBBERS);
    assert_eq!(facts.partially_preserved, regfile::SIMD_LOW64_PRESERVED);
    assert_eq!(facts.partially_preserved_bytes, 8);
    assert_eq!(facts.frame_record_bytes, 16);
    assert_eq!(facts.save_slot_bytes, 8);
    assert_eq!(facts.variadic_save_bytes, [8, 16]);
    assert_eq!(facts.variadic_save_alignment, 16);
    assert_eq!(facts.variadic_save_area_bytes(0), 64);
    assert_eq!(facts.variadic_save_area_bytes(1), 128);
    assert_eq!(facts.incoming_argument_offset(2), 32);
    assert_eq!(facts.outgoing_area_bytes(3), 32);
    assert_eq!(abi.record_register_bytes, 16);
    assert_eq!(abi.aligned_record_pair_rounding, pair_rounding);
    assert_eq!(
        abi.va_list,
        VaListLayout {
            stack: 0,
            general_top: 8,
            vector_top: 16,
            general_offset: 24,
            vector_offset: 28,
        }
    );
}

#[test]
fn darwin_and_linux_differ_only_in_their_existing_rules() {
    // E4010: Darwin refuses a ninth general argument position; Linux lowers
    // stack arguments. Only Linux rounds a 16-aligned record to an even pair.
    frozen("darwin", false, false, &DARWIN);
    frozen("linux", true, true, &LINUX);
    assert_eq!(Target::Darwin.abi(), &DARWIN);
    assert_eq!(Target::Linux.abi(), &LINUX);
    assert_ne!(DARWIN, LINUX);
    assert_eq!(abi::machine_result_registers(0), 8);
    assert_eq!(abi::machine_result_registers(1), 8);
}

#[test]
fn the_kernel_profile_keeps_elf_only_features_on_linux() {
    assert_eq!(KERNEL.name, "kernel");
    assert_eq!(KERNEL.host_abi, "linux");
    assert_eq!(KERNEL.stack_guard_slot_bytes, 16);
    let features = [
        (ProfileFeature::FunctionSections, "function-sections"),
        (ProfileFeature::DataSections, "data-sections"),
        (ProfileFeature::ExplicitSections, "explicit-sections"),
        (ProfileFeature::Tls, "tls"),
        (ProfileFeature::PatchableEntry, "patchable-entry"),
        (ProfileFeature::ProtectedVisibility, "protected-visibility"),
    ];
    assert_eq!(
        KERNEL.host_only,
        features.map(|(_, spelling)| spelling).as_slice()
    );
    for (feature, spelling) in features {
        assert_eq!(feature.spelling(), spelling);
        assert!(KERNEL.admits(Target::Linux, feature));
        assert!(!KERNEL.admits(Target::Darwin, feature));
    }
}
