# EvoIR Specification

**Revision 1.9** | **Authoritative reference** | **2026-08-25**

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
11. [Proof Checking Interface](#11-proof-checking-interface)

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
  | fnptr<Sig>       -- pointer to a function of exactly this signature
  | void             -- absence of a value
```

```text
Sig ::= "(" TypeList ")" "->" "(" TypeList ")"
```

A `fnptr` carries its signature in the type rather than at the call site, so a
callee's parameter and return types are known from the pointer alone. That is
what lets an indirect call be validated without resolving a symbol. Two `fnptr`
types are equal exactly when their signatures are identical, including the
trailing `mem<R>` and `world` positions Section 2.1 requires of every effect
participant.

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

A pointer is a typed reference to either an object or a function. In the Day 8
abstract machine its runtime identity is exactly:

```text
Pointer {
    provenance: Object(ObjectId) | Function(Symbol) | Null,
    byte_offset: signed 64-bit byte offset,
}
```

`Null` has offset zero. `Function` also has offset zero and can be consumed only
by call/comparison or transported as a value; it is never an access address.
Every `Object(ObjectId)` names one storage instance, is allocated
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
| `fnptr<Sig>` | every parameter and result type is well-formed and is not `void`; `mem<R>` and `world` appear exactly once each, as the final two parameters and the final two results, in that order; no other position carries `mem<_>` or `world`; no type nested inside a parameter or result carries either |
| `void` | always well-formed |

---

## 2. Block-Argument SSA Semantics

### 2.1 Module and Function Structure

An EvoIR module contains an ordered global-object table followed by an ordered
function table. A global object binds one unique symbol, stable source-origin
site, and global kind (`file`, `function_static`, or `string_literal`) to a fixed size and
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
func @name(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    ret %v0, %v1, %v2
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
^b1(%v0: bits<32>):
    %v1 = const.bits<32> 1
    %v2 = add.bits<32> %v0, %v1
    ret %v2
```

### 2.3 Block Arguments (Phi-Equivalent)

Block arguments are the phi-equivalent in block-argument SSA. They are bound
at block entry by the predecessor's terminator:

```text
^entry():
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
| `<integer>` | Canonical unsigned decimal literal in the inclusive range `0` to `2^N - 1`. Hexadecimal, binary, signs, separators and leading zeroes are not spellings in Section 9. |

**Semantics.** Produces a `bits<N>` value with the given bit pattern. The
integer literal is interpreted as an unsigned value of exactly that width. An
out-of-range literal is malformed; a parser never truncates or extends it.

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
| `%a`, `%b` | For `eq`/`ne`, identical `bits<N>`, `ptr<AS>`, or `fnptr<Sig>` types. For ordered comparisons, `bits<N>` operands of the same width. |

**Semantics.** Integer comparison producing a `bits<1>` value: `1` if the
condition is true, `0` otherwise.

`eq` / `ne`: bitwise equality / inequality for integers. Pointer equality
compares the complete provenance and byte offset from Section 1.3; function
pointers therefore compare equal exactly when both are null or both name the
same function symbol. No ordered pointer comparison exists in this profile.
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
Its result type is written in the instruction, so `%f = ptr.null : fnptr<Sig>`
constructs a null function pointer of that signature and is the only way to
obtain one. Without it `E3015` would name a condition no module could reach.
`ptr.null` is valid only for a `ptr<AS>` or `fnptr<Sig>` result.
`global_addr` returns the stable provenance of the named global object, has
result type `ptr<generic>` in this profile, and is pure. `alloca` allocates a fresh automatic object in the current function and
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
non-automatic, or foreign-frame object reports `E3018`.

`ptr.offset.<direction>.<signedness>` computes a pointer with the same object
provenance as `%base`. Its operand type must be `ptr<AS>`; validation rejects a
`fnptr` operand. A null base reports `E3009` immediately and produces no
noncanonical `(Null, nonzero_offset)` value. Otherwise it decodes
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
both provenance and byte offset. Ordered pointer comparisons and subtraction
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
provenance and offset; a partial non-pointer overwrite of its object
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

```text
%f = func_addr @symbol : fnptr<Sig>
(%ret_vals..., %m1, %w1) = call %f(args..., %m0, %w0)
```

**Semantics.** `func_addr` yields the address of a named function as a value of
the pointed-to signature. Validation requires the symbol to name a function and
the written `Sig` to equal that function's complete lowered signature. It is
pure, consumes no tokens, and produces an ordinary pointer value that may be
stored, loaded, passed and returned.

An indirect `call` names its callee by value rather than by symbol. Validation
checks arguments and results against the pointer's signature; there is no symbol
to resolve and no declaration to consult.

Revision 1.8 admits only `cc = target_default` for every function. Module
validation rejects another calling-convention identifier. Under that single-CC
profile the parameter/result `Sig` is complete; a later profile adding another
calling convention must add it to `fnptr` identity before admitting such a
module.

**Diagnostics.** Module validation compares the indirect call's argument and
result types to the callee value's `fnptr<Sig>` and reports `E3017` on a
mismatch. At runtime, null provenance reports `E3015`; object or otherwise
non-function provenance reports `E3016`. Function provenance names a concrete
module symbol, whose actual signature is compared to the value's static `Sig`;
a mismatch reports `E3019`. The dynamic check is necessary because a function
pointer loaded from memory may select different symbols by array index, which a
module validator cannot predict.

**Provenance.** A function pointer carries the provenance of the function it
names and preserves it across stores and loads exactly as Section 5.3 requires
of object pointers, so a pointer copied through memory names the same function
as the original.

Its object-representation size and alignment come from the target data layout's
function-pointer layout, never from the host. For the frozen AArch64 LP64 target
both are eight bytes/eight-byte alignment. This equality with the object-pointer
layout is a target fact, not permission for a `ptr<AS>`/`fnptr<Sig>` conversion;
those types remain distinct and no such conversion exists in this profile.

**Call effects.** A direct call takes the effect summary derived in Section 4.6
for its callee. An indirect call takes the top summary of Section 4.6, because
the callee is not known statically.

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
| `ptr.null`, `global_addr`, `func_addr` | `{}` | `{}` | false | false | false | false | false |
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
Bit-vector event values use exact target-width raw bits. Object-pointer event
values are canonicalized to `(ObjectOrigin, byte_offset)` rather than local
ObjectId bits; function-pointer values use `Function(Symbol)`, and null uses
`Null`. Valid provenance observations therefore remain comparable between the
two interpreters without exposing interpreter-local identities.

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

### 4.6 Function Effect Summaries

Section 4.1 gives the shape of an effect summary. This section says where a
function's summary lives, how it is derived, and how a reader knows it is the
summary the module actually implies rather than one an earlier pass left behind.

**Storage.** A summary is a property of a function and is carried in the
function header, not recomputed at each call site. Call sites read it; they do
not own it.

```text
func @name(...) -> (...) cc = target_default
    effects { reads=R, writes=W, volatile=B, atomic=B, io=B, may_trap=B, may_unwind=B }
{
```

`R` and `W` are region sets rendered in the Section 1.4 spelling, sorted by the
region order `all, unknown, global, stack, heap, escaped`, comma-separated with
one following space, and written `{}` when empty. Each boolean is `true` or
`false`. The clause is rendered on its own line, indented four spaces, between
the calling convention and the opening brace, and is present on every function
including those whose summary is empty; an absent clause is malformed rather
than defaulted, because a default would be indistinguishable from an omission.

**Derivation.** A summary is the least fixed point over the call graph:

1. Every function starts at the empty summary.
2. Each function's summary is the join of its own instructions' effects, per
   Section 4.2, with the summaries of every function it calls directly.
3. Repeat until no summary changes.

The join is componentwise: union for region sets, logical or for each boolean.
The lattice is finite and every step is monotone, so the least fixed point
exists and is unique, and an implementation may use any iteration order.

A strongly connected component therefore reaches **one** summary shared by all
its members: recursion cannot be given a smaller summary than the cycle it
participates in. A volatile access anywhere in a function's callee closure makes
every transitive caller volatile, which is what keeps a caller from being
reordered across a volatile access it cannot see.

An indirect call contributes the **top** summary, which reads and writes `all`
and sets every boolean, because the callee is not known statically. A function
containing an indirect call therefore has at least the top summary, and so does
everything that calls it.

**Validation.** The validator recomputes the fixed point from the module and
compares it to the recorded summaries. A summary weaker than the recomputed one
is rejected; a summary stronger than it is also rejected, because a summary that
overstates its effects is still not the summary this module implies and would
silently suppress legal reordering elsewhere. A recorded summary is therefore
either exactly right or invalid.

This matters because summaries are the one part of the effect model a
transformation can invalidate without touching: inlining a callee, or replacing
a direct call with an indirect one, changes what a caller does while leaving its
recorded clause untouched. Recomputation on validation is what makes a stale
summary a rejection rather than a silent licence to reorder.

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

Allocation returns the next positive `ObjectId`; zero is never allocated and no
identifier is reused. Null is represented by the distinct `Null` provenance,
not by an object-table entry. Ending a scope or frame lifetime
marks its objects dead in a new snapshot but retains their tombstones. A stale
pointer therefore remains deterministically dangling and can never become a
pointer to a later allocation. Static and string-literal objects are allocated
once during module initialization and live for the whole execution.
String-literal objects are read-only; an attempted store reports `E3014`.

A function-scoped object declared with static storage duration is **not** an
automatic object of its frame. It has one module-global identity, one allocation
site, and one initialization performed before the entry function runs; it is
never allocated or initialized per activation. Two activations of a recursive
function therefore observe the same object, and a value stored by one is visible
to the next.

**Symbol construction.** A function-scoped static's module symbol is derived,
not chosen:

```text
symbol := "__evocc.static." decimal(site_ordinal)
```

`site_ordinal` is the object's allocation-site ordinal from the shared manifest.
It must equal the global's `origin` field. A local name alone is insufficient:
two blocks in one function may each declare a static `count`, and both are
distinct objects. The unique source-site ordinal already is the cross-IR
observation identity, so using it alone makes the symbol reproducible and lets a
module validator verify the spelling without hidden source names or counters.

String-literal objects use the corresponding generated spelling
`__evocc.string.<site_ordinal>`. A file object retains its linked source symbol.
The global's explicit `kind` is what makes these cases distinguishable to a
validator; neither visibility nor `storage=static` could tell a file-scope
static from a function-scoped static.

**Linkage.** A global carries a visibility field distinguishing a symbol other
translation units may reference from one they may not:

```text
global @name origin=O kind=KIND visibility=VIS size=N align=A bytes=HEX initialized=BITS
KIND := "file" | "function_static" | "string_literal"
VIS := "external" | "internal"
```

A `function_static` or `string_literal` global is always `internal`. A `file`
global is `internal` for a file-scope object declared `static` and `external`
for a definition without `static`. A `function_static` symbol must equal
`__evocc.static.<origin>` and a `string_literal` symbol must equal
`__evocc.string.<origin>`; a `file` symbol is a source C identifier and therefore
contains no dot. These are validation rules, not renderer conventions. This is
a property of the object rather than of the name that reached it, so it is
recorded here rather than left to the front end: a linker and a checker both
need it, and neither can recover it from the symbol text.

The dots are part of the generated symbol. Section 9.2 permits dots after the
first character of an identifier; source C identifiers in this profile cannot
contain a dot, so the `__evocc.` generated namespace cannot collide with a
source symbol.

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
pointer representation preserves the complete `(provenance, byte_offset)`;
overwriting only part of that representation invalidates pointer
reconstruction and a later pointer load reports `E3013`.

A pointer's provenance names an allocation, a function, or nothing:

```text
Provenance ::= Object(ObjectId) | Function(Symbol) | Null
```

`Function` provenance is produced only by `func_addr`. It is carried in the
out-of-band fragment exactly as `Object` provenance is, so a function pointer
stored to memory and loaded back names the same function, and a partially
overwritten representation reports `E3013` on the same rule. It has no object
and no offset: `ptr.offset` applied to it is invalid, and loading or storing
through it is invalid, because a function is not an object with bytes. Only
`call` consumes it.

Two pointers compare equal exactly when both components are equal, which for
function pointers means naming the same symbol. Distinct `ObjectId` values
remain distinct even if an implementation chooses identical abstract address
bits for their object representations. Provenance is never
derived from a host address. `ptrtoint`, integer-to-pointer conversion, and
backend provenance erasure are outside Day 7.

### 5.4 Pointer and Access Classification

Memory faults use this deterministic precedence:

1. `Null` provenance is a null access.
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

Each reason suffix selects one diagnostic. The messages are in Section 6.3,
which is the single authority for them; this table maps reasons to codes and
deliberately does not repeat the text.

| Reason suffix | Diagnostic |
|---|---|
| `division_by_zero` | `E3002` |
| `remainder_by_zero` | `E3003` |
| `signed_overflow` | `E3004` |
| `invalid_shift_count` | `E3005` |
| `invalid_signed_left_shift` | `E3006` |

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

Messages are in Section 6.3, which is their single authority; this table gives
each code's meaning and deliberately does not repeat the text.

| Diagnostic | Meaning |
|---|---|
| `E3001` | At least one loaded byte is uninitialized. |
| `E3009` | The accessed pointer has `Null` provenance. |
| `E3010` | The named object's lifetime has ended. |
| `E3011` | Pointer formation or the complete access range is outside its object, or checked offset arithmetic overflowed. |
| `E3012` | The byte offset does not satisfy the access alignment. |
| `E3013` | The provenance is absent/forged or stored pointer bytes lack one complete provenance representation. |
| `E3014` | A store targets string-literal storage. |
| `E3018` | `lifetime.end` names a live base pointer that is not an automatic object owned by the current frame. |

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
| Null pointer access | pointer has `Null` provenance | memory guard or direct fail-closed access reports `E3009` |
| Dangling pointer access | pointer names a tombstoned object | memory guard or direct fail-closed access reports `E3010` |
| Out-of-bounds access | pointer/range lies outside its object | memory guard or direct fail-closed access reports `E3011` |
| Misaligned access | offset violates the target-required alignment | memory guard or direct fail-closed access reports `E3012` |
| Write to string-literal storage | store targets a read-only object | `store` reports `E3014` |
| Use of uninitialized scalar SSA value | consume a value defined by `uninitialized.bits<N>` | Day 6 interpreter reports `E3001` at the consuming instruction or terminator |
| Read from uninitialized memory storage | any requested initializedness bit is false | `load` reports `E3001` at the load, never allocation or token transport |
| Invalid pointer representation/provenance | pointer bytes or provenance are not valid | pointer load/guard/access reports `E3013` |
| Strict aliasing/effective type | access through incompatible effective type | deferred with alias analysis beyond Day 7 |

`uninitialized.bits<N>` defines an ordinary poison value with a real SSA
identity. Branch operands and block arguments may copy that poison without
trapping, which lets definite assignment on a later path overwrite it. Any
semantic consumption—arithmetic, comparison, conversion, branch condition,
ordinary call argument, or return—reports `E3001`.
It is not a zero value and it must never be silently replaced by one. Memory
initializedness uses the separate per-byte object bitmap and follows the same
consume-time rule: transporting a pointer or memory token does not trap; the
executed load does.

### 6.3 Diagnostic Authority

Every diagnostic this specification names appears here with its exact message.
A code introduced in one section and never listed here has no message, so two
implementations would print different text for one condition; a code listed
twice would let a reader think two conditions are the same finding.

| Code | Message | Detected |
|---|---|---|
| `E3001` | `read-of-uninitialized-object` | runtime |
| `E3002` | `integer-division-by-zero` | runtime |
| `E3003` | `integer-remainder-by-zero` | runtime |
| `E3004` | `signed-integer-overflow` | runtime |
| `E3005` | `invalid-shift-count` | runtime |
| `E3006` | `invalid-signed-left-shift` | runtime |
| `E3007` | `scalar-function-fell-off-end` | runtime |
| `E3009` | `null-pointer-access` | runtime |
| `E3010` | `dangling-pointer-access` | runtime |
| `E3011` | `out-of-bounds-memory-access` | runtime |
| `E3012` | `misaligned-memory-access` | runtime |
| `E3013` | `invalid-pointer-representation` | runtime |
| `E3014` | `write-to-read-only-object` | runtime |
| `E3015` | `call-through-null-function-pointer` | runtime |
| `E3016` | `call-through-non-function-pointer` | runtime |
| `E3017` | `indirect-call-signature-mismatch` | module validation |
| `E2902` | `not-covered: compound-literal` | front end — **retired by revision 1.9**; emitting it after compound literals land is a defect |
| `E2903` | `malformed-attribute` | front end |
| `E2904` | `not-covered: variably-modified-typeof` | front end |
| `E2905` | `statement-expression-outside-function` | front end |
| `E2906` | `designator-outside-object` | front end |
| `E3018` | `invalid-lifetime-end` | runtime |
| `E3019` | `function-pointer-target-signature-mismatch` | runtime |

`E3017` compares a call site to its pointer type; `E3019` compares the runtime
symbol selected by that value to the same type. The separation is deliberate:
a malformed call site and a program-selected incompatible target are different
findings and are acted on differently.

### 6.4 UB and Optimization

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

### 6.5 Propagation Rules

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

**Day 8 P0 profile.** The Day 7 baseline used one deterministic, ephemeral
e-graph per candidate and discarded it after extraction. Revision 1.8 replaces
that lifetime with the persistent state in Section 8.9 while retaining the
small P0 semantic surface. Its operator authority binds every operator,
constant payload, bit width, arity, and pure/no-trap effect class; distinct
input values and distinct constants can never hash-cons merely because they
share a nullary opcode.

The P0 rules are limited to typed constant folding, `x + 0`, `x * 1`,
`x & 0`, `x | 0`, double arithmetic negation encoded exactly as
`sub.bits<N>(0, sub.bits<N>(0, x)) -> x`, and the six identity comparisons:
`eq`, `sle`, and `ule` produce one; `ne`, `slt`, and `ult` produce zero. Section
11.6 is the exact rule authority. Each pattern requires identical operand types
and the poison/effect exclusions below. A deterministic
greedy extractor accepts only a strict cost improvement and prefers the
original on ties. "Original" for a final class means its smallest `source=before`
`NodeId`; if that class has no before node, equal-cost alternatives use the
smallest `NodeId`.

For the Day 8 P0 profile, cost is the number of instructions that a choice newly
emits into the rewritten module, with the existing semantic convention that a
`const` node costs zero. The rules are region-relative and ordered by the first
matching case:

- a seed costs zero;
- a `const` node costs zero;
- a `source=before` witness other than the replacement root costs zero. Its
  instruction already exists earlier in the same block and Section 8.5 leaves
  it in place, so selecting it emits only a reference and does not expand its
  child extraction closure;
- the replacement root's own `source=before` node costs one plus its selected
  child costs; and
- any non-constant `source=derived` node costs one plus its selected child
  costs.

The replacement-root exclusion in the third rule is mandatory: assigning zero
to the original root would make every target tie the original under strict
improvement and would silently disable all P0 optimization. Costs are solved by
repeatedly visiting nodes in `NodeId` order and assigning a finite cost only
when every child required by the applicable rule has a finite cost, until no
choice changes. A class reachable from the root but left without a finite choice
makes extraction fail and triggers baseline fallback rather than recursion or a
placeholder. This is deliberately an instruction-count-only P0 model: it does
not estimate register pressure, and a later multi-objective model must revise
the cost authority rather than reinterpret these zero-cost cases. Cross-block
reuse remains excluded by Section 8.5. Explicit node and epoch budgets bound
every run. Budget,
matching, extraction, validation, or proof failure returns the byte-identical
original EvoIR and continues compilation; equality saturation is never needed
for correctness.

The driver profile fixes `max_nodes = 100000` and `max_epochs = 1024` for one
`saturate_scalar` transaction. The node bound counts the complete persistent
graph after each proposed insertion, not merely nodes newly allocated by this
call. One epoch is one traversal of registry rules in Section 11.6 order over
canonical root e-classes in numeric order and nodes in insertion order. Within
one left-hand node, substitutions are ordered lexicographically by the vector
of bound canonical e-class IDs after variables are sorted by name; a builder
that can reuse several existing nodes chooses the smallest `NodeId`.
Congruence rebuilding visits dirty canonical classes numerically and their
parent nodes in insertion order. Hash-table iteration is never observable. A
fixed-point epoch that creates no node or union terminates successfully and
still counts as one. Reaching either bound before fixed point rolls the whole
transaction back and returns the unchanged module. Tests may inject smaller
bounds, but the bound and fallback semantics do not change.

Every accepted merge is present in a complete proof certificate whose trusted
operator/rule authority, canonical input-module digest, exact witness and seed
manifests, region identity, extracted output, and full event
stream are bound by the receipt. `evo-proofcheck` receives the canonical input
module, verifies its digest, and independently recomputes eligibility and both
manifests for every named region rather than trusting the certificate. It
replays every merge, verifies that the extracted root belongs to the original
root e-class, reconstructs the extracted region, and compares its canonical
bytes and output commitment to the receipt. A merge-valid certificate that
names an unrelated root or output is invalid. Any node
carrying or transitively depending on `mem<all>`, `world`, a pointer, a guard,
another may-trap/effectful instruction, or scalar poison is rejected before
insertion. Poison dependence is computed before region formation and is not
re-derived from the extracted expression.

EVP1 is a standalone proof of one `before.evo` to `after.evo` transaction.
For P0 matching and extraction, the active node set is therefore exactly the
current before-module witness nodes plus nodes derived in that transaction.
Nodes retained only from an earlier transaction remain queryable in persistent
classes and their receipts remain in persistent history, but they are not P0
match, proof, or extraction candidates for a later EVP1 file. Admitting such a
history-only node without serializing its earlier derivation would make the
second certificate impossible for an independent checker to replay. A future
certificate revision may carry cumulative history; EVP1 does not.

The exact driver capability is `-Oeq-scalar`, and only
`-Oeq-scalar --probe` becomes implemented with this bridge. The broader
`-Oeq --probe` remains exit 69 `NOT_IMPLEMENTED`.

Revision 1.8 activates persistence for the eligible EvoIR scalar values in this
P0 profile. The C-HIR and target-representation arrows in Sections 8.1 through
8.4 remain architectural consumers, not authorization to implement target
lowering, backend nodes, pointer/object nodes, or any out-of-scope rewrite in
Day 8.

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

### 8.5 Eligible Regions

Section 8's P0 profile says which rules may fire. This section says what they
may fire on, in enough detail that two implementations reach the same answer.
That matters because the checker derives eligibility for itself: a producer and
a checker sharing this judgment would agree on an ineligible region and accept
the rewrite built on it, whereas disagreeing about a *parse* only costs a
rejection. Independence is spent here and nowhere else.

**Granularity.** A region has exactly one **replacement root**, but its proof
context includes a deterministic **witness closure**. Starting at one candidate
instruction, recursively follow an operand only when its definition is an
earlier instruction in the same block and that instruction is itself on the
operator allowlist below. The closure contains the root and every instruction
reached that way. Stop at a block argument, a definition in another block, or a
definition whose instruction is not allowlisted; that value is a seed.

Only the replacement root is changed during reassembly. Witness instructions
remain where they were, even when they become dead, and two regions may have
overlapping witness closures. This is still one-root local replacement, not
region splitting. The closure is necessary rather than optional: without the
earlier `const` node, `x + 0` cannot be matched, and without the inner `sub`
node, double negation cannot be proved. Calling those definitions opaque leaves
would make the advertised P0 registry unreachable while its tests appeared to
exercise it.

**Operator allowlist.** Exactly these instruction forms may be a region root:

```text
const, add, sub, mul, and, or, xor,
icmp.eq, icmp.ne, icmp.slt, icmp.sle, icmp.ult, icmp.ule
```

Everything else is ineligible, including division and remainder, every shift,
every extension and truncation, every UB predicate, `ub.assume`, all pointer and
memory forms, `call`, and all terminators. An instruction absent from this list
is ineligible even if some future analysis would prove it total: the list is the
authority, not the reasoning behind it.

Every root and witness result in this profile has type `bits<N>` with
`1 <= N <= 64`. Comparisons may consume one common `bits<N>` sort and produce
`bits<1>`. Wider scalar equality is a later registry revision; silently
truncating a wider constant to the host's integer width is invalid.

**Poison and effect exclusion.** A value is *tainted* when it is the result of
`uninitialized`, when any operand it names is tainted, or when it is a block
argument and any terminator edge entering that block passes a tainted value in
its position.

That last clause is not an elaboration. Taint that propagated only along operands
would be laundered by a branch: a tainted value passed as a block argument would
arrive untainted, and the P0 profile would then admit an expression built on
poison. Every edge, on every path into the block, contributes.

Taint is computed before regions are formed, by repeated forward passes over each
function in the order of Section 8.6, until no value changes state. The order of that
iteration does not affect the result, because taint is monotone and the pass
only ever adds; the fixed point is therefore unique and an implementation may
choose any iteration order that reaches it. A region is ineligible when its root,
any witness, or any seed is tainted, carries `mem<R>`, carries `world`, is a
pointer, is a guard result, may trap, or transitively depends on one of those
excluded values.

"Transitively depends" is a second monotone fixed point, not an implementation
guess. A value starts excluded when its type is `mem<R>`, `world`, any pointer
type, or when its defining instruction is absent from the allowlist, may trap,
or carries any non-pure effect. Exclusion propagates from every operand to its
result and across block edges positionally to block arguments. Entry block
arguments of ordinary `bits<N>` type start unexcluded. Iterate forward passes in
Section 8.6 order until no bit changes. Thus a scalar returned by a call is an
opaque but excluded seed, while a scalar block argument reached only from total
pure arithmetic remains a legal opaque seed. Producer and checker must compute
this fixed point separately.

Taint is a property of the value, not of the expression extracted afterwards. An
implementation that re-derived it from an extracted region could reach a
different answer, so it is computed once, before formation.

### 8.6 Traversal Order

Every enumeration this section governs is deterministic, because a seed
manifest and a region identity are compared byte for byte between a producer and
a checker that never share a data structure.

- Functions in module definition order.
- Blocks in function definition order.
- Instructions in block order, terminator last.
- Operands in the order Section 9.3 renders them.
- Witness instructions in their original instruction order, restricted to the
  closure; never in recursive-discovery order.

No enumeration is ordered by a hash, by an address, by an identifier's numeric
value, or by iteration over an unordered container.

### 8.7 Seed Manifest and Region Identity

A **seed** is a leaf of the witness closure: a value a witness instruction names
but no instruction in that closure defines.

```text
Seed {
    value,        -- the function-local value identifier
    ty,           -- the value's type, in Section 9 spelling
    origin,       -- "input" for a block argument,
                  -- "definition" for a value defined earlier in the function
}
```

The **seed manifest** of a region scans witness instructions in their original
instruction order and each instruction's operands in textual order. It lists
the encountered seeds, deduplicated by value with the first mention retained.
It is not sorted by name or identifier. This order and the witness closure are
both independently recomputed from `before.evo`.

The **witness manifest** is the closure's instruction identities in original
instruction order. Each identity uses the same
`function/block/instruction-index` spelling as a region identity. A checker
recomputes the exact witness and seed manifests; the certificate cannot add an
instruction, hide one, or reinterpret a seed as a constant node.

A **region identity** is the tuple

```text
(function name, block label, instruction index within the block)
```

rendered as `function/block/index`, with the index counting non-terminator
instructions from zero. `function` omits the leading `@`, and `block` omits the
leading `^`; both remaining components must be `ident` values. It does not
contain the value identifier of the root. The identity is interpreted against
`before.evo` only; the checker derives the after-module rather than trying to
find the same instruction index after deletions and insertions. Omitting the
value name keeps region binding independent of the subsequent value
renumbering.

Optimization is optional. The checker recomputes the complete ordered set of
eligible regions and both manifests from `before.evo`, but the certificate may
name any duplicate-free subsequence of that set. Every named region must match
the independently derived identity and manifests exactly. An omitted eligible
region means "not optimized", not an invalid proof; otherwise a budget fallback
or an optimizer that deliberately performs no rewrites could never emit the
required valid empty certificate.

### 8.8 Reassembling the After-Module

Extraction replaces the root instruction of each accepted region. Everything
outside the accepted regions is unchanged, so the after-module is derived
mechanically rather than described by the producer:

1. Start from the parsed `before.evo`.
2. Derive every accepted replacement against the unchanged `before.evo`, then
   apply those replacements in Section 8.6 order. A reference to an earlier root
   is resolved through that root's replacement map. This dependency is acyclic
   because witness closure follows only earlier definitions. A rejected,
   omitted, or unattempted region contributes nothing.
3. Renumber values in Section 8.6 order, then render under Section 9.4.

Replacement is defined for the cases a reassembler actually meets:

| Case | Rule |
|---|---|
| A root whose replacement is a single instruction | The replacement occupies the root's position. |
| A root whose replacement is several instructions | They occupy the root's position in extraction order, before any instruction that followed the root. |
| A root replaced by an existing value, so the region is deleted | The root instruction is removed and every use of its result is rewritten to that value. The value must be a seed of the region or defined earlier in the same block; anything else is invalid, because it would move a definition across a control-flow edge. |
| A multi-result instruction | It is never a region root, since Section 8.5 admits only single-result forms. A multi-result instruction may appear as a seed, and its results are seeds individually. |
| A block argument used as a seed | It is never rewritten. Rewriting one would change an edge, which is outside a region. |
| Two accepted regions where one's root occurs in the other's witness closure | Both roots are replaced. Every closure and seed relation is read from `before.evo`, so one replacement never changes which regions were eligible or what another proof meant. |

**Renumbering.** Formal parameters are **not** renumbered. Section 2.1 makes them
non-SSA call-interface binders in the `%arg` namespace, positional and
independent of value identity, so a renumbering that assigned them `%v`
identifiers would conflate two namespaces the specification keeps apart.

Values are assigned consecutive identifiers from zero in Section 8.6 order over
the rewritten module: for each block in order, its arguments in order, then each
instruction's results in order. Identifiers from `before.evo` are not preserved
and must not be, since deleting a root leaves a gap that any preserving scheme
would have to fill by a rule of its own.

Block identity is unchanged. Blocks are neither added, removed, reordered, nor
relabelled, because Section 8.5 permits one replacement root and no CFG change.

The checker performs these steps itself and compares its rendering to
`after.evo` in full canonical bytes. Comparing an extracted root, or an output
commitment carried inside the certificate, is not sufficient: both are claims
about a part, and a producer that changed something outside every accepted
region would satisfy either while shipping a different program.

### 8.9 Equality State

Equality becomes compiler state rather than a per-pass structure. The state owns
the e-graph, the map from `(function, value)` to `EClassId`, and the proof
history.

```text
EqualityState {
    identity,     -- (translation-unit identity, target-layout digest)
    revision,     -- monotonically increasing; every committed mutation advances it
    graph,
    value_to_class,
    proofs,
}
```

Value identifiers are function-local, so the map is keyed by the pair. A bare
value identifier is not a key and must not be used as one.

The state is created for exactly one translation unit and target layout. The
translation-unit identity is the lowercase SHA-256 of the canonical Section 9
bytes first accepted by `ingest_module`; it remains the identity after legal
extraction changes those bytes. The target-layout identity is the SHA-256 of
the ASCII string `aarch64-unknown-linux-gnu/evoir-layout-v1` followed by one LF
byte, namely
`4f12297e5b4b84815a07e990a1e67102e2a33e87efd72a25d5efca3b4dde07e8`.
Changing any layout rule requires a new version string before a state can be
reused. Reuse with another identity is rejected before mutation. `EClassId` is an append-only
arena handle and is never recycled; union chooses the numerically smaller
canonical root but does not rewrite or reuse either handle. `class_of` resolves
the mapped handle through union-find before returning it.

`ingest_module` is deterministic and idempotent. It visits functions, blocks,
instructions and results in Section 8.6 order, reuses every already-mapped
`(function, ValueId)`, and inserts only missing representations. Ingesting the
same module twice adds no node, union, proof event or revision. A conflicting
type or definition for an existing key rejects the transaction rather than
silently rebinding it.

An opaque scalar input is represented by an `input` node whose immutable
identity is `(function, ValueId, type)`; it is not a shared nullary opcode.
Distinct inputs therefore start in distinct classes. EVP1 does not serialize
that implementation node: its seed record creates the corresponding local leaf
class directly, which is why two different seeds can never collapse merely
because both have zero children.

Extraction returns an explicit before-to-after value map. Unchanged values that
are renumbered transfer their existing handles; a new extracted value maps to
the selected root class; a deleted root loses its current-value key but its
class, original node and proof history remain in the database. The map update
and module replacement commit atomically. This is how equality persists while
Section 8.8 is free to renumber textual `%v` identities.

**Transactions.** `ingest_module`, `saturate_scalar` and `extract_module` are
separate phase operations, but one driver optimization wraps all three in one
outer transaction. Phase boundaries may create internal savepoints; they do not
publish a commit or advance the revision. Only after proof replay, extraction,
full-module reassembly and module validation all pass does the outer transaction
atomically publish both module and state. A successful top-level transaction
that made at least one mutation advances the revision exactly once; an
idempotent no-op does not. Budget exhaustion, a proof failure, a validation
failure and an extraction failure all roll the outer transaction back to the
module, graph, maps, proof history and revision it entered with.

Rollback restores the state *and* the module together. A failure that returned
the original module while leaving unions in the database would be invisible in
the returned program and permanent in the state, and it would remain invisible
until some later phase asked what was known. Code that reverts only the module
is correct while the map dies with the call and wrong the moment it does not, so
this is a property of the transaction rather than of any caller.

**Accumulation.** A representation is added to an existing class. A class that
has been optimized retains the representation it started with alongside the one
extraction chose, and a later phase adding a target-specific form joins the same
class rather than creating a new one. An implementation that replaced the
original would be indistinguishable at the point most tests look, and would
differ only when a later phase asked what else was equal, which is exactly when
the difference matters.

**Justification.** Every union carries a receipt. Replaying the accumulated
history against the trusted registry re-derives exactly the partition the state
holds; a partition containing a union the history does not justify is invalid
regardless of how it arose.

### 8.10 Analysis Lattices

An e-class carries facts as well as nodes. Two are defined in this profile;
both are needed to state a rewrite whose legality depends on a value's bits
rather than its syntax, which is the whole of the roadmap's known-bits tier.

```text
KnownBits<N> = { mask: bits<N>, value: bits<N> }   -- bit i known iff mask[i]=1,
                                                  -- and then equal to value[i]
ConstValue<N> = KnownBits<N> with mask all ones
```

`ConstValue` is not a separate lattice; it is the bottom of `KnownBits`. This
keeps one join and one transfer function per operator instead of two of each.

**Order.** `a ⊑ b` when every bit known in `b` is known in `a` with the same
value. The top is nothing known. Facts only ever move down within one
saturation transaction: a class's fact after any step is at or below its fact
before.

**Merge.** When two classes are unioned, the merged class's fact is the meet of
the two: every bit known in either is known in the result. If the two disagree
on a bit both know, the union is **invalid** — two values with a differing
known bit cannot be equal — and the transaction rolls back under Section 8.9.
That check is part of what makes a union sound, so it is performed by the
checker as well as the producer.

**Transfer.** For a node with known-bits inputs, exactly these transfers are
defined; anything not listed transfers to top.

| Operator | Result |
|---|---|
| `const` | fully known |
| `and` | known where either input is known zero, or both are known one |
| `or` | known where either input is known one, or both are known zero |
| `xor` | known where both inputs are known |
| `add`, `sub` | the trailing run of bits that is known in both inputs, computed by ordinary carry propagation until the first unknown bit |
| `mul` | the trailing known-zero run is the sum of the inputs' trailing known-zero runs; nothing else |
| `icmp.*` | fully known when the comparison is decidable from the known bits of both operands, top otherwise |

The transfers are deliberately weak. Each is a statement a checker can
recompute from the region's nodes with no search; a stronger analysis that
needed one would move the judgment into shared code, which Section 11.6
reserves for cases that fail safe.

**Use.** A rewrite may name a fact as a premise. A constant fold's premise is
the operand's `ConstValue`; a known-bits identity such as `and(x, m) -> x` when
every bit of `m` known one covers every bit of `x` known nonzero has the two
facts as premises. Premises appear in the certificate and are recomputed by the
checker under Section 11.3.

### 8.11 Bounded Scheduler

Saturation is never run to a fixed point in production. The scheduler is bounded
in four independent ways and every bound is recorded, so a checker confirms that
a certificate's producer stayed within them rather than trusting that it did.

**Tiers.** Rules belong to one of two tiers and an epoch runs the tiers in
order:

```text
Tier A   canonical reductions          x+0, x*1, x&0, x|0, double negation,
                                       identity comparisons
Tier B   fact-driven rules             constant folding, known-bits identities
```

Tier B depends on facts that Tier A's reductions may sharpen, which is why the
order is fixed rather than left to the implementation.

**Match fuel.** Each rule may fire at most `max(256, 8 × n)` times per epoch,
where `n` is the number of nodes the region held when the transaction began.
Fuel is per rule per epoch, so one prolific rule cannot starve the rest.

**Epochs and nodes.** The node and epoch ceilings of the P0 profile stand:
`max_nodes = 100000`, `max_epochs = 1024`, the node bound counted over the
complete persistent graph after each proposed insertion.

**Stop rule.** Saturation stops at the first of: no rule fired in an epoch
(*saturated*); no root's best extraction became strictly cheaper in two
consecutive epochs (*no-new-best*); the epoch, node or fuel bound was reached
(*epochs*, *nodes*, *fuel*). Hitting a bound is a successful stop, not a
failure: extraction proceeds from the current graph, and Section 8's rule that
saturation is never needed for correctness is what makes that safe.

**Record.** The certificate carries the bounds, the amounts consumed, the stop
reason, and an epoch marker before each epoch's events. The checker verifies
that consumed does not exceed bound, that no epoch marker exceeds the declared
epoch count, and that an exhaustion reason is consistent with the
recorded consumption. It does not re-run the scheduler; a schedule is a choice,
and what is verified is that the choice stayed inside the declared box.

---

## 9. Textual Syntax

### 9.1 Purpose and Canonical Status

The EvoIR textual syntax is the module interchange format. It is used by test
fixtures, by `evo-run`, by `--emit=evoir`, and by the proof checker, which reads
a producer's rendering rather than sharing the producer's in-memory module.

That last use makes the format load-bearing rather than a debugging
convenience. Two obligations follow, and Sections 9.4 and 9.5 discharge them:

- **Canonical.** One module has exactly one rendering, byte for byte. A
  renderer that emits a semantically equal but textually different form is
  non-conforming, because digests are taken over these bytes.
- **Total.** Every field of every construct appears in the rendering. A field a
  renderer omits and a parser does not expect round-trips perfectly while both
  drop it, which would let a checker see a module that is not the one that was
  compiled. Completeness is therefore measured against the enumeration in this
  section, never against a renderer's own output.

Revision 1.8 has one implicit target authority:
`aarch64-unknown-linux-gnu` with the frozen AArch64 LP64 `TargetDataLayout`.
No other target may claim conformance to these bytes. A later multi-target
profile must add a target/layout identity to the module grammar before it can
reuse this interchange format; taking a target from the environment is never an
allowed interpretation.

### 9.2 Lexical Structure

```text
value_name    := "%v" index
param_name    := "%arg" index
block_label   := "^entry" | "^b" index
symbol        := "@" ident
ident         := (letter | "_") (letter | digit | "_" | ".")*
letter        := "A".."Z" | "a".."z"
digit         := "0".."9"
nonzero_digit := "1".."9"
integer       := index
positive_int  := nonzero_digit digit*
index         := "0" | nonzero_digit digit*
hex_byte      := hex_digit hex_digit
hex_digit     := "0".."9" | "a".."f"
bit           := "0" | "1"
newline       := the single byte "\n"
```

Integer literals in instruction operands are unsigned decimal. Hexadecimal and
binary spellings are **not** part of this format and are rejected by Section 9.5.
An earlier revision allowed them in hand-written fixtures; that concession is
withdrawn, because a parser that accepts a spelling no producer emits can read a
file no producer could have written, and the checker's whole value rests on that
being impossible.

`%v` numbering is the function-local `Value.id`. `%arg` numbering is the
positional index of a formal parameter and is independent of `%v` numbering;
the two namespaces never collide because their prefixes differ. Block labels are
positional: the first block of a function is `^entry` regardless of its numeric
identity, and every other block is `^b` followed by its identifier.

There are no comments. A `;` is not special and any occurrence is rejected by
Section 9.5. This format is machine interchange rather than authored source;
explanatory text belongs beside a fixture, not inside a file whose bytes are
digested.

### 9.3 Grammar

```text
module          := ε
                 | global_seq
                 | function_seq
                 | global_seq newline function_seq
global_seq      := global+
function_seq    := function (newline function)*

global          := "global " symbol " origin=" integer
                   " kind=" global_kind " visibility=" visibility
                   " size=" integer " align=" integer
                   " bytes=" hex_byte+ " initialized=" bit+ newline
global_kind     := "file" | "function_static" | "string_literal"
visibility      := "external" | "internal"

type            := "bits<" positive_int ">"
                 | "f32" | "f64"
                 | "ptr<" address_space ">"
                 | "mem<" region ">"
                 | "world"
                 | "vec<" positive_int ", " type ">"
                 | "tuple<" type_list ">"
                 | "fnptr<" signature ">"
                 | "void"
signature       := "(" type_list ")->(" type_list ")"
address_space   := "generic" | "global" | "stack" | "threadlocal"

function        := "func " symbol "(" param_list ")"
                   " -> (" type_list ")"
                   " cc = " ident newline
                   indent effects_clause newline
                   "{" newline
                   block_seq
                   "}" newline
block_seq       := block (newline block)*
effects_clause  := "effects { reads=" region_set ", writes=" region_set
                   ", volatile=" bool ", atomic=" bool ", io=" bool
                   ", may_trap=" bool ", may_unwind=" bool " }"
region_set      := "{}" | "{" region ("," " " region)* "}"
region          := "all" | "unknown" | "global" | "stack" | "heap" | "escaped"
bool            := "true" | "false"
param_list      := ε | param ("," " " param)*
param           := param_name ": " type
type_list       := ε | type ("," " " type)*

block           := block_label "(" block_arg_list "):" newline
                   instruction*
                   terminator
block_arg_list  := ε | block_arg ("," " " block_arg)*
block_arg       := value_name ": " type

instruction     := indent inst_body newline
terminator      := indent term_body newline
indent          := "    "
```

Every instruction form, exhaustively. `N` is a bit width, `T` a type, `S` a
positive scale, `O` an origin ordinal.

```text
inst_body :=
    value_name " = const." type " " integer
  | value_name " = uninitialized." type
  | value_name " = " arith_op "." type " " value_name ", " value_name
  | value_name " = icmp." cmp_op " " value_name ", " value_name
  | value_name " = " cast_op "." type " " value_name
  | value_name " = " binary_predicate " " value_name ", " value_name
  | value_name " = divisor_nonzero " value_name
  | value_name " = " shift_predicate "." index_signedness " "
        value_name ", " value_name
  | "ub.assume." ub_reason " " value_name
  | value_name " = ptr.null : " type
  | value_name " = global_addr " symbol " : " type
  | value_name " = func_addr " symbol " : " type
  | "(" value_name ", " value_name ", " value_name ") = alloca "
        value_name ", " value_name
        " [origin=" integer ", size=" integer ", align=" integer "]"
  | "(" value_name ", " value_name ") = lifetime.end "
        value_name ", " value_name ", " value_name
  | value_name " = pointer_valid " value_name ", " value_name
  | value_name " = access_in_bounds " value_name ", " value_name
        " [size=" integer ", align=" integer "]"
  | "(" value_name ", " value_name ", " value_name ") = load" volatile_marker
        " " value_name ", " value_name ", " value_name " : " type
  | "(" value_name ", " value_name ") = store" volatile_marker
        " " value_name ", " value_name ", " value_name ", " value_name
  | value_name " = ptr.offset." direction "." index_signedness " "
        value_name ", " value_name " [scale=" integer "]"
  | "(" result_list ") = call " callee "(" arg_list ")"

term_body :=
    "br " block_label "(" arg_list ")"
  | "cond_br " value_name ", " block_label "(" arg_list ")"
        ", " block_label "(" arg_list ")"
  | "ret" return_operands
  | "trap." trap_reason

return_operands  := ε | " " value_name ("," " " value_name)*

arith_op         := "add" | "sub" | "mul" | "sdiv" | "udiv" | "srem" | "urem"
                  | "shl" | "ashr" | "lshr" | "and" | "or" | "xor"
cmp_op           := "eq" | "ne" | "slt" | "sle" | "ult" | "ule"
cast_op          := "sext" | "zext" | "trunc"
binary_predicate := "signed_add_defined" | "signed_sub_defined"
                  | "signed_mul_defined" | "signed_div_rem_no_overflow"
shift_predicate  := "shift_count_defined" | "signed_shl_defined"
ub_reason        := "division_by_zero" | "remainder_by_zero" | "signed_overflow"
                  | "invalid_shift_count" | "invalid_signed_left_shift"
trap_reason      := "scalar_function_fell_off_end"
volatile_marker  := ε | " volatile"
direction        := "add" | "sub"
index_signedness := "signed" | "unsigned"
result_list      := value_name ("," " " value_name)*
arg_list         := ε | value_name ("," " " value_name)*
callee           := symbol | value_name
```

The productions above are the canonical type spelling. In particular commas in
`vec`, `tuple`, and function-pointer type lists carry exactly one following
space, while the signature arrow in `fnptr<(args)->(results)>` carries none.
An empty block argument list is still rendered as `()`, so `^entry():` is
canonical and `^entry:` is malformed. Empty branch argument lists follow the
same rule. Formal-parameter spellings are positional `%arg0`, `%arg1`, ...;
source parameter names are not serialized.

#### 9.3.1 Canonical Shapes

The productions above are for a reader. This table is the machine-checkable form
of the same content: every shape a conforming renderer can emit, with `<>`
standing for exactly one operand, attribute value or type. `tools/check-textual-grammar.py`
compares the renderer against this table, so a form whose arity or literal text
drifts is a failure rather than a discrepancy nobody notices.

A previous check compared instruction *names* against the grammar and passed
while a unary predicate was written with two operands. Names are not shapes.

```text
<> = const.<> <>
<> = uninitialized.<>
<> = <>.<> <>, <>
<> = <>.<> <>
<> = icmp.<> <>, <>
<> = <> <>, <>
<> = divisor_nonzero <>
<> = shift_count_defined.<> <>, <>
<> = signed_shl_defined.<> <>, <>
ub.assume.<> <>
<> = ptr.null : <>
<> = global_addr @<> : <>
<> = func_addr @<> : <>
(<>, <>, <>) = alloca <>, <> [origin=<>, size=<>, align=<>]
(<>, <>) = lifetime.end <>, <>, <>
<> = pointer_valid <>, <>
<> = access_in_bounds <>, <> [size=<>, align=<>]
(<>, <>, <>) = load<> <>, <>, <> : <>
(<>, <>) = store<> <>, <>, <>, <>
<> = ptr.offset.<>.<> <>, <> [scale=<>]
(<>) = call @<>(<>)
(<>) = call <>(<>)
br <>
cond_br <>, <>, <>
ret
ret <>
trap.<>
global @<> origin=<> kind=<> visibility=<> size=<> align=<> bytes=<> initialized=<>
func @<>(<>) -> (<>) cc = <>
    effects { reads=<>, writes=<>, volatile=<>, atomic=<>, io=<>, may_trap=<>, may_unwind=<> }
{
```

`load<>` and `store<>` show the optional volatile marker, which is either empty
or one space followed by `volatile`. `ret` appears twice because a function
returning nothing emits the mnemonic alone; a grammar admitting only the second
form could not represent a legal module.

#### 9.3.2 Shapes Not Yet Emitted

A revision may lead its implementation. Any such shapes are listed in this
block so the gap is declared rather than discovered. Revision 1.8.2 has no
pending instruction, global, function-header, or effects-clause shape.

```text
```

The empty `ret`, mandatory effects clause, `func_addr`, indirect callee, full
global attributes, positional `%arg` binders and parenthesized empty block
argument list are all emitted. Whole-module syntax tests, rather than the shape
audit, carry the latter two rules.

The form each replaces is **superseded**. There are no superseded renderer forms
remaining in Revision 1.8.2; this empty block remains machine-readable so the
audit fails if an obsolete form is reintroduced without being declared.

```text
```

### 9.4 Canonical Byte Form

A conforming renderer emits exactly these bytes. Nothing here is stylistic:
each rule removes a degree of freedom that would otherwise make two renderings
of one module disagree on their digest.

| Position | Rule |
|---|---|
| Line ending | `\n`. Never `\r\n`. |
| Final byte | `\n`. The last function's closing brace is followed by exactly one newline and nothing else. |
| Global order | Source-site origin ordinal, ascending. |
| Function order | Module definition order, which lowering fixes from source order. |
| Between globals | Exactly one newline; globals form one block with no blank lines inside it. |
| Globals to functions | Exactly one blank line, and only when the module has at least one global and at least one function. |
| Globals-only module | The final global's line ends the file. No trailing blank line. |
| Empty module | The empty byte string. Not a single newline, which would be a module with one blank line and is therefore malformed. |
| Between functions | Exactly one blank line. |
| Instruction indent | Exactly four spaces. Never a tab. |
| Block label indent | None. Column zero. |
| Effects clause | Present on every function, indented four spaces on its own line between the calling convention and the opening brace. An absent clause is malformed, never an empty default. |
| Closing brace | Column zero, on its own line. |
| Separator | `", "` between operands, results, parameters and arguments. Exactly one space, never zero and never two. |
| Attribute lists | `" [k=v, k=v]"`, one space before the bracket, `", "` between entries, no space inside the brackets. |
| Empty argument list | `"()"` with nothing between the parentheses. |
| Trailing whitespace | Never, on any line. |
| Between blocks | Exactly one blank line. |
| Blank line elsewhere in a function | Never: not after the header, not before the closing brace, not between instructions. |
| Comments | Never in canonical output. |
| Integer literals | Unsigned decimal, no leading zeros other than `0` itself, no sign, no underscores. |
| Hex bytes | Lowercase, exactly two digits per byte, no separators. |

Canonical output contains no comments and no blank lines other than the
separators named above, so a digest over these bytes is a digest over the
module and nothing else.

### 9.5 Parser Rejection Rules

A parser rejects rather than repairs. Rejection is part of the format: a parser
that accepts more than this section allows can read a file a conforming
producer could not have written, and any agreement reached with such a file is
meaningless.

Rejected as **malformed**, meaning the bytes are not a module at all:

- Any deviation from Section 9.3's productions, including a missing or extra
  separator, an unexpected token, or an unterminated construct.
- A tab anywhere, an indent that is not exactly four spaces, trailing
  whitespace on any line, or a `\r`.
- A `%v`, `%arg`, `^b`, or `@` prefix not followed by its required form.
- A function whose formal binders are not exactly `%arg0`, `%arg1`, ... in
  positional order.
- An integer literal that is not unsigned decimal, or that does not fit the
  width its context requires.
- A `bytes=` run whose length is not exactly `2 * size`, an `initialized=` run
  whose length is not exactly `size`, or either containing a character outside
  its alphabet.
- An attribute list whose keys are absent, duplicated, or out of the order
  Section 9.3 fixes.
- A bare `ptr.offset` without both suffixes, or a `vload` or `vstore` mnemonic,
  neither of which exists.
- A `volatile` marker anywhere other than immediately after `load` or `store`.
- An effect region set with a duplicate or whose members are not in the
  canonical order `all, unknown, global, stack, heap, escaped`.
- Any byte after the final newline.

Rejected as **well-formed but invalid**, meaning the bytes parse and the module
they describe is not a legal module. These are the checks Section 2.4, Section
10.4 and the effect model already require, applied to a parsed module:

- A value used without a definition, or defined twice.
- A block argument arity or type mismatch on any edge.
- A memory or world token consumed twice, or a token whose producer does not
  dominate its consumer.
- A load or store whose memory operand is not the output of a matching
  `access_in_bounds` for that pointer and that size and alignment.
- A `global_addr` naming a symbol that is absent or is not a global, or a
  `call` naming a symbol that is absent or is not a function.
- A duplicate symbol across either the global or function tables; a
  `function_static` or `string_literal` whose symbol is not the
  origin-derived spelling; a generated object that is external; or a `file`
  object whose source symbol contains a dot.
- A `ptr.null` whose result is not an object- or function-pointer type, a
  `global_addr` whose result is not `ptr<generic>`, or a `func_addr` whose
  symbol/signature pair does not name an exact function definition.
- A function whose calling convention is not `target_default` in this profile.
- A `size`, `align`, `scale` or width outside the range its definition allows.

The two categories carry distinct exit statuses in Section 11.4, because a file
that cannot be read and a module that must not be trusted are different
findings and are acted on differently.

### 9.6 Complete Example

```text
func @sum(%arg0: ptr<generic>, %arg1: bits<64>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: ptr<generic>, %v1: bits<64>, %v2: mem<all>, %v3: world):
    %v4 = const.bits<64> 0
    %v5 = const.bits<32> 0
    br ^b1(%v0, %v1, %v4, %v5, %v2, %v3)

^b1(%v6: ptr<generic>, %v7: bits<64>, %v8: bits<64>, %v9: bits<32>, %v10: mem<all>, %v11: world):
    %v12 = icmp.ult %v8, %v7
    cond_br %v12, ^b2(%v6, %v7, %v8, %v9, %v10, %v11), ^b3(%v9, %v10, %v11)

^b2(%v13: ptr<generic>, %v14: bits<64>, %v15: bits<64>, %v16: bits<32>, %v17: mem<all>, %v18: world):
    %v19 = ptr.offset.add.unsigned %v13, %v15 [scale=4]
    %v20 = pointer_valid %v17, %v19
    %v21 = access_in_bounds %v20, %v19 [size=4, align=4]
    (%v22, %v23, %v24) = load %v21, %v18, %v19 : bits<32>
    %v25 = add.bits<32> %v16, %v22
    %v26 = const.bits<64> 1
    %v27 = add.bits<64> %v15, %v26
    br ^b1(%v13, %v14, %v27, %v25, %v23, %v24)

^b3(%v28: bits<32>, %v29: mem<all>, %v30: world):
    ret %v28, %v29, %v30
}
```

Formal parameters use the `%arg` namespace and are positional call-interface
binders. The entry block's arguments are the SSA values those binders are bound
to, and are numbered in the `%v` namespace like every other value.

### 9.7 UB-Assume Example

```text
func @checked_add(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = signed_add_defined %v0, %v1
    ub.assume.signed_overflow %v4
    %v5 = add.bits<32> %v0, %v1
    ret %v5, %v2, %v3
}
```

The summary records a possible trap. `add` is total, but `ub.assume` is an
executable obligation in both trusted interpreters: a false condition reports
the reason's typed runtime diagnostic. Treating it as effect-free would permit
moving or deleting the point at which the invalid execution is observed.

### 9.8 Call Example

```text
func @caller(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=true }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call @callee(%v0, %v1, %v2)
    ret %v3, %v4, %v5
}
```

The caller's summary is the join of its own instructions with `@callee`'s, per
Section 4.6. It is shown here as the top summary because `@callee` is not
defined in this fragment; a real module derives it from the callee's body.

### 9.9 Syntax Restrictions

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
| pointer to object `T*` | `ptr<generic>` in the Day 7 profile; storage duration remains object metadata |
| pointer to function | `fnptr<Sig>`, where `Sig` is the complete lowered parameter/result interface including trailing tokens |
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

C function calls lower to EvoIR `call` instructions. Attributes, `typeof`, and
statement expressions remain outside this profile, so no attribute refines a
summary.

**Effects.** Section 4.6 is the only authority. A call's summary is the callee's
summary, and a function's summary is the least fixed point over its call graph;
an indirect call contributes the top summary because its callee is not known
statically.

The superseded Day 7 rule made every call read and write `All`. It is not an
alternative rule in this revision: a direct same-TU call uses the exact fixed
point from Section 4.6, while an indirect call alone contributes top. The
volatile bit is true when the callee or any transitive callee contains a
volatile access; recursive SCCs are solved together. Every call still consumes
and produces `world`, even when that bit is false. Calls without a same-TU body
remain typed unsupported in this profile.

#### 10.1.8 Loop and Branch Lowering

C control flow lowers to EvoIR block-argument SSA:

```text
; while (cond) { body; }
; after;

^entry():
    br ^loop_cond()

^loop_cond():
    %c = ... cond ...
    cond_br %c, ^body(), ^after()

^body():
    ... body ...
    br ^loop_cond()

^after():
    ...
```

`for` loops, `do-while` loops, `goto`, `switch`, and other C control flow
constructs all lower to combinations of `br`, `cond_br`, and appropriate
block structure.

#### 10.1.9 GNU Attributes

The kernel's language is GNU C11, and refreshed reconnaissance shows every parse
failure in the frozen cohort stopping on an `__attribute__`. This section fixes
how attributes are read, where they attach, which are given a meaning, and how
the rest are carried without one.

**Syntax.** An attribute specifier is `__attribute__` `((` attribute-list `))`,
where each attribute is a name, optionally followed by a parenthesised argument
list of expressions or identifiers. Names are accepted with or without the
double-underscore spelling; `__unused__` and `unused` are one attribute.
Arguments are kept as written: an expression argument is parsed and retained, an
identifier argument is retained as a name. An argument list that does not parse
reports `E2903`.

**Placement.** Attribute specifiers are accepted in every position GNU C
accepts them, and each position fixes what the attribute attaches to:

| Position | Attaches to |
|---|---|
| among the declaration specifiers | every declarator of that declaration |
| immediately after a declarator | that declarator alone |
| after `struct`, `union` or `enum` keyword, or after the closing brace | the tag type |
| after a member declarator | that member |
| after a parameter declarator | that parameter |
| after a function declarator, before the body | that function |
| after a label | that label |
| after an enumerator | that enumerator |
| in a type name | the type being named |

An attribute in a position not in this table is malformed and reports `E2903`.

**Representation.** C-HIR carries every attribute losslessly on the entity it
attaches to: name as written, arguments as written, source span, and the
attachment position from the table above. Preservation does not depend on
whether the attribute is understood. A C-HIR dump shows every attribute present
in the source, so a reviewer can confirm nothing was dropped on the way in.

**Applied semantics.** Exactly two attributes have a meaning in this profile,
and both act on layout:

- `aligned(N)` raises alignment. `N` is an integer constant expression that is
  a power of two representable by the target layout; otherwise `E2903`. On a
  type or typedef it produces a type whose alignment is the greater of its
  natural alignment and `N`. On an object it sets the object's alignment
  likewise. On a member it sets that member's alignment, which moves its offset
  and may raise the containing record's alignment. `aligned` with no argument
  means the target's largest fundamental alignment, sixteen bytes on AArch64.
  On a typedef or a type name, `aligned(N)` sets the alignment to exactly `N`,
  which may lower it: `typedef int t __attribute__((aligned(1)))` has alignment
  one under both GCC-14 and Clang-19, as the sealed oracle run of the
  attributed layout corpus records. On a record, a member or an object it can
  only raise; lowering there requires `packed`. An earlier draft said it never
  lowers; the oracle said otherwise and the oracle governs.
- `packed` removes padding. On a record it gives every member alignment one,
  makes the size the sum of member sizes, and gives the record alignment one
  unless `aligned` on the same record says otherwise. On a member it gives that
  member alignment one. Bit-fields in a packed record pack to bit granularity
  across storage-unit boundaries.

These live in the interned type, not in a side table: two record types that
differ only in `packed` are different types with different layouts, and a
`typedef` under `aligned(16)` is a distinct type from its target. The layout
service applies them, and every rule above is checked against the frozen GCC-14
and Clang-19 oracles through the existing layout corpus, which this stage
extends with attributed cases. An oracle disagreement is `ORACLE_CONFLICT` and
is never resolved by choosing one.

**Everything else** is preserved and not applied. The receipt lists, by name,
every attribute the cohort presented that this profile did not apply, so the
size of that set is measured rather than assumed. In particular `section`,
`noreturn`, `unused`, `always_inline`, `gnu_inline`, `no_instrument_function`,
`cold`, `pure`, `const` and `weak` are preserved without effect here; they carry
linkage, effect or backend semantics that belong to later stages, and applying
a partial reading now would create a second source of truth for each.

**Diagnostics introduced.** Messages live in Section 6.3, the single authority;
this table gives conditions only. `E2902`, which rejected compound literals as
not covered, is retired by this revision: Section 10.1.13 covers them, and a
front end that still emits it is defective.

| Code | Condition |
|---|---|
| `E2903` | argument list does not parse, position not in the table, or `aligned` argument not a representable power of two |
| `E2904` | `typeof` of a variably modified type |
| `E2905` | `({ ... })` at file scope or where a constant expression is required |
| `E2906` | designator names a member the type lacks or an index at or beyond a fixed bound |

#### 10.1.10 `typeof`

`typeof(type-name)` and `typeof(expression)` denote a type. `__typeof__` is the
same construct. The result of `typeof(expression)` is the type of the expression
before lvalue conversion: an array-typed lvalue yields the array type, not a
pointer, and qualifiers are retained. The operand expression is not evaluated,
so `typeof(i++)` names `int` and leaves `i` unchanged; the one case in which C
would evaluate it, a variably modified type, is rejected with `E2904` because
this profile has no variable-length objects.

`typeof` produces a type and is fully resolved by the front end; nothing of it
survives into EvoIR.

#### 10.1.11 Statement Expressions

`({ compound-statement })` is an expression whose value is that of its last
statement when that statement is an expression statement, and `void` otherwise.
The value undergoes lvalue conversion: a statement expression is never an
lvalue and its type is never an array or function type.

Objects declared inside end their lifetime at the closing brace, exactly as for
any block, so a pointer to one that escapes is dangling under Section 5.2.
`break`, `continue`, `goto` and `return` may leave a statement expression;
jumping into one is malformed. At file scope, or anywhere a constant expression
is required, it reports `E2905`.

Lowering places the compound statement's blocks into the enclosing function's
control-flow graph as ordinary blocks. When the statement expression contains no
control flow, its value is simply the last instruction's result. When it does,
lowering creates a join block whose single block argument carries the value, and
every path that reaches the closing brace passes its value on that edge. Nothing
new is added to EvoIR: the construct is entirely a lowering pattern over
Section 2.3 block arguments, which is why it belongs in this contract rather
than in the instruction set.

#### 10.1.12 Designated Initializers

A designator list `.member`, `[index]`, or any chain of them, names a subobject
as a `SubobjectPath` of `Field` and `Index` steps, in the same vocabulary as
every other initializer action. The GNU range form `[a ... b]` expands to one
`Index` step per value in the closed range and is otherwise identical.

After a designated subobject, positional initialization continues from the
subobject that follows it, as it would after an undesignated one. When two
actions name overlapping paths, the later one wins and the earlier is dropped
from the plan; the written form still records both, so the dump shows what the
source said. A designator naming a member the type does not have, or an index at
or beyond a fixed bound, reports `E2906` at the designator.

Nothing changes below the plan: lowering consumes `InitAction`s exactly as
before, so an initializer that arrives through designators is
indistinguishable, in EvoIR, from one written positionally.

#### 10.1.13 Compound Literals

`(type-name){ initializer-list }` denotes an object of that type, initialized
by the list, and is an lvalue whose address may be taken. In a block it has
automatic storage and the lifetime of the enclosing block; at file scope it has
static storage. Each occurrence is its own allocation site in the shared
manifest of Section 10.1.3, in source order with every other site, so both
interpreters and the lowering agree on its `ObjectOrigin`.

Lowering is the existing object path: an `alloca` plus the initializer plan for
a block-scope literal, a module global with initial bytes for a file-scope one.
A file-scope literal of `const`-qualified type is a read-only object, and a
store through it reports `E3014` like a string literal; an unqualified one is
writable.

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

Module validation additionally checks each direct call's ordinary argument and
return types against the callee declaration; each indirect call against its
`fnptr<Sig>`; every `func_addr` symbol and written signature against the named
definition; every stored function effect summary against the Section 4.6 fixed
point; symbol uniqueness across functions and globals, kind-specific generated spelling and
visibility; and every global and `alloca` site's pairwise-distinct origin
ordinal. The function-local validator still
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

## 11. Proof Checking Interface

### 11.1 Names

Three different names for one component were in circulation while this was being
designed. They are fixed here so that no implementation chooses them:

| Role | Name |
|---|---|
| Cargo package | `evocc-proofcheck` |
| Installed binary | `evo-proofcheck` |
| Argument interface | `evo-proofcheck before.evo after.evo proof.evp` |
| Capability probe | `evo-proofcheck --probe` |

The compiler driver does **not** answer a `proofcheck` capability probe of its
own. A driver probe would suggest the driver can discharge the check, and the
value of the check is that it runs somewhere the driver's code cannot reach.
`evo-proofcheck --probe` exits zero with empty stdout and stderr, matching the
repository's exact-capability convention. The driver's broad `proof --probe`,
`eqsat --probe`, `-Oeq --probe`, and unrelated broad probes remain exit 69 with
`NOT_IMPLEMENTED`.

### 11.2 Interface

The checker reads three files and takes nothing from its environment. It uses
the one frozen target authority in Section 9.1; no target flag or host default
can change the meaning of these bytes:

```text
before.evo    the canonical rendering of the module as it entered optimization
after.evo     the canonical rendering of the module as it left
proof.evp     the certificate
```

It performs, in order:

1. Parse `before.evo` and `after.evo` under Section 9, rejecting either under
   Section 9.5 rather than repairing it.
2. Verify the certificate's input digest against `before.evo`'s bytes.
3. Recompute the ordered eligible regions and their witness and seed manifests
   from parsed `before.evo`, per Sections 8.5 through 8.7, using its own
   implementation. Verify that the certificate names a duplicate-free
   subsequence and that every named manifest is exact; an omitted eligible
   region is a permitted decision not to optimize it.
4. Replay every merge against the trusted registry of Section 11.6, recomputing
   constant folds rather than accepting claimed results.
5. Recompute the greedy cost and tie-breaking choices, compare the complete
   extraction map, and verify that its root is the final class of the original
   root node.
6. Reassemble the after-module per Section 8.8 and compare its canonical
   rendering to `after.evo` byte for byte.

Every step must pass. A certificate whose merges replay but whose reassembly
differs is invalid, as is one that names an unrelated root or output.

### 11.3 Certificate Format

`proof.evp` is an interchange format with its own grammar, not a serialization of
any implementation's type. A format defined by whatever a producer's struct
happens to hold cannot be validated by a second implementation, and malformed
becomes indistinguishable from invalid: a reader that fails to decode a field
cannot say whether the bytes were wrong or the claim was.

It must nonetheless carry every fact a replay needs. An earlier draft named a
node by its opcode alone, which cannot express a replay at all: without node
identity, owner class, children, and the substitution a rule was applied under,
there is nothing to re-derive.

**Structure.** Line oriented, one record per line, `\n` terminated, ending with a
final newline. Fields are separated by a single space. No field contains a space:
types are written in the Section 9 spelling with every internal space removed, so
`vec<2, bits<32>>` appears as `vec<2,bits<32>>` and a reader restores the spacing
when comparing against a rendered module.

```text
certificate     := header region_block* output_line
header          := version_line registry_line input_line budget_line?
version_line    := "evp1" newline | "evp2" newline
budget_line     := "budget nodes=" evp_int "/" evp_int " epochs=" evp_int "/" evp_int
                   " fuel=" evp_int " stop=" stop_reason newline
stop_reason     := "saturated" | "no-new-best" | "epochs" | "nodes" | "fuel"
registry_line   := "registry " ident " " digest newline
input_line      := "input " digest newline
output_line     := "output " digest newline

region_block    := region_line witness_line* seed_line* event_line*
                   extract_line+ root_line region_end
region_line     := "region " region_id " root " value_name
                   " witnesses " evp_int " seeds " evp_int newline
witness_line    := "witness " evp_int " " region_id newline
seed_line       := "seed " evp_int " " value_name " " packed_type " "
                   seed_origin " " eclass newline
event_line      := insert_line | rewrite_line | congruence_line | epoch_line
epoch_line      := "epoch " evp_int newline
region_end      := "endregion" newline

insert_line     := "node " evp_int " " node_id " " eclass
                   " source=" node_source " op=" op_id " sort=" sort
                   " effect=" effect_class " attrs=" attributes
                   " children=" children newline
rewrite_line    := "rewrite " evp_int " " rule_id " " eclass " " eclass " "
                   eclass " " proof_ref " " proof_ref " bind " bindings
                   premises? newline
premises        := " premises " premise ("," premise)*
premise         := eclass ":" hex_digit+ ":" hex_digit+   -- class, known mask, known value
congruence_line := "congruence " evp_int " " eclass " " eclass " " eclass " "
                   node_id " " node_id newline
extract_line    := "extract " evp_int " " eclass " " extraction newline
root_line       := "root " eclass newline

children        := "-" | eclass ("," eclass)*
bindings        := "-" | binding ("," binding)*
binding         := variable ":" eclass
variable        := "?" ident
attributes      := "-" | attribute ("," attribute)*
attribute       := ident "=" evp_int
extraction      := "seed:" evp_int | "node:" node_id
proof_ref       := "class:" eclass | "node:" node_id
node_source     := "before" | "derived"
op_id           := "const" | "add" | "sub" | "mul" | "and" | "or" | "xor"
                 | "icmp.eq" | "icmp.ne" | "icmp.slt" | "icmp.sle"
                 | "icmp.ult" | "icmp.ule"
sort            := packed_type
effect_class    := "pure" | "guard" | "effectful"
node_id         := "n" evp_int
eclass          := "c" evp_int
region_id       := ident "/" ident "/" evp_int
seed_origin     := "input" | "definition"
rule_id         := ident
digest          := 64 * hex_digit
packed_type     := a Section 9 type with every space removed
evp_int         := "0" | nonzero_digit digit*
```

**What each record carries and why.**

| Record | Content | Why a replay needs it |
|---|---|---|
| `registry` | Registry name and digest | The authority the rules came from. A reader without that exact registry rejects; a certificate cannot introduce one. |
| `input` | Digest of `before.evo` | Binds the claim to one module. |
| `region` | Identity, root value, witness count and seed count | Lets a reader match its own recomputation against the claim. |
| `witness` | Position and instruction identity from the witness closure | Prevents the producer from hiding or adding semantic context around the one replaced root. |
| `seed` | Position, value, type, origin, and the class the seed was inserted into | Without the class, later events cannot be tied to the module's values. |
| `node` | Event number, node identity, fresh owner class, source, operator, sort, effect class, canonical attributes, and child classes | The complete e-node as inserted. In particular `const` requires `value=RAW`; sort alone cannot distinguish zero from one. |
| `rewrite` | Event number, rule, `lhs_pre`, `rhs_pre`, deterministic result, left and right proof references, and the substitution | A rule with variable patterns is meaningless without its matched node/class and bindings; a RHS variable legitimately names a class rather than inventing a leaf node. |
| `congruence` | Event number, `lhs_pre`, `rhs_pre`, deterministic result, and the two congruent nodes | Congruence closure joins classes without any rule firing; a format carrying only rewrites cannot reproduce the partition. |
| `extract` | Position, final e-class, and its chosen seed or node | The explicit e-class-to-representation map removes ambiguity about which child representation a selected parent uses. |
| `root` | Final canonical class of the original root | Binds the extracted subgraph back to the value the region is permitted to replace. |
| `output` | Digest of the reassembled after-module | Binds the claim to a result. |

**Ordering and identities.** Region blocks are a duplicate-free subsequence of
Section 8.6 order. Witnesses and seeds appear in manifest order, counting from
zero. Every node, rewrite, and congruence record is one event; their shared event
numbering starts at zero and is consecutive, so insert/union interleaving is
replayable. Node IDs are `n0`, `n1`, ... in insertion order.

Seed e-classes are `c0`, `c1`, ... in seed order. With `S` seeds, node `nI`
owns fresh class `c(S+I)`. Every node event allocates that class even if an equivalent node already exists; equality with an
existing node requires a later congruence event. A union's endpoints must be the
two distinct canonical roots immediately before that event and its result must
be their numerically smaller ID. Child and binding classes are resolved through
the partition at the event where they are consumed. Attribute keys and binding
names are strictly ascending; a reader rejects rather than normalizes them.

Node and e-class identifiers in EVP1 are certificate-local and restart in every
region block. They are not the persistent `EClassId` handles of Section 8.9.
The producer snapshots the relevant persistent subgraph into this canonical
local numbering; proof validity can never depend on two implementations having
allocated the same persistent handle. The persistent state's own append-only
history still uses its stable handles and must independently replay to the
partition it stores.

The operator table is exact. `const` has zero children and the single attribute
`value=RAW`, where `RAW` is unsigned decimal, has no leading zero except `0`, and
fits its `bits<N>` sort. Arithmetic and bitwise nodes have two children of their
result sort and `attrs=-`. Comparison nodes have two children of one common
`bits<N>` sort, result sort `bits<1>`, and `attrs=-`. Every P0 node has
`effect=pure`; `guard` and `effectful` are parseable so a mutation is classified
as well-formed-but-invalid rather than malformed, but neither is admissible.

For a rewrite, each proof reference must belong to its corresponding pre-merge
endpoint. `class:cI` is permitted only when that side of the instantiated rule
is a bare variable bound to `cI`; an operator pattern or builder requires
`node:nI`. The binding names and classes must be exactly the complete variable
set of the named rule, with no omission or extra binding; `bindings=-` is
therefore parseable but invalid for every rule in `p0.scalar.v1`. The checker
matches the complete nested left pattern and independently
constructs or identifies the right side under the recorded substitution. For a
congruence event, the two nodes must have identical operator, sort, effect class,
attributes and arity, and every pair of corresponding child classes must already
have the same canonical root before the event.

All `source=before` nodes form the exact witness manifest and precede every
`source=derived` node. A before node's operator, sort, attributes and children
must equal the independently parsed witness instruction. The `I`th before node
corresponds to witness `I`, and its owner class is the initial class of that
instruction's single result. Every derived node must
be named by a later rewrite as its right-hand `node:` proof reference; unused derived nodes are
invalid. This prevents an event stream from manufacturing an unproved
representation and relying on extraction to select it.

Extraction entries count from zero and are topological. A selected
`source=before` witness that is not the replacement root is a terminal reuse:
its instruction remains at its earlier position, it emits no instruction at the
root, and its children have no extraction entries. Every other selected node's
child, after canonicalizing its e-class, has exactly one earlier extraction
entry. A `seed:I` choice must belong to that final class; a `node:nI` choice must
name a node whose owner belongs to it. No final class occurs twice. Starting at
`root`, the selected child closure under those terminal-reuse rules must equal
the complete set of extraction entries, so unrelated choices cannot be
smuggled into reconstruction. Seeds and terminal before-witness reuses emit no
instruction; the replacement root's selected `source=before` node and selected
`source=derived` nodes emit in extraction order at the original root's position.
The root class must be the final class of the original root node.

The checker recomputes the region-relative cost from Section 8. A seed, a
`const` node, or a non-root before-witness reuse costs zero. The replacement
root's non-constant before node and every non-constant derived node cost one plus
the sum of selected child costs; repeated child positions are counted repeatedly.
Giving constants zero cost is semantic, not cosmetic: otherwise
`icmp.eq x, x` and its `true` replacement both cost one, making every identity
comparison rule in the reviewed registry unreachable under strict improvement.
A region block is
valid only when the extracted root has strictly lower cost than the original
witness root. Ties retain the original and therefore produce no region block.

**Versions.** `evp1` is the previous revision's format and remains readable:
it carries no budget line, no epoch markers and no premises, so a checker
validates it without a scheduler claim and admits only rules that need no
premise. `evp2` adds the three. A reader that does not implement a version
rejects it as malformed rather than guessing. A Tier B rewrite requires `evp2`,
because a fact-driven rewrite without its premise is not a claim a checker can
verify.

**Premises.** Each premise names a class and the known-bits fact the rule relied
on. Mask and value have the same digit count, which is the class's width in
bits rounded up to a whole hexadecimal digit; a premise whose two fields differ
in length, or whose value has a one where the mask has a zero, is malformed. The checker recomputes the fact from the region's nodes with its own
implementation of the Section 8.10 transfers and refuses the rewrite if the
recorded premise is not at or below the recomputed fact. Facts are recomputed
rather than trusted for the reason Section 11.6 gives: a shared analysis that
was wrong would make producer and checker agree on an unsound rewrite.

**Epoch markers.** The scheduler runs per transaction and a certificate is
organised per region, so markers are written per region: within a region block,
an `epoch k` line precedes the events that epoch produced for that region, and
`k` is strictly increasing within the block. A region touched in no epoch has no
markers. The checker verifies monotonicity within each block and that no `k`
exceeds the recorded epoch count; it does not require every epoch to appear in
every region, since most epochs touch few regions.

**Budget.** The budget line records each bound, the amount consumed, and the
stop reason. The checker verifies consumed does not exceed bound, that no epoch
marker exceeds the recorded epoch count, and that a stop reason of `epochs`,
`nodes` or `fuel` coincides with that quantity having reached its bound. It does not verify `saturated` or `no-new-best`, which would require
re-running the scheduler; those are telemetry.

**Zero regions.** Any compilation that accepts no rewrite produces a certificate
with a header and an `output` line and no region blocks, whether or not eligible
candidates existed. This is valid only when reassembly is byte-identical to
`before.evo`. It is not an empty file: budgets and optimization policy may always
choose the baseline representation, because optimization is never required for
correctness.

**Digests.** SHA-256, lowercase hexadecimal. The input digest covers `before.evo`
in full, including its final newline when the module is nonempty. The output digest covers the reassembled
module rendered under Section 9.4, not the bytes of `after.evo`; a reader whose
output digest matches while those bytes differ has found a producer that rendered
non-canonically, which is a distinct finding from an unsound rewrite.

The registry digest covers the exact canonical bytes in Section 11.6, including
every condition and the final newline. A serialization that omitted conditions
would let a producer and checker attach different meanings to the same digest.

**Rejection.** Anything not derivable from the grammar is malformed: an unknown
version, a field count that does not match its record, a digest that is not
exactly sixty-four lowercase hexadecimal characters, a sequence number out of
order, a witness or seed count disagreeing with the lines that follow, attributes
or bindings out of ascending order, a region block without `root` and
`endregion`, a trailing blank line, or any byte after the final newline. A
certificate that parses but violates identity allocation, node shape, replay,
extraction, registry, digest, or module binding is well-formed but invalid.
Section 11.4 gives each category its own exit status.

### 11.4 Exit Statuses

```text
0    valid
2    malformed input        -- the bytes are not a module or not a certificate
3    well-formed but invalid -- it parses and the claim does not hold
```

The two failure statuses are distinct because they are acted on differently: a
malformed file is a producer or transport defect, while a well-formed invalid
one is a claim that was checked and refused. Collapsing them would hide the
second inside the first during a campaign, which is precisely when the
difference matters. Any other status is itself a defect; the checker does not
exit non-zero for reasons outside this table.

### 11.5 Dependency Boundary

This is a normative constraint, not a convention, and is enforced by a test that
reads the dependency closure.

```text
evocc-evoir        IR types, Section 9 parser and renderer, validator, interpreter
                   must not depend on the e-graph

evocc-eqopt        the optimizer and the equality state
                   depends on evocc-evoir, the e-graph, and the proof crate

evocc-proofcheck   depends on evocc-evoir and evocc-proof only
                   must not depend on evocc-eqopt or on the e-graph
```

An IR crate that depends on the e-graph places the optimizer in the transitive
closure of everything that reads IR, including the checker, which would dissolve
the boundary while every direct dependency still looked correct.

### 11.6 What Is Shared and What Is Not

The boundary is deliberately partial, and the line is drawn by which direction a
shared defect pushes the verdict.

**Shared: the Section 9 parser.** A parser defect makes the checker's view
differ from the producer's, so reassembly disagrees and the certificate is
*rejected*. Sharing fails safe, and duplicating a parser the interpreter needs
anyway would add a second thing to drift.

**Not shared: eligibility and seed derivation.** A common defect here makes both
sides agree that an ineligible region was eligible, and the certificate is
*accepted*. Sharing fails dangerous. The checker implements Sections 8.5 through
8.7 for itself.

**Not shared: the rule registry.** The trusted P0 rules live in the checker.
Rules are stated with variable patterns and explicit constant side conditions,
so a rewrite is accepted only when it instantiates a registered rule. A
certificate commits to the registry and cannot extend it; a certificate that
supplies the rules it is checked against establishes only that its recorded
merges follow from its own recorded claims.

**Reviewed registry `p0.scalar.v1`.** This registry is normative. Its digest is
`ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9`, the
SHA-256 of the bytes in the following code block
starting at the first `p0_` and including the newline after the last rule:

```text
p0_001_add_zero_rhs add(?x,const_like(?x,0)) => ?x when true
p0_002_add_zero_lhs add(const_like(?x,0),?x) => ?x when true
p0_003_mul_one_rhs mul(?x,const_like(?x,1)) => ?x when true
p0_004_mul_one_lhs mul(const_like(?x,1),?x) => ?x when true
p0_005_and_zero_rhs and(?x,const_like(?x,0)) => const_like(?x,0) when true
p0_006_and_zero_lhs and(const_like(?x,0),?x) => const_like(?x,0) when true
p0_007_or_zero_rhs or(?x,const_like(?x,0)) => ?x when true
p0_008_or_zero_lhs or(const_like(?x,0),?x) => ?x when true
p0_009_double_neg sub(const_like(?x,0),sub(const_like(?x,0),?x)) => ?x when true
p0_010_icmp_eq_self icmp.eq(?x,?x) => bool(1) when true
p0_011_icmp_ne_self icmp.ne(?x,?x) => bool(0) when true
p0_012_icmp_slt_self icmp.slt(?x,?x) => bool(0) when true
p0_013_icmp_sle_self icmp.sle(?x,?x) => bool(1) when true
p0_014_icmp_ult_self icmp.ult(?x,?x) => bool(0) when true
p0_015_icmp_ule_self icmp.ule(?x,?x) => bool(1) when true
p0_100_fold_add add(?a,?b) => fold.add(?a,?b) when all_const
p0_101_fold_sub sub(?a,?b) => fold.sub(?a,?b) when all_const
p0_102_fold_mul mul(?a,?b) => fold.mul(?a,?b) when all_const
p0_103_fold_and and(?a,?b) => fold.and(?a,?b) when all_const
p0_104_fold_or or(?a,?b) => fold.or(?a,?b) when all_const
p0_105_fold_xor xor(?a,?b) => fold.xor(?a,?b) when all_const
p0_106_fold_icmp_eq icmp.eq(?a,?b) => fold.icmp.eq(?a,?b) when all_const
p0_107_fold_icmp_ne icmp.ne(?a,?b) => fold.icmp.ne(?a,?b) when all_const
p0_108_fold_icmp_slt icmp.slt(?a,?b) => fold.icmp.slt(?a,?b) when all_const
p0_109_fold_icmp_sle icmp.sle(?a,?b) => fold.icmp.sle(?a,?b) when all_const
p0_110_fold_icmp_ult icmp.ult(?a,?b) => fold.icmp.ult(?a,?b) when all_const
p0_111_fold_icmp_ule icmp.ule(?a,?b) => fold.icmp.ule(?a,?b) when all_const
```

EVP1 accepts no other registry name or digest. The producer and checker each
implement these rules from this specification; the checker does not import the
producer's registry table, and the certificate carries only the fixed name and
digest rather than rule definitions.

Variables bind e-classes and repeated variables require the same canonical
class. `const_like(?x,K)` matches or constructs a `const` node with the same
`bits<N>` sort as `?x` and raw value `K mod 2^N`; `bool(K)` constructs
`const.bits<1> K`. `all_const` succeeds only when every bound operand class has
one unambiguous constant value of the required common sort. A class containing
distinct constant payloads is invalid rather than a choice for the checker.

On a left-hand side `const_like` only matches a node already present in the
before witness closure. On a right-hand side it may identify such a node or a
`source=derived` node whose payload the checker recomputes. `bool` and `fold.*`
are right-hand builders and therefore require a `node:` proof reference.

`fold.add`, `fold.sub`, and `fold.mul` compute modulo `2^N`; the bitwise folds
operate on the exact `N` raw bits. Equality and unsigned comparisons use those
raw values. Signed comparisons decode each raw operand as an `N`-bit two's
complement integer. Comparison builders return `bits<1>`. The checker performs
this calculation itself and compares the resulting derived node's sort and
`value` attribute; the certificate never supplies a trusted fold result.

**Residual risk, recorded rather than implied.** The certificate format
definition remains shared between producer and checker, so a defect in that
definition is invisible to both. Eliminating it would require a second
implementation of the format. It is bounded instead by a mutation campaign whose
property is that **no mutant of the optimizer produces a semantically different
module together with an accepted certificate** — not that every mutant is
rejected, since a mutant that merely disables a rewrite emits a valid empty
certificate and should pass.

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
| Pointer | `ptr.null` | `ptr<AS>` or `fnptr<Sig>` | — | no |
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
| Pointer | `func_addr @symbol` | `fnptr<Sig>` | symbol | no |
| Calls | `call @callee(args..., %m, %w)` | ret values, `mem`, `world` | callee, args, memory, world | depends |
| | `call %fptr(args..., %m, %w)` | ret values, `mem`, `world` | callee value, args, memory, world | depends |
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
            | "vec" "<" Integer "," " " Type ">"
            | "tuple" "<" TypeList ">"
            | "fnptr" "<" Signature ">"
            | "void"

Signature  := "(" TypeList ")" "->" "(" TypeList ")"

TypeList   := ε
            | Type ("," " " Type)*

AddressSpace := "generic" | "global" | "stack" | "threadlocal"

Region     := "all" | "unknown" | "global" | "stack" | "heap" | "escaped"

Integer    := [1-9] [0-9]*
```

---

## Appendix C: Revision History

| Revision | Date | Changes |
|---|---|---|
| 1.9 | 2026-08-25 | Freezes the GNU extension profile the kernel cohort needs. Attributes are read in every GNU position, attached by position, and preserved losslessly; only `aligned` and `packed` are applied, to layout, with every rule checked against the frozen GCC-14 and Clang-19 oracles, and every other attribute is recorded by name as preserved-not-applied. Defines `typeof` as an unevaluated front-end type, statement expressions as a lowering pattern over block arguments with block lifetimes, designated initializers including GNU ranges as `SubobjectPath` actions with later-wins overriding, and compound literals as manifest allocation sites. Adds the known-bits lattice with constant value as its bottom, a fixed weak transfer table, meet-on-union with contradiction as an invalid union, and a two-tier bounded scheduler with per-rule match fuel and a stop rule. Certificate format `evp2` carries budget, epoch markers and rewrite premises; premises are recomputed by the checker, budgets are verified against consumption, and `evp1` remains readable for premise-free certificates. Introduces `E2903`–`E2906`. Defers every other attribute semantic, variably modified types, integer-range and non-zero lattices, and any memory rewrite. |
| 1.8.2 | 2026-08-24 | Records implementation alignment for the complete Section 9 function, global, `func_addr`, and indirect-call shapes; empties the pending and superseded shape manifests without changing their machine-readable contract. Clarifies that shape comparison is an audit hint while whole-module round-trip and the independent field-sentinel manifest carry syntax completeness. |
| 1.8.1 | 2026-08-24 | Clarifies the Day 8 P0 extractor after the first standalone-corpus replay exposed an ambiguity: cost is region-relative newly emitted instruction count with the existing zero-cost `const` convention; selecting a non-root `source=before` witness reuses its earlier same-block SSA value, emits nothing and terminates extraction closure, while the replacement root remains structural and non-constant derived nodes are emitted. Records the mandatory root exclusion, the no-register-pressure scope, and the existing cross-block prohibition so a false-green zero-rewrite implementation cannot satisfy the corpus gate. |
| 1.8 | 2026-08-24 | Makes the textual form load-bearing rather than a debugging convenience: Section 9 becomes a byte grammar with explicit separators, rejection rules, canonical empty-list/positional-binder spellings, complete effects/global forms and deterministic rendering. Freezes independently derived P0 eligibility as one replacement root plus a same-block witness closure, poison/effect fixed points, traversal order, witness/seed manifests, optional accepted-region subsequences and mechanical reassembly. EVP1 carries exact variable bindings, constant payloads, before/derived node origin, complete deterministic merge receipts, explicit extraction wiring and a reviewed `p0.scalar.v1` registry whose digest covers side conditions; constants cost zero so identity comparisons are real strict improvements, certificate IDs are local snapshots, and history-only persistent nodes cannot become unrecorded premises. Adds `fnptr<Sig>`, `func_addr`, indirect calls and `E3015`–`E3017`/`E3019`; gives function pointers explicit Function/Null provenance; assigns `E3018` uniquely to invalid lifetime end; distinguishes file, function-static and string-literal globals so generated identities and visibility are validator-checkable; and makes Section 4.6 the sole call-effect authority with exact validator recomputation. Defines canonical persistent-state identity, revision behavior and one joint outer optimization transaction, fixes the checker interface and crate boundary, and records which components are shared by fail-safe direction. Defers proof minimisation, alias-dependent rewrites, CFG region splitting and attribute-derived summaries. |
| 1.7 | 2026-08-22 | Defines the Day 7 `(ObjectId, signed byte_offset)` provenance model, immutable initialized/lifetime-aware `mem<all>` snapshots, module globals and automatic objects, state-preserving pointer/access guards with exact `E3001`/`E3009`–`E3014` diagnostics, and observable volatile world traces. It freezes executable pointer/memory syntax while deferring pointer-valued static relocations, pointer/integer conversions, region splitting, alias-dependent memory rewrites, atomics, heap-library semantics, and backend lowering. |
| 1.6 | 2026-08-18 | Makes `--emit=evoir` the canonical driver spelling, binds it to the exhaustive Section 9 renderer, names the first stored block `^entry` independently of numeric block identity, and corrects the complete example to use the supported `icmp.ult` comparison. |
| 1.5 | 2026-08-18 | Defines transportable Day 6 scalar `uninitialized.bits<N>` poison with consume-time `E3001`, distinct from Day 7 memory initializedness; adds the zero-operand `trap.scalar_function_fell_off_end` terminator and exact `E3007` behavior without placeholder returns or effect tokens. |
| 1.4 | 2026-08-18 | Requires every module-level call/effect participant, including callers, to use the canonical trailing `mem<R>, world` parameter and return interface; makes the UB reason mandatory semantic metadata; assigns the five Day 6 integer reasons exactly the C-HIR `E3002` through `E3006` diagnostics; and freezes executable remainder, shift, bitwise, and non-conflating div/rem/shift predicate shapes, including independently promoted shift-amount widths and signedness. |
| 1.3 | 2026-08-18 | Adds module validation and explicit heap-backed call-frame execution for direct calls and recursion under one shared step budget; records typed signed-overflow `ub.assume` failure; and names the concrete target data-layout service used by current lowering. |
| 1.2 | 2026-08-18 | Freezes named memory/world input and output token positions for `load`, `store`, and `call`; makes token flow region-polymorphic while Day 6 lowering uses conservative `mem<all>`; permits equal-width identity conversions; separates function-local SSA/effect validation from module call-signature validation; and records the bounded, typed Day 6 interpreter subset. |
| 1.1 | 2026-08-18 | Makes formal parameters non-SSA call-interface binders, requires real unique `Value` IDs on entry and non-entry block arguments, requires all cross-block values to be passed explicitly, types the signed add/subtract/multiply definedness predicates used by lowering, and corrects the Section 9 examples. |
| 1.0 | 2026-08-10 | Initial specification. Defines the complete EvoIR semantic contract for Day 4 of the 30-day EvoCC build. |
