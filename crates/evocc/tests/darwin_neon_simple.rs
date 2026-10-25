// Apple NEON header types and the integer operations used by LLVM's simple.c.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DARWIN: &str = "aarch64-apple-darwin";
const LINUX: &str = "aarch64-unknown-linux-gnu";

struct Work(PathBuf);

impl Work {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("evocc-darwin-neon-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn source(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained NEON test: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn detail(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn compile(source: &Path, target: &str, level: &str, out: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command.current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    command.args([&format!("--target={target}"), "-std=gnu17", level]);
    if target == LINUX {
        command.arg("--sysroot=sysroot/aarch64-unknown-linux-gnu");
    }
    command
        .arg(if target == LINUX { "-S" } else { "-c" })
        .arg(source)
        .arg("-o")
        .arg(out)
        .output()
        .unwrap()
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn mfp8_storage_has_the_apple_layout_and_preserves_all_byte_images() {
    let work = Work::new("opaque");
    let source = work.source(
        "opaque.c",
        r#"
typedef __mfp8 opaque;
static void copy(const opaque *x, opaque *p) { opaque local = *x; *p = local; }
int main(void) {
    union { unsigned char byte; opaque value; } a, b;
    if (sizeof(opaque) != 1 || _Alignof(opaque) != 1) return 1;
    for (unsigned i = 0; i < 256; ++i) {
        a.byte = i;
        copy(&a.value, &b.value);
        if (b.byte != i) return 2;
    }
    return 0;
}
"#,
    );
    for level in ["-O0", "-O2"] {
        let object = work.0.join("opaque.o");
        let result = compile(&source, DARWIN, level, &object);
        assert!(result.status.success(), "{}", detail(&result));
        let program = work.0.join("opaque");
        let link = Command::new("xcrun")
            .args(["clang"])
            .arg(&object)
            .arg("-o")
            .arg(&program)
            .output()
            .unwrap();
        assert!(link.status.success(), "{}", detail(&link));
        let run = Command::new(program).output().unwrap();
        assert!(run.status.success(), "{}", detail(&run));
        let reference = work.0.join("reference");
        let result = Command::new("xcrun")
            .args(["clang", "-std=gnu17", level])
            .arg(&source)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        assert!(result.status.success(), "{}", detail(&result));
        let result = Command::new(reference).output().unwrap();
        assert_eq!(
            result.status.code(),
            run.status.code(),
            "{}",
            detail(&result)
        );
        assert_eq!(result.stdout, run.stdout);
        assert_eq!(result.stderr, run.stderr);
    }
}

#[test]
fn neon_attributes_give_lane_count_and_vector_layout() {
    let work = Work::new("attributes");
    let source = work.source(
        "attributes.c",
        r#"
typedef signed char I __attribute__((neon_vector_type(16)));
typedef unsigned char P __attribute__((neon_polyvector_type(8)));
typedef long long D __attribute__((neon_vector_type(1)));
typedef signed char G __attribute__((vector_size(16)));
typedef unsigned char U __attribute__((neon_vector_type(8)));
_Static_assert(__builtin_types_compatible_p(I, G), "NEON and GNU representation");
_Static_assert(__builtin_types_compatible_p(P, U), "polynomial representation");
#if !__has_attribute(neon_vector_type) || !__has_attribute(neon_polyvector_type)
#error missing NEON attributes
#endif
_Static_assert(sizeof(I) == 16 && _Alignof(I) == 16, "integer vector");
_Static_assert(sizeof(P) == 8 && _Alignof(P) == 8, "polynomial vector");
_Static_assert(sizeof(D) == 8 && _Alignof(D) == 8, "one lane");
I add(I a, I b) { return a + b; }
"#,
    );
    let result = compile(&source, DARWIN, "-O2", &work.0.join("attributes.o"));
    assert!(result.status.success(), "{}", detail(&result));
    let result = Command::new("xcrun")
        .args(["clang", "-std=gnu17", "-fsyntax-only"])
        .arg(source)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", detail(&result));
}

#[test]
fn the_original_apple_header_can_declare_unused_intrinsics() {
    let work = Work::new("header");
    let source = work.source(
        "header.c",
        "#include <arm_neon.h>\nint main(void) { return 0; }\n",
    );
    let result = compile(&source, DARWIN, "-O0", &work.0.join("header.o"));
    assert!(result.status.success(), "{}", detail(&result));
}

#[test]
fn opaque_fp8_rejects_numeric_operations_and_by_value_calls() {
    let work = Work::new("opaque-refusals");
    for (name, source, code) in [
        ("to", "__mfp8 f(int x) { return (__mfp8)x; }", "E2102"),
        ("from", "int f(__mfp8 x) { return (int)x; }", "E2102"),
        (
            "add",
            "__mfp8 f(__mfp8 x, __mfp8 y) { return x+y; }",
            "E2202",
        ),
        (
            "equal",
            "int f(__mfp8 x, __mfp8 y) { return x==y; }",
            "E2202",
        ),
        ("truth", "int f(__mfp8 x) { return !!x; }", "E2103"),
        ("call", "__mfp8 f(__mfp8 x) { return x; }", "E3049"),
        (
            "record",
            "struct S { __mfp8 x; }; struct S f(struct S x) { return x; }",
            "E3049",
        ),
    ] {
        let source = work.source(&format!("{name}.c"), source);
        let result = compile(&source, DARWIN, "-O0", &work.0.join("refused.o"));
        assert!(!result.status.success(), "{name}: {}", detail(&result));
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(code),
            "{name}: {}",
            detail(&result)
        );
    }
    let source = work.source("linux-type.c", "typedef __mfp8 unavailable;\n");
    let result = compile(&source, LINUX, "-O0", &work.0.join("refused.s"));
    assert!(!result.status.success(), "{}", detail(&result));
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("E1007"),
        "{}",
        detail(&result)
    );
}

#[test]
fn target_attributes_follow_the_darwin_register_policy() {
    let work = Work::new("target-attribute");
    let source = work.source(
        "target.c",
        "__attribute__((target(\"neon\"))) int f(int x) { return x+1; }\n",
    );
    let result = compile(&source, DARWIN, "-O2", &work.0.join("target.o"));
    assert!(result.status.success(), "{}", detail(&result));
    for target in [DARWIN, LINUX] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
        command.arg(format!("--target={target}"));
        if target == DARWIN {
            command.arg("-mgeneral-regs-only");
        }
        let result = command
            .args(["-S", "-O2"])
            .arg(&source)
            .arg("-o")
            .arg(work.0.join("refused.s"))
            .output()
            .unwrap();
        assert!(!result.status.success(), "{target}: {}", detail(&result));
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("E2952"),
            "{target}: {}",
            detail(&result)
        );
    }
}

#[test]
fn disabling_features_and_cpu_selections_stay_refused() {
    let work = Work::new("disabled-features");
    for features in [
        "+neon",
        "+sve",
        "sve",
        "sme",
        "crc",
        "neon,sve",
        "neon,",
        "",
        "noneon",
        "+noneon",
        "neon,noaes",
        "arch=armv8-a",
        "cpu=apple-m1",
        "tune=generic",
    ] {
        let source = work.source(
            "target.c",
            &format!("__attribute__((target(\"{features}\"))) int f(int x) {{ return x; }}\n"),
        );
        let result = compile(&source, DARWIN, "-O2", &work.0.join("refused.o"));
        assert!(!result.status.success(), "{features}: {}", detail(&result));
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("E2952"),
            "{features}: {}",
            detail(&result)
        );
    }
}

#[test]
fn acle_rounding_declarations_do_not_admit_builtin_execution() {
    let work = Work::new("acle-rounding");
    let unused = work.source(
        "unused.c",
        "#include <arm_acle.h>\nint f(int x) { return x; }\n",
    );
    let result = compile(&unused, DARWIN, "-O2", &work.0.join("unused.o"));
    assert!(result.status.success(), "{}", detail(&result));
    for (name, ty) in [
        ("__builtin_roundeven", "double"),
        ("__builtin_roundevenf", "float"),
    ] {
        let source = work.source(
            &format!("{name}.c"),
            &format!("#include <arm_acle.h>\n{ty} f({ty} x) {{ return {name}(x); }}\n"),
        );
        let result = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([
                "--target=aarch64-apple-darwin",
                "-std=gnu11",
                "-fsyntax-only",
            ])
            .arg(&source)
            .output()
            .unwrap();
        assert!(result.status.success(), "{name}: {}", detail(&result));
        let result = compile(&source, DARWIN, "-O2", &work.0.join("used.o"));
        assert!(!result.status.success(), "{name}: {}", detail(&result));
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            stderr.contains("E2949") && stderr.contains(name),
            "{name}: {}",
            detail(&result)
        );
    }
}

#[test]
fn floating_neon_declarations_typecheck_but_do_not_enable_execution() {
    let work = Work::new("floating-declarations");
    for (label, declaration) in [
        (
            "round",
            "float32x4_t f(float32x4_t h) { float32x4_t t = vrndq_f32(h); return t; }",
        ),
        (
            "set-lane",
            "float32x4_t f(float32x4_t h) { return vsetq_lane_f32(1.0f,h,2); }",
        ),
        (
            "get-lane",
            "float f(float32x4_t h) { return vgetq_lane_f32(h,2); }",
        ),
        (
            "extract",
            "float32x4_t f(float32x4_t h) { return vextq_f32(h,h,1); }",
        ),
    ] {
        let source = work.source(
            &format!("{label}.c"),
            &format!("#include <arm_neon.h>\n{declaration}\n"),
        );
        let oracle = Command::new("xcrun")
            .args(["clang", "-std=gnu11", "-fsyntax-only"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(oracle.status.success(), "{label}: {}", detail(&oracle));
        let result = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([
                "--target=aarch64-apple-darwin",
                "-std=gnu11",
                "-fsyntax-only",
            ])
            .arg(&source)
            .output()
            .unwrap();
        assert!(result.status.success(), "{label}: {}", detail(&result));
        let result = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args(["--target=aarch64-apple-darwin", "-std=gnu11", "--emit=asm"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(!result.status.success(), "{label}: {}", detail(&result));
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("E2949"),
            "{label}: {}",
            detail(&result)
        );
    }
}

#[test]
fn builtin_declarations_do_not_admit_invalid_calls_or_unknown_operations() {
    let work = Work::new("builtin-refusals");
    for (label, body, code) in [
        (
            "arity",
            "void f(void) { __builtin_neon_vst1q_v(); }",
            "E2909",
        ),
        (
            "type",
            "struct S { int x; }; void f(struct S x) { __builtin_neon_vld1q_v(x,32); }",
            "E2913",
        ),
        (
            "selector",
            "V f(void *p) { return __builtin_neon_vld1q_v(p,31); }",
            "E2949",
        ),
        (
            "unimplemented",
            "V f(V a) { return __builtin_neon_vbfdotq_f32(a,a,a,41); }",
            "E2949",
        ),
        (
            "bit-cast",
            "unsigned long f(void *p) { return __builtin_bit_cast(unsigned long,p); }",
            "E2102",
        ),
    ] {
        let source = work.source(
            "refused.c",
            &format!("typedef signed char V __attribute__((neon_vector_type(16)));\n{body}\n"),
        );
        let result = compile(&source, DARWIN, "-O0", &work.0.join("refused.o"));
        assert!(!result.status.success(), "{label}: {}", detail(&result));
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(code),
            "{label}: {}",
            detail(&result)
        );
    }
}

#[test]
fn a_null_builtin_pointer_keeps_a_runtime_obligation() {
    let work = Work::new("null-builtin");
    let source = work.source("null.c", "typedef signed char V __attribute__((neon_vector_type(16)));\nV f(void) { return __builtin_neon_vld1q_v(0,32); }\n");
    let result = compile(&source, DARWIN, "-O0", &work.0.join("null.o"));
    assert!(result.status.success(), "{}", detail(&result));
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn original_neon_simple_matches_apple_clang() {
    let work = Work::new("original");
    let source = work.source("simple.c", SIMPLE);
    for level in ["-O0", "-O2"] {
        let object = work.0.join("simple.o");
        let result = compile(&source, DARWIN, level, &object);
        assert!(result.status.success(), "{}", detail(&result));
        for (input, name) in [(&source, "reference"), (&object, "subject")] {
            let result = Command::new("xcrun")
                .args(["clang", "-std=gnu17", level])
                .arg(input)
                .arg("-o")
                .arg(work.0.join(name))
                .output()
                .unwrap();
            assert!(result.status.success(), "{}", detail(&result));
        }
        let reference = Command::new(work.0.join("reference")).output().unwrap();
        let subject = Command::new(work.0.join("subject")).output().unwrap();
        assert!(reference.status.success(), "{}", detail(&reference));
        assert!(subject.status.success(), "{}", detail(&subject));
        assert_eq!(subject.stdout, reference.stdout, "{level}");
        assert_eq!(subject.stderr, reference.stderr, "{level}");
        assert_eq!(String::from_utf8_lossy(&subject.stdout).lines().count(), 23);
    }
}

#[test]
#[ignore = "pinned LLVM original; run explicitly with the adjacent team2-goals input checkout"]
fn the_large_intrinsic_original_keeps_a_named_refusal() {
    let work = Work::new("large-original");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../evocc-runs/team2-goals/inputs/llvm-neon-2919e551/SingleSource/UnitTests/Vector/AArch64/aarch64_neon_intrinsics.c");
    assert!(
        source.is_file(),
        "missing pinned original: {}",
        source.display()
    );
    for target in [DARWIN, LINUX] {
        let result = compile(&source, target, "-O0", &work.0.join("refused.s"));
        assert!(!result.status.success(), "{target}: {}", detail(&result));
        let expected = if target == DARWIN { "E2949" } else { "E3050" };
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(expected),
            "{target}: {}",
            detail(&result)
        );
    }
}

fn intrinsic_worker(bits: u32, lanes: u32) -> String {
    format!(
        r#"#include <arm_neon.h>
typedef int{bits}_t E;
typedef int{bits}x{lanes}_t V;
typedef int{bits}x{lanes}x2_t Pair;
V load(const E *p) {{ return vld1q_s{bits}(p); }}
void store(E *p, V x) {{ vst1q_s{bits}(p, x); }}
V add(V a, V b) {{ return vaddq_s{bits}(a, b); }}
V broadcast(E x) {{ return vmovq_n_s{bits}(x); }}
Pair zip(V a, V b) {{ return vzipq_s{bits}(a, b); }}
Pair unzip(V a, V b) {{ return vuzpq_s{bits}(a, b); }}
"#
    )
}

fn intrinsic_driver(bits: u32, lanes: u32) -> String {
    format!(
        r#"#include <arm_neon.h>
typedef int{bits}_t E;
typedef int{bits}x{lanes}_t V;
typedef int{bits}x{lanes}x2_t Pair;
V load(const E *); void store(E *, V); V add(V, V); V broadcast(E);
Pair zip(V, V); Pair unzip(V, V);
int main(void) {{
    E a[{lanes}], b[{lanes}], out[{lanes}];
    for (int i = 0; i < {lanes}; ++i) {{ a[i] = 3*i - 20; b[i] = 90-i; }}
    V x = load(a), y = load(b);
    store(out, x);
    for (int i = 0; i < {lanes}; ++i) if (out[i] != a[i]) return 1;
    store(out, add(x, y));
    for (int i = 0; i < {lanes}; ++i) if (out[i] != a[i]+b[i]) return 2;
    store(out, broadcast(-7));
    for (int i = 0; i < {lanes}; ++i) if (out[i] != -7) return 3;
    Pair z = zip(x, y);
    for (int half = 0; half < 2; ++half) {{
        store(out, z.val[half]);
        for (int i = 0; i < {lanes}/2; ++i) {{
            if (out[2*i] != a[half*({lanes}/2)+i]) return 4;
            if (out[2*i+1] != b[half*({lanes}/2)+i]) return 5;
        }}
    }}
    Pair u = unzip(x, y);
    for (int half = 0; half < 2; ++half) {{
        store(out, u.val[half]);
        for (int i = 0; i < {lanes}/2; ++i) {{
            if (out[i] != a[2*i+half]) return 6;
            if (out[{lanes}/2+i] != b[2*i+half]) return 7;
        }}
    }}
    return 0;
}}
"#
    )
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn all_eighteen_intrinsics_cross_with_apple_clang() {
    let work = Work::new("cross");
    for (bits, lanes) in [(8, 16), (16, 8), (32, 4)] {
        let worker = work.source("worker.c", &intrinsic_worker(bits, lanes));
        let driver = work.source("driver.c", &intrinsic_driver(bits, lanes));
        for level in ["-O0", "-O2"] {
            for (source, name) in [(&worker, "worker"), (&driver, "driver")] {
                let result = compile(source, DARWIN, level, &work.0.join(format!("e-{name}.o")));
                assert!(result.status.success(), "{}", detail(&result));
                let result = Command::new("xcrun")
                    .args(["clang", "-std=gnu17", level, "-c"])
                    .arg(source)
                    .arg("-o")
                    .arg(work.0.join(format!("c-{name}.o")))
                    .output()
                    .unwrap();
                assert!(result.status.success(), "{}", detail(&result));
            }
            for (caller, callee) in [("e", "c"), ("c", "e"), ("e", "e"), ("c", "c")] {
                let program = work.0.join("cross");
                let result = Command::new("xcrun")
                    .arg("clang")
                    .arg(work.0.join(format!("{caller}-driver.o")))
                    .arg(work.0.join(format!("{callee}-worker.o")))
                    .arg("-o")
                    .arg(&program)
                    .output()
                    .unwrap();
                assert!(result.status.success(), "{}", detail(&result));
                let result = Command::new(program).output().unwrap();
                assert!(
                    result.status.success(),
                    "s{bits} {level} {caller}/{callee}: {}",
                    detail(&result)
                );
            }
        }
    }
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn all_eighteen_intrinsics_cross_with_gcc_14() {
    let work = Work::new("linux-cross");
    let mut script = String::from("set -eu\ncd /work\n");
    for (bits, lanes) in [(8, 16), (16, 8), (32, 4)] {
        for (label, text) in [
            ("worker", intrinsic_worker(bits, lanes)),
            ("driver", intrinsic_driver(bits, lanes)),
        ] {
            let source = work.source(&format!("{bits}-{label}.c"), &text);
            for level in ["-O0", "-O2"] {
                let stem = format!("{bits}-{label}{level}");
                let result = compile(&source, LINUX, level, &work.0.join(format!("e-{stem}.s")));
                assert!(result.status.success(), "{}", detail(&result));
                script.push_str(&format!("gcc-14 -c e-{stem}.s -o e-{stem}.o\ngcc-14 -std=gnu17 {level} -c {bits}-{label}.c -o c-{stem}.o\n"));
            }
        }
        for level in ["-O0", "-O2"] {
            for (caller, callee) in [("e", "c"), ("c", "e"), ("e", "e"), ("c", "c")] {
                script.push_str(&format!("gcc-14 {caller}-{bits}-driver{level}.o {callee}-{bits}-worker{level}.o -o cross\n./cross\necho 's{bits} {level} {caller}/{callee}: PASS'\n"));
            }
        }
    }
    work.source("cross.sh", &script);
    let result = Command::new("container")
        .args([
            "run",
            "--rm",
            "--platform",
            "linux/arm64",
            "--network",
            "none",
            "-v",
        ])
        .arg(format!("{}:/work", work.0.display()))
        .args(["evo-nexlab:sealed", "sh", "/work/cross.sh"])
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", detail(&result));
    assert_eq!(
        String::from_utf8_lossy(&result.stdout)
            .matches(": PASS")
            .count(),
        24
    );
}

// LLVM test-suite 2919e551, SingleSource/UnitTests/Vector/NEON/simple.c.
// Original bytes: e99e4973fc99bebecf895cc5c406df1b13ee707af9e51c67e2277795b747e181.
const SIMPLE: &str = r#"#include <stdio.h>
#include <arm_neon.h>

typedef int8x16_t v16i8;
typedef int16x8_t v8i16;
typedef int32x4_t v4i32;

typedef int8x16x2_t v16i8x2;
typedef int16x8x2_t v8i16x2;
typedef int32x4x2_t v4i32x2;

v16i8 init_v16i8(int8_t a0, int8_t a1, int8_t a2, int8_t a3,
                 int8_t a4, int8_t a5, int8_t a6, int8_t a7,
                 int8_t a8, int8_t a9, int8_t a10, int8_t a11,
                 int8_t a12, int8_t a13, int8_t a14, int8_t a15) {
  int8_t d[16];

  d[0] = a0; d[1] = a1; d[2] = a2; d[3] = a3;
  d[4] = a4; d[5] = a5; d[6] = a6; d[7] = a7;
  d[8] = a8; d[9] = a9; d[10] = a10; d[11] = a11;
  d[12] = a12; d[13] = a13; d[14] = a14; d[15] = a15;

  return vld1q_s8(d);
}

v8i16 init_v8i16(int16_t a0, int16_t a1, int16_t a2, int16_t a3,
                 int16_t a4, int16_t a5, int16_t a6, int16_t a7) {
  int16_t d[8];

  d[0] = a0; d[1] = a1; d[2] = a2; d[3] = a3;
  d[4] = a4; d[5] = a5; d[6] = a6; d[7] = a7;

  return vld1q_s16(d);
}

v4i32 init_v4i32(int32_t a0, int32_t a1, int32_t a2, int32_t a3) {
  int32_t d[8];

  d[0] = a0; d[1] = a1; d[2] = a2; d[3] = a3;

  return vld1q_s32(d);
}

void print_v16i8(v16i8 a) {
  int8_t d[16];
  vst1q_s8(d, a);
  printf("(%d, %d, %d, %d, %d, %d, %d, %d, "
          "%d, %d, %d, %d, %d, %d, %d, %d)",
          d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7],
          d[8], d[9], d[10], d[11], d[12], d[13], d[14], d[15]);
}
void print_v8i16(v8i16 a) {
  int16_t d[8];
  vst1q_s16(d, a);
  printf("(%d, %d, %d, %d, %d, %d, %d, %d)",
          d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]);
}
void print_v4i32(v4i32 a) {
  int32_t d[4];
  vst1q_s32(d, a);
  printf("(%d, %d, %d, %d)", d[0], d[1], d[2], d[3]);
}

void test_basic() {
  printf("%s\n", __FUNCTION__);

  v16i8 a0_0 = vmovq_n_s8(1);
  v16i8 a0_1 = init_v16i8(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);
  v16i8 a0_2 = vaddq_s8(a0_0, a0_1);
  printf("a0_0: ");
  print_v16i8(a0_0);
  printf("\n");
  printf("a0_1: ");
  print_v16i8(a0_1);
  printf("\n");
  printf("a0_2: ");
  print_v16i8(a0_2);
  printf("\n");

  v8i16 a1_0 = vmovq_n_s16(1);
  v8i16 a1_1 = init_v8i16(0, 1, 2, 3, 4, 5, 6, 7);
  v8i16 a1_2 = vaddq_s16(a1_0, a1_1);
  printf("a1_0: ");
  print_v8i16(a1_0);
  printf("\n");
  printf("a1_1: ");
  print_v8i16(a1_1);
  printf("\n");
  printf("a1_2: ");
  print_v8i16(a1_2);
  printf("\n");

  v4i32 a2_0 = vmovq_n_s32(1);
  v4i32 a2_1 = init_v4i32(0, 1, 2, 3);
  v4i32 a2_2 = vaddq_s32(a2_0, a2_1);
  printf("a2_0: ");
  print_v4i32(a2_0);
  printf("\n");
  printf("a2_1: ");
  print_v4i32(a2_1);
  printf("\n");
  printf("a2_2: ");
  print_v4i32(a2_2);
  printf("\n");
}

void test_zip() {
  printf("%s\n", __FUNCTION__);

  v16i8 a0_0 = init_v16i8(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);
  v16i8 a0_1 = init_v16i8(15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
  v16i8x2 a0_2 = vzipq_s8(a0_0, a0_1);
  v16i8x2 a0_3 = vuzpq_s8(a0_2.val[0], a0_2.val[1]);
  printf("a0_2.val[0]: ");
  print_v16i8(a0_2.val[0]);
  printf("\n");
  printf("a0_2.val[1]: ");
  print_v16i8(a0_2.val[1]);
  printf("\n");
  printf("a0_3.val[0]: ");
  print_v16i8(a0_3.val[0]);
  printf("\n");
  printf("a0_3.val[1]: ");
  print_v16i8(a0_3.val[1]);
  printf("\n");

  v8i16 a1_0 = init_v8i16(0, 1, 2, 3, 4, 5, 6, 7);
  v8i16 a1_1 = init_v8i16(7, 6, 5, 4, 3, 2, 1, 0);
  v8i16x2 a1_2 = vzipq_s16(a1_0, a1_1);
  v8i16x2 a1_3 = vuzpq_s16(a1_2.val[0], a1_2.val[1]);
  printf("a1_2.val[0]: ");
  print_v8i16(a1_2.val[0]);
  printf("\n");
  printf("a1_2.val[1]: ");
  print_v8i16(a1_2.val[1]);
  printf("\n");
  printf("a1_3.val[0]: ");
  print_v8i16(a1_3.val[0]);
  printf("\n");
  printf("a1_3.val[1]: ");
  print_v8i16(a1_3.val[1]);
  printf("\n");

  v4i32 a2_0 = init_v4i32(0, 1, 2, 3);
  v4i32 a2_1 = init_v4i32(3, 2, 1, 0);
  v4i32x2 a2_2 = vzipq_s32(a2_0, a2_1);
  v4i32x2 a2_3 = vuzpq_s32(a2_2.val[0], a2_2.val[1]);
  printf("a2_2.val[0]: ");
  print_v4i32(a2_2.val[0]);
  printf("\n");
  printf("a2_2.val[1]: ");
  print_v4i32(a2_2.val[1]);
  printf("\n");
  printf("a2_3.val[0]: ");
  print_v4i32(a2_3.val[0]);
  printf("\n");
  printf("a2_3.val[1]: ");
  print_v4i32(a2_3.val[1]);
  printf("\n");
}

int main() {
  test_basic();
  test_zip();
  return 0;
}
"#;
