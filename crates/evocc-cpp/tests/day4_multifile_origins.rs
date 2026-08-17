use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_cpp::{FileId, Lexer, Preprocessor, Token, TokenKind, TokenStream, TokenText};

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

fn preprocess(main_path: &Path, source: &str, main_file: FileId) -> Vec<Token> {
    let mut preprocessor = Preprocessor::new();
    preprocessor.current_file = main_path.to_string_lossy().into_owned();
    preprocessor.add_include_path(
        main_path
            .parent()
            .expect("main file has a parent")
            .to_string_lossy()
            .into_owned(),
    );
    preprocessor
        .process(TokenStream::new(lex(main_file, source)), source)
        .into_vec()
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
    tree.write("alpha.h", "int alpha_value;\n");
    tree.write("beta.h", "int beta_value;\n");
    let main_source = concat!(
        "#include \"alpha.h\"\n",
        "#include \"beta.h\"\n",
        "#include \"alpha.h\"\n",
        "int main_value;\n",
    );
    let main_path = tree.write("main.c", main_source);
    let main_file = FileId::new(4_001);

    let tokens = preprocess(&main_path, main_source, main_file);
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

    let tokens = preprocess(&main_path, main_source, FileId::new(4_011));
    let file_macros = tokens
        .iter()
        .filter(|token| token.kind == TokenKind::StringLiteral)
        .map(|token| spelling(token, main_source).to_owned())
        .collect::<Vec<_>>();
    let expected = vec![
        format!("\"{}\"", alpha_path.to_string_lossy()),
        format!("\"{}\"", beta_path.to_string_lossy()),
    ];

    assert_eq!(
        file_macros, expected,
        "include recursion must switch and restore the active source file"
    );
}
