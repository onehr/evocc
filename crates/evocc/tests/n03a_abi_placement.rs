//! Stable Darwin and Linux ABI placement snapshots.
//!
//! Each C shape is lowered once by the unchanged frontend and then compiled by
//! the backend for both AArch64 profiles. Every entry is the SHA-256 of the
//! complete assembly, or the exact refusal, recorded before the argument,
//! result, variadic, frame and profile facts moved into target descriptions.
//! Extraction must reproduce every entry; this is not an ABI oracle.

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

/// Recorded at the step-2 tip (`e5c05b23`), before ABI and frame extraction.
const RECORDED: &[(&str, &str, &str)] = &[
    (
        "integer_stack_arguments",
        "stop lowering E4010",
        "asm d345d4293f05ac4dd55dab825a34f928e1ad979e53844f8a991770114c637687",
    ),
    (
        "eight_integer_arguments",
        "asm 1f20b12d62465bd06c9d74da06b137c6aec1afd7ff8410a331484a4e417acbe5",
        "asm fb04858c96783a889600623beff1ecb184a8a1661a67d087a000f241c53ab796",
    ),
    (
        "mixed_register_banks",
        "asm a966eaeded7e5dd26ff6c131b493989b3f9474b22fdab3afae31000cb540c639",
        "asm fe147537cdc5d74cf4eaa4bca55635440d30b59a9e0b73463a6b1ffdf444782b",
    ),
    (
        "homogeneous_floating_records",
        "asm b9d4afd4ae151cfcc08adc6d629752b605577d9fa36685af7ba70b54e0749cd0",
        "asm 2ed26c4459616e583ebcac9dce9515e091442d71d6109cd8215a432520d266e4",
    ),
    (
        "small_records",
        "asm e9b4e248128a6a3a7104e3ef4b3ab35376e46971b737c24e91c4474eae7681e3",
        "asm f813d90724f82ed67b566f537df065362e510f3cc67d79dc3c86bf99fabc96aa",
    ),
    (
        "record_register_exhaustion",
        "stop lowering E4010",
        "asm 5b7d22089b66b565eb6522a32a68cf5c4c32f068c8d634c114f6a8023a24c3c4",
    ),
    (
        "large_records",
        "asm dded81bf82715333374fe1d6792e93f9f7a87ccbd9702f58e27d735c1a550ec9",
        "asm 4e1d569f9a3259a144dd815939448bcdcc8c08f5b6cc066cbc75cb2f2a1def62",
    ),
    (
        "integer_pairs",
        "asm dd656ac0d92157731ad5a16045fabd26889bb5a8a354c35cd64e1ede0ac5cfc0",
        "asm f75ebb5240a5df406f19d84d92a4ced25b6f39076975946c3e5042cdf8f43ee1",
    ),
    (
        "variadic_callee",
        "asm 0e6c9c9a96c0d04b994e0842da8e18ec98205729a851703f1ca91835c9576a1d",
        "asm f917c22273765afdf4feffad0786d3454b9e079146fe780a1cd6b4b638d2b9f6",
    ),
    (
        "variadic_callee_general_only",
        "asm f891f5dc2a01514bdd0fca67ee120e1d6cf2d49953ac25df9e68dce986f164e5",
        "asm 515c12204a4a20c173a88c5d5ebb264f654c5634c303ed9eeedaf4d6aa294ff8",
    ),
    (
        "variadic_caller",
        "asm 026ef5ea6ab1e0f08c6ba66080ab48cb22e6f17c2d752e2bfbcc985d662b16cc",
        "asm 2a77134f6d1fb9956b416129f8c5ea79334f9504e4bdca03696d18d2a6cd69d7",
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
        "asm f10ca40a5a6ce2459cb39cdb54cca03ceb914a3b71526c821ec0fea1f574ff02",
        "asm 8e34ef2c72a04a1a474c29c9c89e122f3f8587f21518d3e126859190d947032f",
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
        "asm 77d3f284eb6f7c2bd8e8feb00d85bbe554f7b15d8fc5700aa9e1535d232cf9f9",
        "asm 1a9aa1e2030346585642a9d9f8a9865768d40469ecd5b22158bc6079d59da374",
    ),
    (
        "vector_arguments",
        "asm a9b148cc3c2967a436bbbff566fedcd06a6373db1909c779bd6f8675c74acf57",
        "asm 234a6f0366eb2dc6beca80d5221257b64c35aa4aa061aa248f9cf218d0a658f9",
    ),
    (
        "binary128",
        "stop lowering E4009",
        "asm 7038bbfade2ddce9a0386ad65e002413b19f7651d4a62d3188375eb3565f0305",
    ),
    (
        "patchable_entry",
        "stop emission patchable function entries require the Linux ELF target",
        "asm 7e59255147e04af5f708f90e3cb414794246c5ae9ff8cd5dad6e15c69ba0c6db",
    ),
    (
        "elf_sections",
        "stop emission individual function/data sections require the Linux ELF target",
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
    child
        .stdin
        .take()
        .unwrap()
        .write_all(shape.source.as_bytes())
        .unwrap();
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
        Ok(text) => format!("asm {}", evocc_proof::sha256_hex(text.as_bytes())),
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
