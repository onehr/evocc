//! Build the explanation catalogue from the existing diagnostic authority.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;

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
    let authority = root.join("docs/evoir-spec.md");
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
