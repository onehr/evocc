//! Parser builtin calls.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    /// One of the ACLE types of [`ACLE_BUILTINS`].
    pub(super) fn acle_type(&mut self, name: &str) -> TypeId {
        let integer =
            |parser: &mut Self, rank| parser.intern(TypeKind::Int(rank), Qualifiers::default());
        let pointer = |parser: &mut Self, pointee: TypeId| {
            parser.intern(TypeKind::Pointer(pointee), Qualifiers::default())
        };
        let qualified = |parser: &mut Self, kind, constant, volatile| {
            parser.intern(
                kind,
                Qualifiers {
                    is_const: constant,
                    is_volatile: volatile,
                    ..Qualifiers::default()
                },
            )
        };
        match name {
            "u8" => integer(self, IntRank::UChar),
            "u16" => integer(self, IntRank::UShort),
            "u32" => integer(self, IntRank::UInt),
            "i32" => integer(self, IntRank::Int),
            "u64" => integer(self, IntRank::ULongLong),
            "f32" => self.intern(TypeKind::Float(FloatKind::F32), Qualifiers::default()),
            "f64" => self.intern(TypeKind::Float(FloatKind::F64), Qualifiers::default()),
            "void" => self.intern(TypeKind::Void, Qualifiers::default()),
            "void*" => {
                let void = self.intern(TypeKind::Void, Qualifiers::default());
                pointer(self, void)
            }
            "const void*" | "const volatile void*" | "volatile void*" => {
                let void = qualified(
                    self,
                    TypeKind::Void,
                    name.starts_with("const"),
                    name.contains("volatile"),
                );
                pointer(self, void)
            }
            "u64*" => {
                let element = integer(self, IntRank::ULongLong);
                pointer(self, element)
            }
            "const u64*" => {
                let element = qualified(self, TypeKind::Int(IntRank::ULongLong), true, false);
                pointer(self, element)
            }
            "const char*" => {
                let element = qualified(self, TypeKind::Int(IntRank::Char), true, false);
                pointer(self, element)
            }
            _ => unreachable!("the ACLE table names known types"),
        }
    }

    /// A call of a Clang ACLE builtin is `E2966`. Inside a function body the
    /// refusal is deferred, so the unused intrinsics of `<arm_acle.h>` keep
    /// their bodies; a builtin that the table does not type is refused at
    /// once (EvoIR 10.1.14).
    pub(super) fn parse_acle_builtin(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let builtin = self.token_text(&keyword).to_owned();
        let Some(&(_, result, parameters)) = ACLE_BUILTINS
            .iter()
            .find(|(name, _, _)| *name == builtin)
            .filter(|_| self.function_owner().is_some())
        else {
            self.report(
                "E2966",
                format!("unsupported-acle-builtin: {builtin}"),
                keyword.span,
            );
            return None;
        };
        self.defer_profile_refusal("E2966", "unsupported-acle-builtin", keyword.span);
        self.advance();
        let (arguments, pack) = self.parse_call_arguments()?;
        let close = self.expect(TokenKind::RParen)?;
        if pack.is_some() || arguments.len() != parameters.len() {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let span = self.merge_span(keyword.span, close.span);
        let return_ty = self.acle_type(result);
        let parameter_types = parameters
            .iter()
            .map(|name| self.acle_type(name))
            .collect::<Vec<_>>();
        self.validate_builtin_argument_types(&arguments, &parameter_types)?;
        let function_ty = self.intern(
            TypeKind::Function {
                return_type: return_ty,
                parameters: FunctionParameters::Prototype {
                    types: parameter_types,
                    variadic: false,
                },
            },
            Qualifiers::default(),
        );
        let symbol =
            self.resolve_builtin_library_function(&builtin, function_ty, return_ty, keyword.span)?;
        let facts = self.facts_for_symbol(symbol);
        let callee = self.alloc_node(CHirExpr::Ident {
            symbol,
            spelling: builtin,
            facts,
            span: keyword.span,
        });
        Some(self.alloc_node(CHirExpr::Call(callee, arguments, span)))
    }

    pub(super) fn parse_builtin_library_call(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let builtin = self.token_text(&keyword).to_owned();
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        let (arguments, pack) = self.parse_call_arguments()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        let span = self.merge_span(keyword.span, close.span);
        if let Some(checked) = evocc_chir::fortify::builtin(&builtin) {
            return self.resolve_checked_builtin(checked, arguments, pack, span, keyword.span);
        }
        let variadic = matches!(
            builtin.as_str(),
            "__builtin_printf" | "__builtin_sprintf" | "__builtin_snprintf"
        );
        let Some(pack) = pack.filter(|_| variadic) else {
            if let Some(pack) = pack {
                self.report("E2950", "invalid-va-arg-pack", pack.span());
                return None;
            }
            return self.resolve_builtin_library_call(&builtin, arguments, span, keyword.span);
        };
        // The pack takes the place of anonymous arguments; resolve the named
        // ones as usual and forward the pack after them.
        let call = self.resolve_builtin_library_call(&builtin, arguments, span, keyword.span)?;
        let CHirExpr::Call(callee, arguments, span) = call else {
            self.report("E2950", "invalid-va-arg-pack", pack.span());
            return None;
        };
        let mut arguments = arguments.clone();
        arguments.push(pack);
        Some(self.alloc_node(CHirExpr::Call(callee, arguments, *span)))
    }

    /// Parse the arguments of a call up to its closing parenthesis. A last
    /// argument `__builtin_va_arg_pack ()` forwards the anonymous arguments
    /// of the enclosing variadic function; it is returned separately.
    pub(super) fn parse_call_arguments(
        &mut self,
    ) -> Option<(
        Vec<&'arena CHirExpr<'arena>>,
        Option<&'arena CHirExpr<'arena>>,
    )> {
        let mut arguments = Vec::new();
        if self.peek_kind() == Some(TokenKind::RParen) {
            return Some((arguments, None));
        }
        loop {
            if let Some(pack) = self.parse_forwarded_pack()? {
                return Some((arguments, Some(pack)));
            }
            arguments.push(self.parse_argument_expression()?);
            if !self.match_kind(TokenKind::Comma) {
                return Some((arguments, None));
            }
        }
    }

    /// `__builtin_va_arg_pack ()` directly before the closing parenthesis of
    /// a call. Any other position reaches the expression parser, which
    /// reports it.
    pub(super) fn parse_forwarded_pack(&mut self) -> Option<Option<&'arena CHirExpr<'arena>>> {
        let forwarded = matches!(
            self.tokens.get(self.pos..self.pos + 4),
            Some([name, open, close, end])
                if name.kind == TokenKind::Identifier
                    && name.text.spelling() == Some("__builtin_va_arg_pack")
                    && open.kind == TokenKind::LParen
                    && close.kind == TokenKind::RParen
                    && end.kind == TokenKind::RParen
        );
        if !forwarded {
            return Some(None);
        }
        let name = self.advance().expect("peeked argument pack");
        self.advance();
        let close = self.advance().expect("peeked argument pack parenthesis");
        let span = self.merge_span(name.span, close.span);
        if !self.in_variadic_function() {
            self.report("E2950", "invalid-va-arg-pack", span);
            return None;
        }
        let void_ty = self.intern(TypeKind::Void, Qualifiers::default());
        Some(Some(self.alloc_node(CHirExpr::BuiltinVarArgPack {
            length: false,
            facts: ExprFacts::value(void_ty),
            span,
        })))
    }

    pub(super) fn in_variadic_function(&self) -> bool {
        matches!(
            self.current_function_parameters,
            Some(ParameterSyntax::Prototype { variadic: true, .. })
        )
    }

    /// `__builtin_va_arg_pack_len ()`, or `__builtin_va_arg_pack ()` in a
    /// position other than the last argument of a call, which is invalid.
    pub(super) fn parse_builtin_va_arg_pack_length(
        &mut self,
        spelling: &str,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if !self.match_kind(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        let span = self.merge_span(keyword.span, close.span);
        if spelling != "__builtin_va_arg_pack_len" || !self.in_variadic_function() {
            self.report("E2950", "invalid-va-arg-pack", span);
            return None;
        }
        let int_ty = self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default());
        Some(self.alloc_node(CHirExpr::BuiltinVarArgPack {
            length: true,
            facts: ExprFacts::value(int_ty),
            span,
        }))
    }

    /// `__builtin___memcpy_chk` and its family. The call keeps the checked
    /// entry point unless constant arguments show that the check cannot
    /// fail. Substitution applies the same rule after it resolves
    /// object-size queries.
    pub(super) fn resolve_checked_builtin(
        &mut self,
        builtin: &'static evocc_chir::fortify::CheckedBuiltin,
        mut arguments: Vec<&'arena CHirExpr<'arena>>,
        pack: Option<&'arena CHirExpr<'arena>>,
        span: SourceSpan,
        designator_span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let parameters: Vec<_> = builtin
            .parameters
            .iter()
            .map(|parameter| self.checked_parameter_type(*parameter))
            .collect();
        let named = parameters.len();
        if arguments.len() < named || (!builtin.variadic && arguments.len() != named) {
            self.report("E2909", "builtin-arity", designator_span);
            return None;
        }
        if let Some(pack) = pack.filter(|_| !builtin.variadic) {
            self.report("E2950", "invalid-va-arg-pack", pack.span());
            return None;
        }
        self.validate_builtin_argument_types(&arguments, &parameters)?;
        self.promote_call_arguments(&mut arguments[named..])?;
        let result = self.checked_parameter_type(builtin.result);
        let plain_parameters = builtin
            .plain_parameters()
            .into_iter()
            .map(|parameter| self.checked_parameter_type(parameter))
            .collect();
        let mut function_type = |types| {
            self.intern(
                TypeKind::Function {
                    return_type: result,
                    parameters: FunctionParameters::Prototype {
                        types,
                        variadic: builtin.variadic,
                    },
                },
                Qualifiers::default(),
            )
        };
        let checked_ty = function_type(parameters);
        let plain_ty = function_type(plain_parameters);
        let checked = self.resolve_builtin_library_function(
            builtin.checked,
            checked_ty,
            result,
            designator_span,
        )?;
        let plain = self.resolve_builtin_library_function(
            builtin.plain,
            plain_ty,
            result,
            designator_span,
        )?;
        let redundant = builtin.check_with_source(
            |index| {
                let ty = match builtin.parameters.get(index)? {
                    evocc_chir::fortify::Param::Size => CIntegerType::ULONG,
                    evocc_chir::fortify::Param::Int => CIntegerType::INT,
                    _ => return None,
                };
                let argument = arguments.get(index)?;
                // A query that substitution can still answer differently, such as
                // the object size of a parameter, is not yet a constant.
                if !settled_constant_operand(argument) {
                    return None;
                }
                let value = self.eval_integer_constant_expression(argument).ok()?;
                u64::try_from(value.cast(ty).raw_bits()).ok()
            },
            builtin.literal_copy_size(&arguments),
        );
        let (symbol, spelling) = if redundant {
            arguments = arguments
                .into_iter()
                .enumerate()
                .filter(|(index, _)| builtin.keeps(*index))
                .map(|(_, argument)| argument)
                .collect();
            (plain, builtin.plain)
        } else {
            (checked, builtin.checked)
        };
        arguments.extend(pack);
        self.note_function_use(symbol, designator_span);
        let facts = self.facts_for_symbol(symbol);
        let callee = self.alloc_node(CHirExpr::Ident {
            symbol,
            spelling: spelling.to_owned(),
            facts,
            span: designator_span,
        });
        Some(self.alloc_node(CHirExpr::Call(callee, arguments, span)))
    }

    pub(super) fn checked_parameter_type(
        &mut self,
        parameter: evocc_chir::fortify::Param,
    ) -> TypeId {
        use evocc_chir::fortify::Param;
        let qualified = |is_const| Qualifiers {
            is_const,
            ..Qualifiers::default()
        };
        let pointer = |parser: &mut Self, kind, is_const| {
            let pointee = parser.intern(kind, qualified(is_const));
            parser.intern(TypeKind::Pointer(pointee), Qualifiers::default())
        };
        match parameter {
            Param::Stream => {
                // A header owns FILE's representation. Resolve the file-scope
                // typedef, not a block-local alias that can shadow its name.
                // Before that declaration, the builtin accepts an opaque pointer.
                let file = self
                    .symbols
                    .resolve(self.symbols.file_scope(), "FILE")
                    .ok()
                    .flatten()
                    .and_then(|id| self.symbols.symbol(id).ok())
                    .filter(|symbol| symbol.kind() == SymbolKind::Typedef)
                    .map(|symbol| symbol.ty());
                match file {
                    Some(ty) => self.intern(TypeKind::Pointer(ty), Qualifiers::default()),
                    None => pointer(self, TypeKind::Void, false),
                }
            }
            Param::Pointer => pointer(self, TypeKind::Void, false),
            Param::ConstPointer => pointer(self, TypeKind::Void, true),
            Param::Chars => pointer(self, TypeKind::Int(IntRank::Char), false),
            Param::ConstChars => pointer(self, TypeKind::Int(IntRank::Char), true),
            Param::Size => self.intern(TypeKind::Int(IntRank::ULong), Qualifiers::default()),
            Param::Int => self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default()),
            Param::VaList => self.intern(TypeKind::BuiltinVaList, Qualifiers::default()),
        }
    }

    pub(super) fn resolve_builtin_library_call(
        &mut self,
        builtin: &str,
        mut arguments: Vec<&'arena CHirExpr<'arena>>,
        span: SourceSpan,
        designator_span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if floating::is_builtin(builtin) {
            return self.resolve_floating_builtin(builtin, arguments, span);
        }
        let literal_arity = match builtin {
            "__builtin_strlen" => 1,
            "__builtin_strcmp" => 2,
            _ => 0,
        };
        if literal_arity != 0
            && arguments.len() == literal_arity
            && arguments.iter().all(|argument| {
                matches!(argument, CHirExpr::StringLit(literal, _)
                    if matches!(literal.prefix, StringPrefix::None | StringPrefix::U8))
            })
        {
            // Library string operations stop at NUL. The literal's complete
            // array extent (used by sizeof/initialization) must not change.
            let strings = arguments
                .iter()
                .map(|argument| {
                    let CHirExpr::StringLit(literal, _) = argument else {
                        unreachable!("literal operands checked above")
                    };
                    literal
                        .decoded_units()
                        .map(|units| units.into_iter().take_while(|unit| *unit != 0).collect())
                        .or_else(|| {
                            self.report("E1006", "invalid-string-literal", argument.span());
                            None
                        })
                })
                .collect::<Option<Vec<Vec<u32>>>>()?;
            if builtin == "__builtin_strlen" {
                return Some(self.size_literal(strings[0].len() as u64, span));
            }
            let order = strings[0].cmp(&strings[1]);
            let magnitude = self.alloc_node(CHirExpr::Lit(
                IntLit {
                    value: u64::from(!order.is_eq()),
                    suffix: IntSuffix::None,
                    base: IntBase::Dec,
                },
                span,
            ));
            return Some(if order.is_lt() {
                self.alloc_node(CHirExpr::Unary(UnaryOp::Neg, magnitude, span))
            } else {
                magnitude
            });
        }

        let (external_name, return_ty, parameter_types) =
            self.builtin_library_prototype(builtin, designator_span)?;
        let variable_prefetch_arity = builtin == "__builtin_prefetch";
        let variadic_library = matches!(
            builtin,
            "__builtin_printf" | "__builtin_sprintf" | "__builtin_snprintf"
        );
        let valid_arity = if variable_prefetch_arity {
            (1..=3).contains(&arguments.len())
        } else if variadic_library {
            arguments.len() >= parameter_types.len()
        } else {
            arguments.len() == parameter_types.len()
        };
        if !valid_arity {
            self.report("E2909", "builtin-arity", designator_span);
            return None;
        }
        // A fence admits the six constant orders and any run-time order. A
        // relaxed fence has no effect: only its operand is evaluated. Clang's
        // fences are the GNU ones under another name (EvoIR 10.1.21).
        if matches!(
            builtin,
            "__atomic_thread_fence"
                | "__atomic_signal_fence"
                | "__c11_atomic_thread_fence"
                | "__c11_atomic_signal_fence"
        ) && self.atomic_order_operand(arguments[0], &|_| true)?
            == Some(evocc_chir::AtomicOrder::Relaxed)
        {
            let void_ty = self.intern(TypeKind::Void, Qualifiers::default());
            return Some(self.alloc_node(CHirExpr::Cast(
                void_ty,
                arguments[0],
                arguments[0].span(),
            )));
        }
        if variable_prefetch_arity
            && self
                .converted_expression_type(arguments[0])
                .is_some_and(|inferred| match self.operand_type_class(inferred.ty) {
                    OperandTypeClass::Pointer(pointer) => self
                        .types
                        .get(pointer.pointee)
                        .is_some_and(|ty| !matches!(ty.kind, TypeKind::Function { .. })),
                    _ => false,
                })
        {
            // A hint consumes the address, not a qualified access to the
            // object. Normalize only this operand; ordinary calls keep their
            // implicit qualifier checks and the address is evaluated once.
            arguments[0] = self.alloc_node(CHirExpr::Cast(
                parameter_types[0],
                arguments[0],
                arguments[0].span(),
            ));
        }
        let validation_types = if variable_prefetch_arity {
            &parameter_types[..arguments.len()]
        } else {
            parameter_types.as_slice()
        };
        self.validate_builtin_argument_types(&arguments, validation_types)?;
        if builtin == "__builtin_strlen" {
            if let Some((length, dependencies)) = self.constant_string_length(arguments[0]) {
                self.string_constant_uses
                    .extend(dependencies.into_iter().map(|id| (id, span)));
                return Some(self.size_literal(length, span));
            }
        }
        if variable_prefetch_arity {
            self.promote_call_arguments(&mut arguments[1..])?;
        } else if variadic_library {
            self.promote_call_arguments(&mut arguments[parameter_types.len()..])?;
        }
        let function_ty = self.intern(
            TypeKind::Function {
                return_type: return_ty,
                parameters: FunctionParameters::Prototype {
                    types: if variable_prefetch_arity {
                        vec![parameter_types[0]]
                    } else {
                        parameter_types
                    },
                    variadic: variable_prefetch_arity || variadic_library,
                },
            },
            Qualifiers::default(),
        );
        if let Some(suffix) = nan_builtin_suffix(builtin) {
            self.validate_builtin_library_declaration(
                &external_name,
                function_ty,
                designator_span,
            )?;
            if let CHirExpr::StringLit(literal, _) = arguments[0] {
                if let Some(payload) = nan_literal_payload(literal) {
                    return Some(self.alloc_node(CHirExpr::FloatLit(
                        FloatLit {
                            value: evocc_chir::FloatConstant::QuietNaN(payload),
                            suffix,
                        },
                        span,
                    )));
                }
            }
        }
        if builtin == "__builtin_isdigit" {
            self.validate_builtin_library_declaration(
                &external_name,
                function_ty,
                designator_span,
            )?;
            // This GCC builtin is an integer operation, not a dependency on
            // libc's isdigit symbol. The argument appears exactly once.
            let converted = self.alloc_node(CHirExpr::Cast(return_ty, arguments[0], span));
            let unsigned = self.intern(TypeKind::Int(IntRank::UInt), Qualifiers::default());
            let converted = self.alloc_node(CHirExpr::Cast(unsigned, converted, span));
            let forty_eight = self.alloc_node(CHirExpr::Lit(
                IntLit {
                    value: 48,
                    suffix: IntSuffix::U,
                    base: IntBase::Dec,
                },
                span,
            ));
            let nine = self.alloc_node(CHirExpr::Lit(
                IntLit {
                    value: 9,
                    suffix: IntSuffix::U,
                    base: IntBase::Dec,
                },
                span,
            ));
            let offset = self.alloc_node(CHirExpr::Binary(
                BinaryOp::Sub,
                converted,
                forty_eight,
                span,
            ));
            return Some(self.alloc_node(CHirExpr::Binary(BinaryOp::Le, offset, nine, span)));
        }
        if matches!(
            builtin,
            "__builtin_abs" | "__builtin_labs" | "__builtin_llabs"
        ) {
            self.validate_builtin_library_declaration(
                &external_name,
                function_ty,
                designator_span,
            )?;
            return self.resolve_absolute_builtin(return_ty, &arguments, span);
        }
        let symbol = self.resolve_builtin_library_function(
            &external_name,
            function_ty,
            return_ty,
            designator_span,
        )?;
        self.note_function_use(symbol, designator_span);
        let facts = self.facts_for_symbol(symbol);
        let callee = self.alloc_node(CHirExpr::Ident {
            symbol,
            spelling: external_name,
            facts,
            span: designator_span,
        });
        Some(self.alloc_node(CHirExpr::Call(callee, arguments, span)))
    }

    pub(super) fn standard_function_symbol(&self, expression: &CHirExpr<'_>) -> Option<SymbolId> {
        if !self.standard_builtins {
            return None;
        }
        let CHirExpr::Ident { symbol, .. } = expression else {
            return None;
        };
        let state = self.entities.get(symbol)?;
        if state.linkage != EntityLinkage::External {
            return None;
        }
        let declaration = self.symbols.symbol(*symbol).ok()?;
        if declaration.kind() != SymbolKind::Function
            || !self.recognizes_builtin(declaration.spelling())
        {
            return None;
        }
        Some(*symbol)
    }

    pub(super) fn standard_allocation_target(
        &mut self,
        expression: &CHirExpr<'_>,
    ) -> Option<SymbolId> {
        let symbol = self.standard_function_symbol(expression)?;
        let declaration = self.symbols.symbol(symbol).ok()?;
        if declaration.spelling() != "alloca" {
            return None;
        }
        let actual = declaration.ty();
        let (_, return_type, types) =
            self.builtin_library_prototype("__builtin_alloca", expression.span())?;
        let expected = self.intern(
            TypeKind::Function {
                return_type,
                parameters: FunctionParameters::Prototype {
                    types,
                    variadic: false,
                },
            },
            Qualifiers::default(),
        );
        self.types.compatible(actual, expected).then_some(symbol)
    }

    pub(super) fn standard_absolute_type(&mut self, expression: &CHirExpr<'_>) -> Option<TypeId> {
        let symbol = self.standard_function_symbol(expression)?;
        if self.entities.get(&symbol)?.assembly_named {
            return None;
        }
        let declaration = self.symbols.symbol(symbol).ok()?;
        let actual = declaration.ty();
        let rank = match declaration.spelling() {
            "abs" => IntRank::Int,
            "labs" => IntRank::Long,
            "llabs" => IntRank::LongLong,
            _ => return None,
        };
        let ty = self.intern(TypeKind::Int(rank), Qualifiers::default());
        let prototype = self.intern(
            TypeKind::Function {
                return_type: ty,
                parameters: FunctionParameters::Prototype {
                    types: vec![ty],
                    variadic: false,
                },
            },
            Qualifiers::default(),
        );
        self.types.compatible(actual, prototype).then_some(ty)
    }

    pub(super) fn resolve_absolute_builtin(
        &mut self,
        ty: TypeId,
        arguments: &[&'arena CHirExpr<'arena>],
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let [argument] = arguments else {
            self.report("E2909", "builtin-arity", span);
            return None;
        };
        if self.assignment_constraint(AssignmentOp::Simple, ty, argument)
            == UpdateConstraint::Invalid
        {
            self.report("E2913", "builtin-operand-not-convertible", argument.span());
            return None;
        }
        let converted = self.alloc_node(CHirExpr::Cast(ty, argument, span));
        let evaluated = self.alloc_node(CHirExpr::EvaluateOnce(converted, span));
        Some(self.alloc_node(CHirExpr::Unary(UnaryOp::Abs, evaluated, span)))
    }

    pub(super) fn builtin_library_prototype(
        &mut self,
        builtin: &str,
        span: SourceSpan,
    ) -> Option<(String, TypeId, Vec<TypeId>)> {
        let void_ty = self.intern(TypeKind::Void, Qualifiers::default());
        let const_void_ty = self.intern(
            TypeKind::Void,
            Qualifiers {
                is_const: true,
                ..Qualifiers::default()
            },
        );
        let const_char = self.intern(
            TypeKind::Int(IntRank::Char),
            Qualifiers {
                is_const: true,
                ..Qualifiers::default()
            },
        );
        let void_pointer = self.intern(TypeKind::Pointer(void_ty), Qualifiers::default());
        let const_void_pointer =
            self.intern(TypeKind::Pointer(const_void_ty), Qualifiers::default());
        let const_char_pointer = self.intern(TypeKind::Pointer(const_char), Qualifiers::default());
        let char_ty = self.intern(TypeKind::Int(IntRank::Char), Qualifiers::default());
        let char_pointer = self.intern(TypeKind::Pointer(char_ty), Qualifiers::default());
        let size_ty = self.intern(TypeKind::Int(IntRank::ULong), Qualifiers::default());
        let int_ty = self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default());

        let prototype = match builtin {
            "__builtin_abs" => ("abs", int_ty, vec![int_ty]),
            "__builtin_labs" => {
                let ty = self.intern(TypeKind::Int(IntRank::Long), Qualifiers::default());
                ("labs", ty, vec![ty])
            }
            "__builtin_llabs" => {
                let ty = self.intern(TypeKind::Int(IntRank::LongLong), Qualifiers::default());
                ("llabs", ty, vec![ty])
            }
            "__builtin_abort" => ("abort", void_ty, Vec::new()),
            "__builtin_printf" => ("printf", int_ty, vec![const_char_pointer]),
            "__builtin_sprintf" => ("sprintf", int_ty, vec![char_pointer, const_char_pointer]),
            "__builtin_snprintf" => (
                "snprintf",
                int_ty,
                vec![char_pointer, size_ty, const_char_pointer],
            ),
            "__builtin_puts" => {
                if !self.defer_profile_refusal("E2935", "unsupported-library-builtin", span) {
                    return None;
                }
                ("puts", int_ty, vec![const_char_pointer])
            }
            "__builtin_strchr" => ("strchr", char_pointer, vec![const_char_pointer, int_ty]),
            "__builtin_strncpy" => (
                "strncpy",
                char_pointer,
                vec![char_pointer, const_char_pointer, size_ty],
            ),
            "__builtin_strncat" => (
                "strncat",
                char_pointer,
                vec![char_pointer, const_char_pointer, size_ty],
            ),
            "__builtin_strcat" => (
                "strcat",
                char_pointer,
                vec![char_pointer, const_char_pointer],
            ),
            "__builtin_memchr" => (
                "memchr",
                void_pointer,
                vec![const_void_pointer, int_ty, size_ty],
            ),
            "__builtin_malloc" => ("malloc", void_pointer, vec![size_ty]),
            "__builtin_free" => ("free", void_ty, vec![void_pointer]),
            "__builtin_strcpy" => (
                "strcpy",
                char_pointer,
                vec![char_pointer, const_char_pointer],
            ),
            "__builtin_exit" | "__builtin__Exit" => {
                if !self.defer_profile_refusal("E2933", "unsupported-termination-builtin", span) {
                    return None;
                }
                (
                    if builtin == "__builtin_exit" {
                        "exit"
                    } else {
                        "_Exit"
                    },
                    void_ty,
                    vec![int_ty],
                )
            }
            "__builtin_memset" => ("memset", void_pointer, vec![void_pointer, int_ty, size_ty]),
            "__builtin_memcpy" => (
                "memcpy",
                void_pointer,
                vec![void_pointer, const_void_pointer, size_ty],
            ),
            "__builtin_memmove" => (
                "memmove",
                void_pointer,
                vec![void_pointer, const_void_pointer, size_ty],
            ),
            "__builtin_memcmp" => (
                "memcmp",
                int_ty,
                vec![const_void_pointer, const_void_pointer, size_ty],
            ),
            "__builtin_strcmp" => (
                "strcmp",
                int_ty,
                vec![const_char_pointer, const_char_pointer],
            ),
            "__builtin_strncmp" => (
                "strncmp",
                int_ty,
                vec![const_char_pointer, const_char_pointer, size_ty],
            ),
            "__builtin_strlen" => ("strlen", size_ty, vec![const_char_pointer]),
            "__builtin_fabs" | "__builtin_fabsf" | "__builtin_fabsl" => {
                let kind = match builtin {
                    "__builtin_fabsf" => FloatKind::F32,
                    "__builtin_fabsl" => FloatKind::F128,
                    _ => FloatKind::F64,
                };
                let ty = self.intern(TypeKind::Float(kind), Qualifiers::default());
                (builtin.strip_prefix("__builtin_")?, ty, vec![ty])
            }
            "__builtin_nan" | "__builtin_nanf" | "__builtin_nanl" => {
                let kind = nan_builtin_suffix(builtin)?.type_kind();
                let ty = self.intern(TypeKind::Float(kind), Qualifiers::default());
                (
                    builtin.strip_prefix("__builtin_")?,
                    ty,
                    vec![const_char_pointer],
                )
            }
            "__sync_synchronize" => ("__sync_synchronize", void_ty, Vec::new()),
            "__atomic_thread_fence" | "__atomic_signal_fence" => (builtin, void_ty, vec![int_ty]),
            "__c11_atomic_thread_fence" => ("__atomic_thread_fence", void_ty, vec![int_ty]),
            "__c11_atomic_signal_fence" => ("__atomic_signal_fence", void_ty, vec![int_ty]),
            "__builtin___clear_cache" => {
                ("__clear_cache", void_ty, vec![void_pointer, void_pointer])
            }
            "__builtin_prefetch" => (
                "__builtin_prefetch",
                void_ty,
                vec![const_void_pointer, int_ty, int_ty],
            ),
            "__builtin_extract_return_addr" => (
                "__builtin_extract_return_addr",
                void_pointer,
                vec![void_pointer],
            ),
            "__builtin_alloca" => ("alloca", void_pointer, vec![size_ty]),
            "__builtin_isdigit" => ("isdigit", int_ty, vec![int_ty]),
            "__builtin_isalpha" => ("isalpha", int_ty, vec![int_ty]),
            "__builtin_isspace" => ("isspace", int_ty, vec![int_ty]),
            "__builtin_toupper" => ("toupper", int_ty, vec![int_ty]),
            "__builtin_tolower" => ("tolower", int_ty, vec![int_ty]),
            _ => {
                self.report("E1003", "expected-expression", span);
                return None;
            }
        };
        Some((prototype.0.to_owned(), prototype.1, prototype.2))
    }

    pub(super) fn validate_builtin_library_declaration(
        &mut self,
        external_name: &str,
        function_ty: TypeId,
        span: SourceSpan,
    ) -> Option<()> {
        let visible = self
            .symbols
            .resolve(self.current_scope, external_name)
            .expect("the parser current scope is always live");
        if let Some(symbol) = visible {
            let incompatible = self.symbols.symbol(symbol).is_ok_and(|declaration| {
                declaration.kind() == SymbolKind::Function
                    && !self.types.compatible(declaration.ty(), function_ty)
            });
            if incompatible {
                self.report("E2927", "builtin-library-declaration-incompatible", span);
                return None;
            }
        }
        Some(())
    }

    pub(super) fn declare_implicit_called_function(
        &mut self,
        spelling: &str,
        span: SourceSpan,
    ) -> SymbolId {
        self.lint_findings
            .push(evocc_chir::lints::Finding { lint: 2, span });
        let (return_ty, parameters) = if spelling == "alloca" && self.recognizes_builtin(spelling) {
            let (_, return_type, types) = self
                .builtin_library_prototype("__builtin_alloca", span)
                .expect("the stack allocation prototype is defined");
            (
                return_type,
                FunctionParameters::Prototype {
                    types,
                    variadic: false,
                },
            )
        } else {
            (
                self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default()),
                FunctionParameters::Unspecified,
            )
        };
        let function_ty = self.intern(
            TypeKind::Function {
                return_type: return_ty,
                parameters,
            },
            Qualifiers::default(),
        );
        let name = SpannedName {
            spelling: spelling.to_owned(),
            span,
        };
        let symbol = self.declare_symbol(
            &name,
            SymbolData::Function {
                storage: StorageClass::Extern,
            },
            function_ty,
        );
        let parameters = self
            .parameter_syntax_from_function_type(function_ty, span)
            .expect("the implicit declaration has a function type");
        // This is an ordinary declaration with linkage, not a builtin symbol.
        // The enclosing declaration transaction owns it and rolls it back
        // along with other operand-local declarations when necessary.
        self.side_declarations
            .push(self.alloc_node(CHirDecl::Function(FunctionDecl {
                symbol,
                name,
                ty: function_ty,
                return_specifier: TypeSpecifier::Builtin {
                    ty: return_ty,
                    span,
                },
                written: WrittenDeclarator {
                    layers: vec![WrittenDeclaratorLayer::Function {
                        parameters: Some(parameters.clone()),
                        span,
                    }],
                    span,
                },
                parameters,
                storage: StorageClass::Extern,
                asm_label: None,
                asm_label_span: None,
                inline_spans: Vec::new(),
                gnu_extension_spans: Vec::new(),
                attributes: Vec::new(),
                body: None,
                span,
            })));
        symbol
    }

    pub(super) fn resolve_builtin_library_function(
        &mut self,
        external_name: &str,
        function_ty: TypeId,
        return_ty: TypeId,
        span: SourceSpan,
    ) -> Option<SymbolId> {
        self.validate_builtin_library_declaration(external_name, function_ty, span)?;
        if external_name == "strcmp"
            && self
                .symbols
                .resolve(self.current_scope, external_name)
                .expect("the parser current scope is always live")
                .is_some_and(|symbol| {
                    self.entities
                        .get(&symbol)
                        .is_some_and(|entity| entity.linkage == EntityLinkage::Internal)
                        && self
                            .symbols
                            .symbol(symbol)
                            .is_ok_and(|declaration| declaration.kind() == SymbolKind::Function)
                })
        {
            // Nonliteral calls can bind the static body or libc depending on
            // the reference compiler's optimization. Do not guess a binding.
            self.report("E2946", "builtin-local-binding-conflict", span);
            return None;
        }
        let (symbol, introduced) = self
            .symbols
            .declare_builtin_function(external_name, function_ty, span)
            .expect("builtin symbol table fits u32 IDs");
        if !introduced {
            return Some(symbol);
        }

        let name = SpannedName {
            spelling: external_name.to_owned(),
            span,
        };
        let parameters = self
            .parameter_syntax_from_function_type(function_ty, span)
            .expect("a builtin library prototype is a function type");
        self.builtin_declarations
            .push(self.alloc_node(CHirDecl::Function(FunctionDecl {
                symbol,
                name,
                ty: function_ty,
                return_specifier: TypeSpecifier::Builtin {
                    ty: return_ty,
                    span,
                },
                written: WrittenDeclarator {
                    layers: vec![WrittenDeclaratorLayer::Function {
                        parameters: Some(parameters.clone()),
                        span,
                    }],
                    span,
                },
                parameters,
                storage: StorageClass::Extern,
                asm_label: None,
                asm_label_span: None,
                inline_spans: Vec::new(),
                gnu_extension_spans: Vec::new(),
                attributes: Vec::new(),
                body: None,
                span,
            })));
        Some(symbol)
    }
}
