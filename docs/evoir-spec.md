# EvoIR Specification

**Revision 1.5** | **Authoritative reference** | **2026-08-18**

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

A pointer is a typed reference into one address space.

```text
ptr<generic>         -- default address space
ptr<global>          -- global/static storage
ptr<stack>           -- automatic storage
ptr<threadlocal>     -- thread-local storage
```

Address spaces are disjoint: a `ptr<stack>` cannot alias a `ptr<global>`.
This fact is available to the EqSat agent for memory disambiguation.

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

The safe starting point is `mem<all>`. Region splitting (to `mem<stack>` +
`mem<heap>` + ...) is an optimization enabled by alias analysis.

### 1.5 World Token

The `world` type is a singleton. The world token `%w: world` is a global
ordering token. The Day 6 instruction layout threads it through every memory
operation and call; volatile, atomic, and IO operations give that flow an
observable scheduling-barrier meaning across all regions. A normal memory
operation still produces a fresh SSA identity while preserving the same
abstract world state.

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

### 2.1 Function Structure

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

```
%v = ptr.offset %base, %offset
```

| Field | Description |
|---|---|
| `%base` | `ptr<AS>` value. |
| `%offset` | `bits<N>` value representing a byte offset. |

**Semantics.** Computes a pointer offset from `%base` by `%offset` bytes.
The result is a `ptr<AS>` with the same address space and provenance as
`%base`. The offset is treated as a signed value added to the byte address.

This is an arithmetic operation on pointers. It does not access memory. The
result may point outside any valid object; dereference is only defined
when the resulting pointer is within a valid object.

### 3.8 Memory Operations

```
(%v, %m1, %w1) = load %m0, %w0, %ptr : Type
(%m1, %w1) = store %m0, %w0, %ptr, %val
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

**Semantics.**

`load(%m0, %w0, %ptr)`: returns the value of type `Type` stored at address
`%ptr`, plus explicitly threaded `%m1` and `%w1` tokens. The load is
non-volatile and non-atomic. It is undefined
behavior if `%ptr` does not point to a valid, initialized object of at least
the size and alignment required by `Type`.

`store(%m0, %w0, %ptr, %val)`: returns a new memory token `%m1` that is
identical to `%m0` except that the value at address `%ptr` has been replaced
with `%val`, plus the explicitly threaded `%w1`. Day 6 conservatively uses
one `mem<all>` token; later region refinement does not change this instruction
layout.

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

**Call attributes.** The callee's effect summary determines what the caller
can assume about memory and world state after the call. See Section 4.

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
| `ptr.offset` | `{}` | `{}` | false | false | false | false | false |
| `load` (normal) | `{R}` | `{}` | false | false | false | true | false |
| `store` (normal) | `{}` | `{R}` | false | false | false | true | false |
| `call` | depends on callee | depends on callee | depends | depends | depends | depends | depends |
| `trap.scalar_function_fell_off_end` | `{}` | `{}` | false | false | false | true | false |

For `load` and `store`, the region `R` is determined by the memory token's
region annotation and refined by pointer analysis.

### 4.3 World Token Semantics

The `world` token `%w` is a global ordering token. Day 6 reserves world input
and output positions on every memory instruction. A normal non-observable
access forwards the same abstract state through a distinct SSA token;
instructions that carry `volatile = true`, `atomic = true`, or `io = true`
use that token as an ordering constraint.

```text
; Volatile load pattern
(%val, %m1, %w1) = vload %m, %w, %ptr : Type
```

The world token acts as a **scheduling barrier**. Two operations that do not
touch the same region and do not touch `world` can be reordered with respect
to each other. Two operations that consume/produce `world` cannot be reordered
past each other.

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

### 5.2 Alias Facts

Alias analysis produces facts of three kinds:

```text
AliasResult ::=
    MustAlias(p, q)    -- p and q point to the same storage
  | NoAlias(p, q)      -- p and q point to non-overlapping storage
  | MayAlias(p, q)     -- relationship unknown
```

Alias facts are proof-carrying: they are recorded in the analysis lattice
and cited by rewrite justifications.

### 5.3 Fundamental Memory Rewrite Rules

**Store-Forward (same location):**

```text
load(store(M, p, x), p) -> x
```

Provided that:
- `p` and the access in `load` have the same address and compatible width
- The access is not volatile or atomic
- The access is defined (the pointer is valid)

**Store-Bypass (non-aliasing):**

```text
load(store(M, p, x), q) -> load(M, q)
```

Requires: `NoAlias(p, q)`, both accesses are non-volatile, non-atomic.

**Redundant Load:**

```text
load(M, p) where a preceding load(M, p) produced value v -> reuse v
```

Requires: `M` is the same memory token (no intervening store to aliasing
location).

### 5.4 Pointer Provenance

A pointer carries two components in the abstract semantics:

```text
Ptr {
    address: bits<PTR_WIDTH>,
    provenance: Provenance,
}
```

**Provenance** tracks the origin of a pointer: which allocation it derives
from (following ISO/IEC TS 6010). Two pointers with the same concrete address
but different provenance are **not interchangeable**.

```text
ptrtoint(p) == ptrtoint(q)   does NOT imply   p == q
```

Provenance exists in the abstract semantics and analysis database. It does
not occupy bits at runtime. The production backend erases provenance only
after legality has been established.

### 5.5 Region Splitting

The safe starting point is one global memory region: `mem<all>`. Region
splitting is an optimization enabled by proved alias facts.

When the analysis determines that a `ptr<stack>` never aliases any `ptr<heap>`,
the memory can be split:

```text
Original:    %m: mem<all>

Split:       %m_stack: mem<stack>
             %m_heap:  mem<heap>
```

Operations on stack pointers consume and produce `%m_stack`; operations on
heap pointers consume and produce `%m_heap`. This enables more precise
reordering and eliminates spurious memory dependencies.

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
| Null pointer dereference | `p == null` | reserved for the Day 7 memory model |
| Out-of-bounds access | `p` outside object bounds | reserved for the Day 7 memory model |
| Use of uninitialized scalar SSA value | consume a value defined by `uninitialized.bits<N>` | Day 6 interpreter reports `E3001` at the consuming instruction or terminator |
| Read from uninitialized memory storage | load from storage whose initializedness is false | reserved for the Day 7 memory model |
| Strict aliasing violation | incompatible effective types | reserved for the Day 7 memory model |

`uninitialized.bits<N>` defines an ordinary poison value with a real SSA
identity. Branch operands and block arguments may copy that poison without
trapping, which lets definite assignment on a later path overwrite it. Any
semantic consumption—arithmetic, comparison, conversion, branch condition,
ordinary call argument, or return—reports `E3001 read-of-uninitialized-object`.
It is not a zero value and it must never be silently replaced by one. This
Day 6 rule models scalar automatic locals only; initializedness of memory,
pointers, aggregates, globals, and heap objects remains Day 7 work.

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
| Alignment facts | `%p` is 8-byte aligned | `__attribute__((aligned))` |
| Provenance facts | `%p` has provenance from `malloc` | Allocation site |
| UB assumptions | `signed_add_defined(%x, %y)` | `ub.assume.signed_overflow` |
| No-overflow facts | `add_defined(%x, %y)` | UB assumption propagation |
| Alias facts | `NoAlias(%p, %q)` | Alias analysis |

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
- `--emit-evoir` compiler output
- Debugging and inspection

```text
func @name(%param1: Type1, ..., %paramN: TypeN, %mem: mem<R>, %w: world)
    -> (RetType1, ..., RetTypeK, mem<R>, world)
    cc = calling_convention
{
^block_label(%arg1: Type1, ...):
    %value = op.type_args operand1, ..., operandN
    %value = uninitialized.type
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
- `@name` is a function reference
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
    %done = icmp.uge %i_loop, %n_loop
    cond_br %done, ^exit(%acc_loop, %m_loop, %w_loop), ^body(%p_loop, %n_loop, %i_loop, %acc_loop, %m_loop, %w_loop)

^body(%p_body: ptr<generic>, %n_body: bits<64>, %i_body: bits<64>, %acc_body: bits<32>, %m_body: mem<all>, %w_body: world):
    %addr = ptr.offset %p_body, %i_body
    (%x, %m_loaded, %w_loaded) = load %m_body, %w_body, %addr : bits<32>
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
| `T*` | `ptr<AS>` where `AS` depends on the pointee's storage duration |
| `void` (in return position) | `void` (empty return type) |

The mapping is mediated by the target data-layout service. The current pinned
AArch64 implementation is `TargetDataLayout`, which provides `layout_of` and
`record_layout` queries; lowering must not hard-code target widths.

#### 10.1.2 Qualifier Lowering

| C Qualifier | EvoIR Representation |
|---|---|
| `const` | No IR effect. Purely a C-HIR check. |
| `volatile` | `load`/`store` carry `volatile=true` in their effect summary. |
| `restrict` | Enables `NoAlias` facts in the analysis context `Γ`. |
| `_Atomic` | Lowered to atomic operations (`atomic.load`, `atomic.store`, etc.). |

#### 10.1.3 Lvalue Lowering

C lvalues lower to EvoIR pointer operations:

```text
Read:   %ptr = ptr.offset %base, %field_offset
        (%val, %m1, %w1) = load %m0, %w0, %ptr : Type

Write:  %ptr = ptr.offset %base, %field_offset
        (%m1, %w1) = store %m0, %w0, %ptr, %val
```

The base pointer and field offset are computed from the field placements
returned by `TargetDataLayout::record_layout`.

#### 10.1.4 Sequencing Lowering

C sequence points lower to memory token threading and world token threading.
The lowered EvoIR must preserve the observable ordering of operations as
defined by the C abstract machine:

```text
; x = y + z;
; w = load *p;

%tmp = add %y, %z                                      ; first expression
(%m1, %w1) = store %m0, %w0, %x_addr, %tmp
(%val, %m2, %w2) = load %m1, %w1, %ptr                ; sees the store
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
| `*p` (dereference) | Day 7 memory-model obligation → `load %m, %w, p : Type` |
| `p[i]` | Day 7 memory-model obligation → memory access |

The lowering agent must not elide UB checks. The EqSat agent may later prove
them redundant and eliminate them.

#### 10.1.6 Aggregate Lowering

C struct and union types lower to explicit offset computations:

```text
; struct { int a; char b; } s;
; s.b = 42;

%a_addr = ptr.offset %s_base, const.bits<64> 0           ; offset of field a
%b_addr = ptr.offset %s_base, const.bits<64> 4            ; offset of field b
%val    = const.bits<8> 42
(%m1, %w1) = store %m0, %w0, %b_addr, %val               ; store to s.b
```

Bitfields lower to load-modify-store sequences using bitwise operations:

```text
; struct { unsigned f : 3; } s;
; s.f = 5;

(%old, %m1, %w1) = load %m0, %w0, %s_addr : bits<8>      ; load containing byte
%masked = and.bits<8> %old, const.bits<8> 0b11111000     ; clear field bits
%shift  = shl.bits<8> const.bits<8> 5, const.bits<8> 0   ; shift value into place
%new    = or.bits<8> %masked, %shift                      ; insert field
(%m2, %w2) = store %m1, %w1, %s_addr, %new               ; write back
```

#### 10.1.7 Function Call Lowering

C function calls lower to EvoIR `call` instructions. The effect summary is
derived from the callee's attributes:

| C Attribute / Declaration | Effect |
|---|---|
| `__attribute__((pure))` | reads memory, writes nothing |
| `__attribute__((const))` | reads nothing, writes nothing |
| `__attribute__((noreturn))` | `may_unwind = false`, does not return (terminates the block) |
| No attributes (default) | reads and writes all memory, may have any side effect |

The default (conservative) effect summary for an unknown callee is:

```text
EffectSummary {
    reads: {All},
    writes: {All},
    volatile: false,
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
7. Each `mem<R>` and `world` token is consumed, replaced, and forwarded exactly
   once through each effect, edge, and return boundary

Module validation additionally checks each call's ordinary argument and return
types against the callee declaration. The function-local validator still
checks local use-def, ordinary-value versus effect-token positions, and exact
`mem<R>`/`world` threading without guessing a callee signature. Day 6 lowering
uses the conservative `mem<all>` region.

The Day 6 reference interpreter executes `bits<N>` values for `1 <= N <= 64`
and may forward opaque `mem<R>` and `world` tokens at block/function
boundaries. It advances explicit block and call-frame program counters and
charges one shared step budget for every instruction and terminator across
direct calls and recursion. Module validation resolves every direct callee and
checks the ordinary arguments/results plus the exact trailing effect-token
interface of every call/effect participant, including callers. Pointer,
floating, vector, tuple, and memory-access execution remain typed unsupported
until their scheduled semantic phases; they must never produce a placeholder
value.

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
| Comparison | `icmp.eq` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| | `icmp.ne` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| | `icmp.slt` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| | `icmp.sle` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| | `icmp.ult` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| | `icmp.ule` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| Extension | `sext.bits<N>` | `bits<N>` | `%a` : `bits<W>` (W <= N) | no |
| | `zext.bits<N>` | `bits<N>` | `%a` : `bits<W>` (W <= N) | no |
| | `trunc.bits<N>` | `bits<N>` | `%a` : `bits<W>` (W >= N) | no |
| Pointer | `ptr.offset` | `ptr<AS>` | `%base` : `ptr<AS>`, `%offset` : `bits<N>` | no |
| Memory | `load` | Type, `mem<R>`, `world` | `%m` : `mem<R>`, `%w` : `world`, `%ptr` : `ptr<AS>` | yes |
| | `store` | `mem<R>`, `world` | `%m` : `mem<R>`, `%w` : `world`, `%ptr` : `ptr<AS>`, `%val` : Type | yes |
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
| 1.5 | 2026-08-18 | Defines transportable Day 6 scalar `uninitialized.bits<N>` poison with consume-time `E3001`, distinct from Day 7 memory initializedness; adds the zero-operand `trap.scalar_function_fell_off_end` terminator and exact `E3007` behavior without placeholder returns or effect tokens. |
| 1.4 | 2026-08-18 | Requires every module-level call/effect participant, including callers, to use the canonical trailing `mem<R>, world` parameter and return interface; makes the UB reason mandatory semantic metadata; assigns the five Day 6 integer reasons exactly the C-HIR `E3002` through `E3006` diagnostics; and freezes executable remainder, shift, bitwise, and non-conflating div/rem/shift predicate shapes, including independently promoted shift-amount widths and signedness. |
| 1.3 | 2026-08-18 | Adds module validation and explicit heap-backed call-frame execution for direct calls and recursion under one shared step budget; records typed signed-overflow `ub.assume` failure; and names the concrete target data-layout service used by current lowering. |
| 1.2 | 2026-08-18 | Freezes named memory/world input and output token positions for `load`, `store`, and `call`; makes token flow region-polymorphic while Day 6 lowering uses conservative `mem<all>`; permits equal-width identity conversions; separates function-local SSA/effect validation from module call-signature validation; and records the bounded, typed Day 6 interpreter subset. |
| 1.1 | 2026-08-18 | Makes formal parameters non-SSA call-interface binders, requires real unique `Value` IDs on entry and non-entry block arguments, requires all cross-block values to be passed explicitly, types the signed add/subtract/multiply definedness predicates used by lowering, and corrects the Section 9 examples. |
| 1.0 | 2026-08-10 | Initial specification. Defines the complete EvoIR semantic contract for Day 4 of the 30-day EvoCC build. |
