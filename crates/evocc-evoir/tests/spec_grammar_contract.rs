//! Errata checks for already-approved forms, not a parser-completeness claim.

const SPEC: &str = include_str!("../../../spec/evoir.md");

fn grammar() -> String {
    SPEC.split_once("### 9.3 Grammar\n")
        .unwrap()
        .1
        .split_once("#### 9.3.1 Canonical Shapes")
        .unwrap()
        .0
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn main_grammar_contains_the_existing_approved_forms() {
    let body = grammar();
    for fragment in [
        "const.f128 0x\" hex32",
        "alloca.dynamic ",
        "va_arg.byval ",
        "atomic.rmw.",
        "frame_address depth=",
        "\"floating_to_integer\"",
        "\"builtin_trap\"",
        "\"compound_literal\"",
        "\" readonly=true\"",
        "signed_integer",
        "\", exposed\"",
        "\", diff=\"",
        "call_arg := value_name [byval_mark]",
    ] {
        assert!(
            body.contains(fragment),
            "missing approved grammar: {fragment}"
        );
    }
}

#[test]
fn pointer_offset_has_its_operands_not_the_integer_to_pointer_production() {
    let body = grammar();
    assert!(body.contains(concat!(
        "value_name \" = ptr.offset.\" direction \".\" index_signedness \" \" ",
        "value_name \", \" value_name \" [scale=\" integer \"]\"",
    )));
    assert!(body.contains(concat!(
        "value_name \" = ptr.from_int.\" type \" \" value_name ",
        "| \"(\" result_list",
    )));
}
