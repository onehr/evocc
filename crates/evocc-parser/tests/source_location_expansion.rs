use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_chir::{CHirDecl, CHirUnit};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_cpp::{Lexer, Preprocessor, Token, TokenKind, TokenStream};
use evocc_parser::Parser;

static NEXT_TREE: AtomicU64 = AtomicU64::new(0);

struct SourceTree {
    root: PathBuf,
}

impl SourceTree {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TREE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "evocc-source-location-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create isolated source tree");
        Self { root }
    }

    fn write(&self, relative: &str, source: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::write(&path, source).expect("write source fixture");
        path
    }
}

impl Drop for SourceTree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove isolated source tree");
    }
}

fn lex(file: FileId, source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    tokens
}

fn parse_preprocessed<'arena>(
    source_path: &Path,
    source: &str,
    arena: &'arena Arena,
) -> CHirUnit<'arena> {
    let file = FileId::new(92_001);
    let mut preprocessor = Preprocessor::new();
    preprocessor.current_file = source_path.to_string_lossy().into_owned();
    preprocessor.add_include_path(
        source_path
            .parent()
            .expect("fixture has a parent")
            .to_string_lossy()
            .into_owned(),
    );
    let tokens = preprocessor
        .process(TokenStream::new(lex(file, source)), source)
        .into_vec();
    assert!(
        preprocessor.errors().is_empty(),
        "unexpected preprocessing diagnostics: {:?}",
        preprocessor.errors()
    );
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        parser.diagnostics().is_empty(),
        "unexpected parser diagnostics: {:?}",
        parser.diagnostics()
    );
    unit
}

fn object_named<'arena>(
    unit: &'arena CHirUnit<'arena>,
    name: &str,
) -> &'arena evocc_chir::ObjectDecl<'arena> {
    unit.declarations()
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Object(object) if object.name.spelling == name => Some(object),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing object {name}"))
}

#[test]
fn same_file_macro_tag_uses_the_invocation_location() {
    let tree = SourceTree::new("same-file-tag");
    let source = "#define TAG s\nstruct s { int x; };\nstruct TAG v;\n";
    let main = tree.write("main.c", source);
    let arena = Arena::default();
    let unit = parse_preprocessed(&main, source, &arena);
    let specifier = object_named(&unit, "v").specifier.span();
    let start = source.rfind("struct TAG").expect("use-site struct") as u32;
    assert_eq!(
        specifier,
        SourceSpan::new(FileId::new(92_001), start, start + 10, 3, 1)
    );
}

#[test]
fn header_macro_tag_uses_the_main_file_invocation_location() {
    let tree = SourceTree::new("header-tag");
    tree.write("tag.h", "#define TAG s\n");
    let source = "#include \"tag.h\"\nstruct s { int x; };\nstruct TAG v;\n";
    let main = tree.write("main.c", source);
    let arena = Arena::default();
    let unit = parse_preprocessed(&main, source, &arena);
    let specifier = object_named(&unit, "v").specifier.span();
    let start = source.rfind("struct TAG").expect("use-site struct") as u32;
    assert_eq!(
        specifier,
        SourceSpan::new(FileId::new(92_001), start, start + 10, 3, 1)
    );
}

#[test]
fn macro_expanded_attribute_keeps_spelling_text_and_invocation_location() {
    let tree = SourceTree::new("attribute-argument");
    tree.write("alignment.h", "#define ALIGNMENT 8\n");
    let source = concat!(
        "#include \"alignment.h\"\n",
        "int value __attribute__((aligned(ALIGNMENT)));\n",
    );
    let main = tree.write("main.c", source);
    let arena = Arena::default();
    let unit = parse_preprocessed(&main, source, &arena);
    let object = object_named(&unit, "value");
    let attribute = object
        .attributes
        .iter()
        .find(|attribute| attribute.name == "aligned")
        .expect("aligned attribute is retained");
    let [argument] = attribute.arguments.as_slice() else {
        panic!("aligned has exactly one argument: {attribute:?}");
    };
    let start = source.find("ALIGNMENT").expect("invocation spelling") as u32;
    assert_eq!(argument.text, "8");
    assert_eq!(
        argument.span,
        SourceSpan::new(FileId::new(92_001), start, start + 9, 2, 34)
    );
}
