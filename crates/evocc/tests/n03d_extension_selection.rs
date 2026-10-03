//! Repeated native extension selection through the public C driver.
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(flags: &[&str], source: &str) -> Output {
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
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn assembly(flags: &[&str], source: &str) -> String {
    let mut args = vec!["--emit=asm", "-O2", "--ub-checks=unchecked"];
    args.extend_from_slice(flags);
    let output = run(&args, source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn repeated_narrow_arguments_share_the_existing_extension() {
    for (source, mnemonic, before_count) in [
        (
            "extern int consume(signed char,signed char,signed char); int f(signed char x){return consume(x,x,x);}",
            "    uxtb ",
            3,
        ),
        (
            "extern unsigned consume(unsigned short,unsigned short); unsigned f(unsigned short x){return consume(x,x);}",
            "    uxth ",
            2,
        ),
    ] {
        let before = assembly(&[], source);
        assert_eq!(before.matches(mnemonic).count(), before_count, "{before}");
        let after = assembly(&["--isel-opt=extensions"], source);
        assert_eq!(after.matches(mnemonic).count(), 1, "{after}");
        assert_eq!(before.matches("    bl consume").count(), 1);
        assert_eq!(after.matches("    bl consume").count(), 1);
    }
}
