use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_cpp::{
    FileId, Lexer, Preprocessor, SourceSpan, Token, TokenKind, TokenStream, TokenText,
};

static NEXT_TEMP_TREE: AtomicU64 = AtomicU64::new(0);

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_TREE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "evocc-day4-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create isolated source tree");
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::write(&path, contents).expect("write source fixture");
        path
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove isolated source tree");
    }
}

fn lex(file: FileId, source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    tokens
}

#[test]
fn unreadable_include_is_diagnosed_instead_of_becoming_empty() {
    let tree = TempTree::new("invalid-encoding");
    let header = tree.root.join("bytes.h");
    fs::write(&header, b"#define LOST \"\xff\"\n").unwrap();
    let main = tree.write("main.c", "#include \"bytes.h\"\nint kept;\n");
    let (pp, _) = preprocess(
        &main,
        "#include \"bytes.h\"\nint kept;\n",
        FileId::new(94003),
    );
    assert_eq!(pp.errors().len(), 1, "{:?}", pp.errors());
    assert_eq!(pp.errors()[0].code, "E0002");
}

#[test]
fn include_next_resumes_after_the_directory_that_found_the_wrapper() {
    let tree = TempTree::new("include-next");
    for dir in ["first", "second", "first/nested"] {
        fs::create_dir_all(tree.root.join(dir)).unwrap();
    }
    tree.write(
        "first/wrap.h",
        "#include \"nested/part.h\"\n#include_next <wrap.h>\n",
    );
    tree.write("first/nested/part.h", "#include_next <value.h>\n");
    tree.write("first/value.h", "wrong\n");
    tree.write("second/value.h", "value\n");
    tree.write("second/wrap.h", "wrapped\n");
    let source = "#include <wrap.h>\n";
    let mut pp = Preprocessor::new();
    pp.current_file = tree.root.join("main.c").to_string_lossy().into_owned();
    pp.set_standard_includes_enabled(false);
    for dir in ["first", "second"] {
        pp.add_include_path(tree.root.join(dir).to_string_lossy().into_owned());
    }
    let output = pp
        .process(TokenStream::new(lex(FileId::new(94004), source)), source)
        .into_vec();
    assert!(pp.errors().is_empty(), "{:?}", pp.errors());
    let text = output
        .iter()
        .filter(|t| t.kind != TokenKind::Eof)
        .filter_map(|t| match &t.text {
            TokenText::Decoded(s) => Some(s.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(text, ["value", "wrapped"]);
}

fn preprocess(main_path: &Path, source: &str, main_file: FileId) -> (Preprocessor, Vec<Token>) {
    let mut preprocessor = Preprocessor::new();
    preprocessor.current_file = main_path.to_string_lossy().into_owned();
    preprocessor.add_include_path(
        main_path
            .parent()
            .expect("main file has a parent")
            .to_string_lossy()
            .into_owned(),
    );
    let tokens = preprocessor
        .process(TokenStream::new(lex(main_file, source)), source)
        .into_vec();
    (preprocessor, tokens)
}

fn spelling<'a>(token: &'a Token, main_source: &'a str) -> &'a str {
    match &token.text {
        TokenText::Decoded(text) => text,
        TokenText::Borrowed => {
            &main_source[token.span.byte_start as usize..token.span.byte_end as usize]
        }
    }
}

fn identifier_file_ids(tokens: &[Token], main_source: &str, name: &str) -> Vec<FileId> {
    tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Identifier && spelling(token, main_source) == name)
        .map(|token| token.span.file)
        .collect()
}

#[test]
fn actual_includes_have_stable_distinct_file_ids() {
    let tree = TempTree::new("cpp-file-ids");
    let alpha_path = tree.write("alpha.h", "int alpha_value;\n");
    let beta_path = tree.write("beta.h", "int beta_value;\n");
    let main_source = concat!(
        "#include \"alpha.h\"\n",
        "#include \"beta.h\"\n",
        "#include \"./alpha.h\"\n",
        "int main_value;\n",
    );
    let main_path = tree.write("main.c", main_source);
    let main_file = FileId::new(4_001);

    let (preprocessor, tokens) = preprocess(&main_path, main_source, main_file);
    let canonical_alpha = fs::canonicalize(&alpha_path).expect("canonical alpha.h path");
    let canonical_beta = fs::canonicalize(&beta_path).expect("canonical beta.h path");
    let alpha = identifier_file_ids(&tokens, main_source, "alpha_value");
    let beta = identifier_file_ids(&tokens, main_source, "beta_value");
    let main = identifier_file_ids(&tokens, main_source, "main_value");

    assert_eq!(alpha.len(), 2, "alpha.h must be observed twice");
    assert_eq!(beta.len(), 1, "beta.h must be observed once");
    assert_eq!(
        main,
        vec![main_file],
        "the caller-owned main FileId changed"
    );
    assert_eq!(
        alpha[0], alpha[1],
        "one resolved header path must reuse one stable FileId"
    );
    assert_ne!(alpha[0], FileId::DUMMY, "header FileId must be real");
    assert_ne!(beta[0], FileId::DUMMY, "header FileId must be real");
    assert_ne!(alpha[0], main_file, "alpha.h was attributed to main.c");
    assert_ne!(beta[0], main_file, "beta.h was attributed to main.c");
    assert_ne!(
        alpha[0], beta[0],
        "distinct resolved header paths collapsed to one FileId"
    );
    assert_eq!(
        preprocessor.source_path(alpha[0]),
        canonical_alpha.to_str(),
        "alpha.h FileId did not resolve back to alpha.h"
    );
    assert_eq!(
        preprocessor.source_path(beta[0]),
        canonical_beta.to_str(),
        "beta.h FileId did not resolve back to beta.h"
    );
    assert_eq!(
        preprocessor.source_path(main_file),
        main_path.to_str(),
        "main FileId did not resolve back to main.c"
    );
}

#[test]
fn file_macro_uses_each_resolved_header_path() {
    let tree = TempTree::new("cpp-file-paths");
    let alpha_path = tree.write("alpha.h", "const char *alpha_path = __FILE__;\n");
    let beta_path = tree.write("beta.h", "const char *beta_path = __FILE__;\n");
    let main_source = concat!(
        "#include \"alpha.h\"\n",
        "#include \"beta.h\"\n",
        "int main_value;\n",
    );
    let main_path = tree.write("main.c", main_source);

    let (_, tokens) = preprocess(&main_path, main_source, FileId::new(4_011));
    let file_macros = tokens
        .iter()
        .filter(|token| token.kind == TokenKind::StringLiteral)
        .map(|token| spelling(token, main_source).to_owned())
        .collect::<Vec<_>>();
    let expected = vec![
        format!(
            "\"{}\"",
            fs::canonicalize(alpha_path)
                .expect("canonical alpha.h path")
                .to_string_lossy()
        ),
        format!(
            "\"{}\"",
            fs::canonicalize(beta_path)
                .expect("canonical beta.h path")
                .to_string_lossy()
        ),
    ];

    assert_eq!(
        file_macros, expected,
        "include recursion must switch and restore the active source file"
    );
}

#[cfg(unix)]
#[test]
fn include_identity_preserves_symlink_parent_resolution() {
    use std::os::unix::fs::symlink;

    let tree = TempTree::new("cpp-symlink-parent");
    let real = tree.root.join("real");
    let nested = real.join("nested");
    fs::create_dir_all(&nested).expect("create real include tree");
    let header = real.join("through_symlink.h");
    fs::write(&header, "int through_symlink;\n").expect("write symlinked header");
    symlink(&nested, tree.root.join("link")).expect("create include-directory symlink");

    let main_source = concat!(
        "#include \"link/../through_symlink.h\"\n",
        "#include \"real/through_symlink.h\"\n",
        "int main_value;\n",
    );
    let main_path = tree.write("main.c", main_source);
    let (preprocessor, tokens) = preprocess(&main_path, main_source, FileId::new(4_021));
    let header_ids = identifier_file_ids(&tokens, main_source, "through_symlink");
    let canonical_header = fs::canonicalize(&header).expect("canonical header path");

    assert_eq!(
        header_ids.len(),
        2,
        "an existing include must not disappear while normalizing its path"
    );
    assert_eq!(
        header_ids[0], header_ids[1],
        "two filesystem spellings of one header must reuse its FileId"
    );
    assert_eq!(
        preprocessor.source_path(header_ids[0]),
        canonical_header.to_str(),
        "header identity must use the path the filesystem actually resolved"
    );
}

#[test]
fn top_level_process_starts_a_fresh_source_table() {
    let tree = TempTree::new("cpp-reused-preprocessor");
    let first_source = "int first_root;\n";
    let second_source = "int second_root;\n";
    let first_path = tree.write("first.c", first_source);
    let second_path = tree.write("second.c", second_source);
    let root_file = FileId::new(1);
    let mut preprocessor = Preprocessor::new();

    preprocessor.current_file = first_path.to_string_lossy().into_owned();
    let first = preprocessor
        .process(TokenStream::new(lex(root_file, first_source)), first_source)
        .into_vec();
    assert_eq!(
        identifier_file_ids(&first, first_source, "first_root"),
        vec![root_file]
    );
    assert_eq!(preprocessor.source_path(root_file), first_path.to_str());

    preprocessor.current_file = second_path.to_string_lossy().into_owned();
    let second = preprocessor
        .process(
            TokenStream::new(lex(root_file, second_source)),
            second_source,
        )
        .into_vec();
    assert_eq!(
        identifier_file_ids(&second, second_source, "second_root"),
        vec![root_file]
    );
    assert_eq!(preprocessor.source_path(root_file), second_path.to_str());
}

#[test]
fn allocator_avoids_every_observed_id_even_at_u32_max() {
    let tree = TempTree::new("cpp-max-file-id");
    let header = tree.write("maximum.h", "int maximum_header;\n");
    let main_source = "#include \"maximum.h\"\nint main_value;\n";
    let main_path = tree.write("main.c", main_source);
    let root_file = FileId::new(1);
    let maximum_file = FileId::new(u32::MAX);
    let mut input = lex(root_file, main_source);
    input.last_mut().expect("main EOF token").span.file = maximum_file;

    let mut preprocessor = Preprocessor::new();
    preprocessor.current_file = main_path.to_string_lossy().into_owned();
    preprocessor.add_include_path(tree.root.to_string_lossy().into_owned());
    let tokens = preprocessor
        .process(TokenStream::new(input), main_source)
        .into_vec();
    let header_ids = identifier_file_ids(&tokens, main_source, "maximum_header");

    assert_eq!(header_ids.len(), 1);
    assert_ne!(header_ids[0], FileId::DUMMY);
    assert_ne!(header_ids[0], root_file);
    assert_ne!(header_ids[0], maximum_file);
    assert_eq!(
        preprocessor.source_path(header_ids[0]),
        fs::canonicalize(header)
            .expect("canonical maximum.h path")
            .to_str()
    );
}

#[test]
fn unmatched_conditional_directives_have_exact_diagnostics() {
    let cases = [
        ("orphan-elif", "#elif 1"),
        ("orphan-else", "#else"),
        ("orphan-endif", "#endif"),
        ("unterminated-if", "#if 1"),
    ];
    let mut mismatches = Vec::new();

    for (index, (name, directive)) in cases.into_iter().enumerate() {
        let source = format!("int before;\n{directive}\nint after;\n");
        let file = FileId::new(4_100 + index as u32);
        let path = format!("/virtual/{name}.c");
        let observed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut preprocessor = Preprocessor::new();
            preprocessor.current_file = path.clone();
            let _ = preprocessor.process(TokenStream::new(lex(file, &source)), &source);
            preprocessor
        }));
        let Ok(preprocessor) = observed else {
            mismatches.push(format!("{name}: panicked"));
            continue;
        };
        if preprocessor.errors().len() != 1 {
            mismatches.push(format!(
                "{name}: expected one diagnostic, got {:?}",
                preprocessor.errors()
            ));
            continue;
        }
        let error = &preprocessor.errors()[0];
        let expected_span = SourceSpan::new(file, 12, 13, 2, 1);
        if error.code != "E0102"
            || error.message != "unmatched-conditional"
            || error.span != expected_span
            || preprocessor.source_path(error.span.file) != Some(path.as_str())
        {
            mismatches.push(format!("{name}: wrong diagnostic {error:?}"));
        }
    }

    assert!(
        mismatches.is_empty(),
        "unmatched conditional contract failed:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn balanced_nested_conditionals_remain_error_free() {
    let source = concat!(
        "#if 1\n",
        "#if 0\n",
        "int hidden;\n",
        "#else\n",
        "int visible;\n",
        "#endif\n",
        "#endif\n",
    );
    let mut preprocessor = Preprocessor::new();
    preprocessor.current_file = "/virtual/balanced.c".into();
    let output = preprocessor
        .process(TokenStream::new(lex(FileId::new(4_200), source)), source)
        .into_vec();

    assert!(preprocessor.errors().is_empty());
    assert_eq!(identifier_file_ids(&output, source, "visible").len(), 1);
    assert!(identifier_file_ids(&output, source, "hidden").is_empty());
}

#[test]
fn macro_rescan_cannot_create_a_conditional_directive() {
    let source = "#define GENERATED #if 1\nGENERATED\n";
    let mut preprocessor = Preprocessor::new();
    preprocessor.current_file = "/virtual/macro-rescan.c".into();
    let _ = preprocessor.process(TokenStream::new(lex(FileId::new(4_201), source)), source);

    assert!(
        preprocessor.errors().is_empty(),
        "macro replacement tokens are not preprocessing directive lines"
    );
}

#[test]
fn conditional_groups_cannot_cross_an_include_boundary() {
    struct BoundaryCase {
        name: &'static str,
        header: &'static str,
        main: &'static str,
        expected_paths: &'static [&'static str],
        expected_lines: &'static [u32],
    }
    let cases = [
        BoundaryCase {
            name: "header-closes-caller",
            header: "#endif\nint header_value;\n",
            main: "#if 1\n#include \"cross.h\"\n#endif\nint root;\n",
            expected_paths: &["header"],
            expected_lines: &[1],
        },
        BoundaryCase {
            name: "caller-closes-header",
            header: "#if 1\nint header_value;\n",
            main: "#include \"cross.h\"\n#endif\nint root;\n",
            expected_paths: &["header", "main"],
            expected_lines: &[1, 2],
        },
    ];
    let mut mismatches = Vec::new();

    for (index, case) in cases.into_iter().enumerate() {
        let tree = TempTree::new(case.name);
        let header = tree.write("cross.h", case.header);
        let main = tree.write("main.c", case.main);
        let root_file = FileId::new(4_300 + index as u32);
        let observed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            preprocess(&main, case.main, root_file).0
        }));
        let Ok(preprocessor) = observed else {
            mismatches.push(format!("{}: panicked", case.name));
            continue;
        };
        let canonical_header = fs::canonicalize(header).expect("canonical cross.h path");
        if preprocessor.errors().len() != case.expected_paths.len() {
            mismatches.push(format!(
                "{}: expected {} diagnostics, got {:?}",
                case.name,
                case.expected_paths.len(),
                preprocessor.errors()
            ));
            continue;
        }
        for (error_index, error) in preprocessor.errors().iter().enumerate() {
            let expected_path = if case.expected_paths[error_index] == "header" {
                canonical_header.to_str()
            } else {
                main.to_str()
            };
            if error.code != "E0102"
                || error.message != "unmatched-conditional"
                || error.span.line_start != case.expected_lines[error_index]
                || error.span.column_start != 1
                || preprocessor.source_path(error.span.file) != expected_path
            {
                mismatches.push(format!(
                    "{} diagnostic {error_index}: wrong error {error:?}",
                    case.name
                ));
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "include conditional boundary contract failed:\n{}",
        mismatches.join("\n")
    );
}
