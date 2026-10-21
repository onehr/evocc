//! Floating vectors in EvoIR (3.8, L05). `vec<L, f32>` and `vec<L, f64>` are
//! values whose lanes are ordinary `f32` and `f64` values. `vec.pack`,
//! `vec.extract` and `vec.bitcast` move bits: a signaling NaN keeps its
//! payload and its signaling bit. The text keeps every lane type, and
//! validation refuses a pack, an extract or a bitcast whose types disagree,
//! and an HVA mark whose shape disagrees with its record.

use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, InterpreterLimits, RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

const LANES: &str = include_str!("../../../tests/fixtures/floating-vector-lanes.evoir");

fn tokens() -> [RuntimeValue; 2] {
    [
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ]
}

#[test]
fn floating_vectors_round_trip_through_the_text() {
    let module = parse_section9_module(LANES.as_bytes()).unwrap();
    assert_eq!(render_module(&module), LANES);
}

#[test]
fn lane_moves_keep_every_bit() {
    let module = parse_section9_module(LANES.as_bytes()).unwrap();
    // A signaling NaN and a negative zero cross a pack, two extracts and a
    // bitcast unchanged; only the addition quiets the NaN.
    let signaling = 0x7f80_0011;
    let negative_zero = 0x8000_0000;
    let mut args = vec![
        RuntimeValue::new(EvoIRType::F32, signaling),
        RuntimeValue::new(EvoIRType::F32, negative_zero),
    ];
    args.extend(tokens());
    let outcome = interpret_module(&module, "lanes", &args, InterpreterLimits::new(100)).unwrap();
    // The ordinary results come before the memory and world tokens.
    let values = outcome.return_values[..4]
        .iter()
        .map(RuntimeValue::value)
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        [
            u128::from(negative_zero),
            u128::from(signaling),
            u128::from(negative_zero) << 32 | u128::from(signaling),
            0x7fc0_0011,
        ]
    );
}

#[test]
fn a_lane_index_outside_the_vector_is_e3011() {
    let module = parse_section9_module(LANES.as_bytes()).unwrap();
    let one = 0x3ff0_0000_0000_0000_u64;
    for (index, expected) in [(0, Some(one)), (1, Some(one | 1 << 63)), (2, None)] {
        let mut args = vec![
            RuntimeValue::new(EvoIRType::F64, one),
            RuntimeValue::bits(32, index),
        ];
        args.extend(tokens());
        let outcome = interpret_module(&module, "index", &args, InterpreterLimits::new(100));
        match expected {
            Some(lane) => {
                let outcome = outcome.unwrap();
                assert_eq!(outcome.return_values[0].value(), u128::from(lane));
                // The bitcast keeps lane 0 in the low half.
                assert_eq!(
                    outcome.return_values[1].value(),
                    u128::from(one | 1 << 63) << 64 | u128::from(one)
                );
            }
            None => assert_eq!(outcome.unwrap_err().code(), "E3011"),
        }
    }
}

#[test]
fn mutated_lane_types_and_marks_are_rejected() {
    for invalid in [
        // A pack of f32 lanes into an integer or f64 vector.
        LANES.replace(
            "vec.pack %v0, %v1 : vec<2, f32>",
            "vec.pack %v0, %v1 : vec<2, bits<32>>",
        ),
        LANES.replace(
            "vec.pack %v0, %v1 : vec<2, f32>",
            "vec.pack %v0, %v1 : vec<2, f64>",
        ),
        // A pack with a missing lane, an extract of another lane type.
        LANES.replace(
            "vec.pack %v0, %v1 : vec<2, f32>",
            "vec.pack %v0 : vec<2, f32>",
        ),
        LANES.replace(
            "%v6 = vec.extract %v4, %v5 : f32",
            "%v6 = vec.extract %v4, %v5 : bits<32>",
        ),
        LANES.replace(
            "%v6 = vec.extract %v5, %v1 : f64",
            "%v6 = vec.extract %v5, %v1 : f32",
        ),
        // A bitcast that changes the size, and an operation that changes.
        LANES.replace(
            "%v9 = vec.bitcast %v4 : bits<64>",
            "%v9 = vec.bitcast %v4 : bits<32>",
        ),
        LANES.replace(
            "%v7 = vec.bitcast %v5 : vec<4, bits<32>>",
            "%v7 = vec.bitcast %v5 : vec<2, f32>",
        ),
        LANES.replace(
            "%v10 = fp.add %v8, %v6 : f32",
            "%v10 = fp.add %v8, %v4 : f32",
        ),
        // An HVA mark whose member count or lane width disagrees with the
        // record size.
        LANES.replace("hva=2xvec<4, bits<32>>", "hva=3xvec<4, bits<32>>"),
        LANES.replace("hva=2xvec<4, bits<32>>", "hva=2xvec<4, bits<64>>"),
    ] {
        assert_ne!(invalid, LANES);
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}
