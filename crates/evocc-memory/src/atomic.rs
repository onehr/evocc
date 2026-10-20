use super::*;

/// Source order is semantic metadata even when the target strengthens it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomicOrder {
    Relaxed,
    Consume,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

impl AtomicOrder {
    pub const fn from_c(value: i128) -> Option<Self> {
        match value {
            0 => Some(Self::Relaxed),
            1 => Some(Self::Consume),
            2 => Some(Self::Acquire),
            3 => Some(Self::Release),
            4 => Some(Self::AcqRel),
            5 => Some(Self::SeqCst),
            _ => None,
        }
    }
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Relaxed => "relaxed",
            Self::Consume => "consume",
            Self::Acquire => "acquire",
            Self::Release => "release",
            Self::AcqRel => "acq_rel",
            Self::SeqCst => "seq_cst",
        }
    }
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        (0..=5)
            .filter_map(Self::from_c)
            .find(|order| order.mnemonic() == text)
    }
    pub const fn valid_for(self, load: bool) -> bool {
        if load {
            matches!(
                self,
                Self::Relaxed | Self::Consume | Self::Acquire | Self::SeqCst
            )
        } else {
            matches!(self, Self::Relaxed | Self::Release | Self::SeqCst)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomicValue {
    Integer(u128),
    Pointer(PointerValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomicRmwOp {
    Add,
    Sub,
    And,
    Or,
    Xor,
    Nand,
    Exchange,
    CompareExchange,
}

impl AtomicRmwOp {
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Nand => "nand",
            Self::Exchange => "exchange",
            Self::CompareExchange => "cmpxchg",
        }
    }
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        [
            Self::Add,
            Self::Sub,
            Self::And,
            Self::Or,
            Self::Xor,
            Self::Nand,
            Self::Exchange,
            Self::CompareExchange,
        ]
        .into_iter()
        .find(|op| op.mnemonic() == text)
    }
    pub const fn integer_result(self, old: u128, value: u128) -> u128 {
        match self {
            Self::Add => old.wrapping_add(value),
            Self::Sub => old.wrapping_sub(value),
            Self::And => old & value,
            Self::Or => old | value,
            Self::Xor => old ^ value,
            Self::Nand => !(old & value),
            Self::Exchange | Self::CompareExchange => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtomicRmwResult {
    pub old: AtomicValue,
    pub new: AtomicValue,
    pub exchanged: bool,
}

/// A single-thread event observation, not a concurrent execution model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicEvent {
    pub sequence: u64,
    pub direction: VolatileDirection,
    pub origin: ObjectOrigin,
    pub byte_offset: i64,
    pub access: AccessLayout,
    pub order: AtomicOrder,
    pub volatile: bool,
    pub value: ObservedValue,
    /// A RMW is one indivisible event, including an unsuccessful comparison.
    pub rmw: Option<AtomicRmwOp>,
    pub previous: Option<ObservedValue>,
}

impl MemoryState {
    #[allow(clippy::too_many_arguments)]
    pub fn atomic_rmw(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        order: AtomicOrder,
        volatile: bool,
        operation: AtomicRmwOp,
        value: AtomicValue,
        expected: Option<AtomicValue>,
    ) -> Result<(Self, WorldState, AtomicRmwResult), MemoryFault> {
        if !matches!(access.size_bytes, 1 | 2 | 4 | 8)
            || u64::from(access.align_bytes) != access.size_bytes
            || (operation == AtomicRmwOp::CompareExchange) != expected.is_some()
        {
            return Err(MemoryFault::InvalidAtomicAccess);
        }
        let old = if matches!(value, AtomicValue::Pointer(_)) {
            AtomicValue::Pointer(self.load_pointer_value(address, access)?)
        } else {
            AtomicValue::Integer(self.load_integer(address, access)?)
        };
        let equal = match (old, expected) {
            (_, None) => true,
            (AtomicValue::Integer(a), Some(AtomicValue::Integer(b))) => a == b,
            (AtomicValue::Pointer(a), Some(AtomicValue::Pointer(b))) => {
                self.pointer_address(a)? == self.pointer_address(b)?
            }
            _ => return Err(MemoryFault::InvalidAtomicAccess),
        };
        let mut memory = self.clone();
        let new = if !equal {
            old
        } else {
            match (old, value) {
                (AtomicValue::Integer(a), AtomicValue::Integer(b)) => AtomicValue::Integer(
                    operation.integer_result(a, b) & (u128::MAX >> (128 - access.size_bytes * 8)),
                ),
                (AtomicValue::Pointer(_), AtomicValue::Pointer(pointer))
                    if matches!(
                        operation,
                        AtomicRmwOp::Exchange | AtomicRmwOp::CompareExchange
                    ) =>
                {
                    AtomicValue::Pointer(pointer)
                }
                _ => return Err(MemoryFault::InvalidAtomicAccess),
            }
        };
        if equal {
            memory = match new {
                AtomicValue::Integer(raw) => memory.store_integer(address, access, raw)?,
                AtomicValue::Pointer(pointer) => {
                    memory.store_pointer_value(address, access, pointer)?
                }
            };
        }
        let mut world = self.atomic_event(world, address, access, order, volatile, !equal, new)?;
        let event = world
            .atomic_trace
            .last_mut()
            .expect("atomic event just appended");
        event.rmw = Some(operation);
        event.previous = Some(match old {
            AtomicValue::Integer(raw) => ObservedValue::RawTargetBits(
                encode_integer(self.config.byte_order, access.size_bytes, raw)?.bytes,
            ),
            AtomicValue::Pointer(pointer) => {
                ObservedValue::Pointer(self.observe_pointer_value(pointer)?)
            }
        });
        Ok((
            memory,
            world,
            AtomicRmwResult {
                old,
                new,
                exchanged: equal,
            },
        ))
    }
    /// GNU `__atomic` add or subtract on an object pointer. The byte count is
    /// unscaled, and the result keeps the old pointer's provenance. As C11
    /// 7.17.7.5p3 says for address types, the result may be an address that
    /// no access can use, but the operation itself is defined.
    #[allow(clippy::too_many_arguments)]
    pub fn atomic_pointer_offset(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        order: AtomicOrder,
        volatile: bool,
        operation: AtomicRmwOp,
        bytes: u64,
    ) -> Result<(Self, WorldState, AtomicRmwResult), MemoryFault> {
        if access.size_bytes != 8
            || access.align_bytes != 8
            || !matches!(operation, AtomicRmwOp::Add | AtomicRmwOp::Sub)
        {
            return Err(MemoryFault::InvalidAtomicAccess);
        }
        let old = self.load_pointer_value(address, access)?;
        let delta = if operation == AtomicRmwOp::Sub {
            bytes.wrapping_neg()
        } else {
            bytes
        };
        let exposed = |raw: u64| {
            if raw == 0 {
                PointerValue::Null
            } else {
                PointerValue::Exposed(raw)
            }
        };
        let new = match old {
            PointerValue::Null => exposed(delta),
            PointerValue::Exposed(raw) => exposed(raw.wrapping_add(delta)),
            PointerValue::Object(pointer) if pointer.exposed_address.is_some() => {
                exposed(self.pointer_address(old)?.wrapping_add(delta))
            }
            PointerValue::Object(pointer) => {
                PointerValue::Object(pointer.checked_offset(delta as i64)?)
            }
            PointerValue::Function(_) => return Err(MemoryFault::InvalidAtomicAccess),
        };
        let memory = self.store_pointer_value(address, access, new)?;
        let mut world = self.atomic_event(
            world,
            address,
            access,
            order,
            volatile,
            false,
            AtomicValue::Pointer(new),
        )?;
        let event = world
            .atomic_trace
            .last_mut()
            .expect("atomic event just appended");
        event.rmw = Some(operation);
        event.previous = Some(ObservedValue::Pointer(self.observe_pointer_value(old)?));
        Ok((
            memory,
            world,
            AtomicRmwResult {
                old: AtomicValue::Pointer(old),
                new: AtomicValue::Pointer(new),
                exchanged: true,
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn atomic_load(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        order: AtomicOrder,
        volatile: bool,
        pointer: bool,
    ) -> Result<(AtomicValue, WorldState), MemoryFault> {
        Self::validate_atomic_access(access, order, true)?;
        let value = if pointer {
            AtomicValue::Pointer(self.load_pointer_value(address, access)?)
        } else {
            AtomicValue::Integer(self.load_integer(address, access)?)
        };
        let world = self.atomic_event(world, address, access, order, volatile, true, value)?;
        Ok((value, world))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn atomic_store(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        order: AtomicOrder,
        volatile: bool,
        value: AtomicValue,
    ) -> Result<(Self, WorldState), MemoryFault> {
        Self::validate_atomic_access(access, order, false)?;
        let memory = match value {
            AtomicValue::Integer(value) => self.store_integer(address, access, value)?,
            AtomicValue::Pointer(value) => self.store_pointer_value(address, access, value)?,
        };
        let world = self.atomic_event(world, address, access, order, volatile, false, value)?;
        Ok((memory, world))
    }

    fn validate_atomic_access(
        access: AccessLayout,
        order: AtomicOrder,
        load: bool,
    ) -> Result<(), MemoryFault> {
        if !order.valid_for(load)
            || !matches!(access.size_bytes, 1 | 2 | 4 | 8)
            || u64::from(access.align_bytes) != access.size_bytes
        {
            return Err(MemoryFault::InvalidAtomicAccess);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn atomic_event(
        &self,
        world: &WorldState,
        address: Pointer,
        access: AccessLayout,
        order: AtomicOrder,
        volatile: bool,
        load: bool,
        value: AtomicValue,
    ) -> Result<WorldState, MemoryFault> {
        let address = self.resolve_pointer(address)?;
        let record = self
            .objects
            .get(&address.object)
            .ok_or(MemoryFault::UnknownObject {
                object: address.object,
            })?;
        let value = match value {
            AtomicValue::Integer(value) => ObservedValue::RawTargetBits(
                encode_integer(self.config.byte_order, access.size_bytes, value)?.bytes,
            ),
            AtomicValue::Pointer(value) => {
                ObservedValue::Pointer(self.observe_pointer_value(value)?)
            }
        };
        if world
            .atomic_trace
            .len()
            .saturating_add(world.volatile_trace.len()) as u64
            >= world.limits.max_volatile_events
        {
            return Err(MemoryFault::VolatileEventLimitExceeded {
                max_events: world.limits.max_volatile_events,
            });
        }
        let sequence = world
            .next_sequence
            .ok_or(MemoryFault::VolatileSequenceExhausted)?;
        let mut next = world.clone();
        next.atomic_trace
            .try_reserve(1)
            .map_err(|_| MemoryFault::HostAllocationFailed { size_bytes: 1 })?;
        next.atomic_trace.push(AtomicEvent {
            rmw: None,
            previous: None,
            sequence,
            direction: if load {
                VolatileDirection::Load
            } else {
                VolatileDirection::Store
            },
            origin: record.info.origin,
            byte_offset: address.byte_offset,
            access,
            order,
            volatile,
            value,
        });
        next.next_sequence = sequence.checked_add(1);
        Ok(next)
    }
}
