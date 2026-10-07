//! Drive the installed stdio server with an independent JSON implementation.
use std::path::Path;
use std::process::Command;

#[test]
fn stdio_client_checks_all_five_tools_and_protocol_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let compiler = Path::new(env!("CARGO_BIN_EXE_evocc"));
    let result = Command::new("python3")
        .arg(root.join("tests/ui/commands/mcp_client.py"))
        .arg(compiler.with_file_name("evo"))
        .arg(&root)
        .env("EVOCC", compiler)
        .env(
            "EVOCC_PROOF_CHECKER",
            root.join("lean/.lake/build/bin/evoir-sem"),
        )
        .status()
        .unwrap();
    assert!(result.success());
}

#[test]
fn agent_instructions_are_identical_and_name_the_safety_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let agents = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(agents.lines().count() <= 120);
    assert_eq!(
        agents,
        std::fs::read_to_string(root.join("llms.txt")).unwrap()
    );
    for rule in [
        "run evo check before evocc -c",
        "a review repair needs a human",
    ] {
        assert!(agents.to_ascii_lowercase().contains(rule));
    }
}
