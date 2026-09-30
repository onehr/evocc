//! The native guard profile cannot weaken C-HIR/EvoIR or change proof input.
use std::{fs, process::Command};

#[test]
fn native_profile_is_explicit_and_does_not_change_evoir_or_interpreters() {
    let directory = std::env::temp_dir().join(format!("evocc-ub-profile-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let source = directory.join("input.c");
    let defined = include_str!("../../../tests/fixtures/eqsat-performance/ub-profile.c");
    fs::write(&source, defined).unwrap();
    let run = |mode: &str, profile: &str| {
        Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([mode, "-O1", &format!("--ub-checks={profile}")])
            .arg(&source)
            .output()
            .unwrap()
    };
    let checked = run("--emit=evoir", "checked");
    let unchecked = run("--emit=evoir", "unchecked");
    assert!(checked.status.success(), "{:?}", checked);
    assert!(unchecked.status.success(), "{:?}", unchecked);
    assert_eq!(checked.stdout, unchecked.stdout);
    assert!(String::from_utf8_lossy(&checked.stdout).contains("ub.assume.signed_overflow"));
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        for profile in ["checked", "unchecked"] {
            let out = run(mode, profile);
            assert!(out.status.success(), "{:?}", out);
            assert_eq!(out.stdout, b"0\n");
        }
    }
    let checked = run("--emit=asm", "checked");
    let unchecked = run("--emit=asm", "unchecked");
    assert!(checked.status.success(), "{:?}", checked);
    assert!(unchecked.status.success(), "{:?}", unchecked);
    let c = String::from_utf8(checked.stdout).unwrap();
    let u = String::from_utf8(unchecked.stdout).unwrap();
    assert!(c.contains("evocc-native-ub-checks=checked"));
    assert!(u.contains("evocc-native-ub-checks=unchecked"));
    assert!(c.contains("brk #3004"));
    assert!(!u.contains("brk #3004"));
    fs::write(&source, defined.replace("= 40", "= 2147483647")).unwrap();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        for profile in ["checked", "unchecked"] {
            let out = run(mode, profile);
            assert!(!out.status.success());
            assert!(
                String::from_utf8_lossy(&out.stderr).contains("E3004"),
                "{:?}",
                out
            );
        }
    }
    for flags in [
        vec!["--ub-checks=unknown"],
        vec!["--ub-checks=checked", "--ub-checks=unchecked"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .arg("--emit=asm")
            .args(flags)
            .arg(&source)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
    fs::remove_dir_all(directory).unwrap();
}
