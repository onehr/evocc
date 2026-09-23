use std::io::Write;
use std::process::{Command, Output, Stdio};

use evocc_evoir::Inst;

const SOURCE: &str = include_str!("../../../tests/fixtures/compared-value-selection.c");

fn run(source: &str, level: &str, mode: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([
            "--target=aarch64-unknown-linux-gnu",
            level,
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
    child.wait_with_output().unwrap()
}

fn module(source: &str, level: &str) -> evocc_evoir::Module {
    let output = run(source, level, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    evocc_evoir::validate_module(&module).unwrap();
    module
}

#[test]
fn an_integer_selection_reuses_the_already_compared_values() {
    let module = module(SOURCE, "-O2");
    for (name, expected) in [
        ("minimum", 2),
        ("maximum", 2),
        ("reversed", 2),
        ("unequal", 2),
        ("equal_reversed", 2),
        ("greater_equal", 2),
        ("row_minimum", 3),
        ("member_minimum", 2),
    ] {
        let function = module.functions.iter().find(|f| f.name == name).unwrap();
        let reads = function
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .filter(|i| matches!(i, Inst::Load { .. }))
            .count();
        assert_eq!(
            reads, expected,
            "{name}: repeated ordinary reads after comparison"
        );
        assert!(
            function
                .blocks
                .iter()
                .all(|block| !matches!(block.terminator, Inst::CondBr(..))),
            "{name}: a selection of already observed integer values needs no data-flow join"
        );
    }
}

#[test]
fn repeated_observations_and_effects_are_not_reused() {
    let module = module(SOURCE, "-O2");
    let instructions = |name: &str| {
        module
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap()
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        instructions("volatile_minimum")
            .iter()
            .filter(|i| matches!(i, Inst::Load { .. }))
            .count(),
        4
    );
    assert_eq!(
        instructions("other_object")
            .iter()
            .filter(|i| matches!(i, Inst::Load { .. }))
            .count(),
        4
    );
    assert_eq!(
        instructions("atomic_minimum")
            .iter()
            .filter(|i| matches!(i, Inst::Atomic(_)))
            .count(),
        2
    );
    assert_eq!(
        instructions("effectful")
            .iter()
            .filter(|i| matches!(i, Inst::Call { .. }))
            .count(),
        2
    );
}

#[test]
fn inferring_dereference_arm_types_never_evaluates_them() {
    let source = "int main(void){int a[2]={7,9}; int *p=a; int x=1?*p++:*(int*)0; unsigned char c=250; long y=0?*(int*)0:*&c; return x!=7 || p!=a+1 || y!=250;}";
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, level, mode);
            assert!(
                output.status.success(),
                "{level} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, b"0\n");
        }
    }
}

#[test]
fn machine_selection_preserves_integer_bits_and_surrounding_effects() {
    let module = module(SOURCE, "-O2");
    let machine =
        evocc_mir::lower_module_for(evocc_mir::Target::Linux, &module, &|_| false).unwrap();
    evocc_mir::validate_module(&machine).unwrap();
    assert_eq!(
        evocc_mir::interpret_module(&machine, "main", &[], evocc_mir::MirLimits::new(500_000))
            .unwrap()
            .results,
        [evocc_mir::MirValue::Int(0)]
    );
}

#[test]
fn selection_keeps_conversions_both_edges_and_source_effects() {
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let output = run(SOURCE, level, mode);
            assert!(
                output.status.success(),
                "{level} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(output.stdout, b"0\n");
            }
        }
    }
}

#[test]
fn selection_does_not_erase_arithmetic_or_memory_errors() {
    for (source, code) in [
        ("int f(int a,int b){return a+b<7?a+b:7;} int main(void){volatile int x=2147483647;return f(x,1);}", "E3004"),
        ("int f(int *a,int *b){return *a<*b?*a:*b;} int main(void){int a,b=1;return f(&a,&b);}", "E3001"),
        ("int f(int *a,int *b){return *a<*b?*a:*b;} int main(void){int b=1;return f(0,&b);}", "E3009"),
        ("int f(int *a,int *b){return *a<*b?*a:*b;} int main(void){int a[1]={1};return f(a+1,a);}", "E3011"),
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, "-O2", mode);
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains(code), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
