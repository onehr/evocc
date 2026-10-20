//! Explanation pages are executable examples, not a second diagnostic registry.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The codes of the diagnostic authority table.
fn authority() -> BTreeSet<String> {
    let spec = std::fs::read_to_string(root().join("spec/evoir.md")).unwrap();
    let table = spec
        .split_once("### 6.3 Diagnostic Authority")
        .unwrap()
        .1
        .split_once("### 6.4")
        .unwrap()
        .0;
    table
        .lines()
        .filter(|line| line.starts_with("| `E") || line.starts_with("| `W"))
        .map(|line| {
            line.split('|')
                .nth(1)
                .unwrap()
                .trim()
                .trim_matches('`')
                .to_owned()
        })
        .collect()
}

/// The `CODE<TAB>HINT` lines of the concise repair hints.
fn hints() -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(root().join("crates/evocc/explain/hints.txt")).unwrap();
    text.lines()
        .map(|line| {
            let (code, hint) = line.split_once('\t').expect("CODE<TAB>HINT");
            (code.to_owned(), hint.to_owned())
        })
        .collect()
}

/// A JSON string for page text, which has no control characters.
fn json_string(text: &str) -> String {
    assert!(!text.chars().any(char::is_control), "{text}");
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn run(args: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn every_authoritative_page_has_tested_examples_and_exact_cli_text() {
    let codes = authority();
    let hints = hints();
    let directory = root().join("crates/evocc/explain");
    let pages: BTreeSet<_> = std::fs::read_dir(&directory)
        .expect("explanation pages")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .map(|path| path.file_stem().unwrap().to_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        pages, codes,
        "pages must match the authority table, not source-code mentions"
    );
    let mut untriggered = Vec::new();
    for code in codes {
        let page = std::fs::read_to_string(directory.join(format!("{code}.md"))).unwrap();
        assert!(page.lines().count() <= 40, "{code}: page exceeds 40 lines");
        assert!(
            page.starts_with(&format!("# {code}: ")),
            "{code}: missing title"
        );
        let text = page
            .lines()
            .filter(|line| !line.starts_with("```"))
            .map(|line| format!("{line}\n"))
            .collect::<String>();
        let explained = run(&["--explain", &code], "");
        assert!(explained.status.success(), "{code}: explain failed");
        assert!(
            explained.stderr.is_empty(),
            "{code}: explain emitted a diagnostic"
        );
        assert_eq!(String::from_utf8(explained.stdout).unwrap(), text, "{code}");
        let no_trigger = page.lines().any(|line| line == "Trigger: none");
        let mut block = None;
        let mut source = String::new();
        let (mut triggered, mut fixed) = (0, 0);
        for line in page.lines() {
            if let Some(tag) = line.strip_prefix("```c,") {
                assert!(block.is_none(), "{code}: nested example");
                assert!(tag == code || tag == "fixed", "{code}: unknown example tag");
                block = Some(tag);
                source.clear();
            } else if line == "```" {
                let tag = block.take().expect("closing fence without example");
                let mut flags = vec!["--check", "--error-format=json"];
                flags.extend(
                    page.lines()
                        .find_map(|line| line.strip_prefix("Flags: "))
                        .unwrap_or("")
                        .split_whitespace(),
                );
                flags.push("/dev/stdin");
                let output = run(&flags, &source);
                let diagnostics = String::from_utf8(output.stderr).unwrap();
                if tag == "fixed" {
                    fixed += 1;
                    assert!(
                        output.status.success() && diagnostics.is_empty(),
                        "{code}: {diagnostics}"
                    );
                } else {
                    triggered += 1;
                    assert_eq!(output.status.code(), Some(1), "{code}: {diagnostics}");
                    assert!(
                        diagnostics
                            .lines()
                            .next()
                            .unwrap_or("")
                            .starts_with(&format!("{{\"code\":\"{code}\",")),
                        "{code}: first diagnostic: {diagnostics}"
                    );
                    // The concise record takes `why` from the page and `fix` from the hints.
                    flags[1] = "--error-format=concise";
                    let concise = String::from_utf8(run(&flags, &source).stderr).unwrap();
                    let first = concise.lines().next().unwrap_or("");
                    let why = page.split("\n\n").nth(1).unwrap().lines().next().unwrap();
                    assert!(
                        first.starts_with(&format!("{{\"code\":\"{code}\","))
                            && first.contains(&format!(",\"why\":{}", json_string(why))),
                        "{code}: concise why: {first}"
                    );
                    match hints.get(&code) {
                        Some(hint) => assert!(
                            first.contains(&format!(",\"fix\":{}", json_string(hint))),
                            "{code}: concise fix: {first}"
                        ),
                        None => assert!(!first.contains(",\"fix\":"), "{code}: {first}"),
                    }
                }
            } else if block.is_some() {
                source.push_str(line);
                source.push('\n');
            } else {
                assert!(!line.starts_with("```"), "{code}: untagged example");
            }
        }
        assert!(block.is_none(), "{code}: unclosed example");
        if no_trigger {
            assert_eq!(triggered, 0, "{code}: contradictory Trigger: none");
            assert!(
                page.lines()
                    .any(|line| line.starts_with("Reason: ") && line.len() > 16),
                "{code}: document why no C trigger exists"
            );
            untriggered.push(code);
        } else {
            assert_eq!(triggered, 1, "{code}: one triggering example required");
            assert!(
                fixed > 0 || page.contains("Fix: none"),
                "{code}: fixed example required"
            );
        }
    }
    println!("Trigger: none: {}", untriggered.join(", "));
}

#[test]
fn hints_are_one_short_action_line_per_authority_code() {
    let codes = authority();
    let text = std::fs::read_to_string(root().join("crates/evocc/explain/hints.txt")).unwrap();
    assert!(text.ends_with('\n') && !text.contains("\n\n") && !text.contains('\r'));
    let mut seen = BTreeSet::new();
    for line in text.lines() {
        let (code, hint) = line.split_once('\t').expect("CODE<TAB>HINT");
        assert!(codes.contains(code), "{code}: not in the authority table");
        assert!(seen.insert(code), "{code}: more than one hint");
        assert!(
            !hint.is_empty() && hint.len() <= 120,
            "{code}: a hint has 1 to 120 bytes"
        );
        assert!(
            hint.ends_with('.') && !hint.contains('\t'),
            "{code}: a hint is one line that ends with a period"
        );
    }
    // P17a registers a different E3004 text separately. This hint keeps the
    // action sentence of the page.
    let page = std::fs::read_to_string(root().join("crates/evocc/explain/E3004.md")).unwrap();
    let sentence = page.split("\n\n").nth(2).unwrap().lines().last().unwrap();
    assert_eq!(hints()["E3004"], sentence);
}

#[test]
fn generated_error_index_is_current_and_linked_from_the_book() {
    assert!(Command::new("python3")
        .current_dir(root())
        .args(["tools/gen-error-index.py", "--check"])
        .status()
        .unwrap()
        .success());
    let summary = std::fs::read_to_string(root().join("book/src/SUMMARY.md")).unwrap();
    assert!(summary.contains("](error-index.md)"));
}

#[test]
fn an_unknown_code_has_no_explanation() {
    let output = run(&["--explain", "E9999"], "");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("no explanation for code E9999"));
}
