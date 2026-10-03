//! Lossless semantic-tree export for the selected Core C-HIR closure.
//!
//! This is an input boundary, not a compilation checker. In particular, no
//! successful export grants a verified-output label. The independent reader
//! must recheck binding, typing, initialization, and the compilation relation.

use std::fmt::Write;

use evocc_common::SourceSpan;
use evocc_types::{TargetDataLayout, TypeId, TypeKind};

use evocc_chir::core_profile::{inspect_core, CoreProfileError, CoreRefusal};
use evocc_chir::initializer::InitAction;
use evocc_chir::integer::{integer_literal_value, CIntegerType};
use evocc_chir::{
    AssignmentOp, BinaryOp, CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, CHirUnit,
    FunctionDecl, ParameterSyntax, StorageClass, SymbolId, UnaryOp, UpdateOp,
};

type Result<T> = std::result::Result<T, CoreProfileError>;

// Arrays have fixed, tag-specific arities. A tiny JSON writer keeps this source
// boundary dependency-free; the independent Lean reader uses its JSON parser.
enum Node {
    Text(String),
    Id(u32),
    Flag(bool),
    Null,
    Array(Vec<Node>),
}

impl Node {
    fn text(value: &str) -> Self {
        Self::Text(value.to_owned())
    }

    fn tagged(tag: &str, fields: impl IntoIterator<Item = Self>) -> Self {
        Self::Array(std::iter::once(Self::text(tag)).chain(fields).collect())
    }

    fn render(&self, output: &mut String) {
        match self {
            Self::Text(value) => {
                output.push('"');
                for character in value.chars() {
                    match character {
                        '"' => output.push_str("\\\""),
                        '\\' => output.push_str("\\\\"),
                        '\u{0}'..='\u{1f}' => {
                            write!(output, "\\u{:04x}", u32::from(character)).unwrap();
                        }
                        other => output.push(other),
                    }
                }
                output.push('"');
            }
            Self::Id(id) => write!(output, "{id}").unwrap(),
            Self::Flag(flag) => output.push_str(if *flag { "true" } else { "false" }),
            Self::Null => output.push_str("null"),
            Self::Array(values) => {
                output.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        output.push(',');
                    }
                    value.render(output);
                }
                output.push(']');
            }
        }
    }
}

fn refusal(reason: CoreRefusal, span: SourceSpan) -> CoreProfileError {
    CoreProfileError {
        reason,
        span: Some(span),
    }
}

fn integer_name(integer: CIntegerType) -> Option<&'static str> {
    Some(match integer {
        CIntegerType::INT => "int",
        CIntegerType::UINT => "uint",
        CIntegerType::LONG => "long",
        CIntegerType::ULONG => "ulong",
        CIntegerType::LONG_LONG => "long_long",
        CIntegerType::ULONG_LONG => "ulong_long",
        _ => return None,
    })
}

fn storage(storage: StorageClass) -> Option<&'static str> {
    Some(match storage {
        StorageClass::None => "none",
        StorageClass::Auto => "auto",
        StorageClass::Register => "register",
        StorageClass::Static => "static",
        StorageClass::Extern => "extern",
        _ => return None,
    })
}

/// Export the complete selected definitions in callee-before-caller order.
/// No source expression is folded, rewritten, or dropped, including dead code.
pub fn export_core(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    entries: &[&str],
) -> Result<String> {
    let inventory = inspect_core(unit, target, entries)?;
    let exporter = Exporter { unit };
    let definitions = inventory
        .functions
        .iter()
        .map(|function| exporter.function(exporter.definition(function.symbol)?))
        .collect::<Result<Vec<_>>>()?;
    let entry_ids = entries
        .iter()
        .map(|entry| {
            inventory
                .functions
                .iter()
                .find(|function| function.name == *entry)
                .map(|function| Node::Id(function.symbol.raw()))
                .ok_or(CoreProfileError {
                    reason: CoreRefusal::MissingEntry,
                    span: None,
                })
        })
        .collect::<Result<Vec<_>>>()?;
    let root = Node::tagged(
        "core-v1",
        [
            Node::text("aarch64-unknown-linux-gnu"),
            Node::text("checked"),
            Node::Array(entry_ids),
            Node::Array(definitions),
        ],
    );
    let mut output = String::new();
    root.render(&mut output);
    output.push('\n');
    Ok(output)
}

struct Exporter<'u, 'a> {
    unit: &'u CHirUnit<'a>,
}

impl<'u, 'a> Exporter<'u, 'a> {
    fn definition(&self, symbol: SymbolId) -> Result<&'u FunctionDecl<'a>> {
        // Do not reconstruct a prototype from a later definition after an
        // unprototyped declaration has already affected source call typing.
        for declaration in self.unit.declarations() {
            if let CHirDecl::Function(function) = declaration {
                if function.symbol == symbol
                    && !matches!(
                        function.parameters,
                        ParameterSyntax::Prototype {
                            variadic: false,
                            ..
                        }
                    )
                {
                    return Err(refusal(CoreRefusal::Signature, function.span));
                }
            }
        }
        self.unit
            .declarations()
            .iter()
            .find_map(|declaration| match declaration {
                CHirDecl::Function(function)
                    if function.symbol == symbol && function.body.is_some() =>
                {
                    Some(function)
                }
                _ => None,
            })
            .ok_or(CoreProfileError {
                reason: CoreRefusal::ExternalCall,
                span: None,
            })
    }

    fn qualified_type(&self, ty: TypeId, span: SourceSpan) -> Result<Node> {
        let canonical = self
            .unit
            .types()
            .get(ty)
            .ok_or(refusal(CoreRefusal::Type, span))?;
        let TypeKind::Int(rank) = canonical.kind else {
            return Err(refusal(CoreRefusal::Type, span));
        };
        let name = integer_name(evocc_chir::integer::integer_type_for_rank(rank))
            .ok_or(refusal(CoreRefusal::Type, span))?;
        if canonical.qualifiers.is_volatile
            || canonical.qualifiers.is_atomic
            || canonical.qualifiers.aligned.is_some()
        {
            return Err(refusal(CoreRefusal::Type, span));
        }
        Ok(Node::Array(vec![
            Node::text(name),
            Node::Flag(canonical.qualifiers.is_const),
        ]))
    }

    fn expression(&self, expression: &CHirExpr<'_>) -> Result<Node> {
        let span = expression.span();
        let integer = evocc_chir::constant::integer_expression_type(
            expression,
            self.unit.types(),
            self.unit.symbols(),
        )
        .ok_or(refusal(CoreRefusal::Type, span))?;
        let ty = Node::text(integer_name(integer).ok_or(refusal(CoreRefusal::Type, span))?);
        Ok(match expression {
            CHirExpr::Lit(literal, _) => {
                let value = integer_literal_value(*literal)
                    .map_err(|_| refusal(CoreRefusal::Type, span))?;
                if value.ty() != integer {
                    return Err(refusal(CoreRefusal::Type, span));
                }
                Node::tagged(
                    "lit",
                    [
                        ty,
                        Node::Text(format!(
                            "{:0width$x}",
                            value.raw_bits(),
                            width = usize::from(integer.bits() / 4)
                        )),
                    ],
                )
            }
            CHirExpr::Ident { symbol, .. } => Node::tagged("ident", [ty, Node::Id(symbol.raw())]),
            CHirExpr::Unary(operator, operand, _) => {
                let name = match operator {
                    UnaryOp::Plus => "plus",
                    UnaryOp::Neg => "neg",
                    UnaryOp::BitNot => "bit_not",
                    UnaryOp::Not => "not",
                    _ => return Err(refusal(CoreRefusal::Expression, span)),
                };
                Node::tagged("unary", [ty, Node::text(name), self.expression(operand)?])
            }
            CHirExpr::Binary(operator, left, right, _) => Node::tagged(
                "binary",
                [
                    ty,
                    Node::text(
                        binary_name(*operator).ok_or(refusal(CoreRefusal::Expression, span))?,
                    ),
                    self.expression(left)?,
                    self.expression(right)?,
                ],
            ),
            CHirExpr::Ternary(condition, yes, no, _) => Node::tagged(
                "conditional",
                [
                    ty,
                    self.expression(condition)?,
                    self.expression(yes)?,
                    self.expression(no)?,
                ],
            ),
            CHirExpr::Cast(destination, operand, _) => Node::tagged(
                "cast",
                [
                    ty,
                    self.qualified_type(*destination, span)?,
                    self.expression(operand)?,
                ],
            ),
            CHirExpr::Comma(left, right, _) => Node::tagged(
                "comma",
                [ty, self.expression(left)?, self.expression(right)?],
            ),
            CHirExpr::Assign {
                op,
                place,
                target,
                value,
                ..
            } => Node::tagged(
                "assign",
                [
                    ty,
                    Node::text(assignment_name(*op)),
                    Node::Id(place.symbol().raw()),
                    self.qualified_type(place.ty(), span)?,
                    self.expression(target)?,
                    self.expression(value)?,
                ],
            ),
            CHirExpr::Update {
                op, place, target, ..
            } => Node::tagged(
                "update",
                [
                    ty,
                    Node::text(match op {
                        UpdateOp::PreIncrement => "pre_inc",
                        UpdateOp::PreDecrement => "pre_dec",
                        UpdateOp::PostIncrement => "post_inc",
                        UpdateOp::PostDecrement => "post_dec",
                    }),
                    Node::Id(place.symbol().raw()),
                    self.qualified_type(place.ty(), span)?,
                    self.expression(target)?,
                ],
            ),
            CHirExpr::Call(callee, arguments, _) => {
                let CHirExpr::Ident { symbol, .. } = callee else {
                    return Err(refusal(CoreRefusal::ExternalCall, span));
                };
                let definition = self.definition(*symbol)?;
                let parameters = definition.parameters.parameters();
                if parameters.len() != arguments.len() {
                    return Err(refusal(CoreRefusal::Signature, span));
                }
                let arguments = parameters
                    .iter()
                    .zip(arguments)
                    .map(|(parameter, argument)| {
                        Ok(Node::tagged(
                            "argument",
                            [
                                self.qualified_type(parameter.ty, parameter.span)?,
                                self.expression(argument)?,
                            ],
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Node::tagged("call", [ty, Node::Id(symbol.raw()), Node::Array(arguments)])
            }
            _ => return Err(refusal(CoreRefusal::Expression, span)),
        })
    }

    fn object(&self, declaration: &CHirDecl<'_>) -> Result<Node> {
        let CHirDecl::Object(object) = declaration else {
            return Err(CoreProfileError {
                reason: CoreRefusal::Statement,
                span: None,
            });
        };
        let initializer = match &object.initializer {
            None => Node::Null,
            Some(initializer) => {
                let [InitAction::StoreScalar {
                    path,
                    destination_type,
                    value,
                    ..
                }] = initializer.plan().actions()
                else {
                    return Err(refusal(CoreRefusal::Initializer, object.span));
                };
                if !path.is_root() || *destination_type != object.ty {
                    return Err(refusal(CoreRefusal::Initializer, object.span));
                }
                Node::tagged(
                    "store_scalar",
                    [
                        self.qualified_type(*destination_type, object.span)?,
                        self.expression(value)?,
                    ],
                )
            }
        };
        Ok(Node::tagged(
            "object",
            [
                Node::Id(object.symbol.raw()),
                Node::text(&object.name.spelling),
                self.qualified_type(object.ty, object.span)?,
                Node::text(
                    storage(object.storage)
                        .ok_or(refusal(CoreRefusal::NonlocalObject, object.span))?,
                ),
                initializer,
            ],
        ))
    }

    fn optional_expression(&self, expression: Option<&CHirExpr<'_>>) -> Result<Node> {
        expression.map_or(Ok(Node::Null), |value| self.expression(value))
    }

    fn statement(&self, statement: &CHirStmt<'_>, return_type: TypeId) -> Result<Node> {
        Ok(match statement {
            CHirStmt::Null(_) => Node::tagged("null", []),
            CHirStmt::Break(_) => Node::tagged("break", []),
            CHirStmt::Continue(_) => Node::tagged("continue", []),
            CHirStmt::Expr(expression, _) => Node::tagged("expr", [self.expression(expression)?]),
            CHirStmt::Return(Some(expression), span) => Node::tagged(
                "return",
                [
                    self.qualified_type(return_type, *span)?,
                    self.expression(expression)?,
                ],
            ),
            CHirStmt::Compound(items, _, _) => Node::tagged(
                "compound",
                [Node::Array(
                    items
                        .iter()
                        .map(|item| match item {
                            CHirBlockItem::Declaration(declaration) => self.object(declaration),
                            CHirBlockItem::Statement(statement) => {
                                self.statement(statement, return_type)
                            }
                        })
                        .collect::<Result<Vec<_>>>()?,
                )],
            ),
            CHirStmt::If(condition, yes, no, _) => Node::tagged(
                "if",
                [
                    self.expression(condition)?,
                    self.statement(yes, return_type)?,
                    no.map_or(Ok(Node::Null), |statement| {
                        self.statement(statement, return_type)
                    })?,
                ],
            ),
            CHirStmt::While(condition, body, _) => Node::tagged(
                "while",
                [
                    self.expression(condition)?,
                    self.statement(body, return_type)?,
                ],
            ),
            CHirStmt::DoWhile(body, condition, _) => Node::tagged(
                "do_while",
                [
                    self.statement(body, return_type)?,
                    self.expression(condition)?,
                ],
            ),
            CHirStmt::For(init, condition, increment, body, _) => {
                let init = match init {
                    CHirForInit::Empty => Node::Null,
                    CHirForInit::Expression(expression) => {
                        Node::tagged("expr", [self.expression(expression)?])
                    }
                    CHirForInit::Declaration(declarations) => Node::tagged(
                        "declarations",
                        [Node::Array(
                            declarations
                                .iter()
                                .map(|declaration| self.object(declaration))
                                .collect::<Result<Vec<_>>>()?,
                        )],
                    ),
                };
                Node::tagged(
                    "for",
                    [
                        init,
                        self.optional_expression(*condition)?,
                        self.optional_expression(*increment)?,
                        self.statement(body, return_type)?,
                    ],
                )
            }
            _ => return Err(refusal(CoreRefusal::Statement, statement.span())),
        })
    }

    fn function(&self, function: &FunctionDecl<'_>) -> Result<Node> {
        let Some(TypeKind::Function { return_type, .. }) =
            self.unit.types().get(function.ty).map(|ty| &ty.kind)
        else {
            return Err(refusal(CoreRefusal::Signature, function.span));
        };
        let ParameterSyntax::Prototype {
            parameters,
            variadic: false,
        } = &function.parameters
        else {
            return Err(refusal(CoreRefusal::Signature, function.span));
        };
        let parameters = parameters
            .iter()
            .map(|parameter| {
                Ok(Node::tagged(
                    "parameter",
                    [
                        Node::Id(
                            parameter
                                .symbol
                                .ok_or(refusal(CoreRefusal::Signature, parameter.span))?
                                .raw(),
                        ),
                        self.qualified_type(parameter.ty, parameter.span)?,
                        Node::text(
                            storage(parameter.storage)
                                .ok_or(refusal(CoreRefusal::Signature, parameter.span))?,
                        ),
                    ],
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Node::tagged(
            "function",
            [
                Node::Id(function.symbol.raw()),
                Node::text(&function.name.spelling),
                self.qualified_type(*return_type, function.span)?,
                Node::Flag(function.has_internal_linkage(self.unit.symbols())),
                Node::Array(parameters),
                self.statement(
                    function
                        .body
                        .ok_or(refusal(CoreRefusal::ExternalCall, function.span))?,
                    *return_type,
                )?,
            ],
        ))
    }
}

fn binary_name(operator: BinaryOp) -> Option<&'static str> {
    Some(match operator {
        BinaryOp::Add => "add",
        BinaryOp::Sub => "sub",
        BinaryOp::Mul => "mul",
        BinaryOp::Div => "div",
        BinaryOp::Mod => "rem",
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
        _ => return None,
    })
}

fn assignment_name(operator: AssignmentOp) -> &'static str {
    match operator {
        AssignmentOp::Simple => "set",
        AssignmentOp::Add => "add",
        AssignmentOp::Subtract => "sub",
        AssignmentOp::Multiply => "mul",
        AssignmentOp::Divide => "div",
        AssignmentOp::Remainder => "rem",
        AssignmentOp::ShiftLeft => "shl",
        AssignmentOp::ShiftRight => "shr",
        AssignmentOp::BitAnd => "bit_and",
        AssignmentOp::BitOr => "bit_or",
        AssignmentOp::BitXor => "bit_xor",
    }
}
