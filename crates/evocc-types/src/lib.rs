//! C type and EvoIR type universes with target data layout.
//!
//! Two distinct type systems coexist because they answer different questions:
//!
//! `CType` asks "what does the C standard say about this object?" and models
//! the source-level type with qualifiers, bitfields, rankings, and GNU
//! extensions. It belongs to the frontend and C-HIR.
//!
//! `EvoIRType` asks "what are the bit-level and memory-level properties of
//! this value?" and models signedness-free bit-vector widths, pointer address
//! spaces, memory regions, and effects. It belongs to the optimizer and
//! backend.
//!
//! [`TargetDataLayout`] is the production bridge from canonical [`TypeId`]s to
//! concrete ABI sizes, alignments, record offsets, and bit-field allocation.
//! The older [`TargetLayout`] trait remains only for compatibility with the
//! Day 1--3 structural `CType` API; new frontend code must not use it as layout
//! authority.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// C Type system
// ---------------------------------------------------------------------------

/// C integer ranks in ascending promotion order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntRank {
    Bool,
    Char,
    SChar,
    UChar,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
    LongLong,
    ULongLong,
}

impl IntRank {
    #[must_use]
    pub fn is_signed(self) -> bool {
        !matches!(
            self,
            Self::Bool | Self::UChar | Self::UShort | Self::UInt | Self::ULong | Self::ULongLong
        )
    }
}

/// A C type. This enum reflects the GNU C11 type universe: every variant an
/// ordinary declarator can produce.
///
/// Void and function types are also C types — they participate in expression
/// type-checking even though they are not "objects" in the ISO sense.
///
/// This structural representation is retained for the Day 1--3 frontend API.
/// New frontend work should use [`TypeInterner`], whose child-ID representation
/// can express recursive nominal aggregate types without recursively cloning
/// their definitions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CType {
    Void,
    Int(IntRank),
    Float(FloatKind),
    Complex(ComplexBase),
    Pointer(Box<CType>, Qualifiers),
    Array(Box<CType>, ArrayBound),
    Function(FunctionProto),
    Struct(StructDef),
    Union(UnionDef),
    Enum(EnumDef),
    Bitfield(Box<CType>, u8),
    Typedef(OpaqueTypeId),
    /// GNU `typeof(expr)` or `typeof(type-name)`. The inner `CType` is the
    /// resolved type of the expression or the type-name.
    TypeOf(Box<CType>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatKind {
    F16,
    F32,
    F64,
    F128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComplexBase {
    CF16,
    CF32,
    CF64,
    CF128,
}

/// Type qualifiers. Bitmask-style: multiple qualifiers can apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Qualifiers {
    pub is_const: bool,
    pub is_volatile: bool,
    pub is_restrict: bool,
    pub is_atomic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArrayBound {
    Fixed(u64),
    Variable,  // VLA
    Unbounded, // incomplete array `int a[]`
}

/// Opaque identifier for a C type that lives in a type interning table.
/// Carries no information on its own; the table resolves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpaqueTypeId(u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionProto {
    pub params: Vec<CType>,
    pub is_variadic: bool,
    pub ret: Box<CType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructDef {
    pub name: Option<String>,
    pub fields: Vec<(String, CType)>,
    pub is_packed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnionDef {
    pub name: Option<String>,
    pub fields: Vec<(String, CType)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumDef {
    pub name: Option<String>,
    pub variants: Vec<(String, i64)>,
}

// ---------------------------------------------------------------------------
// Canonical C type interning
// ---------------------------------------------------------------------------

/// Opaque identity of one canonical C type in a [`TypeInterner`].
///
/// IDs are local to the interner that created them. Their numeric value is
/// deterministic for a deterministic insertion trace, but it is not a stable
/// serialization format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(u32);

impl TypeId {
    /// Return the deterministic, interner-local numeric identity.
    #[must_use]
    pub fn raw(self) -> u32 {
        self.0
    }
}

/// Opaque nominal identity of a `struct`, `union`, or `enum` tag.
///
/// Two declarations may have the same spelling and still receive different
/// IDs because C tag identity is determined by scope, not structural shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TagId(u32);

impl TagId {
    /// Return the deterministic, interner-local numeric identity.
    #[must_use]
    pub fn raw(self) -> u32 {
        self.0
    }
}

/// A fully canonicalized C type layer.
///
/// Qualifiers live on every layer. Thus `const int *` and `int * const` use
/// different keys: the former qualifies the pointee's `TypeId`, while the
/// latter qualifies the pointer layer itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalType {
    pub kind: TypeKind,
    pub qualifiers: Qualifiers,
}

/// The structural part of one canonical C type layer.
///
/// Child types are IDs rather than recursively embedded values. Aggregate
/// types refer to nominal tags, allowing an incomplete tag to participate in
/// a pointer type before its definition is completed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeKind {
    Void,
    /// Canonical but deliberately opaque compiler-provided va-list type.
    ///
    /// Day 5 needs its identity for declarations and typedefs. Layout and
    /// varargs operations fail closed instead of guessing a target ABI shape.
    BuiltinVaList,
    Int(IntRank),
    Float(FloatKind),
    Complex(ComplexBase),
    Pointer(TypeId),
    Array {
        element: TypeId,
        bound: ArrayBound,
    },
    Function {
        return_type: TypeId,
        parameters: FunctionParameters,
    },
    Struct(TagId),
    Union(TagId),
    Enum(TagId),
}

/// Parameter-list form is part of a C function type.
///
/// In C, `f()` has an unspecified parameter list while `f(void)` is a
/// prototype with no parameters. They must never share a canonical type ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FunctionParameters {
    Unspecified,
    Prototype { types: Vec<TypeId>, variadic: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagKind {
    Struct,
    Union,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordField {
    pub name: Option<String>,
    pub ty: TypeId,
    /// Bit-field width after integer constant-expression evaluation.
    ///
    /// `None` is an ordinary record member. `Some(0)` represents an unnamed
    /// zero-width allocation barrier; semantic analysis and target layout both
    /// reject a named zero-width bit-field.
    pub bit_field: Option<BitFieldSpec>,
}

/// Target-independent, semantically evaluated bit-field syntax.
///
/// Storage-unit selection and bit offsets are target ABI decisions and are
/// therefore intentionally absent from this frontend representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BitFieldSpec {
    pub width_bits: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumConstant {
    pub name: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TagDefinition {
    Struct(Vec<RecordField>),
    Union(Vec<RecordField>),
    Enum(Vec<EnumConstant>),
}

impl TagDefinition {
    fn kind(&self) -> TagKind {
        match self {
            Self::Struct(_) => TagKind::Struct,
            Self::Union(_) => TagKind::Union,
            Self::Enum(_) => TagKind::Enum,
        }
    }

    fn referenced_types(&self) -> impl Iterator<Item = TypeId> + '_ {
        let fields = match self {
            Self::Struct(fields) | Self::Union(fields) => Some(fields.as_slice()),
            Self::Enum(_) => None,
        };
        fields.into_iter().flatten().map(|field| field.ty)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TagState {
    Incomplete,
    Complete(TagDefinition),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tag {
    pub kind: TagKind,
    pub name: Option<String>,
    pub state: TagState,
}

/// Why a canonical type key could not be interned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeInternError {
    UnknownType(TypeId),
    UnknownTag(TagId),
    TagKindMismatch {
        tag: TagId,
        expected: TagKind,
        actual: TagKind,
    },
}

impl std::fmt::Display for TypeInternError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownType(id) => write!(formatter, "unknown type ID {}", id.raw()),
            Self::UnknownTag(id) => write!(formatter, "unknown tag ID {}", id.raw()),
            Self::TagKindMismatch {
                tag,
                expected,
                actual,
            } => write!(
                formatter,
                "tag ID {} has kind {actual:?}, expected {expected:?}",
                tag.raw()
            ),
        }
    }
}

impl std::error::Error for TypeInternError {}

/// Why an incomplete nominal tag could not be completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagCompletionError {
    UnknownTag(TagId),
    UnknownType(TypeId),
    KindMismatch {
        tag: TagId,
        declared: TagKind,
        definition: TagKind,
    },
    AlreadyComplete(TagId),
}

impl std::fmt::Display for TagCompletionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTag(id) => write!(formatter, "unknown tag ID {}", id.raw()),
            Self::UnknownType(id) => write!(formatter, "unknown type ID {}", id.raw()),
            Self::KindMismatch {
                tag,
                declared,
                definition,
            } => write!(
                formatter,
                "tag ID {} was declared {declared:?}, not {definition:?}",
                tag.raw()
            ),
            Self::AlreadyComplete(id) => {
                write!(formatter, "tag ID {} is already complete", id.raw())
            }
        }
    }
}

impl std::error::Error for TagCompletionError {}

/// Deterministic arena and hash-cons table for canonical C types and tags.
///
/// The vectors are the source of IDs. Hash maps are used only to find an
/// already-interned key, so randomized hash iteration cannot affect ID
/// allocation order.
#[derive(Debug, Clone, Default)]
pub struct TypeInterner {
    types: Vec<CanonicalType>,
    canonical: HashMap<CanonicalType, TypeId>,
    tags: Vec<Tag>,
}

impl TypeInterner {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a validated canonical type key.
    ///
    /// This convenience API fails closed by panicking if a child ID or tag ID
    /// does not belong to this interner. Use [`Self::try_intern`] when parsing
    /// untrusted or independently assembled type keys.
    pub fn intern(&mut self, ty: CanonicalType) -> TypeId {
        self.try_intern(ty)
            .unwrap_or_else(|error| panic!("invalid canonical type: {error}"))
    }

    /// Intern a canonical type key after validating all referenced IDs.
    pub fn try_intern(&mut self, ty: CanonicalType) -> Result<TypeId, TypeInternError> {
        self.validate_type(&ty)?;
        if let Some(id) = self.canonical.get(&ty) {
            return Ok(*id);
        }

        let raw = u32::try_from(self.types.len()).expect("canonical type table exhausted u32 IDs");
        let id = TypeId(raw);
        self.types.push(ty.clone());
        let previous = self.canonical.insert(ty, id);
        debug_assert!(previous.is_none());
        Ok(id)
    }

    #[must_use]
    pub fn get(&self, id: TypeId) -> Option<&CanonicalType> {
        self.types.get(id.raw() as usize)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Allocate a fresh nominal tag. Names are metadata; they do not dedupe
    /// declarations because parser scope determines tag reuse.
    pub fn declare_tag(&mut self, kind: TagKind, name: Option<String>) -> TagId {
        let raw = u32::try_from(self.tags.len()).expect("tag table exhausted u32 IDs");
        let id = TagId(raw);
        self.tags.push(Tag {
            kind,
            name,
            state: TagState::Incomplete,
        });
        id
    }

    #[must_use]
    pub fn tag(&self, id: TagId) -> Option<&Tag> {
        self.tags.get(id.raw() as usize)
    }

    /// Complete a nominal tag exactly once without changing its identity.
    pub fn complete_tag(
        &mut self,
        id: TagId,
        definition: TagDefinition,
    ) -> Result<(), TagCompletionError> {
        let Some(tag) = self.tags.get(id.raw() as usize) else {
            return Err(TagCompletionError::UnknownTag(id));
        };
        if matches!(tag.state, TagState::Complete(_)) {
            return Err(TagCompletionError::AlreadyComplete(id));
        }

        let definition_kind = definition.kind();
        if tag.kind != definition_kind {
            return Err(TagCompletionError::KindMismatch {
                tag: id,
                declared: tag.kind,
                definition: definition_kind,
            });
        }
        for field_type in definition.referenced_types() {
            if self.get(field_type).is_none() {
                return Err(TagCompletionError::UnknownType(field_type));
            }
        }

        self.tags[id.raw() as usize].state = TagState::Complete(definition);
        Ok(())
    }

    fn validate_type(&self, ty: &CanonicalType) -> Result<(), TypeInternError> {
        match &ty.kind {
            TypeKind::Pointer(pointee) => self.require_type(*pointee),
            TypeKind::Array { element, .. } => self.require_type(*element),
            TypeKind::Function {
                return_type,
                parameters,
            } => {
                self.require_type(*return_type)?;
                if let FunctionParameters::Prototype { types, .. } = parameters {
                    for parameter in types {
                        self.require_type(*parameter)?;
                    }
                }
                Ok(())
            }
            TypeKind::Struct(tag) => self.require_tag(*tag, TagKind::Struct),
            TypeKind::Union(tag) => self.require_tag(*tag, TagKind::Union),
            TypeKind::Enum(tag) => self.require_tag(*tag, TagKind::Enum),
            TypeKind::Void
            | TypeKind::BuiltinVaList
            | TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_) => Ok(()),
        }
    }

    fn require_type(&self, id: TypeId) -> Result<(), TypeInternError> {
        if self.get(id).is_some() {
            Ok(())
        } else {
            Err(TypeInternError::UnknownType(id))
        }
    }

    fn require_tag(&self, id: TagId, expected: TagKind) -> Result<(), TypeInternError> {
        let Some(tag) = self.tag(id) else {
            return Err(TypeInternError::UnknownTag(id));
        };
        if tag.kind == expected {
            Ok(())
        } else {
            Err(TypeInternError::TagKindMismatch {
                tag: id,
                expected,
                actual: tag.kind,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// EvoIR Type system
// ---------------------------------------------------------------------------

/// An address space for pointer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressSpace {
    Generic,
    Global,
    Stack,
    ThreadLocal,
}

/// A memory region for alias analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    All,
    Unknown,
    Global,
    Stack,
    Heap,
    Escaped,
}

/// EvoIR type: the narrowing of C's rich type system into bit-level and
/// memory-level representations suitable for equality saturation.
///
/// Signedness lives in *operations* (e.g. `sdiv.bits<32>` vs `udiv.bits<32>`)
/// rather than in integer storage types. This is the central architectural
/// decision documented in the roadmap: bit-vector reasoning is simpler when
/// the type does not declare a sign.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EvoIRType {
    /// Unsigned/signed bit-vector of statically known width `N`.
    Bits(u16),
    /// IEEE 754 binary32 / binary64.
    F32,
    F64,
    /// Typed pointer into one address space.
    Ptr(AddressSpace),
    /// Packed SIMD vector: N elements of element type.
    Vec(u16, Box<EvoIRType>),
    /// An abstract memory snapshot for one region.
    Mem(Region),
    /// The global ordering token for volatile and I/O effects.
    World,
    /// A heterogeneous product of zero or more types.
    Tuple(Vec<EvoIRType>),
    /// The absence of a value.
    Void,
}

// ---------------------------------------------------------------------------
// Target data layout
// ---------------------------------------------------------------------------

/// Knows the size, alignment, and field layout of C types on a concrete
/// target ABI (Apple AArch64, Linux AAPCS64, x86-64 SysV).
///
/// Separate implementations for each target allow the compiler to re-target
/// without changing the type system itself.
pub trait TargetLayout {
    /// Size of `ty` in bytes.
    fn size_of(&self, ty: &CType) -> u64;
    /// Alignment of `ty` in bytes (must be a power of two).
    fn align_of(&self, ty: &CType) -> u32;
    /// Byte offset of `field_index` within `aggregate`. `aggregate` must be a
    /// struct or union, and `field_index` must be in bounds.
    fn field_offset(&self, aggregate: &CType, field_index: usize) -> u64;
    /// Width and byte offset of a bitfield within its storage unit.
    fn bitfield_layout(&self, base: &CType, width_bits: u8) -> (u8, u64);
}

/// Size and ABI alignment of one complete object type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeLayout {
    pub size_bytes: u64,
    pub align_bytes: u32,
}

impl TypeLayout {
    #[must_use]
    pub const fn new(size_bytes: u64, align_bytes: u32) -> Self {
        Self {
            size_bytes,
            align_bytes,
        }
    }
}

/// Target allocation details for one C bit-field.
///
/// `bit_offset` is counted from the least-significant bit of the storage unit
/// because `aarch64-unknown-linux-gnu` is little-endian. The allocation unit
/// may begin before an earlier ordinary field: AAPCS64 permits a bit-field to
/// start at the next free bit when it still fits in its naturally aligned
/// container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BitFieldLayout {
    pub storage_byte_offset: u64,
    pub storage_size_bytes: u32,
    pub bit_offset: u16,
    pub width_bits: u16,
}

/// Layout of one source-order record field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordFieldLayout {
    pub field_index: usize,
    /// Byte containing the first bit of a bit-field, or the ordinary member's
    /// addressable byte offset.
    pub byte_offset: u64,
    /// Layout of the declared base/member type. A flexible array has size zero
    /// here and retains its element alignment.
    pub layout: TypeLayout,
    pub bit_field: Option<BitFieldLayout>,
}

/// Complete ABI layout of a struct or union.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordLayout {
    pub size_bytes: u64,
    pub align_bytes: u32,
    pub fields: Vec<RecordFieldLayout>,
    /// Bytes after the last occupied member (or after a flexible member's
    /// zero-sized position) that are included in `sizeof`.
    pub tail_padding_bytes: u64,
    /// Padding between the occupied fixed prefix and a final flexible array.
    /// `None` distinguishes records without a flexible array from a zero-gap
    /// flexible prefix.
    pub flexible_array_padding_bytes: Option<u64>,
}

impl RecordLayout {
    /// Return source-order byte offsets without discarding richer field data.
    #[must_use]
    pub fn field_offsets(&self) -> Vec<u64> {
        self.fields.iter().map(|field| field.byte_offset).collect()
    }

    #[must_use]
    pub const fn type_layout(&self) -> TypeLayout {
        TypeLayout::new(self.size_bytes, self.align_bytes)
    }
}

/// A canonical type has no valid, finite object layout for this target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutError {
    UnknownType(TypeId),
    UnknownTag(TagId),
    IncompleteTag(TagId),
    IncompleteArray(TypeId),
    NonObjectType(TypeId),
    UnsupportedBuiltinType(TypeId),
    VariableLengthArray(TypeId),
    NotRecordTag {
        tag: TagId,
        actual: TagKind,
    },
    RecursiveByValue(TagId),
    FlexibleArrayNotLast {
        tag: TagId,
        field_index: usize,
    },
    FlexibleArrayInUnion {
        tag: TagId,
        field_index: usize,
    },
    InvalidBitFieldBase {
        tag: TagId,
        field_index: usize,
        ty: TypeId,
    },
    NamedZeroWidthBitField {
        tag: TagId,
        field_index: usize,
    },
    BitFieldWidthExceedsType {
        tag: TagId,
        field_index: usize,
        width_bits: u16,
        type_bits: u16,
    },
    ArithmeticOverflow,
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownType(id) => write!(formatter, "unknown type ID {}", id.raw()),
            Self::UnknownTag(id) => write!(formatter, "unknown tag ID {}", id.raw()),
            Self::IncompleteTag(id) => write!(formatter, "tag ID {} is incomplete", id.raw()),
            Self::IncompleteArray(id) => {
                write!(formatter, "array type ID {} has no bound", id.raw())
            }
            Self::NonObjectType(id) => {
                write!(formatter, "type ID {} is not an object type", id.raw())
            }
            Self::UnsupportedBuiltinType(id) => write!(
                formatter,
                "type ID {} is an opaque builtin type without a target layout",
                id.raw()
            ),
            Self::VariableLengthArray(id) => {
                write!(formatter, "VLA type ID {} has no static layout", id.raw())
            }
            Self::NotRecordTag { tag, actual } => write!(
                formatter,
                "tag ID {} is {actual:?}, not a struct or union",
                tag.raw()
            ),
            Self::RecursiveByValue(id) => write!(
                formatter,
                "tag ID {} contains itself recursively by value",
                id.raw()
            ),
            Self::FlexibleArrayNotLast { tag, field_index } => write!(
                formatter,
                "field {field_index} of tag ID {} is a non-final flexible array",
                tag.raw()
            ),
            Self::FlexibleArrayInUnion { tag, field_index } => write!(
                formatter,
                "field {field_index} of union tag ID {} is a flexible array",
                tag.raw()
            ),
            Self::InvalidBitFieldBase {
                tag,
                field_index,
                ty,
            } => write!(
                formatter,
                "field {field_index} of tag ID {} uses non-integral bit-field type ID {}",
                tag.raw(),
                ty.raw()
            ),
            Self::NamedZeroWidthBitField { tag, field_index } => write!(
                formatter,
                "field {field_index} of tag ID {} is a named zero-width bit-field",
                tag.raw()
            ),
            Self::BitFieldWidthExceedsType {
                tag,
                field_index,
                width_bits,
                type_bits,
            } => write!(
                formatter,
                "field {field_index} of tag ID {} has width {width_bits}, exceeding its {type_bits}-bit base type",
                tag.raw()
            ),
            Self::ArithmeticOverflow => formatter.write_str("object layout arithmetic overflow"),
        }
    }
}

impl std::error::Error for LayoutError {}

/// GNU AArch64 target layout over the canonical [`TypeInterner`] universe.
///
/// The constructor names the target explicitly so a future x86-64 or Apple
/// AArch64 implementation cannot silently inherit this ABI's decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetDataLayout {
    _private: (),
}

impl TargetDataLayout {
    #[must_use]
    pub const fn aarch64_unknown_linux_gnu() -> Self {
        Self { _private: () }
    }

    /// Compute the layout of a complete canonical object type.
    pub fn layout_of(&self, types: &TypeInterner, ty: TypeId) -> Result<TypeLayout, LayoutError> {
        LayoutContext::new(self, types).layout_type(ty)
    }

    /// Compute source-order field placement and padding for a struct or union.
    pub fn record_layout(
        &self,
        types: &TypeInterner,
        tag: TagId,
    ) -> Result<RecordLayout, LayoutError> {
        LayoutContext::new(self, types).layout_record(tag)
    }
}

struct LayoutContext<'a> {
    _target: &'a TargetDataLayout,
    types: &'a TypeInterner,
    active_tags: Vec<TagId>,
}

impl<'a> LayoutContext<'a> {
    fn new(target: &'a TargetDataLayout, types: &'a TypeInterner) -> Self {
        Self {
            _target: target,
            types,
            active_tags: Vec::new(),
        }
    }

    fn layout_type(&mut self, ty: TypeId) -> Result<TypeLayout, LayoutError> {
        let kind = self
            .types
            .get(ty)
            .ok_or(LayoutError::UnknownType(ty))?
            .kind
            .clone();
        match kind {
            TypeKind::Void | TypeKind::Function { .. } => Err(LayoutError::NonObjectType(ty)),
            TypeKind::BuiltinVaList => Err(LayoutError::UnsupportedBuiltinType(ty)),
            TypeKind::Int(rank) => Ok(integer_layout(rank)),
            TypeKind::Float(kind) => Ok(float_layout(kind)),
            TypeKind::Complex(base) => Ok(complex_layout(base)),
            TypeKind::Pointer(_) => Ok(TypeLayout::new(8, 8)),
            TypeKind::Array { element, bound } => match bound {
                ArrayBound::Fixed(count) => {
                    let element = self.layout_type(element)?;
                    let size = element
                        .size_bytes
                        .checked_mul(count)
                        .ok_or(LayoutError::ArithmeticOverflow)?;
                    Ok(TypeLayout::new(size, element.align_bytes))
                }
                ArrayBound::Variable => Err(LayoutError::VariableLengthArray(ty)),
                ArrayBound::Unbounded => Err(LayoutError::IncompleteArray(ty)),
            },
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                Ok(self.layout_record(tag)?.type_layout())
            }
            TypeKind::Enum(tag) => self.layout_enum(tag),
        }
    }

    fn layout_enum(&self, tag_id: TagId) -> Result<TypeLayout, LayoutError> {
        let tag = self
            .types
            .tag(tag_id)
            .ok_or(LayoutError::UnknownTag(tag_id))?;
        let TagState::Complete(TagDefinition::Enum(constants)) = &tag.state else {
            return if matches!(tag.state, TagState::Incomplete) {
                Err(LayoutError::IncompleteTag(tag_id))
            } else {
                Err(LayoutError::NotRecordTag {
                    tag: tag_id,
                    actual: tag.kind,
                })
            };
        };

        // GNU C with -fno-short-enums uses at least `int`. Values outside the
        // 32-bit signed/unsigned domains require an eight-byte integer mode.
        let minimum = constants
            .iter()
            .map(|constant| constant.value)
            .min()
            .unwrap_or(0);
        let maximum = constants
            .iter()
            .map(|constant| constant.value)
            .max()
            .unwrap_or(0);
        let fits_signed_int = minimum >= i64::from(i32::MIN) && maximum <= i64::from(i32::MAX);
        let fits_unsigned_int = minimum >= 0 && maximum <= i64::from(u32::MAX);
        if fits_signed_int || fits_unsigned_int {
            Ok(TypeLayout::new(4, 4))
        } else {
            Ok(TypeLayout::new(8, 8))
        }
    }

    fn layout_record(&mut self, tag_id: TagId) -> Result<RecordLayout, LayoutError> {
        let tag = self
            .types
            .tag(tag_id)
            .ok_or(LayoutError::UnknownTag(tag_id))?;
        let kind = tag.kind;
        if !matches!(kind, TagKind::Struct | TagKind::Union) {
            return Err(LayoutError::NotRecordTag {
                tag: tag_id,
                actual: kind,
            });
        }
        let definition = match &tag.state {
            TagState::Incomplete => return Err(LayoutError::IncompleteTag(tag_id)),
            TagState::Complete(definition) => definition.clone(),
        };
        if self.active_tags.contains(&tag_id) {
            return Err(LayoutError::RecursiveByValue(tag_id));
        }

        self.active_tags.push(tag_id);
        let result = match definition {
            TagDefinition::Struct(fields) => self.layout_struct(tag_id, &fields),
            TagDefinition::Union(fields) => self.layout_union(tag_id, &fields),
            TagDefinition::Enum(_) => unreachable!("tag kind checked above"),
        };
        let popped = self.active_tags.pop();
        debug_assert_eq!(popped, Some(tag_id));
        result
    }

    fn layout_struct(
        &mut self,
        tag: TagId,
        fields: &[RecordField],
    ) -> Result<RecordLayout, LayoutError> {
        let mut cursor_bits = 0_u64;
        let mut occupied_end_bytes = 0_u64;
        let mut record_align = 1_u32;
        let mut layouts = Vec::with_capacity(fields.len());
        let mut flexible_array_padding_bytes = None;
        let mut flexible_position = None;

        for (field_index, field) in fields.iter().enumerate() {
            if let Some(spec) = field.bit_field {
                let (base_layout, type_bits) = self.bit_field_base(tag, field_index, field.ty)?;
                // AAPCS64 10.1.8: the container type contributes to aggregate
                // alignment even for anonymous and zero-width bit-fields.
                record_align = record_align.max(base_layout.align_bytes);
                if spec.width_bits == 0 && field.name.is_some() {
                    return Err(LayoutError::NamedZeroWidthBitField { tag, field_index });
                }
                if spec.width_bits > type_bits {
                    return Err(LayoutError::BitFieldWidthExceedsType {
                        tag,
                        field_index,
                        width_bits: spec.width_bits,
                        type_bits,
                    });
                }

                let unit_bits = base_layout
                    .size_bytes
                    .checked_mul(8)
                    .ok_or(LayoutError::ArithmeticOverflow)?;
                let start_bits = if spec.width_bits == 0 {
                    checked_align_up(cursor_bits, unit_bits)?
                } else {
                    let container_start = (cursor_bits / unit_bits) * unit_bits;
                    let container_end = container_start
                        .checked_add(unit_bits)
                        .ok_or(LayoutError::ArithmeticOverflow)?;
                    let tentative_end = cursor_bits
                        .checked_add(u64::from(spec.width_bits))
                        .ok_or(LayoutError::ArithmeticOverflow)?;
                    if tentative_end <= container_end {
                        cursor_bits
                    } else {
                        checked_align_up(cursor_bits, unit_bits)?
                    }
                };
                let end_bits = start_bits
                    .checked_add(u64::from(spec.width_bits))
                    .ok_or(LayoutError::ArithmeticOverflow)?;
                let storage_bits = (start_bits / unit_bits) * unit_bits;
                let relative_bit_offset = start_bits - storage_bits;
                let bit_offset = u16::try_from(relative_bit_offset)
                    .map_err(|_| LayoutError::ArithmeticOverflow)?;
                let storage_size_bytes = u32::try_from(base_layout.size_bytes)
                    .map_err(|_| LayoutError::ArithmeticOverflow)?;

                cursor_bits = end_bits;
                if spec.width_bits != 0 {
                    occupied_end_bytes = occupied_end_bytes.max(bits_to_bytes(end_bits)?);
                }
                layouts.push(RecordFieldLayout {
                    field_index,
                    byte_offset: start_bits / 8,
                    layout: base_layout,
                    bit_field: Some(BitFieldLayout {
                        storage_byte_offset: storage_bits / 8,
                        storage_size_bytes,
                        bit_offset,
                        width_bits: spec.width_bits,
                    }),
                });
                continue;
            }

            if let Some(element) = self.unbounded_array_element(field.ty)? {
                if field_index + 1 != fields.len() {
                    return Err(LayoutError::FlexibleArrayNotLast { tag, field_index });
                }
                let element_layout = self.layout_type(element)?;
                let prefix_end = occupied_end_bytes;
                let placement_end = bits_to_bytes(cursor_bits)?;
                let offset = checked_align_up(
                    placement_end.max(occupied_end_bytes),
                    u64::from(element_layout.align_bytes),
                )?;
                record_align = record_align.max(element_layout.align_bytes);
                flexible_array_padding_bytes = Some(
                    offset
                        .checked_sub(prefix_end)
                        .ok_or(LayoutError::ArithmeticOverflow)?,
                );
                flexible_position = Some(offset);
                cursor_bits = offset
                    .checked_mul(8)
                    .ok_or(LayoutError::ArithmeticOverflow)?;
                layouts.push(RecordFieldLayout {
                    field_index,
                    byte_offset: offset,
                    layout: TypeLayout::new(0, element_layout.align_bytes),
                    bit_field: None,
                });
                continue;
            }

            let field_layout = self.layout_type(field.ty)?;
            let cursor_bytes = bits_to_bytes(cursor_bits)?;
            let offset = checked_align_up(cursor_bytes, u64::from(field_layout.align_bytes))?;
            let end = offset
                .checked_add(field_layout.size_bytes)
                .ok_or(LayoutError::ArithmeticOverflow)?;
            cursor_bits = end.checked_mul(8).ok_or(LayoutError::ArithmeticOverflow)?;
            occupied_end_bytes = occupied_end_bytes.max(end);
            record_align = record_align.max(field_layout.align_bytes);
            layouts.push(RecordFieldLayout {
                field_index,
                byte_offset: offset,
                layout: field_layout,
                bit_field: None,
            });
        }

        let unrounded_size = bits_to_bytes(cursor_bits)?;
        let size_bytes = checked_align_up(unrounded_size, u64::from(record_align))?;
        let tail_base = flexible_position.unwrap_or(occupied_end_bytes);
        let tail_padding_bytes = size_bytes
            .checked_sub(tail_base)
            .ok_or(LayoutError::ArithmeticOverflow)?;
        Ok(RecordLayout {
            size_bytes,
            align_bytes: record_align,
            fields: layouts,
            tail_padding_bytes,
            flexible_array_padding_bytes,
        })
    }

    fn layout_union(
        &mut self,
        tag: TagId,
        fields: &[RecordField],
    ) -> Result<RecordLayout, LayoutError> {
        let mut member_extent = 0_u64;
        let mut record_align = 1_u32;
        let mut layouts = Vec::with_capacity(fields.len());

        for (field_index, field) in fields.iter().enumerate() {
            if let Some(spec) = field.bit_field {
                let (base_layout, type_bits) = self.bit_field_base(tag, field_index, field.ty)?;
                record_align = record_align.max(base_layout.align_bytes);
                if spec.width_bits == 0 && field.name.is_some() {
                    return Err(LayoutError::NamedZeroWidthBitField { tag, field_index });
                }
                if spec.width_bits > type_bits {
                    return Err(LayoutError::BitFieldWidthExceedsType {
                        tag,
                        field_index,
                        width_bits: spec.width_bits,
                        type_bits,
                    });
                }
                if spec.width_bits != 0 {
                    member_extent = member_extent.max(base_layout.size_bytes);
                }
                layouts.push(RecordFieldLayout {
                    field_index,
                    byte_offset: 0,
                    layout: base_layout,
                    bit_field: Some(BitFieldLayout {
                        storage_byte_offset: 0,
                        storage_size_bytes: u32::try_from(base_layout.size_bytes)
                            .map_err(|_| LayoutError::ArithmeticOverflow)?,
                        bit_offset: 0,
                        width_bits: spec.width_bits,
                    }),
                });
                continue;
            }

            if self.unbounded_array_element(field.ty)?.is_some() {
                return Err(LayoutError::FlexibleArrayInUnion { tag, field_index });
            }
            let field_layout = self.layout_type(field.ty)?;
            member_extent = member_extent.max(field_layout.size_bytes);
            record_align = record_align.max(field_layout.align_bytes);
            layouts.push(RecordFieldLayout {
                field_index,
                byte_offset: 0,
                layout: field_layout,
                bit_field: None,
            });
        }

        let size_bytes = checked_align_up(member_extent, u64::from(record_align))?;
        Ok(RecordLayout {
            size_bytes,
            align_bytes: record_align,
            fields: layouts,
            tail_padding_bytes: size_bytes
                .checked_sub(member_extent)
                .ok_or(LayoutError::ArithmeticOverflow)?,
            flexible_array_padding_bytes: None,
        })
    }

    fn unbounded_array_element(&self, ty: TypeId) -> Result<Option<TypeId>, LayoutError> {
        let canonical = self.types.get(ty).ok_or(LayoutError::UnknownType(ty))?;
        Ok(match canonical.kind {
            TypeKind::Array {
                element,
                bound: ArrayBound::Unbounded,
            } => Some(element),
            _ => None,
        })
    }

    fn bit_field_base(
        &mut self,
        tag: TagId,
        field_index: usize,
        ty: TypeId,
    ) -> Result<(TypeLayout, u16), LayoutError> {
        let kind = self
            .types
            .get(ty)
            .ok_or(LayoutError::UnknownType(ty))?
            .kind
            .clone();
        match kind {
            TypeKind::Int(rank) => {
                let layout = integer_layout(rank);
                let value_bits = if rank == IntRank::Bool {
                    1
                } else {
                    u16::try_from(layout.size_bytes * 8)
                        .map_err(|_| LayoutError::ArithmeticOverflow)?
                };
                Ok((layout, value_bits))
            }
            TypeKind::Enum(enum_tag) => {
                let layout = self.layout_enum(enum_tag)?;
                let value_bits = u16::try_from(layout.size_bytes * 8)
                    .map_err(|_| LayoutError::ArithmeticOverflow)?;
                Ok((layout, value_bits))
            }
            _ => Err(LayoutError::InvalidBitFieldBase {
                tag,
                field_index,
                ty,
            }),
        }
    }
}

fn integer_layout(rank: IntRank) -> TypeLayout {
    match rank {
        IntRank::Bool | IntRank::Char | IntRank::SChar | IntRank::UChar => TypeLayout::new(1, 1),
        IntRank::Short | IntRank::UShort => TypeLayout::new(2, 2),
        IntRank::Int | IntRank::UInt => TypeLayout::new(4, 4),
        IntRank::Long | IntRank::ULong | IntRank::LongLong | IntRank::ULongLong => {
            TypeLayout::new(8, 8)
        }
    }
}

fn float_layout(kind: FloatKind) -> TypeLayout {
    match kind {
        FloatKind::F16 => TypeLayout::new(2, 2),
        FloatKind::F32 => TypeLayout::new(4, 4),
        FloatKind::F64 => TypeLayout::new(8, 8),
        FloatKind::F128 => TypeLayout::new(16, 16),
    }
}

fn complex_layout(base: ComplexBase) -> TypeLayout {
    match base {
        ComplexBase::CF16 => TypeLayout::new(4, 2),
        ComplexBase::CF32 => TypeLayout::new(8, 4),
        ComplexBase::CF64 => TypeLayout::new(16, 8),
        ComplexBase::CF128 => TypeLayout::new(32, 16),
    }
}

fn checked_align_up(value: u64, alignment: u64) -> Result<u64, LayoutError> {
    debug_assert!(alignment.is_power_of_two());
    let mask = alignment - 1;
    value
        .checked_add(mask)
        .map(|sum| sum & !mask)
        .ok_or(LayoutError::ArithmeticOverflow)
}

fn bits_to_bytes(bits: u64) -> Result<u64, LayoutError> {
    bits.checked_add(7)
        .map(|rounded| rounded / 8)
        .ok_or(LayoutError::ArithmeticOverflow)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // CType exhaustiveness
    // -----------------------------------------------------------------------

    /// Every top-level C type specifier category must be representable.
    /// This test is not about correctness — it is about whether the enum has
    /// a variant for each kind of C type the parser must produce.
    #[test]
    fn type_universe_c11_is_exhaustive() {
        // Basic integer types
        let i = CType::Int(IntRank::Int);
        assert!(matches!(i, CType::Int(IntRank::Int)));

        let uc = CType::Int(IntRank::UChar);
        assert!(matches!(uc, CType::Int(IntRank::UChar)));

        // Floating
        assert!(matches!(CType::Float(FloatKind::F64), CType::Float(..)));

        // Pointer
        let ptr = CType::Pointer(Box::new(CType::Void), Qualifiers::default());
        assert!(matches!(ptr, CType::Pointer(..)));

        // Array (fixed, VLA, unbounded)
        let arr = CType::Array(Box::new(CType::Int(IntRank::Int)), ArrayBound::Fixed(10));
        assert!(matches!(arr, CType::Array(..)));

        // Struct
        let s = CType::Struct(StructDef {
            name: Some("S".into()),
            fields: vec![("x".into(), CType::Int(IntRank::Int))],
            is_packed: false,
        });
        assert!(matches!(s, CType::Struct(..)));

        // Function
        let f = CType::Function(FunctionProto {
            params: vec![CType::Int(IntRank::Int)],
            is_variadic: false,
            ret: Box::new(CType::Void),
        });
        assert!(matches!(f, CType::Function(..)));

        // GNU typeof
        let t = CType::TypeOf(Box::new(CType::Int(IntRank::Long)));
        assert!(matches!(t, CType::TypeOf(..)));
    }

    #[test]
    fn int_rank_signedness_is_correct() {
        assert!(!IntRank::Bool.is_signed());
        assert!(!IntRank::UInt.is_signed());
        assert!(IntRank::Int.is_signed());
        assert!(IntRank::Long.is_signed());
    }

    fn canonical(kind: TypeKind) -> CanonicalType {
        CanonicalType {
            kind,
            qualifiers: Qualifiers::default(),
        }
    }

    #[test]
    fn canonical_interner_rejects_unknown_or_mismatched_references() {
        let mut types = TypeInterner::new();
        let unknown_type = TypeId(91);
        assert_eq!(
            types.try_intern(canonical(TypeKind::Pointer(unknown_type))),
            Err(TypeInternError::UnknownType(unknown_type))
        );
        assert!(types.is_empty(), "a rejected key must not consume an ID");

        let structure = types.declare_tag(TagKind::Struct, Some("S".into()));
        assert_eq!(
            types.try_intern(canonical(TypeKind::Union(structure))),
            Err(TypeInternError::TagKindMismatch {
                tag: structure,
                expected: TagKind::Union,
                actual: TagKind::Struct,
            })
        );
        assert!(
            types.is_empty(),
            "a rejected tag key must not consume an ID"
        );
    }

    #[test]
    fn tag_completion_errors_are_typed_and_transactional() {
        let mut types = TypeInterner::new();
        let structure = types.declare_tag(TagKind::Struct, Some("S".into()));

        assert_eq!(
            types.complete_tag(structure, TagDefinition::Union(Vec::new())),
            Err(TagCompletionError::KindMismatch {
                tag: structure,
                declared: TagKind::Struct,
                definition: TagKind::Union,
            })
        );
        assert!(matches!(
            &types.tag(structure).expect("declared tag").state,
            TagState::Incomplete
        ));

        let unknown_type = TypeId(37);
        assert_eq!(
            types.complete_tag(
                structure,
                TagDefinition::Struct(vec![RecordField {
                    name: Some("field".into()),
                    ty: unknown_type,
                    bit_field: None,
                }]),
            ),
            Err(TagCompletionError::UnknownType(unknown_type))
        );
        assert!(matches!(
            &types.tag(structure).expect("declared tag").state,
            TagState::Incomplete
        ));

        types
            .complete_tag(structure, TagDefinition::Struct(Vec::new()))
            .expect("matching first completion");
        assert_eq!(
            types.complete_tag(structure, TagDefinition::Struct(Vec::new())),
            Err(TagCompletionError::AlreadyComplete(structure))
        );

        let unknown_tag = TagId(73);
        assert_eq!(
            types.complete_tag(unknown_tag, TagDefinition::Enum(Vec::new())),
            Err(TagCompletionError::UnknownTag(unknown_tag))
        );
    }

    // -----------------------------------------------------------------------
    // EvoIRType
    // -----------------------------------------------------------------------

    #[test]
    fn evoir_type_universe_is_defined() {
        assert!(matches!(EvoIRType::Bits(32), EvoIRType::Bits(32)));
        assert_eq!(EvoIRType::Bits(64), EvoIRType::Bits(64));
        assert_ne!(EvoIRType::Bits(32), EvoIRType::Bits(64));

        assert!(matches!(
            EvoIRType::Ptr(AddressSpace::Global),
            EvoIRType::Ptr(..)
        ));
        assert_ne!(
            EvoIRType::Ptr(AddressSpace::Global),
            EvoIRType::Ptr(AddressSpace::Stack)
        );

        assert_eq!(EvoIRType::F32, EvoIRType::F32);
        assert!(matches!(EvoIRType::Mem(Region::All), EvoIRType::Mem(..)));
        assert!(matches!(EvoIRType::World, EvoIRType::World));
        assert_eq!(EvoIRType::Void, EvoIRType::Void);
    }

    // -----------------------------------------------------------------------
    // TargetLayout determinism
    // -----------------------------------------------------------------------

    /// A trivial test layout that returns fixed sizes. Real layouts must
    /// satisfy the same contract: determinism (same input → same output).
    struct TestLayout;

    impl TargetLayout for TestLayout {
        fn size_of(&self, ty: &CType) -> u64 {
            match ty {
                CType::Void => 0,
                CType::Int(r) => match r {
                    IntRank::Bool | IntRank::Char | IntRank::SChar | IntRank::UChar => 1,
                    IntRank::Short | IntRank::UShort => 2,
                    IntRank::Int | IntRank::UInt => 4,
                    _ => 8,
                },
                CType::Float(_) => 4,
                CType::Pointer(..) => 8,
                _ => 8,
            }
        }

        fn align_of(&self, ty: &CType) -> u32 {
            self.size_of(ty) as u32
        }

        fn field_offset(&self, _aggregate: &CType, field_index: usize) -> u64 {
            field_index as u64 * 8
        }

        fn bitfield_layout(&self, _base: &CType, _width_bits: u8) -> (u8, u64) {
            (0, 0)
        }
    }

    #[test]
    fn target_layout_is_deterministic() {
        let layout = TestLayout;
        let t = CType::Int(IntRank::Int);

        let s1 = layout.size_of(&t);
        let s2 = layout.size_of(&t);
        assert_eq!(s1, s2, "size_of must be deterministic");

        let a1 = layout.align_of(&t);
        let a2 = layout.align_of(&t);
        assert_eq!(a1, a2, "align_of must be deterministic");
    }
}
