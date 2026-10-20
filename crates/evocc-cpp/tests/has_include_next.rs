//! `__has_include_next` and `#include_next` share one search order (EvoIR
//! 10.4, D18). The search starts after the directory where the current file
//! was found. In the main file both behave as `__has_include` and `#include`,
//! as in GCC 14 and Clang. Clang's resource `<limits.h>` reaches the SDK
//! `<limits.h>` only through this query.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_cpp::{FileId, Lexer, PreprocessError, Preprocessor, Token, TokenKind, TokenStream};

static NEXT_TEMP_TREE: AtomicU64 = AtomicU64::new(0);

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_TREE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "evocc-cpp-has-include-next-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create isolated source tree");
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

    fn directory(&self, relative: &str) -> String {
        self.root.join(relative).to_string_lossy().into_owned()
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove isolated source tree");
    }
}

fn lex(source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(FileId::new(7_301), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    tokens
}

/// The identifiers of the output, from the main file and from headers.
fn identifiers(tokens: &[Token], main_source: &str) -> Vec<String> {
    tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Identifier)
        .map(|token| {
            token.text.spelling().map_or_else(
                || {
                    main_source[token.span.byte_start as usize..token.span.byte_end as usize]
                        .to_owned()
                },
                str::to_owned,
            )
        })
        .collect()
}

/// Preprocess `main.c` of `tree` with the directories `one`, `two` and
/// `three` in that order and no built-in directories.
fn run(tree: &TempTree, source: &str) -> (Vec<String>, Vec<PreprocessError>) {
    let main = tree.write("main.c", source);
    let mut preprocessor = Preprocessor::new();
    preprocessor.current_file = main.to_string_lossy().into_owned();
    preprocessor.set_standard_includes_enabled(false);
    for directory in ["one", "two", "three"] {
        preprocessor.add_include_path(tree.directory(directory));
    }
    let output = preprocessor
        .process(TokenStream::new(lex(source)), source)
        .into_vec();
    (identifiers(&output, source), preprocessor.errors().to_vec())
}

/// The include tree of the search-order test. `tools/` has no copy: the
/// crossed GCC 14 and Apple clang run of the same tree is in
/// `.evidence/rust-r7-20261021/d18-search-order/`.
fn search_order_tree(tree: &TempTree) {
    tree.write(
        "beside.h",
        concat!(
            "beside_main\n",
            "#if __has_include_next(<only_one.h>)\n",
            "beside_next_starts_at_first_directory\n",
            "#endif\n",
        ),
    );
    tree.write("one/only_one.h", "only_one\n");
    tree.write("one/shared.h", "shared_one\n");
    tree.write(
        "one/wrap.h",
        concat!(
            "one_wrap\n",
            "#if __has_include_next(<only_one.h>)\n",
            "wrong_one_own_directory\n",
            "#else\n",
            "one_next_skips_own_directory\n",
            "#endif\n",
            "#if __has_include_next(\"only_one.h\")\n",
            "wrong_one_quoted_own_directory\n",
            "#else\n",
            "one_quoted_next_skips_own_directory\n",
            "#endif\n",
            "#define NEXT_HAS(header) __has_include_next(header)\n",
            "#if NEXT_HAS(<only_two.h>)\n",
            "one_next_sees_two\n",
            "#endif\n",
            "#include_next <shared.h>\n",
            "#if __has_include_next(<wrap.h>)\n",
            "#include_next <wrap.h>\n",
            "#endif\n",
        ),
    );
    tree.write("two/only_two.h", "only_two\n");
    tree.write("two/shared.h", "shared_two\n");
    tree.write(
        "two/wrap.h",
        concat!(
            "two_wrap\n",
            "#if __has_include_next(<only_two.h>)\n",
            "wrong_two_own_directory\n",
            "#else\n",
            "two_next_skips_own_directory\n",
            "#endif\n",
            "#if __has_include_next(<shared.h>)\n",
            "#include_next <shared.h>\n",
            "#endif\n",
            "#if __has_include_next(<wrap.h>)\n",
            "#include_next <wrap.h>\n",
            "#endif\n",
        ),
    );
    tree.write("three/shared.h", "shared_three\n");
    tree.write(
        "three/wrap.h",
        concat!(
            "wrap_three\n",
            "#if __has_include_next(<wrap.h>)\n",
            "wrong_three_last\n",
            "#else\n",
            "three_next_finds_nothing\n",
            "#endif\n",
        ),
    );
}

const SEARCH_ORDER_MAIN: &str = concat!(
    "#if __has_include_next(<only_one.h>)\n",
    "main_next_is_has_include\n",
    "#endif\n",
    "#if __has_include_next(\"beside.h\")\n",
    "main_quoted_next_searches_beside_main\n",
    "#endif\n",
    "#if __has_include_next(<missing.h>)\n",
    "wrong_main_missing\n",
    "#else\n",
    "main_next_misses\n",
    "#endif\n",
    "#include_next \"beside.h\"\n",
    "#include_next <only_one.h>\n",
    "#include <wrap.h>\n",
);

/// The order that GCC 14 and Apple clang give the same tree.
const SEARCH_ORDER_OUTPUT: [&str; 17] = [
    "main_next_is_has_include",
    "main_quoted_next_searches_beside_main",
    "main_next_misses",
    "beside_main",
    "beside_next_starts_at_first_directory",
    "only_one",
    "one_wrap",
    "one_next_skips_own_directory",
    "one_quoted_next_skips_own_directory",
    "one_next_sees_two",
    "shared_two",
    "two_wrap",
    "two_next_skips_own_directory",
    "shared_three",
    "wrap_three",
    "three_next_finds_nothing",
    "",
];

#[test]
fn has_include_next_is_a_defined_feature_query() {
    let tree = TempTree::new("defined");
    let (output, errors) = run(
        &tree,
        concat!(
            "#ifdef __has_include_next\n",
            "ifdef_sees_it\n",
            "#endif\n",
            "#if defined(__has_include_next) && defined __has_include_next\n",
            "defined_sees_it\n",
            "#endif\n",
        ),
    );
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(output, ["ifdef_sees_it", "defined_sees_it"]);
}

#[test]
fn has_include_next_and_include_next_share_one_search_order() {
    let tree = TempTree::new("order");
    search_order_tree(&tree);
    let (output, errors) = run(&tree, SEARCH_ORDER_MAIN);
    assert!(errors.is_empty(), "{errors:?}");
    let expected = SEARCH_ORDER_OUTPUT
        .iter()
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(output.iter().collect::<Vec<_>>(), expected);
}

#[test]
fn a_malformed_has_include_next_is_e0106() {
    let tree = TempTree::new("malformed");
    for source in [
        "#if __has_include_next(x)\n#endif\n",
        "#if __has_include_next()\n#endif\n",
        "#if __has_include_next(\n#endif\n",
    ] {
        let (_, errors) = run(&tree, source);
        assert_eq!(
            errors.first().map(|error| error.code),
            Some("E0106"),
            "{source:?}: {errors:?}"
        );
    }
}
