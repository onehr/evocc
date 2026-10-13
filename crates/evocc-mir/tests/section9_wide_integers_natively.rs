//! Section 9, instrument three, for the `bits<128>` forms of Section 5 and the
//! "128-bit arguments and results" of Section 6.
//!
//! Each function below is EvoIR the whole backend compiles, assembles and
//! links against a host-compiled driver that calls it and compares the answer
//! with the host compiler's own `unsigned __int128` arithmetic. That is what
//! makes the register pair, the carry between the halves, the branch-free
//! shifts, the borrow-aware comparison and the even-aligned argument pair
//! observable rather than asserted: a disagreement is a wrong exit status.
//!
//! The sources are EvoIR to isolate register-pair lowering from C parsing and
//! promotion. Division and remainder use the hosted runtime's pair ABI;
//! the same native oracles serve both front-end and machine tests.

use std::process::Command;

use evocc_evoir::parse_section9_module;
use evocc_mir::{compile_module_for, Target, Toolchain};

/// A function over `param_tys` returning `results`, with `body` between.
fn func(name: &str, param_tys: &[&str], results: &str, body: &str, ret: &str) -> String {
    let entry: Vec<String> = param_tys
        .iter()
        .enumerate()
        .map(|(index, ty)| format!("%v{index}: {ty}"))
        .collect();
    let params: Vec<String> = param_tys
        .iter()
        .enumerate()
        .map(|(index, ty)| format!("%arg{index}: {ty}"))
        .collect();
    let memory = param_tys.len();
    let world = memory + 1;
    let head = params.join(", ");
    let separator = if head.is_empty() { "" } else { ", " };
    let entry = entry.join(", ");
    let entry_separator = if entry.is_empty() { "" } else { ", " };
    // Section 9.4 indents every instruction by four spaces; the bodies below
    // are written for reading, so the indentation is applied here once.
    let body: String = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| format!("    {line}\n"))
        .collect();
    let body = body.as_str();
    format!(
        "func @{name}({head}{separator}%arg{memory}: mem<all>, %arg{world}: world) \
-> ({results}, mem<all>, world) cc = target_default\n    \
effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, \
may_trap=false, may_unwind=false }}\n{{\n\
^entry({entry}{entry_separator}%v90: mem<all>, %v91: world):\n{body}    \
ret {ret}, %v90, %v91\n}}\n"
    )
}

/// The whole module the probe compiles.
fn module_text() -> String {
    let mut text = String::new();

    // `(u64)(((unsigned __int128)a * b) >> s)`: the kernel cohort's entire use
    // of the width, with the shift a runtime value so the branch-free form is
    // the one that runs.
    text.push_str(&func(
        "wide_mul_shr",
        &["bits<64>", "bits<64>", "bits<64>"],
        "bits<64>",
        "    %v3 = zext.bits<128> %v0\n\
         \x20   %v4 = zext.bits<128> %v1\n\
         \x20   %v5 = mul.bits<128> %v3, %v4\n\
         \x20   %v6 = zext.bits<128> %v2\n\
         \x20   %v7 = lshr.bits<128> %v5, %v6\n\
         \x20   %v8 = trunc.bits<64> %v7\n",
        "%v8",
    ));
    text.push('\n');

    // The pair crosses the boundary in both directions: two `bits<128>`
    // arguments in `x0`:`x1` and `x2`:`x3`, one `bits<128>` result in
    // `x0`:`x1`.
    for (name, operation) in [
        ("wide_add", "add"),
        ("wide_sub", "sub"),
        ("wide_mul", "mul"),
    ] {
        text.push_str(&func(
            name,
            &["bits<128>", "bits<128>"],
            "bits<128>",
            &format!("    %v2 = {operation}.bits<128> %v0, %v1\n"),
            "%v2",
        ));
        text.push('\n');
    }
    for (name, operation) in [("wide_and", "and"), ("wide_or", "or"), ("wide_xor", "xor")] {
        text.push_str(&func(
            name,
            &["bits<128>", "bits<128>"],
            "bits<128>",
            &format!("    %v2 = {operation}.bits<128> %v0, %v1\n"),
            "%v2",
        ));
        text.push('\n');
    }
    for (name, operation) in [
        ("wide_sdiv", "sdiv"),
        ("wide_udiv", "udiv"),
        ("wide_srem", "srem"),
        ("wide_urem", "urem"),
    ] {
        text.push_str(
            &func(
                name,
                &["bits<128>", "bits<128>"],
                "bits<128>",
                &format!("%v2 = {operation}.bits<128> %v0, %v1\n"),
                "%v2",
            )
            .replace("may_trap=false", "may_trap=true"),
        );
        text.push('\n');
    }

    // The three variable shifts, the amount widened from the `unsigned long`
    // the host passes.
    for (name, operation) in [
        ("wide_shl", "shl"),
        ("wide_lshr", "lshr"),
        ("wide_ashr", "ashr"),
    ] {
        text.push_str(&func(
            name,
            &["bits<128>", "bits<64>"],
            "bits<128>",
            &format!(
                "    %v2 = zext.bits<128> %v1\n\
                 \x20   %v3 = {operation}.bits<128> %v0, %v2\n"
            ),
            "%v3",
        ));
        text.push('\n');
    }

    // Every comparison, answered as the `int` the host reads.
    for (name, operation) in [
        ("wide_eq", "eq"),
        ("wide_ne", "ne"),
        ("wide_ult", "ult"),
        ("wide_ule", "ule"),
        ("wide_slt", "slt"),
        ("wide_sle", "sle"),
    ] {
        text.push_str(&func(
            name,
            &["bits<128>", "bits<128>"],
            "bits<32>",
            &format!(
                "    %v2 = icmp.{operation} %v0, %v1\n\
                 \x20   %v3 = zext.bits<32> %v2\n"
            ),
            "%v3",
        ));
        text.push('\n');
    }

    // The two width changes, and the result register pair.
    text.push_str(&func(
        "wide_from_unsigned",
        &["bits<64>"],
        "bits<128>",
        "    %v1 = zext.bits<128> %v0\n",
        "%v1",
    ));
    text.push('\n');
    text.push_str(&func(
        "wide_from_signed",
        &["bits<64>"],
        "bits<128>",
        "    %v1 = sext.bits<128> %v0\n",
        "%v1",
    ));
    text.push('\n');

    // Section 6's even-aligned pair: after one 64-bit argument the pair is
    // `x2`:`x3` with `x1` skipped, after two it is `x2`:`x3` with nothing
    // skipped. Both halves are read, so a wrong register is a wrong answer.
    text.push_str(&func(
        "wide_after_one",
        &["bits<64>", "bits<128>"],
        "bits<64>",
        "    %v2 = trunc.bits<64> %v1\n\
         \x20   %v3 = const.bits<128> 64\n\
         \x20   %v4 = lshr.bits<128> %v1, %v3\n\
         \x20   %v5 = trunc.bits<64> %v4\n\
         \x20   %v6 = add.bits<64> %v2, %v5\n\
         \x20   %v7 = add.bits<64> %v6, %v0\n",
        "%v7",
    ));
    text.push('\n');
    text.push_str(&func(
        "wide_after_two",
        &["bits<64>", "bits<64>", "bits<128>"],
        "bits<64>",
        "    %v3 = trunc.bits<64> %v2\n\
         \x20   %v4 = const.bits<128> 64\n\
         \x20   %v5 = lshr.bits<128> %v2, %v4\n\
         \x20   %v6 = trunc.bits<64> %v5\n\
         \x20   %v7 = add.bits<64> %v3, %v6\n\
         \x20   %v8 = add.bits<64> %v7, %v0\n\
         \x20   %v9 = add.bits<64> %v8, %v1\n",
        "%v9",
    ));
    text.push('\n');

    // A union of one `unsigned __int128` and two `unsigned long` halves,
    // stored through a pointer, loaded back and returned: the `stp` and `ldp`
    // of Section 5 against the layout the host compiler gives the union.
    // A union of one `unsigned __int128` and two `unsigned long` halves,
    // stored through a pointer, loaded back and returned: the `stp` and `ldp`
    // of Section 5 against the layout the host compiler gives the union.
    let memory = [
        "func @wide_through_memory(%arg0: ptr<generic>, %arg1: bits<128>, %arg2: mem<all>, %arg3: world) -> (bits<128>, mem<all>, world) cc = target_default",
        "    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }",
        "{",
        "^entry(%v0: ptr<generic>, %v1: bits<128>, %v90: mem<all>, %v91: world):",
        "    %v2 = access_in_bounds %v90, %v0 [size=16, align=16]",
        "    (%v3, %v4) = store %v2, %v91, %v0, %v1",
        "    %v5 = access_in_bounds %v3, %v0 [size=16, align=16]",
        "    (%v6, %v7, %v8) = load %v5, %v4, %v0 : bits<128>",
        "    ret %v6, %v7, %v8",
        "}",
    ];
    for line in memory {
        text.push_str(line);
        text.push('\n');
    }
    text
}

/// The host driver: it calls every function above and compares the answer with
/// its own compiler's `unsigned __int128` arithmetic.
const DRIVER: &str = r##"
#include <stdio.h>
typedef unsigned __int128 u128;
typedef __int128 s128;
typedef unsigned long u64;

u64 wide_mul_shr(u64 a, u64 b, u64 s);
u128 wide_add(u128 a, u128 b);
u128 wide_sub(u128 a, u128 b);
u128 wide_mul(u128 a, u128 b);
u128 wide_udiv(u128 a, u128 b);
u128 wide_urem(u128 a, u128 b);
s128 wide_sdiv(s128 a, s128 b);
s128 wide_srem(s128 a, s128 b);
u128 wide_and(u128 a, u128 b);
u128 wide_or(u128 a, u128 b);
u128 wide_xor(u128 a, u128 b);
u128 wide_shl(u128 a, u64 s);
u128 wide_lshr(u128 a, u64 s);
s128 wide_ashr(s128 a, u64 s);
int wide_eq(u128 a, u128 b);
int wide_ne(u128 a, u128 b);
int wide_ult(u128 a, u128 b);
int wide_ule(u128 a, u128 b);
int wide_slt(s128 a, s128 b);
int wide_sle(s128 a, s128 b);
u128 wide_from_unsigned(u64 a);
s128 wide_from_signed(long a);
u64 wide_after_one(u64 a, u128 b);
u64 wide_after_two(u64 a, u64 b, u128 c);
u128 wide_through_memory(u128 *p, u128 v);

union halves { u128 full; struct { u64 lo, hi; }; };

static int failures = 0;
static int checks = 0;

static void expect(const char *what, u128 got, u128 want) {
    checks++;
    if (got != want) {
        failures++;
        printf("MISMATCH %s: got %lx:%lx want %lx:%lx\n", what,
               (u64)(got >> 64), (u64)got, (u64)(want >> 64), (u64)want);
    }
}

static const u64 values[] = {
    0, 1, 2, 0x7fffffffffffffffUL, 0x8000000000000000UL,
    0xfffffffffffffffeUL, 0xffffffffffffffffUL, 0x123456789abcdef0UL,
    0xfedcba9876543210UL,
};
#define N ((int)(sizeof values / sizeof values[0]))

#ifndef ROUNDED_PAIR
static void check_rounded_pair(void) {}
#endif
#ifndef STACK_ARGUMENTS
static void check_stack_arguments(void) {}
#endif

int main(void) {
    /* The reference values EvoIR 10.1.19 states, one by one. */
    expect("10.1.19 mul_shr 64", wide_mul_shr(~0UL, ~0UL - 1, 64), 0xfffffffffffffffdUL);
    expect("10.1.19 mul_shr 37",
           wide_mul_shr(0x123456789abcdef0UL, 0xfedcba9876543210UL, 37),
           0x56bbeba1111b6c47UL);
    expect("10.1.19 shl 100 high", wide_shl(1, 100) >> 64, 0x1000000000UL);
    expect("10.1.19 add carry", wide_add(((u128)1 << 64) | ~0UL, 1), (u128)2 << 64);
    expect("10.1.19 lt high", wide_ult((u128)1 << 64, ~(u128)0 >> 64), 0);
    expect("10.1.19 lt low", wide_ult(1, (u128)1 << 64), 1);
    expect("10.1.19 earlier shr", (((u128)1 << 100) + 7) >> 64, 68719476736UL);
    expect("10.1.19 earlier mul", wide_lshr(wide_mul(((u128)1 << 100) + 7, 3), 100), 3);

    /* And then the whole cross product against the host's own arithmetic. */
    for (int i = 0; i < N; i++) for (int j = 0; j < N; j++)
    for (int k = 0; k < N; k++) for (int l = 0; l < N; l++) {
        u128 a = ((u128)values[j] << 64) | values[i];
        u128 b = ((u128)values[l] << 64) | values[k];
        s128 sa = (s128)a, sb = (s128)b;
        expect("add", wide_add(a, b), a + b);
        expect("sub", wide_sub(a, b), a - b);
        expect("mul", wide_mul(a, b), a * b);
        if (b) {
            expect("udiv", wide_udiv(a, b), a / b);
            expect("urem", wide_urem(a, b), a % b);
            if (!(a == (u128)1 << 127 && sb == -1)) {
                expect("sdiv", wide_sdiv(sa, sb), sa / sb);
                expect("srem", wide_srem(sa, sb), sa % sb);
            }
        }
        expect("and", wide_and(a, b), a & b);
        expect("or", wide_or(a, b), a | b);
        expect("xor", wide_xor(a, b), a ^ b);
        expect("eq", wide_eq(a, b), a == b);
        expect("ne", wide_ne(a, b), a != b);
        expect("ult", wide_ult(a, b), a < b);
        expect("ule", wide_ule(a, b), a <= b);
        expect("slt", wide_slt(sa, sb), sa < sb);
        expect("sle", wide_sle(sa, sb), sa <= sb);
    }

    for (int i = 0; i < N; i++) for (int j = 0; j < N; j++) for (u64 s = 0; s < 128; s++) {
        u128 a = ((u128)values[j] << 64) | values[i];
        s128 sa = (s128)a;
        expect("shl", wide_shl(a, s), a << s);
        expect("lshr", wide_lshr(a, s), a >> s);
        expect("ashr", wide_ashr(sa, s), (u128)(sa >> s));
        expect("mul_shr", wide_mul_shr(values[i], values[j], s),
               (u64)(((u128)values[i] * values[j]) >> s));
    }

    for (int i = 0; i < N; i++) {
        expect("zext", wide_from_unsigned(values[i]), (u128)values[i]);
        expect("sext", wide_from_signed((long)values[i]), (u128)(s128)(long)values[i]);
        u128 a = ((u128)values[i] << 64) | 0x5a5a5a5a5a5a5a5aUL;
        /* Two 64-bit arguments already leave the next register even, so this
           shape does not turn on the rounding rule. */
        expect("after two", wide_after_two(values[i], 7, a),
               values[i] + 7 + (u64)a + (u64)(a >> 64));
    }

    check_rounded_pair();
    check_stack_arguments();
    printf("checks=%d failures=%d\n", checks, failures);
    return failures != 0;
}
"##;

/// The Darwin lane: the host compiler links and runs the probe.
#[test]
fn the_wide_integer_forms_agree_with_the_host_compiler() {
    let toolchain = match Toolchain::locate() {
        Ok(toolchain) => toolchain,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    println!("toolchain={}", toolchain.version());

    let text = module_text();
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    evocc_evoir::validate_module(&module).expect("the probe module validates");
    let assembly = compile_module_for(Target::Darwin, &module, &|_| true)
        .expect("every wide-integer function lowers, allocates, verifies and emits");

    let directory = std::env::temp_dir().join(format!("evocc-wide-probe-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a working directory");
    let assembly_path = directory.join("wide.s");
    let driver_path = directory.join("wide-driver.c");
    let program = directory.join("wide");
    std::fs::write(&assembly_path, &assembly).expect("the emitted text");
    std::fs::write(&driver_path, DRIVER).expect("the host driver");

    let built = Command::new("clang")
        .arg("-O2")
        .arg("-o")
        .arg(&program)
        .arg(&driver_path)
        .arg(&assembly_path)
        .output()
        .expect("run clang");
    assert!(
        built.status.success(),
        "the toolchain refused the probe:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&program).output().expect("run the probe");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    print!("{stdout}");
    assert_eq!(
        run.status.code(),
        Some(0),
        "the probe reported a disagreement:\n{stdout}"
    );
    assert!(
        stdout.contains("failures=0"),
        "the probe did not report a clean run:\n{stdout}"
    );

    let _ = std::fs::remove_dir_all(&directory);
}

/// The functions only the Linux target lowers: Section 6's stack arguments.
///
/// "On Darwin the platform packs stack arguments by natural alignment
/// instead; that rule is still unmeasured, so more than eight integer
/// arguments stay `E4010` there."
fn stack_argument_text() -> String {
    let mut text = String::new();

    // Nine arguments, the ninth a `char` the front end has promoted to `int`
    // and which therefore occupies the low bytes of its slot.
    let mut body = String::from("    %v9 = sext.bits<64> %v8\n");
    body.push_str("    %v10 = add.bits<64> %v0, %v9\n");
    let mut last = 10;
    for index in 1..8 {
        body.push_str(&format!(
            "    %v{} = add.bits<64> %v{last}, %v{index}\n",
            last + 1
        ));
        last += 1;
    }
    text.push_str(&func(
        "wide_nine",
        &[
            "bits<64>", "bits<64>", "bits<64>", "bits<64>", "bits<64>", "bits<64>", "bits<64>",
            "bits<64>", "bits<32>",
        ],
        "bits<64>",
        &body,
        &format!("%v{last}"),
    ));
    text.push('\n');

    // Ten, so that a second slot at the next eight bytes is read too.
    let mut body = String::from("    %v10 = sext.bits<64> %v8\n");
    body.push_str("    %v11 = add.bits<64> %v0, %v10\n");
    let mut last = 11;
    for index in 1..8 {
        body.push_str(&format!(
            "    %v{} = add.bits<64> %v{last}, %v{index}\n",
            last + 1
        ));
        last += 1;
    }
    body.push_str(&format!(
        "    %v{} = add.bits<64> %v{last}, %v9\n",
        last + 1
    ));
    last += 1;
    text.push_str(&func(
        "wide_ten",
        &[
            "bits<64>", "bits<64>", "bits<64>", "bits<64>", "bits<64>", "bits<64>", "bits<64>",
            "bits<64>", "bits<32>", "bits<64>",
        ],
        "bits<64>",
        &body,
        &format!("%v{last}"),
    ));
    text.push('\n');

    // "after seven it goes to the stack, whole, 16-aligned, with `x7` unused".
    text.push_str(&func(
        "wide_after_seven",
        &[
            "bits<64>",
            "bits<64>",
            "bits<64>",
            "bits<64>",
            "bits<64>",
            "bits<64>",
            "bits<64>",
            "bits<128>",
        ],
        "bits<64>",
        "    %v8 = trunc.bits<64> %v7\n\
         \x20   %v9 = const.bits<128> 64\n\
         \x20   %v10 = lshr.bits<128> %v7, %v9\n\
         \x20   %v11 = trunc.bits<64> %v10\n\
         \x20   %v12 = add.bits<64> %v8, %v11\n\
         \x20   %v13 = add.bits<64> %v12, %v0\n\
         \x20   %v14 = add.bits<64> %v13, %v1\n\
         \x20   %v15 = add.bits<64> %v14, %v2\n\
         \x20   %v16 = add.bits<64> %v15, %v3\n\
         \x20   %v17 = add.bits<64> %v16, %v4\n\
         \x20   %v18 = add.bits<64> %v17, %v5\n\
         \x20   %v19 = add.bits<64> %v18, %v6\n",
        "%v19",
    ));
    text
}

/// The checks only the target whose rounding rule is measured can run.
///
/// EvoMIR Section 6 states the even-aligned pair for both targets and cites
/// `p35-int128.c`, whose recorded run measured the sealed container's oracles
/// alone. Apple clang does not round (evidence
/// `.evidence/builtin-probes/20260905-b6/b6-darwin-abi.out`), so these shapes
/// run on the Linux lane, where GCC-14 and Clang-19 both do.
const ROUNDED_PAIR_DRIVER: &str = r#"
#define ROUNDED_PAIR 1
static void check_rounded_pair(void) {
    for (int i = 0; i < N; i++) {
        u128 a = ((u128)values[i] << 64) | 0x5a5a5a5a5a5a5a5aUL;
        expect("after one", wide_after_one(values[i], a),
               values[i] + (u64)a + (u64)(a >> 64));

        /* The union: stored as one 128-bit object, read back as two halves. */
        union halves u;
        u128 back = wide_through_memory(&u.full, a);
        expect("through memory", back, a);
        expect("union low", u.lo, (u64)a);
        expect("union high", u.hi, (u64)(a >> 64));
    }
}
"#;

/// The extra driver the stack-argument functions need.
const STACK_DRIVER: &str = r#"
#define STACK_ARGUMENTS 1
long wide_nine(long a, long b, long c, long d, long e, long f, long g, long h, int i);
long wide_ten(long a, long b, long c, long d, long e, long f, long g, long h, int i, long j);
unsigned long wide_after_seven(long a, long b, long c, long d, long e, long f, long g,
                               unsigned __int128 h);

static void check_stack_arguments(void) {
    expect("nine", (u128)(u64)wide_nine(1, 2, 3, 4, 5, 6, 7, 8, (char)-9), (u128)(u64)(long)283);
    expect("ten", (u128)(u64)wide_ten(1, 2, 3, 4, 5, 6, 7, 8, (char)-9, 10), (u128)(u64)(long)293);
    u128 h = ((u128)11 << 64) | 12;
    expect("after seven", (u128)wide_after_seven(1, 2, 3, 4, 5, 6, 7, h), (u128)(1+2+3+4+5+6+7+11+12));
}
"#;

/// The Linux lane: GCC-14 inside the sealed container links and runs the same
/// probe, with the stack-argument functions the Linux convention lowers.
#[test]
fn the_wide_integer_forms_agree_with_gcc_in_the_sealed_container() {
    const CONTAINER_BIN: &str = "/usr/local/bin/container";
    const IMAGE: &str = "evo-nexlab:sealed";

    if !std::path::Path::new(CONTAINER_BIN).exists() {
        println!("skipped={CONTAINER_BIN} is absent");
        return;
    }
    let listed = match Command::new(CONTAINER_BIN).args(["image", "list"]).output() {
        Ok(listed) if listed.status.success() => listed,
        _ => {
            println!("skipped=the container runtime is not started");
            return;
        }
    };
    if !String::from_utf8_lossy(&listed.stdout).contains("evo-nexlab") {
        println!("skipped={IMAGE} is not present");
        return;
    }

    let text = format!("{}\n{}", module_text(), stack_argument_text());
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    evocc_evoir::validate_module(&module).expect("the probe module validates");
    let assembly = compile_module_for(Target::Linux, &module, &|_| true)
        .expect("every wide-integer function lowers for Linux");

    let work = std::env::temp_dir().join(format!("evocc-wide-linux-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("a working directory");
    std::fs::write(work.join("wide.s"), &assembly).expect("the emitted text");
    let driver = DRIVER.replace(
        "#ifndef ROUNDED_PAIR",
        &format!("{ROUNDED_PAIR_DRIVER}\n{STACK_DRIVER}\n#ifndef ROUNDED_PAIR"),
    );
    std::fs::write(work.join("wide-driver.c"), driver).expect("the host driver");
    std::fs::write(
        work.join("runner.sh"),
        "#!/bin/sh\ncd /work || exit 2\ngcc-14 -O2 -o /work/wide /work/wide-driver.c /work/wide.s 2>/work/build.err || { echo \"build $(head -c 400 /work/build.err | tr '\\n' ' ')\"; exit 0; }\n/work/wide\necho \"status $?\"\n",
    )
    .expect("the runner");
    let mut permissions = std::fs::metadata(work.join("runner.sh"))
        .expect("the runner exists")
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        permissions.set_mode(0o755);
    }
    std::fs::set_permissions(work.join("runner.sh"), permissions).expect("the runner is runnable");

    let output = Command::new(CONTAINER_BIN)
        .arg("run")
        .arg("--rm")
        .arg("--platform")
        .arg("linux/arm64")
        .arg("--network")
        .arg("none")
        .arg("--volume")
        .arg(format!("{}:/work", work.display()))
        .arg(IMAGE)
        .arg("/work/runner.sh")
        .output()
        .expect("run the sealed container");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    print!("{stdout}");
    assert!(
        !stdout.contains("build "),
        "GCC-14 refused the probe:\n{stdout}"
    );
    assert!(
        stdout.contains("failures=0") && stdout.contains("status 0"),
        "the Linux lane reported a disagreement:\n{stdout}"
    );

    let _ = std::fs::remove_dir_all(&work);
}

/// Darwin keeps refusing more than eight register arguments.
#[test]
fn darwin_still_refuses_more_than_eight_arguments() {
    // Section 10: `E4010` is "more than eight integer arguments on Darwin
    // (Linux lowers stack arguments, revision 0.10)".
    let text = stack_argument_text();
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let refusals = evocc_mir::lower_module(&module)
        .expect_err("Darwin has no measured rule for a ninth argument");
    let mut codes: Vec<&str> = refusals.iter().map(|refusal| refusal.code).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes, vec!["E4010"], "{refusals:?}");
}
