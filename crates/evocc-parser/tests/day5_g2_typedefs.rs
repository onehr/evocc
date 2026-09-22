use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

use evocc_chir::{
    CHirBlockItem, CHirDecl, CHirExpr, CHirStmt, FunctionDecl, ObjectDecl, ScopeKind, SymbolId,
    SymbolKind, TagMembers, TypeSpecifier, TypedefDecl, WrittenDeclaratorLayer,
};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::{ArrayBound, FunctionParameters, Qualifiers, TypeId, TypeKind};

const FILE_RAW: u32 = 73;

struct SourceCase {
    id: &'static str,
    source: &'static str,
}

const FILE_ALIAS: &str = "typedef long Word; Word value;\n";
const NESTED_ALIAS: &str =
    "int f(void) { typedef unsigned char Byte; Byte value; return sizeof(Byte) + value; }\n";
const POINT_OF_DECLARATION: &str =
    "typedef int T; int f(void) { { T T = 1; T = T + 1; } T after = 2; return after; }\n";
const POINTER_DECLARATOR: &str = "typedef int Elem; Elem *pointer;\n";
const ARRAY_DECLARATOR: &str = "typedef short Elem; Elem values[3];\n";
const FUNCTION_DECLARATOR: &str = "typedef long Result; Result convert(Result input);\n";
const FUNCTION_POINTER_DECLARATOR: &str = "typedef int Scalar; Scalar (*callback)(Scalar);\n";
const CAST_SIZEOF_ALIGNOF: &str =
    "typedef unsigned long Word; int f(Word value) { return (Word)value + sizeof(Word) + _Alignof(Word); }\n";
const ABSTRACT_TYPE_NAMES: &str =
    "typedef int Word; int f(void) { return sizeof(Word *) + sizeof(Word[3]); }\n";
const RECORD_MEMBER_ALIAS: &str = "typedef int Field; struct S { Field member; };\n";
const TYPEDEF_AS_EXPRESSION: &str = "typedef int T; int bad(void) { return T; } int after;\n";
const TYPEDEF_THEN_OBJECT: &str = "typedef int T; int T;\n";
const OBJECT_THEN_TYPEDEF: &str = "int X; typedef int X;\n";
const MALFORMED_ROLLBACK: &str = "typedef int Ghost = ; int after;\n";
const INCOMPATIBLE_REDECLARATION: &str = "typedef int T; typedef long T; int after;\n";
const COMPATIBLE_REDECLARATION: &str = "typedef int T; typedef int T; T value;\n";
const PARAMETER_POINT_OF_DECLARATION: &str = "typedef int T; int f(T T, T x);\n";
const TYPEDEF_SPELLED_LABEL: &str = "typedef int T; int f(void) { T: return 0; }\n";
const QUALIFIED_TYPEDEF_ROLLBACK: &str = "typedef int T; const T broken = ; int after;\n";
const QUALIFIED_POINTER_AND_ARRAY: &str =
    "typedef int *P; typedef int A[3]; const P pointer; A const array;\n";
const ABSTRACT_FUNCTION_PARAMETER: &str = "typedef int T; T f(T (T));\n";
const GROUPED_ABSTRACT_FUNCTION_PARAMETER: &str = "typedef int T; T f(T ((T)));\n";

const SOURCES: &[SourceCase] = &[
    SourceCase {
        id: "typedef.file-use",
        source: FILE_ALIAS,
    },
    SourceCase {
        id: "typedef.nested-use",
        source: NESTED_ALIAS,
    },
    SourceCase {
        id: "typedef.point-of-declaration",
        source: POINT_OF_DECLARATION,
    },
    SourceCase {
        id: "typedef.pointer-declarator",
        source: POINTER_DECLARATOR,
    },
    SourceCase {
        id: "typedef.array-declarator",
        source: ARRAY_DECLARATOR,
    },
    SourceCase {
        id: "typedef.function-declarator",
        source: FUNCTION_DECLARATOR,
    },
    SourceCase {
        id: "typedef.function-pointer-declarator",
        source: FUNCTION_POINTER_DECLARATOR,
    },
    SourceCase {
        id: "typedef.cast-sizeof-alignof",
        source: CAST_SIZEOF_ALIGNOF,
    },
    SourceCase {
        id: "typedef.abstract-type-names",
        source: ABSTRACT_TYPE_NAMES,
    },
    SourceCase {
        id: "typedef.record-member",
        source: RECORD_MEMBER_ALIAS,
    },
    SourceCase {
        id: "typedef.expression-misuse",
        source: TYPEDEF_AS_EXPRESSION,
    },
    SourceCase {
        id: "typedef.then-object-conflict",
        source: TYPEDEF_THEN_OBJECT,
    },
    SourceCase {
        id: "object.then-typedef-conflict",
        source: OBJECT_THEN_TYPEDEF,
    },
    SourceCase {
        id: "typedef.malformed-rollback",
        source: MALFORMED_ROLLBACK,
    },
    SourceCase {
        id: "typedef.incompatible-redeclaration",
        source: INCOMPATIBLE_REDECLARATION,
    },
    SourceCase {
        id: "typedef.compatible-redeclaration",
        source: COMPATIBLE_REDECLARATION,
    },
    SourceCase {
        id: "typedef.parameter-point-of-declaration",
        source: PARAMETER_POINT_OF_DECLARATION,
    },
    SourceCase {
        id: "typedef.label-namespace",
        source: TYPEDEF_SPELLED_LABEL,
    },
    SourceCase {
        id: "typedef.qualified-use-rollback",
        source: QUALIFIED_TYPEDEF_ROLLBACK,
    },
    SourceCase {
        id: "typedef.qualified-pointer-and-array",
        source: QUALIFIED_POINTER_AND_ARRAY,
    },
    SourceCase {
        id: "typedef.abstract-function-parameter-ambiguity",
        source: ABSTRACT_FUNCTION_PARAMETER,
    },
    SourceCase {
        id: "typedef.grouped-abstract-function-parameter",
        source: GROUPED_ABSTRACT_FUNCTION_PARAMETER,
    },
];

fn file() -> FileId {
    FileId::new(FILE_RAW)
}

fn span_of(source: &str, spelling: &str, occurrence: usize) -> SourceSpan {
    let bytes = source.as_bytes();
    let needle = spelling.as_bytes();
    let (start, matched) = source
        .match_indices(spelling)
        .filter(|(start, _)| {
            let before = start.checked_sub(1).and_then(|index| bytes.get(index));
            let after = bytes.get(start + needle.len());
            !before.is_some_and(|byte| is_identifier_byte(*byte))
                && !after.is_some_and(|byte| is_identifier_byte(*byte))
        })
        .nth(occurrence)
        .unwrap_or_else(|| panic!("missing occurrence {occurrence} of {spelling:?}"));
    SourceSpan::new(
        file(),
        start as u32,
        (start + matched.len()) as u32,
        1,
        start as u32 + 1,
    )
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn span_text(source: &str, span: SourceSpan) -> &str {
    assert_eq!(span.file, file());
    &source[span.byte_start as usize..span.byte_end as usize]
}

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_owned(),
            Err(_) => "non-string panic payload".to_owned(),
        },
    }
}

fn parse_without_panic<'arena>(
    source: &str,
    arena: &'arena Arena,
) -> (evocc_chir::CHirUnit<'arena>, Option<ParseDiagnostic>) {
    let mut lexer = Lexer::new(file(), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }

    match catch_unwind(AssertUnwindSafe(|| {
        let mut parser = Parser::new(tokens, file(), source);
        let unit = parser.parse_translation_unit_in(arena);
        (unit, parser.diagnostic().cloned())
    })) {
        Ok(parsed) => parsed,
        Err(payload) => panic!(
            "parser panicked instead of returning CHIR/diagnostic for {source:?}: {}",
            panic_text(payload)
        ),
    }
}

fn parse_valid<'arena>(source: &str, arena: &'arena Arena) -> evocc_chir::CHirUnit<'arena> {
    let (unit, diagnostic) = parse_without_panic(source, arena);
    assert_eq!(
        diagnostic
            .as_ref()
            .map(|item| (item.code, item.message.as_str())),
        None,
        "valid source was rejected"
    );
    unit
}

fn typedef<'arena>(unit: &evocc_chir::CHirUnit<'arena>, index: usize) -> &'arena TypedefDecl {
    match unit.declarations()[index] {
        CHirDecl::Typedef(declaration) => declaration,
        other => panic!("declaration {index} is not a typedef: {other:?}"),
    }
}

fn object<'arena>(unit: &evocc_chir::CHirUnit<'arena>, index: usize) -> &'arena ObjectDecl<'arena> {
    match unit.declarations()[index] {
        CHirDecl::Object(declaration) => declaration,
        other => panic!("declaration {index} is not an object: {other:?}"),
    }
}

fn function<'arena>(
    unit: &evocc_chir::CHirUnit<'arena>,
    index: usize,
) -> &'arena FunctionDecl<'arena> {
    match unit.declarations()[index] {
        CHirDecl::Function(declaration) => declaration,
        other => panic!("declaration {index} is not a function: {other:?}"),
    }
}

fn compound_items<'unit, 'arena>(
    statement: &'unit CHirStmt<'arena>,
) -> &'unit [CHirBlockItem<'arena>] {
    match statement {
        CHirStmt::Compound(items, _) => items,
        other => panic!("expected compound statement, got {other:?}"),
    }
}

fn block_declaration<'arena>(item: &CHirBlockItem<'arena>) -> &'arena CHirDecl<'arena> {
    match item {
        CHirBlockItem::Declaration(declaration) => declaration,
        other => panic!("expected declaration block item, got {other:?}"),
    }
}

fn block_statement<'arena>(item: &CHirBlockItem<'arena>) -> &'arena CHirStmt<'arena> {
    match item {
        CHirBlockItem::Statement(statement) => statement,
        other => panic!("expected statement block item, got {other:?}"),
    }
}

fn assert_alias_specifier(
    specifier: &TypeSpecifier,
    source: &str,
    spelling: &str,
    occurrence: usize,
    expected_symbol: SymbolId,
    expected_type: TypeId,
) {
    let TypeSpecifier::TypedefName {
        ty,
        symbol,
        name,
        span,
    } = specifier
    else {
        panic!("expected retained typedef-name source fact, got {specifier:?}");
    };
    assert_eq!(*ty, expected_type, "typedef-name TypeId join");
    assert_eq!(*symbol, expected_symbol, "typedef-name SymbolId join");
    assert_eq!(name.spelling, spelling);
    assert_eq!(name.span, span_of(source, spelling, occurrence));
    assert_eq!(*span, name.span, "unqualified specifier span");
    assert_eq!(specifier.type_id(), expected_type);
    assert_eq!(specifier.span(), *span);
}

fn assert_qualified_alias_specifier(
    specifier: &TypeSpecifier,
    source: &str,
    spelling: &str,
    occurrence: usize,
    expected_whole: &str,
    expected_symbol: SymbolId,
    expected_type: TypeId,
) {
    let TypeSpecifier::TypedefName {
        ty,
        symbol,
        name,
        span,
    } = specifier
    else {
        panic!("expected retained qualified typedef-name fact, got {specifier:?}");
    };
    assert_eq!(*ty, expected_type, "qualified typedef-name TypeId join");
    assert_eq!(*symbol, expected_symbol, "qualified typedef SymbolId join");
    assert_eq!(name.spelling, spelling);
    assert_eq!(name.span, span_of(source, spelling, occurrence));
    assert_eq!(span_text(source, *span), expected_whole);
    assert_eq!(specifier.type_id(), expected_type);
    assert_eq!(specifier.span(), *span);
}

fn assert_identifier_symbol(expression: &CHirExpr<'_>, expected: SymbolId) {
    let CHirExpr::Ident { symbol, .. } = expression else {
        panic!("expected identifier expression, got {expression:?}");
    };
    assert_eq!(*symbol, expected);
}

#[test]
fn source_matrix_is_twenty_two_and_byte_unique() {
    assert_eq!(SOURCES.len(), 22, "S2 typedef denominator changed");
    let mut ids = HashSet::new();
    let mut bytes = HashSet::new();
    for case in SOURCES {
        assert!(ids.insert(case.id), "duplicate case ID {}", case.id);
        assert!(
            bytes.insert(case.source.as_bytes()),
            "duplicate source bytes for {}",
            case.id
        );
        assert!(case.source.ends_with('\n'), "{} lacks newline", case.id);
    }
}

#[test]
fn file_typedef_use_joins_source_symbol_type_and_specifier() {
    let arena = Arena::default();
    let unit = parse_valid(FILE_ALIAS, &arena);
    assert_eq!(unit.declarations().len(), 2);
    let alias = typedef(&unit, 0);
    let value = object(&unit, 1);
    let file_scope = unit.symbols().file_scope();

    assert_eq!((alias.symbol.raw(), value.symbol.raw()), (0, 1));
    assert_eq!(alias.ty, value.ty);
    assert_eq!(
        unit.symbols().symbol(alias.symbol).unwrap().scope(),
        file_scope
    );
    assert_eq!(
        unit.symbols().symbol(alias.symbol).unwrap().kind(),
        SymbolKind::Typedef
    );
    assert_eq!(unit.symbols().symbol(alias.symbol).unwrap().ty(), alias.ty);
    assert_eq!(
        unit.symbols().symbol(alias.symbol).unwrap().span(),
        span_of(FILE_ALIAS, "Word", 0)
    );
    assert_eq!(
        unit.symbols().resolve(file_scope, "Word").unwrap(),
        Some(alias.symbol)
    );
    assert_alias_specifier(
        &value.specifier,
        FILE_ALIAS,
        "Word",
        1,
        alias.symbol,
        alias.ty,
    );
}

#[test]
fn nested_typedef_lives_in_function_scope_and_drives_declaration_and_sizeof() {
    let arena = Arena::default();
    let unit = parse_valid(NESTED_ALIAS, &arena);
    let function = function(&unit, 0);
    let items = compound_items(function.body.expect("function body"));
    let CHirDecl::Typedef(alias) = block_declaration(&items[0]) else {
        panic!("expected nested typedef");
    };
    let CHirDecl::Object(value) = block_declaration(&items[1]) else {
        panic!("expected nested object");
    };
    let function_scope = unit.symbols().symbol(alias.symbol).unwrap().scope();

    assert_eq!(
        unit.symbols().scope(function_scope).unwrap().kind(),
        ScopeKind::Function
    );
    assert_eq!(
        unit.symbols().scope(function_scope).unwrap().parent(),
        Some(unit.symbols().file_scope())
    );
    assert_eq!(
        (
            function.symbol.raw(),
            alias.symbol.raw(),
            value.symbol.raw()
        ),
        (0, 1, 2)
    );
    assert_eq!(alias.ty, value.ty);
    assert_alias_specifier(
        &value.specifier,
        NESTED_ALIAS,
        "Byte",
        1,
        alias.symbol,
        alias.ty,
    );

    let CHirStmt::Return(Some(expression), _) = block_statement(&items[2]) else {
        panic!("expected return expression");
    };
    let CHirExpr::Binary(_, size, value_use, _) = expression else {
        panic!("expected sizeof(Byte) + value");
    };
    let CHirExpr::SizeOf(size_ty, size_span) = size else {
        panic!("expected sizeof type-name");
    };
    assert_eq!(*size_ty, alias.ty);
    assert_eq!(span_text(NESTED_ALIAS, *size_span), "sizeof(Byte)");
    assert_identifier_symbol(value_use, value.symbol);
}

#[test]
fn declarator_name_hides_typedef_only_after_its_own_type_specifier() {
    let arena = Arena::default();
    let unit = parse_valid(POINT_OF_DECLARATION, &arena);
    let outer_alias = typedef(&unit, 0);
    let function = function(&unit, 1);
    let outer_items = compound_items(function.body.expect("function body"));
    let nested_items = compound_items(block_statement(&outer_items[0]));
    let CHirDecl::Object(inner) = block_declaration(&nested_items[0]) else {
        panic!("expected T T declaration");
    };
    let CHirDecl::Object(after) = block_declaration(&outer_items[1]) else {
        panic!("expected declaration after nested shadow");
    };

    assert_eq!(
        (
            outer_alias.symbol.raw(),
            function.symbol.raw(),
            inner.symbol.raw(),
            after.symbol.raw()
        ),
        (0, 1, 2, 3)
    );
    assert_alias_specifier(
        &inner.specifier,
        POINT_OF_DECLARATION,
        "T",
        1,
        outer_alias.symbol,
        outer_alias.ty,
    );
    assert_eq!(inner.name.span, span_of(POINT_OF_DECLARATION, "T", 2));
    assert_alias_specifier(
        &after.specifier,
        POINT_OF_DECLARATION,
        "T",
        5,
        outer_alias.symbol,
        outer_alias.ty,
    );

    let inner_scope = unit.symbols().symbol(inner.symbol).unwrap().scope();
    let function_scope = unit.symbols().symbol(after.symbol).unwrap().scope();
    assert_eq!(
        unit.symbols().scope(inner_scope).unwrap().parent(),
        Some(function_scope)
    );
    assert_eq!(
        unit.symbols().resolve(inner_scope, "T").unwrap(),
        Some(inner.symbol)
    );
    assert_eq!(
        unit.symbols().resolve(function_scope, "T").unwrap(),
        Some(outer_alias.symbol)
    );

    let CHirStmt::Expr(assignment, _) = block_statement(&nested_items[1]) else {
        panic!("expected assignment after T T");
    };
    let CHirExpr::Assign {
        target: lhs,
        value: rhs,
        ..
    } = assignment
    else {
        panic!("expected assignment expression");
    };
    assert_identifier_symbol(lhs, inner.symbol);
    let CHirExpr::Binary(_, rhs_t, _, _) = rhs else {
        panic!("expected T + 1");
    };
    assert_identifier_symbol(rhs_t, inner.symbol);
}

#[test]
fn typedef_specifiers_feed_pointer_array_function_and_function_pointer_declarators() {
    let arena = Arena::default();

    let pointer_unit = parse_valid(POINTER_DECLARATOR, &arena);
    let pointer_alias = typedef(&pointer_unit, 0);
    let pointer = object(&pointer_unit, 1);
    assert_alias_specifier(
        &pointer.specifier,
        POINTER_DECLARATOR,
        "Elem",
        1,
        pointer_alias.symbol,
        pointer_alias.ty,
    );
    assert_eq!(
        pointer_unit.types().get(pointer.ty).unwrap().kind,
        TypeKind::Pointer(pointer_alias.ty)
    );

    let array_unit = parse_valid(ARRAY_DECLARATOR, &arena);
    let array_alias = typedef(&array_unit, 0);
    let array = object(&array_unit, 1);
    assert_alias_specifier(
        &array.specifier,
        ARRAY_DECLARATOR,
        "Elem",
        1,
        array_alias.symbol,
        array_alias.ty,
    );
    assert_eq!(
        array_unit.types().get(array.ty).unwrap().kind,
        TypeKind::Array {
            element: array_alias.ty,
            bound: ArrayBound::Fixed(3)
        }
    );

    let function_unit = parse_valid(FUNCTION_DECLARATOR, &arena);
    let result_alias = typedef(&function_unit, 0);
    let convert = function(&function_unit, 1);
    assert_alias_specifier(
        &convert.return_specifier,
        FUNCTION_DECLARATOR,
        "Result",
        1,
        result_alias.symbol,
        result_alias.ty,
    );
    let evocc_chir::ParameterSyntax::Prototype {
        parameters,
        variadic,
    } = &convert.parameters
    else {
        panic!("expected prototype");
    };
    assert!(!variadic);
    assert_eq!(parameters.len(), 1);
    assert_alias_specifier(
        &parameters[0].specifier,
        FUNCTION_DECLARATOR,
        "Result",
        2,
        result_alias.symbol,
        result_alias.ty,
    );
    let TypeKind::Function {
        return_type,
        parameters: canonical_parameters,
    } = &function_unit.types().get(convert.ty).unwrap().kind
    else {
        panic!("expected canonical function type");
    };
    assert_eq!(*return_type, result_alias.ty);
    assert_eq!(
        canonical_parameters,
        &FunctionParameters::Prototype {
            types: vec![result_alias.ty],
            variadic: false
        }
    );

    let callback_unit = parse_valid(FUNCTION_POINTER_DECLARATOR, &arena);
    let scalar_alias = typedef(&callback_unit, 0);
    let callback = object(&callback_unit, 1);
    assert_alias_specifier(
        &callback.specifier,
        FUNCTION_POINTER_DECLARATOR,
        "Scalar",
        1,
        scalar_alias.symbol,
        scalar_alias.ty,
    );
    let TypeKind::Pointer(function_ty) = callback_unit.types().get(callback.ty).unwrap().kind
    else {
        panic!("callback must be pointer-to-function");
    };
    let TypeKind::Function {
        return_type,
        parameters,
    } = &callback_unit.types().get(function_ty).unwrap().kind
    else {
        panic!("callback pointee must be a function");
    };
    assert_eq!(*return_type, scalar_alias.ty);
    assert_eq!(
        parameters,
        &FunctionParameters::Prototype {
            types: vec![scalar_alias.ty],
            variadic: false
        }
    );
}

#[test]
fn parenthesized_typedef_in_parameter_is_an_abstract_function_declarator() {
    for (source, written_span) in [
        (ABSTRACT_FUNCTION_PARAMETER, "(T)"),
        (GROUPED_ABSTRACT_FUNCTION_PARAMETER, "((T))"),
    ] {
        let arena = Arena::default();
        let unit = parse_valid(source, &arena);
        let alias = typedef(&unit, 0);
        let function = function(&unit, 1);
        assert_alias_specifier(
            &function.return_specifier,
            source,
            "T",
            1,
            alias.symbol,
            alias.ty,
        );
        let evocc_chir::ParameterSyntax::Prototype {
            parameters,
            variadic,
        } = &function.parameters
        else {
            panic!("f must have a prototype for {source:?}");
        };
        assert!(!variadic);
        assert_eq!(parameters.len(), 1);
        let parameter = &parameters[0];
        assert_eq!(
            parameter.name, None,
            "parenthesized typedef is not a named scalar parameter: {source:?}"
        );
        assert_eq!(
            parameter.symbol, None,
            "anonymous parameter has no SymbolId"
        );
        assert_alias_specifier(&parameter.specifier, source, "T", 2, alias.symbol, alias.ty);
        let [WrittenDeclaratorLayer::Function { span, .. }] = parameter.written.layers.as_slice()
        else {
            panic!("parameter must retain the written abstract function layer");
        };
        assert_eq!(span_text(source, *span), "(T)");
        assert_eq!(span_text(source, parameter.written.span), written_span);

        let TypeKind::Pointer(pointee) = unit.types().get(parameter.ty).unwrap().kind else {
            panic!("function parameter must adjust to pointer-to-function");
        };
        let TypeKind::Function {
            return_type,
            parameters: inner_parameters,
        } = &unit.types().get(pointee).unwrap().kind
        else {
            panic!("adjusted parameter must point to a function type");
        };
        assert_eq!(*return_type, alias.ty);
        assert_eq!(
            inner_parameters,
            &FunctionParameters::Prototype {
                types: vec![alias.ty],
                variadic: false
            }
        );
        assert_eq!(
            unit.symbols().symbols_in_source_order().len(),
            2,
            "ambiguous typedef must not allocate a parameter binding"
        );
    }
}

#[test]
fn typedef_names_disambiguate_cast_sizeof_alignof_and_abstract_type_names() {
    let arena = Arena::default();
    let unit = parse_valid(CAST_SIZEOF_ALIGNOF, &arena);
    let alias = typedef(&unit, 0);
    let cast_function = function(&unit, 1);
    let items = compound_items(cast_function.body.expect("function body"));
    let CHirStmt::Return(Some(root), _) = block_statement(&items[0]) else {
        panic!("expected return expression");
    };
    let CHirExpr::Binary(_, left, align, _) = root else {
        panic!("expected (... + sizeof) + _Alignof");
    };
    let CHirExpr::Binary(_, cast, size, _) = left else {
        panic!("expected cast + sizeof");
    };
    let CHirExpr::Cast(cast_ty, _, cast_span) = cast else {
        panic!("expected typedef-name cast");
    };
    let CHirExpr::SizeOf(size_ty, size_span) = size else {
        panic!("expected typedef-name sizeof");
    };
    let CHirExpr::AlignOf(align_ty, align_span) = align else {
        panic!("expected typedef-name _Alignof");
    };
    assert_eq!(
        (*cast_ty, *size_ty, *align_ty),
        (alias.ty, alias.ty, alias.ty)
    );
    assert_eq!(span_text(CAST_SIZEOF_ALIGNOF, *cast_span), "(Word)value");
    assert_eq!(span_text(CAST_SIZEOF_ALIGNOF, *size_span), "sizeof(Word)");
    assert_eq!(
        span_text(CAST_SIZEOF_ALIGNOF, *align_span),
        "_Alignof(Word)"
    );

    let abstract_unit = parse_valid(ABSTRACT_TYPE_NAMES, &arena);
    let abstract_alias = typedef(&abstract_unit, 0);
    let function = function(&abstract_unit, 1);
    let items = compound_items(function.body.expect("function body"));
    let CHirStmt::Return(Some(root), _) = block_statement(&items[0]) else {
        panic!("expected abstract type-name return");
    };
    let CHirExpr::Binary(_, pointer_size, array_size, _) = root else {
        panic!("expected two sizeof expressions");
    };
    let CHirExpr::SizeOf(pointer_ty, _) = pointer_size else {
        panic!("expected pointer sizeof");
    };
    let CHirExpr::SizeOf(array_ty, _) = array_size else {
        panic!("expected array sizeof");
    };
    assert_eq!(
        abstract_unit.types().get(*pointer_ty).unwrap().kind,
        TypeKind::Pointer(abstract_alias.ty)
    );
    assert_eq!(
        abstract_unit.types().get(*array_ty).unwrap().kind,
        TypeKind::Array {
            element: abstract_alias.ty,
            bound: ArrayBound::Fixed(3)
        }
    );
}

#[test]
fn record_member_retains_the_typedef_source_fact() {
    let arena = Arena::default();
    let unit = parse_valid(RECORD_MEMBER_ALIAS, &arena);
    let alias = typedef(&unit, 0);
    let CHirDecl::Tag(tag) = unit.declarations()[1] else {
        panic!("expected struct tag declaration");
    };
    let TagMembers::Struct(members) = &tag.members else {
        panic!("expected struct members");
    };
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].ty, alias.ty);
    assert_alias_specifier(
        &members[0].specifier,
        RECORD_MEMBER_ALIAS,
        "Field",
        1,
        alias.symbol,
        alias.ty,
    );
}

#[test]
fn label_syntax_wins_over_a_visible_typedef_name() {
    let arena = Arena::default();
    let unit = parse_valid(TYPEDEF_SPELLED_LABEL, &arena);
    let alias = typedef(&unit, 0);
    let function = function(&unit, 1);
    let items = compound_items(function.body.expect("function body"));
    assert_eq!(items.len(), 1);
    let CHirStmt::Label(name, nested, _) = block_statement(&items[0]) else {
        panic!("typedef-spelled label was not retained as label syntax");
    };
    assert_eq!(name.spelling, "T");
    assert_eq!(name.span, span_of(TYPEDEF_SPELLED_LABEL, "T", 1));
    assert!(matches!(nested, CHirStmt::Return(Some(_), _)));
    assert_eq!(
        unit.symbols().symbols_in_source_order().len(),
        2,
        "a label must not allocate an ordinary SymbolId"
    );
    assert_eq!(
        unit.symbols()
            .resolve(unit.symbols().file_scope(), "T")
            .unwrap(),
        Some(alias.symbol)
    );
}

#[test]
fn qualified_typedef_use_rolls_back_symbol_scope_and_canonical_type() {
    let arena = Arena::default();
    let (unit, diagnostic) = parse_without_panic(QUALIFIED_TYPEDEF_ROLLBACK, &arena);
    let diagnostic = diagnostic.expect("malformed qualified typedef use must diagnose");
    assert_eq!(diagnostic.code, "E1003");
    assert_eq!(diagnostic.message, "expected-expression");
    assert_eq!(span_text(QUALIFIED_TYPEDEF_ROLLBACK, diagnostic.span), ";");

    let file_scope = unit.symbols().file_scope();
    assert_eq!(unit.declarations().len(), 2);
    let alias = typedef(&unit, 0);
    let after = object(&unit, 1);
    assert_eq!((alias.symbol.raw(), after.symbol.raw()), (0, 1));
    assert_eq!(unit.symbols().resolve(file_scope, "broken").unwrap(), None);
    assert_eq!(unit.symbols().scopes_in_source_order().len(), 1);
    assert_eq!(
        unit.types().len(),
        1,
        "qualified TypeId leaked past rollback"
    );
}

#[test]
fn typedef_qualifiers_apply_to_pointer_top_and_array_element() {
    let arena = Arena::default();
    let unit = parse_valid(QUALIFIED_POINTER_AND_ARRAY, &arena);
    let pointer_alias = typedef(&unit, 0);
    let array_alias = typedef(&unit, 1);
    let pointer = object(&unit, 2);
    let array = object(&unit, 3);
    assert_eq!(
        (
            pointer_alias.symbol.raw(),
            array_alias.symbol.raw(),
            pointer.symbol.raw(),
            array.symbol.raw(),
        ),
        (0, 1, 2, 3)
    );

    assert_qualified_alias_specifier(
        &pointer.specifier,
        QUALIFIED_POINTER_AND_ARRAY,
        "P",
        1,
        "const P",
        pointer_alias.symbol,
        pointer.ty,
    );
    let pointer_alias_type = unit.types().get(pointer_alias.ty).unwrap();
    assert_eq!(pointer_alias_type.qualifiers, Qualifiers::default());
    let TypeKind::Pointer(alias_pointee) = pointer_alias_type.kind else {
        panic!("P must be a pointer typedef");
    };
    let pointer_type = unit.types().get(pointer.ty).unwrap();
    assert_eq!(
        pointer_type.qualifiers,
        Qualifiers {
            is_const: true,
            ..Qualifiers::default()
        }
    );
    assert_eq!(pointer_type.kind, TypeKind::Pointer(alias_pointee));
    assert_eq!(
        unit.types().get(alias_pointee).unwrap().qualifiers,
        Qualifiers::default(),
        "const P must not become pointer-to-const-int"
    );

    assert_qualified_alias_specifier(
        &array.specifier,
        QUALIFIED_POINTER_AND_ARRAY,
        "A",
        1,
        "A const",
        array_alias.symbol,
        array.ty,
    );
    let array_alias_type = unit.types().get(array_alias.ty).unwrap();
    assert_eq!(array_alias_type.qualifiers, Qualifiers::default());
    let TypeKind::Array {
        element: alias_element,
        bound: alias_bound,
    } = &array_alias_type.kind
    else {
        panic!("A must be an array typedef");
    };
    assert_eq!(*alias_bound, ArrayBound::Fixed(3));
    let array_type = unit.types().get(array.ty).unwrap();
    assert_eq!(
        array_type.qualifiers,
        Qualifiers::default(),
        "C11 represents a qualified array typedef through its element"
    );
    let TypeKind::Array {
        element: qualified_element,
        bound,
    } = &array_type.kind
    else {
        panic!("qualified A must remain an array");
    };
    assert_eq!(*bound, ArrayBound::Fixed(3));
    let alias_element = unit.types().get(*alias_element).unwrap();
    let qualified_element = unit.types().get(*qualified_element).unwrap();
    assert_eq!(qualified_element.kind, alias_element.kind);
    assert_eq!(alias_element.qualifiers, Qualifiers::default());
    assert_eq!(
        qualified_element.qualifiers,
        Qualifiers {
            is_const: true,
            ..Qualifiers::default()
        }
    );
}

#[test]
fn typedef_expression_and_ordinary_namespace_conflicts_are_controlled() {
    for (source, code, message, spelling, occurrence) in [
        (
            TYPEDEF_AS_EXPRESSION,
            "E2003",
            "typedef-name-not-expression: T",
            "T",
            1,
        ),
        (
            TYPEDEF_THEN_OBJECT,
            "E2002",
            "incompatible-redeclaration: T",
            "T",
            1,
        ),
        (
            OBJECT_THEN_TYPEDEF,
            "E2002",
            "incompatible-redeclaration: X",
            "X",
            1,
        ),
        (
            PARAMETER_POINT_OF_DECLARATION,
            "E1001",
            "expected-declaration",
            "T",
            3,
        ),
    ] {
        let arena = Arena::default();
        let (_, diagnostic) = parse_without_panic(source, &arena);
        let diagnostic =
            diagnostic.unwrap_or_else(|| panic!("invalid typedef use must diagnose: {source:?}"));
        assert_eq!(diagnostic.code, code, "source {source:?}");
        assert_eq!(diagnostic.message, message, "source {source:?}");
        assert_eq!(diagnostic.span, span_of(source, spelling, occurrence));
    }
}

#[test]
fn typedef_expression_error_rolls_back_function_without_phantoms() {
    let arena = Arena::default();
    let (unit, diagnostic) = parse_without_panic(TYPEDEF_AS_EXPRESSION, &arena);
    let diagnostic = diagnostic.expect("typedef expression must diagnose");
    assert_eq!(diagnostic.code, "E2003");
    assert_eq!(diagnostic.message, "typedef-name-not-expression: T");

    let file_scope = unit.symbols().file_scope();
    assert_eq!(unit.declarations().len(), 2);
    let alias = typedef(&unit, 0);
    let after = object(&unit, 1);
    assert_eq!((alias.symbol.raw(), after.symbol.raw()), (0, 1));
    assert_eq!(unit.symbols().resolve(file_scope, "bad").unwrap(), None);
    assert_eq!(unit.symbols().scopes_in_source_order().len(), 1);
    assert_eq!(unit.symbols().symbols_in_source_order().len(), 2);
}

#[test]
fn malformed_typedef_rolls_back_without_phantom_symbol_or_type_use() {
    let arena = Arena::default();
    let (unit, diagnostic) = parse_without_panic(MALFORMED_ROLLBACK, &arena);
    let diagnostic = diagnostic.expect("malformed typedef must diagnose");
    assert_eq!(diagnostic.code, "E1003");
    assert_eq!(diagnostic.message, "expected-expression");
    assert_eq!(span_text(MALFORMED_ROLLBACK, diagnostic.span), ";");

    let file_scope = unit.symbols().file_scope();
    assert_eq!(unit.symbols().resolve(file_scope, "Ghost").unwrap(), None);
    assert_eq!(unit.symbols().symbols_in_source_order().len(), 1);
    let after = object(&unit, 0);
    assert_eq!(
        after.symbol.raw(),
        0,
        "rolled-back typedef consumed a SymbolId"
    );
    assert_eq!(after.name.spelling, "after");
}

#[test]
fn compatible_redeclaration_reuses_identity_and_incompatible_one_rolls_back() {
    let arena = Arena::default();
    let unit = parse_valid(COMPATIBLE_REDECLARATION, &arena);
    let first = typedef(&unit, 0);
    let second = typedef(&unit, 1);
    let value = object(&unit, 2);
    assert_eq!(first.symbol, second.symbol);
    assert_eq!(first.ty, second.ty);
    assert_eq!(unit.symbols().symbols_in_source_order().len(), 2);
    assert_alias_specifier(
        &value.specifier,
        COMPATIBLE_REDECLARATION,
        "T",
        2,
        first.symbol,
        first.ty,
    );

    let arena = Arena::default();
    let (unit, diagnostic) = parse_without_panic(INCOMPATIBLE_REDECLARATION, &arena);
    let diagnostic = diagnostic.expect("incompatible typedef must diagnose");
    assert_eq!(diagnostic.code, "E2002");
    assert_eq!(diagnostic.message, "incompatible-redeclaration: T");
    assert_eq!(diagnostic.span, span_of(INCOMPATIBLE_REDECLARATION, "T", 1));
    assert_eq!(unit.declarations().len(), 2);
    let retained = typedef(&unit, 0);
    let after = object(&unit, 1);
    assert_eq!((retained.symbol.raw(), after.symbol.raw()), (0, 1));
    assert_eq!(
        unit.symbols()
            .resolve(unit.symbols().file_scope(), "T")
            .unwrap(),
        Some(retained.symbol)
    );
}
