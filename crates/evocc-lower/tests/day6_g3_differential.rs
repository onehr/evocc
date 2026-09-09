use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits as ChirLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_evoir::{
    interpret_module, validate_module, InterpreterLimits as EvoIrLimits, Module, RuntimeValue,
};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::{EvoIRType, Region, TargetDataLayout};

const DAY5_DEFINED: usize = 256;
const DAY5_INVALID: usize = 64;
// The original 320 source identities remain frozen. Four historical E3006
// cases now return measured values under the GNU shift rule (revision 1.9.88).
const RETURN_CASES: usize = DAY5_DEFINED + 4;
const DIAGNOSTIC_CASES: usize = DAY5_INVALID - 4;
const DAY6_GENERATED: usize = 256;
const DAY5_CONTRACT_SHA256: &str =
    "71a757d161cd562d54992511a202925aeb5768ea5a23d93cfb0aabc332ee69a4";
const DAY5_GENERATOR_SHA256: &str =
    "ba0dcb92efbe85f103e41f8dda83e98757f31e66dd29b5afbf319b594da14b9e";
const STEP_BUDGET: u64 = 100_000;
const MAX_FRAME_BYTES: usize = 1 << 20;
const CASE_WATCHDOG: Duration = Duration::from_secs(2);

// The bridge deliberately imports the checked-in authority, audits its frozen
// contract first, and only then emits its actual case objects.  Both records
// and fields are length-prefixed, so C source and diagnostics never pass
// through a delimiter or an independently reimplemented JSON model.
const PYTHON_CORPUS_BRIDGE: &str = r#"
import io
import hashlib
import pathlib
import runpy
import struct
import sys

authority = runpy.run_path(sys.argv[1])
audit = authority["audit_contract"]()
generator_sha256 = hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest()

rows = []
for case in authority["build_defined_cases"]():
    rows.append(("defined", case.case_id, case.source, str(case.value), "", ""))
for case in authority["build_reject_cases"]():
    if case.case_id in authority["SIGNED_SHIFT_RESULTS"]:
        rows.append(("defined", case.case_id, case.source,
                     str(authority["SIGNED_SHIFT_RESULTS"][case.case_id]), "", ""))
        continue
    rows.append((
        "invalid",
        case.case_id,
        case.source,
        "",
        case.diagnostic.code,
        case.diagnostic.message,
    ))

def frame(payload):
    return struct.pack(">Q", len(payload)) + payload

def record(fields):
    payload = io.BytesIO()
    for field in fields:
        payload.write(frame(field.encode("utf-8")))
    return payload.getvalue()

output = sys.stdout.buffer
output.write(frame(b"evocc-day6-g3-lp-v1"))
output.write(frame(audit["contractSha256"].encode("ascii")))
output.write(frame(generator_sha256.encode("ascii")))
output.write(frame(str(len(rows)).encode("ascii")))
for row in rows:
    output.write(frame(record(row)))
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expected {
    Value(i128),
    Diagnostic { code: String, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CorpusCase {
    id: String,
    source: String,
    expected: Expected,
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
            .ok_or_else(|| "length-prefix offset overflow".to_owned())?;
        let encoded_length = self
            .bytes
            .get(self.offset..length_end)
            .ok_or_else(|| "truncated length prefix".to_owned())?;
        let length = usize::try_from(u64::from_be_bytes(
            encoded_length
                .try_into()
                .map_err(|_| "invalid length prefix".to_owned())?,
        ))
        .map_err(|_| "frame length does not fit usize".to_owned())?;
        if length > MAX_FRAME_BYTES {
            return Err(format!("frame exceeds {MAX_FRAME_BYTES} bytes: {length}"));
        }
        let payload_end = length_end
            .checked_add(length)
            .ok_or_else(|| "payload offset overflow".to_owned())?;
        let payload = self
            .bytes
            .get(length_end..payload_end)
            .ok_or_else(|| format!("truncated frame: declared {length} bytes"))?;
        self.offset = payload_end;
        Ok(payload)
    }

    fn finish(self) -> Result<(), String> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(format!(
                "{} trailing bytes after framed protocol",
                self.bytes.len() - self.offset
            ))
        }
    }
}

fn utf8_field<'a>(frames: &mut Frames<'a>, name: &str) -> Result<&'a str, String> {
    std::str::from_utf8(frames.next()?).map_err(|error| format!("{name} is not UTF-8: {error}"))
}

fn day5_authority_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/acceptance/day5-scalar-interpreter.py")
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

fn load_day5_corpus() -> Result<Vec<CorpusCase>, String> {
    let authority = day5_authority_path();
    if !authority.is_file() {
        return Err(format!(
            "Day-5 scalar interpreter authority is missing: {}",
            authority.display()
        ));
    }
    prime_system_python_launcher()?;
    let output = Command::new("/usr/bin/python3")
        .arg("-I")
        .arg("-c")
        .arg(PYTHON_CORPUS_BRIDGE)
        .arg(&authority)
        .output()
        .map_err(|error| format!("failed to execute isolated Day-5 authority: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Day-5 authority audit/export failed with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if !output.stderr.is_empty() {
        return Err(format!(
            "Day-5 authority wrote unexpected stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let mut stream = Frames::new(&output.stdout);
    let magic = stream.next()?;
    if magic != b"evocc-day6-g3-lp-v1" {
        return Err(format!("unexpected corpus protocol magic: {magic:?}"));
    }
    let contract_sha256 = std::str::from_utf8(stream.next()?)
        .map_err(|error| format!("contract SHA-256 is not UTF-8: {error}"))?;
    if contract_sha256 != DAY5_CONTRACT_SHA256 {
        return Err(format!(
            "Day-5 scalar contract identity changed: expected {DAY5_CONTRACT_SHA256}, got {contract_sha256}"
        ));
    }
    let generator_sha256 = std::str::from_utf8(stream.next()?)
        .map_err(|error| format!("generator SHA-256 is not UTF-8: {error}"))?;
    if generator_sha256 != DAY5_GENERATOR_SHA256 {
        return Err(format!(
            "Day-5 scalar generator identity changed: expected {DAY5_GENERATOR_SHA256}, got {generator_sha256}"
        ));
    }
    let count = std::str::from_utf8(stream.next()?)
        .map_err(|error| format!("record count is not UTF-8: {error}"))?
        .parse::<usize>()
        .map_err(|error| format!("record count is not an integer: {error}"))?;
    let mut cases = Vec::with_capacity(count);
    for ordinal in 0..count {
        let payload = stream.next()?;
        let mut fields = Frames::new(payload);
        let kind = utf8_field(&mut fields, "kind")?;
        let id = utf8_field(&mut fields, "id")?.to_owned();
        let source = utf8_field(&mut fields, "source")?.to_owned();
        let value = utf8_field(&mut fields, "value")?;
        let code = utf8_field(&mut fields, "code")?;
        let message = utf8_field(&mut fields, "message")?;
        fields
            .finish()
            .map_err(|error| format!("record {ordinal} ({id}): {error}"))?;
        let expected = match kind {
            "defined" => {
                if !code.is_empty() || !message.is_empty() {
                    return Err(format!("defined record {id} carries a diagnostic"));
                }
                Expected::Value(
                    value
                        .parse::<i128>()
                        .map_err(|error| format!("invalid expected value for {id}: {error}"))?,
                )
            }
            "invalid" => {
                if !value.is_empty() || code.is_empty() || message.is_empty() {
                    return Err(format!("invalid record {id} has malformed expectations"));
                }
                Expected::Diagnostic {
                    code: code.to_owned(),
                    message: message.to_owned(),
                }
            }
            other => return Err(format!("record {id} has unknown kind {other:?}")),
        };
        cases.push(CorpusCase {
            id,
            source,
            expected,
        });
    }
    stream.finish()?;

    let defined = cases
        .iter()
        .filter(|case| matches!(case.expected, Expected::Value(_)))
        .count();
    let invalid = cases.len() - defined;
    if defined != RETURN_CASES || invalid != DIAGNOSTIC_CASES {
        return Err(format!(
            "authoritative denominators changed: defined={defined}, invalid={invalid}"
        ));
    }
    let unique_ids = cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let unique_sources = cases
        .iter()
        .map(|case| case.source.as_bytes())
        .collect::<BTreeSet<_>>();
    if unique_ids.len() != cases.len() || unique_sources.len() != cases.len() {
        return Err("authoritative corpus IDs or source bytes are not unique".to_owned());
    }
    Ok(cases)
}

fn parse<'arena>(
    source: &str,
    file: FileId,
    arena: &'arena Arena,
) -> Result<CHirUnit<'arena>, String> {
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err(format!("lexer rejected source at {:?}", token.span));
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    if parser.had_error() {
        return Err(format!("parser rejected source: {:?}", parser.diagnostic()));
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Execution {
    Value(i128),
    Diagnostic { code: String, message: String },
}

fn execute_chir(unit: &CHirUnit<'_>) -> Result<Execution, String> {
    let main = function_symbol(unit, "main")?;
    match interpret_scalar_function(unit, main, ChirLimits::new(STEP_BUDGET)) {
        Ok(outcome) => {
            let value = outcome
                .return_value
                .ok_or_else(|| "C-HIR main returned no scalar value".to_owned())?;
            Ok(Execution::Value(
                value
                    .mathematical_value(unit.types())
                    .map_err(|error| error.to_string())?,
            ))
        }
        Err(error) => Ok(Execution::Diagnostic {
            code: error.code().to_owned(),
            message: error.message(),
        }),
    }
}

fn entry_arguments(module: &Module) -> Result<Vec<RuntimeValue>, String> {
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .ok_or_else(|| "lowering produced no @main".to_owned())?;
    match main.params.as_slice() {
        [] => Ok(Vec::new()),
        [mem, world] if mem.ty == EvoIRType::Mem(Region::All) && world.ty == EvoIRType::World => {
            Ok(vec![
                RuntimeValue::new(mem.ty.clone(), 0),
                RuntimeValue::new(world.ty.clone(), 0),
            ])
        }
        parameters => Err(format!(
            "@main parameters are not exactly [] or [mem<all>, world]: {:?}",
            parameters
                .iter()
                .map(|parameter| &parameter.ty)
                .collect::<Vec<_>>()
        )),
    }
}

fn signed_scalar(value: &RuntimeValue) -> Result<i128, String> {
    let EvoIRType::Bits(width @ 1..=64) = value.ty else {
        return Err(format!(
            "EvoIR main returned unsupported type {:?}",
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
    Ok(if raw & sign_bit == 0 {
        i128::from(raw)
    } else {
        i128::from(raw) - (1_i128 << width)
    })
}

fn execute_evoir(module: &Module) -> Result<Execution, String> {
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .ok_or_else(|| "lowering produced no @main".to_owned())?;
    let has_effect_interface = match main.ret_tys.as_slice() {
        [EvoIRType::Bits(1..=64)] => false,
        [EvoIRType::Bits(1..=64), EvoIRType::Mem(Region::All), EvoIRType::World] => true,
        returns => {
            return Err(format!(
                "@main results are not exactly [bits<N>] or [bits<N>, mem<all>, world]: {returns:?}"
            ));
        }
    };
    if has_effect_interface != !main.params.is_empty() {
        return Err("@main parameter/result effect interfaces disagree".to_owned());
    }
    let arguments = entry_arguments(module)?;
    match interpret_module(module, "main", &arguments, EvoIrLimits::new(STEP_BUDGET)) {
        Ok(outcome) => {
            if outcome.return_values.len() != main.ret_tys.len() {
                return Err(format!(
                    "EvoIR @main returned {} values for {} declared results",
                    outcome.return_values.len(),
                    main.ret_tys.len()
                ));
            }
            for (index, (value, declared)) in
                outcome.return_values.iter().zip(&main.ret_tys).enumerate()
            {
                if &value.ty != declared {
                    return Err(format!(
                        "EvoIR @main result {index} has type {:?}, declared {declared:?}",
                        value.ty
                    ));
                }
            }
            let value = outcome
                .return_values
                .first()
                .ok_or_else(|| "EvoIR main returned no scalar value".to_owned())?;
            Ok(Execution::Value(signed_scalar(value)?))
        }
        Err(error) => Ok(Execution::Diagnostic {
            code: error.code().to_owned(),
            message: error.message(),
        }),
    }
}

#[derive(Debug)]
struct DifferentialObservation {
    chir: Execution,
    evoir: Execution,
}

fn execute_pipeline(source: &str, file: FileId) -> Result<DifferentialObservation, String> {
    let arena = Arena::default();
    let unit = parse(source, file, &arena)?;
    let chir = execute_chir(&unit)?;
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .map_err(|error| format!("lowering failed: {error}"))?;
    validate_module(&module).map_err(|error| format!("EvoIR validation failed: {error}"))?;
    let evoir = execute_evoir(&module)?;
    Ok(DifferentialObservation { chir, evoir })
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

fn execute_pipeline_guarded(source: &str, file: FileId) -> Result<DifferentialObservation, String> {
    let source = source.to_owned();
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let observed = catch_unwind(AssertUnwindSafe(|| execute_pipeline(&source, file)))
            .map_err(|payload| format!("pipeline panicked: {}", panic_payload(payload)))
            .and_then(|result| result);
        let _ = sender.send(observed);
    });
    receiver
        .recv_timeout(CASE_WATCHDOG)
        .map_err(|_| format!("pipeline exceeded {CASE_WATCHDOG:?} wall-clock watchdog"))?
}

fn evoir_expected_message<'a>(code: &str, message: &'a str) -> &'a str {
    if code == "E3001" {
        // EvoIR has no source object spelling after SSA lowering.  Normalize
        // only that unrepresentable suffix; the diagnostic name stays exact.
        message
            .split_once(": ")
            .map_or(message, |(diagnostic, _)| diagnostic)
    } else {
        message
    }
}

fn check_day5_case(case: &CorpusCase, ordinal: usize) -> Result<(), Vec<String>> {
    let observed = match execute_pipeline_guarded(&case.source, FileId::new(6_100 + ordinal as u32))
    {
        Ok(observed) => observed,
        Err(error) => return Err(vec![error]),
    };
    let mut failures = Vec::new();
    match &case.expected {
        Expected::Value(expected) => {
            let expected = Execution::Value(*expected);
            if observed.chir != expected {
                failures.push(format!("C-HIR {:?}, expected {expected:?}", observed.chir));
            }
            if observed.evoir != expected {
                failures.push(format!("EvoIR {:?}, expected {expected:?}", observed.evoir));
            }
            if observed.chir != observed.evoir {
                failures.push(format!(
                    "interpreter mismatch: C-HIR {:?}, EvoIR {:?}",
                    observed.chir, observed.evoir
                ));
            }
        }
        Expected::Diagnostic { code, message } => {
            let expected_chir = Execution::Diagnostic {
                code: code.clone(),
                message: message.clone(),
            };
            if observed.chir != expected_chir {
                failures.push(format!(
                    "C-HIR {:?}, expected exact {expected_chir:?}",
                    observed.chir
                ));
            }
            match &observed.evoir {
                Execution::Diagnostic {
                    code: actual_code,
                    message: actual_message,
                } => {
                    if actual_code != code {
                        failures.push(format!(
                            "EvoIR code {actual_code:?}, expected exact {code:?}"
                        ));
                    }
                    let expected_message = evoir_expected_message(code, message);
                    if actual_message != expected_message {
                        failures.push(format!(
                            "EvoIR message {actual_message:?}, expected {expected_message:?}"
                        ));
                    }
                }
                other => failures.push(format!(
                    "EvoIR {other:?}, expected diagnostic {code}: {message}"
                )),
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

#[derive(Debug)]
struct GeneratedCase {
    id: String,
    source: String,
    expected: i128,
}

fn generated_cases() -> Vec<GeneratedCase> {
    let cases = (0..DAY6_GENERATED)
        .map(|ordinal| {
            let bias = ordinal % 11 + 1;
            let iterations = ordinal % 7 + 1;
            let depth = ordinal % 5 + 2;
            let source = format!(
                "int helper_{ordinal:03}(int x) {{ return x + {bias}; }} \
                 int recur_{ordinal:03}(int n) {{ if (n <= 0) return 0; return helper_{ordinal:03}(n) + recur_{ordinal:03}(n - 1); }} \
                 int main(void) {{ int total = {ordinal}; for (int i = 0; i < {iterations}; i++) {{ total += helper_{ordinal:03}(i); }} return total + recur_{ordinal:03}({depth}); }}\n"
            );
            let loop_sum = iterations * bias + iterations * (iterations - 1) / 2;
            let recursion_sum = depth * bias + depth * (depth + 1) / 2;
            GeneratedCase {
                id: format!("generated.call-recursion-for.{ordinal:03}"),
                source,
                expected: (ordinal + loop_sum + recursion_sum) as i128,
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), DAY6_GENERATED);
    let sources = cases
        .iter()
        .map(|case| case.source.as_bytes())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        sources.len(),
        DAY6_GENERATED,
        "generated source bytes must be unique"
    );
    for case in &cases {
        assert!(case.source.contains("for ("), "{} has no for loop", case.id);
        assert!(
            case.source.matches("helper_").count() >= 3,
            "{} has no direct helper call",
            case.id
        );
        assert!(
            case.source.matches("recur_").count() >= 3,
            "{} has no executable recursion",
            case.id
        );
    }
    cases
}

fn check_generated_case(case: &GeneratedCase, ordinal: usize) -> Result<(), Vec<String>> {
    let observed = match execute_pipeline_guarded(&case.source, FileId::new(7_000 + ordinal as u32))
    {
        Ok(observed) => observed,
        Err(error) => return Err(vec![error]),
    };
    let expected = Execution::Value(case.expected);
    let mut failures = Vec::new();
    if observed.chir != expected {
        failures.push(format!("C-HIR {:?}, expected {expected:?}", observed.chir));
    }
    if observed.evoir != expected {
        failures.push(format!("EvoIR {:?}, expected {expected:?}", observed.evoir));
    }
    if observed.chir != observed.evoir {
        failures.push(format!(
            "interpreter mismatch: C-HIR {:?}, EvoIR {:?}",
            observed.chir, observed.evoir
        ));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

#[test]
fn day5_and_day6_corpora_are_exactly_differentially_clean() {
    let day5 =
        load_day5_corpus().unwrap_or_else(|error| panic!("Day-5 corpus load failed: {error}"));
    let generated = generated_cases();
    let mut failures = Vec::new();
    let mut day5_defined_passed = 0;
    let mut day5_invalid_passed = 0;
    let mut generated_passed = 0;

    for (ordinal, case) in day5.iter().enumerate() {
        match check_day5_case(case, ordinal) {
            Ok(()) => match case.expected {
                Expected::Value(_) => day5_defined_passed += 1,
                Expected::Diagnostic { .. } => day5_invalid_passed += 1,
            },
            Err(case_failures) => {
                failures.extend(
                    case_failures
                        .into_iter()
                        .map(|failure| format!("{}: {failure}", case.id)),
                );
            }
        }
    }
    for (ordinal, case) in generated.iter().enumerate() {
        match check_generated_case(case, ordinal) {
            Ok(()) => generated_passed += 1,
            Err(case_failures) => {
                failures.extend(
                    case_failures
                        .into_iter()
                        .map(|failure| format!("{}: {failure}", case.id)),
                );
            }
        }
    }

    assert!(
        failures.is_empty()
            && day5_defined_passed == RETURN_CASES
            && day5_invalid_passed == DIAGNOSTIC_CASES
            && generated_passed == DAY6_GENERATED,
        "Day-6 G3 differential gate failed: current returns={day5_defined_passed}/{RETURN_CASES}, \
         current diagnostics={day5_invalid_passed}/{DIAGNOSTIC_CASES}, \
         Day6 call+recursion+for={generated_passed}/{DAY6_GENERATED}; failures:\n{}",
        failures.join("\n")
    );
}
