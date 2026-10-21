//! Framework header search and `#import` (EvoIR 10.5, D19). A framework
//! directory serves `<Name/Header.h>` from `Name.framework/Headers/` and then
//! `Name.framework/PrivateHeaders/`. The first framework directory of the
//! search list that holds `Name.framework` owns the name. A header inside a
//! framework also finds the subframeworks in `Name.framework/Frameworks/`.
//! `#import` includes a file only once. The expected orders are the ones that
//! Apple clang 21 gives the same trees; `crates/evocc/tests/darwin_frameworks.rs`
//! runs the crossed comparison.

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
            "evocc-cpp-frameworks-{label}-{}-{sequence}",
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
    let mut lexer = Lexer::new(FileId::new(7_302), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    tokens
}

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

/// One search directory: `-I DIR` or `-F DIR`.
#[derive(Clone, Copy)]
enum Directory {
    Normal(&'static str),
    Framework(&'static str),
}

/// Preprocess `main.c` of `tree` with `directories` in order and no built-in
/// directories.
fn run(
    tree: &TempTree,
    source: &str,
    directories: &[Directory],
) -> (Vec<String>, Vec<PreprocessError>) {
    let main = tree.write("main.c", source);
    let mut preprocessor = Preprocessor::new();
    preprocessor.current_file = main.to_string_lossy().into_owned();
    preprocessor.set_standard_includes_enabled(false);
    for directory in directories {
        match directory {
            Directory::Normal(path) => preprocessor.add_include_path(tree.directory(path)),
            Directory::Framework(path) => preprocessor.add_framework_path(tree.directory(path)),
        }
    }
    let output = preprocessor
        .process(TokenStream::new(lex(source)), source)
        .into_vec();
    (identifiers(&output, source), preprocessor.errors().to_vec())
}

/// The framework tree of the search tests. `Umbrella` has the subframework
/// `Sub`, and `Beta` exists in both framework directories.
fn framework_tree(tree: &TempTree) {
    tree.write(
        "F1/Alpha.framework/Headers/Alpha.h",
        concat!(
            "alpha_main\n",
            "#include <Alpha/Detail.h>\n",
            "#include \"Local.h\"\n",
            "#include <Alpha/Secret.h>\n",
        ),
    );
    tree.write("F1/Alpha.framework/Headers/Detail.h", "alpha_detail\n");
    tree.write("F1/Alpha.framework/Headers/Local.h", "alpha_local\n");
    tree.write(
        "F1/Alpha.framework/PrivateHeaders/Secret.h",
        "alpha_secret\n",
    );
    tree.write("F1/Beta.framework/Headers/OnlyOne.h", "beta_one\n");
    tree.write("F2/Beta.framework/Headers/OnlyTwo.h", "beta_two\n");
    tree.write("F2/Gamma.framework/Headers/Gamma.h", "gamma_two\n");
    tree.write("I1/Alpha/Alpha.h", "i1_alpha\n");
    tree.write(
        "F1/Umbrella.framework/Headers/Umbrella.h",
        "umbrella_main\n#include <Sub/Sub.h>\n#include <Sub/Next.h>\n",
    );
    tree.write(
        "F1/Umbrella.framework/Frameworks/Sub.framework/Headers/Sub.h",
        concat!(
            "sub_main\n",
            "#include <Sub/Other.h>\n",
            "#include <Sub/Hidden.h>\n",
            "#include <Alpha/Detail.h>\n",
        ),
    );
    tree.write(
        "F1/Umbrella.framework/Frameworks/Sub.framework/Headers/Other.h",
        "sub_other\n",
    );
    tree.write(
        "F1/Umbrella.framework/Frameworks/Sub.framework/PrivateHeaders/Hidden.h",
        "sub_hidden\n",
    );
    tree.write(
        "F1/Umbrella.framework/Frameworks/Sub.framework/Headers/Next.h",
        "sub_next\n#include_next <Next.h>\n",
    );
    tree.write("I2/Next.h", "i2_next\n");
}

const FRAMEWORK_MAIN: &str = concat!(
    "#include <Alpha/Alpha.h>\n",
    "#include <Gamma/Gamma.h>\n",
    "#include <Umbrella/Umbrella.h>\n",
    "#if __has_include(<Beta/OnlyOne.h>)\n",
    "has_beta_one\n",
    "#endif\n",
    "#if __has_include(<Beta/OnlyTwo.h>)\n",
    "has_beta_two\n",
    "#else\n",
    "no_beta_two\n",
    "#endif\n",
    "#if __has_include(<Sub/Sub.h>)\n",
    "main_sees_sub\n",
    "#else\n",
    "main_no_sub\n",
    "#endif\n",
    "#if __has_include(<Alpha/Missing.h>)\n",
    "wrong\n",
    "#endif\n",
    "#include \"Alpha/Detail.h\"\n",
);

/// The words after the framework headers of `Alpha`, in Apple clang's order.
const AFTER_ALPHA: [&str; 9] = [
    "gamma_two",
    "umbrella_main",
    "sub_main",
    "sub_other",
    "sub_hidden",
    "alpha_detail",
    "sub_next",
    "i2_next",
    "main_no_sub",
];

fn expected(alpha: &[&str], beta: &str) -> Vec<String> {
    let mut words: Vec<String> = alpha.iter().map(|word| (*word).to_owned()).collect();
    words.extend(AFTER_ALPHA[..8].iter().map(|word| (*word).to_owned()));
    words.push(beta.to_owned());
    if beta == "has_beta_one" {
        words.push("no_beta_two".to_owned());
    }
    words.push(AFTER_ALPHA[8].to_owned());
    words.push("alpha_detail".to_owned());
    words
}

const ALPHA_FRAMEWORK: [&str; 4] = ["alpha_main", "alpha_detail", "alpha_local", "alpha_secret"];

#[test]
fn a_framework_directory_serves_headers_private_headers_and_subframeworks() {
    use Directory::{Framework, Normal};
    let tree = TempTree::new("search");
    framework_tree(&tree);
    let (output, errors) = run(
        &tree,
        FRAMEWORK_MAIN,
        &[Framework("F1"), Framework("F2"), Normal("I2")],
    );
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(output, expected(&ALPHA_FRAMEWORK, "has_beta_one"));
}

#[test]
fn normal_and_framework_directories_keep_command_line_order() {
    use Directory::{Framework, Normal};
    let tree = TempTree::new("order");
    framework_tree(&tree);
    let (output, errors) = run(
        &tree,
        FRAMEWORK_MAIN,
        &[Normal("I1"), Framework("F1"), Framework("F2"), Normal("I2")],
    );
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(output, expected(&["i1_alpha"], "has_beta_one"));
    let (output, errors) = run(
        &tree,
        FRAMEWORK_MAIN,
        &[Framework("F1"), Normal("I1"), Framework("F2"), Normal("I2")],
    );
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(output, expected(&ALPHA_FRAMEWORK, "has_beta_one"));
}

#[test]
fn the_first_framework_directory_that_holds_a_framework_owns_its_name() {
    use Directory::{Framework, Normal};
    let tree = TempTree::new("owner");
    framework_tree(&tree);
    // F2 holds Beta.framework first, so F1's Beta headers are not found.
    let (output, errors) = run(
        &tree,
        FRAMEWORK_MAIN,
        &[Framework("F2"), Framework("F1"), Normal("I2")],
    );
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(output, expected(&ALPHA_FRAMEWORK, "has_beta_two"));
    let (_, errors) = run(
        &tree,
        "#include <Beta/OnlyOne.h>\n",
        &[Framework("F2"), Framework("F1")],
    );
    assert_eq!(
        errors.iter().map(|error| error.code).collect::<Vec<_>>(),
        ["E0105"],
        "{errors:?}"
    );
}

#[test]
fn an_include_next_in_a_subframework_header_starts_at_the_first_directory() {
    use Directory::{Framework, Normal};
    let tree = TempTree::new("next");
    framework_tree(&tree);
    // `Umbrella` is in the last directory. Clang warns that the include_next
    // of a subframework header searches from the start, and finds `I2`.
    let (output, errors) = run(
        &tree,
        "#include <Umbrella/Umbrella.h>\n",
        &[Framework("F2"), Normal("I2"), Framework("F1")],
    );
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        output,
        [
            "umbrella_main",
            "sub_main",
            "sub_other",
            "sub_hidden",
            "alpha_detail",
            "sub_next",
            "i2_next",
        ]
    );
}

#[test]
fn a_normal_directory_does_not_search_frameworks_and_subframeworks_need_a_framework_includer() {
    use Directory::{Framework, Normal};
    let tree = TempTree::new("normal");
    framework_tree(&tree);
    let (_, errors) = run(&tree, "#include <Gamma/Gamma.h>\n", &[Normal("F2")]);
    assert_eq!(
        errors.iter().map(|error| error.code).collect::<Vec<_>>(),
        ["E0105"],
        "{errors:?}"
    );
    let (_, errors) = run(&tree, "#include <Sub/Sub.h>\n", &[Framework("F1")]);
    assert_eq!(
        errors.iter().map(|error| error.code).collect::<Vec<_>>(),
        ["E0105"],
        "{errors:?}"
    );
}

/// The import tree. GCC 14 and Apple clang print the same words for it.
fn import_tree(tree: &TempTree) {
    tree.write("once.h", "once_body\n");
    tree.write("twice.h", "twice_body\n");
    tree.write("plain.h", "plain_body\n");
    tree.write("nested.h", "nested_body\n#import \"plain.h\"\n");
    tree.write(
        "guarded.h",
        "#ifndef GUARDED\n#define GUARDED\nguarded_body\n#endif\n",
    );
}

const IMPORT_MAIN: &str = concat!(
    "#import \"once.h\"\n",
    "#import \"once.h\"\n",
    "#include \"once.h\"\n",
    "#include \"twice.h\"\n",
    "#import \"twice.h\"\n",
    "#include \"twice.h\"\n",
    "#include \"plain.h\"\n",
    "#include \"nested.h\"\n",
    "#include \"plain.h\"\n",
    "#import <guarded.h>\n",
    "#if __has_include(\"once.h\")\n",
    "has_once\n",
    "#endif\n",
    "#if 0\n",
    "#import <missing.h>\n",
    "#endif\n",
    "end_main\n",
);

#[test]
fn import_includes_a_file_once_and_makes_it_include_once() {
    use Directory::Normal;
    let tree = TempTree::new("import");
    import_tree(&tree);
    let (output, errors) = run(&tree, IMPORT_MAIN, &[Normal(".")]);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        output,
        [
            "once_body",
            "twice_body",
            "plain_body",
            "nested_body",
            "guarded_body",
            "has_once",
            "end_main",
        ]
    );
    let (_, errors) = run(&tree, "#import <missing.h>\n", &[Normal(".")]);
    assert_eq!(
        errors.iter().map(|error| error.code).collect::<Vec<_>>(),
        ["E0105"],
        "{errors:?}"
    );
}
