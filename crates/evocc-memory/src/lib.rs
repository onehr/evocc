//! Shared target-aware object memory for the C-HIR and EvoIR interpreters.
//!
//! `MemoryState` contains only object and lifetime state. `WorldState` contains
//! only externally ordered events. Both are immutable snapshots at the public
//! boundary: successful operations return a new state, while failures return
//! no partially changed state.

pub use evocc_types::ByteOrder;

use evocc_types::{TargetDataLayout, TypeLayout};

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

/// Immutable cursor states make a byte copy of a list independent on advancement.
pub struct ArgumentLists<T> {
    states: Vec<(std::sync::Arc<[T]>, usize)>,
}

impl<T> Default for ArgumentLists<T> {
    fn default() -> Self {
        Self { states: Vec::new() }
    }
}

impl<T> ArgumentLists<T> {
    pub fn start(&mut self, arguments: Vec<T>) -> usize {
        let identity = self.states.len();
        self.states.push((arguments.into(), 0));
        identity
    }

    pub fn read(&self, identity: usize) -> Result<&T, MemoryFault> {
        let (arguments, position) = self
            .states
            .get(identity)
            .ok_or(MemoryFault::InvalidArgumentList)?;
        arguments
            .get(*position)
            .ok_or(MemoryFault::InvalidArgumentList)
    }

    pub fn advance(&mut self, identity: usize) -> Result<usize, MemoryFault> {
        self.read(identity)?;
        let (arguments, position) = &self.states[identity];
        let next = self.states.len();
        self.states.push((arguments.clone(), position + 1));
        Ok(next)
    }
}

/// Stable identity for one allocated object. Zero is reserved for null.
///
/// IDs are capabilities local to the [`MemoryState`] lineage that produced
/// them. Mixing an ID or pointer with an independently created memory root is
/// structural API misuse; cross-interpreter observations use [`ObjectOrigin`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(u64);

impl ObjectId {
    pub const NULL: Self = Self(0);

    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

/// Stable identity for one interpreter call frame in one memory-state lineage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrameId(u64);

impl FrameId {
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Stable identity for one lexical scope within one memory-state lineage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(u64);

impl ScopeId {
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// An abstract C pointer: object provenance plus a signed byte offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pointer {
    object: ObjectId,
    byte_offset: i64,
    exposed_address: Option<u64>,
}

/// Out-of-band provenance carried by one complete pointer object
/// representation.
///
/// Object pointers keep their allocation and byte offset in [`Pointer`]. A
/// function pointer has no byte offset; its non-zero identity is assigned by
/// the translation-unit interpreter in declaration order. `Null` is shared by
/// both static pointer kinds and is interpreted according to the load result
/// type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PointerValue {
    Null,
    Object(Pointer),
    Function(u64),
    Exposed(u64),
}

/// Values admitted by the byte-oriented external C library execution boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryValue {
    Pointer(Pointer),
    Integer(u64),
}

mod atomic;
pub use atomic::{
    AtomicCopyOp, AtomicEvent, AtomicOrder, AtomicRmwOp, AtomicRmwResult, AtomicValue,
};
mod fenv;
pub use fenv::FenvRuntime;

impl PointerValue {
    #[must_use]
    pub const fn object(pointer: Pointer) -> Self {
        if pointer.object.is_null() && pointer.exposed_address.is_none() {
            Self::Null
        } else {
            Self::Object(pointer)
        }
    }

    pub fn function(identity: u64) -> Result<Self, MemoryFault> {
        if identity == 0 {
            return Err(MemoryFault::InvalidFunctionIdentity { identity });
        }
        Ok(Self::Function(identity))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OffsetDirection {
    Add,
    Subtract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexSignedness {
    Signed,
    Unsigned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OffsetOperand {
    Delta(i64),
    Scaled {
        raw_bits: u128,
        width_bits: u16,
        signedness: IndexSignedness,
        direction: OffsetDirection,
        scale_bytes: u64,
    },
}

impl Pointer {
    pub const NULL: Self = Self {
        object: ObjectId::NULL,
        byte_offset: 0,
        exposed_address: None,
    };

    pub fn new(object: ObjectId, byte_offset: i64) -> Result<Self, MemoryFault> {
        if object.is_null() && byte_offset != 0 {
            return Err(MemoryFault::InvalidNullPointer { byte_offset });
        }
        Ok(Self {
            object,
            byte_offset,
            exposed_address: None,
        })
    }

    #[must_use]
    pub const fn base(object: ObjectId) -> Self {
        Self {
            object,
            byte_offset: 0,
            exposed_address: None,
        }
    }

    /// An integer cast does not recover provenance until an actual access.
    #[must_use]
    pub const fn from_address(address: u64) -> Self {
        if address == 0 {
            Self::NULL
        } else {
            Self {
                object: ObjectId::NULL,
                byte_offset: 0,
                exposed_address: Some(address),
            }
        }
    }

    #[must_use]
    pub const fn object(self) -> ObjectId {
        self.object
    }

    #[must_use]
    pub const fn byte_offset(self) -> i64 {
        self.byte_offset
    }

    pub fn checked_offset(self, delta: i64) -> Result<Self, MemoryFault> {
        if let Some(address) = self.exposed_address {
            return Ok(Self::from_address(address.wrapping_add_signed(delta)));
        }
        if self.object.is_null() {
            return Err(MemoryFault::NullDereference { access: None });
        }
        let byte_offset =
            self.byte_offset
                .checked_add(delta)
                .ok_or(MemoryFault::PointerOffsetOverflow {
                    pointer: self,
                    operand: OffsetOperand::Delta(delta),
                })?;
        Self::new(self.object, byte_offset)
    }

    pub fn checked_scaled_offset(
        self,
        raw_bits: u128,
        width_bits: u16,
        signedness: IndexSignedness,
        direction: OffsetDirection,
        scale_bytes: u64,
    ) -> Result<Self, MemoryFault> {
        if self.object.is_null() && self.exposed_address.is_none() {
            return Err(MemoryFault::NullDereference { access: None });
        }
        let operand = OffsetOperand::Scaled {
            raw_bits,
            width_bits,
            signedness,
            direction,
            scale_bytes,
        };
        if width_bits == 0 || width_bits > 128 {
            return Err(MemoryFault::IndexWidthUnsupported { width_bits });
        }
        if scale_bytes == 0 {
            return Err(MemoryFault::InvalidOffsetScale { scale_bytes });
        }

        let mask = if width_bits == 128 {
            u128::MAX
        } else {
            (1_u128 << width_bits) - 1
        };
        let bits = raw_bits & mask;
        let sign_bit = 1_u128 << (width_bits - 1);
        let (negative, magnitude) = match signedness {
            IndexSignedness::Unsigned => (false, bits),
            IndexSignedness::Signed if bits & sign_bit == 0 => (false, bits),
            IndexSignedness::Signed => {
                let magnitude = ((!bits) & mask).wrapping_add(1) & mask;
                (true, magnitude)
            }
        };
        let scaled = magnitude.checked_mul(u128::from(scale_bytes)).ok_or(
            MemoryFault::PointerOffsetOverflow {
                pointer: self,
                operand,
            },
        )?;
        let scaled = i128::try_from(scaled).map_err(|_| MemoryFault::PointerOffsetOverflow {
            pointer: self,
            operand,
        })?;
        let subtract = negative ^ matches!(direction, OffsetDirection::Subtract);
        if let Some(address) = self.exposed_address {
            let delta = scaled as u64;
            return Ok(Self::from_address(if subtract {
                address.wrapping_sub(delta)
            } else {
                address.wrapping_add(delta)
            }));
        }
        let base = i128::from(self.byte_offset);
        let result = if subtract {
            base.checked_sub(scaled)
        } else {
            base.checked_add(scaled)
        }
        .ok_or(MemoryFault::PointerOffsetOverflow {
            pointer: self,
            operand,
        })?;
        let byte_offset =
            i64::try_from(result).map_err(|_| MemoryFault::PointerOffsetOverflow {
                pointer: self,
                operand,
            })?;
        Self::new(self.object, byte_offset)
    }
}

/// Size and required alignment for one memory access or allocated object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccessLayout {
    pub size_bytes: u64,
    pub align_bytes: u32,
}

impl AccessLayout {
    #[must_use]
    pub const fn new(size_bytes: u64, align_bytes: u32) -> Self {
        Self {
            size_bytes,
            align_bytes,
        }
    }

    #[must_use]
    pub const fn from_type_layout(layout: TypeLayout) -> Self {
        Self::new(layout.size_bytes, layout.align_bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InitialState {
    Uninitialized,
    Zeroed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectOwner {
    ThreadLocal { thread: ThreadId },
    External,
    Static,
    ReadOnlyStatic,
    StringLiteral,
    Automatic { frame: FrameId, scope: ScopeId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifetimeEnd {
    ThreadExit(ThreadId),
    ScopeExit(ScopeId),
    FrameExit(FrameId),
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectLifetime {
    Alive,
    Ended(LifetimeEnd),
}

impl ObjectLifetime {
    #[must_use]
    pub const fn is_alive(self) -> bool {
        matches!(self, Self::Alive)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectInfo {
    pub id: ObjectId,
    pub origin: ObjectOrigin,
    pub layout: AccessLayout,
    pub owner: ObjectOwner,
    pub lifetime: ObjectLifetime,
}

/// Cross-interpreter identity for one allocation-site execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectOrigin {
    pub thread: Option<ThreadId>,
    pub site: u64,
    pub instance: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryLimits {
    pub max_objects: u64,
    pub max_object_bytes: u64,
    pub max_total_bytes: u64,
}

// `MemoryLimits` is the trusted process-resource boundary selected by an
// interpreter. Semantic inputs at or below it fail through typed results;
// process-wide allocator exhaustion outside that configured boundary is not a
// C abstract-machine diagnostic.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldLimits {
    pub max_volatile_events: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryConfig {
    byte_order: ByteOrder,
    pointer_layout: AccessLayout,
    argument_list_layout: AccessLayout,
    function_alignment: u32,
    limits: MemoryLimits,
}

impl MemoryConfig {
    pub fn new(
        byte_order: ByteOrder,
        pointer_layout: AccessLayout,
        limits: MemoryLimits,
    ) -> Result<Self, MemoryFault> {
        if pointer_layout.size_bytes == 0
            || pointer_layout.size_bytes > 16
            || !is_valid_alignment(pointer_layout.align_bytes)
        {
            return Err(MemoryFault::InvalidPointerLayout {
                layout: pointer_layout,
            });
        }
        Ok(Self {
            byte_order,
            pointer_layout,
            // Untargeted callers retain the original Linux memory profile.
            // Interpreter entry points must use `for_target` instead.
            argument_list_layout: AccessLayout::new(32, 8),
            function_alignment: 16,
            limits,
        })
    }

    pub fn for_target(
        target: &TargetDataLayout,
        limits: MemoryLimits,
    ) -> Result<Self, MemoryFault> {
        let mut config = Self::new(
            target.byte_order(),
            AccessLayout::from_type_layout(target.pointer_layout()),
            limits,
        )?;
        config.argument_list_layout = AccessLayout::from_type_layout(target.va_list_layout());
        Ok(config)
    }

    /// A module-wide stride keeps every function aligned without changing its
    /// provenance identity. Larger strides also preserve distinct addresses.
    pub fn with_function_alignment(mut self, alignment: u32) -> Result<Self, MemoryFault> {
        if !is_valid_alignment(alignment) {
            return Err(MemoryFault::InvalidAlignment {
                align_bytes: alignment,
            });
        }
        self.function_alignment = alignment.max(16);
        Ok(self)
    }

    #[must_use]
    pub const fn byte_order(&self) -> ByteOrder {
        self.byte_order
    }

    #[must_use]
    pub const fn pointer_layout(&self) -> AccessLayout {
        self.pointer_layout
    }

    #[must_use]
    pub const fn limits(&self) -> MemoryLimits {
        self.limits
    }
}

/// Bytes plus opaque per-byte pointer provenance used by bytewise copies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteSequence {
    bytes: Vec<u8>,
    pointer_fragments: Vec<Option<PointerFragment>>,
}

/// The initializedness mask of a byte whose eight bits are all initialized.
/// Bit `i` of a mask is set when bit `i` of its byte is initialized. Only a
/// bit-field store sets part of a mask.
const INITIALIZED_BYTE: u8 = u8::MAX;

/// A transported object representation, including indeterminate bits and
/// opaque pointer fragments. Transport is not a semantic read of its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRepresentation {
    value: ByteSequence,
    initialized: Vec<u8>,
}

impl ObjectRepresentation {
    pub fn uninitialized(size: usize) -> Result<Self, MemoryFault> {
        Ok(Self {
            value: ByteSequence::try_from_bytes(repeat_value(size, 0, size as u64)?)?,
            initialized: repeat_value(size, 0, size as u64)?,
        })
    }

    pub fn size(&self) -> usize {
        self.value.bytes.len()
    }
    pub fn bytes(&self) -> &[u8] {
        self.value.bytes()
    }
    /// Whether each byte has all eight of its bits initialized.
    pub fn initialized(&self) -> Vec<bool> {
        complete_bytes(&self.initialized)
    }

    pub fn write_integer(
        &mut self,
        offset: usize,
        width: usize,
        raw: Option<u128>,
        order: ByteOrder,
    ) -> Result<(), MemoryFault> {
        let end = offset
            .checked_add(width)
            .filter(|end| *end <= self.size())
            .ok_or(MemoryFault::InvalidAccessSize {
                size_bytes: width as u64,
            })?;
        if width > 16 {
            return Err(MemoryFault::InvalidAccessSize {
                size_bytes: width as u64,
            });
        }
        let bits = raw.unwrap_or(0);
        let bytes = match order {
            ByteOrder::Little => bits.to_le_bytes(),
            ByteOrder::Big => bits.to_be_bytes(),
        };
        let slice = match order {
            ByteOrder::Little => &bytes[..width],
            ByteOrder::Big => &bytes[16 - width..],
        };
        self.value.bytes[offset..end].copy_from_slice(slice);
        self.value.pointer_fragments[offset..end].fill(None);
        self.initialized[offset..end].fill(if raw.is_some() { INITIALIZED_BYTE } else { 0 });
        Ok(())
    }

    pub fn read_integer(&self, offset: usize, width: usize, order: ByteOrder) -> Option<u128> {
        let end = offset.checked_add(width)?;
        if width > 16
            || end > self.size()
            || !self.initialized[offset..end]
                .iter()
                .all(|bits| *bits == INITIALIZED_BYTE)
        {
            return None;
        }
        let mut bytes = [0; 16];
        match order {
            ByteOrder::Little => {
                bytes[..width].copy_from_slice(&self.value.bytes[offset..end]);
                Some(u128::from_le_bytes(bytes))
            }
            ByteOrder::Big => {
                bytes[16 - width..].copy_from_slice(&self.value.bytes[offset..end]);
                Some(u128::from_be_bytes(bytes))
            }
        }
    }

    /// The integer that the bytes form when every bit is initialized and no
    /// byte holds a pointer fragment.
    #[must_use]
    pub fn plain_integer(&self, order: ByteOrder) -> Option<u128> {
        if self.value.pointer_fragments.iter().any(Option::is_some) {
            return None;
        }
        self.read_integer(0, self.size(), order)
    }

    /// A representation of `size` bytes that holds byte `i` of this one at
    /// byte `i + offset`, as a register shift moves bytes. A byte that falls
    /// outside the result is dropped, and every other byte of the result is
    /// an initialized zero. It is meant for register-sized representations.
    #[must_use]
    pub fn placed(&self, size: usize, offset: isize) -> Self {
        let mut placed = Self {
            value: ByteSequence {
                bytes: vec![0; size],
                pointer_fragments: vec![None; size],
            },
            initialized: vec![INITIALIZED_BYTE; size],
        };
        for source in 0..self.size() {
            let Some(target) = isize::try_from(source)
                .ok()
                .and_then(|source| source.checked_add(offset))
                .and_then(|target| usize::try_from(target).ok())
                .filter(|target| *target < size)
            else {
                continue;
            };
            placed.value.bytes[target] = self.value.bytes[source];
            placed.value.pointer_fragments[target] = self.value.pointer_fragments[source];
            placed.initialized[target] = self.initialized[source];
        }
        placed
    }

    /// The bitwise or of two representations of one size, as an `orr` of two
    /// registers combines them. Where one byte is an initialized zero without
    /// a pointer fragment, the result is the other byte, with its pointer
    /// fragment and its initializedness. Otherwise a bit of the result is
    /// initialized when both bits are, or when one of them is an initialized
    /// one. `None` is two sizes, or a pointer fragment that meets a byte that
    /// is not an initialized zero.
    #[must_use]
    pub fn or(&self, other: &Self) -> Option<Self> {
        if self.size() != other.size() {
            return None;
        }
        let mut result = self.clone();
        for index in 0..self.size() {
            if other.is_initialized_zero(index) {
                continue;
            }
            if self.is_initialized_zero(index) {
                result.value.bytes[index] = other.value.bytes[index];
                result.value.pointer_fragments[index] = other.value.pointer_fragments[index];
                result.initialized[index] = other.initialized[index];
                continue;
            }
            if self.value.pointer_fragments[index].is_some()
                || other.value.pointer_fragments[index].is_some()
            {
                return None;
            }
            let (left, right) = (self.value.bytes[index], other.value.bytes[index]);
            let (known_left, known_right) = (self.initialized[index], other.initialized[index]);
            result.value.bytes[index] = left | right;
            result.initialized[index] =
                (known_left & known_right) | (known_left & left) | (known_right & right);
        }
        Some(result)
    }

    fn is_initialized_zero(&self, index: usize) -> bool {
        self.initialized[index] == INITIALIZED_BYTE
            && self.value.bytes[index] == 0
            && self.value.pointer_fragments[index].is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PointerFragment {
    pointer: PointerValue,
    slot_size: u64,
    byte_index: u64,
}

impl PointerFragment {
    // Static initializer bits can denote an integer address independently of
    // a slot's offset. Ordinary runtime stores never manufacture this marker.
    const STATIC_INITIALIZER: Self = Self {
        pointer: PointerValue::Null,
        slot_size: 0,
        byte_index: 0,
    };
}

impl ByteSequence {
    /// Retain initializer bytes for a later pointer-sized address conversion.
    /// This is not a constructor for ordinary runtime byte writes.
    pub fn try_from_static_initializer(bytes: Vec<u8>) -> Result<Self, MemoryFault> {
        let mut value = Self::try_from_bytes(bytes)?;
        value
            .pointer_fragments
            .fill(Some(PointerFragment::STATIC_INITIALIZER));
        Ok(value)
    }

    pub fn try_from_bytes(bytes: Vec<u8>) -> Result<Self, MemoryFault> {
        let size_bytes = u64::try_from(bytes.len()).map_err(|_| MemoryFault::HostSizeExceeded {
            size_bytes: u64::MAX,
        })?;
        let pointer_fragments = repeat_value(bytes.len(), None, size_bytes)?;
        Ok(Self {
            bytes,
            pointer_fragments,
        })
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VolatileDirection {
    Load,
    Store,
}

/// Observable sides of an exact-width representation copy.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CopyVolatility {
    #[default]
    None,
    Read,
    Write,
    ReadWrite,
}

impl CopyVolatility {
    #[must_use]
    pub const fn new(read: bool, write: bool) -> Self {
        match (read, write) {
            (false, false) => Self::None,
            (true, false) => Self::Read,
            (false, true) => Self::Write,
            (true, true) => Self::ReadWrite,
        }
    }

    #[must_use]
    pub const fn reads(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    #[must_use]
    pub const fn writes(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }

    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Read => ".volatile.read",
            Self::Write => ".volatile.write",
            Self::ReadWrite => ".volatile.readwrite",
        }
    }

    #[must_use]
    pub const fn supports(self, access: AccessLayout) -> bool {
        matches!(self, Self::None)
            || (matches!(access.size_bytes, 1 | 2 | 4 | 8)
                && access.align_bytes as u64 >= access.size_bytes)
    }
}

/// Shared runtime code/message pair for program-semantic memory failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryDiagnostic {
    pub code: &'static str,
    pub message: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObservedPointer {
    Null,
    Object {
        origin: ObjectOrigin,
        byte_offset: i64,
    },
    Function {
        identity: u64,
    },
    ExposedAddress(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedValue {
    RawTargetBits(Vec<u8>),
    Pointer(ObservedPointer),
    /// Unknown padding remains unknown even though a single hardware access
    /// transfers the complete representation. Provenance is copied in memory.
    Representation {
        bytes: Vec<u8>,
        initialized: Vec<bool>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvalidLifetimeEndReason {
    NonBasePointer,
    NonAutomaticObject,
    ForeignFrame { current: FrameId, owner: FrameId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolatileEvent {
    pub sequence: u64,
    pub direction: VolatileDirection,
    pub origin: ObjectOrigin,
    pub byte_offset: i64,
    pub access: AccessLayout,
    pub value: ObservedValue,
}

/// Stable semantic category used by interpreter diagnostic mappings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryFaultKind {
    FloatingEnvironment,
    InvalidThread,
    InvalidAtomicAccess,
    InvalidArgumentList,
    ExternalObject,
    ExternalFunction,
    InvalidAlignment,
    InvalidAccessSize,
    InvalidPointerLayout,
    InvalidFunctionIdentity,
    ObjectLimitExceeded,
    ObjectTooLarge,
    TotalBytesLimitExceeded,
    HostSizeExceeded,
    HostAllocationFailed,
    ObjectIdExhausted,
    OriginAlreadyAllocated,
    OriginInstanceExhausted,
    FrameIdExhausted,
    ScopeIdExhausted,
    UnknownFrame,
    EndedFrame,
    UnknownScope,
    EndedScope,
    ScopeNotInFrame,
    ScopeHasLiveChildren,
    InvalidLifetimeEnd,
    InvalidNullPointer,
    InvalidOffsetScale,
    IndexWidthUnsupported,
    PointerOffsetOverflow,
    NullDereference,
    UnknownObject,
    LifetimeEnded,
    OutOfBounds,
    Misaligned,
    UninitializedRead,
    ReadOnlyObject,
    ByteLengthMismatch,
    IntegerWidthUnsupported,
    InvalidPointerRepresentation,
    OverlappingCopy,
    ObjectSizeCheckFailed,
    VolatileEventLimitExceeded,
    VolatileSequenceExhausted,
}

/// Typed memory failure. This layer owns the shared runtime code/message
/// selection, but deliberately owns no source span or IR obligation type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryFault {
    FloatingEnvironment {
        control: u32,
        status: u32,
    },
    InvalidThread {
        thread: ThreadId,
    },
    InvalidAtomicAccess,
    InvalidArgumentList,
    ExternalObject {
        object: ObjectId,
    },
    ExternalFunction {
        name: String,
    },
    InvalidAlignment {
        align_bytes: u32,
    },
    InvalidAccessSize {
        size_bytes: u64,
    },
    InvalidPointerLayout {
        layout: AccessLayout,
    },
    InvalidFunctionIdentity {
        identity: u64,
    },
    ObjectLimitExceeded {
        max_objects: u64,
    },
    ObjectTooLarge {
        requested: u64,
        max: u64,
    },
    TotalBytesLimitExceeded {
        current: u64,
        requested: u64,
        max: u64,
    },
    HostSizeExceeded {
        size_bytes: u64,
    },
    HostAllocationFailed {
        size_bytes: u64,
    },
    ObjectIdExhausted,
    OriginAlreadyAllocated {
        origin: ObjectOrigin,
    },
    OriginInstanceExhausted {
        site: u64,
    },
    FrameIdExhausted,
    ScopeIdExhausted,
    UnknownFrame {
        frame: FrameId,
    },
    EndedFrame {
        frame: FrameId,
    },
    UnknownScope {
        scope: ScopeId,
    },
    EndedScope {
        scope: ScopeId,
    },
    ScopeNotInFrame {
        frame: FrameId,
        scope: ScopeId,
    },
    ScopeHasLiveChildren {
        scope: ScopeId,
    },
    InvalidLifetimeEnd {
        address: Pointer,
        reason: InvalidLifetimeEndReason,
    },
    InvalidNullPointer {
        byte_offset: i64,
    },
    InvalidOffsetScale {
        scale_bytes: u64,
    },
    IndexWidthUnsupported {
        width_bits: u16,
    },
    PointerOffsetOverflow {
        pointer: Pointer,
        operand: OffsetOperand,
    },
    NullDereference {
        access: Option<AccessLayout>,
    },
    UnknownObject {
        object: ObjectId,
    },
    LifetimeEnded {
        object: ObjectId,
        ended_by: LifetimeEnd,
    },
    OutOfBounds {
        address: Pointer,
        access: Option<AccessLayout>,
        object_size: u64,
    },
    Misaligned {
        address: Pointer,
        required_align: u32,
        object_align: u32,
    },
    UninitializedRead {
        address: Pointer,
        size_bytes: u64,
        first_uninitialized: u64,
    },
    ReadOnlyObject {
        object: ObjectId,
    },
    ByteLengthMismatch {
        expected: u64,
        actual: u64,
    },
    IntegerWidthUnsupported {
        size_bytes: u64,
    },
    InvalidPointerRepresentation {
        address: Pointer,
        access: AccessLayout,
    },
    OverlappingCopy {
        destination: Pointer,
        source: Pointer,
        size_bytes: u64,
    },
    /// A checked C library entry point, such as `__memcpy_chk`, received a
    /// length larger than its destination size. The library stops the
    /// program instead of writing.
    ObjectSizeCheckFailed {
        name: String,
        length: u64,
        object_size: u64,
    },
    VolatileEventLimitExceeded {
        max_events: u64,
    },
    VolatileSequenceExhausted,
}

impl MemoryFault {
    #[must_use]
    pub const fn kind(&self) -> MemoryFaultKind {
        match self {
            Self::FloatingEnvironment { .. } => MemoryFaultKind::FloatingEnvironment,
            Self::InvalidThread { .. } => MemoryFaultKind::InvalidThread,
            Self::InvalidAtomicAccess => MemoryFaultKind::InvalidAtomicAccess,
            Self::InvalidArgumentList => MemoryFaultKind::InvalidArgumentList,
            Self::ExternalObject { .. } => MemoryFaultKind::ExternalObject,
            Self::ExternalFunction { .. } => MemoryFaultKind::ExternalFunction,
            Self::InvalidAlignment { .. } => MemoryFaultKind::InvalidAlignment,
            Self::InvalidAccessSize { .. } => MemoryFaultKind::InvalidAccessSize,
            Self::InvalidPointerLayout { .. } => MemoryFaultKind::InvalidPointerLayout,
            Self::InvalidFunctionIdentity { .. } => MemoryFaultKind::InvalidFunctionIdentity,
            Self::ObjectLimitExceeded { .. } => MemoryFaultKind::ObjectLimitExceeded,
            Self::ObjectTooLarge { .. } => MemoryFaultKind::ObjectTooLarge,
            Self::TotalBytesLimitExceeded { .. } => MemoryFaultKind::TotalBytesLimitExceeded,
            Self::HostSizeExceeded { .. } => MemoryFaultKind::HostSizeExceeded,
            Self::HostAllocationFailed { .. } => MemoryFaultKind::HostAllocationFailed,
            Self::ObjectIdExhausted => MemoryFaultKind::ObjectIdExhausted,
            Self::OriginAlreadyAllocated { .. } => MemoryFaultKind::OriginAlreadyAllocated,
            Self::OriginInstanceExhausted { .. } => MemoryFaultKind::OriginInstanceExhausted,
            Self::FrameIdExhausted => MemoryFaultKind::FrameIdExhausted,
            Self::ScopeIdExhausted => MemoryFaultKind::ScopeIdExhausted,
            Self::UnknownFrame { .. } => MemoryFaultKind::UnknownFrame,
            Self::EndedFrame { .. } => MemoryFaultKind::EndedFrame,
            Self::UnknownScope { .. } => MemoryFaultKind::UnknownScope,
            Self::EndedScope { .. } => MemoryFaultKind::EndedScope,
            Self::ScopeNotInFrame { .. } => MemoryFaultKind::ScopeNotInFrame,
            Self::ScopeHasLiveChildren { .. } => MemoryFaultKind::ScopeHasLiveChildren,
            Self::InvalidLifetimeEnd { .. } => MemoryFaultKind::InvalidLifetimeEnd,
            Self::InvalidNullPointer { .. } => MemoryFaultKind::InvalidNullPointer,
            Self::InvalidOffsetScale { .. } => MemoryFaultKind::InvalidOffsetScale,
            Self::IndexWidthUnsupported { .. } => MemoryFaultKind::IndexWidthUnsupported,
            Self::PointerOffsetOverflow { .. } => MemoryFaultKind::PointerOffsetOverflow,
            Self::NullDereference { .. } => MemoryFaultKind::NullDereference,
            Self::UnknownObject { .. } => MemoryFaultKind::UnknownObject,
            Self::LifetimeEnded { .. } => MemoryFaultKind::LifetimeEnded,
            Self::OutOfBounds { .. } => MemoryFaultKind::OutOfBounds,
            Self::Misaligned { .. } => MemoryFaultKind::Misaligned,
            Self::UninitializedRead { .. } => MemoryFaultKind::UninitializedRead,
            Self::ReadOnlyObject { .. } => MemoryFaultKind::ReadOnlyObject,
            Self::ByteLengthMismatch { .. } => MemoryFaultKind::ByteLengthMismatch,
            Self::IntegerWidthUnsupported { .. } => MemoryFaultKind::IntegerWidthUnsupported,
            Self::InvalidPointerRepresentation { .. } => {
                MemoryFaultKind::InvalidPointerRepresentation
            }
            Self::OverlappingCopy { .. } => MemoryFaultKind::OverlappingCopy,
            Self::ObjectSizeCheckFailed { .. } => MemoryFaultKind::ObjectSizeCheckFailed,
            Self::VolatileEventLimitExceeded { .. } => MemoryFaultKind::VolatileEventLimitExceeded,
            Self::VolatileSequenceExhausted => MemoryFaultKind::VolatileSequenceExhausted,
        }
    }

    /// Return the one shared runtime diagnostic for program-semantic faults.
    /// Structural, resource, and interpreter-boundary failures intentionally
    /// return `None` and must fail closed in the consumer.
    #[must_use]
    pub const fn runtime_diagnostic(&self) -> Option<MemoryDiagnostic> {
        let diagnostic = match self {
            Self::FloatingEnvironment { .. } => MemoryDiagnostic {
                code: "E3064",
                message: "incompatible-floating-environment",
            },
            Self::InvalidAtomicAccess => MemoryDiagnostic {
                code: "E3062",
                message: "invalid-atomic-access",
            },
            Self::InvalidArgumentList => MemoryDiagnostic {
                code: "E3038",
                message: "va-arg-mismatch",
            },
            Self::ExternalObject { .. } => MemoryDiagnostic {
                code: "E3040",
                message: "external-object-not-executable",
            },
            Self::ExternalFunction { .. } => MemoryDiagnostic {
                code: "E3039",
                message: "external-call-not-executable",
            },
            Self::InvalidLifetimeEnd { .. } => MemoryDiagnostic {
                code: "E3018",
                message: "invalid-lifetime-end",
            },
            Self::UninitializedRead { .. } => MemoryDiagnostic {
                code: "E3001",
                message: "read-of-uninitialized-object",
            },
            Self::NullDereference { .. } => MemoryDiagnostic {
                code: "E3009",
                message: "null-pointer-access",
            },
            Self::LifetimeEnded { .. } => MemoryDiagnostic {
                code: "E3010",
                message: "dangling-pointer-access",
            },
            Self::PointerOffsetOverflow { .. } | Self::OutOfBounds { .. } => MemoryDiagnostic {
                code: "E3011",
                message: "out-of-bounds-memory-access",
            },
            Self::Misaligned { .. } => MemoryDiagnostic {
                code: "E3012",
                message: "misaligned-memory-access",
            },
            Self::UnknownObject { .. }
            | Self::InvalidNullPointer { .. }
            | Self::InvalidPointerRepresentation { .. } => MemoryDiagnostic {
                code: "E3013",
                message: "invalid-pointer-representation",
            },
            Self::ReadOnlyObject { .. } => MemoryDiagnostic {
                code: "E3014",
                message: "write-to-read-only-object",
            },
            Self::OverlappingCopy { .. } => MemoryDiagnostic {
                code: "E3027",
                message: "overlapping-copy",
            },
            Self::ObjectSizeCheckFailed { .. } => MemoryDiagnostic {
                code: "E3066",
                message: "object-size-check-failed",
            },
            _ => return None,
        };
        Some(diagnostic)
    }
}

impl std::fmt::Display for MemoryFault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "memory fault: {:?}", self.kind())
    }
}

impl std::error::Error for MemoryFault {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObjectRecord {
    address: u64,
    info: ObjectInfo,
    bytes: Vec<u8>,
    /// One initializedness mask per byte; see [`INITIALIZED_BYTE`].
    initialized: Vec<u8>,
    pointer_fragments: Vec<Option<PointerFragment>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameRecord {
    alive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScopeRecord {
    frame: FrameId,
    parent: Option<ScopeId>,
    alive: bool,
}

enum AllocationContents<'a> {
    InitialState(InitialState),
    Bytes(&'a ByteSequence),
}

/// Immutable snapshot of objects, bytes, initialization, and lifetimes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryState {
    current_thread: ThreadId,
    threads: BTreeSet<ThreadId>,
    retired_threads: BTreeSet<ThreadId>,
    frozen_objects: BTreeSet<ObjectId>,
    tls_instances: BTreeMap<(u64, ThreadId), Pointer>,
    exposed_objects: BTreeSet<ObjectId>,
    exposed_functions: BTreeSet<u64>,
    argument_lists: BTreeMap<Pointer, usize>,
    config: MemoryConfig,
    objects: BTreeMap<ObjectId, ObjectRecord>,
    frames: BTreeMap<FrameId, FrameRecord>,
    scopes: BTreeMap<ScopeId, ScopeRecord>,
    next_object_id: Option<u64>,
    next_frame_id: Option<u64>,
    next_scope_id: Option<u64>,
    next_origin_instance: BTreeMap<u64, Option<u64>>,
    total_bytes: u64,
}

impl MemoryState {
    pub fn current_thread(&self) -> ThreadId {
        self.current_thread
    }

    pub fn begin_thread(&self, thread: ThreadId) -> Result<Self, MemoryFault> {
        if self.threads.contains(&thread) || self.retired_threads.contains(&thread) {
            return Err(MemoryFault::InvalidThread { thread });
        }
        let mut next = self.clone();
        next.threads.insert(thread);
        Ok(next)
    }

    pub fn select_thread(&self, thread: ThreadId) -> Result<Self, MemoryFault> {
        if !self.threads.contains(&thread) {
            return Err(MemoryFault::InvalidThread { thread });
        }
        let mut next = self.clone();
        next.current_thread = thread;
        Ok(next)
    }

    pub fn end_thread(&self, thread: ThreadId) -> Result<Self, MemoryFault> {
        if !self.threads.contains(&thread) {
            return Err(MemoryFault::InvalidThread { thread });
        }
        let mut next = self.clone();
        next.threads.remove(&thread);
        next.retired_threads.insert(thread);
        for object in next.objects.values_mut() {
            if object.info.owner == (ObjectOwner::ThreadLocal { thread }) {
                object.info.lifetime = ObjectLifetime::Ended(LifetimeEnd::ThreadExit(thread));
            }
        }
        Ok(next)
    }

    pub fn allocate_thread_local(
        &self,
        site: u64,
        layout: AccessLayout,
        initial: InitialState,
    ) -> Result<(Self, Pointer, bool), MemoryFault> {
        let thread = self.current_thread;
        if !self.threads.contains(&thread) {
            return Err(MemoryFault::InvalidThread { thread });
        }
        if let Some(pointer) = self.tls_instances.get(&(site, thread)) {
            let info = self
                .object_info(pointer.object)
                .expect("TLS map owns its object");
            if info.layout != layout {
                return Err(MemoryFault::OriginAlreadyAllocated {
                    origin: info.origin,
                });
            }
            return Ok((self.clone(), *pointer, false));
        }
        let (mut next, pointer) = self.allocate_object(
            ObjectOrigin {
                site,
                instance: 0,
                thread: Some(thread),
            },
            layout,
            ObjectOwner::ThreadLocal { thread },
            AllocationContents::InitialState(initial),
        )?;
        next.tls_instances.insert((site, thread), pointer);
        Ok((next, pointer, true))
    }
    pub fn new(config: MemoryConfig) -> Result<Self, MemoryFault> {
        Ok(Self {
            current_thread: ThreadId(0),
            threads: BTreeSet::from([ThreadId(0)]),
            retired_threads: BTreeSet::new(),
            frozen_objects: BTreeSet::new(),
            tls_instances: BTreeMap::new(),
            exposed_objects: BTreeSet::new(),
            exposed_functions: BTreeSet::new(),
            argument_lists: BTreeMap::new(),
            config,
            objects: BTreeMap::new(),
            frames: BTreeMap::new(),
            scopes: BTreeMap::new(),
            next_object_id: Some(1),
            next_frame_id: Some(1),
            next_scope_id: Some(1),
            next_origin_instance: BTreeMap::new(),
            total_bytes: 0,
        })
    }

    #[must_use]
    pub fn fork(&self) -> Self {
        self.clone()
    }

    /// Deterministic virtual target addresses, never host addresses.
    pub fn pointer_address(&self, pointer: PointerValue) -> Result<u64, MemoryFault> {
        match pointer {
            PointerValue::Null => Ok(0),
            PointerValue::Exposed(address) => Ok(address),
            PointerValue::Function(identity) => identity
                .checked_mul(u64::from(self.config.function_alignment))
                .filter(|value| identity != 0 && *value < (1_u64 << 63))
                .map(|value| value | (1_u64 << 63))
                .ok_or(MemoryFault::InvalidFunctionIdentity { identity }),
            PointerValue::Object(pointer) => {
                if let Some(address) = pointer.exposed_address {
                    return Ok(address);
                }
                let object =
                    self.objects
                        .get(&pointer.object)
                        .ok_or(MemoryFault::UnknownObject {
                            object: pointer.object,
                        })?;
                Ok(object.address.wrapping_add_signed(pointer.byte_offset))
            }
        }
    }

    pub fn expose_pointer(&self, pointer: PointerValue) -> Result<(Self, u64), MemoryFault> {
        let address = self.pointer_address(pointer)?;
        let mut next = self.clone();
        match pointer {
            PointerValue::Object(pointer) if pointer.exposed_address.is_none() => {
                next.exposed_objects.insert(pointer.object);
            }
            PointerValue::Function(identity) => {
                next.exposed_functions.insert(identity);
            }
            _ => {}
        }
        Ok((next, address))
    }

    pub fn resolve_pointer(&self, pointer: Pointer) -> Result<Pointer, MemoryFault> {
        let Some(address) = pointer.exposed_address else {
            return Ok(pointer);
        };
        let mut matches = self.objects.iter().filter(|(id, object)| {
            self.exposed_objects.contains(id)
                && object.info.lifetime.is_alive()
                && address >= object.address
                && address - object.address < object.info.layout.size_bytes.max(1)
        });
        let Some((id, object)) = matches.next() else {
            return Err(MemoryFault::InvalidPointerRepresentation {
                address: pointer,
                access: self.config.pointer_layout,
            });
        };
        if matches.next().is_some() {
            return Err(MemoryFault::InvalidPointerRepresentation {
                address: pointer,
                access: self.config.pointer_layout,
            });
        }
        Pointer::new(*id, (address - object.address) as i64)
    }

    pub fn resolve_function_pointer(
        &self,
        pointer: PointerValue,
    ) -> Result<PointerValue, MemoryFault> {
        let PointerValue::Exposed(address) = pointer else {
            return Ok(pointer);
        };
        for identity in &self.exposed_functions {
            if self.pointer_address(PointerValue::Function(*identity))? == address {
                return Ok(PointerValue::Function(*identity));
            }
        }
        Err(MemoryFault::InvalidPointerRepresentation {
            address: Pointer::from_address(address),
            access: self.config.pointer_layout,
        })
    }

    /// Iterate over stable object metadata in ascending `ObjectId` order.
    pub fn objects(&self) -> impl Iterator<Item = ObjectInfo> + '_ {
        self.objects.values().map(|record| record.info)
    }

    #[must_use]
    pub fn config(&self) -> &MemoryConfig {
        &self.config
    }

    pub fn begin_frame(&self) -> Result<(Self, FrameId, ScopeId), MemoryFault> {
        let frame_raw = self.next_frame_id.ok_or(MemoryFault::FrameIdExhausted)?;
        let scope_raw = self.next_scope_id.ok_or(MemoryFault::ScopeIdExhausted)?;
        let frame = FrameId(frame_raw);
        let root = ScopeId(scope_raw);
        let mut next = self.clone();
        next.next_frame_id = frame_raw.checked_add(1);
        next.next_scope_id = scope_raw.checked_add(1);
        next.frames.insert(frame, FrameRecord { alive: true });
        next.scopes.insert(
            root,
            ScopeRecord {
                frame,
                parent: None,
                alive: true,
            },
        );
        Ok((next, frame, root))
    }

    pub fn begin_scope(&self, parent: ScopeId) -> Result<(Self, ScopeId), MemoryFault> {
        let parent_record = self
            .scopes
            .get(&parent)
            .copied()
            .ok_or(MemoryFault::UnknownScope { scope: parent })?;
        if !parent_record.alive {
            return Err(MemoryFault::EndedScope { scope: parent });
        }
        let frame_record =
            self.frames
                .get(&parent_record.frame)
                .copied()
                .ok_or(MemoryFault::UnknownFrame {
                    frame: parent_record.frame,
                })?;
        if !frame_record.alive {
            return Err(MemoryFault::EndedFrame {
                frame: parent_record.frame,
            });
        }
        let scope_raw = self.next_scope_id.ok_or(MemoryFault::ScopeIdExhausted)?;
        let scope = ScopeId(scope_raw);
        let mut next = self.clone();
        next.next_scope_id = scope_raw.checked_add(1);
        next.scopes.insert(
            scope,
            ScopeRecord {
                frame: parent_record.frame,
                parent: Some(parent),
                alive: true,
            },
        );
        Ok((next, scope))
    }

    pub fn end_scope(&self, scope: ScopeId) -> Result<Self, MemoryFault> {
        let scope_record = self
            .scopes
            .get(&scope)
            .copied()
            .ok_or(MemoryFault::UnknownScope { scope })?;
        if !scope_record.alive {
            return Err(MemoryFault::EndedScope { scope });
        }
        if self
            .scopes
            .values()
            .any(|candidate| candidate.alive && candidate.parent == Some(scope))
        {
            return Err(MemoryFault::ScopeHasLiveChildren { scope });
        }
        let mut next = self.clone();
        let next_scope = next
            .scopes
            .get_mut(&scope)
            .ok_or(MemoryFault::UnknownScope { scope })?;
        next_scope.alive = false;
        for object in next.objects.values_mut() {
            if matches!(
                object.info.owner,
                ObjectOwner::Automatic {
                    scope: owner_scope,
                    ..
                } if owner_scope == scope
            ) && object.info.lifetime == ObjectLifetime::Alive
            {
                object.info.lifetime = ObjectLifetime::Ended(LifetimeEnd::ScopeExit(scope));
            }
        }
        Ok(next)
    }

    pub fn end_frame(&self, frame: FrameId) -> Result<Self, MemoryFault> {
        let frame_record = self
            .frames
            .get(&frame)
            .copied()
            .ok_or(MemoryFault::UnknownFrame { frame })?;
        if !frame_record.alive {
            return Err(MemoryFault::EndedFrame { frame });
        }
        let mut next = self.clone();
        let next_frame = next
            .frames
            .get_mut(&frame)
            .ok_or(MemoryFault::UnknownFrame { frame })?;
        next_frame.alive = false;
        for scope in next.scopes.values_mut() {
            if scope.frame == frame {
                scope.alive = false;
            }
        }
        for object in next.objects.values_mut() {
            if matches!(
                object.info.owner,
                ObjectOwner::Automatic {
                    frame: owner_frame,
                    ..
                } if owner_frame == frame
            ) && object.info.lifetime == ObjectLifetime::Alive
            {
                object.info.lifetime = ObjectLifetime::Ended(LifetimeEnd::FrameExit(frame));
            }
        }
        Ok(next)
    }

    /// End one live automatic object through its base pointer.
    pub fn end_object_lifetime(
        &self,
        current_frame: FrameId,
        pointer: Pointer,
    ) -> Result<Self, MemoryFault> {
        let current =
            self.frames
                .get(&current_frame)
                .copied()
                .ok_or(MemoryFault::UnknownFrame {
                    frame: current_frame,
                })?;
        if !current.alive {
            return Err(MemoryFault::EndedFrame {
                frame: current_frame,
            });
        }
        if pointer.object.is_null() {
            return Err(MemoryFault::NullDereference { access: None });
        }
        let object = self
            .objects
            .get(&pointer.object)
            .ok_or(MemoryFault::UnknownObject {
                object: pointer.object,
            })?;
        if let ObjectLifetime::Ended(ended_by) = object.info.lifetime {
            return Err(MemoryFault::LifetimeEnded {
                object: pointer.object,
                ended_by,
            });
        }
        if pointer.byte_offset != 0 {
            return Err(MemoryFault::InvalidLifetimeEnd {
                address: pointer,
                reason: InvalidLifetimeEndReason::NonBasePointer,
            });
        }
        let ObjectOwner::Automatic {
            frame: owner_frame, ..
        } = object.info.owner
        else {
            return Err(MemoryFault::InvalidLifetimeEnd {
                address: pointer,
                reason: InvalidLifetimeEndReason::NonAutomaticObject,
            });
        };
        if owner_frame != current_frame {
            return Err(MemoryFault::InvalidLifetimeEnd {
                address: pointer,
                reason: InvalidLifetimeEndReason::ForeignFrame {
                    current: current_frame,
                    owner: owner_frame,
                },
            });
        }
        let mut next = self.clone();
        let next_object =
            next.objects
                .get_mut(&pointer.object)
                .ok_or(MemoryFault::UnknownObject {
                    object: pointer.object,
                })?;
        next_object.info.lifetime = ObjectLifetime::Ended(LifetimeEnd::Explicit);
        Ok(next)
    }

    pub fn allocate_static(
        &self,
        origin_site: u64,
        layout: AccessLayout,
        initial: InitialState,
    ) -> Result<(Self, Pointer), MemoryFault> {
        self.allocate_object(
            ObjectOrigin {
                thread: None,
                site: origin_site,
                instance: 0,
            },
            layout,
            ObjectOwner::Static,
            AllocationContents::InitialState(initial),
        )
    }

    /// Register provenance without allocating bytes for storage owned by another unit.
    pub fn register_external(
        &self,
        origin_site: u64,
        size: Option<u64>,
        align: u32,
    ) -> Result<(Self, Pointer), MemoryFault> {
        let (mut next, pointer) = self.allocate_object(
            ObjectOrigin {
                thread: None,
                site: origin_site,
                instance: 0,
            },
            AccessLayout::new(0, align),
            ObjectOwner::External,
            AllocationContents::InitialState(InitialState::Uninitialized),
        )?;
        next.objects
            .get_mut(&pointer.object)
            .expect("just registered")
            .info
            .layout
            .size_bytes = size.unwrap_or(i64::MAX as u64);
        Ok((next, pointer))
    }

    /// Complete static initialization without exposing any object or function.
    /// Existing symbolic pointer fragments are retained; initialized plain
    /// bytes acquire only the same marker as a module's initializer bytes.
    pub fn finish_static_initializer(&self, pointer: Pointer) -> Result<Self, MemoryFault> {
        let object = self
            .objects
            .get(&pointer.object)
            .ok_or(MemoryFault::UnknownObject {
                object: pointer.object,
            })?;
        if pointer.byte_offset != 0
            || !matches!(
                object.info.owner,
                ObjectOwner::Static | ObjectOwner::ThreadLocal { .. }
            )
        {
            return Err(MemoryFault::ReadOnlyObject {
                object: pointer.object,
            });
        }
        let mut next = self.clone();
        let object = next
            .objects
            .get_mut(&pointer.object)
            .expect("object checked before cloning");
        for (initialized, fragment) in object.initialized.iter().zip(&mut object.pointer_fragments)
        {
            if *initialized == INITIALIZED_BYTE && fragment.is_none() {
                *fragment = Some(PointerFragment::STATIC_INITIALIZER);
            }
        }
        Ok(next)
    }

    /// Freeze a fully initialized static object against subsequent stores.
    pub fn freeze_static(&self, pointer: Pointer) -> Result<Self, MemoryFault> {
        if pointer.object.is_null() {
            return Err(MemoryFault::NullDereference { access: None });
        }
        let object = self
            .objects
            .get(&pointer.object)
            .ok_or(MemoryFault::UnknownObject {
                object: pointer.object,
            })?;
        if pointer.byte_offset != 0
            || !matches!(
                object.info.owner,
                ObjectOwner::Static | ObjectOwner::ThreadLocal { .. }
            )
        {
            return Err(MemoryFault::ReadOnlyObject {
                object: pointer.object,
            });
        }
        let mut next = self.clone();
        if matches!(object.info.owner, ObjectOwner::ThreadLocal { .. }) {
            next.frozen_objects.insert(pointer.object);
            return Ok(next);
        }
        next.objects
            .get_mut(&pointer.object)
            .expect("object checked immediately before cloning")
            .info
            .owner = ObjectOwner::ReadOnlyStatic;
        Ok(next)
    }

    pub fn allocate_string_literal(
        &self,
        origin_site: u64,
        layout: AccessLayout,
        value: &ByteSequence,
    ) -> Result<(Self, Pointer), MemoryFault> {
        self.allocate_object(
            ObjectOrigin {
                thread: None,
                site: origin_site,
                instance: 0,
            },
            layout,
            ObjectOwner::StringLiteral,
            AllocationContents::Bytes(value),
        )
    }

    pub fn allocate_automatic(
        &self,
        frame: FrameId,
        scope: ScopeId,
        origin_site: u64,
        layout: AccessLayout,
    ) -> Result<(Self, Pointer), MemoryFault> {
        let frame_record = self
            .frames
            .get(&frame)
            .copied()
            .ok_or(MemoryFault::UnknownFrame { frame })?;
        if !frame_record.alive {
            return Err(MemoryFault::EndedFrame { frame });
        }
        let scope_record = self
            .scopes
            .get(&scope)
            .copied()
            .ok_or(MemoryFault::UnknownScope { scope })?;
        if !scope_record.alive {
            return Err(MemoryFault::EndedScope { scope });
        }
        if scope_record.frame != frame {
            return Err(MemoryFault::ScopeNotInFrame { frame, scope });
        }
        let instance = self
            .next_origin_instance
            .get(&origin_site)
            .copied()
            .unwrap_or(Some(0))
            .ok_or(MemoryFault::OriginInstanceExhausted { site: origin_site })?;
        let origin = ObjectOrigin {
            thread: None,
            site: origin_site,
            instance,
        };
        let (mut next, pointer) = self.allocate_object(
            origin,
            layout,
            ObjectOwner::Automatic { frame, scope },
            AllocationContents::InitialState(InitialState::Uninitialized),
        )?;
        next.next_origin_instance
            .insert(origin_site, instance.checked_add(1));
        Ok((next, pointer))
    }

    pub fn object_info(&self, object: ObjectId) -> Result<ObjectInfo, MemoryFault> {
        self.objects
            .get(&object)
            .map(|record| record.info)
            .ok_or(MemoryFault::UnknownObject { object })
    }

    /// Check pointer provenance, lifetime, and offset. A one-past pointer is
    /// valid here because this operation does not dereference it.
    pub fn check_pointer(&self, pointer: Pointer) -> Result<(), MemoryFault> {
        let pointer = self.resolve_pointer(pointer)?;
        if pointer.object.is_null() {
            return Err(MemoryFault::NullDereference { access: None });
        }
        let object = self
            .objects
            .get(&pointer.object)
            .ok_or(MemoryFault::UnknownObject {
                object: pointer.object,
            })?;
        if let ObjectLifetime::Ended(ended_by) = object.info.lifetime {
            return Err(MemoryFault::LifetimeEnded {
                object: pointer.object,
                ended_by,
            });
        }
        if pointer.byte_offset < 0
            || u64::try_from(pointer.byte_offset)
                .map_or(true, |offset| offset > object.info.layout.size_bytes)
        {
            return Err(MemoryFault::OutOfBounds {
                address: pointer,
                access: None,
                object_size: object.info.layout.size_bytes,
            });
        }
        Ok(())
    }

    /// Check a complete non-empty access range and its required alignment.
    /// Initialization is deliberately checked only by loads.
    pub fn check_access(&self, address: Pointer, access: AccessLayout) -> Result<(), MemoryFault> {
        let address = self.resolve_pointer(address)?;
        if self
            .objects
            .get(&address.object)
            .is_some_and(|object| object.info.owner == ObjectOwner::External)
        {
            return Err(MemoryFault::ExternalObject {
                object: address.object,
            });
        }
        validate_access_layout(access)?;
        self.check_pointer(address)?;
        let object = self
            .objects
            .get(&address.object)
            .ok_or(MemoryFault::UnknownObject {
                object: address.object,
            })?;
        let offset = u64::try_from(address.byte_offset).map_err(|_| MemoryFault::OutOfBounds {
            address,
            access: Some(access),
            object_size: object.info.layout.size_bytes,
        })?;
        let end = offset
            .checked_add(access.size_bytes)
            .ok_or(MemoryFault::OutOfBounds {
                address,
                access: Some(access),
                object_size: object.info.layout.size_bytes,
            })?;
        if end > object.info.layout.size_bytes {
            return Err(MemoryFault::OutOfBounds {
                address,
                access: Some(access),
                object_size: object.info.layout.size_bytes,
            });
        }
        if object.info.layout.align_bytes < access.align_bytes
            || offset % u64::from(access.align_bytes) != 0
        {
            return Err(MemoryFault::Misaligned {
                address,
                required_align: access.align_bytes,
                object_align: object.info.layout.align_bytes,
            });
        }
        Ok(())
    }

    pub fn is_range_initialized(
        &self,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<bool, MemoryFault> {
        let address = self.resolve_pointer(address)?;
        let range = self.checked_range(address, access)?;
        let object = self
            .objects
            .get(&address.object)
            .ok_or(MemoryFault::UnknownObject {
                object: address.object,
            })?;
        Ok(object.initialized[range]
            .iter()
            .all(|bits| *bits == INITIALIZED_BYTE))
    }

    pub fn load_bytes(
        &self,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<ByteSequence, MemoryFault> {
        let address = self.resolve_pointer(address)?;
        self.check_argument_list_access(address, access, false)?;
        let range = self.checked_range(address, access)?;
        let object = self
            .objects
            .get(&address.object)
            .ok_or(MemoryFault::UnknownObject {
                object: address.object,
            })?;
        if let Some(relative) = object.initialized[range.clone()]
            .iter()
            .position(|bits| *bits != INITIALIZED_BYTE)
        {
            let first_uninitialized = u64::try_from(range.start + relative).map_err(|_| {
                MemoryFault::HostSizeExceeded {
                    size_bytes: access.size_bytes,
                }
            })?;
            return Err(MemoryFault::UninitializedRead {
                address,
                size_bytes: access.size_bytes,
                first_uninitialized,
            });
        }
        Ok(ByteSequence {
            bytes: clone_slice(&object.bytes[range.clone()], access.size_bytes)?,
            pointer_fragments: clone_slice(&object.pointer_fragments[range], access.size_bytes)?,
        })
    }

    pub fn load_representation(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        volatile: bool,
    ) -> Result<(ObjectRepresentation, WorldState), MemoryFault> {
        let resolved = self.resolve_pointer(address)?;
        self.check_argument_list_access(resolved, access, false)?;
        let range = self.checked_range(resolved, access)?;
        let object = &self.objects[&resolved.object];
        let representation = ObjectRepresentation {
            value: ByteSequence {
                bytes: clone_slice(&object.bytes[range.clone()], access.size_bytes)?,
                pointer_fragments: clone_slice(
                    &object.pointer_fragments[range.clone()],
                    access.size_bytes,
                )?,
            },
            initialized: clone_slice(&object.initialized[range], access.size_bytes)?,
        };
        let world = if volatile {
            self.append_world(
                world,
                VolatileDirection::Load,
                address,
                access,
                ObservedValue::Representation {
                    bytes: representation.value.bytes.clone(),
                    initialized: representation.initialized(),
                },
            )?
        } else {
            world.clone()
        };
        Ok((representation, world))
    }

    pub fn store_representation(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        representation: &ObjectRepresentation,
        volatile: bool,
    ) -> Result<(Self, WorldState), MemoryFault> {
        if representation.size() as u64 != access.size_bytes {
            return Err(MemoryFault::ByteLengthMismatch {
                expected: access.size_bytes,
                actual: representation.size() as u64,
            });
        }
        let resolved = self.resolve_pointer(address)?;
        self.check_argument_list_access(resolved, access, false)?;
        let range = self.checked_store_range(resolved, access)?;
        let mut next = self.clone();
        let object = next
            .objects
            .get_mut(&resolved.object)
            .ok_or(MemoryFault::UnknownObject {
                object: resolved.object,
            })?;
        object.bytes[range.clone()].copy_from_slice(&representation.value.bytes);
        object.pointer_fragments[range.clone()]
            .clone_from_slice(&representation.value.pointer_fragments);
        object.initialized[range].copy_from_slice(&representation.initialized);
        let world = if volatile {
            next.append_world(
                world,
                VolatileDirection::Store,
                address,
                access,
                ObservedValue::Representation {
                    bytes: representation.value.bytes.clone(),
                    initialized: representation.initialized(),
                },
            )?
        } else {
            world.clone()
        };
        Ok((next, world))
    }

    pub fn store_bytes(
        &self,
        address: Pointer,
        access: AccessLayout,
        value: &ByteSequence,
    ) -> Result<Self, MemoryFault> {
        let address = self.resolve_pointer(address)?;
        self.check_argument_list_access(address, access, false)?;
        let actual = u64::try_from(value.len()).map_err(|_| MemoryFault::HostSizeExceeded {
            size_bytes: u64::MAX,
        })?;
        if actual != access.size_bytes {
            return Err(MemoryFault::ByteLengthMismatch {
                expected: access.size_bytes,
                actual,
            });
        }
        let range = self.checked_store_range(address, access)?;
        let mut next = self.clone();
        let object = next
            .objects
            .get_mut(&address.object)
            .ok_or(MemoryFault::UnknownObject {
                object: address.object,
            })?;
        object.bytes[range.clone()].copy_from_slice(&value.bytes);
        object.initialized[range.clone()].fill(INITIALIZED_BYTE);
        object.pointer_fragments[range].clone_from_slice(&value.pointer_fragments);
        Ok(next)
    }

    /// Copy one object representation without semantically reading its bytes.
    ///
    /// Unlike [`Self::load_bytes`], this operation transfers the initialized
    /// bitmap verbatim, so padding and other indeterminate bytes do not trap.
    /// Complete stored pointer representations retain their provenance shadow.
    pub fn copy_bytes(
        &self,
        destination: Pointer,
        source: Pointer,
        access: AccessLayout,
    ) -> Result<Self, MemoryFault> {
        self.copy_representation(destination, source, access, false)
    }

    /// Exact-width volatile copy, sharing the ordinary representation transfer
    /// so padding and pointer metadata are not turned into scalar values.
    pub fn copy_with_volatile(
        &self,
        world: &WorldState,
        destination: Pointer,
        source: Pointer,
        access: AccessLayout,
        volatile: CopyVolatility,
    ) -> Result<(Self, WorldState), MemoryFault> {
        if !volatile.supports(access) {
            return Err(MemoryFault::InvalidAccessSize {
                size_bytes: access.size_bytes,
            });
        }
        let memory = self.copy_bytes(destination, source, access)?;
        if volatile == CopyVolatility::None {
            return Ok((memory, world.clone()));
        }
        let resolved = self.resolve_pointer(source)?;
        let range = self.checked_range(resolved, access)?;
        let object = &self.objects[&resolved.object];
        let observed = ObservedValue::Representation {
            bytes: clone_slice(&object.bytes[range.clone()], access.size_bytes)?,
            initialized: complete_bytes(&object.initialized[range]),
        };
        let mut world = world.clone();
        if volatile.reads() {
            world = self.append_world(
                &world,
                VolatileDirection::Load,
                source,
                access,
                observed.clone(),
            )?;
        }
        if volatile.writes() {
            world = memory.append_world(
                &world,
                VolatileDirection::Store,
                destination,
                access,
                observed,
            )?;
        }
        Ok((memory, world))
    }

    /// Execute the specified external library subset against abstract storage.
    /// Copies preserve initialization and provenance; comparisons actually read bytes.
    pub fn call_library(
        &self,
        name: &str,
        arguments: &[LibraryValue],
    ) -> Result<(Self, LibraryValue), MemoryFault> {
        use LibraryValue::{Integer, Pointer as Address};
        let byte = |address: Pointer, offset: u64| -> Result<u8, MemoryFault> {
            let offset = i64::try_from(offset)
                .map_err(|_| MemoryFault::InvalidAccessSize { size_bytes: offset })?;
            Ok(self
                .load_bytes(address.checked_offset(offset)?, AccessLayout::new(1, 1))?
                .bytes()[0])
        };
        // A checked entry point performs the plain operation after its check.
        // A failed check stops the program before the operation writes.
        if let (
            "__memset_chk" | "__memcpy_chk" | "__memmove_chk",
            [_, _, Integer(length), Integer(object_size)],
        ) = (name, arguments)
        {
            if length > object_size {
                return Err(MemoryFault::ObjectSizeCheckFailed {
                    name: name.to_owned(),
                    length: *length,
                    object_size: *object_size,
                });
            }
            let plain = &name[2..name.len() - 4];
            return self.call_library(plain, &arguments[..3]);
        }
        match (name, arguments) {
            ("memset", [Address(destination), Integer(value), Integer(size)]) => {
                if *size == 0 {
                    return Ok((self.clone(), Address(*destination)));
                }
                let access = AccessLayout::new(*size, 1);
                self.check_access(*destination, access)?;
                let length = usize::try_from(*size)
                    .map_err(|_| MemoryFault::HostSizeExceeded { size_bytes: *size })?;
                let bytes =
                    ByteSequence::try_from_bytes(repeat_value(length, *value as u8, *size)?)?;
                Ok((
                    self.store_bytes(*destination, access, &bytes)?,
                    Address(*destination),
                ))
            }
            ("memcpy" | "memmove", [Address(destination), Address(source), Integer(size)]) => {
                if *size == 0 {
                    return Ok((self.clone(), Address(*destination)));
                }
                Ok((
                    self.copy_representation(
                        *destination,
                        *source,
                        AccessLayout::new(*size, 1),
                        name == "memmove",
                    )?,
                    Address(*destination),
                ))
            }
            ("strlen", [Address(source)]) => {
                let mut length = 0;
                while byte(*source, length)? != 0 {
                    length += 1;
                }
                Ok((self.clone(), Integer(length)))
            }
            ("memcmp" | "strncmp", [Address(left), Address(right), Integer(size)]) => {
                for offset in 0..*size {
                    let a = byte(*left, offset)?;
                    let b = byte(*right, offset)?;
                    if a != b {
                        return Ok((self.clone(), Integer((i32::from(a) - i32::from(b)) as u64)));
                    }
                    if name == "strncmp" && a == 0 {
                        break;
                    }
                }
                Ok((self.clone(), Integer(0)))
            }
            ("strcmp", [Address(left), Address(right)]) => {
                let mut offset = 0;
                loop {
                    let a = byte(*left, offset)?;
                    let b = byte(*right, offset)?;
                    if a != b || a == 0 {
                        return Ok((self.clone(), Integer((i32::from(a) - i32::from(b)) as u64)));
                    }
                    offset += 1;
                }
            }
            _ => Err(MemoryFault::ExternalFunction {
                name: name.to_owned(),
            }),
        }
    }

    fn copy_representation(
        &self,
        destination: Pointer,
        source: Pointer,
        access: AccessLayout,
        overlap_allowed: bool,
    ) -> Result<Self, MemoryFault> {
        let destination = self.resolve_pointer(destination)?;
        let source = self.resolve_pointer(source)?;
        if !is_valid_alignment(access.align_bytes) {
            return Err(MemoryFault::InvalidAlignment {
                align_bytes: access.align_bytes,
            });
        }
        self.check_pointer(destination)?;
        self.check_pointer(source)?;
        if access.size_bytes == 0 {
            return Ok(self.clone());
        }
        if access.size_bytes > i64::MAX as u64 {
            return Err(MemoryFault::InvalidAccessSize {
                size_bytes: access.size_bytes,
            });
        }

        let destination_range = self.checked_store_range(destination, access)?;
        let source_range = self.checked_range(source, access)?;
        self.check_argument_list_access(source, access, true)?;
        self.check_argument_list_access(destination, access, true)?;
        if !overlap_allowed
            && destination.object == source.object
            && destination_range != source_range
            && destination_range.start < source_range.end
            && source_range.start < destination_range.end
        {
            return Err(MemoryFault::OverlappingCopy {
                destination,
                source,
                size_bytes: access.size_bytes,
            });
        }
        if destination.object == source.object && destination_range == source_range {
            return Ok(self.clone());
        }

        let source_object = self
            .objects
            .get(&source.object)
            .ok_or(MemoryFault::UnknownObject {
                object: source.object,
            })?;
        let bytes = clone_slice(
            &source_object.bytes[source_range.clone()],
            access.size_bytes,
        )?;
        let initialized = clone_slice(
            &source_object.initialized[source_range.clone()],
            access.size_bytes,
        )?;
        let pointer_fragments = clone_slice(
            &source_object.pointer_fragments[source_range],
            access.size_bytes,
        )?;

        let mut next = self.clone();
        let destination_object =
            next.objects
                .get_mut(&destination.object)
                .ok_or(MemoryFault::UnknownObject {
                    object: destination.object,
                })?;
        destination_object.bytes[destination_range.clone()].copy_from_slice(&bytes);
        destination_object.initialized[destination_range.clone()].copy_from_slice(&initialized);
        destination_object.pointer_fragments[destination_range]
            .clone_from_slice(&pointer_fragments);
        let end = i128::from(destination.byte_offset) + i128::from(access.size_bytes);
        next.argument_lists.retain(|pointer, _| {
            pointer.object != destination.object
                || i128::from(pointer.byte_offset) < i128::from(destination.byte_offset)
                || i128::from(pointer.byte_offset) >= end
        });
        for (pointer, identity) in &self.argument_lists {
            if pointer.object == source.object
                && pointer.byte_offset >= source.byte_offset
                && i128::from(pointer.byte_offset)
                    + i128::from(self.config.argument_list_layout.size_bytes)
                    <= i128::from(source.byte_offset) + i128::from(access.size_bytes)
            {
                let relocated =
                    destination.checked_offset(pointer.byte_offset - source.byte_offset)?;
                next.argument_lists.insert(relocated, *identity);
            }
        }
        Ok(next)
    }

    fn check_argument_list_access(
        &self,
        address: Pointer,
        access: AccessLayout,
        copying: bool,
    ) -> Result<(), MemoryFault> {
        let address = self.resolve_pointer(address)?;
        let start = i128::from(address.byte_offset);
        let end = start + i128::from(access.size_bytes);
        for pointer in self
            .argument_lists
            .keys()
            .filter(|pointer| pointer.object == address.object)
        {
            let list_start = i128::from(pointer.byte_offset);
            let list_end = list_start + i128::from(self.config.argument_list_layout.size_bytes);
            if start < list_end
                && list_start < end
                && (!copying || start > list_start || end < list_end)
            {
                return Err(MemoryFault::InvalidArgumentList);
            }
        }
        Ok(())
    }

    /// The list representation is opaque; ordinary complete copies preserve its identity.
    pub fn argument_list(&self, address: Pointer) -> Result<usize, MemoryFault> {
        let address = self.resolve_pointer(address)?;
        self.checked_range(address, self.config.argument_list_layout)?;
        self.argument_lists
            .get(&address)
            .copied()
            .ok_or(MemoryFault::InvalidArgumentList)
    }

    pub fn with_argument_list(
        &self,
        address: Pointer,
        identity: Option<usize>,
    ) -> Result<Self, MemoryFault> {
        let address = self.resolve_pointer(address)?;
        let access = self.config.argument_list_layout;
        let range = self.checked_store_range(address, access)?;
        self.check_argument_list_access(address, access, true)?;
        let mut next = self.clone();
        let object = next
            .objects
            .get_mut(&address.object)
            .ok_or(MemoryFault::InvalidArgumentList)?;
        object.bytes[range.clone()].fill(0);
        object.initialized[range.clone()].fill(INITIALIZED_BYTE);
        object.pointer_fragments[range].fill(None);
        if let Some(identity) = identity {
            next.argument_lists.insert(address, identity);
        } else {
            next.argument_lists.remove(&address);
        }
        Ok(next)
    }

    pub fn load_integer(
        &self,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<u128, MemoryFault> {
        validate_integer_width(access.size_bytes)?;
        let value = self.load_bytes(address, access)?;
        decode_integer(self.config.byte_order, &value.bytes)
    }

    pub fn store_integer(
        &self,
        address: Pointer,
        access: AccessLayout,
        value: u128,
    ) -> Result<Self, MemoryFault> {
        let representation = encode_integer(self.config.byte_order, access.size_bytes, value)?;
        self.store_bytes(address, access, &representation)
    }

    /// Load an integer of which only the bits that `demanded` selects must
    /// be initialized. Every bit of the result is the bit that memory holds;
    /// a bit outside `demanded` can be uninitialized, and then its value is
    /// one that the program never stored. A bit-field read demands its own
    /// bits, and the read of a bit-field store demands none.
    pub fn load_integer_bits(
        &self,
        address: Pointer,
        access: AccessLayout,
        demanded: u128,
    ) -> Result<u128, MemoryFault> {
        validate_integer_width(access.size_bytes)?;
        let address = self.resolve_pointer(address)?;
        self.check_argument_list_access(address, access, false)?;
        let range = self.checked_range(address, access)?;
        let object = self
            .objects
            .get(&address.object)
            .ok_or(MemoryFault::UnknownObject {
                object: address.object,
            })?;
        let demanded = encode_integer(self.config.byte_order, access.size_bytes, demanded)?;
        if let Some(relative) = demanded
            .bytes
            .iter()
            .zip(&object.initialized[range.clone()])
            .position(|(wanted, present)| wanted & !present != 0)
        {
            let first_uninitialized = u64::try_from(range.start + relative).map_err(|_| {
                MemoryFault::HostSizeExceeded {
                    size_bytes: access.size_bytes,
                }
            })?;
            return Err(MemoryFault::UninitializedRead {
                address,
                size_bytes: access.size_bytes,
                first_uninitialized,
            });
        }
        decode_integer(self.config.byte_order, &object.bytes[range])
    }

    /// Store every bit of an integer, but initialize only the bits that
    /// `initializing` selects. Every other bit keeps its initializedness, so
    /// a bit-field store initializes its own bits and no neighbour's.
    pub fn store_integer_bits(
        &self,
        address: Pointer,
        access: AccessLayout,
        value: u128,
        initializing: u128,
    ) -> Result<Self, MemoryFault> {
        let representation = encode_integer(self.config.byte_order, access.size_bytes, value)?;
        let initializing = encode_integer(self.config.byte_order, access.size_bytes, initializing)?;
        let address = self.resolve_pointer(address)?;
        let mut next = self.store_bytes(address, access, &representation)?;
        let range = self.checked_range(address, access)?;
        let previous = &self.objects[&address.object].initialized[range.clone()];
        let object = next
            .objects
            .get_mut(&address.object)
            .ok_or(MemoryFault::UnknownObject {
                object: address.object,
            })?;
        for ((bits, before), added) in object.initialized[range]
            .iter_mut()
            .zip(previous)
            .zip(&initializing.bytes)
        {
            *bits = before | added;
        }
        Ok(next)
    }

    pub fn load_pointer(
        &self,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<Pointer, MemoryFault> {
        match self.load_pointer_value(address, access)? {
            PointerValue::Null => Ok(Pointer::NULL),
            PointerValue::Object(pointer) => Ok(pointer),
            PointerValue::Exposed(value) => Ok(Pointer::from_address(value)),
            PointerValue::Function(_) => {
                Err(MemoryFault::InvalidPointerRepresentation { address, access })
            }
        }
    }

    pub fn store_pointer(
        &self,
        address: Pointer,
        access: AccessLayout,
        value: Pointer,
    ) -> Result<Self, MemoryFault> {
        self.store_pointer_value(address, access, PointerValue::object(value))
    }

    /// Load one complete pointer representation without discarding its
    /// out-of-band object/function/null provenance.
    pub fn load_pointer_value(
        &self,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<PointerValue, MemoryFault> {
        self.require_pointer_layout(access)?;
        let representation = self.load_bytes(address, access)?;
        self.decode_pointer_value(address, access, &representation)
    }

    /// Store one complete pointer representation with its provenance.
    pub fn store_pointer_value(
        &self,
        address: Pointer,
        access: AccessLayout,
        value: PointerValue,
    ) -> Result<Self, MemoryFault> {
        self.require_pointer_layout(access)?;
        self.checked_store_range(address, access)?;
        self.check_storable_pointer_value(value)?;
        let representation = self.encode_pointer_value(value)?;
        self.store_bytes(address, access, &representation)
    }

    pub fn volatile_load_bytes(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<(ByteSequence, WorldState), MemoryFault> {
        let value = self.load_bytes(address, access)?;
        let observed = ObservedValue::RawTargetBits(clone_slice(&value.bytes, access.size_bytes)?);
        let next_world =
            self.append_world(world, VolatileDirection::Load, address, access, observed)?;
        Ok((value, next_world))
    }

    pub fn volatile_store_bytes(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        value: &ByteSequence,
    ) -> Result<(Self, WorldState), MemoryFault> {
        let next_memory = self.store_bytes(address, access, value)?;
        let observed = ObservedValue::RawTargetBits(clone_slice(&value.bytes, access.size_bytes)?);
        let next_world =
            self.append_world(world, VolatileDirection::Store, address, access, observed)?;
        Ok((next_memory, next_world))
    }

    pub fn volatile_load_integer(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<(u128, WorldState), MemoryFault> {
        validate_integer_width(access.size_bytes)?;
        let representation = self.load_bytes(address, access)?;
        let value = decode_integer(self.config.byte_order, &representation.bytes)?;
        let observed =
            ObservedValue::RawTargetBits(clone_slice(&representation.bytes, access.size_bytes)?);
        let next_world =
            self.append_world(world, VolatileDirection::Load, address, access, observed)?;
        Ok((value, next_world))
    }

    pub fn volatile_store_integer(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        value: u128,
    ) -> Result<(Self, WorldState), MemoryFault> {
        let representation = encode_integer(self.config.byte_order, access.size_bytes, value)?;
        let next_memory = self.store_bytes(address, access, &representation)?;
        let observed =
            ObservedValue::RawTargetBits(clone_slice(&representation.bytes, access.size_bytes)?);
        let next_world =
            self.append_world(world, VolatileDirection::Store, address, access, observed)?;
        Ok((next_memory, next_world))
    }

    /// A volatile load of which only the bits that `demanded` selects must be
    /// initialized: [`Self::load_integer_bits`] with one volatile event. The
    /// read of a volatile bit-field demands the field's bits of its unit, and
    /// the unit read of a volatile bit-field store demands none.
    pub fn volatile_load_integer_bits(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        demanded: u128,
    ) -> Result<(u128, WorldState), MemoryFault> {
        let value = self.load_integer_bits(address, access, demanded)?;
        let observed = self.observed_integer(address, access)?;
        let next_world =
            self.append_world(world, VolatileDirection::Load, address, access, observed)?;
        Ok((value, next_world))
    }

    /// A volatile store that initializes only the bits that `initializing`
    /// selects: [`Self::store_integer_bits`] with one volatile event.
    pub fn volatile_store_integer_bits(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        value: u128,
        initializing: u128,
    ) -> Result<(Self, WorldState), MemoryFault> {
        let next_memory = self.store_integer_bits(address, access, value, initializing)?;
        let observed = next_memory.observed_integer(address, access)?;
        let next_world =
            next_memory.append_world(world, VolatileDirection::Store, address, access, observed)?;
        Ok((next_memory, next_world))
    }

    /// The event value of a masked volatile access: the bits that the access
    /// transfers, and, when a byte of them is not completely initialized,
    /// the initializedness of each byte, as a volatile copy records it.
    fn observed_integer(
        &self,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<ObservedValue, MemoryFault> {
        let resolved = self.resolve_pointer(address)?;
        let range = self.checked_range(resolved, access)?;
        let object = self
            .objects
            .get(&resolved.object)
            .ok_or(MemoryFault::UnknownObject {
                object: resolved.object,
            })?;
        let bytes = clone_slice(&object.bytes[range.clone()], access.size_bytes)?;
        let initialized = complete_bytes(&object.initialized[range]);
        Ok(if initialized.iter().all(|byte| *byte) {
            ObservedValue::RawTargetBits(bytes)
        } else {
            ObservedValue::Representation { bytes, initialized }
        })
    }

    pub fn volatile_load_pointer(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<(Pointer, WorldState), MemoryFault> {
        let (value, next_world) = self.volatile_load_pointer_value(world, address, access)?;
        match value {
            PointerValue::Null => Ok((Pointer::NULL, next_world)),
            PointerValue::Object(pointer) => Ok((pointer, next_world)),
            PointerValue::Exposed(value) => Ok((Pointer::from_address(value), next_world)),
            PointerValue::Function(_) => {
                Err(MemoryFault::InvalidPointerRepresentation { address, access })
            }
        }
    }

    pub fn volatile_store_pointer(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        value: Pointer,
    ) -> Result<(Self, WorldState), MemoryFault> {
        self.volatile_store_pointer_value(world, address, access, PointerValue::object(value))
    }

    pub fn volatile_load_pointer_value(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<(PointerValue, WorldState), MemoryFault> {
        self.require_pointer_layout(access)?;
        let representation = self.load_bytes(address, access)?;
        let pointer = self.decode_pointer_value(address, access, &representation)?;
        let observed = ObservedValue::Pointer(self.observe_pointer_value(pointer)?);
        let next_world =
            self.append_world(world, VolatileDirection::Load, address, access, observed)?;
        Ok((pointer, next_world))
    }

    pub fn volatile_store_pointer_value(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        value: PointerValue,
    ) -> Result<(Self, WorldState), MemoryFault> {
        self.require_pointer_layout(access)?;
        self.checked_store_range(address, access)?;
        self.check_storable_pointer_value(value)?;
        let representation = self.encode_pointer_value(value)?;
        let next_memory = self.store_bytes(address, access, &representation)?;
        let observed = ObservedValue::Pointer(self.observe_pointer_value(value)?);
        let next_world =
            self.append_world(world, VolatileDirection::Store, address, access, observed)?;
        Ok((next_memory, next_world))
    }

    fn allocate_object(
        &self,
        origin: ObjectOrigin,
        layout: AccessLayout,
        owner: ObjectOwner,
        contents: AllocationContents<'_>,
    ) -> Result<(Self, Pointer), MemoryFault> {
        validate_object_layout(layout)?;
        if self
            .objects
            .values()
            .any(|object| object.info.origin == origin)
        {
            return Err(MemoryFault::OriginAlreadyAllocated { origin });
        }
        let limits = self.config.limits;
        if layout.size_bytes > limits.max_object_bytes {
            return Err(MemoryFault::ObjectTooLarge {
                requested: layout.size_bytes,
                max: limits.max_object_bytes,
            });
        }
        if u64::try_from(self.objects.len()).map_or(true, |count| count >= limits.max_objects) {
            return Err(MemoryFault::ObjectLimitExceeded {
                max_objects: limits.max_objects,
            });
        }
        let total_bytes = self.total_bytes.checked_add(layout.size_bytes).ok_or(
            MemoryFault::TotalBytesLimitExceeded {
                current: self.total_bytes,
                requested: layout.size_bytes,
                max: limits.max_total_bytes,
            },
        )?;
        if total_bytes > limits.max_total_bytes {
            return Err(MemoryFault::TotalBytesLimitExceeded {
                current: self.total_bytes,
                requested: layout.size_bytes,
                max: limits.max_total_bytes,
            });
        }
        let length =
            usize::try_from(layout.size_bytes).map_err(|_| MemoryFault::HostSizeExceeded {
                size_bytes: layout.size_bytes,
            })?;
        let (bytes, initialized, pointer_fragments) = match contents {
            AllocationContents::InitialState(initial) => (
                repeat_value(length, 0_u8, layout.size_bytes)?,
                repeat_value(
                    length,
                    if initial == InitialState::Zeroed {
                        INITIALIZED_BYTE
                    } else {
                        0
                    },
                    layout.size_bytes,
                )?,
                repeat_value(
                    length,
                    (initial == InitialState::Zeroed)
                        .then_some(PointerFragment::STATIC_INITIALIZER),
                    layout.size_bytes,
                )?,
            ),
            AllocationContents::Bytes(value) => {
                let actual =
                    u64::try_from(value.len()).map_err(|_| MemoryFault::HostSizeExceeded {
                        size_bytes: layout.size_bytes,
                    })?;
                if actual != layout.size_bytes {
                    return Err(MemoryFault::ByteLengthMismatch {
                        expected: layout.size_bytes,
                        actual,
                    });
                }
                (
                    clone_slice(&value.bytes, layout.size_bytes)?,
                    repeat_value(length, INITIALIZED_BYTE, layout.size_bytes)?,
                    repeat_value(length, None, layout.size_bytes)?,
                )
            }
        };

        let raw = self.next_object_id.ok_or(MemoryFault::ObjectIdExhausted)?;
        let id = ObjectId(raw);
        let alignment = u64::from(layout.align_bytes.max(8));
        // Use semantic allocation origins, not local allocation order, so
        // equivalent executions retain identical target pointer bytes.
        // Mix the two coordinates separately. XORing the raw site with a
        // rotated instance made (site, 1) alias (site ^ (1 << 32), 0),
        // precisely the shape of temporaries in two different functions.
        let mut hash = origin.site;
        if let Some(thread) = origin.thread {
            hash = hash.wrapping_add(thread.0.wrapping_add(1).wrapping_mul(0xd6e8_feb8_6659_fd93));
        }
        hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        hash =
            (hash ^ (hash >> 31)).wrapping_add(origin.instance.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        hash ^= hash >> 31;
        let address = ((hash & ((1_u64 << 62) - 1)) & !(alignment - 1)).max(alignment);
        let end_address = address
            .checked_add(layout.size_bytes.max(1))
            .filter(|value| *value < (1_u64 << 63))
            .ok_or(MemoryFault::ObjectIdExhausted)?;
        if self.objects.values().any(|record| {
            let end = record
                .address
                .saturating_add(record.bytes.len().max(1) as u64);
            address < end && record.address < end_address
        }) {
            return Err(MemoryFault::ObjectIdExhausted);
        }
        let mut next = self.clone();
        next.next_object_id = raw.checked_add(1);
        next.total_bytes = total_bytes;
        next.objects.insert(
            id,
            ObjectRecord {
                address,
                info: ObjectInfo {
                    id,
                    origin,
                    layout,
                    owner,
                    lifetime: ObjectLifetime::Alive,
                },
                bytes,
                initialized,
                pointer_fragments,
            },
        );
        Ok((next, Pointer::base(id)))
    }

    fn checked_range(
        &self,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<Range<usize>, MemoryFault> {
        let address = self.resolve_pointer(address)?;
        self.check_access(address, access)?;
        let start =
            usize::try_from(address.byte_offset).map_err(|_| MemoryFault::HostSizeExceeded {
                size_bytes: access.size_bytes,
            })?;
        let length =
            usize::try_from(access.size_bytes).map_err(|_| MemoryFault::HostSizeExceeded {
                size_bytes: access.size_bytes,
            })?;
        let end = start
            .checked_add(length)
            .ok_or(MemoryFault::HostSizeExceeded {
                size_bytes: access.size_bytes,
            })?;
        Ok(start..end)
    }

    fn checked_store_range(
        &self,
        address: Pointer,
        access: AccessLayout,
    ) -> Result<Range<usize>, MemoryFault> {
        let address = self.resolve_pointer(address)?;
        let range = self.checked_range(address, access)?;
        let object = self
            .objects
            .get(&address.object)
            .ok_or(MemoryFault::UnknownObject {
                object: address.object,
            })?;
        if self.frozen_objects.contains(&address.object)
            || matches!(
                object.info.owner,
                ObjectOwner::StringLiteral | ObjectOwner::ReadOnlyStatic
            )
        {
            return Err(MemoryFault::ReadOnlyObject {
                object: address.object,
            });
        }
        Ok(range)
    }

    fn require_pointer_layout(&self, access: AccessLayout) -> Result<(), MemoryFault> {
        // The target fixes representation width, not the alignment of every
        // pointer field. Packed accesses carry their actual checked alignment.
        if access.size_bytes != self.config.pointer_layout.size_bytes
            || !is_valid_alignment(access.align_bytes)
        {
            return Err(MemoryFault::InvalidPointerLayout { layout: access });
        }
        Ok(())
    }

    fn check_storable_pointer(&self, pointer: Pointer) -> Result<(), MemoryFault> {
        if pointer.exposed_address.is_some() {
            return Ok(());
        }
        if pointer.object.is_null() {
            if pointer.byte_offset == 0 {
                return Ok(());
            }
            return Err(MemoryFault::InvalidNullPointer {
                byte_offset: pointer.byte_offset,
            });
        }
        if !self.objects.contains_key(&pointer.object) {
            return Err(MemoryFault::UnknownObject {
                object: pointer.object,
            });
        }
        Ok(())
    }

    fn check_storable_pointer_value(&self, pointer: PointerValue) -> Result<(), MemoryFault> {
        match pointer {
            PointerValue::Null => Ok(()),
            PointerValue::Exposed(_) => Ok(()),
            PointerValue::Object(pointer) => self.check_storable_pointer(pointer),
            PointerValue::Function(identity) if identity != 0 => Ok(()),
            PointerValue::Function(identity) => {
                Err(MemoryFault::InvalidFunctionIdentity { identity })
            }
        }
    }

    fn encode_pointer_value(&self, pointer: PointerValue) -> Result<ByteSequence, MemoryFault> {
        let abstract_bits = u128::from(self.pointer_address(pointer)?);
        let mut representation = encode_integer(
            self.config.byte_order,
            self.config.pointer_layout.size_bytes,
            abstract_bits,
        )?;
        for (index, fragment) in representation.pointer_fragments.iter_mut().enumerate() {
            let byte_index = u64::try_from(index).map_err(|_| MemoryFault::HostSizeExceeded {
                size_bytes: self.config.pointer_layout.size_bytes,
            })?;
            *fragment = Some(PointerFragment {
                pointer,
                slot_size: self.config.pointer_layout.size_bytes,
                byte_index,
            });
        }
        Ok(representation)
    }

    fn decode_pointer_value(
        &self,
        address: Pointer,
        access: AccessLayout,
        representation: &ByteSequence,
    ) -> Result<PointerValue, MemoryFault> {
        if !representation.pointer_fragments.is_empty()
            && representation
                .pointer_fragments
                .iter()
                .all(|part| *part == Some(PointerFragment::STATIC_INITIALIZER))
        {
            let bits = decode_integer(self.config.byte_order, &representation.bytes)?;
            let bits = u64::try_from(bits)
                .map_err(|_| MemoryFault::InvalidPointerRepresentation { address, access })?;
            return Ok(if bits == 0 {
                PointerValue::Null
            } else {
                PointerValue::Exposed(bits)
            });
        }
        let first = representation
            .pointer_fragments
            .first()
            .and_then(|fragment| *fragment)
            .ok_or(MemoryFault::InvalidPointerRepresentation { address, access })?;
        for (index, fragment) in representation.pointer_fragments.iter().enumerate() {
            let expected_index =
                u64::try_from(index).map_err(|_| MemoryFault::HostSizeExceeded {
                    size_bytes: access.size_bytes,
                })?;
            match fragment {
                Some(part)
                    if part.pointer == first.pointer
                        && part.slot_size == access.size_bytes
                        && part.byte_index == expected_index => {}
                _ => {
                    return Err(MemoryFault::InvalidPointerRepresentation { address, access });
                }
            }
        }
        self.check_storable_pointer_value(first.pointer)?;
        Ok(first.pointer)
    }

    fn observe_pointer_value(&self, pointer: PointerValue) -> Result<ObservedPointer, MemoryFault> {
        match pointer {
            PointerValue::Null => Ok(ObservedPointer::Null),
            PointerValue::Exposed(address) => Ok(ObservedPointer::ExposedAddress(address)),
            PointerValue::Function(identity) if identity != 0 => {
                Ok(ObservedPointer::Function { identity })
            }
            PointerValue::Function(identity) => {
                Err(MemoryFault::InvalidFunctionIdentity { identity })
            }
            PointerValue::Object(pointer) => {
                if let Some(address) = pointer.exposed_address {
                    return Ok(ObservedPointer::ExposedAddress(address));
                }
                let object =
                    self.objects
                        .get(&pointer.object)
                        .ok_or(MemoryFault::UnknownObject {
                            object: pointer.object,
                        })?;
                Ok(ObservedPointer::Object {
                    origin: object.info.origin,
                    byte_offset: pointer.byte_offset,
                })
            }
        }
    }

    fn append_world(
        &self,
        world: &WorldState,
        direction: VolatileDirection,
        address: Pointer,
        access: AccessLayout,
        value: ObservedValue,
    ) -> Result<WorldState, MemoryFault> {
        let address = self.resolve_pointer(address)?;
        let object = self
            .objects
            .get(&address.object)
            .ok_or(MemoryFault::UnknownObject {
                object: address.object,
            })?;
        world.append(
            direction,
            object.info.origin,
            address.byte_offset,
            access,
            value,
        )
    }
}

const fn is_valid_alignment(align_bytes: u32) -> bool {
    align_bytes != 0 && align_bytes.is_power_of_two()
}

fn validate_access_layout(layout: AccessLayout) -> Result<(), MemoryFault> {
    if layout.size_bytes == 0 {
        return Err(MemoryFault::InvalidAccessSize { size_bytes: 0 });
    }
    validate_object_layout(layout)
}

fn validate_object_layout(layout: AccessLayout) -> Result<(), MemoryFault> {
    if layout.size_bytes > i64::MAX as u64 {
        return Err(MemoryFault::InvalidAccessSize {
            size_bytes: layout.size_bytes,
        });
    }
    if !is_valid_alignment(layout.align_bytes) {
        return Err(MemoryFault::InvalidAlignment {
            align_bytes: layout.align_bytes,
        });
    }
    Ok(())
}

fn validate_integer_width(size_bytes: u64) -> Result<(), MemoryFault> {
    if size_bytes == 0 || size_bytes > 16 {
        return Err(MemoryFault::IntegerWidthUnsupported { size_bytes });
    }
    Ok(())
}

/// Whether each byte of `masks` has all eight of its bits initialized.
fn complete_bytes(masks: &[u8]) -> Vec<bool> {
    masks.iter().map(|bits| *bits == INITIALIZED_BYTE).collect()
}

fn repeat_value<T: Clone>(length: usize, value: T, size_bytes: u64) -> Result<Vec<T>, MemoryFault> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| MemoryFault::HostAllocationFailed { size_bytes })?;
    values.resize(length, value);
    Ok(values)
}

fn clone_slice<T: Clone>(values: &[T], size_bytes: u64) -> Result<Vec<T>, MemoryFault> {
    let mut cloned = Vec::new();
    cloned
        .try_reserve_exact(values.len())
        .map_err(|_| MemoryFault::HostAllocationFailed { size_bytes })?;
    cloned.extend_from_slice(values);
    Ok(cloned)
}

fn encode_integer(
    byte_order: ByteOrder,
    size_bytes: u64,
    value: u128,
) -> Result<ByteSequence, MemoryFault> {
    validate_integer_width(size_bytes)?;
    let width =
        usize::try_from(size_bytes).map_err(|_| MemoryFault::HostSizeExceeded { size_bytes })?;
    let bytes = match byte_order {
        ByteOrder::Little => clone_slice(&value.to_le_bytes()[..width], size_bytes)?,
        ByteOrder::Big => clone_slice(&value.to_be_bytes()[16 - width..], size_bytes)?,
    };
    ByteSequence::try_from_bytes(bytes)
}

fn decode_integer(byte_order: ByteOrder, bytes: &[u8]) -> Result<u128, MemoryFault> {
    let size_bytes =
        u64::try_from(bytes.len()).map_err(|_| MemoryFault::IntegerWidthUnsupported {
            size_bytes: u64::MAX,
        })?;
    validate_integer_width(size_bytes)?;
    let mut representation = [0_u8; 16];
    let value = match byte_order {
        ByteOrder::Little => {
            representation[..bytes.len()].copy_from_slice(bytes);
            u128::from_le_bytes(representation)
        }
        ByteOrder::Big => {
            representation[16 - bytes.len()..].copy_from_slice(bytes);
            u128::from_be_bytes(representation)
        }
    };
    Ok(value)
}

/// Immutable snapshot of externally ordered volatile events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldState {
    limits: WorldLimits,
    next_sequence: Option<u64>,
    volatile_trace: Vec<VolatileEvent>,
    atomic_trace: Vec<AtomicEvent>,
    fp_environment: evocc_types::floating::FpEnvironment,
}

impl WorldState {
    #[must_use]
    pub fn new(limits: WorldLimits) -> Self {
        Self {
            limits,
            next_sequence: Some(0),
            volatile_trace: Vec::new(),
            atomic_trace: Vec::new(),
            fp_environment: evocc_types::floating::FpEnvironment::default(),
        }
    }

    #[must_use]
    pub fn limits(&self) -> WorldLimits {
        self.limits
    }

    #[must_use]
    pub fn volatile_trace(&self) -> &[VolatileEvent] {
        &self.volatile_trace
    }

    pub fn atomic_trace(&self) -> &[AtomicEvent] {
        &self.atomic_trace
    }

    pub fn fp_environment(&self) -> evocc_types::floating::FpEnvironment {
        self.fp_environment
    }

    /// A new world snapshot, not a mutation of a process-global environment.
    /// FP effects append no memory/volatile/atomic event.
    pub fn with_fp_environment(&self, environment: evocc_types::floating::FpEnvironment) -> Self {
        Self {
            fp_environment: environment,
            ..self.clone()
        }
    }

    fn append(
        &self,
        direction: VolatileDirection,
        origin: ObjectOrigin,
        byte_offset: i64,
        access: AccessLayout,
        value: ObservedValue,
    ) -> Result<Self, MemoryFault> {
        if u64::try_from(
            self.volatile_trace
                .len()
                .saturating_add(self.atomic_trace.len()),
        )
        .map_or(true, |count| count >= self.limits.max_volatile_events)
        {
            return Err(MemoryFault::VolatileEventLimitExceeded {
                max_events: self.limits.max_volatile_events,
            });
        }
        let sequence = self
            .next_sequence
            .ok_or(MemoryFault::VolatileSequenceExhausted)?;
        let mut next = self.clone();
        next.volatile_trace
            .try_reserve(1)
            .map_err(|_| MemoryFault::HostAllocationFailed { size_bytes: 1 })?;
        next.volatile_trace.push(VolatileEvent {
            sequence,
            direction,
            origin,
            byte_offset,
            access,
            value,
        });
        next.next_sequence = sequence.checked_add(1);
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    const BYTE: AccessLayout = AccessLayout::new(1, 1);
    const U16: AccessLayout = AccessLayout::new(2, 2);
    const U32: AccessLayout = AccessLayout::new(4, 4);

    #[test]
    fn volatile_copy_transports_unknown_bytes_and_reports_one_requested_access() {
        let memory = little_memory();
        let (memory, source) = memory
            .allocate_static(0, U32, InitialState::Uninitialized)
            .unwrap();
        let (memory, destination) = memory
            .allocate_static(1, U32, InitialState::Uninitialized)
            .unwrap();
        let memory = memory.store_integer(source, BYTE, 7).unwrap();
        let world = WorldState::new(WorldLimits {
            max_volatile_events: 10,
        });
        let (copied, world) = memory
            .copy_with_volatile(&world, destination, source, U32, CopyVolatility::Read)
            .unwrap();
        assert_eq!(copied.load_integer(destination, BYTE).unwrap(), 7);
        assert!(matches!(
            copied.load_integer(destination, U32),
            Err(MemoryFault::UninitializedRead { .. })
        ));
        let [event] = world.volatile_trace() else {
            panic!("exactly one source read")
        };
        assert_eq!(event.direction, VolatileDirection::Load);
        assert_eq!(event.access, U32);
        let ObservedValue::Representation { initialized, .. } = &event.value else {
            panic!("representation trace retains initializedness")
        };
        assert_eq!(initialized, &[true, false, false, false]);
        let (_, world) = copied
            .copy_with_volatile(
                &world,
                destination,
                destination,
                U32,
                CopyVolatility::ReadWrite,
            )
            .unwrap();
        assert_eq!(
            world.volatile_trace().len(),
            3,
            "self-copy must not erase volatile events"
        );
    }
    const PTR: AccessLayout = AccessLayout::new(8, 8);

    fn limits() -> MemoryLimits {
        MemoryLimits {
            max_objects: 64,
            max_object_bytes: 1 << 20,
            max_total_bytes: 1 << 22,
        }
    }

    fn memory_with(order: ByteOrder) -> MemoryState {
        let config = MemoryConfig::new(order, PTR, limits()).expect("valid memory config");
        MemoryState::new(config).expect("valid empty memory")
    }

    fn little_memory() -> MemoryState {
        let target = TargetDataLayout::aarch64_unknown_linux_gnu();
        let config = MemoryConfig::for_target(&target, limits()).expect("valid target config");
        assert_eq!(config.pointer_layout(), PTR);
        MemoryState::new(config).expect("valid empty memory")
    }

    fn assert_fault<T: std::fmt::Debug>(
        result: Result<T, MemoryFault>,
        expected: MemoryFaultKind,
    ) -> MemoryFault {
        let fault = result.expect_err("operation must fail");
        assert_eq!(fault.kind(), expected, "wrong fault: {fault:?}");
        fault
    }

    #[test]
    fn argument_lists_copy_whole_cursors_but_refuse_partial_representation_access() {
        let layout = AccessLayout::new(32, 8);
        let (memory, first) = little_memory()
            .allocate_static(1, layout, InitialState::Zeroed)
            .unwrap();
        let (memory, second) = memory
            .allocate_static(2, layout, InitialState::Zeroed)
            .unwrap();
        let mut lists = ArgumentLists::default();
        let initial = lists.start(vec![7, 9]);
        let memory = memory.with_argument_list(first, Some(initial)).unwrap();
        let copied = memory.copy_bytes(second, first, layout).unwrap();
        let advanced = lists.advance(initial).unwrap();
        let changed = copied.with_argument_list(first, Some(advanced)).unwrap();
        assert_eq!(
            *lists.read(changed.argument_list(first).unwrap()).unwrap(),
            9
        );
        assert_eq!(
            *lists.read(changed.argument_list(second).unwrap()).unwrap(),
            7
        );
        assert_eq!(copied.argument_list(first).unwrap(), initial);
        assert_fault(
            changed.load_integer(first, U32),
            MemoryFaultKind::InvalidArgumentList,
        );
        assert_fault(
            changed.store_integer(first, U32, 0),
            MemoryFaultKind::InvalidArgumentList,
        );
        assert_fault(
            changed.copy_bytes(second, first, U32),
            MemoryFaultKind::InvalidArgumentList,
        );
        let ended = changed.with_argument_list(first, None).unwrap();
        assert_fault(
            ended.argument_list(first),
            MemoryFaultKind::InvalidArgumentList,
        );
        assert_eq!(ended.argument_list(second).unwrap(), initial);
    }

    #[test]
    fn argument_list_ranges_follow_target_layout_and_do_not_overlap_neighbors() {
        for target in [
            TargetDataLayout::aarch64_apple_darwin(),
            TargetDataLayout::aarch64_unknown_linux_gnu(),
        ] {
            let config = MemoryConfig::for_target(&target, limits()).unwrap();
            let layout = AccessLayout::from_type_layout(target.va_list_layout());
            let (memory, storage) = MemoryState::new(config)
                .unwrap()
                .allocate_static(
                    1,
                    AccessLayout::new(16 + 2 * layout.size_bytes, 8),
                    InitialState::Zeroed,
                )
                .unwrap();
            let first = storage.checked_offset(8).unwrap();
            let second = first.checked_offset(layout.size_bytes as i64).unwrap();
            let after = second.checked_offset(layout.size_bytes as i64).unwrap();
            let memory = memory.with_argument_list(first, Some(3)).unwrap();
            let memory = memory.copy_bytes(second, first, layout).unwrap();
            let memory = memory.with_argument_list(first, Some(4)).unwrap();
            let memory = memory
                .store_integer(storage, PTR, 23)
                .unwrap()
                .store_integer(after, PTR, 29)
                .unwrap();
            assert_eq!(memory.argument_list(first).unwrap(), 4);
            assert_eq!(memory.argument_list(second).unwrap(), 3);
            assert_eq!(memory.load_integer(after, PTR).unwrap(), 29);
            for address in [first, first.checked_offset(4).unwrap(), second] {
                assert_fault(
                    memory.load_integer(address, U32),
                    MemoryFaultKind::InvalidArgumentList,
                );
                assert_fault(
                    memory.store_integer(address, U32, 0),
                    MemoryFaultKind::InvalidArgumentList,
                );
                assert_fault(
                    memory.copy_bytes(storage, address, U32),
                    MemoryFaultKind::InvalidArgumentList,
                );
                assert_fault(
                    memory.copy_bytes(address, storage, U32),
                    MemoryFaultKind::InvalidArgumentList,
                );
            }
            let ended = memory.with_argument_list(first, None).unwrap();
            assert_fault(
                ended.argument_list(first),
                MemoryFaultKind::InvalidArgumentList,
            );
            assert_eq!(ended.argument_list(second).unwrap(), 3);
            let (memory, small) = memory
                .allocate_static(
                    2,
                    AccessLayout::new(layout.size_bytes - 1, 8),
                    InitialState::Zeroed,
                )
                .unwrap();
            assert_fault(
                memory.with_argument_list(small, Some(0)),
                MemoryFaultKind::OutOfBounds,
            );
        }
    }

    #[test]
    fn null_is_reserved_and_object_ids_are_monotonic_and_never_reused() {
        assert!(ObjectId::NULL.is_null());
        assert_eq!(Pointer::NULL.object(), ObjectId::NULL);
        let memory = little_memory();
        let (memory, frame, scope) = memory.begin_frame().unwrap();
        let (memory, first) = memory.allocate_automatic(frame, scope, 10, BYTE).unwrap();
        assert!(!first.object().is_null());
        let memory = memory.end_frame(frame).unwrap();
        let (_, second) = memory
            .allocate_static(20, BYTE, InitialState::Zeroed)
            .unwrap();
        assert!(second.object().raw() > first.object().raw());
    }

    #[test]
    fn allocation_site_bits_do_not_cancel_dynamic_instance_bits() {
        let memory = little_memory();
        let (memory, frame, scope) = memory.begin_frame().unwrap();
        let site = 1_u64 << 63;
        let (memory, first) = memory.allocate_automatic(frame, scope, site, BYTE).unwrap();
        let (memory, second) = memory.allocate_automatic(frame, scope, site, BYTE).unwrap();
        let (_, other) = memory
            .allocate_automatic(frame, scope, site | (1_u64 << 32), BYTE)
            .unwrap();
        assert_ne!(first.object(), second.object());
        assert_ne!(second.object(), other.object());
    }

    #[test]
    fn object_origins_use_site_local_dynamic_instances() {
        let memory = little_memory();
        let (memory, static_object) = memory
            .allocate_static(10, BYTE, InitialState::Zeroed)
            .unwrap();
        let literal_bytes = ByteSequence::try_from_bytes(vec![0]).unwrap();
        let (memory, literal) = memory
            .allocate_string_literal(20, BYTE, &literal_bytes)
            .unwrap();
        let (memory, frame, scope) = memory.begin_frame().unwrap();
        let (memory, first_at_site) = memory.allocate_automatic(frame, scope, 30, BYTE).unwrap();
        let (memory, unrelated_site) = memory.allocate_automatic(frame, scope, 40, BYTE).unwrap();
        let (memory, second_at_site) = memory.allocate_automatic(frame, scope, 30, BYTE).unwrap();
        assert_eq!(
            memory.object_info(static_object.object()).unwrap().origin,
            ObjectOrigin {
                thread: None,
                site: 10,
                instance: 0
            }
        );
        assert_eq!(
            memory.object_info(literal.object()).unwrap().origin,
            ObjectOrigin {
                thread: None,
                site: 20,
                instance: 0
            }
        );
        assert_eq!(
            memory.object_info(first_at_site.object()).unwrap().origin,
            ObjectOrigin {
                thread: None,
                site: 30,
                instance: 0
            }
        );
        assert_eq!(
            memory.object_info(unrelated_site.object()).unwrap().origin,
            ObjectOrigin {
                thread: None,
                site: 40,
                instance: 0
            }
        );
        assert_eq!(
            memory.object_info(second_at_site.object()).unwrap().origin,
            ObjectOrigin {
                thread: None,
                site: 30,
                instance: 1
            }
        );
    }

    #[test]
    fn integer_storage_obeys_explicit_target_byte_order() {
        for (order, expected) in [
            (ByteOrder::Little, vec![0x04, 0x03, 0x02, 0x01]),
            (ByteOrder::Big, vec![0x01, 0x02, 0x03, 0x04]),
        ] {
            let memory = memory_with(order);
            let (memory, address) = memory
                .allocate_static(10, U32, InitialState::Uninitialized)
                .unwrap();
            let memory = memory.store_integer(address, U32, 0x0102_0304).unwrap();
            assert_eq!(memory.load_bytes(address, U32).unwrap().bytes(), expected);
            assert_eq!(memory.load_integer(address, U32).unwrap(), 0x0102_0304);
        }
    }

    #[test]
    fn initialization_is_tracked_per_byte() {
        let memory = little_memory();
        let object = AccessLayout::new(4, 4);
        let (memory, address) = memory
            .allocate_static(10, object, InitialState::Uninitialized)
            .unwrap();
        let middle = address.checked_offset(1).unwrap();
        let two_bytes = AccessLayout::new(2, 1);
        let memory = memory
            .store_bytes(
                middle,
                two_bytes,
                &ByteSequence::try_from_bytes(vec![7, 8]).unwrap(),
            )
            .unwrap();
        assert!(memory.is_range_initialized(middle, two_bytes).unwrap());
        assert!(!memory.is_range_initialized(address, object).unwrap());
        assert_fault(
            memory.load_bytes(address, object),
            MemoryFaultKind::UninitializedRead,
        );
    }

    #[test]
    fn pointer_and_access_classifiers_separate_one_past_range_and_initialization() {
        let memory = little_memory();
        let object = AccessLayout::new(4, 4);
        let (memory, address) = memory
            .allocate_static(10, object, InitialState::Uninitialized)
            .unwrap();
        let one_past = address.checked_offset(4).unwrap();
        assert_eq!(memory.check_pointer(address), Ok(()));
        assert_eq!(memory.check_pointer(one_past), Ok(()));
        assert_eq!(memory.check_access(address, object), Ok(()));
        assert_fault(
            memory.load_bytes(address, object),
            MemoryFaultKind::UninitializedRead,
        );
        assert_fault(
            memory.check_pointer(Pointer::new(address.object(), -1).unwrap()),
            MemoryFaultKind::OutOfBounds,
        );
        assert_fault(
            memory.check_pointer(address.checked_offset(5).unwrap()),
            MemoryFaultKind::OutOfBounds,
        );
        assert_fault(
            memory.check_access(one_past, BYTE),
            MemoryFaultKind::OutOfBounds,
        );
        assert_fault(
            memory.check_access(address, AccessLayout::new(0, 1)),
            MemoryFaultKind::InvalidAccessSize,
        );
        assert_fault(
            memory.check_access(address.checked_offset(1).unwrap(), U16),
            MemoryFaultKind::Misaligned,
        );
        let (memory, weakly_aligned) = memory
            .allocate_static(30, AccessLayout::new(4, 1), InitialState::Zeroed)
            .unwrap();
        assert_fault(
            memory.check_access(weakly_aligned, U32),
            MemoryFaultKind::Misaligned,
        );

        let (memory, frame, scope) = memory.begin_frame().unwrap();
        let (memory, automatic) = memory.allocate_automatic(frame, scope, 20, BYTE).unwrap();
        let memory = memory.end_frame(frame).unwrap();
        assert_fault(
            memory.check_pointer(automatic),
            MemoryFaultKind::LifetimeEnded,
        );
    }

    #[test]
    fn scaled_pointer_offsets_preserve_signedness_direction_and_checked_math() {
        let object = ObjectId::from_raw(7);
        let base = Pointer::new(object, 10).unwrap();
        assert_eq!(
            base.checked_scaled_offset(0xff, 8, IndexSignedness::Signed, OffsetDirection::Add, 4,)
                .unwrap()
                .byte_offset(),
            6
        );
        assert_eq!(
            base.checked_scaled_offset(3, 32, IndexSignedness::Unsigned, OffsetDirection::Add, 4,)
                .unwrap()
                .byte_offset(),
            22
        );
        assert_eq!(
            base.checked_scaled_offset(
                3,
                32,
                IndexSignedness::Unsigned,
                OffsetDirection::Subtract,
                4,
            )
            .unwrap()
            .byte_offset(),
            -2
        );
        assert_eq!(
            base.checked_scaled_offset(
                0xff,
                8,
                IndexSignedness::Signed,
                OffsetDirection::Subtract,
                4,
            )
            .unwrap()
            .byte_offset(),
            14
        );
        assert_fault(
            base.checked_scaled_offset(
                u64::MAX.into(),
                64,
                IndexSignedness::Unsigned,
                OffsetDirection::Add,
                1,
            ),
            MemoryFaultKind::PointerOffsetOverflow,
        );
        assert_fault(
            Pointer::new(object, i64::MAX)
                .unwrap()
                .checked_scaled_offset(1, 8, IndexSignedness::Unsigned, OffsetDirection::Add, 1),
            MemoryFaultKind::PointerOffsetOverflow,
        );
        assert_fault(
            base.checked_scaled_offset(1, 8, IndexSignedness::Unsigned, OffsetDirection::Add, 0),
            MemoryFaultKind::InvalidOffsetScale,
        );
        assert_fault(
            Pointer::NULL.checked_scaled_offset(
                0,
                32,
                IndexSignedness::Signed,
                OffsetDirection::Add,
                1,
            ),
            MemoryFaultKind::NullDereference,
        );
    }

    #[test]
    fn null_oob_negative_one_past_and_misaligned_accesses_are_transactional() {
        let memory = little_memory();
        let (memory, base) = memory
            .allocate_static(10, AccessLayout::new(8, 8), InitialState::Zeroed)
            .unwrap();
        let before = memory.clone();
        assert_fault(
            memory.store_integer(Pointer::NULL, BYTE, 1),
            MemoryFaultKind::NullDereference,
        );
        assert_fault(
            memory.store_integer(Pointer::new(base.object(), -1).unwrap(), BYTE, 1),
            MemoryFaultKind::OutOfBounds,
        );
        assert_fault(
            memory.store_integer(base.checked_offset(8).unwrap(), BYTE, 1),
            MemoryFaultKind::OutOfBounds,
        );
        assert_fault(
            memory.store_integer(base.checked_offset(4).unwrap(), PTR, 1),
            MemoryFaultKind::OutOfBounds,
        );
        assert_fault(
            memory.store_integer(base.checked_offset(1).unwrap(), U32, 1),
            MemoryFaultKind::Misaligned,
        );
        assert_eq!(memory, before);
    }

    #[test]
    fn pointer_store_classifies_destination_before_value_provenance() {
        let memory = little_memory();
        let bytes = ByteSequence::try_from_bytes(vec![0; 8]).unwrap();
        let (memory, literal) = memory.allocate_string_literal(10, PTR, &bytes).unwrap();
        let forged = Pointer::base(ObjectId::from_raw(9_999));
        assert_fault(
            memory.store_pointer(Pointer::NULL, PTR, forged),
            MemoryFaultKind::NullDereference,
        );
        assert_fault(
            memory.store_pointer(literal, PTR, forged),
            MemoryFaultKind::ReadOnlyObject,
        );
    }

    #[test]
    fn aliases_share_an_object_while_distinct_objects_are_isolated() {
        let memory = little_memory();
        let (memory, first) = memory
            .allocate_static(10, U32, InitialState::Zeroed)
            .unwrap();
        let (memory, second) = memory
            .allocate_static(20, U32, InitialState::Zeroed)
            .unwrap();
        let alias = Pointer::new(first.object(), first.byte_offset()).unwrap();
        let memory = memory.store_integer(alias, U32, 9).unwrap();
        assert_eq!(memory.load_integer(first, U32).unwrap(), 9);
        assert_eq!(memory.load_integer(alias, U32).unwrap(), 9);
        assert_eq!(memory.load_integer(second, U32).unwrap(), 0);
    }

    #[test]
    fn static_address_bytes_retain_bits_and_do_not_invent_provenance() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let memory = memory_with(order);
            let (memory, target) = memory
                .allocate_static(10, U32, InitialState::Zeroed)
                .unwrap();
            let (memory, slot) = memory
                .allocate_static(20, PTR, InitialState::Uninitialized)
                .unwrap();
            let bits = memory
                .pointer_address(PointerValue::object(target))
                .unwrap();
            let bytes = match order {
                ByteOrder::Little => bits.to_le_bytes(),
                ByteOrder::Big => bits.to_be_bytes(),
            };
            let bytes = ByteSequence::try_from_static_initializer(bytes.to_vec()).unwrap();
            let memory = memory.store_bytes(slot, PTR, &bytes).unwrap();
            assert_eq!(
                memory.load_pointer_value(slot, PTR).unwrap(),
                PointerValue::Exposed(bits)
            );
            let recovered = memory.load_pointer(slot, PTR).unwrap();
            assert_fault(
                memory.load_integer(recovered, U32),
                MemoryFaultKind::InvalidPointerRepresentation,
            );
            let (exposed, _) = memory.expose_pointer(PointerValue::object(target)).unwrap();
            assert_eq!(exposed.load_integer(recovered, U32).unwrap(), 0);
        }
    }

    #[test]
    fn static_address_byte_copies_preserve_the_marker_but_runtime_writes_do_not() {
        let memory = little_memory();
        let (memory, source) = memory
            .allocate_static(10, PTR, InitialState::Uninitialized)
            .unwrap();
        let (memory, dest) = memory
            .allocate_static(20, PTR, InitialState::Uninitialized)
            .unwrap();
        let bytes =
            ByteSequence::try_from_static_initializer(0x12340_u64.to_le_bytes().to_vec()).unwrap();
        let memory = memory.store_bytes(source, PTR, &bytes).unwrap();
        let memory = memory.copy_bytes(dest, source, PTR).unwrap();
        assert_eq!(
            memory.load_pointer_value(dest, PTR).unwrap(),
            PointerValue::Exposed(0x12340)
        );
        let damaged = memory.store_integer(dest, BYTE, 0x40).unwrap();
        assert_fault(
            damaged.load_pointer_value(dest, PTR),
            MemoryFaultKind::InvalidPointerRepresentation,
        );
        let raw = memory.store_integer(dest, PTR, 0x12340).unwrap();
        assert_fault(
            raw.load_pointer_value(dest, PTR),
            MemoryFaultKind::InvalidPointerRepresentation,
        );
    }

    #[test]
    fn pointer_slots_round_trip_and_preserve_dangling_provenance() {
        let memory = little_memory();
        let (memory, frame, scope) = memory.begin_frame().unwrap();
        let (memory, target) = memory.allocate_automatic(frame, scope, 10, U32).unwrap();
        let (memory, slot) = memory
            .allocate_static(20, PTR, InitialState::Uninitialized)
            .unwrap();
        let memory = memory.store_pointer(slot, PTR, target).unwrap();
        assert_eq!(memory.load_pointer(slot, PTR).unwrap(), target);
        let memory = memory.end_frame(frame).unwrap();
        let dangling = memory.load_pointer(slot, PTR).unwrap();
        assert_eq!(dangling, target);
        assert_fault(
            memory.load_integer(dangling, U32),
            MemoryFaultKind::LifetimeEnded,
        );
    }

    #[test]
    fn pointer_width_and_access_alignment_are_independent() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let memory = memory_with(order);
            let (memory, target) = memory
                .allocate_static(10, U32, InitialState::Zeroed)
                .unwrap();
            let (memory, record) = memory
                .allocate_static(20, AccessLayout::new(10, 1), InitialState::Zeroed)
                .unwrap();
            let slot = record.checked_offset(1).unwrap();
            let packed_pointer = AccessLayout::new(8, 1);
            let next = memory.store_pointer(slot, packed_pointer, target).unwrap();
            assert_eq!(next.load_pointer(slot, packed_pointer).unwrap(), target);
            assert_eq!(next.load_integer(record, BYTE).unwrap(), 0);
            assert_eq!(
                next.load_integer(record.checked_offset(9).unwrap(), BYTE)
                    .unwrap(),
                0
            );
            assert_eq!(
                memory.load_pointer(slot, packed_pointer).unwrap(),
                Pointer::NULL
            );
            assert_fault(next.load_pointer(slot, PTR), MemoryFaultKind::Misaligned);
            assert_fault(
                next.store_pointer(slot, PTR, target),
                MemoryFaultKind::Misaligned,
            );
            for invalid in [
                AccessLayout::new(4, 1),
                AccessLayout::new(8, 0),
                AccessLayout::new(8, 3),
            ] {
                assert_fault(
                    next.load_pointer(slot, invalid),
                    MemoryFaultKind::InvalidPointerLayout,
                );
                assert_fault(
                    next.store_pointer(slot, invalid, target),
                    MemoryFaultKind::InvalidPointerLayout,
                );
            }
        }
    }

    #[test]
    fn integer_provenance_is_resolved_at_access_in_the_current_snapshot() {
        let memory = little_memory();
        let (memory, target) = memory
            .allocate_static(10, U32, InitialState::Zeroed)
            .unwrap();
        let bits = memory
            .pointer_address(PointerValue::object(target))
            .unwrap();
        let recovered = Pointer::from_address(bits);
        assert!(memory.load_integer(recovered, U32).is_err());
        let (exposed, address) = memory.expose_pointer(PointerValue::object(target)).unwrap();
        assert_eq!(bits, address);
        assert_eq!(exposed.load_integer(recovered, U32).unwrap(), 0);
        assert!(
            memory.load_integer(recovered, U32).is_err(),
            "exposure must not mutate an older snapshot"
        );
        let exposed = exposed.store_integer(recovered, U32, 17).unwrap();
        assert_eq!(exposed.load_integer(target, U32).unwrap(), 17);
    }

    #[test]
    fn initialized_zero_pointer_bytes_decode_as_null() {
        let memory = little_memory();
        let (memory, slot) = memory
            .allocate_static(10, PTR, InitialState::Zeroed)
            .unwrap();
        assert_eq!(
            memory.load_pointer_value(slot, PTR).unwrap(),
            PointerValue::Null
        );
    }

    #[test]
    fn full_pointer_sized_byte_copy_preserves_provenance() {
        let memory = little_memory();
        let (memory, target) = memory
            .allocate_static(10, U32, InitialState::Zeroed)
            .unwrap();
        let (memory, source) = memory
            .allocate_static(20, PTR, InitialState::Uninitialized)
            .unwrap();
        let (memory, destination) = memory
            .allocate_static(30, PTR, InitialState::Uninitialized)
            .unwrap();
        let memory = memory.store_pointer(source, PTR, target).unwrap();
        let representation = memory.load_bytes(source, PTR).unwrap();
        let memory = memory
            .store_bytes(destination, PTR, &representation)
            .unwrap();
        assert_eq!(memory.load_pointer(destination, PTR).unwrap(), target);
    }

    #[test]
    fn function_pointer_provenance_round_trips_and_survives_byte_copy() {
        let memory = little_memory();
        let (memory, source) = memory
            .allocate_static(10, PTR, InitialState::Uninitialized)
            .unwrap();
        let (memory, destination) = memory
            .allocate_static(20, PTR, InitialState::Uninitialized)
            .unwrap();
        let function = PointerValue::function(7).unwrap();
        let memory = memory.store_pointer_value(source, PTR, function).unwrap();
        assert_eq!(memory.load_pointer_value(source, PTR).unwrap(), function);

        let bytes = memory.load_bytes(source, PTR).unwrap();
        let memory = memory.store_bytes(destination, PTR, &bytes).unwrap();
        assert_eq!(
            memory.load_pointer_value(destination, PTR).unwrap(),
            function
        );
        assert_fault(
            memory.load_pointer(destination, PTR),
            MemoryFaultKind::InvalidPointerRepresentation,
        );

        let memory = memory.store_integer(destination, BYTE, 0).unwrap();
        assert_fault(
            memory.load_pointer_value(destination, PTR),
            MemoryFaultKind::InvalidPointerRepresentation,
        );
    }

    #[test]
    fn partial_pointer_overwrite_invalidates_reconstruction() {
        let memory = little_memory();
        let (memory, target) = memory
            .allocate_static(10, U32, InitialState::Zeroed)
            .unwrap();
        let (memory, slot) = memory
            .allocate_static(20, PTR, InitialState::Uninitialized)
            .unwrap();
        let memory = memory.store_pointer(slot, PTR, target).unwrap();
        let memory = memory.store_integer(slot, BYTE, 0).unwrap();
        assert_fault(
            memory.load_pointer(slot, PTR),
            MemoryFaultKind::InvalidPointerRepresentation,
        );
    }

    #[test]
    fn pointer_load_reports_uninitialized_before_missing_provenance_shadow() {
        let memory = little_memory();
        let (memory, slot) = memory
            .allocate_static(10, PTR, InitialState::Uninitialized)
            .unwrap();
        assert_fault(
            memory.load_pointer(slot, PTR),
            MemoryFaultKind::UninitializedRead,
        );
        let initialized = memory
            .store_bytes(
                slot,
                PTR,
                &ByteSequence::try_from_bytes(vec![0; 8]).unwrap(),
            )
            .unwrap();
        assert_fault(
            initialized.load_pointer(slot, PTR),
            MemoryFaultKind::InvalidPointerRepresentation,
        );
    }

    #[test]
    fn scope_and_frame_exit_end_exact_automatic_lifetimes() {
        let memory = little_memory();
        let (memory, frame, root) = memory.begin_frame().unwrap();
        let (memory, child) = memory.begin_scope(root).unwrap();
        let (memory, root_object) = memory.allocate_automatic(frame, root, 10, BYTE).unwrap();
        let (memory, child_object) = memory.allocate_automatic(frame, child, 20, BYTE).unwrap();
        let memory = memory.store_integer(root_object, BYTE, 0).unwrap();
        assert_fault(
            memory.end_scope(root),
            MemoryFaultKind::ScopeHasLiveChildren,
        );
        let memory = memory.end_scope(child).unwrap();
        assert_fault(
            memory.load_integer(child_object, BYTE),
            MemoryFaultKind::LifetimeEnded,
        );
        assert_eq!(memory.load_integer(root_object, BYTE).unwrap(), 0);
        let memory = memory.end_frame(frame).unwrap();
        assert_eq!(
            memory.object_info(child_object.object()).unwrap().lifetime,
            ObjectLifetime::Ended(LifetimeEnd::ScopeExit(child))
        );
        assert_eq!(
            memory.object_info(root_object.object()).unwrap().lifetime,
            ObjectLifetime::Ended(LifetimeEnd::FrameExit(frame))
        );
        assert_fault(
            memory.load_integer(root_object, BYTE),
            MemoryFaultKind::LifetimeEnded,
        );
    }

    #[test]
    fn explicit_lifetime_end_accepts_only_a_live_automatic_base_pointer() {
        let memory = little_memory();
        let (memory, static_object) = memory
            .allocate_static(10, BYTE, InitialState::Zeroed)
            .unwrap();
        let literal_bytes = ByteSequence::try_from_bytes(vec![0]).unwrap();
        let (memory, string_literal) = memory
            .allocate_string_literal(11, BYTE, &literal_bytes)
            .unwrap();
        let (memory, frame, scope) = memory.begin_frame().unwrap();
        let (memory, automatic) = memory
            .allocate_automatic(frame, scope, 20, AccessLayout::new(2, 1))
            .unwrap();
        assert_fault(
            memory.load_integer(automatic, BYTE),
            MemoryFaultKind::UninitializedRead,
        );
        let memory = memory.store_integer(automatic, BYTE, 0).unwrap();
        let (memory, foreign_frame, _) = memory.begin_frame().unwrap();
        let invalid_lifetime_diagnostic = Some(MemoryDiagnostic {
            code: "E3018",
            message: "invalid-lifetime-end",
        });
        let fault = assert_fault(
            memory.end_object_lifetime(frame, automatic.checked_offset(1).unwrap()),
            MemoryFaultKind::InvalidLifetimeEnd,
        );
        assert_eq!(fault.runtime_diagnostic(), invalid_lifetime_diagnostic);
        let fault = assert_fault(
            memory.end_object_lifetime(frame, static_object),
            MemoryFaultKind::InvalidLifetimeEnd,
        );
        assert_eq!(fault.runtime_diagnostic(), invalid_lifetime_diagnostic);
        let fault = assert_fault(
            memory.end_object_lifetime(frame, string_literal),
            MemoryFaultKind::InvalidLifetimeEnd,
        );
        assert_eq!(fault.runtime_diagnostic(), invalid_lifetime_diagnostic);
        assert_fault(
            memory.end_object_lifetime(frame, Pointer::NULL),
            MemoryFaultKind::NullDereference,
        );
        assert_fault(
            memory.end_object_lifetime(frame, Pointer::base(ObjectId::from_raw(9_999))),
            MemoryFaultKind::UnknownObject,
        );
        let fault = assert_fault(
            memory.end_object_lifetime(foreign_frame, automatic),
            MemoryFaultKind::InvalidLifetimeEnd,
        );
        assert_eq!(fault.runtime_diagnostic(), invalid_lifetime_diagnostic);
        assert_fault(
            memory.end_object_lifetime(FrameId(9_999), automatic),
            MemoryFaultKind::UnknownFrame,
        );
        let foreign_ended = memory.end_frame(foreign_frame).unwrap();
        assert_fault(
            foreign_ended.end_object_lifetime(foreign_frame, automatic),
            MemoryFaultKind::EndedFrame,
        );
        let ended = memory.end_object_lifetime(frame, automatic).unwrap();
        assert_eq!(memory.load_integer(automatic, BYTE).unwrap(), 0);
        assert_fault(
            ended.load_integer(automatic, BYTE),
            MemoryFaultKind::LifetimeEnded,
        );
        assert_fault(
            ended.end_object_lifetime(frame, automatic),
            MemoryFaultKind::LifetimeEnded,
        );
    }

    #[test]
    fn program_semantic_faults_have_one_exact_shared_runtime_mapping() {
        let object = ObjectId::from_raw(1);
        let address = Pointer::base(object);
        let cases = [
            (
                MemoryFault::InvalidAtomicAccess,
                MemoryDiagnostic {
                    code: "E3062",
                    message: "invalid-atomic-access",
                },
            ),
            (
                MemoryFault::InvalidLifetimeEnd {
                    address,
                    reason: InvalidLifetimeEndReason::NonBasePointer,
                },
                MemoryDiagnostic {
                    code: "E3018",
                    message: "invalid-lifetime-end",
                },
            ),
            (
                MemoryFault::UninitializedRead {
                    address,
                    size_bytes: 1,
                    first_uninitialized: 0,
                },
                MemoryDiagnostic {
                    code: "E3001",
                    message: "read-of-uninitialized-object",
                },
            ),
            (
                MemoryFault::NullDereference { access: None },
                MemoryDiagnostic {
                    code: "E3009",
                    message: "null-pointer-access",
                },
            ),
            (
                MemoryFault::LifetimeEnded {
                    object,
                    ended_by: LifetimeEnd::Explicit,
                },
                MemoryDiagnostic {
                    code: "E3010",
                    message: "dangling-pointer-access",
                },
            ),
            (
                MemoryFault::PointerOffsetOverflow {
                    pointer: address,
                    operand: OffsetOperand::Delta(1),
                },
                MemoryDiagnostic {
                    code: "E3011",
                    message: "out-of-bounds-memory-access",
                },
            ),
            (
                MemoryFault::OutOfBounds {
                    address,
                    access: Some(BYTE),
                    object_size: 0,
                },
                MemoryDiagnostic {
                    code: "E3011",
                    message: "out-of-bounds-memory-access",
                },
            ),
            (
                MemoryFault::Misaligned {
                    address,
                    required_align: 4,
                    object_align: 1,
                },
                MemoryDiagnostic {
                    code: "E3012",
                    message: "misaligned-memory-access",
                },
            ),
            (
                MemoryFault::UnknownObject { object },
                MemoryDiagnostic {
                    code: "E3013",
                    message: "invalid-pointer-representation",
                },
            ),
            (
                MemoryFault::InvalidNullPointer { byte_offset: 1 },
                MemoryDiagnostic {
                    code: "E3013",
                    message: "invalid-pointer-representation",
                },
            ),
            (
                MemoryFault::InvalidPointerRepresentation {
                    address,
                    access: PTR,
                },
                MemoryDiagnostic {
                    code: "E3013",
                    message: "invalid-pointer-representation",
                },
            ),
            (
                MemoryFault::ReadOnlyObject { object },
                MemoryDiagnostic {
                    code: "E3014",
                    message: "write-to-read-only-object",
                },
            ),
        ];
        for (fault, expected) in cases {
            assert_eq!(fault.runtime_diagnostic(), Some(expected), "{fault:?}");
        }
        assert_eq!(
            MemoryFault::InvalidAlignment { align_bytes: 3 }.runtime_diagnostic(),
            None
        );
        assert_eq!(
            MemoryFault::ObjectLimitExceeded { max_objects: 0 }.runtime_diagnostic(),
            None
        );
    }

    #[test]
    fn frame_exit_keeps_static_and_string_literal_objects_alive() {
        let memory = little_memory();
        let (memory, static_object) = memory
            .allocate_static(10, BYTE, InitialState::Zeroed)
            .unwrap();
        let literal_bytes = ByteSequence::try_from_bytes(vec![42]).unwrap();
        let (memory, string_literal) = memory
            .allocate_string_literal(20, BYTE, &literal_bytes)
            .unwrap();
        let (memory, frame, scope) = memory.begin_frame().unwrap();
        let (memory, automatic) = memory.allocate_automatic(frame, scope, 30, BYTE).unwrap();
        let memory = memory.end_frame(frame).unwrap();
        assert_eq!(memory.load_integer(static_object, BYTE).unwrap(), 0);
        assert_eq!(memory.load_integer(string_literal, BYTE).unwrap(), 42);
        assert_fault(
            memory.load_integer(automatic, BYTE),
            MemoryFaultKind::LifetimeEnded,
        );
    }

    #[test]
    fn string_literals_are_initialized_and_read_only() {
        let memory = little_memory();
        let bytes = ByteSequence::try_from_bytes(vec![b'h', b'i', 0]).unwrap();
        let layout = AccessLayout::new(3, 1);
        let (memory, literal) = memory.allocate_string_literal(10, layout, &bytes).unwrap();
        assert_eq!(
            memory.load_bytes(literal, layout).unwrap().bytes(),
            bytes.bytes()
        );
        assert_eq!(
            memory.object_info(literal.object()).unwrap().owner,
            ObjectOwner::StringLiteral
        );
        let before = memory.clone();
        assert_fault(
            memory.store_integer(literal, BYTE, b'H'.into()),
            MemoryFaultKind::ReadOnlyObject,
        );
        assert_eq!(memory, before);
    }

    #[test]
    fn cloned_snapshots_are_functionally_isolated() {
        let memory = little_memory();
        let (base, address) = memory
            .allocate_static(10, U32, InitialState::Zeroed)
            .unwrap();
        let one = base.store_integer(address, U32, 1).unwrap();
        let two = one.store_integer(address, U32, 2).unwrap();
        assert_eq!(base.load_integer(address, U32).unwrap(), 0);
        assert_eq!(one.load_integer(address, U32).unwrap(), 1);
        assert_eq!(two.load_integer(address, U32).unwrap(), 2);
        assert_eq!(one.fork(), one);
    }

    #[test]
    fn volatile_success_appends_exactly_ordered_world_events() {
        let memory = little_memory();
        let world = WorldState::new(WorldLimits {
            max_volatile_events: 8,
        });
        assert!(world.volatile_trace().is_empty());
        let (memory, address) = memory
            .allocate_static(10, U32, InitialState::Zeroed)
            .unwrap();
        let ordinary_world = world.clone();
        let memory = memory.store_integer(address, U32, 7).unwrap();
        assert_eq!(world, ordinary_world);
        let (memory, world) = memory
            .volatile_store_integer(&world, address, U32, 11)
            .unwrap();
        let (loaded, world) = memory.volatile_load_integer(&world, address, U32).unwrap();
        assert_eq!(loaded, 11);
        let (memory, world) = memory
            .volatile_store_integer(&world, address, U32, 22)
            .unwrap();
        let (loaded, world) = memory.volatile_load_integer(&world, address, U32).unwrap();
        assert_eq!(loaded, 22);
        let trace = world.volatile_trace();
        let origin = memory.object_info(address.object()).unwrap().origin;
        assert_eq!(trace.len(), 4);
        assert!(trace
            .iter()
            .all(|event| event.origin == origin && event.byte_offset == 0 && event.access == U32));
        assert_eq!(
            trace
                .iter()
                .map(|event| (event.sequence, event.direction))
                .collect::<Vec<_>>(),
            vec![
                (0, VolatileDirection::Store),
                (1, VolatileDirection::Load),
                (2, VolatileDirection::Store),
                (3, VolatileDirection::Load),
            ]
        );
        assert_eq!(
            trace[0].value,
            ObservedValue::RawTargetBits(vec![11, 0, 0, 0])
        );
        assert_eq!(
            trace[1].value,
            ObservedValue::RawTargetBits(vec![11, 0, 0, 0])
        );
        assert_eq!(
            trace[2].value,
            ObservedValue::RawTargetBits(vec![22, 0, 0, 0])
        );
        assert_eq!(
            trace[3].value,
            ObservedValue::RawTargetBits(vec![22, 0, 0, 0])
        );
    }

    #[test]
    fn pointer_volatile_trace_is_canonical_across_local_object_id_histories() {
        fn trace(with_unrelated_prefix: bool) -> (Vec<VolatileEvent>, Vec<u8>) {
            let memory = little_memory();
            let memory = if with_unrelated_prefix {
                memory
                    .allocate_static(99, BYTE, InitialState::Zeroed)
                    .unwrap()
                    .0
            } else {
                memory
            };
            let (memory, target) = memory
                .allocate_static(10, U32, InitialState::Zeroed)
                .unwrap();
            let (memory, slot) = memory
                .allocate_static(20, PTR, InitialState::Uninitialized)
                .unwrap();
            let world = WorldState::new(WorldLimits {
                max_volatile_events: 2,
            });
            let (memory, world) = memory
                .volatile_store_pointer(&world, slot, PTR, target)
                .unwrap();
            (
                world.volatile_trace().to_vec(),
                memory.load_bytes(slot, PTR).unwrap().bytes().to_vec(),
            )
        }

        let (without_prefix, without_prefix_bits) = trace(false);
        let (with_prefix, with_prefix_bits) = trace(true);
        assert_eq!(without_prefix, with_prefix);
        assert_eq!(without_prefix_bits, with_prefix_bits);
        assert_eq!(
            without_prefix[0].value,
            ObservedValue::Pointer(ObservedPointer::Object {
                origin: ObjectOrigin {
                    thread: None,
                    site: 10,
                    instance: 0,
                },
                byte_offset: 0,
            })
        );
        assert_eq!(
            (without_prefix[0].origin, without_prefix[0].byte_offset),
            (
                ObjectOrigin {
                    thread: None,
                    site: 20,
                    instance: 0,
                },
                0
            )
        );
        assert_eq!(without_prefix[0].access, PTR);
    }

    #[test]
    fn faulting_volatile_access_changes_neither_memory_nor_world() {
        let memory = little_memory();
        let world = WorldState::new(WorldLimits {
            max_volatile_events: 8,
        });
        let (memory, uninitialized) = memory
            .allocate_static(10, U32, InitialState::Uninitialized)
            .unwrap();
        let before_memory = memory.clone();
        let before_world = world.clone();
        assert_fault(
            memory.volatile_load_integer(&world, uninitialized, U32),
            MemoryFaultKind::UninitializedRead,
        );
        assert_fault(
            memory.volatile_store_integer(&world, uninitialized.checked_offset(4).unwrap(), U32, 1),
            MemoryFaultKind::OutOfBounds,
        );
        assert_eq!(memory, before_memory);
        assert_eq!(world, before_world);
    }

    #[test]
    fn limits_and_extreme_offsets_fail_without_panicking_or_partial_state() {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let config = MemoryConfig::new(
                ByteOrder::Little,
                PTR,
                MemoryLimits {
                    max_objects: 1,
                    max_object_bytes: 4,
                    max_total_bytes: 4,
                },
            )
            .unwrap();
            let memory = MemoryState::new(config).unwrap();
            let too_wide = AccessLayout::new(i64::MAX as u64 + 1, 1);
            assert_fault(
                memory.allocate_static(5, too_wide, InitialState::Zeroed),
                MemoryFaultKind::InvalidAccessSize,
            );
            assert_fault(
                memory.allocate_static(10, AccessLayout::new(5, 1), InitialState::Zeroed),
                MemoryFaultKind::ObjectTooLarge,
            );
            let (memory, address) = memory
                .allocate_static(20, U32, InitialState::Zeroed)
                .unwrap();
            assert_fault(
                memory.check_access(address, too_wide),
                MemoryFaultKind::InvalidAccessSize,
            );
            assert_fault(
                memory.allocate_static(30, BYTE, InitialState::Zeroed),
                MemoryFaultKind::ObjectLimitExceeded,
            );
            assert_fault(
                Pointer::new(address.object(), i64::MAX)
                    .unwrap()
                    .checked_offset(1),
                MemoryFaultKind::PointerOffsetOverflow,
            );
            let world = WorldState::new(WorldLimits {
                max_volatile_events: 1,
            });
            let (memory, world) = memory
                .volatile_store_integer(&world, address, U32, 1)
                .unwrap();
            let before_memory = memory.clone();
            let before_world = world.clone();
            assert_fault(
                memory.volatile_load_integer(&world, address, U32),
                MemoryFaultKind::VolatileEventLimitExceeded,
            );
            assert_eq!(memory, before_memory);
            assert_eq!(world, before_world);
        }));
        assert!(
            result.is_ok(),
            "extreme inputs must return faults, not panic"
        );
    }
}
