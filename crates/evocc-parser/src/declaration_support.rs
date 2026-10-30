//! Parser declaration support.

use super::*;

pub(super) fn derived_declarator_span(layer: &DerivedDeclarator<'_>) -> SourceSpan {
    match layer {
        DerivedDeclarator::Pointer { span, .. }
        | DerivedDeclarator::Array { span, .. }
        | DerivedDeclarator::Function { span, .. } => *span,
    }
}

pub(super) fn checked_initializer_action_add(
    left: u64,
    right: u64,
) -> Result<u64, InitializerPreflightError> {
    left.checked_add(right)
        .ok_or(InitializerPreflightError::TooLarge)
}

/// The list entry for a semantic attribute name.
pub(super) fn convention_attribute(name: &str) -> Option<(&'static str, &'static str)> {
    CONVENTION_ATTRIBUTES
        .iter()
        .copied()
        .find(|(attribute, _)| *attribute == name)
}

/// EvoIR 10.1.9: the x86 calling conventions. GCC 14 and Apple clang ignore
/// them on AArch64 with a warning, and EvoCC reports W0011. Apple clang
/// refuses `regparm`, so on Darwin it stays in the list above.
pub(super) fn ignored_x86_convention(name: &str, darwin: bool) -> bool {
    matches!(
        name,
        "stdcall" | "fastcall" | "thiscall" | "vectorcall" | "regcall" | "sseregparm"
    ) || (name == "regparm" && !darwin)
}

/// EvoIR 10.1.9: the x86 conventions that Apple clang implements. It refuses
/// one on a statement or with an argument. It knows `vectorcall` only in the
/// `__attribute__` syntax, and it does not know `sseregparm`.
pub(super) fn apple_clang_x86_convention(name: &str, bracketed: bool) -> bool {
    matches!(name, "stdcall" | "fastcall" | "thiscall" | "regcall")
        || (name == "vectorcall" && !bracketed)
}

/// Clang attributes whose meaning changes a type, a value, the layout or the
/// ABI. EvoCC refuses them on Darwin instead of silently treating the source
/// as the unannotated type.
pub(super) fn darwin_refuses_attribute(name: &str) -> bool {
    matches!(
        name,
        "ext_vector_type"
            | "matrix_type"
            | "address_space"
            | "pass_object_size"
            | "pass_dynamic_object_size"
            | "noderef"
            | "ms_struct"
            | "counted_by"
            | "counted_by_or_null"
            | "sized_by"
            | "sized_by_or_null"
            | "ended_by"
            | "internal_linkage"
            | "unsafe_indexable"
            | "indexable"
            | "bidi_indexable"
            | "terminated_by"
            | "decay_discards_count_in_parameters"
    )
}

pub(super) fn nullability_keyword(text: &str) -> bool {
    matches!(text, "_Nonnull" | "_Nullable" | "_Null_unspecified")
}

/// EvoIR 10.1.9: whether a function that a declaration marks with a list
/// attribute can have a definition, and whether its designator can be used.
/// Every attribute is accepted on a prototype.
pub(super) fn convention_permits(attribute: &str, definition: bool) -> bool {
    match attribute {
        // A second return has a rule (EvoIR 3.10), so the function can
        // be defined, called and named.
        "returns_twice" => true,
        // The callee takes its arguments as the base AAPCS64 does and
        // preserves a superset of the base callee-saved registers. A caller
        // relies only on the base set, so an ordinary call is correct. EvoCC
        // cannot compile a body to the stronger contract.
        "aarch64_vector_pcs" | "preserve_all" | "preserve_most" => !definition,
        _ => false,
    }
}

/// E2957: an `always_inline` function makes a call that returns twice
/// (EvoIR 10.1.9). Substitution would move the second return into the
/// caller, and an ordinary call would return from the frame that the jump
/// needs. GCC 14 refuses such a function too. The refusal is deferred, so
/// only a retained function reports it, at its first such call.
pub(super) fn always_inline_returns_twice(
    unit: &CHirUnit<'_>,
    uses: &[FunctionUse],
    end: usize,
) -> Vec<DeferredRefusal> {
    let Ok(roles) = evocc_chir::function_definitions::FunctionDefinitions::derive(unit) else {
        return Vec::new();
    };
    if roles.always_inline.is_empty() {
        return Vec::new();
    }
    let returns_twice = evocc_chir::returns_twice::returns_twice_functions(unit);
    let mut refusals = Vec::new();
    for symbol in &roles.always_inline {
        let Some(body) = roles
            .definitions
            .get(symbol)
            .or_else(|| roles.inline_bodies.get(symbol))
            .and_then(|function| function.body)
        else {
            continue;
        };
        let Some((callee, span)) = evocc_chir::inline::designated_calls(body)
            .into_iter()
            .find(|(callee, _)| returns_twice.contains(callee))
        else {
            continue;
        };
        let preceding = uses
            .iter()
            .find(|usage| usage.owner == Some(*symbol) && usage.function == callee)
            .map_or(end, |usage| usage.preceding);
        refusals.push((
            evocc_chir::ProfileRefusal {
                function: *symbol,
                code: "E2957",
                message: "always-inline-returns-twice",
                span,
            },
            preceding,
            None,
        ));
    }
    refusals
}

pub(super) fn span_key(span: SourceSpan) -> SpanKey {
    (span.file.raw(), span.byte_start, span.byte_end)
}

/// E2952 for the list attributes after the whole unit is known. A function is
/// marked by every accepted list attribute on any of its declarations, in any
/// order. A definition and a use outside any function body are reported at
/// once; a use inside a body is deferred, so only a retained function reports
/// it. The first list attribute that forbids the definition or the use names
/// the refusal.
pub(super) fn convention_refusals(
    unit: &CHirUnit<'_>,
    records: ConventionRecords,
    defer_internal_definitions: bool,
    positive_target_attributes: bool,
) -> (Vec<(&'static str, SourceSpan, usize)>, Vec<DeferredRefusal>) {
    let ConventionRecords {
        mut unsettled,
        accepted,
        definitions,
        uses,
    } = records;
    let mut marks = HashMap::<SymbolId, HashSet<&'static str>>::new();
    let mut internal = HashSet::new();
    if !accepted.is_empty() {
        evocc_chir::inline::for_each_declaration(unit, |declaration| {
            if let CHirDecl::Function(function) = declaration {
                if function.storage.base() == StorageClass::Static {
                    internal.insert(function.symbol);
                }
                for attribute in &function.attributes {
                    if positive_target_attributes && positive_target_attribute(attribute) {
                        continue;
                    }
                    if let Some((name, _)) = accepted
                        .contains(&span_key(attribute.span))
                        .then(|| attribute.semantic_name().and_then(convention_attribute))
                        .flatten()
                    {
                        marks.entry(function.symbol).or_default().insert(name);
                    }
                }
            }
        });
    }
    let refusal = |names: &HashSet<&'static str>, definition: bool| {
        CONVENTION_ATTRIBUTES
            .iter()
            .find(|(name, _)| names.contains(name) && !convention_permits(name, definition))
            .map(|(_, message)| *message)
    };
    let functions = marks
        .iter()
        .map(|(symbol, names)| (*symbol, (refusal(names, true), refusal(names, false))))
        .collect::<HashMap<_, _>>();
    let mut deferred = Vec::new();
    for definition in definitions {
        if let Some((Some(message), _)) = functions.get(&definition.function) {
            // On Darwin, Clang's resource headers define their intrinsics as
            // internal functions with `target`. Such a definition is refused
            // only when the unit keeps the function (EvoIR 10.1.9).
            if defer_internal_definitions && internal.contains(&definition.function) {
                deferred.push((
                    evocc_chir::ProfileRefusal {
                        function: definition.function,
                        code: "E2952",
                        message,
                        span: definition.span,
                    },
                    definition.preceding,
                    None,
                ));
            } else {
                unsettled.push((message, definition.span, definition.preceding));
            }
        }
    }
    for usage in uses {
        let Some((_, Some(message))) = functions.get(&usage.function) else {
            continue;
        };
        match usage.owner {
            Some(function) => deferred.push((
                evocc_chir::ProfileRefusal {
                    function,
                    code: "E2952",
                    message,
                    span: usage.span,
                },
                usage.preceding,
                None,
            )),
            None => unsettled.push((message, usage.span, usage.preceding)),
        }
    }
    (unsettled, deferred)
}
