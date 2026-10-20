//! Production dependency boundaries from EvoIR Section 11.5. Dev-only
//! producers are allowed in replay tests, but not in the checker's closure.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn violations<'a>(package: &str, dependencies: &'a BTreeSet<String>) -> Vec<&'a str> {
    dependencies
        .iter()
        .filter_map(|name| {
            let excluded =
                name == "evocc-egraph" || (package == "evocc-proofcheck" && name == "evocc-eqopt");
            excluded.then_some(name.as_str())
        })
        .collect()
}

#[test]
fn production_dependency_closures_exclude_the_untrusted_optimizer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for package in ["evocc-proofcheck", "evocc-evoir"] {
        // Cargo resolves features, renamed dependencies and target sections.
        // `tree` only reads the graph; it starts no nested compilation. All
        // targets and features count, not only the test host's configuration.
        let output = Command::new(env!("CARGO"))
            .current_dir(&root)
            .args([
                "tree",
                "--offline",
                "--locked",
                "--all-features",
                "--target",
                "all",
                "--edges",
                "normal",
                "--prefix",
                "none",
                "--format",
                "{p}",
                "-p",
                package,
            ])
            .output()
            .expect("read Cargo's normal dependency closure");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        let dependencies: BTreeSet<String> = text
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .map(str::to_owned)
            .collect();
        assert!(dependencies.contains(package), "missing graph root: {text}");
        assert!(
            dependencies.contains("evocc-types"),
            "incomplete graph: {text}"
        );
        assert!(
            violations(package, &dependencies).is_empty(),
            "{package} production closure: {text}"
        );
    }
}

#[test]
fn a_forbidden_transitive_package_is_not_hidden_by_allowed_intermediates() {
    let closure = [
        "evocc-proofcheck",
        "evocc-proof",
        "evocc-types",
        "evocc-eqopt",
        "evocc-egraph",
    ]
    .map(str::to_owned)
    .into_iter()
    .collect();
    assert_eq!(
        violations("evocc-proofcheck", &closure),
        ["evocc-egraph", "evocc-eqopt"]
    );
    assert_eq!(violations("evocc-evoir", &closure), ["evocc-egraph"]);
}
