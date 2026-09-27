//! The EvoMIR interpreter: executable semantics for every Section 4 form.
//!
//! Section 4 says each row "states the semantics the EvoMIR interpreter
//! (Section 9) implements, so that the table is executable, not descriptive",
//! and Section 9.1 says the interpreter "executes an EvoMIR function on the
//! same inputs as the EvoIR interpreter and the two must agree on results and
//! on the volatile trace".
//!
//! That second sentence fixes the value domain. EvoIR Sections 1.3 and 5.3
//! give a pointer an out-of-band provenance and the volatile trace of Section
//! 4.3 names an object by its `(origin, byte_offset)`, so a register here
//! holds either raw bits or a provenance-carrying pointer, and memory is the
//! same `evocc-memory` object model both interpreters share. A register file
//! of plain 64-bit words could not produce the trace the comparison needs.
//!
//! Sub-width values are modelled exactly as Section 3 describes them and not
//! more kindly: a `bits<N>` with `N` below the register width lives in the low
//! `N` bits with the bits above them whatever the last machine instruction
//! left there. Nothing masks them on the way in or out, so a lowering that
//! reads them produces a different answer from EvoIR and the comparison of
//! Section 9.1 catches it.

use std::collections::HashMap;

use evocc_evoir::{GlobalKind, GlobalObject};
use evocc_memory::{
    AccessLayout, ByteSequence, FrameId, InitialState, MemoryConfig, MemoryFault, MemoryLimits,
    MemoryState, Pointer, PointerValue, ScopeId, WorldLimits, WorldState,
};
use evocc_types::TargetDataLayout;

use crate::{
    BinOp, CarryOp, Cond, FlagOp, LoadKind, MBlock, MFunction, MInst, MModule, MTerm, RegClass,
    StoreKind, SymbolKind, UnaryOp, VReg, WideMul, Width,
};

// The same host-resource policy the EvoIR interpreter runs under, so that a
// comparison between the two never turns on a limit rather than on semantics.
const MAX_MEMORY_OBJECTS: u64 = 65_536;
const MAX_MEMORY_OBJECT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_MEMORY_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_VOLATILE_EVENTS: u64 = 1_048_576;

/// Resource limits for deterministic EvoMIR execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MirLimits {
    pub step_budget: u64,
}

impl MirLimits {
    #[must_use]
    pub const fn new(step_budget: u64) -> Self {
        Self { step_budget }
    }
}

/// One machine value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirValue {
    Int(u64),
    Ptr(Pointer),
    Fn(PointerValue),
    Vector(evocc_memory::ObjectRepresentation),
}

/// The condition-code register's content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flags {
    /// `cmp` of two raw bit patterns at one width.
    Int { lhs: u64, rhs: u64, width: Width },
    /// `adds`/`subs`: the four condition bits the arithmetic set, which is
    /// what makes `vs`/`vc` and `cs`/`cc` readable (Section 4).
    Arithmetic {
        negative: bool,
        zero: bool,
        carry: bool,
        overflow: bool,
    },
    /// `cmp` where at least one operand carries provenance: only equality is
    /// meaningful, exactly as EvoIR Section 3.5 says of a pointer comparison.
    Identity { equal: bool },
}

#[derive(Debug, Clone)]
enum Slot {
    Value(MirValue),
    Flags(Flags),
}

/// Successful EvoMIR execution.
#[derive(Debug, Clone)]
pub struct MirOutcome {
    pub results: Vec<MirValue>,
    pub steps: u64,
    pub memory: MemoryState,
    pub world: WorldState,
}

/// A defined failure of EvoMIR execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirError {
    BadBranchTarget,
    /// A `brk` was reached; the immediate names the EvoIR reason.
    Trap {
        imm: u16,
    },
    /// The step budget was exhausted.
    BudgetExhausted,
    /// A memory fault, carrying the EvoIR diagnostic of Section 6.1.
    Memory {
        code: &'static str,
        detail: String,
    },
    /// A call through a value that names no function.
    BadCallTarget {
        code: &'static str,
    },
    /// The module could not be executed at all.
    Invalid {
        detail: String,
    },
    /// An `asm` was reached. EvoIR Section 3.11: "an interpreter cannot
    /// execute an `asm` and reports `E3021` when it reaches one, so a program
    /// containing one is verified by the native run of the EvoMIR
    /// specification's Section 9 and by nothing else."
    OpaqueAssembly,
}

impl MirError {
    /// The stable diagnostic code, so a trap can be compared with the EvoIR
    /// interpreter's.
    #[must_use]
    pub fn code(&self) -> String {
        match self {
            Self::BadBranchTarget => "E3048".to_owned(),
            Self::Trap { imm } => format!("E{imm}"),
            Self::BudgetExhausted => "E3008".to_owned(),
            Self::Memory { code, .. } | Self::BadCallTarget { code } => (*code).to_owned(),
            Self::Invalid { .. } => "E3007".to_owned(),
            Self::OpaqueAssembly => "E3021".to_owned(),
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::BadBranchTarget => "invalid-indirect-branch-target".to_owned(),
            Self::Trap { imm } => format!("machine-trap-{imm}"),
            Self::BudgetExhausted => "step-budget-exceeded".to_owned(),
            Self::Memory { detail, .. } => detail.clone(),
            Self::BadCallTarget { code } => format!("call-target-{code}"),
            Self::Invalid { detail } => detail.clone(),
            Self::OpaqueAssembly => "inline-assembly-is-not-interpretable".to_owned(),
        }
    }
}

fn memory_error(error: &MemoryFault) -> MirError {
    match error.runtime_diagnostic() {
        Some(diagnostic) => MirError::Memory {
            code: diagnostic.code,
            detail: diagnostic.message.to_owned(),
        },
        None => MirError::Memory {
            code: "E3007",
            detail: format!("interpreter-memory-failed: {error}"),
        },
    }
}

struct State {
    memory: MemoryState,
    world: WorldState,
    globals: HashMap<String, Pointer>,
    identities: HashMap<String, u64>,
    /// The code provenance of every block, keyed by its function's position
    /// and its own identifier, numbered past the function identities so a
    /// block address never equals a function address.
    blocks: HashMap<(usize, u32), u64>,
    /// The next unused activation-address identity, numbered past the function
    /// and block identities (EvoIR Section 3.7, revision 1.9.72).
    next_activation: u64,
}

impl State {
    /// Mint the pair of addresses one beginning activation holds for its life.
    ///
    /// EvoIR Section 3.7: "the interpreters give each activation a fresh value
    /// for each". The interpreter models no linear stack, so both are the
    /// memory model's opaque non-object provenance, which compares equal to
    /// itself, exposes to its own address and denies every access.
    fn begin_activation(&mut self) -> Result<(u64, u64), MirError> {
        let saved_lr = self.next_activation;
        let frame_pointer = saved_lr.checked_add(1).ok_or_else(|| MirError::Invalid {
            detail: "activation address identity exhausted".to_owned(),
        })?;
        self.next_activation = frame_pointer
            .checked_add(1)
            .ok_or_else(|| MirError::Invalid {
                detail: "activation address identity exhausted".to_owned(),
            })?;
        Ok((saved_lr, frame_pointer))
    }

    fn block_identity(&self, function: usize, block: u32) -> Result<u64, MirError> {
        self.blocks
            .get(&(function, block))
            .copied()
            .ok_or_else(|| MirError::Invalid {
                detail: format!("address of block {block} outside its function"),
            })
    }
}

struct Frame {
    function: usize,
    block: u32,
    next: usize,
    entered: bool,
    incoming: Vec<MirValue>,
    regs: HashMap<u32, Slot>,
    memory_frame: Option<FrameId>,
    memory_scope: Option<ScopeId>,
    results: Vec<VReg>,
    /// The saved link register and the frame pointer of this activation.
    activation: (u64, u64),
}

/// Interpret a module's named entry function.
pub fn interpret_module(
    module: &MModule,
    entry: &str,
    args: &[MirValue],
    limits: MirLimits,
) -> Result<MirOutcome, MirError> {
    interpret_module_with_fp_environment(
        module,
        entry,
        args,
        limits,
        evocc_types::floating::FpEnvironment::default(),
    )
}

pub fn interpret_module_with_fp_environment(
    module: &MModule,
    entry: &str,
    args: &[MirValue],
    limits: MirLimits,
    environment: evocc_types::floating::FpEnvironment,
) -> Result<MirOutcome, MirError> {
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    let config = MemoryConfig::for_target(
        &target,
        MemoryLimits {
            max_objects: MAX_MEMORY_OBJECTS,
            max_object_bytes: MAX_MEMORY_OBJECT_BYTES,
            max_total_bytes: MAX_MEMORY_TOTAL_BYTES,
        },
    )
    .map_err(|error| memory_error(&error))?;
    let mut state = State {
        memory: MemoryState::new(config).map_err(|error| memory_error(&error))?,
        world: WorldState::new(WorldLimits {
            max_volatile_events: MAX_VOLATILE_EVENTS,
        })
        .with_fp_environment(environment),
        globals: HashMap::new(),
        identities: HashMap::new(),
        blocks: HashMap::new(),
        next_activation: 1,
    };
    initialize_globals(module, &mut state)?;
    for (index, function) in module.functions.iter().enumerate() {
        let identity = u64::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or_else(|| MirError::Invalid {
                detail: "function provenance identity exhausted".to_owned(),
            })?;
        state.identities.insert(function.name.clone(), identity);
    }
    let mut next = u64::try_from(module.functions.len())
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or_else(|| MirError::Invalid {
            detail: "block provenance identity exhausted".to_owned(),
        })?;
    for (index, function) in module.functions.iter().enumerate() {
        for block in &function.blocks {
            state.blocks.insert((index, block.id), next);
            next = next.checked_add(1).ok_or_else(|| MirError::Invalid {
                detail: "block provenance identity exhausted".to_owned(),
            })?;
        }
    }
    state.next_activation = next;
    initialize_relocations(module, &mut state)?;

    let entry_index = module
        .functions
        .iter()
        .position(|function| function.name == entry)
        .ok_or_else(|| MirError::Invalid {
            detail: format!("module has no function @{entry}"),
        })?;
    let entry_block = block_of(&module.functions[entry_index], 0)?;
    if entry_block.args.len() != args.len() {
        return Err(MirError::Invalid {
            detail: format!(
                "@{entry} takes {} register arguments, {} were given",
                entry_block.args.len(),
                args.len()
            ),
        });
    }

    let (memory_frame, memory_scope) = begin_frame(&mut state, &module.functions[entry_index])?;
    let activation = state.begin_activation()?;
    let mut frames = vec![Frame {
        function: entry_index,
        block: 0,
        next: 0,
        entered: false,
        incoming: args.to_vec(),
        regs: HashMap::new(),
        memory_frame,
        memory_scope,
        results: Vec::new(),
        activation,
    }];
    let mut steps = 0_u64;

    loop {
        let top = frames.len() - 1;
        let function = &module.functions[frames[top].function];
        if !frames[top].entered {
            let block = block_of(function, frames[top].block)?;
            let incoming = std::mem::take(&mut frames[top].incoming);
            if incoming.len() != block.args.len() {
                return Err(MirError::Invalid {
                    detail: format!(
                        "block {} of @{} takes {} arguments, {} arrived",
                        block.id,
                        function.name,
                        block.args.len(),
                        incoming.len()
                    ),
                });
            }
            frames[top].regs.clear();
            for (arg, value) in block.args.iter().zip(incoming) {
                // A gpr32 block argument is thirty-two bits of a register; a
                // wider incoming pattern loses its upper half exactly as a
                // thirty-two-bit write would.
                write(&mut frames[top], *arg, value);
            }
            frames[top].next = 0;
            frames[top].entered = true;
        }

        let block = block_of(function, frames[top].block)?;
        steps += 1;
        if steps > limits.step_budget {
            return Err(MirError::BudgetExhausted);
        }

        if let Some(inst) = block.insts.get(frames[top].next) {
            let inst = inst.clone();
            frames[top].next += 1;
            match inst {
                MInst::Bl {
                    ref symbol,
                    ref args,
                    ref results,
                    fp_mode,
                } => {
                    let values = read_all(&frames[top], args)?;
                    let index = module
                        .functions
                        .iter()
                        .position(|function| &function.name == symbol);
                    if let Some(index) = index {
                        push_call(
                            module,
                            &mut state,
                            &mut frames,
                            index,
                            values,
                            results.clone(),
                        )?;
                    } else if let Some(outputs) =
                        hosted_numeric_runtime(symbol, &values, &mut state.world, fp_mode)?
                    {
                        if results.len() != outputs.len() {
                            return Err(MirError::Invalid {
                                detail: format!("@{symbol} has the wrong result register count"),
                            });
                        }
                        for (&reg, value) in results.iter().zip(outputs) {
                            write(&mut frames[top], reg, value);
                        }
                    } else {
                        return Err(MirError::Invalid {
                            detail: format!("bl targets missing function @{symbol}"),
                        });
                    }
                }
                MInst::Blr {
                    target,
                    ref args,
                    ref results,
                } => {
                    let identity = match read(&frames[top], target)? {
                        MirValue::Fn(PointerValue::Function(identity)) => identity,
                        MirValue::Fn(PointerValue::Null) | MirValue::Int(0) => {
                            return Err(MirError::BadCallTarget { code: "E3015" });
                        }
                        _ => return Err(MirError::BadCallTarget { code: "E3016" }),
                    };
                    let index = identity
                        .checked_sub(1)
                        .and_then(|identity| usize::try_from(identity).ok())
                        .filter(|index| *index < module.functions.len())
                        .ok_or(MirError::BadCallTarget { code: "E3016" })?;
                    let values = read_all(&frames[top], args)?;
                    let outputs = results.clone();
                    push_call(module, &mut state, &mut frames, index, values, outputs)?;
                }
                other => execute(&mut state, &mut frames[top], &other)?,
            }
            continue;
        }

        match block.term.clone() {
            MTerm::Indirect { target, edges } => {
                let pointer = match read(&frames[top], target)? {
                    MirValue::Fn(pointer) => pointer,
                    MirValue::Ptr(pointer) => PointerValue::Exposed(
                        state
                            .memory
                            .pointer_address(PointerValue::object(pointer))
                            .map_err(|_| MirError::BadBranchTarget)?,
                    ),
                    _ => return Err(MirError::BadBranchTarget),
                };
                let PointerValue::Function(identity) = state
                    .memory
                    .resolve_function_pointer(pointer)
                    .map_err(|_| MirError::BadBranchTarget)?
                else {
                    return Err(MirError::BadBranchTarget);
                };
                let (target, args) = edges
                    .iter()
                    .find(|(target, _)| {
                        state.blocks.get(&(frames[top].function, *target)) == Some(&identity)
                    })
                    .ok_or(MirError::BadBranchTarget)?;
                let values = read_all(&frames[top], args)?;
                frames[top].block = *target;
                frames[top].incoming = values;
                frames[top].entered = false;
            }
            MTerm::B { target, args } => {
                let values = read_all(&frames[top], &args)?;
                frames[top].block = target;
                frames[top].incoming = values;
                frames[top].entered = false;
            }
            MTerm::BCond {
                flags,
                cond,
                taken,
                taken_args,
                untaken,
                untaken_args,
            } => {
                let flags = read_flags(&frames[top], flags)?;
                let go = evaluate(flags, cond)?;
                let (target, args) = if go {
                    (taken, taken_args)
                } else {
                    (untaken, untaken_args)
                };
                let values = read_all(&frames[top], &args)?;
                frames[top].block = target;
                frames[top].incoming = values;
                frames[top].entered = false;
            }
            MTerm::Cb {
                nonzero,
                src,
                taken,
                taken_args,
                untaken,
                untaken_args,
            } => {
                let width = Width::for_class(src.class).unwrap_or(Width::X);
                let raw = read_bits(&frames[top], src)? & width.mask();
                let go = (raw != 0) == nonzero;
                let (target, args) = if go {
                    (taken, taken_args)
                } else {
                    (untaken, untaken_args)
                };
                let values = read_all(&frames[top], &args)?;
                frames[top].block = target;
                frames[top].incoming = values;
                frames[top].entered = false;
            }
            MTerm::Tb {
                nonzero,
                src,
                bit,
                taken,
                taken_args,
                untaken,
                untaken_args,
            } => {
                if bit >= 64 {
                    return Err(MirError::Invalid {
                        detail: format!("bit index {bit} is outside a register"),
                    });
                }
                let raw = read_bits(&frames[top], src)?;
                let set = (raw >> u32::from(bit)) & 1 == 1;
                let go = set == nonzero;
                let (target, args) = if go {
                    (taken, taken_args)
                } else {
                    (untaken, untaken_args)
                };
                let values = read_all(&frames[top], &args)?;
                frames[top].block = target;
                frames[top].incoming = values;
                frames[top].entered = false;
            }
            MTerm::Brk { imm } => return Err(MirError::Trap { imm }),
            // Section 9: "A function containing an `asm` is outside instrument
            // one: neither interpreter executes one."
            MTerm::AsmGoto { .. } => return Err(MirError::OpaqueAssembly),
            MTerm::Ret { values } => {
                let returned = read_all(&frames[top], &values)?;
                let finished = frames.pop().expect("the top frame exists");
                if let Some(frame) = finished.memory_frame {
                    state.memory = state
                        .memory
                        .end_frame(frame)
                        .map_err(|error| memory_error(&error))?;
                }
                let Some(caller) = frames.last_mut() else {
                    return Ok(MirOutcome {
                        results: returned,
                        steps,
                        memory: state.memory,
                        world: state.world,
                    });
                };
                if finished.results.len() != returned.len() {
                    return Err(MirError::Invalid {
                        detail: format!(
                            "a call expects {} results and the callee returned {}",
                            finished.results.len(),
                            returned.len()
                        ),
                    });
                }
                for (destination, value) in finished.results.iter().zip(returned) {
                    write(caller, *destination, value);
                }
            }
        }
    }
}

mod hosted_fp;

/// Only qualified hosted numeric helpers have a model here. A module's
/// own definition is executed first; unrelated unresolved calls stay errors.
fn hosted_numeric_runtime(
    symbol: &str,
    args: &[MirValue],
    world: &mut WorldState,
    fp_mode: Option<crate::FpCallMode>,
) -> Result<Option<Vec<MirValue>>, MirError> {
    if let Some(values) = hosted_fp::execute(symbol, args, world, fp_mode)? {
        return Ok(Some(values));
    }
    Ok(wide_division_runtime(symbol, args)?.map(Vec::from))
}

fn wide_division_runtime(
    symbol: &str,
    args: &[MirValue],
) -> Result<Option<[MirValue; 2]>, MirError> {
    let (signed, remainder) = match symbol {
        "__divti3" => (true, false),
        "__udivti3" => (false, false),
        "__modti3" => (true, true),
        "__umodti3" => (false, true),
        _ => return Ok(None),
    };
    let [MirValue::Int(a0), MirValue::Int(a1), MirValue::Int(b0), MirValue::Int(b1)] = args else {
        return Err(MirError::Invalid {
            detail: format!("@{symbol} requires two integer register pairs"),
        });
    };
    let left = u128::from(*a0) | (u128::from(*a1) << 64);
    let right = u128::from(*b0) | (u128::from(*b1) << 64);
    if right == 0 {
        return Err(MirError::Trap {
            imm: if remainder { 3003 } else { 3002 },
        });
    }
    let value = if signed {
        let (left, right) = (left as i128, right as i128);
        if left == i128::MIN && right == -1 {
            return Err(MirError::Trap { imm: 3004 });
        }
        (if remainder {
            left % right
        } else {
            left / right
        }) as u128
    } else if remainder {
        left % right
    } else {
        left / right
    };
    Ok(Some([
        MirValue::Int(value as u64),
        MirValue::Int((value >> 64) as u64),
    ]))
}

fn push_call(
    module: &MModule,
    state: &mut State,
    frames: &mut Vec<Frame>,
    index: usize,
    values: Vec<MirValue>,
    results: Vec<VReg>,
) -> Result<(), MirError> {
    let (memory_frame, memory_scope) = begin_frame(state, &module.functions[index])?;
    let activation = state.begin_activation()?;
    frames.push(Frame {
        function: index,
        block: 0,
        next: 0,
        entered: false,
        incoming: values,
        regs: HashMap::new(),
        memory_frame,
        memory_scope,
        results,
        activation,
    });
    Ok(())
}

fn begin_frame(
    state: &mut State,
    function: &MFunction,
) -> Result<(Option<FrameId>, Option<ScopeId>), MirError> {
    let needs = function.blocks.iter().any(|block| {
        block
            .insts
            .iter()
            .any(|inst| matches!(inst, MInst::FrameSlot { .. } | MInst::StackAlloc { .. }))
    });
    if !needs {
        return Ok((None, None));
    }
    let (memory, frame, scope) = state
        .memory
        .begin_frame()
        .map_err(|error| memory_error(&error))?;
    state.memory = memory;
    Ok((Some(frame), Some(scope)))
}

fn block_of(function: &MFunction, id: u32) -> Result<&MBlock, MirError> {
    function
        .blocks
        .iter()
        .find(|block| block.id == id)
        .ok_or_else(|| MirError::Invalid {
            detail: format!("@{} has no block {id}", function.name),
        })
}

fn initialize_globals(module: &MModule, state: &mut State) -> Result<(), MirError> {
    for global in &module.globals {
        let pointer = allocate_global(state, global)?;
        state.globals.insert(global.symbol.clone(), pointer);
    }
    Ok(())
}

fn initialize_relocations(module: &MModule, state: &mut State) -> Result<(), MirError> {
    for global in &module.globals {
        for relocation in &global.relocs {
            let resolve = |symbol: &str| -> Result<PointerValue, MirError> {
                if let Some(pointer) = state.globals.get(symbol) {
                    return Ok(PointerValue::object(*pointer));
                }
                if let Some(identity) = state.identities.get(symbol) {
                    return Ok(PointerValue::Function(*identity));
                }
                if let Some((function, block)) = symbol.split_once("^b") {
                    if let Some(index) = module.functions.iter().position(|f| f.name == function) {
                        if let Ok(block) = block.parse::<u32>() {
                            return state
                                .block_identity(index, block)
                                .map(PointerValue::Function);
                        }
                    }
                }
                Err(MirError::Invalid {
                    detail: format!("unresolved static relocation @{symbol}"),
                })
            };
            let pointer = resolve(&relocation.symbol)?;
            let base = relocation
                .difference
                .as_ref()
                .map(|d| resolve(&d.base))
                .transpose()?;
            let destination = state.globals[&global.symbol]
                .checked_offset(relocation.offset as i64)
                .map_err(|error| memory_error(&error))?;
            if let Some(base) = base {
                let value = expose(state, pointer)?
                    .wrapping_sub(expose(state, base)?)
                    .wrapping_add_signed(relocation.addend);
                state.memory = state
                    .memory
                    .store_integer(
                        destination,
                        AccessLayout::new(relocation.size_bytes(), 1),
                        u128::from(value),
                    )
                    .map_err(|error| memory_error(&error))?;
            } else {
                let pointer = if relocation.addend == 0 {
                    pointer
                } else {
                    match pointer {
                        PointerValue::Object(pointer) => PointerValue::object(
                            pointer
                                .checked_offset(relocation.addend)
                                .map_err(|error| memory_error(&error))?,
                        ),
                        _ => PointerValue::Exposed(
                            expose(state, pointer)?.wrapping_add_signed(relocation.addend),
                        ),
                    }
                };
                if relocation.exposed {
                    expose(state, pointer)?;
                }
                state.memory = state
                    .memory
                    .store_pointer_value(destination, AccessLayout::new(8, 1), pointer)
                    .map_err(|error| memory_error(&error))?;
            }
        }
        if global.read_only && global.kind != GlobalKind::StringLiteral {
            state.memory = state
                .memory
                .freeze_static(state.globals[&global.symbol])
                .map_err(|error| memory_error(&error))?;
        }
    }
    Ok(())
}

fn allocate_global(state: &mut State, global: &GlobalObject) -> Result<Pointer, MirError> {
    let (memory, pointer) = match global.kind {
        GlobalKind::StringLiteral => {
            let bytes = ByteSequence::try_from_bytes(global.bytes.clone())
                .map_err(|error| memory_error(&error))?;
            state
                .memory
                .allocate_string_literal(global.origin, global.layout, &bytes)
                .map_err(|error| memory_error(&error))?
        }
        GlobalKind::File | GlobalKind::FunctionStatic | GlobalKind::CompoundLiteral => state
            .memory
            .allocate_static(global.origin, global.layout, InitialState::Uninitialized)
            .map_err(|error| memory_error(&error))?,
    };
    state.memory = memory;
    if global.kind == GlobalKind::StringLiteral {
        return Ok(pointer);
    }
    let mut start = 0_usize;
    while start < global.initialized.len() {
        if !global.initialized[start] {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < global.initialized.len() && global.initialized[end] {
            end += 1;
        }
        let offset = i64::try_from(start).map_err(|_| MirError::Invalid {
            detail: format!("global @{} offset exceeds i64", global.symbol),
        })?;
        let address = pointer
            .checked_offset(offset)
            .map_err(|error| memory_error(&error))?;
        let size = u64::try_from(end - start).map_err(|_| MirError::Invalid {
            detail: format!("global @{} range exceeds u64", global.symbol),
        })?;
        let bytes = ByteSequence::try_from_bytes(global.bytes[start..end].to_vec())
            .map_err(|error| memory_error(&error))?;
        state.memory = state
            .memory
            .store_bytes(address, AccessLayout::new(size, 1), &bytes)
            .map_err(|error| memory_error(&error))?;
        start = end;
    }
    Ok(pointer)
}

fn read(frame: &Frame, reg: VReg) -> Result<MirValue, MirError> {
    match frame.regs.get(&reg.id) {
        Some(Slot::Value(value)) => Ok(value.clone()),
        Some(Slot::Flags(_)) => Err(MirError::Invalid {
            detail: format!("%v{} holds flags and is read as a general value", reg.id),
        }),
        None => Err(MirError::Invalid {
            detail: format!("%v{} is read before it is written", reg.id),
        }),
    }
}

fn read_all(frame: &Frame, regs: &[VReg]) -> Result<Vec<MirValue>, MirError> {
    regs.iter().map(|reg| read(frame, *reg)).collect()
}

/// The target address one pointer-carrying register holds, recording the
/// exposure (EvoIR Section 3.7: `ptr.to_int` "exposes the object whose address
/// it takes: the interpreters record the exposure").
fn expose(state: &mut State, pointer: PointerValue) -> Result<u64, MirError> {
    let (memory, address) = state
        .memory
        .expose_pointer(pointer)
        .map_err(|error| memory_error(&error))?;
    state.memory = memory;
    Ok(address)
}

fn read_bits(frame: &Frame, reg: VReg) -> Result<u64, MirError> {
    match read(frame, reg)? {
        MirValue::Int(raw) => Ok(raw),
        MirValue::Ptr(_) | MirValue::Fn(_) | MirValue::Vector(_) => Err(MirError::Invalid {
            detail: format!("%v{} holds a pointer and is read as raw bits", reg.id),
        }),
    }
}

fn read_pointer(frame: &Frame, reg: VReg) -> Result<Pointer, MirError> {
    match read(frame, reg)? {
        MirValue::Ptr(pointer) => Ok(pointer),
        MirValue::Int(0) => Err(MirError::Memory {
            code: "E3009",
            detail: "null-pointer-access".to_owned(),
        }),
        MirValue::Int(_) | MirValue::Fn(_) | MirValue::Vector(_) => Err(MirError::Memory {
            code: "E3013",
            detail: "invalid-pointer-representation".to_owned(),
        }),
    }
}

fn read_flags(frame: &Frame, reg: VReg) -> Result<Flags, MirError> {
    match frame.regs.get(&reg.id) {
        Some(Slot::Flags(flags)) => Ok(*flags),
        _ => Err(MirError::Invalid {
            detail: format!("%v{} is read as flags and is not", reg.id),
        }),
    }
}

/// Write a general register.
///
/// A `gpr32` destination clears the upper half, which is what a 32-bit AArch64
/// operation does and what makes `gpr32` and `gpr64` "the same physical
/// register file viewed at two widths" (Section 2).
fn write(frame: &mut Frame, reg: VReg, value: MirValue) {
    let value = match (reg.class, value) {
        (RegClass::Gpr32, MirValue::Int(raw)) => MirValue::Int(raw & Width::W.mask()),
        (_, value) => value,
    };
    frame.regs.insert(reg.id, Slot::Value(value));
}

/// The identity two values compare by when at least one carries provenance
/// (EvoIR Sections 1.3 and 3.5).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Identity {
    Null,
    Object(Pointer),
    Function(u64),
    Bits(u64),
    ExposedAddress(u64),
    Vector(evocc_memory::ObjectRepresentation),
}

fn identity(value: MirValue) -> Identity {
    match value {
        MirValue::Vector(value) => Identity::Vector(value),
        MirValue::Int(0) => Identity::Null,
        MirValue::Int(raw) => Identity::Bits(raw),
        MirValue::Ptr(pointer) if pointer == Pointer::NULL => Identity::Null,
        MirValue::Ptr(pointer) => Identity::Object(pointer),
        MirValue::Fn(PointerValue::Null) => Identity::Null,
        MirValue::Fn(PointerValue::Function(id)) => Identity::Function(id),
        MirValue::Fn(PointerValue::Object(pointer)) => Identity::Object(pointer),
        MirValue::Fn(PointerValue::Exposed(address)) => Identity::ExposedAddress(address),
    }
}

fn signed(raw: u64, width: Width) -> i64 {
    match width {
        Width::X => raw as i64,
        Width::W => (raw as u32) as i32 as i64,
    }
}

fn evaluate(flags: Flags, cond: Cond) -> Result<bool, MirError> {
    match flags {
        Flags::Identity { equal } => match cond {
            Cond::Eq => Ok(equal),
            Cond::Ne => Ok(!equal),
            _ => Err(MirError::Invalid {
                detail: "an ordered condition on a pointer comparison".to_owned(),
            }),
        },
        // `cmp rn, rm` sets the flags from `rn - rm`, so the same four bits are
        // available; they are computed here from the operands rather than
        // stored, which keeps one definition of each condition.
        Flags::Int { lhs, rhs, width } => {
            let (left, right) = (lhs & width.mask(), rhs & width.mask());
            let difference = left.wrapping_sub(right) & width.mask();
            Ok(match cond {
                Cond::Eq => left == right,
                Cond::Ne => left != right,
                Cond::Lt => signed(left, width) < signed(right, width),
                Cond::Le => signed(left, width) <= signed(right, width),
                Cond::Gt => signed(left, width) > signed(right, width),
                Cond::Ge => signed(left, width) >= signed(right, width),
                Cond::Lo => left < right,
                Cond::Ls => left <= right,
                Cond::Hi => left > right,
                Cond::Hs => left >= right,
                // A subtraction overflows when the operands differ in sign and
                // the result takes the sign of the subtrahend.
                Cond::Vs | Cond::Vc => {
                    let overflow = (wide_signed(left, width) - wide_signed(right, width))
                        != wide_signed(difference, width);
                    (cond == Cond::Vs) == overflow
                }
                // `cmp` sets the carry when no borrow occurred.
                Cond::Cs => left >= right,
                Cond::Cc => left < right,
            })
        }
        Flags::Arithmetic {
            negative,
            zero,
            carry,
            overflow,
        } => Ok(match cond {
            Cond::Eq => zero,
            Cond::Ne => !zero,
            Cond::Lt => negative != overflow,
            Cond::Ge => negative == overflow,
            Cond::Le => zero || (negative != overflow),
            Cond::Gt => !zero && (negative == overflow),
            Cond::Lo | Cond::Cc => !carry,
            Cond::Hs | Cond::Cs => carry,
            Cond::Ls => !carry || zero,
            Cond::Hi => carry && !zero,
            Cond::Vs => overflow,
            Cond::Vc => !overflow,
        }),
    }
}

/// The four condition bits an `adds` or `subs` writes.
///
/// The machine computes a subtraction as `rn + !rm + 1`, so one addition
/// produces the value, the carry and the overflow for both forms.
fn arithmetic_flags(
    left: u64,
    right: u64,
    width: Width,
    subtract: bool,
    carry_in: bool,
) -> (u64, Flags) {
    let mask = width.mask();
    let (left, right) = (left & mask, right & mask);
    let addend = if subtract { !right & mask } else { right };
    let carry_in = u128::from(carry_in);
    let wide = u128::from(left) + u128::from(addend) + carry_in;
    let value = (wide & u128::from(mask)) as u64;
    let carry = wide >> width.bits() != 0;
    // The machine reads overflow off the adder, so the exact sum is the one
    // the adder was given: the two operands as the form spells them and the
    // carry it was handed. For `subs` that is `left + !right + 1`, which is
    // `left - right`, and for `sbc` it is the same with the borrow the
    // preceding form left.
    let exact = wide_signed(left, width) + wide_signed(addend, width) + carry_in as i128;
    (
        value,
        Flags::Arithmetic {
            negative: wide_signed(value, width) < 0,
            zero: value == 0,
            carry,
            overflow: exact != wide_signed(value, width),
        },
    )
}

/// The signed value of a register at one width, in an arithmetic wide enough
/// that a sum of two of them cannot itself overflow.
fn wide_signed(raw: u64, width: Width) -> i128 {
    i128::from(signed(raw, width))
}

fn layout_for(size: u64) -> AccessLayout {
    // The alignment an EvoIR `access_in_bounds` derives from a scalar type is
    // its size capped at eight, so both interpreters check the same one.
    let align = u32::try_from(size.min(8)).unwrap_or(8);
    AccessLayout::new(size, align)
}

fn execute(state: &mut State, frame: &mut Frame, inst: &MInst) -> Result<(), MirError> {
    match inst {
        MInst::Floating {
            strict,
            integer_bits,
            dst,
            op,
            kind,
            args,
        } => {
            let operation = crate::floating::operation(*dst, *op, *kind, args, *integer_bits)
                .map_err(|detail| MirError::Invalid { detail })?;
            let values = args
                .iter()
                .map(|arg| match read(frame, *arg)? {
                    MirValue::Int(value) => Ok(u128::from(value)),
                    MirValue::Vector(value) => value
                        .read_integer(
                            0,
                            usize::from(arg.class.bit_width().unwrap() / 8),
                            evocc_memory::ByteOrder::Little,
                        )
                        .ok_or(MirError::Memory {
                            code: "E3001",
                            detail: "uninitialized floating operand".into(),
                        }),
                    _ => Err(MirError::Invalid {
                        detail: "invalid floating representation".into(),
                    }),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let bits = if *strict {
                let environment = state.world.fp_environment();
                let result = operation.evaluate_strict(&values, environment.rounding);
                state.world = state
                    .world
                    .with_fp_environment(environment.raised(result.status));
                result.value
            } else {
                if operation.needs_environment() {
                    state
                        .world
                        .check_fixed_fp_environment()
                        .map_err(|e| memory_error(&e))?;
                }
                operation.evaluate(&values)
            };
            let result = if dst.class.is_simd() {
                let bytes = usize::from(dst.class.bit_width().unwrap() / 8);
                let mut value = evocc_memory::ObjectRepresentation::uninitialized(bytes)
                    .map_err(|e| memory_error(&e))?;
                value
                    .write_integer(0, bytes, Some(bits), evocc_memory::ByteOrder::Little)
                    .map_err(|e| memory_error(&e))?;
                MirValue::Vector(value)
            } else {
                MirValue::Int(bits as u64)
            };
            write(frame, *dst, result);
        }
        MInst::Aes { dst, op, args } => {
            let inputs = args
                .iter()
                .map(|arg| {
                    let MirValue::Vector(vector) = read(frame, *arg)? else {
                        return Err(MirError::Invalid {
                            detail: "AES needs a vector representation".into(),
                        });
                    };
                    Ok(std::array::from_fn(|i| {
                        vector
                            .read_integer(i, 1, evocc_memory::ByteOrder::Little)
                            .map(|b| b as u8)
                    }))
                })
                .collect::<Result<Vec<_>, MirError>>()?;
            let mut vector = evocc_memory::ObjectRepresentation::uninitialized(16)
                .map_err(|error| memory_error(&error))?;
            for (index, byte) in op.evaluate(&inputs).into_iter().enumerate() {
                vector
                    .write_integer(
                        index,
                        1,
                        byte.map(u128::from),
                        evocc_memory::ByteOrder::Little,
                    )
                    .map_err(|error| memory_error(&error))?;
            }
            write(frame, *dst, MirValue::Vector(vector));
        }
        MInst::Vector {
            dst,
            extract,
            lane_bits,
            bytes,
            args,
        } => {
            let width = usize::from(*lane_bits / 8);
            let value = if *extract {
                let lane = read_bits(frame, args[1])?;
                if lane >= (usize::from(*bytes) / width) as u64 {
                    return Err(MirError::Memory {
                        code: "E3011",
                        detail: "vector-index-out-of-bounds".into(),
                    });
                }
                let value = match read(frame, args[0])? {
                    MirValue::Vector(vector) => vector
                        .read_integer(
                            lane as usize * width,
                            width,
                            evocc_memory::ByteOrder::Little,
                        )
                        .ok_or(MirError::Memory {
                            code: "E3001",
                            detail: "uninitialized-read".into(),
                        })?,
                    MirValue::Int(bits) if *bytes <= 4 => {
                        u128::from(bits >> (lane * u64::from(*lane_bits)))
                            & ((1_u128 << lane_bits) - 1)
                    }
                    _ => {
                        return Err(MirError::Invalid {
                            detail: "vector extraction needs a vector representation".into(),
                        })
                    }
                };
                MirValue::Int(value as u64)
            } else {
                let mut vector =
                    evocc_memory::ObjectRepresentation::uninitialized(usize::from(*bytes))
                        .map_err(|error| memory_error(&error))?;
                for (lane, arg) in args.iter().enumerate() {
                    vector
                        .write_integer(
                            lane * width,
                            width,
                            Some(u128::from(read_bits(frame, *arg)?)),
                            evocc_memory::ByteOrder::Little,
                        )
                        .map_err(|error| memory_error(&error))?;
                }
                if *bytes <= 4 {
                    MirValue::Int(
                        vector
                            .read_integer(0, usize::from(*bytes), evocc_memory::ByteOrder::Little)
                            .unwrap() as u64,
                    )
                } else {
                    MirValue::Vector(vector)
                }
            };
            write(frame, *dst, value);
        }
        MInst::Asm(_) => return Err(MirError::OpaqueAssembly),
        MInst::Mov { dst, src } => {
            let value = read(frame, *src)?;
            write(frame, *dst, value);
        }
        MInst::MovImm { dst, imm } => write(frame, *dst, MirValue::Int(*imm)),
        MInst::Adr { dst, block } => {
            // The address of a block is code provenance, like a function's:
            // it compares, it stores through `str.p`, and every attempt to
            // read or write memory through it is E3013.
            let identity = state.block_identity(frame.function, *block)?;
            write(
                frame,
                *dst,
                MirValue::Fn(
                    PointerValue::function(identity).map_err(|error| memory_error(&error))?,
                ),
            );
        }
        MInst::Adrp { dst, symbol, kind }
        | MInst::AddPageOff {
            dst, symbol, kind, ..
        } => {
            // The page and page-offset halves of the pair both denote the whole
            // address here: the address of a global is its object's base
            // provenance, which has no page structure to split.
            let value =
                match kind {
                    SymbolKind::Global => {
                        MirValue::Ptr(state.globals.get(symbol).copied().ok_or_else(|| {
                            MirError::Invalid {
                                detail: format!("address of missing global @{symbol}"),
                            }
                        })?)
                    }
                    SymbolKind::Function => {
                        let identity = state.identities.get(symbol).copied().ok_or_else(|| {
                            MirError::Invalid {
                                detail: format!("address of missing function @{symbol}"),
                            }
                        })?;
                        MirValue::Fn(
                            PointerValue::function(identity)
                                .map_err(|error| memory_error(&error))?,
                        )
                    }
                };
            write(frame, *dst, value);
        }
        MInst::FrameSlot {
            dst, origin, align, ..
        }
        | MInst::StackAlloc {
            dst, origin, align, ..
        } => {
            let size = match inst {
                MInst::FrameSlot { size, .. } => *size,
                MInst::StackAlloc { size, .. } => read_bits(frame, *size)?,
                _ => unreachable!("automatic allocation"),
            };
            let memory_frame = frame.memory_frame.ok_or_else(|| MirError::Invalid {
                detail: "a frame slot outside a function frame".to_owned(),
            })?;
            let memory_scope = frame.memory_scope.ok_or_else(|| MirError::Invalid {
                detail: "a frame slot outside a function scope".to_owned(),
            })?;
            let (memory, pointer) = state
                .memory
                .allocate_automatic(
                    memory_frame,
                    memory_scope,
                    *origin,
                    AccessLayout::new(size, *align),
                )
                .map_err(|error| memory_error(&error))?;
            state.memory = memory;
            write(frame, *dst, MirValue::Ptr(pointer));
        }
        MInst::StackRelease { address } => {
            let pointer = read_pointer(frame, *address)?;
            let id = frame.memory_frame.ok_or_else(|| MirError::Invalid {
                detail: "stack release has no function frame".into(),
            })?;
            state.memory = state
                .memory
                .end_object_lifetime(id, pointer)
                .map_err(|error| memory_error(&error))?;
        }
        MInst::AtomicRmw {
            dst,
            base,
            value,
            expected,
            operation,
            kind,
            order,
            volatile,
        } => {
            let address = read_pointer(frame, *base)?;
            let convert = |value| match value {
                MirValue::Int(raw) if !matches!(kind, LoadKind::P | LoadKind::Pf) => {
                    Ok(evocc_memory::AtomicValue::Integer(raw as u128))
                }
                MirValue::Ptr(pointer) => Ok(evocc_memory::AtomicValue::Pointer(
                    PointerValue::object(pointer),
                )),
                MirValue::Fn(pointer) => Ok(evocc_memory::AtomicValue::Pointer(pointer)),
                _ => Err(MirError::Invalid {
                    detail: "invalid atomic RMW operand".into(),
                }),
            };
            let value = convert(read(frame, *value)?)?;
            let expected = expected
                .map(|value| read(frame, value).and_then(convert))
                .transpose()?;
            let (memory, world, result) = state
                .memory
                .atomic_rmw(
                    &state.world,
                    address,
                    layout_for(kind.size_bytes()),
                    *order,
                    *volatile,
                    *operation,
                    value,
                    expected,
                )
                .map_err(|error| memory_error(&error))?;
            state.memory = memory;
            state.world = world;
            let result = match result.old {
                evocc_memory::AtomicValue::Integer(raw) => MirValue::Int(raw as u64),
                evocc_memory::AtomicValue::Pointer(pointer) => match (kind, pointer) {
                    (LoadKind::Pf, _) => MirValue::Fn(pointer),
                    (_, PointerValue::Null) => MirValue::Ptr(Pointer::NULL),
                    (_, PointerValue::Object(pointer)) => MirValue::Ptr(pointer),
                    (_, PointerValue::Exposed(address)) => {
                        MirValue::Ptr(Pointer::from_address(address))
                    }
                    _ => MirValue::Fn(pointer),
                },
            };
            write(frame, *dst, result);
        }
        MInst::Atomic {
            load,
            value,
            base,
            kind,
            order,
            volatile,
        } => {
            let address = read_pointer(frame, *base)?;
            let layout = layout_for(kind.size_bytes());
            if *load {
                let (loaded, world) = state
                    .memory
                    .atomic_load(
                        &state.world,
                        address,
                        layout,
                        *order,
                        *volatile,
                        matches!(kind, LoadKind::P | LoadKind::Pf),
                    )
                    .map_err(|error| memory_error(&error))?;
                state.world = world;
                let loaded = match loaded {
                    evocc_memory::AtomicValue::Integer(raw) => MirValue::Int(raw as u64),
                    evocc_memory::AtomicValue::Pointer(pointer) => match (kind, pointer) {
                        (LoadKind::Pf, _) => MirValue::Fn(pointer),
                        (_, PointerValue::Null) => MirValue::Ptr(Pointer::NULL),
                        (_, PointerValue::Object(value)) => MirValue::Ptr(value),
                        (_, PointerValue::Exposed(value)) => {
                            MirValue::Ptr(Pointer::from_address(value))
                        }
                        _ => MirValue::Fn(pointer),
                    },
                };
                write(frame, *value, loaded);
            } else {
                let value = read(frame, *value)?;
                let value = if matches!(kind, LoadKind::P | LoadKind::Pf) {
                    evocc_memory::AtomicValue::Pointer(match value {
                        MirValue::Ptr(value) => PointerValue::object(value),
                        MirValue::Fn(value) => value,
                        MirValue::Int(_) | MirValue::Vector(_) => {
                            return Err(MirError::Memory {
                                code: "E3013",
                                detail: "invalid-pointer-representation".into(),
                            })
                        }
                    })
                } else {
                    let raw = match value {
                        MirValue::Int(value) => value,
                        MirValue::Ptr(value) => expose(state, PointerValue::object(value))?,
                        MirValue::Fn(value) => expose(state, value)?,
                        MirValue::Vector(_) => {
                            return Err(MirError::Invalid {
                                detail: "vector used as scalar atomic value".into(),
                            })
                        }
                    };
                    evocc_memory::AtomicValue::Integer(u128::from(raw))
                };
                let (memory, world) = state
                    .memory
                    .atomic_store(&state.world, address, layout, *order, *volatile, value)
                    .map_err(|error| memory_error(&error))?;
                state.memory = memory;
                state.world = world;
            }
        }
        MInst::Load {
            align,
            dst,
            base,
            offset,
            kind,
            volatile,
        } => {
            let address = read_pointer(frame, *base)?
                .checked_offset(*offset)
                .map_err(|error| memory_error(&error))?;
            let mut access = layout_for(kind.size_bytes());
            if *align != 0 {
                access.align_bytes = *align;
            }
            let value = match kind {
                LoadKind::S | LoadKind::D | LoadKind::Q => {
                    let (value, world) = state
                        .memory
                        .load_representation(&state.world, address, access, *volatile)
                        .map_err(|error| memory_error(&error))?;
                    state.world = world;
                    MirValue::Vector(value)
                }
                LoadKind::P => {
                    let pointer = if *volatile {
                        let (pointer, world) = state
                            .memory
                            .volatile_load_pointer(&state.world, address, access)
                            .map_err(|error| memory_error(&error))?;
                        state.world = world;
                        pointer
                    } else {
                        state
                            .memory
                            .load_pointer(address, access)
                            .map_err(|error| memory_error(&error))?
                    };
                    MirValue::Ptr(pointer)
                }
                LoadKind::Pf => {
                    let pointer = if *volatile {
                        let (pointer, world) = state
                            .memory
                            .volatile_load_pointer_value(&state.world, address, access)
                            .map_err(|error| memory_error(&error))?;
                        state.world = world;
                        pointer
                    } else {
                        state
                            .memory
                            .load_pointer_value(address, access)
                            .map_err(|error| memory_error(&error))?
                    };
                    MirValue::Fn(pointer)
                }
                _ => {
                    let raw = if *volatile {
                        let (raw, world) = state
                            .memory
                            .volatile_load_integer(&state.world, address, access)
                            .map_err(|error| memory_error(&error))?;
                        state.world = world;
                        raw
                    } else {
                        state
                            .memory
                            .load_integer(address, access)
                            .map_err(|error| memory_error(&error))?
                    };
                    let raw = u64::try_from(raw).map_err(|_| MirError::Invalid {
                        detail: "an access wider than a register".to_owned(),
                    })?;
                    let bits = kind.size_bytes() * 8;
                    MirValue::Int(if kind.is_signed() {
                        sign_extend(raw, u16::try_from(bits).unwrap_or(64))
                    } else {
                        raw
                    })
                }
            };
            write(frame, *dst, value);
        }
        MInst::Store {
            align,
            src,
            base,
            offset,
            kind,
            volatile,
        } => {
            let address = read_pointer(frame, *base)?
                .checked_offset(*offset)
                .map_err(|error| memory_error(&error))?;
            let mut access = layout_for(kind.size_bytes());
            if *align != 0 {
                access.align_bytes = *align;
            }
            let value = read(frame, *src)?;
            match (kind, value) {
                (StoreKind::S | StoreKind::D | StoreKind::Q, MirValue::Vector(value)) => {
                    let (memory, world) = state
                        .memory
                        .store_representation(&state.world, address, access, &value, *volatile)
                        .map_err(|error| memory_error(&error))?;
                    state.memory = memory;
                    state.world = world;
                }
                (StoreKind::P, MirValue::Ptr(pointer)) => {
                    store_pointer_value(
                        state,
                        address,
                        access,
                        PointerValue::object(pointer),
                        *volatile,
                    )?;
                }
                (StoreKind::P, MirValue::Fn(pointer)) => {
                    store_pointer_value(state, address, access, pointer, *volatile)?;
                }
                (StoreKind::P, MirValue::Int(_)) => {
                    return Err(MirError::Memory {
                        code: "E3013",
                        detail: "invalid-pointer-representation".to_owned(),
                    });
                }
                // `strb`/`strh`/`str` write the low bytes of the register.
                //
                // Section 5 lowers `ptr.to_int` to "nothing: the value is the
                // same 64-bit register", so a register a conversion made an
                // integer still carries its provenance here. The machine writes
                // the address bytes, which is what the conversion meant, and the
                // model records the exposure the conversion is.
                (_, value) => {
                    let raw = match value {
                        MirValue::Int(raw) => raw,
                        MirValue::Ptr(pointer) => expose(state, PointerValue::object(pointer))?,
                        MirValue::Fn(pointer) => expose(state, pointer)?,
                        MirValue::Vector(_) => {
                            return Err(MirError::Invalid {
                                detail: "vector stored using a scalar width".into(),
                            })
                        }
                    };
                    let bits = kind.size_bytes() * 8;
                    let masked = if bits >= 64 {
                        raw
                    } else {
                        raw & ((1_u64 << bits) - 1)
                    };
                    if *volatile {
                        let (memory, world) = state
                            .memory
                            .volatile_store_integer(
                                &state.world,
                                address,
                                access,
                                u128::from(masked),
                            )
                            .map_err(|error| memory_error(&error))?;
                        state.memory = memory;
                        state.world = world;
                    } else {
                        state.memory = state
                            .memory
                            .store_integer(address, access, u128::from(masked))
                            .map_err(|error| memory_error(&error))?;
                    }
                }
            }
        }
        MInst::Binary {
            op,
            dst,
            lhs,
            rhs,
            width,
        } => {
            let left = read(frame, *lhs)?;
            let right = read(frame, *rhs)?;
            let value = binary(state, *op, left, right, *width)?;
            write(frame, *dst, value);
        }
        MInst::BinaryImm {
            op,
            dst,
            lhs,
            imm,
            width,
        } => {
            let left = read(frame, *lhs)?;
            let value = binary(state, *op, left, MirValue::Int(*imm), *width)?;
            write(frame, *dst, value);
        }
        MInst::Msub {
            dst,
            n,
            m,
            a,
            width,
        } => {
            let n = read_bits(frame, *n)? & width.mask();
            let m = read_bits(frame, *m)? & width.mask();
            let a = read_bits(frame, *a)? & width.mask();
            let value = a.wrapping_sub(n.wrapping_mul(m)) & width.mask();
            write(frame, *dst, MirValue::Int(value));
        }
        MInst::Madd {
            dst,
            n,
            m,
            a,
            width,
        } => {
            let n = read_bits(frame, *n)? & width.mask();
            let m = read_bits(frame, *m)? & width.mask();
            let a = read_bits(frame, *a)? & width.mask();
            let value = a.wrapping_add(n.wrapping_mul(m)) & width.mask();
            write(frame, *dst, MirValue::Int(value));
        }
        MInst::CarryBinary {
            op,
            dst,
            flags,
            lhs,
            rhs,
            width,
        } => {
            let carry = evaluate(read_flags(frame, *flags)?, Cond::Cs)?;
            let left = read_bits(frame, *lhs)?;
            let right = read_bits(frame, *rhs)?;
            let subtract = *op == CarryOp::Sbc;
            let (value, _) = arithmetic_flags(left, right, *width, subtract, carry);
            write(frame, *dst, MirValue::Int(value));
        }
        MInst::AddCarryFlags {
            dst,
            flags_in,
            lhs,
            rhs,
            width,
        }
        | MInst::SubCarryFlags {
            dst,
            flags_in,
            lhs,
            rhs,
            width,
        } => {
            let carry = evaluate(read_flags(frame, *flags_in)?, Cond::Cs)?;
            let left = read_bits(frame, *lhs)?;
            let right = read_bits(frame, *rhs)?;
            let subtract = matches!(inst, MInst::SubCarryFlags { .. });
            let (_, set) = arithmetic_flags(left, right, *width, subtract, carry);
            frame.regs.insert(dst.id, Slot::Flags(set));
        }
        // `tst rn, #imm` is `ands` into the zero register: the logical flags
        // set `n` and `z` from the result and clear `c` and `v`.
        MInst::TstImm {
            dst,
            lhs,
            imm,
            width,
        } => {
            let value = (read_bits(frame, *lhs)? & *imm) & width.mask();
            frame.regs.insert(
                dst.id,
                Slot::Flags(Flags::Arithmetic {
                    negative: signed(value, *width) < 0,
                    zero: value == 0,
                    carry: false,
                    overflow: false,
                }),
            );
        }
        MInst::Extend { op, dst, src } => {
            let raw = read_bits(frame, *src)?;
            let bits = op.source_bits();
            let value = if op.is_signed() {
                sign_extend(raw, bits)
            } else {
                raw & mask(bits)
            };
            write(frame, *dst, MirValue::Int(value));
        }
        // Section 5's bit-counting and byte-swap row (revision 0.14). Each
        // form reads and writes the whole register at its own width, exactly
        // as the machine does; the lowering is what makes the answer the
        // EvoIR one for a narrower `N`.
        MInst::Unary {
            op,
            dst,
            src,
            width,
        } => {
            let raw = read_bits(frame, *src)? & width.mask();
            let bits = u32::from(width.bits());
            let value = match op {
                UnaryOp::Clz => {
                    if *width == Width::W {
                        u64::from((raw as u32).leading_zeros())
                    } else {
                        u64::from(raw.leading_zeros())
                    }
                }
                UnaryOp::Rbit => {
                    if *width == Width::W {
                        u64::from((raw as u32).reverse_bits())
                    } else {
                        raw.reverse_bits()
                    }
                }
                UnaryOp::Rev16 => {
                    let mut value = 0_u64;
                    for halfword in 0..bits / 16 {
                        let shift = halfword * 16;
                        let half = (raw >> shift) & 0xffff;
                        value |= (((half & 0xff) << 8) | (half >> 8)) << shift;
                    }
                    value
                }
                UnaryOp::Rev => {
                    if *width == Width::W {
                        u64::from((raw as u32).swap_bytes())
                    } else {
                        raw.swap_bytes()
                    }
                }
            };
            write(frame, *dst, MirValue::Int(value & width.mask()));
        }
        MInst::Bfx {
            signed: is_signed,
            dst,
            src,
            lsb,
            bits,
            width,
        } => {
            let raw = read_bits(frame, *src)? & width.mask();
            let shifted = if *lsb >= 64 {
                0
            } else {
                raw >> u32::from(*lsb)
            };
            let value = if *is_signed {
                sign_extend(shifted, *bits)
            } else {
                shifted & mask(*bits)
            };
            write(frame, *dst, MirValue::Int(value & width.mask()));
        }
        MInst::Cmp {
            dst,
            lhs,
            rhs,
            width,
        } => {
            let left = read(frame, *lhs)?;
            let right = read(frame, *rhs)?;
            frame
                .regs
                .insert(dst.id, Slot::Flags(compare(left, right, *width)));
        }
        MInst::CmpImm {
            dst,
            lhs,
            imm,
            width,
        } => {
            let left = read(frame, *lhs)?;
            frame.regs.insert(
                dst.id,
                Slot::Flags(compare(left, MirValue::Int(*imm), *width)),
            );
        }
        MInst::Cset { dst, flags, cond } => {
            let flags = read_flags(frame, *flags)?;
            let value = u64::from(evaluate(flags, *cond)?);
            write(frame, *dst, MirValue::Int(value));
        }
        MInst::Csel {
            dst,
            flags,
            lhs,
            rhs,
            cond,
            width,
        } => {
            let condition = evaluate(read_flags(frame, *flags)?, *cond)?;
            let value = if condition {
                read(frame, *lhs)?
            } else {
                read(frame, *rhs)?
            };
            let value = match value {
                MirValue::Int(raw) => MirValue::Int(raw & width.mask()),
                other => other,
            };
            write(frame, *dst, value);
        }
        MInst::FlagBinary {
            op,
            dst,
            flags,
            lhs,
            rhs,
            width,
        } => {
            let left = read_bits(frame, *lhs)?;
            let right = read_bits(frame, *rhs)?;
            let subtract = *op == FlagOp::Subs;
            let (value, set) = arithmetic_flags(left, right, *width, subtract, subtract);
            write(frame, *dst, MirValue::Int(value));
            frame.regs.insert(flags.id, Slot::Flags(set));
        }
        MInst::WideMultiply { op, dst, lhs, rhs } => {
            let value = match op {
                // `smull xd, wn, wm`: the signed 32x32 product, exact in 64.
                WideMul::Smull => {
                    let left = i64::from(read_bits(frame, *lhs)? as u32 as i32);
                    let right = i64::from(read_bits(frame, *rhs)? as u32 as i32);
                    left.wrapping_mul(right) as u64
                }
                // `smulh xd, xn, xm`: the high half of the signed 64x64 product.
                WideMul::Smulh => {
                    let left = i128::from(read_bits(frame, *lhs)? as i64);
                    let right = i128::from(read_bits(frame, *rhs)? as i64);
                    ((left * right) >> 64) as i64 as u64
                }
                // `umulh xd, xn, xm`: the high half of the unsigned product.
                WideMul::Umulh => {
                    let left = u128::from(read_bits(frame, *lhs)?);
                    let right = u128::from(read_bits(frame, *rhs)?);
                    ((left * right) >> 64) as u64
                }
            };
            write(frame, *dst, MirValue::Int(value));
        }
        MInst::MemCopy {
            dst,
            src,
            size,
            align,
            volatile,
        } => {
            let destination = read_pointer(frame, *dst)?;
            let source = read_pointer(frame, *src)?;
            let layout = AccessLayout::new(*size, *align);
            let (memory, world) = state
                .memory
                .copy_with_volatile(&state.world, destination, source, layout, *volatile)
                .map_err(|error| memory_error(&error))?;
            state.memory = memory;
            state.world = world;
        }
        // Section 4: "`ldp`/`stp rt1, rt2, [rn, #off]` | two registers,
        // prologue and epilogue only". The pair is two adjacent eight-byte
        // accesses through the base, with the pre-index the prologue uses.
        MInst::Ldp {
            rt1,
            rt2,
            base,
            offset,
            writeback,
            volatile,
        } => {
            let address = read_pointer(frame, *base)?;
            let at = if *writeback {
                let moved = address
                    .checked_offset(*offset)
                    .map_err(|error| memory_error(&error))?;
                write(frame, *base, MirValue::Ptr(moved));
                moved
            } else {
                address
                    .checked_offset(*offset)
                    .map_err(|error| memory_error(&error))?
            };
            let access = layout_for(8);
            for (index, dst) in [(0_i64, *rt1), (8, *rt2)] {
                let slot = at
                    .checked_offset(index)
                    .map_err(|error| memory_error(&error))?;
                let raw = if *volatile {
                    let (raw, world) = state
                        .memory
                        .volatile_load_integer(&state.world, slot, access)
                        .map_err(|error| memory_error(&error))?;
                    state.world = world;
                    raw
                } else {
                    state
                        .memory
                        .load_integer(slot, access)
                        .map_err(|error| memory_error(&error))?
                };
                let raw = u64::try_from(raw).map_err(|_| MirError::Invalid {
                    detail: "an access wider than a register".to_owned(),
                })?;
                write(frame, dst, MirValue::Int(raw));
            }
        }
        MInst::Stp {
            rt1,
            rt2,
            base,
            offset,
            writeback,
            volatile,
        } => {
            let address = read_pointer(frame, *base)?;
            let at = address
                .checked_offset(*offset)
                .map_err(|error| memory_error(&error))?;
            if *writeback {
                write(frame, *base, MirValue::Ptr(at));
            }
            let access = layout_for(8);
            for (index, src) in [(0_i64, *rt1), (8, *rt2)] {
                let slot = at
                    .checked_offset(index)
                    .map_err(|error| memory_error(&error))?;
                let value = u128::from(read_bits(frame, src)?);
                if *volatile {
                    let (memory, world) = state
                        .memory
                        .volatile_store_integer(&state.world, slot, access, value)
                        .map_err(|error| memory_error(&error))?;
                    state.memory = memory;
                    state.world = world;
                } else {
                    state.memory = state
                        .memory
                        .store_integer(slot, access, value)
                        .map_err(|error| memory_error(&error))?;
                }
            }
        }
        // The spill pair is expanded by emission against the frame layout of
        // Section 6, which the interpreter has no linear stack to model; see
        // the recorded gap on the prologue.
        MInst::SpillStore { .. } | MInst::SpillReload { .. } => {
            return Err(MirError::Invalid {
                detail: "a spill access exists only after allocation".to_owned(),
            });
        }
        // Section 6 (revision 0.18): a marker for allocation. Before it, the
        // destination pointer of a record result above sixteen bytes is an
        // ordinary last argument, which is what the interpreter passes, so
        // there is nothing here to do.
        MInst::IndirectResult => {}
        MInst::FpEnvCheck { strict } => {
            if !strict {
                state
                    .world
                    .check_fixed_fp_environment()
                    .map_err(|e| memory_error(&e))?;
            }
        }
        // Which arguments find no register is what allocation decides, so a
        // stack argument, like a spill, exists only after it.
        MInst::IncomingArg { .. } | MInst::OutgoingArg { .. } => {
            return Err(MirError::Invalid {
                detail: "a stack argument exists only after allocation".to_owned(),
            });
        }
        // Section 5's activation-address rows (revision 0.17). The interpreter
        // models no linear stack, so it answers them the way EvoIR Section 3.7
        // requires instead: one fresh identity per activation for each, equal
        // to itself for the activation's life and `E3013` through every access.
        MInst::FrameAddress {
            dst,
            place: place @ (crate::FramePlace::SavedLinkRegister | crate::FramePlace::FramePointer),
        } => {
            let identity = if *place == crate::FramePlace::SavedLinkRegister {
                frame.activation.0
            } else {
                frame.activation.1
            };
            write(
                frame,
                *dst,
                MirValue::Fn(
                    PointerValue::function(identity).map_err(|error| memory_error(&error))?,
                ),
            );
        }
        // Section 6's variadic places live above and inside the linear stack
        // the interpreter has no model of, exactly as the prologue does; the
        // native run judges them, as it does physical ancestor-frame walks.
        MInst::FrameAddress { .. } | MInst::VaSaveArea { .. } => {
            return Err(MirError::Invalid {
                detail: "a physical frame place exists only in an emitted frame".to_owned(),
            });
        }
        MInst::Bl { .. } | MInst::Blr { .. } => {
            return Err(MirError::Invalid {
                detail: "a call is executed by the frame stack".to_owned(),
            });
        }
    }
    Ok(())
}

fn store_pointer_value(
    state: &mut State,
    address: Pointer,
    access: AccessLayout,
    value: PointerValue,
    volatile: bool,
) -> Result<(), MirError> {
    if volatile {
        let (memory, world) = state
            .memory
            .volatile_store_pointer_value(&state.world, address, access, value)
            .map_err(|error| memory_error(&error))?;
        state.memory = memory;
        state.world = world;
    } else {
        state.memory = state
            .memory
            .store_pointer_value(address, access, value)
            .map_err(|error| memory_error(&error))?;
    }
    Ok(())
}

fn mask(bits: u16) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else {
        (1_u64 << bits) - 1
    }
}

fn sign_extend(raw: u64, bits: u16) -> u64 {
    if bits == 0 || bits >= 64 {
        return raw;
    }
    let shift = 64 - u32::from(bits);
    (((raw << shift) as i64) >> shift) as u64
}

fn compare(left: MirValue, right: MirValue, width: Width) -> Flags {
    match (left, right) {
        (MirValue::Int(lhs), MirValue::Int(rhs)) => Flags::Int { lhs, rhs, width },
        (left, right) => Flags::Identity {
            equal: identity(left) == identity(right),
        },
    }
}

fn binary(
    state: &mut State,
    op: BinOp,
    left: MirValue,
    right: MirValue,
    width: Width,
) -> Result<MirValue, MirError> {
    // ptr.to_int/from_int need no native instruction. An address used by
    // machine arithmetic therefore exposes its code identity here rather
    // than rejecting the representation-preserving IR conversion.
    let mut expose_code = |value| -> Result<MirValue, MirError> {
        match value {
            MirValue::Fn(pointer) => Ok(MirValue::Ptr(Pointer::from_address(expose(
                state, pointer,
            )?))),
            other => Ok(other),
        }
    };
    let left = expose_code(left)?;
    let right = expose_code(right)?;
    if let (BinOp::Sub, MirValue::Ptr(left), MirValue::Ptr(right)) = (op, &left, &right) {
        return Ok(MirValue::Int(
            expose(state, PointerValue::object(*left))?
                .wrapping_sub(expose(state, PointerValue::object(*right))?)
                & width.mask(),
        ));
    }
    // Pointer arithmetic keeps provenance, which is what makes an EvoIR
    // `ptr.offset` lowered to `add` denote the same value in both
    // interpreters.
    if let (BinOp::Add | BinOp::Sub, MirValue::Ptr(pointer), MirValue::Int(delta)) =
        (op, left.clone(), right.clone())
    {
        let delta = delta as i64;
        let delta = if op == BinOp::Sub {
            delta.wrapping_neg()
        } else {
            delta
        };
        return pointer
            .checked_offset(delta)
            .map(MirValue::Ptr)
            .map_err(|error| memory_error(&error));
    }
    if let (BinOp::Add, MirValue::Int(delta), MirValue::Ptr(pointer)) =
        (op, left.clone(), right.clone())
    {
        return pointer
            .checked_offset(delta as i64)
            .map(MirValue::Ptr)
            .map_err(|error| memory_error(&error));
    }
    let (MirValue::Int(lhs), MirValue::Int(rhs)) = (left, right) else {
        return Err(MirError::Invalid {
            detail: format!("{} on a pointer-carrying register", op.mnemonic()),
        });
    };
    let m = width.mask();
    let (lhs, rhs) = (lhs & m, rhs & m);
    let shift_mask = u64::from(width.bits() - 1);
    let value = match op {
        BinOp::Add => lhs.wrapping_add(rhs),
        BinOp::Sub => lhs.wrapping_sub(rhs),
        BinOp::Mul => lhs.wrapping_mul(rhs),
        BinOp::And => lhs & rhs,
        BinOp::Orr => lhs | rhs,
        BinOp::Eor => lhs ^ rhs,
        BinOp::Lsl => lhs.wrapping_shl((rhs & shift_mask) as u32),
        BinOp::Lsr => lhs.wrapping_shr((rhs & shift_mask) as u32),
        BinOp::Asr => {
            let amount = (rhs & shift_mask) as u32;
            (signed(lhs, width) >> amount) as u64
        }
        BinOp::UDiv => lhs.checked_div(rhs).unwrap_or(0),
        BinOp::SDiv => {
            // Section 4: "division by zero yields 0 on the machine". AArch64
            // also defines the one overflowing quotient to be the dividend.
            let (left, right) = (signed(lhs, width), signed(rhs, width));
            if right == 0 {
                0
            } else {
                left.wrapping_div(right) as u64
            }
        }
    };
    Ok(MirValue::Int(value & m))
}
