//! The `evo-policy-eval` binary, exercised as a separate process.
//!
//! The library tests pin the rules; these pin that the binary is wired to
//! them, because a red line enforced in a library the command line never
//! reaches would protect nothing.

use std::path::PathBuf;
use std::process::Command;

fn binary(name: &str) -> PathBuf {
    let mut directory = std::env::current_exe().expect("the running test binary");
    directory.pop();
    if directory.ends_with("deps") {
        directory.pop();
    }
    let path = directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{} does not exist; build the workspace",
        path.display()
    );
    path
}

fn run(arguments: &[&str]) -> (i32, String, String) {
    run_binary("evo-policy-eval", arguments)
}

fn run_binary(name: &str, arguments: &[&str]) -> (i32, String, String) {
    let output = Command::new(binary(name))
        .env(
            "EVOCC_CHECKER_SHA256",
            evocc_proof::sha256_hex(&std::fs::read(binary("evo-proofcheck")).unwrap()),
        )
        .args(arguments)
        .output()
        .expect("run evo-policy-eval");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn a_forbidden_corpus_path_is_refused_on_the_command_line() {
    for path in [
        ".build/corpus",
        ".evidence/day7",
        "tests/contracts/day5-kernel-cohort.json",
    ] {
        let (status, stdout, stderr) = run(&[
            "--policy",
            "instruction-count",
            "--seed",
            "1",
            "--corpus",
            path,
        ]);
        assert_eq!(status, 2, "{path} was not refused");
        assert!(stdout.is_empty(), "{path} produced a report");
        assert!(stderr.contains("refused corpus path"), "{path}: {stderr}");
    }
}

#[test]
fn an_unknown_policy_names_the_ones_that_ship() {
    let (status, _, stderr) = run(&[
        "--policy",
        "learned-v1",
        "--seed",
        "1",
        "--generated",
        "1:1",
    ]);
    assert_eq!(status, 2);
    assert!(stderr.contains("instruction-count"), "{stderr}");
    assert!(stderr.contains("weighted-operators"), "{stderr}");
}

#[test]
fn the_binary_report_is_deterministic_with_timing_elided() {
    let arguments = [
        "--policy",
        "weighted-operators",
        "--seed",
        "4",
        "--generated",
        "700:6",
        "--elide-timing",
    ];
    let (first_status, first, _) = run(&arguments);
    let (second_status, second, _) = run(&arguments);
    assert_eq!(first_status, 0);
    assert_eq!(second_status, 0);
    assert_eq!(first, second);
    assert!(first.contains("\"corpusDigest\""));
    assert!(
        first.contains("\"measured\": 6"),
        "a deterministic refusal is not evaluation: {first}"
    );
    assert!(first.contains("\"optimizerWallNanos\": null"));
}

#[test]
fn a_weight_vector_measures_through_the_same_command() {
    let weights = evocc_policy::SEARCHED_WEIGHTS.render();
    let (status, report, stderr) = run(&[
        "--weights",
        &weights,
        "--seed",
        "4",
        "--generated",
        "700:6",
        "--elide-timing",
    ]);
    assert_eq!(status, 0, "{stderr}");
    assert!(report.contains("\"measured\": 6"), "{report}");
    assert!(
        report.contains(evocc_policy::SEARCHED_POLICY_NAME),
        "a searched weighting must name itself in the report: {report}"
    );
    assert!(
        report.contains("mul=3"),
        "and carry its weights into it: {report}"
    );

    // A cost function is named exactly once, by a shipped name or by a
    // vector, never by both and never by neither.
    for arguments in [
        vec!["--seed", "4", "--generated", "700:6"],
        vec![
            "--policy",
            "prefer-add",
            "--weights",
            weights.as_str(),
            "--seed",
            "4",
            "--generated",
            "700:6",
        ],
    ] {
        let (status, stdout, stderr) = run(&arguments);
        assert_eq!(status, 2, "{stderr}");
        assert!(stdout.is_empty());
        assert!(stderr.contains("exactly one of --policy and --weights"));
    }

    let (status, _, stderr) = run(&[
        "--weights",
        "add=1,mul=2",
        "--seed",
        "4",
        "--generated",
        "700:6",
    ]);
    assert_eq!(status, 2);
    assert!(stderr.contains("13 fields"), "{stderr}");
}

/// The search must not be able to train on the corpus that judges it, and the
/// command line is where a caller would try.
#[test]
fn the_search_refuses_to_train_on_the_held_out_range() {
    let (first, count) = evocc_policy::HELD_OUT_RANGE;
    let overlapping = format!("{}:{count}", first + count / 2);
    let (status, stdout, stderr) = run_binary(
        "evo-cost-search",
        &["--generated", &overlapping, "--generations", "0"],
    );
    assert_eq!(status, 2, "{stderr}");
    assert!(stdout.is_empty());
    assert!(stderr.contains("held-out"), "{stderr}");

    // The search reads no path at all -- its only corpus is a generated seed
    // range -- so no acceptance corpus can reach it even by mistake.
    let (status, stdout, stderr) = run_binary(
        "evo-cost-search",
        &["--corpus", ".build/corpus", "--generations", "0"],
    );
    assert_eq!(status, 2, "{stderr}");
    assert!(stdout.is_empty());
    assert!(stderr.contains("usage: evo-cost-search"), "{stderr}");
}
