use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::process::Command;
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
const DAY7_DEFINED: usize = 128;
const DAY7_INVALID: usize = 64;
const DAY7_CORPUS_MANIFEST_SHA256: &str =
    "de83fa7a37de1f7ee3e3e3617b99f5137c037c67ecd97015c47376b744f9e217";
const MAX_CORPUS_FRAME_BYTES: usize = 1 << 20;

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
struct EquivalentPlacePair {
    addressed: Case,
    direct: Case,
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
    Case {
        id: "scaled_pointer_updates",
        source: "int main(void) { int a[6] = {10, 20, 30, 40, 50, 60}; int *p = a; int first = *p++; int second = *++p; int third = *p--; int fourth = *--p; p += 3; int fifth = *p; p -= 2; int sixth = *p; return first + second + third + fourth + fifth + sixth; }\n",
        expected: 140,
        expected_volatile_ir_accesses: None,
    },
    Case {
        id: "loaded_pointer_place_chain",
        source: "struct Node { int value; struct Node *next; }; int main(void) { struct Node a; struct Node b; struct Node c; a.next = &b; b.next = &c; b.value = 1; c.value = 7; struct Node *p = &a; p->next->value = 5; p->next->next->value += 2; return p->next->value * 10 + p->next->next->value; }\n",
        expected: 59,
        expected_volatile_ir_accesses: None,
    },
];

const ADDRESS_OF_PLACE_PAIRS: &[EquivalentPlacePair] = &[
    EquivalentPlacePair {
        addressed: Case {
            id: "local_object_through_address",
            source: "struct Node { int value; }; int main(void) { struct Node node; node.value = 11; return (&node)->value; }\n",
            expected: 11,
            expected_volatile_ir_accesses: None,
        },
        direct: Case {
            id: "local_object_direct",
            source: "struct Node { int value; }; int main(void) { struct Node node; node.value = 11; return node.value; }\n",
            expected: 11,
            expected_volatile_ir_accesses: None,
        },
    },
    EquivalentPlacePair {
        addressed: Case {
            id: "global_object_through_address",
            source: "struct Node { int value; }; struct Node node; int main(void) { node.value = 12; return (&node)->value; }\n",
            expected: 12,
            expected_volatile_ir_accesses: None,
        },
        direct: Case {
            id: "global_object_direct",
            source: "struct Node { int value; }; struct Node node; int main(void) { node.value = 12; return node.value; }\n",
            expected: 12,
            expected_volatile_ir_accesses: None,
        },
    },
    EquivalentPlacePair {
        addressed: Case {
            id: "pointer_member_through_address",
            source: "struct Node { int value; }; struct Holder { struct Node head; }; int main(void) { struct Holder holder; struct Holder *pointer = &holder; pointer->head.value = 13; return (&pointer->head)->value; }\n",
            expected: 13,
            expected_volatile_ir_accesses: None,
        },
        direct: Case {
            id: "pointer_member_direct",
            source: "struct Node { int value; }; struct Holder { struct Node head; }; int main(void) { struct Holder holder; struct Holder *pointer = &holder; pointer->head.value = 13; return pointer->head.value; }\n",
            expected: 13,
            expected_volatile_ir_accesses: None,
        },
    },
    EquivalentPlacePair {
        addressed: Case {
            id: "object_member_through_address",
            source: "struct Node { int value; }; struct Holder { struct Node head; }; int main(void) { struct Holder holder; holder.head.value = 14; return (&holder.head)->value; }\n",
            expected: 14,
            expected_volatile_ir_accesses: None,
        },
        direct: Case {
            id: "object_member_direct",
            source: "struct Node { int value; }; struct Holder { struct Node head; }; int main(void) { struct Holder holder; holder.head.value = 14; return holder.head.value; }\n",
            expected: 14,
            expected_volatile_ir_accesses: None,
        },
    },
    EquivalentPlacePair {
        addressed: Case {
            id: "cancelled_address_and_deref",
            source: "struct Node { int value; }; struct Holder { struct Node head; }; int main(void) { struct Holder holder; holder.head.value = 15; return (*&holder.head).value; }\n",
            expected: 15,
            expected_volatile_ir_accesses: None,
        },
        direct: Case {
            id: "cancelled_address_and_deref_control",
            source: "struct Node { int value; }; struct Holder { struct Node head; }; int main(void) { struct Holder holder; holder.head.value = 15; return holder.head.value; }\n",
            expected: 15,
            expected_volatile_ir_accesses: None,
        },
    },
    EquivalentPlacePair {
        addressed: Case {
            id: "address_of_dereferenced_pointer",
            source: "struct Node { int value; }; int main(void) { struct Node node; struct Node *pointer = &node; pointer->value = 16; return (&*pointer)->value; }\n",
            expected: 16,
            expected_volatile_ir_accesses: None,
        },
        direct: Case {
            id: "dereferenced_pointer_control",
            source: "struct Node { int value; }; int main(void) { struct Node node; struct Node *pointer = &node; pointer->value = 16; return pointer->value; }\n",
            expected: 16,
            expected_volatile_ir_accesses: None,
        },
    },
    EquivalentPlacePair {
        addressed: Case {
            id: "array_element_through_address",
            source: "struct Node { int value; }; int main(void) { struct Node nodes[2]; nodes[1].value = 17; return (&nodes[1])->value; }\n",
            expected: 17,
            expected_volatile_ir_accesses: None,
        },
        direct: Case {
            id: "array_element_direct",
            source: "struct Node { int value; }; int main(void) { struct Node nodes[2]; nodes[1].value = 17; return nodes[1].value; }\n",
            expected: 17,
            expected_volatile_ir_accesses: None,
        },
    },
    EquivalentPlacePair {
        addressed: Case {
            id: "nested_subobject_through_address",
            source: "struct Inner { int value; }; struct Middle { struct Inner inner; }; struct Outer { struct Middle middle; }; int main(void) { struct Outer outer; outer.middle.inner.value = 18; return (&outer.middle.inner)->value; }\n",
            expected: 18,
            expected_volatile_ir_accesses: None,
        },
        direct: Case {
            id: "nested_subobject_direct",
            source: "struct Inner { int value; }; struct Middle { struct Inner inner; }; struct Outer { struct Middle middle; }; int main(void) { struct Outer outer; outer.middle.inner.value = 18; return outer.middle.inner.value; }\n",
            expected: 18,
            expected_volatile_ir_accesses: None,
        },
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
    InvalidCHirCase {
        id: "pointer_update_beyond_one_past",
        source: "int main(void) { int values[2] = {1, 2}; int *pointer = values; pointer += 3; return 0; }\n",
        expected_code: "E3011",
        expected_message: "out-of-bounds-memory-access",
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum GeneratedExpectation {
    Value(i128),
    DiagnosticCode(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GeneratedCase {
    id: String,
    source: String,
    expected: GeneratedExpectation,
}

struct Frames<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Frames<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn next(&mut self) -> Result<&'a [u8], String> {
        let length_end = self
            .offset
            .checked_add(8)
            .ok_or_else(|| "corpus frame offset overflow".to_owned())?;
        let encoded = self
            .bytes
            .get(self.offset..length_end)
            .ok_or_else(|| "truncated corpus frame length".to_owned())?;
        let length = usize::try_from(u64::from_be_bytes(
            encoded
                .try_into()
                .map_err(|_| "invalid corpus frame length".to_owned())?,
        ))
        .map_err(|_| "corpus frame length does not fit usize".to_owned())?;
        if length > MAX_CORPUS_FRAME_BYTES {
            return Err(format!("corpus frame exceeds limit: {length}"));
        }
        let payload_end = length_end
            .checked_add(length)
            .ok_or_else(|| "corpus payload offset overflow".to_owned())?;
        let payload = self
            .bytes
            .get(length_end..payload_end)
            .ok_or_else(|| "truncated corpus frame payload".to_owned())?;
        self.offset = payload_end;
        Ok(payload)
    }

    fn finish(self) -> Result<(), String> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(format!(
                "{} trailing corpus protocol bytes",
                self.bytes.len() - self.offset
            ))
        }
    }
}

fn corpus_utf8<'a>(frames: &mut Frames<'a>, field: &str) -> Result<&'a str, String> {
    std::str::from_utf8(frames.next()?)
        .map_err(|error| format!("corpus {field} is not UTF-8: {error}"))
}

fn day7_corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/acceptance/day7-memory-corpus.py")
}

fn prime_system_python_launcher() -> Result<(), String> {
    // A fresh macOS HOME can make the Xcode-provided /usr/bin/python3 emit
    // one-time launcher diagnostics before Python starts. Keep that host
    // bootstrap outside the authority invocation; the real invocation below
    // still requires exact empty stderr.
    let output = Command::new("/usr/bin/python3")
        .arg("-I")
        .arg("-c")
        .arg("")
        .output()
        .map_err(|error| format!("failed to prime isolated system Python: {error}"))?;
    if !output.status.success() || !output.stdout.is_empty() {
        return Err(format!(
            "isolated system Python preflight failed with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn load_generated_corpus() -> Result<Vec<GeneratedCase>, String> {
    let authority = day7_corpus_path();
    if !authority.is_file() {
        return Err(format!(
            "Day-7 memory corpus authority is missing: {}",
            authority.display()
        ));
    }
    prime_system_python_launcher()?;
    let output = Command::new("/usr/bin/python3")
        .arg("-I")
        .arg(&authority)
        .arg("export-framed")
        .output()
        .map_err(|error| format!("failed to execute Day-7 corpus authority: {error}"))?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(format!(
            "Day-7 corpus authority failed with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let mut stream = Frames::new(&output.stdout);
    if stream.next()? != b"evocc-day7-memory-corpus-v1" {
        return Err("unexpected Day-7 corpus protocol magic".to_owned());
    }
    let manifest_sha256 = corpus_utf8(&mut stream, "manifest SHA-256")?;
    if manifest_sha256 != DAY7_CORPUS_MANIFEST_SHA256 {
        return Err(format!(
            "Day-7 corpus manifest changed: expected {DAY7_CORPUS_MANIFEST_SHA256}, got {manifest_sha256}"
        ));
    }
    let count = corpus_utf8(&mut stream, "record count")?
        .parse::<usize>()
        .map_err(|error| format!("invalid corpus record count: {error}"))?;
    let mut cases = Vec::with_capacity(count);
    for ordinal in 0..count {
        let mut record = Frames::new(stream.next()?);
        let kind = corpus_utf8(&mut record, "kind")?;
        let id = corpus_utf8(&mut record, "ID")?.to_owned();
        let source = corpus_utf8(&mut record, "source")?.to_owned();
        let expected = corpus_utf8(&mut record, "expectation")?;
        record
            .finish()
            .map_err(|error| format!("record {ordinal} ({id}): {error}"))?;
        let expected = match kind {
            "defined" => GeneratedExpectation::Value(
                expected
                    .parse::<i128>()
                    .map_err(|error| format!("record {id} has invalid value: {error}"))?,
            ),
            "invalid" => GeneratedExpectation::DiagnosticCode(expected.to_owned()),
            other => return Err(format!("record {id} has unknown kind {other:?}")),
        };
        cases.push(GeneratedCase {
            id,
            source,
            expected,
        });
    }
    stream.finish()?;

    let defined = cases
        .iter()
        .filter(|case| matches!(case.expected, GeneratedExpectation::Value(_)))
        .count();
    let invalid = cases.len() - defined;
    if defined != DAY7_DEFINED || invalid != DAY7_INVALID {
        return Err(format!(
            "Day-7 corpus denominator changed: defined={defined}, invalid={invalid}"
        ));
    }
    let unique_ids = cases.iter().map(|case| &case.id).collect::<BTreeSet<_>>();
    let unique_sources = cases
        .iter()
        .map(|case| case.source.as_bytes())
        .collect::<BTreeSet<_>>();
    if unique_ids.len() != cases.len() || unique_sources.len() != cases.len() {
        return Err("Day-7 corpus IDs or source bytes are not unique".to_owned());
    }
    Ok(cases)
}

fn check_generated_case(case: &GeneratedCase, ordinal: usize) -> Vec<String> {
    let arena = Arena::default();
    let unit = match parse(&case.source, FileId::new(7_500 + ordinal as u32), &arena) {
        Ok(unit) => unit,
        Err(error) => return vec![error],
    };
    let main = match function_symbol(&unit, "main") {
        Ok(main) => main,
        Err(error) => return vec![error],
    };
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    let module = match lower_unit(&unit, &target) {
        Ok(module) => module,
        Err(error) => {
            return vec![format!(
                "lowering failed: {}: {}",
                error.code(),
                error.message()
            )];
        }
    };
    if let Err(error) = validate_module(&module) {
        return vec![format!("EvoIR validation failed: {error}")];
    }

    let mut failures = Vec::new();
    match &case.expected {
        GeneratedExpectation::Value(expected) => {
            let chir = observe_chir(&unit);
            let evoir = observe_evoir(&module);
            match (&chir, &evoir) {
                (Ok(chir), Ok(evoir)) => {
                    if chir.scalar != *expected {
                        failures.push(format!(
                            "C-HIR returned {}, expected {expected}",
                            chir.scalar
                        ));
                    }
                    if evoir.scalar != *expected {
                        failures.push(format!(
                            "EvoIR returned {}, expected {expected}",
                            evoir.scalar
                        ));
                    }
                    if chir != evoir {
                        failures.push(format!(
                            "interpreter observation mismatch: C-HIR={chir:?}, EvoIR={evoir:?}"
                        ));
                    }
                }
                (chir, evoir) => failures.push(format!(
                    "defined execution failed: C-HIR={chir:?}, EvoIR={evoir:?}"
                )),
            }
        }
        GeneratedExpectation::DiagnosticCode(expected_code) => {
            let chir = interpret_scalar_function(&unit, main, CHirLimits::new(STEP_BUDGET));
            let (arguments, _) = match entry_contract(&module) {
                Ok(contract) => contract,
                Err(error) => return vec![error],
            };
            let evoir =
                interpret_module(&module, "main", &arguments, EvoIrLimits::new(STEP_BUDGET));
            match (chir, evoir) {
                (Err(chir), Err(evoir)) => {
                    if chir.code() != expected_code {
                        failures.push(format!(
                            "C-HIR diagnostic {}, expected {expected_code}",
                            chir.code()
                        ));
                    }
                    if evoir.code() != expected_code {
                        failures.push(format!(
                            "EvoIR diagnostic {}, expected {expected_code}",
                            evoir.code()
                        ));
                    }
                    if chir.code() != evoir.code() {
                        failures.push(format!(
                            "diagnostic mismatch: C-HIR={}, EvoIR={}",
                            chir.code(),
                            evoir.code()
                        ));
                    }
                }
                (chir, evoir) => failures.push(format!(
                    "invalid execution did not fail in both interpreters: C-HIR={chir:?}, EvoIR={evoir:?}"
                )),
            }
        }
    }
    failures
}

fn check_generated_case_bounded(case: GeneratedCase, ordinal: usize) -> Vec<String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let result = catch_unwind(AssertUnwindSafe(|| check_generated_case(&case, ordinal)))
            .map_err(panic_payload);
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
    if CASES.len() != 8 {
        failures.push(format!("case denominator is {}, expected 8", CASES.len()));
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

fn assert_addressed_place_pair(index: usize) {
    assert_eq!(
        ADDRESS_OF_PLACE_PAIRS.len(),
        8,
        "addressed-place denominator changed"
    );
    let pair = ADDRESS_OF_PLACE_PAIRS[index];
    let control_failures = check_case_bounded(pair.direct, 400 + index * 2);
    assert!(
        control_failures.is_empty(),
        "{} control failed:\n{}",
        pair.direct.id,
        control_failures.join("\n")
    );
    let addressed_failures = check_case_bounded(pair.addressed, 401 + index * 2);
    assert!(
        addressed_failures.is_empty(),
        "{} failed:\n{}",
        pair.addressed.id,
        addressed_failures.join("\n")
    );
}

#[test]
fn local_object_address_preserves_the_direct_place() {
    assert_addressed_place_pair(0);
}

#[test]
fn global_object_address_preserves_the_direct_place() {
    assert_addressed_place_pair(1);
}

#[test]
fn pointer_member_address_preserves_the_direct_place() {
    assert_addressed_place_pair(2);
}

#[test]
fn object_member_address_preserves_the_direct_place() {
    assert_addressed_place_pair(3);
}

#[test]
fn adjacent_address_and_dereference_preserve_the_direct_place() {
    assert_addressed_place_pair(4);
}

#[test]
fn address_of_a_dereferenced_pointer_preserves_the_direct_place() {
    assert_addressed_place_pair(5);
}

#[test]
fn array_element_address_preserves_the_direct_place() {
    assert_addressed_place_pair(6);
}

#[test]
fn nested_subobject_address_preserves_the_direct_place() {
    assert_addressed_place_pair(7);
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
        5,
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

#[test]
fn invalid_memory_programs_match_in_both_interpreters() {
    for (ordinal, case) in INVALID_CHIR_CASES.iter().copied().enumerate() {
        let arena = Arena::default();
        let unit = parse(case.source, FileId::new(7_301 + ordinal as u32), &arena)
            .unwrap_or_else(|error| panic!("{}: {error}", case.id));
        let target = TargetDataLayout::aarch64_unknown_linux_gnu();
        let module = lower_unit(&unit, &target)
            .unwrap_or_else(|error| panic!("{} lowering: {}", case.id, error.message()));
        let (arguments, _) = entry_contract(&module).expect(case.id);
        let error = interpret_module(&module, "main", &arguments, EvoIrLimits::new(STEP_BUDGET))
            .expect_err(case.id);
        assert_eq!(
            (error.code(), error.message()),
            (case.expected_code, case.expected_message.to_owned()),
            "{}",
            case.id
        );
    }
}

#[test]
fn generated_day7_memory_corpus_is_differentially_clean_without_skips() {
    let cases = load_generated_corpus()
        .unwrap_or_else(|error| panic!("Day-7 memory corpus load failed: {error}"));
    let mut failures = Vec::new();
    let mut defined_passed = 0;
    let mut invalid_passed = 0;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let case_failures = check_generated_case_bounded(case.clone(), ordinal);
        if case_failures.is_empty() {
            match case.expected {
                GeneratedExpectation::Value(_) => defined_passed += 1,
                GeneratedExpectation::DiagnosticCode(_) => invalid_passed += 1,
            }
        } else {
            failures.extend(
                case_failures
                    .into_iter()
                    .map(|failure| format!("{}: {failure}", case.id)),
            );
        }
    }
    assert!(
        failures.is_empty() && defined_passed == DAY7_DEFINED && invalid_passed == DAY7_INVALID,
        "Day-7 G3 internal differential gate failed: defined={defined_passed}/{DAY7_DEFINED}, \
         invalid={invalid_passed}/{DAY7_INVALID}, skipped=0; failures:\n{}",
        failures.join("\n")
    );
}
