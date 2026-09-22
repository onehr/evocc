//! Source type metadata for the normal ELF DWARF-to-BTF toolchain.
//!
//! The canonical C type table and target layout remain the authorities. This
//! does not infer C types from lowered bit vectors or claim local-variable
//! locations/line tables. DWARF 4 sections use CU-relative references, so the
//! assembler/linker can combine independent translation units normally.

use std::collections::BTreeMap;
use std::fmt::Write;

use evocc_chir::{
    CHirDecl, CHirUnit, ParameterSyntax, SymbolId, TagMembers, TypeSpecifier,
    WrittenDeclaratorLayer,
};
use evocc_evoir::{GlobalKind, GlobalVisibility, Module};
use evocc_types::{
    ArrayBound, CanonicalType, ComplexBase, FloatKind, FunctionParameters, GnuInt128Signedness,
    IntRank, TagDefinition, TagId, TagState, TargetBuiltinType, TargetDataLayout, TypeId, TypeKind,
};

// DWARF attributes and forms, from dwarfstd.org/doc/DWARF4.pdf, Section 7.
const NAME: u16 = 0x03;
const BYTE_SIZE: u16 = 0x0b;
const TYPE: u16 = 0x49;
const DECLARATION: u16 = 0x3c;
const EXTERNAL: u16 = 0x3f;

enum Attribute {
    String(String),
    Unsigned(u64),
    Signed(i64),
    Reference(String),
    Address(String),
    ObjectLocation(String),
    Flag(bool),
}

impl Attribute {
    fn form(&self) -> u16 {
        match self {
            Self::String(_) => 0x08,         // DW_FORM_string
            Self::Unsigned(_) => 0x0f,       // DW_FORM_udata
            Self::Signed(_) => 0x0d,         // DW_FORM_sdata
            Self::Reference(_) => 0x13,      // DW_FORM_ref4
            Self::Address(_) => 0x01,        // DW_FORM_addr
            Self::ObjectLocation(_) => 0x18, // DW_FORM_exprloc
            Self::Flag(_) => 0x0c,           // DW_FORM_flag
        }
    }
}

#[derive(PartialEq, Eq)]
struct Abbreviation {
    tag: u16,
    children: bool,
    fields: Vec<(u16, u16)>,
}

#[derive(Default)]
struct Writer {
    text: String,
    abbreviations: Vec<Abbreviation>,
    source_types: BTreeMap<(TypeId, SymbolId, TypeId), String>,
    member_types: BTreeMap<(TagId, usize), String>,
    written_declarators: usize,
}

fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for &byte in text.as_bytes() {
        match byte {
            b'"' | b'\\' => {
                out.push('\\');
                out.push(char::from(byte));
            }
            32..=126 => out.push(char::from(byte)),
            _ => {
                let _ = write!(out, "\\{byte:03o}");
            }
        }
    }
    out.push('"');
    out
}

fn name(text: &str) -> (u16, Attribute) {
    (NAME, Attribute::String(text.to_owned()))
}

fn ty(id: TypeId) -> (u16, Attribute) {
    (TYPE, Attribute::Reference(type_label(id)))
}

fn type_label(id: TypeId) -> String {
    format!(".L_evocc_type_{}", id.raw())
}

fn alias_label(symbol: SymbolId) -> String {
    format!(".L_evocc_typedef_{}", symbol.raw())
}

impl Writer {
    /// Keep source typedef edges in metadata without changing canonical C
    /// type identity. BTF deduplication observes these edges inside records.
    fn source_type(
        &mut self,
        unit: &CHirUnit<'_>,
        id: TypeId,
        specifier: &TypeSpecifier,
    ) -> Result<String, String> {
        let TypeSpecifier::TypedefName {
            symbol, ty: base, ..
        } = specifier
        else {
            return Ok(type_label(id));
        };
        let key = (id, *symbol, *base);
        if let Some(label) = self.source_types.get(&key) {
            return Ok(label.clone());
        }
        let canonical = unit.types().get(id).ok_or("missing written type")?;
        let alias = unit.symbols().symbol(*symbol).map_err(|e| e.to_string())?;
        let alias_type = unit.types().get(alias.ty()).ok_or("missing typedef type")?;
        if id != *base
            && matches!(canonical.kind, TypeKind::Array { .. })
            && matches!(alias_type.kind, TypeKind::Array { .. })
        {
            // A further array declarator combines the dimensions of an
            // array typedef. GNU DWARF encodes the complete matrix in one
            // array DIE, while retaining the typedef declaration itself.
            let target = type_label(id);
            self.source_types.insert(key, target.clone());
            return Ok(target);
        }
        let label = format!(".L_evocc_written_type_{}", self.source_types.len());
        // Reserve before recursively processing declarator layers.
        self.source_types.insert(key, label.clone());
        let (kind, attributes, children) = if id == *base && canonical.kind == alias_type.kind {
            let qualifiers = canonical.qualifiers;
            let inherited = alias_type.qualifiers;
            let tags: Vec<_> = [
                (qualifiers.is_const && !inherited.is_const, 0x26),
                (qualifiers.is_volatile && !inherited.is_volatile, 0x35),
                (qualifiers.is_restrict && !inherited.is_restrict, 0x37),
                (qualifiers.is_atomic && !inherited.is_atomic, 0x47),
            ]
            .into_iter()
            .filter_map(|(enabled, tag)| enabled.then_some(tag))
            .collect();
            let mut target = alias_label(*symbol);
            for (index, tag) in tags.iter().enumerate().rev() {
                let current = if index == 0 {
                    label.clone()
                } else {
                    format!("{label}_{index}")
                };
                self.label(&current);
                self.node(*tag, false, &[(TYPE, Attribute::Reference(target))]);
                target = current;
            }
            self.source_types.insert(key, target.clone());
            return Ok(target);
        } else {
            match canonical.kind {
                TypeKind::Pointer(pointee) => (
                    0x0f,
                    vec![
                        (BYTE_SIZE, Attribute::Unsigned(8)),
                        (
                            TYPE,
                            Attribute::Reference(self.source_type(unit, pointee, specifier)?),
                        ),
                    ],
                    false,
                ),
                TypeKind::Array { .. } => {
                    let (element, _) = array_dimensions(unit, id, Some(*base));
                    (
                        0x01,
                        vec![(
                            TYPE,
                            Attribute::Reference(self.source_type(unit, element, specifier)?),
                        )],
                        true,
                    )
                }
                TypeKind::Function {
                    return_type,
                    ref parameters,
                } => (
                    0x15,
                    vec![
                        (
                            TYPE,
                            Attribute::Reference(self.source_type(unit, return_type, specifier)?),
                        ),
                        (
                            0x27,
                            Attribute::Flag(matches!(
                                parameters,
                                FunctionParameters::Prototype { .. }
                            )),
                        ),
                    ],
                    has_parameters(parameters),
                ),
                _ => {
                    // A source type attribute may replace the base kind (for
                    // example vector_size). Its real canonical type wins.
                    let target = type_label(id);
                    self.source_types.insert(key, target.clone());
                    return Ok(target);
                }
            }
        };
        self.label(&label);
        let mut next = label.clone();
        for (enabled, tag) in [
            (canonical.qualifiers.is_const, 0x26),
            (canonical.qualifiers.is_volatile, 0x35),
            (canonical.qualifiers.is_restrict, 0x37),
            (canonical.qualifiers.is_atomic, 0x47),
        ] {
            if enabled {
                next.push('_');
                self.node(tag, false, &[(TYPE, Attribute::Reference(next.clone()))]);
                self.label(&next);
            }
        }
        self.node(kind, children, &attributes);
        match &canonical.kind {
            TypeKind::Array { .. } => {
                let (_, dimensions) = array_dimensions(unit, id, Some(*base));
                self.array_subranges(&dimensions);
            }
            TypeKind::Function { parameters, .. } => self.parameters(parameters, &[], &[]),
            _ => {}
        }
        Ok(label)
    }

    /// The source parameter list distinguishes callbacks whose canonical
    /// signatures are equal but whose DWARF typedef edges are different.
    fn written_type(
        &mut self,
        unit: &CHirUnit<'_>,
        id: TypeId,
        specifier: &TypeSpecifier,
        layers: &[WrittenDeclaratorLayer],
    ) -> Result<String, String> {
        if !layers.iter().any(|layer| {
            matches!(
                layer,
                WrittenDeclaratorLayer::Function {
                    parameters: Some(_),
                    ..
                }
            )
        }) {
            return self.source_type(unit, id, specifier);
        }
        let canonical = unit
            .types()
            .get(id)
            .ok_or("missing written declarator type")?;
        let label = format!(".L_evocc_declarator_{}", self.written_declarators);
        self.written_declarators += 1;
        let (kind, attributes, parameters, dimensions) = match (&canonical.kind, layers.first()) {
            (TypeKind::Pointer(pointee), Some(layer)) => {
                // Function/array parameter adjustment adds a pointer even
                // when no '*' appears in the written parameter declarator.
                let rest = if matches!(layer, WrittenDeclaratorLayer::Function { .. }) {
                    layers
                } else {
                    &layers[1..]
                };
                let target = self.written_type(unit, *pointee, specifier, rest)?;
                (
                    0x0f,
                    vec![
                        (BYTE_SIZE, Attribute::Unsigned(8)),
                        (TYPE, Attribute::Reference(target)),
                    ],
                    None,
                    Vec::new(),
                )
            }
            (TypeKind::Array { .. }, Some(WrittenDeclaratorLayer::Array { .. })) => {
                let mut element = id;
                let mut dimensions = Vec::new();
                for layer in layers {
                    let Some(CanonicalType {
                        kind:
                            TypeKind::Array {
                                element: next,
                                bound,
                            },
                        ..
                    }) = unit.types().get(element)
                    else {
                        break;
                    };
                    if !matches!(layer, WrittenDeclaratorLayer::Array { .. }) {
                        break;
                    }
                    dimensions.push(bound.clone());
                    element = *next;
                }
                let target =
                    self.written_type(unit, element, specifier, &layers[dimensions.len()..])?;
                (
                    0x01,
                    vec![(TYPE, Attribute::Reference(target))],
                    None,
                    dimensions,
                )
            }
            (
                TypeKind::Function {
                    return_type,
                    parameters,
                },
                Some(WrittenDeclaratorLayer::Function {
                    parameters: Some(syntax),
                    ..
                }),
            ) => {
                let target = self.written_type(unit, *return_type, specifier, &layers[1..])?;
                let parameter_types = self.parameter_types(unit, parameters, syntax)?;
                (
                    0x15,
                    vec![
                        (TYPE, Attribute::Reference(target)),
                        (
                            0x27,
                            Attribute::Flag(matches!(
                                parameters,
                                FunctionParameters::Prototype { .. }
                            )),
                        ),
                    ],
                    Some((parameters, parameter_types)),
                    Vec::new(),
                )
            }
            _ => return self.source_type(unit, id, specifier),
        };
        self.label(&label);
        let mut next = label.clone();
        for (enabled, tag) in [
            (canonical.qualifiers.is_const, 0x26),
            (canonical.qualifiers.is_volatile, 0x35),
            (canonical.qualifiers.is_restrict, 0x37),
            (canonical.qualifiers.is_atomic, 0x47),
        ] {
            if enabled {
                next.push('_');
                self.node(tag, false, &[(TYPE, Attribute::Reference(next.clone()))]);
                self.label(&next);
            }
        }
        self.node(
            kind,
            !dimensions.is_empty() || parameters.as_ref().is_some_and(|(p, _)| has_parameters(p)),
            &attributes,
        );
        if let Some((parameters, types)) = parameters {
            // A subroutine type does not bind parameter variables. Preserve
            // their names on subprogram DIEs, not on callback type identity.
            self.parameters(parameters, &[], &types);
        } else if !dimensions.is_empty() {
            self.array_subranges(&dimensions);
        }
        Ok(label)
    }

    fn parameter_types(
        &mut self,
        unit: &CHirUnit<'_>,
        parameters: &FunctionParameters,
        syntax: &ParameterSyntax,
    ) -> Result<Vec<String>, String> {
        let FunctionParameters::Prototype { types, .. } = parameters else {
            return Ok(Vec::new());
        };
        types
            .iter()
            .enumerate()
            .map(|(index, &id)| {
                if let Some(source) = syntax.parameters().get(index) {
                    self.written_type(unit, id, &source.specifier, &source.written.layers)
                } else {
                    Ok(type_label(id))
                }
            })
            .collect()
    }

    fn array_subranges(&mut self, dimensions: &[ArrayBound]) {
        for bound in dimensions {
            let mut attrs = vec![(TYPE, Attribute::Reference(".L_evocc_index_type".into()))];
            if let ArrayBound::Fixed(count) = bound {
                attrs.push((0x37, Attribute::Unsigned(*count)));
            }
            self.node(0x21, false, &attrs);
        }
        self.end_children();
    }

    fn label(&mut self, name: &str) {
        let _ = writeln!(self.text, "{name}:");
    }

    fn node(&mut self, tag: u16, children: bool, attributes: &[(u16, Attribute)]) {
        let abbreviation = Abbreviation {
            tag,
            children,
            fields: attributes
                .iter()
                .map(|(key, value)| (*key, value.form()))
                .collect(),
        };
        let index = self
            .abbreviations
            .iter()
            .position(|a| *a == abbreviation)
            .unwrap_or_else(|| {
                self.abbreviations.push(abbreviation);
                self.abbreviations.len() - 1
            });
        let _ = writeln!(self.text, ".uleb128 {}", index + 1);
        for (_, value) in attributes {
            match value {
                Attribute::String(value) => {
                    let _ = writeln!(self.text, ".asciz {}", quoted(value));
                }
                Attribute::Unsigned(value) => {
                    let _ = writeln!(self.text, ".uleb128 {value}");
                }
                Attribute::Signed(value) => {
                    let _ = writeln!(self.text, ".sleb128 {value}");
                }
                Attribute::Reference(label) => {
                    let _ = writeln!(self.text, ".4byte {label} - .L_evocc_debug_cu");
                }
                Attribute::Address(symbol) => {
                    let _ = writeln!(self.text, ".8byte {}", quoted(symbol));
                }
                Attribute::ObjectLocation(symbol) => {
                    // One pointer-sized DW_OP_addr, relocated by the ELF linker.
                    let _ = writeln!(self.text, ".uleb128 9\n.byte 3\n.8byte {}", quoted(symbol));
                }
                Attribute::Flag(value) => {
                    let _ = writeln!(self.text, ".byte {}", u8::from(*value));
                }
            }
        }
    }

    fn end_children(&mut self) {
        self.text.push_str(".byte 0\n");
    }

    fn finish(mut self) -> String {
        self.text.push_str(
            ".L_evocc_debug_end:\n.section .debug_abbrev,\"\",%progbits\n.L_evocc_debug_abbrev:\n",
        );
        for (index, abbreviation) in self.abbreviations.iter().enumerate() {
            let _ = writeln!(
                self.text,
                ".uleb128 {}\n.uleb128 {}\n.byte {}",
                index + 1,
                abbreviation.tag,
                u8::from(abbreviation.children)
            );
            for (attribute, form) in &abbreviation.fields {
                let _ = writeln!(self.text, ".uleb128 {attribute}\n.uleb128 {form}");
            }
            self.text.push_str(".byte 0, 0\n");
        }
        self.text.push_str(".byte 0\n");
        self.text
    }

    fn parameters(
        &mut self,
        parameters: &FunctionParameters,
        names: &[Option<&str>],
        written: &[String],
    ) {
        if !has_parameters(parameters) {
            return;
        }
        match parameters {
            FunctionParameters::Prototype { types, variadic } => {
                for (index, &parameter) in types.iter().enumerate() {
                    let mut attributes = vec![(
                        TYPE,
                        Attribute::Reference(
                            written
                                .get(index)
                                .cloned()
                                .unwrap_or_else(|| type_label(parameter)),
                        ),
                    )];
                    if let Some(Some(spelling)) = names.get(index) {
                        attributes.push(name(spelling));
                    }
                    self.node(0x05, false, &attributes); // formal_parameter
                }
                if *variadic {
                    self.node(0x18, false, &[]); // unspecified_parameters
                }
            }
            FunctionParameters::Unspecified => self.node(0x18, false, &[]),
        }
        self.end_children();
    }

    fn c_type(
        &mut self,
        unit: &CHirUnit<'_>,
        target: &TargetDataLayout,
        id: TypeId,
        canonical: &CanonicalType,
    ) -> Result<(), String> {
        let qualifiers = canonical.qualifiers;
        let mut label = type_label(id);
        self.label(&label);
        for (enabled, tag) in [
            (qualifiers.is_const, 0x26),
            (qualifiers.is_volatile, 0x35),
            (qualifiers.is_restrict, 0x37),
            (qualifiers.is_atomic, 0x47),
        ] {
            if enabled {
                label.push('_');
                self.node(tag, false, &[(TYPE, Attribute::Reference(label.clone()))]);
                self.label(&label);
            }
        }
        match &canonical.kind {
            TypeKind::Void => self.node(0x3b, false, &[name("void")]),
            TypeKind::Int(rank) => {
                let integer = evocc_chir::integer::integer_type_for_rank(*rank);
                self.node(
                    0x24,
                    false,
                    &[
                        name(integer_name(*rank)),
                        (
                            BYTE_SIZE,
                            Attribute::Unsigned(u64::from(integer.bits()).div_ceil(8)),
                        ),
                        (
                            0x3e,
                            Attribute::Unsigned(if *rank == IntRank::Bool {
                                2
                            } else if matches!(rank, IntRank::Char | IntRank::UChar) {
                                8
                            } else if matches!(rank, IntRank::SChar | IntRank::SignedPlainChar) {
                                6
                            } else if integer.is_signed() {
                                5
                            } else {
                                7
                            }),
                        ),
                    ],
                );
            }
            TypeKind::GnuInt128(signedness) => {
                let signed = *signedness == GnuInt128Signedness::Signed;
                self.node(
                    0x24,
                    false,
                    &[
                        name(if signed {
                            "__int128"
                        } else {
                            "__int128 unsigned"
                        }),
                        (BYTE_SIZE, Attribute::Unsigned(16)),
                        (0x3e, Attribute::Unsigned(if signed { 5 } else { 7 })),
                    ],
                );
            }
            TypeKind::Float(kind) => self.base_type(
                float_name(*kind),
                target
                    .layout_of(unit.types(), id)
                    .map_err(|e| e.to_string())?
                    .size_bytes,
                4,
            ),
            TypeKind::Complex(base) => self.base_type(
                &format!(
                    "{} _Complex",
                    match base {
                        ComplexBase::Integer(rank) => integer_name(*rank),
                        ComplexBase::CF16 => float_name(FloatKind::F16),
                        ComplexBase::CF32 => float_name(FloatKind::F32),
                        ComplexBase::CF64 => float_name(FloatKind::F64),
                        ComplexBase::CF128 => float_name(FloatKind::F128),
                    }
                ),
                target
                    .layout_of(unit.types(), id)
                    .map_err(|e| e.to_string())?
                    .size_bytes,
                3,
            ),
            TypeKind::TargetBuiltin(builtin) => {
                if let Some(layout) = builtin.layout() {
                    let encoding = match builtin {
                        TargetBuiltinType::ArmPoly8
                        | TargetBuiltinType::ArmPoly16
                        | TargetBuiltinType::ArmPoly64
                        | TargetBuiltinType::ArmPoly128 => 7,
                        _ => 4,
                    };
                    self.base_type(builtin.spelling(), layout.size_bytes, encoding);
                } else {
                    // Sizeless target builtins do not have a fabricated size.
                    self.node(0x3b, false, &[name(builtin.spelling())]);
                }
            }
            TypeKind::BuiltinVaList => {
                // Opaque source builtin: record the known layout, not invented fields.
                self.node(
                    0x13,
                    false,
                    &[
                        name("__builtin_va_list"),
                        (BYTE_SIZE, Attribute::Unsigned(32)),
                    ],
                );
            }
            TypeKind::Pointer(pointee) => self.node(
                0x0f,
                false,
                &[
                    (
                        BYTE_SIZE,
                        Attribute::Unsigned(target.pointer_layout().size_bytes),
                    ),
                    ty(*pointee),
                ],
            ),
            TypeKind::Vector { element, lanes, .. } => {
                self.node(0x01, true, &[ty(*element), (0x2107, Attribute::Flag(true))]);
                self.node(
                    0x21,
                    false,
                    &[
                        (TYPE, Attribute::Reference(".L_evocc_index_type".into())),
                        (0x37, Attribute::Unsigned(u64::from(*lanes))),
                    ],
                );
                self.end_children();
            }
            TypeKind::Array { .. } => {
                let (element, dimensions) = array_dimensions(unit, id, None);
                self.node(0x01, true, &[ty(element)]);
                // Runtime/incomplete bounds are unknown, never fabricated as zero.
                self.array_subranges(&dimensions);
            }
            TypeKind::Function {
                return_type,
                parameters,
            } => {
                self.node(
                    0x15,
                    has_parameters(parameters),
                    &[
                        ty(*return_type),
                        (
                            0x27,
                            Attribute::Flag(matches!(
                                parameters,
                                FunctionParameters::Prototype { .. }
                            )),
                        ),
                    ],
                );
                self.parameters(parameters, &[], &[]);
            }
            TypeKind::Struct(tag_id) | TypeKind::Union(tag_id) | TypeKind::Enum(tag_id) => {
                let tag = unit.types().tag(*tag_id).ok_or("missing canonical tag")?;
                let tag_kind = match canonical.kind {
                    TypeKind::Struct(_) => 0x13,
                    TypeKind::Union(_) => 0x17,
                    _ => 0x04,
                };
                let mut attrs = Vec::new();
                if let Some(spelling) = &tag.name {
                    attrs.push(name(spelling));
                }
                let TagState::Complete(definition) = &tag.state else {
                    attrs.push((DECLARATION, Attribute::Flag(true)));
                    self.node(tag_kind, false, &attrs);
                    return Ok(());
                };
                let layout = target
                    .layout_of(unit.types(), id)
                    .map_err(|e| e.to_string())?;
                attrs.push((BYTE_SIZE, Attribute::Unsigned(layout.size_bytes)));
                if let Some(rank) = unit.types().enum_representation(*tag_id) {
                    attrs.push((
                        0x3e,
                        Attribute::Unsigned(
                            if evocc_chir::integer::integer_type_for_rank(rank).is_signed() {
                                5
                            } else {
                                7
                            },
                        ),
                    ));
                }
                let has_members = match definition {
                    TagDefinition::Enum(values) => !values.is_empty(),
                    TagDefinition::Struct(fields) | TagDefinition::Union(fields) => {
                        fields.iter().any(|field| {
                            !field
                                .bit_field
                                .is_some_and(|bits| bits.width_bits == 0 || field.name.is_none())
                        })
                    }
                };
                self.node(tag_kind, has_members, &attrs);
                match definition {
                    TagDefinition::Enum(values) => {
                        let rank = unit
                            .types()
                            .enum_representation(*tag_id)
                            .ok_or("missing enum representation")?;
                        let signed = evocc_chir::integer::integer_type_for_rank(rank).is_signed();
                        for value in values {
                            let number = if signed {
                                Attribute::Signed(
                                    i64::try_from(value.value)
                                        .map_err(|_| "enum value exceeds signed metadata")?,
                                )
                            } else {
                                Attribute::Unsigned(
                                    u64::try_from(value.value)
                                        .map_err(|_| "enum value exceeds unsigned metadata")?,
                                )
                            };
                            self.node(0x28, false, &[name(&value.name), (0x1c, number)]);
                        }
                    }
                    TagDefinition::Struct(fields) | TagDefinition::Union(fields) => {
                        let layout = target
                            .record_layout(unit.types(), *tag_id)
                            .map_err(|e| e.to_string())?;
                        for (index, (field, location)) in
                            fields.iter().zip(layout.fields).enumerate()
                        {
                            if field
                                .bit_field
                                .is_some_and(|bits| bits.width_bits == 0 || field.name.is_none())
                            {
                                continue;
                            }
                            let mut attrs = vec![(
                                TYPE,
                                Attribute::Reference(
                                    self.member_types
                                        .get(&(*tag_id, index))
                                        .cloned()
                                        .unwrap_or_else(|| type_label(field.ty)),
                                ),
                            )];
                            if let Some(spelling) = &field.name {
                                attrs.push(name(spelling));
                            }
                            if let Some(bits) = location.bit_field {
                                attrs.push((0x0d, Attribute::Unsigned(u64::from(bits.width_bits))));
                                attrs.push((
                                    0x6b,
                                    Attribute::Unsigned(
                                        bits.storage_byte_offset * 8 + u64::from(bits.bit_offset),
                                    ),
                                ));
                            } else {
                                attrs.push((0x38, Attribute::Unsigned(location.byte_offset)));
                            }
                            self.node(0x0d, false, &attrs);
                        }
                    }
                }
                if has_members {
                    self.end_children();
                }
            }
        }
        Ok(())
    }

    fn base_type(&mut self, spelling: &str, size: u64, encoding: u64) {
        self.node(
            0x24,
            false,
            &[
                name(spelling),
                (BYTE_SIZE, Attribute::Unsigned(size)),
                (0x3e, Attribute::Unsigned(encoding)),
            ],
        );
    }
}

fn array_dimensions(
    unit: &CHirUnit<'_>,
    mut id: TypeId,
    typedef_base: Option<TypeId>,
) -> (TypeId, Vec<ArrayBound>) {
    let mut dimensions = Vec::new();
    while dimensions.is_empty() || Some(id) != typedef_base {
        let Some(CanonicalType {
            kind: TypeKind::Array { element, bound },
            ..
        }) = unit.types().get(id)
        else {
            break;
        };
        dimensions.push(bound.clone());
        id = *element;
    }
    (id, dimensions)
}

fn float_name(kind: FloatKind) -> &'static str {
    match kind {
        FloatKind::F16 => "__fp16",
        FloatKind::F32 => "float",
        FloatKind::F64 => "double",
        FloatKind::F128 => "long double",
    }
}

fn has_parameters(parameters: &FunctionParameters) -> bool {
    !matches!(parameters, FunctionParameters::Prototype { types, variadic: false } if types.is_empty())
}

fn integer_name(rank: IntRank) -> &'static str {
    // BTF includes base-type names in structural deduplication. Use the
    // frozen GNU toolchain's vocabulary, not abbreviated pretty-print names.
    match rank {
        IntRank::Bool => "_Bool",
        IntRank::Char | IntRank::SignedPlainChar => "char",
        IntRank::SChar => "signed char",
        IntRank::UChar => "unsigned char",
        IntRank::Short => "short int",
        IntRank::UShort => "short unsigned int",
        IntRank::Int => "int",
        IntRank::UInt => "unsigned int",
        IntRank::Long => "long int",
        IntRank::ULong => "long unsigned int",
        IntRank::LongLong => "long long int",
        IntRank::ULongLong => "long long unsigned int",
    }
}

pub(crate) fn type_information(
    unit: &CHirUnit<'_>,
    module: &Module,
    target: &TargetDataLayout,
    input: &str,
) -> Result<String, String> {
    let mut writer = Writer::default();
    writer.text.push_str("\n.section .debug_info,\"\",%progbits\n.L_evocc_debug_cu:\n.4byte .L_evocc_debug_end - .L_evocc_debug_header\n.L_evocc_debug_header:\n.2byte 4\n.4byte .L_evocc_debug_abbrev\n.byte 8\n");
    writer.node(
        0x11,
        true,
        &[
            (0x25, Attribute::String("EvoCC".into())),
            (0x13, Attribute::Unsigned(0x1d)),
            name(input),
        ],
    ); // DW_LANG_C11
    writer.label(".L_evocc_index_type");
    writer.node(
        0x24,
        false,
        &[
            name("unsigned long"),
            (BYTE_SIZE, Attribute::Unsigned(8)),
            (0x3e, Attribute::Unsigned(7)),
        ],
    );
    let linkage = unit.symbol_linkage().map_err(|e| e.message.to_owned())?;
    // Use the backend's exact retention rule; debug relocations must never
    // keep a function alive after executable emission has discarded it.
    let emitted_functions = evocc_mir::emitted_functions(module);
    let mut functions = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    let mut records = BTreeMap::new();
    evocc_chir::inline::for_each_declaration(unit, |decl| match decl {
        CHirDecl::Function(function) if function.body.is_some() => {
            functions.insert(function.symbol, function);
        }
        CHirDecl::Typedef(alias) => {
            aliases.insert(alias.symbol, alias);
        }
        CHirDecl::Tag(tag)
            if matches!(tag.members, TagMembers::Struct(_) | TagMembers::Union(_)) =>
        {
            records.insert(tag.tag, &tag.members);
        }
        _ => {}
    });
    for alias in aliases.into_values() {
        let target =
            writer.written_type(unit, alias.ty, &alias.specifier, &alias.written.layers)?;
        writer.label(&alias_label(alias.symbol));
        writer.node(
            0x16,
            false,
            &[
                name(&alias.name.spelling),
                (TYPE, Attribute::Reference(target)),
            ],
        );
    }
    for (tag, members) in records {
        let (TagMembers::Struct(members) | TagMembers::Union(members)) = members else {
            unreachable!()
        };
        for (index, member) in members.iter().enumerate() {
            let target =
                writer.written_type(unit, member.ty, &member.specifier, &member.written.layers)?;
            writer.member_types.insert((tag, index), target);
        }
    }
    for (id, canonical) in unit.types().iter() {
        writer.c_type(unit, target, id, canonical)?;
    }
    // This is the normalized inventory used by lowering, including lifted
    // inline statics. Never guess their source types from synthetic names.
    let file_objects: BTreeMap<_, _> = unit
        .iter()
        .filter_map(|decl| {
            let CHirDecl::Object(object) = decl else {
                return None;
            };
            let symbol = unit.symbols().symbol(object.symbol).ok()?;
            Some((symbol.spelling(), (symbol.ty(), object)))
        })
        .collect();
    for global in &module.globals {
        if global.kind != GlobalKind::File {
            continue;
        }
        let Some(&(id, object)) = file_objects.get(global.symbol.as_str()) else {
            return Err(format!("file object {} has no source type", global.symbol));
        };
        let written = writer.written_type(unit, id, &object.specifier, &object.written.layers)?;
        let mut attributes = vec![name(&global.symbol), (TYPE, Attribute::Reference(written))];
        // The normal DWARF-to-BTF loader checks attribute presence. Omit
        // false external flags; ELF visibility does not change C linkage.
        if global.visibility != GlobalVisibility::Internal {
            attributes.push((EXTERNAL, Attribute::Flag(true)));
        }
        attributes.push((0x02, Attribute::ObjectLocation(global.symbol.clone())));
        writer.node(0x34, false, &attributes);
    }
    for function in functions.into_values() {
        let spelling = linkage.function_name(function.symbol, &function.name.spelling);
        if !emitted_functions.contains(spelling) {
            continue;
        }
        let Some(CanonicalType {
            kind:
                TypeKind::Function {
                    return_type,
                    parameters,
                },
            ..
        }) = unit.types().get(function.ty)
        else {
            return Err("function has no canonical function type".into());
        };
        let return_layers = function.written.layers.get(1..).unwrap_or_default();
        let written_return = writer.written_type(
            unit,
            *return_type,
            &function.return_specifier,
            return_layers,
        )?;
        let written_parameters = writer.parameter_types(unit, parameters, &function.parameters)?;
        let mut attributes = vec![
            name(spelling),
            (TYPE, Attribute::Reference(written_return)),
            (0x11, Attribute::Address(spelling.into())),
        ];
        if !function.has_internal_linkage(unit.symbols()) {
            attributes.push((EXTERNAL, Attribute::Flag(true)));
        }
        attributes.push((
            0x27,
            Attribute::Flag(matches!(parameters, FunctionParameters::Prototype { .. })),
        ));
        writer.node(0x2e, has_parameters(parameters), &attributes);
        let names = function
            .parameters
            .parameters()
            .iter()
            .map(|parameter| parameter.name.as_ref().map(|name| name.spelling.as_str()))
            .collect::<Vec<_>>();
        writer.parameters(parameters, &names, &written_parameters);
    }
    writer.end_children();
    Ok(writer.finish())
}
