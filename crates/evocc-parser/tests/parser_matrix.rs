use std::collections::HashSet;

use evocc_chir::{BinaryOp, CHirDecl, CHirExpr, UnaryOp};
use evocc_common::FileId;
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::{ArrayBound, CType, IntRank};

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
        expected: "index(id(items),id(index))",
    },
    ExpressionCase {
        id: "expr.postfix.member",
        expression: "object.member",
        expected: "member(id(object),member)",
    },
    ExpressionCase {
        id: "expr.postfix.pointer_member",
        expression: "pointer->member",
        expected: "ptr_member(id(pointer),member)",
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

fn parse_unit(source: &str) -> Vec<CHirDecl> {
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
    let unit = parser.parse_translation_unit();
    assert!(
        !parser.had_error(),
        "parser reported an error for {source:?}"
    );
    unit
}

fn canonical_expr(expr: &CHirExpr) -> String {
    match expr {
        CHirExpr::Lit(value, _) => format!("int({})", value.value),
        CHirExpr::FloatLit(_, _) => "float".into(),
        CHirExpr::CharLit(_, _) => "char".into(),
        CHirExpr::StringLit(_, _) => "string".into(),
        CHirExpr::Ident(name, _) => format!("id({name})"),
        CHirExpr::Unary(op, operand, _) => {
            format!("unary({},{})", unary_name(*op), canonical_expr(operand))
        }
        CHirExpr::Binary(op, left, right, _) => format!(
            "binary({},{},{})",
            binary_name(*op),
            canonical_expr(left),
            canonical_expr(right)
        ),
        CHirExpr::Ternary(condition, left, right, _) => format!(
            "ternary({},{},{})",
            canonical_expr(condition),
            canonical_expr(left),
            canonical_expr(right)
        ),
        CHirExpr::Call(callee, arguments, _) => format!(
            "call({},[{}])",
            canonical_expr(callee),
            arguments
                .iter()
                .map(canonical_expr)
                .collect::<Vec<_>>()
                .join(",")
        ),
        CHirExpr::Cast(ty, operand, _) => {
            format!("cast({},{})", canonical_type(ty), canonical_expr(operand))
        }
        CHirExpr::SizeOf(ty, _) => format!("sizeof_type({})", canonical_type(ty)),
        CHirExpr::SizeOfExpr(operand, _) => {
            format!("sizeof_expr({})", canonical_expr(operand))
        }
        CHirExpr::AlignOf(ty, _) => format!("alignof({})", canonical_type(ty)),
        CHirExpr::Member(object, member, _) => {
            format!("member({},{member})", canonical_expr(object))
        }
        CHirExpr::PtrMember(object, member, _) => {
            format!("ptr_member({},{member})", canonical_expr(object))
        }
        CHirExpr::Index(object, index, _) => format!(
            "index({},{})",
            canonical_expr(object),
            canonical_expr(index)
        ),
        CHirExpr::Assign(left, right, _) => {
            format!("assign({},{})", canonical_expr(left), canonical_expr(right))
        }
        CHirExpr::CompoundLit(ty, values, _) => format!(
            "compound({},[{}])",
            canonical_type(ty),
            values
                .iter()
                .map(canonical_expr)
                .collect::<Vec<_>>()
                .join(",")
        ),
        CHirExpr::Comma(left, right, _) => {
            format!("comma({},{})", canonical_expr(left), canonical_expr(right))
        }
    }
}

fn canonical_decl(decl: &CHirDecl) -> String {
    match decl {
        CHirDecl::VarDecl(name, ty, None, _) => {
            format!("var({name},{})", canonical_type(ty))
        }
        CHirDecl::VarDecl(name, ty, Some(init), _) => format!(
            "var({name},{},init:{})",
            canonical_type(ty),
            canonical_expr(init)
        ),
        CHirDecl::FuncDecl(name, params, ret, None, _) => format!(
            "fn({name},[{}],{})",
            params
                .iter()
                .map(|(name, ty)| format!("{name}:{}", canonical_type(ty)))
                .collect::<Vec<_>>()
                .join(","),
            canonical_type(ret)
        ),
        other => format!("unexpected:{other:?}"),
    }
}

fn canonical_type(ty: &CType) -> String {
    match ty {
        CType::Void => "void".into(),
        CType::Int(rank) => match rank {
            IntRank::Char => "char".into(),
            IntRank::Short => "short".into(),
            IntRank::Int => "int".into(),
            IntRank::Long => "long".into(),
            other => format!("int:{other:?}"),
        },
        CType::Float(kind) => format!("float:{kind:?}"),
        CType::Complex(kind) => format!("complex:{kind:?}"),
        CType::Pointer(inner, qualifiers) => {
            assert_eq!(*qualifiers, Default::default());
            format!("ptr({})", canonical_type(inner))
        }
        CType::Array(inner, bound) => {
            let bound = match bound {
                ArrayBound::Fixed(value) => format!("fixed:{value}"),
                ArrayBound::Variable => "variable".into(),
                ArrayBound::Unbounded => "unbounded".into(),
            };
            format!("array({bound},{})", canonical_type(inner))
        }
        CType::Function(proto) => format!(
            "fn([{}])->{}",
            proto
                .params
                .iter()
                .map(canonical_type)
                .collect::<Vec<_>>()
                .join(","),
            canonical_type(&proto.ret)
        ),
        CType::Struct(value) => format!("struct:{:?}", value.name),
        CType::Union(value) => format!("union:{:?}", value.name),
        CType::Enum(value) => format!("enum:{:?}", value.name),
        CType::Bitfield(inner, width) => format!("bitfield({width},{})", canonical_type(inner)),
        CType::Typedef(_) => "typedef".into(),
        CType::TypeOf(inner) => format!("typeof({})", canonical_type(inner)),
    }
}

fn unary_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "neg",
        UnaryOp::Not => "not",
        UnaryOp::BitNot => "bit_not",
        UnaryOp::AddrOf => "addr",
        UnaryOp::Deref => "deref",
        UnaryOp::PreInc => "pre_inc",
        UnaryOp::PreDec => "pre_dec",
        UnaryOp::PostInc => "post_inc",
        UnaryOp::PostDec => "post_dec",
        UnaryOp::Plus => "plus",
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
        let source = format!("int sink = {};", case.expression);
        let unit = parse_unit(&source);
        assert_eq!(unit.len(), 1, "case {}: {source}", case.id);
        let CHirDecl::VarDecl(name, _, Some(init), _) = &unit[0] else {
            panic!("case {} did not produce one initialized object", case.id);
        };
        assert_eq!(name, "sink", "case {}", case.id);
        assert_eq!(canonical_expr(init), case.expected, "case {}", case.id);
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
        let unit = parse_unit(case.source);
        assert_eq!(unit.len(), 1, "case {}: {}", case.id, case.source);
        assert_eq!(canonical_decl(&unit[0]), case.expected, "case {}", case.id);
    }

    println!(
        "PARSER_MATRIX declarators={} unique={}",
        DECLARATORS.len(),
        sources.len()
    );
}
