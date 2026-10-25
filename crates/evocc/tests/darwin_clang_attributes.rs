//! Darwin's Clang attributes: type/ABI-changing attributes are refused,
//! diagnostics-only attributes are accepted without effect.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn command(target: &str) -> Command {
    let mut command = Command::new(evocc());
    command.args(["--target", target, "-std=gnu17", "-w"]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command
}

fn run(source: &str, target: &str, mode: &str) -> Output {
    let mut command = command(target);
    command
        .args([mode, "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assembly(source: &str, target: &str) -> Output {
    let directory = std::env::temp_dir().join(format!(
        "evocc-attr-{}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("source.c");
    std::fs::write(&path, source).unwrap();
    let output = command(target)
        .args(["-S", "-o", "-"])
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&directory);
    output
}

const REFUSED: [(&str, &str); 12] = [
    (
        "ext_vector_type",
        "typedef float f4 __attribute__((ext_vector_type(4)));\nfloat f(f4 v) { return v.x; }\n",
    ),
    (
        "matrix_type",
        "typedef float m __attribute__((matrix_type(2,2)));\nfloat f(m a) { return a[0][0]; }\n",
    ),
    (
        "address_space",
        "int x __attribute__((address_space(1)));\nint f(void) { return x; }\n",
    ),
    (
        "pass_object_size",
        "void f(char *p __attribute__((pass_object_size(0))));\nvoid f(char *p) { (void)p; }\n",
    ),
    (
        "noderef",
        "void f(int * __attribute__((noderef)) p);\nvoid f(int *p) { (void)p; }\n",
    ),
    (
        "ms_struct",
        "struct __attribute__((ms_struct)) s { char a; int b : 4; char c; };\n",
    ),
    (
        "counted_by",
        "struct S { int n; int a[] __attribute__((counted_by(n))); };\n",
    ),
    (
        "counted_by_or_null",
        "struct S { int n; int *a __attribute__((counted_by_or_null(n))); };\n",
    ),
    (
        "sized_by",
        "struct S { int n; int *a __attribute__((sized_by(n))); };\n",
    ),
    (
        "sized_by_or_null",
        "struct S { int n; int *a __attribute__((sized_by_or_null(n))); };\n",
    ),
    (
        "ended_by",
        "struct S { int n; int *a __attribute__((ended_by(0))); };\n",
    ),
    (
        "internal_linkage",
        "int f(void) __attribute__((internal_linkage));\nint f(void) { return 0; }\n",
    ),
];

const ACCEPTED: [(&str, &str, &str); 14] = [
    ("annotate", "int x __attribute__((annotate(\"tag\")));\n", " __attribute__((annotate(\"tag\")))"),
    (
        "enable_if",
        "void f(int x) __attribute__((enable_if(1, \"ok\")));\nvoid f(int x) { (void)x; }\n",
        " __attribute__((enable_if(1, \"ok\")))",
    ),
    (
        "diagnose_if",
        "void f(int x) __attribute__((diagnose_if(0, \"msg\", \"warning\")));\nvoid f(int x) { (void)x; }\n",
        " __attribute__((diagnose_if(0, \"msg\", \"warning\")))",
    ),
    (
        "swift_name",
        "void f(void) __attribute__((swift_name(\"f()\")));\nvoid f(void) {}\n",
        " __attribute__((swift_name(\"f()\")))",
    ),
    (
        "objc_bridge",
        "struct S { int x; } __attribute__((objc_bridge(NSObject)));\n",
        " __attribute__((objc_bridge(NSObject)))",
    ),
    (
        "ns_returns_retained",
        "void *f(void) __attribute__((ns_returns_retained));\nvoid *f(void) { return 0; }\n",
        " __attribute__((ns_returns_retained))",
    ),
    (
        "cf_returns_retained",
        "void *f(void) __attribute__((cf_returns_retained));\nvoid *f(void) { return 0; }\n",
        " __attribute__((cf_returns_retained))",
    ),
    (
        "os_returns_retained",
        "void *f(void) __attribute__((os_returns_retained));\nvoid *f(void) { return 0; }\n",
        " __attribute__((os_returns_retained))",
    ),
    (
        "btf_type_tag",
        "int x __attribute__((btf_type_tag(\"tag\")));\n",
        " __attribute__((btf_type_tag(\"tag\")))",
    ),
    (
        "preferred_name",
        "struct S { int x; } __attribute__((preferred_name(struct S)));\n",
        " __attribute__((preferred_name(struct S)))",
    ),
    (
        "warn_unused_result",
        "int f(void) __attribute__((warn_unused_result));\nint f(void) { return 1; }\n",
        " __attribute__((warn_unused_result))",
    ),
    (
        "align_value",
        "typedef int *p __attribute__((align_value(16)));\n",
        " __attribute__((align_value(16)))",
    ),
    (
        "unsafe_buffer_usage",
        "void f(void) __attribute__((unsafe_buffer_usage));\n",
        " __attribute__((unsafe_buffer_usage))",
    ),
    (
        "mig_server_routine",
        "int f(void) __attribute__((mig_server_routine));\n",
        " __attribute__((mig_server_routine))",
    ),
];

#[test]
fn darwin_refuses_attributes_that_change_type_or_abi() {
    for (name, source) in REFUSED {
        let output = assembly(source, DARWIN);
        assert!(!output.status.success(), "{name}\n{}", text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("error[E2967]"),
            "{name}\n{}",
            text(&output)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(name),
            "{name} should be named in the diagnostic\n{}",
            text(&output)
        );
    }
}

#[test]
fn darwin_accepts_internal_linkage_on_a_static_declaration() {
    let source = "static int f(void) __attribute__((internal_linkage));\nint f(void) { return 0; }\nint main(void) { return f(); }\n";
    let output = assembly(source, DARWIN);
    assert!(output.status.success(), "{}", text(&output));
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains(".globl _f"),
        "static internal_linkage leaked a global symbol"
    );
}

#[test]
fn internal_linkage_uses_the_declaration_storage_class() {
    let source = "int __attribute__((internal_linkage)) g(int a[static 2]) { return a[0]; }\n";
    let output = assembly(source, DARWIN);
    assert!(!output.status.success(), "{}", text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("error[E2967]"),
        "{}",
        text(&output)
    );
    let named = assembly(
        "static int f(void) __attribute__((internal_linkage));\nint f(void) { return 0; }\n",
        DARWIN,
    );
    assert!(named.status.success(), "{}", text(&named));
    assert!(!String::from_utf8_lossy(&named.stdout).contains(".globl _f"));
}

#[test]
fn darwin_refuses_ms_struct_on_and_accepts_off_or_reset() {
    let source = "#pragma ms_struct on\nstruct s { char a; int b : 4; char c; };\nint main(void) { return 0; }\n";
    let output = assembly(source, DARWIN);
    assert!(!output.status.success(), "{}", text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("error[E2967]"),
        "{}",
        text(&output)
    );
    for action in ["off", "reset"] {
        let source = format!(
            "#pragma ms_struct {action}\nstruct s {{ char a; int b : 4; char c; }};\nint main(void) {{ return 0; }}\n"
        );
        let output = assembly(&source, DARWIN);
        assert!(output.status.success(), "{action}\n{}", text(&output));
    }
}

#[test]
fn darwin_accepts_diagnostic_only_attributes_without_effect() {
    for (name, source, spelling) in ACCEPTED {
        let attributed = assembly(source, DARWIN);
        assert!(attributed.status.success(), "{name}\n{}", text(&attributed));
        let without = source.replace(spelling, "");
        let plain = assembly(&without, DARWIN);
        assert!(
            plain.status.success(),
            "{name} without attribute\n{}",
            text(&plain)
        );
        assert_eq!(
            attributed.stdout, plain.stdout,
            "{name} changed the emitted assembly"
        );
    }
}

#[test]
fn darwin_accepts_nullability_keywords_without_effect() {
    for keyword in ["_Nonnull", "_Nullable", "_Null_unspecified"] {
        let source = format!("int * {keyword} p;\nint f(void) {{ return p ? *p : 0; }}\n");
        let attributed = assembly(&source, DARWIN);
        assert!(
            attributed.status.success(),
            "{keyword}\n{}",
            text(&attributed)
        );
        let plain = assembly("int * p;\nint f(void) { return p ? *p : 0; }\n", DARWIN);
        assert!(
            plain.status.success(),
            "{keyword} without keyword\n{}",
            text(&plain)
        );
        assert_eq!(
            attributed.stdout, plain.stdout,
            "{keyword} changed the emitted assembly"
        );
    }
}

#[test]
fn darwin_nullability_qualifiers_and_conflicts() {
    let source = "int * _Nonnull const p = 0;\nint main(void) { return p != 0; }\n";
    let attributed = assembly(source, DARWIN);
    assert!(attributed.status.success(), "{}", text(&attributed));
    let plain = assembly(
        "int * const p = 0;\nint main(void) { return p != 0; }\n",
        DARWIN,
    );
    assert!(plain.status.success(), "{}", text(&plain));
    assert_eq!(attributed.stdout, plain.stdout);

    let conflicting = assembly("int * _Nonnull _Nullable z = 0;\n", DARWIN);
    assert!(!conflicting.status.success(), "{}", text(&conflicting));
    assert!(
        String::from_utf8_lossy(&conflicting.stderr).contains("error[E1001]"),
        "{}",
        text(&conflicting)
    );

    let subscript = assembly("void g(int a[_Nonnull]);\n", DARWIN);
    assert!(!subscript.status.success(), "{}", text(&subscript));
    assert!(
        String::from_utf8_lossy(&subscript.stderr).contains("error[E2001]"),
        "{}",
        text(&subscript)
    );
}

#[test]
fn linux_keeps_clang_attributes_ignored_like_gcc() {
    let source = "typedef float f4 __attribute__((ext_vector_type(4)));\nf4 add(f4 a, f4 b) { return a + b; }\nfloat f(void) { return add(1.5f, 2.25f); }\n";
    let output = assembly(source, LINUX);
    assert!(output.status.success(), "{}\n{}", source, text(&output));
    let assembly = String::from_utf8_lossy(&output.stdout);
    assert!(assembly.contains("fadd"), "{assembly}");
}

#[test]
fn darwin_has_attribute_answers_zero_for_refused_names() {
    let source = "#if __has_attribute(ext_vector_type) || __has_attribute(matrix_type) || __has_attribute(address_space) || __has_attribute(pass_object_size) || __has_attribute(noderef) || __has_attribute(ms_struct) || __has_attribute(counted_by) || __has_attribute(counted_by_or_null) || __has_attribute(sized_by) || __has_attribute(sized_by_or_null) || __has_attribute(ended_by) || __has_attribute(internal_linkage)\n#error refused attribute claims support\n#endif\nint main(void) { return 0; }\n";
    let output = run(source, DARWIN, "-E");
    assert!(output.status.success(), "{}", text(&output));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("#error"));
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_matches_gcc_for_the_example() {
    let directory = std::env::temp_dir().join(format!(
        "evocc-darwin-attributes-linux-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let source = "typedef float f4 __attribute__((ext_vector_type(4)));\nf4 add(f4 a, f4 b) { return a + b; }\nint main(void) { return add(1.5f, 2.25f) == 3.75f ? 0 : 1; }\n";
    let source_file = directory.join("example.c");
    let assembly = directory.join("example.s");
    std::fs::write(&source_file, source).unwrap();
    let compiled = command(LINUX)
        .args(["-S", "-o", assembly.to_str().unwrap()])
        .arg(&source_file)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{}", text(&compiled));
    let script = "cd /work && gcc-14 -std=gnu17 -O2 example.c -o ref && ./ref; echo ref=$? && gcc-14 example.s -o evocc && ./evocc; echo evocc=$?";
    let output = Command::new("container")
        .args([
            "run",
            "--rm",
            "--platform",
            "linux/arm64",
            "--network",
            "none",
            "-v",
        ])
        .arg(format!("{}:/work", directory.display()))
        .args(["evo-nexlab:sealed", "sh", "-c", script])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("ref=0"), "{stdout}");
    assert!(stdout.contains("evocc=0"), "{stdout}");
}
