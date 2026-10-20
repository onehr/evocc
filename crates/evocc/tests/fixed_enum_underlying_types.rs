//! Enumerations with a fixed underlying type (C23 6.7.2.2; EvoIR 10.1.14,
//! D19). `enum E : T { ... }`, `enum E : T;` and `typedef enum E : T { ... }
//! name;` are accepted in every language mode, as GCC 14 and Apple clang
//! accept them before C23. The underlying type sets the size, the alignment,
//! the range of the enumerators and the call ABI, and each enumerator has the
//! enumerated type. Darwin's `kern/kcdata.h` and `malloc/malloc.h` use them.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
const TARGETS: [&str; 2] = [LINUX, DARWIN];
const LANGUAGES: [&str; 8] = [
    "-std=c89",
    "-std=gnu89",
    "-std=c99",
    "-std=gnu99",
    "-std=c11",
    "-std=gnu11",
    "-std=c17",
    "-std=gnu17",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("evocc-fixed-enum-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn source(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, source).unwrap();
        path
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed fixed-enum probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn evocc(source: &Path, target: &str, mode: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command
        .arg(format!("--target={target}"))
        .args(["-w", "--error-format=short"]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ));
    }
    command.args(mode).arg(source).output().unwrap()
}

/// Sizes, alignments, constant types, compatibility, promotions and
/// conversions, as GCC 14 and Apple clang give them. C89 syntax only.
const LAYOUT: &str = r#"enum E8 : unsigned char { A8 = 1, B8 = 200 };
enum S8 : signed char { AS = -3, BS = 100 };
enum E16 : unsigned short { A16 = 65535 };
enum E64 : unsigned long long { A64 = 1, B64 = 0xffffffffffffffffULL };
enum EL : long { AL = -1 };
enum F : short;
enum F forward_object;
enum F : short { F1 = 7 };
enum F : short;
typedef enum T : unsigned short { T1, T2 } tname;
typedef unsigned char U8;
enum : U8 { ANON = 255 };
enum Q : const int { Q1 };
enum C : char { C1 = 'a' };
enum __attribute__((packed)) PK : int { PK1 };
enum IN : unsigned char { IN1 = 255, IN2 = IN1 - 1, IN3 = sizeof(enum IN) };
struct BF { char c; enum E8 f : 3; enum E8 g : 5; enum E8 h; };
struct UB { enum E8 : 3; enum E8 last : 5; };

_Static_assert(sizeof(enum E8) == 1 && __alignof__(enum E8) == 1, "E8");
_Static_assert(sizeof(enum S8) == 1 && __alignof__(enum S8) == 1, "S8");
_Static_assert(sizeof(enum E16) == 2 && __alignof__(enum E16) == 2, "E16");
_Static_assert(sizeof(enum E64) == 8 && __alignof__(enum E64) == 8, "E64");
_Static_assert(sizeof(enum EL) == 8 && sizeof(enum F) == 2, "EL F");
_Static_assert(sizeof forward_object == 2, "forward object");
_Static_assert(sizeof(tname) == 2 && sizeof(enum T) == 2, "typedef");
_Static_assert(sizeof(enum Q) == 4 && sizeof(enum C) == 1, "qualifier and char");
_Static_assert(sizeof(enum PK) == 4, "packed does not change a fixed type");
_Static_assert(sizeof(struct BF) == 3 && sizeof(struct UB) == 1, "bit-fields");
_Static_assert(sizeof(A8) == 1 && sizeof(A64) == 8 && sizeof(ANON) == 1, "constants");
_Static_assert(__builtin_types_compatible_p(__typeof__(A8), enum E8), "A8 type");
_Static_assert(__builtin_types_compatible_p(__typeof__(T2), tname), "T2 type");
_Static_assert(__builtin_types_compatible_p(enum E8, unsigned char), "E8 compatible");
_Static_assert(!__builtin_types_compatible_p(enum E8, signed char), "E8 distinct");
_Static_assert(__builtin_types_compatible_p(enum E64, unsigned long long), "E64");
_Static_assert(!__builtin_types_compatible_p(enum E64, unsigned long), "E64 distinct");
_Static_assert(__builtin_types_compatible_p(enum C, char), "C compatible");
_Static_assert(__builtin_types_compatible_p(enum Q, int), "Q compatible");
_Static_assert(B8 == 200 && AS == -3 && A16 == 65535 && AL == -1, "values");
_Static_assert(B64 == 0xffffffffffffffffULL && T2 == 1 && F1 == 7, "values");
_Static_assert(ANON == 255 && IN2 == 254 && IN3 == 1, "values");
_Static_assert(A8 - 2 == -1 && -AS == 3, "E8 and S8 promote to int");
_Static_assert(B64 + 1 == 0, "E64 is an unsigned 64-bit type");
_Static_assert((enum E8)300 == 44 && (enum S8)200 == -56, "conversions");

void takes_unsigned_char(unsigned char);
void takes_unsigned_char(enum E8);

int main(void) {
    volatile int big = 300;
    enum E8 narrow = big;
    enum S8 negative = (enum S8)(big - 100);
    enum E64 wide = B64;
    struct BF fields = { 0 };
    fields.f = A8 + 7;
    fields.g = 31;
    fields.h = B8;
    wide += 2;
    return !(narrow == 44 && negative == -56 && wide == 1 && fields.f == 0
             && fields.g == 31 && fields.h == 200 && sizeof narrow == 1);
}
"#;

#[test]
fn fixed_enumerations_follow_their_underlying_type_in_every_language_mode() {
    let work = Work::new("layout");
    let path = work.source("layout.c", LAYOUT);
    for target in TARGETS {
        for language in LANGUAGES {
            let output = evocc(&path, target, &[language, "--interpret-c-hir"]);
            let label = format!("{target} {language}");
            assert!(output.status.success(), "{label}\n{}", text(&output));
            assert_eq!(output.stdout, b"0\n", "{label}\n{}", text(&output));
        }
        for mode in [
            &["--interpret-evoir"][..],
            &["-O0", "--emit=asm"],
            &["-O2", "--emit=asm"],
        ] {
            let output = evocc(&path, target, mode);
            let label = format!("{target} {mode:?}");
            assert!(output.status.success(), "{label}\n{}", text(&output));
            if mode[0] == "--interpret-evoir" {
                assert_eq!(output.stdout, b"0\n", "{label}\n{}", text(&output));
            }
        }
    }
}

/// Constraint violations and the shapes that the reference compilers do not
/// agree on, with the code of each.
#[test]
fn invalid_and_unsupported_fixed_enumerations_keep_named_diagnostics() {
    let work = Work::new("diagnostics");
    let cases: [(&str, &str, &str); 25] = [
        ("explicit", "enum E : unsigned char { A = 256 };", "E1306"),
        (
            "implicit",
            "enum E : unsigned char { A = 255, B };",
            "E1306",
        ),
        ("negative", "enum E : unsigned char { A = -1 };", "E1306"),
        ("signed", "enum : signed char { X = 128 };", "E1306"),
        (
            "expression",
            "enum E : unsigned char { B = 255, C = B + 1 };",
            "E1306",
        ),
        (
            "unsigned",
            "enum E : unsigned { A = 0xffffffff, B = -1 };",
            "E1306",
        ),
        (
            "wide",
            "enum E : unsigned long long { A = 18446744073709551615ULL, B };",
            "E1306",
        ),
        ("float", "enum E : float { A };", "E1307"),
        ("enum", "enum X { Q }; enum E : enum X;", "E1307"),
        ("record", "struct R { int r; }; enum E : struct R;", "E1307"),
        ("restrict", "enum E : int restrict { A };", "E1307"),
        ("mismatch", "enum E : int; enum E : long;", "E1307"),
        (
            "defined-mismatch",
            "enum E : int { A }; enum E : long;",
            "E1307",
        ),
        ("defined-without", "enum E : int; enum E { A };", "E1307"),
        ("declared-without", "enum E { A }; enum E : int;", "E1307"),
        ("forward-without", "enum E; enum E : int { A };", "E1307"),
        ("declarator", "enum E : int x;", "E1307"),
        ("parameter", "void f(enum E : int x);", "E1307"),
        (
            "cast",
            "enum E8 : unsigned char { Z }; int x = (enum E8 : unsigned char)1;",
            "E1307",
        ),
        ("typedef", "typedef enum E : int;", "E1307"),
        ("bool", "enum E : _Bool { A };", "E2958"),
        ("int128", "enum E : __int128 { A };", "E2958"),
        ("atomic", "enum E : _Atomic int { A };", "E2958"),
        (
            "aligned",
            "enum __attribute__((aligned(8))) E : char { A };",
            "E2903",
        ),
        (
            "type-attribute",
            "enum E : int __attribute__((packed)) { A };",
            "E2903",
        ),
    ];
    for (name, source, code) in cases {
        let path = work.source(
            &format!("{name}.c"),
            &format!("{source}\nint main(void) {{ return 0; }}\n"),
        );
        for target in TARGETS {
            let output = evocc(&path, target, &["-fsyntax-only"]);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.code(), Some(1), "{name} {target}\n{stderr}");
            assert!(
                stderr
                    .lines()
                    .next()
                    .is_some_and(|line| line.contains(code)),
                "{name} {target}: expected {code}\n{stderr}"
            );
        }
    }
    let path = work.source(
        "duplicate.c",
        "enum E : int { A }; enum E : int { B };\nint main(void) { return 0; }\n",
    );
    for target in TARGETS {
        let output = evocc(&path, target, &["-fsyntax-only"]);
        assert_eq!(output.status.code(), Some(1), "{target}\n{}", text(&output));
        assert!(String::from_utf8_lossy(&output.stderr).contains("E1101"));
    }
}

/// The Darwin SDK headers of the sweep's fixed-enumeration groups.
#[cfg(target_os = "macos")]
#[test]
fn darwin_sdk_headers_with_fixed_enumerations_compile() {
    let work = Work::new("darwin-headers");
    for header in [
        "kern/kcdata.h",
        "malloc/malloc.h",
        "net/if_var_status.h",
        "corpses/task_corpse.h",
        "objc/objc-auto.h",
    ] {
        let path = work.source(
            "unit.c",
            &format!("#include <{header}>\nint main(void) {{ return 0; }}\n"),
        );
        for level in ["-O0", "-O2"] {
            let output = evocc(&path, DARWIN, &[level, "--emit=asm"]);
            assert!(
                output.status.success(),
                "{header} {level}\n{}",
                text(&output)
            );
        }
    }
    let path = work.source(
        "values.c",
        "#include <malloc/malloc.h>\n#include <kern/kcdata.h>\n\
         _Static_assert(sizeof(malloc_zone_malloc_options_t) == 8, \"malloc\");\n\
         _Static_assert(sizeof(enum task_exec_flags) == 8, \"kcdata\");\n\
         _Static_assert(MALLOC_ZONE_MALLOC_OPTION_CANONICAL_TAG == 2, \"value\");\n\
         int main(void) { return 0; }\n",
    );
    let output = evocc(&path, DARWIN, &["--check"]);
    assert!(output.status.success(), "{}", text(&output));
}

/// The two translation units of the crossed call test. The callee and the
/// caller pass and return enumerations with an 8-bit and a 64-bit underlying
/// type, in registers and, from the ninth argument on, on the stack.
const CROSS_HEADER: &str = r#"enum Small : unsigned char { SMALL_ZERO, SMALL_TOP = 255 };
enum Tiny : signed char { TINY_LOW = -128, TINY_HIGH = 127 };
enum Wide : unsigned long long { WIDE_ZERO, WIDE_TOP = 0xffffffffffffffffULL };
enum Small step_small(enum Small value, int step);
enum Tiny negate_tiny(enum Tiny value);
enum Wide mix_wide(enum Wide a, enum Small b, enum Wide c);
long sum_ten(enum Small a0, enum Tiny a1, enum Small a2, enum Tiny a3,
             enum Small a4, enum Tiny a5, enum Wide a6, enum Small a7,
             enum Tiny a8, enum Small a9);
"#;

const CROSS_CALLEE: &str = r#"#include "cross.h"
enum Small step_small(enum Small value, int step) { return (enum Small)(value + step); }
enum Tiny negate_tiny(enum Tiny value) { return (enum Tiny)-value; }
enum Wide mix_wide(enum Wide a, enum Small b, enum Wide c) { return a * 3 + b - c; }
long sum_ten(enum Small a0, enum Tiny a1, enum Small a2, enum Tiny a3,
             enum Small a4, enum Tiny a5, enum Wide a6, enum Small a7,
             enum Tiny a8, enum Small a9) {
    return a0 + a1 * 10 + a2 * 100 - a3 + a4 + a5 + (long)(a6 >> 60) + a7 + a8 * 1000 + a9;
}
"#;

const CROSS_CALLER: &str = r#"#include "cross.h"
static volatile int step = 10;
static volatile enum Small top = SMALL_TOP;
static volatile enum Tiny low = TINY_LOW;
int main(void) {
    int wrong = 0;
    int stepped = step_small(top - 5, step);
    unsigned wrapped = step_small(SMALL_TOP, 1);
    wrong += stepped != 4;
    wrong += wrapped != 0;
    wrong += negate_tiny(low) != TINY_LOW;
    wrong += negate_tiny(TINY_HIGH) != -127;
    wrong += mix_wide(WIDE_TOP, SMALL_TOP, 2) != 0xffffffffffffffffULL * 3 + 253;
    wrong += sum_ten(1, -2, 3, -4, 5, -6, WIDE_TOP, 7, low, top)
             != 1 - 20 + 300 + 4 + 5 - 6 + 15 + 7 - 128000 + 255;
    return wrong;
}
"#;

/// Darwin: an EvoCC caller and an Apple clang callee, then the reverse, at
/// -O0 and -O2.
#[cfg(target_os = "macos")]
#[test]
fn darwin_calls_cross_with_apple_clang_in_both_directions() {
    let work = Work::new("darwin-cross");
    work.source("cross.h", CROSS_HEADER);
    let callee = work.source("callee.c", CROSS_CALLEE);
    let caller = work.source("caller.c", CROSS_CALLER);
    for level in ["-O0", "-O2"] {
        for (evocc_part, clang_part) in [(&caller, &callee), (&callee, &caller)] {
            let evocc_object = work.0.join(format!("evocc{level}.o"));
            let output = evocc(
                evocc_part,
                DARWIN,
                &[level, "-c", "-o", evocc_object.to_str().unwrap()],
            );
            assert!(output.status.success(), "{level}\n{}", text(&output));
            let program = work.0.join(format!("cross{level}"));
            let output = Command::new("clang")
                .args(["-std=gnu11", "-w", level])
                .arg(clang_part)
                .arg(&evocc_object)
                .arg("-o")
                .arg(&program)
                .output()
                .unwrap();
            assert!(output.status.success(), "{level}\n{}", text(&output));
            let status = Command::new(&program).status().unwrap();
            assert_eq!(
                status.code(),
                Some(0),
                "{level} EvoCC {}",
                evocc_part.display()
            );
        }
    }
}

/// Linux: the same crossed calls against GCC 14 in the sealed image.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_calls_cross_with_gcc_14_in_both_directions() {
    let work = Work::new("linux-cross");
    work.source("cross.h", CROSS_HEADER);
    let mut script = String::from("set -u\ncd /work\nfailed=0\n");
    let mut lines = 0;
    for (name, source) in [("callee", CROSS_CALLEE), ("caller", CROSS_CALLER)] {
        let path = work.source(&format!("{name}.c"), source);
        for level in ["-O0", "-O2"] {
            let assembly = work.0.join(format!("{name}{level}.s"));
            let output = evocc(
                &path,
                LINUX,
                &[level, "-S", "-o", assembly.to_str().unwrap()],
            );
            assert!(output.status.success(), "{name} {level}\n{}", text(&output));
        }
    }
    for level in ["-O0", "-O2"] {
        script.push_str(&format!(
            "gcc-14 -std=gnu11 -w {level} caller.c callee.c -o gcc{level} && ./gcc{level}; s=$?; echo \"gcc-14 {level} $s\"; [ $s -eq 0 ] || failed=1\n\
             gcc-14 -std=gnu11 -w {level} caller{level}.s callee.c -o evocc-caller{level} && ./evocc-caller{level}; s=$?; echo \"evocc caller {level} $s\"; [ $s -eq 0 ] || failed=1\n\
             gcc-14 -std=gnu11 -w {level} caller.c callee{level}.s -o evocc-callee{level} && ./evocc-callee{level}; s=$?; echo \"evocc callee {level} $s\"; [ $s -eq 0 ] || failed=1\n"
        ));
        lines += 3;
    }
    script.push_str("exit $failed\n");
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
        .arg(format!("{}:/work", work.0.display()))
        .args(["evo-nexlab:sealed", "sh", "-c", &script])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}");
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(
        stdout.lines().filter(|line| line.ends_with(" 0")).count(),
        lines,
        "{stdout}"
    );
}
