//! N03a: the generic machine layer names no instruction set, and every
//! migrated target table has exactly one owner.
//!
//! The scan covers comments too: a generic algorithm explained in terms of one
//! target's registers or mnemonics is the first step towards depending on it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn rust_sources(directory: &Path) -> Vec<(PathBuf, String)> {
    let mut pending = vec![directory.to_path_buf()];
    let mut sources = Vec::new();
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            pending.extend(
                std::fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            sources.push((path, text));
        }
    }
    sources.sort();
    assert!(
        !sources.is_empty(),
        "{} has no sources",
        directory.display()
    );
    sources
}

fn crate_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn identifiers(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
}

fn forbidden_lowercase() -> BTreeSet<String> {
    let mut words: BTreeSet<String> = [
        // Instruction sets, ABIs and object formats.
        "aarch64",
        "arm64",
        "armv8",
        "aapcs",
        "aapcs64",
        "darwin",
        "linux",
        "macho",
        "elf",
        "x86",
        "x86_64",
        "amd64",
        "riscv",
        "riscv64",
        // Special registers and condition state.
        "sp",
        "wsp",
        "xzr",
        "wzr",
        "fp",
        "lr",
        "pc",
        "nzcv",
        "fpcr",
        "fpsr",
        "sp_el0",
        "tpidr_el0",
        // Mnemonics the AArch64 emitter prints.
        "adr",
        "adrp",
        "ldr",
        "ldur",
        "ldp",
        "stp",
        "stur",
        "movz",
        "movk",
        "movn",
        "blr",
        "csel",
        "cset",
        "csinc",
        "csneg",
        "cbz",
        "cbnz",
        "tbz",
        "tbnz",
        "mrs",
        "msr",
        "paciasp",
        "autiasp",
        "xpaclri",
        "ldxr",
        "stxr",
        "ldaxr",
        "stlxr",
        "sxtw",
        "uxtw",
        "sxtb",
        "uxtb",
        "sxth",
        "uxth",
        "madd",
        "msub",
        "umulh",
        "smulh",
        "udiv",
        "sdiv",
        "eor",
        "orr",
        "bic",
        "adds",
        "subs",
        "fmov",
        "fadd",
        "fsub",
        "fmul",
        "fdiv",
        "fcvt",
        "scvtf",
        "ucvtf",
        "fcmp",
        "brk",
        "dmb",
        "dsb",
        "isb",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    // Architectural register spellings in every width.
    for prefix in ["x", "w", "v", "q", "d", "s", "h", "b"] {
        for number in 0..32 {
            words.insert(format!("{prefix}{number}"));
        }
    }
    words
}

/// Target types and enums that only the ISA adapter may name.
const FORBIDDEN_TYPES: &[&str] = &[
    "MInst",
    "MTerm",
    "RegClass",
    "AsmBody",
    "AsmOperand",
    "AsmConstraint",
    "AsmClobber",
    "Aarch64Allocation",
    "Aarch64Abi",
];

#[test]
fn generic_modules_contain_no_isa_names_or_imports() {
    let forbidden = forbidden_lowercase();
    let mut hits = Vec::new();
    for (path, text) in rust_sources(&crate_source().join("generic")) {
        for (number, line) in text.lines().enumerate() {
            let at = format!("{}:{}", path.display(), number + 1);
            for word in identifiers(line) {
                if forbidden.contains(&word.to_ascii_lowercase()) {
                    hits.push(format!("{at}: ISA name {word}"));
                }
                if FORBIDDEN_TYPES.contains(&word) {
                    hits.push(format!("{at}: target type {word}"));
                }
            }
            // The generic layer reaches only itself; target facts arrive
            // through its traits and description structures.
            let mut rest = line;
            while let Some(index) = rest.find("crate::") {
                rest = &rest[index + "crate::".len()..];
                if !rest.starts_with("generic") {
                    hits.push(format!(
                        "{at}: imports crate::{}",
                        identifiers(rest).next().unwrap_or("")
                    ));
                }
            }
            if line.contains("super::super") || line.contains("isa::") || line.contains("Target::")
            {
                hits.push(format!("{at}: leaves the generic layer"));
            }
        }
    }
    assert!(hits.is_empty(), "{}", hits.join("\n"));
}

#[test]
fn migrated_target_tables_have_one_owner() {
    // Generated tables are the owners; handwritten code may only consume them.
    let redeclarations = [
        "const MAX_REGISTER_ARGUMENTS",
        "const MAX_REGISTER_RESULTS",
        "const RECORD_REGISTER_LIMIT",
        "const REGISTER_SAVE_BYTES",
        "const VECTOR_SAVE_BYTES",
        "const VA_STACK",
        "const VA_GR_TOP",
        "const VA_VR_TOP",
        "const VA_GR_OFFS",
        "const VA_VR_OFFS",
        "const CLOBBERED",
        "const INDIRECT_RESULT",
        "const BOUNDARY",
        "fn refuses_stack_arguments",
        "fn argument_bank",
        "fn indirect_result_register",
        "[19, 20, 21, 22, 23, 24, 25, 26, 27, 28]",
        "[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]",
        "[64, 65, 66, 67, 68, 69, 70, 71",
    ];
    let mut hits = Vec::new();
    let mut cursors = Vec::new();
    for (path, text) in rust_sources(&crate_source()) {
        if text.starts_with("// Generated by tools/generate-target.py") {
            continue;
        }
        for (number, line) in text.lines().enumerate() {
            for needle in redeclarations {
                if line.contains(needle) {
                    hits.push(format!("{}:{}: {needle}", path.display(), number + 1));
                }
            }
            if line.contains("struct ArgumentCursor") {
                cursors.push(path.clone());
            }
        }
    }
    assert!(hits.is_empty(), "{}", hits.join("\n"));
    assert_eq!(
        cursors,
        [crate_source().join("generic/abi.rs")],
        "one shared argument cursor"
    );
}

fn constants(text: &str) -> BTreeSet<&str> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let declaration = line
                .strip_prefix("pub const ")
                .or_else(|| line.strip_prefix("const "))?;
            let (name, _) = declaration.split_once(':')?;
            let name = name.trim();
            (name != "DIGEST" && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
                .then_some(name)
        })
        .collect()
}

#[test]
fn cost_encoding_and_pattern_tables_have_no_second_constant_owner() {
    let sources = rust_sources(&crate_source());
    for file in ["costs.rs", "encodings.rs", "patterns.rs"] {
        let owner = crate_source().join("isa/aarch64").join(file);
        let text = &sources.iter().find(|(path, _)| path == &owner).unwrap().1;
        assert!(text.starts_with("// Generated by tools/generate-target.py from "));
        let names = constants(text);
        assert!(!names.is_empty(), "no facts read from {file}");
        for (path, contents) in &sources {
            if path == &owner {
                continue;
            }
            let declared = constants(contents);
            let copies = names.intersection(&declared).collect::<Vec<_>>();
            assert!(
                copies.is_empty(),
                "{} duplicates {file}: {copies:?}",
                path.display()
            );
        }
    }
}

#[test]
fn the_table_scan_detects_a_handwritten_copy_from_each_new_owner() {
    for (file, name) in [
        ("costs.rs", "WIDE_BYTES"),
        ("encodings.rs", "ADD_SUB_MAX"),
        ("patterns.rs", "CONSTANT_PATTERNS"),
    ] {
        let text = std::fs::read_to_string(crate_source().join("isa/aarch64").join(file)).unwrap();
        assert!(constants(&text).contains(name), "missing owner for {name}");
        for prefix in ["const", "pub const"] {
            let copy = format!("{prefix} {name}: u64 = 0;");
            assert!(constants(&copy).contains(name), "missed {copy}");
        }
    }
}
