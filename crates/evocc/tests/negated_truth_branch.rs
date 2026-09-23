use std::io::Write;
use std::process::{Command, Stdio};

fn run(level: &str, mode: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", level, mode])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/negated-truth-branch.c"
        ))
        .output()
        .unwrap()
}

#[test]
fn negation_uses_one_full_width_comparison_not_two_truth_conversions() {
    for level in ["-O0", "-O2"] {
        let output = run(level, "--emit=evoir");
        assert!(output.status.success(), "{level}: {:?}", output.stderr);
        let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
        evocc_evoir::validate_module(&module).unwrap();
        for name in [
            "null_case",
            "integer_case",
            "narrow_case",
            "wide_case",
            "function_case",
        ] {
            let function = module.functions.iter().find(|f| f.name == name).unwrap();
            let comparisons = function
                .blocks
                .iter()
                .flat_map(|b| &b.insts)
                .filter(|inst| matches!(inst, evocc_evoir::Inst::ICmp(..)))
                .count();
            assert_eq!(comparisons, 1, "{level} {name}: redundant boolean compare");
        }
        let machine =
            evocc_mir::lower_module_for(evocc_mir::Target::Linux, &module, &|_| false).unwrap();
        evocc_mir::validate_module(&machine).unwrap();
        for name in ["null_case", "integer_case", "narrow_case"] {
            let function = machine.functions.iter().find(|f| f.name == name).unwrap();
            assert!(
                function
                    .blocks
                    .iter()
                    .flat_map(|b| &b.insts)
                    .all(|inst| !matches!(inst, evocc_mir::MInst::Cset { .. })),
                "{level} {name}: materialized a boolean used only by its branch"
            );
        }
    }
}

#[test]
fn negation_does_not_erase_poison_or_operand_obligations() {
    for (source, code) in [
        ("int main(void){volatile int x; return !x;}", "E3001"),
        ("int main(void){volatile int x=0; return !(1/x);}", "E3002"),
        (
            "int main(void){volatile int x=32; return !(1u<<x);}",
            "E3005",
        ),
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
                .args([
                    "--target=aarch64-unknown-linux-gnu",
                    "-O2",
                    mode,
                    "/dev/stdin",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(source.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert_eq!(output.status.code(), Some(1), "{mode} {source}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(code),
                "{mode}: {:?}",
                output.stderr
            );
        }
    }
}

#[test]
fn effects_narrow_and_wide_values_signed_zero_and_nan_are_preserved() {
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let output = run(level, mode);
            assert!(
                output.status.success(),
                "{level} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(output.stdout, b"0\n", "{level} {mode}");
            }
        }
    }
}
