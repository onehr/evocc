use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_cpp::{FileId, Lexer, Preprocessor, Token, TokenKind, TokenStream, TokenText};

static NEXT_TEMP_TREE: AtomicU64 = AtomicU64::new(0);

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new() -> Self {
        let sequence = NEXT_TEMP_TREE.fetch_add(1, Ordering::Relaxed);
        let requested = std::env::temp_dir().join(format!(
            "evocc-cpp-day5-macro-prefix-map-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&requested).expect("create isolated source tree");
        let root = fs::canonicalize(requested).expect("canonical temporary source tree");
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
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

fn spelling<'a>(token: &'a Token, main_source: &'a str) -> &'a str {
    match &token.text {
        TokenText::Decoded(text) => text,
        TokenText::Borrowed => {
            &main_source[token.span.byte_start as usize..token.span.byte_end as usize]
        }
    }
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("fixture path is UTF-8")
}

#[test]
fn file_mapping_is_separate_from_physical_source_identity() {
    let tree = TempTree::new();
    let header = tree.write(
        "headers/identity.h",
        "const char *header_file = __FILE__;\n",
    );
    let main_source = concat!(
        "#include \"headers/identity.h\"\n",
        "const char *main_file = __FILE__;\n",
    );
    let main = tree.write("main.c", main_source);
    let main_file = FileId::new(5_101);
    let mut preprocessor = Preprocessor::new();
    preprocessor.current_file = path_text(&main).to_owned();
    preprocessor.add_macro_prefix_map(path_text(&tree.root).to_owned(), "visible".to_owned());

    let tokens = preprocessor
        .process(TokenStream::new(lex(main_file, main_source)), main_source)
        .into_vec();

    assert!(preprocessor.errors().is_empty());
    let file_names = tokens
        .iter()
        .filter(|token| token.kind == TokenKind::StringLiteral)
        .map(|token| spelling(token, main_source).to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        file_names,
        vec![
            "\"visible/headers/identity.h\"".to_owned(),
            "\"visible/main.c\"".to_owned(),
        ]
    );

    let header_file = tokens
        .iter()
        .find(|token| {
            token.kind == TokenKind::Identifier && spelling(token, main_source) == "header_file"
        })
        .expect("header declaration token")
        .span
        .file;
    assert_eq!(
        preprocessor.source_path(header_file),
        Some(path_text(&header))
    );
    assert_eq!(preprocessor.source_path(main_file), Some(path_text(&main)));
    assert_eq!(preprocessor.current_file, path_text(&main));
}
