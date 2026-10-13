use super::*;
use evocc_chir::floating::{FloatingBuiltin, FloatingExpr};

/// The floating builtins that the front end resolves.
pub(super) const BUILTINS: &[&str] = &[
    "__builtin_isnan",
    "__builtin_isgreater",
    "__builtin_isgreaterequal",
    "__builtin_isless",
    "__builtin_islessequal",
    "__builtin_islessgreater",
    "__builtin_isunordered",
    "__builtin_isinf",
    "__builtin_isinf_sign",
    "__builtin_isfinite",
    "__builtin_isnormal",
    "__builtin_signbit",
    "__builtin_signbitf",
    "__builtin_signbitl",
    "__builtin_fabs",
    "__builtin_fabsf",
    "__builtin_fabsl",
    "__builtin_copysign",
    "__builtin_copysignf",
    "__builtin_copysignl",
    "__builtin_inf",
    "__builtin_inff",
    "__builtin_infl",
    "__builtin_huge_val",
    "__builtin_huge_valf",
    "__builtin_huge_vall",
];

pub(super) fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn standard_floating_builtin(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Option<&'static str> {
        let symbol = self.standard_function_symbol(expression)?;
        if self.entities.get(&symbol)?.assembly_named {
            return None;
        }
        let declaration = self.symbols.symbol(symbol).ok()?;
        let (name, kind) = match declaration.spelling() {
            "fabs" => ("__builtin_fabs", FloatKind::F64),
            "fabsf" => ("__builtin_fabsf", FloatKind::F32),
            _ => return None,
        };
        let TypeKind::Function {
            return_type,
            parameters:
                FunctionParameters::Prototype {
                    types,
                    variadic: false,
                },
        } = &self.types.get(declaration.ty())?.kind
        else {
            return None;
        };
        let [parameter] = types.as_slice() else {
            return None;
        };
        [return_type, parameter]
            .into_iter()
            .all(|ty| {
                self.types
                    .get(*ty)
                    .is_some_and(|ty| ty.kind == TypeKind::Float(kind))
            })
            .then_some(name)
    }

    pub(super) fn resolve_floating_builtin(
        &mut self,
        name: &str,
        mut operands: Vec<&'arena CHirExpr<'arena>>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        use FloatingBuiltin::*;
        let infinity = name.starts_with("__builtin_inf") || name.starts_with("__builtin_huge_val");
        let op = match name {
            "__builtin_isnan" => IsNan,
            "__builtin_isgreater" => IsGreater,
            "__builtin_isgreaterequal" => IsGreaterEqual,
            "__builtin_isless" => IsLess,
            "__builtin_islessequal" => IsLessEqual,
            "__builtin_islessgreater" => IsLessGreater,
            "__builtin_isunordered" => IsUnordered,
            "__builtin_isinf" => IsInf,
            "__builtin_isinf_sign" => IsInfSign,
            "__builtin_isfinite" => IsFinite,
            "__builtin_isnormal" => IsNormal,
            "__builtin_signbit" | "__builtin_signbitf" | "__builtin_signbitl" => SignBit,
            "__builtin_fabs" | "__builtin_fabsf" | "__builtin_fabsl" => Abs,
            _ => CopySign,
        };
        let arity = if infinity {
            0
        } else if op == CopySign || op.is_comparison() {
            2
        } else {
            1
        };
        if operands.len() != arity {
            self.report("E2909", "builtin-arity", span);
            return None;
        }
        let suffix = if name.ends_with('l') && name != "__builtin_huge_val" {
            FloatSuffix::L
        } else if name.ends_with('f') && name != "__builtin_inf" {
            FloatSuffix::F
        } else {
            FloatSuffix::None
        };
        let selected = match suffix {
            FloatSuffix::F => FloatKind::F32,
            FloatSuffix::L => FloatKind::F128,
            _ => FloatKind::F64,
        };
        if infinity {
            let raw = match selected {
                FloatKind::F32 => 0x7f800000,
                FloatKind::F64 => 0x7ff0000000000000,
                _ => 0x7fff0000000000000000000000000000,
            };
            return Some(self.alloc_node(CHirExpr::FloatLit(
                FloatLit {
                    value: evocc_chir::FloatConstant::Real(
                        evocc_types::floating::FloatBits::from_bits(selected, raw)?,
                    ),
                    suffix,
                },
                span,
            )));
        }
        let floating_result = matches!(op, Abs | CopySign);
        let ty = self.intern(TypeKind::Float(selected), Qualifiers::default());
        if op.is_comparison() {
            let left = self.expression_type(operands[0])?.ty;
            let right = self.expression_type(operands[1])?.ty;
            let Some(InferredType::Real(kind)) = self.usual_arithmetic_expression_type(left, right)
            else {
                self.report("E2951", "builtin-argument-type", span);
                return None;
            };
            let common = self.intern(TypeKind::Float(kind), Qualifiers::default());
            self.validate_builtin_argument_types(&operands, &[common, common])?;
            for operand in &mut operands {
                *operand = self.alloc_node(CHirExpr::Cast(common, operand, operand.span()));
            }
        } else if floating_result {
            self.validate_builtin_argument_types(&operands, &vec![ty; arity])?;
            for operand in &mut operands {
                *operand = self.alloc_node(CHirExpr::Cast(ty, operand, operand.span()));
            }
        } else {
            // Classification is type-generic: inspecting a float must not first
            // widen it, quiet a signaling NaN, or truncate long-double layout.
            if !matches!(self.expression_type(operands[0])?.ty, InferredType::Real(_))
                && !matches!(
                self.expression_type(operands[0])?.ty,
                InferredType::Canonical(t) if matches!(self.types.get(t)?.kind, TypeKind::Float(_)))
            {
                self.report("E2951", "builtin-argument-type", span);
                return None;
            }
        }
        let result = if floating_result {
            ty
        } else {
            self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default())
        };
        Some(self.alloc_node(CHirExpr::Floating(FloatingExpr {
            op,
            operands,
            facts: ExprFacts::value(result),
            span,
        })))
    }
}
