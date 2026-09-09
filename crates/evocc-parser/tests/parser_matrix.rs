use std::collections::HashSet;

use evocc_chir::{
    AssignmentOp, BinaryOp, CHirBlockItem, CHirDecl, CHirExpr, CHirStmt, InitAction,
    ParameterSyntax, UnaryOp, UpdateOp,
};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::{ArrayBound, FunctionParameters, IntRank, TypeId, TypeInterner, TypeKind};

struct ExpressionCase {
    id: &'static str,
    expression: &'static str,
    expected: &'static str,
}

struct DeclaratorCase {
    id: &'static str,
    source: &'static str,
    expected: &'static str,
}

const EXPRESSIONS: &[ExpressionCase] = &[
    ExpressionCase {
        id: "expr.primary",
        expression: "seed",
        expected: "id(seed)",
    },
    ExpressionCase {
        id: "expr.postfix.call",
        expression: "callee(left, right)",
        expected: "call(id(callee),[id(left),id(right)])",
    },
    ExpressionCase {
        id: "expr.postfix.index",
        expression: "items[index]",
        expected: "memory_read(id(items),id(index))",
    },
    ExpressionCase {
        id: "expr.postfix.member",
        expression: "object.member",
        expected: "memory_read()",
    },
    ExpressionCase {
        id: "expr.postfix.pointer_member",
        expression: "pointer->member",
        expected: "memory_read(id(pointer))",
    },
    ExpressionCase {
        id: "expr.postfix.increment",
        expression: "counter++",
        expected: "unary(post_inc,id(counter))",
    },
    ExpressionCase {
        id: "expr.unary.prefix",
        expression: "!*&value",
        expected: "unary(not,unary(deref,unary(addr,id(value))))",
    },
    ExpressionCase {
        id: "expr.cast",
        expression: "(long)value",
        expected: "cast(long,id(value))",
    },
    ExpressionCase {
        id: "expr.sizeof.expression",
        expression: "sizeof value",
        expected: "sizeof_expr(id(value))",
    },
    ExpressionCase {
        id: "expr.sizeof.type",
        expression: "sizeof(long)",
        expected: "sizeof_type(long)",
    },
    ExpressionCase {
        id: "expr.alignof.type",
        expression: "_Alignof(long)",
        expected: "alignof(long)",
    },
    ExpressionCase {
        id: "expr.multiplicative_additive",
        expression: "left + middle * right",
        expected: "binary(add,id(left),binary(mul,id(middle),id(right)))",
    },
    ExpressionCase {
        id: "expr.shift_additive",
        expression: "left << middle + right",
        expected: "binary(shl,id(left),binary(add,id(middle),id(right)))",
    },
    ExpressionCase {
        id: "expr.relational_equality",
        expression: "left < middle == right",
        expected: "binary(eq,binary(lt,id(left),id(middle)),id(right))",
    },
    ExpressionCase {
        id: "expr.bitwise_precedence",
        expression: "alpha | beta ^ gamma & delta",
        expected:
            "binary(bit_or,id(alpha),binary(bit_xor,id(beta),binary(bit_and,id(gamma),id(delta))))",
    },
    ExpressionCase {
        id: "expr.logical_precedence",
        expression: "left || middle && right",
        expected: "binary(or,id(left),binary(and,id(middle),id(right)))",
    },
    ExpressionCase {
        id: "expr.conditional",
        expression: "condition ? left + one : right * two",
        expected:
            "ternary(id(condition),binary(add,id(left),id(one)),binary(mul,id(right),id(two)))",
    },
    ExpressionCase {
        id: "expr.assignment.right_associative",
        expression: "left = middle = right",
        expected: "assign(id(left),assign(id(middle),id(right)))",
    },
    ExpressionCase {
        id: "expr.comma.left_associative",
        expression: "(left, middle, right)",
        expected: "comma(comma(id(left),id(middle)),id(right))",
    },
    ExpressionCase {
        id: "expr.parentheses.override",
        expression: "(left + middle) * right",
        expected: "binary(mul,binary(add,id(left),id(middle)),id(right))",
    },
];

// The postfix probes below are real C, not merely token sequences: subscripting
// requires a pointer operand and member access requires a record operand, so
// `items`, `object`, `pointer` and `callee` have the types those operators
// actually accept.
const EXPRESSION_BINDINGS: &str = concat!(
    "struct Probe { int member; }; ",
    "int seed; int callee(int, int); int left; int right; int *items; int index; ",
    "struct Probe object; struct Probe *pointer; int counter; int value; int middle; ",
    "int alpha; int beta; int gamma; int delta; int condition; int one; int two; "
);

const DECLARATORS: &[DeclaratorCase] = &[
    DeclaratorCase {
        id: "decl.object",
        source: "int value;",
        expected: "var(value,int)",
    },
    DeclaratorCase {
        id: "decl.pointer",
        source: "int *pointer;",
        expected: "var(pointer,ptr(int))",
    },
    DeclaratorCase {
        id: "decl.array.fixed",
        source: "int values[4];",
        expected: "var(values,array(fixed:4,int))",
    },
    DeclaratorCase {
        id: "decl.array.unbounded",
        source: "int values[];",
        expected: "var(values,array(unbounded,int))",
    },
    DeclaratorCase {
        id: "decl.function",
        source: "int compute(int left, long right);",
        expected: "fn(compute,[left:int,right:long],int)",
    },
    DeclaratorCase {
        id: "decl.function_pointer",
        source: "int (*handler)(int argument);",
        expected: "var(handler,ptr(fn([int])->int))",
    },
    DeclaratorCase {
        id: "decl.array_of_pointers",
        source: "int *table[4];",
        expected: "var(table,array(fixed:4,ptr(int)))",
    },
    DeclaratorCase {
        id: "decl.pointer_to_array",
        source: "int (*row)[4];",
        expected: "var(row,ptr(array(fixed:4,int)))",
    },
    DeclaratorCase {
        id: "decl.array_of_function_pointers",
        source: "int (*callbacks[3])(int argument);",
        expected: "var(callbacks,array(fixed:3,ptr(fn([int])->int)))",
    },
];

fn parse_unit<'arena>(source: &str, arena: &'arena Arena) -> evocc_chir::CHirUnit<'arena> {
    let mut lexer = Lexer::new(FileId::new(17), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    assert_eq!(
        tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Eof)
            .count(),
        1,
        "lexer must produce exactly one EOF for {source:?}"
    );

    let mut parser = Parser::new(tokens, FileId::new(17), source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "parser reported an error for {source:?}"
    );
    unit
}

fn canonical_expr(expr: &CHirExpr<'_>, types: &TypeInterner) -> String {
    match expr {
        CHirExpr::Lit(value, _) => format!("int({})", value.value),
        CHirExpr::FloatLit(_, _) => "float".into(),
        CHirExpr::CharLit(_, _) => "char".into(),
        CHirExpr::StringLit(_, _) => "string".into(),
        CHirExpr::Ident { spelling, .. } => format!("id({spelling})"),
        CHirExpr::LabelAddress { label, .. } => format!("label_address({})", label.spelling),
        CHirExpr::Unary(op, operand, _) => {
            format!(
                "unary({},{})",
                unary_name(*op),
                canonical_expr(operand, types)
            )
        }
        CHirExpr::EvaluateOnce(operand, _) => canonical_expr(operand, types),
        CHirExpr::Update { op, target, .. } => format!(
            "unary({},{})",
            update_name(*op),
            canonical_expr(target, types)
        ),
        CHirExpr::MemoryRead { place, .. } => format!(
            "memory_read({})",
            place
                .source_expressions()
                .map(|source| canonical_expr(source, types))
                .collect::<Vec<_>>()
                .join(",")
        ),
        CHirExpr::MemoryUpdate { op, place, .. } => format!(
            "memory_unary({},{})",
            update_name(*op),
            place
                .source_expressions()
                .map(|source| canonical_expr(source, types))
                .collect::<Vec<_>>()
                .join(",")
        ),
        CHirExpr::Binary(op, left, right, _) => format!(
            "binary({},{},{})",
            binary_name(*op),
            canonical_expr(left, types),
            canonical_expr(right, types)
        ),
        CHirExpr::Ternary(condition, left, right, _) => format!(
            "ternary({},{},{})",
            canonical_expr(condition, types),
            canonical_expr(left, types),
            canonical_expr(right, types)
        ),
        CHirExpr::Elvis(operand, fallback, _) => format!(
            "elvis({},{})",
            canonical_expr(operand, types),
            canonical_expr(fallback, types)
        ),
        CHirExpr::Call(callee, arguments, _) => format!(
            "call({},[{}])",
            canonical_expr(callee, types),
            arguments
                .iter()
                .map(|argument| canonical_expr(argument, types))
                .collect::<Vec<_>>()
                .join(",")
        ),
        CHirExpr::CheckedArithmetic {
            op,
            lhs,
            rhs,
            target,
            ..
        } => format!(
            "checked({op:?},{},{},{})",
            canonical_expr(lhs, types),
            canonical_expr(rhs, types),
            canonical_expr(target, types)
        ),
        CHirExpr::BitBuiltin { op, operand, .. } => {
            format!("bit_builtin({op:?},{})", canonical_expr(operand, types))
        }
        CHirExpr::BuiltinUnreachable { .. } => "builtin_unreachable".into(),
        CHirExpr::BuiltinTrap { .. } => "builtin_trap".into(),
        CHirExpr::BuiltinReturnAddress { .. } => "builtin_return_address".into(),
        CHirExpr::BuiltinFrameAddress { .. } => "builtin_frame_address".into(),
        CHirExpr::BuiltinVarArgs {
            op, first, second, ..
        } => format!(
            "builtin_varargs({op:?},{},{})",
            canonical_expr(first, types),
            second.map_or_else(|| "none".into(), |second| canonical_expr(second, types))
        ),
        CHirExpr::Cast(ty, operand, _) => {
            format!(
                "cast({},{})",
                canonical_type(*ty, types),
                canonical_expr(operand, types)
            )
        }
        CHirExpr::SizeOf(ty, _) => format!("sizeof_type({})", canonical_type(*ty, types)),
        CHirExpr::SizeOfExpr(operand, _, _) => {
            format!("sizeof_expr({})", canonical_expr(operand, types))
        }
        CHirExpr::AlignOf(ty, _) => format!("alignof({})", canonical_type(*ty, types)),
        CHirExpr::Member(object, member, _) => {
            format!("member({},{member})", canonical_expr(object, types))
        }
        CHirExpr::PtrMember(object, member, _) => {
            format!("ptr_member({},{member})", canonical_expr(object, types))
        }
        CHirExpr::Index(object, index, _) => format!(
            "index({},{})",
            canonical_expr(object, types),
            canonical_expr(index, types)
        ),
        CHirExpr::Assign {
            op,
            target: left,
            value: right,
            ..
        } => {
            format!(
                "{}({},{})",
                assignment_name(*op),
                canonical_expr(left, types),
                canonical_expr(right, types)
            )
        }
        CHirExpr::MemoryAssign {
            op,
            place,
            value: right,
            ..
        } => format!(
            "memory_{}({},{})",
            assignment_name(*op),
            place
                .source_expressions()
                .map(|source| canonical_expr(source, types))
                .collect::<Vec<_>>()
                .join(","),
            canonical_expr(right, types)
        ),
        CHirExpr::CompoundLit(ty, initializer, _) => format!(
            "compound({},[{}])",
            canonical_type(*ty, types),
            initializer
                .plan()
                .actions()
                .iter()
                .filter_map(|action| match action {
                    InitAction::StoreScalar { value, .. }
                    | InitAction::CopyAggregate { value, .. } => {
                        Some(canonical_expr(value, types))
                    }
                    InitAction::StoreString { .. } | InitAction::ZeroScalar { .. } => None,
                })
                .collect::<Vec<_>>()
                .join(",")
        ),
        CHirExpr::StatementExpr { value, .. } => format!(
            "statement_expr({})",
            value
                .map(|value| canonical_expr(value, types))
                .unwrap_or_else(|| "void".into())
        ),
        CHirExpr::Comma(left, right, _) => {
            format!(
                "comma({},{})",
                canonical_expr(left, types),
                canonical_expr(right, types)
            )
        }
    }
}

fn canonical_decl(decl: &CHirDecl<'_>, types: &TypeInterner) -> String {
    match decl {
        CHirDecl::Object(object) if object.initializer.is_none() => {
            format!(
                "var({},{})",
                object.name.spelling,
                canonical_type(object.ty, types)
            )
        }
        CHirDecl::Object(object) => format!(
            "var({},{},init:{})",
            object.name.spelling,
            canonical_type(object.ty, types),
            canonical_expr(
                object
                    .initializer
                    .as_ref()
                    .expect("matched initialized object")
                    .written()
                    .expression()
                    .expect("matrix uses expression initializers"),
                types
            )
        ),
        CHirDecl::Function(function) if function.body.is_none() => {
            let parameters = match &function.parameters {
                ParameterSyntax::Unspecified => Vec::new(),
                ParameterSyntax::Prototype { parameters, .. }
                | ParameterSyntax::IdentifierList { parameters } => parameters
                    .iter()
                    .map(|parameter| {
                        format!(
                            "{}:{}",
                            parameter
                                .name
                                .as_ref()
                                .map_or("", |name| name.spelling.as_str()),
                            canonical_type(parameter.ty, types)
                        )
                    })
                    .collect::<Vec<_>>(),
            };
            format!(
                "fn({},[{}],{})",
                function.name.spelling,
                parameters.join(","),
                canonical_type(function.return_specifier.type_id(), types)
            )
        }
        other => format!("unexpected:{other:?}"),
    }
}

fn canonical_type(ty: TypeId, types: &TypeInterner) -> String {
    let canonical = types.get(ty).expect("type ID must resolve in its unit");
    assert_eq!(canonical.qualifiers, Default::default());
    match &canonical.kind {
        TypeKind::Void => "void".into(),
        TypeKind::BuiltinVaList => "builtin-va-list".into(),
        TypeKind::GnuInt128(signedness) => format!("gnu-int128:{signedness:?}"),
        TypeKind::Int(rank) => match rank {
            IntRank::Char => "char".into(),
            IntRank::Short => "short".into(),
            IntRank::Int => "int".into(),
            IntRank::Long => "long".into(),
            other => format!("int:{other:?}"),
        },
        TypeKind::Float(kind) => format!("float:{kind:?}"),
        TypeKind::Complex(kind) => format!("complex:{kind:?}"),
        TypeKind::Pointer(inner) => format!("ptr({})", canonical_type(*inner, types)),
        TypeKind::Array { element, bound } => {
            let bound = match bound {
                ArrayBound::Fixed(value) => format!("fixed:{value}"),
                ArrayBound::Variable => "variable".into(),
                ArrayBound::Unbounded => "unbounded".into(),
            };
            format!("array({bound},{})", canonical_type(*element, types))
        }
        TypeKind::Function {
            return_type,
            parameters,
        } => {
            let params = match parameters {
                FunctionParameters::Unspecified => Vec::new(),
                FunctionParameters::Prototype { types: params, .. } => params
                    .iter()
                    .map(|parameter| canonical_type(*parameter, types))
                    .collect(),
            };
            format!(
                "fn([{}])->{}",
                params.join(","),
                canonical_type(*return_type, types)
            )
        }
        TypeKind::Struct(tag) => format!("struct#{}", tag.raw()),
        TypeKind::Union(tag) => format!("union#{}", tag.raw()),
        TypeKind::Enum(tag) => format!("enum#{}", tag.raw()),
        TypeKind::TargetBuiltin(kind) => format!("builtin({kind:?})"),
    }
}

fn unary_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "neg",
        UnaryOp::Not => "not",
        UnaryOp::BitNot => "bit_not",
        UnaryOp::AddrOf => "addr",
        UnaryOp::Deref => "deref",
        UnaryOp::Plus => "plus",
        UnaryOp::Abs => "abs",
    }
}

fn update_name(op: UpdateOp) -> &'static str {
    match op {
        UpdateOp::PreIncrement => "pre_inc",
        UpdateOp::PreDecrement => "pre_dec",
        UpdateOp::PostIncrement => "post_inc",
        UpdateOp::PostDecrement => "post_dec",
    }
}

fn assignment_name(op: AssignmentOp) -> &'static str {
    match op {
        AssignmentOp::Simple => "assign",
        AssignmentOp::Add => "add_assign",
        AssignmentOp::Subtract => "sub_assign",
        AssignmentOp::Multiply => "mul_assign",
        AssignmentOp::Divide => "div_assign",
        AssignmentOp::Remainder => "mod_assign",
        AssignmentOp::ShiftLeft => "shl_assign",
        AssignmentOp::ShiftRight => "shr_assign",
        AssignmentOp::BitAnd => "bit_and_assign",
        AssignmentOp::BitOr => "bit_or_assign",
        AssignmentOp::BitXor => "bit_xor_assign",
    }
}

fn binary_name(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "add",
        BinaryOp::Sub => "sub",
        BinaryOp::Mul => "mul",
        BinaryOp::Div => "div",
        BinaryOp::Mod => "mod",
        BinaryOp::Lt => "lt",
        BinaryOp::Le => "le",
        BinaryOp::Gt => "gt",
        BinaryOp::Ge => "ge",
        BinaryOp::Eq => "eq",
        BinaryOp::Ne => "ne",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::BitAnd => "bit_and",
        BinaryOp::BitOr => "bit_or",
        BinaryOp::BitXor => "bit_xor",
        BinaryOp::Shl => "shl",
        BinaryOp::Shr => "shr",
        BinaryOp::Comma => "comma",
        BinaryOp::Assign => "assign",
        BinaryOp::AddAssign => "add_assign",
        BinaryOp::SubAssign => "sub_assign",
        BinaryOp::MulAssign => "mul_assign",
        BinaryOp::DivAssign => "div_assign",
        BinaryOp::ModAssign => "mod_assign",
        BinaryOp::ShlAssign => "shl_assign",
        BinaryOp::ShrAssign => "shr_assign",
        BinaryOp::BitAndAssign => "bit_and_assign",
        BinaryOp::BitOrAssign => "bit_or_assign",
        BinaryOp::BitXorAssign => "bit_xor_assign",
    }
}

#[test]
fn expression_precedence_and_associativity_matrix() {
    assert_eq!(EXPRESSIONS.len(), 20, "expression denominator changed");
    let mut ids = HashSet::new();
    let mut sources = HashSet::new();

    for case in EXPRESSIONS {
        assert!(ids.insert(case.id), "duplicate case id {}", case.id);
        assert!(
            sources.insert(case.expression.as_bytes()),
            "duplicate expression bytes for {}",
            case.id
        );
        let source = format!(
            "{EXPRESSION_BINDINGS}int matrix_probe(void) {{ {}; return 0; }}",
            case.expression
        );
        let arena = Arena::default();
        let unit = parse_unit(&source, &arena);
        let CHirDecl::Function(function) = unit.declarations().last().expect("probe declaration")
        else {
            panic!("case {} did not produce one probe function", case.id);
        };
        let CHirStmt::Compound(items, _) = function.body.expect("probe definition") else {
            panic!("case {} probe body was not compound", case.id);
        };
        let Some(CHirBlockItem::Statement(statement)) = items.first() else {
            panic!("case {} has no expression statement", case.id);
        };
        let CHirStmt::Expr(expression, _) = statement else {
            panic!("case {} first statement was not an expression", case.id);
        };
        assert_eq!(
            canonical_expr(expression, unit.types()),
            case.expected,
            "case {}",
            case.id
        );
    }

    println!(
        "PARSER_MATRIX expressions={} unique={}",
        EXPRESSIONS.len(),
        sources.len()
    );
}

#[test]
fn ordinary_declarator_matrix() {
    assert_eq!(DECLARATORS.len(), 9, "declarator denominator changed");
    let mut ids = HashSet::new();
    let mut sources = HashSet::new();

    for case in DECLARATORS {
        assert!(ids.insert(case.id), "duplicate case id {}", case.id);
        assert!(
            sources.insert(case.source.as_bytes()),
            "duplicate declarator bytes for {}",
            case.id
        );
        let arena = Arena::default();
        let unit = parse_unit(case.source, &arena);
        assert_eq!(unit.len(), 1, "case {}: {}", case.id, case.source);
        assert_eq!(
            canonical_decl(unit[0], unit.types()),
            case.expected,
            "case {}",
            case.id
        );
    }

    println!(
        "PARSER_MATRIX declarators={} unique={}",
        DECLARATORS.len(),
        sources.len()
    );
}
