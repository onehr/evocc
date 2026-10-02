//! Public-driver boundary for the optional allocator candidate order.
use std::io::Write;
use std::process::{Command, Output, Stdio};

const SOURCE: &str = "unsigned long f(int p,unsigned long x,unsigned long y){unsigned long a;if(p)a=x;else a=y;return a+7;}";

fn run(flags: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(flags)
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(SOURCE.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn assembly(flags: &[&str]) -> String {
    let mut args = vec!["--emit=asm", "-O2", "--ub-checks=unchecked"];
    args.extend_from_slice(flags);
    let output = run(&args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn n03c_copy_hint_driver_is_opt_in_and_records_the_request() {
    let off = assembly(&[]);
    assert!(!off.contains("evocc-native-regalloc="));
    let on = assembly(&["--regalloc-opt=copy-hints"]);
    assert!(on.contains("// evocc-native-regalloc=copy-hints\n"));
    assert_eq!(on, assembly(&["--regalloc-opt=copy-hints"]));
}
