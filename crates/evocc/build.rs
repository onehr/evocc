//! Build the explanation catalogue from the existing diagnostic authority,
//! and record the source revision that `evocc --version` prints.
use std::collections::{BTreeMap, BTreeSet};
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

fn codes(text: &str, found: &mut BTreeSet<String>) {
    for (index, bytes) in text.as_bytes().windows(5).enumerate() {
        if bytes[0] == b'E'
            && bytes[1..].iter().all(u8::is_ascii_digit)
            && (index == 0 || !text.as_bytes()[index - 1].is_ascii_alphanumeric())
            && text
                .as_bytes()
                .get(index + 5)
                .is_none_or(|b| !b.is_ascii_alphanumeric())
        {
            found.insert(std::str::from_utf8(bytes).unwrap().to_owned());
        }
    }
}

fn source_codes(dir: &Path, found: &mut BTreeSet<String>) {
    println!("cargo:rerun-if-changed={}", dir.display());
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            source_codes(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            println!("cargo:rerun-if-changed={}", path.display());
            codes(&std::fs::read_to_string(path).unwrap(), found);
        }
    }
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
    let mut entries = BTreeMap::new();
    for row in table.lines().filter(|line| line.starts_with("| `E")) {
        let fields: Vec<_> = row.split('|').map(str::trim).collect();
        assert_eq!(fields.len(), 5, "malformed diagnostic row: {row}");
        let code = fields[1].trim_matches('`');
        let previous = entries.insert(code.to_owned(), (fields[2].trim_matches('`'), fields[3]));
        assert!(previous.is_none(), "duplicate diagnostic authority: {code}");
    }
    assert!(!entries.is_empty());
    let mut known = BTreeSet::new();
    codes(&spec, &mut known);
    println!("cargo:rerun-if-changed={}", root.join("crates").display());
    for entry in std::fs::read_dir(root.join("crates")).unwrap() {
        let source = entry.unwrap().path().join("src");
        if source.is_dir() {
            source_codes(&source, &mut known);
        }
    }
    let mut generated = String::from("const CATALOGUE: &[Explanation] = &[\n");
    for code in known {
        let (message, detected) = match entries.get(&code) {
            Some((message, detected)) => {
                (format!("Some({message:?})"), format!("Some({detected:?})"))
            }
            None => ("None".into(), "None".into()),
        };
        writeln!(
            generated,
            "Explanation {{ code: {code:?}, message: {message}, detected: {detected} }},"
        )
        .unwrap();
    }
    generated.push_str("] ;\n");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(output.join("diagnostic_catalogue.rs"), generated).unwrap();
}
