//! Stable Darwin and Linux ABI placement snapshots.
//!
//! Each C shape is lowered once by the unchanged frontend and then compiled by
//! the backend for both AArch64 profiles. Every entry is the SHA-256 of the
//! complete assembly, or the exact refusal, recorded before the argument,
//! result, variadic, frame and profile facts moved into target descriptions.
//! The comparison restores only the historical Linux producer version; the
//! driver tests separately require the emitted version to match this release.
//! This is an assembly regression snapshot, not an ABI oracle.
//! The October 15 diagnostic repair retains complete lowering records and
//! reports profile refusals as E4009. The October 16 constant-address selection
//! changes seven assembly entries in four record shapes. A replay of the
//! retained compiler reproduces their old hashes; the reviewed diffs replace
//! constant offset preparation only, including a zero-offset alias. Argument,
//! result and frame placement, calls, guards and refusal records are unchanged.
//! Live-range holes then change four variadic entries: temporary registers and
//! identity moves differ, but ABI save areas, frame offsets and guards do not.
//! The old hull policy reproduces all four prior hashes on the same EvoIR.
//! Literal conversion selection changes eighteen entries: literal extensions
//! become constants, which also allows three stores to use an immediate
//! address. Exact prior assembly hashes and reviewed diffs are retained.
//! Calls, argument positions, frame layouts and checked branches do not change.
//! Zero-upper-bit selection changes only the two dynamic-stack entries: the
//! mask after Cset becomes a W-to-X copy. The complete before/after comparison
//! has no other assembly difference, and all preceding hashes reproduce.

use std::io::Write;
use std::process::{Command, Stdio};

use evocc_mir::emit::{CodegenOptions, StackGuard};
use evocc_mir::{compile_module_with_options, RelocationModel, Target};

struct Shape {
    name: &'static str,
    flags: &'static [&'static str],
    options: fn() -> CodegenOptions,
    source: &'static str,
}

fn plain() -> CodegenOptions {
    CodegenOptions::default()
}

fn system_guard() -> CodegenOptions {
    CodegenOptions {
        stack_guard: StackGuard::SystemRegister {
            register: "sp_el0".into(),
            offset: 64,
        },
        ..CodegenOptions::default()
    }
}

fn far_system_guard() -> CodegenOptions {
    CodegenOptions {
        stack_guard: StackGuard::SystemRegister {
            register: "sp_el0".into(),
            offset: -40_000,
        },
        ..CodegenOptions::default()
    }
}

fn general_only() -> CodegenOptions {
    CodegenOptions {
        general_registers_only: true,
        ..CodegenOptions::default()
    }
}

fn unwound_signed() -> CodegenOptions {
    CodegenOptions {
        unwind_tables: true,
        sign_returns: true,
        strict_alignment: true,
        ..CodegenOptions::default()
    }
}

fn sectioned() -> CodegenOptions {
    CodegenOptions {
        function_sections: true,
        data_sections: true,
        ..CodegenOptions::default()
    }
}

const SHAPES: &[Shape] = &[
    Shape {
        name: "integer_stack_arguments",
        flags: &[],
        options: plain,
        source: "extern long sink10(long, long, long, long, long, long, long, long, long, long);
long take10(long a0, long a1, long a2, long a3, long a4, long a5, long a6, long a7, long a8, long a9)
{ return a0 + a1 + a7 + a8 * 3 + a9 * 5; }
long give10(long x) { return sink10(x, 1, 2, 3, 4, 5, 6, 7, x + 8, x + 9); }",
    },
    Shape {
        name: "eight_integer_arguments",
        flags: &[],
        options: plain,
        source: "extern int sink8(int, long, int, long, int, long, int, long);
long take8(int a0, long a1, int a2, long a3, int a4, long a5, int a6, long a7)
{ return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7; }
int give8(int x) { return sink8(x, 1, 2, 3, 4, 5, 6, x + 7) + 1; }",
    },
    Shape {
        name: "mixed_register_banks",
        flags: &[],
        options: plain,
        source: "extern double fsink(double, float, int, double, float, long, double, double, double, double, int, double, float);
double mixed(double a, float b, int c, double d, float e, long f, double g, double h, double i, double j, int k, double l, float m)
{ return a + b + c + d + e + f + g + h + i + j + k + l + m; }
double call_mixed(double x) { return fsink(x, 1.0f, 2, x, 3.0f, 4, x, x, x, x, 5, x * 2, 6.0f); }",
    },
    Shape {
        name: "homogeneous_floating_records",
        flags: &[],
        options: plain,
        source: "struct h3 { float a, b, c; };
struct h4 { double a, b, c, d; };
extern struct h4 hsink(struct h3, struct h4, struct h4, double);
struct h4 hfa(struct h3 x, struct h4 y, struct h4 z, double w)
{ struct h4 r = { x.a + y.a, x.b + z.b, x.c + w, y.d + z.d }; return r; }
double call_hfa(struct h3 x, struct h4 y) { struct h4 r = hsink(x, y, y, 1.0); return r.a + r.d; }",
    },
    Shape {
        name: "small_records",
        flags: &[],
        options: plain,
        source: "struct s12 { int a, b, c; };
struct a16 { _Alignas(16) long a; long b; };
extern long rsink(int, struct a16, struct s12, int);
long records(int p, struct a16 q, struct s12 r, int t) { return p + q.a + q.b + r.c + t; }
long call_records(struct s12 r) { struct a16 q = { 1, 2 }; return rsink(3, q, r, 4); }
struct s12 make12(int x) { struct s12 r = { x, x + 1, x + 2 }; return r; }",
    },
    Shape {
        name: "record_register_exhaustion",
        flags: &[],
        options: plain,
        source: "struct s16 { long a, b; };
struct a16 { _Alignas(16) long a; long b; };
extern long xsink(int, int, int, int, int, int, int, struct s16, struct a16, long);
long xtake(int a, int b, int c, int d, int e, int f, int g, struct s16 s, struct a16 q, long z)
{ return a + g + s.a + s.b + q.b + z; }
long xcall(struct s16 s) { struct a16 q = { 5, 6 }; return xsink(1, 2, 3, 4, 5, 6, 7, s, q, 8); }",
    },
    Shape {
        name: "large_records",
        flags: &[],
        options: plain,
        source: "struct big { long a[5]; };
extern struct big bsink(struct big, long);
struct big bigpass(struct big b, long n) { b.a[0] += n; return b; }
long call_big(struct big b) { struct big r = bsink(b, 7); return r.a[4]; }",
    },
    Shape {
        name: "integer_pairs",
        flags: &[],
        options: plain,
        source: "extern __int128 isink(int, __int128, long, __int128);
__int128 pairs(int a, __int128 b, long c, __int128 d) { return b + d + a + c; }
__int128 call_pairs(__int128 x) { return isink(1, x, 2, x); }",
    },
    Shape {
        name: "variadic_callee",
        flags: &[],
        options: plain,
        source: "struct pt { long x, y; };
struct hp { double a, b; };
long vsum(int n, ...)
{
    __builtin_va_list ap;
    __builtin_va_start(ap, n);
    long s = 0;
    for (int i = 0; i < n; i++) {
        s += __builtin_va_arg(ap, long);
        s += (long)__builtin_va_arg(ap, double);
        struct pt p = __builtin_va_arg(ap, struct pt);
        struct hp h = __builtin_va_arg(ap, struct hp);
        s += p.y + (long)h.b;
    }
    __builtin_va_end(ap);
    return s;
}
long vnamed(int a, double b, long c, ...)
{ __builtin_va_list ap; __builtin_va_start(ap, c); long s = __builtin_va_arg(ap, long); __builtin_va_end(ap); return a + (long)b + c + s; }",
    },
    Shape {
        name: "variadic_callee_general_only",
        flags: &[],
        options: general_only,
        source: "long vfirst(int n, ...)
{ __builtin_va_list ap; __builtin_va_start(ap, n); long x = __builtin_va_arg(ap, long); __builtin_va_end(ap); return x + n; }",
    },
    Shape {
        name: "variadic_caller",
        flags: &[],
        options: plain,
        source: "struct pt { long x, y; };
extern long vsink(int, ...);
long vcall(struct pt p) { return vsink(3, 1L, 2.0, p, 4L, 5.0, p); }
long vcall_long(long x) { return vsink(9, x, 1.5, x + 1, 2.5, x + 2, 3.5, x + 3, 4.5, x + 4, 5.5, x + 5); }",
    },
    Shape {
        name: "frame_objects_and_spills",
        flags: &[],
        options: plain,
        source: "extern void use(void *, long);
extern long work(long);
long frame(long n)
{
    char buf[40];
    long arr[7];
    long a = work(n), b = work(a), c = work(b), d = work(c), e = work(d), f = work(e);
    long g = work(f), h = work(g), i = work(h), j = work(i), k = work(j), l = work(k);
    use(buf, a);
    use(arr, b);
    return a + b + c + d + e + f + g + h + i + j + k + l;
}",
    },
    Shape {
        name: "frame_options",
        flags: &[],
        options: unwound_signed,
        source: "extern void use(void *, long);
extern long work(long);
long framed(long n) { long arr[3]; long a = work(n), b = work(a); use(arr, a); return a + b; }",
    },
    Shape {
        name: "dynamic_stack",
        flags: &[],
        options: plain,
        source: "extern void use(void *, long);
long dyn(long n) { char vla[n]; char *p = __builtin_alloca(n * 2); use(vla, n); use(p, n); return n; }",
    },
    Shape {
        name: "over_aligned_frame",
        flags: &[],
        options: plain,
        source: "extern void use(void *, long);
long aligned(long n) { _Alignas(64) char buf[64]; char small[3]; use(buf, n); use(small, n); return n; }",
    },
    Shape {
        name: "stack_guard_global",
        flags: &["-fstack-protector-strong"],
        options: plain,
        source: "extern void use(void *, long);
int prot(int c) { char a[16]; use(a, c); if (c) return 1; return 2; }",
    },
    Shape {
        name: "stack_guard_system_register",
        flags: &["-fstack-protector-strong"],
        options: system_guard,
        source: "extern void use(void *, long);
int prot(int c) { char a[16]; use(a, c); if (c) return 1; return 2; }",
    },
    Shape {
        name: "stack_guard_far_system_register",
        flags: &["-fstack-protector-strong"],
        options: far_system_guard,
        source: "extern void use(void *, long);
int prot(int c) { char a[16]; use(a, c); if (c) return 1; return 2; }",
    },
    Shape {
        name: "partially_preserved_vectors",
        flags: &[],
        options: plain,
        source: "extern void use(void *, long);
long vclob(long n) { __asm__ volatile(\"\" ::: \"v8\", \"v9\", \"v15\", \"x19\", \"x20\"); use(0, n); return n; }",
    },
    Shape {
        name: "vector_arguments",
        flags: &[],
        options: plain,
        source: "typedef int v4 __attribute__((vector_size(16)));
extern v4 vsink4(v4, v4, v4, v4, v4, v4, v4, v4, v4, v4);
v4 vtake(v4 a, v4 b, v4 c, v4 d, v4 e, v4 f, v4 g, v4 h, v4 i, v4 j) { return a + b + h + i + j; }
v4 vgive(v4 x) { return vsink4(x, x, x, x, x, x, x, x, x, x + x); }",
    },
    Shape {
        name: "binary128",
        flags: &[],
        options: plain,
        source: "long double ld(long double a, long double b) { return a * b; }",
    },
    Shape {
        name: "patchable_entry",
        flags: &[],
        options: plain,
        source: "__attribute__((patchable_function_entry(2, 1))) int pe(int x) { return x + 1; }",
    },
    Shape {
        name: "elf_sections",
        flags: &[],
        options: sectioned,
        source: "int counter = 3; int bump(int x) { return x + counter; }",
    },
];

/// Started at `6911afd4`; the reviewed updates are listed above.
const RECORDED: &[(&str, &str, &str)] = &[
    (
        "integer_stack_arguments",
        "stop lowering E4010 abi: too-many-register-arguments: @take10 block 0 instruction 0: @take10 takes 10 register arguments\nE4010 abi: too-many-register-arguments: @give10 block 0 instruction 24: a call to @sink10 passes 10 arguments",
        "asm a406d0e1fce5b61ad495a8d428e69df53b7b910bd5180f63dd9121b4e256809f",
    ),
    (
        "eight_integer_arguments",
        "asm d6974ad601e4f09f87d428156150959336314ec37581f6f10e5d4db046180728",
        "asm 5ccc975a2add96110d30bae0e2c760618a86ce5861b007775bca07d7cc7147e0",
    ),
    (
        "mixed_register_banks",
        "asm 7406c7cff5b130141059aebb5a94b6e494e91f2f8263f88e1b1d7d6155b7966d",
        "asm d2058ba43fdf22db6e777dc1936d5c8872d1f3d182a148412c066ac8bf64c5de",
    ),
    (
        "homogeneous_floating_records",
        "asm f6e73ed2074400c8938f1736c37488378a1f00316746c60357fd0463bdd14639",
        "asm 32cc199cf432a42267e229aa8bc839b7b353af352b0cda4d170b89abda3079b8",
    ),
    (
        "small_records",
        "asm 6e21e51dc847ddb7157d6c7630a700d9b09f955087b7b9a730d3eaed174a8a09",
        "asm 786727c0be418f7841f0b5066943187cdeba8918c5415231943e1114e132bedc",
    ),
    (
        "record_register_exhaustion",
        "stop lowering E4010 abi: too-many-register-arguments: @xtake block 0 instruction 0: @xtake takes 13 register arguments\nE4010 abi: too-many-register-arguments: @xcall block 0 instruction 28: a call to @xsink passes 13 arguments",
        "asm a232235b6479cd7da16393819c059a7e41bcb9c5cca61138992112b0277a0f0a",
    ),
    (
        "large_records",
        "asm c92606e223958ae8eaf71cdf34eed35fd02d8c6d752c363f1fb7dc82e0b4b2de",
        "asm 7d542175c8320223eb78ff9e81cc0ae8c6749915276c7f376dec3a2909a4751c",
    ),
    (
        "integer_pairs",
        // D04: the Darwin profile places a `bits<128>` argument in the next
        // two registers, as Apple clang does, with no even-register filler.
        "asm 075eb5e962eb5fb2ad372de1faa8befee4efb7aafe521f1466b426f3ac3c9842",
        "asm 5cd61d1e45226367b66ec2906a1137975224c46c019a1b0fe92da8c429cbd25e",
    ),
    (
        "variadic_callee",
        "asm 7c92e102771fde57e20e976c670f14e9bd16a2053650a3b16fc96581ce78efcb",
        "asm 893af468dd0873f11632fbca0f08a220b1209c5a47b49b8eff5d37314e2d049f",
    ),
    (
        "variadic_callee_general_only",
        "asm a410ef804c70b1b6253300c12768596d11462615571c8650a0f7c431a741d67e",
        "asm b1ba2d1c874b6581b77c59db8cdb2157d3c1cecccb7f355d768e9ad602a98e0e",
    ),
    (
        "variadic_caller",
        "asm d6d9978c113eed8aa5523bd8e3dcf12d2817640853dd11c8ceed48c37f8cb9bb",
        "asm fa8296bd3047b7e2edba83723dedce3baa27131c37cc490791d5b4baaa274028",
    ),
    (
        "frame_objects_and_spills",
        "asm 4eb3e0d65f5b5c3c0af3d227b17df545054c4f195dd34fa4fddbfe5dac68b251",
        "asm 060c97b26abfbc4677c05014b838a0bcc728975fb97eb2b8e5dd98ed23d93e03",
    ),
    (
        "frame_options",
        "asm cff11afa83c4ffc7ce20b2c96556019a0c29b2120236c7539fa80b66a08c465e",
        "asm ac7139b36741d92de5b6b40ab3f12768231db4f4e19f173e783b520a51af916a",
    ),
    (
        "dynamic_stack",
        "asm 3b13aa251fe09e71b24b552d46ab1245ac6d8f02548b88b6b882f15c7ef9d41c",
        "asm 4c68f880e44a5fcf0eb3dfea53ef970c99cf32973acf924f72bf6e284fd9556d",
    ),
    (
        "over_aligned_frame",
        "asm cae2317bfc79b3e41bd6467895247841da1ee360486d756a052ba37226cae4ed",
        "asm 729abc6719a0cca8ee6e8ceea3e40a205bfbca5655876b438010bf56b05d5d0c",
    ),
    (
        "stack_guard_global",
        // Darwin uses Mach-O GOT relocations and its C symbol prefix. These
        // three guard records replace the invalid ELF-spelled Darwin output;
        // every Linux record and every other Darwin record stays unchanged.
        "asm 28d707b4e0546c8ac09dc214cf469e08c215406de9e426b3421a8f5cea97cdb1",
        "asm e4d48e05c513bd2b28d2c43cd4ea4e21b7edc417b472bbc4fa2acdda369149f8",
    ),
    (
        "stack_guard_system_register",
        "asm 12e5b64e2956e64632ca6d7dcac726db100a83718e963ed7d646d06f07371591",
        "asm 2f53622df5ad502a3756e43bbb5dd2a28000e348b8fd5554d8e8e3978fdc67ed",
    ),
    (
        "stack_guard_far_system_register",
        "asm 1dcdbd90709bd8aa443857226aa389e12a2361a2525cb2249572ec1d0fbbdd8d",
        "asm 40c558dbb3d796aa64a0dc8518f946d7a5161501594999c4f71e14a0b4b5d7e1",
    ),
    (
        "partially_preserved_vectors",
        // T03g: the `asm` also clobbers x19 and x20, which both profiles now
        // save and restore beside x21, d8, d9 and d15. No other line changed.
        "asm 15e65ec05261d501fd2c44dbc2bcb19850bbbc90e86f00cfc4142e4d97d6b361",
        "asm 0ed45099153841c744cdf4690da4f4eef8c4b284456dd9b30c03cb5f865b12f8",
    ),
    (
        "vector_arguments",
        "asm a9b148cc3c2967a436bbbff566fedcd06a6373db1909c779bd6f8675c74acf57",
        "asm 234a6f0366eb2dc6beca80d5221257b64c35aa4aa061aa248f9cf218d0a658f9",
    ),
    (
        "binary128",
        "stop lowering E4009 not-lowered: unsupported-form: @ld block 0 instruction 0: binary128 ABI is qualified only for Linux\nE4009 not-lowered: unsupported-form: @ld block 0 instruction 0: binary128 is qualified only for Linux\nE4009 not-lowered: unsupported-form: @ld block 0 instruction 1: binary128 is qualified only for Linux",
        "asm 7038bbfade2ddce9a0386ad65e002413b19f7651d4a62d3188375eb3565f0305",
    ),
    (
        "patchable_entry",
        "stop lowering E4009 not-lowered: unsupported-form: @pe block 0 instruction 0: patchable function entries require the Linux ELF target",
        "asm 7e59255147e04af5f708f90e3cb414794246c5ae9ff8cd5dad6e15c69ba0c6db",
    ),
    (
        "elf_sections",
        "stop lowering E4009 not-lowered: unsupported-form: individual function/data sections require the Linux ELF target",
        "asm b22bfb1790657856807259f2c68a1d1727a6039ccde5b997b290623fc70de429",
    ),
];

fn evoir(shape: &Shape) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg("--emit=evoir")
        .args(shape.flags)
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child
        .stdin
        .take()
        .unwrap()
        .write_all(shape.source.as_bytes());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        shape.name,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn outcome(target: Target, shape: &Shape, ir: &str) -> String {
    let module = evocc_evoir::parse_section9_module(ir.as_bytes()).expect("canonical EvoIR");
    match compile_module_with_options(
        target,
        &module,
        &|_| true,
        RelocationModel::PositionIndependent,
        &(shape.options)(),
    ) {
        Ok(text) => {
            let text = if target == Target::Linux {
                let producer = format!(
                    ".ident \"EvoCC {} aarch64-unknown-linux-gnu\"\n",
                    env!("CARGO_PKG_VERSION")
                );
                assert_eq!(text.matches(&producer).count(), 1, "{}", shape.name);
                assert_eq!(
                    text.lines()
                        .filter(|line| line.starts_with(".ident "))
                        .count(),
                    1,
                    "{}",
                    shape.name
                );
                // Compare the recorded hashes in memory. Never
                // rewrite emitted artifacts or relax any instruction bytes.
                text.replace(
                    &producer,
                    ".ident \"EvoCC 0.0.0 aarch64-unknown-linux-gnu\"\n",
                )
            } else {
                text
            };
            format!("asm {}", evocc_proof::sha256_hex(text.as_bytes()))
        }
        Err(stop) => format!("stop {} {}", stop.component(), stop.detail()),
    }
}

#[test]
fn darwin_and_linux_placement_matches_recorded_output() {
    let mut actual = Vec::new();
    for shape in SHAPES {
        let ir = evoir(shape);
        actual.push((
            shape.name,
            outcome(Target::Darwin, shape, &ir),
            outcome(Target::Linux, shape, &ir),
        ));
    }
    let recorded: Vec<_> = RECORDED
        .iter()
        .map(|(name, darwin, linux)| (*name, (*darwin).to_owned(), (*linux).to_owned()))
        .collect();
    if actual != recorded {
        let mut table = String::new();
        for (name, darwin, linux) in &actual {
            table.push_str(&format!(
                "    (\n        {name:?},\n        {darwin:?},\n        {linux:?},\n    ),\n"
            ));
        }
        panic!("placement snapshot differs; actual table:\n{table}");
    }
}
