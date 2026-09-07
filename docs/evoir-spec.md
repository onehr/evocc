# EvoIR Specification

**Revision 1.9.76** | **Authoritative reference** | **2026-09-07**

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
Sig ::= "(" TypeList [ ", ..." ] ")" "->" "(" TypeList ")"
```

A signature whose parameter list ends in `...` is **variadic**: a call may
pass further *anonymous* arguments after the listed ones (Section 3.10), and
the callee reads them with the instructions of Section 3.12. The marker sits
after the ordinary parameters and before the trailing `mem<R>` and `world`
positions, so `(bits<64>, ..., mem<all>, world)` is variadic and
`(bits<64>, mem<all>, world, ...)` is malformed. Two signatures are equal only
when they agree on the marker (revision 1.9.61).

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
| `fnptr<Sig>` | every parameter and result type is well-formed and is not `void`; `mem<R>` and `world` appear exactly once each, as the final two parameters and the final two results, in that order; no other position carries `mem<_>` or `world`; no type nested inside a parameter or result carries either; a `...` marker, when present, immediately precedes the `mem<R>` parameter |
| `void` | always well-formed |

---

## 2. Block-Argument SSA Semantics

### 2.1 Module and Function Structure

An EvoIR module contains an ordered global-object table, an ordered
**external-object table**, an ordered **external-function table** and an
ordered function table (the two external tables since revision 1.9.64),
followed by an ordered **module assembly table** (revision 1.9.66): each
entry is one file-scope `asm("template")` of the unit, its template text
kept verbatim in the escaping of Section 9. Module assembly defines
nothing EvoIR can see: the interpreters skip it, and a symbol it defines
that C code uses is an external symbol under the rules below. The machine
lane emits every entry verbatim, in table order, after all functions and
data (EvoMIR Section 8); the reference compilers also place file-scope
assembly at their own discretion relative to functions, so an entry may not
depend on what surrounds it, only on the symbols it names. The kernel's
`EXPORT_SYMBOL` is such an entry in nearly every unit. An
external object binds a symbol this module uses but does not define to a size
(or `unknown`, for an incomplete type) and an alignment; it has no bytes and
no initializedness here, because another unit supplies them. An external
function binds a symbol this module calls or takes the address of, but does
not define, to its complete lowered signature. Neither may be defined in the
same module (`E3041`, `symbol-declared-and-defined`), and each symbol appears
at most once across all four tables. A `call @f` or `func_addr @f` names
either a function or an external function; a `global_addr @g` names either a
global or an external object. A global object binds one unique symbol, stable source-origin
site, and global kind (`file`, `function_static`, `string_literal`, or
`compound_literal`) to a fixed size and
alignment, initial bytes, and a byte-for-byte initializedness bitmap. Module
interpretation allocates every global exactly once before entering the
requested function; `global_addr` then returns that object's stable
provenance. A duplicate global symbol or malformed initializer closure makes
the module invalid. Ordinary scalar, array, record, and string/byte
initializers are represented directly as bytes. A pointer-valued static
initializer is a **relocation** (revision 1.9.65): the global carries a list
`relocs=[(offset, @symbol, addend), ...]`, each naming eight bytes at an
8-aligned `offset` inside the object, a target and a signed byte `addend`.
A target is a symbol from any of the four tables (global, external object,
function, external function), or, since revision 1.9.69, a block of a
function of this module written `@function^block` (for example
`@main^b3`), whose address is the one `block_addr` yields (Section 3.7):
this is how a static initializer holding `&&label` lowers, and the machine
emits it as the block's local symbol (EvoMIR 0.15). Relocations are sorted by offset, do not overlap, and lie within
the size. At module load the eight bytes of each relocation hold the address
of the symbol plus the addend with that symbol's provenance (function
provenance for a function or external function), and are initialized;
`bytes=` for those positions are zero and `initialized=` bits there are one.
The list is omitted when empty, which keeps every existing module's text
unchanged. `(void *)(unsigned long)&f`, `&arr[3]` and `&sv.l` are all
symbol-plus-addend forms (measured: both oracles emit a symbol with an
addend for each, `.evidence/builtin-probes/20260902-2c23684c/p38-ptrint.c`,
`p38.out`). A static initializer that is not a symbol plus an addend (a
difference of two addresses, a pointer read through an integer that is not a
whole-pointer round trip) stays `E3026` (`not-lowered: static-relocation`).
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

**Bit counting and byte swap (revision 1.9.66).**

```
%v = ctlz.bits<N> %a
%v = cttz.bits<N> %a
%v = ctpop.bits<N> %a
%v = bswap.bits<N> %a
```

`ctlz` counts the leading zero bits of `%a`, `cttz` the trailing zero bits,
`ctpop` the set bits; each is total, answers `N` (or `0` for `ctpop`) for
`%a = 0`, and yields `bits<N>`. `bswap` reverses the byte order of `%a`;
`N` is 16, 32 or 64. None is in the Section 8.5 allowlist. They are the
lowering of the bit builtins of 10.1.14: `__builtin_clz(x)` is
`trunc.bits<32>` of `ctlz` after the front end asserts
`ub.assume.bit_builtin_zero_operand` on `icmp.ne %x, 0` (the C builtin is
undefined at zero, `E3022`); `__builtin_ffs(x)` is `0` when `%x` is zero
and `cttz + 1` otherwise, written with a `cond_br`; `__builtin_popcount`
is `ctpop`; `__builtin_bswap16/32/64` are `bswap`. Reference values, both
oracles (`.evidence/builtin-probes/20260902-2c23684c/p39-bits.c`, `p39.out`):
`clz(1u) = 31`, `ctz(8u) = 3`, `clzll(1ull) = 63`, `popcountll(~0ull) = 64`,
`ffs(0) = 0`, `ffs(16) = 5`, `bswap32(0x11223344) = 0x44332211`,
`bswap16(0x1234) = 0x3412`.

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
| `%a`, `%b` | For `eq`/`ne`, identical `bits<N>`, `ptr<AS>`, or `fnptr<Sig>` types. For ordered comparisons, `bits<N>` operands of the same width; an ordered comparison of two pointers is written by the front end as `ptr.to_int` of each followed by `icmp.ult`/`icmp.ule` on the addresses (revision 1.9.68: C's same-array obligation is carried by the C-HIR interpreter, as for pointer difference; both oracles print `15` for the same-array cases in `.evidence/builtin-probes/20260906-pointer-ordering/order.c`; `E3045`, the executor's fail-closed name, is withdrawn). |

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

**Poison of pointer type.** `uninitialized.ptr<AS>` and
`uninitialized.fnptr<Sig>` are the pointer forms of the poison value: an
automatic object of pointer type declared without an initializer holds one
until it is assigned. Their provenance is *absent*, so any dereference
reports `E3013` by the rule Section 5.3 already gives, a comparison against
them is the comparison of an absent-provenance pointer, and the e-graph
excludes them as it excludes every poison value. A pointer local without an
initializer is ordinary C and the kernel writes it everywhere; refusing it
would have frozen a hole rather than named one (revision 1.9.59, after
`int *t;` joined from two branches stopped the lowering with a message about
integers).

**Pointer and integer conversions.** A conversion from a pointer to an
integer lowers to `ptr.to_int` followed by `trunc` when the integer is
narrower than 64 bits; a conversion from an integer to a pointer lowers to
`sext` or `zext` to `bits<64>` by the integer's signedness followed by
`ptr.from_int` (Section 3.7, revision 1.9.65; both oracles agree on the
extension and the truncation, `p38.out`). `E3036`
(`not-lowered: pointer-integer-conversion`) is withdrawn and no longer
reported. A null pointer constant
converted to a pointer is not this conversion and stays admitted.

**Label addresses (revision 1.9.68).**

```text
%p = block_addr ^label : ptr<generic>
```

`block_addr` yields the address of the first instruction of block `^label`
of the current function as a `ptr<generic>` with *code* provenance: two
`block_addr` of the same block are equal, of different blocks unequal, and
a load, store or `access_in_bounds` through one is `E3013`. It is pure,
consumes no tokens, is not in the Section 8.5 allowlist, and `ptr.to_int`
of it is an ordinary integer that names no object. It exists because the
kernel writes `({ __label__ __here; __here: (unsigned long)&&__here; })`
in `local_bh_disable` and its relatives to record an instruction address
for lock and trace bookkeeping (the ninth survey put it first in 1,040
units); the value is only ever compared, stored and printed. Jumping to
one (`goto *`) remains `E3029`: the two units that write it are the
allowed residue of 1.9.67.

**Activation addresses (revision 1.9.72).**

```text
%p = return_address : ptr<generic>
%p = frame_address : ptr<generic>
```

`return_address` yields the address the current activation will return
to, with code provenance like `block_addr`: equal to itself within one
activation, comparable, storable, `ptr.to_int`-able, and `E3013` when
dereferenced. `frame_address` yields the address of the current
activation's frame record (the saved frame-pointer pair, EvoMIR Section 6)
with stack provenance of an object no load or store may touch (`E3013`).
Both are pure, consume no tokens, are not in the Section 8.5 allowlist, and
the interpreters give each activation a fresh value for each. They exist
because the kernel's `_RET_IP_` is `(unsigned long)__builtin_return_address(0)`,
first in 2,714 units once label addresses lowered, and its tracing and lock
helpers pass the value to `printk` and store it, never dereferencing it.

**Pointer and integer conversions (revision 1.9.65).**

```text
%i = ptr.to_int %p
%p = ptr.from_int.ptr<AS> %i
```

`ptr.to_int` takes a `ptr<AS>` or `fnptr<Sig>` and yields the `bits<64>`
address it denotes on the target: the object's address plus the offset, the
function's address, or the null pointer's zero. It is pure and produces no
tokens, but it **exposes** the object whose address it takes: the
interpreters record the exposure. `ptr.from_int` takes a `bits<64>` and
yields a pointer of the written type whose provenance is *exposed-address*:
a dereference through it, or an `access_in_bounds` on it, resolves at that
moment to the unique live, exposed allocation (a global, external object, or
a still-live `alloca`) whose byte range contains the address, and proceeds
with that object's provenance; when no such allocation exists the access is
`E3013`, exactly as through a dangling pointer. A zero yields the null
pointer. `ptr.from_int.fnptr<Sig>` is the function form (revision 1.9.67):
a call through it resolves at that moment to the function whose address
the integer is (every function's address counts as exposed), and to no
function otherwise (`E3016`). A C conversion between an object pointer and
a function pointer, in either direction, is `ptr.to_int` followed by
`ptr.from_int` of the target type; both oracles execute the direct form
(`.evidence/builtin-probes/20260906-function-pointer-conversion/conversion.c`,
`7`). `E3043` (`not-lowered: object-function-pointer-conversion`, the
executor's fail-closed name) is withdrawn. `ptr.from_int` from a narrower integer does not exist: the front end
extends first, and the extension follows the integer's signedness (measured,
both oracles: `(void *)(int)-1` is `0xffffffffffffffff`, `(void *)(unsigned)-1`
is `0xffffffff`; `p38.out`). A conversion to a narrower integer is
`ptr.to_int` then `trunc` (measured: `(int)p` and `(unsigned short)p` are the
low bits of the address). Neither form is in the Section 8.5 allowlist and
no rewrite family may introduce, remove or reorder one; the machine lowers
both to nothing (EvoMIR 0.12). The e-graph's memory profile, when it exists,
must treat every `ptr.to_int` as an escape of its object. Provenance through
integers is thereby PNVI-style: address arithmetic on integers is ordinary
`bits<64>` arithmetic, and only the dereference asks which object is meant.

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

**Object copy.**

```text
(%m1, %w1) = copy %m0, %w0, %dst, %src [size=N, align=A]
```

| Field | Description |
|---|---|
| `%m0`, `%w0`, `%m1`, `%w1` | As for `store`. |
| `%dst`, `%src` | `ptr<AS>` values; both complete ranges `[p, p + N)` must be valid accesses at alignment `A`. |
| `size=N` | Byte count, a compile-time constant; `N = 0` is a no-op that still validates both pointers. |
| `align=A` | Access alignment for both ranges. |

`copy` transfers the object representation of `N` bytes from `%src` to `%dst`
together with each byte's initializedness: an uninitialized source byte makes
the destination byte uninitialized and reports nothing (Section 5 already
lets a copy move uninitialized bytes; only a scalar `load` of one reports
`E3001`). Stored pointer bytes keep their provenance representation. The
two ranges must be identical or disjoint; any other overlap reports `E3027`.
Access checks are those of `store` on both ranges (`E3009`-`E3013`). `copy`
is the lowering of every C aggregate copy: assignment, copy-initialization,
argument passing and return by value of a record or union (10.1.3); no other
source form produces it, and no rewrite family may introduce, remove or
reorder one. It is never `volatile`; a volatile aggregate access is a sequence
of member accesses (10.1.3).

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

**Anonymous arguments (revision 1.9.61).** When the callee's signature is
variadic, the arguments between the last listed parameter and `%m0` are
anonymous. Each has one of the types `bits<32>`, `bits<64>`, `bits<128>`,
`f64`, `ptr<AS>` or `fnptr<Sig>`: these are the C types that survive the
default argument promotions (10.1.3), and the front end performs the
promotions, so `bits<8>` and `bits<16>` never appear here. There may be any
number of them, including none. Their types are not part of the signature and
are not checked against a declaration; they are recorded at the call and read
back by `va_arg`. A non-variadic signature admits no argument beyond its
list, as before. Anonymous arguments carry no `mem<_>`, `world`, `tuple` or
`vec` type; a C aggregate passed anonymously stays `E3030`
(`not-lowered: varargs`, message `anonymous aggregate argument`).

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


### 3.11 Inline Assembly

An `asm` instruction carries a template the assembler will read and the
operands it substitutes into it. It is **opaque**: no rule reads the
template, no analysis interprets it, and the equality database never merges
it with anything. It exists so that the kernel's assembly can pass through
the compiler without any part of the compiler pretending to understand it.

```text
(%o1, ..., %ok, %m1, %w1) = asm[ volatile] "template" %m0, %w0 outputs [...] inputs [...] clobbers [...]
(%o1, ..., %ok, %m1, %w1) = asm.goto[ volatile] "template" %m0, %w0 outputs [...] inputs [...] clobbers [...] labels [...] fallthrough ^b0(args)
```

`asm.goto` is a terminator (Section 3.9) whose successors are its labels and
its fall-through; it produces `mem` and `world` on every edge. A plain `asm`
produces its outputs, then `mem<all>` and `world`.

**Output and edge binding.** Section 9 gives the complete one-line syntax;
the two lines above are explanatory schemata, not additional accepted forms.
Each output entry retains its constraint, symbolic name (empty when absent),
type, and optional incoming value. A write-only register output has no incoming
value; a read-write register output has one of exactly its result type. Each
register output defines one fresh SSA result, in output-list order. A memory
output instead carries its place's pointer as the incoming value and its type
is that pointer's type; it defines no scalar result, even for `=Q`. It must not
cause a separate load or store. Explicit inputs retain their constraint, name
and value. Numeric template references count all outputs (including memory
outputs), followed by explicit inputs. A read-write output's incoming value
does not add another explicit input or change those numbers. A digit input
constraint names a register output of exactly the same type.

Both forms define exactly the register results followed by a fresh `mem<all>`
and a fresh `world`. Inputs, memory addresses and read-write incoming values
must be prior local definitions or block arguments: none may reference a
result of this same instruction. For `asm.goto`, results become available
only after the assembly, on **every** successor, including named labels.
An edge argument may name a result of this terminator; this is not permission
for a self-use in its input operands. Every use in a successor block must use
that block's own argument, never the predecessor's result ID directly. All
other edge operands must already be local to the predecessor, as in Section
2.4. Successor arity and types are checked by the ordinary edge rules.

The validator checks assembly inputs first, registers its fresh results,
advances the current effect state, then checks each edge. Every successor
edge passes the new memory and world exactly once, in the destination's
corresponding token positions; passing either input token, dropping a token,
duplicating it within one edge, or introducing another token is invalid.
Passing the same new token on different mutually exclusive successor edges
is valid, exactly as for `cond_br`; a dynamic execution takes only one edge.
These rules do not introduce per-edge SSA namespaces or implicit dominance.

Register results of both assembly forms start excluded under Section 8.5,
because their defining instruction is not allowlisted. This includes results
defined by the terminator, not only results in a block's instruction list.
Poison and exclusion propagation traverse every assembly successor edge;
passing a value through such an edge cannot make it an eligible scalar seed.
The producer and independent checker each enforce this existing fixed-point
rule. Neither may infer a successor choice by reading the opaque template.

Lowering evaluates output places and inputs once before the assembly.
Register output writeback is performed on every successor, using explicit
edge blocks where necessary, before entering the source label or continuing
with the next source statement. Those stores consume the assembly's new
tokens, not its input tokens. `+` preserves its incoming value on an execution
path where the template does not write it; a write-only output makes no such
promise. The compiler does not inspect assembler instructions to infer which
path writes an output. Label entries retain the source label name alongside
the target block and edge arguments, so `%l[name]` does not depend on block
numbering.

The output-availability rule is measured in
`.evidence/builtin-probes/20260905-asm-edges/outputs.c`: sealed GCC-14 and
Clang-19 agree on read-write output values 12 (fallthrough) and 7 (taken),
and on a write-only early-clobber output written before the branch, 9 on
both edges. Both result files have SHA-256
`fd77c7e8979204601b9e41908ea48ac19e0701dd562b0a98f1ad22c98535b0c2`.
The SSA encoding and canonical text below are a compiler contract choice,
not a claim about an oracle's internal representation.

**Semantics.** The results are values of their declared types about which
nothing is known: an interpreter cannot execute an `asm` and reports `E3021`
when it reaches one, so a program containing one is verified by the native
run of the EvoMIR specification's Section 9 and by nothing else. This is a
real weakening of the instrument and is stated rather than hidden.

**Constraints and modifiers.** The admitted vocabulary is the one the
configured kernel uses, measured over twenty-four preprocessed translation
units containing 24,735 `asm` statements:

| Constraint | Meaning | Measured uses |
|---|---|---|
| `r`, `=r`, `+r` | a general register, read, written, or both | 14,038 |
| `=&r`, `+&r` | written, and not sharing a register with any input (early clobber) | 7,537 |
| `rZ` | a general register, or the zero register when the operand is the constant zero; the template must spell it with a width modifier, since the zero prints as `xzr`/`wzr` only under `%x`/`%w` (measured: `orr %0, %1, %2` with `rZ` is an assembler error under GCC-14 and `orr %0, %1, %x2` is accepted by both) | 3,301 |
| `Q`, `=Q`, `+Q`, `Qo` | a memory operand whose address is a register with no offset | 8,113 |
| a digit `0` to `9` | this operand occupies the same location as the output that digit names (a tied operand) | 3,528 |
| `I`, `J` | an immediate the add or subtract forms accept, and its negation | 876 |
| `Ir` (source input) | either an `I` immediate or a general register; lowering selects the legal `r` alternative | 2,726 first-error units in survey `20260907T043340Z-13363` |
| `K`, `L` | an immediate the 32-bit and 64-bit logical forms accept | 1,104 |
| `i`, `n` | an integer constant | 113 |
| `p` | an address | 48 |

For source input `Ir`, C-HIR preserves the written constraint and lowering
evaluates the operand exactly once, then records the selected `r` constraint
in the EvoIR `asm` or `asm.goto`. This applies to constants and dynamic values,
with the same operand types and template modifiers as `r`; it does not admit
`Ir` as an output constraint or as an unresolved EvoIR constraint. No new
backend constraint is required. This choice is authorized on 2026-09-07 and
measured with both GCC-14 and Clang-19 in
`.evidence/builtin-probes/20260907-inline-residue/ir-constraint.c` and `oracles.out`.

A constraint outside this table, or a constraint string combining letters
this table does not combine, is `E3021` with the constraint named. Clobbers:
`memory` (implied by the effects below and otherwise inert), `cc` (the flags),
and a named physical register, which the allocator must treat as written.
Template operand references: `%0` to `%9`, `%[name]`, a width modifier
`%w0`/`%x0` before the number or name, `%l[label]` in an `asm.goto`, and
`%%` for a literal percent; any other reference is `E3021`. Every rule in
this paragraph is measured in `p33-asm-constraints.out`, where both oracles
compile the whole vocabulary and print the same values.

**Effects.** An `asm` reads and writes `mem<all>` and `world`, may trap, and
may not be reordered against any other effectful instruction, duplicated,
or removed, whether or not it is `volatile`; `volatile` is preserved for the
assembler and changes nothing in EvoIR. Section 8.5's allowlist already makes
it ineligible as a region root or witness, so no rewrite can reach it.
Defined at revision 1.9.58.
### 3.12 Variadic Argument Access

```text
(%m1, %w1) = va_start %ap, %m0, %w0
(%v, %m1, %w1) = va_arg.Type %ap, %m0, %w0
(%m1, %w1) = va_end %ap, %m0, %w0
```

| Field | Description |
|---|---|
| `%ap` | A `ptr<AS>` to a **list object**: 32 bytes at alignment 8, the lowering of one `__builtin_va_list` (10.1.5). |
| `Type` | For `va_arg`: one of `bits<32>`, `bits<64>`, `bits<128>`, `f64`, `ptr<AS>`, `fnptr<Sig>`, the promoted types of Section 3.10. |

**Semantics.** A variadic function receives, besides its listed parameters,
the sequence of anonymous arguments its caller passed. `va_start` makes the
list object at `%ap` denote that sequence positioned at its first element;
it is valid only inside a function whose own signature is variadic (module
validation, `E3037`). `va_arg` yields the element at the list's position
and advances the position by one. The yielded value has the argument's
type: a `va_arg.Type` whose `Type` differs from the type the caller passed
in that position, or that runs past the last anonymous argument, is
**undefined**; both interpreters report `E3038` (`va-arg-mismatch`) and
stop, and the machine returns whatever the calling convention's procedure
finds (EvoMIR Section 6). `va_end` ends the list; after it, and before a
`va_start`, the list object holds no list, and a `va_arg` on it is the same
undefined behaviour.

**The list object's bytes.** Between `va_start` and `va_end` the object
representation of the list is unspecified. A `copy` (Section 3.8) of the
whole 32 bytes produces a second list denoting the same sequence at the same
position, which is how `__builtin_va_copy` lowers (10.1.3); any narrower
`load`, `store` or partial `copy` of those bytes is undefined, and the
interpreters may keep their own representation inside them. The kernel
cohort passes `va_list` objects to callees by value: 10.1.3 already sends a
record parameter as the address of a caller-made copy, and a list is a
record for that rule (`__builtin_va_list` is a 32-byte structure on this
target, evidence `p31-residue.c`), so the callee's `%ap` is that address.

**Effects and rewriting.** All three read and write `{All}` (Section 4.2):
the list object is memory, and the anonymous arguments are state no other
instruction can name. None is allowlisted (8.5), no rewrite family may
introduce, remove or reorder one, and the effect edges keep them in program
order, exactly as for `copy`.

**Measured convention (AArch64 Linux, general registers only).** The
sealed oracles agree on every row (`.evidence/builtin-probes/20260902-2c23684c/p34-varargs.c`,
`p34-callsite.c`, outputs `p34-p35.out` for GCC-14 and `p34-p35-clang.out`
for Clang-19): a call site cannot tell a variadic prototype from a fixed one
(`call_v` and `call_f` are instruction-for-instruction identical under both
compilers); an anonymous `char` is promoted by the caller and occupies one
8-byte slot; after `va_start` with `k` named integer parameters the object
holds `gr_offs = -8 * (8 - k)` and `vr_offs = 0`; the hand-written
Appendix B procedure in `p34-varargs.c` agrees with each compiler's own
`va_arg` on all fourteen items including a 16-byte-aligned `__int128`, a
16-byte record in the save area, a 24-byte record by reference, and the
transition to stack arguments. The procedure itself is EvoMIR's to state
(EvoMIR Section 6); EvoIR only promises the sequence.

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
| `uninitialized.bits<N>`, `uninitialized.ptr<AS>`, `uninitialized.fnptr<Sig>` | `{}` | `{}` | false | false | false | false | false |
| `add`, `sub`, `mul` | `{}` | `{}` | false | false | false | false | false |
| `sdiv`, `udiv`, `srem`, `urem` | `{}` | `{}` | false | false | false | true | false |
| integer UB predicates | `{}` | `{}` | false | false | false | false | false |
| `ub.assume.<reason>` | `{}` | `{}` | false | false | false | true | false |
| `and`, `or`, `xor` | `{}` | `{}` | false | false | false | false | false |
| `ctlz`, `cttz`, `ctpop`, `bswap` | `{}` | `{}` | false | false | false | false | false |
| `shl`, `lshr`, `ashr` | `{}` | `{}` | false | false | false | false | false |
| `icmp.*` | `{}` | `{}` | false | false | false | false | false |
| `sext`, `zext`, `trunc` | `{}` | `{}` | false | false | false | false | false |
| `ptr.null`, `global_addr`, `func_addr` | `{}` | `{}` | false | false | false | false | false |
| `ptr.offset.*.*` | `{}` | `{}` | false | false | false | true | false |
| `ptr.to_int`, `ptr.from_int.*` | `{}` | `{}` | false | false | false | false | false |
| `block_addr` | `{}` | `{}` | false | false | false | false | false |
| `return_address`, `frame_address` | `{}` | `{}` | false | false | false | false | false |
| `asm`, `asm.goto` | `{all}` | `{all}` | true | false | true | true | false |
| `va_start`, `va_arg.*`, `va_end` | `{All}` | `{All}` | false | false | false | true | false |
| `alloca`, `lifetime.end` | `{All}` | `{All}` | false | false | false | true | false |
| `pointer_valid`, `access_in_bounds` | `{All}` | `{}` | false | false | false | true | false |
| `load` (normal) | `{All}` | `{}` | false | false | false | true | false |
| `store` (normal) | `{}` | `{All}` | false | false | false | true | false |
| `copy` | `{All}` | `{All}` | false | false | false | true | false |
| `load volatile` | `{All}` | `{}` | true | false | false | true | false |
| `store volatile` | `{}` | `{All}` | true | false | false | true | false |
| `call` | depends on callee | depends on callee | depends | depends | depends | depends | depends |
| `trap.<reason>` | `{}` | `{}` | false | false | false | true | false |

**Trap terminators.** `trap.<reason>` is the zero-operand terminator family.
It takes no operands, produces no results, has no successors, and reaching it
reports its reason's diagnostic and ends execution. Two reasons exist:

| Reason | Diagnostic | Emitted for |
|---|---|---|
| `scalar_function_fell_off_end` | `E3007` | **withdrawn at 1.9.71**: the reachable end of a non-void function now returns poison (10.1.3); the reason stays in the grammar so old modules parse |
| `unreachable_reached` | `E3024` | `__builtin_unreachable()` (10.1.17) |

A trap terminator is a report, never a licence. No analysis may treat a block
that ends in one as dead, and no rewrite may use its presence to conclude that
a predecessor edge is not taken, delete a computation, or strengthen a fact
elsewhere; the only optimistic transition this profile permits is one a proof
certificate justifies. Its `may_trap` effect keeps it from being reordered or
removed, exactly as for the other reason.

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
Every `uninitialized` form and every value transitively depending on poison
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

0. Every external function (2.1) has the **top** summary: `reads={all},
   writes={all}` and every boolean `true`. Nothing this module holds can
   bound what another unit's code does, so no smaller summary is sound; a
   later revision may let a declared attribute (`const`, `pure`) lower it,
   with a measurement.
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
String-literal objects and `const`-qualified file-scope compound literals are
read-only; an attempted store reports `E3014`.

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
for a definition without `static`.

**Function linkage (revision 1.9.70).** A function carries the same
distinction, written after the calling convention:

```text
func @name(...) -> (...) cc = target_default visibility=internal
```

`visibility=internal` is written for a function whose C definition has
storage class `static` (including `static inline`); the field is omitted
for every other function, and its absence means external, so every module
text written before this revision is unchanged. The machine emits no
global symbol for an internal function or an internal global, and it omits
an internal function that nothing in the module references by `call`,
`func_addr` or a relocation, repeating until nothing more can be omitted,
exactly as the reference compilers drop an unused `static inline` (EvoMIR
0.16). The kernel measurement found the need: every kernel unit defines
hundreds of `static inline` helpers from shared headers, and emitting them
with external linkage made any two EvoCC objects collide at link time
(`multiple definition`), which capped the mixed link at 14 units; unused
stubs also call functions the configuration does not build, so an
unreferenced internal function must not reach the assembler. A `function_static` symbol must equal
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
| `E3027` | `copy` ranges overlap without being identical. |
| `E3029` | A label address is used, or a `goto *`, is lowered. |
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
| `E3007` | `scalar-function-fell-off-end` | runtime — **withdrawn at 1.9.71** (poison instead) |
| `E3008` | `interpreter-fuel-exhausted` | host resource |
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
| `E2002` | `incompatible-redeclaration` | front end |
| `E2903` | `malformed-attribute` | front end |
| `E2904` | `not-covered: variably-modified-typeof` | front end |
| `E2905` | `statement-expression-outside-function` | front end |
| `E2906` | `designator-outside-object` | front end |
| `E2907` | `generic-selection-duplicate-association` | front end |
| `E2908` | `generic-selection-no-match` | front end |
| `E2909` | `builtin-arity` | front end |
| `E2910` | `builtin-not-called` | front end |
| `E2911` | `static-assertion-failed` | front end |
| `E2912` | `static-assertion-not-constant` | front end |
| `E2913` | `builtin-operand-not-convertible` | front end |
| `E2914` | `malformed-asm` | front end |
| `E2915` | `auto-type-misuse` | front end |
| `E2916` | `case-range-invalid` | front end |
| `E2917` | `designator-index-not-constant` | front end |
| `E2918` | `jump-into-statement-expression` | front end |
| `E2919` | `flexible-array-initializer-not-static` | front end |
| `E2920` | `undeclared-label` | front end |
| `E2922` | `volatile-aggregate-copy-unsupported` | front end |
| `E2923` | `invalid-array-bound` | front end |
| `E2924` | `attribute-on-non-null-statement` | front end |
| `E2925` | `builtin-mode-out-of-range` | front end |
| `E2926` | `va-start-outside-variadic-function` | front end |
| `E2927` | `builtin-library-declaration-incompatible` | front end |
| `E2928` | `address-of-register-object` | front end |
| `E2929` | `unpromoted-va-arg-type` | front end |
| `E2930` | `use-with-incomplete-enumeration` | front end |
| `E2931` | `unprototyped-call-mismatch` | front end |
| `E2932` | `always-inline-cycle` | front end |
| `E0102` | `unmatched-conditional` | preprocessor |
| `E0104` | `error-directive` | preprocessor |
| `E0105` | `include-not-found` | preprocessor |
| `E0106` | `feature-query-malformed` | preprocessor |
| `E0107` | `pragma-pack-unsupported` | preprocessor |
| `E0108` | `poisoned-identifier-used` | preprocessor |
| `E0109` | `counter-overflow` | preprocessor |
| `E0110` | `invalid-token-paste` | preprocessor |
| `E0111` | `conditional-operand-not-integer` | preprocessor |
| `E0112` | `stray-token` | front end |
| `E3018` | `invalid-lifetime-end` | runtime |
| `E3019` | `function-pointer-target-signature-mismatch` | runtime |
| `E3020` | `not-lowered: checked-arithmetic` | lowering — **withdrawn at 1.9.66** |
| `E3021` | `not-lowered: asm` | lowering |
| `E3022` | `bit-builtin-zero-operand` | runtime |
| `E3023` | `not-lowered: bit-builtin` | lowering — **withdrawn at 1.9.66** |
| `E3024` | `unreachable-reached` | runtime |
| `E3025` | `not-lowered: return-address` | lowering (levels above zero only since 1.9.72) |
| `E3026` | `not-lowered: static-relocation` | lowering |
| `E3027` | `overlapping-copy` | runtime |
| `E3030` | `not-lowered: varargs` | lowering |
| `E3031` | `not-lowered: frame-address` | lowering (levels above zero only since 1.9.72) |
| `E3032` | `not-lowered: pointer-difference` | lowering — **withdrawn at 1.9.67** |
| `E3033` | `not-executed: wide-integer` | lowering (division and remainder only, revision 1.9.61) |
| `E3034` | `not-lowered: function-pointer-arithmetic` | lowering |
| `E3035` | `not-lowered: global-register-variable` | lowering — **withdrawn at 1.9.67** (implicit `asm`) |
| `E3042` | `not-lowered: checked-arithmetic-width` | lowering — executor-allocated, **withdrawn at 1.9.67** |
| `E3043` | `not-lowered: object-function-pointer-conversion` | lowering — executor-allocated, **withdrawn at 1.9.67** |
| `E3036` | `not-lowered: pointer-integer-conversion` | lowering — **withdrawn at 1.9.65**, never reported |
| `E3037` | `va-start-outside-variadic-signature` | validation |
| `E3038` | `va-arg-mismatch` | runtime |
| `E3039` | `external-call-not-executable` | runtime |
| `E3040` | `external-object-not-executable` | runtime |
| `E3041` | `symbol-declared-and-defined` | module validation |
| `E3029` | `not-lowered: computed-goto` | lowering (`goto *` only since 1.9.68) |
| `E3045` | `not-lowered: ordered-pointer-comparison` | lowering — executor-allocated, **withdrawn at 1.9.68** |

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

**Cost is a policy.** The cost function above is the default policy of this
profile, not a property the checker verifies: a certificate proves that every
replacement is equivalent, never that it was the cheapest, and Section 8.7
already lets a producer decline any eligible region. A producer may therefore
extract under any other cost function provided it is deterministic for a
given module, budget and policy, applies the same strict-improvement
rule relative to its own costs, and is identified in the producer's report
by a name together with a complete description from which the function can
be rebuilt (for a weight vector, every weight); the name alone is a label
(revision 1.9.56, after a searched weighting shared one name across
vectors). The certificate carries none of this and its format does not
change. Two policies that differ
only in cost produce certificates the checker accepts or refuses on identical
grounds. The kernel cohort is an acceptance corpus and never enters the
fitness of any policy (the contract's red lines). Defined at revision 1.9.50
to open the measurement of learned cost functions and schedules.

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
The closure additionally contains every allowlisted instruction of the same
block, earlier than the root, that is a *structural duplicate* of an
instruction already in the closure: the same operation and attributes, with
operands that are pairwise the same values; duplicates of duplicates join in
turn, to a fixed point, and each joins with its own operand cone by the rule
above. This is what lets a region record, through congruence alone, that its
root is an earlier instruction's twin and extract the earlier value (admitted
at revision 1.9.44, after 16 of 16 duplicates in 256 random programs had twins
outside their closures).

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
icmp.eq, icmp.ne, icmp.slt, icmp.sle, icmp.ult, icmp.ule,
shl, lshr, ashr, zext, sext, trunc
```

The shifts and width changes were admitted at revision 1.9.44: none of them
may trap (Appendix A), the shift's definedness guard is a separate predicate
that stays excluded, and a shift's amount operand is a witness or seed of its
own sort. Everything else is ineligible, including division and remainder,
every UB predicate, `ub.assume`, all pointer and
memory forms, `call`, and all terminators. An instruction absent from this list
is ineligible even if some future analysis would prove it total: the list is the
authority, not the reasoning behind it.

Every root and witness result in this profile has type `bits<N>` with
`1 <= N <= 64`. Comparisons consume one common `bits<N>` sort and produce
`bits<1>`; a shift consumes its amount in any `bits<M>`; a width change
consumes one sort and produces another, both within the bound. Wider scalar equality is a later registry revision; silently
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
or carries any non-pure effect. An instruction is absent from the allowlist when
its operator is not listed or when its result sort lies outside this
profile's bound, so an `add.bits<100>` is absent and its result starts
excluded (ruled at revision 1.9.46; the other reading would let the closure
follow that instruction as a witness, contradicting the bound). Exclusion propagates from every operand to its
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
but no instruction in that closure defines. In this profile every seed has origin
`input`: a value defined earlier in the same block by an instruction the
closure does not follow is, by the exclusion rule of 8.5, excluded, and its
region ineligible (measured at revision 1.9.46 over 577,015 regions, none
with a `definition` seed). The `definition` origin stays in the manifest
format for a later profile and is recomputed by the checker like any field.

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

**Facts of seeds.** A seed of origin `input` carries a fact computed once per
function, before regions are formed, by a forward fixed point over the
function in Section 8.6 order: the fact of a block argument is the *join* of
the facts of the values passed to it on every edge entering the block, the
join keeping only the bits every edge knows with the same value (the class
merge of the paragraph above combines knowledge and is the other operation;
revision 1.9.47 named them apart after the two had been called by one word), a
passed value's fact being the transfer of its defining instruction over the
facts of its operands (an unlisted instruction's result, a pointer, and any
excluded value being top); every edge counts, none is pruned by a fact, and
a block argument not yet visited contributes the join's identity. Iteration
stops when no fact changes; the lattice is finite and every step only loses
knowledge, so it terminates, and the order of visits does not change the
result. Entry block arguments are top. A premise may name a seed's class;
the checker recomputes the seed's fact by the same fixed point over
`before.evo`. Defined at revision 1.9.46; before it, seeds were top and no
fact crossed a block edge.

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
| `icmp.*` | decided by intervals, as the paragraph below states; top otherwise |
| `shl` | when the amount is fully known and below `N`: bit `i` is known zero for `i < k` and equals input bit `i - k` above; top otherwise |
| `lshr` | when the amount is fully known and below `N`: bit `i` equals input bit `i + k` for `i + k < N` and is known zero above; top otherwise |
| `ashr` | when the amount is fully known and below `N`: bit `i` equals input bit `i + k` for `i + k < N` and equals the input's sign bit above, known only when that bit is; top otherwise |
| `zext` | the low `W` bits equal the input's bits; the upper bits are known zero |
| `sext` | the low `W` bits equal the input's bits; the upper bits equal the input's sign bit, known only when that bit is |
| `trunc` | bit `i` equals input bit `i` for `i < N` |

**The `icmp` procedure.** Let `lo(a)` be the fact of `a` with every unknown bit
zero and `hi(a)` with every unknown bit one, read as unsigned `N`-bit values.
`eq` is known one when both operands are fully known and equal, known zero
when some bit is known in both with different values, top otherwise; `ne` is
its negation. `ult` is known one when `hi(a) < lo(b)`, known zero when
`lo(a) >= hi(b)`, top otherwise; `ule` the same with `<=` and `>`. `slt` and
`sle` are decided by the same interval rule with both operands read as
two's-complement `N`-bit values when the sign bit of each operand is known,
and are top when either sign bit is unknown. No other case is decided; a
procedure that decided more would be a stronger analysis than this table
states. Defined at revision 1.9.40.

The shift and width rows were added at revision 1.9.43; a shift amount that is not fully known, or is known and not below the width, transfers to top, because the shift's own definedness guard (Section 4) is what excludes the out-of-range case and this analysis states nothing about guarded operations. The transfers are deliberately weak. Each is a statement a checker can
recompute from the region's nodes with no search; a stronger analysis that
needed one would move the judgment into shared code, which Section 11.6
reserves for cases that fail safe.

**Use.** A rewrite may name a fact as a premise. A constant fold (`all_const`) reads
its constants from the nodes and carries no premise; a known-bits identity such as `and(x, m) -> x` when
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

**Runs.** A run is a sequence of at most `max_transactions = 8` transactions
over one module, each transaction taking the previous one's output as its
input. The driver stops the run when a transaction admits no region or at
the ceiling. Each transaction's certificate stands alone and is verified
alone; the run is verified by digest chaining, the `output` digest of one
certificate being the `input` digest of the next, and a chain that breaks
anywhere fails the run. A run exists because 11.6 lets a left-hand
`const_like` match only a constant present in the before witness closure, so
a constant one transaction derives becomes matchable only in the next;
measured at revision 1.9.38, 20 of 21,000 random programs admitted a region
on a second transaction (0.095 percent), one region each. The count of
transactions a run took, and the residual regions after its last, are
telemetry the campaign reports.

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

There are no comments. A `;` outside a quoted assembly string is rejected by
Section 9.5; inside one it is assembler data and is retained. This format is machine interchange rather than authored source;
explanatory text belongs beside a fixture, not inside a file whose bytes are
digested.

### 9.3 Grammar

```text
module          := ε | segment (newline segment)*
                   -- at most one of each, in this order:
                   -- global_seq, extern_seq, declare_seq, function_seq, module_asm_seq
global_seq      := global+
extern_seq      := extern_object+
extern_object   := "extern " symbol " size=" (integer | "unknown")
                   " align=" integer newline
declare_seq     := declare+
declare         := "declare " symbol "(" sig_param_list [", ..."] ")"
                   " -> (" type_list ")" " cc = " ident newline
function_seq    := function (newline function)*
module_asm_seq  := module_asm+
module_asm      := "asm " quoted_string newline

global          := "global " symbol " origin=" integer
                   " kind=" global_kind " visibility=" visibility
                   " size=" integer " align=" integer
                   " bytes=" hex_byte+ " initialized=" bit+
                   [" relocs=[" reloc (", " reloc)* "]"] newline
reloc           := "(" integer ", " reloc_target ", " integer ")"
reloc_target    := symbol | symbol block_label
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
signature       := "(" sig_param_list [", ..."] ")->(" type_list ")"
sig_param_list  := ε | sig_param ("," " " sig_param)*
sig_param       := type [" byval(" integer ", " integer ")" | " sret(" integer ", " integer ")"]
address_space   := "generic" | "global" | "stack" | "threadlocal"

function        := "func " symbol "(" param_list [", ..."] ")"
                   " -> (" type_list ")"
                   " cc = " ident [" visibility=internal"] newline
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
param           := param_name ": " type [" byval(" integer ", " integer ")" | " sret(" integer ", " integer ")"]
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
  | value_name " = " bit_op "." type " " value_name
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
  | "(" value_name ", " value_name ") = copy "
        value_name ", " value_name ", " value_name ", " value_name
        " [size=" integer ", align=" integer "]"
  | value_name " = ptr.offset." direction "." index_signedness " "
  | value_name " = ptr.to_int " value_name
  | value_name " = block_addr " block_label " : " type
  | value_name " = return_address : " type
  | value_name " = frame_address : " type
  | value_name " = ptr.from_int." type " " value_name
        value_name ", " value_name " [scale=" integer "]"
  | "(" result_list ") = call " callee "(" arg_list ")"
  | "(" result_list ") = va_start " value_name ", " value_name ", " value_name
  | "(" result_list ") = va_arg." type " " value_name ", " value_name ", " value_name
  | "(" result_list ") = va_end " value_name ", " value_name ", " value_name
  | "(" result_list ") = asm" volatile_marker " " asm_payload

term_body :=
    "br " block_label "(" arg_list ")"
  | "cond_br " value_name ", " block_label "(" arg_list ")"
        ", " block_label "(" arg_list ")"
  | "ret" return_operands
  | "trap." trap_reason
  | "(" result_list ") = asm.goto" volatile_marker " " asm_payload
        " labels [" asm_label_list "] fallthrough " block_label "(" arg_list ")"

return_operands  := ε | " " value_name ("," " " value_name)*

arith_op         := "add" | "sub" | "mul" | "sdiv" | "udiv" | "srem" | "urem"
                  | "shl" | "ashr" | "lshr" | "and" | "or" | "xor"
cmp_op           := "eq" | "ne" | "slt" | "sle" | "ult" | "ule"
cast_op          := "sext" | "zext" | "trunc"
bit_op           := "ctlz" | "cttz" | "ctpop" | "bswap"
binary_predicate := "signed_add_defined" | "signed_sub_defined"
                  | "signed_mul_defined" | "signed_div_rem_no_overflow"
shift_predicate  := "shift_count_defined" | "signed_shl_defined"
ub_reason        := "division_by_zero" | "remainder_by_zero" | "signed_overflow"
                  | "invalid_shift_count" | "invalid_signed_left_shift"
                  | "bit_builtin_zero_operand"
trap_reason      := "scalar_function_fell_off_end" | "unreachable_reached"
volatile_marker  := ε | " volatile"
direction        := "add" | "sub"
index_signedness := "signed" | "unsigned"
result_list      := value_name ("," " " value_name)*
arg_list         := ε | value_name ("," " " value_name)*
callee           := symbol | value_name

asm_payload      := quoted_string " " value_name ", " value_name
                    " outputs [" asm_output_list "] inputs [" asm_input_list
                    "] clobbers [" quoted_list "]"
asm_output_list  := ε | asm_output (", " asm_output)*
asm_output       := "(" quoted_string ", " quoted_string ", " type ", " asm_incoming ")"
asm_incoming     := "-" | value_name
asm_input_list   := ε | asm_input (", " asm_input)*
asm_input        := "(" quoted_string ", " quoted_string ", " value_name ")"
asm_label_list   := ε | asm_label (", " asm_label)*
asm_label        := "(" quoted_string ", " block_label "(" arg_list "))"
quoted_list      := ε | quoted_string (", " quoted_string)*
quoted_string    := quote_byte quoted_byte* quote_byte
quoted_byte      := quoted_ascii | backslash_byte quote_byte
                  | backslash_byte backslash_byte | backslash_byte "x" hex_byte
quote_byte       := the single byte 0x22
backslash_byte   := the single byte 0x5c
quoted_ascii     := a byte in 0x20..0x21, 0x23..0x5b, or 0x5d..0x7e
```

In an assembly payload the first two values are the input `mem<all>` and
`world`. An output tuple is `(constraint, name, type, incoming)` and an input
tuple is `(constraint, name, value)`. Empty lists are spelled `[]` and an
absent symbolic name is `""`. `-` is allowed only for write-only register
outputs. Result IDs and types are bound by Section 3.11; memory outputs are
not additional scalar results. Lists retain source order, including clobbers
and named labels; no sorting or deduplication changes assembler data.

Quoted strings encode the UTF-8 bytes of their decoded assembler text.
Bytes 0x20 through 0x7e are literal except quote and backslash, which use
`\"` and `\\`; every other byte uses exactly `\xhh` with lowercase hex.
Thus a template newline is `\x0a`, not an actual line break or `\n` escape.
Hex-escaping a printable byte, alternate escapes, unterminated strings,
raw non-ASCII bytes and a decoded byte sequence that is not UTF-8 are
noncanonical and rejected. Whitespace, semicolons, brackets and percent
signs inside strings are data, not grammar delimiters. Escaping changes no
template, constraint, name or clobber byte after decoding.

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
<> = ptr.to_int <>
<> = block_addr <> : <>
<> = return_address : <>
<> = frame_address : <>
<> = ptr.from_int.<> <>
(<>) = call @<>(<>)
(<>) = call <>(<>)
(<>) = va_start <>, <>, <>
(<>) = va_arg.<> <>, <>, <>
(<>) = va_end <>, <>, <>
(<>) = asm<> <> <>, <> outputs [<>] inputs [<>] clobbers [<>]
(<>) = asm.goto<> <> <>, <> outputs [<>] inputs [<>] clobbers [<>] labels [<>] fallthrough <>
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
block so the gap is declared rather than discovered. Revision 1.9.60 adds
the two assembly shapes below; they remain pending until their implementation.

```text
(<>) = asm<> <> <>, <> outputs [<>] inputs [<>] clobbers [<>]
(<>) = asm.goto<> <> <>, <> outputs [<>] inputs [<>] clobbers [<>] labels [<>] fallthrough <>
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

**Alignment is an object property, not a type (revision 1.9.62).** A
declared alignment (`aligned(N)`, `_Alignas`) never enters the type of an
object and therefore never enters the manifest's or any redeclaration's type
identity: `extern long a[1]; long a[1] __attribute__((aligned(16)));` is one
object of type `long[1]`, compatible with `long[1]`, whose alignment is the
maximum over every declaration of it in the unit, in either order (measured,
both oracles: 16, and 32 when only the `extern` declaration carries
`aligned(32)`; `.evidence/builtin-probes/20260902-2c23684c/p36-aa-tail.c`,
`p36.out`). A manifest type comparison that reads alignment is a defect.

**External symbols (revision 1.9.64).** A unit uses symbols it does not
define: every kernel unit calls `printk` and reads `jiffies`. A function
that the unit declares and calls or takes the address of, but does not
define, lowers to one `declare` (Section 2.1, Section 9) carrying its
complete lowered signature, with record parameters and results in the
pointer form of 10.1.3 and the `...` marker of 1.1 when its prototype ends
in `...`; a function declared without a prototype (`int f();`) and called
takes the promoted types of its first call, and a later call with other
promoted types is `E2931`. An object declared `extern` and used (loaded,
stored or address-taken) but not defined lowers to one `extern` line with
its type's size, `unknown` for an incomplete type such as `extern int a[];`,
and its alignment (the maximum over declarations, 1.9.62). A declared but
unused symbol produces nothing. The two interpreters execute a call to an
external function only for the library set of 10.1.14 (`memset`, `memcpy`,
`memmove`, `memcmp`, `strcmp`, `strncmp`, `strlen`), which they implement
on the byte-and-initializedness model (`memset` initializes the bytes it
writes, `memcpy` and `memmove` transfer initializedness exactly as `copy`,
the readers require initialized bytes up to the last one they examine and
report `E3001` otherwise); every other external call is `E3039`
(`external-call-not-executable`) when reached. `global_addr` of an external
object is a distinct, stable provenance that may be compared and passed;
a `load` or `store` through it is `E3040` (`external-object-not-executable`)
when reached. Neither code is a front-end refusal: the unit lowers, the
e-graph treats the call by its top summary (Section 4), and the machine lane
calls or addresses the symbol and lets the linker resolve it (EvoMIR
Section 8). Measured under both oracles: a call of `memset` after
`__builtin_memset` with no declaration in scope is still an implicit
declaration (`p37-use.c`), so the synthesized prototype below is the only
source of the `declare` for a builtin.

**Library builtins introduce no identifier (revision 1.9.63).** A use of
`__builtin_memset` and the other library builtins of 10.1.14 lowers to a
call of the library function's symbol with the prototype 10.1.14 gives it,
but declares nothing in the source: `static int memset = 3;` after
`__builtin_memset(p, 0, 1)` is accepted, and `memset(p, 0, 1)` after it is an
implicit declaration, refused by both oracles
(`.evidence/builtin-probes/20260902-2c23684c/p37-builtin-scope.c`,
`p37-use.c`, `p37.out`). The prototype the front end synthesizes therefore
lives in the builtin namespace, keyed by the library function's name, once
per unit, visible in no scope, and never in a function's scope: the manifest
carries it as a unit-level function symbol, exactly as it carries a function
the unit declares at file scope and calls without defining. It is compared
against a visible declaration of the same library function only through
`E2927`, and a source object or function named like it is a different
symbol.

**Declarations with an incomplete enumeration return type (revision
1.9.62).** `enum e f(unsigned int);` with `e` not yet complete is an
accepted declaration (both oracles). A function that is only declared and
never defined, called or address-taken produces nothing in EvoIR, because
EvoIR carries definitions and the calls that name them, not declarations
(Section 3.10, revision 0.7). A call or `func_addr` whose function type
still names an incomplete enumeration at that point is `E2002`'s neighbour
`E2930` (`use-with-incomplete-enumeration`) at the use; both oracles
reject it (`p36-use-never.c`, GCC "invalid use of undefined type",
Clang "calling with incomplete return type"). An enumeration completed
before the use has its completed type there, as usual.

**Arithmetic on `void *`.** `void *` participates in pointer arithmetic with
scale 1 (`sizeof(void)` is 1, 10.1.19), in every form: `p + n`, `p += n`,
`p++`, `--p`, subtraction of two `void *`. Measured, both oracles: `p += 3`
advances three bytes. Measured defect at revision 1.9.27: the binary form was
accepted and the update forms refused — one rule on two paths again.

**128-bit assignment.** `__int128` and `unsigned __int128` objects are
assigned, loaded and stored exactly as any integer object; 10.1.19's closure
of `__int128` value semantics applies to the checked-arithmetic builtins, not
to plain assignment. Measured defect at revision 1.9.27: `__uint128_t tmp;
tmp = *p;` was refused as not a modifiable lvalue while `__uint128_t tmp =
*p;` and 64-bit assignment were accepted — the assignment target check
carried a width list the initializer path did not.

**Character pointers of differing signedness.** A pointer to `char`, `signed
char` or `unsigned char` (with or without qualifiers on the pointee) converts
implicitly to a pointer to any other of the three in initialization,
assignment, argument passing and return. Both oracles accept every such
conversion and diagnose it only as a warning under `-Wall` (`pointer-sign`);
the kernel's `struct qstr` initializer `.name = value` converts `char *` to
`const unsigned char *`. This profile accepts it without a diagnostic. The
three pointee types have identical size, alignment and representation, so no
access changes meaning. Measured defect at revision 1.9.28: six lanes stopped
on `E2202` for exactly that initializer.

**Pointer difference.** `a - b` on two pointers to `T` has type `long`
(`ptrdiff_t`); on two `void *` the scale is 1 (measured: `10`, type `long`,
both oracles). Lowering (revision 1.9.67): `ptr.to_int` of each operand,
`sub.bits<64>`, then `sdiv.bits<64>` by `sizeof(T)` (1 for `void`), which
is exact whenever the program is defined. C's same-object obligation is
not a value the EvoIR interpreters can check after `ptr.to_int`, so it is
carried by the C-HIR interpreter alone, which executes the same-object
form and reports `E3013` otherwise; the machine computes the subtraction.
`E3032` (`not-lowered: pointer-difference`) is withdrawn. The cohort's
eight units are zstd.

**128-bit execution (revision 1.9.61).** `__int128` and `unsigned __int128`
values are `bits<128>` scalars, which Section 1.1 has always admitted;
what was missing was execution. A `bits<128>` value lowers, is interpreted
and is optimized exactly as any other `bits<N>`: `load`/`store` at 16 bytes
and alignment 16, `copy` of records holding one, `zext`/`sext` from and
`trunc` to a narrower width, `add`, `sub`, `mul`, `and`, `or`, `xor`,
`shl`, `lshr`, `ashr`, every `icmp`, the UB predicates, block arguments,
parameters, results and anonymous arguments. Both interpreters extend their
scalar value model to 128 bits for these forms. The kernel cohort's uses
are exactly a 64-by-64-bit product read as two halves (`math64.h`,
`lib/math/div64.c`), the `cmpxchg128` family (a union of one `u128` and two
`u64` halves crossing `asm` operands and a return), and `GENMASK_U128`
constants: `zext`, `mul`, `lshr`, `trunc`, `load`, `store`, `copy` and the
call boundary. `sdiv`, `udiv`, `srem` and `urem` on `bits<128>` stay
`E3033` (`not-executed: wide-integer`) until a unit needs one; the cohort's
`mul_u64_u64_div_u64` divides the halves by hand. Reference values, both
oracles (`p35-run.c` in `.evidence/builtin-probes/20260902-2c23684c/`):
`((u128)0xffffffffffffffff * 0xfffffffffffffffe) >> 64` is
`0xfffffffffffffffd`; `((u128)0x123456789abcdef0 * 0xfedcba9876543210) >> 37`
is `0x56bbeba1111b6c47`; `(1 << 100)` has high half `0x1000000000`;
`(((u128)1 << 64) | ~0ul) + 1` is `2:0`; `(1 << 64) < (~(u128)0 >> 64)` is 0
and `1 < (1 << 64)` is 1. Earlier reference, still valid:
`((__uint128_t)1 << 100) + 7` shifted right by 64 is `68719476736`, and
`(u * 3) >> 100` is `3`. The register-pair convention for passing and
returning one is EvoMIR's (EvoMIR Section 6, measured in `p35-int128.c`).

**What may root a place.** Any expression of pointer type may be the root of a
place through `*`, `->` or `[]`: a variable, a loaded pointer, an address-of,
a call result, a conditional expression, a statement expression, a cast, a
comma expression. The rule has one implementation; there is no list of
admitted root forms, because every such list so far has been incomplete.
Measured defects: places rooted at a loaded pointer (revision 1.9.x, window
G), at an address-of (window M), and — at revision 1.9.26 — at a call result
(`*get(p) = (lock_t){ .raw = 0 }`, ten units, the kernel's spinlock
initialisation) and at a conditional (`(c ? p : (void *)0)->v`, the kernel's
`to_acpi_data_node`). Both oracles accept every form (evidence
`.evidence/builtin-probes/20260901-2c23684c/strlen.out`, `call_root`,
`cond_root`).

**Aggregate copies.** Assignment of a record or union, copy-initialization of
an automatic object from another object or from a call result, passing one by
value and returning one by value each lower to one `copy` of the type's size
at the type's alignment (Section 3). The C-HIR interpreter performs the same
byte-and-bitmap transfer. A `volatile`-qualified record without bit-field
members is copied member by member with `volatile` scalar accesses in
declaration order. A `volatile` union has no member the copy could select,
and a `volatile` bit-field has no portable access width, mask or trace
granularity, so both report `E2922`
(`volatile-aggregate-copy-unsupported`) and fail closed; the kernel cohort
contains neither. `_Atomic` objects remain outside this profile (`E2702`). The padding bytes of an initialized
aggregate are uninitialized, and after a copy so are the destination's
(measured: after `b = a`, GCC-14 leaves `a`'s padding bytes in `b`, Clang-19
zeros them; both are consistent with "unspecified", so the profile transfers
initializedness rather than choosing bytes; evidence
`.evidence/builtin-probes/20260826-2c23684c/g2.out`).

**Records across calls.** EvoIR has no record-typed value, so records cross a
call boundary through memory. Each record parameter is one `ptr<generic>`
argument holding the address of a caller-allocated copy of the argument, in the
parameter's declared position. A function returning a record takes one
additional `ptr<generic>` argument, the return destination, after every declared
parameter and before the trailing `mem<R>` and `world` arguments, and returns no
value for it; `return e;` copies `e` into that destination. The caller's
temporaries for both are ordinary `alloca` sites whose `origin` comes from a
reserved half of the ordinal space, disjoint from every source-derived
allocation site, assigned in lowering order so two lowerings of one unit agree
byte for byte. Register-level classification (which of these copies a target ABI
may keep in registers) is a backend concern and never changes this form, but
the backend must be told which pointers are records (revision 1.9.73): a
record parameter is written `%argN: ptr<generic> byval(size, align)` and the
return destination `%argN: ptr<generic> sret(size, align)`, in the function
header, in every `declare`, and in every `fnptr<Sig>` naming such a
signature; the attribute is part of the signature for equality. `size` and
`align` are the record type's. The machine maps these to the platform
convention (EvoMIR Section 6): the zstd rung found EvoCC-built `timefn.o`
faulting inside a GCC-built program because a 8-byte record went through a
hidden pointer where AAPCS64 puts it in a register.

**Flexible array members.** A record whose last member is an array of unknown
bound may be initialized at file scope with elements for that member; the
object's storage is the record size plus the elements, `sizeof` the type is
unchanged (measured: `sizeof` 4, second element readable, both oracles). At
block scope such an initializer is refused by both oracles and here reports
`E2919` (`flexible-array-initializer-not-static`) at the initializer.

**Modifiable lvalues.** An lvalue is modifiable unless its own type is
const-qualified, is an array type, or is a structure or union with a
const-qualified member at any depth. Qualifiers of a pointed-to type never
qualify the pointer: `const char * const *p` is modifiable and `*p` is not.
The test is one predicate on the lvalue's type, applied at every assignment
and update site, with no list of admitted forms (evidence `p19.c`, `m3`).
Measured defect at revision 1.9.32: `init.pn = pnames` with `const char *
const *pn` was `E2101` in 40 units.

**Array lvalues.** Every expression of array type that is an lvalue decays
to a pointer to its first element wherever an array expression may: a named
object, an array-typed member of a structure or union reached through `.` or
`->` (fixed-size, zero-length or flexible), an element of an array of arrays,
a string literal, and a compound literal. One decay rule serves them all, so
`buf->head->iov_len` with `struct kvec head[1]`, `f->fib6_nh->fam` with a
flexible `fib6_nh[]`, `p->buf` with `char buf[0]`, and `"RSDTZW"[i]` are
valid (evidence `p27-array-lvalues.c`, both oracles compile). Measured defect
at revision 1.9.33: these were `E2102` (`invalid-projected-lvalue`) in about
100 units (`WARN_ON` in `sunrpc/svc.h`, `ip6_route.h`, `binfmt_elf.c`).

**String literals at every site.** A string literal decays to `char *` and
then converts under the pointer-sign rule of 10.1.18 at every site: assigning
`"unsupported"` to `unsigned char *` and initializing `const unsigned char
*name` from `""` are accepted (evidence `p31-residue.c`, `strs` and `e`).
Measured defect at revision 1.9.52: `E2102` and `E2202` in five units.

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
| reachable end of a non-`main`, non-void function | `ret` of `uninitialized.T` for the return type `T` (revision 1.9.71): C makes the fall-off undefined only when the caller uses the value, so the value is poison and its use is the ordinary poison report; the reference compilers return whatever the register holds. A non-void `main` still returns 0 at its end |
| `__builtin_unreachable()` | `trap.unreachable_reached` reports `E3024` |
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

**Anonymous members.** A member of an anonymous structure or union is a member
of the enclosing record for every purpose (C11 6.7.2.1p13): name lookup, `.` and
`->`, a designator in an initializer, and `__builtin_offsetof`. A name is
resolved by searching the record's own members, then recursively the members of
its anonymous members; the resulting `SubobjectPath` records every step it
traversed, so the byte offset and the initializer plan are the same as if the
name had been written out in full. A name found in two different anonymous
members is ambiguous and reports `E2906` at the name (both oracles reject it).

Measured, both oracles, for
`struct outer { int head; union { struct { long regs[2]; long sp; }; long raw[3]; }; int tail; }`:
`sizeof` 40, `offsetof(head)` 0, `offsetof(sp)` 24, `offsetof(raw)` 8,
`offsetof(tail)` 32, and `{ .head = 1, .sp = 7 }` initializes through the
anonymous path; nesting is unbounded. Evidence
`.evidence/builtin-probes/20260826-2c23684c/anon.out` and `anon2.out`.

Measured defect at revision 1.9.19: member *access* traversed anonymous members
while the designator-path resolver did not, so `o.sp` worked but
`offsetof(struct outer, sp)` and `{ .sp = 7 }` both reported `E2906`. The two
resolvers are one rule and must share one implementation.

#### 10.1.7 Function Call Lowering

C function calls lower to EvoIR `call` instructions. Attributes (10.1.9) never
refine a call summary in this revision; `typeof` (10.1.10) and statement
expressions (10.1.11) are resolved before this lowering sees a call.

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
| immediately before a declarator that follows a comma in a declarator list | that declarator alone |
| after `*` (and its qualifiers) inside a pointer declarator, before the rest of the declarator | the pointer type formed by that `*`; preserved, never applied (10.1.14) |
| after `struct`, `union` or `enum` keyword, or after the closing brace | the tag type |
| after a member declarator | that member |
| after a parameter declarator | that parameter |
| after a function declarator, before the body | that function |
| after a label | that label |
| before a null statement (`__attribute__((...)) ;`) | that null statement |
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
`noreturn`, `unused`, `gnu_inline`, `no_instrument_function`,
`cold`, `pure`, `const` and `weak` are preserved without effect here; they carry
linkage, effect or backend semantics that belong to later stages, and applying
a partial reading now would create a second source of truth for each.
`always_inline` has an effect since revision 1.9.74, stated next.

**`always_inline` (revision 1.9.74).** A call whose callee is a function
defined in this unit with the `always_inline` attribute is replaced, in
C-HIR before lowering, by the callee's body: each parameter becomes a fresh
automatic object of the parameter's type initialized with the corresponding
argument after the conversions a call performs; `return e;` becomes an
assignment of `e` to a fresh result object followed by a jump to the end of
the substituted body, and the call expression's value is that object; labels
are renamed so two substitutions of one callee do not collide; a
function-scope `static` object of the callee stays the one object of that
function, shared by every substitution and by the out-of-line body;
`__func__` keeps the callee's name; the effect ordering of the body is the
call's. Substitution proceeds callee-first, so an `always_inline` callee
inside an inlined body is substituted too. A variadic callee, a callee whose
address the unit also takes, and a cycle of `always_inline` calls are not
substituted: the first two are lowered as ordinary calls, the cycle is
`E2932` (`always-inline-cycle`; GCC and Clang both refuse it as "inlining
failed"). After substitution the front end folds constants again, so an
`asm` operand under an immediate constraint (`i`, `n`, `I`, `J`, `K`, `L`)
whose expression was a parameter now bound to an integer constant
expression is a constant, and 3.11 admits it; the operand that is still
not constant stays `E3021`. The out-of-line body of a `static` `always_inline`
function is **not lowered** when every call to it was substituted and the
unit takes no address of it (revision 1.9.75): the reference compilers
never emit such a body, and lowering it would report `E3021` for an
immediate operand that is only constant after substitution, which is the
kernel's normal case. A body that keeps a use (an address taken, a call the
rules above did not substitute) is lowered as before, internal, and any
immediate operand still not constant there is `E3021` as before. The kernel needs this in 2,727 of
2,799 units: `alternative_has_cap_likely` is `static __always_inline` and
feeds its `cpucap` parameter to `asm goto` under `"i"`, and every caller
passes a constant; the reference compilers see the constant only because
they inline. This is the last construct between the front end and nearly
every kernel unit, measured at the tenth survey.

**Diagnostics introduced.** Messages live in Section 6.3, the single authority;
this table gives conditions only. `E2902`, which rejected compound literals as
not covered, is retired by this revision: Section 10.1.13 covers them, and a
front end that still emits it is defective.

| Code | Condition |
|---|---|
| `E2903` | argument list does not parse, position not in the table, or `aligned` argument not a representable power of two |
| `E2904` | `typeof` of a variably modified type |
| `E2905` | `({ ... })` at file scope or where a constant expression is required |
| `E2906` | designator names a member the type lacks, a member ambiguous between two anonymous members, or an index at or beyond a fixed bound |
| `E2907` | two `_Generic` associations name compatible types, or `default` appears twice |
| `E2908` | `_Generic` has no association compatible with the controlling type and no `default` |
| `E2909` | `__builtin_expect`, `__builtin_constant_p` or `__alignof__` called with the wrong number of operands |
| `E2910` | a resolved builtin name used without a call |
| `E2911` | `_Static_assert` condition evaluates to zero |
| `E2912` | `_Static_assert` condition is not an integer constant expression |
| `E2913` | `__builtin_expect` operand of non-arithmetic type |
| `E2914` | `asm` template names an unknown operand, or an output operand is not a modifiable lvalue |
| `E2915` | `__auto_type` with no initializer, several declarators, or an array/function declarator |
| `E2916` | `case a ... b` with `a > b` or overlapping another case |
| `E2917` | designator index or range bound not an integer constant expression, range with `a > b`, or a range initializer that is not constant |
| `E2918` | `goto` into a statement expression |
| `E2919` | initializer for a flexible array member outside file scope |
| `E2920` | `goto` names a label no enclosing function scope declares |
| `E2922` | copy of a `volatile` union, or of a `volatile` record with a bit-field member |
| `E2923` | array bound folds to a negative value |
| `E2924` | an attribute specifier precedes a statement that is not a null statement |
| `E2925` | `__builtin_object_size` mode is not a constant in `0..3` |
| `E2926` | `va_start` in a non-variadic function, or naming a parameter other than the last named one |
| `E2927` | a library builtin's non-constant form used where a visible declaration of the library function has an incompatible type |
| `E2928` | the address of a `register` object is taken |
| `E2929` | `__builtin_va_arg` names a type the default argument promotions change (`char`, `short`, `_Bool`, `float`, an enumeration narrower than `int`) |
| `E2930` | a call or address-of names a function whose type contains an enumeration still incomplete at that point |
| `E2931` | a function declared without a prototype and not defined in the unit is called with promoted argument types that differ from those of its first call |
| `E2932` | a cycle of `always_inline` calls, which no substitution order can eliminate |

#### 10.1.10 `typeof` and `_Generic`

`typeof(type-name)` and `typeof(expression)` denote a type. `__typeof__` and
`__typeof` are the same construct (both oracles; the kernel's syscall wrappers
write `__typeof`). The result of `typeof(expression)` is the type of the expression
before lvalue conversion: an array-typed lvalue yields the array type, not a
pointer, and qualifiers are retained. The operand expression is not evaluated,
so `typeof(i++)` names `int` and leaves `i` unchanged; the one case in which C
would evaluate it, a variably modified type, is rejected with `E2904` because
this profile has no variable-length objects.

`typeof` produces a type and is fully resolved by the front end; nothing of it
survives into EvoIR. A statement expression inside a `typeof` operand is a
block of its own: every declaration it makes — objects, tags, and
declarations with linkage such as `extern void c(void)` — is scoped to it and
leaves nothing behind. Measured defect at revision 1.9.30: an `extern`
function declared inside `typeof(({ ... }))` escaped the operand's scope, so a
later compatible declaration of the same function was refused as a
redefinition; the kernel's `READ_ONCE` inside `hlist_for_each_entry_rcu`
writes exactly that pattern (two lanes). When its operand is a function designator the result is a
function type, and that type may be used as a declaration specifier exactly as
a typedef naming a function type may (10.1.18): `extern typeof(g) g;` declares
`g` a function and may never define one. Both oracles accept it; the kernel's
`EXPORT_SYMBOL` writes it.

`__typeof_unqual__` is `typeof` with the top-level qualifiers removed: `const`,
`volatile`, `restrict` and `_Atomic` are dropped from the resulting type, and
nothing else changes. Its operand is a type name or an unevaluated expression,
exactly as for `typeof`, and it is likewise fully resolved by the front end.
Measured, both oracles (`.evidence/builtin-probes/20260826-2c23684c/tuq.out`,
`tuq2.out`): `const int`, `volatile int`, `const volatile int` and `_Atomic int`
all give `int` and the result is writable; `int *const` gives `int *` while
`const int *` and `const char *const` keep the pointee qualifier; a `const`
member of a record stays `const`; the operand is not evaluated. The cohort
writes it 4,797 times.

`ORACLE_CONFLICT` on an array: for `const int arr[3]`, GCC-14 keeps the element
qualifier, so a store through `__typeof_unqual__(arr)` is refused, while
Clang-19 strips it and admits the store. This profile keeps the qualifier,
which is the direction that refuses a write rather than admitting one.

Only the `__typeof_unqual__` spelling exists in this dialect; the C23 keyword
`typeof_unqual` is not admitted, as neither oracle admits it under `gnu11`.

`_Generic(controlling-expression, association-list)` is C11 generic selection
and is resolved by the front end in the same way. The controlling expression
is not evaluated; its type is taken after lvalue conversion, so qualifiers are
dropped and an array or function designator becomes a pointer. Exactly one
association may name a type compatible with that type; if one does, it is
selected, otherwise the `default` association is selected. No two associations
may name compatible types and `default` may appear at most once (`E2907`); a
selection with no compatible association and no `default` is `E2908`. Only the
selected expression is type-checked as part of the enclosing expression and only
it is evaluated; the others are parsed and their type names resolved. The
selection has the type, value category and value of the selected expression, is
a constant expression when that expression is, and nothing of the selection
survives into EvoIR: the lowered module is byte-identical to the one obtained by
writing the selected expression in its place.

#### 10.1.11 Statement Expressions

`({ compound-statement })` is an expression whose value is that of its last
statement when that statement is an expression statement, and `void` otherwise.
The value undergoes lvalue conversion: a statement expression is never an
lvalue and its type is never an array or function type.

Objects declared inside end their lifetime at the closing brace, exactly as for
any block, so a pointer to one that escapes is dangling under Section 5.2.
`break`, `continue`, `goto` and `return` may leave a statement expression;
jumping into one is `E2918` (`jump-into-statement-expression`) at the `goto`
(both oracles reject it).

A statement expression is permitted **anywhere inside a function definition**
and nowhere outside one; outside, it is `E2905`
(`statement-expression-outside-function`). That is the whole rule, and it is
what both oracles enforce ("braced-group within expression allowed only inside
a function"). In particular it is permitted inside `typeof`, `sizeof` and
`_Alignof`, where the operand is unevaluated and only its type is taken, even
when that operand sits inside a `_Static_assert` condition: the kernel's
`container_of` writes exactly that, and 26 lanes stopped on it.

Measured defect at revision 1.9.22: `E2905` also fired wherever a constant
expression was required, which is not the rule. Both oracles additionally fold a
statement expression with a constant result where a constant is required
(`_Static_assert(({ 1; }), "")` and `int a[({ 4; })]` inside a function are
accepted). This profile does not fold one: where the *value* is required as an
integer constant expression the context reports its own diagnostic (`E2912` in a
static assertion, `E2923` or `E2203` in an array bound). That is a deliberate
deferral, counted rather than silent; a later revision may fold it.

Lowering places the compound statement's blocks into the enclosing function's
control-flow graph as ordinary blocks. When the statement expression contains no
control flow, its value is simply the last instruction's result. When it does,
lowering creates a join block with one block argument for the value, in
addition to whatever block arguments the enclosing lowering already threads
across that edge (the current `mem<all>` and `world` tokens and any live
local carried by value); the value argument is the last one. Every path
that reaches the closing brace passes its value on that edge. On every exit
from the statement expression, including `break`, `continue`, `goto` and
`return`, the objects it declared end with the same `lifetime.end` sequence
the enclosing block lowering emits for any block exit (Section 3, `lifetime.end`;
no new edge form exists: the exits are ordinary block exits). Nothing
new is added to EvoIR: the construct is entirely a lowering pattern over
Section 2.3 block arguments, which is why it belongs in this contract rather
than in the instruction set.

#### 10.1.12 Designated Initializers

A designator list `.member`, `[index]`, or any chain of them, names a subobject
as a `SubobjectPath` of `Field` and `Index` steps, in the same vocabulary as
every other initializer action. The GNU range form `[a ... b]` expands to one
`Index` step per value in the closed range and is otherwise identical.

After a designated subobject, positional initialization continues from the
subobject that follows it, as it would after an undesignated one (measured:
`{ [1] = 1, 9, 9 }` has four elements). When two
actions name overlapping paths, the later one wins and the earlier is dropped
from the plan; the written form still records both, so the dump shows what the
source said. A designator naming a member the type does not have, an index at
or beyond a fixed bound, or any designator applied to a scalar, reports `E2906`
at the designator. An index or range bound that is not an integer constant
expression, or a range with `a > b`, is `E2917`
(`designator-index-not-constant`) at that expression. For an array of
unknown bound the bound becomes one more than the largest index any
designator or positional element reaches (measured: `[5] = 1` gives 6,
`[2 ... 4] = 7` gives 5). A range designator's initializer must be an
integer constant expression or an address constant: the oracles disagree
on a non-constant one (GCC-14 evaluates it once, Clang-19 refuses), so this
profile refuses it with `E2917` (`ORACLE_CONFLICT`, evidence
`.evidence/builtin-probes/20260826-2c23684c/hgaps.out`).

Nothing changes below the plan: lowering consumes `InitAction`s exactly as
before, so an initializer that arrives through designators is
indistinguishable, in EvoIR, from one written positionally.

**Range designators and inferred bounds.** `[lo ... hi]` designates every
element from `lo` to `hi` inclusive; `lo` and `hi` are integer constant
expressions with `lo <= hi`. An array declared without a bound takes as its
size the larger of the positional count and one more than the largest index
any designator or range names, at any nesting depth of the same array. A
later initializer for an element already initialized replaces it (Clang-19
warns `-Winitializer-overrides`, both oracles accept; evidence
`p22-designators.c`, whose `check1..3` arrays make the sizes compile-time
assertions). Measured defect at revision 1.9.33: range designators were
`E1001` and designators past the positional count in an unbounded array were
`E2906` in 64 units.

#### 10.1.13 Compound Literals

`(type-name){ initializer-list }` denotes an object of that type, initialized
by the list, and is an lvalue whose address may be taken. In a block it has
automatic storage and the lifetime of the enclosing block; at file scope it has
static storage. Each occurrence is its own allocation site in the shared
manifest of Section 10.1.3, in source order with every other site, so both
interpreters and the lowering agree on its `ObjectOrigin`.

Lowering is the existing object path: an `alloca` plus the initializer plan for
a block-scope literal, a module global with initial bytes for a file-scope one.
A file-scope literal is a global of kind `compound_literal` (Section 2.1); its
symbol is generated from its allocation-site ordinal, `.lit.<ordinal>`, so two
lowerings of one unit agree byte for byte. A file-scope literal of
`const`-qualified type is a read-only object exactly as a string-literal global
is (Section 5 read-only storage now names both kinds), and a store through it
reports `E3014`; an unqualified one is writable. A file-scope *pointer*
initializer that takes a literal's address (`int *r = (int []){5, 6};`, accepted
by both oracles) needs the static relocation schema Section 2.1 defers; until
that schema exists lowering reports `E3026` (`not-lowered: static-relocation`)
and fails closed, and the receipt counts each occurrence.

#### 10.1.14 Kernel builtins resolved by the front end

Three GNU builtins block every kernel parse lane once `typeof` resolves. All
three are resolved by the front end and none survives into EvoIR; the lowered
module is byte-identical to the one obtained by writing the resolved form.
The rules below were measured in the sealed GCC-14/Clang-19 image
(`.evidence/builtin-probes/20260825-2c23684c`); where the two compilers
disagree the profile records `ORACLE_CONFLICT` and takes the rule both
compilers document as permitted, never a vote.

- `__builtin_expect(e, c)`: both operands are converted to `long`; the result
  is the converted `e` with type `long`. `e` is evaluated; `c` is parsed and
  converted but not evaluated (both oracles: a side effect in `c` does not
  happen). `c` need not be a constant (both oracles accept a non-constant `c`
  without diagnostic). The result is an integer constant expression exactly
  when `e` is. The expectation is dropped by the front end; no hint reaches
  EvoIR in this revision.
- `__builtin_constant_p(e)`: `e` is parsed and type-checked but never
  evaluated. The result has type `int` and is an integer constant expression.
  It is `1` when `e` is an integer constant expression or a string literal,
  and `0` otherwise, in particular for address constants (`&g` is `0` in both
  oracles), objects, parameters and every expression with side effects.
  `ORACLE_CONFLICT`: a `const`-qualified local initialised with a constant is
  `0` in GCC-14 at `-O0` and `1` in Clang-19; both compilers document `0` as
  always permitted, so this profile answers `0`. The predicate is one
  function shared by the parser, the C-HIR interpreter and lowering; no
  optimizer knowledge may change its answer.
- `__alignof__(type-name)` and `__alignof__(expression)`: the alignment of the
  type, or of the expression's type before lvalue conversion, as an integer
  constant expression of type `size_t`; the expression is not evaluated. It
  is the C11 `_Alignof` with an expression operand admitted, and the same
  target layout answers both.

**`packed` on an enumeration.** `packed` on an enum type selects the smallest
integer type that represents every enumerator: by value range `0..255` one
unsigned byte, `0..65535` two, otherwise four; with a negative enumerator the
smallest *signed* type instead (measured: `-1..6` is 1 byte, `-1..200` is 2,
because 200 does not fit a signed byte). Alignment equals size. The
enumeration *constants* keep type `int` regardless (`sizeof(A1)` is 4), and
the underlying type's signedness is observable (`(enum e5)-1 < 0` is 1). The
attribute is accepted before the tag and after the closing brace alike. All
measured, both oracles, `.evidence/builtin-probes/20260901-2c23684c/penum.out`;
the cohort declares 123 packed enums and 15 lanes stopped on `sizeof(enum
rw_hint)` folding as 4 instead of 1 inside a static assertion.
`ORACLE_CONFLICT`: `packed` and `aligned(4)` together on an enum give 1/1 in
GCC-14 and 1/4 in Clang-19; the cohort never combines them, so the combination
is refused (`E2903`) rather than arbitrated.

**Enumerators outside `int`.** An enumerator whose value does not fit `int` is
a GNU (and C23) extension: the enumeration's underlying type becomes the
smallest of `unsigned int`, `long`, `unsigned long` that represents every
enumerator, signed when any enumerator is negative (measured: `0xffffffff00000000`
gives an 8-byte enum whose constant has type `unsigned long`; `-1` with
`0x100000000` gives an 8-byte enum whose wide constant has type `long`). A
value that fits no such type is `E1303`. `ORACLE_CONFLICT` on the *other*
constants of such an enum: GCC-14 gives every constant the underlying type
(`SMALL` is `unsigned long`), Clang-19 keeps `int` for those that fit. This
profile follows GCC, which is also the C23 rule (all enumerators take the
enumerated type once the list is complete); the difference is observable only
through `_Generic` and `sizeof` on the constant. Evidence
`.evidence/builtin-probes/20260901-2c23684c/p10.out`.

**`__attribute` spelling.** `__attribute` is a synonym for `__attribute__`
(both oracles); the kernel writes it. It is read wherever `__attribute__` is.

Pointer-declarator attributes (10.1.9, position after `*`) are preserved and
not applied: `aligned` in that position is `ORACLE_CONFLICT` (GCC-14 applies
it to the pointer type, Clang-19 does not), so this profile applies nothing
there and lists the attribute by name as preserved-not-applied.

Errors: a builtin used with the wrong arity is `E2909`
(`builtin-arity`); a builtin name used other than as a call is `E2910`
(`builtin-not-called`); an `__builtin_expect` operand that is not of
arithmetic type is `E2913` (`builtin-operand-not-convertible`; both oracles
reject a pointer or a structure there and accept a floating operand, which
converts to `long`).

`_Static_assert(condition, message)` and the C23 one-operand form: the
condition must be an integer constant expression, else `E2912`
(`static-assertion-not-constant`) at the condition's span; a condition that
evaluates to zero is `E2911` (`static-assertion-failed`) at the `_Static_assert`
keyword. `__builtin_constant_p` and `__builtin_types_compatible_p` are integer
constant expressions in that position; `__builtin_*_overflow` is not (both
oracles reject it in a static assertion).

**Library builtins.** `__builtin_memset`, `__builtin_memcpy`,
`__builtin_memcmp`, `__builtin_strcmp`, `__builtin_strncmp` and
`__builtin_strlen`, and `__builtin_memmove` (`void *(void *, const void *, size_t)`), have the prototypes of the library functions they name; `__builtin_prefetch(addr[, rw[, locality]])` is `void` with one to three arguments; `__builtin_extract_return_addr(p)` returns `void *` (evidence `p29-residue.c`, `lib`, `ra`, `tys`)
(`memset` returns `void *`, `strlen` returns `size_t`); `__builtin_alloca(n)`
returns `void *`; `__builtin_va_list`, `__builtin_va_start`,
`__builtin_va_arg(ap, T)` (of type `T`) and `__builtin_va_end` are typed by the
front end while their execution stays `E3030`. `__builtin_isdigit`,
`__builtin_isalpha`, `__builtin_isspace`, `__builtin_toupper` and
`__builtin_tolower` exist only in GCC-14; Clang-19 has no such builtins
(evidence `p26-builtins.c`), and the kernel's `ctype.h` selects them with
`__has_builtin`, which the profile answers as GCC does. They return `int`
with C-locale semantics over the value of their `int` argument; the full
tables over 0..255 and the answers for -1 and 0x130 (both 0) are recorded in
`p28-table.out` (digest `39c6154f...cb09`), and the implementation must
reproduce that file. Measured defect at revision 1.9.33: these were `E2001`
in 27 units.

#### 10.1.15 Checked arithmetic builtins

`__builtin_add_overflow(a, b, p)`, `__builtin_sub_overflow(a, b, p)` and
`__builtin_mul_overflow(a, b, p)`: `a` and `b` have integer types, which may
differ in width and signedness; `p` is a pointer to a non-`_Bool` integer type.
The operation is computed in infinite precision, the result is converted to the
pointee type as if by assignment (wrapping) and stored through `p`, and the call
yields `_Bool` `1` when the infinite-precision result is not representable in the
pointee type, else `0`. Both operands and `p` are evaluated exactly once, left to
right. The call is never an integer constant expression. A `p` whose pointee is
`const`-qualified is `E2913` at the third operand (both oracles reject it).
`__int128` operands or pointee are `E2913` in this profile (both oracles accept
them; the cohort never uses them there, and 10.1.19 keeps `__int128` value
semantics closed), so the profile fails closed rather than half-implements them. `__builtin_*_overflow_p`
is out of this profile (`ORACLE_CONFLICT`: Clang-19 does not provide it; the
cohort does not use it). Measured in `.evidence/builtin-probes/20260825-2c23684c/`
(`ovf.c`): `mul(70000, 70000) -> int` is `1` with the wrapped `605032704` stored,
`-> long` is `0` with `4900000000`; `add(200, 100) -> unsigned char` is `1`
with `44`; `sub(4000000000u, -1) -> long long` is `0`; `add(-1, 0) -> unsigned`
is `1`.

The front end type-checks the call and keeps it in C-HIR as a checked
arithmetic node; the C-HIR interpreter implements the rule above. Lowering
(revision 1.9.66) computes the mathematical result exactly and then asks
whether it fits: the `bits<W>` arithmetic is **unsigned** when the
operation is `mul` and both operands are unsigned, and **signed**
otherwise (revision 1.9.67: the executor found that `unsigned long *
unsigned long` needs 129 signed bits, `E3042` in 48 units; as an unsigned
product it needs exactly 128). Let `W` be the smallest of 32, 64 and 128 in
which every mathematical result of the operation on the operands' types is
representable under that signedness (`add`/`sub`: one more than the wider
operand, counting an unsigned operand as one bit wider; signed `mul`: the
sum of the two widths so counted; unsigned `mul`: the sum of the two
widths; every cohort use fits 128). Each operand is
`sext` or `zext` to `bits<W>` by its own signedness, the operation is
performed in `bits<W>`, the stored value is `trunc` to the result type
`R`, and the builtin's value is `icmp.ne` between the `bits<W>` result and
the `sext`/`zext` (by `R`'s signedness) of that truncation: 1 exactly when
the mathematical result is not representable in `R`, which is the C rule
above (with unsigned `bits<W>` arithmetic the comparison is against the
`zext`/`sext` of the truncation read as a `W`-bit pattern, and a value at
or above `2^(W-1)` compares unequal to any sign extension, which is the
right answer because no signed `R` holds it). No UB predicate is involved,
because the arithmetic in `bits<W>` cannot overflow by construction.
`E3020` (`not-lowered: checked-arithmetic`) and `E3042`
(`not-lowered: checked-arithmetic-width`, the executor's fail-closed name
for the 129-bit case) are withdrawn.

#### 10.1.16 GNU `asm` at the front-end boundary

Syntax accepted, in every position GNU C accepts it:

- basic `asm("template");` at file scope and as a statement;
- extended `asm` with up to four colon-separated sections: outputs, inputs,
  clobbers, and (for `asm goto`) labels; each operand is `[symbolic-name]`
  optional, a constraint string, and a parenthesised expression;
- qualifiers `volatile`, `inline`, `goto` in any order, each at most once;
- `__asm__` and `__volatile__` spellings;
- an `asm("name")` label after a declarator (`int x asm("sym");`), which
  renames the object or function for the assembler and is otherwise inert.

The front end checks that output operands are modifiable lvalues, that every
`%[name]` in the template names an operand, that `asm goto` label operands are
labels in scope, and that string literals concatenate; it keeps the statement
in C-HIR losslessly (template text, operands with constraints, clobbers,
labels, qualifiers, spans). Constraint strings are preserved as text and not
validated in this revision. `asm` has maximal effects: it reads and writes
`mem<all>` and `world`, may not be reordered, duplicated or removed, and an
`asm goto` is a terminator whose successors are its labels plus fall-through.
Lowering produces the `asm` instruction of Section 3.11 when every operand
constraint, clobber and template reference is in the vocabulary that section
admits, and reports `E3021` (`not-lowered: asm`) naming the offending
constraint, clobber or reference otherwise. A file-scope `asm("template")`
lowers to one entry of the module assembly table (Section 2.1, revision
1.9.66) and is never `E3021`; the sixth full-configuration survey showed
`EXPORT_SYMBOL` as the whole of that residue. An output operand lowers to a
result of the operand's lowered type followed by a store to the operand's
place; an input operand lowers to the loaded value; a `+` operand lowers to
both, tied; a `Q` operand lowers to the address of its place, and the place
is not otherwise loaded or stored, so the memory effect of Section 3.11 is
what covers it. Both interpreters report `E3021` when they reach an `asm`:
it is executed by the native run and by nothing else. Revision 1.9.58
replaced the unconditional deferral of revision 1.9.

Errors (`E2914`, `malformed-asm`), each at the span named: an output operand
that is not a modifiable lvalue, at the operand expression; a `%[name]` that
names no operand or label, at the template string literal; a qualifier written
twice, at the second qualifier; an `asm goto` label that does not resolve, at
the label operand; a labels section without `goto`, at the fourth colon.

**Statement placement.** An attribute specifier may precede a **null statement**
and no other statement: `__attribute__((__fallthrough__));` is admitted, and an
attribute before any other statement is `E2924`
(`attribute-on-non-null-statement`) at the attribute. Both oracles behave that
way, and all 221 uses in the kernel cohort are null statements. The attribute is
preserved in C-HIR with its placement and applied to nothing: a null statement
has no semantics to refine, so `fallthrough` in particular does not change
control flow, suppress a diagnostic, or reach EvoIR. Evidence
`.evidence/builtin-probes/20260830-2c23684c-frontier/rulings.out`.

Lossless means: the qualifier spellings and `asm`/`__asm__` spelling as
written, the template as the single concatenated string the assembler will
see (the individual string-literal pieces are not kept), every operand's
symbolic name, constraint text and expression, clobber strings in order, and
label names. The keyword-alias and piece spellings are the only things the
representation drops.

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

**Fuel.** An interpreter that exhausts its shared step budget stops with
`E3008` (`interpreter-fuel-exhausted`). That stop is a limit of the host, not a
property of the program: it is neither undefined behaviour nor a comparison
result. A differential trial that hits it on either side is counted on its own
and is never folded into the undefined-behaviour discard count or the
mismatch count. Defined at revision 1.9.35 after the randomized gate found the
interpreter emitting the code without an allocation (0 of 21,000 trials hit
it under a 200,000-step budget).

#### 10.1.17 Compile-time builtins, `__auto_type` and small GNU forms

All measured in `.evidence/builtin-probes/20260826-2c23684c/win_i.out`; GCC-14
and Clang-19 agree on every row.

- `__builtin_choose_expr(c, a, b)`: `c` is an integer constant expression;
  the result *is* `a` when `c` is nonzero, else `b`: same type (no conversion),
  same value category (an lvalue stays assignable), same constant-expression
  status. The unselected operand is parsed and type-checked but never
  evaluated. Non-constant `c` is `E2912` at `c`.
- `__builtin_types_compatible_p(T1, T2)`: integer constant expression of type
  `int`, `1` when the two type names are compatible after dropping top-level
  qualifiers (`const int` vs `int` is `1`; `int[4]` vs `int[]` is `1`).
- `__builtin_offsetof(T, designator)`: the designator is a member path with `.`
  and `[index]` steps (`i` is 4, `arr[2]` is 16, `in.x` is 24 in the probe). A
  path the type lacks is `E2906`. With every index an integer constant
  expression the result is an integer constant expression of type `size_t`.

  An index need not be constant. `__builtin_offsetof(T, m[e])` is then the
  runtime value `offsetof(T, m) + e * sizeof(*m)`, `e` is evaluated exactly
  once, and the result is **not** an integer constant expression, so each
  context that requires one reports its own diagnostic (`E2912` in a static
  assertion, `E2923` or `E2203` in an array bound). The index is not bounds
  checked: a negative or past-the-end index yields the arithmetic value, which
  is what both oracles do and what a flexible array member requires
  (measured: `arr[-1]` is 0 and `arr[9]` is 40 for
  `struct { int head; int arr[4]; }`).
- `__builtin_ffs`, `__builtin_ffsl`, `__builtin_ffsll`, `__builtin_clz`,
  `__builtin_clzl`, `__builtin_clzll`, `__builtin_ctz`/`l`/`ll`,
  `__builtin_popcount`/`l`/`ll`: result type `int`; `ffs(0)` is `0`; `clz`/`ctz`
  of `0` is undefined (`E3004`-class runtime report `E3022`,
  `bit-builtin-zero-operand`); an integer-constant-expression operand makes the
  call an integer constant expression (both oracles accept them in
  `_Static_assert`). The front end folds constant operands; non-constant
  operands become a C-HIR intrinsic that lowers to the `ctlz`, `cttz`,
  `ctpop` and `bswap` instructions of Section 3.4 (revision 1.9.66; the
  zero-operand obligation is `ub.assume.bit_builtin_zero_operand`). `E3023`
  (`not-lowered: bit-builtin`) is withdrawn.
- `__builtin_unreachable()`: a statement-expression of type `void`; reaching
  it is undefined behaviour reported as `E3024` (`unreachable-reached`) by the
  interpreters and lowered to `trap.unreachable_reached` (Section 3).
- `__builtin_object_size(p, mode)`: `mode` is an integer constant expression in
  `0..3`; any other value, or a non-constant mode, is `E2925`
  (`builtin-mode-out-of-range`) at the mode operand (both oracles reject it).
  The operand is parsed and type-checked but **never evaluated**
  (measured, both oracles). This profile performs no object-size analysis and
  therefore always answers as if the object were unknown: `(size_t) -1` for
  modes 0 and 1, `0` for modes 2 and 3, as an integer constant expression. That
  is the answer both oracles give for a genuinely unknown object, and it asserts
  nothing about storage the compiler has not analysed.

  This is a deliberate profile choice, not a conformance claim: for an object
  the oracles *can* see, they answer with its size (16 for a known array) and
  this profile still answers unknown. It also makes a recorded disagreement moot
  rather than arbitrated — for a side-effecting expression naming a known array,
  GCC-14 answers `(size_t) -1` and Clang-19 answers `16`. All 42 uses in the
  kernel cohort are modes 0 and 1 on parameters and none is in a constant
  context, so nothing in the cohort depends on the difference. A later revision
  that adds the analysis must say so and re-measure.
- `__builtin_has_attribute(operand, attribute)`: a GCC builtin; Clang-19 does
  not provide it, and it appears only in GCC-dialect input, which is what
  EvoCPP presents itself as (10.4). The result is an integer constant
  expression of type `_Bool` (measured: `sizeof` 1, usable in
  `_Static_assert`). The operand is a type name or an expression; an
  expression is not evaluated, and the answer is `1` exactly when the named
  attribute was written on the declaration of the entity the expression
  designates — for `nonstring`, on the array member it names, through `.` or
  `->` alike — and `0` otherwise, including for a type name and for an entity
  declared without it. Because 10.1.9 preserves every attribute losslessly, the
  answer is read from that inventory; no attribute needs a meaning to be
  queried. The cohort asks only about `nonstring`, 34 times, inside
  `_Static_assert`. Evidence `.evidence/builtin-probes/20260901-2c23684c/hasattr.out`.
- **Variable arguments at the front end.** `__builtin_va_start(ap, param)`,
  `__builtin_va_end(ap)` and `__builtin_va_copy(dst, src)` are accepted with
  type `void`: `ap`, `dst` and `src` must have type `__builtin_va_list`
  (10.1.5), `va_start` may appear only in a function whose parameter list ends
  in `...` (both oracles reject it elsewhere; `E2926`,
  `va-start-outside-variadic-function`), and `param` must name the last named
  parameter (both oracles accept another parameter silently and the standard
  makes it undefined, so this profile refuses it, `E2926` at `param`).
  `__builtin_va_arg(ap, T)` is admitted at the front end (revision 1.9.45,
  after the full configuration showed three units using `va_arg`): `ap` must
  have type `__builtin_va_list` and `T` must be an object type that is not an
  array or function type; the expression is an rvalue of type `T`, kept in
  C-HIR as a `VaArg` node holding the list place and the type, with no
  constant value. Lowering (revision 1.9.61): a function type whose
  parameter list ends in `...` lowers to a variadic signature (Section
  1.1), for declarations, definitions and `fnptr` alike; a call passes the
  arguments after the last named parameter as anonymous arguments after the
  default argument promotions (`char`, `short`, `_Bool` and enumerations to
  `int`, `float` to `double`; Section 3.10); `__builtin_va_start(ap, param)`
  lowers to `va_start` on the address of `ap`, `__builtin_va_arg(ap, T)` to
  `va_arg.T'` where `T'` is the lowering of `T` after the same promotions
  applied to `T` (so `va_arg(ap, char)` and `va_arg(ap, short)`, undefined in
  C, are `E2929` at the type), followed by
  the conversion to `T`; `__builtin_va_end(ap)` to `va_end`; and
  `__builtin_va_copy(dst, src)` to a 32-byte `copy` (Section 3.12). A
  `va_arg` of a record or union type, and a record or union passed as an
  anonymous argument, stay `E3030` (`not-lowered: varargs`); the cohort has
  neither. `E3030` is no longer reported for a variadic declaration or call.
- **Empty records.** A struct or union whose member list declares no member
  (for instance one containing only a `_Static_assert`) is a GNU extension with
  `sizeof` 0 and alignment 1 (measured, both oracles); the kernel's
  `__must_be_cstr` writes `sizeof(struct { _Static_assert(...); })`.
- `__builtin_strlen(s)`: with a string-literal operand, an integer constant
  expression of type `size_t` equal to the literal's length (both oracles
  fold it in `_Static_assert`). With any other operand it is an ordinary call
  to the external function `strlen` with the prototype the builtin carries
  (10.1.14, `size_t (const char *)`), whether or not a declaration of
  `strlen` is visible (evidence `p26-builtins.c`, which declares none and
  compiles under both oracles); a visible declaration of `strlen` with an
  incompatible type is `E2927` (`builtin-library-declaration-incompatible`).
  The same holds for every library builtin of 10.1.14: the builtin's
  prototype is the call's type, and the call names the external function.
  No intrinsic reaches EvoIR. Measured: with a user-defined `strlen` both
  oracles call it for a non-literal operand. Revision 1.9.45 replaced the
  earlier rule that required a visible declaration.
- `__builtin_frame_address(n)`: type `void *`; `n` must be an integer
  constant expression (both oracles reject a non-constant); accepted by the
  front end for any constant `n` (both accept `1`). `(0)` lowers to
  `frame_address` (Section 3.7, revision 1.9.72); a level above zero walks
  the caller's frames and stays `E3031` (`not-lowered: frame-address`). The
  cohort writes `(0)` in four lanes.
- `__builtin_return_address(0)`: type `void *`; accepted by the front end;
  lowers to `return_address` (Section 3.7, revision 1.9.72); `E3025`
  (`not-lowered: return-address`) is withdrawn for level zero, and an
  argument other than the constant `0` is `E2913`.
- `__auto_type`: exactly one declarator, which must be a plain identifier (no
  pointer, array or function declarator), and an initializer is required; the
  declared type is the initializer's type after lvalue conversion (qualifiers
  dropped, arrays and functions decay: measured `const int` gives `int`,
  `int[3]` gives `int *`). The declared object may not appear in its own
  initializer. Violations are `E2915` (`auto-type-misuse`) at the declarator.
  `ORACLE_CONFLICT` on a pointer declarator (`__auto_type *p = &l;`): GCC-14
  refuses it, Clang-19 accepts it, so this profile refuses it; both refuse the
  array form and the self-reference. Evidence
  `.evidence/builtin-probes/20260826-2c23684c/gaps3.out`.
- A stray `;` where an external declaration or a block item may appear is an
  empty declaration and has no effect. Both oracles accept it silently and warn
  only under `-pedantic`; the kernel writes one after every `static inline`
  function that a macro expanded. It is not a diagnostic in this dialect.
- `a ?: b`: `a` is evaluated once; the result is `a` when nonzero, else `b`,
  with the usual conditional-operator typing.
- `case a ... b`: inclusive range in a `switch` whose bounds are integer
  constant expressions. A non-constant bound, `a > b`, or a range overlapping
  another case is `E2916` (`case-range-invalid`) (all three rejected by both
  oracles).
- `__label__ name;` at the start of a block declares a block-local label. A
  `goto` naming a label that no enclosing function scope declares is `E2920`
  (`undeclared-label`) at the label name (both oracles reject it).
- Label addresses: `&&name` is the address of a label in the current function,
  of type `void *`; it is an address constant (10.1.18 "Label addresses",
  measured at revision 1.9.48 under both oracles), so a static initializer
  may use it, and `&&name` where no enclosing function scope declares that
  label reports `E2920`. `goto *expr` transfers control to such an address.
  Both forms are parsed, type-checked and carried in C-HIR; lowering reports
  `E3029`. Revision 1.9.49 corrected this entry, which had said the opposite
  since 1.9.17 without a probe.

  Neither reaches EvoIR in this revision. An indirect branch would need a
  terminator whose successor set is decided at run time, edge arguments for each
  successor, validator rules, and proof-checker handling, and no unit of the
  cohort needs it: the kernel writes `({ __label__ __here; __here:
  (unsigned long)&&__here; })` to capture an instruction address, whose cast to an
  integer is `ptrtoint` and already outside this profile (Section 5.3), and
  writes no `goto *` at all. Since revision 1.9.68 the label address is
  `block_addr` and its cast is `ptr.to_int`; only `goto *` reports `E3029`
  (`not-lowered: computed-goto`) and fails closed. The measurement that
  opened this was the ninth survey, whose first error was `E3029` in 1,040
  units through `local_bh_disable`.

**Function names.** Inside every function body the identifiers `__func__`
and `__FUNCTION__` denote one implicitly declared object of type
`static const char[N]` holding the function's name: `sizeof __func__` is N,
`_Generic` sees `const char *` after decay, `&__func__` is
`const char (*)[N]`, and its address is an address constant (`static const
char *p = __func__;` is accepted). `__builtin_constant_p(__func__[0])`
follows the constant rule of this section and answers 0, which makes the
oracles' disagreement on it (GCC-14 0, Clang-19 1) moot. `__PRETTY_FUNCTION__`
is `E2921` (`pretty-function-unsupported`): `ORACLE_CONFLICT`, GCC-14
spells it `main`, Clang-19 `int main(void)` (evidence `p17.out`); the pinned
configuration builds no unit that uses it. Measured defect at revision
1.9.32: `__func__` was `E2001` in 508 units.

**Labeled last statement.** The value and type of a statement expression whose
last statement is a labeled statement are those of the statement the label
prefixes, recursively; `__label__` declarations inside the statement
expression do not change its type (evidence `p19.c`, `ll`, both oracles
compile `r = ({ __label__ out; ...; out: q; })`). Measured defect at revision
1.9.32: the kernel's `___wait_event` was `E2102` in 41 units.

#### 10.1.19 Integer constant expressions and array bounds

The specification names an integer constant expression in a dozen places
(`_Static_assert`, `case`, array bounds, bit-field widths, designator indexes,
`aligned`, enumeration constants, the builtins of 10.1.14 and 10.1.17) without
ever saying what one is. It is the C11 definition with this profile's
extensions, and every one of those places uses the same evaluator:

- integer, character and enumeration constants;
- `sizeof` and `_Alignof` applied **to a type name or to an expression**; the
  operand expression is not evaluated, only its type is taken, so
  `sizeof(*(int *)8)` is `4` and `sizeof(i++)` leaves `i` alone;
- `__builtin_offsetof`, `__builtin_types_compatible_p`, `__builtin_constant_p`,
  `__builtin_choose_expr` and the constant-operand bit builtins (10.1.14,
  10.1.17);
- casts to an integer type, the unary and binary arithmetic, bitwise,
  relational and logical operators, and the conditional operator, applied to
  the above.

`sizeof(void)` is `1`, as is `sizeof` of a function type; both are GNU
extensions the kernel relies on (measured, both oracles).

**Signed arithmetic in a constant expression** is evaluated in the operand
type's width with two's-complement wrap-around, as both oracles evaluate it,
not in host arithmetic and not as a refusal: `1 << 31` is the `int` value
`-2147483648`, `(1 << 31) < 0` holds in a static assertion, and an enumerator
`X = 1 << 31` has type `int` in a 4-byte enumeration (measured). This is the
constant evaluator's arithmetic only; the runtime obligations of 10.1.5 are
unchanged, so `1 << 31` executed at run time still carries
`ub.assume.invalid_signed_left_shift`. Measured defect at revision 1.9.28:
the enumerator was refused as not an integer constant expression.

`__int128_t` and `__uint128_t` are compiler-provided spellings of `__int128`
and `unsigned __int128`, not distinct types: both oracles give them size 16 and
`__builtin_types_compatible_p(__uint128_t, unsigned __int128)` is `1`. The
cohort writes `__uint128_t` 102 times. They are type specifiers wherever the
underlying spelling is one, and carry its value semantics unchanged.

**Null pointer constants.** An integer constant expression with value zero, or
such an expression cast to `void *`, is a null pointer constant (C11 6.3.2.3p3)
and converts, in assignment, initialization, argument passing, return and
comparison, to **any** pointer type — object pointers and function pointers
alike. Measured defect at revision 1.9.23: `call->fn = ((void *)0);` was
`E2102` for a function-pointer member while `= 0` was accepted; the kernel
writes the cast form throughout. The resulting value has `Null` provenance.

**Address constants through integer casts.** An address constant cast to an
integer type and back to a pointer type, `(void *)(uintptr_t)&f`, is an address
constant (both oracles fold it; the kernel's `EXPORT_SYMBOL` writes exactly
this). This admits the *form*; whether the initializer can be lowered is
still Section 2.1's relocation question, counted as `E3026`.

**Conditional operator with a null pointer constant.** In `c ? a : b` where
one of `a` and `b` is a null pointer constant and the other is a pointer, the
result type is the *other* operand's pointer type, not a composite with `void *`
(C11 6.5.15p6). This is load-bearing: the kernel writes

```c
#define __is_constexpr(x) \
    (sizeof(int) == sizeof(*(8 ? ((void *)((long)(x) * 0l)) : (int *)8)))
```

which reads `4 == 4` when `x` is an integer constant expression, because
`(void *)((long)(x) * 0l)` is then a null pointer constant and the conditional
has type `int *`, and reads `4 == 1` otherwise, because the conditional is then
`void *` and `sizeof(void)` is `1`. Both oracles confirm all three values.
Getting this rule wrong does not produce a diagnostic; it produces the opposite
answer in every kernel size and overflow check that uses the macro.

**Array bounds.** A bound is evaluated wherever it is written, not only when an
initializer follows.

A bound of **zero** is valid: a zero-length array is a GNU extension with size
`0` that contributes no bytes and no alignment, may appear anywhere in a record
including before other members, and does not make the record flexible.
Measured, both oracles: `struct { int a; char d[0]; int b; }` is 8,
`struct { int a; char d[0]; }` is 4, `char pad[20 - 2*sizeof(long) - sizeof(unsigned)]`
is a valid empty member whose successor is at offset 0, and a file-scope
`int g[0]` has `sizeof` 0. The kernel writes both the explicit form (`__u8
data[0]`) and the computed form (`sysinfo._f`); evidence
`.evidence/builtin-probes/20260826-2c23684c/zerobound.out`.

A bound that folds to a **negative** value is `E2923` (`invalid-array-bound`);
both oracles reject it. A bound that is not an integer constant expression is
`E2203` (`variable-length-object`) wherever it appears: at file scope and on a
member both oracles reject it too, and inside a block they accept it as a C99
variable-length array, which this profile does not have. `E2203` has one
message, `variable-length-object`, at every site.

Measured defect at revision 1.9.17: `int probe[-1];`, `struct s { int a[-1]; };`
and a file-scope `int probe[n];` were all accepted silently, and only an
initializer made the bound reachable, so no `sizeof` of such a type meant
anything.

**Address constants.** `&E` is an address constant when `E` designates an
object of static storage duration or a function, or an element or member of
such an object reached through constant subscripts and member access
(`&vals[1]`, `&s.m`, `&s.a[2]`); so are an array or function name that
decays, a string literal, and `__func__`. An address constant converted by a
cast to any pointer type or to an integer type at least as wide as a pointer
stays an address constant. This holds in every static initializer position,
nested aggregate members included (evidence `p19.c`, `tt` and `pv`).
Measured defect at revision 1.9.32: `(void *)&sysctl_vals[0]` (`SYSCTL_ZERO`)
was `E2208` in 54 units.

**Integer constants as pointers; decay in static objects.** An integer
constant expression converted by a cast to a pointer type is an address
constant (`(void *) 2`, the kernel's `TIMER_ENTRY_STATIC`). The decay of any
array lvalue that designates part of an object with static storage duration
is an address constant as well: `init_s.x`, `arr2[1]`,
`init_files.close_on_exec_init` (evidence `p25-static-init.c`, both oracles
compile). Measured defect at revision 1.9.33: `DEFINE_STATIC_SRCU`,
`DEFINE_KFIFO` and array-member initializers were `E2208` in 21 units.

**More address constants.** `&"literal"` is an address constant (the address
of the literal's array), so `(kernel_ulong_t)&"PARPORT_SERIAL"` is one under
the integer-cast rule. A comparison of an address constant with a null
pointer constant is a constant expression (`impl == (fn_t)0` is 0,
`impl != 0` is 1), so the `_OF_DECLARE` conditional
`(fn == (fn_type)NULL) ? fn : fn` is an address constant under the
conditional rule of 10.1.18 (evidence `p31-residue.c`, `ids` and `tbl`).
Measured defect at revision 1.9.52: `E2208` in ten units.

**Initializer plans have no size cap.** A plan records the entries an
initializer writes and the zero fill of the rest; its cost is bounded by the
number of entries, never by the object's size: a range designator `[lo ... hi]` is one entry
of the plan (its expansion to elements in 10.1.12 is a description of
meaning, not of representation), and no diagnostic refuses an
object for being large (`_printk_rb_static_infos[4096]` with two designated
entries). `E2207` (`initializer-plan-too-large`), which the implementation
emitted without a definition, is withdrawn at revision 1.9.53.

#### 10.1.20 Source locations

Every token the preprocessor hands to the parser carries two spans. Its
spelling span is the range of the characters that spell it, in the file where
they were written; for a token produced by macro expansion that is the
replacement list or the argument that spelled it. Its expansion span is the
range, in the file the preprocessor was reading when the token was produced,
of the outermost macro invocation that produced it, from the macro name
through the closing parenthesis of its arguments; a token that no expansion
produced has an expansion span equal to its spelling span. Tokens from an
included or `-include`d file carry that file's identity in both spans.

The parser composes node spans from expansion spans only. A composite node's
span is the expansion span of its first token, extended to the end of the
expansion span of its last token when both lie in the same file and the last
does not precede the first; otherwise it is the expansion span of the first
token alone. Composition is total: it never fails and never yields a span
whose end precedes its start. A span is a range in exactly one file; nothing
indexes a file buffer with a span from another file, and the parser never
obtains a token's text by slicing a source buffer: it uses the token's own
spelling. Diagnostics report the expansion span (the use site) and may add the
spelling span as a note.

Measured defect at revision 1.9.31: the parser merged spelling spans and
sliced the main file's buffer with spans that named headers. The
full-configuration survey (run `20260902T023241Z-87371`) recorded 2,768 of
2,832 units panicking at those two places, and the three-line file
`#define TAG s` / `struct s { int x; };` / `struct TAG v;` reproduces the
first. The sixteen cohort units had parsed clean only because their parse
lanes consume oracle-preprocessed text, where every span lies in one file.

#### 10.1.18 C11 declaration forms

The front end accepts the complete C11 declaration grammar, not the subset the
early milestones happened to exercise. Three forms the kernel cohort needs were
absent and are named here so their acceptance is checkable:

- **Specifier order is free.** Type qualifiers, storage-class specifiers and
  function specifiers may appear before or after a type specifier, including
  after a record or enum specifier: `struct midr_range const *ranges` and
  `enum e const x` mean exactly what `const struct midr_range *ranges` and
  `const enum e x` mean. This is 16 of the 28 remaining parse lanes, in 8 units.
- **A typedef may name a function type.** `typedef int fn(void);` then
  `extern fn f;` declares `f` a function, and `fn` may also be written
  parenthesised (`typedef int (fn)(void);`). Such a typedef may not be used to
  define a function body, and an object declared with it is a function
  declaration, never an object. This is 10 lanes, in 5 units.
- **A declarator list may declare several functions.** `fn a, b;` with `fn`
  a typedef naming a function type, and `int f(void), g(void);`, each declare
  every listed name a function; none of them may be a definition. Measured
  defect at revision 1.9.29: the list form was refused with `E1002` as a
  deliberate implementation restriction from the attribute window ("no
  consumer in this profile"); the kernel's `fs_param_type fs_param_is_bool,
  fs_param_is_u32, ...` is that consumer, six lanes in three units.
- **`typeof` of a dereferenced function pointer.** `*fp`, with `fp` a pointer
  to function, is a function designator, so `typeof(*fp)` is that function
  type and `extern typeof(*fp) h;` declares `h` a function exactly as
  `extern typeof(g) h;` does (10.1.10). Measured defect at revision 1.9.29:
  `typeof(g) h;` was accepted and `typeof(*fp) h;` refused while
  `typeof(*fp) *q` was accepted; the kernel's static-call machinery writes the
  refused form, four lanes in two units.
- **Redeclaration.** An identifier with linkage — a function, or an object
  declared `extern` — may be declared any number of times, in the same scope
  or in nested ones, provided every declaration's type is compatible with the
  previous ones (C11 6.2.7, 6.7p4); the declared type is their composite.
  Attributes on a redeclaration accumulate. Declaring it again with an
  incompatible type is `E2002` (`incompatible-redeclaration`) at the later
  declarator, as is a second definition of an object or function, or a second
  declaration of an identifier without linkage in the same scope. Measured,
  both oracles: repeated compatible block-scope `extern` declarations accepted,
  `extern int g(int)` then `extern long g(int)` refused. Revision 1.9.30 had
  used `E2002` without ever defining it.
- **Static assertions are member declarations.** `_Static_assert` may appear in
  a struct or union member list, including an anonymous one, where it declares
  no member and contributes nothing to layout. This is 2 lanes, in 1 unit.

**Array parameters.** A parameter declared with array type, whatever the
bound and whatever qualifiers or `static` stand inside the brackets, has the
adjusted pointer type in the function's type and at every use of the
parameter name. The adjustment happens once, when the declarator is built,
and no other code path sees the array type (evidence `p19.c`, `m1`). Measured
defect at revision 1.9.32: an initializer taken from an array parameter was
`E2202` in 64 units (`pm_opp.h`).

**Pointer sign in conversions.** In initialization, assignment, argument
passing and `return`, a pointer to an integer type converts to a pointer to
the same type with the other signedness, or between `char`, `signed char`
and `unsigned char`; the conversion is accepted without a diagnostic (both
oracles warn under `-Wpointer-sign`, which the kernel disables; evidence
`p21.out`). Pointers to otherwise incompatible types stay `E2202`, which is
`ORACLE_CONFLICT` resolved toward the refusal: GCC-14 makes
`-Wincompatible-pointer-types` an error, Clang-19 a warning (the probe's
swapped struct order measured that case as well). Measured defect at revision
1.9.32: `DEFINE_CLASS(_gpiochip_for_each_data, ...)`, whose constructor takes
`int *i` for an `unsigned int *` member, was `E2202` in 20 units.

**Function pointers and `void *`.** A pointer to function converts to and
from `void *` in initialization, assignment, argument passing, `return`,
casts and both arms of the conditional operator; `(fn_t)0` and `(void *)0`
are null pointer constants there, and `cond ? impl : (void *)0` has the
function pointer type. Arithmetic on a `void *` operand follows 10.1.3 and
its result may be converted to a function pointer the same way (evidence
`p23-function-pointers.c`, both oracles compile). Measured defect at revision
1.9.33: `_OF_DECLARE`, `SIMPLE_DEV_PM_OPS`, `DEFINE_STATIC_CALL`, `bd_holder`
and `__pa_symbol` assignments were `E2202`, `E2102` or `E1003` in about 60
units.

**Qualified `void *` and function pointers.** The conversion between a
pointer to function and `void *` of 10.1.18 holds for `void *` with any
qualifiers on the pointee (`const void *data` to `fn_t`, and back) at every
site named there (evidence `p29-residue.c`, `cv`). Measured defect at
revision 1.9.47: `cb = match->data` with `const void *data` was `E2102`,
`E2202` or `E2001` in about nine units.

**Conditional address constants.** A conditional expression is an address
constant when its condition is an integer constant expression or an address
constant (a function or object address, which is never null) and the arm
selected is an address constant: `(impl == (fn_t)0) ? impl : impl`,
`1 ? impl : (void *)0`, `impl ? impl : 0` (evidence `p29-residue.c`, both
oracles warn `-Waddress` and compile). Measured defect at revision 1.9.47:
`_OF_DECLARE` tables were `E2208` in nine units.

**Label addresses.** `&&label` is an address constant of type `void *`
usable in static initializers, including range designators
(`[0 ... 255] = &&dflt`); `goto *expr` is accepted by the front end with
`expr` of pointer type. A used `&&label` lowers to `block_addr` of the
label's block (Section 3.7, revision 1.9.68), and as a static initializer
to a relocation against a local code symbol the backend emits for that
block; `goto *expr` alone stays `E3029` (evidence `p29-residue.c`, `lab`). Measured defect at revision 1.9.47: the BPF
interpreter's jump table was `E2208`, `E2001` and `E1003`.

**`_Pragma`, spellings and attribute placement.** `_Pragma("...")` is handled
exactly where `#pragma` would be, including at file scope between
declarations and inside a macro expansion; `__inline` and `__inline__` spell
`inline`; `__restrict` and `__restrict__` spell `restrict`; an attribute may
stand between any two declaration specifiers, as in `static const
__attribute__((unused)) struct r` (evidence `p24-spellings.c`, both oracles
compile). Measured defect at revision 1.9.33: `__bpf_kfunc_start_defs()`,
`MEM_STATIC`, `__maybe_unused` between specifiers and `__restrict` parameters
were `E1001` or `E1002` in 37 units.

**Global register variables.** A file-scope declaration
`register T name asm("reg")` is a global register variable (GNU). It
declares no object and supplies no storage: it has no allocation site, and
the allocation-site manifest of 10.1.6 passes over it rather than failing to
derive one. Its type is `T`, `sizeof` and `_Alignof` are `T`'s (measured 8
and 8 for `unsigned long`), and `&name` is `E2928`. A read yields the
current value of the named machine register and a write sets it. EvoIR has
no machine-register value, and does not need one: a read lowers to the
`asm` instruction of Section 3.11 with the template `mov %0, reg`, one
`=r` output of `T`'s lowered type, no inputs and the `volatile` qualifier;
a write lowers to `asm volatile "mov reg, %0"` with one `r` input (revision
1.9.67, after the eighth survey put the read of `current_stack_pointer`
first in 2,711 units). The asm is *implicit*: it is a lowering artefact, not
source text, and it carries every property of Section 3.11, so the value
is opaque to the equality database, ordered by the effect tokens, and
executed only by the machine; both interpreters report `E3021` when they
reach it, as for any assembly. `E3035`
(`not-lowered: global-register-variable`) is withdrawn. The declaration
alone was already accepted (evidence `p32-register-variables.out`, both
oracles compile the reads and refuse the addresses). A block-scope
`register T name asm("reg")` is an ordinary automatic object whose register
binding matters only when it is an `asm` operand, where `E3021` already
applies; its value behaves as any automatic object's (measured, both
oracles). Measured defect at revision 1.9.56:
`arch/arm64/include/asm/stack_pointer.h` declares
`register unsigned long current_stack_pointer asm("sp")`, which nearly every
arm64 translation unit includes and 20 of 2,798 use; the manifest's refusal
to derive a site for it stopped 2,754 of 2,798 units at lowering, all with
one generic message.

**Lowering diagnostics name their class.** A lowering failure carries a code
of its own, as a front-end diagnostic does; `E3101`
(`evoir-lowering-failed`) is a wrapper for a class that has not been given
one yet, never a resting place. A survey that ranks lowering stops ranks
codes, so every class large enough to appear in a ranking is given a code
before the next survey runs (revision 1.9.57, after one wrapper carried four
distinct classes and 2,768 of 2,794 stops).

**Storage class `register`, va_list objects, 128-bit places.** `register` is
accepted on parameters and block-scope objects and has no effect beyond
forbidding `&` on the object, which is `E2928` (`address-of-register-object`) at the `&` (both oracles reject it). `__builtin_va_list` is an object type that may
be a member, an array element or a typedef target; on the target it occupies
32 bytes at alignment 8 (evidence `p31-residue.c`: a structure holding one
pointer and one `va_list` is 40 bytes at 8). An element of an array of
`__uint128_t` or `__int128_t` is a modifiable lvalue like any element, and
`*__builtin_va_arg(ap, T *)` is a modifiable lvalue of type `T`; both
execute since revision 1.9.61 (Section 3.12, 10.1.19). Measured defect at revision 1.9.52:
`E1001`, `E1005`, `E2101` and `E2102` in six units.

**Parameter compatibility.** Top-level qualifiers on parameter types do not
affect compatibility (C11 6.7.6.3p15): `int (*)(const clockid_t, struct ts *)`
and `int (*)(clockid_t, struct ts *)` are compatible (evidence
`p31-residue.c`, `o`). Function pointer arithmetic `p + n`, `p - n` and the
compound forms scale by one at the front end (GNU; evidence `fnarith`); because Function provenance admits only offset zero (Section 4), lowering reports `E3034` (`not-lowered: function-pointer-arithmetic`) at the operation and fails closed. Measured defect at
revision 1.9.52: `E2202` in three units and `E2102` in one.

**The `\e` escape.** `'\e'` is the GNU escape for 27 (evidence
`p31-residue.c`, `esc`). Measured defect at revision 1.9.52: `E1006`.

### 10.4 EvoCPP Target Profile

EvoCPP presents itself to headers as the GNU C dialect of the layout oracle,
GCC 14.2 for `aarch64-unknown-linux-gnu`, so that every `#if` a header takes
is the branch the oracle takes. Nothing here changes what the front end
accepts; it changes only which lines reach the front end.

**Predefined macros.** The predefined set is the sealed list
`.evidence/predefined-macros/20260826-2c23684c/gcc-14-aarch64-linux.h`
(sha256 `74801974f845be4d`, 421 macros, captured with
`gcc-14 -std=gnu11 -dM -E - </dev/null` in image `evo-flight-lab:sealed`),
defined verbatim before any source token, in that order. `__STDC_VERSION__`
is `201112L` as in the list. `__OPTIMIZE__` is absent, as in the list. The
driver's `-D`/`-U` apply after the list. The list is the authority; the
implementation carries a copy and a test compares the copy to the sealed
digest. `__CHAR_UNSIGNED__ 1` is in the list and agrees with the type profile
(plain `char` is unsigned on this target).

**Dynamic macros.** `__FILE__`, `__LINE__`, `__BASE_FILE__`, `__DATE__`,
`__TIME__` as ISO C; `__COUNTER__` expands to `0`, `1`, `2`, ... per expansion
in translation order (measured `0 1 2`); a translation unit that expands it
2^32 times is `E0109` (`counter-overflow`).

**Token pasting.** The operands of `##` are concatenated and the result is
re-lexed as one *preprocessing* token (C11 6.4, with pp-numbers per 6.4.8): an identifier, a
punctuator, a string or character literal, or a pp-number, which is a digit
followed by any sequence of digits, identifier characters, `.`, and `e`/`E`/`p`/`P`
followed by a sign, and may begin with `.` followed by a digit (`.5foo` and
`1.2.3e+x` are each one pp-number). `1_MODULE`, `1x` and `1.5` are therefore valid results
(measured, both oracles: `CAT(1,_MODULE)` gives `1_MODULE`; the kernel's
`IS_ENABLED(CONFIG_X)` expands `IS_MODULE(1)` to `1##_MODULE` and relies on
it). A result that is not one preprocessing token is `E0110`
(`invalid-token-paste`) at the `##` operator (both oracles reject
`CAT(1,+)`). A header-name is never a paste result, and two non-empty operands
never form an *other* token (both oracles reject `C(@,@)` and `C(\\,x)`); `$`
is an identifier character in this dialect, so a lone `$` is an identifier,
not a stray character. A pp-number that is not a valid integer constant is an error
only where it is consumed: `E0111` (`conditional-operand-not-integer`) in a
`#if`, or the C lexer's diagnostic in the translation unit. GNU comma elision
before an empty `__VA_ARGS__` is honoured.

**Other characters.** Any non-white-space character that does not begin another
preprocessing token (`\\`, `@`, `` ` ``, `$` outside identifiers) is a single
preprocessing token of its own (C11 6.4 *each non-white-space character that
cannot be one of the above*). It passes through macro expansion and is kept by
`#` stringification (measured, both oracles: `S(\\sreg)` is `"\\sreg"`, which
the kernel's `sysreg.h` relies on to build assembler text). It is an error only
when it reaches the C lexer as a token: `E0112` (`stray-token`) at that token,
the same code every preprocessing token that has no C token form reports at
conversion (a pp-number that is not a C constant included). Measured defect at
revision 1.9.11: `\\` inside a macro body was `E0001` at the `#define`, which
stopped all 14 raw kernel lanes at `sysreg.h:1163`.

Measured defect at revision
1.9.10: the re-lexer rejected `1_MODULE`, which stopped all 14 raw kernel
lanes at the `1` of `#define CONFIG_ARM64_PAN 1` (440 diagnostics per unit).

**`#if` arithmetic.** Every pp-number in a controlling expression, including
one produced by `##` or by macro expansion, is re-lexed as an integer constant:
any of the suffixes `u`, `l`, `ul`, `ll`, `ull` in either case, decimal, octal
and hexadecimal forms. Evaluation is in `intmax_t`/`uintmax_t` with the usual
arithmetic conversions; an identifier that is not a macro is `0`; `defined`
and the feature queries are evaluated before expansion. Measured defect at
revision 1.9.7: suffixed literals were right (`#if 1UL == 1` true) but pasted
pp-numbers were not (`#if CAT(300,) > 100` and `#if CAT(0x1,00) > 100` false),
which is what made `ARM64_NCAPS >= BIT(15)` true in every raw kernel lane.

**Feature queries.** All four are defined as macros, so `#ifdef __has_attribute`
is true (measured for `__has_attribute`, `__has_builtin`, `__has_feature`,
`__has_include`). Their arguments are one token or one header name; anything
else is `E0106` (`feature-query-malformed`).

- `__has_attribute(name)`: `name` with a surrounding `__` pair stripped. The
  answer is the measured GCC-14 answer for the names the kernel cohort and
  headers use (`.evidence/predefined-macros/20260826-2c23684c/has.out`): `202311` for `fallthrough` and `noreturn`; `1`
  for `aligned`, `alloc_size`, `always_inline`, `assume_aligned`, `cleanup`, `cold`, `const`, `copy`, `designated_init`, `error`, `externally_visible`, `format`, `gnu_inline`, `malloc`, `no_instrument_function`, `no_profile_instrument_function`, `no_sanitize_coverage`, `no_stack_protector`, `noclone`, `noinline`, `noipa`, `nonnull`, `nonstring`, `packed`, `pure`, `retain`, `section`, `uninitialized`, `unused`, `used`, `warn_unused_result`, `warning`, `weak`; `0` for `btf_type_tag`, `diagnose_as_builtin`, `disable_sanitizer_instrumentation`, `no_caller_saved_registers`, `overloadable`, `pass_dynamic_object_size`, `pass_object_size`, `preserve_most`;
  `0` for any name not in that table. The table is data, not code: the
  implementation reads it from the sealed file's copy and a test checks the
  digest.
- `__has_builtin(name)`: `1` when the front end resolves `name` (10.1.14,
  10.1.15, `__builtin_va_list` and the names 10.1.17 will add) or the measured
  table says `1` (`__builtin_dynamic_object_size`, `__builtin_has_attribute`,
  `__builtin_isdigit`); `0` otherwise (`__builtin_alloca_uninitialized`).
- `__has_feature(name)`: `0` for every name (measured: GCC-14 answers `0` for
  all seven sanitizer and shadow-call-stack names the kernel asks about).
- `__has_include(<h>)` and `__has_include("h")`: `1` when the include search
  the directive would perform finds the file, else `0` (measured).

**Pragmas.** `_Pragma("text")` destringizes and is processed as `#pragma text`.
`#pragma once` is honoured. `#pragma GCC diagnostic ...`, `#pragma GCC
visibility ...`, `#pragma GCC poison`, `#pragma pack(...)` are accepted:
`diagnostic` and `visibility` are ignored-safe, `poison` is honoured, and
`pack(N)` sets the maximum member alignment of every record defined while it
is in effect to `N` (a power of two); `pack()` restores the default;
`pack(push[, N])` and `pack(pop)` maintain a stack. Measured (both oracles):
`pack(1)` on `{char; int}` gives size 5 alignment 1, `pack(2)` on
`{char; int; long}` gives 14/2, `pack()` restores 8/4. The preprocessor turns
the directive into a layout marker the parser applies as a record-level
`max_align`: member alignment is `min(natural, N)`, and a member `aligned(M)`
with `M > N` does **not** raise it while the pack is active (measured, both
oracles, all three attribute placements: `pack(1)` with `int
__attribute__((aligned(4)))` gives size 5 alignment 1;
`.evidence/builtin-probes/20260826-2c23684c/pack-stack.out`, sha256
`58033385db011629e6ebf9619876bb814d1fa6c563f85f66b0add88a46352ecf`; the
push/pop stack there measures `5/1, 12/4, 5/1, 8/4`). `pack` directives are
honoured at file scope and at block scope (measured: block-scope `pack(1)` gives
5/1, then 8/4 after `pack()`, both oracles); a `pack` directive inside a
record's member list is `E0107` (`ORACLE_CONFLICT`: GCC-14 applies it to the
following members, 5/1, Clang-19 ignores it, 8/4; evidence
`pack-gaps.out`). `pack(pop)` on an empty stack is ignored (both oracles warn
and continue); the receipt counts it. `pack(show)` and any other
argument form are `E0107` (`pragma-pack-unsupported`). A poisoned identifier
used later is `E0108` (`poisoned-identifier-used`) at the use.
Any other pragma is ignored-safe; the acceptance receipt lists the ignored
pragma names it met.

**Whitespace flag and stringization.** Every pp-token carries one flag:
preceded by white space. A token spelled in a file has the flag when white
space, a comment, or a line break inside a macro argument separates it from
the previous token. A token produced by macro expansion keeps the flag its
spelling had in the replacement list or argument, except that the first token
of an expansion takes the flag of the macro name that began the invocation;
when an expansion produces no tokens, the token that follows the invocation
takes the flag if the macro name had it. Stringization (`#`) writes the
spellings of the argument's tokens with exactly one space before each token
whose flag is set, dropping leading and trailing white space. Measured
(evidence `p15.out`, both oracles): with `A` = `a`, `B` = `b`, `E` empty and
`F(x)` = `x`, `__stringify(A/B)` is `"a/b"`, `__stringify(A / B)` is
`"a / b"`, `__stringify(A E B)` is `"a b"`, `__stringify(A E/B)` is `"a /b"`,
`__stringify(F(A)B)` is `"ab"`, `__stringify(F( A )/B)` is `"a/b"`, and the
kernel's `define_trace.h` computed include
`__stringify(TRACE_INCLUDE_PATH/system.h)` is `"./trace_arm.h"`. The same
flag decides the spelling of `-E` output; that output is a convenience, not
an acceptance surface, because the parser and the survey consume the token
stream. Measured defect at revision 1.9.31: a space was written before every
expanded token, so the computed include named `. / trace_arm .h` and 56
units of the full configuration failed with `E0105`.

**The `_Pragma` operator.** `_Pragma ( string-literal )` is processed wherever
it stands after macro expansion, including when the string literal was
produced by stringization inside an expansion: the literal is destringized
(C11 6.10.9) and handled exactly as a `#pragma` directive at that point,
under the pragma rules of this section (`pack` applied, others ignored-safe
and counted). The operand must be exactly one string literal after
expansion; anything else is `E0112` (`stray-token`). A `;` that follows the
operator at file scope is an empty declaration and is accepted (both oracles
compile it; evidence `p30-pragma-operator.out`, whose `F` line is a malformed
probe of the architect's and is not evidence). Measured defect at revision
1.9.47: `_Pragma` with an expanded operand was left in the token stream and
`__bpf_kfunc_start_defs()` failed with `E1001` in 18 units.

**Whitespace of a substituted argument.** When an argument is substituted
for a parameter in a replacement list, the first token of the argument takes
the whitespace flag of that occurrence of the parameter, and the remaining
tokens keep the flags of their spelling (evidence `p30-pragma-operator.out`:
with `STR(GCC diagnostic s)`, `D(push)` is `"GCC diagnostic push"`, `E(x)`
with `STR(a s b)` is `"a x b"`, and `G(name)` with `STR(s.h)` is `"name.h"`).
Measured defect at revision 1.9.47: the flag was dropped and the pragma
text read `GCC diagnosticpush`.

**One conditional stack.** Conditional directives form one stack whether they
are met at directive level or while the arguments of a function-like macro
are being collected: a branch opened before an invocation may close inside
its arguments and the reverse, and the tokens of the branch not taken never
reach the argument collector (evidence `p31-residue.c`, `cond`, the kernel's
`#ifdef ... pr_warn( #else pr_err( #endif` shape, both oracles). Measured
defect at revision 1.9.52: `E0102` in two units.

**Empty operands of `##`.** When either operand of `##` is empty, the result
is the other operand unchanged, whether that operand is a parameter or a
token of the replacement list (`mod##SNMP_INC` with `mod` empty is
`SNMP_INC`; evidence `p31-residue.c`, `paste`). Measured defect at revision
1.9.52: `E0110` when the right operand was a literal token.

**Comments across continued definition lines.** A block comment inside a
`#define` whose lines continue with `\` is white space, including when the
comment itself spans continued lines (evidence `p31-residue.c`, `RES_MAXSZ`
is 20). Measured defect at revision 1.9.52: `E1001` in `callback_xdr.c`.

**`__VA_OPT__`.** `__VA_OPT__ ( tokens )` in a variadic replacement list
expands to `tokens` when the variable arguments contain at least one token
and to nothing otherwise (C23 6.10.4.1; both oracles accept it in gnu11;
evidence `p31-residue.c`, `vaopt` is 4).

**Parameter names.** A macro parameter is any identifier, keywords included
(`register` in the kernel's `read_roothub`); the preprocessor knows no
keywords (evidence `p31-residue.c`, `kw`). Measured defect at revision
1.9.52: `E1003` in three units.

**Wide strings under the profile.** The kernel's command line carries
`-fshort-wchar`: `wchar_t` is `unsigned short`, `L"..."` has type
`unsigned short[N]` counting a terminating zero, and a wide literal
concatenated with a narrow one is wide (`L"" "abc"` is `unsigned short[4]`;
evidence `p31-residue.c`, both 8 bytes). Measured defect at revision
1.9.52: `E2205` in two units.

**Rescanning.** After a macro's replacement list has its arguments
substituted, it is rescanned for further macro names together with the
tokens that follow it in the sequence it was expanded in: the rest of the
source file, the rest of the enclosing replacement list, or the rest of the
enclosing argument. A function-like macro name that ends an expansion is
invoked when the next token of that outer sequence is `(`, across line
breaks. The name of the macro being replaced is not expanded during its own
rescan (C11 6.10.3.4). Measured (evidence `p16.out`, both oracles): with
`smp()` = `Z` and `G` = `smp`, `G()` is `Z`; `G` followed by `()` on the next
line is `Z`; `H(1)()` with `H(x)` = `x smp` is `1 Z`; `K(G)()` with `K(x)` =
`x` is `Z`. The kernel relies on this in `linux/atomic.h`, where
`__atomic_acquire_fence` is defined as `smp_mb__after_atomic` and invoked as
`__atomic_acquire_fence()`. Measured defect at revision 1.9.31: rescanning
stopped at the end of the replacement list, leaving `smp_mb__after_atomic()`
unexpanded in every unit that includes `linux/atomic.h`; the parse then
reports `E2001`.

**Directives inside macro arguments.** A conditional directive (`#if`,
`#ifdef`, `#ifndef`, `#elif`, `#else`, `#endif`) met while the arguments of a
function-like macro are being collected is processed as a directive, and the
tokens of the branch taken join the argument; `#define` and `#undef` there are
processed as well (evidence `p19-p20.out`, both oracles: `p20`, `p20b`).
`#pragma` inside an argument is `E0113` (`directive-in-macro-argument`):
`ORACLE_CONFLICT`, GCC-14 processes it ahead of the expansion, Clang-19
refuses (`p20c`). Any other directive there is `E0113` too. Measured defect
at revision 1.9.32: the directive lines were emitted as ordinary tokens
(`#ifdef 1`), so `struct_group` in `skbuff.h`, whose member list carries
`#ifdef CONFIG_NET_CLS_ACT`, failed with `E1001` in 562 units of the full
configuration.

**Empty arguments and variadic forwarding.** An argument may be empty. A
named variadic parameter (`args...`) and `__VA_ARGS__` denote the exact
sequence of the remaining arguments, empty ones and their separating commas
included; when that sequence is substituted into a nested invocation and
rescanned, the nested invocation's arguments are collected from the
substituted tokens exactly as spelled. Measured (evidence `p18.out`, both
oracles): `E(,)` is `[|]`, `V(,2)` is `[|2]`, `N()` is `[|]`, and
`__MAP(3,__SC_ARGS,,r0,,r1,,r2)` through the kernel's `__MAP` chain yields
`, r0 , r1 , r2`. Measured defect at revision 1.9.32: the chain dropped every
argument after the first empty one (`h(,regs->regs[0],,,,)`), so
`SYSCALL_DEFINEn` failed with `E1003` in about 50 units.

Diagnostics of this section are front-end (preprocessor) diagnostics: `E0102`
`unmatched-conditional`, `E0104` `error-directive`, `E0105`
`include-not-found`, `E0106` `feature-query-malformed`, `E0107`
`pragma-pack-unsupported`, `E0108` `poisoned-identifier-used`, `E0109`
`counter-overflow`, `E0110` `invalid-token-paste`, `E0111`
`conditional-operand-not-integer`, `E0112` `stray-token`, `E0113`
`directive-in-macro-argument`.

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
                   ; budget_line is absent in evp1 and present in evp2, never otherwise
version_line    := "evp1" newline | "evp2" newline
budget_line     := "budget nodes=" quantity " epochs=" quantity " fuel=" quantity
                   " n=" evp_int " stop=" stop_reason newline
quantity        := evp_int "/" evp_int
                   ; consumed "/" bound, in that order, never the reverse
stop_reason     := "saturated" | "no-new-best" | "policy" | "epochs" | "nodes" | "fuel"
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
                 | "shl" | "lshr" | "ashr" | "zext" | "sext" | "trunc"
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
representation and relying on extraction to select it. A derived node is used
when a rewrite names it or when it is a child, at any depth, of a derived
node a rewrite names: the `const_like` operand that an emitting right-hand
side such as `shl(?x,const_like(?x,1))` writes is used through the `shl`
that names it (child at any depth is decided at class granularity, since the node stream records children as classes: a derived node is used when it belongs to the canonical class of a child, transitively, of a named derived node; a node event always allocates a fresh class, so membership is reached only through a recorded merge), and the checker recomputes its payload from the pattern
exactly as for a named constant (revision 1.9.52, after the first emitting
rule stopped on the unqualified sentence).

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

The checker does not judge cost (revision 1.9.51; before it the checker
recomputed the default cost and refused a region that was not a strict
improvement under it). It verifies that every replacement is equivalent by
the recorded events, that the extracted root is not the original root node
itself (a region must change something), and every structural rule of this
section. Whether the replacement is an improvement is decided by the
producer's policy (7.5 "Cost is a policy") and is producer conformance the
checker cannot observe, like the ceiling-stop rule of 8.11. The default
policy's cost, for the record: a seed, a `const` node, or a non-root
before-witness reuse costs zero; the replacement root's non-constant before
node and every non-constant derived node cost one plus the sum of selected
child costs, repeated child positions counted repeatedly; constants cost
zero so that `icmp.eq x, x` and its `true` replacement do not tie. Measured
at revision 1.9.50: under the P0 registries every replacement is an alias or
a constant, so two all-positive cost functions optimize 0 of 1,000 programs
differently, and cost discriminates only by assigning zero; the fourth
question's search space opens only when a right-hand side may emit an
instruction, which the registry below admits.
A producer admits a region block only when the extracted root has strictly lower cost than the original witness root under its own policy, ties retaining the original; that is producer conformance, not a checker rule (revision 1.9.55 restated the sentence that had still read as one).

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
implementation of the Section 8.10 transfers and refuses the rewrite unless
the recomputed fact is at or below the recorded premise in the order of 8.10,
that is, unless every bit the premise claims is known to the checker with the
same value; a premise claiming a bit the checker cannot confirm is refused
(status 3). Revision 1.9.40 corrected the direction, which had been stated so
that the strongest premise always passed. Facts are recomputed
rather than trusted for the reason Section 11.6 gives: a shared analysis that
was wrong would make producer and checker agree on an unsound rewrite.

**Epoch markers.** The scheduler runs per transaction and a certificate is
organised per region, so markers are written per region: within a region block,
an `epoch k` line precedes the events that epoch produced for that region, and
`k` is strictly increasing within the block. A region touched in no epoch has no
markers. The checker verifies monotonicity within each block and that no `k`
exceeds the recorded epoch count; it does not require every epoch to appear in
every region, since most epochs touch few regions. Epochs are numbered from 1: with `epochs=c/1024` the legal markers are `1..=c`, so the inequality `k <= c` is tight, and a marker `0` is malformed (status 2). Defined at revision 1.9.37. A congruence that holds as its two nodes are inserted was produced by no epoch and precedes every marker, like the node inserts; a congruence a merge enabled follows the marker of the epoch whose merge enabled it. A rewrite whose union would join a class with itself, because an earlier merge already placed the root where the rewrite would, is not recorded; the replacement it would have justified stands on the recorded merges alone, which the checker verifies as it verifies any replacement. Defined at revision 1.9.42 after congruence closure was measured: regions rose 14.7 percent (7,900 to 9,062 in the 1000-program gate) and 197 of them are justified by a congruence with no rewrite of their own.

**Budget.** Every `quantity` is written consumed `/` bound, in that order:
`budget nodes=3/100000 epochs=1/1024 fuel=0/256 n=0 stop=saturated` is a
transaction that inserted three nodes into a graph bounded at one hundred
thousand, ran one epoch of a possible 1024, and stopped because nothing new
could be proposed. The `nodes` and `epochs` bounds are the profile's
constants (8.11), and the checker verifies that the recorded bounds equal
them. `fuel` is the per-rule, per-epoch quantity of 8.11: `n` is the number
of nodes the region held when the transaction began, the fuel bound is
`max(256, 8 * n)`, and the fuel consumed is the largest number of firings any
single rule made in any single epoch. The checker verifies that each consumed
does not exceed its bound, that the fuel bound equals `max(256, 8 * n)` for
the recorded `n`, that no epoch marker exceeds the consumed epoch count, and
that a stop reason of `epochs`, `nodes` or `fuel` coincides with that
quantity's consumed equalling its bound. `n` itself is declared, not
re-derived: a producer that overstates it only widens a box the checker
still holds it to. `saturated` and `no-new-best` are not verified; they would
require re-running the scheduler and are telemetry. Defined at revision
1.9.36 after the randomized gate stopped on the two readings of an unordered
`a/b` and of a single fuel integer with no bound. A producer that stops by a rule of its own, neither at a profile ceiling nor at saturation, records `stop=policy`; it is telemetry like `saturated` and `no-new-best`. `saturated` has exactly the meaning of 8.11: no rule fired in the transaction's last epoch. It says nothing about a fresh transaction over the output, which may admit regions because 11.6 lets a left-hand `const_like` match only a constant present in the before witness closure, never one an earlier epoch derived; such *residual regions* are a measured quality metric of the registry, not a false record (revision 1.9.37 conflated the two; corrected at 1.9.38 after the executor produced the distinguishing input, seed 1592591027). For a certificate covering several regions, `n` is the largest starting node count among them. With P0 regions the fuel bound's floor of 256 is not reachable, so `stop=fuel` is reserved for a profile with larger regions; the grammar keeps it. Hitting any ceiling keeps every region admitted before the stop (8.11): a producer that answers its own budget exhaustion with a zero-region certificate is non-conforming, because it discards work the checker would have verified. An `evp2` header without a budget line, or an `evp1` header with one or with epoch markers, is malformed (status 2).

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

**A certificate with no region.** A certificate that admits no region block
claims that `after.evo` is the canonical rendering (Section 9.4, consecutive
identifiers in Section 8.6 order) of the module parsed from `before.evo`, and
nothing else. The checker verifies it by rendering the parsed `before.evo`
itself and comparing byte for byte; it does not compare against the raw input
bytes, which need not be canonical (identifiers in a hand-written or reduced
module may have gaps). A producer that admits no region emits that canonical
rendering. Measured defect at revision 1.9.34: the checker compared against
the raw bytes and refused every gapped input the producer had merely
renumbered.

**Bindings are recomputed from the recorded children.** The substitution a
rewrite line records must equal the one the checker derives from the
operation and children of the node the rewrite names, after the merges
recorded before it; a producer therefore computes each binding from the
node stream it emitted, never from an operand remapped by a later
replacement in the same witness closure. Measured defect at revision 1.9.34:
seven of eight reduced refusals from the randomized gate (rules
`p0_010_icmp_eq_self`, `p0_100_fold_add`, `p0_103_fold_and`,
`p0_104_fold_or`, `p0_109_fold_icmp_sle`) bound a class equal in content to
the child's class but never unified with it; the checker's refusal was
correct.

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

**Reviewed registry `p0.scalar.v2`.** This registry is normative for `evp2`
certificates and includes every rule of `p0.scalar.v1` unchanged, followed by
the Tier B rules. Its digest is `c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce`, the SHA-256 of the
bytes of the following code block starting at the first rule id and including
the newline after the last rule:

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
p1_200_known_and and(?a,?b) => known_const when fully_known
p1_201_known_or or(?a,?b) => known_const when fully_known
p1_202_known_xor xor(?a,?b) => known_const when fully_known
p1_203_known_add add(?a,?b) => known_const when fully_known
p1_204_known_sub sub(?a,?b) => known_const when fully_known
p1_205_known_mul mul(?a,?b) => known_const when fully_known
p1_210_known_icmp_eq icmp.eq(?a,?b) => known_const when fully_known
p1_211_known_icmp_ne icmp.ne(?a,?b) => known_const when fully_known
p1_212_known_icmp_slt icmp.slt(?a,?b) => known_const when fully_known
p1_213_known_icmp_sle icmp.sle(?a,?b) => known_const when fully_known
p1_214_known_icmp_ult icmp.ult(?a,?b) => known_const when fully_known
p1_215_known_icmp_ule icmp.ule(?a,?b) => known_const when fully_known
p1_220_and_cover_rhs and(?x,?m) => ?x when covers(?m,?x)
p1_221_and_cover_lhs and(?m,?x) => ?x when covers(?m,?x)
p1_222_or_cover_rhs or(?x,?m) => ?m when covers(?m,?x)
p1_223_or_cover_lhs or(?m,?x) => ?m when covers(?m,?x)
p1_224_xor_known_zero_rhs xor(?x,?z) => ?x when known_zero(?z)
p1_225_xor_known_zero_lhs xor(?z,?x) => ?x when known_zero(?z)
```

**Reviewed registry `p0.scalar.v3`.** This registry is normative for `evp2`
certificates and includes every rule of `p0.scalar.v2` unchanged, followed by
the fact-driven folds for the shifts and width changes admitted at revision
1.9.44. Its digest is `aca1f15b47e602ce18c593e06d77008862947c7c7252a9f76bb823c71f53fb15`, computed as for the earlier registries:

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
p1_200_known_and and(?a,?b) => known_const when fully_known
p1_201_known_or or(?a,?b) => known_const when fully_known
p1_202_known_xor xor(?a,?b) => known_const when fully_known
p1_203_known_add add(?a,?b) => known_const when fully_known
p1_204_known_sub sub(?a,?b) => known_const when fully_known
p1_205_known_mul mul(?a,?b) => known_const when fully_known
p1_210_known_icmp_eq icmp.eq(?a,?b) => known_const when fully_known
p1_211_known_icmp_ne icmp.ne(?a,?b) => known_const when fully_known
p1_212_known_icmp_slt icmp.slt(?a,?b) => known_const when fully_known
p1_213_known_icmp_sle icmp.sle(?a,?b) => known_const when fully_known
p1_214_known_icmp_ult icmp.ult(?a,?b) => known_const when fully_known
p1_215_known_icmp_ule icmp.ule(?a,?b) => known_const when fully_known
p1_220_and_cover_rhs and(?x,?m) => ?x when covers(?m,?x)
p1_221_and_cover_lhs and(?m,?x) => ?x when covers(?m,?x)
p1_222_or_cover_rhs or(?x,?m) => ?m when covers(?m,?x)
p1_223_or_cover_lhs or(?m,?x) => ?m when covers(?m,?x)
p1_224_xor_known_zero_rhs xor(?x,?z) => ?x when known_zero(?z)
p1_225_xor_known_zero_lhs xor(?z,?x) => ?x when known_zero(?z)
p1_206_known_shl shl(?a,?k) => known_const when fully_known
p1_207_known_lshr lshr(?a,?k) => known_const when fully_known
p1_208_known_ashr ashr(?a,?k) => known_const when fully_known
p1_230_known_zext zext(?a) => known_const when fully_known
p1_231_known_sext sext(?a) => known_const when fully_known
p1_232_known_trunc trunc(?a) => known_const when fully_known
```

**Reviewed registry `p0.scalar.v4`.** This registry is normative for `evp2`
certificates and includes every rule of `p0.scalar.v3` unchanged, followed by
the first emitting rules: the three spellings of doubling are one class, and
which of them survives is the policy's choice. Its digest is `1a5391603386e18e18bfb1b2fc61985c9eb252c6e65bcb3259104bd49b2a9d74`, computed
as for the earlier registries:

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
p1_200_known_and and(?a,?b) => known_const when fully_known
p1_201_known_or or(?a,?b) => known_const when fully_known
p1_202_known_xor xor(?a,?b) => known_const when fully_known
p1_203_known_add add(?a,?b) => known_const when fully_known
p1_204_known_sub sub(?a,?b) => known_const when fully_known
p1_205_known_mul mul(?a,?b) => known_const when fully_known
p1_210_known_icmp_eq icmp.eq(?a,?b) => known_const when fully_known
p1_211_known_icmp_ne icmp.ne(?a,?b) => known_const when fully_known
p1_212_known_icmp_slt icmp.slt(?a,?b) => known_const when fully_known
p1_213_known_icmp_sle icmp.sle(?a,?b) => known_const when fully_known
p1_214_known_icmp_ult icmp.ult(?a,?b) => known_const when fully_known
p1_215_known_icmp_ule icmp.ule(?a,?b) => known_const when fully_known
p1_220_and_cover_rhs and(?x,?m) => ?x when covers(?m,?x)
p1_221_and_cover_lhs and(?m,?x) => ?x when covers(?m,?x)
p1_222_or_cover_rhs or(?x,?m) => ?m when covers(?m,?x)
p1_223_or_cover_lhs or(?m,?x) => ?m when covers(?m,?x)
p1_224_xor_known_zero_rhs xor(?x,?z) => ?x when known_zero(?z)
p1_225_xor_known_zero_lhs xor(?z,?x) => ?x when known_zero(?z)
p1_206_known_shl shl(?a,?k) => known_const when fully_known
p1_207_known_lshr lshr(?a,?k) => known_const when fully_known
p1_208_known_ashr ashr(?a,?k) => known_const when fully_known
p1_230_known_zext zext(?a) => known_const when fully_known
p1_231_known_sext sext(?a) => known_const when fully_known
p1_232_known_trunc trunc(?a) => known_const when fully_known
p2_300_mul_two_rhs mul(?x,const_like(?x,2)) => add(?x,?x) when width_min(?x,2)
p2_301_mul_two_lhs mul(const_like(?x,2),?x) => add(?x,?x) when width_min(?x,2)
p2_302_add_self_shl add(?x,?x) => shl(?x,const_like(?x,1)) when width_min(?x,2)
p2_303_shl_one_add shl(?x,const_like(?x,1)) => add(?x,?x) when width_min(?x,2)
```

**Registries and revisions.** A registry is admissible only when this
specification prints its rule block and digest; a producer and a checker may
not agree on a registry between themselves, because a certificate checked
against rules it supplied establishes only that its merges follow from its own
claims. A revision includes every rule of the registry it revises, unchanged
and in the same order, before its new rules, so a rule id keeps its meaning
across revisions. `evp1` accepts `p0.scalar.v1` only; `evp2` accepts
`p0.scalar.v1` through `p0.scalar.v4`. Defined at revision 1.9.40 after the
randomized gate stopped for want of a procedure.

**Which registry a certificate names, and rule order within a tier.** A
certificate names a reviewed registry that contains every rule it fires; the
producer names the narrowest such registry, so a certificate that fired no
`p1_` rule names `p0.scalar.v1` and stays readable by a checker that admits
only it. Within a tier the producer applies the first applicable rule in
registry order, so a premise-free fold on a node precedes a fact-driven rule
on the same node. When both operands of `xor`, `add` or `sub` are
constant nodes, `all_const` holds and the fold `p0_105`, `p0_100` or `p0_101`
precedes `p1_202`, `p1_203` or `p1_204` in registry order; the fact rules fire
only when the operands are fully known without being constant nodes, which
the facts of seeds (8.10, revision 1.9.46) make possible for a block argument.
Before revision 1.9.46 that could not happen and the three were dominated;
revision 1.9.47 lifted the sentence that forbade firing them, and a coverage
claim counts them as reached when such an input exists. Premise classes are resolved through the
partition at the rewrite that consumes them, like binding classes. A premise
on a rule whose side condition reads no class is invalid (status 3), and so is
a premise whose digit count disagrees with its class's width. Measured at
revision 1.9.40: with the fact-driven rules the residual-region count of 8.11
fell from 20 in 21,000 programs to 0, because the facts state in one
transaction what the before-witness rule of `const_like` had left for the
next. Defined at revision 1.9.41.

**Tier B side conditions and builders.** `fully_known` holds when the fact of
the class of the matched left-hand node, recomputed under 8.10, has every bit
known; `known_const` then constructs a `const` of that node's sort whose raw
value is the fact's value. `covers(?m,?x)` holds when every bit that is not
known zero in the fact of `?x` is known one in the fact of `?m`.
`known_zero(?z)` holds when the fact of `?z` has every bit known and every
value bit zero. A Tier B rewrite records as premises, in this order, the fact
of every class its side condition read: the matched node's class for
`fully_known`, `?m` then `?x` for `covers`, `?z` for `known_zero`. Every Tier B
rewrite requires `evp2`, and `known_const` is a right-hand builder and
therefore requires a `node:` proof reference.

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

**Emitting right-hand sides.** A right-hand side may be an operator applied to
bound variables and `const_like` terms, such as `add(?x,?x)` or
`shl(?x,const_like(?x,1))`. The producer emits it as a `source=derived` node
whose operator, sort and children the checker reconstructs from the pattern
and the recorded bindings, in a class of its own that the rewrite then unions
with the left-hand node's class; it requires a `node:` proof reference like
every builder, and its fact under 8.10 is computed like any node's. The side
condition `width_min(?x,K)` holds when the sort of `?x` is `bits<N>` with
`N >= K`; it records no premise. Whether the emitted node is extracted is the
policy's decision (7.5). Defined at revision 1.9.51.

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
| | `ctlz.bits<N>`, `cttz.bits<N>`, `ctpop.bits<N>`, `bswap.bits<N>` | `bits<N>` | `%a` : `bits<N>` | no |
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
| | `trap.<reason>` | — | reason (`scalar_function_fell_off_end`, `unreachable_reached`) | yes |
| Assembly | `asm [volatile]` | register outputs, `mem<all>`, `world` | template, constrained operands, clobbers, input tokens | yes; interpreter E3021 |
| | `asm.goto [volatile]` | register outputs and tokens, bound on successor edges | same payload, named label edges, fallthrough edge | yes; interpreter E3021 |
| Pointer | `func_addr @symbol` | `fnptr<Sig>` | symbol | no |
| | `ptr.to_int %p` | `bits<64>` | pointer | no |
| | `block_addr ^b : ptr<generic>` | `ptr<generic>` | block label | on dereference (`E3013`) |
| | `return_address : ptr<generic>`, `frame_address : ptr<generic>` | `ptr<generic>` | none | on dereference (`E3013`) |
| | `ptr.from_int.ptr<AS> %i` | `ptr<AS>` | `bits<64>` | on dereference (`E3013`) |
| Calls | `call @callee(args..., %m, %w)` | ret values, `mem`, `world` | callee, args, memory, world | depends |
| | `call %fptr(args..., %m, %w)` | ret values, `mem`, `world` | callee value, args, memory, world | depends |
| Varargs | `va_start %ap, %m, %w` | `mem<all>`, `world` | list object, memory, world | E3037 at validation |
| | `va_arg.Type %ap, %m, %w` | Type, `mem<all>`, `world` | list object, memory, world | yes; E3038 |
| | `va_end %ap, %m, %w` | `mem<all>`, `world` | list object, memory, world | yes; E3038 |
| UB | `ub.assume.<reason> %cond` | — | `%cond` : `bits<1>` | yes |
| Poison | `uninitialized.bits<N>` | `bits<N>` | — | on consumption |
| | `uninitialized.ptr<AS>`, `uninitialized.fnptr<Sig>` | that type | — | on dereference (`E3013`) |
| `copy` | `(%m1, %w1) = copy %m0, %w0, %dst, %src [size=N, align=A]` | copy N bytes with initializedness; identical or disjoint ranges |

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
| 1.9.76 | 2026-09-07 | Admit source input `Ir` under the user's ruling, selecting its existing `r` alternative before EvoIR construction. C-HIR retains the spelling; executable validation must cover the complete source-to-machine path. This specification change alone changes no production code. |
| 1.9.75 | 2026-09-07 | The out-of-line body of a `static always_inline` function is not lowered when every call was substituted and no address is taken; otherwise it is lowered as before. The executor found the retained body reporting `E3021` before the machine could drop it. |
| 1.9.74 | 2026-09-07 | `always_inline` definitions are substituted at their calls in C-HIR (parameters as fresh objects, returns as a result object, statics shared, callee-first, cycles `E2932`), and constants are folded again afterwards so immediate `asm` operands fed through a parameter become constant. The tenth kernel survey put that construct first in 2,727 units. |
| 1.9.73 | 2026-09-06 | Record parameters and the record return destination carry `byval(size, align)` and `sret(size, align)` on their pointer, so the machine can apply the platform's composite convention; found by the zstd rung, where an 8-byte record crossed to GCC-built code through a hidden pointer. |
| 1.9.72 | 2026-09-06 | `__builtin_return_address(0)` and `__builtin_frame_address(0)` lower to `return_address` and `frame_address`, code- and stack-provenance values the interpreters refresh per activation; the kernel's `_RET_IP_` became the face of 2,714 units once label addresses lowered. Levels above zero stay refused. |
| 1.9.71 | 2026-09-06 | The reachable end of a non-void function returns poison instead of trapping: C leaves the fall-off undefined only when the value is used, and the first GCC torture run tripped `E3007` in 28 programs that never use it. |
| 1.9.70 | 2026-09-06 | Functions carry linkage: `visibility=internal` for `static` definitions, omitted otherwise; the machine emits no global symbol for internal functions and drops unreferenced ones. The kernel lane found EvoCC objects colliding on `static inline` helpers from shared headers, which capped the mixed link at 14 units. |
| 1.9.69 | 2026-09-06 | A relocation may target a block of a function of the module, written `@function^block`, so a static `&&label` initializer has a spelling (the backend executor found 2.1's target set closed to blocks after 0.15 had promised the emitter half). |
| 1.9.68 | 2026-09-06 | Rules the ninth survey's face (30 clean): a used label address is `block_addr` with code provenance (`local_bh_disable` in 1,040 units; `goto *` alone stays `E3029`), and ordered pointer comparisons compare the addresses (`E3045` withdrawn, 19 units). The `i` constraint fed by a parameter now stands at 35 units through `alternative_has_cap_likely` and stays the allowed residue until an inliner exists. |
| 1.9.67 | 2026-09-06 | Rules the eighth survey's face (19 clean; `E3030`, `E3033`, `E3020`, `E3023`, `E3026`, `E3036`, `E3101` all at zero): a global register variable is read and written through an implicit `asm` (`E3035` withdrawn, 2,711 units); checked multiplication of two unsigned operands computes unsigned (`E3042` withdrawn, 48); pointer difference is integer subtraction and exact division (`E3032` withdrawn, 8); object and function pointers convert through `ptr.to_int`/`ptr.from_int` with a `fnptr` form (`E3043` withdrawn, 7). Accepts the Appendix D rulings on compound storage, block linkage and parameter qualifiers as written. Names the two allowed residues of the finish line: an `i` assembly constraint whose operand is a parameter (`E3021`, 3 units, needs inlining) and computed goto (`E3029`, 2 units). |
| 1.9.66 | 2026-09-06 | Rules the three classes the seventh survey (6 clean, `E3033` gone, `E3030` 2,737) leaves under varargs: file-scope `asm` becomes the module assembly table (2.1, Section 9; `EXPORT_SYMBOL` in nearly every unit), checked arithmetic lowers exactly in the smallest of 32, 64 and 128 bits with a representability test (`E3020` withdrawn), and the bit builtins lower to new total instructions `ctlz`, `cttz`, `ctpop`, `bswap` (3.4; `E3023` withdrawn; `bit_builtin_zero_operand` obligation). |
| 1.9.65 | 2026-09-05 | Rules the two lowering classes still hidden behind the survey's face so no window stops on them: static pointer initializers are relocations `(offset, @symbol, addend)` on a global (2.1, Section 9; `E3026` narrows to non-symbolic initializers), and pointer-integer conversions are `ptr.to_int` / `ptr.from_int` with exposed-address provenance and the oracles' measured extension and truncation (3.7; `E3036` withdrawn). |
| 1.9.64 | 2026-09-05 | Gives the module external-object and external-function tables (`extern`, `declare`), so a unit that calls or reads what another unit defines lowers instead of stopping at a missing callee: top effect summary for external functions, distinct provenance for external objects, interpreters execute only the 10.1.14 library set (`E3039`, `E3040` otherwise), `E3041` for a symbol both declared and defined, `E2931` for inconsistent unprototyped calls. This was a gap of the specification, not of the executor: revision 0.7 of EvoMIR had assumed every callee is defined beside its caller, which is true of the generated corpora and of no kernel unit. |
| 1.9.63 | 2026-09-05 | A library builtin's synthesized prototype is a unit-level symbol in the builtin namespace: it introduces no ordinary identifier and belongs to no function scope (both oracles, `p37-*`). |
| 1.9.62 | 2026-09-05 | Rules the two declaration-identity questions of the lowering frontier: declared alignment is an object property, the maximum over a unit's declarations, and never part of a type identity; a declaration-only function with an incomplete enumeration in its type produces nothing, and a use while it is incomplete is `E2930`. Both measured under the sealed oracles (`p36-*`). |
| 1.9.61 | 2026-09-05 | Opens the two classes the sixth lowering survey put first (1,697 and 1,022 of 2,792 diagnosed units): `bits<128>` executes in every scalar form but division (10.1.19), and variadic signatures, anonymous arguments and the three list instructions `va_start`, `va_arg`, `va_end` enter Sections 1.1, 3.10, 3.12 and 9, with `va_copy` a 32-byte `copy`. Both are measured against the sealed oracles (`p34-*`, `p35-*`: fourteen `va_arg` items and eight `__int128` ABI shapes, GCC-14 and Clang-19 agreeing); the register-level procedures are EvoMIR's. New codes `E3037`, `E3038`; `E3030` narrows to aggregates; `E3033` narrows to division and remainder. |
| 1.9.60 | 2026-09-05 | Under the user's authorization to close the two reported blockers, defines the missing assembly interchange grammar and output/effect edge bindings. Both-successor output availability is measured with the sealed oracles; the explicit SSA and byte encoding are compiler contract choices. No production code is changed by this specification revision. |
| 1.9.59 | 2026-09-04 | Gives poison a pointer form (`uninitialized.ptr`, `uninitialized.fnptr`), so a pointer local without an initializer lowers rather than stopping the unit, and allocates `E3036` to pointer and integer conversions in either direction, which revision 1.7 deferred and the wrapper `E3101` had been carrying. Both classes were reduced to one line each by the architect. |
| 1.9.58 | 2026-09-04 | Specifies inline assembly as an opaque EvoIR instruction with the constraint vocabulary the configured kernel uses, measured over 24,735 `asm` statements in twenty-four preprocessed units and against both oracles; `asm.goto` is a terminator, the effects are maximal, no rewrite can reach it, and neither interpreter executes it, so a program containing one is verified by the native run alone. |
| 1.9.57 | 2026-09-04 | First full-configuration lowering survey (4 clean of 2,798). Defines global register variables: no allocation site, the manifest passes over them, a use is `E3035` and the declaration is accepted, which is what stopped 2,754 units; states that a lowering failure names its class and `E3101` is a wrapper, never a resting place. |
| 1.9.56 | 2026-09-04 | First search over cost weights: a searched vector ties the best hand-written functions on held-out programs and beats none, because the registry's cost can express one preference only (add before mul before shl). States that a policy is identified by name plus a complete description in the report (7.5). |
| 1.9.55 | 2026-09-04 | Emitting rewrites measured: the two all-positive cost functions now optimize 569 of 1,000 programs differently. Restates the strict-improvement sentence of 11.3 as producer conformance and fixes the class granularity of a derived node's use through its parent (11.3). |
| 1.9.54 | 2026-09-04 | The configured kernel's every translation unit parses clean (2,798 of 2,798). Assigns `E2928` to the address of a `register` object, `E3034` to the lowering of function-pointer arithmetic, and states that a range designator is one plan entry; the three edges the front-end executor named. |
| 1.9.53 | 2026-09-03 | Fifth full-configuration survey (2,760 of 2,798). Defines the sixteen rules behind the remaining 38 units: one conditional stack across argument collection, empty `##` operands, comments across continued definitions, `__VA_OPT__`, keyword parameter names, `-fshort-wchar` wide strings (10.4); `register`, `va_list` objects, 128-bit and `va_arg` places, parameter compatibility, function-pointer arithmetic, `\e` (10.1.18); literal addresses, null comparisons in conditionals, no initializer size cap with `E2207` withdrawn (10.1.19); string literals at every site (10.1.3). All measured. |
| 1.9.52 | 2026-09-03 | States that a derived node reachable as a child of a named derived node is used, so an emitting right-hand side may write a `const_like` operand (11.3); the executor stopped `p2_302` on the unqualified sentence. |
| 1.9.51 | 2026-09-03 | The checker stops judging cost: strict improvement is producer conformance under the declared policy (11.3), after the policy harness measured that no all-positive cost function can differ from another in the P0 profile. Defines emitting right-hand sides and `width_min` (11.6) and prints `p0.scalar.v4` with the doubling rules `mul(x,2) = add(x,x) = shl(x,1)`, the first rewrites whose extraction is a genuine choice. |
| 1.9.50 | 2026-09-03 | States that extraction cost is a producer policy the checker does not verify, so cost functions and schedules may be replaced and measured without a format change (7.5). |
| 1.9.49 | 2026-09-03 | Corrects the 10.1.17 label-address entry, which had denied address-constant status without a probe and contradicted the measured rule of 1.9.48; the executor stopped on the contradiction. |
| 1.9.48 | 2026-09-03 | Fourth full-configuration survey (2,694 of 2,798 clean). Defines the `_Pragma` operator through expansion and the whitespace flag of a substituted argument (10.4); qualified `void *` to function pointer, conditional and label address constants, and three more library builtins (10.1.14, 10.1.18). |
| 1.9.47 | 2026-09-03 | Names the edge combination of seed facts a join, distinct from the class merge, after both had been called a meet (8.10); lifts the sentence that made `p1_202`-`p1_204` unfireable, since a fully known block argument now satisfies `fully_known` without `all_const` (11.6). Both gaps found by the executor implementing facts of seeds. |
| 1.9.46 | 2026-09-03 | Regions admitted in every block (12,913 to 27,195 in the 1000-program gate). Rules that an instruction outside the sort bound is absent from the allowlist, records that `definition` seeds cannot arise in this profile (8.5, 8.7), and defines facts of seeds across block edges by a forward fixed point the checker recomputes (8.10). |
| 1.9.45 | 2026-09-03 | Admits `__builtin_va_arg` at the front end as a typed C-HIR node under the same fail-closed lowering, and makes a library builtin's prototype the call's type whether or not the library function is declared, `E2927` now naming an incompatible visible declaration; both sentences had contradicted 1.9.34 and stopped the front-end executor. |
| 1.9.44 | 2026-09-03 | Admits shifts and width changes as region roots and witnesses (8.5) and in the certificate's `op_id` (11.3), after the 1.9.43 transfer rows were measured to enable zero premised rewrites while excluded from regions; extends the witness closure to structural duplicates so a common subexpression is a region proved by congruence alone (8.5); prints `p0.scalar.v3` with six fact-driven folds for the new operators (11.6). |
| 1.9.43 | 2026-09-03 | Class-based matching measured (+25 regions, +26 merge-enabled matches over 256 seeds). Extends the 8.10 transfer table with `shl`, `lshr`, `ashr` under a fully known in-range amount and with `zext`, `sext`, `trunc`; the registry is unchanged, the new facts feed the existing Tier B rules. |
| 1.9.42 | 2026-09-03 | Congruence closure measured. Places a congruence that holds at insertion before every epoch marker and one a merge enabled after that epoch's marker; states that a rewrite joining a class with itself is not recorded and the replacement stands on the recorded merges (11.3). |
| 1.9.41 | 2026-09-03 | Fact-driven rewrites measured (772 of 7,900 regions carry a premise; residual regions 20 to 0). Settles four readings: folds carry no premise (8.10), a certificate names the narrowest reviewed registry containing its rules, rule order within a tier is registry order so three Tier B rules are dominated, premise classes resolve through the partition, and a premise on a premise-free rule or with the wrong digit count is invalid (11.6). |
| 1.9.40 | 2026-09-03 | Prints the reviewed registry `p0.scalar.v2` (18 Tier B rules over the 8.10 facts) with its digest, defines registries and revisions, the Tier B side conditions and premises (11.6); corrects the premise comparison so a claim the checker cannot confirm is refused (11.3); fixes the `icmp` transfer as an interval procedure (8.10). Three gaps the randomized gate stopped on. |
| 1.9.39 | 2026-09-03 | Defines a run: at most eight chained transactions verified by digest chaining (8.11), after the residual-region measurement of 20 in 21,000. Records that a producer keeping admitted regions at a ceiling stop is conformance the checker cannot observe, since 11.2 permits declining to optimize an eligible region. |
| 1.9.38 | 2026-09-03 | Bounded scheduler measured (722 of 977 optimized programs run more than one epoch). Corrects `saturated` to the 8.11 meaning, names residual regions as a registry metric, selects the largest region's count as the certificate's `n`, reserves `stop=fuel`, and requires admitted regions to survive a ceiling stop (11.3). |
| 1.9.37 | 2026-09-03 | evp2 produced and verified across 21,000 programs. Epoch markers are numbered from 1; `evp2` requires its budget line and `evp1` forbids it; adds the `policy` stop reason for a producer's own stop rule so that `saturated` is never claimed without a fixpoint (11.3). |
| 1.9.36 | 2026-09-03 | Second randomized campaign on extended shapes (21,000 programs, 0 mismatches, one producer refusal fixed: alias right-hand sides come from the binding). Fixes the `budget` line: every quantity is consumed/bound, `fuel` carries its bound and the `n` it derives from, and the checker verifies the profile constants and the fuel derivation (11.3). |
| 1.9.35 | 2026-09-02 | First randomized semantic gate campaign (21,000 programs, 0 semantic mismatches, 12 certificate refusals). Allocates `E3008` `interpreter-fuel-exhausted` as a host-resource stop (6.3, 10.4); defines what a certificate with no region claims and how the checker verifies it; states that rewrite bindings are recomputed from the recorded node children (11.3). The `evp1` versions paragraph already answers the scheduler-record question: budget and epoch lines belong to `evp2`. |
| 1.9.34 | 2026-09-02 | Third full-configuration survey (2,445 of 2,798 clean). Defines array lvalue decay for members, flexible members and string literals (10.1.3); range designators and inferred bounds (10.1.12); function pointer and `void *` conversions, `_Pragma`, `__inline`, `__restrict` and attribute placement (10.1.18); integer constants cast to pointers and static array decay as address constants (10.1.19); library and GCC-only ctype builtins (10.1.14). Measured. |
| 1.9.33 | 2026-09-02 | Second full-configuration survey (1,292 of 2,832 clean). Defines directives inside macro arguments (`E0113` for `#pragma`, an oracle conflict) and empty-argument variadic forwarding (10.4); `__func__` and `__FUNCTION__`, `E2921` for `__PRETTY_FUNCTION__` (oracle conflict), labeled last statements in statement expressions (10.1.17); array parameter adjustment and pointer sign in conversions (10.1.18); modifiable lvalues as one predicate (10.1.3); address constants through casts and nested aggregates (10.1.19). Measured. |
| 1.9.32 | 2026-09-02 | Defines source locations: spelling and expansion spans, total span composition from expansion spans, no buffer slicing (10.1.20); defines the whitespace flag, stringization spacing, and rescanning into the outer token sequence (10.4). Three measured defects behind the full-configuration survey's 2,768 panics and 56 `E0105` units. Measured. |
| 1.9.31 | 2026-09-01 | Admits the `__typeof` spelling; states that a statement expression inside a `typeof` operand scopes every declaration it makes, including ones with linkage, after an `extern` declared there escaped and made a later compatible declaration a redefinition; defines the redeclaration rule and `E2002`, which had been emitted without a definition. Measured. |
| 1.9.30 | 2026-09-01 | Makes explicit in 10.1.18 that a declarator list may declare several functions and that `typeof` of a dereferenced function pointer names a function type; both were already implied and both were refused by the implementation, the first by a deliberate restriction the kernel turned out to need. |
| 1.9.29 | 2026-09-01 | Rules the five gaps the wide-enum window named: character pointers of differing signedness convert implicitly (both oracles warn only); `__builtin_frame_address` accepted and fail-closed (`E3031`); signed shifts in constant expressions wrap in the operand width as both oracles do, so `1 << 31` is an enumerator; `void *` difference typed and fail-closed in lowering (`E3032`); 128-bit values accepted throughout the front end and fail-closed in execution (`E3033`). All measured. |
| 1.9.28 | 2026-09-01 | Enumerators outside `int` widen the enumeration to the smallest representing type (one `ORACLE_CONFLICT` on the other constants' type, resolved to the C23/GCC rule); `__attribute` as a synonym; `void *` arithmetic in update forms; 128-bit assignment. All measured; the last two were one rule on two paths. |
| 1.9.27 | 2026-09-01 | States that any pointer-typed expression may root a place, with one implementation and no list of admitted forms, after the fourth root form (call results, then conditionals) was refused; specifies `__builtin_strlen` as a constant on a literal and an ordinary call to the in-scope `strlen` otherwise (`E2927`). Measured. |
| 1.9.26 | 2026-09-01 | Specifies `__builtin_has_attribute` as a `_Bool` constant answered from the preserved attribute inventory, the three `va_*` builtins at the front end with fail-closed lowering (`E2926`, `E3030`), and empty records with size zero. All measured; added after 13 GCC lanes stopped on the first and 13 Clang lanes on the second. |
| 1.9.25 | 2026-09-01 | Defines `packed` on enumerations from measured oracle behaviour: smallest representable integer type, signed when a negative enumerator exists, alignment equal to size, constants still `int`; the `packed`+`aligned` combination is refused as an unarbitrated oracle conflict the cohort never uses. Added after the executing agent bisected 15 failing static assertions to `sizeof(enum rw_hint)`. |
| 1.9.24 | 2026-09-01 | Generalizes the null pointer constant to every conversion context including function pointers, after `(void *)0` into a function-pointer member was refused while `0` was accepted; admits an address constant cast through an integer type and back, as the kernel's symbol exports write it, with lowering still governed by the relocation deferral. Both measured. |
| 1.9.23 | 2026-08-31 | Corrects 10.1.11: a statement expression is permitted anywhere inside a function definition and nowhere outside one, `E2905` meaning only the latter. Revision 1.9.22 also fired it wherever a constant expression was required, which stopped 26 lanes on the kernel's `container_of`, where the statement expression sits inside `typeof` and is never evaluated. Records that this profile does not fold a statement expression to a constant although both oracles do. Adds `typeof` of a function designator as a declaration specifier, and `E2925` for an out-of-range `__builtin_object_size` mode. |
| 1.9.22 | 2026-08-30 | Rules the three frontier constructs. An attribute may precede a null statement and nothing else (`E2924`), is preserved and applied to nothing. `__builtin_offsetof` admits a non-constant index: the result is runtime arithmetic, the index is evaluated once, it is not an integer constant expression, and it is not bounds checked. `__builtin_object_size` never evaluates its operand and always answers unknown, which asserts nothing and makes the recorded GCC/Clang disagreement moot. |
| 1.9.21 | 2026-08-29 | Specifies `__typeof_unqual__` in 10.1.10: `typeof` with top-level `const`, `volatile`, `restrict` and `_Atomic` removed, operand unevaluated, pointee and member qualifiers kept, resolved entirely by the front end. One `ORACLE_CONFLICT` on array element qualifiers resolved by keeping them, the direction that refuses a write. Added after 28 kernel parse lanes stopped on it; the cohort writes it 4,797 times. |
| 1.9.20 | 2026-08-29 | States in 10.1.6 that a member of an anonymous structure or union is a member of the enclosing record for designators and `__builtin_offsetof` as well as for access, that resolution recurses through anonymous members and records every step, and that a name ambiguous between two of them is `E2906`. Added after 28 kernel parse lanes stopped on `offsetof(struct pt_regs, sp)` while `o.sp` already worked. |
| 1.9.19 | 2026-08-29 | Corrects 10.1.19: a zero array bound is a valid GNU zero-length array, measured in every record position and at file scope; `E2923` is for a negative bound only, and `E2203` has one message everywhere. Revision 1.9.18 refused zero without probing it, which no cohort unit could have passed. Adds `__int128_t` and `__uint128_t` as spellings, and repairs the dangling `__int128` cross-reference. |
| 1.9.18 | 2026-08-29 | Adds 10.1.19: what an integer constant expression is, that `sizeof`/`_Alignof` fold over expressions as well as type names, `sizeof(void)` and `sizeof` of a function type are `1`, and the conditional operator's null-pointer-constant rule that the kernel's `__is_constexpr` depends on. Requires array bounds to be evaluated wherever written (`E2203`, new `E2923`) after they were found to be checked only when an initializer followed. |
| 1.9.17 | 2026-08-28 | Defers computed `goto` and label-address lowering to `E3029` instead of specifying an indirect-branch terminator: the cohort writes no `goto *`, and its only label-address use is a cast to an integer that `ptrtoint` already excludes. Revision 1.9.16 named a lowering that Sections 3 and 9 did not define. Adds 10.1.18, the three C11 declaration forms the remaining 28 parse lanes stop on. |
| 1.9.16 | 2026-08-28 | Closes the eight gaps the compile-time-builtin and aggregate-copy windows reported and the one blocking every parse lane: the empty external declaration is accepted; `__auto_type` needs a plain identifier and may not reference itself (one `ORACLE_CONFLICT`, refused); computed `goto` and label addresses are specified with their conservative successor set (`E3028`); `E2920` for an undeclared label; a non-constant case-range bound is `E2916`; `volatile` union and bit-field copies fail closed (`E2922`); and records cross calls through memory with a trailing return-destination pointer and reserved synthetic origins. |
| 1.9.15 | 2026-08-27 | Names `trap.unreachable_reached` as the second trap reason, with its textual form, effects, appendix row and C-HIR contract row, and states that a trap terminator is a report and never a licence for an optimistic transition. Revision 1.9.14's 10.1.17 referred to an unreachable terminator that Sections 3, 4 and 9 did not define. |
| 1.9.14 | 2026-08-27 | Adds the `copy` instruction (Section 3, effects, textual form, `E3027`) as the single lowering of C aggregate assignment, copy-initialization, by-value passing and return; states padding and initializedness transfer from measured oracle behaviour; admits file-scope flexible-array-member initializers (`E2919` elsewhere). Opened because six kernel parse lanes stop on aggregate copy-initialization. |
| 1.9.13 | 2026-08-27 | Closes the fourteen gaps the preprocessing-grammar and statement-expression/initializer windows reported: `E2917`, `E2918`, `E3026`; join-block argument order and exit cleanup for statement expressions; scalar and non-constant designators; incomplete-array bounds and range initializers (measured; one `ORACLE_CONFLICT` resolved by refusal); the `compound_literal` global kind and read-only storage; paste results and `$`; the stale exclusion in 10.1.7. |
| 1.9.12 | 2026-08-27 | Completes the preprocessing-token grammar: `.5foo` is a pp-number, stray characters such as `\\` are single pp-tokens kept by stringification, and `E0112` is the one diagnostic for a pp-token with no C form when it reaches the C lexer. Measured after every raw kernel lane stopped on `\\sreg` in `sysreg.h`. |
| 1.9.11 | 2026-08-26 | Token pasting yields any preprocessing token, including pp-numbers such as `1_MODULE`; `E0111` for a pp-number that is not an integer in `#if`. Measured after the re-lexer rejected the kernel's `IS_MODULE(1)` expansion in every raw lane. |
| 1.9.10 | 2026-08-26 | Closes the three gaps the paste/pack window reported: `E0110` for an invalid `##` result, `pack(pop)` on an empty stack ignored, `pack` honoured at block scope and refused inside a member list (`ORACLE_CONFLICT`). All measured. |
| 1.9.9 | 2026-08-26 | Corrects 10.4 `#pragma pack`: a member `aligned` attribute does not raise alignment above the active pack cap. Revision 1.9.8 asserted the opposite from documentation without a probe; the executing agent's sealed probe showed both oracles at 5/1. |
| 1.9.8 | 2026-08-26 | Adds 10.1.17 (compile-time builtins, bit builtins, `__builtin_unreachable`, `__builtin_return_address`, `__auto_type`, `?:`, case ranges, `__label__`) with measured oracle semantics; gives `#pragma pack` its measured layout meaning; freezes `#if` arithmetic on suffixed and pasted pp-numbers after a measured defect; `E0109`, `E2915`, `E2916`, `E3022`-`E3025`. Fixes the dangling 10.1.17 reference and the pragma-count wording reported by the preprocessor window. |
| 1.9.7 | 2026-08-26 | Adds 10.4, the EvoCPP target profile: the sealed GCC-14 predefined macro list as authority, dynamic macros, the four feature queries with measured answers, pragmas, and preprocessor diagnostics `E0102`-`E0108`. Added because all 16 raw kernel lanes stop on `#error \"Unknown compiler\"`. |
| 1.9.6 | 2026-08-26 | Closes the five gaps the checked-arithmetic/asm window reported: `E2914` spans per case, what lossless `asm` keeps and drops, `const` pointee and `__int128` operands as `E2913`. Contract out-of-scope text corrected from `inline asm` to `asm lowering`. |
| 1.9.5 | 2026-08-25 | Freezes `_Static_assert` diagnostics (`E2911`, `E2912`) and the non-arithmetic `__builtin_expect` operand (`E2913`), adds 10.1.15 checked arithmetic builtins and 10.1.16 GNU `asm` at the front-end boundary with measured oracle semantics, both fail-closed in lowering (`E3020`, `E3021`) until Section 4 gains the operations. Added because 18 and 12 kernel parse lanes stop on them. |
| 1.9.4 | 2026-08-25 | Adds 10.1.14: `__builtin_expect`, `__builtin_constant_p` and `__alignof__` resolved by the front end with rules measured in the sealed oracle image, two recorded `ORACLE_CONFLICT`s resolved to the documented-permitted answer, and `E2909`/`E2910`. Pointer-declarator attributes are preserved and never applied. Added because 28 of 30 kernel parse lanes stop on these builtins. |
| 1.9.3 | 2026-08-25 | Adds the pointer-declarator attribute position to 10.1.9: after `*` inside a pointer declarator the attribute is a type attribute of that pointer type. Added because two kernel parse lanes stop on `void * __attribute__((__warn_unused_result__)) ERR_PTR(long)`. |
| 1.9.2 | 2026-08-25 | Adds C11 generic selection to Section 10.1.10 beside `typeof`: controlling type after lvalue conversion, one compatible association or `default`, unselected branches unevaluated, nothing surviving into EvoIR; assigns `E2907` and `E2908`. Added because every kernel parse lane stops on `typeof(_Generic(...))`. |
| 1.9.1 | 2026-08-25 | Records two corrections from the sealed layout oracle and the kernel cohort. `aligned(N)` on a typedef or type name sets the alignment exactly and may lower it, while on records and members it only raises; the frozen attributed layout corpus is the authority. Adds the tenth attribute position, immediately before a declarator that follows a comma in a declarator list, attaching to that declarator alone; declarator lists are admitted in every declaration position including record members, where each declarator carries its own bit width (a constant expression) and attributes. |
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

## Appendix D: Exhitect review

### Termination builtins and the existing external-call boundary

`__builtin_abort()` has prototype `void (void)` and resolves to the ordinary external `abort` call, with the same compatible-declaration check and no invented interpreter library model.
Both sealed oracles compile and execute the unreachable-abort control in `.evidence/builtin-probes/20260907-function-specifiers/abort.c`; GCC accepts `termination.c`, but Clang-19 rejects `__builtin_exit` and `__builtin__Exit` as unknown builtins, so those spellings remain refused as E2933 (`unsupported-termination-builtin`).
The result is recorded in `termination.out`; reached external `abort` still has the body's E3039 interpreter boundary and machine execution delegates to the linked library.
The architecture's preceding torture inventory counted 404 termination-builtin refusals without a spelling split; the next lane reports the actual effect rather than attributing all 404 to this one accepted spelling.

### Linkage attributes still conflict with the preserved-only body

Both sealed oracles make `alternative` an alias of `value` and emit `fallback` as a weak symbol in `.evidence/builtin-probes/20260907-function-specifiers/linkage.c`, recorded in `termination.out`.
Section 10.1.9 explicitly says `weak` and other non-layout attributes are preserved without effect; this body constraint takes precedence over inventing linker semantics in Appendix D, so the execution request's applied alias/weak feature is not closed.
No validator is loosened, alias body fabricated, or new diagnostic allocated; C-HIR retains the attributes, but linked program behavior is not claimed equivalent.
The supplied musl inventory identifies 108 affected symbols, not a measured first-error unit count; this remains an explicit incomplete requirement.

### C11 function specifiers and the variable-length-array capability

`_Noreturn` is accepted on function declarations and definitions, retained as a `noreturn` attribute with written spelling `_Noreturn`, and supplies no new dead-code or control-flow licence.
Both sealed oracles accept the valid forms in `.evidence/builtin-probes/20260907-function-specifiers/valid.c`; GCC warns but accepts the object/typedef probes while Clang rejects them, so the profile refuses those placements with E2934 (`function-specifier-on-non-function`), as recorded in `results.out`; revision 1.9.74 reserves the previously proposed E2932 for `always-inline-cycle`, and that body allocation takes precedence.
The 2026-09-07 execution request adds the truthful profile capability `__STDC_NO_VLA__=1` after the unchanged sealed predefined baseline and before command-line overrides; this is an EvoCC profile declaration, not a claim that the VLA-supporting oracles define it.
No full-configuration first-error count is yet measured; the toybox lane currently strips this specifier from two files, so an adjusted lane alone cannot prove native acceptance.

### Empty parameter lists in function definitions

A function definition written with an empty parameter list has zero parameters; its preserved C declarator remains distinct from a prototype, while the lowered definition has zero data arguments.
Both sealed compilers accept and execute `.evidence/builtin-probes/20260907-function-specifiers/empty-definition.c` with exit zero, recorded in `empty-definition.out`.
Declarations without a definition keep the existing observed-call signature rule, and named old-style parameter declarations are not enabled by this change.
No diagnostic is added; the torture lane rewrites 957 empty lists, so its existing adjusted pass count does not measure this native acceptance.

### Equality of integer pointers with differing signedness

Equality and inequality accept pointers to integer types that differ only in signedness, using the existing pointer-conversion compatibility relation; ordered comparisons retain their existing rules.
Both sealed GCC-14 and Clang-19 accept and return zero for `.evidence/builtin-probes/20260907-function-specifiers/pointer-equality.c`, with distinct-pointer-type warnings recorded in `pointer-equality.out`.
No diagnostic is added and no memory or validator rule is relaxed.
This reproduces Csmith base seed 5 from run `20260906T184351Z`; that run has 164 E2102 refusals across both campaigns, but their individual construct attribution is not yet established.

### Address-only blocks versus terminator-edge reachability

Section 3.7 admits label addresses, but Section 2.5 still requires every validated block to be reachable by terminator edges; `unsigned long f(void){return (unsigned long)&&after; after:return 0;}` exposes the conflict.
Both sealed oracles produce a nonzero address (`.evidence/builtin-probes/20260906-code-addresses/address.c`, `address.out`); the body's reachability invariant takes precedence, so this form remains an existing E3101 validation refusal, with no fabricated edge or validator relaxation.
No new code is assigned; affected full-configuration unit count is not yet measured, and this is not an allowed completion residual.

### Ordered pointer comparisons remain excluded by the body

Sections 3.4 and 3.7 explicitly exclude ordered pointer comparisons; revision 1.9.67 admits pointer subtraction but does not override that separate exclusion.
Both sealed oracles print `15` for the same-array `<`, `<=`, `>`, `>=` cases in `.evidence/builtin-probes/20260906-pointer-ordering/order.c`, recorded in `order.out`; this does not authorize broadening the body's IR profile.
The existing lowering refusal is named E3045 (`not-lowered: ordered-pointer-comparison`) without changing front-end acceptance or inventing a pointer-ordering rewrite.
The construct has been located at `fdt.c:321` and `fdt_ro.c:276`; final unit counts must come from the sealed full-configuration survey, and E3045 is not part of the architecture's allowed terminal residuals.

### Checked-product representability at the signed boundary

Section 10.1.15's mathematical representability rule takes precedence over its claim that a high-bit-set unsigned result always compares unequal to a sign extension: equal-width extension preserves exactly those bits, whereas a wider signed destination can hold them.
Both sealed oracles at `-O2` report overflow for `~0ul * ~0ul` stored in signed 128 bits and no overflow for two unsigned 16-bit maxima stored in signed 64 bits (`.evidence/builtin-probes/20260906-unsigned-result/representability.c` and `representability.out`, including the initial Clang runtime-helper link failure).
The predicate therefore also rejects negative signed exact results stored unsigned, and high-bit-set unsigned exact results stored signed when the destination is no wider; no diagnostic is added.
This refines the 48 unsigned-product first-error units in sealed run `20260905T213244Z-60582`; subsequent impact must be measured.

### Checked operands beyond the available exact representation

Section 10.1.15 explicitly excludes 128-bit operands and result pointees as E2913; that specific boundary takes precedence over the general 128-bit execution opening, and the executor's earlier expansion and contrary statement here are corrected.
For admitted operands, the body's signed-width formula can still exceed 128 bits: unsigned 64-bit maximum multiplied by signed `-2L` has wrapped unsigned result `2` and overflow `1`, as both sealed oracles report in `.evidence/builtin-probes/20260906-mixed-product/product.c` and `product.out`.
Where that formula requires more than 128 bits, its available `bits<W>` set takes precedence: lowering remains fail-closed as E3044 (`not-lowered: checked-arithmetic-exceeds-128-bits`), not withdrawn E3042 or a wrapping substitute for exact arithmetic.
The prior sealed survey identifies all 48 width refusals as unsigned 64-bit multiplication, so this separate boundary has no identified first-error unit there; its next-run count must be measured.

### Checked arithmetic outside the signed exact-width bound

Section 10.1.15's signed-width formula remains authoritative: two unsigned 64-bit operands require 130 counted bits for multiplication, contradicting its claim that every cohort use fits 128.
Both sealed oracles print `0 4900000000`, `1 18446744073709551614`, and `1 1` for `.evidence/builtin-probes/20260906-checked-width/width.c`, recorded in `width.out`; this does not authorize replacing the body's signed exact representation with an unsigned one.
The existing refusal is named E3042 (`not-lowered: checked-arithmetic-width`), rather than hidden under E3101.
This construct is the first error in 47 units of sealed run `20260905T201056Z-67314`.

### Direct object/function pointer conversion boundary

Section 10.1.18 admits the C conversion, but Section 3.10 explicitly keeps `ptr<AS>` and `fnptr<Sig>` distinct without a direct conversion; this boundary is retained as E3043 (`not-lowered: object-function-pointer-conversion`).
Both sealed oracles print `7` for `.evidence/builtin-probes/20260906-function-pointer-conversion/conversion.c`, recorded in `conversion.out`; the front end still accepts that source, while an explicit whole-pointer integer round trip uses the separately specified Section 3 conversions.
No pointer retyping, invented callee, or validator relaxation is introduced.
Seven units first encounter the direct conversion in sealed run `20260905T201056Z-67314`.

### Compiler hints and dynamic stack allocation

Per the 2026-09-06 architecture request, `__builtin_prefetch` emits no memory operation or external call, but evaluates its address argument exactly once and preserves those effects.
Both sealed oracles report `prefetch-evaluations=3 stack-value=7` for `.evidence/builtin-probes/20260906-compiler-hints/hints.c`, recorded in `hints.out`; stack allocation is valid C but remains an explicit profile deferral rather than an unresolved library call.
Lowering a call to the synthesized `__builtin_alloca` prototype reports E3046 (`not-lowered: dynamic-stack-allocation`), while an ordinary user-defined function named `alloca` is unaffected.
The baseline torture run `20260906T070921Z` reports five prefetch link failures and four alloca link failures; this is not a new whole-kernel measurement.

### Assembly immediates before specialization

Section 3.11's `i` operand must be a proved integer constant, and Section 10.1.9 preserves `always_inline` without applying it, so an unspecialized parameter retains E3021 with constraint name `i`.
Both sealed oracles accept the constant-specialized form and reject the dynamic form in `.evidence/builtin-probes/20260906-immediate-parameter/immediate.c`, recorded in `immediate.out`.
No inlining promise is inferred from a preserved attribute and no new diagnostic is allocated.
This is the first error in three units of sealed run `20260905T201056Z-67314`.

### Pointer difference remains a separate execution obligation

The new pointer/integer casts do not discharge the same-object obligation of pointer subtraction, whose existing body-required lowering refusal remains E3032.
The measured value and type are retained in `.evidence/builtin-probes/20260901-2c23684c/p11.c` and `p11.out`, with a lowering-refusal control in `crates/evocc-lower/tests/fail_closed_forms.rs`.
Eight zstd units first encounter this construct in sealed run `20260905T201056Z-67314`; no diagnostic or pointer-difference instruction is invented.

### Static compound storage and block linkage

Both sealed oracles compile `.evidence/builtin-probes/20260906-static-storage/storage.c`
and print `12 7 7`: a static compound initializer copies its value, a discarded
pointer expression keeps its side effects, and a block-declared external function
may later be defined at file scope with the same linkage identity.
Section 2.1 already names `compound_literal` but Section 9 omits its kind spelling;
use that spelling and the private symbol `__evocc.compound.<origin>` without
changing allocation-site identity.
A function definition binds its file-child lexical scope to the existing
non-builtin function symbol even when its first declaration was inside a block;
this does not permit a definition inside that block.
No diagnostic is added; survey `20260905T185525Z-27277` measured one aggregate-copy
initializer, one block function-address lookup and two discarded-pointer sites.

### Qualified function parameter identities

Section 10.1.18's existing compatibility rule ignores parameter top-level
qualifiers when comparing a function declaration with its symbol or written
parameter list, without removing those qualifiers from the parameter object.
Both sealed oracles compile `.evidence/builtin-probes/20260906-parameter-qualifiers/qualifiers.c`
and print `7 7 7 7` in `qualifiers.out` (SHA-256
`29b8ca085beae90f9c40224f98b731817a064d4cb9dd118bdbac6d8e90cda26b`).
The implementation reuses TypeInterner compatibility instead of a second
comparison; no diagnostic is added, and ten units in lowering survey
`20260905T185525Z-27277` reached the corresponding identity failure.

### Completion target versus explicit execution deferrals

Lowering survey `20260905T185525Z-27277` records 2,700 first errors at E3035
and two at E3029, with zero panic or timeout.
Section 10.1.18 explicitly requires refusal of global-register reads/writes
and used label addresses/computed jumps; these body rules remain authoritative
even though the requested completion target permits only E3030, E3033 and E3026.
Existing minimal controls are `lowering_diagnostic_frontier.rs` and
`computed_goto_lowering.rs`; no diagnostic or unmeasured register/branch
semantics is invented, and the overall completion target remains unmet.

### Signed relocation addends

Section 2.1 explicitly makes a relocation addend signed, while Section 9's
abbreviated `reloc` production refers to unsigned `integer` for that field;
the explicit signed rule takes precedence, with canonical optional `-` and no `-0`.
The positive, negative and malformed byte-form controls are in
`crates/evocc-evoir/tests/relocations_and_addresses.rs` and the independent
Section 9 field sentinel; this IR spelling conflict is not an oracle-measurable
C behavior, adds no diagnostic, and has no measured cohort count yet.

### Implemented instruction shape corrections

The existing Section 9 `copy` production supplies the omitted canonical shape
below, and the existing assembly forms are now implemented rather than pending.
This is instruction-shape metadata only, tested by whole-module text tests,
not a change to C behavior; it adds no code or affected-unit count.

```text
(<>) = copy <>, <>, <>, <> [size=<>, align=<>]
(<>) = asm<> <> <>, <> outputs [<>] inputs [<>] clobbers [<>]
(<>) = asm.goto<> <> <>, <> outputs [<>] inputs [<>] clobbers [<>] labels [<>] fallthrough <>
```

### Negative checked results stored into an unsigned type

Section 10.1.15's mathematical representability rule requires overflow for a negative result stored into an unsigned type, even when that type is as wide as (or wider than) `W`; the abbreviated truncation/extension test alone does not establish this case.
The lowered overflow predicate additionally includes signed `exact_result < 0` for unsigned destinations, without changing the specified arithmetic width or runtime UB rules.
Both sealed oracles print `1 18446744073709551615 1 4294967295` for `.evidence/builtin-probes/20260906-checked-conversion/result.c`, recorded in `result.out` (SHA-256 `3a4f266c33c39dac31175baedf4d36afd03fbbd95c86f969dd957505630a315a`).
No diagnostic is added; affected full-configuration unit count is not yet measured.

### Variadic marker placement

Section 1.1's explicit placement rule takes precedence over the abbreviated
Section 9 productions: `...` appears immediately before `mem<R>, world`,
never after the effect-token parameters.
The minimal canonical-text reproducer and its rejection control are in
`crates/evocc-evoir/tests/external_symbols.rs`; this is an internal IR grammar
conflict, not an oracle-measurable C behavior, and introduces no diagnostic code.
Full-configuration impact has not yet been measured.

### External ctype execution boundary

Revision 1.9.64's seven-function execution boundary takes precedence over
the older Section 10.1.14 ctype execution-table requirement: without a
definition, calls to the five ctype functions, including their builtin
spellings, report the existing E3039 in both interpreters.
Evidence is the minimal calls in `crates/evocc/tests/external_symbol_lowering.rs`;
the sealed `p26-builtins.c` and `p28-table.out` describe C behavior, not
availability of an implementation in this executor.
The original oracle-table fixture is unchanged; its interpreter test now
pins the explicit refusal, while front-end admission and typing are unchanged.
Full-configuration impact has not yet been measured.

### Array parameter identity checks

Identity checks apply Section 10.1.18's outermost array-to-pointer adjustment
when comparing a parameter's preserved written declarator with its canonical
type, including a typedef naming an array; remaining array layers, element
types and qualifiers are still checked without adjustment.
Both sealed oracles compile the four type assertions in
`.evidence/builtin-probes/20260905-external-parameters/parameters.c` and print
`7 7 7 7` in `parameters.out`.
This removes the existing E3101 identity refusal rather than introducing a
new code, and affects one first-error unit in sealed lowering run
`20260905T113121Z-43616` (`lib/crypto/chacha-block-generic.c`).
The normalization is restricted to parameter checks and does not relax
ordinary object or nested-array identity checks.

### Function parameter identity checks

A parameter written with function type, directly or through a typedef, is
adjusted to a pointer to that exact function type; preserved-spelling identity
checks undo only this outer adjustment and still check the function signature.
Both sealed oracles print `7 7 7` for the three spellings in
`.evidence/builtin-probes/20260906-function-parameters/parameter.c` and
`parameter.out` (output SHA-256
`69d15060f5a52403979ee890fa0e7ccd967e5ae9dc5483cf000b14974df57588`).
No diagnostic is added: this removes the E3101 refusal affecting 2,464 units
at `blockdev_direct_IO` in sealed lowering run `20260905T175021Z-94287`.

### Narrow bit-field integer promotions

Integer promotions use a bit-field's range even when its declared base is
`long` or `long long`: widths below 32 and signed width 32 promote to `int`,
and unsigned width 32 promotes to `unsigned int`; this does not change the
field's storage layout or introduce promotion in a non-arithmetic context.
Both sealed oracles execute `tests/fixtures/bitfield-integer-promotions.c`
with exit 0 (evidence `.evidence/builtin-probes/20260907-bitfield-promotions/oracle.txt`).
No diagnostic is added; one known torture failure, `bf-sign-2`, exercises
this rule, and no full-campaign improvement is claimed before replay.

### Nonnegative enumeration objects and integer constants

A non-packed enumeration whose values are all in `0..UINT_MAX` uses
`unsigned int` as its object representation; a range including a negative
value and fitting `int` uses `int`, including when either is a bit-field base.
Enumeration constants whose entire range fits `int` retain type `int`, not
the enum object's unsigned mode, and the existing wider-range rule is unchanged.
Both sealed oracles compile and run `tests/fixtures/enum-bitfield-signedness.c`
at `-O0` and `-O2` with exit 0, recorded in
`.evidence/builtin-probes/20260907-bitfield-promotions/enum-oracle.txt`.
No diagnostic is added; the reduced field from torture `20030714-1.c`
accounts for one known failure, with full-campaign impact still unmeasured.

### Named stack parameters conflict with the variadic starting address

With nine named `long` parameters, the first anonymous argument follows a
named stack slot, whereas EvoMIR Section 6 fixes `va_start.stack` at
`x29 + 16` without that displacement.
Both sealed oracles and both IR interpreters return zero for
`.evidence/builtin-probes/20260907-variadic-stack/probe.c`, but EvoCC's
native code returns one (`results.txt` in that directory).
The body rule takes precedence, so this entry records the required
correction without silently overriding Section 6 or allocating a refusal
code for a valid scalar variadic call.
One minimized case reproduces torture `va-arg-1.c`; additional affected
unit counts await the next full run.

### Hosted library builtin prototypes

`__builtin_printf`, `__builtin_malloc`, and `__builtin_strcpy` use the
ordinary external prototypes `int printf(const char *, ...)`,
`void *malloc(size_t)`, and `char *strcpy(char *, const char *)` through
the existing unit-local builtin symbol mechanism, with default promotions
for `printf`'s anonymous arguments and no new intrinsic or library executor.
Existing checks remain `E2909` for arity, `E2913` for incompatible arguments,
and `E2927` for an incompatible visible function declaration; unsupported
external execution still follows Sections 3.10 and 5 rather than fabricating a result.
Both sealed oracles execute `tests/fixtures/hosted-library-builtins.c` at
`-O0` and `-O2`, print `abc 7\n` and exit zero, recorded in
`.evidence/builtin-probes/20260907-library-builtins/oracle.txt`.
Torture run `20260907T093504Z` has respectively 11, 7, and 5 first-error
refusals at these spellings; this entry does not predict how many become clean.
