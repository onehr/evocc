# EvoIR Specification

**Revision 1.7** | **Authoritative reference** | **2026-08-22**

> EvoIR is a typed, contextual equality substrate over explicit state. It is the
> single semantic contract consumed by the parser agent, lowering agent, EqSat
> agent, proof agent, and backend agents. Any transformation that does not
> respect this specification is unsound regardless of empirical results.

---

## Table of Contents

1. [Type Universe](#1-type-universe)
2. [Block-Argument SSA Semantics](#2-block-argument-ssa-semantics)
3. [Instruction Set](#3-instruction-set)
4. [Effect Model](#4-effect-model)
5. [Memory Model](#5-memory-model)
6. [Undefined Behavior](#6-undefined-behavior)
7. [Equality Criterion](#7-equality-criterion)
8. [Persistent Equality Database](#8-persistent-equality-database)
9. [Textual Syntax](#9-textual-syntax)
10. [Cross-IR Contracts](#10-cross-ir-contracts)

---

## 1. Type Universe

### 1.1 Type Grammar

Every EvoIR value has a statically known type drawn from the following grammar:

```text
Type ::=
    bits<N>          -- unsigned bit-vector of width N (arbitrary positive integer)
  | f32 | f64        -- IEEE 754 binary32 and binary64
  | ptr<AS>          -- typed pointer in address space AS
  | mem<R>           -- memory snapshot for region R
  | world            -- global ordering token for volatile/IO effects
  | vec<N, Type>     -- packed SIMD vector of N elements
  | tuple<Type...>   -- heterogeneous product type (zero or more elements)
  | void             -- absence of a value
```

```text
AddressSpace ::=
    generic
  | global
  | stack
  | threadlocal
```

```text
Region ::=
    all
  | unknown
  | global
  | stack
  | heap
  | escaped
```

### 1.2 Integer Types and Signedness

The central architectural decision: **signedness lives in operations, not in integer
storage types.**

A `bits<32>` value is neither signed nor unsigned. It is a bit-vector of width 32.
The operation determines interpretation:

```text
sdiv.bits<32>    -- signed division (treats operands as two's-complement signed)
udiv.bits<32>    -- unsigned division (treats operands as unsigned magnitudes)
icmp.slt         -- signed less-than
icmp.ult         -- unsigned less-than
sext             -- sign-extend
zext             -- zero-extend
```

This is fundamental to bit-vector equality reasoning. Without this separation,
every rewrite rule must branch on the signedness of its operands, which doubles
or triples the rule surface.

### 1.3 Pointer Types

A pointer is a typed reference into one address space. In the Day 7 abstract
machine its runtime identity is exactly:

```text
Pointer {
    object: ObjectId,
    byte_offset: signed 64-bit byte offset,
}
```

`ObjectId(0)` is the canonical null provenance and never names an object.
Every non-null `ObjectId` names one storage instance, is allocated
monotonically, and is never reused after that object's lifetime ends. The
signed byte offset is kept separate from provenance; checked addition overflow
reports `E3011`, and no host address or Rust pointer participates in the
semantics. A one-past offset is representable but
cannot be dereferenced. Integer-to-pointer and pointer-to-integer conversions
are outside the Day 7 profile.

```text
ptr<generic>         -- default address space
ptr<global>          -- global/static storage
ptr<stack>           -- automatic storage
ptr<threadlocal>     -- thread-local storage
```

Address-space distinctions remain part of the general type grammar, but the
Day 7 C lowering profile maps every object pointer to `ptr<generic>`. Storage
duration is object metadata, not a pointer type, because one C `T *` may point
to automatic, static, or string-literal storage over its lifetime. No
address-space spelling is a `NoAlias` proof in the Day 7 profile.

### 1.4 Memory Types

A `mem<R>` value is a functional snapshot of all memory in region `R` at a
particular program point. Memory tokens are threaded through operations.

```text
mem<all>             -- the entire addressable memory
mem<unknown>         -- memory whose region is not statically known
mem<global>          -- global/static storage only
mem<stack>           -- stack storage only
mem<heap>            -- heap storage only
mem<escaped>         -- storage whose address has escaped analysis
```

The Day 7 executable profile requires exactly `mem<all>`. Other region
spellings remain reserved by the general type grammar, but region splitting
and every optimization that depends on `NoAlias` are outside Day 7.

### 1.5 World Token

The `world` type is a singleton. The world token `%w: world` is a global
ordering token. The Day 6 instruction layout threads it through every memory
load/store, allocation/lifetime operation, and call; volatile, atomic, and IO
operations give that flow an observable scheduling-barrier meaning across all
regions. A normal operation with a world result produces a fresh SSA identity
while preserving the same abstract world state. State-only
`pointer_valid`/`access_in_bounds` guards consume and replace `mem<all>` only
and have no world operand or result.

### 1.6 SIMD and Tuples

`vec<N, Type>` is a packed SIMD vector. `N` is the lane count (positive
integer). All lanes share the same element type.

`tuple<Type...>` is a heterogeneous product type. It is used for multiple
return values from calls. A `tuple<>` (empty tuple) is equivalent to `void`.

### 1.7 Type Well-Formedness

| Type | Well-formedness condition |
|---|---|
| `bits<N>` | `N > 0` |
| `f32` | always well-formed |
| `f64` | always well-formed |
| `ptr<AS>` | always well-formed |
| `mem<R>` | always well-formed |
| `world` | always well-formed |
| `vec<N, T>` | `N > 0`, `T` is well-formed, `T != void`, `T` is not `mem<_>` or `world` |
| `tuple<T...>` | every `T_i` is well-formed |
| `void` | always well-formed |

---

## 2. Block-Argument SSA Semantics

### 2.1 Module and Function Structure

An EvoIR module contains an ordered global-object table followed by an ordered
function table. A global object binds one unique symbol, stable source-origin
site, and storage kind (`static` or `string_literal`) to a fixed size and
alignment, initial bytes, and a byte-for-byte initializedness bitmap. Module
interpretation allocates every global exactly once before entering the
requested function; `global_addr` then returns that object's stable
provenance. A duplicate global symbol or malformed initializer closure makes
the module invalid. Pointer-valued static initializers require a relocation
schema and are explicitly outside the Day 7 profile; ordinary scalar, array,
record, and string/byte initializers are represented directly as bytes.
Top-level module execution starts from that deterministic declaration-order
memory snapshot and an empty world trace; callers do not inject arbitrary
initial memory or world states through the ABI-shaped entry token positions.

A function consists of:

- A **name** (symbol)
- A list of **formal parameters** with names and types
- A list of **return types**
- A **calling convention** tag
- A list of **basic blocks** (one of which is the entry block)

Formal parameters belong to the call interface and are not SSA definitions in
the function CFG. The entry block has one block argument for each formal
parameter, in the same order and with the same type. Those entry block
arguments are the SSA definitions that the CFG may use. A call binds actual
arguments to the entry block arguments positionally.

```text
func @name(%p1: T1, ..., %pN: TN, %m: mem<R>, %w: world)
    -> (R1, ..., RK, mem<R>, world)
    cc = target_default
{
    ; blocks
}
```

The function signature **explicitly carries** memory and world tokens so effects
are threaded through the call graph. Every function that may read or write
memory, or that may call such functions, must accept and return `mem<R>` and
`world` tokens. For every such call/effect participant, the Day 6 canonical
interface places exactly one `mem<R>` followed by exactly one `world` after all
ordinary parameters, and the same `mem<R>, world` pair after all ordinary
returns.

### 2.2 Basic Blocks

A basic block has:

- A **label** (e.g. `^entry`, `^loop`, `^body`)
- An ordered list of **block arguments** (named, typed formal parameters)
- An ordered list of **non-terminator instructions**
- Exactly one **terminator instruction**

```text
^block_name(%arg1: T1, ..., %argN: TN):
    inst1
    inst2
    ...
    terminator
```

### 2.3 Block Arguments (Phi-Equivalent)

Block arguments are the phi-equivalent in block-argument SSA. They are bound
at block entry by the predecessor's terminator:

```text
^entry:
    %v = const.bits<32> 42
    br ^loop(%v, %v)

^loop(%a: bits<32>, %b: bits<32>):
    ...
```

In this example, `%a` is bound to `%v` and `%b` is bound to `%v` on the edge
from `^entry` to `^loop`. On any other edge entering `^loop`, the block
arguments receive the values passed by that edge's terminator.

The entry block `^entry` receives its block arguments from the function's
formal parameters. The entry block must have the same number of block arguments
as the function has formal parameters, and their types must match positionally.
The formal-parameter spelling in the signature is an interface binder, not a
CFG value. Each entry block argument has its own globally unique `Value` ID and
is the only corresponding value directly visible inside the entry block.

### 2.4 SSA Invariants

**Single Definition.** Every `Value` has exactly one definition site. The
validator rejects multiple definitions of the same `Value`.

Every block argument, including every entry block argument, owns a real
function-unique `Value` ID. Names are presentation only and do not determine
identity. Synthetic IDs derived from a block-argument index are forbidden.

**Use-Def Chains.** Every `Value` referenced as an operand must be defined by:

- A preceding instruction in the same block, or
- A block argument of the current block.

A `Value` used in block `B` cannot be defined in a different block unless
it is passed as a block argument through the terminator edges.

**Block Argument Arity.** For every edge from block `A` to block `B`, the
number and types of values passed by the terminator of `A` must match the
number and types of block arguments of `B`.

**Edge Binding.** Every value passed by an edge is used at the predecessor's
terminator and therefore must be defined locally in that predecessor. Each
target block argument is a distinct `Value` defined at the target block entry;
the edge binds the incoming value to that definition without sharing its
identity. All other uses are preceded by their definition within the same
block.

### 2.5 Block Reachability

Every block except the entry block must be reachable from the entry block
via some path of terminator edges. Dead blocks are permitted as an intermediate
state during transformations but must not appear in validated IR.

---

## 3. Instruction Set

### 3.1 Notation

```
%result = op.type_args operand1, ..., operandN
```

where:
- `%result` is a fresh `Value` name (omitted for terminators and store)
- `op` is the instruction mnemonic
- `type_args` encode instantiated type parameters (e.g. bit width)
- `operand1, ..., operandN` are `Value` references

### 3.2 Constants

```
%v = const.bits<N> <integer>
```

| Field | Description |
|---|---|
| `N` | Bit width (positive integer) |
| `<integer>` | Integer literal in decimal (`42`), hex (`0x2A`), or binary (`0b101010`). Ranges from `0` to `2^N - 1`. |

**Semantics.** Produces a `bits<N>` value with the given bit pattern. The
integer literal is interpreted as an unsigned value and truncated or
zero-extended to `N` bits as needed.

### 3.3 Integer Arithmetic

```
%v = add.bits<N> %a, %b
%v = sub.bits<N> %a, %b
%v = mul.bits<N> %a, %b
```

| Field | Description |
|---|---|
| `N` | Bit width. Must match the types of both operands and the result. |
| `%a`, `%b` | `bits<N>` operands. |

**Semantics.** Two's-complement addition, subtraction, and multiplication on
`N`-bit values with wrap-around overflow. These operations are total: they
never trap. The result is the mathematical result modulo `2^N`.

```text
add(x, y)  = (x + y) mod 2^N
sub(x, y)  = (x - y) mod 2^N
mul(x, y)  = (x * y) mod 2^N
```

```
%v = sdiv.bits<N> %a, %b
%v = udiv.bits<N> %a, %b
%v = srem.bits<N> %a, %b
%v = urem.bits<N> %a, %b
```

| Field | Description |
|---|---|
| `N` | Bit width. Must match the types of operands and result. |

**Semantics.**

`sdiv` performs signed two's-complement division. Traps if `%b == 0` or if
`(%a == INT_MIN and %b == -1)`. The result is truncated toward zero.

`udiv` performs unsigned division. Traps if `%b == 0`.

`srem` is the signed remainder satisfying `%a = (%a / %b) * %b + (%a % %b)`
with truncation toward zero. Traps if `%b == 0` or if `(%a == INT_MIN and
%b == -1)`.

`urem` is the unsigned remainder satisfying the same equation with unsigned
division. Traps if `%b == 0`.

A trap means the program terminates abnormally. The optimizer may not reorder
potentially trapping operations ahead of operations with observable side
effects unless the UB assumption guaranteeing non-trapping dominates that
reordering.

#### 3.3.1 Integer UB Predicates

The predicates used to guard division, remainder, and shifts are executable
`bits<1>` instructions, not annotations inferred by the interpreter:

```text
%nonzero = divisor_nonzero %divisor
%no_overflow = signed_div_rem_no_overflow %lhs, %rhs
%count_ok = shift_count_defined.signed %shifted, %amount
%count_ok = shift_count_defined.unsigned %shifted, %amount
%shl_ok = signed_shl_defined.signed %shifted, %amount
%shl_ok = signed_shl_defined.unsigned %shifted, %amount
```

The div/rem predicate operands must have matching widths. Shifted values and
shift amounts may have different widths after integer promotion:
`%shifted : bits<N>` and `%amount : bits<M>`. `divisor_nonzero` is false exactly
for a zero bit pattern. `signed_div_rem_no_overflow` is false exactly for signed
`MIN, -1`; it remains true for a zero divisor so that a separate assumption
selects `E3002` or `E3003`. `shift_count_defined` uses `N` as the bound and
decodes `%amount` at width `M` with the declared source signedness; it is true
exactly for mathematical counts in `[0, N)`. `signed_shl_defined` performs the
same independent-width decode and is false for an invalid count, a negative
left operand, or a shifted result not representable in signed `bits<N>`; it
never traps while computing the predicate.

### 3.4 Bitwise Operations

```
%v = and.bits<N> %a, %b
%v = or.bits<N> %a, %b
%v = xor.bits<N> %a, %b
%v = shl.bits<N> %a, %b
%v = lshr.bits<N> %a, %b
%v = ashr.bits<N> %a, %b
```

| Field | Description |
|---|---|
| `N` | Bit width of `%a` and the result. |
| `%b` | Shift amount of independently promoted type `bits<M>` for shifts; `bits<N>` for `and`, `or`, and `xor`. |

**Semantics.**

`and`, `or`, `xor`: bitwise logical operations on `N`-bit values. Total.

`shl`: shift left. `%b` is decoded at its own `M`-bit width. C
lowering must first assert `shift_count_defined` and, for signed left shift,
`signed_shl_defined` with their distinct reasons.

`lshr`: logical shift right. Zeros fill the vacated high bits.
C lowering must first assert `shift_count_defined`.

`ashr`: arithmetic shift right. The sign bit (bit `N-1` of `%a`) fills the
vacated high bits. C lowering must first assert `shift_count_defined`.

The raw shift instructions are total so the interpreter cannot panic on a
malformed or unguarded path. For an amount at least `N`, `shl` and `lshr`
produce zero; `ashr` produces all zeroes or all ones according to the sign bit.
Those fallback values are not C semantics: a valid C lowering dominates the
instruction with the typed invalid-count assumption.

### 3.5 Comparisons

```
%v = icmp.eq  %a, %b
%v = icmp.ne  %a, %b
%v = icmp.slt %a, %b
%v = icmp.sle %a, %b
%v = icmp.ult %a, %b
%v = icmp.ule %a, %b
```

| Field | Description |
|---|---|
| `%a`, `%b` | `bits<N>` operands of the same bit width. |

**Semantics.** Integer comparison producing a `bits<1>` value: `1` if the
condition is true, `0` otherwise.

`eq` / `ne`: bitwise equality / inequality.
`slt` / `sle`: signed two's-complement less-than / less-or-equal.
`ult` / `ule`: unsigned less-than / less-or-equal.

All comparison operations are total, pure, and never trap.

### 3.6 Extension and Truncation

```
%v = sext.bits<N> %a
%v = zext.bits<N> %a
%v = trunc.bits<N> %a
```

| Field | Description |
|---|---|
| `N` | Target bit width. Must be a positive integer. |

**Semantics.**

`sext`: sign-extends `%a` from its current width to `N` bits. The sign bit
(bit `W-1` where `W` is the source width) is replicated into the high `N-W`
bits. Requires `N >= W`; `N == W` is the identity conversion.

`zext`: zero-extends `%a` from its current width to `N` bits. The high `N-W`
bits are set to zero. Requires `N >= W`; `N == W` is the identity conversion.

`trunc`: truncates `%a` from its current width to `N` bits. The low `N` bits
are preserved. Requires `N <= W`; `N == W` is the identity conversion.

### 3.7 Pointer Operations

```text
%null = ptr.null : ptr<generic>
%global = global_addr @symbol : ptr<generic>
(%object, %m1, %w1) = alloca %m0, %w0 [origin=O, size=N, align=A]
(%m1, %w1) = lifetime.end %m0, %w0, %object
%v = ptr.offset.<add|sub>.<signed|unsigned> %base, %index [scale=S]
%m1 = pointer_valid %m0, %ptr
%m2 = access_in_bounds %m1, %ptr [size=N, align=A]
```

| Field | Description |
|---|---|
| `%base` | `ptr<AS>` value. |
| `%index` | `bits<N>` source index, interpreted by the mnemonic signedness. |
| `scale=S` | Positive target-derived element size in bytes. |
| `origin=O` | Stable nonnegative source-site ordinal used for cross-IR observations. |
| `size=N` | Access or allocation size in `1..=i64::MAX` bytes. |
| `align=A` | Positive power-of-two alignment representable by the target layout. |

`ptr.null` constructs the unique pointer with null provenance and offset zero.
`global_addr` returns the stable provenance of the named global object and is
pure. `alloca` allocates a fresh automatic object in the current function and
scope, returning a new `mem<all>` snapshot and forwarding the unchanged
abstract world state through a fresh token identity. Every dynamic execution
of `alloca`, including recursive calls and loop re-entry, receives a fresh
nonzero `ObjectId` and the next dynamic instance for origin site `O`.
Every byte of a newly allocated automatic object is uninitialized; only a
successful store initializes its destination bytes.
`lifetime.end` marks the addressed automatic object dead;
its tombstone and `ObjectId` remain in the new memory snapshot.
Returning from a function implicitly ends every still-live automatic object
owned by that invocation's frame, so an escaped pointer becomes dangling even
when explicit `lifetime.end` was not emitted on that exit.
The operand of `lifetime.end` must be the live base pointer (offset zero) of an
automatic object owned by the current frame. Validation rejects a statically
known non-base, static/string-literal, or foreign-frame operand. Runtime
checking remains fail closed: null reports `E3009`, an unknown object reports
`E3013`, an already-ended object reports `E3010`, and a live non-base,
non-automatic, or foreign-frame object reports `E3007 invalid-lifetime-end`.

`ptr.offset.<direction>.<signedness>` computes a pointer with the same
`ObjectId` as `%base`. A null base reports `E3009` immediately and produces no
noncanonical `(ObjectId(0), nonzero_offset)` value. Otherwise it decodes
`%index` according to the mnemonic, computes
`index * S` in unbounded mathematical arithmetic, then adds or subtracts that
delta according to `direction`. The final byte offset must fit signed 64-bit;
otherwise the instruction reports `E3011`. It does not access memory and may
construct a pointer outside
the object's valid range; C lowering must immediately emit `pointer_valid`
when formation of such a result is required to be defined.

`pointer_valid` and `access_in_bounds` are state-preserving, typed memory
guards. Each consumes the current `mem<all>` token and produces a fresh token
identity denoting the same abstract snapshot. A successful `pointer_valid`
requires a non-null, known, live object and an offset in the closed interval
`[0, object_size]`; the upper endpoint is a valid one-past pointer.
`access_in_bounds` additionally requires the complete half-open byte range
`[offset, offset + N)` to lie within the object and the offset to satisfy
alignment `A`: `object.align_bytes >= A` and `offset mod A == 0`. Failure
reports the exact memory diagnostic in Section 6.1.
Neither guard consumes or produces `world`.

`icmp.eq` and `icmp.ne` accept two values of the same pointer type and compare
both `ObjectId` and byte offset. Ordered pointer comparisons and subtraction
between pointers are outside the Day 7 profile.

The validator requires every Day 7 memory guard and memory operation to use
`mem<all>`. A load or store must consume the output of a matching
`access_in_bounds` guard for the same pointer, size, alignment, and current
snapshot. The interpreter rechecks the access even for forged IR, so omitting
or fabricating a guard never bypasses runtime safety.
In addition, every `ptr.offset.*.*` result must be consumed by the immediately
following `pointer_valid` instruction on the current memory token before that
pointer has any other use. The guard output advances the current memory token.
This is a validator invariant, not merely a lowering convention: an IR program
cannot form an out-of-range pointer, omit its formation obligation, offset it
back into range, and thereby erase the source C undefined behavior.

### 3.8 Memory Operations

```text
(%v, %m1, %w1) = load %m0, %w0, %ptr : Type
(%m1, %w1) = store %m0, %w0, %ptr, %val
(%v, %m1, %w1) = load volatile %m0, %w0, %ptr : Type
(%m1, %w1) = store volatile %m0, %w0, %ptr, %val
```

| Field | Description |
|---|---|
| `%m0` | `mem<R>` token defining the memory snapshot to read from or update. |
| `%w0` | `world` token before the operation. |
| `%m1` | `mem<R>` token after the operation. A load produces a distinct SSA token for the unchanged snapshot. |
| `%w1` | `world` token after the operation. For a non-observable access it denotes the same world state with a new SSA identity. |
| `%ptr` | `ptr<AS>` value. |
| `%val` | Value to store. |
| `Type` | The type being loaded. |
| `volatile` | Optional access marker; absence means ordinary access. |

**Semantics.**

`load(%m0, %w0, %ptr)`: returns the value of type `Type` stored at address
`%ptr`, plus explicitly threaded `%m1` and `%w1` tokens. `%m1` denotes the
same immutable memory snapshot as `%m0`. The load fails with `E3001` when any
requested byte is uninitialized, and otherwise uses the exact pointer,
lifetime, bounds, and alignment classification from Section 6.1.

`store(%m0, %w0, %ptr, %val)`: returns a new memory token `%m1` that is
identical to `%m0` except that the value at address `%ptr` has been replaced
with `%val` and the written bytes are initialized, plus the explicitly
threaded `%w1`. Storing and later loading a pointer preserves its complete
`ObjectId` provenance; a partial non-pointer overwrite of its object
representation makes a later pointer load fail with `E3013`.

For an ordinary access, `%w1` denotes the unchanged world state through a
fresh SSA identity. A successful volatile access appends exactly one ordered
load or store event, including pointer, width, and accessed value, to the world
trace represented by `%w1`. A failing volatile access appends no event. A
volatile load does not alter the memory snapshot, and volatile state is never
stored inside a memory token. Atomic accesses are outside Day 7.

### 3.9 Control Flow (Terminators)

```
br ^target(args...)
```

| Field | Description |
|---|---|
| `^target` | Target block label. |
| `args...` | Values to bind to the target block's arguments. |

**Semantics.** Unconditionally transfers control to `^target`, binding the
provided arguments to the target block's block arguments. The number and
types of `args` must exactly match the target block's block arguments.

```
cond_br %cond, ^then_target(then_args...), ^else_target(else_args...)
```

| Field | Description |
|---|---|
| `%cond` | `bits<1>` condition value. |
| `^then_target` | Block taken when `%cond != 0`. |
| `^else_target` | Block taken when `%cond == 0`. |

**Semantics.** If `%cond` is non-zero, transfers to `^then_target` with
`then_args`. Otherwise, transfers to `^else_target` with `else_args`. The
argument arities and types must match their respective target blocks.

```
ret values...
```

**Semantics.** Returns from the function with the given values. The number and
types of `values` must exactly match the function's declared return types.
For an effect-participant function, `ret` consumes the current `mem<all>` and
world tokens. Before crossing the function boundary it ends every still-live
automatic object owned by the current frame, yielding a finalized immutable
memory snapshot; the world snapshot is unchanged. The caller's `call` defines
fresh output token identities for those returned snapshots. Thus frame cleanup
is an explicit part of return-boundary semantics rather than a mutation hidden
behind a forwarded memory token.

### 3.10 Function Calls

```
(%ret_vals..., %m1, %w1) = call @callee(args..., %m0, %w0)
```

| Field | Description |
|---|---|
| `@callee` | Callee function name (symbol). |
| `args...` | Ordinary arguments whose types do not contain `mem<_>` or `world`, including inside tuples or vectors. |
| `%m0` | `mem<R>` token before the call. |
| `%w0` | `world` token before the call. |
| `%ret_vals...` | Ordinary return values whose types do not contain `mem<_>` or `world`. |
| `%m1` | `mem<R>` token after the call. |
| `%w1` | `world` token after the call. |

**Semantics.** Calls `@callee` with the provided arguments. The callee
receives the memory and world tokens and returns updated tokens. The types
of arguments and return values are derived from the callee's declaration.

**Call effects.** Day 7 applies the conservative memory/world effect summary in
Section 10.1.7 to every call. Attribute-refined summaries are outside this
profile.

---

## 4. Effect Model

### 4.1 Effect Summary

Every instruction carries an `EffectSummary` that describes its interaction
with memory and observable state:

```text
EffectSummary {
    reads:   RegionSet,     -- memory regions read
    writes:  RegionSet,     -- memory regions written
    volatile: bool,         -- is this a volatile access?
    atomic: bool,           -- is this an atomic access?
    io: bool,               -- does this interact with I/O?
    may_trap: bool,         -- can this instruction trap?
    may_unwind: bool,       -- can this instruction unwind the stack?
}
```

### 4.2 Effect Assignment by Instruction Category

| Instruction | reads | writes | volatile | atomic | io | may_trap | may_unwind |
|---|---|---|---|---|---|---|---|
| `const` | `{}` | `{}` | false | false | false | false | false |
| `uninitialized.bits<N>` | `{}` | `{}` | false | false | false | false | false |
| `add`, `sub`, `mul` | `{}` | `{}` | false | false | false | false | false |
| `sdiv`, `udiv`, `srem`, `urem` | `{}` | `{}` | false | false | false | true | false |
| integer UB predicates | `{}` | `{}` | false | false | false | false | false |
| `ub.assume.<reason>` | `{}` | `{}` | false | false | false | true | false |
| `and`, `or`, `xor` | `{}` | `{}` | false | false | false | false | false |
| `shl`, `lshr`, `ashr` | `{}` | `{}` | false | false | false | false | false |
| `icmp.*` | `{}` | `{}` | false | false | false | false | false |
| `sext`, `zext`, `trunc` | `{}` | `{}` | false | false | false | false | false |
| `ptr.null`, `global_addr` | `{}` | `{}` | false | false | false | false | false |
| `ptr.offset.*.*` | `{}` | `{}` | false | false | false | true | false |
| `alloca`, `lifetime.end` | `{All}` | `{All}` | false | false | false | true | false |
| `pointer_valid`, `access_in_bounds` | `{All}` | `{}` | false | false | false | true | false |
| `load` (normal) | `{All}` | `{}` | false | false | false | true | false |
| `store` (normal) | `{}` | `{All}` | false | false | false | true | false |
| `load volatile` | `{All}` | `{}` | true | false | false | true | false |
| `store volatile` | `{}` | `{All}` | true | false | false | true | false |
| `call` | depends on callee | depends on callee | depends | depends | depends | depends | depends |
| `trap.scalar_function_fell_off_end` | `{}` | `{}` | false | false | false | true | false |

Every Day 7 memory effect uses region `All`. Region refinement is not active.
The pointer guards inspect object metadata in the current snapshot and return
an unchanged snapshot under a fresh SSA identity.

### 4.3 World Token Semantics

The `world` token `%w` is a global ordering token. Day 6 reserves world input
and output positions on every memory instruction. A normal non-observable
access forwards the same abstract state through a distinct SSA token;
instructions that carry `volatile = true`, `atomic = true`, or `io = true`
use that token as an ordering constraint.

```text
; Volatile load pattern
(%val, %m1, %w1) = load volatile %m, %w, %ptr : Type
```

The world token acts as a **scheduling barrier**. Two operations that do not
touch the same region and do not touch `world` can be reordered with respect
to each other. Two operations that consume/produce `world` cannot be reordered
past each other.

The observable world state contains an ordered volatile trace. Each event
records a monotonically increasing sequence number, load/store direction, the
`(ObjectOrigin, byte_offset)` observation address, access width, and exact
accessed object representation. An implementation may additionally retain its
local `ObjectId`, but cross-IR equality never compares it. The C-HIR and EvoIR
interpreters must expose the canonical origin-based trace in their outcomes;
equal scalar returns with unequal volatile traces are not semantically
equivalent.
Bit-vector event values use exact target-width raw bits. Pointer event values
are canonicalized to `(ObjectOrigin, byte_offset)` rather than local ObjectId
object-representation bits, so valid provenance observations remain comparable
between the two interpreters.

### 4.4 Reordering Rules

Two instructions `I1` and `I2` where `I1` dominates `I2` in program order may
be reordered (I2 scheduled before I1) if and only if:

1. `I1.writes ∩ I2.reads = {}` (no write-after-read hazard)
2. `I1.reads ∩ I2.writes = {}` (no read-after-write hazard)
3. `I1.writes ∩ I2.writes = {}` (no write-after-write hazard)
4. Neither `I1` nor `I2` touches `world`
5. Neither `I1` nor `I2` may trap ahead of a volatile/IO operation

### 4.5 E-Class Effect Invariant

> **An e-class may never contain representations with different semantic result
> types or incompatible observable-effect contracts.**

This is a hard invariant enforced by the equality engine. A pure integer
expression and a volatile load can never be alternatives, even if they happen
to produce the same integer value in some execution. The effect summary is
part of the sort/type of the e-node and participates in hash-consing.

The Day 7 scalar P0 e-graph bridge admits only total, pure scalar nodes. Every
instruction that carries `mem<all>` or `world`, every pointer operation, every
guard, every may-trap instruction, and every value transitively depending on
one of them is ineligible and remains byte-for-byte outside the e-graph region.
`uninitialized.bits<N>` and every value transitively depending on scalar poison
are likewise ineligible. P0 therefore cannot erase, advance, or delay an
`E3001` by applying an identity such as `x & 0` or an identity comparison to a
poison-dependent value.

---

## 5. Memory Model

### 5.1 Functional Memory

Memory is represented as a functional state threaded through operations.
Conceptually:

```text
load(Mem, World, Ptr) -> (Value, Mem', World')
store(Mem, World, Ptr, Val) -> (Mem', World')
```

Memory and world tokens are immutable values. Neither operation mutates its
input tokens; each produces fresh SSA token identities. For a normal load,
`Mem'` and `World'` denote unchanged abstract states. A store's `Mem'` denotes
the updated state, while its `World'` remains an explicitly threaded ordering
state.

### 5.2 Objects, Storage Duration, and Lifetime

One immutable memory snapshot contains an ordered object table:

```text
ObjectId -> Object {
    origin,             ; stable source site plus dynamic per-site instance
    size_bytes,
    align_bytes,
    owner,              ; static, string literal, or automatic frame/scope
    lifetime,           ; alive or an explicit ended tombstone
    bytes,
    initialized_bitmap,
    pointer_provenance_fragments,
}
```

Allocation returns the next positive `ObjectId`; zero is permanently reserved
for null and no identifier is reused. Ending a scope or frame lifetime
marks its objects dead in a new snapshot but retains their tombstones. A stale
pointer therefore remains deterministically dangling and can never become a
pointer to a later allocation. Static and string-literal objects are allocated
once during module initialization and live for the whole execution.
String-literal objects are read-only; an attempted store reports `E3014`.

`ObjectId` is an interpreter-local provenance identity and is not compared
across different IR executions. `ObjectOrigin` is the cross-IR observation
identity: it consists of a stable source-site ordinal and a monotonically
increasing per-site dynamic instance. Globals and string literals use instance
zero; each execution of an automatic allocation advances only that site's
instance counter. C-HIR and EvoIR lowering preserve the same source-site
ordinal even when unrelated scalar locals remain in SSA, so materializing or
eliding such locals cannot perturb volatile-trace comparison.

Every object has an exact target-derived size in `1..=i64::MAX` and a
target-layout-representable power-of-two alignment.
Object bytes and one initialized bit per byte are distinct state. A store marks
exactly its destination range initialized. A load of any range containing an
uninitialized byte reports `E3001`; merely allocating, copying, passing, or
branching with an uninitialized object does not report a diagnostic.

### 5.3 Object Representations and Pointer Provenance

Integer object representations use the target's explicit byte order and never
the host's native byte order. Pointer values are stored as target-width object
representations plus out-of-band provenance fragments. Copying every byte of a
pointer representation preserves the complete `(ObjectId, byte_offset)`;
overwriting only part of that representation invalidates pointer
reconstruction and a later pointer load reports `E3013`.

Two pointers compare equal exactly when both components are equal. Distinct
`ObjectId` values remain distinct even if an implementation chooses identical
abstract address bits for their object representations. Provenance is never
derived from a host address. `ptrtoint`, integer-to-pointer conversion, and
backend provenance erasure are outside Day 7.

### 5.4 Pointer and Access Classification

Memory faults use this deterministic precedence:

1. `ObjectId(0)` is a null access.
2. An absent or forged `ObjectId` in the pointer used as the access address is
   invalid provenance.
3. A tombstoned object is a dangling access.
4. An offset outside `[0, object_size]`, checked arithmetic overflow, or an
   access range outside `[0, object_size)` is out of bounds.
5. An access is misaligned unless `object.align_bytes >= required_alignment`
   and `byte_offset mod required_alignment == 0`.
6. A store to string-literal storage is a write to a read-only object.
7. A load whose requested byte range is not fully initialized is an
   uninitialized read.
8. After an initialized pointer-sized range has been read, a pointer result
   whose bytes do not have one exact provenance fragment is an invalid pointer
   representation.

Steps 1--6 validate the address and operation. A load then checks the
initialized bitmap before reconstructing its result. Consequently an
uninitialized pointer object reports `E3001`, not `E3013`; `E3013` applies only
after all loaded bytes are initialized but their pointer provenance shadow is
missing, partial, or malformed.

The one-past pointer at `offset == object_size` passes `pointer_valid` but any
positive-width access through it fails `access_in_bounds`. Every failed memory
operation is transactional: it changes no bytes, initialized bits, lifetimes,
identifier counters, memory snapshots, or world trace.

### 5.5 Day 7 Memory-Optimization Boundary

Alias analysis, region splitting, store forwarding, store bypass, redundant
load elimination, and every rewrite that depends on `MustAlias` or `NoAlias`
are strictly outside Day 7. The only Day 7 e-graph input is a pure, total scalar
region that carries neither `mem<all>` nor `world`; memory and pointer nodes are
never inserted. These future optimizations require separately scheduled facts
and proof rules and do not become valid merely because the functional memory
model can describe them.

---

## 6. Undefined Behavior

### 6.1 Explicit UB Model

C undefined behavior is made explicit in EvoIR through typed
`ub.assume.<reason>` instructions:

```text
%ok = signed_add_defined %x, %y
ub.assume.signed_overflow %ok       ; false reports E3004
%z = add.bits %x, %y                ; total bit-vector operation
```

The `ub.assume.<reason>` instruction declares a runtime condition and carries
the source-level UB class explicitly. If the condition is false at runtime,
the program has undefined behavior, the interpreter reports the diagnostic
selected by `<reason>`, and the compiler may assume the condition is true for
all reachable executions. The reason is semantic metadata, not an inference
from the condition's `bits<1>` type.

The Day 6 integer reasons are fixed as follows:

| Reason suffix | Diagnostic | Runtime message |
|---|---|---|
| `division_by_zero` | `E3002` | `integer-division-by-zero` |
| `remainder_by_zero` | `E3003` | `integer-remainder-by-zero` |
| `signed_overflow` | `E3004` | `signed-integer-overflow` |
| `invalid_shift_count` | `E3005` | `invalid-shift-count` |
| `invalid_signed_left_shift` | `E3006` | `invalid-signed-left-shift` |

For operands of type `bits<N>`, `signed_add_defined`,
`signed_sub_defined`, and `signed_mul_defined` each have type
`(bits<N>, bits<N>) -> bits<1>`. They are true exactly when the corresponding
mathematical result is representable by the signed `N`-bit C result type.

Memory obligations are deliberately not encoded as `bits<1>` plus one generic
`ub.assume` reason. Such an encoding would erase whether a dynamic failure was
null, dangling, out of bounds, misaligned, or an invalid pointer
representation. The state-aware `pointer_valid` and `access_in_bounds` guards
in Section 3.7 instead inspect the named immutable snapshot, preserve it under
a fresh `mem<all>` identity, and report the exact fault below.

| Diagnostic | Runtime message | Meaning |
|---|---|---|
| `E3001` | `read-of-uninitialized-object` | At least one loaded byte is uninitialized. |
| `E3009` | `null-pointer-access` | The accessed pointer has `ObjectId(0)`. |
| `E3010` | `dangling-pointer-access` | The named object's lifetime has ended. |
| `E3011` | `out-of-bounds-memory-access` | Pointer formation or the complete access range is outside its object, or checked offset arithmetic overflowed. |
| `E3012` | `misaligned-memory-access` | The byte offset does not satisfy the access alignment. |
| `E3013` | `invalid-pointer-representation` | The provenance is absent/forged or stored pointer bytes lack one complete provenance representation. |
| `E3014` | `write-to-read-only-object` | A store targets string-literal storage. |

Both interpreters use the deterministic classification order from Section 5.4
and delegate code/message selection to the shared typed memory-fault authority;
they may not maintain divergent local mapping tables. C-HIR attaches the source access span;
EvoIR attaches its block identity and the driver reports the established
source-independent `1:1` location until a source map is specified.

### 6.2 UB Obligations

| Obligation | Condition | Lowered Pattern |
|---|---|---|
| Signed addition overflow | `x + y` is not representable | `signed_add_defined(x, y)` → `ub.assume.signed_overflow` → `add` |
| Signed subtraction overflow | `x - y` is not representable | `signed_sub_defined(x, y)` → `ub.assume.signed_overflow` → `sub` |
| Signed multiplication overflow | `x * y` is not representable | `signed_mul_defined(x, y)` → `ub.assume.signed_overflow` → `mul` |
| Signed division/remainder overflow | `x == INT_MIN && y == -1` | `signed_div_rem_no_overflow(x, y)` → `ub.assume.signed_overflow` → signed div/rem |
| Division by zero | `y == 0` | `divisor_nonzero(y)` → `ub.assume.division_by_zero` → `sdiv`/`udiv` |
| Remainder by zero | `y == 0` | `divisor_nonzero(y)` → `ub.assume.remainder_by_zero` → `srem`/`urem` |
| Shift amount invalid for width | mathematical `shift_amt` is outside `[0, N)` | `shift_count_defined.<signedness>(x, amt)` → `ub.assume.invalid_shift_count` → shift |
| Invalid signed left shift | signed `x << y` is not representable | `signed_shl_defined.<signedness>(x, y)` → `ub.assume.invalid_signed_left_shift` → `shl` |
| Pointer arithmetic validity | result must name a live object at an offset in `[0,size]` | `ptr.offset.*.*` → `pointer_valid` |
| Memory access validity | complete positive-width range and alignment must be valid in the current snapshot | `access_in_bounds` → matching `load`/`store` |
| Null pointer access | pointer has `ObjectId(0)` | memory guard or direct fail-closed access reports `E3009` |
| Dangling pointer access | pointer names a tombstoned object | memory guard or direct fail-closed access reports `E3010` |
| Out-of-bounds access | pointer/range lies outside its object | memory guard or direct fail-closed access reports `E3011` |
| Misaligned access | offset violates the target-required alignment | memory guard or direct fail-closed access reports `E3012` |
| Write to string-literal storage | store targets a read-only object | `store` reports `E3014` |
| Use of uninitialized scalar SSA value | consume a value defined by `uninitialized.bits<N>` | Day 6 interpreter reports `E3001` at the consuming instruction or terminator |
| Read from uninitialized memory storage | any requested initializedness bit is false | `load` reports `E3001` at the load, never allocation or token transport |
| Invalid pointer representation/provenance | pointer bytes or ObjectId are not valid | pointer load/guard/access reports `E3013` |
| Strict aliasing/effective type | access through incompatible effective type | deferred with alias analysis beyond Day 7 |

`uninitialized.bits<N>` defines an ordinary poison value with a real SSA
identity. Branch operands and block arguments may copy that poison without
trapping, which lets definite assignment on a later path overwrite it. Any
semantic consumption—arithmetic, comparison, conversion, branch condition,
ordinary call argument, or return—reports `E3001 read-of-uninitialized-object`.
It is not a zero value and it must never be silently replaced by one. Memory
initializedness uses the separate per-byte object bitmap and follows the same
consume-time rule: transporting a pointer or memory token does not trap; the
executed load does.

### 6.3 UB and Optimization

A `ub.assume.<reason> %ok` instruction creates a fact `ok == true` that is available
in all dominated blocks. The EqSat engine uses this fact:

```text
rule signed_add_associative
    add(add(?a, ?b), ?c)
 => add(?a, add(?b, ?c))
requires
    signed_add_defined(?a, ?b) && signed_add_defined(?a + ?b, ?c)
```

Without the UB assumption, `(a+b)+c` and `a+(b+c)` differ for signed
two's-complement overflow — their overflow behavior may differ even though
the `add.bits` results are equal. The UB assumption makes them semantically
equivalent.

### 6.4 Propagation Rules

UB facts propagate forward. An `ub.assume.<reason> %ok` in block `B` creates a fact
that holds in every block dominated by `B` along paths where `%ok` was true.
This is expressed in the equality context `Γ` (Section 7).

A successful memory guard creates a fact keyed by its output `mem<all>` value,
pointer, and (for `access_in_bounds`) exact size/alignment. Because every later
memory effect advances the token, a fact cannot be silently reused against a
different snapshot. Day 7 validation uses these facts to require a matching
guard before each load/store; the Day 7 scalar P0 e-graph does not consume
memory facts.

---

## 7. Equality Criterion

### 7.1 The Central Judgment

```
Γ ⊢ A ≡ B : τ
```

Meaning: under context `Γ`, the EvoIR terms `A` and `B` have the same
semantic value **and** the same observable effects.

This is stronger than "same bit pattern." Two terms with the same result
value but different memory side effects are **not** equal. Two terms that
produce the same value but trap under different conditions are **not** equal.

### 7.2 Context Γ

`Γ` is a set of facts known at a particular program point. It includes:

| Fact kind | Example | Source |
|---|---|---|
| Branch conditions | `%cond != 0` | `cond_br` true edge |
| Non-zero facts | `%x != 0` | Derived from comparisons |
| Range facts | `%x ∈ [0, 100]` | Loop bounds, comparisons |
| Alignment facts | `%p` is 8-byte aligned | `alloca` / `access_in_bounds` |
| Provenance facts | `%p` has one automatic object's `ObjectId` | `alloca` |
| UB assumptions | `signed_add_defined(%x, %y)` | `ub.assume.signed_overflow` |
| No-overflow facts | `add_defined(%x, %y)` | UB assumption propagation |
| Alias facts (post-Day 7) | `NoAlias(%p, %q)` | Future alias analysis |

### 7.3 Equality and E-Classes

Two e-nodes may be placed in the same e-class only if:

1. They have the same result type (including bit width)
2. They have compatible effect summaries (reads the same regions, writes the
   same regions, same volatile/atomic/io/trap/unwind flags)
3. In context `Γ`, they produce the same value for all defined executions

The type check is structural. The effect check uses the effect lattice.
The value check is the semantic equality judgment.

### 7.4 Congruence

EvoIR equality is a congruence relation. If:

```
Γ ⊢ A ≡ A' : τ_A
Γ ⊢ B ≡ B' : τ_B
```

then for any operation `op(A, B)`:

```
Γ ⊢ op(A, B) ≡ op(A', B') : τ_result
```

This is the closure rule that drives e-graph rebuilding.

### 7.5 Extraction and Equality

At extraction time, the extractor may substitute any e-node in an e-class
for any other e-node in the same e-class, provided that the substitution
respects:

- Dominance constraints
- Effect ordering constraints
- The requirement that shared subexpressions remain shared or are explicitly
  duplicated with known cost

---

## 8. Persistent Equality Database

**Day 7 P0 profile.** Persistent `ValueId`/`EClassId` identity begins after
Day 7. The first bridge uses one deterministic, ephemeral e-graph per eligible
pure scalar region and discards it after extraction. Its operator authority
binds every operator, constant payload, bit width, arity, and pure/no-trap
effect class; distinct input values and distinct constants can never hash-cons
merely because they share a nullary opcode.

The P0 rules are limited to typed constant folding, `x + 0`, `x * 1`,
`x & 0`, `x | 0`, double arithmetic negation encoded exactly as
`sub.bits<N>(0, sub.bits<N>(0, x)) -> x`, and identity comparisons encoded as
`icmp.eq(x, x) -> const.bits<1> 1` and
`icmp.ne(x, x) -> const.bits<1> 0`. Each pattern requires identical operand
types and the poison/effect exclusions below. A deterministic
greedy extractor accepts only a strict cost improvement and prefers the
original on ties. Explicit node and epoch budgets bound every run. Budget,
matching, extraction, validation, or proof failure returns the byte-identical
original EvoIR and continues compilation; equality saturation is never needed
for correctness.

Every accepted merge is present in a complete proof certificate whose trusted
operator/rule authority, canonical input-module digest, exact input seed
manifest, region identity, extracted output, and full event prefix are bound
by the receipt. `evocc-proof` receives the canonical input module, verifies its
digest, and independently recomputes the complete eligible region and seed
manifest from that module rather than trusting the receipt's enumeration. It
replays every merge, verifies that the extracted root belongs to the original
root e-class, reconstructs the extracted region, and compares its canonical
bytes and output commitment to the receipt. A merge-valid certificate that
names an unrelated root or output is invalid. Any node
carrying or transitively depending on `mem<all>`, `world`, a pointer, a guard,
another may-trap/effectful instruction, or scalar poison is rejected before
insertion. Poison dependence is computed before region formation and is not
re-derived from the extracted expression.

The exact driver capability is `-Oeq-scalar`, and only
`-Oeq-scalar --probe` becomes implemented with this bridge. The broader
`-Oeq --probe` remains exit 69 `NOT_IMPLEMENTED`.

Sections 8.1 through 8.4 describe the later persistent architecture. They are
not authorization to introduce persistent `EClassId` mappings in Day 7.

### 8.1 Architecture

The equality database persists `EClassId`s across lowering phases. It is not
a pass-local temporary data structure; it is the persistent state of compiler
knowledge.

```text
C-HIR lowering        adds EvoIR representations to existing EClasses
EqSat optimization    adds alternative EvoIR representations
Target lowering       adds machine-specific representations
```

EvoIR value IDs map to EClassIds. Lowering from C-HIR to EvoIR creates new
EvoIR representations and adds them to existing EClasses. Target instruction
selection adds machine-specific representations to the same classes.

### 8.2 Core Data Structures

```text
EClass {
    id: EClassId,
    nodes: [ENodeId],
    facts: AnalysisFacts,
    parents: [ParentUse],
}

ENode {
    op: OpId,
    children: [EClassId],
    sort: SortId,
    attrs: AttrSetId,
}

EquivDb {
    revision: u64,

    classes: Arena<EClass>,
    nodes: Arena<ENode>,

    union_find: UnionFind<EClassId>,
    hashcons: Map<ENodeKey, EClassId>,
    dirty: Queue<EClassId>,

    proofs: ProofDb,

    evo_value_to_class: Map<ValueId, EClassId>,
    mir_value_to_class: Map<MirValueId, EClassId>,
}
```

### 8.3 Multi-Level Representations

The two maps at the bottom of `EquivDb` are the bridge between IR layers:

```text
evo_value_to_class:  "EvoIR value %v42 belongs to e-class C17"
mir_value_to_class:  "EvoMIR value %r3 belongs to e-class C17"
```

When `add(x, shl(y, 3))` is lowered toward AArch64, EvoCC adds a target node:

```text
a64.add_shifted(x, y, LSL 3)
```

to the **same semantic class** instead of deleting the generic representation.
Extraction can then choose between:

```text
lsl xTmp, y, #3        ; two-instruction expansion
add dst, x, xTmp

add dst, x, y, lsl #3  ; single fused instruction
```

based on target cost, register pressure, and legality.

### 8.4 Incremental Rebuilding

The equality database supports incremental updates:

```text
revision number     -- monotonically increasing, tracks change epochs
dirty queue         -- e-classes needing congruence repair
analysis queue      -- e-classes needing analysis recomputation
match index         -- per-rule index of active matches
dependency index    -- which e-classes depend on which facts
```

After a constructive compiler transformation (e.g., lowering a call to an
inline body), only the affected regions are resaturated rather than rebuilding
the entire e-graph from scratch.

---

## 9. Textual Syntax

### 9.1 Grammar

The EvoIR textual syntax is a human-readable format used in:

- Test fixtures
- `evo-run` interpreter input
- `--emit=evoir` compiler output
- Debugging and inspection

```text
global @name origin=O storage=KIND size=N align=A bytes=HEX initialized=BITS

func @name(%param1: Type1, ..., %paramN: TypeN, %mem: mem<R>, %w: world)
    -> (RetType1, ..., RetTypeK, mem<R>, world)
    cc = calling_convention
{
^block_label(%arg1: Type1, ...):
    %value = op.type_args operand1, ..., operandN
    %value = uninitialized.type
    %null = ptr.null : ptr<generic>
    %global = global_addr @name : ptr<generic>
    (%ptr, %mem1, %world1) = alloca %mem0, %world0 [origin=O, size=N, align=A]
    (%mem1, %world1) = lifetime.end %mem0, %world0, %ptr
    %mem1 = pointer_valid %mem0, %ptr
    %mem2 = access_in_bounds %mem1, %ptr [size=N, align=A]
    (%value, %mem3, %world1) = load [volatile] %mem2, %world0, %ptr : Type
    (%mem3, %world1) = store [volatile] %mem2, %world0, %ptr, %value
    ub.assume.<reason> %condition
    terminator

^block_label2(%arg1: Type1, ...):
    ...
}

<reason> := division_by_zero | remainder_by_zero | signed_overflow
          | invalid_shift_count | invalid_signed_left_shift
```

### 9.2 Lexical Rules

- Lines starting with `^` are block labels
- `%name` is a value reference or definition
- `%name = op args` is an instruction definition
- `%name = uninitialized.bits<N>` defines transportable scalar poison; it does
  not trap until semantically consumed
- A global's `KIND` is exactly `static` or `string_literal`; its `HEX` is
  exactly two lowercase hexadecimal digits per byte and
  `BITS` is exactly one `0` or `1` per byte; both lengths equal `size=N`, with
  `N` in `1..=i64::MAX`, and `align=A` is a target-layout-representable positive
  power of two. Every global and every `alloca` instruction site has one
  module-unique `origin=O`; `O` is the stable source-site ordinal used by
  cross-IR observations
- `@name` is a module symbol; `call` resolves a function and `global_addr`
  resolves a global object, and using the wrong symbol kind is invalid
- `ptr.null`, `global_addr`, `alloca`, `lifetime.end`, `pointer_valid`, and
  `access_in_bounds` have the exact Day 7 forms above
- `ptr.offset` has exact direction and index-signedness suffixes plus a
  positive `scale`; a bare `ptr.offset` is malformed
- The optional literal `volatile` appears only immediately after `load` or
  `store`; separate `vload`/`vstore` mnemonics are not valid syntax
- Every load/store memory operand is the output of a matching
  `access_in_bounds` for that pointer and target-derived size/alignment
- `ub.assume.<reason> %condition` is a typed UB obligation; the reason suffix
  is mandatory and `%condition` has type `bits<1>`
- `trap.scalar_function_fell_off_end` is a zero-operand CFG terminator that
  reports `E3007 scalar-function-fell-off-end`; it carries no placeholder
  return value or effect token
- Types are delimited by angle brackets and parentheses: `bits<32>`, `ptr<generic>`, `mem<all>`, `tuple<...>`
- Comments start with `;` and continue to end of line
- Block comments are not supported in textual syntax
- Whitespace separates tokens; indentation is not significant

### 9.3 Complete Example

```text
func @sum(%p: ptr<generic>, %n: bits<64>, %m: mem<all>, %w: world)
    -> (bits<32>, mem<all>, world)
    cc = target_default
{
^entry(%p_entry: ptr<generic>, %n_entry: bits<64>, %m_entry: mem<all>, %w_entry: world):
    %zero = const.bits<64> 0
    %acc  = const.bits<32> 0
    br ^loop(%p_entry, %n_entry, %zero, %acc, %m_entry, %w_entry)

^loop(%p_loop: ptr<generic>, %n_loop: bits<64>, %i_loop: bits<64>, %acc_loop: bits<32>, %m_loop: mem<all>, %w_loop: world):
    %more = icmp.ult %i_loop, %n_loop
    cond_br %more, ^body(%p_loop, %n_loop, %i_loop, %acc_loop, %m_loop, %w_loop), ^exit(%acc_loop, %m_loop, %w_loop)

^body(%p_body: ptr<generic>, %n_body: bits<64>, %i_body: bits<64>, %acc_body: bits<32>, %m_body: mem<all>, %w_body: world):
    %addr = ptr.offset.add.unsigned %p_body, %i_body [scale=4]
    %m_pointer = pointer_valid %m_body, %addr
    %m_access = access_in_bounds %m_pointer, %addr [size=4, align=4]
    (%x, %m_loaded, %w_loaded) = load %m_access, %w_body, %addr : bits<32>
    %acc1 = add.bits<32> %acc_body, %x
    %one  = const.bits<64> 1
    %i1   = add.bits<64> %i_body, %one
    br ^loop(%p_body, %n_body, %i1, %acc1, %m_loaded, %w_loaded)

^exit(%r_exit: bits<32>, %m_exit: mem<all>, %w_exit: world):
    ret %r_exit, %m_exit, %w_exit
}
```

### 9.4 UB-Assume Example

```text
func @checked_add(%x: bits<32>, %y: bits<32>, %m: mem<all>, %w: world)
    -> (bits<32>, mem<all>, world)
    cc = target_default
{
^entry(%x_entry: bits<32>, %y_entry: bits<32>, %m_entry: mem<all>, %w_entry: world):
    %ok = signed_add_defined %x_entry, %y_entry
    ub.assume.signed_overflow %ok
    %z = add.bits<32> %x_entry, %y_entry
    ret %z, %m_entry, %w_entry
}
```

### 9.5 Call Example

```text
func @caller(%a: bits<32>, %m0: mem<all>, %w0: world)
    -> (bits<32>, mem<all>, world)
    cc = target_default
{
^entry(%a_entry: bits<32>, %m_entry: mem<all>, %w_entry: world):
    (%result, %m1, %w1) = call @callee(%a_entry, %m_entry, %w_entry)
    ret %result, %m1, %w1
}
```

### 9.6 Syntax Restrictions

- Value names (`%name`) are local to a function. They are not visible across
  function boundaries.
- Names in a function signature are non-SSA interface binders. Only the
  positionally corresponding entry block arguments are CFG values.
- A value name must be defined before its first use in any block, respecting
  block argument definitions.
- Block labels (`^name`) are local to a function.
- Function names (`@name`) are global symbols.
- A `ub.assume` without a reason suffix is malformed. The Day 6 accepted
  suffixes are `division_by_zero`, `remainder_by_zero`, `signed_overflow`,
  `invalid_shift_count`, and `invalid_signed_left_shift`.
- The entry block must be named `^entry` (convention) and must be the first
  block in the function body.
- Terminators must appear as the last instruction in a block.

---

## 10. Cross-IR Contracts

### 10.1 C-HIR to EvoIR Lowering Contract

The lowering agent transforms C-HIR into EvoIR. The following contracts define
the semantics of that transformation and are binding on the lowering agent.

#### 10.1.1 Value Correspondence

Every C-HIR expression lowers to an EvoIR value of the corresponding
`EvoIRType`. The mapping is:

| C Type | EvoIR Type |
|---|---|
| `int`, `unsigned int`, `_Bool`, `char`, etc. | `bits<N>` where `N` is the target width in bits |
| `float` | `f32` |
| `double` | `f64` |
| `T*` | `ptr<generic>` in the Day 7 profile; storage duration remains object metadata |
| `void` (in return position) | `void` (empty return type) |

The mapping is mediated by the target data-layout service. The current pinned
AArch64 implementation is `TargetDataLayout`, which provides `layout_of`,
`pointer_layout`, and `record_layout` queries; lowering and interpreters must
not hard-code target widths, pointer size, or pointer alignment.

#### 10.1.2 Qualifier Lowering

| C Qualifier | EvoIR Representation |
|---|---|
| `const` | No IR effect. Purely a C-HIR check. |
| `volatile` | Exact `load volatile` / `store volatile` syntax and ordered world-trace events. |
| `restrict` | Preserved in C-HIR only; `NoAlias` facts are outside Day 7. |
| `_Atomic` | Typed unsupported in Day 7; atomic operations remain reserved. |

#### 10.1.3 Lvalue Lowering

C lvalues lower to EvoIR pointer operations:

```text
Read:   %ptr = ptr.offset.add.unsigned %base, %field_offset [scale=1]
        %m1 = pointer_valid %m0, %ptr
        %m2 = access_in_bounds %m1, %ptr [size=S, align=A]
        (%val, %m3, %w1) = load %m2, %w0, %ptr : Type

Write:  %ptr = ptr.offset.add.unsigned %base, %field_offset [scale=1]
        %m1 = pointer_valid %m0, %ptr
        %m2 = access_in_bounds %m1, %ptr [size=S, align=A]
        (%m3, %w1) = store %m2, %w0, %ptr, %val
```

The base pointer and field offset are computed from the field placements
returned by `TargetDataLayout::record_layout`.

Addressable automatic objects use `alloca`; globals, static locals, and string
literals use module globals plus `global_addr`. Non-address-taken scalar locals
may remain in SSA, preserving the Day 6 scalar lowering. Array indexing passes
the target element size as `scale` and preserves the promoted index's
signedness in the `ptr.offset` mnemonic. Every
source-level pointer-arithmetic result is followed by `pointer_valid`, and every
executed dereference is immediately preceded by a matching
`access_in_bounds`. Scope and frame exits end automatic lifetimes.

Before either interpretation or scalar replacement, C-HIR derives one shared
allocation-site manifest over allocation-bearing semantic objects, not raw
declaration nodes. A file-scope/linkage object appears exactly once, keyed by
its merged `SymbolId`; its source anchor is the initializer-bearing definition,
otherwise the first tentative definition. An extern-only declaration has no
site until a same-TU definition supplies storage. Each automatic or static
local that supplies storage appears once under its object symbol; a block-scope
`extern` resolves to the linked object rather than creating a site. Each
named parameter of a function definition is an automatic-object site keyed by
its `ParameterDecl.symbol`; parameters that occur only in prototypes have no
runtime site. Each string-literal expression occurrence is a separate site.
The manifest sorts
these canonical anchors in deterministic source order and assigns distinct
module-wide ordinals. It includes eligible scalar locals even when lowering
later keeps them in SSA and never allocates them. The C-HIR interpreter and
EvoIR lowering consume this same manifest for `ObjectOrigin.site`; neither may
derive origins from raw declaration count or its own materialized-object
sequence.

#### 10.1.4 Sequencing Lowering

C sequence points lower to memory token threading and world token threading.
The lowered EvoIR must preserve the observable ordering of operations as
defined by the C abstract machine:

```text
; x = y + z;
; w = load *p;

%tmp = add %y, %z                                      ; first expression
%m1 = pointer_valid %m0, %x_addr
%m2 = access_in_bounds %m1, %x_addr [size=Sx, align=Ax]
(%m3, %w1) = store %m2, %w0, %x_addr, %tmp
%m4 = pointer_valid %m3, %ptr
%m5 = access_in_bounds %m4, %ptr [size=Sw, align=Aw]
(%val, %m6, %w2) = load %m5, %w1, %ptr : Tw           ; sees the store
```

#### 10.1.5 Undefined Behavior Lowering

C operations with undefined behavior lower to a typed `ub.assume.<reason>`
before a total operation:

| C Expression | EvoIR Pattern |
|---|---|
| `x + y` (signed) | `ub.assume.signed_overflow signed_add_defined(x, y)` → `add.bits<N> x, y` |
| `x - y` (signed) | `ub.assume.signed_overflow signed_sub_defined(x, y)` → `sub.bits<N> x, y` |
| `x * y` (signed) | `ub.assume.signed_overflow signed_mul_defined(x, y)` → `mul.bits<N> x, y` |
| `x / y` (signed) | `divisor_nonzero(y)` → `ub.assume.division_by_zero`; `signed_div_rem_no_overflow(x, y)` → `ub.assume.signed_overflow`; then `sdiv.bits<N> x, y` |
| `x / y` (unsigned) | `divisor_nonzero(y)` → `ub.assume.division_by_zero` → `udiv.bits<N> x, y` |
| `x % y` (signed) | `divisor_nonzero(y)` → `ub.assume.remainder_by_zero`; `signed_div_rem_no_overflow(x, y)` → `ub.assume.signed_overflow`; then `srem.bits<N> x, y` |
| `x % y` (unsigned) | `divisor_nonzero(y)` → `ub.assume.remainder_by_zero` → `urem.bits<N> x, y` |
| any shift | `shift_count_defined.<signedness>(x, y)` → `ub.assume.invalid_shift_count` → shift operation |
| signed `x << y` | count obligation; `signed_shl_defined.<signedness>(x, y)` → `ub.assume.invalid_signed_left_shift`; then `shl.bits<N> x, y` |
| read of an uninitialized scalar local | `uninitialized.bits<N>` is threaded through SSA until a semantic consumer reports `E3001` |
| reachable end of a non-`main`, non-void function | `trap.scalar_function_fell_off_end` reports `E3007` |
| pointer arithmetic | `ptr.offset.<direction>.<signedness>` with target scale → `pointer_valid` on current `mem<all>` |
| `*p` (read) | `access_in_bounds` with target size/alignment → `load` |
| `*p = v` (write) | `access_in_bounds` with target size/alignment → `store` |
| `p[i]` | typed/scaled `ptr.offset.add.*` → `pointer_valid` → access guard → load/store |

The lowering agent must not elide UB checks. The EqSat agent may later prove
them redundant and eliminate them.

#### 10.1.6 Aggregate Lowering

C struct and union types lower to explicit offset computations:

```text
; struct { int a; char b; } s;
; s.b = 42;

%off_b  = const.bits<64> 4
%val    = const.bits<8> 42
%b_addr = ptr.offset.add.unsigned %s_base, %off_b [scale=1]
%m1 = pointer_valid %m0, %b_addr
%m2 = access_in_bounds %m1, %b_addr [size=1, align=1]
(%m3, %w1) = store %m2, %w0, %b_addr, %val               ; store to s.b
```

Bitfields lower to load-modify-store sequences using bitwise operations:

```text
; struct { unsigned f : 3; } s;
; s.f = 5;

%m1 = access_in_bounds %m0, %s_addr [size=1, align=1]
(%old, %m2, %w1) = load %m1, %w0, %s_addr : bits<8>      ; load containing byte
%mask   = const.bits<8> 248
%five   = const.bits<8> 5
%zero   = const.bits<8> 0
%masked = and.bits<8> %old, %mask                         ; clear field bits
%shift  = shl.bits<8> %five, %zero                        ; shift value into place
%new    = or.bits<8> %masked, %shift                      ; insert field
%m3 = access_in_bounds %m2, %s_addr [size=1, align=1]
(%m4, %w2) = store %m3, %w1, %s_addr, %new               ; write back
```

#### 10.1.7 Function Call Lowering

C function calls lower to EvoIR `call` instructions. Day 7 does not consume
`__attribute__` syntax; attributes, `typeof`, and statement expressions remain
the Day 9 telemetry-driven boundary. Every Day 7 call therefore uses the
conservative effect contract below. Reads/writes remain `All`; the volatile bit
is the least fixed point over the direct same-TU call graph and is true when
the callee or any transitive callee contains a volatile access. Recursive SCCs
are solved together. Every call still consumes and produces `world`, even when
this bit is false. Calls without a same-TU body remain typed unsupported in
this profile.

The Day 7 conservative effect summary for every call is:

```text
EffectSummary {
    reads: {All},
    writes: {All},
    volatile: transitive_contains_volatile,
    atomic: false,
    io: false,
    may_trap: true,
    may_unwind: true,
}
```

#### 10.1.8 Loop and Branch Lowering

C control flow lowers to EvoIR block-argument SSA:

```text
; while (cond) { body; }
; after;

^entry:
    br ^loop_cond()

^loop_cond:
    %c = ... cond ...
    cond_br %c, ^body, ^after

^body:
    ... body ...
    br ^loop_cond

^after:
    ...
```

`for` loops, `do-while` loops, `goto`, `switch`, and other C control flow
constructs all lower to combinations of `br`, `cond_br`, and appropriate
block structure.

### 10.2 EvoIR to EvoMIR Extraction Contract

After EqSat optimization and extraction, the chosen EvoIR representations
are lowered to EvoMIR. The contracts for this phase are defined in the
EvoMIR specification and are not part of this document, except for the
following invariant carried through the persistent equality database:

> **Every EvoMIR representation added to an EClass shares the EvoIR-level
> semantics of that EClass.** The EvoIR-to-EvoMIR lowering agent must not
> add a representation to an EClass unless it faithfully implements the
> abstract semantics of that EClass for all defined executions.

### 10.3 Agent Contract Summary

| Agent | Must | Must Not |
|---|---|---|
| Parser agent | Produce syntactically valid textual EvoIR conforming to Section 9 | Produce IR with type errors or SSA violations |
| Lowering agent | Transform C-HIR according to Section 10 | Elide UB checks, reorder volatile operations, drop provenance |
| EqSat agent | Respect the equality criterion (Section 7) and effect invariant (Section 4.5) | Merge e-classes with incompatible types or effects |
| Proof agent | Generate justifications for every trusted merge (Section 8) | Produce unverifiable or circular proof DAGs |
| Backend agents | Add only semantically faithful target representations to existing EClasses | Drop or weaken semantic guarantees during lowering |

### 10.4 Verification

Every transformation that claims to produce a valid EvoIR program must pass
the SSA validator (see `crates/evocc-evoir/src/lib.rs:validate_ssa`). The
function-local validator must enforce:

1. Every block argument and instruction result owns one function-unique
   `Value.id` definition
2. Every operand is a current-block argument or a preceding instruction result
   in that same block
3. Entry arguments match interface parameters positionally
4. Block argument arities and types match across every predecessor edge
5. Every non-entry block is reachable from the entry block
6. Return types match the function declaration
7. Each instruction consumes and replaces exactly the token kinds declared by
   its signature: guards consume only the current `mem<all>`, while
   allocation, lifetime, load/store, and calls consume their declared memory
   and world inputs. At an edge or return boundary each current token is
   forwarded exactly once. A `cond_br` may place that same current token once
   on each mutually exclusive successor edge; this is control-flow transport,
   not two simultaneous token uses.
8. Day 7 executable memory operations use only `mem<all>`; each guard observes
   the current token and advances it to a fresh identity
9. Every load/store consumes a matching `access_in_bounds` output for the same
   pointer and exact target-derived size/alignment, with no intervening memory
   effect
10. Every `ptr.offset.*.*` definition is immediately followed by
    `pointer_valid` on that exact pointer and the current memory token before
    any other use of the pointer

Module validation additionally checks each call's ordinary argument and return
types against the callee declaration, and requires every global and every
`alloca` instruction site to have one pairwise-distinct origin ordinal. The
function-local validator still
checks local use-def, ordinary-value versus effect-token positions, and exact
`mem<R>`/`world` threading without guessing a callee signature. Day 6 lowering
uses the conservative `mem<all>` region.

The Day 6 reference interpreter executes `bits<N>` values for `1 <= N <= 64`
and may forward opaque `mem<R>` and `world` tokens at block/function
boundaries. It advances explicit block and call-frame program counters and
charges one shared step budget for every instruction and terminator across
direct calls and recursion. Module validation resolves every direct callee and
checks the ordinary arguments/results plus the exact trailing effect-token
interface of every call/effect participant, including callers. Revision 1.7
additionally executes `ptr<generic>`, globals, automatic object allocation and
lifetime, pointer offset/equality, memory guards, and ordinary/volatile
load/store through the shared object model. It returns immutable memory/world
snapshots and the exact volatile trace; every invalid access reports the typed
diagnostic in Section 6.1. Floating, vector, tuple, atomic, heap-library, and
pointer/integer-conversion execution remain typed unsupported and must never
produce a placeholder value.

---

## Appendix A: Instruction Quick Reference

| Category | Instruction | Result | Operands | May Trap |
|---|---|---|---|---|
| Constants | `const.bits<N>` | `bits<N>` | integer literal | no |
| Arithmetic | `add.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | no |
| | `sub.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | no |
| | `mul.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | no |
| | `sdiv.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | yes |
| | `udiv.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | yes |
| | `srem.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | yes |
| | `urem.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | yes |
| UB predicate | `divisor_nonzero` | `bits<1>` | `%divisor` : `bits<N>` | no |
| | `signed_div_rem_no_overflow` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| | `shift_count_defined.signed` / `.unsigned` | `bits<1>` | `%shifted` : `bits<N>`, `%amount` : `bits<M>` | no |
| | `signed_shl_defined.signed` / `.unsigned` | `bits<1>` | `%shifted` : `bits<N>`, `%amount` : `bits<M>` | no |
| Bitwise | `and.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | no |
| | `or.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | no |
| | `xor.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | no |
| | `shl.bits<N>` | `bits<N>` | `%a` : `bits<N>`, `%b` : `bits<M>` | no |
| | `lshr.bits<N>` | `bits<N>` | `%a` : `bits<N>`, `%b` : `bits<M>` | no |
| | `ashr.bits<N>` | `bits<N>` | `%a` : `bits<N>`, `%b` : `bits<M>` | no |
| Comparison | `icmp.eq` | `bits<1>` | `%a`, `%b` : same `bits<N>` or same pointer type | no |
| | `icmp.ne` | `bits<1>` | `%a`, `%b` : same `bits<N>` or same pointer type | no |
| | `icmp.slt` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| | `icmp.sle` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| | `icmp.ult` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| | `icmp.ule` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| Extension | `sext.bits<N>` | `bits<N>` | `%a` : `bits<W>` (W <= N) | no |
| | `zext.bits<N>` | `bits<N>` | `%a` : `bits<W>` (W <= N) | no |
| | `trunc.bits<N>` | `bits<N>` | `%a` : `bits<W>` (W >= N) | no |
| Pointer | `ptr.null` | `ptr<generic>` | — | no |
| | `global_addr` | `ptr<generic>` | global symbol | no |
| | `ptr.offset.<direction>.<signedness>` | `ptr<AS>` | `%base` : `ptr<AS>`, `%index` : `bits<N>`, positive scale | yes |
| Memory | `alloca` | `ptr<generic>`, `mem<all>`, `world` | `%m`, `%w`, size, align | yes |
| | `lifetime.end` | `mem<all>`, `world` | `%m`, `%w`, `%ptr` | yes |
| | `pointer_valid` | `mem<all>` | `%m`, `%ptr` | yes |
| | `access_in_bounds` | `mem<all>` | `%m`, `%ptr`, size, align | yes |
| | `load [volatile]` | Type, `mem<all>`, `world` | `%m`, `%w`, `%ptr` | yes |
| | `store [volatile]` | `mem<all>`, `world` | `%m`, `%w`, `%ptr`, `%val` : Type | yes |
| Control | `br ^target(args...)` | — | block label, values | n/a |
| | `cond_br %c, ^t(targs), ^e(eargs)` | — | condition, blocks, values | n/a |
| | `ret values...` | — | return values | n/a |
| | `trap.scalar_function_fell_off_end` | — | — | yes |
| Calls | `call @callee(args..., %m, %w)` | ret values, `mem`, `world` | callee, args, memory, world | depends |
| UB | `ub.assume.<reason> %cond` | — | `%cond` : `bits<1>` | yes |
| Scalar poison | `uninitialized.bits<N>` | `bits<N>` | — | on consumption |

---

## Appendix B: Type Grammar (Formal)

```
Type       := "bits" "<" Integer ">"
            | "f32" | "f64"
            | "ptr" "<" AddressSpace ">"
            | "mem" "<" Region ">"
            | "world"
            | "vec" "<" Integer "," Type ">"
            | "tuple" "<" TypeList ">"
            | "void"

TypeList   := ε
            | Type ("," Type)*

AddressSpace := "generic" | "global" | "stack" | "threadlocal"

Region     := "all" | "unknown" | "global" | "stack" | "heap" | "escaped"

Integer    := [1-9] [0-9]*
```

---

## Appendix C: Revision History

| Revision | Date | Changes |
|---|---|---|
| 1.7 | 2026-08-22 | Defines the Day 7 `(ObjectId, signed byte_offset)` provenance model, immutable initialized/lifetime-aware `mem<all>` snapshots, module globals and automatic objects, state-preserving pointer/access guards with exact `E3001`/`E3009`–`E3014` diagnostics, and observable volatile world traces. It freezes executable pointer/memory syntax while deferring pointer-valued static relocations, pointer/integer conversions, region splitting, alias-dependent memory rewrites, atomics, heap-library semantics, and backend lowering. |
| 1.6 | 2026-08-18 | Makes `--emit=evoir` the canonical driver spelling, binds it to the exhaustive Section 9 renderer, names the first stored block `^entry` independently of numeric block identity, and corrects the complete example to use the supported `icmp.ult` comparison. |
| 1.5 | 2026-08-18 | Defines transportable Day 6 scalar `uninitialized.bits<N>` poison with consume-time `E3001`, distinct from Day 7 memory initializedness; adds the zero-operand `trap.scalar_function_fell_off_end` terminator and exact `E3007` behavior without placeholder returns or effect tokens. |
| 1.4 | 2026-08-18 | Requires every module-level call/effect participant, including callers, to use the canonical trailing `mem<R>, world` parameter and return interface; makes the UB reason mandatory semantic metadata; assigns the five Day 6 integer reasons exactly the C-HIR `E3002` through `E3006` diagnostics; and freezes executable remainder, shift, bitwise, and non-conflating div/rem/shift predicate shapes, including independently promoted shift-amount widths and signedness. |
| 1.3 | 2026-08-18 | Adds module validation and explicit heap-backed call-frame execution for direct calls and recursion under one shared step budget; records typed signed-overflow `ub.assume` failure; and names the concrete target data-layout service used by current lowering. |
| 1.2 | 2026-08-18 | Freezes named memory/world input and output token positions for `load`, `store`, and `call`; makes token flow region-polymorphic while Day 6 lowering uses conservative `mem<all>`; permits equal-width identity conversions; separates function-local SSA/effect validation from module call-signature validation; and records the bounded, typed Day 6 interpreter subset. |
| 1.1 | 2026-08-18 | Makes formal parameters non-SSA call-interface binders, requires real unique `Value` IDs on entry and non-entry block arguments, requires all cross-block values to be passed explicitly, types the signed add/subtract/multiply definedness predicates used by lowering, and corrects the Section 9 examples. |
| 1.0 | 2026-08-10 | Initial specification. Defines the complete EvoIR semantic contract for Day 4 of the 30-day EvoCC build. |
