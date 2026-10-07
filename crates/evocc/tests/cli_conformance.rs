use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn unique_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("c30-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

fn stdin_run(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(evocc())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn invalid_target_may_exit_without_reading_standard_input() {
    let output = stdin_run(
        &["--target=invalid", "-E", "/dev/stdin"],
        &vec![b' '; 1024 * 1024],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("target"));
}

#[test]
fn neverball_macro_reduction_does_not_weaken_float_conversion_checks() {
    let source = include_bytes!("../../../tests/fixtures/neverball-material-conversion.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let fixed = stdin_run(&[mode, "/dev/stdin"], source);
        assert!(
            fixed.status.success(),
            "{}",
            String::from_utf8_lossy(&fixed.stderr)
        );
        assert_eq!(fixed.stdout, b"0\n");
        let original = stdin_run(&[mode, "-DORIGINAL_MACRO", "/dev/stdin"], source);
        assert!(!original.status.success());
        assert!(String::from_utf8_lossy(&original.stderr).contains("E3063"));
    }
}

#[test]
fn final_include_without_newline_keeps_implementation_body() {
    let root = unique_dir("include-eof");
    fs::write(root.join("body.c"), "int answer(void){return 42;}\n").unwrap();
    for source in [
        "#include \"body.c\"",
        "#define BODY \"body.c\"\n#include BODY",
    ] {
        let input = root.join("input.c");
        fs::write(&input, source).unwrap();
        let output = Command::new(evocc())
            .arg("--emit=asm")
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("answer:"),
            "include silently lost"
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn date_and_time_share_the_frozen_translation_timestamp() {
    let root = unique_dir("translation-time");
    let input = root.join("input.c");
    fs::write(&input, "__DATE__;__TIME__;\n").unwrap();
    for (epoch, expected) in [
        ("0", "\"Jan  1 1970\";\"00:00:00\";"),
        ("86399", "\"Jan  1 1970\";\"23:59:59\";"),
        ("86401", "\"Jan  2 1970\";\"00:00:01\";"),
    ] {
        let output = Command::new(evocc())
            .env("SOURCE_DATE_EPOCH", epoch)
            .arg("-E")
            .arg(&input)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cpp_flags_have_real_semantics() {
    let root = unique_dir("cpp-flags");
    let include = root.join("include");
    fs::create_dir(&include).unwrap();
    fs::write(include.join("witness.h"), "#define HEADER_VALUE 73\n").unwrap();
    let source = root.join("input.c");
    fs::write(&source, "#include <witness.h>\nCLI_VALUE HEADER_VALUE\n").unwrap();

    let output = Command::new(evocc())
        .args([
            "-std=gnu11",
            "-E",
            "-P",
            "-undef",
            "-nostdinc",
            &format!("-I{}", include.display()),
            "-DCLI_VALUE=42",
            source.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "42 73");
    assert!(output.stderr.is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_system_includes_follow_user_paths_and_preserve_include_next() {
    let root = unique_dir("system-includes");
    let user = root.join("user");
    let system = root.join("system");
    fs::create_dir(&user).unwrap();
    fs::create_dir(&system).unwrap();
    fs::write(
        user.join("chain.h"),
        "#define USER_VALUE 17\n#include_next <chain.h>\n",
    )
    .unwrap();
    fs::write(system.join("chain.h"), "#define SYSTEM_VALUE 29\n").unwrap();
    let source = root.join("input.c");
    fs::write(&source, "#include <chain.h>\nUSER_VALUE SYSTEM_VALUE\n").unwrap();
    for joined in [false, true] {
        let mut args = vec!["-E".to_owned(), "-P".to_owned(), "-nostdinc".to_owned()];
        if joined {
            args.push(format!("-isystem{}", system.display()));
        } else {
            args.extend(["-isystem".to_owned(), system.display().to_string()]);
        }
        // A directory also named with -isystem must not remain an early -I
        // entry: otherwise include_next cannot reach the expected wrapper.
        args.extend([
            format!("-I{}/.", system.display()),
            format!("-I{}", user.display()),
            source.display().to_string(),
        ]);
        let output = Command::new(evocc()).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "17 29");
    }
    let missing = Command::new(evocc())
        .args([source.to_str().unwrap(), "-isystem"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("requires an argument"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_inputs_fail_closed() {
    for input in [
        b"int value".as_slice(),
        b"int main(void) { return 0;".as_slice(),
        b"int value; @".as_slice(),
    ] {
        let output = stdin_run(&["--parse-only", "/dev/stdin"], input);
        assert_eq!(output.status.code(), Some(1), "input={input:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("error["));
    }

    let output = stdin_run(&["-E", "/dev/stdin"], b"#error stop now\n");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("error[E0104]"));
}

#[test]
fn declaration_free_translation_units_are_valid() {
    for input in [
        b"".as_slice(),
        b"\n".as_slice(),
        b"_Static_assert(1, \"valid\");\n".as_slice(),
    ] {
        let output = stdin_run(&["--parse-only", "/dev/stdin"], input);
        assert!(
            output.status.success(),
            "input={:?}; stderr={}",
            String::from_utf8_lossy(input),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn preprocessed_output_does_not_fuse_tokens() {
    for (input, forbidden) in [
        ("int x = left + +right;\n", "left++right"),
        ("int x = left / *ptr;\n", "left/*ptr"),
        ("int x = left - -right;\n", "left--right"),
    ] {
        let output = stdin_run(&["-E", "/dev/stdin"], input.as_bytes());
        assert!(output.status.success());
        let rendered = String::from_utf8(output.stdout).unwrap();
        assert!(!rendered.contains(forbidden), "{rendered}");
    }
}

#[test]
fn supported_target_is_accepted_for_frontend_modes() {
    let output = stdin_run(
        &["--target=aarch64-unknown-linux-gnu", "-E", "/dev/stdin"],
        b"int value;\n",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "int value;"
    );

    let output = stdin_run(
        &[
            "--parse-only",
            "--target",
            "aarch64-unknown-linux-gnu",
            "/dev/stdin",
        ],
        b"int value;\n",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn unsupported_or_missing_target_fails_closed() {
    for args in [
        vec!["--target=x86_64-unknown-linux-gnu", "-E", "/dev/stdin"],
        vec!["-E", "--target", "arm64-apple-darwin", "/dev/stdin"],
        vec!["-E", "--target="],
        vec!["-E", "--target"],
        vec![
            "-E",
            "--target=aarch64-unknown-linux-gnu",
            "--target=aarch64-unknown-linux-gnu",
            "/dev/stdin",
        ],
    ] {
        let output = stdin_run(&args, b"int value;\n");
        assert_eq!(output.status.code(), Some(1), "args={args:?}");
        assert!(output.stdout.is_empty(), "args={args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("target"),
            "args={args:?}, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
