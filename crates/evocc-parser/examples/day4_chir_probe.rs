//! Generic tests-only C-HIR observation probe for the Day-4 generated corpus.
//!
//! Input is one tab-separated record per line: `case-id<TAB>hex(source)`.
//! Output is either:
//!
//! - `OK<TAB>case-id<TAB>hex(fingerprint)<TAB>path-span,...`
//! - `ERR<TAB>case-id<TAB>code<TAB>hex(message)<TAB>file:start:end:line:column`
//!
//! This executable contains no fixture IDs or expected values.  It is merely
//! a stable projection of production C-HIR which an independent oracle can
//! compare against.

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use evocc_chir::{
    AssignmentOp, BinaryOp, CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt,
    ParameterSyntax, SpannedName, StorageClass, TagMembers, TypeSpecifier, UnaryOp, UpdateOp,
    WrittenDeclarator, WrittenDeclaratorLayer,
};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::{ArrayBound, FunctionParameters, IntRank, TagId, TagKind, TypeId, TypeKind};

const FILE_ID: u32 = 73;

struct Projection<'unit, 'arena> {
    unit: &'unit evocc_chir::CHirUnit<'arena>,
    tag_ordinals: HashMap<TagId, usize>,
    spans: Vec<(String, SourceSpan)>,
}

impl<'unit, 'arena> Projection<'unit, 'arena> {
    fn new(unit: &'unit evocc_chir::CHirUnit<'arena>) -> Self {
        let mut tag_ordinals = HashMap::new();
        for declaration in unit.declarations() {
            if let CHirDecl::Tag(tag) = declaration {
                let next = tag_ordinals.len();
                tag_ordinals.entry(tag.tag).or_insert(next);
            }
        }
        Self {
            unit,
            tag_ordinals,
            spans: Vec::new(),
        }
    }

    fn record(&mut self, path: impl Into<String>, span: SourceSpan) {
        self.spans.push((path.into(), span));
    }

    fn tag_label(&self, tag: TagId) -> String {
        let record = self
            .unit
            .types()
            .tag(tag)
            .expect("C-HIR tag belongs to unit");
        let kind = match record.kind {
            TagKind::Struct => "struct",
            TagKind::Union => "union",
            TagKind::Enum => "enum",
        };
        let name = record.name.as_deref().unwrap_or("anon");
        let ordinal = self
            .tag_ordinals
            .get(&tag)
            .copied()
            .expect("tag declaration establishes ordinal");
        format!("{kind}#{ordinal}({name})")
    }

    fn type_fingerprint(&self, ty: TypeId) -> String {
        let canonical = self.unit.types().get(ty).expect("type belongs to unit");
        let unqualified = match &canonical.kind {
            TypeKind::Void => "void".to_owned(),
            TypeKind::BuiltinVaList => "builtin-va-list".to_owned(),
            TypeKind::Int(IntRank::Int) => "int".to_owned(),
            TypeKind::Int(IntRank::UInt) => "uint".to_owned(),
            TypeKind::Int(IntRank::Long) => "long".to_owned(),
            TypeKind::Int(rank) => format!("int:{rank:?}"),
            TypeKind::Float(kind) => format!("float:{kind:?}"),
            TypeKind::Complex(kind) => format!("complex:{kind:?}"),
            TypeKind::Pointer(pointee) => format!("ptr({})", self.type_fingerprint(*pointee)),
            TypeKind::Array { element, bound } => {
                let bound = match bound {
                    ArrayBound::Fixed(value) => format!("Constant({value})"),
                    ArrayBound::Variable => "Variable".to_owned(),
                    ArrayBound::Unbounded => "Unbounded".to_owned(),
                };
                format!("array({bound},{})", self.type_fingerprint(*element))
            }
            TypeKind::Function {
                return_type,
                parameters,
            } => {
                let (parameters, form, variadic) = match parameters {
                    FunctionParameters::Unspecified => (Vec::new(), "unspecified", false),
                    FunctionParameters::Prototype { types, variadic } => (
                        types
                            .iter()
                            .map(|parameter| self.type_fingerprint(*parameter))
                            .collect(),
                        "prototype",
                        *variadic,
                    ),
                };
                format!(
                    "fn([{}],{form},variadic:{variadic})->{}",
                    parameters.join(","),
                    self.type_fingerprint(*return_type)
                )
            }
            TypeKind::Struct(tag) | TypeKind::Union(tag) | TypeKind::Enum(tag) => {
                self.tag_label(*tag)
            }
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

    fn name(&mut self, path: &str, name: &SpannedName) {
        self.record(path, name.span);
    }

    fn specifier(&mut self, path: &str, specifier: &TypeSpecifier) {
        self.record(path, specifier.span());
        if let TypeSpecifier::Tag {
            name: Some(name), ..
        } = specifier
        {
            self.name(&format!("{path}.name"), name);
        }
    }

    fn written(&mut self, path: &str, written: &WrittenDeclarator) {
        self.record(path, written.span);
        for (index, layer) in written.layers.iter().enumerate() {
            let span = match layer {
                WrittenDeclaratorLayer::Pointer { span, .. }
                | WrittenDeclaratorLayer::Array { span, .. }
                | WrittenDeclaratorLayer::Function { span } => *span,
            };
            self.record(format!("{path}.layer[{index}]"), span);
        }
    }

    fn expression(&mut self, path: &str, expression: &CHirExpr<'arena>) -> String {
        self.record(path, expression.span());
        match expression {
            CHirExpr::Lit(value, _) => format!("int({})", value.value),
            CHirExpr::FloatLit(value, _) => format!("float({:?},{:?})", value.value, value.suffix),
            CHirExpr::CharLit(value, _) => format!("char({},{:?})", value.value, value.prefix),
            CHirExpr::StringLit(value, _) => {
                format!("string({:?},{:?})", value.value, value.prefix)
            }
            CHirExpr::Ident { spelling, .. } => format!("id({spelling})"),
            CHirExpr::Unary(operator, child, _) => {
                let operator = unary_name(*operator);
                let child = self.expression(&format!("{path}.operand"), child);
                format!("{operator}({child})")
            }
            CHirExpr::Update {
                op, target: child, ..
            } => {
                let operator = update_name(*op);
                let child = self.expression(&format!("{path}.operand"), child);
                format!("{operator}({child})")
            }
            CHirExpr::Binary(operator, left, right, _) => {
                let operator = binary_name(*operator);
                let left = self.expression(&format!("{path}.left"), left);
                let right = self.expression(&format!("{path}.right"), right);
                format!("{operator}({left},{right})")
            }
            CHirExpr::Ternary(condition, then_value, else_value, _) => {
                let condition = self.expression(&format!("{path}.condition"), condition);
                let then_value = self.expression(&format!("{path}.then"), then_value);
                let else_value = self.expression(&format!("{path}.else"), else_value);
                format!("ternary({condition},{then_value},{else_value})")
            }
            CHirExpr::Call(callee, arguments, _) => {
                let callee = self.expression(&format!("{path}.callee"), callee);
                let arguments = arguments
                    .iter()
                    .enumerate()
                    .map(|(index, argument)| {
                        self.expression(&format!("{path}.argument[{index}]"), argument)
                    })
                    .collect::<Vec<_>>();
                format!("call({callee},[{}])", arguments.join(","))
            }
            CHirExpr::Cast(ty, child, _) => {
                let ty = self.type_fingerprint(*ty);
                let child = self.expression(&format!("{path}.operand"), child);
                format!("cast({ty},{child})")
            }
            CHirExpr::SizeOf(ty, _) => format!("sizeof(type:{})", self.type_fingerprint(*ty)),
            CHirExpr::SizeOfExpr(child, _) => {
                let child = self.expression(&format!("{path}.operand"), child);
                format!("sizeof_expr({child})")
            }
            CHirExpr::AlignOf(ty, _) => format!("alignof(type:{})", self.type_fingerprint(*ty)),
            CHirExpr::Member(object, member, _) => {
                let object = self.expression(&format!("{path}.object"), object);
                format!("member({object},{member})")
            }
            CHirExpr::PtrMember(object, member, _) => {
                let object = self.expression(&format!("{path}.object"), object);
                format!("ptr_member({object},{member})")
            }
            CHirExpr::Index(object, index, _) => {
                let object = self.expression(&format!("{path}.object"), object);
                let index = self.expression(&format!("{path}.index"), index);
                format!("index({object},{index})")
            }
            CHirExpr::Assign {
                op,
                target: left,
                value: right,
                ..
            } => {
                let operator = assignment_name(*op);
                let left = self.expression(&format!("{path}.left"), left);
                let right = self.expression(&format!("{path}.right"), right);
                format!("{operator}({left},{right})")
            }
            CHirExpr::CompoundLit(ty, values, _) => {
                let values = values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| self.expression(&format!("{path}.value[{index}]"), value))
                    .collect::<Vec<_>>();
                format!(
                    "compound_lit({},[{}])",
                    self.type_fingerprint(*ty),
                    values.join(",")
                )
            }
            CHirExpr::Comma(left, right, _) => {
                let left = self.expression(&format!("{path}.left"), left);
                let right = self.expression(&format!("{path}.right"), right);
                format!("comma({left},{right})")
            }
        }
    }

    fn statement(&mut self, path: &str, statement: &CHirStmt<'arena>) -> String {
        self.record(path, statement.span());
        match statement {
            CHirStmt::Expr(expression, _) => {
                format!(
                    "expr({})",
                    self.expression(&format!("{path}.expression"), expression)
                )
            }
            CHirStmt::If(condition, then_branch, else_branch, _) => {
                let condition = self.expression(&format!("{path}.condition"), condition);
                let then_branch = self.statement(&format!("{path}.then"), then_branch);
                let else_branch = else_branch.map_or_else(
                    || "none".to_owned(),
                    |branch| self.statement(&format!("{path}.else"), branch),
                );
                format!("if({condition},{then_branch},{else_branch})")
            }
            CHirStmt::Switch(expression, body, _) => {
                let expression = self.expression(&format!("{path}.expression"), expression);
                let body = self.statement(&format!("{path}.body"), body);
                format!("switch({expression},{body})")
            }
            CHirStmt::While(expression, body, _) => {
                let expression = self.expression(&format!("{path}.condition"), expression);
                let body = self.statement(&format!("{path}.body"), body);
                format!("while({expression},{body})")
            }
            CHirStmt::DoWhile(body, expression, _) => {
                let body = self.statement(&format!("{path}.body"), body);
                let expression = self.expression(&format!("{path}.condition"), expression);
                format!("do({body},{expression})")
            }
            CHirStmt::For(initializer, condition, step, body, _) => {
                let initializer = match initializer {
                    CHirForInit::Empty => "empty".to_owned(),
                    CHirForInit::Expression(expression) => format!(
                        "expr({})",
                        self.expression(&format!("{path}.initializer.expression"), expression)
                    ),
                    CHirForInit::Declaration(declarations) if declarations.len() == 1 => format!(
                        "decl({})",
                        self.declaration(
                            &format!("{path}.initializer.declaration"),
                            declarations[0],
                        )
                    ),
                    CHirForInit::Declaration(declarations) => format!(
                        "decls[{}]",
                        declarations
                            .iter()
                            .enumerate()
                            .map(|(index, declaration)| self.declaration(
                                &format!("{path}.initializer.declaration[{index}]"),
                                declaration,
                            ))
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                };
                let condition = condition.map_or_else(
                    || "none".to_owned(),
                    |expression| self.expression(&format!("{path}.condition"), expression),
                );
                let step = step.map_or_else(
                    || "none".to_owned(),
                    |expression| self.expression(&format!("{path}.step"), expression),
                );
                let body = self.statement(&format!("{path}.body"), body);
                format!("for({initializer},{condition},{step},{body})")
            }
            CHirStmt::Return(value, _) => {
                let value = value.map_or_else(String::new, |expression| {
                    self.expression(&format!("{path}.value"), expression)
                });
                format!("return({value})")
            }
            CHirStmt::Compound(items, _) => {
                let items = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| match item {
                        CHirBlockItem::Declaration(declaration) => format!(
                            "decl({})",
                            self.declaration(
                                &format!("{path}.item[{index}].declaration"),
                                declaration
                            )
                        ),
                        CHirBlockItem::Statement(statement) => {
                            self.statement(&format!("{path}.item[{index}].statement"), statement)
                        }
                    })
                    .collect::<Vec<_>>();
                format!("compound[{}]", items.join(","))
            }
            CHirStmt::Break(_) => "break".to_owned(),
            CHirStmt::Continue(_) => "continue".to_owned(),
            CHirStmt::Goto(label, _) => {
                self.name(&format!("{path}.label"), label);
                format!("goto({})", label.spelling)
            }
            CHirStmt::Label(label, nested, _) => {
                self.name(&format!("{path}.label"), label);
                let nested = self.statement(&format!("{path}.statement"), nested);
                format!("label({},{nested})", label.spelling)
            }
            CHirStmt::Case(value, nested, _) => {
                let value = self.expression(&format!("{path}.value"), value);
                let nested = self.statement(&format!("{path}.statement"), nested);
                format!("case({value},{nested})")
            }
            CHirStmt::Default(nested, _) => {
                let nested = self.statement(&format!("{path}.statement"), nested);
                format!("default({nested})")
            }
            CHirStmt::Null(_) => "null".to_owned(),
        }
    }

    fn parameter_syntax(
        &mut self,
        path: &str,
        parameters: &ParameterSyntax,
    ) -> (String, &'static str, bool, Option<String>) {
        match parameters {
            ParameterSyntax::Unspecified => (String::new(), "unspecified", false, None),
            ParameterSyntax::Prototype {
                parameters,
                variadic,
            } => {
                let mut written_array = None;
                let parameters = parameters
                    .iter()
                    .enumerate()
                    .map(|(index, parameter)| {
                        let parameter_path = format!("{path}.parameter[{index}]");
                        self.record(&parameter_path, parameter.span);
                        if let Some(name) = &parameter.name {
                            self.name(&format!("{parameter_path}.name"), name);
                        }
                        self.specifier(
                            &format!("{parameter_path}.specifier"),
                            &parameter.specifier,
                        );
                        self.written(&format!("{parameter_path}.written"), &parameter.written);
                        for layer in &parameter.written.layers {
                            if let WrittenDeclaratorLayer::Array {
                                bound: ArrayBound::Fixed(value),
                                ..
                            } = layer
                            {
                                written_array = Some(format!("array({value})"));
                            }
                        }
                        format!(
                            "{}:{}",
                            parameter
                                .name
                                .as_ref()
                                .map_or("anon", |name| name.spelling.as_str()),
                            self.type_fingerprint(parameter.ty)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                (parameters, "prototype", *variadic, written_array)
            }
        }
    }

    fn declaration(&mut self, path: &str, declaration: &CHirDecl<'arena>) -> String {
        self.record(path, declaration.span());
        match declaration {
            CHirDecl::Tag(tag) => {
                if let Some(name) = &tag.name {
                    self.name(&format!("{path}.name"), name);
                }
                self.record(format!("{path}.specifier"), tag.specifier_span);
                let (state, members) = match &tag.members {
                    TagMembers::Incomplete => ("forward", Vec::new()),
                    TagMembers::Struct(fields) | TagMembers::Union(fields) => {
                        let fields = fields
                            .iter()
                            .enumerate()
                            .map(|(index, field)| {
                                let field_path = format!("{path}.member[{index}]");
                                self.record(&field_path, field.span);
                                if let Some(name) = &field.name {
                                    self.name(&format!("{field_path}.name"), name);
                                }
                                self.specifier(
                                    &format!("{field_path}.specifier"),
                                    &field.specifier,
                                );
                                self.written(&format!("{field_path}.written"), &field.written);
                                let bit_width = field.bit_width.map(|value| {
                                    self.expression(&format!("{field_path}.bit_width"), value)
                                });
                                format!(
                                    "{}:{}{}",
                                    field
                                        .name
                                        .as_ref()
                                        .map_or("anon", |name| name.spelling.as_str()),
                                    self.type_fingerprint(field.ty),
                                    bit_width.map_or_else(String::new, |value| format!(
                                        ":bits={}",
                                        integer_fingerprint_value(&value)
                                    ))
                                )
                            })
                            .collect();
                        ("definition", fields)
                    }
                    TagMembers::Enum(enumerators) => {
                        let enumerators = enumerators
                            .iter()
                            .enumerate()
                            .map(|(index, enumerator)| {
                                let enumerator_path = format!("{path}.enumerator[{index}]");
                                self.record(&enumerator_path, enumerator.span);
                                self.name(&format!("{enumerator_path}.name"), &enumerator.name);
                                if let Some(value) = enumerator.written_value {
                                    let _ = self.expression(
                                        &format!("{enumerator_path}.written_value"),
                                        value,
                                    );
                                }
                                format!("{}={}", enumerator.name.spelling, enumerator.value)
                            })
                            .collect();
                        ("definition", enumerators)
                    }
                };
                let members = if members.is_empty() {
                    String::new()
                } else {
                    format!(",[{}]", members.join(","))
                };
                format!("tag({},{state}{members})", self.tag_label(tag.tag))
            }
            CHirDecl::Object(object) => {
                self.name(&format!("{path}.name"), &object.name);
                self.specifier(&format!("{path}.specifier"), &object.specifier);
                self.written(&format!("{path}.written"), &object.written);
                let mut suffixes = Vec::new();
                if object.storage != StorageClass::None {
                    suffixes.push(format!("storage:{}", storage_name(object.storage)));
                }
                if let Some(initializer) = object.initializer.as_ref() {
                    suffixes.push(format!(
                        "init={}",
                        self.expression(
                            &format!("{path}.initializer"),
                            initializer
                                .written()
                                .expression()
                                .expect("Day 4 uses expression initializers"),
                        )
                    ));
                }
                let suffix = if suffixes.is_empty() {
                    String::new()
                } else {
                    format!(",{}", suffixes.join(","))
                };
                format!(
                    "object({},{suffix_type}{suffix})",
                    object.name.spelling,
                    suffix_type = self.type_fingerprint(object.ty)
                )
            }
            CHirDecl::Function(function) => {
                self.name(&format!("{path}.name"), &function.name);
                self.specifier(
                    &format!("{path}.return_specifier"),
                    &function.return_specifier,
                );
                self.written(&format!("{path}.written"), &function.written);
                let (parameters, form, variadic, written_array) =
                    self.parameter_syntax(path, &function.parameters);
                let TypeKind::Function { return_type, .. } = &self
                    .unit
                    .types()
                    .get(function.ty)
                    .expect("function type belongs to unit")
                    .kind
                else {
                    panic!("function declaration does not have function type")
                };
                let mut suffixes = Vec::new();
                if let Some(written_array) = written_array {
                    suffixes.push(format!("written:{written_array}"));
                }
                if let Some(body) = function.body {
                    suffixes.push(format!(
                        "body={}",
                        self.statement(&format!("{path}.body"), body)
                    ));
                }
                let suffix = if suffixes.is_empty() {
                    String::new()
                } else {
                    format!(",{}", suffixes.join(","))
                };
                format!(
                    "function({},[{}],{},{form},variadic:{variadic}{suffix})",
                    function.name.spelling,
                    parameters,
                    self.type_fingerprint(*return_type)
                )
            }
            CHirDecl::Typedef(declaration) => {
                self.name(&format!("{path}.name"), &declaration.name);
                self.specifier(&format!("{path}.specifier"), &declaration.specifier);
                self.written(&format!("{path}.written"), &declaration.written);
                format!(
                    "typedef({},{})",
                    declaration.name.spelling,
                    self.type_fingerprint(declaration.ty)
                )
            }
        }
    }

    fn unit_fingerprint(&mut self) -> String {
        let declarations = self
            .unit
            .declarations()
            .iter()
            .enumerate()
            .map(|(index, declaration)| self.declaration(&format!("decl[{index}]"), declaration))
            .collect::<Vec<_>>();
        format!("unit[{}]", declarations.join(";"))
    }
}

fn unary_name(operator: UnaryOp) -> &'static str {
    match operator {
        UnaryOp::Neg => "neg",
        UnaryOp::Not => "not",
        UnaryOp::BitNot => "bit_not",
        UnaryOp::AddrOf => "addr_of",
        UnaryOp::Deref => "deref",
        UnaryOp::Plus => "plus",
    }
}

fn update_name(operator: UpdateOp) -> &'static str {
    match operator {
        UpdateOp::PreIncrement => "pre_inc",
        UpdateOp::PreDecrement => "pre_dec",
        UpdateOp::PostIncrement => "post_inc",
        UpdateOp::PostDecrement => "post_dec",
    }
}

fn assignment_name(operator: AssignmentOp) -> &'static str {
    match operator {
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

fn binary_name(operator: BinaryOp) -> &'static str {
    match operator {
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

fn storage_name(storage: StorageClass) -> &'static str {
    match storage {
        StorageClass::None => "none",
        StorageClass::Typedef => "typedef",
        StorageClass::Extern => "extern",
        StorageClass::Static => "static",
        StorageClass::Auto => "auto",
        StorageClass::Register => "register",
        StorageClass::ThreadLocal => "thread_local",
    }
}

fn integer_fingerprint_value(fingerprint: &str) -> &str {
    fingerprint
        .strip_prefix("int(")
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or(fingerprint)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn hex_decode(encoded: &str) -> Result<Vec<u8>, String> {
    if !encoded.len().is_multiple_of(2) {
        return Err("odd-length hex source".to_owned());
    }
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex digit {byte:?}")),
    }
}

fn serialize_spans(spans: &[(String, SourceSpan)]) -> String {
    spans
        .iter()
        .map(|(path, span)| {
            format!(
                "{}:{}:{}:{}:{}:{}",
                hex_encode(path.as_bytes()),
                span.file.raw(),
                span.byte_start,
                span.byte_end,
                span.line_start,
                span.column_start
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn project(case_id: &str, source: &str) -> String {
    let file = FileId::new(FILE_ID);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return format!(
                "ERR\t{case_id}\tE0001\t{}\t{}:{}:{}:{}:{}",
                hex_encode(b"lexical-error"),
                token.span.file.raw(),
                token.span.byte_start,
                token.span.byte_end,
                token.span.line_start,
                token.span.column_start
            );
        }
        tokens.push(token);
    }
    let arena = Arena::default();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    if parser.had_error() {
        let diagnostic = parser.diagnostic().expect("parser error has diagnostic");
        return format!(
            "ERR\t{case_id}\t{}\t{}\t{}:{}:{}:{}:{}",
            diagnostic.code,
            hex_encode(diagnostic.message.as_bytes()),
            diagnostic.span.file.raw(),
            diagnostic.span.byte_start,
            diagnostic.span.byte_end,
            diagnostic.span.line_start,
            diagnostic.span.column_start
        );
    }
    let mut projection = Projection::new(&unit);
    let fingerprint = projection.unit_fingerprint();
    format!(
        "OK\t{case_id}\t{}\t{}",
        hex_encode(fingerprint.as_bytes()),
        serialize_spans(&projection.spans)
    )
}

fn run() -> Result<(), String> {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("read protocol: {error}"))?;
        let (case_id, source_hex) = line
            .split_once('\t')
            .ok_or_else(|| "input record lacks tab separator".to_owned())?;
        if case_id.is_empty() || case_id.contains(['\t', '\n', '\r']) {
            return Err("invalid case ID".to_owned());
        }
        let source = String::from_utf8(hex_decode(source_hex)?)
            .map_err(|_| "source is not UTF-8".to_owned())?;
        writeln!(stdout, "{}", project(case_id, &source))
            .map_err(|error| format!("write protocol: {error}"))?;
    }
    stdout
        .flush()
        .map_err(|error| format!("flush protocol: {error}"))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("day4 C-HIR probe failed: {error}");
        std::process::exit(2);
    }
}
