use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits as CHirLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_evoir::{
    interpret_module, render_module, validate_module, InterpreterLimits as EvoIrLimits, Module,
    RuntimeValue,
};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_memory::{AccessLayout, ObjectOrigin, ObservedValue, VolatileDirection, VolatileEvent};
use evocc_parser::Parser;
use evocc_types::{EvoIRType, Region, TargetDataLayout};

const STEP_BUDGET: u64 = 500_000;
const CASE_WATCHDOG: Duration = Duration::from_secs(2);

const ORDERED_VOLATILE_IR_ACCESSES: &[VolatileDirection] = &[
    VolatileDirection::Store,
    VolatileDirection::Load,
    VolatileDirection::Store,
    VolatileDirection::Load,
];

#[derive(Debug, PartialEq, Eq)]
struct ExecutionObservation {
    scalar: i128,
    volatile_trace: Vec<VolatileEvent>,
}

#[derive(Clone, Copy)]
struct Case {
    id: &'static str,
    source: &'static str,
    expected: i128,
    expected_volatile_ir_accesses: Option<&'static [VolatileDirection]>,
}

#[derive(Clone, Copy)]
struct InvalidCHirCase {
    id: &'static str,
    source: &'static str,
    expected_code: &'static str,
    expected_message: &'static str,
}

const CASES: &[Case] = &[
    Case {
        id: "linked_list_build_and_traverse",
        source: "struct Node { int value; struct Node *next; }; int main(void) { struct Node a; struct Node b; struct Node c; a.value = 4; b.value = 5; c.value = 6; a.next = &b; b.next = &c; c.next = 0; struct Node *p = &a; int sum = 0; while (p) { sum += p->value; p = p->next; } return sum; }\n",
        expected: 15,
        expected_volatile_ir_accesses: None,
    },
    Case {
        id: "array_bubble_sort",
        source: "int main(void) { int a[5] = {5, 1, 4, 2, 3}; for (int i = 0; i < 5; i++) { for (int j = 0; j + 1 < 5 - i; j++) { if (a[j] > a[j + 1]) { int t = a[j]; a[j] = a[j + 1]; a[j + 1] = t; } } } return a[0] * 10000 + a[1] * 1000 + a[2] * 100 + a[3] * 10 + a[4]; }\n",
        expected: 12_345,
        expected_volatile_ir_accesses: None,
    },
    Case {
        id: "alias_write_then_read",
        source: "int main(void) { int x = 5; int *p = &x; int *q = p; *q = 9; return *p; }\n",
        expected: 9,
        expected_volatile_ir_accesses: None,
    },
    Case {
        id: "global_counter_across_calls",
        source: "int counter = 3; int bump(void) { counter += 2; return counter; } int main(void) { bump(); bump(); return counter; }\n",
        expected: 7,
        expected_volatile_ir_accesses: None,
    },
    Case {
        id: "string_byte_copy",
        source: "int main(void) { char src[5] = \"EVO!\"; char dst[5] = {0}; for (int i = 0; i < 5; i++) { dst[i] = src[i]; } return dst[0] == 'E' && dst[1] == 'V' && dst[2] == 'O' && dst[3] == '!' && dst[4] == 0; }\n",
        expected: 1,
        expected_volatile_ir_accesses: None,
    },
    Case {
        id: "volatile_load_store_order",
        source: "int main(void) { volatile int port; port = 11; int first = port; port = 22; int second = port; return first * 100 + second; }\n",
        expected: 1_122,
        expected_volatile_ir_accesses: Some(ORDERED_VOLATILE_IR_ACCESSES),
    },
];

const STRING_UNIT_CASES: &[Case] = &[
    Case {
        id: "string_fragment_hex_boundary",
        source: "int main(void) { char value[3] = \"\\x1\" \"2\"; return value[0] * 100 + value[1]; }\n",
        expected: 150,
        expected_volatile_ir_accesses: None,
    },
    Case {
        id: "string_ordinary_escapes",
        source: "int main(void) { char value[5] = \"\\n\\t\\\\\\\"\"; return value[0] + value[1] + value[2] + value[3] + value[4]; }\n",
        expected: 145,
        expected_volatile_ir_accesses: None,
    },
    Case {
        id: "string_embedded_and_terminating_zero",
        source: "int main(void) { char value[4] = \"A\\0B\"; return value[0] == 'A' && value[1] == 0 && value[2] == 'B' && value[3] == 0; }\n",
        expected: 1,
        expected_volatile_ir_accesses: None,
    },
];

const INVALID_CHIR_CASES: &[InvalidCHirCase] = &[
    InvalidCHirCase {
        id: "uninitialized_read",
        source: "int main(void) { int value; int *pointer = &value; return *pointer; }\n",
        expected_code: "E3001",
        expected_message: "read-of-uninitialized-object",
    },
    InvalidCHirCase {
        id: "null_pointer_access",
        source: "int main(void) { int *pointer = 0; return *pointer; }\n",
        expected_code: "E3009",
        expected_message: "null-pointer-access",
    },
    InvalidCHirCase {
        id: "out_of_bounds_access",
        source: "int main(void) { int value[1] = {7}; return value[1]; }\n",
        expected_code: "E3011",
        expected_message: "out-of-bounds-memory-access",
    },
    InvalidCHirCase {
        id: "dangling_pointer_access",
        source: "int main(void) { int *pointer; { int value = 7; pointer = &value; } return *pointer; }\n",
        expected_code: "E3010",
        expected_message: "dangling-pointer-access",
    },
];

fn parse<'arena>(
    source: &str,
    file: FileId,
    arena: &'arena Arena,
) -> Result<CHirUnit<'arena>, String> {
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err(format!(
                "lexer rejected source at {}:{}",
                token.span.line_start, token.span.column_start
            ));
        }
        tokens.push(token);
    }

    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    if let Some(diagnostic) = parser.diagnostic().filter(|_| parser.had_error()) {
        return Err(format!(
            "parser diagnostic {} at {}:{}: {}",
            diagnostic.code,
            diagnostic.span.line_start,
            diagnostic.span.column_start,
            diagnostic.message
        ));
    }
    if parser.had_error() {
        return Err("parser failed without a diagnostic".to_owned());
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

fn observe_chir(unit: &CHirUnit<'_>) -> Result<ExecutionObservation, String> {
    let main = function_symbol(unit, "main")?;
    let outcome = interpret_scalar_function(unit, main, CHirLimits::new(STEP_BUDGET))
        .map_err(|error| format!("{}: {}", error.code(), error.message()))?;
    let value = outcome
        .return_value
        .ok_or_else(|| "main returned no scalar value".to_owned())?;
    let scalar = value
        .mathematical_value(unit.types())
        .map_err(|error| error.to_string())?;
    Ok(ExecutionObservation {
        scalar,
        volatile_trace: outcome.world.volatile_trace().to_vec(),
    })
}

fn entry_contract(module: &Module) -> Result<(Vec<RuntimeValue>, Vec<EvoIRType>), String> {
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .ok_or_else(|| "lowering produced no @main".to_owned())?;
    let arguments = match main.params.as_slice() {
        [] => Vec::new(),
        [memory, world]
            if memory.ty == EvoIRType::Mem(Region::All) && world.ty == EvoIRType::World =>
        {
            vec![
                RuntimeValue::new(memory.ty.clone(), 0),
                RuntimeValue::new(world.ty.clone(), 0),
            ]
        }
        parameters => {
            return Err(format!(
                "@main parameters are not canonical: {:?}",
                parameters
                    .iter()
                    .map(|parameter| &parameter.ty)
                    .collect::<Vec<_>>()
            ));
        }
    };
    let has_effect_results = match main.ret_tys.as_slice() {
        [EvoIRType::Bits(1..=64)] => false,
        [EvoIRType::Bits(1..=64), EvoIRType::Mem(Region::All), EvoIRType::World] => true,
        results => return Err(format!("@main results are not canonical: {results:?}")),
    };
    if has_effect_results != !arguments.is_empty() {
        return Err("@main parameter/result effect interfaces disagree".to_owned());
    }
    Ok((arguments, main.ret_tys.clone()))
}

fn signed_scalar(value: &RuntimeValue) -> Result<i128, String> {
    let EvoIRType::Bits(width @ 1..=64) = value.ty else {
        return Err(format!("unsupported scalar result type: {:?}", value.ty));
    };
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    let raw = value.raw_bits & mask;
    let sign_bit = 1_u64 << (width - 1);
    Ok(if raw & sign_bit == 0 {
        i128::from(raw)
    } else {
        i128::from(raw) - (1_i128 << width)
    })
}

fn observe_evoir(module: &Module) -> Result<ExecutionObservation, String> {
    let (arguments, declared_results) = entry_contract(module)?;
    let outcome = interpret_module(module, "main", &arguments, EvoIrLimits::new(STEP_BUDGET))
        .map_err(|error| format!("{}: {}", error.code(), error.message()))?;
    if outcome.return_values.len() != declared_results.len()
        || outcome
            .return_values
            .iter()
            .zip(&declared_results)
            .any(|(value, declared)| &value.ty != declared)
    {
        return Err("@main result values do not match its declaration".to_owned());
    }
    let value = outcome
        .return_values
        .first()
        .ok_or_else(|| "@main returned no scalar value".to_owned())?;
    Ok(ExecutionObservation {
        scalar: signed_scalar(value)?,
        volatile_trace: outcome.world.volatile_trace().to_vec(),
    })
}

fn lowered_volatile_ir_accesses(module: &Module) -> Vec<VolatileDirection> {
    render_module(module)
        .lines()
        .filter_map(|line| {
            let line = line.to_ascii_lowercase();
            let marked_volatile =
                line.contains("volatile") || line.contains("vload") || line.contains("vstore");
            if !marked_volatile {
                None
            } else if line.contains("store") {
                Some(VolatileDirection::Store)
            } else if line.contains("load") {
                Some(VolatileDirection::Load)
            } else {
                None
            }
        })
        .collect()
}

fn expected_volatile_trace() -> Vec<VolatileEvent> {
    let origin = ObjectOrigin {
        // `port` is the first semantic storage object in this single-function
        // translation unit's canonical source-ordered allocation manifest.
        site: 0,
        instance: 0,
    };
    let access = AccessLayout::new(4, 4);
    [
        (VolatileDirection::Store, 11_u8),
        (VolatileDirection::Load, 11_u8),
        (VolatileDirection::Store, 22_u8),
        (VolatileDirection::Load, 22_u8),
    ]
    .into_iter()
    .enumerate()
    .map(|(sequence, (direction, value))| VolatileEvent {
        sequence: sequence as u64,
        direction,
        origin,
        byte_offset: 0,
        access,
        value: ObservedValue::RawTargetBits(vec![value, 0, 0, 0]),
    })
    .collect()
}

fn check_case(case: Case, ordinal: usize) -> Vec<String> {
    let arena = Arena::default();
    let file = FileId::new(7_001 + ordinal as u32);
    let unit = match parse(case.source, file, &arena) {
        Ok(unit) => unit,
        Err(error) => return vec![error],
    };

    let mut failures = Vec::new();
    let expected_trace = case
        .expected_volatile_ir_accesses
        .map_or_else(Vec::new, |_| expected_volatile_trace());
    let chir = match observe_chir(&unit) {
        Ok(actual) => {
            if actual.scalar != case.expected {
                failures.push(format!(
                    "C-HIR returned {}, expected {}",
                    actual.scalar, case.expected
                ));
            }
            if actual.volatile_trace != expected_trace {
                failures.push(format!(
                    "C-HIR volatile trace was {:?}, expected {:?}",
                    actual.volatile_trace, expected_trace
                ));
            }
            Some(actual)
        }
        Err(error) => {
            failures.push(format!("C-HIR failed: {error}"));
            None
        }
    };

    let module = match lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()) {
        Ok(module) => module,
        Err(error) => {
            failures.push(format!(
                "lowering failed: {}: {}",
                error.code(),
                error.message()
            ));
            return failures;
        }
    };

    if let Err(error) = validate_module(&module) {
        failures.push(format!("EvoIR validation failed: {error}"));
    }
    let evoir = match observe_evoir(&module) {
        Ok(actual) => {
            if actual.scalar != case.expected {
                failures.push(format!(
                    "EvoIR returned {}, expected {}",
                    actual.scalar, case.expected
                ));
            }
            if actual.volatile_trace != expected_trace {
                failures.push(format!(
                    "EvoIR volatile trace was {:?}, expected {:?}",
                    actual.volatile_trace, expected_trace
                ));
            }
            Some(actual)
        }
        Err(error) => {
            failures.push(format!("EvoIR failed: {error}"));
            None
        }
    };

    if let (Some(chir), Some(evoir)) = (&chir, &evoir) {
        if chir.volatile_trace != evoir.volatile_trace {
            failures.push(format!(
                "volatile trace mismatch: C-HIR={:?}, EvoIR={:?}",
                chir.volatile_trace, evoir.volatile_trace
            ));
        }
    }

    if let Some(expected) = case.expected_volatile_ir_accesses {
        let actual = lowered_volatile_ir_accesses(&module);
        if actual != expected {
            failures.push(format!(
                "volatile IR access order was {actual:?}, expected {expected:?}"
            ));
        }
    }
    failures
}

fn panic_payload(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn check_case_bounded(case: Case, ordinal: usize) -> Vec<String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let result =
            catch_unwind(AssertUnwindSafe(|| check_case(case, ordinal))).map_err(panic_payload);
        let _ = sender.send(result);
    });
    match receiver.recv_timeout(CASE_WATCHDOG) {
        Ok(Ok(failures)) => failures,
        Ok(Err(error)) => vec![format!("pipeline panicked: {error}")],
        Err(_) => vec![format!(
            "pipeline exceeded {CASE_WATCHDOG:?} wall-clock watchdog"
        )],
    }
}

#[test]
fn parsed_c_memory_programs_are_equivalent_in_both_interpreters() {
    let mut failures = Vec::new();
    if CASES.len() != 6 {
        failures.push(format!("case denominator is {}, expected 6", CASES.len()));
    }
    let unique_ids = CASES.iter().map(|case| case.id).collect::<BTreeSet<_>>();
    let unique_sources = CASES
        .iter()
        .map(|case| case.source.as_bytes())
        .collect::<BTreeSet<_>>();
    if unique_ids.len() != CASES.len() || unique_sources.len() != CASES.len() {
        failures.push("case IDs and source bytes must be unique".to_owned());
    }

    for (ordinal, case) in CASES.iter().copied().enumerate() {
        for failure in check_case_bounded(case, ordinal) {
            failures.push(format!("{}: {failure}", case.id));
        }
    }

    assert!(
        failures.is_empty(),
        "Day 7 parsed-C memory equivalence failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn chir_string_initializers_use_one_fragment_aware_code_unit_authority() {
    assert_eq!(STRING_UNIT_CASES.len(), 3, "string RED denominator changed");
    for (ordinal, case) in STRING_UNIT_CASES.iter().copied().enumerate() {
        let arena = Arena::default();
        let unit = parse(case.source, FileId::new(7_101 + ordinal as u32), &arena)
            .unwrap_or_else(|error| panic!("{}: {error}", case.id));
        let actual = observe_chir(&unit).unwrap_or_else(|error| panic!("{}: {error}", case.id));
        assert_eq!(actual.scalar, case.expected, "{}", case.id);
        assert!(actual.volatile_trace.is_empty(), "{}", case.id);
    }
}

#[test]
fn chir_invalid_memory_programs_report_exact_typed_diagnostics() {
    assert_eq!(
        INVALID_CHIR_CASES.len(),
        4,
        "invalid RED denominator changed"
    );
    for (ordinal, case) in INVALID_CHIR_CASES.iter().copied().enumerate() {
        let arena = Arena::default();
        let unit = parse(case.source, FileId::new(7_201 + ordinal as u32), &arena)
            .unwrap_or_else(|error| panic!("{}: {error}", case.id));
        let main = function_symbol(&unit, "main").unwrap();
        let error = interpret_scalar_function(&unit, main, CHirLimits::new(STEP_BUDGET))
            .expect_err(case.id);
        assert_eq!(
            (error.code(), error.message()),
            (case.expected_code, case.expected_message.to_owned()),
            "{}",
            case.id
        );
    }
}
