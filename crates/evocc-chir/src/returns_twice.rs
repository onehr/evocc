//! Functions that return twice (EvoIR 10.1.9 and 3.10).
//!
//! The rule is GCC 14's. A function returns twice when a declaration of it
//! carries the GNU `returns_twice` attribute. GCC also marks a function by
//! its C identifier (`special_function_p` in `calls.cc`): the function must
//! have external linkage, and the identifier must have at most 11 bytes.
//! After one leading `__` or `_` is removed, it must equal `setjmp` or
//! `sigsetjmp`. Otherwise, the whole identifier must equal `savectx`, `vfork`
//! or `getcontext`. An assembler label does not count.

use std::collections::BTreeSet;

use crate::{CHirDecl, CHirExpr, CHirUnit, ExprCategory, PlaceRoot, SymbolId, UnaryOp};

/// GCC 14's name rule for a C identifier with external linkage.
#[must_use]
pub fn returns_twice_name(identifier: &str) -> bool {
    if identifier.len() > 11 {
        return false;
    }
    let stem = identifier
        .strip_prefix("__")
        .or_else(|| identifier.strip_prefix('_'))
        .unwrap_or(identifier);
    matches!(stem, "setjmp" | "sigsetjmp")
        || matches!(identifier, "savectx" | "vfork" | "getcontext")
}

/// Every function of the unit that returns twice. Every declaration counts,
/// in any order. GCC 14 applies the name rule to file-scope declarations
/// only. EvoCC also applies it to a block-scope declaration with external
/// linkage: the addition is conservative, because it only gives more calls
/// the treatment of a call that returns twice.
#[must_use]
pub fn returns_twice_functions(unit: &CHirUnit<'_>) -> BTreeSet<SymbolId> {
    let mut marked = BTreeSet::new();
    let mut named = BTreeSet::new();
    let mut internal = BTreeSet::new();
    crate::inline::for_each_declaration(unit, |declaration| {
        let CHirDecl::Function(function) = declaration else {
            return;
        };
        if function
            .attributes
            .iter()
            .any(|attribute| attribute.semantic_name() == Some("returns_twice"))
        {
            marked.insert(function.symbol);
        }
        if function.has_internal_linkage(unit.symbols()) {
            internal.insert(function.symbol);
        }
        if returns_twice_name(&function.name.spelling) {
            named.insert(function.symbol);
        }
    });
    marked.extend(named.difference(&internal));
    marked
}

/// The function that a call names directly. GCC folds `*f` and `&f` of a
/// function designator `f`, so any chain of them still names `f`. A cast or
/// a pointer object makes an indirect call.
#[must_use]
pub fn designated_function(callee: &CHirExpr<'_>) -> Option<SymbolId> {
    let mut expression = callee;
    loop {
        match expression {
            CHirExpr::Unary(UnaryOp::Deref | UnaryOp::AddrOf, operand, _) => expression = operand,
            // The operand of `&` in `&*f` is the place `*f`.
            CHirExpr::MemoryRead { place, .. } if place.projections().is_empty() => {
                let PlaceRoot::PointerExpression { pointer, .. } = place.root() else {
                    return None;
                };
                expression = pointer;
            }
            CHirExpr::Ident { symbol, facts, .. }
                if facts.category() == ExprCategory::FunctionDesignator =>
            {
                return Some(*symbol);
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::returns_twice_name;

    #[test]
    fn the_name_rule_is_gcc_special_function_p() {
        for name in [
            "setjmp",
            "_setjmp",
            "__setjmp",
            "sigsetjmp",
            "_sigsetjmp",
            "__sigsetjmp",
            "savectx",
            "vfork",
            "getcontext",
        ] {
            assert!(returns_twice_name(name), "{name}");
        }
        for name in [
            // A third underscore is not removed.
            "___setjmp",
            // Only `setjmp` and `sigsetjmp` lose a prefix.
            "_vfork",
            "__vfork",
            "_getcontext",
            "_savectx",
            // Twelve bytes exceed the limit before the prefix is removed.
            "___sigsetjmp",
            "__sigsetjmp_",
            "setjmp_",
            "longjmp",
            "sigsetjmp1",
            "fork",
            "setcontext",
            "swapcontext",
            "",
        ] {
            assert!(!returns_twice_name(name), "{name}");
        }
    }
}
