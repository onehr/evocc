use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_cpp::{FileId, Lexer, Preprocessor, SourceSpan, Token, TokenStream};

static NEXT_TEMP_TREE: AtomicU64 = AtomicU64::new(0);

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_TREE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "evocc-cpp-day5-forced-include-{label}-{}-{sequence}",
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
fn forced_header_diagnostic_keeps_canonical_file_identity_and_exact_span() {
    let tree = TempTree::new("diagnostic-identity");
    let header_source = "#define READY 1\n#error forced stop\n";
    let header = tree.write("forced.h", header_source);
    let main_source = "int after;\n";
    let main = tree.write("main.c", main_source);
    let main_file = FileId::new(5_001);

    let mut preprocessor = Preprocessor::new();
    preprocessor.current_file = main.to_string_lossy().into_owned();
    preprocessor.add_forced_include(header.to_string_lossy().into_owned());
    let _ = preprocessor.process(TokenStream::new(lex(main_file, main_source)), main_source);

    assert_eq!(preprocessor.errors().len(), 1);
    let error = &preprocessor.errors()[0];
    assert_eq!(error.code, "E0104");
    assert_eq!(error.message, "error-directive: forced stop");
    assert_ne!(error.span.file, FileId::DUMMY);
    assert_ne!(error.span.file, main_file);
    assert_eq!(error.span, SourceSpan::new(error.span.file, 16, 17, 2, 1));
    assert_eq!(
        preprocessor.source_path(error.span.file),
        fs::canonicalize(header)
            .expect("canonical forced header path")
            .to_str()
    );
}
