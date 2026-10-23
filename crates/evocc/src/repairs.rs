//! Edits are derived from checked declarations and unexpanded source tokens.
use crate::diagnostics::{Diagnostic, Repair, Span};
use crate::DriverOptions;
use evocc_chir::{constant, inline, lints::Finding, *};
use evocc_common::SourceSpan;
use evocc_lex::{Lexer, TokenKind as K};
use evocc_types::TypeKind as Type;
use evocc_types::{ArrayBound, FloatKind, IntRank, TargetDataLayout, TypeId, TypeInterner};
use std::{error::Error, process::ExitCode};

// Render a C abstract declarator, not a debug name or a host type spelling.
fn c_type(types: &TypeInterner, id: TypeId, name: &str) -> Option<String> {
    let t = types.get(id)?;
    let q = t.qualifiers;
    if q.is_atomic {
        return None;
    }
    let qualifiers = format!(
        "{}{}{}",
        if q.is_const { "const " } else { "" },
        if q.is_volatile { "volatile " } else { "" },
        if q.is_restrict { "restrict " } else { "" }
    );
    let base = match &t.kind {
        Type::Pointer(inner) => return c_type(types, *inner, &format!("(*{qualifiers}{name})")),
        Type::Array {
            element,
            bound: ArrayBound::Fixed(n),
        } => return c_type(types, *element, &format!("{name}[{n}]")),
        Type::Function {
            return_type,
            parameters,
        } => {
            let args = match parameters {
                evocc_types::FunctionParameters::Unspecified => String::new(),
                evocc_types::FunctionParameters::Prototype {
                    types: args,
                    variadic,
                } => {
                    let mut args = args
                        .iter()
                        .map(|id| c_type(types, *id, ""))
                        .collect::<Option<Vec<_>>>()?;
                    if *variadic {
                        args.push("...".into());
                    }
                    if args.is_empty() {
                        args.push("void".into());
                    }
                    args.join(", ")
                }
            };
            return c_type(types, *return_type, &format!("{name}({args})"));
        }
        Type::Void => "void",
        Type::Int(rank) => match rank {
            IntRank::Bool => "_Bool",
            IntRank::Char | IntRank::SignedPlainChar => "char",
            IntRank::SChar => "signed char",
            IntRank::UChar => "unsigned char",
            IntRank::Short => "short",
            IntRank::UShort => "unsigned short",
            IntRank::Int => "int",
            IntRank::UInt => "unsigned int",
            IntRank::Long => "long",
            IntRank::ULong => "unsigned long",
            IntRank::LongLong => "long long",
            IntRank::ULongLong => "unsigned long long",
        },
        Type::Float(FloatKind::F32) => "float",
        Type::Float(FloatKind::F64) => "double",
        Type::Float(FloatKind::F128) => "long double",
        Type::GnuInt128(evocc_types::GnuInt128Signedness::Signed) => "__int128",
        Type::GnuInt128(evocc_types::GnuInt128Signedness::Unsigned) => "unsigned __int128",
        // A local/anonymous tag has no valid file-scope spelling. Do not invent one.
        _ => return None,
    };
    Some(format!("{qualifiers}{base} {name}").trim().into())
}

pub fn suggest<'a>(
    finding: Finding,
    findings: &[Finding],
    unit: &CHirUnit<'a>,
    location: &Span,
    expanded: &[SourceSpan],
    target: &TargetDataLayout,
) -> Option<Repair> {
    let source = &location.source;
    let mut declarations = Vec::new();
    inline::for_each_declaration(unit, |d| declarations.push(d));
    let contains = |s: SourceSpan| {
        s.file == finding.span.file
            && s.byte_start <= finding.span.byte_start
            && finding.span.byte_end <= s.byte_end
    };
    let function = declarations.iter().find_map(|d| match d {
        CHirDecl::Function(f) if f.body.is_some() && contains(f.span) => Some(f),
        _ => None,
    });
    let types = unit.types();
    let symbols = unit.symbols();
    let inert_action = |a: &InitAction<'a>| {
        let ty = a.destination_type();
        inert(types, ty)
            && match a {
                InitAction::StoreScalar { value: v, .. } => {
                    constant::eval_executable_integer_constant(v, types, symbols, target).is_some()
                        || (!unit.runtime_floating_environment()
                            && constant::floating_constant_representation(
                                v, ty, types, symbols, target,
                            )
                            .is_some())
                        || matches!(v, CHirExpr::StringLit(..))
                }
                InitAction::ZeroScalar { .. } | InitAction::StoreString { .. } => true,
                _ => false,
            }
    };
    let mut lexer = Lexer::from_bytes(finding.span.file, source);
    let tokens: Vec<_> = std::iter::from_fn(|| lexer.next()).collect();
    let mut edit = finding.span;
    let id = [
        "W0001.remove-declaration",
        "W0002.void-cast",
        "W0003.insert-prototype",
        "W0004.insert-int",
        "W0005.return-zero",
        "W0006.no-repair",
        "W0007.no-repair",
        "W0008.no-repair",
        "W0009.no-repair",
    ][finding.lint];
    let title = finding.help();
    let (safe, replacement) = match finding.lint {
        0 => {
            let object = declarations.iter().find_map(|d| match d {
                CHirDecl::Object(o) if o.name.span == finding.span => Some(o),
                _ => None,
            })?;
            let mut siblings: Vec<_> = declarations
                .iter()
                .filter_map(|d| match d {
                    CHirDecl::Object(o) if o.span == object.span => Some(o),
                    _ => None,
                })
                .collect();
            siblings.sort_by_key(|o| o.written.span.byte_start);
            let unused = |o: &&ObjectDecl<'a>| {
                findings
                    .iter()
                    .any(|f| f.lint == 0 && f.span == o.name.span)
            };
            let index = siblings.iter().position(|o| o.symbol == object.symbol)?;
            let mut lo = index;
            let mut hi = index + 1;
            while lo > 0 && unused(&siblings[lo - 1]) {
                lo -= 1;
            }
            while hi < siblings.len() && unused(&siblings[hi]) {
                hi += 1;
            }
            edit = object.span;
            // Adjacent unused declarators share one edit, including their commas.
            if lo != 0 || hi != siblings.len() {
                let comma = |i: usize| {
                    // Require the separator, not a comma inside an initializer or attribute.
                    tokens
                        .iter()
                        .rev()
                        .find(|t| t.span.byte_end <= siblings[i].written.span.byte_start)
                        .filter(|t| t.kind == K::Comma)
                        .map(|t| t.span.byte_start)
                };
                edit.byte_start = if lo == 0 {
                    siblings[lo].written.span.byte_start
                } else {
                    comma(lo)?
                };
                edit.byte_end = if hi == siblings.len() {
                    edit.byte_end - 1
                } else if lo == 0 {
                    siblings[hi].written.span.byte_start
                } else {
                    comma(hi)?
                };
            } else {
                if declarations
                    .iter()
                    .any(|d| matches!(d, CHirDecl::Tag(t) if t.span == object.span))
                {
                    return None;
                }
                let preceding: Vec<_> = tokens
                    .iter()
                    .filter(|t| t.span.byte_end <= edit.byte_start)
                    .map(|t| t.kind)
                    .collect();
                if preceding.ends_with(&[K::For, K::LParen]) {
                    edit.byte_end -= 1;
                } else {
                    let start = edit.byte_start as usize;
                    let end = edit.byte_end as usize;
                    let line_start = source
                        .get(..start)?
                        .iter()
                        .rposition(|b| *b == b'\n')
                        .map_or(0, |i| i + 1);
                    let line_end = source
                        .get(end..)?
                        .iter()
                        .position(|b| *b == b'\n')
                        .map_or(source.len(), |i| end + i + 1);
                    // Remove the line only when no neighboring code or comment remains.
                    if source[line_start..start]
                        .iter()
                        .chain(&source[end..line_end])
                        .all(u8::is_ascii_whitespace)
                    {
                        edit.byte_start = line_start as u32;
                        edit.byte_end = line_end as u32;
                    }
                }
            }
            let safe = |o: &&ObjectDecl<'a>| {
                inert(unit.types(), o.ty)
                    && o.attributes.is_empty()
                    && o.asm_label.is_none()
                    && o.cleanup.is_none()
                    && o.written.layers.iter().all(|layer| match layer {
                        WrittenDeclaratorLayer::Array { bound, .. } => {
                            matches!(bound, ArrayBound::Fixed(_) | ArrayBound::Unbounded)
                        }
                        _ => true,
                    })
                    && o.initializer
                        .as_ref()
                        .is_some_and(|i| i.plan.actions().iter().all(inert_action))
            };
            (siblings[lo..hi].iter().all(safe), String::new())
        }
        1 => {
            let function = function?;
            let parameter = function
                .parameters
                .parameters()
                .iter()
                .find(|p| p.name.as_ref().is_some_and(|n| n.span == finding.span))?;
            let body = function.body?.span();
            if source.get(body.byte_start as usize) != Some(&b'{') {
                return None;
            }
            edit = body;
            edit.byte_start += 1;
            edit.byte_end = edit.byte_start;
            (
                inert(types, parameter.ty),
                format!(" (void){};", parameter.name.as_ref()?.spelling),
            )
        }
        2 => {
            let symbol = symbols
                .symbols_in_source_order()
                .iter()
                .find(|s| s.span() == finding.span)?;
            let args = unit
                .unprototyped_parameters(symbol.id())?
                .iter()
                .map(|id| c_type(unit.types(), *id, ""))
                .collect::<Option<Vec<_>>>()?;
            edit = function?.span;
            edit.byte_end = edit.byte_start;
            let args = match args.as_slice() {
                [] => "void".into(),
                _ => args.join(", "),
            };
            (false, format!("int {}({args});\n", symbol.spelling()))
        }
        3 => {
            edit.byte_end = edit.byte_start;
            (true, "int ".into())
        }
        4 => {
            let function = function?;
            let Type::Function { return_type, .. } = unit.types().get(function.ty)?.kind else {
                return None;
            };
            if !matches!(
                unit.types().get(return_type)?.kind,
                Type::Int(_) | Type::Float(_) | Type::Pointer(_) | Type::GnuInt128(_)
            ) {
                return None;
            }
            edit = function.body?.span();
            edit.byte_start = edit.byte_end.checked_sub(1)?;
            if source.get(edit.byte_start as usize) != Some(&b'}') {
                return None;
            }
            edit.byte_end = edit.byte_start;
            (false, "return 0; ".into())
        }
        _ => return None,
    };
    // Expansion coordinates locate an invocation, not editable declaration text.
    if edit.file != finding.span.file
        || expanded.iter().any(|s| {
            s.file == edit.file
                && if edit.byte_start == edit.byte_end {
                    // Keep insertions at expansion boundaries conservative.
                    s.byte_start <= edit.byte_start && edit.byte_start <= s.byte_end
                } else {
                    s.byte_start < edit.byte_end && edit.byte_start < s.byte_end
                }
        })
        || expanded.iter().any(|s| contains(*s))
    {
        return None;
    }
    let prefix = source.get(..edit.byte_start as usize)?;
    source.get(edit.byte_start as usize..edit.byte_end as usize)?;
    edit.line_start = prefix.iter().filter(|b| **b == b'\n').count() as u32 + 1;
    edit.column_start = prefix.iter().rev().take_while(|b| **b != b'\n').count() as u32 + 1;
    Some(Repair {
        range: edit.byte_start as usize..edit.byte_end as usize,
        repair_id: id,
        kind: if safe { "safe" } else { "review" },
        span: Span::source(&location.file, source, edit, title),
        replacement,
        title,
    })
}

fn inert(types: &TypeInterner, id: TypeId) -> bool {
    use evocc_types::{TagDefinition as Definition, TagState};
    types.get(id).is_some_and(|t| {
        !t.qualifiers.is_volatile
            && !t.qualifiers.is_atomic
            && match t.kind {
                Type::Array { element, .. } => inert(types, element),
                Type::Struct(tag) | Type::Union(tag) => match &types.tag(tag).unwrap().state {
                    TagState::Complete(Definition::Struct(fields) | Definition::Union(fields)) => {
                        fields.iter().all(|field| inert(types, field.ty))
                    }
                    _ => false,
                },
                _ => true,
            }
    })
}

pub fn finish(options: &DriverOptions, records: &[Diagnostic]) -> Result<ExitCode, Box<dyn Error>> {
    use crate::diagnostics::{quoted, render, Color, Format};
    use std::{fs, process::Command};
    let mode = options.repair_mode.as_deref().unwrap();
    let all: Vec<_> = records
        .iter()
        .flat_map(|d| &d.repairs)
        .filter(|r| r.span.file == options.input)
        .collect();
    let mut selected: Vec<_> = all
        .iter()
        .filter(|r| mode == "review" || (mode == "safe" && r.kind == "safe"))
        .copied()
        .collect();
    selected.sort_by_key(|r| (r.range.start, r.range.end, r.repair_id));
    selected.dedup_by(|a, b| a.range == b.range && a.replacement == b.replacement);
    for pair in selected.windows(2) {
        if pair[0].range.end > pair[1].range.start {
            return Err("repair spans overlap; apply them separately".into());
        }
    }
    let mut rows = records
        .iter()
        .map(|d| render(d, Format::Json, Color::Never).trim().to_owned())
        .collect::<Vec<_>>()
        .join(",");
    let mut exit = 0;
    if !selected.is_empty() {
        let original = options.diagnostic_source(&options.input);
        if !fs::metadata(&options.input)?.is_file() || fs::read(&options.input)? != original {
            return Err("repair source is not an unchanged regular file".into());
        }
        let mut bytes = original.clone();
        for r in selected.iter().rev() {
            bytes.splice(r.range.clone(), r.replacement.bytes());
        }
        let mut check = Command::new(std::env::current_exe()?);
        check.args(
            std::env::args()
                .skip(1)
                .filter(|s| !s.starts_with("--repair=")),
        );
        fs::write(&options.input, &bytes)?;
        let checked = check.output();
        if !checked.as_ref().is_ok_and(|out| out.status.success()) {
            fs::write(&options.input, original)?;
        }
        let out = checked?;
        exit = out.status.code().unwrap_or(1);
        rows = std::str::from_utf8(&out.stderr)?
            .lines()
            .collect::<Vec<_>>()
            .join(",");
        if options.error_format != Format::Json {
            use std::io::Write;
            std::io::stderr().write_all(&out.stderr)?;
        }
    }
    let applied = if exit == 0 { selected.len() } else { 0 };
    if options.error_format == Format::Json {
        println!("{{\"file\":{},\"repairs\":[{}],\"diagnostics\":[{rows}],\"applied\":{applied},\"exit_code\":{exit}}}", quoted(&options.input), all.iter().map(|r| r.json()).collect::<Vec<_>>().join(","));
    } else {
        for r in all {
            let replacement = quoted(&r.replacement);
            println!("{} [{}]: {} {replacement}", r.repair_id, r.kind, r.title);
        }
        println!("applied {applied} repairs; syntax check exit {exit}");
    }
    Ok(ExitCode::from(exit as u8))
}
