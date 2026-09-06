//! Section 8, "Linkage" (revision 0.16).
//!
//! "An EvoIR function or global whose `visibility` is `internal` is emitted
//! without `.globl`, so its symbol is local to the object. Before emission,
//! every internal function that no `call`, `func_addr`, `block_addr`
//! relocation or other emitted function references is dropped, and the check
//! repeats until no internal function is dropped; an internal global is kept,
//! since data is cheap and a relocation may name it. Module assembly may
//! reference any symbol, so a symbol named in a module assembly entry counts
//! as referenced."
//!
//! The two halves live in different places. Suppressing `.globl` is emission's
//! and is written where the symbol is; deciding which functions reach emission
//! is this module's, because it is a property of the whole EvoIR unit and has
//! to be answered before anything is lowered: an unreferenced `static inline`
//! stub from a shared header calls functions the configuration does not build,
//! so lowering it at all would put an undefined symbol in the object.

use std::collections::BTreeSet;

use evocc_evoir::{Function, GlobalVisibility, Inst, Module};

/// The names of the functions of `module` that emission keeps.
///
/// Every external function is kept, because another object may call it. An
/// internal function is kept only when something that is itself emitted names
/// it, computed to a fixed point from the external functions, the module
/// globals' relocations and the module assembly table.
#[must_use]
pub fn emitted_functions(module: &Module) -> BTreeSet<String> {
    let internal = module
        .functions
        .iter()
        .filter(|function| function.visibility == GlobalVisibility::Internal)
        .map(|function| function.name.as_str())
        .collect::<BTreeSet<&str>>();
    let mut kept = module
        .functions
        .iter()
        .filter(|function| function.visibility == GlobalVisibility::External)
        .map(|function| function.name.clone())
        .collect::<BTreeSet<String>>();

    // A global is kept whatever its visibility, so every relocation it carries
    // is emitted and names a symbol the object must define. A relocation
    // against a block of a function (EvoIR 1.9.69) is written `@f^block` and
    // names the function `f`.
    for global in &module.globals {
        for relocation in &global.relocs {
            let symbol = relocation
                .symbol
                .split_once('^')
                .map_or(relocation.symbol.as_str(), |(function, _)| function);
            if internal.contains(symbol) {
                kept.insert(symbol.to_owned());
            }
        }
    }

    // Emission copies an assembly entry verbatim and never parses it, so the
    // only honest reading is that an entry which spells an internal symbol
    // references it.
    for entry in &module.assembly {
        for name in &internal {
            if names_symbol(entry, name) {
                kept.insert((*name).to_owned());
            }
        }
    }

    // "the check repeats until no internal function is dropped": a function
    // reached only from a function that is itself dropped is dropped too, so
    // the set grows from the roots rather than shrinking from everything.
    loop {
        let reached = module
            .functions
            .iter()
            .filter(|function| kept.contains(&function.name))
            .flat_map(references_of)
            .filter(|name| internal.contains(name.as_str()))
            .collect::<BTreeSet<String>>();
        let before = kept.len();
        kept.extend(reached);
        if kept.len() == before {
            return kept;
        }
    }
}

/// Every function symbol one function names directly.
fn references_of(function: &Function) -> Vec<String> {
    let mut names = Vec::new();
    for block in &function.blocks {
        for inst in block.insts.iter().chain(std::iter::once(&block.terminator)) {
            match inst {
                Inst::Call { callee, .. } => names.push(callee.clone()),
                Inst::FuncAddr { symbol, .. } => names.push(symbol.clone()),
                _ => {}
            }
        }
    }
    names
}

/// Whether `text` spells `name` as a symbol rather than inside a longer word.
///
/// An entry is platform assembly, so it spells the prefixed symbol `_helper`
/// on Darwin and `helper` on Linux; `_` therefore has to bound a match, and
/// the boundary is any character that is not alphanumeric. That admits
/// `other_helper` as a reference to `helper`, which keeps a function emission
/// could have dropped. The direction is deliberate: dropping a function an
/// entry calls would leave the object with an undefined symbol, and emission
/// cannot parse the entry to tell the two apart.
fn names_symbol(text: &str, name: &str) -> bool {
    let mut rest = text;
    while let Some(start) = rest.find(name) {
        let bounded = |character: Option<char>| !character.is_some_and(char::is_alphanumeric);
        let before = rest[..start].chars().next_back();
        rest = &rest[start + name.len()..];
        if bounded(before) && bounded(rest.chars().next()) {
            return true;
        }
    }
    false
}
