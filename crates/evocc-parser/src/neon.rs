//! AArch64 target-header declarations. Builtin prototypes are independent of
//! the C wrapper being parsed and participate in ordinary call type checking.

use super::*;
use evocc_types::neon::{self, BuiltinType};

impl<'src, 'arena> Parser<'src, 'arena> {
    /// Bit casts keep the object image. The destination is a type operand,
    /// so it must be parsed before the value operand like `convertvector`.
    pub(super) fn parse_neon_bit_cast(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        self.expect(TokenKind::LParen)?;
        let (ty, _) = self.parse_type_name()?;
        self.expect(TokenKind::Comma)?;
        let value = self.parse_argument_expression()?;
        let close = self.expect(TokenKind::RParen)?;
        let span = self.merge_span(keyword.span, close.span);
        let source = self.converted_expression_type(value)?.ty;
        let source = self.materialize_inferred_type(source, Some(value))?;
        if ![source, ty].into_iter().all(|ty| {
            matches!(
                self.types.get(ty).map(|ty| &ty.kind),
                Some(
                    TypeKind::Int(_)
                        | TypeKind::GnuInt128(_)
                        | TypeKind::Float(_)
                        | TypeKind::TargetBuiltin(_)
                        | TypeKind::Vector { .. }
                )
            )
        }) {
            self.report("E2102", "unsupported-bit-cast-type", span);
            return None;
        }
        let size = |ty| {
            self.target_layout
                .layout_of(&self.types, ty)
                .ok()
                .map(|layout| layout.size_bytes)
        };
        if size(source).is_none_or(|bytes| bytes > 16) || size(source) != size(ty) {
            self.report("E2102", "bit-cast-size-mismatch", span);
            return None;
        }
        Some(self.vector_node(
            evocc_chir::vector::VectorOp::Reinterpret,
            vec![value],
            ty,
            span,
        ))
    }

    pub(super) fn validate_neon_call(
        &mut self,
        callee: &CHirExpr<'arena>,
        arguments: &[&CHirExpr<'arena>],
        parameters: &FunctionParameters,
    ) -> Option<()> {
        let CHirExpr::Ident {
            symbol, spelling, ..
        } = callee
        else {
            return Some(());
        };
        if !self.symbols.is_builtin_function(*symbol)
            || (neon::signature(spelling).is_none()
                && (!self.target_layout.names_clang_extensions()
                    || neon::apple_signature(spelling).is_none()))
        {
            return Some(());
        }
        let FunctionParameters::Prototype {
            types,
            variadic: false,
        } = parameters
        else {
            return None;
        };
        if arguments.len() != types.len() {
            self.report("E2909", "builtin-arity", callee.span());
            return None;
        }
        self.validate_builtin_argument_types(arguments, types)
    }

    pub(super) fn validate_builtin_argument_types(
        &mut self,
        arguments: &[&CHirExpr<'arena>],
        parameters: &[TypeId],
    ) -> Option<()> {
        if let Some(invalid) = arguments
            .iter()
            .zip(parameters)
            .find_map(|(argument, parameter)| {
                (self.assignment_constraint(AssignmentOp::Simple, *parameter, argument)
                    == UpdateConstraint::Invalid)
                    .then_some(*argument)
            })
        {
            self.report("E2913", "builtin-operand-not-convertible", invalid.span());
            return None;
        }
        Some(())
    }

    pub(super) fn consume_neon_header_marker(&mut self) -> bool {
        if !self
            .peek()
            .is_some_and(evocc_cpp::is_arm_neon_header_marker)
        {
            return false;
        }
        let span = self.advance().expect("peeked target-header marker").span;
        for &(public, builtin) in neon::TUPLE_BASES {
            let kind = BuiltinType::from_spelling(builtin)
                .expect("known vector")
                .kind(&mut self.types);
            let element = self.intern(kind, Qualifiers::default());
            for count in 2..=4 {
                let name = SpannedName {
                    spelling: format!("{public}x{count}_t"),
                    span,
                };
                if self.neon_tuple_types.contains_key(&name.spelling) {
                    continue;
                }
                let array = self.intern(
                    TypeKind::Array {
                        element,
                        bound: ArrayBound::Fixed(count),
                    },
                    Qualifiers::default(),
                );
                let tag = self.types.declare_tag(TagKind::Struct, None);
                self.types
                    .complete_tag(
                        tag,
                        TagDefinition::Struct(vec![RecordField {
                            name: Some("val".into()),
                            ty: array,
                            layout: FieldLayoutAttributes::default(),
                            bit_field: None,
                        }]),
                    )
                    .expect("fresh target-header tuple tag");
                let ty = self.intern(TypeKind::Struct(tag), Qualifiers::default());
                self.neon_tuple_types.insert(name.spelling.clone(), ty);
                let symbol = self.declare_symbol(&name, SymbolData::Typedef, ty);
                self.builtin_declarations
                    .push(self.alloc_node(CHirDecl::Typedef(TypedefDecl {
                        symbol,
                        name,
                        ty,
                        specifier: TypeSpecifier::Builtin { ty, span },
                        written: WrittenDeclarator {
                            layers: Vec::new(),
                            span,
                        },
                        gnu_extension_spans: Vec::new(),
                        attributes: Vec::new(),
                        span,
                    })));
            }
        }
        true
    }

    pub(super) fn resolve_neon_builtin(
        &mut self,
        name: &str,
        span: SourceSpan,
    ) -> Option<SymbolId> {
        let apple = self
            .target_layout
            .names_clang_extensions()
            .then(|| neon::apple_signature(name))
            .flatten();
        if apple.is_none()
            && !name.starts_with("__builtin_aarch64_")
            && self.neon_tuple_types.is_empty()
        {
            return None;
        }
        let signature = apple.or_else(|| neon::signature(name))?;
        let return_ty = self.neon_signature_type(signature[0])?;
        let parameters = signature[1..]
            .iter()
            .map(|name| self.neon_signature_type(name))
            .collect::<Option<Vec<_>>>()?;
        let function_ty = self.intern(
            TypeKind::Function {
                return_type: return_ty,
                parameters: FunctionParameters::Prototype {
                    types: parameters,
                    variadic: false,
                },
            },
            Qualifiers::default(),
        );
        self.resolve_builtin_library_function(name, function_ty, return_ty, span)
    }

    fn neon_signature_type(&mut self, name: &str) -> Option<TypeId> {
        let kind = if let Some(inner) = name.strip_prefix("vec(").and_then(|s| s.strip_suffix(')'))
        {
            let (lanes, inner) = inner.split_once(',')?;
            let element = self.neon_signature_type(inner)?;
            let family = if matches!(self.types.get(element)?.kind, TypeKind::TargetBuiltin(_)) {
                evocc_types::VectorFamily::ArmNeon
            } else {
                evocc_types::VectorFamily::Gnu
            };
            TypeKind::Vector {
                element,
                lanes: lanes.parse().ok()?,
                family,
            }
        } else if let Some(inner) = name.strip_prefix("ptr(").and_then(|s| s.strip_suffix(')')) {
            TypeKind::Pointer(self.neon_signature_type(inner)?)
        } else if let Some(inner) = name.strip_prefix("const ") {
            let ty = self.neon_signature_type(inner)?;
            return Some(self.qualify_typedef_type(
                ty,
                Qualifiers {
                    is_const: true,
                    ..Qualifiers::default()
                },
            ));
        } else if let Some(builtin) = BuiltinType::from_spelling(name) {
            builtin.kind(&mut self.types)
        } else if let Some(builtin) = TargetBuiltinType::from_spelling(name) {
            TypeKind::TargetBuiltin(builtin)
        } else {
            match name {
                "void" => TypeKind::Void,
                "signed char" => TypeKind::Int(IntRank::SChar),
                "unsigned char" => TypeKind::Int(IntRank::UChar),
                "short int" => TypeKind::Int(IntRank::Short),
                "short" => TypeKind::Int(IntRank::Short),
                "short unsigned int" => TypeKind::Int(IntRank::UShort),
                "unsigned short" => TypeKind::Int(IntRank::UShort),
                "int" => TypeKind::Int(IntRank::Int),
                "unsigned int" => TypeKind::Int(IntRank::UInt),
                "long int" => TypeKind::Int(IntRank::Long),
                "long unsigned int" => TypeKind::Int(IntRank::ULong),
                "long long int" => TypeKind::Int(IntRank::LongLong),
                "long long" => TypeKind::Int(IntRank::LongLong),
                "unsigned long long" => TypeKind::Int(IntRank::ULongLong),
                "unsigned __int128" => TypeKind::GnuInt128(GnuInt128Signedness::Unsigned),
                "float" => TypeKind::Float(FloatKind::F32),
                "double" => TypeKind::Float(FloatKind::F64),
                tuple => {
                    // A local typedef shadow cannot alter a compiler builtin
                    // prototype. The pragma-created nominal type is authority.
                    return self.neon_tuple_types.get(tuple).copied();
                }
            }
        };
        Some(self.intern(kind, Qualifiers::default()))
    }
}
