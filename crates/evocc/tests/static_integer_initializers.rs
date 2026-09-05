use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn run(source: &str, mode: &str) -> Output {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let nonce = SERIAL.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "evocc-static-integer-{}-{nonce}.c",
        std::process::id()
    ));
    fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", mode])
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    output
}

fn agrees(source: &str, expected: i64) {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("{expected}\n"),
            "{mode}: {source}"
        );
    }
}

#[test]
fn bitwise_array_initializer_has_the_measured_value() {
    agrees(
        "const unsigned char a[] = {32 | 8}; int main(void) { return a[0]; }",
        40,
    );
    agrees("#define C 32\n#define S 8\nconst unsigned char a[] = {C, C|S, C^S, C&S}; int main(void) { return a[0]+a[1]+a[2]+a[3]; }", 112);
}

#[test]
fn initializer_arithmetic_uses_promotions_and_intermediate_casts() {
    agrees("int a = (unsigned char)257 + ((signed char)255 < 0); unsigned long b = 4294967295U + 1U; long c = (signed char)255; int main(void) { return a == 2 && b == 0 && c == -1; }", 1);
    agrees("int a = ((12 * 9) / 3) % 11; int b = ~0 & 15; int main(void) { return a == 3 && b == 15; }", 1);
}

#[test]
fn static_signed_arithmetic_wraps_in_its_operand_width() {
    agrees(
        "int a = 1 << 31; int b = 2147483647 + 1; int main(void) { return a == b && a < 0; }",
        1,
    );
}

#[test]
fn static_short_circuiting_does_not_evaluate_dead_operands() {
    agrees("int a = 0 && (1 / 0); int b = 1 ? 9 : (1 / 0); int c = 1 || (1 / 0); int main(void) { return a == 0 && b == 9 && c == 1; }", 1);
}

#[test]
fn static_conditional_converts_the_selected_arm_to_the_common_type() {
    agrees("int a = (1 ? -1 : 1U) > 0; int main(void) { return a; }", 1);
}

#[test]
fn static_sizeof_and_alignment_use_the_target_without_evaluation() {
    agrees("int side; unsigned long a = sizeof(int[3]) + _Alignof(long); unsigned long b = sizeof(side++); int main(void) { return a == 20 && b == 4 && side == 0; }", 1);
}

#[test]
fn static_integer_folding_does_not_erase_the_runtime_shift_obligation() {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run("int a = 1 << 31; int main(void) { return 1 << 31; }", mode);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3006"),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn automatic_null_pointers_remain_supported() {
    agrees("int main(void) { int *p = (int *)0; return p == 0; }", 1);
}

#[test]
fn static_pointer_bits_are_not_fabricated_without_relocations() {
    for source in [
        "void *p = (void *)1;",
        "void *p = (void *)0;",
        "int *p = 0;",
    ] {
        let output = run(source, "--emit=evoir");
        assert!(
            !output.status.success(),
            "a raw integer is not pointer provenance"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3026"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
