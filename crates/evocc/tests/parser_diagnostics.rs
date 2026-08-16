use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

struct NegativeCase {
    id: &'static str,
    source: &'static str,
    column: u32,
    code: &'static str,
    message: &'static str,
}

const NEGATIVE_CASES: &[NegativeCase] = &[
    NegativeCase {
        id: "decl.missing_semicolon",
        source: "int value",
        column: 10,
        code: "E1002",
        message: "expected-token: ;",
    },
    NegativeCase {
        id: "decl.pointer.missing_name",
        source: "int *;",
        column: 6,
        code: "E1001",
        message: "expected-declaration",
    },
    NegativeCase {
        id: "decl.array.missing_rbracket",
        source: "int values[4;",
        column: 13,
        code: "E1002",
        message: "expected-token: ]",
    },
    NegativeCase {
        id: "decl.function_pointer.missing_rparen",
        source: "int (*handler)(int argument;",
        column: 28,
        code: "E1002",
        message: "expected-token: )",
    },
    NegativeCase {
        id: "decl.array_of_function",
        source: "int handlers[2](int argument);",
        column: 16,
        code: "E1005",
        message: "invalid-declarator",
    },
    NegativeCase {
        id: "decl.function_returning_array",
        source: "int factory(void)[4];",
        column: 18,
        code: "E1005",
        message: "invalid-declarator",
    },
    NegativeCase {
        id: "expr.postfix.call.missing_rparen",
        source: "int sink = callee(argument;",
        column: 27,
        code: "E1002",
        message: "expected-token: )",
    },
    NegativeCase {
        id: "expr.postfix.index.missing_rbracket",
        source: "int sink = items[index;",
        column: 23,
        code: "E1002",
        message: "expected-token: ]",
    },
    NegativeCase {
        id: "expr.postfix.member.missing_name",
        source: "int sink = object.;",
        column: 19,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.unary.missing_operand",
        source: "int sink = !;",
        column: 13,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.cast.missing_operand",
        source: "int sink = (long);",
        column: 17,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.multiplicative.missing_rhs",
        source: "int sink = left *;",
        column: 18,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.additive.missing_rhs",
        source: "int sink = left +;",
        column: 18,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.shift.missing_rhs",
        source: "int sink = left <<;",
        column: 19,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.relational.missing_rhs",
        source: "int sink = left <;",
        column: 18,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.equality.missing_rhs",
        source: "int sink = left ==;",
        column: 19,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.bitwise_and.missing_rhs",
        source: "int sink = left &;",
        column: 18,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.bitwise_xor.missing_rhs",
        source: "int sink = left ^;",
        column: 18,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.bitwise_or.missing_rhs",
        source: "int sink = left |;",
        column: 18,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.logical_and.missing_rhs",
        source: "int sink = left &&;",
        column: 19,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.logical_or.missing_rhs",
        source: "int sink = left ||;",
        column: 19,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.conditional.missing_true",
        source: "int sink = condition ? : fallback;",
        column: 24,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.assignment.missing_rhs",
        source: "int sink = left =;",
        column: 18,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.comma.missing_rhs",
        source: "int sink = (left,);",
        column: 18,
        code: "E1003",
        message: "expected-expression",
    },
    NegativeCase {
        id: "expr.trailing_token",
        source: "int sink = left right;",
        column: 17,
        code: "E1004",
        message: "trailing-token",
    },
];

const POSITIVE_CONTROLS: &[(&str, &str)] = &[
    ("control.object", "int value;"),
    ("control.precedence", "int sink = left + middle * right;"),
    ("control.function", "int compute(int left, long right);"),
];

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn run_parse_only(source: &str) -> Output {
    let mut child = Command::new(evocc())
        .args(["--parse-only", "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn evocc");
    let mut stdin = child.stdin.take().expect("child stdin");
    stdin.write_all(source.as_bytes()).expect("write source");
    drop(stdin);

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait().expect("poll evocc") {
            Some(_) => return child.wait_with_output().expect("collect evocc output"),
            None if Instant::now() >= deadline => {
                child.kill().expect("kill timed-out evocc");
                let output = child.wait_with_output().expect("collect timed-out evocc");
                panic!(
                    "evocc timed out; stdout={:?} stderr={:?}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

#[test]
fn valid_parse_only_controls_are_accepted() {
    assert_eq!(POSITIVE_CONTROLS.len(), 3, "positive denominator changed");
    let mut ids = HashSet::new();
    let mut sources = HashSet::new();

    for (id, source) in POSITIVE_CONTROLS {
        assert!(ids.insert(*id), "duplicate positive id {id}");
        assert!(
            sources.insert(source.as_bytes()),
            "duplicate positive bytes {id}"
        );
        let output = run_parse_only(source);
        assert_eq!(output.status.code(), Some(0), "case {id}: {:?}", output);
        assert!(output.stdout.is_empty(), "case {id}");
        assert!(output.stderr.is_empty(), "case {id}: {:?}", output);
    }

    println!(
        "PARSER_DIAGNOSTICS positive={} unique={}",
        POSITIVE_CONTROLS.len(),
        sources.len()
    );
}

#[test]
fn malformed_matrix_is_rejected_with_exact_diagnostics() {
    assert_eq!(NEGATIVE_CASES.len(), 25, "negative denominator changed");
    let mut ids = HashSet::new();
    let mut sources = HashSet::new();

    for case in NEGATIVE_CASES {
        assert!(ids.insert(case.id), "duplicate negative id {}", case.id);
        assert!(
            sources.insert(case.source.as_bytes()),
            "duplicate negative bytes {}",
            case.id
        );
        let output = run_parse_only(case.source);
        assert_eq!(
            output.status.code(),
            Some(1),
            "case {}: {:?}",
            case.id,
            output
        );
        assert!(output.stdout.is_empty(), "case {}", case.id);
        let expected = format!(
            "/dev/stdin:1:{}: error[{}]: {}\n",
            case.column, case.code, case.message
        );
        assert_eq!(
            String::from_utf8(output.stderr).expect("diagnostic is UTF-8"),
            expected,
            "case {}",
            case.id
        );
    }

    println!(
        "PARSER_DIAGNOSTICS negative={} unique={}",
        NEGATIVE_CASES.len(),
        sources.len()
    );
}
