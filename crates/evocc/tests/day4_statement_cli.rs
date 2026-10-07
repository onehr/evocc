mod support;

use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

struct PositiveCase {
    id: &'static str,
    source: &'static str,
}

const DAY_1_TO_3_CONTROLS: &[PositiveCase] = &[
    PositiveCase {
        id: "control.object",
        source: "int value;\n",
    },
    PositiveCase {
        id: "control.expression",
        source: "int sink = 1 + 2 * 3;\n",
    },
    PositiveCase {
        id: "control.function_declarator",
        source: "int compute(int left, long right);\n",
    },
];

const STATEMENT_CASES: &[PositiveCase] = &[
    PositiveCase {
        id: "function.compound.empty",
        source: "int empty(void) {}\n",
    },
    PositiveCase {
        id: "compound.declaration_expression_return",
        source: "int mixed(int input) { int local = input; local = local + 1; return local; }\n",
    },
    PositiveCase {
        id: "statement.expression.call",
        source: "int notify(void); int invoke(void) { notify(); return 0; }\n",
    },
    PositiveCase {
        id: "statement.empty",
        source: "int null_statement(void) { ; return 0; }\n",
    },
    PositiveCase {
        id: "selection.if",
        source: "int choose_one(int flag) { if (flag) return 1; return 0; }\n",
    },
    PositiveCase {
        id: "selection.if_else",
        source: "int choose_two(int flag) { if (flag) return 1; else return 2; }\n",
    },
    PositiveCase {
        id: "selection.dangling_else",
        source: "int nested_choice(int a, int b) { if (a) if (b) return 1; else return 2; return 0; }\n",
    },
    PositiveCase {
        id: "iteration.while",
        source: "int walk(int n) { while (n) n = n - 1; return n; }\n",
    },
    PositiveCase {
        id: "iteration.while_continue_break",
        source: "int scan(int n) { while (n) { n = n - 1; if (n) continue; break; } return n; }\n",
    },
    PositiveCase {
        id: "iteration.do_while",
        source: "int retry(int n) { do n = n - 1; while (n); return n; }\n",
    },
    PositiveCase {
        id: "iteration.for_empty_clauses",
        source: "int forever(void) { for (;;) break; return 0; }\n",
    },
    PositiveCase {
        id: "iteration.for_expression_init",
        source: "int count(int n) { int i; for (i = 0; i < n; i = i + 1) ; return i; }\n",
    },
    PositiveCase {
        id: "iteration.for_declaration_init",
        source: "int count_local(int n) { for (int i = 0; i < n; i = i + 1) ; return n; }\n",
    },
    PositiveCase {
        id: "selection.switch_case_default",
        source: "int classify(int n) { switch (n) { case 0: return 10; default: return 20; } }\n",
    },
    PositiveCase {
        id: "selection.switch_stacked_labels",
        source: "int stacked(int n) { switch (n) { case 0: case 1: return 10; case 2: break; default: return 20; } return 0; }\n",
    },
    PositiveCase {
        id: "jump.goto_forward_label",
        source: "int forward(int x) { goto done; x = x + 1; done: return x; }\n",
    },
    PositiveCase {
        id: "jump.label_empty_statement",
        source: "int labelled(void) { marker: ; return 0; }\n",
    },
    PositiveCase {
        id: "jump.return_without_value",
        source: "void finish(void) { return; }\n",
    },
    PositiveCase {
        id: "compound.nested_block_items",
        source: "int scoped(int x) { { int y = x; x = y; } return x; }\n",
    },
    PositiveCase {
        id: "control.nested_loop_switch",
        source: "int control(int x) { while (x) { switch (x) { case 1: break; default: x = x - 1; continue; } break; } return x; }\n",
    },
];

fn evocc() -> &'static str {
    env!("CARGO_BIN_EXE_evocc")
}

fn run_parse_only(source: &str) -> Output {
    support::warm_evocc();
    let mut child = Command::new(evocc())
        .args(["--parse-only", "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn evocc");

    let mut stdin = child.stdin.take().expect("child stdin");
    let _ = stdin.write_all(source.as_bytes());
    drop(stdin);

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait().expect("poll evocc") {
            Some(_) => return child.wait_with_output().expect("collect evocc output"),
            None if Instant::now() >= deadline => {
                child.kill().expect("kill timed-out evocc");
                let output = child.wait_with_output().expect("collect timed-out evocc");
                panic!(
                    "evocc exceeded the two-second watchdog; stdout={:?} stderr={:?}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn observed(output: Output) -> (Option<i32>, String, String) {
    (
        output.status.code(),
        String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        String::from_utf8(output.stderr).expect("stderr is UTF-8"),
    )
}

fn assert_unique(cases: impl IntoIterator<Item = &'static PositiveCase>, expected: usize) {
    let mut ids = HashSet::new();
    let mut sources = HashSet::new();
    let mut count = 0;
    for case in cases {
        count += 1;
        assert!(ids.insert(case.id), "duplicate case id {}", case.id);
        assert!(
            sources.insert(case.source.as_bytes()),
            "duplicate source bytes for {}",
            case.id
        );
    }
    assert_eq!(count, expected, "positive denominator changed");
}

#[test]
fn day_1_to_3_controls_remain_green() {
    assert_unique(DAY_1_TO_3_CONTROLS, 3);
    for case in DAY_1_TO_3_CONTROLS {
        assert_eq!(
            observed(run_parse_only(case.source)),
            (Some(0), String::new(), String::new()),
            "case {}",
            case.id
        );
    }
}

#[test]
fn statement_matrix_retains_acceptance_and_reports_missing_return() {
    assert_eq!(STATEMENT_CASES.len(), 20, "statement denominator changed");
    assert_unique(DAY_1_TO_3_CONTROLS.iter().chain(STATEMENT_CASES), 23);

    let mut mismatches = Vec::new();
    for case in STATEMENT_CASES {
        let actual = observed(run_parse_only(case.source));
        let expected_diagnostic = if case.id == "function.compound.empty" {
            // An empty non-void definition parses, but its fallthrough now warns.
            actual.2.starts_with("warning[W0005]:")
                && actual.2.matches("warning[").count() == 1
                && !actual.2.contains("error[")
        } else {
            actual.2.is_empty()
        };
        if actual.0 != Some(0) || !actual.1.is_empty() || !expected_diagnostic {
            mismatches.push(format!("{}: {actual:?}", case.id));
        }
    }

    assert!(
        mismatches.is_empty(),
        "valid statement/function-body translation units were rejected:\n{}",
        mismatches.join("\n")
    );
}
