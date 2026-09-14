//! Source type metadata for the normal ELF DWARF-to-BTF toolchain.
//!
//! The canonical C type table and target layout remain the authorities. This
//! does not infer C types from lowered bit vectors or claim local-variable
//! locations/line tables. DWARF 4 sections use CU-relative references, so the
//! assembler/linker can combine independent translation units normally.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use evocc_chir::{CHirDecl, CHirUnit};
use evocc_evoir::{GlobalKind, GlobalVisibility, Module};
use evocc_types::{
    ArrayBound, CanonicalType, ComplexBase, FloatKind, FunctionParameters, GnuInt128Signedness,
    IntRank, TagDefinition, TagState, TargetBuiltinType, TargetDataLayout, TypeId, TypeKind,
};

// DWARF attributes and forms, from dwarfstd.org/doc/DWARF4.pdf, Section 7.
const NAME: u16 = 0x03;
const BYTE_SIZE: u16 = 0x0b;
const TYPE: u16 = 0x49;
const DECLARATION: u16 = 0x3c;

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

impl Writer {
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

    fn parameters(&mut self, parameters: &FunctionParameters, names: &[Option<&str>]) {
        if !has_parameters(parameters) {
            return;
        }
        match parameters {
            FunctionParameters::Prototype { types, variadic } => {
                for (index, &parameter) in types.iter().enumerate() {
                    let mut attributes = vec![ty(parameter)];
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
                            } else if *rank == IntRank::SChar {
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
                            "unsigned __int128"
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
                    float_name(match base {
                        ComplexBase::CF16 => FloatKind::F16,
                        ComplexBase::CF32 => FloatKind::F32,
                        ComplexBase::CF64 => FloatKind::F64,
                        ComplexBase::CF128 => FloatKind::F128,
                    })
                ),
                target
                    .layout_of(unit.types(), id)
                    .map_err(|e| e.to_string())?
                    .size_bytes,
                3,
            ),
            TypeKind::TargetBuiltin(builtin) => {
                match builtin {
                    TargetBuiltinType::ArmFloat32x4 | TargetBuiltinType::ArmFloat64x2 => {
                        let element_label = format!("{label}_element");
                        let (element_name, size, count) =
                            if *builtin == TargetBuiltinType::ArmFloat32x4 {
                                ("float", 4, 4)
                            } else {
                                ("double", 8, 2)
                            };
                        self.node(
                            0x01,
                            true,
                            &[
                                name(builtin.spelling()),
                                (TYPE, Attribute::Reference(element_label.clone())),
                                (0x2107, Attribute::Flag(true)), // DW_AT_GNU_vector
                            ],
                        );
                        self.node(
                            0x21,
                            false,
                            &[
                                (TYPE, Attribute::Reference(".L_evocc_index_type".into())),
                                (0x37, Attribute::Unsigned(count)),
                            ],
                        );
                        self.end_children();
                        self.label(&element_label);
                        self.base_type(element_name, size, 4);
                    }
                    _ => {
                        if let Some(layout) = builtin.layout() {
                            self.base_type(builtin.spelling(), layout.size_bytes, 4);
                        } else {
                            // Sizeless target builtins do not have a fabricated size.
                            self.node(0x3b, false, &[name(builtin.spelling())]);
                        }
                    }
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
            TypeKind::Array { element, bound } => {
                self.node(0x01, true, &[ty(*element)]);
                let mut attrs = vec![(TYPE, Attribute::Reference(".L_evocc_index_type".into()))];
                if let ArrayBound::Fixed(count) = bound {
                    attrs.push((0x37, Attribute::Unsigned(*count)));
                }
                // Runtime/incomplete bounds are unknown, never fabricated as zero.
                self.node(0x21, false, &attrs);
                self.end_children();
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
                self.parameters(parameters, &[]);
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
                    TagDefinition::Struct(fields) | TagDefinition::Union(fields) => fields
                        .iter()
                        .any(|field| !field.bit_field.is_some_and(|bits| bits.width_bits == 0)),
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
                        for (field, location) in fields.iter().zip(layout.fields) {
                            if field.bit_field.is_some_and(|bits| bits.width_bits == 0) {
                                continue;
                            }
                            let mut attrs = vec![ty(field.ty)];
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
    match rank {
        IntRank::Bool => "_Bool",
        IntRank::Char => "char",
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
    for (id, canonical) in unit.types().iter() {
        writer.c_type(unit, target, id, canonical)?;
    }
    let linkage = unit.symbol_linkage().map_err(|e| e.message.to_owned())?;
    // Use the backend's exact retention rule; debug relocations must never
    // keep a function alive after executable emission has discarded it.
    let emitted_functions = evocc_mir::emitted_functions(module);
    let mut functions = BTreeMap::new();
    let mut aliases = BTreeSet::new();
    evocc_chir::inline::for_each_declaration(unit, |decl| match decl {
        CHirDecl::Function(function) if function.body.is_some() => {
            functions.insert(function.symbol, function);
        }
        CHirDecl::Typedef(alias) => {
            aliases.insert((alias.name.spelling.as_str(), alias.ty));
        }
        _ => {}
    });
    for (spelling, id) in aliases {
        writer.node(0x16, false, &[name(spelling), ty(id)]);
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
            Some((symbol.spelling(), symbol.ty()))
        })
        .collect();
    for global in &module.globals {
        if global.kind != GlobalKind::File {
            continue;
        }
        let Some(&id) = file_objects.get(global.symbol.as_str()) else {
            return Err(format!("file object {} has no source type", global.symbol));
        };
        writer.node(
            0x34,
            false,
            &[
                name(&global.symbol),
                ty(id),
                (
                    0x3f,
                    Attribute::Flag(global.visibility == GlobalVisibility::External),
                ),
                (0x02, Attribute::ObjectLocation(global.symbol.clone())),
            ],
        );
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
        writer.node(
            0x2e,
            has_parameters(parameters),
            &[
                name(spelling),
                ty(*return_type),
                (0x11, Attribute::Address(spelling.into())),
                (
                    0x3f,
                    Attribute::Flag(!function.has_internal_linkage(unit.symbols())),
                ),
                (
                    0x27,
                    Attribute::Flag(matches!(parameters, FunctionParameters::Prototype { .. })),
                ),
            ],
        );
        let names = function
            .parameters
            .parameters()
            .iter()
            .map(|parameter| parameter.name.as_ref().map(|name| name.spelling.as_str()))
            .collect::<Vec<_>>();
        writer.parameters(parameters, &names);
    }
    writer.end_children();
    Ok(writer.finish())
}
