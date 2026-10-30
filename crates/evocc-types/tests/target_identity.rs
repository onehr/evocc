use evocc_types::{Isa, ObjectFormat, OperatingSystem, TargetDataLayout, TargetPlatform};

#[test]
fn instruction_set_os_and_object_format_are_separate_facts() {
    for (platform, os, format) in [
        (
            TargetPlatform::Aarch64UnknownLinuxGnu,
            OperatingSystem::Linux,
            ObjectFormat::Elf,
        ),
        (
            TargetPlatform::Aarch64AppleDarwin,
            OperatingSystem::Darwin,
            ObjectFormat::MachO,
        ),
    ] {
        assert_eq!(platform.isa(), Isa::Aarch64);
        assert_eq!(platform.operating_system(), os);
        assert_eq!(platform.object_format(), format);
        assert_eq!(platform.layout().platform(), platform);
        assert_eq!(platform.layout().isa(), Isa::Aarch64);
    }
}

#[test]
fn pointer_lists_do_not_force_anonymous_arguments_to_the_stack() {
    for pointer in [false, true] {
        for on_stack in [false, true] {
            let layout =
                TargetDataLayout::aarch64_unknown_linux_gnu().with_variadic_abi(pointer, on_stack);
            assert_eq!(layout.va_list_is_pointer(), pointer);
            assert_eq!(layout.anonymous_arguments_on_stack(), on_stack);
            assert_eq!(layout.passes_half_anonymous_arguments(), !on_stack);
            assert_eq!(
                layout.va_list_layout().size_bytes,
                if pointer { 8 } else { 32 }
            );
        }
    }
    assert!(TargetDataLayout::aarch64_apple_darwin().anonymous_arguments_on_stack());
    assert!(!TargetDataLayout::aarch64_unknown_linux_gnu().anonymous_arguments_on_stack());
}

#[test]
fn barrier_text_comes_from_the_instruction_set() {
    assert_eq!(Isa::Aarch64.memory_barrier(false), "dmb ish");
    assert_eq!(Isa::Aarch64.memory_barrier(true), "dmb ishld");
}
