use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn diagnostic(source: &str) -> Option<(String, String)> {
    diagnostic_with_expansion_spans(source, false)
}

fn diagnostic_with_expansion_spans(source: &str, shared: bool) -> Option<(String, String)> {
    let file = FileId::new(1);
    let arena = Arena::default();
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexical error in {source}");
        tokens.push(token);
    }
    if shared {
        // Replacement-list tokens can all have the same invocation span.
        let invocation = tokens[0].span;
        for token in &mut tokens {
            token.span = invocation;
        }
    }
    let mut parser = Parser::new(tokens, file, source);
    parser.parse_translation_unit_in(&arena);
    parser
        .diagnostic()
        .map(|d| (d.code.to_owned(), d.message.clone()))
}

#[test]
fn a_sign_comparison_cannot_erase_a_neighboring_macro_refusal() {
    let source =
        "struct S{unsigned long x:40;};int f(void){struct S s={0};return ((s.x+1)<0)+~s.x;}";
    assert_eq!(
        diagnostic_with_expansion_spans(source, true).map(|d| d.0),
        Some("E2939".into())
    );
}

#[test]
fn consensus_wide_field_consumers_are_admitted() {
    let source = include_str!("../../../tests/fixtures/wide-field-consensus.c");
    assert_eq!(diagnostic(source), None);
}

#[test]
fn narrowing_an_unsigned_mask_observes_only_the_consensus_low_bits() {
    assert_eq!(
        diagnostic(include_str!(
            "../../../tests/fixtures/narrowed-wide-field-mask.c"
        )),
        None
    );
    for expression in [
        "(unsigned)((s.x + 1) & 0xffffffff)",
        "((unsigned)(s.x & 0xffffffff)) + ~s.x",
    ] {
        let source = format!("struct S{{unsigned long x:40;}}; unsigned f(void){{struct S s={{0xffffffffffUL}};return {expression};}}");
        assert_eq!(
            diagnostic_with_expansion_spans(&source, true).map(|d| d.0),
            Some("E2939".into()),
            "{source}"
        );
    }
}

#[test]
fn nested_conditionals_preserve_precision_without_rewalking_their_wide_arm() {
    let mut expression = String::from("s.x");
    for _ in 0..20 {
        expression = format!("(c ? {expression} : 0)");
    }
    let source = format!("struct S{{unsigned long x:40;}};int f(int c){{struct S s={{3}};return {expression} != 0;}}");
    assert_eq!(diagnostic(&source), None);
}

#[test]
fn unsigned_sign_comparisons_do_not_observe_the_disputed_precision() {
    for expression in ["(s.x-2)<0", "(s.x+1)>=0", "0>(s.x*2)", "0<=(s.x^1)"] {
        let source = format!("struct S {{unsigned long long x:35;}}; int f(void){{struct S s={{0}}; return {expression};}}");
        assert_eq!(diagnostic(&source), None, "{source}");
    }
}

#[test]
fn nested_precision_conflicts_remain_explicit() {
    for expression in [
        "(0, s.x) << 1UL",
        "+s.x + 1",
        "(s.x >> 0) + 1",
        "(c ? s.x : 0) + 1",
        "(s.x ?: 0) + 1",
        "(s.x += 0) + 1",
        "s.x++ + 1",
        "(0, s.x) + 1",
        "(s.x+1)==0",
        "(s.x+1)>0",
        "((s.x+1)<0)+~s.x",
        "c ? s.x : -1",
        "s.x /= -1",
        "s.x %= -1",
        "s.x == -1",
        "s.x < -1L",
        "s.x < c",
        "s.x / -1L",
        "s.x % -1L",
        "(s.x + -0x10000000000L) < 0",
        "(0 ? s.x : -1L) < 0",
    ] {
        let source = format!("struct S {{unsigned long x:40;}}; unsigned long f(int c){{struct S s={{0xffffffffffUL}}; return {expression};}}");
        assert_eq!(
            diagnostic(&source).map(|d| d.0),
            Some("E2939".into()),
            "{source}"
        );
    }
}

#[test]
fn wide_field_arithmetic_names_the_unsupported_promotion() {
    for sign in ["signed", "unsigned"] {
        for expression in [
            "s.x + 1",
            "1 - s.x",
            "s.x * 2",
            "s.x / 2",
            "s.x % 2",
            "s.x & 1",
            "s.x | 1",
            "s.x ^ 1",
            "(+s.x) << 1",
            "-s.x",
            "~s.x",
            "sizeof(s.x + 0)",
            "p->x + 0",
        ] {
            let source = format!("struct S {{{sign} long x:40;}}; long f(int c){{struct S s={{3}},*p=&s; return {expression};}}");
            assert_eq!(
                diagnostic(&source),
                Some((
                    "E2939".into(),
                    "unsupported-wide-bit-field-promotion".into()
                )),
                "{source}"
            );
        }
    }
}

#[test]
fn unsupported_updates_and_default_argument_promotions_remain_explicit() {
    for statement in [
        "s.x /= -1;",
        "s.x %= -1;",
        "s.x = (+s.x) << 1UL;",
        "take(0,s.x);",
        "old(s.x);",
    ] {
        let source = format!("struct S {{unsigned long x:40;}}; void take(int,...); void old(); void f(void){{struct S s={{3}}; {statement}}}");
        assert_eq!(
            diagnostic(&source).map(|d| d.0),
            Some("E2939".into()),
            "{source}"
        );
    }
    for statement in ["s.x++;", "++s.x;", "s.x--;", "--s.x;", "s.x += 1;"] {
        let source = format!(
            "struct S {{signed long x:40;}}; void f(void){{struct S s={{3}}; {statement}}}"
        );
        assert_eq!(
            diagnostic(&source).map(|d| d.0),
            Some("E2939".into()),
            "{source}"
        );
    }
}

#[test]
fn wide_storage_and_explicit_conversion_do_not_request_integer_promotion() {
    let source = "struct S {unsigned long x:40;}; unsigned long f(void){struct S s={3}; s.x=7; unsigned long value=s.x; return value + (unsigned long)s.x;}";
    assert_eq!(diagnostic(source), None);
    assert_eq!(diagnostic("struct S {signed long a:32; unsigned long b:32; unsigned long c:31;}; long f(void){struct S s={-1,2,3};s.c++;return s.a+s.b+s.c;}"), None);
    assert_eq!(diagnostic("struct S {unsigned long x:40;}; int f(void){struct S s={3};return !s.x || (s.x && 1);}"), None);
    assert_eq!(diagnostic("struct S {unsigned long x:64; signed long y:64;}; long f(void){struct S s={3,4};s.x++; s.y++;return (s.x+1)+(s.y-1);}"), None);
}

#[test]
fn scalar_storage_order_is_refused_instead_of_silently_ignored() {
    for source in [
        "struct __attribute__((scalar_storage_order(\"big-endian\"))) S {int x;};",
        "struct S {int x;} __attribute__((__scalar_storage_order__(\"little-endian\")));",
        "typedef struct {int a[2];} S __attribute__((scalar_storage_order(\"big-endian\")));",
        "struct Outer {struct __attribute__((scalar_storage_order(\"big-endian\"))) {int x;} member;};",
    ] {
        assert_eq!(diagnostic(source), Some(("E2940".into(), "unsupported-scalar-storage-order".into())), "{source}");
    }
}
