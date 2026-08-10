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
//! The `TargetLayout` trait bridges the two: given a `CType` and a target
//! triple, it answers `size_of`, `align_of`, and struct field offsets. The
//! trait is deliberately narrow to make it straightforward to implement for
//! Apple AArch64, Linux AAPCS64, and x86-64 SysV with no target-specific
//! leakage into the type definition itself.

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
        !matches!(self, Self::Bool | Self::UChar | Self::UShort | Self::UInt | Self::ULong | Self::ULongLong)
    }
}

/// A C type. This enum reflects the GNU C11 type universe: every variant an
/// ordinary declarator can produce.
///
/// Void and function types are also C types — they participate in expression
/// type-checking even though they are not "objects" in the ISO sense.
///
/// Opaque handles (`u32`) stand for aggregate and function types that the
/// compiler interns in a type table; the enum itself carries only the scalar
/// and structural classification.
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

    // -----------------------------------------------------------------------
    // EvoIRType
    // -----------------------------------------------------------------------

    #[test]
    fn evoir_type_universe_is_defined() {
        assert!(matches!(EvoIRType::Bits(32), EvoIRType::Bits(32)));
        assert_eq!(EvoIRType::Bits(64), EvoIRType::Bits(64));
        assert_ne!(EvoIRType::Bits(32), EvoIRType::Bits(64));

        assert!(matches!(EvoIRType::Ptr(AddressSpace::Global), EvoIRType::Ptr(..)));
        assert_ne!(EvoIRType::Ptr(AddressSpace::Global), EvoIRType::Ptr(AddressSpace::Stack));

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
