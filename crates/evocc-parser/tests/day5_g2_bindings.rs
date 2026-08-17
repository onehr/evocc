use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

use evocc_chir::{
    CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, ExprCategory, FunctionDecl,
    ObjectDecl, ParameterSyntax, ScopeId, ScopeKind, StorageClass, SymbolId, SymbolKind, TagDecl,
    TagMembers, TypeSpecifier, TypedefDecl, WrittenDeclaratorLayer,
};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::{ArrayBound, FunctionParameters, IntRank, TypeId, TypeKind};

const FILE_RAW: u32 = 71;

struct SourceCase {
    id: &'static str,
    source: &'static str,
}

#[derive(Clone, Copy)]
enum UnnamedParameterShape {
    Pointer,
    Array,
    FunctionPointer,
}

const FILE_BINDINGS: &str = "int first; int second;\n";
const FUNCTION_SCOPE: &str = "int f(int p) { int local; return p + local; }\n";
const NESTED_SHADOW: &str = "int f(int x) { { int x; x = 1; } return x; }\n";
const FOR_SCOPE: &str = "int f(void) { for (int i = 0; i < 1; i = i + 1) ; return 0; }\n";
const ENUM_BINDINGS: &str = "enum E { A = 3, B = A + 1 }; int value = B;\n";
const TYPEDEF_BINDINGS: &str = "typedef int T;\n";
const IDENT_FACTS: &str = "int callee(void) { return 0; } int mutable; const int fixed = 1; int use(int parameter) { return callee() + mutable + fixed + parameter; }\n";
const ENUM_LEAK: &str = "int f(void) { { enum { A = 1 }; } return A; }\n";
const DUPLICATE_ROLLBACK: &str = "int same; long same; int after;\n";
const TAG_NAMESPACE: &str = "struct Node { int member; }; int Node;\n";
const PROTOTYPE_ENUM_LEAK: &str = "int f(enum E { A=1 } x); int leaked=A;\n";
const DEFINITION_ENUM_SCOPE: &str = "int f(enum E { A=1 } x){ return A; }\n";
const REPEATED_FORWARD: &str = "struct S; struct S;\n";
const INNER_FORWARD: &str = "struct S; int f(void){ struct S; struct S *p; return 0; }\n";
const ARRAY_COMPOSITE: &str = "extern int items[]; int items[3];\n";
const FUNCTION_COMPOSITE: &str = "int convert(); int convert(int value);\n";
const EXTERN_THEN_TENTATIVE: &str = "extern int external; int external;\n";
const STATIC_THEN_EXTERN: &str = "static int internal; extern int internal;\n";
const STATIC_FUNCTION_THEN_NONE: &str = "static int f(void); int f(void);\n";
const REPEATED_BLOCK_EXTERN: &str =
    "int f(void) { extern int shared; extern int shared; return shared; }\n";
const SIBLING_EXTERN_THEN_FILE: &str =
    "int f(void){ {extern int x; x=1;} {extern int x; return x;} } int x;\n";
const COMPATIBLE_TYPEDEF: &str = "typedef int Alias; typedef int Alias;\n";
const TENTATIVE_THEN_DEFINITION: &str =
    "int completed; int completed; int completed = 1; extern int completed;\n";
const LINKAGE_CONFLICT: &str =
    "extern int conflict; static int conflict; static int conflict; int after;\n";
const DUPLICATE_FUNCTION_DEFINITION: &str =
    "int defined(void) { return 1; } int defined(void) { return 2; } int defined(void) { return 3; } int after;\n";
const DUPLICATE_OBJECT_DEFINITION: &str =
    "int defined = 1; int defined = 2; int defined = 3; int after;\n";
const INCOMPATIBLE_TYPEDEF: &str =
    "typedef int Alias; typedef long Alias; typedef int Alias; int after;\n";
const INCOMPATIBLE_ARRAY_BOUND: &str =
    "extern int width[2]; extern int width[3]; extern int width[2]; int after;\n";
const INCOMPATIBLE_FUNCTION_PROTO: &str =
    "int signature(); int signature(int value, ...); int signature(); int after;\n";
const UNNAMED_POINTER_PARAMETER: &str = "int f(int *);\n";
const UNNAMED_ARRAY_PARAMETER: &str = "int f(int [3]);\n";
const UNNAMED_FUNCTION_POINTER_PARAMETER: &str = "int f(int (*)(int));\n";
const UNNAMED_POINTER_THEN_SCALAR: &str = "int f(const int *); int f(int);\n";
const UNNAMED_SCALAR_THEN_POINTER: &str = "int f(int); int f(const int *);\n";
const INNER_EXTERN_SKIPS_OUTER_AUTO: &str =
    "int x; int f(void){ int x; { extern int x; return x; } }\n";
const INNER_EXTERN_WITHOUT_FILE: &str = "int f(void){ int x; { extern int x; return x; } }\n";
const SAME_SCOPE_AUTO_EXTERN_CONFLICT: &str = "int f(void){ int x; extern int x; }\n";
const STATIC_OBJECT_THEN_PLAIN: &str = "static int x; int x;\n";
const PLAIN_OBJECT_THEN_STATIC: &str = "int x; static int x;\n";
const EXTERN_OBJECT_THEN_STATIC: &str = "extern int x; static int x;\n";
const PLAIN_FUNCTION_THEN_STATIC: &str = "int f(void); static int f(void);\n";
const THREAD_LOCAL_THEN_PLAIN: &str = "_Thread_local int x; int x;\n";
const STATIC_FUNCTION_THEN_EXTERN: &str = "static int f(void); extern int f(void);\n";
const TOP_QUALIFIED_PARAMETER: &str = "int f(const int); int f(int);\n";
const UNSPECIFIED_THEN_CHAR: &str = "int f(); int f(char);\n";

const SOURCES: &[SourceCase] = &[
    SourceCase {
        id: "binding.file-source-order",
        source: FILE_BINDINGS,
    },
    SourceCase {
        id: "scope.function-parameter-body",
        source: FUNCTION_SCOPE,
    },
    SourceCase {
        id: "scope.nested-shadow",
        source: NESTED_SHADOW,
    },
    SourceCase {
        id: "scope.for-lifetime",
        source: FOR_SCOPE,
    },
    SourceCase {
        id: "binding.enumerators",
        source: ENUM_BINDINGS,
    },
    SourceCase {
        id: "binding.typedef-name",
        source: TYPEDEF_BINDINGS,
    },
    SourceCase {
        id: "expression.identifier-facts",
        source: IDENT_FACTS,
    },
    SourceCase {
        id: "diagnostic.enum-scope-leak",
        source: ENUM_LEAK,
    },
    SourceCase {
        id: "diagnostic.duplicate-rollback",
        source: DUPLICATE_ROLLBACK,
    },
    SourceCase {
        id: "binding.tag-namespace",
        source: TAG_NAMESPACE,
    },
    SourceCase {
        id: "scope.prototype-enum-no-leak",
        source: PROTOTYPE_ENUM_LEAK,
    },
    SourceCase {
        id: "scope.definition-enum-visible",
        source: DEFINITION_ENUM_SCOPE,
    },
    SourceCase {
        id: "binding.repeated-forward-events",
        source: REPEATED_FORWARD,
    },
    SourceCase {
        id: "scope.inner-forward-hides-outer",
        source: INNER_FORWARD,
    },
    SourceCase {
        id: "redeclaration.array-composite",
        source: ARRAY_COMPOSITE,
    },
    SourceCase {
        id: "redeclaration.function-composite",
        source: FUNCTION_COMPOSITE,
    },
    SourceCase {
        id: "redeclaration.extern-then-tentative",
        source: EXTERN_THEN_TENTATIVE,
    },
    SourceCase {
        id: "redeclaration.static-then-extern",
        source: STATIC_THEN_EXTERN,
    },
    SourceCase {
        id: "redeclaration.static-function-then-none",
        source: STATIC_FUNCTION_THEN_NONE,
    },
    SourceCase {
        id: "redeclaration.repeated-block-extern",
        source: REPEATED_BLOCK_EXTERN,
    },
    SourceCase {
        id: "redeclaration.sibling-extern-then-file",
        source: SIBLING_EXTERN_THEN_FILE,
    },
    SourceCase {
        id: "redeclaration.compatible-typedef",
        source: COMPATIBLE_TYPEDEF,
    },
    SourceCase {
        id: "redeclaration.tentative-then-definition",
        source: TENTATIVE_THEN_DEFINITION,
    },
    SourceCase {
        id: "redeclaration.linkage-conflict",
        source: LINKAGE_CONFLICT,
    },
    SourceCase {
        id: "redeclaration.duplicate-function-definition",
        source: DUPLICATE_FUNCTION_DEFINITION,
    },
    SourceCase {
        id: "redeclaration.duplicate-object-definition",
        source: DUPLICATE_OBJECT_DEFINITION,
    },
    SourceCase {
        id: "redeclaration.incompatible-typedef",
        source: INCOMPATIBLE_TYPEDEF,
    },
    SourceCase {
        id: "redeclaration.incompatible-array-bound",
        source: INCOMPATIBLE_ARRAY_BOUND,
    },
    SourceCase {
        id: "redeclaration.incompatible-function-prototype",
        source: INCOMPATIBLE_FUNCTION_PROTO,
    },
    SourceCase {
        id: "parameter.unnamed-pointer",
        source: UNNAMED_POINTER_PARAMETER,
    },
    SourceCase {
        id: "parameter.unnamed-array",
        source: UNNAMED_ARRAY_PARAMETER,
    },
    SourceCase {
        id: "parameter.unnamed-function-pointer",
        source: UNNAMED_FUNCTION_POINTER_PARAMETER,
    },
    SourceCase {
        id: "parameter.unnamed-pointer-then-scalar-conflict",
        source: UNNAMED_POINTER_THEN_SCALAR,
    },
    SourceCase {
        id: "parameter.unnamed-scalar-then-pointer-conflict",
        source: UNNAMED_SCALAR_THEN_POINTER,
    },
    SourceCase {
        id: "redeclaration.inner-extern-skips-outer-auto",
        source: INNER_EXTERN_SKIPS_OUTER_AUTO,
    },
    SourceCase {
        id: "redeclaration.inner-extern-without-file",
        source: INNER_EXTERN_WITHOUT_FILE,
    },
    SourceCase {
        id: "redeclaration.same-scope-auto-extern-conflict",
        source: SAME_SCOPE_AUTO_EXTERN_CONFLICT,
    },
    SourceCase {
        id: "redeclaration.static-object-then-plain-conflict",
        source: STATIC_OBJECT_THEN_PLAIN,
    },
    SourceCase {
        id: "redeclaration.plain-object-then-static-conflict",
        source: PLAIN_OBJECT_THEN_STATIC,
    },
    SourceCase {
        id: "redeclaration.extern-object-then-static-conflict",
        source: EXTERN_OBJECT_THEN_STATIC,
    },
    SourceCase {
        id: "redeclaration.plain-function-then-static-conflict",
        source: PLAIN_FUNCTION_THEN_STATIC,
    },
    SourceCase {
        id: "redeclaration.thread-local-then-plain-conflict",
        source: THREAD_LOCAL_THEN_PLAIN,
    },
    SourceCase {
        id: "redeclaration.static-function-then-extern",
        source: STATIC_FUNCTION_THEN_EXTERN,
    },
    SourceCase {
        id: "parameter.top-qualified-compatible",
        source: TOP_QUALIFIED_PARAMETER,
    },
    SourceCase {
        id: "parameter.unspecified-then-char-conflict",
        source: UNSPECIFIED_THEN_CHAR,
    },
];

fn file() -> FileId {
    FileId::new(FILE_RAW)
}

fn span_of(source: &str, spelling: &str, occurrence: usize) -> SourceSpan {
    let (start, matched) = source
        .match_indices(spelling)
        .filter(|(start, matched)| {
            let start = *start;
            let matched = *matched;
            let before = source.as_bytes().get(start.wrapping_sub(1)).copied();
            let after = source.as_bytes().get(start + matched.len()).copied();
            !before.is_some_and(is_identifier_byte) && !after.is_some_and(is_identifier_byte)
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

fn function<'arena>(
    unit: &evocc_chir::CHirUnit<'arena>,
    index: usize,
) -> &'arena FunctionDecl<'arena> {
    match unit.declarations()[index] {
        CHirDecl::Function(declaration) => declaration,
        other => panic!("declaration {index} is not a function: {other:?}"),
    }
}

fn object<'arena>(unit: &evocc_chir::CHirUnit<'arena>, index: usize) -> &'arena ObjectDecl<'arena> {
    match unit.declarations()[index] {
        CHirDecl::Object(declaration) => declaration,
        other => panic!("declaration {index} is not an object: {other:?}"),
    }
}

fn typedef<'arena>(unit: &evocc_chir::CHirUnit<'arena>, index: usize) -> &'arena TypedefDecl {
    match unit.declarations()[index] {
        CHirDecl::Typedef(declaration) => declaration,
        other => panic!("declaration {index} is not a typedef: {other:?}"),
    }
}

fn tag<'arena>(unit: &evocc_chir::CHirUnit<'arena>, index: usize) -> &'arena TagDecl<'arena> {
    match unit.declarations()[index] {
        CHirDecl::Tag(declaration) => declaration,
        other => panic!("declaration {index} is not a tag: {other:?}"),
    }
}

fn declaration_symbol(declaration: &CHirDecl<'_>) -> SymbolId {
    match declaration {
        CHirDecl::Object(declaration) => declaration.symbol,
        CHirDecl::Function(declaration) => declaration.symbol,
        CHirDecl::Typedef(declaration) => declaration.symbol,
        CHirDecl::Tag(other) => panic!("tag declaration has no ordinary SymbolId: {other:?}"),
    }
}

fn body<'arena>(function: &FunctionDecl<'arena>) -> &'arena CHirStmt<'arena> {
    function.body.expect("function definition body")
}

fn compound_items<'unit, 'arena>(
    statement: &'unit CHirStmt<'arena>,
) -> &'unit [CHirBlockItem<'arena>] {
    match statement {
        CHirStmt::Compound(items, _) => items,
        other => panic!("expected compound statement, got {other:?}"),
    }
}

fn statement<'arena>(item: &CHirBlockItem<'arena>) -> &'arena CHirStmt<'arena> {
    match item {
        CHirBlockItem::Statement(statement) => statement,
        other => panic!("expected statement block item, got {other:?}"),
    }
}

fn declaration<'arena>(item: &CHirBlockItem<'arena>) -> &'arena CHirDecl<'arena> {
    match item {
        CHirBlockItem::Declaration(declaration) => declaration,
        other => panic!("expected declaration block item, got {other:?}"),
    }
}

fn assert_ident(
    expression: &CHirExpr<'_>,
    source: &str,
    occurrence: usize,
    expected_symbol: SymbolId,
    expected_type: TypeId,
    expected_category: ExprCategory,
    expected_modifiable: bool,
) {
    let CHirExpr::Ident {
        symbol,
        spelling,
        facts,
        span,
    } = expression
    else {
        panic!("expected resolved identifier, got {expression:?}");
    };
    assert_eq!(*symbol, expected_symbol, "identifier SymbolId join");
    assert_eq!(
        spelling,
        &source[span.byte_start as usize..span.byte_end as usize],
        "identifier spelling"
    );
    assert_eq!(*span, span_of(source, spelling, occurrence));
    assert_eq!(facts.ty(), expected_type, "identifier TypeId join");
    assert_eq!(facts.category(), expected_category, "identifier category");
    assert_eq!(
        facts.is_modifiable_lvalue(),
        expected_modifiable,
        "identifier modifiability"
    );
}

#[allow(clippy::too_many_arguments)]
fn assert_symbol(
    unit: &evocc_chir::CHirUnit<'_>,
    id: SymbolId,
    raw: u32,
    spelling: &str,
    scope: ScopeId,
    kind: SymbolKind,
    ty: TypeId,
    span: SourceSpan,
) {
    assert_eq!(id.raw(), raw, "source-order SymbolId for {spelling}");
    assert_symbol_join(unit, id, spelling, scope, kind, ty, span);
}

fn assert_symbol_join(
    unit: &evocc_chir::CHirUnit<'_>,
    id: SymbolId,
    spelling: &str,
    scope: ScopeId,
    kind: SymbolKind,
    ty: TypeId,
    span: SourceSpan,
) {
    let symbol = unit.symbols().symbol(id).expect("symbol owned by unit");
    assert_eq!(symbol.id(), id);
    assert_eq!(symbol.spelling(), spelling);
    assert_eq!(symbol.scope(), scope);
    assert_eq!(symbol.kind(), kind);
    assert_eq!(symbol.ty(), ty);
    assert_eq!(symbol.span(), span);
}

fn assert_unnamed_parameter(source: &str, expected: UnnamedParameterShape) {
    let arena = Arena::default();
    let unit = parse_valid(source, &arena);
    assert_eq!(unit.declarations().len(), 1);
    let function = function(&unit, 0);
    assert_eq!(
        function.span,
        SourceSpan::new(file(), 0, (source.len() - 1) as u32, 1, 1),
        "the complete declaration, including every abstract-declarator token, must be consumed"
    );
    let ParameterSyntax::Prototype {
        parameters,
        variadic,
    } = &function.parameters
    else {
        panic!("expected one-parameter prototype for {source:?}");
    };
    assert!(!variadic);
    assert_eq!(parameters.len(), 1);
    let parameter = &parameters[0];
    assert_eq!(parameter.symbol, None);
    assert_eq!(parameter.name, None);

    let TypeKind::Function {
        parameters:
            FunctionParameters::Prototype {
                types,
                variadic: canonical_variadic,
            },
        ..
    } = &unit.types().get(function.ty).unwrap().kind
    else {
        panic!("function declaration must retain a canonical prototype");
    };
    assert!(!canonical_variadic);
    assert_eq!(types.as_slice(), &[parameter.ty]);

    match expected {
        UnnamedParameterShape::Pointer => {
            assert_eq!(span_text(source, parameter.span), "int *");
            let [WrittenDeclaratorLayer::Pointer { qualifiers, span }] =
                parameter.written.layers.as_slice()
            else {
                panic!("unnamed pointer must retain exactly one written pointer layer");
            };
            assert_eq!(*qualifiers, Default::default());
            assert_eq!(span_text(source, *span), "*");
            let TypeKind::Pointer(pointee) = unit.types().get(parameter.ty).unwrap().kind else {
                panic!("unnamed pointer parameter must be canonical pointer type");
            };
            let pointee = unit.types().get(pointee).unwrap();
            assert_eq!(pointee.qualifiers, Default::default());
            assert_eq!(pointee.kind, TypeKind::Int(IntRank::Int));
        }
        UnnamedParameterShape::Array => {
            assert_eq!(span_text(source, parameter.span), "int [3]");
            let [WrittenDeclaratorLayer::Array { bound, span }] =
                parameter.written.layers.as_slice()
            else {
                panic!("unnamed array must retain exactly one written array layer");
            };
            assert_eq!(*bound, ArrayBound::Fixed(3));
            assert_eq!(span_text(source, *span), "[3]");
            let TypeKind::Pointer(pointee) = unit.types().get(parameter.ty).unwrap().kind else {
                panic!("array parameter must adjust to pointer type");
            };
            let pointee = unit.types().get(pointee).unwrap();
            assert_eq!(pointee.qualifiers, Default::default());
            assert_eq!(pointee.kind, TypeKind::Int(IntRank::Int));
        }
        UnnamedParameterShape::FunctionPointer => {
            assert_eq!(span_text(source, parameter.span), "int (*)(int)");
            let [WrittenDeclaratorLayer::Pointer {
                qualifiers,
                span: pointer_span,
            }, WrittenDeclaratorLayer::Function {
                span: function_span,
            }] = parameter.written.layers.as_slice()
            else {
                panic!("unnamed function pointer must retain pointer then function layers");
            };
            assert_eq!(*qualifiers, Default::default());
            assert_eq!(span_text(source, *pointer_span), "*");
            assert_eq!(span_text(source, *function_span), "(int)");
            let TypeKind::Pointer(pointee) = unit.types().get(parameter.ty).unwrap().kind else {
                panic!("function parameter must adjust to pointer-to-function");
            };
            let TypeKind::Function {
                return_type,
                parameters:
                    FunctionParameters::Prototype {
                        types,
                        variadic: inner_variadic,
                    },
            } = &unit.types().get(pointee).unwrap().kind
            else {
                panic!("unnamed parameter must point to a canonical function type");
            };
            assert!(!inner_variadic);
            assert_eq!(types.len(), 1);
            assert!(matches!(
                unit.types().get(*return_type).unwrap().kind,
                TypeKind::Int(IntRank::Int)
            ));
            assert!(matches!(
                unit.types().get(types[0]).unwrap().kind,
                TypeKind::Int(IntRank::Int)
            ));
        }
    }
    assert_eq!(unit.symbols().symbols_in_source_order().len(), 1);
}

#[test]
fn source_matrix_is_forty_five_and_byte_unique() {
    assert_eq!(SOURCES.len(), 45, "S1 structural denominator changed");
    let mut ids = HashSet::new();
    let mut bytes = HashSet::new();
    for case in SOURCES {
        assert!(ids.insert(case.id), "duplicate case id {}", case.id);
        assert!(
            bytes.insert(case.source.as_bytes()),
            "duplicate source bytes for {}",
            case.id
        );
        assert!(case.source.ends_with('\n'), "{} lacks newline", case.id);
    }
}

#[test]
fn file_declarations_join_scope_zero_in_exact_source_order() {
    let arena = Arena::default();
    let unit = parse_valid(FILE_BINDINGS, &arena);
    let symbols = unit.symbols();
    let file_scope = symbols.file_scope();
    assert_eq!(file_scope.raw(), 0);
    assert_eq!(symbols.scopes_in_source_order().len(), 1);
    assert_eq!(symbols.scope(file_scope).unwrap().kind(), ScopeKind::File);

    let first = object(&unit, 0);
    let second = object(&unit, 1);
    assert_symbol(
        &unit,
        first.symbol,
        0,
        "first",
        file_scope,
        SymbolKind::Object,
        first.ty,
        span_of(FILE_BINDINGS, "first", 0),
    );
    assert_symbol(
        &unit,
        second.symbol,
        1,
        "second",
        file_scope,
        SymbolKind::Object,
        second.ty,
        span_of(FILE_BINDINGS, "second", 0),
    );
    assert_eq!(
        symbols.scope_symbols(file_scope).unwrap(),
        &[first.symbol, second.symbol]
    );
}

#[test]
fn parameters_and_outer_body_declarations_share_function_scope() {
    let arena = Arena::default();
    let unit = parse_valid(FUNCTION_SCOPE, &arena);
    let function = function(&unit, 0);
    let ParameterSyntax::Prototype { parameters, .. } = &function.parameters else {
        panic!("expected prototype parameters");
    };
    let parameter = &parameters[0];
    let parameter_symbol = parameter.symbol.expect("named parameter SymbolId");
    let items = compound_items(body(function));
    let CHirDecl::Object(local) = declaration(&items[0]) else {
        panic!("expected outer-body local");
    };

    let symbols = unit.symbols();
    let file_scope = symbols.file_scope();
    let function_scope = symbols.symbol(parameter_symbol).unwrap().scope();
    assert_eq!((file_scope.raw(), function_scope.raw()), (0, 1));
    assert_eq!(
        symbols.scope(function_scope).unwrap().kind(),
        ScopeKind::Function
    );
    assert_eq!(
        symbols.scope(function_scope).unwrap().parent(),
        Some(file_scope)
    );
    assert_eq!(
        symbols.symbol(local.symbol).unwrap().scope(),
        function_scope
    );
    assert_eq!(
        symbols.scope_symbols(function_scope).unwrap(),
        &[parameter_symbol, local.symbol]
    );
    assert_symbol(
        &unit,
        function.symbol,
        0,
        "f",
        file_scope,
        SymbolKind::Function,
        function.ty,
        span_of(FUNCTION_SCOPE, "f", 0),
    );
    assert_symbol(
        &unit,
        parameter_symbol,
        1,
        "p",
        function_scope,
        SymbolKind::Parameter,
        parameter.ty,
        span_of(FUNCTION_SCOPE, "p", 0),
    );
    assert_symbol(
        &unit,
        local.symbol,
        2,
        "local",
        function_scope,
        SymbolKind::Object,
        local.ty,
        span_of(FUNCTION_SCOPE, "local", 0),
    );

    let CHirStmt::Return(Some(value), _) = statement(&items[1]) else {
        panic!("expected return expression");
    };
    let CHirExpr::Binary(_, left, right, _) = value else {
        panic!("expected p + local");
    };
    assert_ident(
        left,
        FUNCTION_SCOPE,
        1,
        parameter_symbol,
        parameter.ty,
        ExprCategory::LValue,
        true,
    );
    assert_ident(
        right,
        FUNCTION_SCOPE,
        1,
        local.symbol,
        local.ty,
        ExprCategory::LValue,
        true,
    );
}

#[test]
fn nested_block_shadowing_uses_distinct_ids_and_nearest_resolution() {
    let arena = Arena::default();
    let unit = parse_valid(NESTED_SHADOW, &arena);
    let function = function(&unit, 0);
    let ParameterSyntax::Prototype { parameters, .. } = &function.parameters else {
        panic!("expected prototype parameters");
    };
    let outer = parameters[0].symbol.expect("parameter symbol");
    let outer_ty = parameters[0].ty;
    let outer_items = compound_items(body(function));
    let nested_items = compound_items(statement(&outer_items[0]));
    let CHirDecl::Object(inner_decl) = declaration(&nested_items[0]) else {
        panic!("expected shadow declaration");
    };
    let inner = inner_decl.symbol;

    let symbols = unit.symbols();
    let function_scope = symbols.symbol(outer).unwrap().scope();
    let nested_scope = symbols.symbol(inner).unwrap().scope();
    assert_eq!((function.symbol.raw(), outer.raw(), inner.raw()), (0, 1, 2));
    assert_ne!(outer, inner);
    assert_eq!(
        symbols.scope(nested_scope).unwrap().kind(),
        ScopeKind::Block
    );
    assert_eq!(
        symbols.scope(nested_scope).unwrap().parent(),
        Some(function_scope)
    );
    assert_eq!(symbols.resolve(nested_scope, "x").unwrap(), Some(inner));
    assert_eq!(symbols.resolve(function_scope, "x").unwrap(), Some(outer));
    assert_eq!(
        symbols
            .symbols_in_source_order()
            .iter()
            .map(|symbol| (symbol.id().raw(), symbol.spelling()))
            .collect::<Vec<_>>(),
        [(0, "f"), (1, "x"), (2, "x")]
    );

    let CHirStmt::Expr(assignment, _) = statement(&nested_items[1]) else {
        panic!("expected nested assignment");
    };
    let CHirExpr::Assign { target: left, .. } = assignment else {
        panic!("expected x = 1");
    };
    assert_ident(
        left,
        NESTED_SHADOW,
        2,
        inner,
        inner_decl.ty,
        ExprCategory::LValue,
        true,
    );
    let CHirStmt::Return(Some(value), _) = statement(&outer_items[1]) else {
        panic!("expected outer return");
    };
    assert_ident(
        value,
        NESTED_SHADOW,
        3,
        outer,
        outer_ty,
        ExprCategory::LValue,
        true,
    );
}

#[test]
fn for_declaration_gets_one_dedicated_child_scope() {
    let arena = Arena::default();
    let unit = parse_valid(FOR_SCOPE, &arena);
    let function = function(&unit, 0);
    let items = compound_items(body(function));
    let CHirStmt::For(initializer, condition, step, _, _) = statement(&items[0]) else {
        panic!("expected for statement");
    };
    let CHirForInit::Declaration(declarations) = initializer else {
        panic!("expected declaration initializer");
    };
    let CHirDecl::Object(induction) = declarations[0] else {
        panic!("expected induction object");
    };
    let symbols = unit.symbols();
    let for_scope = symbols.symbol(induction.symbol).unwrap().scope();
    assert_eq!((function.symbol.raw(), induction.symbol.raw()), (0, 1));
    assert_eq!(for_scope.raw(), 2);
    assert_eq!(symbols.scope(for_scope).unwrap().kind(), ScopeKind::For);
    assert_eq!(symbols.scope(for_scope).unwrap().parent().unwrap().raw(), 1);
    assert_eq!(
        symbols.scope_symbols(for_scope).unwrap(),
        &[induction.symbol]
    );

    let CHirExpr::Binary(_, condition_i, _, _) = condition.expect("for condition") else {
        panic!("expected i < 1");
    };
    assert_ident(
        condition_i,
        FOR_SCOPE,
        1,
        induction.symbol,
        induction.ty,
        ExprCategory::LValue,
        true,
    );
    let CHirExpr::Assign {
        target: step_i,
        value,
        ..
    } = step.expect("for step")
    else {
        panic!("expected i = i + 1");
    };
    assert_ident(
        step_i,
        FOR_SCOPE,
        2,
        induction.symbol,
        induction.ty,
        ExprCategory::LValue,
        true,
    );
    let CHirExpr::Binary(_, value_i, _, _) = value else {
        panic!("expected i + 1");
    };
    assert_ident(
        value_i,
        FOR_SCOPE,
        3,
        induction.symbol,
        induction.ty,
        ExprCategory::LValue,
        true,
    );
}

#[test]
fn enumerator_declarations_and_uses_join_exact_symbols() {
    let arena = Arena::default();
    let unit = parse_valid(ENUM_BINDINGS, &arena);
    let tag = tag(&unit, 0);
    let TagMembers::Enum(enumerators) = &tag.members else {
        panic!("expected enum members");
    };
    let value = object(&unit, 1);
    let a = enumerators[0].symbol;
    let b = enumerators[1].symbol;
    let symbols = unit.symbols();
    let file_scope = symbols.file_scope();
    assert_eq!(tag.scope, file_scope);
    assert_eq!((a.raw(), b.raw(), value.symbol.raw()), (0, 1, 2));
    assert_symbol(
        &unit,
        a,
        0,
        "A",
        file_scope,
        SymbolKind::Enumerator,
        symbols.symbol(a).unwrap().ty(),
        span_of(ENUM_BINDINGS, "A", 0),
    );
    assert_symbol(
        &unit,
        b,
        1,
        "B",
        file_scope,
        SymbolKind::Enumerator,
        symbols.symbol(b).unwrap().ty(),
        span_of(ENUM_BINDINGS, "B", 0),
    );
    assert_eq!(symbols.symbol(a).unwrap().enumerator_value(), Some(3));
    assert_eq!(symbols.symbol(b).unwrap().enumerator_value(), Some(4));

    let CHirExpr::Binary(_, written_a, _, _) =
        enumerators[1].written_value.expect("B written value")
    else {
        panic!("expected A + 1");
    };
    assert_ident(
        written_a,
        ENUM_BINDINGS,
        1,
        a,
        symbols.symbol(a).unwrap().ty(),
        ExprCategory::Value,
        false,
    );
    assert_ident(
        value.initializer.expect("value initializer"),
        ENUM_BINDINGS,
        1,
        b,
        symbols.symbol(b).unwrap().ty(),
        ExprCategory::Value,
        false,
    );
}

#[test]
fn typedef_declaration_gets_a_stable_ordinary_symbol() {
    let arena = Arena::default();
    let unit = parse_valid(TYPEDEF_BINDINGS, &arena);
    let alias = typedef(&unit, 0);
    let file_scope = unit.symbols().file_scope();
    assert_eq!(unit.declarations().len(), 1);
    assert_symbol(
        &unit,
        alias.symbol,
        0,
        "T",
        file_scope,
        SymbolKind::Typedef,
        alias.ty,
        span_of(TYPEDEF_BINDINGS, "T", 0),
    );
    assert_eq!(
        unit.symbols().scope_symbols(file_scope).unwrap(),
        &[alias.symbol]
    );
}

#[test]
fn valid_identifier_nodes_carry_exact_type_category_and_modifiability() {
    let arena = Arena::default();
    let unit = parse_valid(IDENT_FACTS, &arena);
    let callee = function(&unit, 0);
    let mutable = object(&unit, 1);
    let fixed = object(&unit, 2);
    let use_function = function(&unit, 3);
    let ParameterSyntax::Prototype { parameters, .. } = &use_function.parameters else {
        panic!("expected use prototype");
    };
    let parameter = &parameters[0];
    let parameter_symbol = parameter.symbol.expect("parameter symbol");
    assert_eq!(
        (
            callee.symbol.raw(),
            mutable.symbol.raw(),
            fixed.symbol.raw(),
            use_function.symbol.raw(),
            parameter_symbol.raw(),
        ),
        (0, 1, 2, 3, 4)
    );

    let items = compound_items(body(use_function));
    let CHirStmt::Return(Some(sum), _) = statement(&items[0]) else {
        panic!("expected return sum");
    };
    let CHirExpr::Binary(_, left_three, parameter_use, _) = sum else {
        panic!("expected final + parameter");
    };
    let CHirExpr::Binary(_, left_two, fixed_use, _) = left_three else {
        panic!("expected + fixed");
    };
    let CHirExpr::Binary(_, call, mutable_use, _) = left_two else {
        panic!("expected call + mutable");
    };
    let CHirExpr::Call(callee_use, _, _) = call else {
        panic!("expected callee call");
    };
    assert_ident(
        callee_use,
        IDENT_FACTS,
        1,
        callee.symbol,
        callee.ty,
        ExprCategory::FunctionDesignator,
        false,
    );
    assert_ident(
        mutable_use,
        IDENT_FACTS,
        1,
        mutable.symbol,
        mutable.ty,
        ExprCategory::LValue,
        true,
    );
    assert_ident(
        fixed_use,
        IDENT_FACTS,
        1,
        fixed.symbol,
        fixed.ty,
        ExprCategory::LValue,
        false,
    );
    assert_ident(
        parameter_use,
        IDENT_FACTS,
        1,
        parameter_symbol,
        parameter.ty,
        ExprCategory::LValue,
        true,
    );
}

#[test]
fn block_enumerator_leak_is_e2001_without_parser_panic() {
    let arena = Arena::default();
    let (unit, diagnostic) = parse_without_panic(ENUM_LEAK, &arena);
    let diagnostic = diagnostic.expect("out-of-scope enumerator diagnostic");
    assert_eq!(diagnostic.code, "E2001");
    assert_eq!(diagnostic.message, "undeclared-identifier: A");
    assert_eq!(diagnostic.span, span_of(ENUM_LEAK, "A", 1));

    let symbols = unit.symbols();
    assert_eq!(symbols.scopes_in_source_order().len(), 3);
    let function_scope = symbols.scopes_in_source_order()[1].id();
    let block_scope = symbols.scopes_in_source_order()[2].id();
    assert_eq!(
        symbols.scope(function_scope).unwrap().kind(),
        ScopeKind::Function
    );
    assert_eq!(symbols.scope(block_scope).unwrap().kind(), ScopeKind::Block);
    assert_eq!(symbols.resolve(function_scope, "A").unwrap(), None);
    let a = symbols.resolve(block_scope, "A").unwrap().expect("inner A");
    assert_eq!(a.raw(), 1);
    assert_eq!(symbols.symbol(a).unwrap().kind(), SymbolKind::Enumerator);

    let function = function(&unit, 0);
    let outer_items = compound_items(body(function));
    let nested_items = compound_items(statement(&outer_items[0]));
    let CHirDecl::Tag(inner_tag) = declaration(&nested_items[0]) else {
        panic!("expected inner anonymous enum");
    };
    assert_eq!(inner_tag.scope, block_scope);
    let TagMembers::Enum(enumerators) = &inner_tag.members else {
        panic!("expected enumerator");
    };
    assert_eq!(enumerators[0].symbol, a);
}

#[test]
fn duplicate_e2002_rolls_back_without_a_ghost_symbol() {
    let arena = Arena::default();
    let (unit, diagnostic) = parse_without_panic(DUPLICATE_ROLLBACK, &arena);
    let diagnostic = diagnostic.expect("duplicate declaration diagnostic");
    assert_eq!(diagnostic.code, "E2002");
    assert_eq!(diagnostic.message, "ordinary-identifier-redefinition: same");
    assert_eq!(diagnostic.span, span_of(DUPLICATE_ROLLBACK, "same", 1));

    let symbols = unit.symbols();
    assert_eq!(
        unit.declarations().len(),
        2,
        "duplicate CHIR must roll back"
    );
    assert_eq!(symbols.symbols_in_source_order().len(), 2);
    assert_eq!(
        symbols
            .symbols_in_source_order()
            .iter()
            .map(|symbol| (symbol.id().raw(), symbol.spelling()))
            .collect::<Vec<_>>(),
        [(0, "same"), (1, "after")]
    );
    let same = object(&unit, 0);
    let after = object(&unit, 1);
    assert_eq!((same.symbol.raw(), after.symbol.raw()), (0, 1));
    assert_eq!(
        symbols.resolve(symbols.file_scope(), "same").unwrap(),
        Some(same.symbol)
    );
    assert_eq!(
        symbols.resolve(symbols.file_scope(), "after").unwrap(),
        Some(after.symbol)
    );
}

#[test]
fn tag_scope_is_explicit_and_tag_namespace_stays_independent() {
    let arena = Arena::default();
    let unit = parse_valid(TAG_NAMESPACE, &arena);
    let tag = tag(&unit, 0);
    let object = object(&unit, 1);
    let symbols = unit.symbols();
    let file_scope = symbols.file_scope();
    assert_eq!(tag.scope, file_scope);
    assert_eq!(object.symbol.raw(), 0);
    assert_eq!(symbols.symbol(object.symbol).unwrap().spelling(), "Node");
    assert_eq!(
        symbols.resolve(file_scope, "Node").unwrap(),
        Some(object.symbol)
    );
    assert_eq!(
        symbols.resolve_tag(file_scope, "Node").unwrap(),
        Some(tag.tag)
    );
    assert_eq!(symbols.tag_len(), 1);
    assert_eq!(symbols.tags_in_source_order()[0].scope(), file_scope);
    assert_eq!(symbols.tags_in_source_order()[0].tag(), tag.tag);
    assert_eq!(symbols.tags_in_source_order()[0].spelling(), "Node");
}

#[test]
fn prototype_enum_bindings_stay_in_function_prototype_scope() {
    let arena = Arena::default();
    let (unit, diagnostic) = parse_without_panic(PROTOTYPE_ENUM_LEAK, &arena);
    let diagnostic = diagnostic.expect("prototype enumerator leak diagnostic");
    assert_eq!(diagnostic.code, "E2001");
    assert_eq!(diagnostic.message, "undeclared-identifier: A");
    assert_eq!(diagnostic.span, span_of(PROTOTYPE_ENUM_LEAK, "A", 1));
    assert_eq!(
        unit.declarations().len(),
        2,
        "failed leaked object must leave no CHIR declaration"
    );

    let enum_decl = tag(&unit, 0);
    let function = function(&unit, 1);
    assert!(function.body.is_none(), "prototype unexpectedly has a body");
    let ParameterSyntax::Prototype { parameters, .. } = &function.parameters else {
        panic!("expected prototype parameter syntax");
    };
    let parameter = &parameters[0];
    let x = parameter.symbol.expect("named prototype parameter symbol");
    let TagMembers::Enum(enumerators) = &enum_decl.members else {
        panic!("expected inline enum definition");
    };
    let a = enumerators[0].symbol;

    let symbols = unit.symbols();
    let file_scope = symbols.file_scope();
    assert_eq!(symbols.scopes_in_source_order().len(), 2);
    let prototype_scope = symbols.scopes_in_source_order()[1].id();
    assert_eq!(prototype_scope.raw(), 1);
    assert_eq!(
        symbols.scope(prototype_scope).unwrap().kind(),
        ScopeKind::FunctionPrototype
    );
    assert_eq!(
        symbols.scope(prototype_scope).unwrap().parent(),
        Some(file_scope)
    );
    assert_eq!(enum_decl.scope, prototype_scope);
    assert_ne!(function.symbol, a);
    assert_ne!(function.symbol, x);
    assert_ne!(a, x);
    assert_symbol_join(
        &unit,
        function.symbol,
        "f",
        file_scope,
        SymbolKind::Function,
        function.ty,
        span_of(PROTOTYPE_ENUM_LEAK, "f", 0),
    );
    assert_symbol_join(
        &unit,
        a,
        "A",
        prototype_scope,
        SymbolKind::Enumerator,
        unit.symbols().symbol(a).unwrap().ty(),
        span_of(PROTOTYPE_ENUM_LEAK, "A", 0),
    );
    assert_symbol_join(
        &unit,
        x,
        "x",
        prototype_scope,
        SymbolKind::Parameter,
        parameter.ty,
        span_of(PROTOTYPE_ENUM_LEAK, "x", 0),
    );
    assert_eq!(
        symbols.scope_symbols(file_scope).unwrap(),
        &[function.symbol]
    );
    assert_eq!(symbols.scope_symbols(prototype_scope).unwrap(), &[a, x]);
    assert_eq!(symbols.scope_tags(file_scope).unwrap(), &[]);
    assert_eq!(
        symbols.scope_tags(prototype_scope).unwrap(),
        &[enum_decl.tag]
    );
    assert_eq!(symbols.resolve(file_scope, "A").unwrap(), None);
    assert_eq!(symbols.resolve(file_scope, "x").unwrap(), None);
    assert_eq!(symbols.resolve(file_scope, "leaked").unwrap(), None);
    assert_eq!(symbols.resolve_tag(file_scope, "E").unwrap(), None);
    assert_eq!(symbols.resolve(prototype_scope, "A").unwrap(), Some(a));
    assert_eq!(
        symbols.resolve_tag(prototype_scope, "E").unwrap(),
        Some(enum_decl.tag)
    );
}

#[test]
fn definition_parameter_enum_shares_function_scope_with_body_use() {
    let arena = Arena::default();
    let unit = parse_valid(DEFINITION_ENUM_SCOPE, &arena);
    assert_eq!(unit.declarations().len(), 2);
    let enum_decl = tag(&unit, 0);
    let function = function(&unit, 1);
    let ParameterSyntax::Prototype { parameters, .. } = &function.parameters else {
        panic!("expected definition parameter syntax");
    };
    let parameter = &parameters[0];
    let x = parameter.symbol.expect("named definition parameter symbol");
    let TagMembers::Enum(enumerators) = &enum_decl.members else {
        panic!("expected inline enum definition");
    };
    let a = enumerators[0].symbol;

    let symbols = unit.symbols();
    let file_scope = symbols.file_scope();
    assert_eq!(symbols.scopes_in_source_order().len(), 2);
    let function_scope = symbols.scopes_in_source_order()[1].id();
    assert_eq!(function_scope.raw(), 1);
    assert_eq!(
        symbols.scope(function_scope).unwrap().kind(),
        ScopeKind::Function
    );
    assert_eq!(enum_decl.scope, function_scope);
    assert_ne!(function.symbol, a);
    assert_ne!(function.symbol, x);
    assert_ne!(a, x);
    assert_eq!(
        symbols.scope_symbols(file_scope).unwrap(),
        &[function.symbol]
    );
    assert_eq!(symbols.scope_symbols(function_scope).unwrap(), &[a, x]);
    assert_eq!(symbols.resolve(file_scope, "A").unwrap(), None);
    assert_eq!(symbols.resolve(file_scope, "x").unwrap(), None);
    assert_eq!(symbols.resolve_tag(file_scope, "E").unwrap(), None);
    assert_eq!(symbols.resolve(function_scope, "A").unwrap(), Some(a));
    assert_eq!(
        symbols.resolve_tag(function_scope, "E").unwrap(),
        Some(enum_decl.tag)
    );

    let items = compound_items(body(function));
    let CHirStmt::Return(Some(value), _) = statement(&items[0]) else {
        panic!("expected body return of A");
    };
    assert_ident(
        value,
        DEFINITION_ENUM_SCOPE,
        1,
        a,
        symbols.symbol(a).unwrap().ty(),
        ExprCategory::Value,
        false,
    );
}

#[test]
fn repeated_file_forward_declarations_emit_two_events_for_one_tag() {
    let arena = Arena::default();
    let unit = parse_valid(REPEATED_FORWARD, &arena);
    assert_eq!(unit.declarations().len(), 2);
    let first = tag(&unit, 0);
    let second = tag(&unit, 1);
    let symbols = unit.symbols();
    let file_scope = symbols.file_scope();
    assert_eq!(first.scope, file_scope);
    assert_eq!(second.scope, file_scope);
    assert_eq!(first.tag, second.tag);
    assert_eq!(first.tag.raw(), 0);
    assert!(matches!(first.members, TagMembers::Incomplete));
    assert!(matches!(second.members, TagMembers::Incomplete));
    assert_eq!(
        first.name.as_ref().expect("first tag name").span,
        span_of(REPEATED_FORWARD, "S", 0)
    );
    assert_eq!(
        second.name.as_ref().expect("second tag name").span,
        span_of(REPEATED_FORWARD, "S", 1)
    );
    assert_eq!(symbols.tag_len(), 1, "one binding, two declaration events");
    assert_eq!(symbols.scope_tags(file_scope).unwrap(), &[first.tag]);
    assert_eq!(
        symbols.resolve_tag(file_scope, "S").unwrap(),
        Some(first.tag)
    );
}

#[test]
fn inner_pure_forward_hides_outer_and_pointer_uses_inner_tag() {
    let arena = Arena::default();
    let unit = parse_valid(INNER_FORWARD, &arena);
    assert_eq!(unit.declarations().len(), 2);
    let outer = tag(&unit, 0);
    let function = function(&unit, 1);
    let items = compound_items(body(function));
    let CHirDecl::Tag(inner) = declaration(&items[0]) else {
        panic!("expected inner pure forward declaration");
    };
    let CHirDecl::Object(pointer) = declaration(&items[1]) else {
        panic!("expected pointer object after inner forward");
    };

    let symbols = unit.symbols();
    let file_scope = symbols.file_scope();
    let function_scope = symbols.scopes_in_source_order()[1].id();
    assert_eq!(
        symbols.scope(function_scope).unwrap().kind(),
        ScopeKind::Function
    );
    assert_eq!(outer.scope, file_scope);
    assert_eq!(inner.scope, function_scope);
    assert_eq!((outer.tag.raw(), inner.tag.raw()), (0, 1));
    assert_ne!(outer.tag, inner.tag);
    assert_eq!(
        symbols.resolve_tag(file_scope, "S").unwrap(),
        Some(outer.tag)
    );
    assert_eq!(
        symbols.resolve_tag(function_scope, "S").unwrap(),
        Some(inner.tag)
    );
    assert_eq!(symbols.scope_tags(file_scope).unwrap(), &[outer.tag]);
    assert_eq!(symbols.scope_tags(function_scope).unwrap(), &[inner.tag]);
    assert_eq!(symbols.tags_in_source_order().len(), 2);
    assert_eq!(
        symbols.symbol(pointer.symbol).unwrap().scope(),
        function_scope
    );

    let TypeSpecifier::Tag {
        tag: pointer_tag,
        name: Some(pointer_tag_name),
        ..
    } = &pointer.specifier
    else {
        panic!("pointer declaration must retain written struct S specifier");
    };
    assert_eq!(*pointer_tag, inner.tag);
    assert_eq!(
        pointer_tag_name.span,
        span_of(INNER_FORWARD, "S", 2),
        "second inner reference must name the hidden tag"
    );
    let TypeKind::Pointer(pointee) = &unit.types().get(pointer.ty).unwrap().kind else {
        panic!("p must have pointer type");
    };
    let TypeKind::Struct(pointee_tag) = &unit.types().get(*pointee).unwrap().kind else {
        panic!("p must point to struct S");
    };
    assert_eq!(*pointee_tag, inner.tag);
}

#[test]
fn compatible_redeclarations_merge_one_binding_and_the_required_composite_type() {
    let arena = Arena::default();
    let unit = parse_valid(ARRAY_COMPOSITE, &arena);
    let incomplete = object(&unit, 0);
    let complete = object(&unit, 1);
    assert_eq!(incomplete.symbol, complete.symbol);
    assert_eq!(incomplete.storage, StorageClass::Extern);
    assert_eq!(complete.storage, StorageClass::None);
    assert!(matches!(
        unit.types().get(incomplete.ty).unwrap().kind,
        TypeKind::Array {
            bound: ArrayBound::Unbounded,
            ..
        }
    ));
    assert!(matches!(
        unit.types().get(complete.ty).unwrap().kind,
        TypeKind::Array {
            bound: ArrayBound::Fixed(3),
            ..
        }
    ));
    let binding = unit.symbols().symbol(incomplete.symbol).unwrap();
    assert_eq!(
        binding.ty(),
        complete.ty,
        "binding must expose array[3] composite"
    );
    assert_eq!(binding.storage_class(), Some(StorageClass::Extern));
    assert_eq!(
        unit.symbols()
            .scope_symbols(unit.symbols().file_scope())
            .unwrap(),
        &[incomplete.symbol]
    );

    let arena = Arena::default();
    let unit = parse_valid(FUNCTION_COMPOSITE, &arena);
    let unspecified = function(&unit, 0);
    let prototype = function(&unit, 1);
    assert_eq!(unspecified.symbol, prototype.symbol);
    assert!(matches!(
        unit.types().get(unspecified.ty).unwrap().kind,
        TypeKind::Function {
            parameters: FunctionParameters::Unspecified,
            ..
        }
    ));
    let TypeKind::Function {
        parameters: FunctionParameters::Prototype { types, variadic },
        ..
    } = &unit.types().get(prototype.ty).unwrap().kind
    else {
        panic!("second declaration must retain its written prototype");
    };
    assert_eq!(types.len(), 1);
    assert!(!variadic);
    assert!(matches!(
        unit.types().get(types[0]).unwrap().kind,
        TypeKind::Int(IntRank::Int)
    ));
    assert_eq!(
        unit.symbols().symbol(prototype.symbol).unwrap().ty(),
        prototype.ty,
        "binding must expose the prototype composite"
    );
    assert_eq!(
        unit.symbols()
            .scope_symbols(unit.symbols().file_scope())
            .unwrap(),
        &[prototype.symbol]
    );
}

#[test]
fn compatible_storage_sequences_preserve_written_storage_and_effective_linkage() {
    let cases = [
        (
            EXTERN_THEN_TENTATIVE,
            StorageClass::Extern,
            StorageClass::None,
            StorageClass::Extern,
        ),
        (
            STATIC_THEN_EXTERN,
            StorageClass::Static,
            StorageClass::Extern,
            StorageClass::Static,
        ),
    ];
    assert_eq!(cases.len(), 2);
    for (source, first_storage, second_storage, binding_storage) in cases {
        let arena = Arena::default();
        let unit = parse_valid(source, &arena);
        let first = object(&unit, 0);
        let second = object(&unit, 1);
        assert_eq!(first.symbol, second.symbol, "{source:?}");
        assert_eq!(first.storage, first_storage, "{source:?}");
        assert_eq!(second.storage, second_storage, "{source:?}");
        let binding = unit.symbols().symbol(first.symbol).unwrap();
        assert_eq!(binding.storage_class(), Some(binding_storage), "{source:?}");
        assert_eq!(binding.ty(), second.ty, "{source:?}");
        assert_eq!(
            unit.symbols()
                .scope_symbols(unit.symbols().file_scope())
                .unwrap(),
            &[first.symbol],
            "{source:?}"
        );
    }

    let function_cases = [
        (STATIC_FUNCTION_THEN_NONE, StorageClass::None),
        (STATIC_FUNCTION_THEN_EXTERN, StorageClass::Extern),
    ];
    assert_eq!(function_cases.len(), 2);
    for (source, second_storage) in function_cases {
        let arena = Arena::default();
        let unit = parse_valid(source, &arena);
        let first = function(&unit, 0);
        let second = function(&unit, 1);
        assert_eq!(first.symbol, second.symbol, "{source:?}");
        assert_eq!(first.storage, StorageClass::Static, "{source:?}");
        assert_eq!(second.storage, second_storage, "{source:?}");
        let binding = unit.symbols().symbol(first.symbol).unwrap();
        assert_eq!(binding.kind(), SymbolKind::Function, "{source:?}");
        assert_eq!(
            binding.storage_class(),
            Some(StorageClass::Static),
            "{source:?}"
        );
        assert_eq!(binding.ty(), second.ty, "{source:?}");
        assert_eq!(
            unit.symbols()
                .scope_symbols(unit.symbols().file_scope())
                .unwrap(),
            &[first.symbol],
            "{source:?}"
        );
    }
}

#[test]
fn incompatible_storage_and_linkage_sequences_are_exact_e2002() {
    let cases = [
        (
            STATIC_OBJECT_THEN_PLAIN,
            "x",
            SymbolKind::Object,
            StorageClass::Static,
            1,
        ),
        (
            PLAIN_OBJECT_THEN_STATIC,
            "x",
            SymbolKind::Object,
            StorageClass::None,
            1,
        ),
        (
            EXTERN_OBJECT_THEN_STATIC,
            "x",
            SymbolKind::Object,
            StorageClass::Extern,
            1,
        ),
        (
            PLAIN_FUNCTION_THEN_STATIC,
            "f",
            SymbolKind::Function,
            StorageClass::None,
            2,
        ),
        (
            THREAD_LOCAL_THEN_PLAIN,
            "x",
            SymbolKind::Object,
            StorageClass::ThreadLocal,
            1,
        ),
    ];
    assert_eq!(cases.len(), 5);
    for (source, name, kind, storage, scope_count) in cases {
        let arena = Arena::default();
        let (unit, diagnostic) = parse_without_panic(source, &arena);
        let diagnostic = diagnostic.expect("storage/linkage conflict must diagnose");
        assert_eq!(diagnostic.code, "E2002", "{source:?}");
        assert_eq!(
            diagnostic.message,
            format!("ordinary-identifier-redefinition: {name}"),
            "{source:?}"
        );
        assert_eq!(diagnostic.span, span_of(source, name, 1), "{source:?}");
        assert_eq!(
            unit.declarations().len(),
            1,
            "failed redeclaration must not emit CHIR: {source:?}"
        );
        let first = declaration_symbol(unit.declarations()[0]);
        assert_eq!(first.raw(), 0, "{source:?}");
        let binding = unit.symbols().symbol(first).unwrap();
        assert_eq!(binding.kind(), kind, "{source:?}");
        assert_eq!(binding.storage_class(), Some(storage), "{source:?}");
        assert_eq!(unit.symbols().symbols_in_source_order().len(), 1);
        assert_eq!(
            unit.symbols().scopes_in_source_order().len(),
            scope_count,
            "failed declarator scope must roll back: {source:?}"
        );
    }
}

#[test]
fn repeated_tentatives_then_one_definition_and_later_extern_share_one_binding() {
    let arena = Arena::default();
    let unit = parse_valid(TENTATIVE_THEN_DEFINITION, &arena);
    assert_eq!(unit.declarations().len(), 4);
    let first = object(&unit, 0);
    let repeated = object(&unit, 1);
    let definition = object(&unit, 2);
    let later_extern = object(&unit, 3);
    assert_eq!(
        [
            first.symbol,
            repeated.symbol,
            definition.symbol,
            later_extern.symbol,
        ],
        [first.symbol; 4]
    );
    assert!(first.initializer.is_none());
    assert!(repeated.initializer.is_none());
    assert!(definition.initializer.is_some());
    assert!(later_extern.initializer.is_none());
    assert_eq!(first.storage, StorageClass::None);
    assert_eq!(repeated.storage, StorageClass::None);
    assert_eq!(definition.storage, StorageClass::None);
    assert_eq!(later_extern.storage, StorageClass::Extern);
    assert_eq!(
        unit.symbols()
            .scope_symbols(unit.symbols().file_scope())
            .unwrap(),
        &[first.symbol]
    );
    assert_eq!(unit.symbols().symbol(first.symbol).unwrap().ty(), first.ty);
}

#[test]
fn repeated_block_extern_and_exact_compatible_typedef_reuse_one_symbol() {
    let arena = Arena::default();
    let unit = parse_valid(REPEATED_BLOCK_EXTERN, &arena);
    let function = function(&unit, 0);
    let items = compound_items(body(function));
    let CHirDecl::Object(first) = declaration(&items[0]) else {
        panic!("expected first block extern");
    };
    let CHirDecl::Object(second) = declaration(&items[1]) else {
        panic!("expected repeated block extern");
    };
    assert_eq!(first.storage, StorageClass::Extern);
    assert_eq!(second.storage, StorageClass::Extern);
    assert_eq!(first.symbol, second.symbol);
    let function_scope = unit.symbols().symbol(first.symbol).unwrap().scope();
    assert_eq!(
        unit.symbols().scope(function_scope).unwrap().kind(),
        ScopeKind::Function
    );
    assert_eq!(
        unit.symbols().scope_symbols(function_scope).unwrap(),
        &[first.symbol]
    );
    let CHirStmt::Return(Some(value), _) = statement(&items[2]) else {
        panic!("expected return of repeated block extern");
    };
    assert_ident(
        value,
        REPEATED_BLOCK_EXTERN,
        2,
        first.symbol,
        first.ty,
        ExprCategory::LValue,
        true,
    );

    let arena = Arena::default();
    let unit = parse_valid(COMPATIBLE_TYPEDEF, &arena);
    let first = typedef(&unit, 0);
    let second = typedef(&unit, 1);
    assert_eq!(first.symbol, second.symbol);
    assert_eq!(first.ty, second.ty);
    assert_eq!(
        unit.symbols().symbol(first.symbol).unwrap().kind(),
        SymbolKind::Typedef
    );
    assert_eq!(
        unit.symbols()
            .scope_symbols(unit.symbols().file_scope())
            .unwrap(),
        &[first.symbol]
    );
}

#[test]
fn sibling_block_externs_and_later_file_declaration_share_one_entity() {
    let arena = Arena::default();
    let unit = parse_valid(SIBLING_EXTERN_THEN_FILE, &arena);
    assert_eq!(unit.declarations().len(), 2);
    let function = function(&unit, 0);
    let file_declaration = object(&unit, 1);
    let outer_items = compound_items(body(function));
    assert_eq!(outer_items.len(), 2);

    let first_block = compound_items(statement(&outer_items[0]));
    let second_block = compound_items(statement(&outer_items[1]));
    let CHirDecl::Object(first_extern) = declaration(&first_block[0]) else {
        panic!("expected extern declaration in first sibling block");
    };
    let CHirDecl::Object(second_extern) = declaration(&second_block[0]) else {
        panic!("expected extern declaration in second sibling block");
    };
    assert_eq!(
        [
            first_extern.symbol,
            second_extern.symbol,
            file_declaration.symbol,
        ],
        [first_extern.symbol; 3]
    );
    assert_eq!(first_extern.storage, StorageClass::Extern);
    assert_eq!(second_extern.storage, StorageClass::Extern);
    assert_eq!(file_declaration.storage, StorageClass::None);
    assert_eq!(
        first_extern.name.span,
        span_of(SIBLING_EXTERN_THEN_FILE, "x", 0)
    );
    assert_eq!(
        second_extern.name.span,
        span_of(SIBLING_EXTERN_THEN_FILE, "x", 2)
    );
    assert_eq!(
        file_declaration.name.span,
        span_of(SIBLING_EXTERN_THEN_FILE, "x", 4)
    );

    let CHirStmt::Expr(assignment, _) = statement(&first_block[1]) else {
        panic!("expected assignment through first block extern");
    };
    let CHirExpr::Assign {
        target: first_use, ..
    } = assignment
    else {
        panic!("expected x = 1");
    };
    assert_ident(
        first_use,
        SIBLING_EXTERN_THEN_FILE,
        1,
        first_extern.symbol,
        file_declaration.ty,
        ExprCategory::LValue,
        true,
    );
    let CHirStmt::Return(Some(second_use), _) = statement(&second_block[1]) else {
        panic!("expected return through second block extern");
    };
    assert_ident(
        second_use,
        SIBLING_EXTERN_THEN_FILE,
        3,
        first_extern.symbol,
        file_declaration.ty,
        ExprCategory::LValue,
        true,
    );

    let symbols = unit.symbols();
    let file_scope = symbols.file_scope();
    assert_eq!(
        symbols.scope_symbols(file_scope).unwrap(),
        &[function.symbol, first_extern.symbol]
    );
    assert_eq!(
        symbols.resolve(file_scope, "x").unwrap(),
        Some(first_extern.symbol)
    );
    let function_scopes = symbols
        .scopes_in_source_order()
        .iter()
        .filter(|scope| scope.kind() == ScopeKind::Function)
        .collect::<Vec<_>>();
    assert_eq!(function_scopes.len(), 1);
    assert_eq!(symbols.scope_symbols(function_scopes[0].id()).unwrap(), &[]);
    let block_scopes = symbols
        .scopes_in_source_order()
        .iter()
        .filter(|scope| scope.kind() == ScopeKind::Block)
        .collect::<Vec<_>>();
    assert_eq!(block_scopes.len(), 2);
    for scope in block_scopes {
        assert_eq!(
            symbols.scope_symbols(scope.id()).unwrap(),
            &[first_extern.symbol]
        );
        assert_eq!(
            symbols.resolve(scope.id(), "x").unwrap(),
            Some(first_extern.symbol)
        );
    }
    assert_eq!(symbols.symbols_in_source_order().len(), 2);
    let binding = symbols.symbol(first_extern.symbol).unwrap();
    assert_eq!(binding.kind(), SymbolKind::Object);
    assert_eq!(binding.storage_class(), Some(StorageClass::Extern));
    assert_eq!(binding.ty(), file_declaration.ty);
}

#[test]
fn inner_extern_skips_outer_auto_and_joins_the_file_entity() {
    let arena = Arena::default();
    let unit = parse_valid(INNER_EXTERN_SKIPS_OUTER_AUTO, &arena);
    assert_eq!(unit.declarations().len(), 2);
    let file_x = object(&unit, 0);
    let function = function(&unit, 1);
    let outer_items = compound_items(body(function));
    let CHirDecl::Object(auto_x) = declaration(&outer_items[0]) else {
        panic!("expected outer automatic x");
    };
    let inner_items = compound_items(statement(&outer_items[1]));
    let CHirDecl::Object(extern_x) = declaration(&inner_items[0]) else {
        panic!("expected inner extern x");
    };
    assert_eq!(
        (
            file_x.symbol.raw(),
            function.symbol.raw(),
            auto_x.symbol.raw()
        ),
        (0, 1, 2)
    );
    assert_eq!(extern_x.symbol, file_x.symbol);
    assert_ne!(extern_x.symbol, auto_x.symbol);
    assert_eq!(extern_x.storage, StorageClass::Extern);

    let symbols = unit.symbols();
    let file_scope = symbols.file_scope();
    let function_scope = symbols.symbol(auto_x.symbol).unwrap().scope();
    let inner_scope = symbols
        .scopes_in_source_order()
        .iter()
        .find(|scope| scope.kind() == ScopeKind::Block)
        .expect("inner block scope")
        .id();
    assert_eq!(
        symbols.resolve(file_scope, "x").unwrap(),
        Some(file_x.symbol)
    );
    assert_eq!(
        symbols.resolve(function_scope, "x").unwrap(),
        Some(auto_x.symbol)
    );
    assert_eq!(
        symbols.resolve(inner_scope, "x").unwrap(),
        Some(file_x.symbol)
    );
    assert_eq!(
        symbols.scope_symbols(inner_scope).unwrap(),
        &[file_x.symbol]
    );
    let CHirStmt::Return(Some(use_x), _) = statement(&inner_items[1]) else {
        panic!("expected return through inner extern");
    };
    assert_ident(
        use_x,
        INNER_EXTERN_SKIPS_OUTER_AUTO,
        3,
        file_x.symbol,
        file_x.ty,
        ExprCategory::LValue,
        true,
    );
}

#[test]
fn inner_extern_without_file_entity_is_distinct_from_outer_auto() {
    let arena = Arena::default();
    let unit = parse_valid(INNER_EXTERN_WITHOUT_FILE, &arena);
    assert_eq!(unit.declarations().len(), 1);
    let function = function(&unit, 0);
    let outer_items = compound_items(body(function));
    let CHirDecl::Object(auto_x) = declaration(&outer_items[0]) else {
        panic!("expected outer automatic x");
    };
    let inner_items = compound_items(statement(&outer_items[1]));
    let CHirDecl::Object(extern_x) = declaration(&inner_items[0]) else {
        panic!("expected inner extern x");
    };
    assert_eq!(
        (
            function.symbol.raw(),
            auto_x.symbol.raw(),
            extern_x.symbol.raw()
        ),
        (0, 1, 2)
    );
    assert_ne!(extern_x.symbol, auto_x.symbol);
    assert_eq!(extern_x.storage, StorageClass::Extern);

    let symbols = unit.symbols();
    let file_scope = symbols.file_scope();
    let function_scope = symbols.symbol(auto_x.symbol).unwrap().scope();
    let inner_scope = symbols.symbol(extern_x.symbol).unwrap().scope();
    assert_eq!(symbols.resolve(file_scope, "x").unwrap(), None);
    assert_eq!(
        symbols.resolve(function_scope, "x").unwrap(),
        Some(auto_x.symbol)
    );
    assert_eq!(
        symbols.resolve(inner_scope, "x").unwrap(),
        Some(extern_x.symbol)
    );
    assert_eq!(
        symbols.scope_symbols(inner_scope).unwrap(),
        &[extern_x.symbol]
    );
    let CHirStmt::Return(Some(use_x), _) = statement(&inner_items[1]) else {
        panic!("expected return through newly linked inner extern");
    };
    assert_ident(
        use_x,
        INNER_EXTERN_WITHOUT_FILE,
        2,
        extern_x.symbol,
        extern_x.ty,
        ExprCategory::LValue,
        true,
    );
}

#[test]
fn same_scope_auto_then_extern_is_exact_e2002_and_rolls_back_function() {
    let arena = Arena::default();
    let (unit, diagnostic) = parse_without_panic(SAME_SCOPE_AUTO_EXTERN_CONFLICT, &arena);
    let diagnostic = diagnostic.expect("same-scope auto/extern conflict must diagnose");
    assert_eq!(diagnostic.code, "E2002");
    assert_eq!(diagnostic.message, "ordinary-identifier-redefinition: x");
    assert_eq!(
        diagnostic.span,
        span_of(SAME_SCOPE_AUTO_EXTERN_CONFLICT, "x", 1)
    );
    assert!(unit.declarations().is_empty());
    assert!(unit.symbols().symbols_in_source_order().is_empty());
    assert_eq!(unit.symbols().scopes_in_source_order().len(), 1);
}

#[test]
fn unnamed_parameter_abstract_declarators_preserve_shape_and_canonical_adjustment() {
    assert_unnamed_parameter(UNNAMED_POINTER_PARAMETER, UnnamedParameterShape::Pointer);
    assert_unnamed_parameter(UNNAMED_ARRAY_PARAMETER, UnnamedParameterShape::Array);
    assert_unnamed_parameter(
        UNNAMED_FUNCTION_POINTER_PARAMETER,
        UnnamedParameterShape::FunctionPointer,
    );
}

#[test]
fn unnamed_pointer_and_scalar_parameter_redeclarations_conflict_in_both_orders() {
    let cases = [
        (UNNAMED_POINTER_THEN_SCALAR, true, "int f(const int *);"),
        (UNNAMED_SCALAR_THEN_POINTER, false, "int f(int);"),
    ];
    assert_eq!(cases.len(), 2);
    for (source, first_is_pointer, first_declaration_text) in cases {
        let arena = Arena::default();
        let (unit, diagnostic) = parse_without_panic(source, &arena);
        let diagnostic = diagnostic.expect("incompatible function redeclaration must diagnose");
        assert_eq!(diagnostic.code, "E2002", "{source:?}");
        assert_eq!(
            diagnostic.message, "ordinary-identifier-redefinition: f",
            "{source:?}"
        );
        assert_eq!(diagnostic.span, span_of(source, "f", 1), "{source:?}");
        assert_eq!(
            unit.declarations().len(),
            1,
            "incompatible second prototype must leave no CHIR event: {source:?}"
        );
        let first = function(&unit, 0);
        assert_eq!(span_text(source, first.span), first_declaration_text);
        let ParameterSyntax::Prototype { parameters, .. } = &first.parameters else {
            panic!("first declaration must remain a prototype: {source:?}");
        };
        assert_eq!(parameters.len(), 1);
        let parameter = &parameters[0];
        assert_eq!(parameter.symbol, None);
        assert_eq!(parameter.name, None);
        if first_is_pointer {
            let [WrittenDeclaratorLayer::Pointer { .. }] = parameter.written.layers.as_slice()
            else {
                panic!("qualified pointer tokens were silently lost: {source:?}");
            };
            let TypeKind::Pointer(pointee) = &unit.types().get(parameter.ty).unwrap().kind else {
                panic!("first parameter must be pointer type: {source:?}");
            };
            let pointee = unit.types().get(*pointee).unwrap();
            assert!(pointee.qualifiers.is_const, "{source:?}");
            assert_eq!(pointee.kind, TypeKind::Int(IntRank::Int), "{source:?}");
        } else {
            assert!(parameter.written.layers.is_empty(), "{source:?}");
            let scalar = unit.types().get(parameter.ty).unwrap();
            assert_eq!(scalar.qualifiers, Default::default(), "{source:?}");
            assert_eq!(scalar.kind, TypeKind::Int(IntRank::Int), "{source:?}");
        }
        assert_eq!(unit.symbols().symbols_in_source_order().len(), 1);
        assert_eq!(
            unit.symbols().scopes_in_source_order().len(),
            2,
            "failed second prototype scope must roll back: {source:?}"
        );
        assert_eq!(
            unit.symbols().symbol(first.symbol).unwrap().ty(),
            first.ty,
            "failed composite must not mutate the first binding: {source:?}"
        );
    }
}

#[test]
fn top_qualified_parameters_are_compatible_but_unspecified_to_char_is_not() {
    let arena = Arena::default();
    let unit = parse_valid(TOP_QUALIFIED_PARAMETER, &arena);
    assert_eq!(unit.declarations().len(), 2);
    let qualified = function(&unit, 0);
    let unqualified = function(&unit, 1);
    assert_eq!(qualified.symbol, unqualified.symbol);
    let ParameterSyntax::Prototype {
        parameters: qualified_parameters,
        ..
    } = &qualified.parameters
    else {
        panic!("expected qualified prototype");
    };
    let ParameterSyntax::Prototype {
        parameters: unqualified_parameters,
        ..
    } = &unqualified.parameters
    else {
        panic!("expected unqualified prototype");
    };
    assert_eq!(qualified_parameters.len(), 1);
    assert_eq!(unqualified_parameters.len(), 1);
    assert_eq!(qualified_parameters[0].symbol, None);
    assert_eq!(unqualified_parameters[0].symbol, None);
    assert_eq!(
        span_text(TOP_QUALIFIED_PARAMETER, qualified_parameters[0].span),
        "const int"
    );
    assert_eq!(
        span_text(TOP_QUALIFIED_PARAMETER, unqualified_parameters[0].span),
        "int"
    );
    let qualified_type = unit.types().get(qualified_parameters[0].ty).unwrap();
    assert!(qualified_type.qualifiers.is_const);
    assert_eq!(qualified_type.kind, TypeKind::Int(IntRank::Int));
    let unqualified_type = unit.types().get(unqualified_parameters[0].ty).unwrap();
    assert_eq!(unqualified_type.qualifiers, Default::default());
    assert_eq!(unqualified_type.kind, TypeKind::Int(IntRank::Int));
    assert_eq!(
        unit.symbols().symbol(qualified.symbol).unwrap().ty(),
        unqualified.ty,
        "binding must expose the top-qualifier-insensitive composite"
    );

    let arena = Arena::default();
    let (unit, diagnostic) = parse_without_panic(UNSPECIFIED_THEN_CHAR, &arena);
    let diagnostic = diagnostic.expect("unspecified-to-char prototype must diagnose");
    assert_eq!(diagnostic.code, "E2002");
    assert_eq!(diagnostic.message, "ordinary-identifier-redefinition: f");
    assert_eq!(diagnostic.span, span_of(UNSPECIFIED_THEN_CHAR, "f", 1));
    assert_eq!(unit.declarations().len(), 1);
    let first = function(&unit, 0);
    assert!(matches!(first.parameters, ParameterSyntax::Unspecified));
    assert_eq!(unit.symbols().symbols_in_source_order().len(), 1);
    assert_eq!(unit.symbols().scopes_in_source_order().len(), 2);
    assert_eq!(unit.symbols().symbol(first.symbol).unwrap().ty(), first.ty);
}

#[test]
fn conflicting_redeclarations_are_exact_e2002_and_leave_no_ghost_event() {
    let cases = [
        (LINKAGE_CONFLICT, "conflict", 2, 1, 1, false),
        (DUPLICATE_FUNCTION_DEFINITION, "defined", 2, 2, 2, false),
        (DUPLICATE_OBJECT_DEFINITION, "defined", 2, 1, 1, false),
        (INCOMPATIBLE_TYPEDEF, "Alias", 3, 1, 1, true),
        (INCOMPATIBLE_ARRAY_BOUND, "width", 3, 2, 1, true),
        (INCOMPATIBLE_FUNCTION_PROTO, "signature", 3, 2, 3, true),
    ];
    assert_eq!(cases.len(), 6);
    for (source, name, declaration_count, type_count, scope_count, has_recovery) in cases {
        let arena = Arena::default();
        let (unit, diagnostic) = parse_without_panic(source, &arena);
        let diagnostic = diagnostic.expect("conflicting redeclaration must diagnose");
        assert_eq!(diagnostic.code, "E2002", "{source:?}");
        assert_eq!(
            diagnostic.message,
            format!("ordinary-identifier-redefinition: {name}"),
            "{source:?}"
        );
        assert_eq!(diagnostic.span, span_of(source, name, 1), "{source:?}");
        assert_eq!(
            unit.declarations().len(),
            declaration_count,
            "failed declaration event must roll back while parsing resumes: {source:?}"
        );
        let first = declaration_symbol(unit.declarations()[0]);
        let after_index = usize::from(has_recovery) + 1;
        if has_recovery {
            assert_eq!(
                declaration_symbol(unit.declarations()[1]),
                first,
                "compatible recovery must reuse the uncorrupted binding: {source:?}"
            );
        }
        let after = object(&unit, after_index);
        assert_ne!(first, after.symbol, "{source:?}");
        assert_eq!(
            unit.symbols()
                .symbols_in_source_order()
                .iter()
                .map(|symbol| symbol.spelling())
                .collect::<Vec<_>>(),
            [name, "after"],
            "failed redeclaration must leave no ghost symbol: {source:?}"
        );
        assert_eq!(
            unit.symbols()
                .resolve(unit.symbols().file_scope(), name)
                .unwrap(),
            Some(first),
            "{source:?}"
        );
        assert_eq!(
            unit.symbols()
                .resolve(unit.symbols().file_scope(), "after")
                .unwrap(),
            Some(after.symbol),
            "{source:?}"
        );
        assert_eq!(unit.types().len(), type_count, "type rollback: {source:?}");
        assert_eq!(
            unit.symbols().scopes_in_source_order().len(),
            scope_count,
            "scope rollback: {source:?}"
        );
        let binding = unit.symbols().symbol(first).unwrap();
        let first_declaration = unit.declarations()[0];
        match first_declaration {
            CHirDecl::Object(declaration) => {
                assert_eq!(binding.kind(), SymbolKind::Object, "{source:?}");
                assert_eq!(
                    binding.ty(),
                    declaration.ty,
                    "binding type rollback: {source:?}"
                );
                assert_eq!(
                    binding.storage_class(),
                    Some(declaration.storage),
                    "binding storage rollback: {source:?}"
                );
            }
            CHirDecl::Function(declaration) => {
                assert_eq!(binding.kind(), SymbolKind::Function, "{source:?}");
                assert_eq!(
                    binding.ty(),
                    declaration.ty,
                    "binding type rollback: {source:?}"
                );
                assert_eq!(
                    binding.storage_class(),
                    Some(declaration.storage),
                    "binding storage rollback: {source:?}"
                );
            }
            CHirDecl::Typedef(declaration) => {
                assert_eq!(binding.kind(), SymbolKind::Typedef, "{source:?}");
                assert_eq!(
                    binding.ty(),
                    declaration.ty,
                    "binding type rollback: {source:?}"
                );
            }
            CHirDecl::Tag(_) => panic!("invalid ordinary redeclaration fixture began with a tag"),
        }
    }
}
