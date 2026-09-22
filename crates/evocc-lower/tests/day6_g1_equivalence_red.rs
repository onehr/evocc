use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_evoir::{BlockId, CmpOp, Function, Inst, Module, RuntimeValue, Value};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::{AddressSpace, EvoIRType, Region, TargetDataLayout};

struct Case {
    id: &'static str,
    source: &'static str,
    expected: i128,
    golden_evoir: &'static str,
}

const CASES: &[Case] = &[
    Case {
        id: "literal",
        source: "int main(void) { return 42; }\n",
        expected: 42,
        golden_evoir: r#"func @main() -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry():
    %v0 = const.bits<32> 42
    ret %v0
}
"#,
    },
    Case {
        id: "ternary",
        source: "int main(void) { return 1 ? 7 : 9; }\n",
        expected: 7,
        golden_evoir: r#"func @main() -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry():
    %v0 = const.bits<32> 1
    %v1 = const.bits<32> 0
    %v2 = icmp.ne %v0, %v1
    br ^b1()

^b1():
    %v3 = const.bits<32> 7
    br ^b2(%v3)

^b2(%v4: bits<32>):
    ret %v4
}
"#,
    },
    Case {
        id: "compound_assignment",
        source: "int main(void) { int x = 3; x += 4; return x; }\n",
        expected: 7,
        golden_evoir: r#"func @main() -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry():
    %v0 = const.bits<32> 3
    %v1 = const.bits<32> 4
    %v2 = signed_add_defined %v0, %v1
    ub.assume.signed_overflow %v2
    %v3 = add.bits<32> %v0, %v1
    ret %v3
}
"#,
    },
    Case {
        id: "iterative_fib",
        source: "int main(void) { int n = 8; int a = 0; int b = 1; while (n > 0) { int next = a + b; a = b; b = next; n -= 1; } return a; }\n",
        expected: 21,
        golden_evoir: r#"func @main() -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry():
    %v0 = const.bits<32> 8
    %v1 = const.bits<32> 0
    %v2 = const.bits<32> 1
    br ^b1(%v0, %v1, %v2)

^b1(%v3: bits<32>, %v4: bits<32>, %v5: bits<32>):
    %v6 = const.bits<32> 0
    %v7 = icmp.slt %v6, %v3
    cond_br %v7, ^b2(%v3, %v4, %v5), ^b3(%v4)

^b2(%v8: bits<32>, %v9: bits<32>, %v10: bits<32>):
    %v11 = signed_add_defined %v9, %v10
    ub.assume.signed_overflow %v11
    %v12 = add.bits<32> %v9, %v10
    %v13 = const.bits<32> 1
    %v14 = signed_sub_defined %v8, %v13
    ub.assume.signed_overflow %v14
    %v15 = sub.bits<32> %v8, %v13
    br ^b1(%v15, %v10, %v12)

^b3(%v16: bits<32>):
    ret %v16
}
"#,
    },
    Case {
        id: "nested_if_switch",
        source: "int main(void) { int x = 2; int r = 0; if (x == 2) { switch (x) { case 1: r = 3; break; case 2: r = 10; default: r += 5; break; } } else { r = 99; } return r; }\n",
        expected: 15,
        golden_evoir: r#"func @main() -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry():
    %v0 = const.bits<32> 2
    %v1 = const.bits<32> 0
    %v2 = const.bits<32> 2
    %v3 = icmp.eq %v0, %v2
    cond_br %v3, ^b1(%v0, %v1), ^b2(%v1)

^b1(%v4: bits<32>, %v5: bits<32>):
    %v6 = const.bits<32> 1
    %v7 = icmp.eq %v4, %v6
    cond_br %v7, ^b3(%v5), ^b4(%v4, %v5)

^b2(%v8: bits<32>):
    %v9 = const.bits<32> 99
    br ^b7(%v9)

^b3(%v10: bits<32>):
    %v11 = const.bits<32> 3
    br ^b7(%v11)

^b4(%v12: bits<32>, %v13: bits<32>):
    %v14 = const.bits<32> 2
    %v15 = icmp.eq %v12, %v14
    cond_br %v15, ^b5(%v13), ^b6(%v13)

^b5(%v16: bits<32>):
    %v17 = const.bits<32> 10
    br ^b6(%v17)

^b6(%v18: bits<32>):
    %v19 = const.bits<32> 5
    %v20 = signed_add_defined %v18, %v19
    ub.assume.signed_overflow %v20
    %v21 = add.bits<32> %v18, %v19
    br ^b7(%v21)

^b7(%v22: bits<32>):
    ret %v22
}
"#,
    },
    Case {
        id: "for_continue_break",
        source: "int main(void) { int sum = 0; for (int i = 0; i < 10; i++) { if (i == 2) continue; if (i == 7) break; sum += i; } return sum; }\n",
        expected: 19,
        golden_evoir: r#"func @main() -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry():
    %v0 = const.bits<32> 0
    %v1 = const.bits<32> 0
    br ^b1(%v1, %v0)

^b1(%v2: bits<32>, %v3: bits<32>):
    %v4 = const.bits<32> 10
    %v5 = icmp.slt %v2, %v4
    cond_br %v5, ^b2(%v2, %v3), ^b6(%v3)

^b2(%v6: bits<32>, %v7: bits<32>):
    %v8 = const.bits<32> 2
    %v9 = icmp.eq %v6, %v8
    cond_br %v9, ^b4(%v6, %v7), ^b3(%v6, %v7)

^b3(%v10: bits<32>, %v11: bits<32>):
    %v12 = const.bits<32> 7
    %v13 = icmp.eq %v10, %v12
    cond_br %v13, ^b6(%v11), ^b5(%v10, %v11)

^b4(%v14: bits<32>, %v15: bits<32>):
    %v16 = const.bits<32> 1
    %v17 = signed_add_defined %v14, %v16
    ub.assume.signed_overflow %v17
    %v18 = add.bits<32> %v14, %v16
    br ^b1(%v18, %v15)

^b5(%v19: bits<32>, %v20: bits<32>):
    %v21 = signed_add_defined %v20, %v19
    ub.assume.signed_overflow %v21
    %v22 = add.bits<32> %v20, %v19
    br ^b4(%v19, %v22)

^b6(%v23: bits<32>):
    ret %v23
}
"#,
    },
    Case {
        id: "pure_call",
        source: "int add1(int x) { return x + 1; } int main(void) { return add1(41); }\n",
        expected: 42,
        golden_evoir: r#"func @add1(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 1
    %v4 = signed_add_defined %v0, %v3
    ub.assume.signed_overflow %v4
    %v5 = add.bits<32> %v0, %v3
    ret %v5, %v1, %v2
}

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 41
    (%v3, %v4, %v5) = call @add1(%v2, %v0, %v1)
    ret %v3, %v4, %v5
}
"#,
    },
    Case {
        id: "recursive_fib",
        source: "int fib(int n) { if (n < 2) return n; return fib(n - 1) + fib(n - 2); } int main(void) { return fib(8); }\n",
        expected: 21,
        golden_evoir: r#"func @fib(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 2
    %v4 = icmp.slt %v0, %v3
    cond_br %v4, ^b1(%v0, %v1, %v2), ^b2(%v0, %v1, %v2)

^b1(%v5: bits<32>, %v6: mem<all>, %v7: world):
    ret %v5, %v6, %v7

^b2(%v8: bits<32>, %v9: mem<all>, %v10: world):
    %v11 = const.bits<32> 1
    %v12 = signed_sub_defined %v8, %v11
    ub.assume.signed_overflow %v12
    %v13 = sub.bits<32> %v8, %v11
    (%v14, %v15, %v16) = call @fib(%v13, %v9, %v10)
    %v17 = const.bits<32> 2
    %v18 = signed_sub_defined %v8, %v17
    ub.assume.signed_overflow %v18
    %v19 = sub.bits<32> %v8, %v17
    (%v20, %v21, %v22) = call @fib(%v19, %v15, %v16)
    %v23 = signed_add_defined %v14, %v20
    ub.assume.signed_overflow %v23
    %v24 = add.bits<32> %v14, %v20
    ret %v24, %v21, %v22
}

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 8
    (%v3, %v4, %v5) = call @fib(%v2, %v0, %v1)
    ret %v3, %v4, %v5
}
"#,
    },
];

struct ExtendedCase {
    case: Case,
    expected_evoir_steps: Option<u64>,
}

const EXTENDED_CASES: &[ExtendedCase] = &[
    ExtendedCase {
        case: Case {
            id: "logical_short_circuit_side_effects",
            source: "int main(void) { int x = 7; int a = 0 && (x = 99); int b = 1 || (x = 88); return x * 100 + a * 10 + b; }\n",
            expected: 701,
            golden_evoir: r#"func @main() -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry():
    %v0 = const.bits<32> 7
    %v1 = const.bits<32> 0
    %v2 = const.bits<32> 0
    %v3 = icmp.ne %v1, %v2
    %v4 = zext.bits<32> %v3
    %v5 = const.bits<32> 1
    %v6 = const.bits<32> 0
    %v7 = icmp.ne %v5, %v6
    %v8 = zext.bits<32> %v7
    %v9 = const.bits<32> 100
    %v10 = signed_mul_defined %v0, %v9
    ub.assume.signed_overflow %v10
    %v11 = mul.bits<32> %v0, %v9
    %v12 = const.bits<32> 10
    %v13 = signed_mul_defined %v4, %v12
    ub.assume.signed_overflow %v13
    %v14 = mul.bits<32> %v4, %v12
    %v15 = signed_add_defined %v11, %v14
    ub.assume.signed_overflow %v15
    %v16 = add.bits<32> %v11, %v14
    %v17 = signed_add_defined %v16, %v8
    ub.assume.signed_overflow %v17
    %v18 = add.bits<32> %v16, %v8
    ret %v18
}
"#,
        },
        expected_evoir_steps: None,
    },
    ExtendedCase {
        case: Case {
            id: "do_while_continue_break",
            source: "int main(void) { int i = 0; int sum = 0; do { i++; if (i == 2) continue; if (i == 5) break; sum += i; } while (i < 8); return sum * 10 + i; }\n",
            expected: 85,
            golden_evoir: r#"func @main() -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry():
    %v0 = const.bits<32> 0
    %v1 = const.bits<32> 0
    br ^b1(%v0, %v1)

^b1(%v2: bits<32>, %v3: bits<32>):
    %v4 = const.bits<32> 1
    %v5 = signed_add_defined %v2, %v4
    ub.assume.signed_overflow %v5
    %v6 = add.bits<32> %v2, %v4
    %v7 = const.bits<32> 2
    %v8 = icmp.eq %v6, %v7
    cond_br %v8, ^b4(%v6, %v3), ^b2(%v3, %v6)

^b2(%v9: bits<32>, %v10: bits<32>):
    %v11 = const.bits<32> 5
    %v12 = icmp.eq %v10, %v11
    cond_br %v12, ^b5(%v10, %v9), ^b3(%v9, %v10)

^b3(%v13: bits<32>, %v14: bits<32>):
    %v15 = signed_add_defined %v13, %v14
    ub.assume.signed_overflow %v15
    %v16 = add.bits<32> %v13, %v14
    br ^b4(%v14, %v16)

^b4(%v17: bits<32>, %v18: bits<32>):
    %v19 = const.bits<32> 8
    %v20 = icmp.slt %v17, %v19
    cond_br %v20, ^b1(%v17, %v18), ^b5(%v17, %v18)

^b5(%v21: bits<32>, %v22: bits<32>):
    %v23 = const.bits<32> 10
    %v24 = signed_mul_defined %v22, %v23
    ub.assume.signed_overflow %v24
    %v25 = mul.bits<32> %v22, %v23
    %v26 = signed_add_defined %v25, %v21
    ub.assume.signed_overflow %v26
    %v27 = add.bits<32> %v25, %v21
    ret %v27
}
"#,
        },
        expected_evoir_steps: None,
    },
    ExtendedCase {
        case: Case {
            id: "forward_and_backward_goto",
            source: "int main(void) { int x = 0; goto start; again: x += 3; start: x += 1; if (x < 5) goto again; return x; }\n",
            expected: 5,
            golden_evoir: r#"func @main() -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry():
    %v0 = const.bits<32> 0
    br ^b2(%v0)

^b1(%v1: bits<32>):
    %v2 = const.bits<32> 3
    %v3 = signed_add_defined %v1, %v2
    ub.assume.signed_overflow %v3
    %v4 = add.bits<32> %v1, %v2
    br ^b2(%v4)

^b2(%v5: bits<32>):
    %v6 = const.bits<32> 1
    %v7 = signed_add_defined %v5, %v6
    ub.assume.signed_overflow %v7
    %v8 = add.bits<32> %v5, %v6
    %v9 = const.bits<32> 5
    %v10 = icmp.slt %v8, %v9
    cond_br %v10, ^b3(%v8), ^b4(%v8)

^b3(%v11: bits<32>):
    br ^b1(%v11)

^b4(%v12: bits<32>):
    ret %v12
}
"#,
        },
        expected_evoir_steps: None,
    },
    ExtendedCase {
        case: Case {
            id: "void_helper_expression_statement",
            source: "void touch(int x) { x += 1; return; } int main(void) { touch(41); return 42; }\n",
            expected: 42,
            golden_evoir: r#"func @touch(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 1
    %v4 = signed_add_defined %v0, %v3
    ub.assume.signed_overflow %v4
    %v5 = add.bits<32> %v0, %v3
    ret %v1, %v2
}

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 41
    (%v3, %v4) = call @touch(%v2, %v0, %v1)
    %v5 = const.bits<32> 42
    ret %v5, %v3, %v4
}
"#,
        },
        expected_evoir_steps: Some(9),
    },
];

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = FileId::new(6_001);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err(format!("lexer rejected {source:?} at {:?}", token.span));
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    if parser.had_error() {
        return Err(format!(
            "parser rejected {source:?}: {:?}",
            parser.diagnostic()
        ));
    }
    Ok(unit)
}

fn function_symbol(unit: &CHirUnit<'_>, name: &str) -> Result<SymbolId, String> {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function.symbol),
            _ => None,
        })
        .ok_or_else(|| format!("missing function {name}"))
}

fn observe_chir(unit: &CHirUnit<'_>) -> Result<i128, String> {
    let main = function_symbol(unit, "main")?;
    let outcome = interpret_scalar_function(unit, main, InterpreterLimits::new(100_000))
        .map_err(|error| format!("{error:?}"))?;
    let value = outcome
        .return_value
        .ok_or_else(|| "main returned no scalar value".to_owned())?;
    value
        .mathematical_value(unit.types())
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EvoExecutionObservation {
    value: i128,
    steps: u64,
}

fn observe_evoir(functions: &[Function]) -> Result<EvoExecutionObservation, String> {
    let main = functions
        .iter()
        .find(|function| function.name == "main")
        .ok_or_else(|| "lowering produced no @main".to_owned())?;
    let expected_return_count = main.ret_tys.len();
    let entry_arguments = main
        .params
        .iter()
        .enumerate()
        .map(|(index, parameter)| match &parameter.ty {
            EvoIRType::Mem(_) | EvoIRType::World => Ok(RuntimeValue::new(parameter.ty.clone(), 0)),
            _ => Err(format!(
                "EvoIR main has unsupported ordinary entry parameter {index}: {:?}",
                parameter.ty
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let module = Module {
        tls: Vec::new(),
        lifecycle: Vec::new(),
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![],
        functions: functions.to_vec(),
    };
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let evaluated = catch_unwind(AssertUnwindSafe(|| {
            evocc_evoir::interpret_module(
                &module,
                "main",
                &entry_arguments,
                evocc_evoir::InterpreterLimits::new(100_000),
            )
            .map_err(Box::new)
        }))
        .map_err(|_| "EvoIR interpreter panicked".to_owned());
        let _ = sender.send(evaluated);
    });
    let evaluated = receiver
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| "EvoIR interpreter exceeded the one-second watchdog".to_owned())??
        .map_err(|error| error.to_string())?;
    if evaluated.return_values.len() != expected_return_count {
        return Err(format!(
            "EvoIR interpreter returned {} values for {} declared results",
            evaluated.return_values.len(),
            expected_return_count
        ));
    }
    let value = evaluated
        .return_values
        .first()
        .ok_or_else(|| "EvoIR interpreter returned no scalar value".to_owned())?;
    let EvoIRType::Bits(width @ 1..=64) = value.ty else {
        return Err(format!(
            "EvoIR main returned non-scalar or unsupported type {:?}",
            value.ty
        ));
    };
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    let raw = value.raw_bits & mask;
    let sign_bit = 1_u64 << (width - 1);
    let value = if raw & sign_bit == 0 {
        i128::from(raw)
    } else {
        i128::from(raw) - (1_i128 << width)
    };
    Ok(EvoExecutionObservation {
        value,
        steps: evaluated.steps,
    })
}

fn render_type(ty: &EvoIRType) -> String {
    match ty {
        EvoIRType::Bits(width) => format!("bits<{width}>"),
        EvoIRType::F32 => "f32".to_owned(),
        EvoIRType::F64 => "f64".to_owned(),
        EvoIRType::Ptr(space) => format!(
            "ptr<{}>",
            match space {
                AddressSpace::Generic => "generic",
                AddressSpace::Global => "global",
                AddressSpace::Stack => "stack",
                AddressSpace::ThreadLocal => "threadlocal",
            }
        ),
        EvoIRType::FnPtr {
            params, results, ..
        } => format!(
            "fnptr<({})->({})>",
            params
                .iter()
                .map(render_type)
                .collect::<Vec<_>>()
                .join(", "),
            results
                .iter()
                .map(render_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        EvoIRType::Vec(count, element) => format!("vec<{count}, {}>", render_type(element)),
        EvoIRType::Mem(region) => format!(
            "mem<{}>",
            match region {
                Region::All => "all",
                Region::Unknown => "unknown",
                Region::Global => "global",
                Region::Stack => "stack",
                Region::Heap => "heap",
                Region::Escaped => "escaped",
            }
        ),
        EvoIRType::World => "world".to_owned(),
        EvoIRType::Tuple(elements) => format!(
            "tuple<{}>",
            elements
                .iter()
                .map(render_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        EvoIRType::Void => "void".to_owned(),
    }
}

fn value(value: &Value) -> String {
    format!("%v{}", value.id)
}

fn block_name(id: BlockId) -> String {
    if id == 0 {
        "^entry".to_owned()
    } else {
        format!("^b{id}")
    }
}

fn values(values: &[Value]) -> String {
    values.iter().map(value).collect::<Vec<_>>().join(", ")
}

fn branch_target(id: BlockId, args: &[Value]) -> String {
    format!("{}({})", block_name(id), values(args))
}

fn render_inst(inst: &Inst) -> String {
    match inst {
        Inst::IndirectBr { .. }
        | Inst::TlsAddr(_)
        | Inst::Vector(_)
        | Inst::Floating(_)
        | Inst::Atomic(_)
        | Inst::Asm(_)
        | Inst::AsmGoto { .. }
        | Inst::VaAccess(_)
        | Inst::BitOperation { .. }
        | Inst::ReturnAddress { .. }
        | Inst::FrameAddress { .. }
        | Inst::PointerCast { .. } => {
            panic!("assembly, variadic lists and activation addresses are outside these scalar golden fixtures")
        }
        Inst::Const(result, immediate) => format!(
            "{} = const.{} {immediate}",
            value(result),
            render_type(&result.ty)
        ),
        Inst::Uninitialized(result) => format!(
            "{} = uninitialized.{}",
            value(result),
            render_type(&result.ty)
        ),
        Inst::Add(result, left, right) => format!(
            "{} = add.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::Sub(result, left, right) => format!(
            "{} = sub.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::Mul(result, left, right) => format!(
            "{} = mul.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::SRem(result, left, right) => format!(
            "{} = srem.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::URem(result, left, right) => format!(
            "{} = urem.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::Shl(result, left, right) => format!(
            "{} = shl.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::AShr(result, left, right) => format!(
            "{} = ashr.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::LShr(result, left, right) => format!(
            "{} = lshr.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::And(result, left, right) => format!(
            "{} = and.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::Or(result, left, right) => format!(
            "{} = or.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::Xor(result, left, right) => format!(
            "{} = xor.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::SignedAddDefined(result, left, right) => format!(
            "{} = signed_add_defined {}, {}",
            value(result),
            value(left),
            value(right)
        ),
        Inst::SignedSubDefined(result, left, right) => format!(
            "{} = signed_sub_defined {}, {}",
            value(result),
            value(left),
            value(right)
        ),
        Inst::SignedMulDefined(result, left, right) => format!(
            "{} = signed_mul_defined {}, {}",
            value(result),
            value(left),
            value(right)
        ),
        Inst::DivisorNonZero(result, divisor) => {
            format!("{} = divisor_nonzero {}", value(result), value(divisor))
        }
        Inst::SignedDivRemNoOverflow(result, left, right) => format!(
            "{} = signed_div_rem_no_overflow {}, {}",
            value(result),
            value(left),
            value(right)
        ),
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => format!(
            "{} = shift_count_defined.{} {}, {}",
            value(result),
            if *amount_signed { "signed" } else { "unsigned" },
            value(shifted),
            value(amount)
        ),
        Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => format!(
            "{} = signed_shl_defined.{} {}, {}",
            value(result),
            if *amount_signed { "signed" } else { "unsigned" },
            value(shifted),
            value(amount)
        ),
        Inst::UbAssume(condition, reason) => {
            format!("ub.assume.{} {}", reason.mnemonic(), value(condition))
        }
        Inst::SDiv(result, left, right) => format!(
            "{} = sdiv.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::UDiv(result, left, right) => format!(
            "{} = udiv.{} {}, {}",
            value(result),
            render_type(&result.ty),
            value(left),
            value(right)
        ),
        Inst::ICmp(result, op, left, right) => format!(
            "{} = icmp.{} {}, {}",
            value(result),
            match op {
                CmpOp::Eq => "eq",
                CmpOp::Ne => "ne",
                CmpOp::SLt => "slt",
                CmpOp::SLe => "sle",
                CmpOp::ULt => "ult",
                CmpOp::ULe => "ule",
            },
            value(left),
            value(right)
        ),
        Inst::Br(target, args) => format!("br {}", branch_target(*target, args)),
        Inst::CondBr(condition, true_target, true_args, false_target, false_args) => format!(
            "cond_br {}, {}, {}",
            value(condition),
            branch_target(*true_target, true_args),
            branch_target(*false_target, false_args)
        ),
        Inst::Ret(returned) => format!("ret {}", values(returned)),
        Inst::Trap(reason) => format!("trap.{}", reason.mnemonic()),
        Inst::Load {
            result,
            address,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => format!(
            "({}, {}, {}) = load {}, {}, {} : {}",
            value(result),
            value(mem_out),
            value(world_out),
            value(mem_in),
            value(world_in),
            value(address),
            render_type(&result.ty)
        ),
        Inst::Store {
            address,
            value: stored,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => format!(
            "({}, {}) = store {}, {}, {}, {}",
            value(mem_out),
            value(world_out),
            value(mem_in),
            value(world_in),
            value(address),
            value(stored)
        ),
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            mem_out,
            world_out,
            layout,
            volatile,
        } => format!(
            "({}, {}) = copy{} {}, {}, {}, {} [size={}, align={}]",
            value(mem_out),
            value(world_out),
            volatile.suffix(),
            value(mem_in),
            value(world_in),
            value(destination),
            value(source),
            layout.size_bytes,
            layout.align_bytes
        ),
        Inst::PtrOffset {
            result,
            base,
            index,
            direction,
            signedness,
            scale,
        } => format!(
            "{} = ptr.offset.{direction:?}.{signedness:?} {}, {} [scale={scale}]",
            value(result),
            value(base),
            value(index)
        ),
        Inst::PtrNull(result) => format!("{} = ptr.null", value(result)),
        Inst::GlobalAddr { result, symbol } => {
            format!("{} = global_addr @{symbol}", value(result))
        }
        Inst::FuncAddr { result, symbol } => {
            format!("{} = func_addr @{symbol}", value(result))
        }
        Inst::BlockAddr { result, block } => {
            format!("{} = block_addr ^b{block}", value(result))
        }
        Inst::Alloca { .. }
        | Inst::DynamicAlloca { .. }
        | Inst::LifetimeEnd { .. }
        | Inst::PointerValid { .. }
        | Inst::AccessInBounds { .. } => "<day7-memory>".to_owned(),
        Inst::Call {
            results,
            callee,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => {
            let mut rendered_results = results.iter().map(value).collect::<Vec<_>>();
            rendered_results.push(value(mem_out));
            rendered_results.push(value(world_out));
            let mut rendered_args = args.iter().map(value).collect::<Vec<_>>();
            rendered_args.push(value(mem_in));
            rendered_args.push(value(world_in));
            format!(
                "({}) = call @{callee}({})",
                rendered_results.join(", "),
                rendered_args.join(", ")
            )
        }
        Inst::IndirectCall {
            results,
            callee,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => {
            let mut rendered_results = results.iter().map(value).collect::<Vec<_>>();
            rendered_results.push(value(mem_out));
            rendered_results.push(value(world_out));
            let mut rendered_args = args.iter().map(value).collect::<Vec<_>>();
            rendered_args.push(value(mem_in));
            rendered_args.push(value(world_in));
            format!(
                "({}) = call {}({})",
                rendered_results.join(", "),
                value(callee),
                rendered_args.join(", ")
            )
        }
        Inst::SExt(result, source, ty) => format!(
            "{} = sext.{} {}",
            value(result),
            render_type(ty),
            value(source)
        ),
        Inst::ZExt(result, source, ty) => format!(
            "{} = zext.{} {}",
            value(result),
            render_type(ty),
            value(source)
        ),
        Inst::Trunc(result, source, ty) => format!(
            "{} = trunc.{} {}",
            value(result),
            render_type(ty),
            value(source)
        ),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct IndependentEffectSummary {
    reads_all: bool,
    writes_all: bool,
    volatile: bool,
    atomic: bool,
    io: bool,
    may_trap: bool,
    may_unwind: bool,
}

impl IndependentEffectSummary {
    const fn top() -> Self {
        Self {
            reads_all: true,
            writes_all: true,
            volatile: true,
            atomic: true,
            io: true,
            may_trap: true,
            may_unwind: true,
        }
    }

    fn join(&mut self, other: Self) {
        self.reads_all |= other.reads_all;
        self.writes_all |= other.writes_all;
        self.volatile |= other.volatile;
        self.atomic |= other.atomic;
        self.io |= other.io;
        self.may_trap |= other.may_trap;
        self.may_unwind |= other.may_unwind;
    }
}

fn independent_local_effects(function: &Function) -> IndependentEffectSummary {
    let mut summary = IndependentEffectSummary::default();
    for inst in function
        .blocks
        .iter()
        .flat_map(|block| block.insts.iter().chain(std::iter::once(&block.terminator)))
    {
        let local = match inst {
            Inst::SDiv(..)
            | Inst::UDiv(..)
            | Inst::SRem(..)
            | Inst::URem(..)
            | Inst::UbAssume(..)
            | Inst::PtrOffset { .. }
            | Inst::Trap(..) => IndependentEffectSummary {
                may_trap: true,
                ..IndependentEffectSummary::default()
            },
            Inst::Alloca { .. } | Inst::DynamicAlloca { .. } | Inst::LifetimeEnd { .. } => {
                IndependentEffectSummary {
                    reads_all: true,
                    writes_all: true,
                    may_trap: true,
                    ..IndependentEffectSummary::default()
                }
            }
            Inst::PointerValid { .. } | Inst::AccessInBounds { .. } => IndependentEffectSummary {
                reads_all: true,
                may_trap: true,
                ..IndependentEffectSummary::default()
            },
            Inst::Load { volatile, .. } => IndependentEffectSummary {
                reads_all: true,
                volatile: *volatile,
                may_trap: true,
                ..IndependentEffectSummary::default()
            },
            Inst::Store { volatile, .. } => IndependentEffectSummary {
                writes_all: true,
                volatile: *volatile,
                may_trap: true,
                ..IndependentEffectSummary::default()
            },
            _ => IndependentEffectSummary::default(),
        };
        summary.join(local);
    }
    summary
}

fn independent_effects(functions: &[Function]) -> Vec<IndependentEffectSummary> {
    let positions = functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.name.as_str(), index))
        .collect::<HashMap<_, _>>();
    let local = functions
        .iter()
        .map(independent_local_effects)
        .collect::<Vec<_>>();
    let mut summaries = local.clone();
    loop {
        let previous = summaries.clone();
        for (index, function) in functions.iter().enumerate() {
            let mut summary = local[index];
            for inst in function.blocks.iter().flat_map(|block| &block.insts) {
                if let Inst::Call { callee, .. } = inst {
                    summary.join(
                        positions
                            .get(callee.as_str())
                            .map_or(IndependentEffectSummary::top(), |callee| previous[*callee]),
                    );
                }
            }
            summaries[index] = summary;
        }
        if summaries == previous {
            return summaries;
        }
    }
}

fn render_function(function: &Function, effects: IndependentEffectSummary) -> String {
    let mut rendered = String::new();
    let parameters = function
        .params
        .iter()
        .enumerate()
        .map(|(index, parameter)| format!("%arg{index}: {}", render_type(&parameter.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let returns = function
        .ret_tys
        .iter()
        .map(render_type)
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(
        rendered,
        "func @{}({parameters}) -> ({returns}) cc = target_default",
        function.name
    )
    .unwrap();
    writeln!(
        rendered,
        "    effects {{ reads={}, writes={}, volatile={}, atomic={}, io={}, may_trap={}, may_unwind={} }}",
        if effects.reads_all { "{all}" } else { "{}" },
        if effects.writes_all { "{all}" } else { "{}" },
        effects.volatile,
        effects.atomic,
        effects.io,
        effects.may_trap,
        effects.may_unwind,
    )
    .unwrap();
    rendered.push_str("{\n");
    for (block_index, block) in function.blocks.iter().enumerate() {
        if block_index != 0 {
            rendered.push('\n');
        }
        let arguments = block
            .args
            .iter()
            .map(|argument| {
                format!(
                    "{}: {}",
                    value(&argument.value),
                    render_type(&argument.value.ty)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(rendered, "{}({arguments}):", block_name(block.id)).unwrap();
        for instruction in &block.insts {
            writeln!(rendered, "    {}", render_inst(instruction)).unwrap();
        }
        writeln!(rendered, "    {}", render_inst(&block.terminator)).unwrap();
    }
    rendered.push_str("}\n");
    rendered
}

fn render_unit(functions: &[Function]) -> String {
    let summaries = independent_effects(functions);
    functions
        .iter()
        .zip(summaries)
        .map(|(function, effects)| render_function(function, effects))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parenthesized_count(line: &str) -> usize {
    let Some(open) = line.find('(') else {
        return 0;
    };
    let close = line[open + 1..]
        .find(')')
        .map(|offset| open + 1 + offset)
        .expect("golden line has a closing parenthesis");
    let contents = line[open + 1..close].trim();
    if contents.is_empty() {
        0
    } else {
        contents.split(',').count()
    }
}

fn percent_names(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut names = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        names.push(text[start..index].to_owned());
    }
    names
}

#[derive(Debug)]
struct GoldenBlock<'text> {
    label: String,
    args: Vec<(String, String)>,
    instructions: Vec<&'text str>,
}

#[derive(Clone, Debug)]
struct GoldenSignature {
    params: Vec<String>,
    returns: Vec<String>,
}

fn typed_values(line: &str) -> Result<Vec<(String, String)>, String> {
    let open = line
        .find('(')
        .ok_or_else(|| format!("missing `(` in {line:?}"))?;
    let close = line[open + 1..]
        .find(')')
        .map(|offset| open + 1 + offset)
        .ok_or_else(|| format!("missing `)` in {line:?}"))?;
    let contents = line[open + 1..close].trim();
    if contents.is_empty() {
        return Ok(Vec::new());
    }
    contents
        .split(',')
        .map(|item| {
            let (name, ty) = item
                .trim()
                .split_once(':')
                .ok_or_else(|| format!("missing type in {item:?}"))?;
            Ok((name.trim().to_owned(), ty.trim().to_owned()))
        })
        .collect()
}

fn parse_golden_blocks(function: &str) -> Result<Vec<GoldenBlock<'_>>, String> {
    let mut blocks = Vec::new();
    let mut current: Option<GoldenBlock<'_>> = None;
    for line in function.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() || line == "{" || line == "}" || line.starts_with("effects { ") {
            continue;
        }
        if line.starts_with('^') {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            let label_end = line.find(['(', ':']).unwrap_or(line.len());
            current = Some(GoldenBlock {
                label: line[..label_end].to_owned(),
                args: if line.contains('(') {
                    typed_values(line)?
                } else {
                    Vec::new()
                },
                instructions: Vec::new(),
            });
        } else {
            current
                .as_mut()
                .ok_or_else(|| format!("instruction before first block: {line}"))?
                .instructions
                .push(line);
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    Ok(blocks)
}

fn inferred_definition_type(rhs: &str) -> Option<String> {
    let operation = rhs.split_whitespace().next()?;
    for prefix in ["sext.", "zext.", "trunc."] {
        if let Some(ty) = operation.strip_prefix(prefix) {
            return Some(ty.to_owned());
        }
    }
    if let Some(ty) = operation.strip_prefix("const.") {
        return Some(ty.to_owned());
    }
    if let Some(ty) = operation.strip_prefix("uninitialized.") {
        return Some(ty.to_owned());
    }
    for prefix in [
        "add.", "sub.", "mul.", "sdiv.", "udiv.", "srem.", "urem.", "shl.", "ashr.", "lshr.",
        "and.", "or.", "xor.",
    ] {
        if let Some(ty) = operation.strip_prefix(prefix) {
            return Some(ty.to_owned());
        }
    }
    if operation.starts_with("icmp.")
        || operation.ends_with("_defined")
        || operation.starts_with("shift_count_defined.")
        || operation.starts_with("signed_shl_defined.")
        || operation == "divisor_nonzero"
        || operation == "signed_div_rem_no_overflow"
    {
        return Some("bits<1>".to_owned());
    }
    None
}

fn function_name(signature: &str) -> Result<String, String> {
    signature
        .strip_prefix("func @")
        .and_then(|rest| rest.split_once('(').map(|(name, _)| name.to_owned()))
        .ok_or_else(|| format!("malformed function signature: {signature}"))
}

fn return_types(signature: &str) -> Result<Vec<String>, String> {
    let after_arrow = signature
        .split_once("-> (")
        .map(|(_, returns)| returns)
        .ok_or_else(|| format!("missing function return list: {signature}"))?;
    let close = after_arrow
        .find(')')
        .ok_or_else(|| format!("unterminated function return list: {signature}"))?;
    let returns = after_arrow[..close].trim();
    if returns.is_empty() {
        Ok(Vec::new())
    } else {
        Ok(returns.split(',').map(|ty| ty.trim().to_owned()).collect())
    }
}

fn is_token_type(ty: &str) -> bool {
    ty.starts_with("mem<") || ty == "world"
}

fn parse_call(rhs: &str) -> Result<Option<(String, Vec<String>)>, String> {
    let Some(call) = rhs.strip_prefix("call @") else {
        return Ok(None);
    };
    let (callee, arguments) = call
        .split_once('(')
        .ok_or_else(|| format!("malformed call: {rhs}"))?;
    let arguments = arguments
        .strip_suffix(')')
        .ok_or_else(|| format!("unterminated call: {rhs}"))?;
    Ok(Some((callee.to_owned(), percent_names(arguments))))
}

fn branch_targets(line: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let bytes = line.as_bytes();
    let mut targets = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'^' {
            index += 1;
            continue;
        }
        let label_start = index;
        index += 1;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        let label = line[label_start..index].to_owned();
        if bytes.get(index) != Some(&b'(') {
            return Err(format!("branch target {label} has no argument list"));
        }
        let args_start = index + 1;
        let args_end = line[args_start..]
            .find(')')
            .map(|offset| args_start + offset)
            .ok_or_else(|| format!("branch target {label} has no closing `)`"))?;
        targets.push((label, percent_names(&line[args_start..args_end])));
        index = args_end + 1;
    }
    Ok(targets)
}

fn validate_golden_function(
    function: &str,
    signatures: &BTreeMap<String, GoldenSignature>,
) -> Result<(), String> {
    let signature = function
        .lines()
        .next()
        .ok_or_else(|| "missing function signature".to_owned())?;
    let name = function_name(signature)?;
    let own_signature = signatures
        .get(&name)
        .ok_or_else(|| format!("missing indexed signature for @{name}"))?;
    let signature_args = typed_values(signature)?;
    let blocks = parse_golden_blocks(function)?;
    let entry = blocks
        .first()
        .ok_or_else(|| "function has no blocks".to_owned())?;
    if entry.label != "^entry" {
        return Err(format!("first block is {}, not ^entry", entry.label));
    }
    let signature_types = signature_args.iter().map(|(_, ty)| ty).collect::<Vec<_>>();
    let entry_types = entry.args.iter().map(|(_, ty)| ty).collect::<Vec<_>>();
    if signature_types != entry_types {
        return Err(format!(
            "signature/entry type mismatch: {signature_types:?} != {entry_types:?}"
        ));
    }

    let block_by_label = blocks
        .iter()
        .map(|block| (block.label.clone(), block))
        .collect::<BTreeMap<_, _>>();
    if block_by_label.len() != blocks.len() {
        return Err("duplicate block label".to_owned());
    }

    let mut global_definitions = BTreeSet::new();
    let mut edges = Vec::new();
    for block in &blocks {
        let terminator = block
            .instructions
            .last()
            .ok_or_else(|| format!("{} has no terminator", block.label))?;
        if !(terminator.starts_with("br ")
            || terminator.starts_with("cond_br ")
            || terminator.starts_with("ret "))
        {
            return Err(format!("{} has no final terminator", block.label));
        }
        if block.instructions[..block.instructions.len() - 1]
            .iter()
            .any(|line| {
                line.starts_with("br ") || line.starts_with("cond_br ") || line.starts_with("ret ")
            })
        {
            return Err(format!("{} has a non-final terminator", block.label));
        }

        let mut local_definitions = BTreeMap::new();
        let mut current_tokens = BTreeMap::new();
        for (name, ty) in &block.args {
            let numeric_id = name.strip_prefix("%v").is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
            });
            if !numeric_id {
                return Err(format!("block argument {name} has no explicit Value ID"));
            }
            if !global_definitions.insert(name.clone()) {
                return Err(format!("Value {name} has multiple definition sites"));
            }
            local_definitions.insert(name.clone(), ty.clone());
            if is_token_type(ty) && current_tokens.insert(ty.clone(), name.clone()).is_some() {
                return Err(format!(
                    "{} has more than one live token of type {ty}",
                    block.label
                ));
            }
        }

        for line in &block.instructions {
            let (definitions, rhs) = if let Some((lhs, rhs)) = line.split_once(" = ") {
                (percent_names(lhs), rhs)
            } else {
                (Vec::new(), *line)
            };
            for used in percent_names(rhs) {
                if !local_definitions.contains_key(&used) {
                    return Err(format!(
                        "{} uses {used} without a preceding local definition or block argument",
                        block.label
                    ));
                }
            }
            if let Some(assumption) = line.strip_prefix("ub.assume.") {
                let (reason, condition) = assumption
                    .split_once(' ')
                    .ok_or_else(|| format!("{} has malformed ub.assume", block.label))?;
                if !matches!(
                    reason,
                    "division_by_zero"
                        | "remainder_by_zero"
                        | "signed_overflow"
                        | "invalid_shift_count"
                        | "invalid_signed_left_shift"
                ) {
                    return Err(format!(
                        "{} has unknown ub.assume reason {reason}",
                        block.label
                    ));
                }
                let condition = percent_names(condition);
                if condition.len() != 1
                    || local_definitions.get(&condition[0]).map(String::as_str) != Some("bits<1>")
                {
                    return Err(format!("{} has ill-typed ub.assume", block.label));
                }
            }
            if let Some(condition) = line.strip_prefix("cond_br ") {
                let condition = percent_names(condition)
                    .first()
                    .cloned()
                    .ok_or_else(|| format!("{} has conditionless cond_br", block.label))?;
                if local_definitions.get(&condition).map(String::as_str) != Some("bits<1>") {
                    return Err(format!(
                        "{} cond_br condition {condition} is not bits<1>",
                        block.label
                    ));
                }
            }

            let definition_types = if let Some((callee, arguments)) = parse_call(rhs)? {
                let callee_signature = signatures
                    .get(&callee)
                    .ok_or_else(|| format!("call targets unknown function @{callee}"))?;
                let argument_types = arguments
                    .iter()
                    .map(|argument| {
                        local_definitions.get(argument).cloned().ok_or_else(|| {
                            format!("call @{callee} uses unknown argument {argument}")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if argument_types != callee_signature.params {
                    return Err(format!(
                        "call @{callee} argument types {argument_types:?} do not match {:?}",
                        callee_signature.params
                    ));
                }
                if definitions.len() != callee_signature.returns.len() {
                    return Err(format!(
                        "call @{callee} defines {} results, expected {}",
                        definitions.len(),
                        callee_signature.returns.len()
                    ));
                }
                for (argument, ty) in arguments.iter().zip(argument_types.iter()) {
                    if is_token_type(ty)
                        && current_tokens.get(ty).map(String::as_str) != Some(argument.as_str())
                    {
                        return Err(format!(
                            "call @{callee} consumes stale {ty} token {argument}"
                        ));
                    }
                }
                callee_signature.returns.clone()
            } else {
                let inferred = inferred_definition_type(rhs);
                if definitions.len() > 1 {
                    return Err(format!("non-call instruction has multiple results: {line}"));
                }
                definitions
                    .iter()
                    .map(|_| inferred.clone().unwrap_or_else(|| "<opaque>".to_owned()))
                    .collect()
            };

            for (defined, ty) in definitions.into_iter().zip(definition_types) {
                let numeric_id = defined.strip_prefix("%v").is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
                });
                if !numeric_id {
                    return Err(format!("definition {defined} has no explicit Value ID"));
                }
                if !global_definitions.insert(defined.clone()) {
                    return Err(format!("Value {defined} has multiple definition sites"));
                }
                local_definitions.insert(defined.clone(), ty.clone());
                if is_token_type(&ty) {
                    current_tokens.insert(ty, defined);
                }
            }
        }

        if let Some(returned) = terminator.strip_prefix("ret ") {
            let returned = percent_names(returned);
            let returned_types = returned
                .iter()
                .map(|value| {
                    local_definitions
                        .get(value)
                        .cloned()
                        .ok_or_else(|| format!("return uses unknown value {value}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if returned_types != own_signature.returns {
                return Err(format!(
                    "@{name} return types {returned_types:?} do not match {:?}",
                    own_signature.returns
                ));
            }
            for (returned, ty) in returned.iter().zip(returned_types.iter()) {
                if is_token_type(ty)
                    && current_tokens.get(ty).map(String::as_str) != Some(returned.as_str())
                {
                    return Err(format!("@{name} returns stale {ty} token {returned}"));
                }
            }
        }

        for (target, arguments) in branch_targets(terminator)? {
            let argument_types = arguments
                .iter()
                .map(|argument| {
                    local_definitions
                        .get(argument)
                        .cloned()
                        .ok_or_else(|| format!("edge from {} uses unknown {argument}", block.label))
                })
                .collect::<Result<Vec<_>, _>>()?;
            for (ty, current) in &current_tokens {
                let passed = arguments
                    .iter()
                    .zip(argument_types.iter())
                    .any(|(argument, argument_type)| argument == current && argument_type == ty);
                if !passed {
                    return Err(format!(
                        "edge {} -> {target} does not pass current {ty} token {current}",
                        block.label
                    ));
                }
            }
            edges.push((block.label.clone(), target, arguments, argument_types));
        }
    }

    let mut successors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (source, target, arguments, argument_types) in &edges {
        let target_block = block_by_label
            .get(target)
            .ok_or_else(|| format!("edge from {source} targets missing block {target}"))?;
        if arguments.len() != target_block.args.len() {
            return Err(format!(
                "edge {source} -> {target} has arity {}, expected {}",
                arguments.len(),
                target_block.args.len()
            ));
        }
        for ((_, target_type), argument_type) in target_block.args.iter().zip(argument_types.iter())
        {
            if argument_type == "<opaque>" || argument_type != target_type {
                return Err(format!(
                    "edge {source} -> {target} type mismatch: {argument_type} != {target_type}"
                ));
            }
        }
        successors
            .entry(source.clone())
            .or_default()
            .push(target.clone());
    }

    let mut reachable = BTreeSet::from(["^entry".to_owned()]);
    let mut frontier = vec!["^entry".to_owned()];
    while let Some(block) = frontier.pop() {
        for successor in successors.get(&block).into_iter().flatten() {
            if reachable.insert(successor.clone()) {
                frontier.push(successor.clone());
            }
        }
    }
    if reachable.len() != blocks.len() {
        return Err(format!(
            "unreachable blocks: expected {}, reached {}",
            blocks.len(),
            reachable.len()
        ));
    }
    Ok(())
}

fn validate_golden_text(golden: &str) -> Result<(), String> {
    let functions = golden
        .split("\n\nfunc @")
        .enumerate()
        .map(|(index, function)| {
            if index == 0 {
                function.to_owned()
            } else {
                format!("func @{function}")
            }
        })
        .collect::<Vec<_>>();
    let mut signatures = BTreeMap::new();
    for function in &functions {
        let signature = function
            .lines()
            .next()
            .ok_or_else(|| "missing function signature".to_owned())?;
        let name = function_name(signature)?;
        let params = typed_values(signature)?
            .into_iter()
            .map(|(_, ty)| ty)
            .collect();
        let indexed = GoldenSignature {
            params,
            returns: return_types(signature)?,
        };
        if signatures.insert(name.clone(), indexed).is_some() {
            return Err(format!("duplicate function @{name}"));
        }
    }
    for function in &functions {
        validate_golden_function(function, &signatures)?;
    }
    Ok(())
}

fn assert_golden_ruling(case: &Case) {
    let mut function_count = 0;
    for function in case.golden_evoir.split("\n\nfunc @") {
        let function = function.strip_prefix("func @").unwrap_or(function);
        let signature = function.lines().next().expect("golden function signature");
        let entry = function
            .lines()
            .find(|line| line.trim_start().starts_with("^entry"))
            .expect("golden entry block");
        let signature_count = parenthesized_count(signature);
        let entry_count = parenthesized_count(entry);
        assert_eq!(
            entry_count, signature_count,
            "{}: entry args must correspond 1:1 with signature params",
            case.id
        );

        let signature_binders = percent_names(signature);
        let body = function
            .split_once('{')
            .map(|(_, body)| body)
            .expect("golden function body");
        let body_names = percent_names(body).into_iter().collect::<BTreeSet<_>>();
        for binder in signature_binders {
            assert!(
                !body_names.contains(&binder),
                "{}: non-SSA signature binder {binder} leaked into the CFG",
                case.id
            );
        }

        let mut block_arguments = BTreeSet::new();
        for block_line in function
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('^') && line.contains('('))
        {
            let open = block_line.find('(').unwrap();
            let close = block_line[open + 1..].find(')').unwrap() + open + 1;
            for name in percent_names(&block_line[open + 1..close]) {
                assert!(
                    block_arguments.insert(name.clone()),
                    "{}: block argument {name} is not function-unique",
                    case.id
                );
            }
        }
        function_count += 1;
    }
    assert!(function_count >= 1, "{}: no golden functions", case.id);
    validate_golden_text(case.golden_evoir)
        .unwrap_or_else(|error| panic!("{}: invalid golden SSA: {error}", case.id));
}

#[test]
fn golden_ssa_guard_rejects_false_green_ruling_violations() {
    let wrong_entry_type =
        CASES[6]
            .golden_evoir
            .replacen("^entry(%v0: bits<32>", "^entry(%v0: bits<64>", 1);
    assert!(
        validate_golden_text(&wrong_entry_type).is_err(),
        "entry block arguments must match signature parameter types in order"
    );

    let cross_block_use = CASES[1]
        .golden_evoir
        .replacen("    ret %v4", "    ret %v3", 1);
    assert!(
        validate_golden_text(&cross_block_use).is_err(),
        "a value from another block must be passed through a block argument"
    );

    let wrong_edge_arity =
        CASES[3]
            .golden_evoir
            .replacen("br ^b1(%v0, %v1, %v2)", "br ^b1(%v0, %v1)", 1);
    assert!(
        validate_golden_text(&wrong_edge_arity).is_err(),
        "edge arity must match the target block arguments"
    );

    let wrong_edge_type =
        CASES[1]
            .golden_evoir
            .replacen("^b2(%v4: bits<32>)", "^b2(%v4: bits<1>)", 1);
    assert!(
        validate_golden_text(&wrong_edge_type).is_err(),
        "edge operand types must match target block argument types"
    );

    let wrong_call_arity =
        CASES[6]
            .golden_evoir
            .replacen("call @add1(%v2, %v0, %v1)", "call @add1(%v2, %v0)", 1);
    assert!(
        validate_golden_text(&wrong_call_arity).is_err(),
        "call operands must match the callee signature"
    );

    let stale_recursive_tokens = CASES[7].golden_evoir.replacen(
        "call @fib(%v19, %v15, %v16)",
        "call @fib(%v19, %v9, %v10)",
        1,
    );
    assert!(
        validate_golden_text(&stale_recursive_tokens).is_err(),
        "a second call must consume the first call's current mem/world tokens"
    );

    let dropped_return_token =
        CASES[6]
            .golden_evoir
            .replacen("ret %v3, %v4, %v5", "ret %v3, %v4", 1);
    assert!(
        validate_golden_text(&dropped_return_token).is_err(),
        "return arity and token results must match the function signature"
    );
}

#[test]
fn ruling_compliant_goldens_are_frozen_for_eight_unique_programs() {
    assert_eq!(
        CASES.len(),
        8,
        "the Day 6 RED denominator is frozen at eight"
    );
    let mut ids = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut goldens = BTreeSet::new();
    for case in CASES {
        assert!(ids.insert(case.id), "duplicate case ID {}", case.id);
        assert!(
            sources.insert(case.source),
            "duplicate source for {}",
            case.id
        );
        assert!(
            goldens.insert(case.golden_evoir),
            "duplicate golden for {}",
            case.id
        );
        assert_golden_ruling(case);
    }
    let mut step_proofs = Vec::new();
    for extended in EXTENDED_CASES {
        let case = &extended.case;
        assert!(
            ids.insert(case.id),
            "extended case ID is duplicated: {}",
            case.id
        );
        assert!(
            sources.insert(case.source),
            "extended source is duplicated: {}",
            case.id
        );
        assert!(
            goldens.insert(case.golden_evoir),
            "extended golden is duplicated: {}",
            case.id
        );
        assert_golden_ruling(case);
        if let Some(steps) = extended.expected_evoir_steps {
            step_proofs.push((case.id, steps));
        }
    }
    assert_eq!(ids.len(), 12, "combined proof-set ID denominator changed");
    assert_eq!(
        sources.len(),
        12,
        "combined proof-set source denominator changed"
    );
    assert_eq!(
        goldens.len(),
        12,
        "combined proof-set golden denominator changed"
    );
    assert_eq!(
        step_proofs,
        vec![("void_helper_expression_statement", 9)],
        "exact execution-step proof set changed"
    );
}

#[test]
fn block_arguments_must_expose_real_value_identity() {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("evocc-lower lives under workspace/crates")
        .to_owned();
    let probe = std::env::temp_dir().join(format!(
        "c30-day6-block-arg-value-id-{}",
        std::process::id()
    ));
    assert!(!probe.exists(), "block-arg API probe path already exists");
    std::fs::create_dir_all(probe.join("src")).expect("create block-arg API probe");
    let manifest = format!(
        r#"[package]
name = "day6-block-arg-value-id-probe"
version = "0.0.0"
edition = "2021"

[workspace]

[dependencies]
evocc-evoir = {{ path = {:?} }}
evocc-types = {{ path = {:?} }}
"#,
        workspace.join("crates/evocc-evoir"),
        workspace.join("crates/evocc-types")
    );
    std::fs::write(probe.join("Cargo.toml"), manifest).expect("write probe manifest");
    std::fs::write(
        probe.join("src/main.rs"),
        r#"use evocc_evoir::{validate_ssa, Block, BlockArg, Function, Inst, Value};
use evocc_types::EvoIRType;

fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}

fn main() {
    let bits1 = EvoIRType::Bits(1);
    let bits32 = EvoIRType::Bits(32);
    let condition = value(0, bits1.clone());
    let payload = value(1, bits32.clone());
    let left = value(2, bits32.clone());
    let right = value(3, bits32.clone());
    let valid = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        variadic: false,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        name: "block_arg_identity".to_owned(),
        params: Vec::new(),
        param_marks: Vec::new(),
        ret_tys: vec![bits32.clone()],
        effects: Default::default(),
        blocks: vec![
            Block {
                id: 0,
                args: Vec::new(),
                insts: vec![
                    Inst::Const(condition.clone(), 1),
                    Inst::Const(payload.clone(), 7),
                ],
                terminator: Inst::CondBr(
                    condition.clone(),
                    1,
                    vec![payload.clone()],
                    2,
                    vec![payload.clone()],
                ),
            },
            Block {
                id: 1,
                args: vec![BlockArg {
                    name: "left".to_owned(),
                    value: left.clone(),
                }],
                insts: Vec::new(),
                terminator: Inst::Ret(vec![left.clone()]),
            },
            Block {
                id: 2,
                args: vec![BlockArg {
                    name: "right".to_owned(),
                    value: right.clone(),
                }],
                insts: Vec::new(),
                terminator: Inst::Ret(vec![right]),
            },
        ],
    };
    assert!(validate_ssa(&valid).is_ok(), "distinct block arguments are valid");

    let mut duplicate = valid.clone();
    duplicate.blocks[2].args[0].value = left.clone();
    duplicate.blocks[2].terminator = Inst::Ret(vec![left]);
    assert!(validate_ssa(&duplicate).is_err(), "duplicate block-arg Value IDs must fail");

    let mut cross_block = valid.clone();
    cross_block.blocks[1].terminator = Inst::Ret(vec![payload.clone()]);
    assert!(validate_ssa(&cross_block).is_err(), "direct cross-block use must fail");

    let mut wrong_arity = valid.clone();
    wrong_arity.blocks[0].terminator =
        Inst::CondBr(condition.clone(), 1, Vec::new(), 2, vec![payload.clone()]);
    assert!(validate_ssa(&wrong_arity).is_err(), "wrong edge arity must fail");

    let mut wrong_type = valid;
    wrong_type.blocks[0].terminator =
        Inst::CondBr(condition.clone(), 1, vec![condition], 2, vec![payload]);
    assert!(validate_ssa(&wrong_type).is_err(), "wrong edge type must fail");
}
"#,
    )
    .expect("write probe source");

    let output =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .arg("--config")
            .arg(workspace.join(".cargo/config.toml"))
            .args(["run", "--offline", "--quiet"])
            .current_dir(&probe)
            .env("CARGO_TARGET_DIR", probe.join("target"))
            .output()
            .expect("run block-arg API compile probe");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    std::fs::remove_dir_all(&probe).expect("clean block-arg API probe");

    assert!(
        output.status.success(),
        "BlockArg plus validate_ssa must enforce real Value identity, local use-def, and edge types/arity instead of synthesizing identity from an index:\n{stderr}"
    );
}

#[derive(Debug)]
struct CaseObservation {
    chir: Result<i128, String>,
    evoir: Result<i128, String>,
    evoir_steps: Result<u64, String>,
    validation: Result<(), String>,
    actual_evoir: String,
}

fn run_case(case: &Case) -> Result<CaseObservation, String> {
    let arena = Arena::default();
    let unit = parse(case.source, &arena)?;
    let chir = observe_chir(&unit);
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .map_err(|error| error.to_string())?;
    let functions = module.functions.clone();
    let actual_evoir = evocc_evoir::render_module(&module);
    let independent_render = render_unit(&functions);
    if independent_render != actual_evoir {
        return Err("production and independent EvoIR renderers disagree".to_owned());
    }
    let validation = evocc_evoir::validate_module(&module);
    let execution = observe_evoir(&functions);
    let (evoir, evoir_steps) = match execution {
        Ok(execution) => (Ok(execution.value), Ok(execution.steps)),
        Err(error) => (Err(error.clone()), Err(error)),
    };
    Ok(CaseObservation {
        chir,
        evoir,
        evoir_steps,
        validation,
        actual_evoir,
    })
}

#[test]
fn additional_day6_lowering_constructs_have_golden_and_execution_proofs() {
    assert_eq!(
        EXTENDED_CASES.len(),
        4,
        "extended proof denominator changed"
    );
    for extended in EXTENDED_CASES {
        let case = &extended.case;
        let observation = run_case(case)
            .unwrap_or_else(|error| panic!("{}: lowering pipeline failed: {error}", case.id));
        assert_eq!(observation.chir, Ok(case.expected), "{}: C-HIR", case.id);
        assert_eq!(observation.evoir, Ok(case.expected), "{}: EvoIR", case.id);
        assert_eq!(
            observation.validation,
            Ok(()),
            "{}: generated EvoIR must validate",
            case.id
        );
        assert_eq!(
            observation.actual_evoir, case.golden_evoir,
            "{}: exact EvoIR golden mismatch; steps={:?}",
            case.id, observation.evoir_steps
        );
        if let Some(expected_steps) = extended.expected_evoir_steps {
            assert_eq!(
                observation.evoir_steps,
                Ok(expected_steps),
                "{}: exact execution proof changed",
                case.id
            );
        }
        assert_golden_ruling(case);
    }
}

#[test]
fn chir_and_evoir_must_agree_for_every_representative_program() {
    let mut semantic_failures = Vec::new();
    let mut golden_failures = Vec::new();
    let mut validation_failures = Vec::new();
    let mut pipeline_failures = Vec::new();
    let mut reference_failures = 0;

    for case in CASES {
        let observation = catch_unwind(AssertUnwindSafe(|| run_case(case)))
            .map_err(|_| "pipeline panicked".to_owned())
            .and_then(|result| result);
        let Ok(observation) = observation else {
            pipeline_failures.push(format!("case={}: {observation:?}", case.id));
            continue;
        };
        if observation.chir.is_err() {
            reference_failures += 1;
        }
        let expected = Ok(case.expected);
        if observation.chir != expected
            || observation.evoir != expected
            || observation.chir != observation.evoir
        {
            semantic_failures.push(format!(
                "case={} expected={} chir={:?} evoir={:?}",
                case.id, case.expected, observation.chir, observation.evoir
            ));
        }
        if let Err(error) = observation.validation {
            validation_failures.push(format!("case={}: {error}", case.id));
        }
        if observation.actual_evoir != case.golden_evoir {
            golden_failures.push(format!(
                "case={}\n--- expected EvoIR ---\n{}\n--- actual EvoIR ---\n{}",
                case.id, case.golden_evoir, observation.actual_evoir
            ));
        }
    }

    let semantic_passes = CASES.len() - semantic_failures.len() - pipeline_failures.len();
    let golden_passes = CASES.len() - golden_failures.len() - pipeline_failures.len();
    assert!(
        semantic_failures.is_empty()
            && golden_failures.is_empty()
            && validation_failures.is_empty()
            && pipeline_failures.is_empty(),
        "Day 6 RED: semantic_pass={semantic_passes}/{total} golden_pass={golden_passes}/{total} reference_failures={reference_failures} validation_failures={} pipeline_failures={}\n\n--- semantic mismatches ---\n{}\n\n--- golden mismatches ---\n{}\n\n--- validation failures ---\n{}\n\n--- pipeline failures ---\n{}",
        validation_failures.len(),
        pipeline_failures.len(),
        semantic_failures.join("\n"),
        golden_failures.join("\n\n"),
        validation_failures.join("\n"),
        pipeline_failures.join("\n"),
        total = CASES.len(),
    );
}
