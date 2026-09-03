//! Typed, target-independent descriptions of C memory lvalues.
//!
//! A memory place records semantic identity and source expressions only.  It
//! deliberately contains no byte offsets: record placement, array stride, and
//! pointer width remain decisions of [`evocc_types::TargetDataLayout`].

use std::{collections::HashSet, fmt};

use evocc_types::{
    Qualifiers, TagDefinition, TagId, TagState, TargetDataLayout, TypeId, TypeInterner, TypeKind,
};

use crate::constant::eval_integer_constant;
use crate::integer::CIntegerType;
use crate::{BinaryOp, CHirExpr, StorageClass, SymbolId, SymbolKind, SymbolTable, UnaryOp};

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
    /// Derive the pointer type of any expression after the ordinary C value
    /// conversions. This is the sole place-root type classifier used by both
    /// the producer and every validating consumer.
    pub fn from_expression(
        symbols: &SymbolTable,
        types: &TypeInterner,
        expression: &CHirExpr<'_>,
    ) -> Result<Self, MemoryPlaceError> {
        pointer_expression_type(symbols, types, expression)
    }

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
    /// Descriptor for a pointer to `pointee`.
    ///
    /// Taking an address and array-to-pointer decay both produce a pointer
    /// value whose type need never have been written in the source, so this
    /// deliberately does not require the pointer type to have been interned.
    pub fn to_object(types: &TypeInterner, pointee: TypeId) -> Option<Self> {
        Some(Self {
            pointee,
            pointee_qualifiers: types.get(pointee)?.qualifiers,
        })
    }

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
            || (matches!(left_kind, TypeKind::Void) && is_object_or_function_type(right_kind))
        {
            left.pointee
        } else if matches!(right_kind, TypeKind::Void) && is_object_or_function_type(left_kind) {
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
    /// One exact compound-literal occurrence. Evaluating this root allocates
    /// and initializes that object exactly once for this place evaluation.
    CompoundLiteral {
        expression: &'arena CHirExpr<'arena>,
        ty: TypeId,
    },
}

impl<'arena> PlaceRoot<'arena> {
    /// The one source expression evaluated to resolve this root, if any.
    #[must_use]
    pub const fn source_expression(&self) -> Option<&'arena CHirExpr<'arena>> {
        match self {
            Self::Object { .. } => None,
            Self::PointerExpression { pointer, .. } => Some(*pointer),
            Self::CompoundLiteral { .. } => None,
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

impl<'arena> IndexProjection<'arena> {
    /// Describe one subscript step of a place.
    ///
    /// `index` is the subscript expression and is evaluated exactly once, in
    /// the order [`MemoryPlace::source_expressions`] yields it.
    ///
    /// `element_ty` must be the designated element's type; [`MemoryPlace::derive`]
    /// rechecks it against the array or pointee actually being projected, so a
    /// wrong value here cannot produce a place with a wrong stride.
    ///
    /// `promoted_index` records the subscript's type after integer promotion.
    /// Integer promotion is value preserving, so this is descriptive metadata
    /// for backends that must choose a machine width; interpreters compute the
    /// address from the subscript's own runtime value and therefore cannot be
    /// misled by it.
    #[must_use]
    pub const fn new(
        index: &'arena CHirExpr<'arena>,
        element_ty: TypeId,
        promoted_index: CIntegerType,
    ) -> Self {
        Self {
            index,
            element_ty,
            promoted_index,
        }
    }

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

/// Resolve one named member to its complete projection path.
///
/// Anonymous structure and union members are traversed recursively. Two
/// matching anonymous paths are ambiguous and therefore fail closed. Keeping
/// this lookup beside the checked projection type gives every producer and
/// consumer one rule for member access, designators, `offsetof`, and retained
/// attribute queries.
#[must_use]
pub fn record_member_path(
    types: &TypeInterner,
    record_ty: TypeId,
    name: &str,
) -> Option<Vec<FieldProjection>> {
    fn resolve(
        types: &TypeInterner,
        record_ty: TypeId,
        name: &str,
        visiting: &mut HashSet<TypeId>,
    ) -> Option<Vec<FieldProjection>> {
        if !visiting.insert(record_ty) {
            return None;
        }
        let canonical = types.get(record_ty)?;
        let tag = match canonical.kind {
            TypeKind::Struct(tag) | TypeKind::Union(tag) => tag,
            _ => return None,
        };
        let definition = types.tag(tag)?;
        let fields = match &definition.state {
            TagState::Complete(TagDefinition::Struct(fields))
            | TagState::Complete(TagDefinition::Union(fields)) => fields,
            TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => return None,
        };
        if let Some((index, field)) = fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name.as_deref() == Some(name))
        {
            visiting.remove(&record_ty);
            return Some(vec![FieldProjection::new(tag, index, field.ty)]);
        }

        let mut found = None;
        for (index, field) in fields
            .iter()
            .enumerate()
            .filter(|(_, field)| field.name.is_none() && field.bit_field.is_none())
        {
            if !matches!(
                types.get(field.ty).map(|ty| &ty.kind),
                Some(TypeKind::Struct(_) | TypeKind::Union(_))
            ) {
                continue;
            }
            let Some(mut nested) = resolve(types, field.ty, name, visiting) else {
                continue;
            };
            let mut path = vec![FieldProjection::new(tag, index, field.ty)];
            path.append(&mut nested);
            if found.replace(path).is_some() {
                visiting.remove(&record_ty);
                return None;
            }
        }
        visiting.remove(&record_ty);
        found
    }

    resolve(types, record_ty, name, &mut HashSet::new())
}

impl FieldProjection {
    /// Describe one member step of a place.
    ///
    /// `tag` is the record being projected and `field_index` is source-order
    /// member identity, not a target byte offset: offsets belong to the target
    /// layout service, so the same place stays valid across targets.
    ///
    /// [`MemoryPlace::derive`] rechecks the tag, the index bound, the record's
    /// completeness, and that `field_ty` is the member's declared type.
    #[must_use]
    pub const fn new(tag: TagId, field_index: usize, field_ty: TypeId) -> Self {
        Self {
            tag,
            field_index,
            field_ty,
        }
    }

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
                if !pointer_expression_matches(symbols, types, pointer, *pointer_type) {
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
            PlaceRoot::CompoundLiteral { expression, ty } => {
                if !matches!(expression, CHirExpr::CompoundLit(actual, _, _) if actual == ty) {
                    return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
                }
                let qualifiers = types
                    .get(*ty)
                    .ok_or(MemoryPlaceError::UnknownType(*ty))?
                    .qualifiers;
                (*ty, qualifiers, true)
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
        let modifiable = !access_qualifiers.is_const && is_modifiable_object_type(types, value_ty);

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
            PlaceRoot::PointerExpression { .. } | PlaceRoot::CompoundLiteral { .. } => None,
        }
    }

    /// Pointer type designating this place's root, before any projection.
    ///
    /// Address folding starts from this type, not from the projected value's
    /// type, so a consumer checking the root it pushed against the root it
    /// popped compares like with like.
    #[must_use]
    pub fn root_pointer_type(
        &self,
        symbols: &SymbolTable,
        types: &TypeInterner,
    ) -> Option<PointerTypeDesc> {
        match &self.root {
            PlaceRoot::PointerExpression { pointer_type, .. } => Some(*pointer_type),
            PlaceRoot::Object { symbol } => {
                PointerTypeDesc::to_object(types, symbols.symbol(*symbol).ok()?.ty())
            }
            PlaceRoot::CompoundLiteral { ty, .. } => PointerTypeDesc::to_object(types, *ty),
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
        // Alignment is a lower bound on both sides, so the merged access keeps
        // the stricter one.
        aligned: match (left.aligned, right.aligned) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        },
    }
}

fn pointer_expression_type(
    symbols: &SymbolTable,
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
) -> Result<PointerTypeDesc, MemoryPlaceError> {
    if let CHirExpr::StatementExpr {
        value: Some(value),
        facts,
        ..
    } = expression
    {
        // The producer records the post-lvalue-conversion result type, but a
        // checked place must not trust that record by itself. Reconstruct the
        // final expression's pointer descriptor, recursively for nested GNU
        // statement expressions, and require the two authorities to agree.
        if facts.category() != crate::ExprCategory::Value {
            return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
        }
        let derived = pointer_expression_type(symbols, types, value)?;
        let claimed = PointerTypeDesc::from_canonical_pointer(types, facts.ty())?;
        return (derived == claimed)
            .then_some(claimed)
            .ok_or(MemoryPlaceError::PointerExpressionTypeMismatch);
    }
    if let CHirExpr::Unary(UnaryOp::AddrOf, addressed, _) = expression {
        let pointee = addressed_expression_type(symbols, types, addressed)?;
        return PointerTypeDesc::to_object(types, pointee)
            .ok_or(MemoryPlaceError::UnknownType(pointee));
    }

    match expression {
        CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            ..
        } => {
            let declaration = symbols
                .symbol(*symbol)
                .map_err(|_| MemoryPlaceError::UnknownSymbol(*symbol))?;
            let ty = declaration.ty();
            let category_matches = match declaration.kind() {
                SymbolKind::Function => facts.category() == crate::ExprCategory::FunctionDesignator,
                SymbolKind::Object | SymbolKind::Parameter => {
                    facts.category() == crate::ExprCategory::LValue
                }
                SymbolKind::Enumerator | SymbolKind::Typedef => false,
            };
            if declaration.spelling() != spelling || facts.ty() != ty || !category_matches {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            converted_pointer_type(types, ty)
        }
        CHirExpr::MemoryRead { place, facts, .. } => {
            if facts.ty() != place.value_ty() || facts.category() != crate::ExprCategory::Value {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            converted_pointer_type(types, place.value_ty())
        }
        CHirExpr::Assign { place, facts, .. } | CHirExpr::Update { place, facts, .. } => {
            if facts.ty() != place.ty() || facts.category() != crate::ExprCategory::Value {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            converted_pointer_type(types, place.ty())
        }
        CHirExpr::MemoryAssign { place, facts, .. }
        | CHirExpr::MemoryUpdate { place, facts, .. } => {
            if facts.ty() != place.value_ty() || facts.category() != crate::ExprCategory::Value {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            converted_pointer_type(types, place.value_ty())
        }
        CHirExpr::Cast(ty, _, _) | CHirExpr::CompoundLit(ty, _, _) => {
            converted_pointer_type(types, *ty)
        }
        CHirExpr::Call(callee, _, _) => {
            converted_pointer_type(types, call_expression_return_type(symbols, types, callee)?)
        }
        CHirExpr::Unary(UnaryOp::Deref, pointer, _) => converted_pointer_type(
            types,
            pointer_expression_type(symbols, types, pointer)?.pointee(),
        ),
        CHirExpr::EvaluateOnce(operand, _) => pointer_expression_type(symbols, types, operand),
        CHirExpr::Binary(BinaryOp::Add, left, right, _) => {
            pointer_arithmetic_result(symbols, types, left, right, false)
        }
        CHirExpr::Binary(BinaryOp::Sub, left, right, _) => {
            pointer_arithmetic_result(symbols, types, left, right, true)
        }
        CHirExpr::Binary(BinaryOp::Comma, _, right, _) | CHirExpr::Comma(_, right, _) => {
            pointer_expression_type(symbols, types, right)
        }
        CHirExpr::Ternary(_, when_true, when_false, _) => {
            conditional_pointer_type(symbols, types, when_true, when_false)
        }
        CHirExpr::Elvis(operand, fallback, _) => {
            conditional_pointer_type(symbols, types, operand, fallback)
        }
        CHirExpr::BuiltinVarArgs { facts, .. }
        | CHirExpr::LabelAddress { facts, .. }
        | CHirExpr::BuiltinReturnAddress { facts, .. }
        | CHirExpr::BuiltinFrameAddress { facts, .. } => converted_pointer_type(types, facts.ty()),
        CHirExpr::Member(_, _, _) | CHirExpr::PtrMember(_, _, _) | CHirExpr::Index(_, _, _) => {
            converted_pointer_type(
                types,
                addressed_expression_type(symbols, types, expression)?,
            )
        }
        CHirExpr::Lit(_, _)
        | CHirExpr::FloatLit(_, _)
        | CHirExpr::CharLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::Unary(_, _, _)
        | CHirExpr::Binary(_, _, _, _)
        | CHirExpr::CheckedArithmetic { .. }
        | CHirExpr::BitBuiltin { .. }
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::SizeOf(_, _)
        | CHirExpr::SizeOfExpr(_, _, _)
        | CHirExpr::AlignOf(_, _)
        | CHirExpr::StatementExpr { value: None, .. } => {
            Err(MemoryPlaceError::PointerExpressionTypeUnavailable)
        }
        CHirExpr::StatementExpr { value: Some(_), .. } => {
            unreachable!("statement expressions are validated before the general match")
        }
    }
}

fn pointer_expression_matches(
    symbols: &SymbolTable,
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
    expected: PointerTypeDesc,
) -> bool {
    if pointer_expression_type(symbols, types, expression).is_ok_and(|actual| actual == expected) {
        return true;
    }
    match expression {
        CHirExpr::StringLit(literal, _) => string_literal_matches(types, literal.prefix, expected),
        CHirExpr::Ternary(_, when_true, when_false, _) => {
            conditional_arm_matches(symbols, types, when_true, expected)
                && conditional_arm_matches(symbols, types, when_false, expected)
        }
        CHirExpr::Elvis(operand, fallback, _) => {
            conditional_arm_matches(symbols, types, operand, expected)
                && conditional_arm_matches(symbols, types, fallback, expected)
        }
        _ => false,
    }
}

fn conditional_arm_matches(
    symbols: &SymbolTable,
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
    expected: PointerTypeDesc,
) -> bool {
    is_null_pointer_constant(symbols, types, expression)
        || pointer_expression_matches(symbols, types, expression, expected)
}

fn string_literal_matches(
    types: &TypeInterner,
    prefix: crate::StringPrefix,
    expected: PointerTypeDesc,
) -> bool {
    let Some(pointee) = types.get(expected.pointee()) else {
        return false;
    };
    let matches_literal = matches!(
        (prefix, &pointee.kind),
        (
            crate::StringPrefix::None | crate::StringPrefix::U8,
            TypeKind::Int(
                evocc_types::IntRank::Char
                    | evocc_types::IntRank::SChar
                    | evocc_types::IntRank::UChar
            )
        ) | (
            crate::StringPrefix::L,
            TypeKind::Int(evocc_types::IntRank::UShort)
        ) | (
            crate::StringPrefix::U16,
            TypeKind::Int(evocc_types::IntRank::UShort)
        ) | (
            crate::StringPrefix::U,
            TypeKind::Int(evocc_types::IntRank::UInt)
        )
    );
    matches_literal && expected.pointee_qualifiers() == pointee.qualifiers
}

fn converted_pointer_type(
    types: &TypeInterner,
    ty: TypeId,
) -> Result<PointerTypeDesc, MemoryPlaceError> {
    match types.get(ty).map(|canonical| &canonical.kind) {
        Some(TypeKind::Pointer(_)) => PointerTypeDesc::from_canonical_pointer(types, ty),
        Some(TypeKind::Array { element, .. }) => PointerTypeDesc::to_object(types, *element)
            .ok_or(MemoryPlaceError::UnknownType(*element)),
        Some(TypeKind::Function { .. }) => {
            PointerTypeDesc::to_object(types, ty).ok_or(MemoryPlaceError::UnknownType(ty))
        }
        Some(_) => Err(MemoryPlaceError::NotPointerType(ty)),
        None => Err(MemoryPlaceError::UnknownType(ty)),
    }
}

fn pointer_arithmetic_result(
    symbols: &SymbolTable,
    types: &TypeInterner,
    left: &CHirExpr<'_>,
    right: &CHirExpr<'_>,
    subtraction: bool,
) -> Result<PointerTypeDesc, MemoryPlaceError> {
    let left_pointer = pointer_expression_type(symbols, types, left);
    let right_pointer = pointer_expression_type(symbols, types, right);
    match (left_pointer, right_pointer, subtraction) {
        (Ok(pointer), Err(_), _) | (Err(_), Ok(pointer), false) => Ok(pointer),
        _ => Err(MemoryPlaceError::PointerExpressionTypeUnavailable),
    }
}

fn conditional_pointer_type(
    symbols: &SymbolTable,
    types: &TypeInterner,
    left: &CHirExpr<'_>,
    right: &CHirExpr<'_>,
) -> Result<PointerTypeDesc, MemoryPlaceError> {
    let left_pointer = pointer_expression_type(symbols, types, left);
    let right_pointer = pointer_expression_type(symbols, types, right);
    match (left_pointer, right_pointer) {
        (Ok(_), Ok(right)) if is_null_pointer_constant(symbols, types, left) => Ok(right),
        (Ok(left), Ok(_)) if is_null_pointer_constant(symbols, types, right) => Ok(left),
        (Ok(left), Ok(right)) => PointerTypeDesc::composite(types, left, right),
        (Ok(left), Err(_)) if is_null_pointer_constant(symbols, types, right) => Ok(left),
        (Err(_), Ok(right)) if is_null_pointer_constant(symbols, types, left) => Ok(right),
        _ => Err(MemoryPlaceError::PointerExpressionTypeUnavailable),
    }
}

fn is_null_pointer_constant(
    symbols: &SymbolTable,
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
) -> bool {
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    if eval_integer_constant(expression, types, symbols, &target)
        .is_some_and(|value| value.mathematical_value() == 0)
    {
        return true;
    }
    let CHirExpr::Cast(pointer, operand, _) = expression else {
        return false;
    };
    let Some(TypeKind::Pointer(_)) = types.get(*pointer).map(|ty| &ty.kind) else {
        return false;
    };
    eval_integer_constant(operand, types, symbols, &target)
        .is_some_and(|value| value.mathematical_value() == 0)
}

/// The bounded addressable forms needed to validate a pointer yielded by a
/// statement expression. More complex projected places arrive as a checked
/// `MemoryRead`; raw projection syntax remains fail-closed here.
fn addressed_expression_type(
    symbols: &SymbolTable,
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
) -> Result<TypeId, MemoryPlaceError> {
    let ty = match expression {
        CHirExpr::Ident { symbol, facts, .. } => {
            let ty = symbols
                .symbol(*symbol)
                .map_err(|_| MemoryPlaceError::UnknownSymbol(*symbol))?
                .ty();
            if facts.ty() != ty {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            ty
        }
        CHirExpr::MemoryRead { place, .. } => place.value_ty(),
        CHirExpr::CompoundLit(ty, _, _) => *ty,
        CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
            pointer_expression_type(symbols, types, pointer)?.pointee()
        }
        CHirExpr::Member(base, name, _) => {
            let base = addressed_expression_type(symbols, types, base)?;
            record_member_path(types, base, name)
                .and_then(|path| path.last().copied())
                .map(FieldProjection::field_ty)
                .ok_or(MemoryPlaceError::PointerExpressionTypeUnavailable)?
        }
        CHirExpr::PtrMember(pointer, name, _) => {
            let base = pointer_expression_type(symbols, types, pointer)?.pointee();
            record_member_path(types, base, name)
                .and_then(|path| path.last().copied())
                .map(FieldProjection::field_ty)
                .ok_or(MemoryPlaceError::PointerExpressionTypeUnavailable)?
        }
        CHirExpr::Index(left, right, _) => {
            let base = pointer_expression_type(symbols, types, left)
                .or_else(|_| pointer_expression_type(symbols, types, right))?;
            base.pointee()
        }
        _ => return Err(MemoryPlaceError::PointerExpressionTypeUnavailable),
    };
    types
        .get(ty)
        .map(|_| ty)
        .ok_or(MemoryPlaceError::UnknownType(ty))
}

fn call_expression_return_type(
    symbols: &SymbolTable,
    types: &TypeInterner,
    callee: &CHirExpr<'_>,
) -> Result<TypeId, MemoryPlaceError> {
    let function_ty = pointer_expression_type(symbols, types, callee)?.pointee();
    match types.get(function_ty).map(|ty| &ty.kind) {
        Some(TypeKind::Function { return_type, .. }) => Ok(*return_type),
        _ => Err(MemoryPlaceError::PointerExpressionTypeUnavailable),
    }
}

fn is_object_or_function_type(kind: &TypeKind) -> bool {
    !matches!(kind, TypeKind::Void)
}

/// Whether an object type itself is modifiable under C's recursive aggregate
/// rule. Qualifiers below a pointer never qualify the pointer object.
#[must_use]
pub fn is_modifiable_object_type(types: &TypeInterner, ty: TypeId) -> bool {
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
                    layout: Default::default(),
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
                    layout: Default::default(),
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

    /// A subscript place is only meaningful if the element type it claims is
    /// the array's actual element type, so the checked constructor plus
    /// `derive` must reject a forged element type instead of silently
    /// computing a wrong stride.
    #[test]
    fn subscript_projection_must_match_the_arrays_element_type() {
        let mut types = TypeInterner::new();
        let int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        });
        let long = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Long),
            qualifiers: Qualifiers::default(),
        });
        let array = types.intern(CanonicalType {
            kind: TypeKind::Array {
                element: int,
                bound: evocc_types::ArrayBound::Fixed(3),
            },
            qualifiers: Qualifiers::default(),
        });
        let mut symbols = SymbolTable::new();
        let object = symbols
            .declare(
                symbols.file_scope(),
                "values",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                array,
                span(),
            )
            .expect("declare array object");
        let index = literal_zero();

        let honest = MemoryPlace::derive(
            &symbols,
            &types,
            PlaceRoot::Object { symbol: object },
            vec![PlaceProjection::Index(IndexProjection::new(
                &index,
                int,
                CIntegerType::INT,
            ))],
            int,
        )
        .expect("an int element of an int array is a valid subscript place");
        assert_eq!(honest.value_ty(), int);
        assert!(honest.is_modifiable());

        let forged = MemoryPlace::derive(
            &symbols,
            &types,
            PlaceRoot::Object { symbol: object },
            vec![PlaceProjection::Index(IndexProjection::new(
                &index,
                long,
                CIntegerType::INT,
            ))],
            long,
        );
        assert!(
            matches!(forged, Err(MemoryPlaceError::ProjectionTypeMismatch { .. })),
            "a subscript claiming the wrong element type must not produce a place"
        );
    }

    /// Const on a member propagates to the projected access even when the
    /// containing object is unqualified, so the place must report itself
    /// unmodifiable rather than leaving that check to each consumer.
    #[test]
    fn a_const_member_projection_is_not_modifiable() {
        let mut types = TypeInterner::new();
        let int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        });
        let const_int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers {
                is_const: true,
                ..Qualifiers::default()
            },
        });
        let tag = types.declare_tag(TagKind::Struct, Some("S".to_owned()));
        types
            .complete_tag(
                tag,
                TagDefinition::Struct(vec![
                    RecordField {
                        layout: Default::default(),
                        name: Some("locked".to_owned()),
                        ty: const_int,
                        bit_field: None,
                    },
                    RecordField {
                        layout: Default::default(),
                        name: Some("open".to_owned()),
                        ty: int,
                        bit_field: None,
                    },
                ]),
            )
            .expect("define struct S");
        let record = types.intern(CanonicalType {
            kind: TypeKind::Struct(tag),
            qualifiers: Qualifiers::default(),
        });
        let mut symbols = SymbolTable::new();
        let object = symbols
            .declare(
                symbols.file_scope(),
                "s",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                record,
                span(),
            )
            .expect("declare struct object");

        let locked = MemoryPlace::derive(
            &symbols,
            &types,
            PlaceRoot::Object { symbol: object },
            vec![PlaceProjection::Field(FieldProjection::new(
                tag, 0, const_int,
            ))],
            const_int,
        )
        .expect("projecting a const member is legal; assigning to it is not");
        assert!(!locked.is_modifiable());
        assert!(locked.access_qualifiers().is_const);

        let open = MemoryPlace::derive(
            &symbols,
            &types,
            PlaceRoot::Object { symbol: object },
            vec![PlaceProjection::Field(FieldProjection::new(tag, 1, int))],
            int,
        )
        .expect("an unqualified sibling member stays modifiable");
        assert!(open.is_modifiable());
        assert!(!open.access_qualifiers().is_const);
    }

    fn span() -> SourceSpan {
        SourceSpan::new(FileId::new(1), 0, 1, 1, 1)
    }

    fn literal_zero() -> CHirExpr<'static> {
        CHirExpr::Lit(
            IntLit {
                value: 0,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span(),
        )
    }
}
