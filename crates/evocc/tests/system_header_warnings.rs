//! System search paths change warning provenance, not C validity.
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "system-header-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("header.h"),
            "static inline int helper(int unused) { return 7; }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("source.c"),
            "#include <header.h>\nint main(void) { return helper(0) != 7; }\n",
        )
        .unwrap();
        Self(root)
    }
    fn run(&self, target: &str, includes: &[&str], extra: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
        command.args([
            "--target",
            target,
            "-Wall",
            "-Wextra",
            "-Werror",
            "--parse-only",
        ]);
        for option in includes {
            command.arg(option).arg(&self.0);
        }
        command
            .args(extra)
            .arg(self.0.join("source.c"))
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn system_headers_do_not_turn_their_unused_parameters_into_user_errors() {
    let f = Fixture::new();
    for target in ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        let system = f.run(target, &["-isystem"], &[]);
        assert!(
            system.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&system.stderr)
        );
        assert!(!String::from_utf8_lossy(&system.stderr).contains("W0002"));
        let user = f.run(target, &["-I"], &[]);
        assert!(
            !user.status.success(),
            "user header warning was suppressed: {target}"
        );
        assert!(String::from_utf8_lossy(&user.stderr).contains("W0002"));
        let duplicate = f.run(target, &["-I", "-isystem"], &[]);
        assert!(
            duplicate.status.success(),
            "system identity lost: {target}: {duplicate:?}"
        );
    }
}

#[test]
fn system_paths_do_not_suppress_user_warnings_or_language_errors() {
    let f = Fixture::new();
    std::fs::write(
        f.0.join("header.h"),
        "static inline int helper(int used) { return used; }\n",
    )
    .unwrap();
    std::fs::write(
        f.0.join("source.c"),
        "#include <header.h>\nint main(void) { int unused = 2; return helper(0); }\n",
    )
    .unwrap();
    let user = f.run("aarch64-unknown-linux-gnu", &["-isystem"], &[]);
    assert!(!user.status.success());
    assert!(String::from_utf8_lossy(&user.stderr).contains("W0001"));
    std::fs::write(f.0.join("header.h"), "#error invalid header\n").unwrap();
    let error = f.run("aarch64-unknown-linux-gnu", &["-isystem"], &[]);
    assert!(!error.status.success());
    assert!(String::from_utf8_lossy(&error.stderr).contains("invalid header"));
    std::fs::write(f.0.join("header.h"), "#warning explicit warning\n").unwrap();
    let warning = f.run("aarch64-unknown-linux-gnu", &["-isystem"], &[]);
    assert!(!warning.status.success());
    assert!(String::from_utf8_lossy(&warning.stderr).contains("W0010"));
    std::fs::write(
        f.0.join("header.h"),
        "static inline int helper(int used) { return undeclared(used); }\n",
    )
    .unwrap();
    let undeclared = f.run("aarch64-unknown-linux-gnu", &["-isystem"], &[]);
    assert!(!undeclared.status.success());
    assert!(String::from_utf8_lossy(&undeclared.stderr).contains("W0003"));
}

#[test]
fn explicit_system_warning_switches_keep_their_order() {
    let f = Fixture::new();
    for flags in [
        vec!["-Wno-system-headers"],
        vec!["-Wsystem-headers", "-Wno-system-headers"],
    ] {
        let output = f.run("aarch64-unknown-linux-gnu", &["-isystem"], &flags);
        assert!(output.status.success(), "{flags:?}: {output:?}");
    }
    for flags in [
        vec!["-Wsystem-headers"],
        vec!["-Wno-system-headers", "-Wsystem-headers"],
    ] {
        let output = f.run("aarch64-unknown-linux-gnu", &["-isystem"], &flags);
        assert!(!output.status.success(), "{flags:?}: {output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("W0002"));
    }
}

#[test]
fn header_classification_uses_search_origin_not_a_path_prefix() {
    let f = Fixture::new();
    std::fs::create_dir(f.0.join("nested")).unwrap();
    std::fs::write(
        f.0.join("nested/header.h"),
        "static inline int helper(int unused) { return 7; }\n",
    )
    .unwrap();
    let nested = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([
            "--target=aarch64-unknown-linux-gnu",
            "-Wall",
            "-Wextra",
            "-Werror",
            "--parse-only",
        ])
        .arg("-isystem")
        .arg(&f.0)
        .arg("-I")
        .arg(f.0.join("nested"))
        .arg(f.0.join("source.c"))
        .output()
        .unwrap();
    assert!(
        !nested.status.success(),
        "user directory below a system root was suppressed"
    );
    assert!(String::from_utf8_lossy(&nested.stderr).contains("W0002"));
    std::fs::write(
        f.0.join("source.c"),
        format!(
            "#include \"{}\"\nint main(void) {{ return helper(0) != 7; }}\n",
            f.0.join("header.h").display()
        ),
    )
    .unwrap();
    let absolute = f.run("aarch64-unknown-linux-gnu", &["-isystem"], &[]);
    assert!(
        !absolute.status.success(),
        "an absolute user include became a system include"
    );
    assert!(String::from_utf8_lossy(&absolute.stderr).contains("W0002"));
}

#[test]
fn quoted_children_inherit_their_system_includer() {
    let f = Fixture::new();
    std::fs::write(f.0.join("header.h"), "#include \"child.h\"\n").unwrap();
    std::fs::write(
        f.0.join("child.h"),
        "static inline int helper(int unused) { return 7; }\n",
    )
    .unwrap();
    let output = f.run("aarch64-unknown-linux-gnu", &["-isystem"], &[]);
    assert!(output.status.success(), "quoted system child: {output:?}");
}
