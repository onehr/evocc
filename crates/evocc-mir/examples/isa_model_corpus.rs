//! Frozen N07 E2 integer fixtures through the real interpreters and backend.
use evocc_evoir::{interpret_module, parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::emit::{CodegenOptions, FrameVariants};
use evocc_mir::{
    compile_module_with_options, lane, lower_module, MirLimits, RelocationModel, Target,
};
use evocc_types::EvoIRType;

const WIDTHS: [u16; 9] = [1, 2, 8, 16, 31, 32, 33, 63, 64];
const OPS: [&str; 9] = [
    "add", "sub", "mul", "and", "or", "xor", "shl", "lshr", "ashr",
];
const CONSTANTS: [u64; 8] = [
    0,
    1,
    0xff,
    0xffff,
    0xffff_ffff,
    u64::MAX,
    0x1234_5678_9abc_def0,
    1 << 63,
];

fn function(name: &str, body: &str, result: &str) -> String {
    format!(
        "func @{name}(%arg0: bits<64>, %arg1: bits<64>) -> (bits<64>) cc = target_default\n    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}\n{{\n^entry(%v0: bits<64>, %v1: bits<64>):\n{body}    ret {result}\n}}\n"
    )
}

fn source(name: &str, op: &str, width: u16) -> String {
    if width == 64 {
        function(name, &format!("    %v2 = {op}.bits<64> %v0, %v1\n"), "%v2")
    } else {
        function(
            name,
            &format!(
                "    %v2 = trunc.bits<{width}> %v0\n    %v3 = trunc.bits<{width}> %v1\n    %v4 = {op}.bits<{width}> %v2, %v3\n    %v5 = zext.bits<64> %v4\n"
            ),
            "%v5",
        )
    }
}

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn inputs(width: u16, ordinal: u64, shift: bool) -> Vec<(u64, u64)> {
    let mask = u64::MAX >> (64 - width);
    let sign = 1_u64 << (width - 1);
    let mut values = vec![
        (0, 0),
        (1, 1),
        (mask, mask),
        (sign, 1),
        (sign - 1, sign),
        (0xaaaa_aaaa_aaaa_aaaa, 0x5555_5555_5555_5555),
        (mask, 0),
        (0, mask),
    ];
    let mut state = 0x5037 ^ (ordinal << 32);
    for _ in 0..24 {
        values.push((next(&mut state), next(&mut state)));
    }
    if shift {
        for (_, count) in &mut values {
            *count %= u64::from(width);
        }
    }
    values
}

fn vectors(name: &str, source: &str, values: &[(u64, u64)]) -> Result<(), String> {
    let module = parse_section9_module(source.as_bytes())?;
    let machine = lower_module(&module).map_err(|e| format!("{e:?}"))?;
    for (index, &(a, b)) in values.iter().enumerate() {
        let args = [RuntimeValue::bits(64, a), RuntimeValue::bits(64, b)];
        let ir = interpret_module(&module, name, &args, InterpreterLimits::new(1000))
            .map_err(|e| format!("{name}/{index}: {e:?}"))?;
        let mir = lane::observe_entry(
            &machine,
            name,
            &args,
            &[EvoIRType::Bits(64)],
            MirLimits::new(1000),
        )
        .map_err(|e| format!("{name}/{index}: {e:?}"))?;
        if ir.return_values != mir.returns {
            return Err(format!("{name}/{index}: EvoIR/MIR mismatch"));
        }
        println!(
            "{name} {index} {a:016x} {b:016x} {:016x}",
            ir.return_values[0].raw_bits
        );
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let mode = std::env::args()
        .nth(1)
        .ok_or("expected vectors, ir, darwin or linux")?;
    if !matches!(mode.as_str(), "vectors" | "ir" | "darwin" | "linux") {
        return Err("expected vectors, ir, darwin or linux".into());
    }
    let mut text = String::new();
    let mut ordinal = 0;
    for width in WIDTHS {
        for op in OPS {
            let name = format!("model_{op}_{width}");
            let src = source(&name, op, width);
            if mode == "vectors" {
                vectors(
                    &name,
                    &src,
                    &inputs(width, ordinal, matches!(op, "shl" | "lshr" | "ashr")),
                )?;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&src);
            ordinal += 1;
        }
    }
    for (index, value) in CONSTANTS.into_iter().enumerate() {
        let name = format!("model_const_{index}");
        let src = function(&name, &format!("    %v2 = const.bits<64> {value}\n"), "%v2");
        if mode == "vectors" {
            vectors(&name, &src, &[(u64::MAX, 0xaaaa_aaaa_aaaa_aaaa)])?;
        }
        text.push('\n');
        text.push_str(&src);
    }
    if mode == "ir" {
        print!("{text}");
    } else if mode != "vectors" {
        let module = parse_section9_module(text.as_bytes())?;
        let options = CodegenOptions {
            frame: FrameVariants {
                leaf_frames: true,
                ..FrameVariants::OFF
            },
            ..CodegenOptions::default()
        };
        let target = if mode == "darwin" {
            Target::Darwin
        } else {
            Target::Linux
        };
        let assembly = compile_module_with_options(
            target,
            &module,
            &|_| true,
            RelocationModel::Static,
            &options,
        )
        .map_err(|e| e.detail())?;
        print!("{assembly}");
    }
    Ok(())
}
