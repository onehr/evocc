//! T03r (audit finding A15): the cc command line refuses an option that
//! changes the ABI and that this compiler cannot honor, with a diagnostic at
//! the option, whatever the warning level (EvoMIR Section 6.14). Before, it
//! ignored such an option with a warning, and silently under `-w`, so
//! `sizeof` and the record layout disagreed with every caller built with it.

use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const SOURCE: &str = "struct s { char c; long x; }; enum e { A }; \
                      int f(void) { return (int)sizeof(struct s) + (int)sizeof(enum e); }\n";

fn run(args: &[&str]) -> Output {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let directory = std::env::temp_dir().join(format!(
        "evocc-cc-abi-options-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("input.c"), SOURCE).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .current_dir(&directory)
        .args(args)
        .args(["-S", "input.c", "-o", "input.s"])
        .output()
        .unwrap();
    assert!(
        !(directory.join("input.s").exists() && !output.status.success()),
        "a refused compilation wrote its output"
    );
    std::fs::remove_dir_all(directory).unwrap();
    output
}

#[test]
fn abi_changing_options_are_refused_at_the_option_under_every_lint_level() {
    for option in [
        "-fpack-struct",
        "-fpack-struct=1",
        "-mabi=ilp32",
        "-fshort-enums",
        "-mbig-endian",
        "-ffixed-x19",
        "-fcall-used-x19",
        "-fcall-saved-x9",
    ] {
        for level in [&[][..], &["-w"][..], &["--cap-lints=allow"][..]] {
            let mut args = vec!["--target=aarch64-unknown-linux-gnu"];
            args.extend_from_slice(level);
            args.push(option);
            let output = run(&args);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{option} {level:?}\n{stderr}"
            );
            assert!(
                stderr.contains("changes the ABI") && stderr.contains(option),
                "{option} {level:?}\n{stderr}"
            );
        }
    }
}

#[test]
fn options_that_keep_the_abi_are_still_accepted() {
    // `-mabi=lp64` names the default ABI; `-ffixed-x18` reserves a register
    // that the backend never uses.
    for option in [
        "-mabi=lp64",
        "-ffixed-x18",
        "-fno-short-enums",
        "-fno-pack-struct",
    ] {
        let output = run(&["--target=aarch64-unknown-linux-gnu", "-w", option]);
        assert!(
            output.status.success(),
            "{option}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
