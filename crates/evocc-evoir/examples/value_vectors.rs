//! Answer the `value` correspondence vectors through the EvoIR interpreter.
//!
//! `tools/lean/vectors.py` writes one call per line; this copies every line and
//! appends what the interpreter computes. The point of going through a module
//! rather than through a helper is that the interpreter is what the proof layer
//! claims corresponds to the Lean semantics, so a runner with its own
//! arithmetic would measure the runner.
//!
//! ```text
//! cargo run -p evocc-evoir --example value_vectors -- <input.jsonl> [<output.jsonl>]
//! ```
//!
//! With no output path the answers go to `<input stem>.evoir.rust.jsonl` beside
//! the input.
//!
//! Each vector becomes the smallest module that computes it: one entry function
//! whose parameters are the operands, one instruction, and a return. The
//! operands are parameters rather than `const` instructions because `const`
//! carries at most sixty-four bits and the shift-amount vectors reach `2^64`.
//! Modules are built once per shape and reused, so the file is measured against
//! the interpreter and not against the module builder.
//!
//! `skipped` marks a kind this runner does not implement -- the transfers belong
//! to the two known-bits runners -- and `unsupported` marks a width the executed
//! value model does not carry: EvoIR 1.1 admits every positive width, while
//! execution admits one to sixty-four and a hundred and twenty-eight, so a
//! `bits<65>` shift amount has no run to report.

use std::collections::HashMap;
use std::io::{BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use evocc_evoir::{
    interpret_module, Block, BlockArg, CmpOp, EffectSummary, EvoIRType, Function, FunctionParam,
    GlobalVisibility, Inst, InterpreterLimits, Linkage, Module, RuntimeValue, Value,
};

/// One instruction and one return; the budget only has to admit both.
const STEP_BUDGET: u64 = 16;

/// The widths the executed value model of EvoIR 10.1.19 carries.
fn executed(width: u16) -> bool {
    (1..=64).contains(&width) || width == 128
}

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let (input, output) = match arguments.as_slice() {
        [input] => {
            let input = PathBuf::from(input);
            let output = default_output(&input);
            (input, output)
        }
        [input, output] => (PathBuf::from(input), PathBuf::from(output)),
        _ => {
            eprintln!("usage: value_vectors <input.jsonl> [<output.jsonl>]");
            return ExitCode::from(2);
        }
    };
    match run(&input, &output) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("value_vectors: {message}");
            ExitCode::from(1)
        }
    }
}

fn default_output(input: &Path) -> PathBuf {
    let stem = input.file_stem().map_or_else(
        || "vectors".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    input.with_file_name(format!("{stem}.evoir.rust.jsonl"))
}

fn run(input: &Path, output: &Path) -> Result<String, String> {
    let source =
        std::fs::File::open(input).map_err(|error| format!("open {}: {error}", input.display()))?;
    let sink = std::fs::File::create(output)
        .map_err(|error| format!("create {}: {error}", output.display()))?;
    let mut writer = BufWriter::new(sink);
    let mut shapes: HashMap<Shape, Module> = HashMap::new();

    let mut answered = 0_u64;
    let mut skipped = 0_u64;
    let mut unsupported = 0_u64;
    for (number, line) in std::io::BufReader::new(source).lines().enumerate() {
        let line = line.map_err(|error| format!("read {}: {error}", input.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let result = answer(&line, &mut shapes)
            .ok_or_else(|| format!("line {}: not a vector", number + 1))?;
        match result.as_str() {
            "\"skipped\"" => skipped += 1,
            "\"unsupported\"" => unsupported += 1,
            _ => answered += 1,
        }
        let body = line
            .trim_end()
            .strip_suffix('}')
            .ok_or_else(|| format!("line {}: not a JSON object", number + 1))?;
        writeln!(writer, "{body},\"result\":{result}}}")
            .map_err(|error| format!("write {}: {error}", output.display()))?;
    }
    writer
        .flush()
        .map_err(|error| format!("write {}: {error}", output.display()))?;
    Ok(format!(
        "{}: answered={answered}, skipped={skipped}, unsupported={unsupported}",
        output.display()
    ))
}

/// The module a vector needs, up to the operand values: an operator and the
/// widths its operands and result carry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Shape {
    op: String,
    left: u16,
    right: Option<u16>,
    result: u16,
}

fn answer(line: &str, shapes: &mut HashMap<Shape, Module>) -> Option<String> {
    let kind = text_field(line, "kind")?;
    if kind != "value" {
        return Some("\"skipped\"".to_owned());
    }
    let op = text_field(line, "op")?;
    let n = number_field(line, "n")?;
    let arguments = hex_list(field(line, "args").unwrap_or("[]"));
    let source_width = number_field(line, "w").unwrap_or(n);
    let amount_width = number_field(line, "m").unwrap_or(n);

    let shape = match op {
        "zext" | "sext" | "trunc" => Shape {
            op: op.to_owned(),
            left: source_width,
            right: None,
            result: n,
        },
        "shl" | "lshr" | "ashr" => Shape {
            op: op.to_owned(),
            left: n,
            right: Some(amount_width),
            result: n,
        },
        _ if op.starts_with("icmp.") => Shape {
            op: op.to_owned(),
            left: n,
            right: Some(n),
            result: 1,
        },
        _ => Shape {
            op: op.to_owned(),
            left: n,
            right: Some(n),
            result: n,
        },
    };
    if !executed(shape.left) || !executed(shape.result) {
        return Some(unsupported());
    }
    if shape.right.is_some_and(|width| !executed(width)) {
        return Some(unsupported());
    }
    if arguments.len() != usize::from(shape.right.is_some()) + 1 {
        return Some(unsupported());
    }

    let module = match shapes.get(&shape) {
        Some(module) => module,
        None => {
            let built = build(&shape)?;
            shapes.entry(shape.clone()).or_insert(built)
        }
    };
    let mut entry = vec![RuntimeValue::wide(
        EvoIRType::Bits(shape.left),
        arguments[0],
    )];
    if let Some(width) = shape.right {
        entry.push(RuntimeValue::wide(EvoIRType::Bits(width), arguments[1]));
    }
    let outcome =
        interpret_module(module, "main", &entry, InterpreterLimits::new(STEP_BUDGET)).ok()?;
    Some(format!(
        "\"0x{:x}\"",
        outcome.return_values.first()?.value()
    ))
}

/// The smallest module computing one operator: the operands are the entry
/// arguments, the instruction is the vector, and the result is returned.
fn build(shape: &Shape) -> Option<Module> {
    let left = Value {
        id: 0,
        ty: EvoIRType::Bits(shape.left),
    };
    let right = shape.right.map(|width| Value {
        id: 1,
        ty: EvoIRType::Bits(width),
    });
    let result = Value {
        id: 2,
        ty: EvoIRType::Bits(shape.result),
    };
    let instruction = instruction(&shape.op, &result, &left, right.as_ref())?;

    let mut params = vec![FunctionParam {
        name: "a".to_owned(),
        ty: left.ty.clone(),
    }];
    let mut args = vec![BlockArg {
        name: "v0".to_owned(),
        value: left,
    }];
    if let Some(right) = right {
        params.push(FunctionParam {
            name: "b".to_owned(),
            ty: right.ty.clone(),
        });
        args.push(BlockArg {
            name: "v1".to_owned(),
            value: right,
        });
    }

    Some(Module {
        aliases: Vec::new(),
        assembly: Vec::new(),
        globals: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        functions: vec![Function {
            linkage: Linkage::Strong,
            visibility: GlobalVisibility::External,
            name: "main".to_owned(),
            params,
            param_marks: Vec::new(),
            variadic: false,
            ret_tys: vec![result.ty.clone()],
            effects: EffectSummary::default(),
            blocks: vec![Block {
                id: 0,
                args,
                insts: vec![instruction],
                terminator: Inst::Ret(vec![result]),
            }],
        }],
    })
}

fn instruction(op: &str, result: &Value, left: &Value, right: Option<&Value>) -> Option<Inst> {
    if let Some(comparison) = comparison(op) {
        return Some(Inst::ICmp(
            result.clone(),
            comparison,
            left.clone(),
            right?.clone(),
        ));
    }
    Some(match op {
        "add" => Inst::Add(result.clone(), left.clone(), right?.clone()),
        "sub" => Inst::Sub(result.clone(), left.clone(), right?.clone()),
        "mul" => Inst::Mul(result.clone(), left.clone(), right?.clone()),
        "and" => Inst::And(result.clone(), left.clone(), right?.clone()),
        "or" => Inst::Or(result.clone(), left.clone(), right?.clone()),
        "xor" => Inst::Xor(result.clone(), left.clone(), right?.clone()),
        "shl" => Inst::Shl(result.clone(), left.clone(), right?.clone()),
        "lshr" => Inst::LShr(result.clone(), left.clone(), right?.clone()),
        "ashr" => Inst::AShr(result.clone(), left.clone(), right?.clone()),
        "zext" => Inst::ZExt(result.clone(), left.clone(), result.ty.clone()),
        "sext" => Inst::SExt(result.clone(), left.clone(), result.ty.clone()),
        "trunc" => Inst::Trunc(result.clone(), left.clone(), result.ty.clone()),
        _ => return None,
    })
}

fn comparison(op: &str) -> Option<CmpOp> {
    Some(match op {
        "icmp.eq" => CmpOp::Eq,
        "icmp.ne" => CmpOp::Ne,
        "icmp.slt" => CmpOp::SLt,
        "icmp.sle" => CmpOp::SLe,
        "icmp.ult" => CmpOp::ULt,
        "icmp.ule" => CmpOp::ULe,
        _ => return None,
    })
}

fn unsupported() -> String {
    "\"unsupported\"".to_owned()
}

// -- reading one generated line ---------------------------------------------

/// The raw text of one top-level field, brackets and quotes included.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\":");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let mut depth = 0_usize;
    let mut quoted = false;
    for (index, character) in rest.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '[' | '{' if !quoted => depth += 1,
            ']' | '}' if !quoted => {
                if depth == 0 {
                    return Some(&rest[..index]);
                }
                depth -= 1;
            }
            ',' if !quoted && depth == 0 => return Some(&rest[..index]),
            _ => {}
        }
    }
    Some(rest)
}

fn text_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    field(line, key)?.trim_matches('"').into()
}

fn number_field(line: &str, key: &str) -> Option<u16> {
    field(line, key)?.parse().ok()
}

/// Every quoted hexadecimal literal in `raw`, in order.
fn hex_list(raw: &str) -> Vec<u128> {
    raw.split('"')
        .filter_map(|piece| piece.strip_prefix("0x"))
        .filter_map(|digits| u128::from_str_radix(digits, 16).ok())
        .collect()
}
