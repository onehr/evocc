use evocc_eqopt::EqualityState;
use evocc_evoir::Module;
use evocc_types::TargetDataLayout;

#[test]
fn supplied_layout_preserves_the_existing_profile_identity() {
    for (triple, layout) in [
        (
            "aarch64-unknown-linux-gnu",
            TargetDataLayout::aarch64_unknown_linux_gnu(),
        ),
        (
            "aarch64-apple-darwin",
            TargetDataLayout::aarch64_apple_darwin(),
        ),
    ] {
        let module = Module::default();
        let expected = EqualityState::for_target_module(&module, triple)
            .unwrap()
            .audit();
        for short in [false, true] {
            for signed in [false, true] {
                let selected = layout.with_short_wchar(short).with_signed_char(signed);
                let mut state =
                    EqualityState::for_target_module_with_layout(&module, triple, &selected)
                        .unwrap();
                assert_eq!(state.audit(), expected);
            }
        }
    }
}

#[test]
fn a_mismatched_abi_cannot_reuse_an_equality_state() {
    let module = Module::default();
    let linux = TargetDataLayout::aarch64_unknown_linux_gnu();
    let darwin = TargetDataLayout::aarch64_apple_darwin();
    assert!(
        EqualityState::for_target_module_with_layout(&module, "aarch64-apple-darwin", &linux)
            .is_err()
    );
    assert!(EqualityState::for_target_module_with_layout(
        &module,
        "aarch64-unknown-linux-gnu",
        &darwin
    )
    .is_err());
    assert!(EqualityState::for_target_module_with_layout(
        &module,
        "aarch64-unknown-linux-gnu",
        &linux.with_variadic_abi(true, false)
    )
    .is_err());
}
