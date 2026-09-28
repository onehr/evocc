use std::process::Command;

#[test]
fn doom_angle_requires_explicit_wrapping_and_keeps_default_overflow_check() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/doom-angle-wrap.c"
    );
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let red = Command::new(env!("CARGO_BIN_EXE_evocc"))
                .args([level, mode, fixture])
                .output()
                .unwrap();
            assert!(!red.status.success());
            assert!(String::from_utf8_lossy(&red.stderr).contains("E3004"));
            let green = Command::new(env!("CARGO_BIN_EXE_evocc"))
                .args([level, "-fwrapv", mode, fixture])
                .output()
                .unwrap();
            assert!(
                green.status.success(),
                "{}",
                String::from_utf8_lossy(&green.stderr)
            );
            assert_eq!(green.stdout, b"0\n");
        }
        let ir = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([level, "-fwrapv", "--emit=evoir", fixture])
            .output()
            .unwrap();
        assert!(ir.status.success());
        let module = evocc_evoir::parse_section9_module(&ir.stdout).unwrap();
        let mir =
            evocc_mir::lower_module_for(evocc_mir::Target::Linux, &module, &|_| false).unwrap();
        assert_eq!(
            evocc_mir::interpret_module(&mir, "main", &[], evocc_mir::MirLimits::new(100_000))
                .unwrap()
                .results,
            [evocc_mir::MirValue::Int(0)]
        );
    }
}
