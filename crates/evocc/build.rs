//! Build the explanation catalogue from the existing diagnostic authority,
//! and record the source revision that `evocc --version` prints.
use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::Path;
use std::process::Command;

/// The output of one git query, or nothing when git or the repository is absent.
fn git(root: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    (output.status.success() && !text.is_empty()).then(|| text.to_owned())
}

/// Record the commit and its date. A release tarball has no repository, so
/// `EVOCC_BUILD_COMMIT` and `EVOCC_BUILD_DATE` can supply both values, and
/// the fallback is `unknown`.
fn revision(root: &Path) {
    println!("cargo:rerun-if-env-changed=EVOCC_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=EVOCC_BUILD_DATE");
    if let Some(directory) = git(root, &["rev-parse", "--absolute-git-dir"]) {
        // A missing path would make the script run on every build, so only
        // existing files are watched.
        let directory = Path::new(&directory);
        let reference = git(root, &["symbolic-ref", "-q", "HEAD"]);
        let watched = [
            Some(directory.join("HEAD")),
            Some(directory.join("packed-refs")),
            reference.map(|reference| directory.join(reference)),
        ];
        for path in watched.into_iter().flatten().filter(|path| path.is_file()) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    let commit = std::env::var("EVOCC_BUILD_COMMIT")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| git(root, &["rev-parse", "--short=12", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_owned());
    let date = std::env::var("EVOCC_BUILD_DATE")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| git(root, &["log", "-1", "--format=%cs", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=EVOCC_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=EVOCC_BUILD_DATE={date}");
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    revision(&root);
    let authority = root.join("spec/evoir.md");
    println!("cargo:rerun-if-changed={}", authority.display());
    let spec = std::fs::read_to_string(authority).unwrap();
    let table = spec
        .split_once("### 6.3 Diagnostic Authority")
        .expect("diagnostic authority section")
        .1
        .split_once("### 6.4")
        .expect("end of diagnostic authority")
        .0;
    let mut entries = BTreeSet::new();
    for row in table
        .lines()
        .filter(|line| line.starts_with("| `E") || line.starts_with("| `W"))
    {
        let fields: Vec<_> = row.split('|').map(str::trim).collect();
        assert_eq!(fields.len(), 5, "malformed diagnostic row: {row}");
        let code = fields[1].trim_matches('`');
        assert!(
            entries.insert(code),
            "duplicate diagnostic authority: {code}"
        );
    }
    assert!(!entries.is_empty());
    let mut generated = String::from("const CATALOGUE: &[(&str, &str)] = &[\n");
    for code in entries {
        let page = root.join(format!("crates/evocc/explain/{code}.md"));
        println!("cargo:rerun-if-changed={}", page.display());
        writeln!(generated, "({code:?}, include_str!({page:?})),").unwrap();
    }
    generated.push_str("] ;\n");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(output.join("diagnostic_catalogue.rs"), generated).unwrap();
}
