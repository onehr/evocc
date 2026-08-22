//! Typed, target-independent descriptions of C memory lvalues.
//!
//! A memory place records semantic identity and source expressions only.  It
//! deliberately contains no byte offsets: record placement, array stride, and
//! pointer width remain decisions of [`evocc_types::TargetDataLayout`].

use std::{collections::HashSet, fmt};

use evocc_types::{Qualifiers, TagDefinition, TagId, TagState, TypeId, TypeInterner, TypeKind};

use crate::integer::CIntegerType;
use crate::{CHirExpr, StorageClass, SymbolId, SymbolKind, SymbolTable};

/// A pointer expression's target type without requiring a synthetic pointer
/// [`TypeId`].
///
/// `pointee_qualifiers` is authoritative.  It may be the qualifier union of a
/// conditional expression even when that exact qualified type has no entry in
/// the immutable [`TypeInterner`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PointerTypeDesc {
    pointee: TypeId,
    pointee_qualifiers: Qualifiers,
}

impl PointerTypeDesc {
    /// Describe an already-interned pointer type without changing the
    /// interner. Top-level qualifiers on the pointer object are deliberately
    /// discarded by lvalue conversion; pointee qualifiers remain semantic.
    pub fn from_canonical_pointer(
        types: &TypeInterner,
        pointer: TypeId,
    ) -> Result<Self, MemoryPlaceError> {
        let canonical = types
            .get(pointer)
            .ok_or(MemoryPlaceError::UnknownType(pointer))?;
        let TypeKind::Pointer(pointee) = canonical.kind else {
            return Err(MemoryPlaceError::NotPointerType(pointer));
        };
        let pointee_qualifiers = types
            .get(pointee)
            .ok_or(MemoryPlaceError::UnknownType(pointee))?
            .qualifiers;
        Ok(Self {
            pointee,
            pointee_qualifiers,
        })
    }

    /// Form the result type of address-of from a place whose facts have
    /// already been checked against the symbol/type tables.
    #[must_use]
    pub const fn from_addressed_place(place: &MemoryPlace<'_>) -> Self {
        Self {
            pointee: place.value_ty,
            pointee_qualifiers: place.access_qualifiers,
        }
    }

    /// Form the C composite of two compatible pointer value types.
    pub fn composite(
        types: &TypeInterner,
        left: Self,
        right: Self,
    ) -> Result<Self, MemoryPlaceError> {
        let left_kind = &types
            .get(left.pointee)
            .ok_or(MemoryPlaceError::UnknownType(left.pointee))?
            .kind;
        let right_kind = &types
            .get(right.pointee)
            .ok_or(MemoryPlaceError::UnknownType(right.pointee))?
            .kind;
        let pointee = if left_kind == right_kind
            || (matches!(left_kind, TypeKind::Void) && is_object_type(right_kind))
        {
            left.pointee
        } else if matches!(right_kind, TypeKind::Void) && is_object_type(left_kind) {
            right.pointee
        } else {
            return Err(MemoryPlaceError::IncompatiblePointerTypes {
                left: left.pointee,
                right: right.pointee,
            });
        };
        Ok(Self {
            pointee,
            pointee_qualifiers: merge_qualifiers(left.pointee_qualifiers, right.pointee_qualifiers),
        })
    }

    #[must_use]
    pub const fn pointee(self) -> TypeId {
        self.pointee
    }

    #[must_use]
    pub const fn pointee_qualifiers(self) -> Qualifiers {
        self.pointee_qualifiers
    }
}

/// The identity from which a memory place is resolved.
#[derive(Debug, Clone)]
pub enum PlaceRoot<'arena> {
    /// A declared object or parameter.
    Object { symbol: SymbolId },
    /// The address produced by evaluating `pointer` exactly once.
    PointerExpression {
        pointer: &'arena CHirExpr<'arena>,
        pointer_type: PointerTypeDesc,
    },
}

impl<'arena> PlaceRoot<'arena> {
    /// The one source expression evaluated to resolve this root, if any.
    #[must_use]
    pub const fn source_expression(&self) -> Option<&'arena CHirExpr<'arena>> {
        match self {
            Self::Object { .. } => None,
            Self::PointerExpression { pointer, .. } => Some(*pointer),
        }
    }
}

/// One semantic projection from a place root to a subobject.
#[derive(Debug, Clone)]
pub enum PlaceProjection<'arena> {
    /// Array/pointer indexing. Construction remains private until the parser
    /// can derive the promoted index type from authoritative expression facts.
    Index(IndexProjection<'arena>),
    /// A named record member.  The ordinal is source-order identity, not a
    /// target ABI byte offset.
    Field(FieldProjection),
}

/// Checked array/pointer-index projection payload.
///
/// The semantic promotion claim is intentionally not constructible outside
/// this module until typed index parsing is attached:
///
/// ```compile_fail
/// use evocc_chir::{CHirExpr, IndexProjection};
/// use evocc_chir::integer::CIntegerType;
/// use evocc_types::TypeId;
///
/// fn forge<'a>(
///     index: &'a CHirExpr<'a>,
///     element_ty: TypeId,
/// ) -> IndexProjection<'a> {
///     IndexProjection {
///         index,
///         element_ty,
///         promoted_index: CIntegerType::ULONG,
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct IndexProjection<'arena> {
    index: &'arena CHirExpr<'arena>,
    element_ty: TypeId,
    promoted_index: CIntegerType,
}

impl IndexProjection<'_> {
    #[must_use]
    pub const fn index(&self) -> &CHirExpr<'_> {
        self.index
    }

    #[must_use]
    pub const fn element_ty(&self) -> TypeId {
        self.element_ty
    }

    #[must_use]
    pub const fn promoted_index(&self) -> CIntegerType {
        self.promoted_index
    }
}

/// Checked record-field projection payload.
#[derive(Debug, Clone, Copy)]
pub struct FieldProjection {
    tag: TagId,
    field_index: usize,
    field_ty: TypeId,
}

impl FieldProjection {
    #[must_use]
    pub const fn tag(self) -> TagId {
        self.tag
    }

    #[must_use]
    pub const fn field_index(self) -> usize {
        self.field_index
    }

    #[must_use]
    pub const fn field_ty(self) -> TypeId {
        self.field_ty
    }
}

impl<'arena> PlaceProjection<'arena> {
    /// The one source expression evaluated to resolve this projection, if any.
    #[must_use]
    pub const fn source_expression(&self) -> Option<&'arena CHirExpr<'arena>> {
        match self {
            Self::Index(projection) => Some(projection.index),
            Self::Field(_) => None,
        }
    }
}

/// A checked C lvalue description consumed by interpreters and lowering.
#[derive(Debug, Clone)]
pub struct MemoryPlace<'arena> {
    root: PlaceRoot<'arena>,
    projections: Vec<PlaceProjection<'arena>>,
    value_ty: TypeId,
    access_qualifiers: Qualifiers,
    modifiable: bool,
    addressable: bool,
    bit_field: bool,
}

impl<'arena> MemoryPlace<'arena> {
    /// Derive every lvalue fact from canonical roots and projections.
    pub fn derive(
        symbols: &SymbolTable,
        types: &TypeInterner,
        root: PlaceRoot<'arena>,
        projections: Vec<PlaceProjection<'arena>>,
        value_ty: TypeId,
    ) -> Result<Self, MemoryPlaceError> {
        Self::derive_inner(symbols, types, root, projections, value_ty)
    }

    /// Construct a place after checking all table-backed identities and the
    /// projection type chain, then verify independently supplied facts exactly
    /// match the derived authority. This entry point is intended for readers
    /// of serialized or independently assembled C-HIR.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        symbols: &SymbolTable,
        types: &TypeInterner,
        root: PlaceRoot<'arena>,
        projections: Vec<PlaceProjection<'arena>>,
        value_ty: TypeId,
        access_qualifiers: Qualifiers,
        modifiable: bool,
        addressable: bool,
        bit_field: bool,
    ) -> Result<Self, MemoryPlaceError> {
        let derived = Self::derive_inner(symbols, types, root, projections, value_ty)?;
        if access_qualifiers != derived.access_qualifiers {
            return Err(MemoryPlaceError::AccessQualifierMismatch {
                expected: derived.access_qualifiers,
                actual: access_qualifiers,
            });
        }
        if modifiable != derived.modifiable {
            return Err(MemoryPlaceError::ModifiableMismatch {
                expected: derived.modifiable,
                actual: modifiable,
            });
        }
        if addressable != derived.addressable {
            return Err(MemoryPlaceError::AddressableMismatch {
                expected: derived.addressable,
                actual: addressable,
            });
        }
        if bit_field != derived.bit_field {
            return Err(MemoryPlaceError::BitFieldMismatch {
                expected: derived.bit_field,
                actual: bit_field,
            });
        }
        Ok(derived)
    }

    fn derive_inner(
        symbols: &SymbolTable,
        types: &TypeInterner,
        root: PlaceRoot<'arena>,
        projections: Vec<PlaceProjection<'arena>>,
        value_ty: TypeId,
    ) -> Result<Self, MemoryPlaceError> {
        let root_is_pointer = matches!(&root, PlaceRoot::PointerExpression { .. });
        let (mut current, mut access_qualifiers, mut addressable) = match &root {
            PlaceRoot::Object { symbol } => {
                let declaration = symbols
                    .symbol(*symbol)
                    .map_err(|_| MemoryPlaceError::UnknownSymbol(*symbol))?;
                if !matches!(
                    declaration.kind(),
                    SymbolKind::Object | SymbolKind::Parameter
                ) {
                    return Err(MemoryPlaceError::RootIsNotObject {
                        symbol: *symbol,
                        actual: declaration.kind(),
                    });
                }
                let ty = declaration.ty();
                let qualifiers = types
                    .get(ty)
                    .ok_or(MemoryPlaceError::UnknownType(ty))?
                    .qualifiers;
                (
                    ty,
                    qualifiers,
                    declaration.storage_class() != Some(StorageClass::Register),
                )
            }
            PlaceRoot::PointerExpression {
                pointer,
                pointer_type,
            } => {
                let expression_type = pointer_expression_type(types, pointer)?;
                if expression_type != *pointer_type {
                    return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
                }
                if types.get(pointer_type.pointee()).is_none() {
                    return Err(MemoryPlaceError::UnknownType(pointer_type.pointee()));
                }
                (
                    pointer_type.pointee(),
                    pointer_type.pointee_qualifiers(),
                    true,
                )
            }
        };

        let mut bit_field = false;
        for (projection_index, projection) in projections.iter().enumerate() {
            if bit_field {
                return Err(MemoryPlaceError::ProjectionAfterBitField);
            }
            current = match projection {
                PlaceProjection::Index(projection) => {
                    let element_ty = projection.element_ty;
                    if types.get(element_ty).is_none() {
                        return Err(MemoryPlaceError::UnknownType(element_ty));
                    }
                    match types.get(current).map(|ty| &ty.kind) {
                        Some(TypeKind::Array { element, .. }) if *element == element_ty => {
                            access_qualifiers = merge_qualifiers(
                                access_qualifiers,
                                types
                                    .get(element_ty)
                                    .ok_or(MemoryPlaceError::UnknownType(element_ty))?
                                    .qualifiers,
                            );
                            element_ty
                        }
                        // A pointer-expression root already identifies its
                        // pointee rather than a canonical pointer layer.
                        _ if root_is_pointer && projection_index == 0 && current == element_ty => {
                            element_ty
                        }
                        _ => {
                            return Err(MemoryPlaceError::ProjectionTypeMismatch {
                                expected: current,
                                actual: element_ty,
                            });
                        }
                    }
                }
                PlaceProjection::Field(projection) => {
                    let tag = projection.tag;
                    let field_index = projection.field_index;
                    let field_ty = projection.field_ty;
                    let Some(canonical) = types.get(current) else {
                        return Err(MemoryPlaceError::UnknownType(current));
                    };
                    let current_tag = match canonical.kind {
                        TypeKind::Struct(tag) | TypeKind::Union(tag) => tag,
                        _ => {
                            return Err(MemoryPlaceError::FieldOnNonRecord { ty: current });
                        }
                    };
                    if current_tag != tag {
                        return Err(MemoryPlaceError::RecordTagMismatch {
                            expected: current_tag,
                            actual: tag,
                        });
                    }
                    let definition = types.tag(tag).ok_or(MemoryPlaceError::UnknownTag(tag))?;
                    let fields = match &definition.state {
                        TagState::Complete(TagDefinition::Struct(fields))
                        | TagState::Complete(TagDefinition::Union(fields)) => fields,
                        TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => {
                            return Err(MemoryPlaceError::IncompleteRecord(tag));
                        }
                    };
                    let field = fields
                        .get(field_index)
                        .ok_or(MemoryPlaceError::UnknownFieldIndex { tag, field_index })?;
                    if field.ty != field_ty {
                        return Err(MemoryPlaceError::ProjectionTypeMismatch {
                            expected: field.ty,
                            actual: field_ty,
                        });
                    }
                    access_qualifiers = merge_qualifiers(
                        access_qualifiers,
                        types
                            .get(field_ty)
                            .ok_or(MemoryPlaceError::UnknownType(field_ty))?
                            .qualifiers,
                    );
                    bit_field = field.bit_field.is_some();
                    addressable &= !bit_field;
                    field_ty
                }
            };
        }

        if types.get(value_ty).is_none() {
            return Err(MemoryPlaceError::UnknownType(value_ty));
        }
        if current != value_ty {
            return Err(MemoryPlaceError::ProjectionTypeMismatch {
                expected: current,
                actual: value_ty,
            });
        }
        let modifiable =
            !access_qualifiers.is_const && is_complete_modifiable_type(types, value_ty);

        Ok(Self {
            root,
            projections,
            value_ty,
            access_qualifiers,
            modifiable,
            addressable,
            bit_field,
        })
    }

    #[must_use]
    pub const fn root(&self) -> &PlaceRoot<'arena> {
        &self.root
    }

    /// The table-backed object identity at the root, when this is a direct
    /// object/subobject place. This is semantic authority, not a source
    /// expression, so visitors must consume it separately from
    /// [`Self::source_expressions`].
    #[must_use]
    pub const fn root_symbol(&self) -> Option<SymbolId> {
        match self.root {
            PlaceRoot::Object { symbol } => Some(symbol),
            PlaceRoot::PointerExpression { .. } => None,
        }
    }

    #[must_use]
    pub fn projections(&self) -> &[PlaceProjection<'arena>] {
        &self.projections
    }

    /// Source expressions needed to resolve the place, in C evaluation order.
    ///
    /// Consumers must traverse this iterator exactly once. In particular,
    /// there is no retained surface-syntax lvalue to evaluate a second time.
    /// A pointer root consumes its pointer expression first, followed by each
    /// index expression. Indexing through a loaded pointer subobject is not
    /// representable in this bounded slice and remains fail-closed.
    pub fn source_expressions(
        &self,
    ) -> impl DoubleEndedIterator<Item = &'arena CHirExpr<'arena>> + '_ {
        self.root.source_expression().into_iter().chain(
            self.projections
                .iter()
                .filter_map(PlaceProjection::source_expression),
        )
    }

    /// Recheck this place against the tables of its consuming C-HIR unit.
    /// This rejects a checked place transplanted into an unrelated unit whose
    /// numeric IDs happen to overlap.
    pub fn validate_against(
        &self,
        symbols: &SymbolTable,
        types: &TypeInterner,
    ) -> Result<(), MemoryPlaceError> {
        Self::try_new(
            symbols,
            types,
            self.root.clone(),
            self.projections.clone(),
            self.value_ty,
            self.access_qualifiers,
            self.modifiable,
            self.addressable,
            self.bit_field,
        )
        .map(|_| ())
    }

    #[must_use]
    pub const fn value_ty(&self) -> TypeId {
        self.value_ty
    }

    #[must_use]
    pub const fn access_qualifiers(&self) -> Qualifiers {
        self.access_qualifiers
    }

    #[must_use]
    pub const fn is_modifiable(&self) -> bool {
        self.modifiable
    }

    #[must_use]
    pub const fn is_addressable(&self) -> bool {
        self.addressable
    }

    #[must_use]
    pub const fn is_bit_field(&self) -> bool {
        self.bit_field
    }
}

/// A structural inconsistency in a typed memory place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPlaceError {
    UnknownSymbol(SymbolId),
    RootIsNotObject {
        symbol: SymbolId,
        actual: SymbolKind,
    },
    UnknownType(TypeId),
    NotPointerType(TypeId),
    PointerExpressionTypeUnavailable,
    PointerExpressionTypeMismatch,
    IncompatiblePointerTypes {
        left: TypeId,
        right: TypeId,
    },
    UnknownTag(TagId),
    IncompleteRecord(TagId),
    FieldOnNonRecord {
        ty: TypeId,
    },
    RecordTagMismatch {
        expected: TagId,
        actual: TagId,
    },
    UnknownFieldIndex {
        tag: TagId,
        field_index: usize,
    },
    ProjectionTypeMismatch {
        expected: TypeId,
        actual: TypeId,
    },
    ProjectionAfterBitField,
    AccessQualifierMismatch {
        expected: Qualifiers,
        actual: Qualifiers,
    },
    ModifiableMismatch {
        expected: bool,
        actual: bool,
    },
    AddressableMismatch {
        expected: bool,
        actual: bool,
    },
    BitFieldMismatch {
        expected: bool,
        actual: bool,
    },
}

impl fmt::Display for MemoryPlaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSymbol(symbol) => write!(formatter, "unknown symbol {}", symbol.raw()),
            Self::RootIsNotObject { symbol, actual } => write!(
                formatter,
                "memory-place root {} has kind {actual:?}, not object/parameter",
                symbol.raw()
            ),
            Self::UnknownType(ty) => write!(formatter, "unknown type {}", ty.raw()),
            Self::NotPointerType(ty) => write!(formatter, "type {} is not a pointer", ty.raw()),
            Self::PointerExpressionTypeUnavailable => formatter
                .write_str("pointer-place root expression has no checked canonical pointer type"),
            Self::PointerExpressionTypeMismatch => formatter
                .write_str("pointer-place descriptor disagrees with its root expression type"),
            Self::IncompatiblePointerTypes { left, right } => write!(
                formatter,
                "pointer pointee types {} and {} are incompatible",
                left.raw(),
                right.raw()
            ),
            Self::UnknownTag(tag) => write!(formatter, "unknown tag {}", tag.raw()),
            Self::IncompleteRecord(tag) => {
                write!(formatter, "record tag {} is incomplete", tag.raw())
            }
            Self::FieldOnNonRecord { ty } => {
                write!(
                    formatter,
                    "field projection starts from non-record type {}",
                    ty.raw()
                )
            }
            Self::RecordTagMismatch { expected, actual } => write!(
                formatter,
                "field projection names tag {}, expected {}",
                actual.raw(),
                expected.raw()
            ),
            Self::UnknownFieldIndex { tag, field_index } => write!(
                formatter,
                "record tag {} has no field {field_index}",
                tag.raw()
            ),
            Self::ProjectionTypeMismatch { expected, actual } => write!(
                formatter,
                "place projection type {} does not match expected {}",
                actual.raw(),
                expected.raw()
            ),
            Self::ProjectionAfterBitField => {
                formatter.write_str("a place projection follows a bit-field")
            }
            Self::AccessQualifierMismatch { expected, actual } => write!(
                formatter,
                "claimed access qualifiers {actual:?} do not match derived {expected:?}"
            ),
            Self::ModifiableMismatch { expected, actual } => write!(
                formatter,
                "claimed modifiable={actual} does not match derived {expected}"
            ),
            Self::AddressableMismatch { expected, actual } => write!(
                formatter,
                "claimed addressable={actual} does not match derived {expected}"
            ),
            Self::BitFieldMismatch { expected, actual } => write!(
                formatter,
                "claimed bit_field={actual} does not match derived {expected}"
            ),
        }
    }
}

impl std::error::Error for MemoryPlaceError {}

fn merge_qualifiers(left: Qualifiers, right: Qualifiers) -> Qualifiers {
    Qualifiers {
        is_const: left.is_const || right.is_const,
        is_volatile: left.is_volatile || right.is_volatile,
        is_restrict: left.is_restrict || right.is_restrict,
        is_atomic: left.is_atomic || right.is_atomic,
    }
}

fn pointer_expression_type(
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
) -> Result<PointerTypeDesc, MemoryPlaceError> {
    let ty = match expression {
        CHirExpr::Ident { facts, .. }
        | CHirExpr::Assign { facts, .. }
        | CHirExpr::MemoryAssign { facts, .. }
        | CHirExpr::Update { facts, .. }
        | CHirExpr::MemoryUpdate { facts, .. } => facts.ty(),
        CHirExpr::Cast(ty, _, _) => *ty,
        _ => return Err(MemoryPlaceError::PointerExpressionTypeUnavailable),
    };
    PointerTypeDesc::from_canonical_pointer(types, ty)
}

fn is_object_type(kind: &TypeKind) -> bool {
    !matches!(kind, TypeKind::Void | TypeKind::Function { .. })
}

fn is_complete_modifiable_type(types: &TypeInterner, ty: TypeId) -> bool {
    let Some(canonical) = types.get(ty) else {
        return false;
    };
    if canonical.qualifiers.is_const {
        return false;
    }
    match canonical.kind {
        TypeKind::Void | TypeKind::Array { .. } | TypeKind::Function { .. } => false,
        TypeKind::Struct(tag) | TypeKind::Union(tag) => {
            record_is_complete_without_const_subobjects(types, tag, &mut HashSet::new())
        }
        TypeKind::Enum(tag) => types
            .tag(tag)
            .is_some_and(|tag| matches!(tag.state, TagState::Complete(TagDefinition::Enum(_)))),
        TypeKind::BuiltinVaList
        | TypeKind::GnuInt128(_)
        | TypeKind::Int(_)
        | TypeKind::Float(_)
        | TypeKind::Complex(_)
        | TypeKind::Pointer(_) => true,
    }
}

fn record_is_complete_without_const_subobjects(
    types: &TypeInterner,
    tag: TagId,
    visiting: &mut HashSet<TagId>,
) -> bool {
    if !visiting.insert(tag) {
        return false;
    }
    let result = types.tag(tag).is_some_and(|definition| {
        let fields = match &definition.state {
            TagState::Complete(TagDefinition::Struct(fields))
            | TagState::Complete(TagDefinition::Union(fields)) => fields,
            TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => return false,
        };
        fields
            .iter()
            .all(|field| !type_contains_const_subobject(types, field.ty, visiting))
    });
    visiting.remove(&tag);
    result
}

fn type_contains_const_subobject(
    types: &TypeInterner,
    ty: TypeId,
    visiting: &mut HashSet<TagId>,
) -> bool {
    let Some(canonical) = types.get(ty) else {
        return true;
    };
    if canonical.qualifiers.is_const {
        return true;
    }
    match canonical.kind {
        TypeKind::Array { element, .. } => type_contains_const_subobject(types, element, visiting),
        TypeKind::Struct(tag) | TypeKind::Union(tag) => {
            !record_is_complete_without_const_subobjects(types, tag, visiting)
        }
        TypeKind::Void
        | TypeKind::BuiltinVaList
        | TypeKind::GnuInt128(_)
        | TypeKind::Int(_)
        | TypeKind::Float(_)
        | TypeKind::Complex(_)
        | TypeKind::Pointer(_)
        | TypeKind::Function { .. }
        | TypeKind::Enum(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IntBase, IntLit, IntSuffix, SymbolData};
    use evocc_common::{FileId, SourceSpan};
    use evocc_types::{CanonicalType, IntRank, RecordField, TagDefinition, TagKind};

    #[test]
    fn checked_place_rejects_forged_access_qualifiers() {
        let mut types = TypeInterner::new();
        let volatile_int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers {
                is_volatile: true,
                ..Qualifiers::default()
            },
        });
        let pointer = types.intern(CanonicalType {
            kind: TypeKind::Pointer(volatile_int),
            qualifiers: Qualifiers::default(),
        });
        let pointer_type = PointerTypeDesc::from_canonical_pointer(&types, pointer).unwrap();
        let operand = CHirExpr::Lit(
            IntLit {
                value: 0,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            SourceSpan::new(FileId::new(1), 0, 1, 1, 1),
        );
        let expression = CHirExpr::Cast(
            pointer,
            &operand,
            SourceSpan::new(FileId::new(1), 0, 1, 1, 1),
        );

        let error = MemoryPlace::try_new(
            &SymbolTable::new(),
            &types,
            PlaceRoot::PointerExpression {
                pointer: &expression,
                pointer_type,
            },
            Vec::new(),
            volatile_int,
            Qualifiers::default(),
            true,
            true,
            false,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            MemoryPlaceError::AccessQualifierMismatch { .. }
        ));
    }

    #[test]
    fn object_root_is_explicit_and_revalidated_in_the_consuming_tables() {
        let mut types = TypeInterner::new();
        let int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        });
        let mut symbols = SymbolTable::new();
        let symbol = symbols
            .declare(
                symbols.file_scope(),
                "x",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                int,
                SourceSpan::new(FileId::new(1), 0, 1, 1, 1),
            )
            .unwrap();
        let place = MemoryPlace::derive(
            &symbols,
            &types,
            PlaceRoot::Object { symbol },
            Vec::new(),
            int,
        )
        .unwrap();
        assert_eq!(place.root_symbol(), Some(symbol));
        assert_eq!(place.source_expressions().count(), 0);

        let mut foreign = SymbolTable::new();
        foreign
            .declare(
                foreign.file_scope(),
                "x",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                int,
                SourceSpan::new(FileId::new(2), 0, 1, 1, 1),
            )
            .unwrap();
        assert!(matches!(
            place.validate_against(&foreign, &types),
            Err(MemoryPlaceError::RootIsNotObject { .. })
        ));
    }

    #[test]
    fn pointer_root_descriptor_must_match_the_retained_expression_type() {
        let mut types = TypeInterner::new();
        let int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        });
        let long = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Long),
            qualifiers: Qualifiers::default(),
        });
        let int_pointer = types.intern(CanonicalType {
            kind: TypeKind::Pointer(int),
            qualifiers: Qualifiers::default(),
        });
        let long_pointer = types.intern(CanonicalType {
            kind: TypeKind::Pointer(long),
            qualifiers: Qualifiers::default(),
        });
        let mut symbols = SymbolTable::new();
        let symbol = symbols
            .declare(
                symbols.file_scope(),
                "p",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                int_pointer,
                SourceSpan::new(FileId::new(1), 0, 1, 1, 1),
            )
            .unwrap();
        let expression = CHirExpr::Ident {
            symbol,
            spelling: "p".to_owned(),
            facts: crate::ExprFacts::lvalue(int_pointer, true, true).unwrap(),
            span: SourceSpan::new(FileId::new(1), 0, 1, 1, 1),
        };
        let wrong = PointerTypeDesc::from_canonical_pointer(&types, long_pointer).unwrap();
        assert!(matches!(
            MemoryPlace::derive(
                &symbols,
                &types,
                PlaceRoot::PointerExpression {
                    pointer: &expression,
                    pointer_type: wrong,
                },
                Vec::new(),
                long,
            ),
            Err(MemoryPlaceError::PointerExpressionTypeMismatch)
        ));
    }

    #[test]
    fn aggregate_with_recursive_const_member_is_not_modifiable() {
        let mut types = TypeInterner::new();
        let const_int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers {
                is_const: true,
                ..Qualifiers::default()
            },
        });
        let inner_tag = types.declare_tag(TagKind::Struct, Some("Inner".to_owned()));
        let inner = types.intern(CanonicalType {
            kind: TypeKind::Struct(inner_tag),
            qualifiers: Qualifiers::default(),
        });
        types
            .complete_tag(
                inner_tag,
                TagDefinition::Struct(vec![RecordField {
                    name: Some("frozen".to_owned()),
                    ty: const_int,
                    bit_field: None,
                }]),
            )
            .unwrap();
        let outer_tag = types.declare_tag(TagKind::Struct, Some("Outer".to_owned()));
        let outer = types.intern(CanonicalType {
            kind: TypeKind::Struct(outer_tag),
            qualifiers: Qualifiers::default(),
        });
        types
            .complete_tag(
                outer_tag,
                TagDefinition::Struct(vec![RecordField {
                    name: Some("inner".to_owned()),
                    ty: inner,
                    bit_field: None,
                }]),
            )
            .unwrap();
        let mut symbols = SymbolTable::new();
        let symbol = symbols
            .declare(
                symbols.file_scope(),
                "outer",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                outer,
                SourceSpan::new(FileId::new(1), 0, 5, 1, 1),
            )
            .unwrap();
        let place = MemoryPlace::derive(
            &symbols,
            &types,
            PlaceRoot::Object { symbol },
            Vec::new(),
            outer,
        )
        .unwrap();
        assert!(!place.is_modifiable());
    }
}
