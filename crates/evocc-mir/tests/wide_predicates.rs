//! The full-width predicates stay total, including overflowing operands.
//! Sealed references: 20260909-wide-predicates, GCC-14/Clang-19 O0/O2.

use evocc_evoir::{interpret_module, parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::lane::observe_entry;
use evocc_mir::{lower_module, validate_module, MirLimits};
use evocc_types::{EvoIRType, Region};

fn function(name: &str, predicate: &str, widths: &[u16]) -> String {
    let params = widths
        .iter()
        .enumerate()
        .map(|(i, width)| format!("%arg{i}: bits<{width}>, "))
        .collect::<String>();
    let args = widths
        .iter()
        .enumerate()
        .map(|(i, width)| format!("%v{i}: bits<{width}>, "))
        .collect::<String>();
    let memory = widths.len();
    let world = memory + 1;
    format!(
        "func @{name}({params}%arg{memory}: mem<all>, %arg{world}: world) -> (bits<32>, mem<all>, world) cc = target_default\n\
         \x20   effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}\n\
         {{\n^entry({args}%v90: mem<all>, %v91: world):\n\
         \x20   %v2 = {predicate}\n\
         \x20   %v3 = zext.bits<32> %v2\n\
         \x20   ret %v3, %v90, %v91\n}}\n"
    )
}

struct Predicate {
    source: evocc_evoir::Module,
    lowered: evocc_mir::MModule,
    widths: Vec<u16>,
}

impl Predicate {
    fn new(predicate: &str, widths: &[u16]) -> Self {
        let text = function("probe", predicate, widths);
        let source = parse_section9_module(text.as_bytes()).expect("valid predicate syntax");
        evocc_evoir::validate_module(&source).expect("valid predicate module");
        let lowered = lower_module(&source).expect("full-width predicates must lower");
        validate_module(&lowered).expect("the independent machine validator must accept");
        let rendered = evocc_mir::render_module(&lowered);
        let parsed = evocc_mir::parse_module(&rendered).expect("machine text must round-trip");
        assert_eq!(evocc_mir::render_module(&parsed), rendered);
        Self {
            source,
            lowered,
            widths: widths.to_vec(),
        }
    }

    fn check(&self, values: &[u128], expected: bool) {
        let mut arguments: Vec<_> = self
            .widths
            .iter()
            .zip(values)
            .map(|(&width, &value)| RuntimeValue::wide(EvoIRType::Bits(width), value))
            .collect();
        arguments.push(RuntimeValue::new(EvoIRType::Mem(Region::All), 0));
        arguments.push(RuntimeValue::new(EvoIRType::World, 0));
        let source = interpret_module(
            &self.source,
            "probe",
            &arguments,
            InterpreterLimits::new(1000),
        )
        .expect("a predicate is total, not an overflowing arithmetic operation");
        assert_eq!(
            source.return_values[0].raw_bits,
            u64::from(expected),
            "EvoIR {values:x?}"
        );
        let machine = observe_entry(
            &self.lowered,
            "probe",
            &arguments,
            &[EvoIRType::Bits(32)],
            MirLimits::new(1000),
        )
        .expect("machine predicate executes");
        assert_eq!(
            machine.returns[0].raw_bits,
            u64::from(expected),
            "EvoMIR {values:x?}"
        );
    }
}

fn values() -> Vec<i128> {
    let limbs = [
        0_u64,
        1,
        2,
        i64::MAX as u64,
        1 << 63,
        u64::MAX - 1,
        u64::MAX,
        0x1234_5678_9abc_def0,
    ];
    limbs
        .iter()
        .flat_map(|&high| {
            limbs
                .iter()
                .map(move |&low| ((u128::from(high) << 64) | u128::from(low)) as i128)
        })
        .collect()
}

fn binary(predicate: &str, expected: impl Fn(i128, i128) -> bool) {
    let fixture = Predicate::new(&format!("{predicate} %v0, %v1"), &[128, 128]);
    for a in values() {
        for b in values() {
            fixture.check(&[a as u128, b as u128], expected(a, b));
        }
    }
}

#[test]
fn addition_checks_the_final_signed_carry_chain() {
    binary("signed_add_defined", |a, b| a.checked_add(b).is_some());
}

#[test]
fn subtraction_checks_the_final_signed_borrow_chain() {
    binary("signed_sub_defined", |a, b| a.checked_sub(b).is_some());
}

#[test]
fn multiplication_checks_all_four_product_limbs_and_both_sign_limits() {
    binary("signed_mul_defined", |a, b| a.checked_mul(b).is_some());
}

#[test]
fn division_overflow_is_exactly_minimum_divided_by_minus_one() {
    binary("signed_div_rem_no_overflow", |a, b| {
        !(a == i128::MIN && b == -1)
    });
}

#[test]
fn a_divisor_with_only_high_bits_is_nonzero() {
    let fixture = Predicate::new("divisor_nonzero %v0", &[128]);
    for value in values() {
        fixture.check(&[value as u128], value != 0);
    }
}

fn mask(width: u16) -> u128 {
    if width == 128 {
        u128::MAX
    } else {
        (1_u128 << width) - 1
    }
}

#[test]
fn shift_counts_use_every_amount_bit_and_its_declared_signedness() {
    for width in [1_u16, 8, 32, 64, 128] {
        for amount_width in [1_u16, 8, 32, 64, 128] {
            for signed in [false, true] {
                let suffix = if signed { "signed" } else { "unsigned" };
                let fixture = Predicate::new(
                    &format!("shift_count_defined.{suffix} %v0, %v1"),
                    &[width, amount_width],
                );
                for raw in [
                    0,
                    1,
                    u128::from(width - 1),
                    u128::from(width),
                    u128::from(width + 1),
                    127,
                    128,
                    1 << 64,
                    u128::MAX,
                ] {
                    let count = raw & mask(amount_width);
                    let negative = signed && count & (1 << (amount_width - 1)) != 0;
                    fixture.check(&[0, count], !negative && count < u128::from(width));
                }
            }
        }
    }
}

#[test]
fn older_signed_shift_predicates_remain_total_at_full_width() {
    for amount_width in [1, 8, 64, 128] {
        for signed in [false, true] {
            let suffix = if signed { "signed" } else { "unsigned" };
            let fixture = Predicate::new(
                &format!("signed_shl_defined.{suffix} %v0, %v1"),
                &[128, amount_width],
            );
            for value in values() {
                for raw in [0, 1, 63, 64, 127, 128, 1 << 64, u128::MAX] {
                    let count = raw & mask(amount_width);
                    let negative = signed && count & (1 << (amount_width - 1)) != 0;
                    let expected = !negative
                        && count < 128
                        && value >= 0
                        && (value as u128) <= ((i128::MAX as u128) >> count);
                    fixture.check(&[value as u128, count], expected);
                }
            }
        }
    }
}

#[cfg(all(target_arch = "aarch64", any(target_os = "macos", target_os = "linux")))]
#[test]
fn native_predicates_agree_with_independent_magnitude_bounds() {
    use evocc_mir::{compile_module_for, Target};
    use std::process::Command;

    let mut text = String::new();
    for (name, operation) in [
        ("add_ok", "signed_add_defined"),
        ("sub_ok", "signed_sub_defined"),
        ("mul_ok", "signed_mul_defined"),
        ("div_ok", "signed_div_rem_no_overflow"),
        ("count_unsigned", "shift_count_defined.unsigned"),
        ("count_signed", "shift_count_defined.signed"),
        ("old_shift_ok", "signed_shl_defined.unsigned"),
    ] {
        text.push_str(&function(
            name,
            &format!("{operation} %v0, %v1"),
            &[128, 128],
        ));
        text.push('\n');
    }
    text.push_str(&function("nonzero", "divisor_nonzero %v0", &[128]));
    let source = parse_section9_module(text.as_bytes()).expect("valid predicate module");
    let target = if cfg!(target_os = "macos") {
        Target::Darwin
    } else {
        Target::Linux
    };
    let assembly = compile_module_for(target, &source, &|_| true)
        .expect("every predicate must lower, allocate, verify and emit");
    let work = std::env::temp_dir().join(format!("evocc-wide-predicates-{}", std::process::id()));
    std::fs::create_dir_all(&work).expect("private test directory");
    let path = work.join("predicates.s");
    std::fs::write(&path, assembly).expect("emitted assembly");
    let output = Command::new("clang")
        .args(["-O2", "-o"])
        .arg(work.join("probe"))
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/wide-predicate-driver.c"
        ))
        .arg(&path)
        .output()
        .expect("host driver compilation");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(work.join("probe"))
        .output()
        .expect("native execution");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("checks=21074 failures=0"));
    std::fs::remove_dir_all(work).expect("remove this test's private files");
}
