use std::collections::{HashMap, HashSet};

use evocc_chir::{
    CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, ParameterSyntax, StorageClass,
    TagMembers, TypeSpecifier, WrittenDeclaratorLayer,
};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::{
    FunctionParameters, IntRank, TagDefinition, TagId, TagKind, TagState, TypeId, TypeInterner,
    TypeKind,
};

const FILE_RAW: u32 = 41;

fn file() -> FileId {
    FileId::new(FILE_RAW)
}

fn parse<'arena>(source: &str, arena: &'arena Arena) -> evocc_chir::CHirUnit<'arena> {
    let mut lexer = Lexer::new(file(), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file(), source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    assert_unit_owned(arena, &unit);
    unit
}

fn parse_diagnostic(source: &str) -> evocc_parser::ParseDiagnostic {
    let arena = Arena::default();
    let mut lexer = Lexer::new(file(), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error);
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file(), source);
    let _ = parser.parse_translation_unit_in(&arena);
    assert!(parser.had_error(), "malformed tag input was accepted");
    parser.diagnostic().expect("structured diagnostic").clone()
}

fn resolved(types: &TypeInterner, id: TypeId) -> &TypeKind {
    &types.get(id).expect("type ID owned by unit").kind
}

fn assert_span(span: SourceSpan, source: &str) {
    assert_eq!(span.file, file());
    assert!(span.byte_start < span.byte_end, "nonempty source range");
    assert!((span.byte_end as usize) <= source.len());
    assert!(span.line_start >= 1 && span.column_start >= 1);
}

fn assert_exact(span: SourceSpan, start: u32, end: u32) {
    assert_eq!(
        span,
        SourceSpan::new(file(), start, end, 1, start + 1),
        "unexpected half-open source range"
    );
}

fn assert_expr_owned(arena: &Arena, expression: &CHirExpr<'_>) {
    assert!(
        arena.contains(expression),
        "expression escaped caller Arena"
    );
    match expression {
        CHirExpr::Floating(operation) => {
            for operand in &operation.operands {
                assert_expr_owned(arena, operand);
            }
        }
        CHirExpr::Complex(operation) => {
            for operand in &operation.operands {
                assert_expr_owned(arena, operand);
            }
        }
        CHirExpr::Vector(operation) => {
            for operand in &operation.operands {
                assert_expr_owned(arena, operand);
            }
        }
        CHirExpr::Atomic(access) => {
            for operand in access.operands() {
                assert_expr_owned(arena, operand);
            }
        }
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..)
        | CHirExpr::Ident { .. }
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::SizeOf(..)
        | CHirExpr::AlignOf(..)
        | CHirExpr::BuiltinTrap { .. }
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinFrameAddress { .. } => {}
        CHirExpr::BuiltinVarArgs { first, second, .. } => {
            assert_expr_owned(arena, first);
            if let Some(second) = second {
                assert_expr_owned(arena, second);
            }
        }
        CHirExpr::Unary(_, child, _)
        | CHirExpr::EvaluateOnce(child, _)
        | CHirExpr::Cast(_, child, _)
        | CHirExpr::SizeOfExpr(child, _, _)
        | CHirExpr::BitBuiltin { operand: child, .. }
        | CHirExpr::BuiltinConstantP { operand: child, .. }
        | CHirExpr::BuiltinObjectSize { operand: child, .. }
        | CHirExpr::Member(child, _, _)
        | CHirExpr::PtrMember(child, _, _) => assert_expr_owned(arena, child),
        CHirExpr::Update { target, .. } => {
            assert_expr_owned(arena, target);
        }
        CHirExpr::MemoryUpdate { place, .. } | CHirExpr::MemoryRead { place, .. } => {
            for source in place.source_expressions() {
                assert_expr_owned(arena, source);
            }
        }
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _)
        | CHirExpr::BuiltinExpect(left, right, _) => {
            assert_expr_owned(arena, left);
            assert_expr_owned(arena, right);
        }
        CHirExpr::Assign { target, value, .. } => {
            assert_expr_owned(arena, target);
            assert_expr_owned(arena, value);
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            for source in place.source_expressions() {
                assert_expr_owned(arena, source);
            }
            assert_expr_owned(arena, value);
        }
        CHirExpr::Ternary(condition, then_value, else_value, _) => {
            assert_expr_owned(arena, condition);
            assert_expr_owned(arena, then_value);
            assert_expr_owned(arena, else_value);
        }
        CHirExpr::Call(callee, arguments, _) => {
            assert_expr_owned(arena, callee);
            for argument in arguments {
                assert_expr_owned(arena, argument);
            }
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            assert_expr_owned(arena, lhs);
            assert_expr_owned(arena, rhs);
            assert_expr_owned(arena, target);
        }
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let evocc_chir::InitAction::StoreScalar { value, .. } = action {
                    assert_expr_owned(arena, value);
                }
            }
        }
        CHirExpr::StatementExpr { body, .. } => assert_stmt_owned(arena, body),
    }
}

fn assert_stmt_owned(arena: &Arena, statement: &CHirStmt<'_>) {
    assert!(arena.contains(statement), "statement escaped caller Arena");
    match statement {
        CHirStmt::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                assert_expr_owned(arena, operand.expression);
            }
        }
        CHirStmt::Expr(expression, _) => assert_expr_owned(arena, expression),
        CHirStmt::ComputedGoto(expression, _) => assert_expr_owned(arena, expression),
        CHirStmt::If(condition, then_branch, else_branch, _) => {
            assert_expr_owned(arena, condition);
            assert_stmt_owned(arena, then_branch);
            if let Some(else_branch) = else_branch {
                assert_stmt_owned(arena, else_branch);
            }
        }
        CHirStmt::Switch(expression, body, _) | CHirStmt::While(expression, body, _) => {
            assert_expr_owned(arena, expression);
            assert_stmt_owned(arena, body);
        }
        CHirStmt::DoWhile(body, expression, _) => {
            assert_stmt_owned(arena, body);
            assert_expr_owned(arena, expression);
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => assert_expr_owned(arena, expression),
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        assert_decl_owned(arena, declaration);
                    }
                }
            }
            if let Some(condition) = condition {
                assert_expr_owned(arena, condition);
            }
            if let Some(step) = step {
                assert_expr_owned(arena, step);
            }
            assert_stmt_owned(arena, body);
        }
        CHirStmt::Return(value, _) => {
            if let Some(value) = value {
                assert_expr_owned(arena, value);
            }
        }
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        assert_decl_owned(arena, declaration)
                    }
                    CHirBlockItem::Statement(statement) => assert_stmt_owned(arena, statement),
                }
            }
        }
        CHirStmt::Label(_, nested, _) | CHirStmt::Default(nested, _) => {
            assert_stmt_owned(arena, nested)
        }
        CHirStmt::Case(value, nested, _) => {
            assert_expr_owned(arena, value);
            assert_stmt_owned(arena, nested);
        }
        CHirStmt::CaseRange(first, last, nested, _) => {
            assert_expr_owned(arena, first);
            assert_expr_owned(arena, last);
            assert_stmt_owned(arena, nested);
        }
        CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
}

fn assert_decl_owned(arena: &Arena, declaration: &CHirDecl<'_>) {
    assert!(
        arena.contains(declaration),
        "declaration escaped caller Arena"
    );
    match declaration {
        CHirDecl::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                assert_expr_owned(arena, operand.expression);
            }
        }
        CHirDecl::Object(object) => {
            if let Some(initializer) = object.initializer.as_ref() {
                assert_expr_owned(
                    arena,
                    initializer
                        .written()
                        .expression()
                        .expect("Day 4 uses expression initializers"),
                );
            }
        }
        CHirDecl::Function(function) => {
            if let Some(body) = function.body {
                assert_stmt_owned(arena, body);
            }
        }
        CHirDecl::Tag(tag) => match &tag.members {
            TagMembers::Incomplete => {}
            TagMembers::Struct(members) | TagMembers::Union(members) => {
                for member in members {
                    if let Some(width) = member.bit_width {
                        assert_expr_owned(arena, width);
                    }
                }
            }
            TagMembers::Enum(enumerators) => {
                for enumerator in enumerators {
                    if let Some(value) = enumerator.written_value {
                        assert_expr_owned(arena, value);
                    }
                }
            }
        },
        CHirDecl::Typedef(_) => {}
    }
}

fn assert_unit_owned(arena: &Arena, unit: &evocc_chir::CHirUnit<'_>) {
    for declaration in unit.declarations() {
        assert_decl_owned(arena, declaration);
    }
}

fn assert_tag_use(specifier: &TypeSpecifier, expected_kind: TagKind, start: u32, end: u32) {
    let TypeSpecifier::Tag { kind, name, .. } = specifier else {
        panic!("expected a written tag specifier")
    };
    assert_eq!(*kind, expected_kind);
    assert_exact(name.as_ref().expect("named tag use").span, start, end);
}

fn assert_tag_consistent(unit: &evocc_chir::CHirUnit<'_>) {
    let defined = unit
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Tag(tag) if !matches!(tag.members, TagMembers::Incomplete) => Some(tag.tag),
            _ => None,
        })
        .collect::<HashSet<_>>();
    for declaration in unit.declarations() {
        let CHirDecl::Tag(declaration) = declaration else {
            continue;
        };
        let state = &unit
            .types()
            .tag(declaration.tag)
            .expect("C-HIR tag owned by unit")
            .state;
        match (&declaration.members, state) {
            (TagMembers::Incomplete, TagState::Incomplete) => {}
            (TagMembers::Incomplete, TagState::Complete(_))
                if defined.contains(&declaration.tag) =>
            {
                // A historical forward event may precede a later definition.
            }
            (TagMembers::Struct(source), TagState::Complete(TagDefinition::Struct(canonical)))
            | (TagMembers::Union(source), TagState::Complete(TagDefinition::Union(canonical))) => {
                assert_eq!(source.len(), canonical.len());
                for (source, canonical) in source.iter().zip(canonical) {
                    assert_eq!(
                        source.name.as_ref().map(|name| name.spelling.as_str()),
                        canonical.name.as_deref()
                    );
                    assert_eq!(source.ty, canonical.ty);
                }
            }
            (TagMembers::Enum(source), TagState::Complete(TagDefinition::Enum(canonical))) => {
                assert_eq!(source.len(), canonical.len());
                for (source, canonical) in source.iter().zip(canonical) {
                    assert_eq!(source.name.spelling, canonical.name);
                    assert_eq!(source.value, canonical.value);
                }
            }
            mismatch => panic!("C-HIR/interner tag mismatch: {mismatch:?}"),
        }
    }
}

fn assert_function_consistent(
    unit: &evocc_chir::CHirUnit<'_>,
    function: &evocc_chir::FunctionDecl<'_>,
) {
    let TypeKind::Function {
        return_type,
        parameters: canonical,
    } = resolved(unit.types(), function.ty)
    else {
        panic!("function declaration does not reference a function type")
    };
    assert_eq!(*return_type, function.return_specifier.type_id());
    match (&function.parameters, canonical) {
        (ParameterSyntax::Unspecified, FunctionParameters::Unspecified) => {}
        (
            ParameterSyntax::Prototype {
                parameters,
                variadic,
            },
            FunctionParameters::Prototype {
                types,
                variadic: canonical_variadic,
            },
        ) => {
            assert_eq!(*variadic, *canonical_variadic);
            assert_eq!(
                parameters
                    .iter()
                    .map(|parameter| parameter.ty)
                    .collect::<Vec<_>>()
                    .as_slice(),
                types.as_slice()
            );
        }
        mismatch => panic!("surface/canonical function mismatch: {mismatch:?}"),
    }
}

fn tag_label(
    unit: &evocc_chir::CHirUnit<'_>,
    ordinals: &HashMap<TagId, usize>,
    tag: TagId,
) -> String {
    let record = unit.types().tag(tag).expect("tag ID owned by unit");
    let kind = match record.kind {
        TagKind::Struct => "struct",
        TagKind::Union => "union",
        TagKind::Enum => "enum",
    };
    let name = record.name.as_deref().unwrap_or("anon");
    format!("{kind}#{}({name})", ordinals[&tag])
}

fn type_fingerprint(
    unit: &evocc_chir::CHirUnit<'_>,
    ordinals: &HashMap<TagId, usize>,
    ty: TypeId,
) -> String {
    let canonical = unit.types().get(ty).expect("type ID owned by unit");
    let unqualified = match &canonical.kind {
        TypeKind::Void => "void".into(),
        TypeKind::BuiltinVaList => "builtin-va-list".into(),
        TypeKind::GnuInt128(signedness) => format!("gnu-int128:{signedness:?}"),
        TypeKind::Int(IntRank::Int) => "int".into(),
        TypeKind::Int(IntRank::Long) => "long".into(),
        TypeKind::Int(rank) => format!("int:{rank:?}"),
        TypeKind::Pointer(pointee) => {
            format!("ptr({})", type_fingerprint(unit, ordinals, *pointee))
        }
        TypeKind::Vector { element, lanes, .. } => format!(
            "vector({lanes},{})",
            type_fingerprint(unit, ordinals, *element)
        ),
        TypeKind::Array { element, bound } => format!(
            "array({bound:?},{})",
            type_fingerprint(unit, ordinals, *element)
        ),
        TypeKind::Function {
            return_type,
            parameters,
        } => {
            let (parameter_types, form, variadic) = match parameters {
                FunctionParameters::Unspecified => (Vec::new(), "unspecified", false),
                FunctionParameters::Prototype { types, variadic } => (
                    types
                        .iter()
                        .map(|ty| type_fingerprint(unit, ordinals, *ty))
                        .collect(),
                    "prototype",
                    *variadic,
                ),
            };
            format!(
                "fn([{}],{},variadic:{variadic})->{}",
                parameter_types.join(","),
                form,
                type_fingerprint(unit, ordinals, *return_type)
            )
        }
        TypeKind::Struct(tag) | TypeKind::Union(tag) | TypeKind::Enum(tag) => {
            tag_label(unit, ordinals, *tag)
        }
        TypeKind::Float(kind) => format!("float:{kind:?}"),
        TypeKind::Complex(kind) => format!("complex:{kind:?}"),
        TypeKind::TargetBuiltin(kind) => format!("builtin:{kind:?}"),
    };
    let qualifiers = canonical.qualifiers;
    if qualifiers == Default::default() {
        unqualified
    } else {
        let mut names = Vec::new();
        if qualifiers.is_const {
            names.push("const");
        }
        if qualifiers.is_volatile {
            names.push("volatile");
        }
        if qualifiers.is_restrict {
            names.push("restrict");
        }
        if qualifiers.is_atomic {
            names.push("atomic");
        }
        format!("qual[{}]({unqualified})", names.join(","))
    }
}

fn fingerprint(unit: &evocc_chir::CHirUnit<'_>) -> String {
    let ordinals = unit
        .declarations()
        .iter()
        .filter_map(|decl| match decl {
            CHirDecl::Tag(tag) => Some(tag.tag),
            _ => None,
        })
        .fold(HashMap::new(), |mut map, tag| {
            let next = map.len();
            map.entry(tag).or_insert(next);
            map
        });

    unit.declarations()
        .iter()
        .map(|decl| match decl {
            CHirDecl::Tag(tag) => {
                let (state, members) = match &tag.members {
                    TagMembers::Incomplete => ("forward", String::new()),
                    TagMembers::Struct(fields) | TagMembers::Union(fields) => (
                        "definition",
                        fields
                            .iter()
                            .map(|field| {
                                format!(
                                    "{}:{}",
                                    field
                                        .name
                                        .as_ref()
                                        .map(|name| name.spelling.as_str())
                                        .unwrap_or("anon"),
                                    type_fingerprint(unit, &ordinals, field.ty)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                    TagMembers::Enum(constants) => (
                        "definition",
                        constants
                            .iter()
                            .map(|item| format!("{}={}", item.name.spelling, item.value))
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                };
                format!(
                    "tag({},{}{}{})",
                    tag_label(unit, &ordinals, tag.tag),
                    state,
                    if members.is_empty() { "" } else { ",[" },
                    if members.is_empty() {
                        String::new()
                    } else {
                        format!("{members}]")
                    }
                )
            }
            CHirDecl::Object(object) => format!(
                "object({},{})",
                object.name.spelling,
                type_fingerprint(unit, &ordinals, object.ty)
            ),
            CHirDecl::Function(function) => {
                let (parameters, form, variadic) = match &function.parameters {
                    ParameterSyntax::Unspecified => (Vec::new(), "unspecified", false),
                    ParameterSyntax::IdentifierList { parameters } => (
                        parameters
                            .iter()
                            .map(|parameter| {
                                format!(
                                    "{}:{}",
                                    parameter.name.as_ref().unwrap().spelling,
                                    type_fingerprint(unit, &ordinals, parameter.ty)
                                )
                            })
                            .collect(),
                        "identifier-list",
                        false,
                    ),
                    ParameterSyntax::Prototype {
                        parameters,
                        variadic,
                    } => (
                        parameters
                            .iter()
                            .map(|parameter| {
                                format!(
                                    "{}:{}",
                                    parameter
                                        .name
                                        .as_ref()
                                        .map(|name| name.spelling.as_str())
                                        .unwrap_or("anon"),
                                    type_fingerprint(unit, &ordinals, parameter.ty)
                                )
                            })
                            .collect(),
                        "prototype",
                        *variadic,
                    ),
                };
                let TypeKind::Function { return_type, .. } = resolved(unit.types(), function.ty)
                else {
                    panic!("function declaration must reference a function type")
                };
                format!(
                    "function({},[{}],{},{},variadic:{variadic})",
                    function.name.spelling,
                    parameters.join(","),
                    type_fingerprint(unit, &ordinals, *return_type),
                    form
                )
            }
            other => format!("unexpected:{other:?}"),
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[test]
fn aggregate_definitions_preserve_nominal_types_source_facts_and_spans() {
    let arena = Arena::default();
    let sources = [
        "struct Pair { int left; long right; };\n",
        "struct Forward; struct Forward *forward_head;\n",
        "struct { int x; int y; } point;\n",
        "struct Inner { int value; }; struct Outer { struct Inner inner; };\n",
        "struct Node { int value; struct Node *next; };\n",
        "union Word { int signed_value; long wide_value; };\n",
        "enum Mode { MODE_IDLE, MODE_BUSY = 7, MODE_DONE };\n",
        "struct Item; int inspect(struct Item *item);\n",
    ];
    let expected_fingerprints = [
        "tag(struct#0(Pair),definition,[left:int,right:long])",
        "tag(struct#0(Forward),forward);object(forward_head,ptr(struct#0(Forward)))",
        "tag(struct#0(anon),definition,[x:int,y:int]);object(point,struct#0(anon))",
        "tag(struct#0(Inner),definition,[value:int]);tag(struct#1(Outer),definition,[inner:struct#0(Inner)])",
        "tag(struct#0(Node),definition,[value:int,next:ptr(struct#0(Node))])",
        "tag(union#0(Word),definition,[signed_value:int,wide_value:long])",
        "tag(enum#0(Mode),definition,[MODE_IDLE=0,MODE_BUSY=7,MODE_DONE=8])",
        "tag(struct#0(Item),forward);function(inspect,[item:ptr(struct#0(Item))],int,prototype,variadic:false)",
    ];
    assert_eq!(sources.len(), 8);
    assert_eq!(
        sources
            .iter()
            .map(|source| source.as_bytes())
            .collect::<HashSet<_>>()
            .len(),
        8,
        "aggregate input denominator must be byte-unique"
    );
    for (source, expected) in sources.iter().zip(expected_fingerprints) {
        let unit = parse(source, &arena);
        assert_tag_consistent(&unit);
        assert_eq!(fingerprint(&unit), expected, "source {source:?}");
    }

    let pair = parse(sources[0], &arena);
    let CHirDecl::Tag(pair_decl) = &pair.declarations()[0] else {
        panic!("expected pair tag")
    };
    assert_eq!(pair_decl.name.as_ref().unwrap().spelling, "Pair");
    assert_exact(pair_decl.span, 0, 38);
    assert_exact(pair_decl.specifier_span, 0, 37);
    assert_exact(pair_decl.name.as_ref().unwrap().span, 7, 11);
    let TagMembers::Struct(pair_fields) = &pair_decl.members else {
        panic!("expected struct members")
    };
    assert_eq!(pair_fields.len(), 2);
    assert_eq!(pair_fields[0].name.as_ref().unwrap().spelling, "left");
    assert!(matches!(
        resolved(pair.types(), pair_fields[0].ty),
        TypeKind::Int(IntRank::Int)
    ));
    assert!(matches!(
        resolved(pair.types(), pair_fields[1].ty),
        TypeKind::Int(IntRank::Long)
    ));
    assert_span(pair_decl.span, sources[0]);
    assert_exact(pair_fields[0].span, 14, 23);
    assert_exact(pair_fields[0].specifier.span(), 14, 17);
    assert_exact(pair_fields[0].name.as_ref().unwrap().span, 18, 22);
    assert_exact(pair_fields[0].written.span, 18, 22);
    assert_exact(pair_fields[1].span, 24, 35);
    assert_exact(pair_fields[1].specifier.span(), 24, 28);
    assert_exact(pair_fields[1].name.as_ref().unwrap().span, 29, 34);
    assert_exact(pair_fields[1].written.span, 29, 34);
    assert!(pair_fields.iter().all(|field| field.bit_width.is_none()));

    let forward = parse(sources[1], &arena);
    let (CHirDecl::Tag(forward_tag), CHirDecl::Object(forward_object)) =
        (&forward.declarations()[0], &forward.declarations()[1])
    else {
        panic!("expected forward tag and object")
    };
    assert!(matches!(forward_tag.members, TagMembers::Incomplete));
    assert_exact(forward_tag.span, 0, 15);
    assert_exact(forward_tag.specifier_span, 0, 14);
    assert_exact(forward_tag.name.as_ref().unwrap().span, 7, 14);
    assert_exact(forward_object.span, 16, 45);
    assert_exact(forward_object.name.span, 32, 44);
    assert_exact(forward_object.specifier.span(), 16, 30);
    assert_tag_use(&forward_object.specifier, TagKind::Struct, 23, 30);
    let TypeKind::Pointer(pointee) = resolved(forward.types(), forward_object.ty) else {
        panic!("forward object must be a pointer")
    };
    assert_eq!(
        resolved(forward.types(), *pointee),
        &TypeKind::Struct(forward_tag.tag)
    );
    assert_eq!(forward_object.written.layers.len(), 1);
    assert_exact(forward_object.written.span, 31, 44);
    assert_eq!(forward_object.storage, StorageClass::None);
    assert!(matches!(
        forward_object.written.layers[0],
        WrittenDeclaratorLayer::Pointer { .. }
    ));

    let anonymous = parse(sources[2], &arena);
    let (CHirDecl::Tag(anonymous_tag), CHirDecl::Object(point)) =
        (&anonymous.declarations()[0], &anonymous.declarations()[1])
    else {
        panic!("expected anonymous tag and point object")
    };
    assert!(anonymous_tag.name.is_none());
    assert_exact(anonymous_tag.span, 0, 31);
    assert_exact(anonymous_tag.specifier_span, 0, 24);
    assert_exact(point.span, 0, 31);
    assert_exact(point.name.span, 25, 30);
    let TagMembers::Struct(anonymous_fields) = &anonymous_tag.members else {
        panic!("expected anonymous fields")
    };
    assert_exact(anonymous_fields[0].span, 9, 15);
    assert_exact(anonymous_fields[0].name.as_ref().unwrap().span, 13, 14);
    assert_exact(anonymous_fields[1].span, 16, 22);
    assert_exact(anonymous_fields[1].name.as_ref().unwrap().span, 20, 21);
    assert_eq!(
        resolved(anonymous.types(), point.ty),
        &TypeKind::Struct(anonymous_tag.tag)
    );

    let nested = parse(sources[3], &arena);
    let (CHirDecl::Tag(inner), CHirDecl::Tag(outer)) =
        (&nested.declarations()[0], &nested.declarations()[1])
    else {
        panic!("expected inner and outer tags")
    };
    let TagMembers::Struct(outer_fields) = &outer.members else {
        panic!("expected outer fields")
    };
    assert_exact(inner.span, 0, 28);
    assert_exact(inner.specifier_span, 0, 27);
    assert_exact(outer.span, 29, 66);
    assert_exact(outer.specifier_span, 29, 65);
    let TagMembers::Struct(inner_fields) = &inner.members else {
        panic!("expected inner fields")
    };
    assert_exact(inner_fields[0].span, 15, 25);
    assert_exact(inner_fields[0].name.as_ref().unwrap().span, 19, 24);
    assert_exact(outer_fields[0].span, 44, 63);
    assert_exact(outer_fields[0].specifier.span(), 44, 56);
    assert_exact(outer_fields[0].name.as_ref().unwrap().span, 57, 62);
    assert_tag_use(&outer_fields[0].specifier, TagKind::Struct, 51, 56);
    assert_eq!(
        resolved(nested.types(), outer_fields[0].ty),
        &TypeKind::Struct(inner.tag)
    );

    let recursive = parse(sources[4], &arena);
    let CHirDecl::Tag(node) = &recursive.declarations()[0] else {
        panic!("expected node tag")
    };
    let TagMembers::Struct(node_fields) = &node.members else {
        panic!("expected node fields")
    };
    assert_exact(node.span, 0, 46);
    assert_exact(node.specifier_span, 0, 45);
    assert_exact(node_fields[1].span, 25, 43);
    assert_exact(node_fields[0].span, 14, 24);
    assert_exact(node_fields[0].name.as_ref().unwrap().span, 18, 23);
    assert_exact(node_fields[1].specifier.span(), 25, 36);
    assert_tag_use(&node_fields[1].specifier, TagKind::Struct, 32, 36);
    let TypeKind::Pointer(next_pointee) = resolved(recursive.types(), node_fields[1].ty) else {
        panic!("next must be a pointer")
    };
    assert_eq!(
        resolved(recursive.types(), *next_pointee),
        &TypeKind::Struct(node.tag)
    );

    let union = parse(sources[5], &arena);
    let CHirDecl::Tag(word) = &union.declarations()[0] else {
        panic!("expected union tag")
    };
    assert_eq!(union.types().tag(word.tag).unwrap().kind, TagKind::Union);
    let TagMembers::Union(word_fields) = &word.members else {
        panic!("expected union members")
    };
    assert_eq!(word_fields.len(), 2);
    assert_exact(word.span, 0, 50);
    assert_exact(word.specifier_span, 0, 49);
    assert_exact(word.name.as_ref().unwrap().span, 6, 10);
    assert_exact(word_fields[0].span, 13, 30);
    assert_exact(word_fields[0].name.as_ref().unwrap().span, 17, 29);
    assert_exact(word_fields[1].span, 31, 47);
    assert_exact(word_fields[1].name.as_ref().unwrap().span, 36, 46);

    let enumeration = parse(sources[6], &arena);
    let CHirDecl::Tag(mode) = &enumeration.declarations()[0] else {
        panic!("expected enum tag")
    };
    let TagMembers::Enum(constants) = &mode.members else {
        panic!("expected enumerators")
    };
    assert_eq!(
        constants
            .iter()
            .map(|item| (item.name.spelling.as_str(), item.value))
            .collect::<Vec<_>>(),
        vec![("MODE_IDLE", 0), ("MODE_BUSY", 7), ("MODE_DONE", 8)]
    );
    assert_exact(mode.span, 0, 50);
    assert_exact(mode.specifier_span, 0, 49);
    assert_exact(mode.name.as_ref().unwrap().span, 5, 9);
    assert_exact(constants[0].span, 12, 21);
    assert_exact(constants[0].name.span, 12, 21);
    assert_exact(constants[1].span, 23, 36);
    assert_exact(constants[1].name.span, 23, 32);
    assert_exact(constants[1].written_value.as_ref().unwrap().span(), 35, 36);
    assert_exact(constants[2].span, 38, 47);
    assert_exact(constants[2].name.span, 38, 47);
    assert!(constants[0].written_value.is_none());
    assert!(constants[2].written_value.is_none());
    for constant in constants {
        assert_span(constant.span, sources[6]);
    }

    let function = parse(sources[7], &arena);
    let (CHirDecl::Tag(item), CHirDecl::Function(inspect)) =
        (&function.declarations()[0], &function.declarations()[1])
    else {
        panic!("expected item tag and inspect function")
    };
    let ParameterSyntax::Prototype {
        parameters,
        variadic,
    } = &inspect.parameters
    else {
        panic!("inspect must be a prototype")
    };
    assert!(!variadic && parameters.len() == 1);
    assert_eq!(inspect.storage, StorageClass::None);
    assert_eq!(parameters[0].name.as_ref().unwrap().spelling, "item");
    assert_exact(item.span, 0, 12);
    assert_exact(item.specifier_span, 0, 11);
    assert_exact(item.name.as_ref().unwrap().span, 7, 11);
    assert_exact(inspect.span, 13, 44);
    assert_exact(inspect.return_specifier.span(), 13, 16);
    assert_exact(inspect.name.span, 17, 24);
    assert_exact(parameters[0].span, 25, 42);
    assert_exact(parameters[0].specifier.span(), 25, 36);
    assert_exact(parameters[0].name.as_ref().unwrap().span, 38, 42);
    assert_exact(parameters[0].written.span, 37, 42);
    assert_tag_use(&parameters[0].specifier, TagKind::Struct, 32, 36);
    let TypeKind::Pointer(item_pointee) = resolved(function.types(), parameters[0].ty) else {
        panic!("item parameter must be a pointer")
    };
    assert_eq!(
        resolved(function.types(), *item_pointee),
        &TypeKind::Struct(item.tag)
    );
    assert_span(inspect.span, sources[7]);
    assert_function_consistent(&function, inspect);
}

#[test]
fn function_parameter_form_is_canonical_but_names_and_spans_are_not() {
    let arena = Arena::default();
    let source = "int unspecified(); int empty(void); int named(int left); int renamed(int right); int varargs(int head, ...);\n";
    let unit = parse(source, &arena);
    assert_eq!(unit.declarations().len(), 5);

    let functions = unit
        .declarations()
        .iter()
        .map(|decl| match decl {
            CHirDecl::Function(function) => function,
            _ => panic!("expected only functions"),
        })
        .collect::<Vec<_>>();
    let kind = |index: usize| resolved(unit.types(), functions[index].ty);
    for function in &functions {
        assert_function_consistent(&unit, function);
    }
    assert!(matches!(
        kind(0),
        TypeKind::Function {
            parameters: FunctionParameters::Unspecified,
            ..
        }
    ));
    assert!(matches!(
        kind(1),
        TypeKind::Function {
            parameters: FunctionParameters::Prototype { types, variadic: false },
            ..
        } if types.is_empty()
    ));
    assert_eq!(
        functions[2].ty, functions[3].ty,
        "parameter spelling must not split type identity"
    );
    assert_ne!(functions[0].ty, functions[1].ty);
    assert!(matches!(
        kind(4),
        TypeKind::Function {
            parameters: FunctionParameters::Prototype { types, variadic: true },
            ..
        } if types.len() == 1
    ));
    let names = |function: &evocc_chir::FunctionDecl<'_>| match &function.parameters {
        ParameterSyntax::Prototype { parameters, .. }
        | ParameterSyntax::IdentifierList { parameters } => parameters
            .iter()
            .map(|parameter| parameter.name.as_ref().map(|name| name.spelling.clone()))
            .collect::<Vec<_>>(),
        ParameterSyntax::Unspecified => Vec::new(),
    };
    assert_eq!(names(functions[2]), vec![Some("left".to_owned())]);
    assert_eq!(names(functions[3]), vec![Some("right".to_owned())]);
    assert_exact(functions[2].span, 36, 56);
    assert_exact(functions[2].name.span, 40, 45);
    assert_exact(functions[2].written.span, 40, 55);
    let ParameterSyntax::Prototype {
        parameters: named_parameters,
        ..
    } = &functions[2].parameters
    else {
        unreachable!()
    };
    assert_exact(named_parameters[0].span, 46, 54);
    assert_exact(named_parameters[0].name.as_ref().unwrap().span, 50, 54);
    assert_exact(named_parameters[0].written.span, 50, 54);
    assert_exact(functions[3].span, 57, 80);
    assert_exact(functions[3].name.span, 61, 68);
    assert_exact(functions[3].written.span, 61, 79);
    let ParameterSyntax::Prototype {
        parameters: renamed_parameters,
        ..
    } = &functions[3].parameters
    else {
        unreachable!()
    };
    assert_exact(renamed_parameters[0].span, 69, 78);
    assert_exact(renamed_parameters[0].name.as_ref().unwrap().span, 73, 78);
    assert_exact(renamed_parameters[0].written.span, 73, 78);
    for function in functions {
        assert_span(function.span, source);
        assert_span(function.written.span, source);
    }
}

#[test]
fn same_shape_anonymous_tags_remain_nominally_distinct() {
    let arena = Arena::default();
    let source = "struct { int value; } first; struct { int value; } second;\n";
    let unit = parse(source, &arena);
    assert_eq!(unit.declarations().len(), 4);
    let tags = unit
        .declarations()
        .iter()
        .filter_map(|decl| match decl {
            CHirDecl::Tag(tag) => Some(tag.tag),
            _ => None,
        })
        .collect::<Vec<_>>();
    let objects = unit
        .declarations()
        .iter()
        .filter_map(|decl| match decl {
            CHirDecl::Object(object) => Some(object.ty),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tags.len(), 2);
    assert_eq!(objects.len(), 2);
    assert_ne!(tags[0], tags[1]);
    assert_ne!(objects[0], objects[1]);
    assert_eq!(
        resolved(unit.types(), objects[0]),
        &TypeKind::Struct(tags[0])
    );
    assert_eq!(
        resolved(unit.types(), objects[1]),
        &TypeKind::Struct(tags[1])
    );
    assert_tag_consistent(&unit);
}

#[test]
fn forward_definition_and_later_reference_share_one_completed_tag() {
    let arena = Arena::default();
    let source = "struct Later; struct Later { int value; }; struct Later *tail;\n";
    let unit = parse(source, &arena);
    assert_eq!(unit.declarations().len(), 3);
    let (CHirDecl::Tag(forward), CHirDecl::Tag(definition), CHirDecl::Object(tail)) = (
        &unit.declarations()[0],
        &unit.declarations()[1],
        &unit.declarations()[2],
    ) else {
        panic!("expected forward, definition, and object")
    };
    assert!(matches!(forward.members, TagMembers::Incomplete));
    assert!(matches!(definition.members, TagMembers::Struct(_)));
    assert_eq!(forward.tag, definition.tag);
    assert!(matches!(
        &unit.types().tag(forward.tag).unwrap().state,
        TagState::Complete(TagDefinition::Struct(fields)) if fields.len() == 1
    ));
    let TypeKind::Pointer(pointee) = resolved(unit.types(), tail.ty) else {
        panic!("tail must be a pointer")
    };
    assert_eq!(
        resolved(unit.types(), *pointee),
        &TypeKind::Struct(forward.tag)
    );
    assert_tag_consistent(&unit);
}

#[test]
fn returned_unit_owns_types_and_source_facts_after_parser_input_is_dropped() {
    let arena = Arena::default();
    let unit = {
        let source = String::from("struct Durable { int value; };\n");
        parse(&source, &arena)
    };
    assert_eq!(
        fingerprint(&unit),
        "tag(struct#0(Durable),definition,[value:int])"
    );
    assert_eq!(unit.len(), 1, "slice-style CHirUnit compatibility");
    assert!(matches!(&unit[0], CHirDecl::Tag(_)));
    assert_eq!(unit.iter().count(), 1);
    assert_tag_consistent(&unit);
}

#[test]
fn array_parameter_adjusts_to_pointer_without_losing_written_declarator() {
    let arena = Arena::default();
    let source = "int array_parameter(int values[3]); int pointer_parameter(int *values);\n";
    let unit = parse(source, &arena);
    let functions = unit
        .declarations()
        .iter()
        .map(|declaration| match declaration {
            CHirDecl::Function(function) => function,
            _ => panic!("expected functions"),
        })
        .collect::<Vec<_>>();
    assert_eq!(functions.len(), 2);
    assert_eq!(functions[0].ty, functions[1].ty);
    let ParameterSyntax::Prototype {
        parameters: arrays, ..
    } = &functions[0].parameters
    else {
        panic!("array parameter prototype")
    };
    let ParameterSyntax::Prototype {
        parameters: pointers,
        ..
    } = &functions[1].parameters
    else {
        panic!("pointer parameter prototype")
    };
    assert!(matches!(
        arrays[0].written.layers.as_slice(),
        [WrittenDeclaratorLayer::Array { .. }]
    ));
    assert!(matches!(
        pointers[0].written.layers.as_slice(),
        [WrittenDeclaratorLayer::Pointer { .. }]
    ));
    assert_eq!(arrays[0].ty, pointers[0].ty);
    let TypeKind::Pointer(pointee) = resolved(unit.types(), arrays[0].ty) else {
        panic!("adjusted array parameter must be a pointer")
    };
    let pointee = unit.types().get(*pointee).unwrap();
    assert_eq!(pointee.qualifiers, Default::default());
    assert_eq!(pointee.kind, TypeKind::Int(IntRank::Int));
    assert_function_consistent(&unit, functions[0]);
    assert_function_consistent(&unit, functions[1]);
}

#[test]
fn bit_field_width_syntax_is_preserved_without_computing_layout() {
    let arena = Arena::default();
    let source = "struct Flags { unsigned ready : 1; unsigned mode : 3; unsigned : 0; };\n";
    let unit = parse(source, &arena);
    let CHirDecl::Tag(flags) = &unit.declarations()[0] else {
        panic!("expected Flags tag")
    };
    let TagMembers::Struct(fields) = &flags.members else {
        panic!("expected Flags members")
    };
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].name.as_ref().unwrap().spelling, "ready");
    assert_eq!(fields[1].name.as_ref().unwrap().spelling, "mode");
    assert!(fields[2].name.is_none(), "zero-width separator is unnamed");
    for field in fields {
        assert!(matches!(
            resolved(unit.types(), field.ty),
            TypeKind::Int(IntRank::UInt)
        ));
    }
    let width = |field: &evocc_chir::RecordMember<'_>| match field.bit_width {
        Some(CHirExpr::Lit(value, _)) => value.value,
        other => panic!("expected literal bit width, got {other:?}"),
    };
    assert_eq!(
        [width(&fields[0]), width(&fields[1]), width(&fields[2])],
        [1, 3, 0]
    );
    assert_exact(flags.span, 0, 70);
    assert_exact(flags.specifier_span, 0, 69);
    assert_exact(fields[0].span, 15, 34);
    assert_exact(fields[0].bit_width.as_ref().unwrap().span(), 32, 33);
    assert_exact(fields[1].span, 35, 53);
    assert_exact(fields[1].bit_width.as_ref().unwrap().span(), 51, 52);
    assert_exact(fields[2].span, 54, 67);
    assert_exact(fields[2].bit_width.as_ref().unwrap().span(), 65, 66);
    assert_tag_consistent(&unit);
}

#[test]
fn same_scope_tag_kind_conflicts_and_duplicate_definitions_fail_closed() {
    let cases = [
        (
            "struct S; union S;",
            "E1102",
            "conflicting-tag-kind: S",
            SourceSpan::new(file(), 16, 17, 1, 17),
        ),
        (
            "struct S { int x; }; struct S { long y; };",
            "E1101",
            "duplicate-tag-definition: S",
            SourceSpan::new(file(), 28, 29, 1, 29),
        ),
    ];
    assert_eq!(cases.len(), 2);
    for (source, code, message, span) in cases {
        let diagnostic = parse_diagnostic(source);
        assert_eq!(diagnostic.code, code);
        assert_eq!(diagnostic.message, message);
        assert_eq!(diagnostic.span, span);
    }
}

/// The kernel's `struct list_head` declares two members with one specifier.
/// Every declarator in a member list is its own member, with its own
/// derived type, bit width and per-declarator attributes.
#[test]
fn a_member_declarator_list_yields_one_member_per_declarator() {
    let arena = Arena::default();
    let source = "struct list_head { struct list_head *next, *prev; } lh;\n\
                  struct flags { unsigned a : 3, b : 5, c; int *p, q[4], __attribute__((aligned(8))) r; } fl;\n";
    let unit = parse(source, &arena);
    let tags: Vec<(TagId, Vec<String>)> = unit
        .declarations()
        .iter()
        .filter_map(|decl| match decl {
            CHirDecl::Tag(tag) => match &tag.members {
                TagMembers::Struct(members) => Some((
                    tag.tag,
                    members
                        .iter()
                        .map(|m| {
                            m.name
                                .as_ref()
                                .map_or(String::new(), |n| n.spelling.clone())
                        })
                        .collect(),
                )),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let names: Vec<&[String]> = tags.iter().map(|(_, names)| names.as_slice()).collect();
    assert_eq!(
        names,
        [
            &["next".to_string(), "prev".to_string()][..],
            &[
                "a".into(),
                "b".into(),
                "c".into(),
                "p".into(),
                "q".into(),
                "r".into()
            ][..],
        ]
    );
    let types = unit.types();
    let layout_of = |object: &str| {
        let scope = unit.symbols().file_scope();
        let symbol = unit
            .symbols()
            .resolve(scope, object)
            .expect("symbol table is live")
            .unwrap_or_else(|| panic!("{object} is declared"));
        let ty = unit.symbols().symbol(symbol).expect("declared").ty();
        let layout = evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu()
            .layout_of(types, ty)
            .unwrap_or_else(|e| panic!("layout of {object}: {e:?}"));
        (layout.size_bytes, layout.align_bytes)
    };
    assert_eq!(layout_of("lh"), (16, 8));
    // a,b share one unsigned unit (4), c (4), p (8 at 8), q (16 at 16), r aligned 8 (4 at 32) -> 40
    assert_eq!(layout_of("fl"), (40, 8));
    let fields = match &types.tag(tags[1].0).expect("flags tag").state {
        TagState::Complete(TagDefinition::Struct(fields)) => fields,
        other => panic!("struct flags has fields, got {other:?}"),
    };
    assert_eq!(
        fields[5].layout.aligned,
        Some(8),
        "aligned attaches to `r` only"
    );
    assert_eq!(fields[3].layout.aligned, None);
}
