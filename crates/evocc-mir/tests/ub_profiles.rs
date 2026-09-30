//! Native-only fan-out preserves non-obligation traps and predicate data uses.
use evocc_mir::{MirLimits, MirValue, Target, UbCheckProfile};

fn source(tail: &str) -> String {
    format!("func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }}
{{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 1
    %v4 = signed_add_defined %v0, %v3
    ub.assume.signed_overflow %v4
    %v5 = add.bits<32> %v0, %v3
{tail}
}}
")
}

#[test]
fn checked_and_unchecked_share_ir_and_defined_results_not_undefined_admission() {
    let ir =
        evocc_evoir::parse_section9_module(source("    ret %v5, %v1, %v2").as_bytes()).unwrap();
    evocc_evoir::validate_module(&ir).unwrap();
    let before = evocc_evoir::render_module(&ir);
    for target in [Target::Darwin, Target::Linux] {
        for profile in [UbCheckProfile::Checked, UbCheckProfile::Unchecked] {
            let mir =
                evocc_mir::lower_module_for_profile(target, &ir, &|_| false, profile).unwrap();
            evocc_mir::validate_module(&mir).unwrap();
            assert_eq!(evocc_evoir::render_module(&ir), before);
            for value in [0, 40, 2147483646, 2147483648] {
                let out = evocc_mir::interpret_module(
                    &mir,
                    "main",
                    &[MirValue::Int(value)],
                    MirLimits::new(100),
                )
                .unwrap();
                assert_eq!(out.results, [MirValue::Int(value + 1)]);
            }
            let out = evocc_mir::interpret_module(
                &mir,
                "main",
                &[MirValue::Int(2147483647)],
                MirLimits::new(100),
            );
            if profile == UbCheckProfile::Checked {
                assert_eq!(out.unwrap_err(), evocc_mir::MirError::Trap { imm: 3004 });
            } else {
                // Raw MIR behavior is not defined C behavior or benchmark admission.
                assert!(out.is_ok());
            }
        }
    }
}

#[test]
fn unchecked_does_not_erase_explicit_traps_or_observed_predicates() {
    for tail in [
        "    trap.unreachable_reached",
        "    %v6 = zext.bits<32> %v4\n    ret %v6, %v1, %v2",
    ] {
        let ir = evocc_evoir::parse_section9_module(source(tail).as_bytes()).unwrap();
        let mir = evocc_mir::lower_module_for_profile(
            Target::Linux,
            &ir,
            &|_| false,
            UbCheckProfile::Unchecked,
        )
        .unwrap();
        evocc_mir::validate_module(&mir).unwrap();
        let out = evocc_mir::interpret_module(
            &mir,
            "main",
            &[MirValue::Int(2147483647)],
            MirLimits::new(100),
        );
        if tail.contains("trap") {
            assert!(matches!(out.unwrap_err(), evocc_mir::MirError::Trap { .. }));
        } else {
            assert_eq!(out.unwrap().results, [MirValue::Int(0)]);
        }
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn real_native_checked_failure_and_defined_twins() {
    use std::{fs, process::Command};
    let directory = std::env::temp_dir().join(format!("evocc-native-twins-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    for valid in [true, false] {
        let text = format!("func @main() -> (bits<32>) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }}
{{
^entry():
    %v0 = const.bits<1> {}
    ub.assume.signed_overflow %v0
    %v1 = const.bits<32> 0
    ret %v1
}}
", u8::from(valid));
        let ir = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
        evocc_evoir::validate_module(&ir).unwrap();
        for profile in [UbCheckProfile::Checked, UbCheckProfile::Unchecked] {
            if !valid && profile == UbCheckProfile::Unchecked {
                continue; // No unchecked execution is admitted after a failing twin.
            }
            let asm = evocc_mir::compile_module_with_options(
                Target::Darwin,
                &ir,
                &|_| true,
                evocc_mir::RelocationModel::PositionIndependent,
                &evocc_mir::emit::CodegenOptions {
                    ub_checks: profile,
                    ..Default::default()
                },
            )
            .unwrap();
            let source = directory.join("input.s");
            let executable = directory.join("program");
            fs::write(&source, asm).unwrap();
            let out = Command::new("clang")
                .arg(&source)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(out.status.success(), "{:?}", out);
            let out = Command::new("/bin/sh")
                .args(["-c", "ulimit -c 0; exec \"$1\"", "twin-test"])
                .arg(&executable)
                .output()
                .unwrap();
            if valid {
                assert_eq!(out.status.code(), Some(0));
            } else {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(out.status.signal(), Some(5)); // AArch64 BRK -> SIGTRAP.
            }
        }
    }
    fs::remove_dir_all(directory).unwrap();
}
