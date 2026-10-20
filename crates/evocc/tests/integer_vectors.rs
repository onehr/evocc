use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn run(source: &str, mode: &str) -> Output {
    run_with_options(source, mode, &[])
}

fn run_with_options(source: &str, mode: &str, options: &[&str]) -> Output {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let file = std::env::temp_dir().join(format!(
        "evocc-vector-{}-{}.c",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&file, source).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", mode])
        .args(options)
        .arg(&file)
        .output()
        .unwrap();
    fs::remove_file(file).unwrap();
    result
}

#[test]
fn target_aes_rounds_preserve_byte_order_and_operand_evaluation() {
    let source = format!(
        "#define TEST_PRIVATE\n{}",
        include_str!("../../../tests/fixtures/arm-neon-aes.c")
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = run(&source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n");
        }
    }
    let output = run(&source, "--emit=evoir");
    assert!(output.status.success());
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    assert_eq!(
        evocc_evoir::parse_section9_module(evocc_evoir::render_module(&module).as_bytes()).unwrap(),
        module
    );
    let machine =
        evocc_mir::lower_module_for(evocc_mir::Target::Linux, &module, &|_| false).unwrap();
    let text = evocc_mir::render_module(&machine);
    assert_eq!(evocc_mir::parse_module(&text).unwrap(), machine);
    assert_eq!(
        evocc_mir::interpret_module(&machine, "main", &[], evocc_mir::MirLimits::new(1_000_000))
            .unwrap()
            .results,
        [evocc_mir::MirValue::Int(0)]
    );
    for name in ["aese", "aesd", "aesmc", "aesimc"] {
        assert!(text.contains(&format!("crypto.{name} ")));
    }
    for operation in module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .filter_map(|i| {
            if let evocc_evoir::Inst::Vector(v) = i {
                Some(v)
            } else {
                None
            }
        })
        .filter(|v| matches!(v.op, evocc_evoir::VectorOp::Aes(_)))
    {
        let mut invalid = operation.clone();
        invalid.arguments.pop();
        assert!(invalid.validate().is_err(), "AES arity is part of its type");
        let mut invalid = operation.clone();
        invalid.arguments[0].ty = evocc_types::EvoIRType::Bits(128);
        assert!(
            invalid.validate().is_err(),
            "equal size is not the required vector type"
        );
    }
}

#[test]
fn target_aes_keeps_indeterminate_byte_dependencies() {
    for (body, expected) in [
        ("a[0]=0; __Uint8x16_t v=__builtin_aarch64_crypto_aesev16qi_uuu(a,k); return v[0]!=0x63;", None),
        ("a[0]=0; __Uint8x16_t v=__builtin_aarch64_crypto_aesev16qi_uuu(a,k); return v[1];", Some("E3001")),
        ("a[0]=0; __Uint8x16_t v=__builtin_aarch64_crypto_aesmcv16qi_uu(a); return v[0];", Some("E3001")),
        ("a[0]=a[1]=a[2]=a[3]=0; __Uint8x16_t v=__builtin_aarch64_crypto_aesmcv16qi_uu(a); return v[0];", None),
    ] {
        let source = format!("int main(void){{__Uint8x16_t a,k={{0}}; {body}}}");
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let out = run(&source, mode);
            if let Some(code) = expected {
                assert!(!out.status.success());
                assert!(String::from_utf8_lossy(&out.stderr).contains(code), "{mode}: {}", String::from_utf8_lossy(&out.stderr));
            } else {
                assert!(out.status.success(), "{mode}: {}", String::from_utf8_lossy(&out.stderr));
                assert_eq!(out.stdout, b"0\n");
            }
        }
    }
}

#[test]
fn aes_does_not_bypass_general_registers_only() {
    let source = "__Uint8x16_t f(__Uint8x16_t a, __Uint8x16_t b) { return __builtin_aarch64_crypto_aesev16qi_uuu(a,b); }";
    assert!(run(source, "--emit=asm").status.success());
    let output = run_with_options(source, "--emit=asm", &["-mgeneral-regs-only"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("E4002"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn target_polynomial_long_product_keeps_all_128_bits() {
    let source = format!(
        "#define TEST_PRIVATE\n{}",
        include_str!("../../../tests/fixtures/arm-neon-polynomial-long.c")
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = run(&source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n");
        }
    }
}

#[test]
fn target_byte_bit_reverse_covers_every_byte_without_reordering_lanes() {
    let source = format!(
        "#define TEST_PRIVATE\n{}",
        include_str!("../../../tests/fixtures/arm-neon-byte-reverse.c")
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        // Keep the CLI's 100,000-instruction budget. The exhaustive reference
        // loop is split into four disjoint batches only for that interpreter;
        // every byte is still checked, and native runs use the whole domain.
        let batches = if mode == "--interpret-evoir" { 4 } else { 1 };
        for batch in 0..batches {
            let start = batch * 256 / batches;
            let end = (batch + 1) * 256 / batches;
            let source = format!("#define TEST_START {start}\n#define TEST_END {end}\n{source}");
            let output = run(&source, mode);
            assert!(
                output.status.success(),
                "{mode}, bytes {start}..{end}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(output.stdout, b"0\n");
            }
        }
    }
}

#[test]
fn polynomial_scalars_and_high_lane_keep_storage_and_initializedness() {
    for (source, expected) in [
        ("int main(void){ __Poly128_t x=(__Poly128_t)((unsigned __int128)1<<100); __Poly128_t *p=&x; *p=x; return ((unsigned __int128)*p>>100)!=1; }", None),
        ("int main(void){ __Poly64x2_t a,b; a[1]=3; b[1]=3; return (unsigned __int128)__builtin_aarch64_crypto_pmullv2di_ppp(a,b)!=5; }", None),
        ("int main(void){ __Poly64x2_t a,b; a[0]=3; b[1]=3; return (unsigned __int128)__builtin_aarch64_crypto_pmullv2di_ppp(a,b)!=5; }", Some("E3001")),
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            if let Some(code) = expected {
                assert!(!output.status.success());
                assert!(String::from_utf8_lossy(&output.stderr).contains(code), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
            } else {
                assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
                assert_eq!(output.stdout, b"0\n");
            }
        }
    }
}

#[test]
fn target_combine_preserves_halves_and_operand_evaluation() {
    let source = format!(
        "#define TEST_PRIVATE\n{}",
        include_str!("../../../tests/fixtures/arm-neon-combine.c")
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = run(&source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n");
        }
    }
}

#[test]
fn target_extract_halves_preserves_lane_bits_and_scalar_doubleword() {
    let source = format!(
        "#define TEST_PRIVATE\n{}",
        include_str!("../../../tests/fixtures/arm-neon-halves.c")
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = run(&source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n");
        }
    }
}

#[test]
fn target_vector_halves_transport_unread_lanes_without_initializing_them() {
    for (source, error) in [
        ("int main(void){__Int8x16_t a; a[8]=3; __Int8x8_t h=__builtin_aarch64_get_highv16qi(a); return h[0]-3;}", None),
        ("int main(void){__Int8x16_t a; a[8]=3; __Int8x8_t h=__builtin_aarch64_get_highv16qi(a); return h[1];}", Some("E3001")),
        ("int main(void){__Int8x8_t a,b; a[0]=3; __Int8x16_t c=__builtin_aarch64_combinev8qi(a,b); return c[0]-3;}", None),
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            if let Some(code) = error {
                assert!(!output.status.success());
                assert!(String::from_utf8_lossy(&output.stderr).contains(code), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
            } else {
                assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
                assert_eq!(output.stdout, b"0\n");
            }
        }
    }
}

#[test]
fn integer_vector_types_values_masks_and_shuffle_execute() {
    let source = include_str!("../../../tests/fixtures/integer-vector-values.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n", "{mode}");
        }
    }
}

#[test]
fn vector_signature_remains_a_vector_in_ir() {
    let output = run(
        "typedef int v __attribute__((vector_size(16))); v f(v x){return x+x;}",
        "--emit=evoir",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("vec<4, bits<32>>"));
}

#[test]
fn target_integer_load_store_table_and_full_width_shift_execute() {
    let source = include_str!("../../../tests/fixtures/arm-neon-builtin-memory.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n", "{mode}");
        }
    }
}

#[test]
fn target_left_shifts_wrap_lanes_and_preserve_boundary_counts() {
    let source = "int main(void) {
        __Int8x16_t a = {-1, 1, 2, 3, 4, 5, 6, 7};
        __Int8x16_t same = __builtin_aarch64_ashlv16qi(a, 0);
        __Int8x16_t high = __builtin_aarch64_ashlv16qi(a, 7);
        for (int i=0; i<8; ++i) {
          if(same[i]!=a[i]) return 1;
          if((unsigned char)high[i] != ((unsigned char)a[i] << 7 & 255)) return 2;
        }
        return 0;
    }";
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n");
        }
    }
}

#[test]
fn target_immediates_use_the_declared_parameter_conversion() {
    let source = "int main(void) {
        __Uint8x16_t a={128};
        __Uint8x16_t b=__builtin_aarch64_lshrv16qi_uus(a,0x100000001ULL);
        __Int8x16_t c={1};
        c=__builtin_aarch64_ashlv16qi(c,0x100000007ULL);
        return b[0]!=64 || (unsigned char)c[0]!=128;
    }";
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n");
        }
    }
}

#[test]
fn target_narrow_insert_and_interleaved_loads_share_lane_semantics() {
    for case in ["TEST_NARROW", "TEST_INSERT", "TEST_INTERLEAVE"] {
        let source = format!(
            "#define TEST_PRIVATE\n#define {case}\n{}",
            include_str!("../../../tests/fixtures/arm-neon-interleave-shifts.c")
        );
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let output = run(&source, mode);
            assert!(
                output.status.success(),
                "{case} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(output.stdout, b"0\n", "{case} {mode}");
            }
        }
    }
}

#[test]
fn target_immediates_and_unsupported_intrinsics_do_not_become_library_calls() {
    for source in [
        "int main(void){__Int8x16_t v={0};v=__builtin_aarch64_ashlv16qi(v,8);return v[0];}",
        "int main(void){__Uint8x16_t v={0};v=__builtin_aarch64_lshrv16qi_uus(v,0);return v[0];}",
        "int main(void){int n=2;__Uint8x16_t v={0};v=__builtin_aarch64_lshrv16qi_uus(v,n);return v[0];}",
        "int main(void){__builtin_aarch64_im_lane_boundsi(16,1,16);return 0;}",
        "int main(void){__Int8x16_t v={0};v=__builtin_aarch64_absv16qi(v);return v[0];}",
        "int main(void){__Uint16x8_t v={0};return __builtin_aarch64_shrn_nv8hi_uus(v,0)[0];}",
        "int main(void){__Uint16x8_t v={0};return __builtin_aarch64_shrn_nv8hi_uus(v,9)[0];}",
        "int main(void){int n=2;__Uint16x8_t v={0};return __builtin_aarch64_shrn_nv8hi_uus(v,n)[0];}",
        "int main(void){__Uint8x8_t v={0};return __builtin_aarch64_usli_nv8qi_uuus(v,v,8)[0];}",
        "int main(void){__Uint8x16_t v={0};return __builtin_aarch64_usri_nv16qi_uuus(v,v,0)[0];}",
        "int main(void){int n=2;__Uint8x16_t v={0};return __builtin_aarch64_usri_nv16qi_uuus(v,v,n)[0];}",
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=evoir", "--emit=asm"] {
            let output = run(source, mode);
            assert!(!output.status.success(), "{mode}: {source}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("E3052"),
                "{mode}: {source}: {}",String::from_utf8_lossy(&output.stderr));
        }
    }
}

#[test]
fn target_interleaved_loads_preserve_memory_faults() {
    for (body, code) in [
        ("short a[15]={0}; return __builtin_aarch64_ld2v8hi(a).val[1][7];", "E3011"),
        ("short a[16]; for(int i=0;i<15;++i) a[i]=0; return __builtin_aarch64_ld2v8hi(a).val[1][7];", "E3001"),
        ("short a[17]={0}; return __builtin_aarch64_ld2v8hi((const short *)((char *)a+1)).val[0][0];", "E3012"),
    ] {
        let source = format!("#pragma GCC aarch64 \"arm_neon.h\"\nint main(void){{{body}}}");
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(&source, mode);
            assert!(!output.status.success(), "{mode}: {source}");
            assert!(String::from_utf8_lossy(&output.stderr).contains(code),
                "{mode}: {source}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

#[test]
fn polynomial_multiply_uses_xor_products_not_integer_carries() {
    let source = "int main(void) {
        __Poly8x16_t a={0,1,2,3,127,128,255,29,0,1,2,3,127,128,255,29};
        __Poly8x16_t b={255,3,3,3,255,2,255,128,255,3,3,3,255,2,255,128};
        __Poly8x16_t p=__builtin_aarch64_pmulv16qi_ppp(a,b);
        for(int i=0;i<16;++i){
          unsigned int expected=0;
          for(int bit=0;bit<8;++bit) if((unsigned char)b[i] & (1u<<bit))
            expected ^= (unsigned int)(unsigned char)a[i] << bit;
          if((unsigned char)p[i] != (unsigned char)expected) return 1;
        }
        return 0;
    }";
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(output.stdout, b"0\n");
        }
    }
}

#[test]
fn reading_a_lane_does_not_read_other_uninitialized_lanes() {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run("typedef int v __attribute__((vector_size(16))); int main(void){v x;x[2]=17;return x[2];}", mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"17\n");
        let output = run("typedef int v __attribute__((vector_size(16))); int main(void){v x;x[2]=17;return x[1];}", mode);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3001"));
    }
}

#[test]
fn homogeneous_and_wide_vector_abi_are_not_scalar_record_abi() {
    let source = concat!(
        include_str!("../../../tests/fixtures/integer-vector-abi.h"),
        "\n"
    );
    let worker = include_str!("../../../tests/fixtures/integer-vector-abi-worker.c")
        .lines()
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n");
    let output = run(&format!("{source}{worker}"), "--emit=asm");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = String::from_utf8_lossy(&output.stdout);
    assert!(assembly.contains("q0"));
    assert!(assembly.contains("d0") || assembly.contains("v0.8b"));
    let driver = include_str!("../../../tests/fixtures/integer-vector-abi-driver.c")
        .lines()
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(&format!("{source}{worker}\n{driver}"), mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
}

#[test]
fn vector_variadic_values_keep_the_simd_argument_class() {
    let source = format!(
        "{}\n{}\n{}",
        include_str!("../../../tests/fixtures/integer-vector-varargs.h"),
        include_str!("../../../tests/fixtures/integer-vector-varargs-worker.c")
            .lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n"),
        include_str!("../../../tests/fixtures/integer-vector-varargs-driver.c")
            .lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let out = run(&source, mode);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(out.stdout, b"0\n");
        }
    }
}

#[test]
fn vector_updates_conversions_and_wide_representations_execute() {
    let source = include_str!("../../../tests/fixtures/integer-vector-operations.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let out = run(source, mode);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(out.stdout, b"0\n", "{mode}");
        }
    }
}

#[test]
fn vector_index_checks_keep_the_entire_operand_width() {
    for index in ["-1", "4", "((__uint128_t)1<<100)", "0x8000000000000000UL"] {
        let source=format!("typedef int v __attribute__((vector_size(16))); v f(void){{return (v){{1,2,3,4}};}} int main(void){{return f()[{index}];}}");
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let out = run(&source, mode);
            assert!(
                !out.status.success(),
                "{mode}: {index}: {} {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                String::from_utf8_lossy(&out.stderr).contains("E3011"),
                "{mode}: {index}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
fn small_vector_abi_lowers_without_admitting_non_consensus_operations() {
    // L06: tiny vectors at a call boundary follow the target rule; the
    // crossed tests are in tiny_vector_abi.rs.
    let source = include_str!("../../../tests/fixtures/integer-vector-small.c");
    let out = run(source, "--emit=asm");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for source in [
        "typedef int v __attribute__((vector_size(16))); void f(v *p){(*p)++;}",
        "typedef int v __attribute__((vector_size(16))); int *f(v *p){return &(*p)[0];}",
    ] {
        let out = run(source, "--emit=evoir");
        assert!(
            !out.status.success(),
            "accepted a non-consensus GNU vector operation: {source}"
        );
    }
}

#[test]
fn arm_integer_header_types_keep_their_nominal_identity_and_real_lanes() {
    let source="typedef signed char bytes8 __attribute__((vector_size(8))); _Static_assert(sizeof(__Int8x8_t)==8,\"size\"); _Static_assert(_Alignof(__Int8x8_t)==8,\"alignment\"); _Static_assert(!__builtin_types_compatible_p(__Int8x8_t,bytes8),\"nominal identity\"); __Int8x8_t add(__Int8x8_t x){return x+1;} int main(void){__Int8x8_t x={1,2,3,4,5,6,7,8}; x=add(x); __Uint32x4_t y={2,3,4,5};y+=1;return x[7]+y[3]-15;}";
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let out = run(source, mode);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(out.stdout, b"0\n");
        }
    }
}

#[test]
fn integer_vectors_with_a_wide_lane_keep_the_full_comparison_mask() {
    let source="typedef unsigned __int128 V __attribute__((vector_size(16))); void mask(V v,V *r){*r=v!=0;} int main(void){V r;mask((V){5},&r);if(r[0]!=~(__uint128_t)0)return 1;mask((V){0x500000005ULL},&r);if(r[0]!=~(__uint128_t)0)return 2;mask((V){0},&r);return r[0]!=0;}";
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let out = run(source, mode);
        assert!(
            out.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if mode.starts_with("--interpret") {
            assert_eq!(out.stdout, b"0\n");
        }
    }
}
