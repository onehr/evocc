use std::path::PathBuf;
use std::process::Command;

#[test]
fn host_compiler_programs_are_excluded_from_the_target_inventory() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = project.join("tests/acceptance/full-config-survey-inventory-selftest.py");
    let output = Command::new("python3")
        .arg(script)
        .current_dir(project)
        .output()
        .expect("run inventory self-test");
    assert!(
        output.status.success(),
        "inventory self-test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
