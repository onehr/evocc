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

const AGGREGATE_CASES: &[PositiveCase] = &[
    PositiveCase {
        id: "aggregate.struct.named_definition",
        source: "struct Pair { int left; long right; };\n",
    },
    PositiveCase {
        id: "aggregate.struct.forward_then_object",
        source: "struct Forward; struct Forward *forward_head;\n",
    },
    PositiveCase {
        id: "aggregate.struct.anonymous_object",
        source: "struct { int x; int y; } point;\n",
    },
    PositiveCase {
        id: "aggregate.struct.nested_member",
        source: "struct Inner { int value; }; struct Outer { struct Inner inner; };\n",
    },
    PositiveCase {
        id: "aggregate.struct.self_referential_pointer",
        source: "struct Node { int value; struct Node *next; };\n",
    },
    PositiveCase {
        id: "aggregate.union.named_definition",
        source: "union Word { int signed_value; long wide_value; };\n",
    },
    PositiveCase {
        id: "aggregate.enum.explicit_and_implicit",
        source: "enum Mode { MODE_IDLE, MODE_BUSY = 7, MODE_DONE };\n",
    },
    PositiveCase {
        id: "aggregate.function.struct_parameter",
        source: "struct Item; int inspect(struct Item *item);\n",
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

    let _ = child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(source.as_bytes());

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
fn day_4_aggregate_matrix_is_accepted_silently() {
    assert_unique(DAY_1_TO_3_CONTROLS.iter().chain(AGGREGATE_CASES), 11);
    assert_eq!(AGGREGATE_CASES.len(), 8, "aggregate denominator changed");

    let mut mismatches = Vec::new();
    for case in AGGREGATE_CASES {
        let actual = observed(run_parse_only(case.source));
        if actual != (Some(0), String::new(), String::new()) {
            mismatches.push(format!("{}: {actual:?}", case.id));
        }
    }

    assert!(
        mismatches.is_empty(),
        "valid aggregate translation units were rejected:\n{}",
        mismatches.join("\n")
    );
}
