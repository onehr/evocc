use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_cpp::{
    pragma_pack_action, pragma_visibility_action, FileId, Lexer, PragmaVisibilityAction,
    PreprocessError, Preprocessor, Token, TokenKind, TokenStream, TokenText,
};
use evocc_proof::sha256_hex;

const PROFILE_SHA256: &str = "74801974f845be4d198bbedc2f20130ddd98645550c05e3453d0294d47bc5a95";
const ATTRIBUTE_TABLE_SHA256: &str =
    "6429082232ca76ee2c7ff5593decea43b4cf6f97a87cccd81192e27173abc8dd";
/// `crates/evocc-parser/tests/builtin_inventory.rs` checks that this table
/// lists exactly the builtins that the front end implements.
const BUILTIN_TABLE_SHA256: &str =
    "ee3ab2488a4e2a6666424ec7b16d06038e8e9c8ae5a861706214f1393c6fdd78";

static NEXT_TEMP_TREE: AtomicU64 = AtomicU64::new(0);

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_TREE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "evocc-cpp-target-profile-{label}-{}-{sequence}",
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
}

impl Drop for TempTree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove isolated source tree");
    }
}

struct Run {
    output: String,
    errors: Vec<PreprocessError>,
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

fn spelling(token: &Token, source: &str) -> String {
    match &token.text {
        TokenText::Decoded(text) | TokenText::RawBytes { escaped: text, .. } => text.clone(),
        TokenText::Borrowed => source
            .get(token.span.byte_start as usize..token.span.byte_end as usize)
            .expect("token span belongs to fixture")
            .to_owned(),
    }
}

fn can_concatenate(group: &[(TokenKind, String)], kind: TokenKind, text: &str) -> bool {
    let candidate = group
        .iter()
        .map(|(_, spelling)| spelling.as_str())
        .chain(core::iter::once(text))
        .collect::<String>();
    let expected = group
        .iter()
        .map(|(expected_kind, _)| *expected_kind)
        .chain(core::iter::once(kind))
        .collect::<Vec<_>>();
    let actual = lex(FileId::DUMMY, &candidate)
        .into_iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| token.kind)
        .collect::<Vec<_>>();
    actual == expected
}

fn render(tokens: &[Token], source: &str) -> String {
    let mut rendered = String::new();
    let mut unseparated = Vec::<(TokenKind, String)>::new();
    for token in tokens
        .iter()
        .filter(|token| token.kind != TokenKind::Eof && pragma_pack_action(token).is_none())
    {
        let text = spelling(token, source);
        if text.is_empty() {
            continue;
        }
        if !unseparated.is_empty() && !can_concatenate(&unseparated, token.kind, &text) {
            rendered.push(' ');
            unseparated.clear();
        }
        rendered.push_str(&text);
        unseparated.push((token.kind, text));
    }
    rendered
}

fn run_with(source: &str, configure: impl FnOnce(&mut Preprocessor)) -> Run {
    let mut preprocessor = Preprocessor::new();
    configure(&mut preprocessor);
    let output = preprocessor
        .process(TokenStream::new(lex(FileId::new(7_001), source)), source)
        .into_vec();
    Run {
        output: render(&output, source),
        errors: preprocessor.errors().to_vec(),
    }
}

fn preprocess(source: &str) -> String {
    let run = run_with(source, |_| {});
    assert!(run.errors.is_empty(), "unexpected errors: {:?}", run.errors);
    run.output
}

#[test]
fn the_profile_no_longer_claims_variable_length_arrays_are_absent() {
    assert_eq!(preprocess("__STDC_NO_VLA__\n"), "__STDC_NO_VLA__");
}

fn first_error_code(source: &str) -> &'static str {
    run_with(source, |_| {})
        .errors
        .first()
        .map_or("NONE", |error| error.code)
}

fn expected_tokens(source: &str) -> Vec<(TokenKind, String)> {
    lex(FileId::DUMMY, source)
        .into_iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| {
            let text = spelling(&token, source);
            (token.kind, text)
        })
        .collect()
}

fn split_definition(line: &str) -> (&str, bool, &str) {
    let definition = line
        .strip_prefix("#define ")
        .expect("sealed line is a definition");
    let name_end = definition
        .find(|character: char| character == '(' || character.is_ascii_whitespace())
        .unwrap_or(definition.len());
    let name = &definition[..name_end];
    let remainder = &definition[name_end..];
    if remainder.starts_with('(') {
        let close = remainder.find(')').expect("function macro parameter list");
        (name, true, remainder[close + 1..].trim_start())
    } else {
        (name, false, remainder.trim_start())
    }
}

#[test]
fn predefined_macros_are_the_sealed_gcc_14_list() {
    let sealed_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/predefined/aarch64-unknown-linux-gnu.h"
    );
    let sealed = fs::read_to_string(sealed_path).expect("read target profile copy");
    assert_eq!(sha256_hex(sealed.as_bytes()), PROFILE_SHA256);
    assert_eq!(sealed.lines().count(), 421);

    let preprocessor = Preprocessor::new();
    for line in sealed.lines() {
        let (name, is_function, body) = split_definition(line);
        let defined = preprocessor
            .macros()
            .get(name)
            .unwrap_or_else(|| panic!("missing sealed macro {name}"));
        assert_eq!(defined.is_function, is_function, "{name}");
        let actual = defined
            .body
            .iter()
            .map(|token| (token.kind, spelling(token, "")))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected_tokens(body), "{name}");
    }

    assert_eq!(
        preprocess(concat!(
            "gnuc __GNUC__ __GNUC_MINOR__ __aarch64__ __linux__ ",
            "__CHAR_UNSIGNED__ __SIZEOF_LONG__ __LP64__ __STDC_VERSION__\n",
        )),
        "gnuc 14 2 1 1 1 8 1 201112L"
    );
    assert_eq!(
        preprocess("#ifdef __OPTIMIZE__\nopt\n#else\nnoopt\n#endif\n"),
        "noopt"
    );
}

#[test]
fn counter_increments_per_expansion() {
    assert_eq!(
        preprocess("a __COUNTER__ __COUNTER__ __COUNTER__\n"),
        "a 0 1 2"
    );
    assert_eq!(
        preprocess("#define U(x) x##__COUNTER__\nU(id_) U(id_)\n"),
        "id___COUNTER__ id___COUNTER__"
    );
    assert_eq!(
        preprocess(concat!(
            "#define P(a,b) a##b\n",
            "#define U2(x,y) P(x,y)\n",
            "#define U(x) U2(x,__COUNTER__)\n",
            "U(id_) U(id_)\n",
        )),
        "id_0 id_1"
    );
}

#[test]
fn gnu_comma_paste_elides_only_an_omitted_variadic_argument() {
    assert_eq!(
        preprocess(concat!(
            "#define APPLY(f, ...) f(1, ## __VA_ARGS__)\n",
            "APPLY(a)\n",
            "APPLY(b, 2, 3)\n",
            "APPLY(c,)\n",
        )),
        "a(1)b(1,2,3)c(1,)"
    );
}

#[test]
fn gnu_comma_variadic_forwarding_preserves_raw_tokens_for_nested_paste() {
    let definitions = concat!(
        "#define FOO 12\n#define FOO_BAR 29\n",
        "#define KEEP_RAW(_,x) x##_BAR\n",
        "#define KEEP_VALUE(_,x) x\n",
        "#define FORWARD(A...) KEEP_RAW(_, ##A)\n",
        "#define FORWARD_STD(...) KEEP_RAW(_, ##__VA_ARGS__)\n",
        "#define VALUE(A...) KEEP_VALUE(_, ##A)\n",
    );
    for (call, expected) in [
        ("FORWARD(FOO)", "29"),
        ("FORWARD_STD(FOO)", "29"),
        ("VALUE(FOO)", "12"),
    ] {
        assert_eq!(
            preprocess(&format!("{definitions}{call}\n")),
            expected,
            "{call}"
        );
    }
}

#[test]
fn variadic_dispatch_does_not_expand_names_before_the_selected_macro_pastes_them() {
    assert_eq!(
        preprocess(concat!(
            "#define FOO 12\n#define NV_FOO_BAR 31\n",
            "#define NVVAL_X(drf,v) drf\n",
            "#define NVDEF_N(X,d,r,f,v) NVVAL_X(d##_##r##_##f, d##_##r##_##f##_##v)\n",
            "#define NVDEF_I(X,d,r,f,i,v) NVVAL_X(d##_##r##_##f(i), d##_##r##_##f##_##v)\n",
            "#define NVDEF_(X,_1,_2,_3,_4,_5,IMPL,...) IMPL\n",
            "#define NVDEF(A...) NVDEF_(X, ##A, NVDEF_I, NVDEF_N)(X, ##A)\n",
            "NVDEF(NV,FOO,BAR,BAZ)\n",
        )),
        "31"
    );
}

#[test]
fn feature_query_tables_are_digest_checked_data() {
    let attributes = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/predefined/has-attribute.tsv"
    ))
    .expect("read attribute answer table");
    assert_eq!(sha256_hex(attributes.as_bytes()), ATTRIBUTE_TABLE_SHA256);
    assert_eq!(attributes.lines().count(), 46);

    let builtins = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/predefined/has-builtin.txt"
    ))
    .expect("read builtin answer table");
    assert_eq!(sha256_hex(builtins.as_bytes()), BUILTIN_TABLE_SHA256);
    assert_eq!(builtins.lines().count(), 307);
}

#[test]
fn feature_queries_are_defined_and_answer_as_measured() {
    for query in [
        "__has_attribute",
        "__has_builtin",
        "__has_feature",
        "__has_include",
    ] {
        assert_eq!(
            preprocess(&format!("#ifdef {query}\nd\n#else\nu\n#endif\n")),
            "d",
            "{query}"
        );
    }

    let table = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/predefined/has-attribute.tsv"
    ))
    .expect("read attribute answer table");
    for line in table.lines() {
        let (name, answer) = line.split_once('\t').expect("name and answer");
        assert_eq!(
            preprocess(&format!("__has_attribute({name})\n")),
            answer,
            "{name}"
        );
        assert_eq!(
            preprocess(&format!("__has_attribute(__{name}__)\n")),
            answer,
            "__{name}__"
        );
    }
    assert_eq!(
        preprocess(concat!(
            "__has_attribute(fallthrough) __has_attribute(noreturn) ",
            "__has_attribute(not_a_real_one)\n",
        )),
        "202311 202311 0"
    );
    assert_eq!(
        preprocess(concat!(
            "__has_builtin(__builtin_expect) ",
            "__has_builtin(__builtin_mul_overflow) ",
            "__has_builtin(__builtin_isdigit) ",
            "__has_builtin(__builtin_alloca_uninitialized)\n",
        )),
        "1 1 1 0"
    );
    for feature in [
        "address_sanitizer",
        "coverage_sanitizer",
        "hwaddress_sanitizer",
        "memory_sanitizer",
        "shadow_call_stack",
        "thread_sanitizer",
        "undefined_behavior_sanitizer",
    ] {
        assert_eq!(
            preprocess(&format!("__has_feature({feature})\n")),
            "0",
            "{feature}"
        );
    }
    assert_eq!(
        preprocess(concat!(
            "#if __has_attribute(unused) && !__has_feature(address_sanitizer)\n",
            "y\n",
            "#endif\n",
        )),
        "y"
    );
}

#[test]
fn has_include_performs_the_real_search() {
    let tree = TempTree::new("has-include");
    let main = tree.write("input.c", "");
    let _ = tree.write("include/hit.h", "");
    let source = concat!(
        "#if __has_include(<hit.h>)\n",
        "y\n",
        "#else\n",
        "n\n",
        "#endif\n",
        "#if __has_include(\"miss.h\")\n",
        "y\n",
        "#else\n",
        "n\n",
        "#endif\n",
    );
    let run = run_with(source, |preprocessor| {
        preprocessor.current_file = main.to_string_lossy().into_owned();
        preprocessor.add_include_path(tree.root.join("include").to_string_lossy().into_owned());
    });
    assert!(run.errors.is_empty(), "{:?}", run.errors);
    assert_eq!(run.output, "y n");
}

#[test]
fn malformed_feature_queries_are_e0106() {
    for source in [
        "#if __has_attribute(\n#endif\n",
        "#if __has_attribute()\n#endif\n",
        "#if __has_attribute(a b)\n#endif\n",
        "#if __has_include(x)\n#endif\n",
    ] {
        assert_eq!(first_error_code(source), "E0106", "{source:?}");
    }
}

#[test]
fn pragma_operator_is_processed_after_macro_expansion() {
    assert_eq!(
        preprocess("_Pragma(\"GCC diagnostic push\") after\n"),
        "after"
    );
    assert_eq!(
        preprocess(concat!(
            "#define P _Pragma(\"GCC diagnostic ignored \\\"-Wfoo\\\"\")\n",
            "P x\n",
        )),
        "x"
    );
}

#[test]
fn pragma_lines_are_classified() {
    let source = "#pragma GCC visibility push(default)\nx\n";
    let mut pp = Preprocessor::new();
    let tokens = pp
        .process(TokenStream::new(lex(FileId::DUMMY, source)), source)
        .into_vec();
    assert_eq!(
        tokens
            .iter()
            .filter_map(pragma_visibility_action)
            .collect::<Vec<_>>(),
        vec![PragmaVisibilityAction::Push {
            visibility: "default"
        }]
    );
    assert_eq!(pp.ignored_pragma_count(), 0);
    assert_eq!(preprocess("#pragma GCC poison bad\nok\n"), "ok");
    assert_eq!(first_error_code("#pragma GCC poison bad\nbad\n"), "E0108");
    assert_eq!(first_error_code("#pragma pack(1)\n"), "NONE");
    assert_eq!(first_error_code("#pragma pack(show)\n"), "E0107");
    assert_eq!(preprocess("#pragma something_else here\nx\n"), "x");
}

#[test]
fn the_kernel_compiler_check_takes_the_gcc_branch() {
    let source = concat!(
        "#ifdef __clang__\n",
        "clang\n",
        "#elif defined(__GNUC__)\n",
        "gcc __GNUC__\n",
        "#else\n",
        "#error \"Unknown compiler\"\n",
        "#endif\n",
    );
    assert_eq!(preprocess(source), "gcc 14");
}
