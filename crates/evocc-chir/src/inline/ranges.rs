//! Conservative facts after an evaluated branch, never speculative reads.

use super::*;
use crate::integer::{usual_arithmetic_type, CIntegerType};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Range {
    low: i128,
    high: i128,
    ty: CIntegerType,
}

impl Range {
    fn full(ty: CIntegerType) -> Option<Self> {
        Some(Self {
            low: if ty.is_signed() { ty.signed_min() } else { 0 },
            high: if ty.is_signed() {
                ty.signed_max()
            } else {
                i128::try_from(ty.mask()).ok()?
            },
            ty,
        })
    }

    fn convert(self, ty: CIntegerType) -> Option<Self> {
        let full = Self::full(ty)?;
        (self.low >= full.low && self.high <= full.high).then_some(Self { ty, ..self })
    }

    pub(super) fn boolean(self) -> Option<bool> {
        (self.ty == CIntegerType::INT && self.low == self.high && (0..=1).contains(&self.low))
            .then_some(self.low != 0)
    }
}

impl Substitution<'_, '_> {
    fn range_symbol_type(&self, symbol: SymbolId, c: &Context<'_>) -> Option<CIntegerType> {
        let entry = self.unit.symbols.symbol(symbol).ok()?;
        if entry.scope() == self.unit.symbols.file_scope()
            || !matches!(
                entry.storage_class(),
                Some(StorageClass::None | StorageClass::Auto)
            )
            || c.exposed_objects.contains(&symbol)
            || c.replacements
                .iter()
                .any(|(old, new)| *new == symbol && c.exposed_objects.contains(old))
        {
            return None;
        }
        let ty = self.unit.types.get(entry.ty())?;
        if ty.qualifiers.is_volatile || ty.qualifiers.is_atomic {
            return None;
        }
        crate::constant::canonical_integer_type(&self.unit.types, entry.ty())
    }

    pub(super) fn query_range(
        &self,
        expression: &CHirExpr<'_>,
        c: &Context<'_>,
        depth: u8,
    ) -> Option<Range> {
        let depth = depth.checked_sub(1)?;
        if let Some(v) = crate::constant::eval_executable_integer_constant(
            expression,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        ) {
            let value = v.try_mathematical_value()?;
            return Some(Range {
                low: value,
                high: value,
                ty: v.ty(),
            });
        }
        match expression {
            CHirExpr::Ident { symbol, .. } => {
                self.range_symbol_type(*symbol, c)?;
                c.path_ranges.get(symbol).copied()
            }
            CHirExpr::Cast(ty, operand, _) => self.query_range(operand, c, depth)?.convert(
                crate::constant::canonical_integer_type(&self.unit.types, *ty)?,
            ),
            CHirExpr::Binary(op, left, right, _) => {
                let a = self.query_range(left, c, depth)?;
                let b = self.query_range(right, c, depth)?;
                let common = usual_arithmetic_type(a.ty, b.ty);
                let a = a.convert(common)?;
                let b = b.convert(common)?;
                let result = match op {
                    BinaryOp::Lt if a.high < b.low => true,
                    BinaryOp::Lt if a.low >= b.high => false,
                    BinaryOp::Le if a.high <= b.low => true,
                    BinaryOp::Le if a.low > b.high => false,
                    BinaryOp::Gt if a.low > b.high => true,
                    BinaryOp::Gt if a.high <= b.low => false,
                    BinaryOp::Ge if a.low >= b.high => true,
                    BinaryOp::Ge if a.high < b.low => false,
                    BinaryOp::Eq | BinaryOp::Ne if a.low == a.high && b.low == b.high => {
                        (a.low == b.low) == (*op == BinaryOp::Eq)
                    }
                    BinaryOp::Eq | BinaryOp::Ne if a.high < b.low || b.high < a.low => {
                        *op == BinaryOp::Ne
                    }
                    _ => return None,
                };
                Some(Range {
                    low: i128::from(result),
                    high: i128::from(result),
                    ty: CIntegerType::INT,
                })
            }
            // No calls, loads, updates, wrapping/narrowing casts or arithmetic
            // are replayed. In particular, transporting an address proves no value.
            _ => None,
        }
    }

    pub(super) fn invalidate_ranges(&self, uses: &Uses<'_>, c: &mut Context<'_>) {
        c.path_ranges.retain(|symbol, _| {
            !uses.mutable.contains(symbol)
                && !c
                    .replacements
                    .iter()
                    .any(|(old, new)| new == symbol && uses.mutable.contains(old))
        });
        c.memo.clear();
    }

    pub(super) fn assume_range(&self, expression: &CHirExpr<'_>, truth: bool, c: &mut Context<'_>) {
        if !c.track_ranges {
            return;
        }
        let CHirExpr::Binary(op, CHirExpr::Ident { symbol, .. }, right, _) = expression else {
            return;
        };
        let Some(ty) = self.range_symbol_type(*symbol, c) else {
            return;
        };
        let Some(value) = crate::constant::eval_executable_integer_constant(
            right,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        ) else {
            return;
        };
        let Some(full) = Range::full(ty) else {
            return;
        };
        let common = usual_arithmetic_type(ty, value.ty());
        if full.convert(common).is_none() {
            return;
        }
        let Some(value) = value.cast(common).try_mathematical_value() else {
            return;
        };
        let mut range = c.path_ranges.get(symbol).copied().unwrap_or(full);
        match (op, truth) {
            (BinaryOp::Gt, true) | (BinaryOp::Le, false) => {
                let Some(bound) = value.checked_add(1) else {
                    return;
                };
                range.low = range.low.max(bound);
            }
            (BinaryOp::Ge, true) | (BinaryOp::Lt, false) => range.low = range.low.max(value),
            (BinaryOp::Lt, true) | (BinaryOp::Ge, false) => {
                let Some(bound) = value.checked_sub(1) else {
                    return;
                };
                range.high = range.high.min(bound);
            }
            (BinaryOp::Le, true) | (BinaryOp::Gt, false) => range.high = range.high.min(value),
            (BinaryOp::Eq, true) | (BinaryOp::Ne, false) => {
                range.low = range.low.max(value);
                range.high = range.high.min(value);
            }
            _ => return,
        }
        if range.low <= range.high {
            c.path_ranges.insert(*symbol, range);
            c.memo.clear();
        }
    }
}
