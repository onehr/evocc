//! Check authored diagnostic text, not user text substituted into a template.
use std::path::Path;
use std::process::Command;

fn check(rule: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("python3")
        .arg("crates/evocc/tests/diagnostic_messages.py")
        .arg(rule)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn header_starts_with_a_lowercase_letter() {
    check("lowercase");
}

#[test]
fn header_has_one_sentence() {
    check("sentence");
}

#[test]
fn header_has_no_final_period() {
    check("period");
}

#[test]
fn header_does_not_repeat_the_error_prefix() {
    check("prefix");
}

#[test]
fn primary_label_describes_a_finding() {
    check("label");
}

#[test]
fn note_states_a_fact_not_an_instruction() {
    check("note");
}

#[test]
fn help_starts_with_an_action() {
    check("help");
}

#[test]
fn message_inventory_reads_literal_forms_and_excludes_test_data() {
    check("inventory");
}
