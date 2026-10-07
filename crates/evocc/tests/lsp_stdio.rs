//! Exercise the editor protocol through independent Content-Length framed pipes.
use std::path::Path;
use std::process::Command;

#[test]
fn framed_client_checks_diagnostics_changes_and_safe_workspace_edits() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let compiler = Path::new(env!("CARGO_BIN_EXE_evocc"));
    let result = Command::new("python3")
        .arg(root.join("tests/ui/commands/lsp_client.py"))
        .arg(compiler.with_file_name("evo"))
        .arg(&root)
        .env("EVOCC", compiler)
        .status()
        .unwrap();
    assert!(result.success());
}

#[test]
fn lsp_help_is_available_in_text_and_command_json() {
    let executable = Path::new(env!("CARGO_BIN_EXE_evocc")).with_file_name("evo");
    let help = Command::new(&executable)
        .args(["lsp", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success(), "{help:?}");
    assert_eq!(help.stdout, b"usage: evo lsp\n");
    assert!(help.stderr.is_empty());
    let json = Command::new(&executable)
        .args(["lsp", "--help", "--json"])
        .output()
        .unwrap();
    assert!(json.status.success(), "{json:?}");
    assert_eq!(
        json.stdout,
        b"{\"usage\":\"usage: evo lsp\",\"exit_code\":0}\n"
    );
    assert!(json.stderr.is_empty());
    let global = Command::new(executable).arg("help").output().unwrap();
    assert!(global.status.success());
    assert!(String::from_utf8_lossy(&global.stdout).contains("|lsp|"));
}
