use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const WATCHDOG: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Area {
    Scope,
    LvalueUpdate,
    Initializer,
    Day9Boundary,
}

#[derive(Clone, Copy, Debug)]
enum Expected {
    SilentSuccess,
    RedDiagnostic {
        column: u32,
        code: &'static str,
        message: &'static str,
    },
}

#[derive(Clone, Copy, Debug)]
struct Case {
    id: &'static str,
    area: Area,
    source: &'static str,
    expected: Expected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Observation {
    timed_out: bool,
    rc: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

struct Control {
    id: &'static str,
    source: &'static str,
}

const DAY_1_TO_4_CONTROLS: &[Control] = &[
    Control {
        id: "control.function-parameter",
        source: "int identity(int x) { return x; }\n",
    },
    Control {
        id: "control.assignment-expression",
        source: "int assignment(void) { int x = 1; x = x + 2; return x; }\n",
    },
    Control {
        id: "control.aggregate-declaration",
        source: "int aggregate(void) { struct S { int a; int b; }; struct S value; return 0; }\n",
    },
    Control {
        id: "control.enum-expression",
        source: "enum E { A = 1, B = A + 1 }; int f(void) { return B; }\n",
    },
];

const G2_CASES: &[Case] = &[
    // Symbol identity and lexical scope (10).
    Case {
        id: "scope.nested-shadow",
        area: Area::Scope,
        source: "int f(int x) { { int x = 2; x = x + 1; } return x; }\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "scope.parameter-visible",
        area: Area::Scope,
        source: "int f(int x) { return x + 1; }\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "scope.undeclared-read",
        area: Area::Scope,
        source: "int f(void) { return y; }\n",
        expected: Expected::RedDiagnostic {
            column: 22,
            code: "E2001",
            message: "undeclared-identifier: y",
        },
    },
    Case {
        id: "scope.undeclared-write",
        area: Area::Scope,
        source: "int f(void) { y = 1; return 0; }\n",
        expected: Expected::RedDiagnostic {
            column: 15,
            code: "E2001",
            message: "undeclared-identifier: y",
        },
    },
    Case {
        id: "scope.duplicate-local",
        area: Area::Scope,
        source: "int f(void) { int x; int x; return 0; }\n",
        expected: Expected::RedDiagnostic {
            column: 26,
            code: "E2002",
            message: "ordinary-identifier-redefinition: x",
        },
    },
    Case {
        id: "scope.parameter-redeclaration",
        area: Area::Scope,
        source: "int f(int x) { int x; return x; }\n",
        expected: Expected::RedDiagnostic {
            column: 20,
            code: "E2002",
            message: "ordinary-identifier-redefinition: x",
        },
    },
    Case {
        id: "scope.for-leak",
        area: Area::Scope,
        source: "int f(void) { for (int i = 0; i < 1; i++) { } return i; }\n",
        expected: Expected::RedDiagnostic {
            column: 54,
            code: "E2001",
            message: "undeclared-identifier: i",
        },
    },
    Case {
        id: "scope.typedef-use",
        area: Area::Scope,
        source: "typedef int T; T value;\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "scope.typedef-shadow-restore",
        area: Area::Scope,
        source:
            "typedef int T; int f(void) { { int T = 1; T = T + 1; } T value = 2; return value; }\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "scope.enum-leak",
        area: Area::Scope,
        source: "int f(void) { { enum { A = 1 }; } return A; }\n",
        expected: Expected::RedDiagnostic {
            column: 42,
            code: "E2001",
            message: "undeclared-identifier: A",
        },
    },
    // Modifiable lvalues, assignment associativity, and exact-one updates (11).
    Case {
        id: "lvalue.simple-assignment",
        area: Area::LvalueUpdate,
        source: "int f(void) { int x = 0; x = 2; return x; }\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "lvalue.literal-lhs",
        area: Area::LvalueUpdate,
        source: "int f(void) { 1 = 2; return 0; }\n",
        expected: Expected::RedDiagnostic {
            column: 15,
            code: "E2101",
            message: "expected-modifiable-lvalue",
        },
    },
    Case {
        id: "lvalue.binary-lhs",
        area: Area::LvalueUpdate,
        source: "int f(void) { int x = 0; (x + 1) = 2; return x; }\n",
        expected: Expected::RedDiagnostic {
            column: 26,
            code: "E2101",
            message: "expected-modifiable-lvalue",
        },
    },
    Case {
        id: "lvalue.const-lhs",
        area: Area::LvalueUpdate,
        source: "int f(void) { const int x = 1; x = 2; return x; }\n",
        expected: Expected::RedDiagnostic {
            column: 32,
            code: "E2101",
            message: "expected-modifiable-lvalue",
        },
    },
    Case {
        id: "lvalue.array-lhs",
        area: Area::LvalueUpdate,
        source: "int f(void) { int a[2]; a = 0; return 0; }\n",
        expected: Expected::RedDiagnostic {
            column: 25,
            code: "E2101",
            message: "expected-modifiable-lvalue",
        },
    },
    Case {
        id: "lvalue.add-assign",
        area: Area::LvalueUpdate,
        source: "int f(void) { int x = 1; x += 2; return x; }\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "lvalue.shift-assign",
        area: Area::LvalueUpdate,
        source: "int f(void) { int x = 1; x <<= 2; return x; }\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "lvalue.bit-assign",
        area: Area::LvalueUpdate,
        source: "int f(void) { int x = 7; x &= 3; return x; }\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "lvalue.prefix-literal",
        area: Area::LvalueUpdate,
        source: "int f(void) { ++1; return 0; }\n",
        expected: Expected::RedDiagnostic {
            column: 17,
            code: "E2101",
            message: "expected-modifiable-lvalue",
        },
    },
    Case {
        id: "lvalue.postfix-const",
        area: Area::LvalueUpdate,
        source: "int f(void) { const int x = 1; x++; return x; }\n",
        expected: Expected::RedDiagnostic {
            column: 32,
            code: "E2101",
            message: "expected-modifiable-lvalue",
        },
    },
    Case {
        id: "lvalue.right-associative",
        area: Area::LvalueUpdate,
        source: "int f(void) { int x = 0; int y = 0; x = y = 3; return x; }\n",
        expected: Expected::SilentSuccess,
    },
    // Written initializer trees and normalized scalar/aggregate/string plans (13).
    Case {
        id: "init.scalar-brace",
        area: Area::Initializer,
        source: "int x = { 3 };\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "init.scalar-nested-brace",
        area: Area::Initializer,
        source: "int x = { { 3 } };\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "init.scalar-excess",
        area: Area::Initializer,
        source: "int x = { 1, 2 };\n",
        expected: Expected::RedDiagnostic {
            column: 14,
            code: "E2201",
            message: "excess-initializer",
        },
    },
    Case {
        id: "init.array-partial",
        area: Area::Initializer,
        source: "int values[3] = { 1, 2 };\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "init.array-inferred",
        area: Area::Initializer,
        source: "int values[] = { 1, 2, 3 };\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "init.array-nested",
        area: Area::Initializer,
        source: "int values[2][2] = { { 1, 2 }, { 3 } };\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "init.array-brace-elision",
        area: Area::Initializer,
        source: "int values[2][2] = { 1, 2, 3 };\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "init.struct-partial",
        area: Area::Initializer,
        source: "struct S { int a; int b; }; struct S value = { 1 };\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "init.union-first",
        area: Area::Initializer,
        source: "union U { int a; long b; }; union U value = { 7 };\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "init.char-fixed",
        area: Area::Initializer,
        source: "char text[4] = \"hi\";\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "init.char-inferred",
        area: Area::Initializer,
        source: "char text[] = \"hi\";\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "init.char-too-long",
        area: Area::Initializer,
        source: "char text[2] = \"abc\";\n",
        expected: Expected::RedDiagnostic {
            column: 16,
            code: "E2204",
            message: "string-initializer-too-long: 3 > 2",
        },
    },
    Case {
        id: "init.trailing-comma",
        area: Area::Initializer,
        source: "int values[2] = { 1, 2, };\n",
        expected: Expected::SilentSuccess,
    },
    // Frozen extension boundaries migrate independently as their semantics land (2).
    Case {
        id: "boundary.designated-initializer",
        area: Area::Day9Boundary,
        source: "struct S { int a; int b; }; struct S value = { .b = 2 };\n",
        expected: Expected::SilentSuccess,
    },
    Case {
        id: "boundary.compound-literal",
        area: Area::Day9Boundary,
        source: "struct S { int a; }; int f(void) { return ((struct S){ 1 }).a; }\n",
        expected: Expected::SilentSuccess,
    },
];

fn run_parse_only(source: &str) -> Observation {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([
            "--parse-only",
            "--target=aarch64-unknown-linux-gnu",
            "/dev/stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn evocc");

    let mut stdin = child.stdin.take().expect("child stdin");
    stdin.write_all(source.as_bytes()).expect("write source");
    drop(stdin);

    let deadline = Instant::now() + WATCHDOG;
    loop {
        match child.try_wait().expect("poll evocc") {
            Some(_) => {
                let output = child.wait_with_output().expect("collect evocc output");
                return Observation {
                    timed_out: false,
                    rc: output.status.code(),
                    stdout: output.stdout,
                    stderr: output.stderr,
                };
            }
            None if Instant::now() >= deadline => {
                child.kill().expect("kill timed-out evocc");
                let output = child.wait_with_output().expect("collect timed-out evocc");
                return Observation {
                    timed_out: true,
                    rc: output.status.code(),
                    stdout: output.stdout,
                    stderr: output.stderr,
                };
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn expected(case: &Case) -> Observation {
    match case.expected {
        Expected::SilentSuccess => Observation {
            timed_out: false,
            rc: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        },
        Expected::RedDiagnostic {
            column,
            code,
            message,
        } => Observation {
            timed_out: false,
            rc: Some(1),
            stdout: Vec::new(),
            stderr: format!("/dev/stdin:1:{column}: error[{code}]: {message}\n").into_bytes(),
        },
    }
}

fn display(observation: &Observation) -> String {
    format!(
        "timeout={} rc={:?} stdout={:?} stderr={:?}",
        observation.timed_out,
        observation.rc,
        String::from_utf8_lossy(&observation.stdout),
        String::from_utf8_lossy(&observation.stderr)
    )
}

fn assert_topology() {
    assert_eq!(G2_CASES.len(), 36, "G2 denominator changed");
    assert_eq!(DAY_1_TO_4_CONTROLS.len(), 4, "control denominator changed");

    let mut ids = HashSet::new();
    let mut sources = HashSet::new();
    for control in DAY_1_TO_4_CONTROLS {
        assert!(ids.insert(control.id), "duplicate id {}", control.id);
        assert!(
            sources.insert(control.source.as_bytes()),
            "duplicate source bytes for {}",
            control.id
        );
        assert!(
            control.source.ends_with('\n'),
            "{} lacks newline",
            control.id
        );
    }

    let mut scope = 0;
    let mut lvalue_update = 0;
    let mut initializer = 0;
    let mut day9_boundary = 0;
    let mut silent_success = 0;
    let mut red_diagnostic = 0;
    for case in G2_CASES {
        assert!(ids.insert(case.id), "duplicate id {}", case.id);
        assert!(
            sources.insert(case.source.as_bytes()),
            "duplicate source bytes for {}",
            case.id
        );
        assert!(case.source.ends_with('\n'), "{} lacks newline", case.id);
        match case.area {
            Area::Scope => scope += 1,
            Area::LvalueUpdate => lvalue_update += 1,
            Area::Initializer => initializer += 1,
            Area::Day9Boundary => day9_boundary += 1,
        }
        match case.expected {
            Expected::SilentSuccess => silent_success += 1,
            Expected::RedDiagnostic { code, .. } => {
                red_diagnostic += 1;
                assert!(
                    code.starts_with("E2"),
                    "{} diagnostic left the semantic taxonomy",
                    case.id
                );
            }
        }
    }

    assert_eq!(ids.len(), 40, "combined id denominator changed");
    assert_eq!(sources.len(), 40, "combined source denominator changed");
    assert_eq!(
        (scope, lvalue_update, initializer, day9_boundary),
        (10, 11, 13, 2),
        "G2 topology changed"
    );
    assert_eq!(
        (silent_success, red_diagnostic),
        (22, 14),
        "G2 expectation topology changed"
    );
}

#[test]
fn day_1_to_4_controls_are_silent_green() {
    assert_topology();

    let mut mismatches = Vec::new();
    for control in DAY_1_TO_4_CONTROLS {
        let actual = run_parse_only(control.source);
        let wanted = Observation {
            timed_out: false,
            rc: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        if actual != wanted {
            mismatches.push(format!("{}: {}", control.id, display(&actual)));
        }
    }

    assert!(
        mismatches.is_empty(),
        "Day 1-4 controls regressed:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn day5_g2_contract_is_behaviorally_red() {
    assert_topology();

    let mut mismatches = Vec::new();
    for case in G2_CASES {
        let wanted = expected(case);
        let actual = run_parse_only(case.source);
        if actual != wanted {
            mismatches.push(format!(
                "{} ({:?}):\n  expected {}\n  observed {}",
                case.id,
                case.area,
                display(&wanted),
                display(&actual)
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "Day 5 G2 semantic contract is RED: {}/{} exact outcomes differ\n{}",
        mismatches.len(),
        G2_CASES.len(),
        mismatches.join("\n")
    );
}
