//! Parser atomic builtins.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    /// One constant memory order of an atomic builtin, or `None` for a
    /// dynamic order. A constant outside 0..5 or outside `valid` is `E2947`.
    pub(super) fn atomic_order_operand(
        &mut self,
        order: &'arena CHirExpr<'arena>,
        valid: &dyn Fn(evocc_chir::AtomicOrder) -> bool,
    ) -> Option<Option<evocc_chir::AtomicOrder>> {
        let inferred = self.converted_expression_type(order)?;
        if !is_integer_class(self.operand_type_class(inferred.ty)) {
            self.report("E2913", "builtin-operand-not-convertible", order.span());
            return None;
        }
        let Ok(value) = self.eval_integer_constant_expression(order) else {
            return Some(None);
        };
        let chosen = value
            .try_mathematical_value()
            .and_then(evocc_chir::AtomicOrder::from_c)
            .filter(|chosen| valid(*chosen));
        if chosen.is_none() {
            self.report("E2947", "invalid-atomic-memory-order", order.span());
            return None;
        }
        Some(chosen)
    }

    /// The `__atomic` read-modify-write, exchange, compare-exchange,
    /// test-and-set, clear and lock-free builtins (EvoIR 10.1.14).
    pub(super) fn parse_gnu_atomic(
        &mut self,
        keyword: Token,
        form: GnuAtomic,
    ) -> Option<&'arena CHirExpr<'arena>> {
        use evocc_chir::{AtomicOrder as O, AtomicRmwOp as R, AtomicRmwReturn as V};
        if !self.match_kind(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        let mut operands = Vec::new();
        if self.peek_kind() != Some(TokenKind::RParen) {
            loop {
                operands.push(self.parse_argument_expression()?);
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
            }
        }
        let arity = match form {
            GnuAtomic::Generic(
                evocc_chir::AtomicCopyOp::Load | evocc_chir::AtomicCopyOp::Store,
            )
            | GnuAtomic::Update(..) => 3,
            GnuAtomic::Generic(evocc_chir::AtomicCopyOp::Exchange) => 4,
            GnuAtomic::Generic(evocc_chir::AtomicCopyOp::CompareExchange)
            | GnuAtomic::CompareExchange => 6,
            GnuAtomic::TestAndSet
            | GnuAtomic::Clear
            | GnuAtomic::AlwaysLockFree
            | GnuAtomic::IsLockFree => 2,
        };
        if operands.len() != arity || self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance()?;
        let span = self.merge_span(keyword.span, close.span);
        let int_ty = self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default());
        let bool_ty = self.intern(TypeKind::Int(IntRank::Bool), Qualifiers::default());
        if matches!(form, GnuAtomic::AlwaysLockFree | GnuAtomic::IsLockFree) {
            return self.parse_atomic_lock_free(form, operands[0], operands[1], span);
        }
        if let GnuAtomic::Generic(op) = form {
            return self.parse_generic_atomic(op, &operands, span);
        }
        let address = operands[0];
        let inferred = self.converted_expression_type(address)?;
        let OperandTypeClass::Pointer(pointer) = self.operand_type_class(inferred.ty) else {
            self.report("E2913", "builtin-operand-not-convertible", address.span());
            return None;
        };
        let canonical = self.types.get(pointer.pointee)?.clone();
        if canonical.qualifiers.is_const {
            self.report("E2913", "builtin-operand-not-convertible", address.span());
            return None;
        }
        let volatile = canonical.qualifiers.is_volatile;
        let literal = |parser: &Self, value: u64| {
            parser.alloc_node(CHirExpr::Lit(
                IntLit {
                    value,
                    suffix: IntSuffix::None,
                    base: IntBase::Dec,
                },
                span,
            ))
        };
        // Test-and-set and clear act on the first byte of any object.
        if matches!(form, GnuAtomic::TestAndSet | GnuAtomic::Clear) {
            if matches!(canonical.kind, TypeKind::Function { .. }) {
                self.report("E2913", "builtin-operand-not-convertible", address.span());
                return None;
            }
            let byte = self.intern(TypeKind::Int(IntRank::UChar), Qualifiers::default());
            let qualifiers = Qualifiers {
                is_volatile: volatile,
                ..Qualifiers::default()
            };
            let pointee = self.intern(TypeKind::Int(IntRank::UChar), qualifiers);
            let byte_pointer = self.intern(TypeKind::Pointer(pointee), Qualifiers::default());
            let address = self.alloc_node(CHirExpr::Cast(byte_pointer, address, address.span()));
            let set = form == GnuAtomic::TestAndSet;
            let order = operands[1];
            let constant_order = self.atomic_order_operand(order, &|order| {
                set || matches!(order, O::Relaxed | O::Release | O::SeqCst)
            })?;
            let value = literal(self, u64::from(set));
            let value = Some(self.alloc_node(CHirExpr::Cast(byte, value, span)));
            let order = self.alloc_node(CHirExpr::Cast(int_ty, order, order.span()));
            let void_ty = self.intern(TypeKind::Void, Qualifiers::default());
            let access = self.alloc_node(CHirExpr::Atomic(evocc_chir::CHirAtomicAccess {
                address,
                value,
                expected: None,
                rmw: set.then_some((R::Exchange, V::Old)),
                order,
                constant_order,
                access_ty: byte,
                volatile,
                writeback: false,
                weak: None,
                failure: None,
                memory: false,
                result: None,
                facts: ExprFacts::value(if set { byte } else { void_ty }),
                span,
            }));
            // The result is whether the byte was set before.
            return Some(if set {
                self.alloc_node(CHirExpr::Cast(bool_ty, access, span))
            } else {
                access
            });
        }
        let pointer_object = matches!(canonical.kind, TypeKind::Pointer(_));
        let integer = self
            .canonical_integer_type(pointer.pointee)
            .ok()
            .filter(|integer| integer.bits() <= 64);
        let pointee_is_function = match canonical.kind {
            TypeKind::Pointer(target) => self
                .types
                .get(target)
                .is_some_and(|ty| matches!(ty.kind, TypeKind::Function { .. })),
            _ => false,
        };
        let valid_object = match form {
            // Arithmetic needs an integer other than `_Bool`, or an object
            // pointer for add and subtract, which count bytes.
            GnuAtomic::Update(operation, _)
                if !matches!(operation, R::Exchange | R::CompareExchange) =>
            {
                integer.is_some_and(|integer| integer != CIntegerType::BOOL)
                    || (pointer_object
                        && !pointee_is_function
                        && matches!(operation, R::Add | R::Sub))
            }
            _ => integer.is_some() || pointer_object,
        };
        if !valid_object {
            self.report("E2913", "builtin-operand-not-convertible", address.span());
            return None;
        }
        // An `_Atomic` object means its non-atomic type here.
        let access_ty = self.intern(canonical.kind.clone(), Qualifiers::default());
        let pointer_offset =
            pointer_object && matches!(form, GnuAtomic::Update(R::Add | R::Sub, _));
        let value_operand = operands[if form == GnuAtomic::CompareExchange {
            2
        } else {
            1
        }];
        let value = if pointer_offset {
            let class = self.operand_type_class(self.converted_expression_type(value_operand)?.ty);
            if !is_integer_class(class) {
                self.report(
                    "E2913",
                    "builtin-operand-not-convertible",
                    value_operand.span(),
                );
                return None;
            }
            let bytes = self.intern(TypeKind::Int(IntRank::ULong), Qualifiers::default());
            self.alloc_node(CHirExpr::Cast(bytes, value_operand, value_operand.span()))
        } else {
            if !matches!(
                self.assignment_constraint(AssignmentOp::Simple, access_ty, value_operand),
                UpdateConstraint::Scalar
            ) {
                self.report(
                    "E2913",
                    "builtin-operand-not-convertible",
                    value_operand.span(),
                );
                return None;
            }
            self.alloc_node(CHirExpr::Cast(
                access_ty,
                value_operand,
                value_operand.span(),
            ))
        };
        if form == GnuAtomic::CompareExchange {
            let expected = operands[1];
            let inferred = self.converted_expression_type(expected)?;
            let OperandTypeClass::Pointer(target) = self.operand_type_class(inferred.ty) else {
                self.report("E2913", "builtin-operand-not-convertible", expected.span());
                return None;
            };
            // The expected object is an unqualified object of the same type:
            // its ordinary load and store carry no qualifier of their own.
            let same = self.types.get(target.pointee).is_some_and(|ty| {
                ty.qualifiers == Qualifiers::default()
                    && self
                        .types
                        .compatible_ignoring_top_qualifiers(target.pointee, access_ty)
            });
            if !same {
                self.report("E2913", "builtin-operand-not-convertible", expected.span());
                return None;
            }
            let weak = operands[3];
            if !is_scalar_class(self.operand_type_class(self.converted_expression_type(weak)?.ty)) {
                self.report("E2913", "builtin-operand-not-convertible", weak.span());
                return None;
            }
            let success = self.atomic_order_operand(operands[4], &|_| true)?;
            let failure = self.atomic_order_operand(operands[5], &|failure| {
                !matches!(failure, O::Release | O::AcqRel)
                    && success.is_none_or(|success| failure as u8 <= success as u8)
            })?;
            let constant_order = success
                .zip(failure)
                .map(|(success, failure)| merged_exchange_order(success, failure));
            let order = self.alloc_node(CHirExpr::Cast(int_ty, operands[4], operands[4].span()));
            let failure = self.alloc_node(CHirExpr::Cast(int_ty, operands[5], operands[5].span()));
            return Some(
                self.alloc_node(CHirExpr::Atomic(evocc_chir::CHirAtomicAccess {
                    address,
                    value: Some(value),
                    expected: Some(expected),
                    rmw: Some((R::CompareExchange, V::Success)),
                    order,
                    constant_order,
                    access_ty,
                    volatile,
                    writeback: true,
                    weak: Some(weak),
                    failure: Some(failure),
                    memory: false,
                    result: None,
                    facts: ExprFacts::value(bool_ty),
                    span,
                })),
            );
        }
        let GnuAtomic::Update(operation, result) = form else {
            unreachable!("classified atomic form")
        };
        let constant_order = self.atomic_order_operand(operands[2], &|_| true)?;
        let order = self.alloc_node(CHirExpr::Cast(int_ty, operands[2], operands[2].span()));
        Some(
            self.alloc_node(CHirExpr::Atomic(evocc_chir::CHirAtomicAccess {
                address,
                value: Some(value),
                expected: None,
                rmw: Some((operation, result)),
                order,
                constant_order,
                access_ty,
                volatile,
                writeback: false,
                weak: None,
                failure: None,
                memory: false,
                result: None,
                facts: ExprFacts::value(access_ty),
                span,
            })),
        )
    }

    /// The generic forms on memory (EvoIR 10.1.14). `*p` is an integer,
    /// enumeration, pointer, floating, record or union object, `_Atomic` or
    /// not, of 1, 2, 4 or 8 bytes whose alignment is at least its size. Every
    /// buffer points to a complete object of a compatible type and the same
    /// size; a buffer that the operation writes, and the object of a store,
    /// exchange or compare-exchange, is not const. Any other shape is
    /// `E2913`. The orders follow the scalar forms.
    pub(super) fn parse_generic_atomic(
        &mut self,
        op: evocc_chir::AtomicCopyOp,
        operands: &[&'arena CHirExpr<'arena>],
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        use evocc_chir::{
            AtomicCopyOp as C, AtomicOrder as O, AtomicRmwOp as R, AtomicRmwReturn as V,
        };
        let refuse = |parser: &mut Self, operand: &CHirExpr<'_>| {
            parser.report("E2913", "builtin-operand-not-convertible", operand.span());
        };
        let address = operands[0];
        let OperandTypeClass::Pointer(pointer) =
            self.operand_type_class(self.converted_expression_type(address)?.ty)
        else {
            refuse(self, address);
            return None;
        };
        let object = pointer.pointee;
        let canonical = self.types.get(object)?.clone();
        let admitted_kind = matches!(
            canonical.kind,
            TypeKind::Int(_)
                | TypeKind::Enum(_)
                | TypeKind::Pointer(_)
                | TypeKind::Float(_)
                | TypeKind::Struct(_)
                | TypeKind::Union(_)
        );
        let access_ty = self.intern(canonical.kind.clone(), Qualifiers::default());
        let object_layout = self.target_layout.layout_of(&self.types, object).ok();
        let access_layout = self.target_layout.layout_of(&self.types, access_ty).ok();
        let admitted = admitted_kind
            && !self.type_is_variably_modified(object)
            && (!canonical.qualifiers.is_const || op == C::Load)
            && object_layout
                .zip(access_layout)
                .is_some_and(|(object, access)| {
                    matches!(access.size_bytes, 1 | 2 | 4 | 8)
                        && object.size_bytes == access.size_bytes
                        && u64::from(object.align_bytes) >= access.size_bytes
                });
        if !admitted {
            refuse(self, address);
            return None;
        }
        // The buffers in source order, and whether the operation writes each.
        let buffers: &[(usize, bool)] = match op {
            C::Load => &[(1, true)],
            C::Store => &[(1, false)],
            C::Exchange => &[(1, false), (2, true)],
            C::CompareExchange => &[(1, true), (2, false)],
        };
        for &(index, written) in buffers {
            let buffer = operands[index];
            let class = self.operand_type_class(self.converted_expression_type(buffer)?.ty);
            let OperandTypeClass::Pointer(target) = class else {
                refuse(self, buffer);
                return None;
            };
            let same = self.types.get(target.pointee).is_some_and(|ty| {
                !(written && ty.qualifiers.is_const)
                    && self
                        .types
                        .compatible_ignoring_top_qualifiers(target.pointee, access_ty)
            }) && self
                .target_layout
                .layout_of(&self.types, target.pointee)
                .ok()
                .zip(access_layout)
                .is_some_and(|(buffer, access)| buffer.size_bytes == access.size_bytes);
            if !same {
                refuse(self, buffer);
                return None;
            }
        }
        let int_ty = self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default());
        let order_index = match op {
            C::Load | C::Store => 2,
            C::Exchange => 3,
            C::CompareExchange => 4,
        };
        let constant_order = match op {
            C::Load => self.atomic_order_operand(operands[2], &|order| order.valid_for(true))?,
            C::Store => self.atomic_order_operand(operands[2], &|order| order.valid_for(false))?,
            C::Exchange => self.atomic_order_operand(operands[3], &|_| true)?,
            C::CompareExchange => {
                let weak = operands[3];
                if !is_scalar_class(
                    self.operand_type_class(self.converted_expression_type(weak)?.ty),
                ) {
                    refuse(self, weak);
                    return None;
                }
                let success = self.atomic_order_operand(operands[4], &|_| true)?;
                let failure = self.atomic_order_operand(operands[5], &|failure| {
                    !matches!(failure, O::Release | O::AcqRel)
                        && success.is_none_or(|success| failure as u8 <= success as u8)
                })?;
                success
                    .zip(failure)
                    .map(|(success, failure)| merged_exchange_order(success, failure))
            }
        };
        let order = self.alloc_node(CHirExpr::Cast(
            int_ty,
            operands[order_index],
            operands[order_index].span(),
        ));
        let compare = op == C::CompareExchange;
        let failure = compare
            .then(|| self.alloc_node(CHirExpr::Cast(int_ty, operands[5], operands[5].span())));
        let result_ty = if compare {
            self.intern(TypeKind::Int(IntRank::Bool), Qualifiers::default())
        } else {
            self.intern(TypeKind::Void, Qualifiers::default())
        };
        Some(
            self.alloc_node(CHirExpr::Atomic(evocc_chir::CHirAtomicAccess {
                address,
                value: match op {
                    C::Load => None,
                    C::Store | C::Exchange => Some(operands[1]),
                    C::CompareExchange => Some(operands[2]),
                },
                expected: compare.then(|| operands[1]),
                rmw: match op {
                    C::Load | C::Store => None,
                    C::Exchange => Some((R::Exchange, V::Old)),
                    C::CompareExchange => Some((R::CompareExchange, V::Success)),
                },
                order,
                constant_order,
                access_ty,
                volatile: canonical.qualifiers.is_volatile,
                writeback: compare,
                weak: compare.then(|| operands[3]),
                failure,
                memory: true,
                result: match op {
                    C::Load => Some(operands[1]),
                    C::Exchange => Some(operands[2]),
                    C::Store | C::CompareExchange => None,
                },
                facts: ExprFacts::value(result_ty),
                span,
            })),
        )
    }

    /// `__atomic_always_lock_free` and `__atomic_is_lock_free`. Sizes 1, 2,
    /// 4 and 8 are lock-free, and 16 where the target says so. A null pointer
    /// constant means typical alignment. `__atomic_always_lock_free` takes
    /// only that pointer and is an integer constant expression; for
    /// `__atomic_is_lock_free` a pointer to a type aligned to the size also
    /// answers 1, after its evaluation. Any other answer needs the run-time
    /// library and is `E2935`.
    pub(super) fn parse_atomic_lock_free(
        &mut self,
        form: GnuAtomic,
        size: &'arena CHirExpr<'arena>,
        pointer: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let Ok(bytes) = self.eval_integer_constant_expression(size) else {
            self.report("E2913", "builtin-operand-not-convertible", size.span());
            return None;
        };
        let bytes = bytes.try_mathematical_value().unwrap_or(-1);
        let lock_free = matches!(bytes, 1 | 2 | 4 | 8)
            || (bytes == 16 && self.target_layout.sixteen_byte_atomics_are_lock_free());
        let bool_ty = self.intern(TypeKind::Int(IntRank::Bool), Qualifiers::default());
        let answer = |parser: &Self, value: bool| {
            let literal = parser.alloc_node(CHirExpr::Lit(
                IntLit {
                    value: u64::from(value),
                    suffix: IntSuffix::None,
                    base: IntBase::Dec,
                },
                span,
            ));
            parser.alloc_node(CHirExpr::Cast(bool_ty, literal, span))
        };
        if self.is_null_pointer_constant(pointer) {
            return Some(answer(self, lock_free));
        }
        let class = self.operand_type_class(self.converted_expression_type(pointer)?.ty);
        let OperandTypeClass::Pointer(target) = class else {
            self.report("E2913", "builtin-operand-not-convertible", pointer.span());
            return None;
        };
        if form == GnuAtomic::AlwaysLockFree {
            self.report("E2913", "builtin-operand-not-convertible", pointer.span());
            return None;
        }
        let aligned = self
            .target_layout
            .layout_of(&self.types, target.pointee)
            .is_ok_and(|layout| i128::from(layout.align_bytes) >= bytes);
        if !(lock_free && aligned) {
            self.report(
                "E2935",
                "unsupported-library-builtin: __atomic_is_lock_free",
                span,
            );
            return None;
        }
        let void_ty = self.intern(TypeKind::Void, Qualifiers::default());
        let evaluated = self.alloc_node(CHirExpr::Cast(void_ty, pointer, pointer.span()));
        Some(self.alloc_node(CHirExpr::Comma(evaluated, answer(self, true), span)))
    }

    /// Clang's `__c11_atomic_*` builtins of the Darwin `<stdatomic.h>` (EvoIR
    /// 10.1.21). The first operand points to an `_Atomic` object of a type
    /// that `_Atomic` objects admit, and its non-atomic type is the value
    /// type. The forms are the scalar accesses of the GNU builtins: `init` is
    /// a relaxed store, and `fetch_add` and `fetch_sub` move a pointer by
    /// whole elements. Any other shape is `E2913`; an order constant that
    /// Apple clang diagnoses is `E2947`.
    pub(super) fn parse_clang_atomic(
        &mut self,
        keyword: Token,
        form: ClangAtomic,
    ) -> Option<&'arena CHirExpr<'arena>> {
        use evocc_chir::{AtomicOrder as O, AtomicRmwOp as R, AtomicRmwReturn as V};
        if !self.match_kind(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        let mut operands = Vec::new();
        if self.peek_kind() != Some(TokenKind::RParen) {
            loop {
                operands.push(self.parse_argument_expression()?);
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
            }
        }
        let arity = match form {
            ClangAtomic::IsLockFree => 1,
            ClangAtomic::Init | ClangAtomic::Load => 2,
            ClangAtomic::Store | ClangAtomic::Exchange | ClangAtomic::Fetch(_) => 3,
            ClangAtomic::CompareExchange => 5,
        };
        if operands.len() != arity || self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance()?;
        let span = self.merge_span(keyword.span, close.span);
        if form == ClangAtomic::IsLockFree {
            return self.parse_clang_lock_free(operands[0], span);
        }
        let address = operands[0];
        let OperandTypeClass::Pointer(pointer) =
            self.operand_type_class(self.converted_expression_type(address)?.ty)
        else {
            self.report("E2913", "builtin-operand-not-convertible", address.span());
            return None;
        };
        let canonical = self.types.get(pointer.pointee)?.clone();
        // Only a load reads a const object.
        if !canonical.qualifiers.is_atomic
            || (canonical.qualifiers.is_const && form != ClangAtomic::Load)
            || !evocc_chir::atomic_access_admitted(
                &self.types,
                &self.target_layout,
                pointer.pointee,
            )
        {
            self.report("E2913", "builtin-operand-not-convertible", address.span());
            return None;
        }
        let access_ty = self.intern(canonical.kind.clone(), Qualifiers::default());
        let int_ty = self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default());
        let void_ty = self.intern(TypeKind::Void, Qualifiers::default());
        let bool_ty = self.intern(TypeKind::Int(IntRank::Bool), Qualifiers::default());
        let value = match form {
            ClangAtomic::Load => None,
            ClangAtomic::Fetch(operation) => {
                Some(self.clang_atomic_operand(operation, access_ty, operands[1])?)
            }
            ClangAtomic::CompareExchange => {
                Some(self.converted_atomic_value(access_ty, operands[2])?)
            }
            _ => Some(self.converted_atomic_value(access_ty, operands[1])?),
        };
        let expected = if form == ClangAtomic::CompareExchange {
            // The expected object is an unqualified object of the value type:
            // its ordinary load and store carry no qualifier of their own.
            let expected = operands[1];
            let class = self.operand_type_class(self.converted_expression_type(expected)?.ty);
            let same = matches!(class, OperandTypeClass::Pointer(target)
            if self.types.get(target.pointee).is_some_and(|ty| {
                ty.qualifiers == Qualifiers::default()
                    && self.types.compatible_ignoring_top_qualifiers(target.pointee, access_ty)
            }));
            if !same {
                self.report("E2913", "builtin-operand-not-convertible", expected.span());
                return None;
            }
            Some(expected)
        } else {
            None
        };
        let (order, constant_order, failure) = match form {
            // Initialization is a relaxed store, as GCC's `atomic_init` is.
            ClangAtomic::Init => (
                self.alloc_node(CHirExpr::Lit(
                    IntLit {
                        value: 0,
                        suffix: IntSuffix::None,
                        base: IntBase::Dec,
                    },
                    span,
                )),
                Some(O::Relaxed),
                None,
            ),
            ClangAtomic::Load | ClangAtomic::Store => {
                let load = form == ClangAtomic::Load;
                let order = operands[if load { 1 } else { 2 }];
                let constant = self.atomic_order_operand(order, &|order| order.valid_for(load))?;
                (order, constant, None)
            }
            ClangAtomic::Exchange | ClangAtomic::Fetch(_) => {
                let order = operands[2];
                (order, self.atomic_order_operand(order, &|_| true)?, None)
            }
            ClangAtomic::CompareExchange => {
                let success = self.atomic_order_operand(operands[3], &|_| true)?;
                let failure = self.atomic_order_operand(operands[4], &|failure| {
                    !matches!(failure, O::Release | O::AcqRel)
                })?;
                let merged = success
                    .zip(failure)
                    .map(|(success, failure)| merged_clang_exchange_order(success, failure));
                let failure =
                    self.alloc_node(CHirExpr::Cast(int_ty, operands[4], operands[4].span()));
                (operands[3], merged, Some(failure))
            }
            ClangAtomic::IsLockFree => unreachable!("answered above"),
        };
        let order = self.alloc_node(CHirExpr::Cast(int_ty, order, order.span()));
        let (rmw, result_ty) = match form {
            ClangAtomic::Init | ClangAtomic::Store => (None, void_ty),
            ClangAtomic::Load => (None, access_ty),
            ClangAtomic::Exchange => (Some((R::Exchange, V::Old)), access_ty),
            ClangAtomic::Fetch(operation) => (Some((operation, V::Old)), access_ty),
            ClangAtomic::CompareExchange => (Some((R::CompareExchange, V::Success)), bool_ty),
            ClangAtomic::IsLockFree => unreachable!("answered above"),
        };
        Some(
            self.alloc_node(CHirExpr::Atomic(evocc_chir::CHirAtomicAccess {
                address,
                value,
                expected,
                rmw,
                order,
                constant_order,
                access_ty,
                volatile: canonical.qualifiers.is_volatile,
                writeback: form == ClangAtomic::CompareExchange,
                weak: None,
                failure,
                memory: false,
                result: None,
                facts: ExprFacts::value(result_ty),
                span,
            })),
        )
    }

    /// A value operand of an atomic builtin, converted to the value type as
    /// by assignment, or `E2913`.
    pub(super) fn converted_atomic_value(
        &mut self,
        access_ty: TypeId,
        value: &'arena CHirExpr<'arena>,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if !matches!(
            self.assignment_constraint(AssignmentOp::Simple, access_ty, value),
            UpdateConstraint::Scalar
        ) {
            self.report("E2913", "builtin-operand-not-convertible", value.span());
            return None;
        }
        Some(self.alloc_node(CHirExpr::Cast(access_ty, value, value.span())))
    }

    /// The operand of a Clang fetch-and-modify builtin. An integer object
    /// other than `_Bool` takes a value of its type. An object pointer takes
    /// a `ptrdiff_t` count of elements for add and subtract, which becomes a
    /// byte count that wraps at 64 bits, as Apple clang computes it.
    pub(super) fn clang_atomic_operand(
        &mut self,
        operation: evocc_chir::AtomicRmwOp,
        access_ty: TypeId,
        value: &'arena CHirExpr<'arena>,
    ) -> Option<&'arena CHirExpr<'arena>> {
        use evocc_chir::AtomicRmwOp as R;
        let refuse = |parser: &mut Self, operand: &CHirExpr<'_>| {
            parser.report("E2913", "builtin-operand-not-convertible", operand.span());
        };
        if self
            .canonical_integer_type(access_ty)
            .is_ok_and(|integer| integer != CIntegerType::BOOL)
        {
            return self.converted_atomic_value(access_ty, value);
        }
        let element = match self.types.get(access_ty).map(|ty| ty.kind.clone()) {
            Some(TypeKind::Pointer(element)) if matches!(operation, R::Add | R::Sub) => element,
            _ => {
                refuse(self, value);
                return None;
            }
        };
        let complete_object = !matches!(
            self.types.get(element).map(|ty| &ty.kind),
            Some(TypeKind::Void | TypeKind::Function { .. }) | None
        ) && !self.type_is_variably_modified(element);
        let size = complete_object
            .then(|| self.target_layout.layout_of(&self.types, element).ok())
            .flatten()
            .map(|layout| layout.size_bytes);
        let class = self.operand_type_class(self.converted_expression_type(value)?.ty);
        let Some(size) = size.filter(|_| is_arithmetic_class(class)) else {
            refuse(self, value);
            return None;
        };
        let long_ty = self.intern(TypeKind::Int(IntRank::Long), Qualifiers::default());
        let bytes_ty = self.intern(TypeKind::Int(IntRank::ULong), Qualifiers::default());
        let difference = self.alloc_node(CHirExpr::Cast(long_ty, value, value.span()));
        let count = self.alloc_node(CHirExpr::Cast(bytes_ty, difference, value.span()));
        if size == 1 {
            return Some(count);
        }
        let scale = self.alloc_node(CHirExpr::Lit(
            IntLit {
                value: size,
                suffix: IntSuffix::UL,
                base: IntBase::Dec,
            },
            value.span(),
        ));
        Some(self.alloc_node(CHirExpr::Binary(BinaryOp::Mul, count, scale, value.span())))
    }

    /// `__c11_atomic_is_lock_free(size)`: an integer constant expression of
    /// type `_Bool` that is 1 for the sizes that `__atomic_always_lock_free`
    /// answers 1 for. Apple clang calls the run-time library for any other
    /// size, so that is `E2935`; a size that is not an integer constant
    /// expression is `E2913`.
    pub(super) fn parse_clang_lock_free(
        &mut self,
        size: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let Ok(bytes) = self.eval_integer_constant_expression(size) else {
            self.report("E2913", "builtin-operand-not-convertible", size.span());
            return None;
        };
        let bytes = bytes.try_mathematical_value().unwrap_or(-1);
        let lock_free = matches!(bytes, 1 | 2 | 4 | 8)
            || (bytes == 16 && self.target_layout.sixteen_byte_atomics_are_lock_free());
        if !lock_free {
            self.report(
                "E2935",
                "unsupported-library-builtin: __c11_atomic_is_lock_free",
                span,
            );
            return None;
        }
        let bool_ty = self.intern(TypeKind::Int(IntRank::Bool), Qualifiers::default());
        let one = self.alloc_node(CHirExpr::Lit(
            IntLit {
                value: 1,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span,
        ));
        Some(self.alloc_node(CHirExpr::Cast(bool_ty, one, span)))
    }
}
