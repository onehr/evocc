//! ELF load-instance registrations, independent of ordinary calls.
use crate::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::SourceSpan;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Constructor,
    Destructor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registration {
    pub function: SymbolId,
    pub phase: Phase,
    pub priority: u16,
}

/// Redeclarations contribute attributes to a single registration per phase.
/// Order is first registration order, not symbol-table or hash iteration order.
pub fn registrations(unit: &CHirUnit<'_>) -> Result<Vec<Registration>, SourceSpan> {
    let mut entries = Vec::new();
    let mut seen = BTreeMap::new();
    let definitions: std::collections::BTreeSet<_> = unit
        .iter()
        .filter_map(|decl| match decl {
            CHirDecl::Function(function) if function.body.is_some() => Some(function.symbol),
            _ => None,
        })
        .collect();
    for decl in unit.iter() {
        let CHirDecl::Function(function) = decl else {
            continue;
        };
        if !definitions.contains(&function.symbol) {
            continue;
        }
        for attribute in &function.attributes {
            let phase = match attribute.semantic_name() {
                Some("constructor") => Phase::Constructor,
                Some("destructor") => Phase::Destructor,
                _ => continue,
            };
            let priority = match attribute.arguments.as_slice() {
                [] => u16::MAX,
                [arg] => arg
                    .integer
                    .or_else(|| arg.text.parse().ok())
                    .filter(|n| (101..=65535).contains(n))
                    .and_then(|n| u16::try_from(n).ok())
                    .ok_or(attribute.span)?,
                _ => return Err(attribute.span),
            };
            if let Some(old) = seen.insert((function.symbol, phase), priority) {
                if old != priority {
                    return Err(attribute.span);
                }
            } else {
                entries.push(Registration {
                    function: function.symbol,
                    phase,
                    priority,
                });
            }
        }
    }
    Ok(entries)
}
