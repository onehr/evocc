# EvoIR Specification

**Revision 2.32.0** | **Pinned ground truth** | Darwin NEON header subset on 2026-10-24

> EvoIR is a typed, contextual equality substrate over explicit state. It is the
> single semantic contract of the parser, the C lowering, equality saturation,
> the checker and the backend. A transformation that does not respect this
> specification is unsound, whatever its measured results.

## Status and scope

**Normative and informative text.** Sections 1 to 11 and Appendices A and B
are normative. In those parts, a requirement uses "must" or "must not", and
"can" states a permission. The following text is informative: paragraphs and
list items that start with "Note", "Example", "Rationale" or "Informative";
Sections 8.1 to 8.4 and 10.2 and the fact table of Section 7.2, which
describe a design that no code of this revision applies; Section 10.3, which
summarizes rules that other sections state; the worked examples
of Sections 9.6 to 9.8 and other code blocks that are introduced as examples;
and references to tests, fixtures, measurements and reference-compiler
behavior, which identify conformance evidence and do not add requirements.

**Formal semantics of `core-v1`.** For the checked subset `core-v1`, the Lean
definitions in `lean/EvoIR/Core/` are the formal semantics of EvoIR. The prose
of this document is a reading of those definitions. Where the prose and the
Lean definitions differ, the Lean definitions win, and the difference is a
defect in this document. Outside `core-v1`, this document is the authority.

**Machine-read parts.** Code reads some parts of this document. These parts
must keep their shape:

| Part | Reader |
|---|---|
| Rule registries `p0.scalar.v1` to `p0.scalar.v4` and `g0.ub.v1` (Section 11.6) | Byte-pinned by the SHA-256 digest printed beside each block; `crates/evocc-eqopt/tests/registry_digests.rs`, `tools/lean/registry_to_lean.py`, `tools/lean/audit.py` |
| Diagnostic table (Section 6.3) | `crates/evocc/build.rs`, `crates/evocc/tests/check_product.rs` |
| Grammar productions and shape blocks (Sections 9.3, 9.3.1, 9.3.2) | `crates/evocc-evoir/tests/spec_grammar_contract.rs`, `tools/check-textual-grammar.py` |
| Revision line of this header | `tools/render-status.py`, `tools/lean/audit.py` |

**Related specifications.** `spec/verification.md` is the contract of the
checkers. `spec/evomir.md` specifies EvoMIR, the IR that follows EvoIR.

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
- [Appendix A: Instruction Quick Reference](#appendix-a-instruction-quick-reference)
- [Appendix B: Type Grammar (Formal)](#appendix-b-type-grammar-formal)
- [Revision](#revision)

---

## 1. Type Universe

### 1.1 Type Grammar

Every EvoIR value has a statically known type drawn from the following grammar:

```text
Type ::=
    bits<N>          -- unsigned bit-vector of width N (arbitrary positive integer)
  | f16 | bf16      -- the half storage formats binary16 and bfloat16 (3.13.6)
  | f32 | f64 | f128 -- IEEE 754 binary32, binary64 and binary128
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
when they agree on the marker.

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

A pointer is a typed reference to either an object or a function. In the
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
cannot be dereferenced. Section 3.7 adds three provenance forms to this
record: code provenance (`block_addr`, `return_address`), exposed-address
provenance (`ptr.from_int`) and absent provenance (pointer poison).
Conversions between pointers and integers use `ptr.to_int` and `ptr.from_int`.

```text
ptr<generic>         -- default address space
ptr<global>          -- global/static storage
ptr<stack>           -- automatic storage
ptr<threadlocal>     -- thread-local storage
```

Address-space distinctions remain part of the general type grammar, but the
C lowering profile maps every object pointer to `ptr<generic>`. Storage
duration is object metadata, not a pointer type, because one C `T *` may point
to automatic, static, or string-literal storage over its lifetime. No
address-space spelling is a `NoAlias` proof.

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

The executable profile requires exactly `mem<all>`. Other region spellings
are reserved by the general type grammar. Region splitting and every
optimization that depends on `NoAlias` are outside this revision.

### 1.5 World Token

The `world` type is a singleton. The world token `%w: world` is a global
ordering token. The instruction layout threads it through every memory
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
| `f16`, `bf16` | always well-formed |
| `f32` | always well-formed |
| `f64` | always well-formed |
| `f128` | always well-formed |
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
ordered function table, followed by an ordered **module assembly table**: each
entry is one file-scope `asm("template")` of the unit, its template text
kept verbatim in the escaping of Section 9. Module assembly defines
nothing EvoIR can see: the interpreters skip it, and a symbol it defines
that C code uses is an external symbol under the rules below. The machine
lane emits every entry verbatim, in table order, after all functions and
data (EvoMIR Section 8); the reference compilers also place file-scope
assembly at their own discretion relative to functions, so an entry may not
depend on what surrounds it, only on the symbols it names. An
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
initializer is a **relocation**: the global carries a list
`relocs=[(offset, @symbol, addend), ...]`, each naming eight bytes at a
byte `offset` inside the object, a target and a signed byte `addend`.
The offset need not be a multiple of eight: packed fields retain their
source layout, as AArch64 ELF ABS64 data relocations allow. Installing a relocation uses byte alignment while preserving its
complete pointer representation and provenance; later loads/stores retain
their own checked access alignment rather than inheriting natural pointer
alignment from the relocation width.
A target is a symbol from any of the four tables (global, external object,
function, external function), or a block of a
function of this module written `@function^block` (for example
`@main^b3`), whose address is the one `block_addr` yields (Section 3.7):
this is how a static initializer holding `&&label` lowers, and the machine
emits it as the block's local symbol (see `spec/evomir.md`). Relocations are sorted by offset, do not overlap, and lie within
the size. At module load the eight bytes of each relocation hold the address
of the symbol plus the addend with that symbol's provenance (function
provenance for a function or external function), and are initialized;
`bytes=` for those positions are zero and `initialized=` bits there are one.
The list is omitted when empty. `(void *)(unsigned long)&f`, `&arr[3]` and
`&sv.l` are all symbol-plus-addend forms. A pointer initialized from an integer constant holds that integer's target
bits, after each intermediate cast's own width and signedness, and no
relocation. Loading such a complete initialized pointer-sized representation
decodes it as `ptr.from_int` does; known symbolic fragments keep their
relocations. Storing the bits exposes no object or function, and runtime byte
writes never create such a representation. A label difference (below) is a separate relocation form. Any other static
initializer that is not a symbol plus an addend, such as a pointer read through
an integer that is not a whole-pointer round trip, stays `E3026`
(`not-lowered: static-relocation`).
Top-level module execution starts from that deterministic declaration-order
memory snapshot and an empty world trace; callers do not inject arbitrary
initial memory or world states through the ABI-shaped entry token positions.

**Exposing relocations.** A static numeric pointer offset keeps pointee
scaling and every intermediate integer cast, and a pointer-width integer static
initializer can hold a symbol plus a byte addend. When a symbolic initializer
casts an address to an integer, including a later cast back to a pointer, its
relocation carries the fourth field `exposed`, `(offset, @symbol, addend,
exposed)`, which exposes the target (Section 3.7) at module initialization. The
three-field form exposes nothing. `exposed` changes neither the stored bytes
nor any access check. Raw numeric addresses expose nothing, and a dereference
of an integer-derived pointer to an unexposed object is `E3013`.

**Label differences.** The difference of two labels of one function is a
static integer initializer, not an integer constant expression. Its relocation
is `(offset, @f^bT, addend, diff=@f^bB, bits=W)` with `W` one of 8, 16, 32 and
64. Unlike the eight-byte form, it covers `W/8` bytes, under the same sorting
and nonoverlap rules, and holds the low `W` bits of the target address minus
the base address plus the addend, initialized and without provenance. Both
labels must name retained blocks of the same defined function, and both count
as address-taken for `br.indirect` validation. A label difference cannot be
`exposed`. The interpreters compute it from their code-address mapping. GNU
`void *` byte arithmetic can carry an exposed object or code address, but it
never makes a label dereferenceable.

**Object placement.** A global object can carry ` section="name"` after its
initialization bitmap and `readonly` mark and before `relocs`, in the
quoted-string encoding. It is placement metadata: object identity, layout,
initialization and relocations are unchanged. On the GNU/Linux target an
explicit writable section is emitted as a named PROGBITS section even for zero
bytes, except `.bss` and `.bss.*`, which are NOBITS and must hold no nonzero
byte and no relocation. Without the field the ordinary data or BSS choice
applies. An empty name, a control byte in the name, conflicting section
requests and a nonzero initializer in a NOBITS section are `E2943`.

**Load-instance metadata.** After the module assembly table, the module
carries a TLS-symbol table and an ordered lifecycle-registration
table. TLS metadata names a global template, external TLS object, or an alias
of another TLS symbol, and one of `global_dynamic`, `local_dynamic`,
`initial_exec`, `local_exec`; duplicate/missing/non-object entries are invalid.
A TLS template is not an ordinary process-global allocation. Static relocations
may not target TLS, including through an alias. Source TLS-model conflicts
remain E2943. Thread storage is independent of linkage and ELF visibility.

Lifecycle entries contain phase (`init`/`fini`), a defined zero-argument void
function, and priority 101..65535 (default 65535). Redeclarations contribute
one registration per phase; a declaration without a definition registers
nothing. Registration order is deterministic first-registration order, not
a cross-TU source-order guarantee. Entries are retention roots. Program mode
initializes objects/relocations, runs constructors by ascending priority,
invokes main, then runs finalizers in reverse registration/priority order,
sharing memory, effects and one execution budget. Normal return and `exit`
finalize; `_Exit`, `quick_exit` and `abort` do not run these registrations.
Ordinary function-call APIs do not initialize/finalize a load instance.
Native CRT/loader owns executable/DSO lifetime: this does not prescribe that
musl `dlclose` unloads, or simulate pthread/dlopen in a single-module interpreter.

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
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    ret %v0, %v1, %v2
}
```

The function signature **explicitly carries** memory and world tokens so effects
are threaded through the call graph. Every function that may read or write
memory, or that may call such functions, must accept and return `mem<R>` and
`world` tokens. For every such call/effect participant, the canonical
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
effects unless the obligation guaranteeing non-trapping dominates that
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
lowering must first assert `shift_count_defined`. When lowering knows a
compile-time amount in `[0, N)`, it may give that predicate the canonical
`const.bits<1> 1` result; checked MIR then omits the branch under 8.12/9.4.
A compile-time amount outside the range and a nonconstant amount keep the
ordinary guard. The GNU profile asserts nothing else for a signed left shift
(Section 6.1). `signed_shl_defined`
remains in the grammar, and a module can use it.

`lshr`: logical shift right. Zeros fill the vacated high bits.
C lowering must first assert `shift_count_defined`.

`ashr`: arithmetic shift right. The sign bit (bit `N-1` of `%a`) fills the
vacated high bits. C lowering must first assert `shift_count_defined`.

The raw shift instructions are total so the interpreter cannot panic on a
malformed or unguarded path. For an amount at least `N`, `shl` and `lshr`
produce zero; `ashr` produces all zeroes or all ones according to the sign bit.
Those fallback values are not C semantics: a valid C lowering dominates the
instruction with the typed invalid-count assumption.

**Bit counting and byte swap.**

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
is `ctpop`; `__builtin_bswap16/32/64` are `bswap`. Example values: `clz(1u) = 31`, `ctz(8u) = 3`, `clzll(1ull) = 63`, `popcountll(~0ull) = 64`,
`ffs(0) = 0`, `ffs(16) = 5`, `bswap32(0x11223344) = 0x44332211`,
`bswap16(0x1234) = 0x3412`.

### 3.5 Comparisons

```
%v = icmp.eq %a, %b
%v = icmp.ne %a, %b
%v = icmp.slt %a, %b
%v = icmp.sle %a, %b
%v = icmp.ult %a, %b
%v = icmp.ule %a, %b
```

| Field | Description |
|---|---|
| `%a`, `%b` | For `eq`/`ne`, identical `bits<N>`, `ptr<AS>`, or `fnptr<Sig>` types. For ordered comparisons, `bits<N>` operands of the same width; an ordered comparison of two pointers is written by the front end as `ptr.to_int` of each followed by `icmp.ult`/`icmp.ule` on the addresses; C's same-array obligation is carried by the C-HIR interpreter, as for pointer difference (10.1.3). |

**Semantics.** Integer comparison producing a `bits<1>` value: `1` if the
condition is true, `0` otherwise.

`eq` / `ne`: bitwise equality / inequality for integers. Pointer equality
compares the abstract addresses of the two operands (Section 5.3), not their
provenance. Two pointers into one object compare equal exactly when their
byte offsets are equal, and a pointer from `ptr.from_int` compares by its
integer address. Function pointers compare equal exactly when both are null
or both name the same function symbol. No instruction compares pointers by
order.
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
| `size=N` | Access size in `1..=i64::MAX` bytes; `alloca` allocation size in `0..=i64::MAX` bytes. |
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

`(%p, %m1, %w1) = alloca.dynamic %m0, %w0, %size [origin=O, align=A]`
allocates a fresh uninitialized automatic object of `%size` bytes, read as an
unsigned `bits<64>`, at the nonzero power-of-two alignment `A`. Origin
uniqueness, fresh `ObjectId`s, token threading and effects are those of
`alloca`. The object lives until a `lifetime.end` on it or the function's
return, and a later access is `E3010`. It lowers `__builtin_alloca` (10.1.14)
and variable-length arrays (10.1.19), takes no part in pure equality regions,
and guarantees nothing about stack overflow.

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
both provenance and byte offset. No instruction orders or subtracts pointers;
C lowering writes those operations over `ptr.to_int` (3.5, 10.1.3).

The validator requires every memory guard and memory operation to use
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
excludes them as it excludes every poison value.

**Label addresses.**

```text
%p = block_addr ^label : ptr<generic>
```

`block_addr` yields the address of the first instruction of block `^label`
of the current function as a `ptr<generic>` with *code* provenance: two
`block_addr` of the same block are equal, of different blocks unequal, and
a load, store or `access_in_bounds` through one is `E3013`. It is pure,
consumes no tokens, is not in the Section 8.5 allowlist, and `ptr.to_int`
of it is an ordinary integer that names no object. It is the lowering of
GNU `&&label`. A jump to it uses `br.indirect` (3.9).
The label identifies a block, not a closure: each activation supplies its
own local-object addresses through that branch's explicit edge arguments.

**Activation addresses.**

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
the interpreters give each activation a fresh value for each. They are the
lowering of `__builtin_return_address(0)` and `__builtin_frame_address(0)`
(10.1.17). `%p = frame_address depth=N : ptr<generic>`, with `N` decimal 1 to
65535 and no leading zero, yields the frame record reached by following `N`
saved frame-pointer links; native code follows the `x29` chain. It has the
effects of depth zero, and both interpreters report `E3031`
(`not-executed: frame-address`) when they reach it.

**Pointer and integer conversions.**

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
pointer. `ptr.from_int.fnptr<Sig>` is the function form:
a call through it resolves at that moment to the function whose address
the integer is (every function's address counts as exposed), and to no
function otherwise (`E3016`). A C conversion between an object pointer and
a function pointer, in either direction, is `ptr.to_int` followed by
`ptr.from_int` of the target type. A cast between pointers to different
function types is `ptr.to_int` followed by `ptr.from_int.fnptr<Sig>`: it keeps
the function identity and changes only the static call interface, and casting
back recovers the original interface, also after storage. A call through a
mismatched interface reports `E3019` only when the call is reached, after its
arguments are evaluated. `ptr.from_int` from a narrower integer
does not exist: the front end extends first with `sext` or `zext` by the
integer's signedness, so `(void *)(int)-1` is `0xffffffffffffffff` and
`(void *)(unsigned)-1` is `0xffffffff`. A conversion to a narrower integer is
`ptr.to_int` then `trunc`, which keeps the low bits of the address. A null
pointer constant converted to a pointer type is not this conversion. Neither form is in the Section 8.5 allowlist and
no rewrite family may introduce, remove or reorder one; the machine lowers
both to nothing (see `spec/evomir.md`). The e-graph's memory profile, when it exists,
must treat every `ptr.to_int` as an escape of its object. Provenance through
integers is thereby PNVI-style: address arithmetic on integers is ordinary
`bits<64>` arithmetic, and only the dereference asks which object is meant.

### 3.8 Memory Operations

```text
(%v, %m1, %w1) = load %m0, %w0, %ptr : Type
(%m1, %w1) = store %m0, %w0, %ptr, %val
(%v, %m1, %w1) = load volatile %m0, %w0, %ptr : Type
(%m1, %w1) = store volatile %m0, %w0, %ptr, %val
(%v, %m1, %w1) = load %m0, %w0, %ptr : bits<N> [mask=M]
(%m1, %w1) = store %m0, %w0, %ptr, %val [mask=M]
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
| `mask=M` | Optional bit mask of a `bits<N>` access, volatile or not, with `N` 8, 16, 32, 64 or 128. Bit `i` of `M` selects bit `i` of the value. `M` must leave at least one bit unselected: an access of every bit has no mask. |

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
together with each bit's initializedness: an uninitialized source bit makes
the destination bit uninitialized and reports nothing (Section 5 already
lets a copy move uninitialized bytes; only a scalar `load` of one reports
`E3001`). Stored pointer bytes keep their provenance representation. The
two ranges must be identical or disjoint; any other overlap reports `E3027`.
Access checks are those of `store` on both ranges (`E3009`-`E3013`). `copy`
is the lowering of every C aggregate copy: assignment, copy-initialization,
argument passing and return by value of a record or union (10.1.3); no other
source form produces it, and no EqSat rewrite family may introduce, remove or
reorder one. The bounded C lowering in Section 10.1.3 can replace a complete,
initialized private integer-cell copy. Ordinary `copy` is not volatile. Exact-width whole-union access
uses `copy.volatile.read`, `copy.volatile.write` or `copy.volatile.readwrite`
with the same operands and results: these require N in {1,2,4,8} and A >= N,
emit one source read and/or one destination write respectively (read first),
and transfer the same representation metadata without interpreting padding.
Each observable event records its width and the bytes' initializedness, a
byte counting as initialized when all eight of its bits are; even
an identical-range copy retains the requested events. No atomicity or MMIO
contract is implied. Larger/misaligned unions remain E2922; ordinary volatile
record member access follows 10.1.3.

**Semantics.**

`load(%m0, %w0, %ptr)`: returns the value of type `Type` stored at address
`%ptr`, plus explicitly threaded `%m1` and `%w1` tokens. `%m1` denotes the
same immutable memory snapshot as `%m0`. The load fails with `E3001` when any
requested bit is uninitialized, and otherwise uses the exact pointer,
lifetime, bounds, and alignment classification from Section 6.1.

`store(%m0, %w0, %ptr, %val)`: returns a new memory token `%m1` that is
identical to `%m0` except that the value at address `%ptr` has been replaced
with `%val` and the written bits are initialized, plus the explicitly
threaded `%w1`. Storing and later loading a pointer preserves its complete
provenance and offset; a partial non-pointer overwrite of its object
representation makes a later pointer load fail with `E3013`.

**Masked accesses.** `load ... [mask=M]` reports `E3001` only when a bit
that `M` selects is uninitialized. Each bit of its result is the bit that
memory holds. A bit that `M` does not select can be uninitialized; its value
is then one that the program never stored. `store ... [mask=M]` writes every
bit of `%val`. Afterwards the bits that `M` selects are initialized, and every
other bit keeps its initializedness. Both forms keep every other check of the
unmasked forms, and a masked store still makes a later pointer load of the
bytes it writes fail with `E3013`. A volatile masked access is one volatile
event, as the unmasked form is. Its event records the bytes that the access
transfers and, when one of them is not completely initialized, the
initializedness of each byte; a byte counts as initialized when all eight of
its bits are. A mask changes no value, so native code emits the unmasked
access. The only producer is the lowering of a bit-field access (10.1.6).

For an ordinary access, `%w1` denotes the unchanged world state through a
fresh SSA identity. A successful volatile access appends exactly one ordered
load or store event, including pointer, width, and accessed value, to the world
trace represented by `%w1`. A failing volatile access appends no event. A
volatile load does not alter the memory snapshot, and volatile state is never
stored inside a memory token.

**Scalar atomic accesses.**

```text
(%v, %m1, %w1) = atomic.load acquire volatile %m0, %w0, %ptr : Type
(%m1, %w1) = atomic.store release %m0, %w0, %ptr, %val
```

The order is required; `volatile` is optional and independent. `Type` is
`bits<1|8|16|32|64>` or an object/function pointer; size is respectively
1/1/2/4/8 or 8 bytes. Each access requires the immediately applicable
`access_in_bounds` guard with that exact size and natural alignment, the
current memory/world tokens, and produces fresh memory/world tokens. Loads
permit `relaxed`, `consume`, `acquire`, `seq_cst`; stores permit `relaxed`,
`release`, `seq_cst`. Invalid type/order/guard layout is `E3062`
`invalid-atomic-access`; existing local-SSA/effect-token checks also apply.
Memory faults and pointer initializedness/provenance use the ordinary access
rules: an atomic operation does not create provenance from untracked bytes.

In concurrent native execution each atomic object has a total modification
order consistent with happens-before; a load reads a write allowed by the
C11 atomic coherence rules. A release store synchronizes with an acquire
load that reads it (consume is implemented as acquire). Sequentially
consistent operations additionally participate in the C11 SC total order.
Atomic accesses use naturally aligned ordinary shared Normal memory. Section
10.1.21 maps C `_Atomic` objects onto these accesses; no generic aggregate,
128-bit or MMIO support is implied.

**Atomic read-modify-write.**
`(%old, %m1, %w1) = atomic.rmw.<op> <order> [volatile] %m0, %w0, %ptr, %value [, %expected] : T`
has `<op>` in `add`, `sub`, `and`, `or`, `xor`, `nand`, `exchange` and
`cmpxchg`. `T` is `bits<8|16|32|64>` or a pointer, and `bits<1>` for
`exchange` and `cmpxchg` only. A function pointer admits only `exchange` and
`cmpxchg`. An object pointer admits those two and `add` and `sub` with a
`bits<64>` `%value`: a byte count, never scaled by the pointee size. Otherwise
`%value`, `%expected` and the result have type `T`, and only `cmpxchg` takes
`%expected`. The guard, size and natural alignment rules are those of
`atomic.load`; a `bits<1>` access is one byte. The operation returns the old
value and fresh memory/world tokens. Arithmetic wraps to the width of `T`;
`exchange` and `cmpxchg` keep a pointer's complete representation and
provenance. A pointer `add` or `sub` keeps the old pointer's provenance and
moves its address by the byte count. As C11 7.17.7.5p3 says for address
types, the result can be an address that no access may use, but the operation
itself is defined; from a null or integer-derived pointer it gives an
integer-derived address. `cmpxchg` stores `%value` only when the old value
equals `%expected`, and a failed comparison stores nothing. Invalid operands,
type or order are `E3062`. These operations are outside P0; native code uses
a width-correct exclusive load/store loop that keeps the `cmpxchg` failure
path.

**Atomic object copies.**

```text
(%m1, %w1) = atomic.copy.load <order> [volatile] %m0, %w0, %ptr, %dst [size=N, align=A]
(%m1, %w1) = atomic.copy.store <order> [volatile] %m0, %w0, %ptr, %src [size=N, align=A]
(%m1, %w1) = atomic.copy.exchange <order> [volatile] %m0, %w0, %ptr, %src, %dst [size=N, align=A]
(%ok, %m1, %w1) = atomic.copy.cmpxchg <order> [volatile] %m0, %w0, %ptr, %expected, %desired, %observed [size=N, align=A]
```

These forms access the `N`-byte object at `%ptr` as one atomic object and
move its representation to or from ordinary buffers. `N` is 1, 2, 4 or 8. The
object access needs the immediately applicable `access_in_bounds` guard with
size `N` and alignment `N`, as `atomic.load` does. Every buffer is a
`ptr<AS>` range of `N` bytes at alignment `A`, with `A` a power of two no
greater than `N`; the buffers use the access checks of `copy`. A load admits
the orders of `atomic.load`, a store those of `atomic.store`, and an exchange
or a compare-exchange every order. Every transfer moves the object
representation as `copy` does: each bit keeps its initializedness, and
stored pointer bytes keep their provenance. `atomic.copy.load` moves the
object to `%dst`. `atomic.copy.store` moves `%src` to the object.
`atomic.copy.exchange` moves `%src` to the object and the old object to
`%dst`. `atomic.copy.cmpxchg` reads every byte of the object and of
`%expected`, so an uninitialized byte is `E3001`; pointer bytes compare by
address, without their provenance. When all bytes are equal it moves
`%desired` to the object, and `%ok` is 1; otherwise the object is unchanged
and `%ok` is 0. In both cases the old object moves to `%observed`, and
`%expected` is not written. Each form appends one atomic event with the old
and the new representation. Invalid size, alignment, order, buffer count or
guard is `E3062`. These operations are outside P0. Native code performs one
access of the object through a general register, with `LDAR`/`STLR` or the
exclusive loop of `atomic.rmw`, and ordinary loads and stores of the buffers.

`world` orders execution/scheduling within one thread; it does not establish
cross-thread sequential consistency. The three interpreters implement only
single-thread values and ordered events, not a concurrent memory-model
checker. Each successful access appends one immutable event containing
direction, object origin/offset, access layout, order, volatile flag and
observed bits/provenance; atomic and volatile events share one sequence and
resource budget. A volatile atomic has one atomic event, not a second access.
These operations remain outside P0; ordinary forwarding, merging, splitting
or speculative-store rewrites may not erase or change their atomic/order
metadata. Independent proof reconstruction preserves that metadata.

**TLS address acquisition.**
`(%p, %m1, %w1) = tls_addr @symbol, %m0, %w0` returns `ptr<generic>`.
The symbol must have TLS metadata. Current mem/world tokens are required,
fresh tokens are returned, and normal local SSA applies. The immutable memory
state maps `(origin, logical ThreadId)` to one ordinary Object instance,
initialized from the template/relocations only on first acquisition. Aliases
use that same origin. The returned pointer retains the owner's identity when
passed to another thread; dereference never rebinds to the accessing thread.
Ending a thread ends its instances; logical thread IDs are not recycled.
Acquisition does not inspect stored bytes or append a volatile event. It may
allocate/fail, reads/writes memory state, and is excluded from P0. Unresolved
external TLS remains an external-object execution error, not a fabricated zero.

**Vector representations.** An integer vector `vec<L, bits<N>>` has positive
power-of-two L and N in 8/16/32/64/128. A floating vector `vec<L, f32>` or
`vec<L, f64>` has positive power-of-two L; its lanes are ordinary `f32` or
`f64` values. `vec.pack` takes L operands of the lane type and preserves each
lane's initializedness. `vec.extract` takes a vector and integer index and
returns a value of the lane type; it checks the entire index before narrowing
(E3011 when out of range), and consumes only the selected lane's
initializedness. `vec.bitcast` changes equal-sized integer, floating and vector
representations without initializing bytes or numerically converting lanes.
A pack, an extract and a bitcast move bits: they never quiet a signaling NaN
or change a payload. These have one result and no effects; extract may trap.
Vector arithmetic is expanded lane by lane through the existing checked
scalar rules, the floating ones of 3.13 included. No vector operation is added
to the P0 allowlist; proof reconstruction retains all fields. Vector
loads/stores preserve lane representation, rather than zero-filling
uninitialized lanes. A scalar lane use still observes existing poison rules.

**AES rounds.** `vec.aese` and `vec.aesd` take a data operand and a key
operand; `vec.aesmc` and `vec.aesimc` take one operand. Every operand and the
result are `vec<16, bits<8>>`. `aese`/`aesd` XOR data and key, then apply the
forward/inverse AES byte substitution and row permutation; `aesmc`/`aesimc`
apply the forward/inverse AES column mix in GF(2^8). They are pure, have no
memory, world or provenance effect, and make a result byte indeterminate only
when an input byte it depends on is indeterminate. They are outside P0, and
text and independent proof reconstruction keep the exact opcode and operands.
Native emission requires the AES extension; `-mgeneral-regs-only` refuses them.

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

```text
br.indirect %target [^b1(args...), ^b2(args...)]
```

**Indirect edge binding.** `%target` is a
current-block `ptr<generic>` value. The instruction has no results and
transfers control to the block identified by its code address in this
function. It selects the edge first, then simultaneously binds that edge's
arguments to the destination's Value IDs. It performs no memory access,
allocation, lifetime action or volatile event. An initialized value that
does not identify a listed local label reports `E3048`; a poison target
retains `E3013`. Neither error is a function-call diagnostic.

The list is nonempty, sorted by BlockId and contains each destination once.
Module validation requires exactly all retained address-taken blocks of the
function, including `block_addr` and static `@function^block` relocations.
Function-only validation checks its locally observable required subset;
it is not a substitute for module validation. Every argument obeys the
unchanged local use-def rule and matches the destination position's type.
Each edge passes each current memory/world token exactly once. Repeating
that token on another mutually exclusive edge is allowed; stale, duplicate,
missing or incorrectly positioned tokens are not. Direct edges, including
two `cond_br` alternatives to the same block, retain separate bindings.

The C lowering profile carries ordinary live-across data in existing local
objects; indirect entries receive their required **storage addresses** and
effect tokens. `Address(p)` denotes the object storing `p`, not the pointer
value currently held by `p`. This role is constructed from the lowering
symbol/object map, not inferred by a generic validator from pointer types,
names or producer flags. Only live-across values need storage; captured type
bounds also count as live. Passing an address does not read its object's
contents or initialize it. Accesses retain lifetime/initializedness checks,
dynamic allocations retain their specified lifetime, and prohibited entry
into a captured variably modified scope remains `E2945`. For `goto *expr` the
check uses the function's complete address-taken successor set: if any member
bypasses a captured variably modified declaration, the indirect goto reports
`E2945`, whichever label its operand names. Entering an already active capture
scope is allowed; its hidden dimension object's address travels with the other
live storage addresses.

Scope-specific allocation or cleanup must run only on the selected path.
An implementation may route the actual target to edge adapters before doing
those actions; merely retargeting CFG metadata does not route a code address.
The backend must preserve label/relocation identity and implement the complete
positional parallel copy, including copy cycles, spills and the target's
liveness (EvoMIR Section 7). No implicit frame-address or effect channel is
introduced. All successors participate in Sections 8/11 taint, exclusion,
known-bits and proof reconstruction; the terminator is not added to the
EqSat allowlist. Cloning/inlining must preserve the owner/label correspondence
or retain the original function.

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

```text
(%ret_vals..., %m1, %w1) = call.returns_twice @callee(args..., %m0, %w0)
```

**Calls that return twice.** The mark `.returns_twice` makes a direct call
one that returns twice (10.1.9). After the call returns, a later jump can
resume its continuation once more, with new results: `longjmp` resumes
`setjmp`, `setcontext` resumes `getcontext`, and the parent resumes after
`vfork`. Only a direct call carries the mark. An indirect call is an
ordinary call. In every execution that this specification defines, the call
has the semantics of `call`. Only external code makes a second return, and
the interpreters refuse an external callee (`E3039`), so they never run one.

The mark is a contract with the machine. A second return resumes the
continuation with the memory as the jump found it and with no register value
of the caller. The machine therefore keeps every value that is live across
the call in a stack slot that no other value writes at any point that the
call reaches, and reloads the value after the call (EvoMIR Section 7.4). An
automatic object that is not volatile and changed between the call and the
jump is indeterminate after the second return (C11 7.13.2.1p3). The call
takes the top effect summary (4.6) whatever its callee is, because the second
return follows code that the call does not bound. No transformation duplicates, merges, removes or reorders a call that
returns twice (4.4). A call that returns twice is outside `core-v1`.

**Anonymous arguments.** When the callee's signature is
variadic, the arguments between the last listed parameter and `%m0` are
anonymous. Each has one of the types `bits<32>`, `bits<64>`, `bits<128>`,
`f64`, `f128`, `ptr<AS>` or `fnptr<Sig>`: these are the C types that survive the
default argument promotions (10.1.3), and the front end performs the
promotions, so `bits<8>` and `bits<16>` never appear here. There may be any
number of them, including none. Their types are not part of the signature and
are not checked against a declaration; they are recorded at the call and read
back by `va_arg`. A non-variadic signature admits no argument beyond its
list. Anonymous arguments carry no `mem<_>`, `world` or `tuple`.
An anonymous argument can also be a vector `vec<L, bits<N>>`, `vec<L, f32>`
or `vec<L, f64>`, without lane promotion, or a homogeneous-vector record with
explicit byval/HVA shape. Other
records and unions of scalar members pass in the form of 10.1.3, a
`ptr<generic>` to a caller-made copy, marked `%v byval(size, align)` with
`size` possibly zero and `align` a nonzero power of two. Only anonymous
operands carry this call-site mark, never fixed operands or tokens, and `sret`
appears only in signatures. The machine passes such a record of at most 16
bytes in its own AAPCS64 slots and a larger one as a pointer to a
caller-owned copy. Other aggregate shapes retain E3030 rather than becoming
ordinary pointer arguments.

Every function has `cc = target_default`. Module validation rejects another
calling-convention identifier. With one calling convention, the
parameter/result `Sig` is complete. A revision that adds a calling convention
must add it to `fnptr` identity first.

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
function-pointer layout, never from the host. For the AArch64 LP64 target
both are eight bytes/eight-byte alignment. This equality with the object-pointer
layout is a target fact, not permission for a direct `ptr<AS>`/`fnptr<Sig>`
conversion. The types remain distinct; a C conversion between them uses
`ptr.to_int` and `ptr.from_int` (3.7).

**Call effects.** A direct call takes the effect summary derived in Section 4.6
for its callee. An indirect call takes the top summary of Section 4.6, because
the callee is not known statically.

### 3.11 Inline Assembly

An `asm` instruction carries a template the assembler will read and the
operands it substitutes into it. It is **opaque**: no rule reads the
template, no analysis interprets it, and the equality database never merges
it with anything. It lets assembly pass through the compiler without any part
of the compiler interpreting it.

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
with the next source statement. For an ordinary unaddressed automatic scalar,
writeback updates its existing local SSA binding; a register constraint does
not itself require an addressable object or a store. When the destination
requires object storage (including addressed or volatile objects), its stores
consume the assembly's new tokens, not its input tokens. `+` preserves its
incoming value on an execution
path where the template does not write it; a write-only output makes no such
promise. The compiler does not inspect assembler instructions to infer which
path writes an output. Label entries retain the source label name alongside
the target block and edge arguments, so `%l[name]` does not depend on block
numbering.

**Semantics.** The results are values of their declared types about which
nothing is known: an interpreter cannot execute an `asm` and reports `E3021`
when it reaches one, so a program containing one is verified by the native
run of the EvoMIR specification's Section 9 and by nothing else.

**Constraints and modifiers.** The admitted constraint vocabulary is:

| Constraint | Meaning |
|---|---|
| `r`, `=r`, `+r` | a general register, read, written, or both |
| `rm`, `mr`, `=rm`, `=mr`, `+rm`, `+mr`, `=&rm`, `=&mr`, `+&rm`, `+&mr` (source only) | a register or memory alternative; lowering selects the existing register form, preserving read/write and early-clobber markers |
| `=&r`, `+&r` | written, and not sharing a register with any input (early clobber) |
| `rZ` | a general register, or the zero register when the operand is the constant zero; the template must spell it with a width modifier, since the zero prints as `xzr`/`wzr` only under `%x`/`%w` |
| `Q`, `=Q`, `+Q`, `Qo` | a memory operand whose address is a register with no offset |
| a digit `0` to `9` | this operand occupies the same location as the output that digit names (a tied operand) |
| `I`, `J` | an immediate the add or subtract forms accept, and its negation |
| `Ir`, `Jr`, `Kr`, `Lr`, `ir`, `nr` (source input, either letter order) | an admitted immediate letter or a general register; lowering selects the `r` alternative |
| `K`, `L` | an immediate the 32-bit and 64-bit logical forms accept |
| `i` | an integer constant or a link-time symbol plus a signed byte addend |
| `n` | an integer constant only |
| `p` | an address |
| `m`, `=m`, `+m` (source only) | a memory operand; lowering selects the address-only `Q`, `=Q` or `+Q` form |
| `g` (source input), `=g`, `+g`, `=&g`, `+&g` (source only) | a general operand; lowering selects the corresponding `r` form, preserving read/write and early-clobber markers |
| `w`, `=w`, `+w`, `=&w`, `+&w` | the SIMD register bank: 64-bit and 128-bit integer and polynomial vectors, and scalar `f32`/`f64` |

**Scalar floating assembly.** The `w` family carries the
exact `f32`/`f64` representation; it does not perform a floating conversion or
quiet a NaN during operand transport. Matching inputs have the output's exact
type and physical location. Input/place evaluation, read-write inputs,
early-clobber exclusion and selected-edge output binding use the existing
assembly rules. Unmodified SIMD references print `vN`, including scalar
operands; `%b/%h/%s/%d/%q` select a view of that same register, not another bank
or an implicit conversion. A template is responsible for using an instruction
and view appropriate to its data; selecting a wider view does not define the
unprovided high bits of a scalar input. General-register modifiers on `w`
operands remain invalid. Existing `vN/dN/sN` clobber aliases and local `@vN`
bindings keep their scope. `f128`, floating vectors, integer vectors other than
64 or 128 bits, half formats and SVE are not admitted under `w` (`E3021`), and
`r` binds no floating or vector operand. An early-clobber `w` output shares no
register with an independent input or another output. Register allocation,
spills, selected-edge writeback and the independent allocation and proof
checks preserve tied and early-clobber placement.

Assembly keeps the effect summary of Section 4.2 regardless of `volatile`: it
reads and writes `all`, and `volatile`, `io`, `fenv` and `may_trap` are true;
`atomic` and `may_unwind` are false. It can change FP controls/status; native
admission knowledge must be invalidated across it before subsequent baseline
or strict arithmetic.
The compiler never resets FPCR/FPSR to accommodate it. Both interpreters still
report E3021 upon reaching opaque assembly. Native execution, allocation
verification and proof reconstruction are required; type admission alone is
not implementation acceptance. `-mgeneral-regs-only` still rejects the generated
SIMD accesses needed to bind these operands.

For a source input whose constraint is exactly one of the admitted immediate
letters `I`, `J`, `K`, `L`, `i`, `n` together with `r`, in either order,
C-HIR preserves the
written constraint and lowering evaluates the operand exactly once, then
records the selected `r` constraint in the EvoIR `asm` or `asm.goto`. The
register alternative is legal for every value the immediate alternative
admits, so selecting it never changes what the template means, and it applies
to constants and dynamic values alike with the same operand types and template
modifiers as `r`. None of these is admitted as an output constraint or as an
unresolved EvoIR constraint, and no new backend constraint is required.

**Symbolic immediates.** An `i` input may name a static
object, external object, function or function-local label, with a signed
64-bit constant byte addend. It is represented by the existing
`global_addr`/`func_addr`/`block_addr`, pointer casts, and constant
`add.bits<64>`/`sub.bits<64>` forms, not a new instruction or an invented
address. C lowering resolves the source address constant without evaluating
an operand with effects and materializes it in the assembly's own block after
the ordinary operands have been evaluated; numeric immediates follow the same
local-definition rule. A dynamic address or an address under `n` remains
`E3021` with the constraint named. The template modifier `%c0` or `%c[name]`
prints an `i` or numeric immediate without an immediate prefix; it cannot name
a register or memory operand. The machine emitter writes the symbol's target
spelling and signed addend, preserving linkage and block identity. A symbolic
`i` operand does not make user-written assembly position independent.

For the exact source `rm`/`mr` forms in the table, lowering
evaluates each input or output place once, selects `r`, and preserves `=`,
`+` and `&`; register type, binding, clobber and all outgoing-edge rules are
unchanged. C-HIR retains the written constraint, while EvoIR contains only
the selected constraint. This does not admit arbitrary memory constraints or
wider register values.

**Memory and general operands.** An `m` operand is evaluated once and causes no
implicit load or writeback. A memory operand must designate an addressable
object; a memory output can name an array but not a const subobject. Either
violation is `E2914`. A matching input wider than its register output supplies
the output's low bits through an explicit `trunc`, after `ptr.to_int` for a
pointer input. A matching input narrower than its output is refused, and tied
operands keep exactly equal EvoIR types.

**Fixed local registers.** A block-scope `register T x asm("reg")` used as an
operand keeps its binding as a canonical `@xN` suffix on that operand's quoted
constraint; source `wN` and `rN` name the same `xN`. The suffix is lowered
metadata, not a source constraint spelling. It applies only at the assembly
boundary, including tied and read-write operands, and an output is bound on
every outgoing `asm.goto` edge. `x30` can be bound; it stays out of general
allocation, and every function saves and restores its incoming link register.
A binding to a scratch, platform, frame, stack or other reserved register, or
one that conflicts with a clobber or another operand, is `E3021`; a binding is
never dropped.

**Small aggregate operands.** A 4- or 8-byte record or union under a
general-register constraint transfers its complete object representation as
`bits<32>` or `bits<64>`, not its address or a chosen union member. A register
input loads that representation; a register output follows the write-only and
read-write rules and writes every representation byte back on every outgoing
edge. Memory constraints keep their address-only behavior. A record or union of
any other size under a register constraint is `E3021`.

A constraint outside this table, or a constraint string combining letters
this table does not combine, is `E3021` with the constraint named. Clobbers:
`memory` (implied by the effects below and otherwise inert), `cc` (the flags),
and a named physical register, which the allocator must treat as written.
`v0` to `v31` (with their `dN` and `sN` aliases) name SIMD registers; a `vN`
clobber never reserves `xN`. `qN`, `hN` and `bN` clobbers and malformed
register names are `E3021`. Normal emission preserves the low 64 bits of a
clobbered `v8` to `v15` on every return, including `asm.goto` paths, with frame
and unwind locations; other SIMD clobbers need no callee save. It also saves a
clobbered `x19` to `x28` and restores it on every return, and the backend
refuses a clobbered `x29` (EvoMIR Section 6.7). Under
`-mgeneral-regs-only` the compiler generates no SIMD access for this purpose.
Template operand references: `%0` to `%9`, `%[name]`, a width modifier
`%w0`/`%x0` before the number or name, `%c0`/`%c[name]` for immediates,
`%l[label]` or `%l<N>` in an `asm.goto` (`N` counts the outputs, then the
inputs, then the labels, in that order, so the first label of an `asm.goto`
with `k` operands is `%l<k>`; an index outside the label range is `E3021`),
`%a0`/`%a[name]` for an address operand, and
`%%` for a literal percent; the SIMD views described above are also admitted;
any other reference is `E3021`.

**Address operands.** The constraint `p` is a general
register holding an address; lowering treats it exactly as `r` for a
`ptr<AS>` or `bits<64>` operand and C-HIR keeps the written letter. The
template modifier `%a` applies to a `p` or `r` input operand of pointer or
64-bit integer type and prints it as an AArch64 address operand, the 64-bit
register in brackets, `[x<n>]`; on any other operand class or type it is
`E3021` with the reference named. The operand is evaluated once. Example:
`prfm pldl1keep, %a0`.

**Effects.** An `asm` reads and writes `mem<all>` and `world`, may trap, and
may not be reordered against any other effectful instruction, duplicated,
or removed, whether or not it is `volatile`; `volatile` is preserved for the
assembler and changes nothing in EvoIR. Section 8.5's allowlist already makes
it ineligible as a region root or witness, so no rewrite can reach it.

### 3.12 Variadic Argument Access

```text
(%m1, %w1) = va_start %ap, %m0, %w0
(%v, %m1, %w1) = va_arg.Type %ap, %m0, %w0
(%m1, %w1) = va_end %ap, %m0, %w0
```

| Field | Description |
|---|---|
| `%ap` | A `ptr<AS>` to a **list object**, the lowering of one `__builtin_va_list` (10.1.5): 32 bytes on Linux, 8 on Darwin, alignment 8 on both targets (10.5). |
| `Type` | For `va_arg`: one of `bits<32>`, `bits<64>`, `bits<128>`, `f16`, `bf16`, `f64`, `f128`, `ptr<AS>`, `fnptr<Sig>` or a vector `vec<L, bits<N>>`, `vec<L, f32>` or `vec<L, f64>`, the promoted types of Section 3.10. A half format has no default argument promotion (C23 6.5.2.2), so it is one of them (3.13.6). |

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

`(%m1, %w1) = va_arg.byval %ap, %m0, %w0, %dst [size=N, align=A]` copies the
next anonymous record (3.10) into the allocated `ptr<generic>` `%dst`,
preserving initializedness and provenance, and advances the list as `va_arg`
does. The interpreters check the recorded record shape, which differs from a
scalar pointer argument, and report `E3038` on a mismatch or an invalid list.

**The list object's bytes.** Between `va_start` and `va_end` the object
representation of the list is unspecified. A `copy` (Section 3.8) of the
whole target-sized representation produces a second list denoting the same sequence at the same
position, which is how `__builtin_va_copy` lowers (10.1.3); any narrower
`load`, `store` or partial `copy` of those bytes is undefined, and the
interpreters may keep their own representation inside them.
The module's selected target fixes this size; the allocation's remaining
capacity must not determine it. This same layout bounds cursor access,
complete copies and overlap checks, including adjacent list subobjects.
`__builtin_va_list` is a 32-byte structure on Linux and an 8-byte pointer
representation on Darwin. A `va_list` passed
by value is a record parameter (10.1.3): the caller passes the address of a
copy, and the callee's `%ap` is that address.

**Effects and rewriting.** All three read and write `{All}` (Section 4.2):
the list object is memory, and the anonymous arguments are state no other
instruction can name. None is allowlisted (8.5), no rewrite family may
introduce, remove or reorder one, and the effect edges keep them in program
order, exactly as for `copy`.

**Target convention (informative; AArch64 Linux, general registers).** A
call site cannot tell a variadic prototype from a fixed one. An anonymous
`char` is promoted by the caller and occupies one 8-byte slot. After
`va_start` with `k` named integer parameters, the list object holds
`gr_offs = -8 * (8 - k)` and `vr_offs = 0`. The `va_arg` procedure is
EvoMIR's to state (EvoMIR Section 6); EvoIR only promises the sequence.

### 3.13 Scalar Floating Operations

The scalar floating profile admits binary32, binary64 and binary128 for the
Linux GNU target: RNE,
gradual underflow, source-format evaluation, no implicit contraction, and IEEE
exceptional arithmetic results rather than integer-style division/overflow UB.
An arithmetic NaN selects the first signaling operand (left before right),
otherwise the first quiet operand, preserving its sign/payload and setting its
quiet bit; invalid arithmetic without a NaN operand yields positive default NaN.
Format conversion quiets NaNs and transfers the highest payload bits and sign.
Same-format transport, constants, copies and loads/stores preserve every bit;
NaN is not poison, and numerical equality does not identify object representations.

```text
%v = const.f16 0x3c00
%v = const.bf16 0x3f80
%v = const.f32 0x3f800000
%v = const.f64 0x3ff0000000000000
%v = const.f128 0x3fff0000000000000000000000000000
%v = fp.OP %a, %b : T
%v = fp.OP %a : T
```

Constants have exactly 4/4/8/16/32 lowercase hexadecimal digits. Binary `add/sub/mul/div`
require matching FP operands/results; `oeq/une/olt/ole/ogt/oge` require matching FP
operands and return `bits<1>` (only `une` is true for unordered operands).
Unary `neg` preserves the format; `convert` changes admitted scalar format; `from_signed`
and `from_unsigned` convert full-width `bits<1..128>` to FP, rounding once.
`to_signed/to_unsigned` truncate toward zero into `bits<1..128>`; the raw IR
operation is total with zero for nonrepresentable input, but C lowering must
first emit `fp.to_int_defined.sN/uN %a : bits<1>` and
`ub.assume.floating_to_integer`, reporting **E3063** on false, including NaN
and infinity. `to_bits/from_bits` are exact representation conversions between
f16/bits<16>, bf16/bits<16>, f32/bits<32>, f64/bits<64> or f128/bits<128>, not
numerical casts. The half formats admit only constants, conversions and these
representation moves (3.13.6).

These baseline operations are pure and excluded from P0 roots/witnesses; all
operands still obey local SSA, initializedness, text/proof reconstruction and
existing call/edge rules. Native **E3064** denotes an incompatible FP environment,
not C UB; no hidden FPCR reset is permitted. Section 3.13.3 admits the bounded
non-trapping `STDC FENV_ACCESS` profile. Unsupported or malformed requests
retain **E3065**; unsupported strict/fast/contraction options are not ignored.
Section 3.13.5 admits the parts of a complex product and quotient, and
Section 10.1.14 applies these operations to each lane of a floating vector.
Section 3.13.1 specifies the strict IR.

The driver advertises source-format evaluation (`__FLT_EVAL_METHOD__=0`) but
does not advertise Annex F/IEC-60559 conformance, complex FP or fast FMA:
the corresponding `__STDC_IEC_*` and `__FP_FAST_FMA*` macros are absent and
`__GCC_IEC_559{,_COMPLEX}` is zero; this overrides the GCC predefined-macro
list of Section 10.5.
`-fexcess-precision=standard` explicitly selects this existing source-format
evaluation contract: each operation, assignment and cast rounds to its semantic
format, with no hidden excess-precision carrier; an operation on half operands
evaluates in binary32, as `FLT_EVAL_METHOD` 0 requires (3.13.6). It changes
neither the selected rounding/environment policy nor contraction, and
repetition is idempotent. It does not authorize `-ffast-math`,
`-fexcess-precision=fast/16` or any other unsupported profile. This is
equivalent option admission under `FLT_EVAL_METHOD=0`, not a new numerical rule.
For f32/f64/f128, `__builtin_isnan/isinf/isinf_sign/isfinite/isnormal/signbit` inspect
the exact representation; truth results are 0/1 except `isinf_sign`, which
returns -1/0/1. `fabs/fabsf/fabsl` clear the sign bit, and `copysign/copysignf/copysignl` copy it
after their prototype conversions, preserving payloads and signaling bits.
These builtin operands are evaluated once in source order; infinity/huge-value
builtins are exact constants. Ordinary library calls retain their external
effects and are not replaced by these builtins merely because names resemble.
With standard builtins enabled, a direct call of the external, non-renamed
function `fabs` or `fabsf` with its exact one-argument prototype uses the
corresponding builtin; static functions, function-pointer calls, other
prototypes, `-fno-builtin` and `-ffreestanding` keep ordinary call semantics.

`__builtin_fpclassify(nan, infinite, normal, subnormal, zero, x)` returns the
class argument that names the class of `x`, as GCC 14 documents it: NaN,
infinity, normal, subnormal or zero, in that argument order. The result has
type `int`. It inspects the representation of `x` in the type of `x`, with no
promotion, so a `float` subnormal is subnormal. The five class arguments must
be integer constant expressions, and each converts to `int`. `x` must have
type `float`, `double` or `long double`, and it is evaluated once. glibc's
`<math.h>` defines `fpclassify` with this builtin.

`__builtin_nanf`, `__builtin_nan` and `__builtin_nanl` with a string-literal
operand yield a quiet binary32/64/128 NaN whose low significand bits hold the
payload. An empty string means payload zero; a complete unsigned decimal,
octal (`0` prefix) or hexadecimal (`0x` prefix) string of any length is read in
its base without host floating arithmetic. A signed or space-prefixed string,
empty hexadecimal digits, an embedded NUL, a malformed string or a dynamic
operand is not folded and follows the ordinary `nanf`/`nan`/`nanl` call
boundary (`E2208` where a static constant is required).

HFA records of one to four equal f16/f32/f64/f128 members carry
`hfa=Cxf16/f32/f64/f128` in byval/sret marks, distinct from `hva`; the same shape survives `va_arg.byval`
attributes. Classification is recursive over completed layout: array elements
count individually and a union counts its largest member; size must equal
member count times the base size, and alignment-induced padding uses the
composite ABI. A record with floating members that is not an HFA passes through
the general-register composite ABI as an object representation, also as an
anonymous argument: copying its floating or padding bytes is not floating
arithmetic, initializes nothing and reads no scalar.
Scalar FP and HFA arguments share AAPCS64's SIMD argument cursor,
independent of GPRs, with one shared stack cursor and packed HFA stack objects;
float anonymous arguments promote to double. No object-layout substitution or
private scalarized external ABI is authorized.

HFA transport alignment is the members' natural alignment clamped to 8 or 16,
independent of any explicit alignment of the complete object. The mark
`, abi_align=16` after `hfa=` records the 16-byte case; its absence means 8.
Calls, entries and `va_arg` use the recorded alignment, and a packed stack copy
must not claim the destination's stronger alignment for its source. Object
layout is unchanged, and the mark is part of text, validation and proof identity.

**Translation-time evaluation.** A floating literal, decimal or hexadecimal,
rounds once to its source format: binary32 with suffix `f`, binary64 without a
suffix, binary128 with `L`. A later conversion starts from that rounded value
and must not recover digits the first rounding lost. Constant arithmetic,
constant path selection, integer projection and representation stores evaluate
host-independently with RNE at each operation's semantic width, without
contraction and without saturating an invalid integer conversion.

**Native environment checks.** Native code must establish a compatible FPCR
before an admitted baseline operation; otherwise it reports `E3064`. A check
serves a later operation only when every reachable predecessor path, indirect
successors included, has performed it and no call, opaque `asm` or `asm.goto`
intervenes. Function entry assumes nothing about the caller. A check never
writes FPCR and changes no FP operation, token or proof identity.

**Comparison reasoning.** Source lowering can decide a conditional from repeated
comparisons of one stable f32/f64 operand pair through the LT/EQ/GT/unordered
partition, but only after keeping every original comparison with its poison and
environment obligations. The analysis excludes object and volatile reads,
calls, updates and format changes, and adds no EqSat rule. After the original
comparison has executed, a sign-cleared f32/f64 value can be treated as never
less than either zero, NaN included. This does not make `>= 0` a tautology or
remove operand consumption or FPCR admission.

#### 3.13.1 Strict world operations and explicit FMA

`fp.fma %a, %b, %c : T` accepts three identical f32/f64/f128 operands and returns
their product-plus-addend rounded once to T. It is never introduced implicitly
from multiply/add. For f32/f64 its target NaN order is addend, a, b (signaling first);
a quiet addend does not hide invalid zero*infinity, which yields default NaN.

```text
(%r, %w1) = fp.strict.add %w0, %a, %b : f64
(%r, %w1) = fp.strict.fma %w0, %a, %b, %c : f32
(%r, %w1) = fp.strict.convert %w0, %a : f32
```

Strict forms exist for add/sub/mul/div/fma, oeq/une/olt/ole/ogt/oge, convert,
from_signed/from_unsigned, to_signed/to_unsigned and the complex parts of
3.13.5. Numeric operand/result
types obey the baseline rules; the first operand is the currently live world
token and the second result is a fresh world token. Negation, constants, bits
transport and to_int_defined have no strict form. Strict effects read no memory,
append no volatile/atomic event, and are not I/O or traps in this masked profile.

The world snapshot holds the executing logical thread's rounding direction
(nearest-even, upward, downward or toward-zero) and sticky invalid, divide-by-zero,
overflow, underflow and inexact flags. Each executed operation reads that rounding
direction and ORs its raised flags into the returned world. Arm tininess is
detected before rounding, with underflow only for an inexact tiny result.
Strict equality/inequality are quiet comparisons (invalid for signaling NaN);
strict ordered comparisons signal invalid for any NaN. Classification and
quiet source comparison builtins retain their separate contracts.

The `quiet_olt/quiet_ole/quiet_ogt/quiet_oge` predicates have the same
numeric results as the corresponding ordered predicates but
invalid status only for signaling NaNs. Their strict forms explicitly carry
world, including when the numeric result is discarded. Source GNU comparison
builtins use these quiet predicates; ordinary C relational operators use the
signaling ordered predicates. `isunordered` similarly records quiet comparison
status before representation classification. Baseline forms remain pure.
`__builtin_isgreater`, `isgreaterequal`, `isless`, `islessequal`,
`islessgreater` and `isunordered` take two real arithmetic operands, at least
one floating, apply the usual arithmetic conversions, evaluate each operand once
in source order and return `int` 0 or 1; only `isunordered` is true for
unordered operands. Wrong arity is `E2909`. An operand pair without a floating
type after the usual arithmetic conversions is `E2951` (`builtin-argument-type`),
and so is a classification builtin of this section with an operand that is
not floating. A class argument of `__builtin_fpclassify` that is not an
integer constant expression, and an `x` of another type, are `E2951` too.

Integer conversions still truncate toward zero regardless of dynamic rounding;
nonrepresentable C conversions retain E3063 and the distinct pure guard. An
invalid raw conversion returns zero plus invalid status, never a defined C value.
Translation-time literal/static initialization remains RNE. Strict executed
operations, including discarded numeric results, cannot be deleted, cloned or
reordered in ways that change world state. All FP remains outside P0.

Native strict admission permits all four rounding directions but refuses other
unsupported FPCR controls with E3064, never resetting FPCR/FPSR. External/runtime
functions own concrete fenv_t encoding; the IR does not encode libc structures.
Missing external execution models fail closed. Baseline pure FP retains its
environment-unobservable admission contract; it is not a substitute for a
strict operation inside an observable-environment source region.

The source `-frounding-math` option selects these strict forms for executed
scalar arithmetic, numeric conversions, truth tests and comparisons; the last
`-frounding-math`/`-fno-rounding-math` option wins. Runtime expressions are not
folded under translation-time RNE merely because their operands are constant.
This includes automatic initializers and selected conditional-arm conversions.
Literal parsing and static object initialization remain translation-time RNE.
Exact representation operations remain pure. Both C-HIR and EvoIR execution
use the same explicit FP environment contract. The option and the separately
specified `STDC FENV_ACCESS` pragma do not enable unsupported formats, traps or
fast math.

#### 3.13.2 Target floating-environment library boundary

The AArch64 Linux GNU interpreter runtime models `fegetround`, `fesetround`,
`feclearexcept`, `feraiseexcept`, `fetestexcept`, `fegetexceptflag`,
`fesetexceptflag`, `fegetenv`, `fesetenv`, `feholdexcept`, and `feupdateenv`
through the shared library resolver. Each call transfers both memory and world;
the ordinary external top summary includes `fenv=true`. No process-global host
floating state participates. `fesetround` accepts exactly the four supported
target encodings and otherwise returns nonzero without changing state.

The concrete runtime binding owns the 8-byte/alignment-4 `fenv_t` (FPCR then
FPSR, two little-endian 32-bit words), 4-byte `fexcept_t`, rounding encodings
0/0x400000/0x800000/0xc00000 and five low exception bits. FE_DFL_ENV's -1 address
is a runtime sentinel, not a memory access. Ordinary pointers still undergo
initializedness, alignment, bounds and lifetime checks. Restoring unsupported
controls or status fails closed with E3064, not C UB or a silent control reset.

The selected GNU/glibc resolver reproduces the glibc `feraiseexcept` and
`feupdateenv` behavior: raising overflow/underflow also raises inexact. Exact
`fesetexceptflag` and `fesetenv` do not add flags. A separate explicit musl
runtime adapter uses its direct requested-bit raise policy; sharing the object
encoding does not make these library behaviors identical. The source/EvoIR
GNU interpreter entry uses the GNU/glibc binding, not a runtime-name guess.
Native calls use the actual linked runtime. This is not a pthread/loader
interpreter or an Annex F claim.

After a modeled library changes rounding, executing baseline pure numeric FP
under a non-RNE world fails E3064 in the interpreters as it does natively.
Exact representation transports and the pure conversion-domain predicate do
not depend on rounding. Strict operations continue from the returned world.

#### 3.13.3 Lexical environment access

On the AArch64 Linux GNU profile, `#pragma STDC FENV_ACCESS ON`, `OFF` and
`DEFAULT` are admitted; the implementation default is OFF. The pragma occurs
outside external declarations or before all explicit declarations/statements
of a compound statement (C11 7.6.1). Misplaced block directives are E3065. File-scope
settings govern following function bodies; a compound's setting is restored
to its enclosing lexical setting at its end. `_Pragma` and included source
use the same ordered directive path. Preprocessed source preserves equivalent
directives rather than exposing internal tokens or dropping them.

C-HIR compounds retain their effective lexical policy. Synthesized blocks may
inherit the enclosing policy; copied/inlined source blocks retain their own.
This metadata determines instruction selection, not the hardware environment:
entering/exiting a source region never writes FPCR/FPSR or implicitly invokes
`fesetenv`. Calls bind arguments under the caller's policy, body arithmetic and
return conversions use their own lexical policies, and returning restores the
caller's compilation policy without restoring its runtime FP state. Goto,
computed goto, switch dispatch, loop exits and statement expressions preserve
the policy of code actually executed, including entrances that bypass the
beginning of a compound.

Executed rounding/exception-sensitive operations in ON regions lower to strict
world operations, even with constant operands or discarded numeric results.
The independent `-frounding-math` option likewise selects strict execution in
OFF regions; OFF does not cancel that option. Without either request, baseline
pure operations retain their existing fixed-environment contract. Literal
conversion and static initialization remain translation-time RNE. Source
constant-result substitution must not erase executed FP effects or substitute
RNE values into these runtime regions. Derived unit summaries conservatively
disable such substitution when lexical FP access is present; they are not
unchecked producer annotations authorizing a fold.

This profile covers the four rounding modes, five sticky flags and §3.13.2's
non-trapping environment library on admitted scalar formats, the complex
parts of 3.13.5 and the lanes of floating vectors (10.1.14). It does not add
enabled traps, Annex F or a concurrent interpreter.

#### 3.13.4 Binary128

The Linux GNU `long double` type is real IEEE binary128: size 16, alignment 16,
113 significand bits. It uses the same scalar instructions, typed memory,
initializedness, strict world ordering, C E3063 guards and block-local SSA as
the smaller formats. Literal/static evaluation is RNE directly to binary128;
numeric casts never narrow through binary64. Representation constants, memory,
negation and sign-only builtins preserve all 128 bits, including signaling NaNs.

The qualified hosted AArch64 libgcc profile uses positive all-one-fraction NaN
(`0x7fffffffffffffffffffffffffffffff`) for invalid non-NaN arithmetic. Binary
arithmetic still selects signaling operands before quiet operands, left first.
Quad fused arithmetic selects signaling NaNs in addend/a/b order, then quiet
NaNs in a/b/addend order. Its zero-times-infinity condition still raises invalid.
This is distinct from scalar hardware FMADD quiet-NaN selection. Four rounding
modes, before-rounding tininess and five sticky flags use the shared adapter.

Quad HFA members use the existing homogeneous classifier and require ABI
alignment 16. Native quad operations may use audited hosted runtime helpers
behind EvoMIR; helper names are not EvoIR semantics. Other targets and
freestanding helper availability fail closed. No `_Float128` value, binary128
vector lane, traps, contraction or Annex F claim is implied; 3.13.5 gives the
binary128 complex parts and 3.13.6 the half formats. All FP is excluded from P0.

#### 3.13.5 Complex multiplication and division

```text
%x = fp.cmul.real %a, %b, %c, %d : T
%y = fp.cmul.imag %a, %b, %c, %d : T
%x = fp.cdiv.real %a, %b, %c, %d : T
%y = fp.cdiv.imag %a, %b, %c, %d : T
(%x, %w1) = fp.strict.cmul.real %w0, %a, %b, %c, %d : T
(%y, %w2) = fp.strict.cmul.imag %w1, %a, %b, %c, %d : T
```

The four operands and the result have one format `T`: f32, f64 or f128.
The operands are the parts of `a + bi` and `c + di`. `cmul.real` and
`cmul.imag` give the real and the imaginary part of the product. `cdiv.real`
and `cdiv.imag` give the parts of the quotient. Each part is the part that
the target's complex helper computes, with its NaN sign and payload. Each part
also raises every status flag that the helper raises. The flags are sticky,
so the two parts of one operation raise the helper's flags once. A pure form
uses RNE and needs the fixed environment of 3.13. A strict form takes and
returns the world token as 3.13.1 specifies, and uses the rounding direction
of the world. C lowering emits the real part, then the imaginary part, with
the same operands and nothing between them. A part has no other effect, and
no rewrite can replace it by the ordinary operations.

On the Linux GNU target, the helpers are `__mulsc3`, `__muldc3`, `__multc3`,
`__divsc3`, `__divdc3` and `__divtc3` of the libgcc.a of GCC 14.2.0 (Debian
14.2.0-19, SHA-256
`60dbf748e1c310aa50300a33f30bf4ee070061071aee74a52cf115a7b8765e50`).
`crates/evocc-types/src/floating/complex.rs` is their model. It follows the
instruction sequence of each helper:

- The product computes `ac`, `bd`, `ad` and `cb` (a NaN `c` wins over a NaN
  `b`), then `ac - bd` and `ad + cb`. When both parts are NaN, the helper
  recovers an infinite operand or product as Annex G.5.1 recommends. The
  binary32 and binary64 recovery fuses `ac - bd` and `ad + bc`; binary128
  uses separate soft-fp operations.
- `__divsc3` computes the quotient in binary64 with fused sums and rounds each
  part once to binary32.
- `__divdc3` and `__divtc3` use Smith's method with libgcc's scaling. Their
  magnitude comparisons signal invalid for a NaN. `__divdc3` fuses each
  product-plus-term; `__divtc3` does not.
- When both parts of a quotient are NaN, the helper recovers a zero divisor
  of a dividend that is not NaN, an infinite dividend of a finite divisor and
  an infinite divisor of a finite dividend.

The model gives the recorded hashes of the special-value grid of
`tests/fixtures/floating-complex-grid.c` and of its sample, in the four
rounding modes. The two interpreters and translation-time evaluation use this
model.

On the Darwin target, the helpers are compiler-rt's, and this revision has no
model of them. An interpreter that reaches a part stops with `E3067`
(`not-executed: complex-arithmetic`). Translation-time evaluation of a product
or quotient is `E2960` (`unmodeled-complex-constant`). Native code uses the
platform helpers as EvoMIR specifies.

The pure parts are baseline operations: they are pure and excluded from P0,
as 3.13 states. The strict forms have the `fp.strict.*` row of Section 4.
Validation rejects a part with another operand count, with formats that
differ, or with a format other than f32, f64 or f128.

#### 3.13.6 Half formats

The 2026-10-13 decision on L03 admits two storage formats on both targets:
`f16`, IEEE binary16, and `bf16`, bfloat16, which has the binary32 exponent
and eight significand bits. `_Float16` and `__fp16` values are `f16` values,
and `__bf16` values are `bf16` values. The three C types stay distinct types
of size 2 and alignment 2.

A half value is an exact 16-bit representation. Constants, copies, loads,
stores, block arguments and `to_bits`/`from_bits` with `bits<16>` preserve
every bit, a signaling NaN included. The only numeric operations on a half
format are `convert`, `from_signed` and `from_unsigned`, and their strict
forms. `convert` admits each pair of formats. Each conversion rounds once,
directly from the operand value; it quiets a NaN and keeps its sign and its
leading payload bits. A widening is exact and raises INVALID only for a
signaling NaN. A value beyond the range of the format overflows as IEEE 754
defines, to infinity in the nearest direction, and a tiny inexact result
underflows with tininess detected before rounding. No other operation
accepts `f16` or `bf16`; validation rejects one that does.

C operations on half operands follow `FLT_EVAL_METHOD` 0 (C23 H.3), as GCC 14
evaluates them on Armv8-A without the half-precision arithmetic extension:

- Each operand of `+`, `-`, `*`, `/`, a relational or equality operator,
  unary `-`, `!`, a truth test, `++` and `--` converts to binary32, and the
  operation evaluates in binary32 under the rules of 3.13. An integer operand
  of such an operation converts directly to binary32. Unary `+` converts
  nothing.
- `_Float16` and `__bf16` expressions keep their type: `sizeof (x + x)` is 2.
  Both types rank below `float` in the usual arithmetic conversions. The
  binary32 value of an operation is the value of the expression; it rounds to
  the half format once, at assignment, initialization, a cast, `return`, a
  prototyped argument, an anonymous argument, the store of a compound
  assignment or of `++` and `--`, and the value of a statement expression.
  A conditional of a half type evaluates in binary32: its value is the
  binary32 value of the selected operand, and an integer operand converts to
  binary32 directly. A comma expression keeps the binary32 value of its right
  operand.
- An `__fp16` operand promotes to `float` before every operator, as ACLE
  requires, so its expressions have type `float`; an anonymous `__fp16`
  argument promotes to `double`.
- A constant with the suffix `f16` or `F16` has the type `_Float16`, and on
  Linux one with `bf16` or `BF16` has the type `__bf16`, as GCC 14 accepts
  it. Each rounds once to binary32; a conversion to the half format rounds
  that value again, as GCC 14 does. Apple clang rejects the bfloat16 suffix,
  so on Darwin a constant with it is `E0112` (`stray-token`). The `__FLT16_*`
  macros are predefined on both targets and the `__BFLT16_*` macros on Linux
  only, as the two reference compilers predefine them.
- Operands of `_Float16` and `__bf16` together have no common type. Neither
  value set contains the other (C23 H.4.2), and the front end reports `E2961`
  (`mixed-half-formats`) at the operator.
- A conversion from a half value to an integer truncates its exact binary32
  value; the `E3063` obligation of 3.13 applies.

Translation-time evaluation follows the same rules: a half constant and an
operation round to binary32, and a static object of a half type rounds that
value once. A half value converts from an integer of 64 or 128 bits as the C
lowering writes it: binary16 from the value saturated to 32 bits, since every
larger magnitude overflows binary16 in every rounding direction; bfloat16
from the round-to-odd value of 24 significant bits, which binary32 holds
exactly, so the final rounding is the only one.

A half value crosses a call in the low 16 bits of a vector register, which
AAPCS64 C.1 leaves otherwise unspecified, and in an 8-byte stack slot on
Linux (EvoMIR 6.3). A record of one to four binary16 members is an HFA
(`hfa=Cxf16`). A record of one to four `__bf16` members is an HFA under
AAPCS64, but GCC 14 passes it in general registers (GCC PR 120007), so it is
`E3049` at every call boundary, in both interpreters and in native code.
AAPCS64 gives binary16 and bfloat16 two fundamental types, so a record that
mixes them is no HFA. On Linux it passes as an ordinary composite, as GCC 14
passes it; Clang 19 passes it in vector registers, a recorded
incompatibility. Apple clang passes it in vector registers too, so on Darwin
such a record is `E3049` at every call boundary. An anonymous `_Float16` or
`__bf16` argument stays unpromoted and `va_arg` reads it as written, as GCC
14 and Clang 19 do on Linux. On Darwin, Apple clang has no stable convention
for an anonymous `_Float16` or `__bf16` argument or an anonymous binary16
HFA, so these arguments and their `va_arg` are `E3049`. A binary16 HFA on the
stack, and a half argument on the Darwin stack, are `E4010` (EvoMIR 6.3).

Retained refusals: a `vector_size` attribute on a half format (`E2903`), the
nominal `arm_neon.h` half vector types (`E3050`, 10.1.14), half complex
values (`E2202` or `E2102` for `_Float16 _Complex`, `E1007` for the other two,
10.1.14), half `asm` operands (`E3021`), the classification and comparison
builtins of 3.13 on a half operand (`E2951`), `_Atomic` half objects
(`E2702`), half values under `-mgeneral-regs-only` (`E4002`, EvoMIR 2.6) and
`-fexcess-precision=16`, which is an unsupported option.

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
    fenv: bool,             -- reads/modifies observable floating environment
    may_trap: bool,         -- can this instruction trap?
    may_unwind: bool,       -- can this instruction unwind the stack?
}
```

### 4.2 Effect Assignment by Instruction Category

| Instruction | reads | writes | volatile | atomic | io | fenv | may_trap | may_unwind |
|---|---|---|---|---|---|---|---|---|
| `const` | `{}` | `{}` | false | false | false | false | false | false |
| baseline `fp.*` (3.13) | `{}` | `{}` | false | false | false | false | false | false |
| `fp.strict.*` (3.13.1) | `{}` | `{}` | false | false | false | true | false | false |
| `uninitialized.bits<N>`, `uninitialized.ptr<AS>`, `uninitialized.fnptr<Sig>` | `{}` | `{}` | false | false | false | false | false | false |
| `add`, `sub`, `mul` | `{}` | `{}` | false | false | false | false | false | false |
| `sdiv`, `udiv`, `srem`, `urem` | `{}` | `{}` | false | false | false | false | true | false |
| integer predicates (3.3.1) | `{}` | `{}` | false | false | false | false | false | false |
| `ub.assume.<reason>` | `{}` | `{}` | false | false | false | false | true | false |
| `and`, `or`, `xor` | `{}` | `{}` | false | false | false | false | false | false |
| `ctlz`, `cttz`, `ctpop`, `bswap` | `{}` | `{}` | false | false | false | false | false | false |
| `shl`, `lshr`, `ashr` | `{}` | `{}` | false | false | false | false | false | false |
| `icmp.*` | `{}` | `{}` | false | false | false | false | false | false |
| `sext`, `zext`, `trunc` | `{}` | `{}` | false | false | false | false | false | false |
| `ptr.null`, `global_addr`, `func_addr` | `{}` | `{}` | false | false | false | false | false | false |
| `tls_addr` | `{All}` | `{All}` | false | false | false | false | true | false |
| `vec.pack`, `vec.bitcast` | `{}` | `{}` | false | false | false | false | false | false |
| `vec.extract` | `{}` | `{}` | false | false | false | false | true | false |
| `vec.aese`, `vec.aesd`, `vec.aesmc`, `vec.aesimc` | `{}` | `{}` | false | false | false | false | false | false |
| `ptr.offset.*.*` | `{}` | `{}` | false | false | false | false | true | false |
| `ptr.to_int`, `ptr.from_int.*` | `{}` | `{}` | false | false | false | false | false | false |
| `block_addr` | `{}` | `{}` | false | false | false | false | false | false |
| `br.indirect` | `{}` | `{}` | false | false | false | false | true | false |
| `return_address`, `frame_address` | `{}` | `{}` | false | false | false | false | false | false |
| `asm`, `asm.goto` | `{all}` | `{all}` | true | false | true | true | true | false |
| `va_start`, `va_arg.*`, `va_end` | `{All}` | `{All}` | false | false | false | false | true | false |
| `alloca`, `alloca.dynamic`, `lifetime.end` | `{All}` | `{All}` | false | false | false | false | true | false |
| `pointer_valid`, `access_in_bounds` | `{All}` | `{}` | false | false | false | false | true | false |
| `load` (normal) | `{All}` | `{}` | false | false | false | false | true | false |
| `store` (normal) | `{}` | `{All}` | false | false | false | false | true | false |
| `copy` | `{All}` | `{All}` | false | false | false | false | true | false |
| `copy.volatile.read/write/readwrite` | `{All}` | `{All}` | true | false | false | false | true | false |
| `load volatile` | `{All}` | `{}` | true | false | false | false | true | false |
| `store volatile` | `{}` | `{All}` | true | false | false | false | true | false |
| `atomic.load` | `{All}` | `{}` | marker | true | false | false | true | false |
| `atomic.store` | `{}` | `{All}` | marker | true | false | false | true | false |
| `atomic.rmw.*` | `{All}` | `{All}` | marker | true | false | false | true | false |
| `atomic.copy.*` | `{All}` | `{All}` | marker | true | false | false | true | false |
| `call` | depends on callee | depends on callee | depends | depends | depends | depends | depends | depends |
| `trap.<reason>` | `{}` | `{}` | false | false | false | false | true | false |

**Trap terminators.** `trap.<reason>` is the zero-operand terminator family.
It takes no operands, produces no results, has no successors, and reaching it
reports its reason's diagnostic and ends execution. Three reasons exist:

| Reason | Diagnostic | Emitted for |
|---|---|---|
| `scalar_function_fell_off_end` | `E3007` | a hand-written EvoIR module can trap with it; source fall-through returns poison (10.1.3) |
| `unreachable_reached` | `E3024` | `__builtin_unreachable()` (10.1.17) |
| `builtin_trap` | `E3047` | `__builtin_trap()` (10.1.17): an explicit termination, not a UB report |

A trap terminator is a report, never a licence. No analysis may treat a block
that ends in one as dead, and no rewrite may use its presence to conclude that
a predecessor edge is not taken, delete a computation, or strengthen a fact
elsewhere; the only optimistic transition this profile permits is one a proof
certificate justifies. Its `may_trap` effect keeps it from being reordered or
removed, exactly as for the other reasons.

Every memory effect uses region `All`. Region refinement is not active.
The pointer guards inspect object metadata in the current snapshot and return
an unchanged snapshot under a fresh SSA identity.

### 4.3 World Token Semantics

The `world` token `%w` is a global ordering token. Every memory instruction
has world input and output positions. A normal non-observable
access forwards the same abstract state through a distinct SSA token;
instructions that carry `volatile = true`, `atomic = true`, `io = true`, or `fenv = true`
use that token as an ordering constraint.

```text
; Volatile load pattern
(%val, %m1, %w1) = load volatile %m, %w, %ptr : Type
```

The world token acts as a **scheduling barrier**. Two operations that do not
touch the same region and do not touch `world` can be reordered with respect
to each other. Two operations that consume/produce `world` cannot be reordered
past each other.

The observable world state also carries the per-logical-thread FP environment
of Section 3.13.1. It is part of immutable snapshot equality, not a hidden
interpreter-global variable. It has no independent implicit token channel.

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

No transformation of this revision reorders two EvoIR instructions. The P0
and guard rewrites replace one root in place (Sections 8.8 and 8.12), and
EvoMIR lowering keeps the order of memory operations (EvoMIR Section 5.1).

Informative. A later transformation could schedule `I2` before an `I1` that
dominates it only if `I1.writes ∩ I2.reads = {}` (no write-after-read
hazard), `I1.reads ∩ I2.writes = {}` (no read-after-write hazard),
`I1.writes ∩ I2.writes = {}` (no write-after-write hazard), neither
instruction touches `world`, and neither may trap ahead of a volatile or I/O
operation. A revision that adds such a transformation must make these
conditions normative and test them.

### 4.5 E-Class Effect Invariant

> **An e-class may never contain representations with different semantic result
> types or incompatible observable-effect contracts.**

This is a hard invariant enforced by the equality engine. A pure integer
expression and a volatile load can never be alternatives, even if they happen
to produce the same integer value in some execution. The effect summary is
part of the sort/type of the e-node and participates in hash-consing.

The scalar P0 e-graph bridge admits only total, pure scalar nodes. Every
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
    effects { reads=R, writes=W, volatile=B, atomic=B, io=B, fenv=B, may_trap=B, may_unwind=B }
{
```

`R` and `W` are region sets rendered in the Section 1.4 spelling, sorted by the
region order `all, unknown, global, stack, heap, escaped`, comma-separated with
one following space, and written `{}` when empty. Each boolean is `true` or
`false`. The clause is rendered on its own line, indented four spaces, between
the calling convention and the opening brace, and is present on every function
including those whose summary is empty; an absent clause is malformed rather
than defaulted, because a default would be indistinguishable from an omission.

The `fenv` field is mandatory. A canonical parser rejects a clause without it
and never reads a missing field as `fenv=false`. Only the explicit legacy
reader `parse_pre_fenv_module` (`evo-proofcheck --legacy-pre-fenv`) reads text
written without the field. That reader rejects strict FP and FMA, derives the
`fenv` component and validates every recorded field. It is never a fallback
for current text, and a migration regenerates the canonical subject and its
proof. Canonical subject hashes bind the `fenv` field and strict world edges.

**Derivation.** A summary is the least fixed point over the call graph:

0. Every external function (2.1) has the **top** summary: `reads={all},
   writes={all}` and every boolean `true`. Nothing this module holds can
   bound what another unit's code does, so no smaller summary is sound. A
   later revision can let a declared attribute (`const`, `pure`) lower it.
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
everything that calls it. A call that returns twice (3.10) contributes the top
summary too, whatever its callee is.

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
String-literal objects, `const`-qualified file-scope compound literals and
globals marked `readonly=true` are read-only; an attempted store reports
`E3014`. A `readonly=true` global becomes read-only once its bytes and
relocations are installed, and each TLS instance does so without changing its
thread owner. C lowering sets the mark for a static-storage object of
const-qualified type, following array element types but not pointees or
individual record members. The mark survives text, module identity, proof
reconstruction and machine lowering, and it does not license folding a volatile
read. Native placement is read-only data, all-zero objects included; a
position-independent object with relocations uses data that is writable only
during relocation (`.data.rel.ro` on Linux). TLS sections and explicit
`section` placement keep their own rules.

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
KIND := "file" | "function_static" | "string_literal" | "compound_literal"
VIS := "external" | "internal" | "hidden" | "protected"
```

Section 9.3 gives the complete global form, including relocations,
`readonly`, `linkage` and `section`.

A `function_static` or `string_literal` global is always `internal`. A `file`
global is `internal` for a file-scope object declared `static` and `external`
for a definition without `static`; `hidden` and `protected` are the ELF
visibilities described below.

**Function linkage.** A function carries the same distinction, written after
the calling convention:

```text
func @name(...) -> (...) cc = target_default visibility=internal
```

`visibility=internal` is written for a function whose C definition has
storage class `static` (including `static inline`); absent an ELF visibility
attribute the field is omitted for every other function, and its absence
means external. The machine emits no global symbol for an internal function
or an internal global. It omits an internal function that nothing in the
module references by `call`, `func_addr` or a relocation, and repeats until
nothing more can be omitted, as the reference compilers drop an unused
`static inline`. An unreferenced internal function must not reach the
assembler.

**Generated symbols.** A `function_static` symbol must equal
`__evocc.static.<origin>` and a `string_literal` symbol must equal
`__evocc.string.<origin>`; a `file` symbol is a source C identifier and
therefore contains no dot. These are validation rules, not renderer
conventions. The kind is a property of the object rather than of the name
that reached it, so it is recorded here: a linker and a checker both need it,
and neither can recover it from the symbol text. The dots are part of the
generated symbol. Section 9.2 permits dots after the first character of an
identifier; source C identifiers in this profile cannot contain a dot, so the
`__evocc.` generated namespace cannot collide with a source symbol.

**Linkage marks.** `weak` and `alias` are facts about symbols and nothing
else, so they are written on module records, never on types or instructions.

`linkage=weak` may follow the visibility of a `global`, the calling convention
and optional `visibility` of a `func`, and the end of an `extern` or
`declare`. On a definition it means the symbol is emitted weak, so an ordinary
definition of the same name in another object wins at link time; inside the
module the definition is the definition, and the interpreter executes it
exactly as a strong one, because the module alone contains no competitor. On
an `extern` or `declare` it means a reference that resolves to address zero
when no object defines the name; the interpreter treats it as any other
external, at the external-call boundary, and states nothing about its
address. `visibility=internal` together with `linkage=weak` is `E3060`
(`weak-internal-symbol`). C-HIR writes the mark for `__attribute__((weak))` on
a definition or declaration of an object or function. A `weak` or `alias` mark
written after `*` in a named declarator still marks the declared object or
function, not the pointer type, and is recorded once with its written
placement; a parameter's mark does not reach its function.

`alias @name = @target` declares a second symbol for something defined in
this module. `@target` must name a `global` or a `func` of this module, not an
`extern`, a `declare` or another `alias`; anything else is `E3061`
(`alias-target-not-defined`), and an alias of an alias is written as a second
alias of the same target. The alias has the target's kind, size, alignment or
signature, and every use of `@name` in the module (`global_addr`, `func_addr`,
`call`, a relocation) means the target, so `global_addr @name` and
`global_addr @target` are one provenance and compare equal. The alias carries its own `visibility` and `linkage`, external
and strong when omitted; the target's are unchanged, and an alias counts as a
reference to its target for the omission rule of function linkage above. C-HIR writes an
alias for `__attribute__((alias("target")))`, resolving the string to the
symbol of this unit's definition. `spec/evomir.md` specifies the emission.

**Code alignment.** A function may carry `align=N`, a power
of two of at least 4, written after its linkage marks; absent, it is 4, the
instruction alignment. It is the alignment of the function's first
instruction in the object, so `func_addr @f` (and every pointer with
provenance `Function(@f)`) is a multiple of `N`. Both interpreters must honor
the same alignment when assigning virtual function addresses: `ptr.to_int`
makes their low bits observable. Distinct functions retain distinct identities
and addresses, aliases retain their target's identity, and exposing and
reconstructing a function pointer must round-trip. This does not prescribe
the numeric address beyond its alignment and the existing pointer rules. The
machine emits `.p2align log2(N)` before the label. C-HIR
writes the field for `__attribute__((aligned(N)))` on a function: the value
is the maximum over every declaration and the definition of the symbol in
the unit, a later declaration never lowers it, and `__alignof__` of a
function designator answers that maximum while `__alignof__` of a function
type answers 4. The final `-fmin-function-alignment=N` accepts 0, which keeps
the 4-byte default, or a power of two up to 65536, and raises every
definition's `align` to at least `max(4, N)`, cold functions included. It
changes no source declaration or `__alignof__` answer; interpreters, native
emission and patchable entries use the raised value. Other values of `N` are
option errors.

**Frame protection.** A definition may carry ` stack_protect` after its
optional `align`; the field is omitted when false and survives canonical text
and proof reconstruction. It requests a native stack-guard check and adds no
abstract operation, effect or token. Under the final `-fstack-protector-strong`,
C lowering sets it for a function with an automatic array (also inside an
aggregate), dynamic stack allocation, a source reference to a local-frame
address, or an addressable parameter. A `no_stack_protector` attribute on the
definition or a final `-fno-stack-protector` clears it. The options that
choose the guard's symbol or system-register source, return-address signing
and general-register-only code act independently of the field.
After private-cell promotion (10.1.3), C lowering can clear the request only
if every fixed allocation is promoted and no dynamic allocation or frame-address
operation remains. Native emission must still honor an explicit IR request.

**Patchable entries.** The final `-fpatchable-function-entry=N[,M]` sets a
default with `0 <= M <= N <= 65535`; an omitted `M` is 0. A function's
`patchable_function_entry(N, M)` attribute, with integer constant expression
arguments, overrides it, `(0, 0)` included, and `__has_attribute` reports it.
The effective value is written ` patchable_entry=N,M` after `stack_protect`;
validation requires `M <= N`. It is part of validation, text and proof identity
but not of the callable type or abstract effects. For nonzero `N`, Linux ELF
emission places `N` NOPs, `M` of them before the callable symbol, and a
link-ordered patch-table relocation to the first NOP. Declarations before the
definition merge into it; a conflicting later declaration is E2943 and
malformed arguments or placement are E2903. When the callable symbol has an
explicit `align`, padding goes before the patch region so that the symbol
itself stays aligned.

**Function placement.** A definition may carry optional
` section="name"`, last in its canonical header after any `patchable_entry`.
It names the ELF executable PROGBITS section containing the complete function,
including entry padding, local labels and its unwind range; omission selects
ordinary text. The decoded UTF-8 name must be nonempty and contain no control
characters. This is placement metadata, not part of the callable type, linkage,
alignment, abstract effects or pointer identity; interpreters do not invent an
address ordering or reclaim code based on a section name.

C lowering merges function `section` across the complete declaration chain
and assembler-name identities. A `section` written inside a named pointer
declarator still describes the declared function or object, not its pointer
type; it enters the same symbol-attribute merging path. Identical requests agree, including later
repetitions; malformed or conflicting requests are E2943. A first placement
request after its function definition is also E2943: GCC applies it and Clang
ignores it, so this profile does not silently choose either layout. Numeric
escapes in a source function-section attribute are E2943 because Clang rejects
them as unevaluated-string escapes; raw UTF-8, ordinary escaping and adjacent
string tokens retain their decoded bytes. This source boundary does not change
the canonical IR quoted-string encoding. An alias
shares its target's placement and cannot request a different section. Declarations
without a definition produce no code or section allocation; GNU inline-only and
ordinary inlining/retention rules are unchanged, and an inlined body does not
change its caller's placement. The field survives canonical text, optimization
and independent proof reconstruction: adding, deleting or changing it is not a
scalar equality rewrite. Native emission must consume the validated metadata,
preserve alignment and link-ordered patch registrations, and select each following
function's own section; unsupported object formats fail closed (E4009).

**ELF visibility.** C linkage, weak/strong binding and ELF visibility are
independent. The compact encoding is: `internal` means
C internal linkage, `external` means C external linkage with ELF default,
`hidden` means C external linkage with ELF hidden, and `protected` means C
external linkage with ELF protected. These marks apply equally to functions,
global objects, declarations and aliases; declarations cannot be C internal.
Only C internal linkage permits removal as an unused private definition.

A protected symbol remains exported. References from its final executable
or shared object bind to that component's definition; the boundary is not a
translation unit or an EvoIR module. A default-visible definition in the
current module is not a local-binding promise: PIC address materialization
must remain interposable. The assembler/linker implements the component
boundary, including refusing an invalid protected-data copy relocation.
The two single-module interpreters do not model ELF dynamic loading.

In this AArch64 ELF profile, source `visibility("internal")` is normalized
to effective `hidden`, retaining the written attribute in C-HIR and leaving
C linkage and strength unchanged. This deliberately follows Clang's ELF
encoding, not GCC's `STV_INTERNAL` or its additional optimization assumptions.
Redeclarations compare effective visibility, so internal/hidden agree;
conflicting effective values or malformed attributes remain E2943.
Conformance tests (informative): `tests/fixtures/symbol-component-*.c`.

`ObjectId` is an interpreter-local provenance identity and is not compared
across different IR executions. `ObjectOrigin` is the cross-IR observation
identity: it consists of a stable source-site ordinal and a monotonically
increasing per-site dynamic instance. Globals and string literals use instance
zero; each execution of an automatic allocation advances only that site's
instance counter. C-HIR and EvoIR lowering preserve the same source-site
ordinal even when unrelated scalar locals remain in SSA, so materializing or
eliding such locals cannot perturb volatile-trace comparison.

Every object has an exact target-derived size in `0..=i64::MAX` and a
target-layout-representable power-of-two alignment. A zero-sized object has a
live allocation identity and no bytes: initializing or copying it accesses
nothing, a byte access reports `E3011`, and an access after its lifetime
reports `E3010`. A zero-sized global has empty `bytes=` and `initialized=`
fields. Distinct zero-sized objects need not have distinct addresses.
Object bytes and one initialized flag per bit are distinct state. A store marks
exactly its destination bits initialized: every bit of its range, or the bits
of its mask (3.8). A load reports `E3001` when a bit that it requests is
uninitialized: every bit of its range, or the bits of its mask. Merely
allocating, copying, passing, or branching with an uninitialized object does
not report a diagnostic. A byte is partly initialized only after a masked
store to it, or after an operation that moves such a byte unchanged, such as
`copy`. A program without a bit-field store has no such byte. The
`initialized=` field of a global has one flag per byte, for all eight of its
bits.

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

`Function` provenance is produced by `func_addr` and by a relocation that
targets a function (2.1). It is carried in the
out-of-band fragment exactly as `Object` provenance is, so a function pointer
stored to memory and loaded back names the same function, and a partially
overwritten representation reports `E3013` on the same rule. It has no object
and no offset: `ptr.offset` applied to it is invalid, and loading or storing
through it is invalid, because a function is not an object with bytes. Only
`call` consumes it.

Each object has a deterministic abstract address, derived from its
allocation origin, and the address ranges of two objects never overlap. A pointer's abstract address is its object's
address plus its byte offset, the integer address of an exposed-address
pointer, zero for `Null`, or a value fixed by the function symbol. Pointer
equality (Section 3.5) compares these addresses, so for function pointers it
means naming the same symbol. Provenance stays part of the pointer: two
pointers with equal addresses and different provenance compare equal, and an
access through each still follows its own provenance (Section 5.4).
Provenance is never derived from a host address. Conversions between pointers
and integers follow Section 3.7: `ptr.to_int` exposes the object, and `ptr.from_int` yields
exposed-address provenance. Backend provenance erasure is outside EvoIR.

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
7. A load that requests an uninitialized bit (5.2) is an uninitialized
   read.
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

### 5.5 Memory-Optimization Boundary

Alias analysis, region splitting, store forwarding, store bypass, redundant
load elimination, and every rewrite that depends on `MustAlias` or `NoAlias`
are outside this revision. The only e-graph input is a pure, total scalar
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
| `bit_builtin_zero_operand` | `E3022` |
| `floating_to_integer` | `E3063` |

For operands of type `bits<N>`, `signed_add_defined`,
`signed_sub_defined`, and `signed_mul_defined` each have type
`(bits<N>, bits<N>) -> bits<1>`. They are true exactly when the corresponding
mathematical result is representable by the signed `N`-bit C result type.

**Explicit signed-wrapping mode.** The default above applies unless an option
selects this mode. `-fwrapv` enables two's-complement wrapping for signed addition,
subtraction, multiplication and negation, including their update forms,
after C promotions and conversions. `-fno-strict-overflow` also enables
this arithmetic mode; `-fno-wrapv` and `-fstrict-overflow` disable it, in
command-line order. The selected mode belongs to the C-HIR translation unit
and survives substitution. In this mode lowering omits only the signed
add/subtract/multiply overflow obligations; EvoIR's bit-vector operations
already have the required semantics. Division, remainder, shifts, pointer
provenance, object bounds and all other diagnostics remain unchanged. In
particular this profile does not use either option to waive pointer safety
or admit out-of-object accesses; it does not claim GCC's full pointer-wrap
option compatibility.

**Signed left shift in the GNU profile.** For a signed left
operand and a valid count (`shift_count_defined` still asserted, so the
count is in `[0, N)`), `x << y` is the low `N` bits of the mathematical
product `x * 2^y` in two's complement, whatever the sign of `x` and whether
or not the result is representable. C11 6.5.7 leaves both cases undefined;
this profile follows the documented GNU semantics. Lowering emits
`shift_count_defined` and `ub.assume.invalid_shift_count` before every shift
and no `signed_shl_defined` obligation; an invalid count remains `E3005`.
Signed addition, subtraction and multiplication keep their obligations unless
the wrapping mode above is selected.

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
| `E3001` | At least one bit that a load requests is uninitialized. |
| `E3009` | The accessed pointer has `Null` provenance. |
| `E3010` | The named object's lifetime has ended. |
| `E3011` | Pointer formation or the complete access range is outside its object, or checked offset arithmetic overflowed. |
| `E3012` | The byte offset does not satisfy the access alignment. |
| `E3013` | The provenance is absent/forged or stored pointer bytes lack one complete provenance representation. |
| `E3027` | `copy` ranges overlap without being identical. |
| `E3048` | An executed indirect branch target does not identify a listed label in the current function. |
| `E3014` | A store targets string-literal storage. |
| `E3018` | `lifetime.end` names a live base pointer that is not an automatic object owned by the current frame. |

Both interpreters use the deterministic classification order from Section 5.4
and delegate code/message selection to the shared typed memory-fault authority;
they may not maintain divergent local mapping tables. C-HIR attaches the
source access span. EvoIR attaches the function, block and instruction of the
fault. The driver
reports the C source span that lowering recorded for that instruction. When
the instruction has no recorded span, or the function changed after
lowering, it reports the span of the function's declaration; with neither,
it reports the input file without a position.

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
| Invalid signed left shift | signed `x << y` is not representable (C11 6.5.7) | not asserted: the GNU profile defines the result as the truncated two's-complement bit pattern (6.1); `signed_shl_defined` and `E3006` remain readable |
| Pointer arithmetic validity | result must name a live object at an offset in `[0,size]` | `ptr.offset.*.*` → `pointer_valid` |
| Memory access validity | complete positive-width range and alignment must be valid in the current snapshot | `access_in_bounds` → matching `load`/`store` |
| Null pointer access | pointer has `Null` provenance | memory guard or direct fail-closed access reports `E3009` |
| Dangling pointer access | pointer names a tombstoned object | memory guard or direct fail-closed access reports `E3010` |
| Out-of-bounds access | pointer/range lies outside its object | memory guard or direct fail-closed access reports `E3011` |
| Misaligned access | offset violates the target-required alignment | memory guard or direct fail-closed access reports `E3012` |
| Write to string-literal storage | store targets a read-only object | `store` reports `E3014` |
| Use of uninitialized scalar SSA value | consume a value defined by `uninitialized.bits<N>` | the interpreter reports `E3001` at the consuming instruction or terminator |
| Read from uninitialized memory storage | any requested initializedness bit is false | `load` reports `E3001` at the load, never allocation or token transport |
| Invalid pointer representation/provenance | pointer bytes or provenance are not valid | pointer load/guard/access reports `E3013` |
| Strict aliasing/effective type | access through incompatible effective type | not checked; deferred with alias analysis (5.5) |

`-fno-strict-aliasing` explicitly selects the conservative access
contract: no analysis or optimization derives disjointness
solely from differing C access types. This option neither removes object
identity/provenance, lifetime, initializedness, bounds, alignment or read-only
checks nor licenses incompatible pointer representations. Source and IR
execution and emission already have this behavior; the option does not change
them. The effective-type/TBAA extension is outside this revision, and
`-fstrict-aliasing` is not silently accepted.

`uninitialized.bits<N>` defines an ordinary poison value with a real SSA
identity. Branch operands and block arguments may copy that poison without
trapping, which lets definite assignment on a later path overwrite it. Any
semantic consumption—arithmetic, comparison, conversion, branch condition,
ordinary call argument, or return—reports `E3001`.
It is not a zero value and it must never be silently replaced by one. Memory
initializedness uses the separate per-bit object state and follows the same
consume-time rule: transporting a pointer or memory token does not trap; the
executed load does.

### 6.3 Diagnostic Authority

Every diagnostic this specification names must appear here with its exact
message. A code introduced in one section and never listed here has no
authoritative message, so two implementations could print different text for
one condition; a code listed twice would let a reader think two conditions are
the same finding. The following codes are named in this document but have no
row yet, because the implementation uses several messages for each or none is
fixed: `E0002`, `E1001`, `E1006`, `E1303`, `E2102`, `E3051`, `E3052`,
`E3063`, `E3064` and `E3065`. `evocc --explain` reports a missing explanation
for them. Codes `E4xxx` belong to EvoMIR (`spec/evomir.md`).

| Code | Message | Detected |
|---|---|---|
| `E0102` | `unmatched-conditional` | preprocessor |
| `E0104` | `error-directive` | preprocessor |
| `E0105` | `include-not-found` | preprocessor |
| `E0106` | `feature-query-malformed` | preprocessor |
| `E0107` | `pragma-pack-unsupported` | preprocessor; front end for placement inside a record |
| `E0108` | `poisoned-identifier-used` | preprocessor |
| `E0109` | `counter-overflow` | preprocessor |
| `E0110` | `invalid-token-paste` | preprocessor |
| `E0111` | `conditional-operand-not-integer` | preprocessor |
| `E0112` | `stray-token` | front end |
| `E0113` | `directive-in-macro-argument` | preprocessor |
| `E0114` | `unsupported-attribute-pragma` | preprocessor |
| `E1003` | `expected-expression` | front end |
| `E1005` | `invalid-declarator` | front end |
| `E1207` | `invalid-return-value` | front end |
| `E1306` | `enumerator-outside-underlying-type` | front end |
| `E1307` | `invalid-enumeration-underlying-type` | front end |
| `E2001` | `undeclared-identifier` | front end |
| `E2002` | `incompatible-redeclaration` | front end |
| `E2103` | `controlling-expression-not-scalar` | front end |
| `E2202` | `invalid-initializer-type` | front end |
| `E2203` | `variable-length-object` | front end |
| `E2208` | `non-constant-static-initializer` | front end |
| `E2702` | `atomic-update-unsupported` | front end |
| `E2902` | `not-covered: compound-literal` | front end; withdrawn: compound literals are lowered (10.1.13) |
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
| `E2933` | `unsupported-termination-builtin` | front end |
| `E2934` | `function-specifier-on-non-function` | front end |
| `E2935` | `unsupported-library-builtin` | front end |
| `E2936` | `invalid-line-directive` | preprocessor |
| `E2937` | `invalid-call-target` | front end |
| `E2938` | `invalid-alignment-specifier` | front end |
| `E2939` | `unsupported-wide-bit-field-promotion` | front end |
| `E2940` | `unsupported-scalar-storage-order` | front end |
| `E2941` | `unsupported-type-classification` | front end |
| `E2942` | `incompatible-union-cast` | front end |
| `E2943` | `invalid-or-conflicting-symbol-attribute` | front end |
| `E2944` | `ordinary-builtin-binding-conflict` | front end |
| `E2945` | `jump-into-variable-type-scope` | front end |
| `E2946` | `builtin-local-binding-conflict` | front end; lowering for a library-symbol/object collision |
| `E2947` | `invalid-atomic-memory-order` | front end |
| `E2948` | `integer-complex-division-oracle-conflict` | front end; withdrawn: integer-complex division follows 10.1.14 |
| `E2949` | `unknown-builtin` | front end |
| `E2950` | `invalid-va-arg-pack` | front end; lowering, for a body that keeps the pack |
| `E2951` | `builtin-argument-type` | front end |
| `E2952` | `unsupported-function-attribute` | front end |
| `E2953` | `incompatible-record-cast` | front end |
| `E2954` | `constant-string-binding-conflict` | front end |
| `E2955` | `invalid-atomic-type` | front end |
| `E2956` | `atomic-member-access` | front end |
| `E2957` | `always-inline-returns-twice` | front end |
| `E2958` | `unsupported-enumeration-underlying-type` | front end |
| `E2959` | `unsupported-complex-type-generic-call` | front end |
| `E2960` | `unmodeled-complex-constant` | front end |
| `E2961` | `mixed-half-formats` | front end |
| `E2962` | `unavailable-declaration` | front end |
| `E2963` | `unsupported-blocks` | front end |
| `E2964` | `unsupported-overloadable-attribute` | front end |
| `E2965` | `unsupported-os-log-builtin` | front end |
| `E2966` | `unsupported-acle-builtin` | front end |
| `E2967` | `unsupported-darwin-attribute` | front end |
| `E2968` | `unsupported-pedantic-errors` | driver |
| `E3001` | `read-of-uninitialized-object` | runtime |
| `E3002` | `integer-division-by-zero` | runtime |
| `E3003` | `integer-remainder-by-zero` | runtime |
| `E3004` | `signed-integer-overflow` | runtime |
| `E3005` | `invalid-shift-count` | runtime |
| `E3006` | `invalid-signed-left-shift` | runtime |
| `E3007` | `unsupported-interpreter-input` | interpreter or execution-entry refusal; not a finding |
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
| `E3018` | `invalid-lifetime-end` | runtime |
| `E3019` | `function-pointer-target-signature-mismatch` | runtime |
| `E3020` | `not-lowered: checked-arithmetic` | lowering; withdrawn |
| `E3021` | `not-lowered: asm` | lowering |
| `E3022` | `bit-builtin-zero-operand` | runtime |
| `E3023` | `not-lowered: bit-builtin` | lowering; withdrawn |
| `E3024` | `unreachable-reached` | runtime |
| `E3025` | `not-lowered: return-address` | lowering; levels above zero only |
| `E3026` | `not-lowered: static-relocation` | lowering |
| `E3027` | `overlapping-copy` | runtime |
| `E3029` | `not-lowered: computed-goto` | lowering; withdrawn for the `br.indirect` form of 3.9; not a runtime target error |
| `E3030` | `not-lowered: varargs` | lowering |
| `E3031` | `not-lowered: frame-address` | lowering; the interpreters report `not-executed: frame-address` on reaching a nonzero depth (3.7) |
| `E3032` | `not-lowered: pointer-difference` | lowering; withdrawn |
| `E3033` | `not-executed: wide-integer` | execution of an unsupported integer width; not raised for 128-bit division and remainder (10.1.3) |
| `E3034` | `not-lowered: function-pointer-arithmetic` | lowering; withdrawn: function-pointer arithmetic uses exposed addresses (10.1.3) |
| `E3035` | `not-lowered: global-register-variable` | lowering; withdrawn: a global register variable is an implicit `asm` (10.1.18) |
| `E3036` | `not-lowered: pointer-integer-conversion` | lowering; withdrawn, never reported |
| `E3037` | `va-start-outside-variadic-signature` | validation |
| `E3038` | `va-arg-mismatch` | runtime |
| `E3039` | `external-call-not-executable` | runtime |
| `E3040` | `external-object-not-executable` | runtime |
| `E3041` | `symbol-declared-and-defined` | module validation |
| `E3042` | `not-lowered: checked-arithmetic-width` | lowering; withdrawn |
| `E3043` | `not-lowered: object-function-pointer-conversion` | lowering; withdrawn |
| `E3045` | `not-lowered: ordered-pointer-comparison` | lowering; withdrawn |
| `E3047` | `builtin-trap-reached` | runtime |
| `E3048` | `invalid-indirect-branch-target` | runtime |
| `E3049` | `not-lowered: floating-call-interface` | lowering |
| `E3050` | `not-lowered: floating-value` | lowering |
| `E3060` | `weak-internal-symbol` | module validation |
| `E3061` | `alias-target-not-defined` | module validation |
| `E3062` | `invalid-atomic-access` | validation / interpreter access contract |
| `E3066` | `object-size-check-failed` | runtime |
| `E3067` | `not-executed: complex-arithmetic` | runtime |
| `E3101` | `evoir-lowering-failed` | lowering |
| `W0001` | `unused variable` | front-end lint |
| `W0002` | `unused parameter` | front-end lint |
| `W0003` | `function has no declaration` | front-end lint |
| `W0004` | `declaration has no type specifier` | front-end lint |
| `W0005` | `function does not return the required value` | front-end lint |
| `W0006` | `pointer target differs in signedness` | front-end lint |
| `W0007` | `pointer arithmetic on void or function type` | front-end lint |
| `W0008` | `pointer target discards qualifiers` | front-end lint |
| `W0009` | `pointer target discards array element qualifiers` | front-end lint |
| `W0010` | `warning directive` | preprocessor lint |

The ten warning names, in code order, are `unused-variable`,
`unused-parameter`, `implicit-function-declaration`, `implicit-int`,
`return-type`, `pointer-sign`, `pointer-arith`, `discarded-qualifiers`,
`discarded-array-qualifiers` and `cpp`. The unused warnings default to allow; the
implicit and return warnings default to warn, except an implicit function
declaration defaults to deny in C99 and later modes. The pointer sign and
pointer arithmetic warnings default to allow. The two discarded-qualifier
warnings default to warn. The `cpp` warning for an active `#warning` defaults
to warn; `-Wcpp`, `-Wno-cpp` and `-Werror=cpp` set its level, and
`-Wno-error=cpp` keeps it a warning under `-Werror`. The driver keeps implicit
declaration evidence
before lowering. An allowed or warned implicit declaration uses the existing
implicit `int` and unprototyped-function rules. This does not enable old-style
definitions in a modern language mode.

`-WNAME`, `-Wno-NAME` and `-Werror=NAME` set warn, allow and deny. The last
setting of the same specificity wins. `-Wall` and `-Wextra` enable the unused
warnings and `pointer-sign`, but not `pointer-arith`; `-Wpedantic` enables
`pointer-sign` and `pointer-arith`. An explicit named setting takes priority
over a group. `-Werror` promotes enabled warnings to errors. `-w` suppresses
all ten. `-Wno-error=NAME` only stops `-Werror` from promoting that lint; it
does not enable or change the lint's own level. `-Werror` promotes only
EvoCC's lints, so a program that only GCC 14 would warn about stays accepted.
`--cap-lints=allow|warn|deny` limits the final severity; it never enables a
lint. A denied lint returns exit 1 before output publication. A warning does
not change the exit status. Warning codes remain `Wxxxx` when promoted to
errors. Their presentation uses `spec/diagnostics.md`. These diagnostics do
not change executable IR.

`-pedantic-errors`, `--pedantic-errors` and `-Werror=pedantic` are `E2968` at
the command line; EvoCC has no ISO pedantic diagnostic mode. `-pedantic` and
`-Wpedantic` remain accepted.

`E3017` compares a call site to its pointer type; `E3019` compares the runtime
symbol selected by that value to the same type. The separation is deliberate:
a malformed call site and a program-selected incompatible target are different
findings and are acted on differently.

A withdrawn code keeps its row, so `evocc --explain` still answers it, and no
new condition may reuse it. The codes `E2207`, `E2921`, `E3028`, `E3044` and `E3046` are
retired without a row: no rule of this revision raises them, and no new
condition may reuse them.

### 6.4 UB and Optimization

No transformation of this revision reads a fact from `ub.assume`. No rule of
the registries of Section 11.6 has a premise from it, and `ub.assume` and
every predicate of Section 3.3.1 stay outside the P0 regions (Section 8.5).
The guard folding of Section 8.12 replaces a predicate only on the known bits
of its operands, never on an assumption.

Informative. A `ub.assume.<reason> %ok` lets a later transformation assume
`%ok == true` in every block that it dominates. Reassociation of a signed
addition is an example: `(a+b)+c` and `a+(b+c)` have equal `add.bits`
results, but their signed overflow occurs on different operands, so a rule
that reassociates must require the obligations of both inner additions.

### 6.5 Propagation Rules

Informative. UB facts propagate forward: an `ub.assume.<reason> %ok` in block
`B` gives a fact that holds in every block that `B` dominates, along paths
where `%ok` was true, and the equality context `Γ` (Section 7) would hold it.
This revision reads no such fact (Section 6.4).

A successful memory guard creates a fact keyed by its output `mem<all>` value,
pointer, and (for `access_in_bounds`) exact size/alignment. Because every later
memory effect advances the token, a fact cannot be silently reused against a
different snapshot. Validation uses these facts to require a matching
guard before each load/store; the scalar P0 e-graph does not consume
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

`Γ` is a set of facts known at a particular program point. In this revision
`Γ` holds only the known-bits facts of Section 8.10. They come from the
operands of a node and from the edges into its block, never from a branch
condition, an assumption, a range or provenance.

Informative. The design adds these fact kinds in later revisions; no
transformation of this revision uses them as facts of `Γ`:

| Fact kind | Example | Source |
|---|---|---|
| Branch conditions | `%cond != 0` | `cond_br` true edge |
| Non-zero facts | `%x != 0` | Derived from comparisons |
| Range facts | `%x ∈ [0, 100]` | Loop bounds, comparisons |
| Alignment facts | `%p` is 8-byte aligned | `alloca` / `access_in_bounds` |
| Provenance facts | `%p` has one automatic object's `ObjectId` | `alloca` |
| Obligations | `signed_add_defined(%x, %y)` | `ub.assume.signed_overflow` |
| No-overflow facts | `add_defined(%x, %y)` | Obligation propagation |
| Alias facts (not in this revision) | `NoAlias(%p, %q)` | Alias analysis (5.5) |

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

**P0 profile.** The P0 profile combines the persistent state of Section 8.9
with a small semantic surface. Its operator authority binds every operator,
constant payload, bit width, arity, and pure/no-trap effect class; distinct
input values and distinct constants can never hash-cons merely because they
share a nullary opcode.

The base P0 rules are typed constant folding, `x + 0`, `x * 1`,
`x & 0`, `x | 0`, double arithmetic negation encoded exactly as
`sub.bits<N>(0, sub.bits<N>(0, x)) -> x`, and the six identity comparisons:
`eq`, `sle`, and `ule` produce one; `ne`, `slt`, and `ult` produce zero. Later
registries add rules over the facts of Section 8.10. Section 11.6 is the exact
rule authority. Each pattern requires identical operand types
and the poison/effect exclusions below. A deterministic
greedy extractor accepts only a strict cost improvement and prefers the
original on ties. "Original" for a final class means its smallest `source=before`
`NodeId`; if that class has no before node, equal-cost alternatives use the
smallest `NodeId`.

In the P0 profile, cost is the number of instructions that a choice newly
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
be rebuilt (for a weight vector, every weight); the name alone is a label.
The certificate carries none of this, and its format does not depend on the
policy. Two policies that differ only in cost produce certificates the checker
accepts or refuses on identical grounds. An acceptance corpus, such as the
kernel cohort, must never enter the fitness of any policy.

The replacement-root exclusion in the third rule is mandatory: assigning zero
to the original root would make every target tie the original under strict
improvement and would silently disable all P0 optimization. Costs are solved by
repeatedly visiting nodes in `NodeId` order and assigning a finite cost only
when every child required by the applicable rule has a finite cost, until no
choice changes. A class reachable from the root but left without a finite choice
makes the extraction of that region fail, and the root keeps its original
instruction; extraction never recurses or inserts a placeholder. This is
deliberately an instruction-count-only P0 model: it does
not estimate register pressure, and a later multi-objective model must revise
the cost authority rather than reinterpret these zero-cost cases. Cross-block
reuse remains excluded by Section 8.5. Explicit node and epoch budgets bound
every transaction (Section 8.11). A region whose extraction fails is
declined, and its root keeps its original instruction. A proof failure, a
reassembly or validation failure of the output module, or a disagreement
with the equality state fails the whole transaction: it returns the
byte-identical original EvoIR, a certificate with no region, and the equality
state it entered with (Section 8.9). The driver's chain of transactions (Section 8.11, "Runs") then stops
the compilation with exit 1; it does not continue with the module that has
no rewrite. Equality saturation is never needed for correctness: a
compilation without `-Oeq-scalar` applies no rewrite.

The P0 profile bounds one transaction by `max_nodes = 100000` and
`max_epochs = 1024` (Section 8.11). The driver admits smaller limits of its
own (`--eq-max-nodes`, `--eq-max-epochs`) and refuses larger ones; a
certificate records the profile bounds, never a smaller limit (Section 11.3,
"Budget"). Reaching a bound or a limit is a successful stop: the transaction
keeps every region that it admitted before the stop, admits no further
region, and records the stop in its certificate. Tests may inject smaller
limits, but the stop semantics do not change. The producer is deterministic:
one module, one set of options and one policy give one certificate and one
output, byte for byte. Hash-table iteration is never observable.

Every accepted merge is present in a complete certificate whose trusted
operator/rule authority, canonical input-module digest, exact witness and seed
manifests, region identity, extracted output, and full event
stream are bound by the certificate. `evo-proofcheck` receives the canonical input
module, verifies its digest, and independently recomputes eligibility and both
manifests for every named region rather than trusting the certificate. It
replays every merge, verifies that the extracted root belongs to the original
root e-class, reconstructs the extracted region, and compares its canonical
bytes and output commitment to the certificate. A merge-valid certificate that
names an unrelated root or output is invalid. Any node
carrying or transitively depending on `mem<all>`, `world`, a pointer, a guard,
another may-trap/effectful instruction, or scalar poison is rejected before
insertion. Poison dependence is computed before region formation and is not
re-derived from the extracted expression.

An EVP certificate (`evp1` or `evp2`, Section 11.3) is a standalone proof of
one `before.evo` to `after.evo` transaction.
For P0 matching and extraction, the active node set is therefore exactly the
current before-module witness nodes plus nodes derived in that transaction.
Nodes retained only from an earlier transaction remain queryable in persistent
classes and their proof events remain in persistent history, but they are not P0
match, proof, or extraction candidates for a later certificate. Admitting such a
history-only node without serializing its earlier derivation would make the
second certificate impossible for an independent checker to replay. A future
certificate revision can carry cumulative history; no current EVP version
does.

The exact driver capability is `-Oeq-scalar`, and `-Oeq-scalar --probe` is
implemented. The broader `-Oeq --probe` exits 69 `NOT_IMPLEMENTED`.

Persistence is active for the eligible EvoIR scalar values of the P0 profile.
Sections 8.1 to 8.4 are informative: they describe an architecture of which
this revision implements only the persistent P0 scalar classes of this
section. No C-HIR, EvoMIR or target representation enters an e-class. These
sections do not authorize target lowering, backend nodes, pointer/object
nodes, or any rewrite outside this section.

### 8.1 Architecture

Informative. The equality database persists `EClassId`s across lowering
phases. It is not a pass-local temporary data structure; it is the persistent
state of compiler knowledge.

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
root is an earlier instruction's twin and extract the earlier value.

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

None of the shifts and width changes may trap (Appendix A), the shift's definedness predicate is a separate instruction
that stays excluded, and a shift's amount operand is a witness or seed of its
own sort. Everything else is ineligible, including division and remainder,
every predicate of 3.3.1, `ub.assume`, all pointer and
memory forms, `call`, and all terminators. An instruction absent from this list
is ineligible even if some future analysis would prove it total: the list is the
authority, not the reasoning behind it.

Every root and witness result in this profile has type `bits<N>` with
`1 <= N <= 64`. Comparisons consume one common `bits<N>` sort and produce
`bits<1>`; a shift consumes its amount in a `bits<M>` with `1 <= M <= 64`,
as the 64-bit masks of the 8.10 lattice require (a shift whose amount is wider
is an ineligible region, never a refusal, and the interpreter still executes
it under 3.4); a width change consumes one sort and produces another, both
within the bound. Wider scalar equality needs a later registry revision;
silently truncating a wider constant to the host's integer width is invalid.

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
excluded (otherwise the closure would follow that instruction as a witness,
which contradicts the bound). Exclusion propagates from every operand to its
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
region ineligible. The `definition` origin remains in the manifest format for
a later profile, and the checker recomputes it like any field.

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
region means "not optimized", not an invalid proof; otherwise a budget stop,
which omits every region after it, or an optimizer that deliberately performs
no rewrites could never emit a valid certificate.

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
the ASCII string `aarch64-unknown-linux-gnu/evoir-layout-v4-hfa-natural-alignment` followed by one LF
byte, namely
`9460b084c6b110935328ced6acded2c869d6718a6fdbfa69516b989cc1d59798`.
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
Distinct inputs therefore start in distinct classes. A certificate does not serialize
that implementation node: its seed record creates the corresponding local leaf
class directly, which is why two different seeds can never collapse merely
because both have zero children.

Extraction returns an explicit before-to-after value map. Unchanged values that
are renumbered transfer their existing handles; a new extracted value maps to
the selected root class; a deleted root loses its current-value key but its
class, original node and proof history remain in the database. The map update
and module replacement commit atomically. This is how equality persists while
Section 8.8 is free to renumber textual `%v` identities.

**Transactions.** `ingest_module` seeds the state from the first module, once.
After it, one optimization is one transaction: the producer derives the
regions, the certificate and the reassembled output module, which must pass
module validation; the transaction then applies the admitted merges to a
staged copy of the state, remaps the values and checks the history (below).
Only when every step passes does it publish the module and the state
together. A transaction that made at least one mutation advances the
revision exactly once; one that made none changes nothing. A proof failure,
a reassembly or validation failure, or a disagreement between the
certificate and the state returns the original module with a certificate
that has no region, and leaves the module, graph, maps, proof history and
revision as the transaction entered them. Reaching a bound is not a failure:
the transaction keeps every region admitted before the stop (Section 8.11).
A chain of transactions (Section 8.11, "Runs") stages the state once for the
whole chain and publishes it only after the checker accepts every
certificate of the chain.

Rollback restores the state *and* the module together. A failure that returned
the original module while leaving unions in the database would be invisible in
the returned program and permanent in the state, and it would remain invisible
until some later phase asked what was known. Code that reverts only the module
is correct while the map dies with the call and wrong the moment it does not, so
this is a property of the transaction rather than of any caller.

**Accumulation.** A representation is added to an existing class. A class that
has been optimized retains the representation it started with alongside the one
extraction chose. No later phase adds a representation in this revision; a
phase that does must join the existing class rather than create a new one. An
implementation that replaced the original would be indistinguishable at the
point most tests look, and would differ only when a later phase asked what else
was equal, which is exactly when the difference matters.

**Justification.** Every union carries a proof event. Before a transaction
publishes, the producer replays the accumulated e-graph certificate under the
rules that the certificate itself carries, re-derives each history entry with
its own rule code as an instance of a reviewed rule, and requires the classes
of each entry to be one class of the partition. If a check fails, the
transaction fails as above. This is a self-check of the producer, not an
independent check: the independent check of each transaction is the checker
of Section 11 on its EVP certificate.

### 8.10 Analysis Lattices

An e-class carries facts as well as nodes. Two are defined in this profile;
both are needed to state a rewrite whose legality depends on a value's bits
rather than its syntax.

```text
KnownBits<N> = { mask: bits<N>, value: bits<N> }   -- bit i known iff mask[i]=1,
                                                  -- and then equal to value[i]
ConstValue<N> = KnownBits<N> with mask all ones
```

`ConstValue` is not a separate lattice: a fully known fact is a minimal
element of `KnownBits`, and two different constants are minimal and
incomparable, so no fact lies below them. The contradiction that arises when
two facts disagree on a bit both know is not a fact of this lattice but the
invalid union of the merge paragraph below. This keeps one join and one transfer function per operator
instead of two of each.

**Order.** `a ⊑ b` when every bit known in `b` is known in `a` with the same
value. The top is nothing known. Facts only ever move down within one
saturation transaction: a class's fact after any step is at or below its fact
before.

**Merge.** When two classes are unioned, the merged class's fact is the meet of
the two: every bit known in either is known in the result. If the two disagree
on a bit both know, the union is **invalid** — two values with a differing
known bit cannot be equal. A producer abandons the region that would contain
it (Section 8.7 lets it decline any region), and a checker refuses a
certificate that records it (status 3). That check is part of what makes a
union sound, so it is performed by the checker as well as the producer.

**Facts of seeds.** A seed of origin `input` carries a fact computed once per
function, before regions are formed, by a forward fixed point over the
function in Section 8.6 order: the fact of a block argument is the *join* of
the facts of the values passed to it on every edge entering the block, the
join keeping only the bits every edge knows with the same value (the class
merge of the paragraph above combines knowledge and is the other operation), a
passed value's fact being the transfer of its defining instruction over the
facts of its operands (an unlisted instruction's result, a pointer, and any
excluded value being top); every edge counts, none is pruned by a fact, and
a block argument not yet visited contributes the join's identity. Iteration
stops when no fact changes; the lattice is finite and every step only loses
knowledge, so it terminates, and the order of visits does not change the
result. Entry block arguments are top. A premise may name a seed's class;
the checker recomputes the seed's fact by the same fixed point over
`before.evo`.

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
states.

A shift amount that is not fully known, or is known and not below the width,
transfers to top, because the shift's own definedness predicate (3.3.1)
excludes the out-of-range case and this analysis states nothing about guarded
operations. The transfers are deliberately weak. Each is a statement a checker
can recompute from the region's nodes with no search; a stronger analysis that
needed one would move the judgment into shared code, which Section 11.6
reserves for cases that fail safe.

**Use.** A rewrite may name a fact as a premise. A constant fold (`all_const`) reads
its constants from the nodes and carries no premise; a known-bits identity such as `and(x, m) -> x` when
every bit of `m` known one covers every bit of `x` known nonzero has the two
facts as premises. Premises appear in the certificate and are recomputed by the
checker under Section 11.3.

### 8.11 Bounded Scheduler

Saturation is never run to a fixed point in production. A transaction has
three bounds, nodes, epochs and match fuel, and its certificate records each
one, so a checker confirms that the producer stayed within them rather than
trusting that it did. A run of transactions has a fourth bound (below).

**Tiers.** Rules belong to one of two tiers, and a producer tries the tiers in
order on each candidate:

```text
Tier A   canonical reductions          x+0, x*1, x&0, x|0, double negation,
                                       identity comparisons
Tier B   fact-driven rules             constant folding, known-bits identities
```

Tier B depends on facts that Tier A's reductions may sharpen, which is why the
order is fixed rather than left to the implementation. Within a tier the order
is registry order (Section 11.6). The emitting rules of `p0.scalar.v4` belong
to neither tier; a producer tries them after both, so a fold wins over an
emitted node.

**Match fuel.** Each rule may fire at most `max(256, 8 × n)` times per epoch
in one region, where `n` is the number of nodes the region held when the
transaction began. Fuel is per rule per epoch, so one prolific rule cannot
starve the rest.

**Epochs and nodes.** The ceilings of the P0 profile are `max_nodes = 100000`
and `max_epochs = 1024` for one transaction. The consumed node quantity is
the number of nodes that the regions of the certificate record, and the
consumed epoch quantity is the highest epoch marker (Section 11.3).

**Stop rule.** A transaction stops at the first of: no further rewrite to
propose (*saturated*); no root's best extraction became strictly cheaper in
two consecutive epochs (*no-new-best*); a limit of the producer's own
(*policy*); the epoch, node or fuel bound (*epochs*, *nodes*, *fuel*).
Hitting a bound or a limit is a successful stop, not a failure: the
transaction keeps every region admitted before the stop, and Section 8's rule
that saturation is never needed for correctness is what makes that safe.

**Record.** The certificate carries the bounds, the amounts consumed, the stop
reason, and an epoch marker before each epoch's events. The checker verifies
that consumed does not exceed bound, that no epoch marker exceeds the declared
epoch count, and that an exhaustion reason is consistent with the
recorded consumption. It does not re-run the scheduler; a schedule is a choice,
and what is verified is that the choice stayed inside the declared box.

Informative. The producer of this revision (`crates/evocc-eqopt`) does not
iterate rules over e-classes. It makes one forward sweep over each block in
instruction order and proposes at most one rewrite for each candidate root,
in the tier order above. It then builds one region for each proposal that its
policy keeps, and recomputes the facts and the merges of that region from the
region's own nodes, as a checker does. Both steps stop at the first proposal
that would pass the node limit or the epoch limit: the sweep counts the nodes
of its candidate graphs and its rewrites, and the region builder counts the
certificate nodes and the admitted regions. A rewrite gets the epoch after
the latest epoch that changed a class it matches on, and never an epoch below
the previous marker of its region. Where an existing constant node fits a
right-hand side, the producer reuses the one with the smallest `NodeId`. It
records `stop=policy` when a limit stopped it and `stop=saturated`
otherwise; it never writes `no-new-best`, `epochs`, `nodes` or `fuel`.

**Runs.** A run is a sequence of at most `max_transactions = 8` transactions
over one module, each transaction taking the previous one's output as its
input. The driver admits a smaller ceiling (`--eq-max-transactions`). It
stops the run after a transaction that admits no region or that stops at a
limit, or at the ceiling. Each transaction's certificate stands alone and is
verified alone; the run is verified by digest chaining, the `output` digest of
one certificate being the `input` digest of the next, and a chain that breaks
anywhere fails the run. A run exists because 11.6 lets a left-hand
`const_like` match only a constant present in the before witness closure, so
a constant one transaction derives becomes matchable only in the next. The
count of transactions a run took, and the residual regions after its last, are
telemetry (informative).

### 8.12 Certified Total-Integer Guard Folding

`g0.ub.v1` is a separate, opt-in proof profile, not an extension of P0's
allowlist. Its only roots are the seven total predicate forms in Sections
3.3.1 and 6.1: divisor nonzero, signed div/rem overflow, signed add/sub/mul
definedness and signed/unsigned shift-count definedness. `signed_shl_defined`
remains excluded. Arithmetic operand width N and independent count width M
are each 1..64. The predicate result is `bits<1>`.

Only the root itself is exempt from P0's opcode exclusion. All operands,
witnesses and seeds obey the existing exclusion and all-edge poison fixed
points. Witness closure consists of this one root and P0 operand cones,
including their earlier structural duplicates. Another predicate, load,
effect or excluded cone is not an opaque seed. In particular, a poison
shifted operand forbids folding even when the count alone proves valid.
Facts are solely existing normalized known bits and Section 8.10's all-edge
seed facts; no edge conditions, assumptions or execution results are premises.

For normalized fact `(m,v)` at width N, let `u=((2^N-1) xor m)` and
`s=2^(N-1)`. The unsigned maximum is `v or u`. Signed endpoints are
`[S_N(v or s), S_N(v or (u and (s-1)))]` if the sign bit is unknown, otherwise
`[S_N(v), S_N(v or u)]`. These endpoints are derived from bit facts, not a
new interval lattice. Transfers return exact true `(mask=1,value=1)` only
under the following sufficient conditions, and TOP otherwise:

| Predicate transfer | Sufficient condition |
| --- | --- |
| nonzero | a known-one divisor bit |
| signed div/rem | lhs facts exclude INT_MIN or rhs facts exclude all-one -1 |
| signed add | sum of the lower endpoints and sum of the upper endpoints are in signed range |
| signed sub | lower lhs minus upper rhs, and upper lhs minus lower rhs, are in signed range |
| signed mul | all four endpoint products are in signed range |
| unsigned count | maximum M-bit unsigned count is less than mathematical N |
| signed count | M-bit sign is known zero and maximum unsigned count is less than N |

Endpoint calculations are mathematical integers. N is never truncated to M
bits or masked as a machine shift count. Each transfer has a width-parametric,
kernel-only `TransferSound` theorem via `BitVec.toInt`/integer order reasoning;
the seven rows do not use `bv_decide`, finite-width enumeration or Mathlib.
The checker independently recomputes the seven rows and existing seed facts.

The only extraction is `const.bits<1> 1` at the predicate's original result
identity. Section 8.8 whole-module reassembly remains unchanged: obligations,
their reasons, other users and conservative effect summaries remain in EvoIR.
Checked MIR may omit an obligation's branch only when its operand directly
names canonical `const.bits<1> 1`. Publication uses the independently
verified chain and staging wrapper;
diagnostic MIR attribution cannot authorize this profile or installation.

## 9. Textual Syntax

### 9.1 Purpose and Canonical Status

The EvoIR textual syntax is the module interchange format. It is used by test
fixtures, by `evo-run`, by `--emit=evoir`, and by the checker, which reads
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

**Target identity.** Text with no target line describes the implicit target
`aarch64-unknown-linux-gnu` with its AArch64 LP64 `TargetDataLayout`. The text
of any other target starts with the line `target TRIPLE`, then a blank line
before a nonempty module (Section 9.3). Today that target is
`aarch64-apple-darwin`, whose data layout Section 10.5 states; `--emit=evoir`
writes its line. A reader serves one target: `parse_section9_module` refuses
text that names a target, and `parse_target_module` refuses text that does not
name the target it was asked for. The rewrite checker admits the implicit
Linux profile and the explicit `aarch64-apple-darwin` profile. The bytes of
`before.evo` select the profile; `after.evo` and the independently reconstructed
output must use that same profile. The target line is included in both
certificate digests, chain identity and canonical reassembly. The persistent
state is bound to the selected target and cannot change it during a chain.
Unknown or mixed text profiles are malformed input (exit 2); changing the
profile of both files without a matching certificate gives exit 3. The explicit
legacy reader remains Linux-only. No host default or environment variable
selects the meaning of these bytes.

All five registries (`p0.scalar.v1` through `p0.scalar.v4` and `g0.ub.v1`) are
admitted for both profiles. Each rule uses total `bits<1..64>` operations or
the seven total integer guard forms. Widths and signed comparison/shift
choices are explicit in the terms. Their denotations and known-bits transfers
do not read a C type layout, ABI, pointer representation or floating-point
format. Eligibility still excludes pointer, memory and floating-point
rewrites; those instructions and their metadata remain subject to exact
whole-module reassembly. This adds no rule and does not extend the separate
`core-v1` proof-required target profile.

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
hex4          := hex_byte hex_byte
hex8          := hex4 hex4
hex16         := hex8 hex8
hex32         := hex16 hex16
signed_integer := integer | "-" positive_int
bit           := "0" | "1"
newline       := the single byte "\n"
```

Integer literals in instruction operands are unsigned decimal. Hexadecimal and
binary spellings are **not** part of this format and are rejected by Section 9.5:
a parser that accepts a spelling no producer emits can read a file no producer
could have written, and the checker's whole value rests on that being
impossible.

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
module          := [target_line [newline body]] | body
                   -- a target line names any target but the implicit one (9.1)
target_line     := "target " triple newline
triple          := (letter | digit | "_" | "-")+
body            := ε | segment (newline segment)*
                   -- at most one of each, in this order:
                   -- global_seq, extern_seq, declare_seq, alias_seq, function_seq, module_asm_seq, tls_seq, lifecycle_seq
global_seq      := global+
extern_seq      := extern_object+
extern_object   := "extern " symbol " size=" (integer | "unknown")
                   " align=" integer [declaration_visibility] [" linkage=weak"] newline
declare_seq     := declare+
declare         := "declare " symbol "(" sig_param_list ")"
                   " -> (" type_list ")" " cc = " ident [declaration_visibility] [" linkage=weak"] newline
alias_seq       := alias+
alias           := "alias " symbol " = " symbol [visibility_mark]
                   [" linkage=weak"] newline
function_seq    := function (newline function)*
module_asm_seq  := module_asm+
module_asm      := "asm " quoted_string newline
tls_seq         := tls_symbol+
tls_symbol      := "tls " symbol " model=" tls_model newline
tls_model       := "global_dynamic" | "local_dynamic" | "initial_exec" | "local_exec"
lifecycle_seq   := lifecycle+
lifecycle       := ("init " | "fini ") symbol " priority=" integer newline

global          := "global " symbol " origin=" integer
                   " kind=" global_kind " visibility=" visibility
                   [" linkage=weak"] " size=" integer " align=" integer
                   " bytes=" hex_byte* " initialized=" bit*
                   [" readonly=true"] [" section=" quoted_string]
                   [" relocs=[" reloc (", " reloc)* "]"] newline
reloc           := "(" integer ", " reloc_target ", " signed_integer
                   [", exposed" | ", diff=" reloc_target ", bits=" integer] ")"
reloc_target    := symbol | symbol block_label
global_kind     := "file" | "function_static" | "string_literal" | "compound_literal"
visibility      := "external" | "internal" | "hidden" | "protected"
visibility_mark := " visibility=" ("internal" | "hidden" | "protected")
declaration_visibility := " visibility=" ("hidden" | "protected")

type            := "bits<" positive_int ">"
                 | "f16" | "bf16" | "f32" | "f64" | "f128"
                 | "ptr<" address_space ">"
                 | "mem<" region ">"
                 | "world"
                 | "vec<" positive_int ", " type ">"
                 | "tuple<" type_list ">"
                 | "fnptr<" signature ">"
                 | "void"
signature       := "(" sig_param_list ")->(" type_list ")"
sig_param_list  := ε | sig_param_item (", " sig_param_item)*
sig_param_item  := sig_param | "..."
sig_param       := type [record_mark | " sext"]
record_mark     := byval_mark | " sret(" integer ", " integer [hva_mark | hfa_mark] ")"
byval_mark      := " byval(" integer ", " integer [hva_mark | hfa_mark] ")"
hfa_mark        := ", hfa=" integer "x" ("f16" | "f32" | "f64" | "f128") [", abi_align=16"]
hva_mark        := ", hva=" positive_int "xvec<" positive_int ", bits<" positive_int ">>"
address_space   := "generic" | "global" | "stack" | "threadlocal"

function        := "func " symbol "(" param_list ")"
                   " -> (" type_list ")"
                   " cc = " ident [visibility_mark] [" linkage=weak"]
                   [" align=" integer] [" stack_protect"]
                   [" patchable_entry=" integer "," integer]
                   [" section=" quoted_string] newline
                   indent effects_clause newline
                   "{" newline
                   block_seq
                   "}" newline
block_seq       := block (newline block)*
effects_clause  := "effects { reads=" region_set ", writes=" region_set
                   ", volatile=" bool ", atomic=" bool ", io=" bool
                   ", fenv=" bool
                   ", may_trap=" bool ", may_unwind=" bool " }"
region_set      := "{}" | "{" region ("," " " region)* "}"
region          := "all" | "unknown" | "global" | "stack" | "heap" | "escaped"
bool            := "true" | "false"
param_list      := ε | param_item (", " param_item)*
param_item      := param | "..."
param           := param_name ": " type [record_mark | " sext"]
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
    value_name " = const.f16 0x" hex4
  | value_name " = const.bf16 0x" hex4
  | value_name " = const.f32 0x" hex8
  | value_name " = const.f64 0x" hex16
  | value_name " = const.f128 0x" hex32
  | value_name " = fp." fp_op " " value_list " : " type
  | "(" value_name ", " value_name ") = fp.strict." strict_fp_op
        " " value_list " : " type
  | value_name " = const." type " " integer
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
  | "(" value_name ", " value_name ", " value_name ") = tls_addr " symbol ", " value_name ", " value_name
  | value_name " = vec." ("pack" | "extract" | "bitcast" | "aese" | "aesd" | "aesmc" | "aesimc")
        " " value_list " : " type
  | value_name " = func_addr " symbol " : " type
  | "(" value_name ", " value_name ", " value_name ") = alloca "
        value_name ", " value_name
        " [origin=" integer ", size=" integer ", align=" integer "]"
  | "(" value_name ", " value_name ", " value_name ") = alloca.dynamic "
        value_name ", " value_name ", " value_name
        " [origin=" integer ", align=" integer "]"
  | "(" value_name ", " value_name ") = lifetime.end "
        value_name ", " value_name ", " value_name
  | value_name " = pointer_valid " value_name ", " value_name
  | value_name " = access_in_bounds " value_name ", " value_name
        " [size=" integer ", align=" integer "]"
  | "(" value_name ", " value_name ", " value_name ") = load" volatile_marker
        " " value_name ", " value_name ", " value_name " : " type [mask_mark]
  | "(" value_name ", " value_name ") = store" volatile_marker
        " " value_name ", " value_name ", " value_name ", " value_name [mask_mark]
  | "(" value_name ", " value_name ", " value_name ") = atomic.load " atomic_order volatile_marker
        " " value_name ", " value_name ", " value_name " : " type
  | "(" value_name ", " value_name ") = atomic.store " atomic_order volatile_marker
        " " value_name ", " value_name ", " value_name ", " value_name
  | "(" value_name ", " value_name ", " value_name ") = atomic.rmw." atomic_rmw_op
        " " atomic_order volatile_marker " " value_name ", " value_name ", " value_name
        ", " value_name [", " value_name] " : " type
  | "(" [value_name ", "] value_name ", " value_name ") = atomic.copy." atomic_copy_op
        " " atomic_order volatile_marker " " value_name ", " value_name ", " value_name
        (", " value_name)+ " [size=" integer ", align=" integer "]"
  | "(" value_name ", " value_name ") = copy" [".volatile.read" | ".volatile.write" | ".volatile.readwrite"] " "
        value_name ", " value_name ", " value_name ", " value_name
        " [size=" integer ", align=" integer "]"
  | value_name " = ptr.offset." direction "." index_signedness " "
        value_name ", " value_name " [scale=" integer "]"
  | value_name " = ptr.to_int " value_name
  | value_name " = block_addr " block_label " : " type
  | value_name " = return_address : " type
  | value_name " = frame_address : " type
  | value_name " = frame_address depth=" positive_int " : " type
  | value_name " = ptr.from_int." type " " value_name
  | "(" result_list ") = call " callee "(" call_arg_list ")"
  | "(" result_list ") = call.returns_twice " symbol "(" call_arg_list ")"
  | "(" result_list ") = va_start " value_name ", " value_name ", " value_name
  | "(" result_list ") = va_arg." type " " value_name ", " value_name ", " value_name
  | "(" result_list ") = va_arg.byval " value_name ", " value_name ", " value_name
        ", " value_name " [size=" integer ", align=" integer [hva_mark | hfa_mark] "]"
  | "(" result_list ") = va_end " value_name ", " value_name ", " value_name
  | "(" result_list ") = asm" volatile_marker " " asm_payload

term_body :=
    "br " block_label "(" arg_list ")"
  | "br.indirect " value_name " [" branch_edge (", " branch_edge)* "]"
  | "cond_br " value_name ", " block_label "(" arg_list ")"
        ", " block_label "(" arg_list ")"
  | "ret" return_operands
  | "trap." trap_reason
  | "(" result_list ") = asm.goto" volatile_marker " " asm_payload
        " labels [" asm_label_list "] fallthrough " block_label "(" arg_list ")"

branch_edge     := block_label "(" arg_list ")"
return_operands  := ε | " " value_name ("," " " value_name)*

arith_op         := "add" | "sub" | "mul" | "sdiv" | "udiv" | "srem" | "urem"
                  | "shl" | "ashr" | "lshr" | "and" | "or" | "xor"
cmp_op           := "eq" | "ne" | "slt" | "sle" | "ult" | "ule"
cast_op          := "sext" | "zext" | "trunc"
fp_op            := "add" | "sub" | "mul" | "div" | "fma" | "neg"
                  | "oeq" | "une" | "olt" | "ole" | "ogt" | "oge"
                  | "quiet_olt" | "quiet_ole" | "quiet_ogt" | "quiet_oge"
                  | "convert" | "from_signed" | "from_unsigned"
                  | "to_signed" | "to_unsigned" | "to_bits" | "from_bits"
                  | "to_int_defined." ("s" | "u") positive_int
                  | complex_fp_op
strict_fp_op     := "add" | "sub" | "mul" | "div" | "fma"
                  | "oeq" | "une" | "olt" | "ole" | "ogt" | "oge"
                  | "quiet_olt" | "quiet_ole" | "quiet_ogt" | "quiet_oge"
                  | "convert" | "from_signed" | "from_unsigned"
                  | "to_signed" | "to_unsigned" | complex_fp_op
complex_fp_op    := "cmul.real" | "cmul.imag" | "cdiv.real" | "cdiv.imag"
bit_op           := "ctlz" | "cttz" | "ctpop" | "bswap"
binary_predicate := "signed_add_defined" | "signed_sub_defined"
                  | "signed_mul_defined" | "signed_div_rem_no_overflow"
shift_predicate  := "shift_count_defined" | "signed_shl_defined"
ub_reason        := "division_by_zero" | "remainder_by_zero" | "signed_overflow"
                  | "invalid_shift_count" | "invalid_signed_left_shift"
                  | "bit_builtin_zero_operand"
                  | "floating_to_integer"
trap_reason      := "scalar_function_fell_off_end" | "unreachable_reached" | "builtin_trap"
volatile_marker  := ε | " volatile"
mask_mark        := " [mask=" integer "]"
atomic_order     := "relaxed" | "consume" | "acquire" | "release" | "acq_rel" | "seq_cst"
atomic_rmw_op    := "add" | "sub" | "and" | "or" | "xor" | "nand" | "exchange" | "cmpxchg"
atomic_copy_op   := "load" | "store" | "exchange" | "cmpxchg"
direction        := "add" | "sub"
index_signedness := "signed" | "unsigned"
result_list      := value_name ("," " " value_name)*
value_list       := value_name (", " value_name)*
arg_list         := ε | value_name ("," " " value_name)*
call_arg_list    := ε | call_arg (", " call_arg)*
call_arg         := value_name [byval_mark]
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

The optional `...` occurs at most once in a parameter list, immediately before
its final `mem<all>, world` pair, as required by §1.1; it is not a parameter
binder and does not advance `%arg` numbering. Call-site `byval` marks apply only
to anonymous arguments; fixed parameter marks come from the callee signature.
The atomic instruction's order/type rules remain operation-specific: adding
`acq_rel` to the spelling vocabulary does not make it legal for load/store.
Only `cmpxchg` has the additional expected-value operand. Relative relocations
require the same-function label/width rules, and cannot also be `exposed`.

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

For `br.indirect`, operand traversal visits the control target first, then each
edge's arguments in written positional order; edges are ordered by BlockId.
Renumbering rewrites both destination IDs and every argument ID, then restores
destination order, without merging bindings on other terminators that happen
to name the same destination.

#### 9.3.1 Canonical Shapes

The productions above are for a reader. This table is the machine-checkable form
of the same content: every shape a conforming renderer can emit, with `<>`
standing for exactly one operand, attribute value or type. `tools/check-textual-grammar.py`
compares the renderer against this table, so a form whose arity or literal text
drifts is a failure rather than a discrepancy nobody notices.

The check compares shapes, not names: a name check passes while a unary
predicate is written with two operands.

```text
<> = const.<> <>
<> = const.<> 0x<>
<> = fp.<> <> : <>
(<>, <>) = fp.strict.<> <> : <>
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
(<>, <>, <>) = tls_addr @<>, <>, <>
<> = vec.<> <> : <>
<> = func_addr @<> : <>
(<>, <>, <>) = alloca <>, <> [origin=<>, size=<>, align=<>]
(<>, <>, <>) = alloca.dynamic <>, <>, <> [origin=<>, align=<>]
(<>, <>) = lifetime.end <>, <>, <>
<> = pointer_valid <>, <>
<> = access_in_bounds <>, <> [size=<>, align=<>]
(<>, <>, <>) = load<> <>, <>, <> : <>
(<>, <>) = store<> <>, <>, <>, <>
(<>, <>, <>) = load<> <>, <>, <> : <> [mask=<>]
(<>, <>) = store<> <>, <>, <>, <> [mask=<>]
(<>) = copy<> <>, <>, <>, <> [size=<>, align=<>]
(<>, <>, <>) = atomic.load <><> <>, <>, <> : <>
(<>, <>) = atomic.store <><> <>, <>, <>, <>
(<>, <>, <>) = atomic.rmw.<> <> <>, <>, <>, <> : <>
(<>) = atomic.copy.<> <><> <> [size=<>, align=<>]
<> = ptr.offset.<>.<> <>, <> [scale=<>]
<> = ptr.to_int <>
<> = block_addr <> : <>
<> = return_address : <>
<> = frame_address : <>
<> = ptr.from_int.<> <>
(<>) = call<> @<>(<>)
(<>) = call <>(<>)
(<>) = va_start <>, <>, <>
(<>) = va_arg.<> <>, <>, <>
(<>) = va_arg.byval <>, <>, <>, <> [size=<>, align=<>]
(<>) = va_end <>, <>, <>
(<>) = asm<> <> <>, <> outputs [<>] inputs [<>] clobbers [<>]
(<>) = asm.goto<> <> <>, <> outputs [<>] inputs [<>] clobbers [<>] labels [<>] fallthrough <>
<> = frame_address depth=<> : <>
br <>
br.indirect <> [<>]
cond_br <>, <>, <>
ret
ret <>
trap.<>
global @<> origin=<> kind=<> visibility=<> size=<> align=<> bytes=<> initialized=<>
func @<>(<>) -> (<>) cc = <>
effects { reads=<>, writes=<>, volatile=<>, atomic=<>, io=<>, fenv=<>, may_trap=<>, may_unwind=<> }
{
```

`load<>` and `store<>` show the optional volatile marker, which is either empty
or one space followed by `volatile`. A masked load or store has its own shape,
because its mask is a field. `call<> @` shows the optional mark of a
direct call, which is either empty or `.returns_twice` (3.10). `ret` appears
twice because a function returning nothing emits the mnemonic alone; a grammar
admitting only the second form could not represent a legal module.

#### 9.3.2 Shapes Not Yet Emitted

A revision can lead its implementation. A shape that the renderer does not yet
emit is listed in the first block below, so the gap is declared rather than
discovered. No shape is pending in this revision. This does not assert full
source-language support.

```text
```

The empty `ret`, mandatory effects clause, `func_addr`, indirect callee, full
global attributes, positional `%arg` binders and parenthesized empty block
argument list are all emitted. Whole-module syntax tests, rather than the shape
audit, carry the latter two rules.

A shape that a later form replaces is listed in the second block. The form each replaces is **superseded**, and a
conforming parser rejects it. No shape is superseded in this revision; the
empty block remains machine-readable so the audit fails if an obsolete form is
reintroduced without being declared.

```text
```

### 9.4 Canonical Byte Form

A conforming renderer emits exactly these bytes. Nothing here is stylistic:
each rule removes a degree of freedom that would otherwise make two renderings
of one module disagree on their digest.

| Position | Rule |
|---|---|
| Line ending | `\n`. Never `\r\n`. |
| Final byte | `\n`. The last module item's line is followed by exactly one newline and nothing else. |
| Global order | Source-site origin ordinal, ascending. |
| Function order | Module definition order, which lowering fixes from source order. |
| Between globals | Exactly one newline; globals form one block with no blank lines inside it. |
| Globals to functions | Exactly one blank line, and only when the module has at least one global and at least one function. |
| Globals-only module | The final global's line ends the file. No trailing blank line. |
| Empty module | The empty byte string. Not a single newline, which would be a module with one blank line and is therefore malformed. |
| Between functions | Exactly one blank line. |
| TLS/lifecycle tables | Follow assembly, separated from preceding nonempty tables by one blank line. TLS retains table order; lifecycle retains registration order. |
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
- A `volatile` marker outside the positions specified for load/store, atomic
  access and assembly. Volatile copies instead use their explicit suffix.
- A `.returns_twice` mark on an indirect call, or any other suffix of `call`.
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
- A mask on a volatile access or on an access that is not `bits<8>`,
  `bits<16>`, `bits<32>`, `bits<64>` or `bits<128>`, or a mask that selects
  every bit of the access or a bit beyond it (3.8).

The two categories carry distinct exit statuses in Section 11.4, because a file
that cannot be read and a module that must not be trusted are different
findings and are acted on differently.

### 9.6 Complete Example

```text
func @sum(%arg0: ptr<generic>, %arg1: bits<64>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
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
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
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
    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=true, fenv=true, may_trap=true, may_unwind=true }
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
- A `ub.assume` without a reason suffix is malformed. The accepted suffixes
  are those of the `ub_reason` production of Section 9.3.
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
| `_Float16`, `__fp16` | `f16`; the value of a `_Float16` operation is `f32` (3.13.6) |
| `__bf16` | `bf16`; the value of an operation is `f32` (3.13.6) |
| `float` | `f32` |
| `double` | `f64` |
| `long double` | `f128` (3.13.4) |
| pointer to object `T*` | `ptr<generic>`; storage duration remains object metadata |
| pointer to function | `fnptr<Sig>`, where `Sig` is the complete lowered parameter/result interface including trailing tokens |
| `void` (in return position) | `void` (empty return type) |

The mapping is mediated by the target data-layout service. The AArch64
implementation is `TargetDataLayout`, which provides `layout_of`,
`pointer_layout`, and `record_layout` queries; lowering and interpreters must
not hard-code target widths, pointer size, or pointer alignment.

#### 10.1.2 Qualifier Lowering

| C Qualifier | EvoIR Representation |
|---|---|
| `const` | No IR effect. Purely a C-HIR check. |
| `volatile` | Exact `load volatile` / `store volatile` syntax and ordered world-trace events. |
| `restrict` | Preserved in C-HIR only; `NoAlias` facts are outside this revision (5.5). |
| `_Atomic` | The target's atomic layout; `seq_cst` `atomic.load` and `atomic.store` for a read and a simple assignment of an admitted scalar, and a compare-exchange loop for an update (10.1.21). Other atomic objects are `E2702`. |

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
may remain in SSA. Array indexing passes
the target element size as `scale` and preserves the promoted index's
signedness in the `ptr.offset` mnemonic. Every
source-level pointer-arithmetic result is followed by `pointer_valid`, and every
executed dereference is immediately preceded by a matching
`access_in_bounds`. Scope and frame exits end automatic lifetimes.

**Private integer cells.** Optimized C lowering can replace the ordinary
loads and stores of a private fixed object with local SSA values and block
arguments. The admitted cells are nonoverlapping, complete `bits<8>`,
`bits<16>`, `bits<32>` or `bits<64>` representations at constant offsets.
The pointer must not escape, and every incoming edge, including backedges,
must agree on its object and offset. Each allocation site must execute at
most once. The pass keeps `alloca`, `lifetime.end`, pointer and bounds checks.
It must show that every removed store consumes initialized bits and that
every removed load is preceded by initialization on all incoming paths. A
masked store initializes only part of its cell, so it never counts as that
initialization.
Allocation and lifetime end clear initialization facts. Uninitialized SSA
seeds are transport only; they must not reach a semantic read.

A nonvolatile `copy` can participate only when its source is one complete,
initialized private integer cell of 1, 2, 4 or 8 bytes. The replacement must
retain source and destination access checks. A private destination becomes
an SSA assignment; an external destination keeps an ordinary store. Private
copy endpoints are admitted together or retained together. This excludes
padding, pointer representations, partial cells, unknown source bytes,
overlap other than an identical cell, volatile and atomic accesses, escaped
addresses and assembly. The bounded implementation admits at most 64 objects,
64 cells and 4096 added block arguments; excess cases retain their memory
operations. This is a C lowering optimization, not a certified EqSat rule or
an extension of the proof-required profile.

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

**Alignment is an object property, not a type.** A
declared alignment (`aligned(N)`, `_Alignas`) never enters the type of an
object and therefore never enters the manifest's or any redeclaration's type
identity: `extern long a[1]; long a[1] __attribute__((aligned(16)));` is one
object of type `long[1]`, compatible with `long[1]`, whose alignment is the
maximum over every declaration of it in the unit, in either order. A manifest type comparison that reads alignment is a defect.

**External symbols.** A function that the unit declares and calls or takes the address of, but does not
define, lowers to one `declare` (Section 2.1, Section 9) carrying its
complete lowered signature, with record parameters and results in the
pointer form of 10.1.3 and the `...` marker of 1.1 when its prototype ends
in `...`; a function declared without a prototype (`int f();`) and called
takes the promoted types of its first call, and a later call with other
promoted types is `E2931`. An object declared `extern` and used (loaded,
stored or address-taken) but not defined lowers to one `extern` line with
its type's size, `unknown` for an incomplete type such as `extern int a[];`,
and its alignment (the maximum over declarations). A declared but
unused symbol produces nothing. The two interpreters execute a call to an
external function only for the library set of 10.1.14 (`memset`, `memcpy`,
`memmove`, `memcmp`, `strcmp`, `strncmp`, `strlen`), which they implement
on the byte-and-initializedness model (`memset` initializes the bytes it
writes, `memcpy` and `memmove` transfer initializedness exactly as `copy`,
the readers require initialized bytes up to the last one they examine and
report `E3001` otherwise), and for the checked entry points `__memset_chk`,
`__memcpy_chk` and `__memmove_chk` of 10.1.14. A checked entry point compares
its length with its last argument, the destination size. A larger length is
`E3066` (`object-size-check-failed`), because the C library stops the
program there; otherwise the call is the plain function. Every other
external call is `E3039`
(`external-call-not-executable`) when reached. `global_addr` of an external
object is a distinct, stable provenance that may be compared and passed;
a `load` or `store` through it is `E3040` (`external-object-not-executable`)
when reached. Neither code is a front-end refusal: the unit lowers, the
e-graph treats the call by its top summary (Section 4), and the machine lane
calls or addresses the symbol and lets the linker resolve it (EvoMIR
Section 8). A call of `memset` after `__builtin_memset` with no declaration
in scope is still an implicit declaration, so the synthesized prototype below is the only
source of the `declare` for a builtin.

**Library builtins introduce no identifier.** A use of
`__builtin_memset` and the other library builtins of 10.1.14 lowers to a
call of the library function's symbol with the prototype 10.1.14 gives it,
but declares nothing in the source: `static int memset = 3;` after
`__builtin_memset(p, 0, 1)` parses, and `memset(p, 0, 1)` after it is an
implicit declaration. The prototype the front end synthesizes therefore
lives in the builtin namespace, keyed by the library function's name, once
per unit, visible in no scope, and never in a function's scope: the manifest
carries it as a unit-level function symbol, exactly as it carries a function
the unit declares at file scope and calls without defining. It is compared
against a visible declaration of the same library function only through
`E2927`, and a source object or function named like it is a different
symbol. If lowering would emit both a library function declaration and a
same-named linked object, it refuses with `E2946` at the object's declaration.
It does not rename a source object or let an object satisfy a function call.
An automatic or block-static object with the same source name does not cause
this collision; block-static storage has a separate emitted name. This is a
lowering limit, not a source namespace error or a claim that the program has UB.

**Declarations with an incomplete enumeration return type.**
`enum e f(unsigned int);` with `e` not yet complete is an accepted
declaration. A function that is only declared and
never defined, called or address-taken produces nothing in EvoIR, because
EvoIR carries definitions and the calls that name them, not declarations
(Section 3.10). A call or `func_addr` whose function type
still names an incomplete enumeration at that point is `E2002`'s neighbour
`E2930` (`use-with-incomplete-enumeration`) at the use. An enumeration completed
before the use has its completed type there, as usual.

**Arithmetic on `void *`.** `void *` participates in pointer arithmetic with
scale 1 (`sizeof(void)` is 1, 10.1.19), in every form: `p + n`, `p += n`,
`p++`, `--p`, subtraction of two `void *`. `p += 3` advances three bytes.

**Zero-sized elements.** Arithmetic and indexing on a pointer to a GNU
zero-sized type (an empty record or a zero-length array) evaluate the index,
compound and prefix/postfix updates included, and move the pointer by zero
bytes, keeping its identity. Lowering consumes the index in a zero product and
emits `ptr.offset` with scale 1, so the `pointer_valid` obligation and the
runtime poison, lifetime and access checks still apply.

**128-bit assignment.** `__int128` and `unsigned __int128` objects are
assigned, loaded and stored exactly as any integer object, and the
checked-arithmetic builtins of 10.1.15 accept them.

**Character pointers of differing signedness.** A pointer to `char`, `signed
char` or `unsigned char` (with or without qualifiers on the pointee) converts
implicitly to a pointer to any other of the three in initialization,
assignment, argument passing and return. This profile accepts it without a
diagnostic. The three pointee types have identical size, alignment and
representation, so no access changes meaning. In the same four contexts, a
conversion between otherwise compatible non-atomic pointer types, or between
these character pointers, can discard immediate `const` or `volatile` pointee
qualification without a diagnostic. Nested-pointer and atomic qualification
rules are unchanged, and no write through a const lvalue becomes valid.

**Casts to record and union types.** A cast to the operand's own record or
union type, ignoring top-level qualifiers, yields the operand's value. A GNU
cast to a union type constructs a union value from an operand whose type, after
array and function decay and ignoring top-level qualifiers, is compatible with
a member type; a narrow bit-field member does not match, a full-width one does.
In a static initializer the member value must be constant. In both forms the
operand is evaluated once and the result is not an lvalue; lowering uses the
existing aggregate allocation and copy forms and keeps volatile source
accesses. A cast to a struct type from an incompatible type, including a
different struct tag, is `E2953` (`incompatible-record-cast`). A union cast
with no compatible member, or one that would need an assignment conversion, a
different tag or the loss of nested pointer qualifiers, is `E2942`
(`incompatible-union-cast`).

**Pointer difference.** `a - b` on two pointers to `T` has type `long`
(`ptrdiff_t`); on two `void *` the scale is 1. Lowering: `ptr.to_int` of each operand,
`sub.bits<64>`, then `sdiv.bits<64>` by `sizeof(T)` (1 for `void`), which
is exact whenever the program is defined. A `divisor_nonzero` predicate and
`ub.assume.division_by_zero` precede the division. If `T` is zero-sized,
both interpreters report `E3002`, and checked native code traps with `brk #3002`.
C's same-object obligation cannot be checked after this lowering loses pointer
identity through `ptr.to_int`. The C-HIR interpreter checks the enclosing
memory-object identity and exact element difference. It refuses distinct
objects or a non-exact difference as `E3007` (`unsupported-interpreter-input`,
with an `unsupported-interpreter-expression` reason); these are not `E3013`
findings.
Its object identity check is not a complete C subobject relation.

`--check` executes the lowered EvoIR, not the C-HIR interpreter. It does not
check the same-object or exact-element obligations of pointer subtraction, or
the same-object obligation of ordered pointer comparison. Thus `&x[3]-&y`,
`&a<&b` for distinct objects, and a non-exact element difference can complete
without a finding. The completion record states these limits. The machine
also computes addresses and differences without these checks. This does not
remove the separate pointer formation, access, lifetime or zero-stride checks.

**128-bit execution.** `__int128` and `unsigned __int128` values are
`bits<128>` scalars. A `bits<128>` value lowers, is interpreted and is
optimized exactly as any other `bits<N>`: `load`/`store` at 16 bytes and
alignment 16, `copy` of records holding one, `zext`/`sext` from and `trunc` to
a narrower width, `add`, `sub`, `mul`, `and`, `or`, `xor`, `shl`, `lshr`,
`ashr`, every `icmp`, the predicates, block arguments, parameters, results
and anonymous arguments. Both interpreters extend their scalar value model to
128 bits for these forms. `sdiv`, `udiv`, `srem` and `urem` on `bits<128>`
execute in both interpreters, and hosted native lowering calls the ordinary
pair-ABI runtime helpers `__divti3`, `__udivti3`, `__modti3` and `__umodti3`.
Division by zero remains E3002, remainder by zero E3003, and signed MIN/-1
E3004 for either operator. The native runtime dependency is explicit, not a
freestanding-library guarantee. The machine interpreter models only these four
helpers, with the same register-pair interface; when the module defines a
function of that name it executes the definition instead, and it still refuses
every other unresolved callee. The register-pair convention for passing and
returning one is EvoMIR's (EvoMIR Section 6). Example values:
`((u128)0xffffffffffffffff * 0xfffffffffffffffe) >> 64` is
`0xfffffffffffffffd`; `((u128)0x123456789abcdef0 * 0xfedcba9876543210) >> 37`
is `0x56bbeba1111b6c47`; `(1 << 100)` has high half `0x1000000000`;
`(((u128)1 << 64) | ~0ul) + 1` is `2:0`.

**What may root a place.** Any expression of pointer type may be the root of a
place through `*`, `->` or `[]`: a variable, a loaded pointer, an address-of,
a call result, a conditional expression, a statement expression, a cast, a
comma expression. The rule has one implementation; there is no list of
admitted root forms. Examples: `*get(p) = (lock_t){ .raw = 0 }` and
`(c ? p : (void *)0)->v`.

**Aggregate copies.** Assignment of a record or union, copy-initialization of
an automatic object from another object or from a call result, passing one by
value and returning one by value each lower to one `copy` of the type's size
at the type's alignment (Section 3). The C-HIR interpreter performs the same
byte-and-bitmap transfer. A `volatile`-qualified record without bit-field
members is copied member by member with `volatile` scalar accesses in
declaration order. A complete, naturally aligned 1/2/4/8-byte `volatile`
union is read or written as one representation access through Section 3.8,
without selecting a member; unsupported sizes/alignment and volatile record
copies containing bit-fields retain `E2922` (`volatile-aggregate-copy-unsupported`).
An `_Atomic` object is read and written through atomic accesses (10.1.21).
A read of an atomic object that 10.1.21 does not admit is `E2702` at the
read, as an assignment or an update is at its target; `sizeof`, `&` and the
other operands that read nothing stay admitted. The padding bytes of an initialized
aggregate are uninitialized, and after a copy so are the destination's. The
reference compilers differ on padding bytes after `b = a`; both behaviors
are consistent with "unspecified", so the profile transfers initializedness
rather than choosing bytes.

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
the backend must be told which pointers are records: a record parameter is written `%argN: ptr<generic> byval(size, align)` and the
return destination `%argN: ptr<generic> sret(size, align)`, in the function
header, in every `declare`, and in every `fnptr<Sig>` naming such a
signature; the attribute is part of the signature for equality. `size` and
`align` are the record type's; a zero `size` conveys no bytes and uses no
alignment-filler registers. The machine maps these to the platform
convention (EvoMIR Section 6), which can keep a small record in registers.

**Narrow integers across calls on Darwin.** The Apple arm64 convention has a
caller extend an integer argument narrower than 32 bits, and a callee such a
result, to 32 bits by its C signedness. On `aarch64-apple-darwin` a parameter
whose C type is a signed integer narrower than 32 bits (`signed char`,
`short`, and plain `char`, which is signed there) is written
`%argN: bits<N> sext`, in the function header, in every `declare` and in every
`fnptr<Sig>`; the mark is part of the signature for equality, and it marks
only a `bits<N>` with `1 < N < 32` (validation). An unmarked narrow integer is
extended by zeros (EvoMIR Section 6.2). A function whose C result type is a
signed integer narrower than 32 bits returns `bits<32>`: `return e;` converts
`e` to the C result type and then sign-extends it, and the caller truncates
the call result to the C result type. Other results, and every Linux
signature, are unchanged.

**Flexible array members.** A record whose last member is an array of unknown
bound may be initialized at file scope with elements for that member; the
object's storage is the record size plus the elements, `sizeof` the type is
unchanged. At block scope such an initializer reports
`E2919` (`flexible-array-initializer-not-static`) at the initializer.
The file-scope form applies only to the declared record's own final flexible
member. A nonempty initializer for a flexible member reached through a nested
record or union subobject is `E2202`; an empty list there is accepted and adds
no storage. The direct member can be initialized from a narrow, wide or empty
string literal, including its terminating zero, without changing `sizeof`.

**Modifiable lvalues.** An lvalue is modifiable unless its own type is
const-qualified, is an array type, or is a structure or union with a
const-qualified member at any depth. Qualifiers of a pointed-to type never
qualify the pointer: `const char * const *p` is modifiable and `*p` is not.
The test is one predicate on the lvalue's type, applied at every assignment
and update site, with no list of admitted forms.

**Array lvalues.** Every expression of array type that is an lvalue decays
to a pointer to its first element wherever an array expression may: a named
object, an array-typed member of a structure or union reached through `.` or
`->` (fixed-size, zero-length or flexible), an element of an array of arrays,
a string literal, and a compound literal. One decay rule serves them all, so
`buf->head->iov_len` with `struct kvec head[1]`, `f->fib6_nh->fam` with a
flexible `fib6_nh[]`, `p->buf` with `char buf[0]`, and `"RSDTZW"[i]` are
valid.

**Array members of record values.** An array member selected from a record
that is not an lvalue (a call, comma, conditional or assignment result) decays
to a pointer in a value context (C11 6.3.2.1p3). The record expression is
evaluated once and does not become an addressable lvalue. Nested selection
keeps the base's value category, and a conditional of record type keeps its
record type. The temporary's lifetime is not extended, and modifying it is not
admitted (C11 6.2.4p8).

**String literals at every site.** A string literal decays to `char *` and
then converts under the pointer-sign rule of 10.1.18 at every site: assigning
`"unsupported"` to `unsigned char *` and initializing `const unsigned char
*name` from `""` are accepted.

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
| signed `x << y` | count obligation only; then `shl.bits<N> x, y`, the low N bits under the GNU profile in 6.1; no new signed-left-shift obligation |
| read of an uninitialized scalar local | `uninitialized.bits<N>` is threaded through SSA until a semantic consumer reports `E3001` |
| reachable end of a non-`main`, non-void function | `ret` of `uninitialized.T` for the return type `T`: C makes the fall-off undefined only when the caller uses the value, so the value is poison and its use is the ordinary poison report; the reference compilers return whatever the register holds. A non-void `main` still returns 0 at its end |
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
(%old, %m2, %w1) = load %m1, %w0, %s_addr : bits<8> [mask=0] ; load containing byte
%mask   = const.bits<8> 248
%five   = const.bits<8> 5
%zero   = const.bits<8> 0
%masked = and.bits<8> %old, %mask                         ; clear field bits
%shift  = shl.bits<8> %five, %zero                        ; shift value into place
%new    = or.bits<8> %masked, %shift                      ; insert field
%m3 = access_in_bounds %m2, %s_addr [size=1, align=1]
(%m4, %w2) = store %m3, %w1, %s_addr, %new [mask=7]      ; write back
```

**Bit-field initializedness.** A store to a bit-field writes only its own
bits. It is never a read of another member, and every other bit keeps its
initializedness. A non-volatile bit-field access goes byte by byte, and the
masks of Section 3.8 state the rule:

- A read loads each byte that the field touches. The load mask selects the
  field's bits of that byte, so only those bits must be initialized.
- A store writes a byte that the field covers completely with an unmasked
  store. For a byte that the field covers in part, the load has `[mask=0]`
  and requests no bit, and the store mask selects the field's bits.

A later read of a member that was never stored is `E3001`; a read of a stored
member is not. A record copy moves the state of each bit. The C-HIR
interpreter applies the same rule to the same bytes. Native code is the same
sequence without masks. A volatile bit-field access keeps the unit accesses
below, and masks state the same rule for them (D25): the load of each unit
for a read selects the field's bits of that unit, and for a store the load of
each unit has `[mask=0]` and the store mask selects the field's bits.

**Volatile bit-fields.** A read of a volatile bit-field accesses its declared
allocation container. A write is a read-modify-write of that container that
keeps the neighbouring bits, and a compound update also reads the field. The
read of the container for a write is one volatile access in native code, but
it is no C read of the other members, so an uninitialized neighbour is no
finding. Each underlying load and store is one volatile event that advances
`world`; none is atomic. In a packed record the accesses use power-of-two units of at most 16
bytes, in increasing byte order and never past the record, and a field that
crosses units keeps its full value. C-HIR and EvoIR must produce identical
value-and-access traces. Volatile record copies with bit-field members remain
`E2922` (10.1.3).

**Anonymous members.** A member of an anonymous structure or union is a member
of the enclosing record for every purpose (C11 6.7.2.1p13): name lookup, `.` and
`->`, a designator in an initializer, and `__builtin_offsetof`. A name is
resolved by searching the record's own members, then recursively the members of
its anonymous members; the resulting `SubobjectPath` records every step it
traversed, so the byte offset and the initializer plan are the same as if the
name had been written out in full. A name found in two different anonymous
members is ambiguous and reports `E2906` at the name. Member access and
designator resolution are one rule and must share one implementation; nesting
is unbounded.

Example: for
`struct outer { int head; union { struct { long regs[2]; long sp; }; long raw[3]; }; int tail; }`,
`sizeof` is 40, `offsetof(head)` 0, `offsetof(sp)` 24, `offsetof(raw)` 8,
`offsetof(tail)` 32, and `{ .head = 1, .sp = 7 }` initializes through the
anonymous path.

**Automatic zero initialization.** Under the final
`-ftrivial-auto-var-init=zero`, each executed declaration of an automatic
object without an initializer stores a zero representation, padding included;
typed pointer subobjects receive `Null` provenance. A brace initializer, and
each evaluation of a block-scope compound literal, also zeroes gaps between and
after members and beyond the selected union member, recursively, before any
later copy; each explicit member expression still executes exactly once. A
whole-aggregate copy initializer keeps its source representation. The
`uninitialized` attribute on a declaration, or a final
`-ftrivial-auto-var-init=uninitialized`, keeps ordinary uninitialized
semantics; reserving storage or transporting its address never runs the zeroing.

#### 10.1.7 Function Call Lowering

C function calls lower to EvoIR `call` instructions. Attributes (10.1.9) never
refine a call summary in this revision, and a call that returns twice takes
the top summary (3.10); `typeof` (10.1.10) and statement expressions (10.1.11)
are resolved before this lowering sees a call.

**Effects.** Section 4.6 is the only authority. A call's summary is the callee's
summary, and a function's summary is the least fixed point over its call graph;
an indirect call contributes the top summary because its callee is not known
statically.

A direct same-TU call uses the exact fixed point from Section 4.6; an indirect
call and a call of an external function contribute top. The volatile bit is
true when the callee or any transitive callee contains a volatile access;
recursive SCCs are solved together. Every call still consumes and produces
`world`, even when that bit is false.

**Floating and arity lowering classes.** A floating scalar, floating complex
or homogeneous floating aggregate call interface that lowering does not support
is `E3049` (`not-lowered: floating-call-interface`). A record of one to four
binary16 members is an HFA. An HFA of `__bf16` members is `E3049`, and so is,
on Darwin, a record that mixes the two half formats (3.13.6). A record of one
to four
short vectors (8 or 16 bytes) of one size is an HVA whatever its lane types,
and it passes in vector registers (10.1.14). A required floating value
or conversion on an integer-only lowering path is `E3050`
(`not-lowered: floating-value`). A call whose actual arity differs from its
resolved canonical interface is `E3051`, not `E3101`.
These codes change no acceptance or execution rule, and other unsupported
types, malformed C-HIR and IR validation failures never take them.

**Return statements and fixed arguments.** In a void function, `return e;`
with `e` of type void is valid: `e` is evaluated once and no value is returned.
Discarded void conditional, comma and statement expressions keep their
selected and sequenced effects and yield no scalar. A non-void value returned
from a void function, and a bare `return;` in a non-void function, are `E1207`
(`invalid-return-value`). The front end checks every written return, including
an unreachable one, before inlining, execution or IR construction; falling off
the end is not this check. A return expression and each fixed prototype
argument, in direct and function-pointer calls alike, must satisfy the
simple-assignment compatibility of 10.1.18, after transparent-union member
selection (10.1.9) and before numeric conversion; an incompatible implicit
pointer or aggregate conversion there is `E2202`. The check evaluates nothing.
Unprototyped arguments and the anonymous variadic tail keep the default
argument promotions. Signed and unsigned `__int128` count as integer types for
null pointer constants, pointer arithmetic and conditional pointer typing, and
a `__builtin_va_list` argument keeps the target-sized copy of 3.12.

**Unprototyped function pointers.** A pointer to a function type with an
unspecified parameter list keeps its address and provenance in storage; it is
not a pointer to an empty prototype. A call through it applies the default
argument promotions. Lowering carries the value as `ptr<generic>` and converts
it at the call with `ptr.to_int` and `ptr.from_int.fnptr<Sig>`, where `Sig` is
the promoted call's signature; the dynamic `E3019` check applies unchanged.

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

C requires a scalar controlling expression for `if`, `while`, `do` and `for`,
for the first operand of `?:`, and for the operands of `!`, `&&` and `||`
(C11 6.5.3.3, 6.5.13, 6.5.14, 6.5.15, 6.8.4.1, 6.8.5). Pointers and complex
values are scalar. A record, union or void operand is `E2103`
(`controlling-expression-not-scalar`) at the operand. A vector controlling
expression is also `E2103`, except that the existing vector operand
diagnostics remain for `!`, `&&` and `||` (`E2102`) and for `?:` (`E2202`).

#### 10.1.9 GNU Attributes

This section fixes how GNU attributes are read, where they attach, which are given a meaning, and how
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
| after `*` (and its qualifiers) inside a pointer declarator, before the rest of the declarator | the pointer type formed by that `*`; preserved, never applied (10.1.14), except that `weak`, `alias`, `section`, `used` and `retain` in a named declarator mark the declared object or function (Section 5.2), and that the `unused-variable` lint (Section 6.3) reads `unused` on the pointer next to a variable's name, as GCC 14 does |
| after the `(` of a parenthesized declarator, before the inner declarator | the declared entity; preserved, never applied (see below) |
| after `struct`, `union` or `enum` keyword, or after the closing brace | the tag type |
| after a member declarator | that member |
| after a parameter declarator | that parameter |
| after a function declarator, before the body | that function |
| after a label | that label |
| before a null statement (`__attribute__((...)) ;`) | that null statement |
| after an enumerator | that enumerator |
| in a type name | the type being named |

An attribute in a position not in this table is malformed and reports `E2903`.

**Parenthesized declarators.** GCC 14 and Apple clang accept GNU attributes
right after the `(` of a parenthesized declarator, as libxml writes
`typedef void *(__attribute__((alloc_size(1))) *xmlMallocFunc)(size_t);`.
Both refuse an attribute after the inner declarator and a bracketed attribute
there. GCC 14 attaches such an attribute to the type that the declarator has
formed at that point, or to the declaration when the inner declarator is its
identifier. Apple clang attaches some of them elsewhere: it applies `aligned`
in `int (__attribute__((aligned(16))) *p);` to `p`, not to `*p`, and it
ignores `weak` on an object there. In a named declarator, such an attribute
therefore attaches to the declared entity and is preserved without effect.
The `unused-variable` lint reads `unused` there, as both compilers do. An
attribute that this section gives an effect on a declaration or a type is
`E2903` in this position: `aligned`, `packed`, `mode`, `vector_size`,
`transparent_union`, `weak`, `weakref`, `alias`, `section`, `used`, `retain`,
`visibility`, `cleanup`, `tls_model`, `uninitialized`, `constructor`,
`destructor`, `always_inline`, `gnu_inline`, `noinline`, `noipa`, `noclone`,
`patchable_function_entry` and `no_stack_protector`. An attribute that this
section refuses keeps its code, such as `E2952` and `E2940`. In an abstract
declarator, the position is the type-name position.

**Representation.** C-HIR carries every attribute losslessly on the entity it
attaches to: name as written, arguments as written, source span, and the
attachment position from the table above. Preservation does not depend on
whether the attribute is understood. A C-HIR dump shows every attribute present
in the source, so a reviewer can confirm nothing was dropped on the way in.

**Darwin Clang attributes.** On `aarch64-apple-darwin`, Clang attributes
that change a type, a value, the layout, the linkage or the ABI are `E2967`
(`unsupported-darwin-attribute`) at the attribute rather than being ignored.
The group is `ext_vector_type`, `matrix_type`, `address_space`,
`pass_object_size`, `pass_dynamic_object_size`, `noderef`, `ms_struct`,
`counted_by`,
`counted_by_or_null`, `sized_by`, `sized_by_or_null`, `ended_by`,
`internal_linkage`, `unsafe_indexable`, `indexable`, `bidi_indexable`,
`terminated_by` and `decay_discards_count_in_parameters`. `internal_linkage`
is accepted when the declaration is already `static`. `#pragma ms_struct on`
is `E2967` at the pragma; `off` and `reset` are accepted without effect.
Attributes that only affect diagnostics, static analysis or another language
are accepted without effect. Every other attribute that EvoCC does not know
is ignored on Darwin, as on Linux. The nullability keywords `_Nonnull`,
`_Nullable` and `_Null_unspecified` are accepted without effect there.
Qualifiers after a nullability keyword still apply, and a second nullability
keyword at the same pointer level is `E1001`. On Linux GCC 14 warns about an
unknown attribute and EvoCC ignores it without a warning; an unknown
nullability keyword is refused, as GCC 14 refuses it. `__has_attribute(name)`
on Darwin answers 0 for every name in the `E2967` group.

**Applied semantics.** `aligned` and `packed` act on layout; integer `mode`
selects the canonical integer type as specified below. The separately stated
inlining and linkage attributes also have their explicitly defined effects.
The profile also applies `constructor`/`destructor` through the load-instance
registrations of Section 2.1, `vector_size` through the real integer type of
Section 10.1.14, and `tls_model` through the thread-object model below; these
are not included in the preserve-only catch-all.

- `aligned(N)` raises alignment. `N` is an integer constant expression that is
  a power of two representable by the target layout; otherwise `E2903`. On a
  type or typedef it produces a type whose alignment is the greater of its
  natural alignment and `N`. On an object it sets the object's alignment
  likewise. On a member it sets that member's alignment, which moves its offset
  and may raise the containing record's alignment. `aligned` with no argument
  means the target's largest fundamental alignment, sixteen bytes on AArch64.
  On a typedef or a type name, `aligned(N)` sets the alignment to exactly `N`,
  which can lower it: `typedef int t __attribute__((aligned(1)))` has alignment
  one. On a record, a member or an object it can only raise; lowering there
  requires `packed`.
- `packed` removes padding. On a record it gives every member alignment one,
  makes the size the sum of member sizes, and gives the record alignment one
  unless `aligned` on the same record says otherwise. On a member it gives that
  member alignment one. Bit-fields in a packed record pack to bit granularity
  across storage-unit boundaries.

These live in the interned type, not in a side table: two record types that
differ only in `packed` are different types with different layouts, and a
`typedef` under `aligned(16)` is a distinct type from its target. The layout
service applies them.

**Integer `mode`.** On a non-`_Bool` builtin integer declaration, typedef,
member or parameter, `mode(QI)`, `mode(HI)`, `mode(SI)`, `mode(DI)` and
`mode(TI)` select the signed or unsigned canonical integer type of 8, 16,
32, 64 and 128 bits respectively. On this AArch64 target those types are
`signed char`/`unsigned char`, `short`/`unsigned short`, `int`/`unsigned int`,
`long`/`unsigned long`, and signed/unsigned `__int128`; `byte` means `QI`
and `word` means `DI`, and enclosing the mode name in `__` is equivalent.
The original integer's signedness and top-level qualifiers are preserved;
layout, promotions and every consumer use the selected canonical type, not
the original spelling. An unknown mode, wrong arity or non-integer target is
`E2903`; pointer-declarator attributes keep the existing never-applied rule.
An integer `mode` written in an abstract type name is also `E2903`, because
the reference compilers give it two layouts. `_Bool` and enumeration bases are
refused with `E2903`.

**Thread storage.** `__thread` and `_Thread_local` give an
object thread storage duration; file-scope definitions/references and block
static objects retain their normal linkage, qualifiers and template initializer.
Redeclarations must agree on thread storage. A block definition without static
or extern storage is refused, as is a static initializer containing a TLS
address. `tls_model("global-dynamic"|"local-dynamic"|"initial-exec"|"local-exec")`
and the corresponding command option select the explicit model; conflicting
attributes, unknown models and a model on a non-TLS object are E2943. Instance
identity, escaped addresses and lifetimes obey Section 3.8, not process-global
substitution.

**Availability.** On `aarch64-apple-darwin` the attributes `availability`,
`unavailable` and `weak_import` have the meaning of Apple clang for the macOS
deployment target of Section 10.5, because Apple's SDK marks its declarations
with them through `__API_AVAILABLE` and the related macros. On Linux they are
preserved without effect, as GCC 14 has none of them.

- The arguments of `availability` are a platform name, then any of
  `introduced=V`, `deprecated=V`, `obsoleted=V`, `unavailable`, `strict`,
  `message="..."`, `replacement="..."` and `environment=E`. A version `V` has
  one to three decimal components, separated by `.` or by `_`, and a missing
  component is zero. `introduced=NA` means `unavailable` and `deprecated=NA`
  means nothing, as in Clang. Any other argument is `E2903`. A version such
  as `10.14.4` after `introduced=`, `deprecated=` or `obsoleted=` inside the
  attribute is no stray token on this target.
- Only the platform `macos`, also spelled `macosx` and `macOS`, applies. Every
  other platform, the application extension platforms included, and an
  attribute with `environment=` are preserved without effect.
- The macOS attributes of every declaration of an entity merge in source
  order, as Apple clang 21 merges them: an attribute that disagrees with the
  merged one, by a different version where both have one or by a different
  `unavailable`, removes both; an agreeing one fills in the missing versions;
  and a merge whose versions are out of order, such as `deprecated` before
  `introduced`, removes the attribute. The tests list the measured cases.
- For the deployment target `D`, `unavailable` makes the entity unavailable,
  "not available on macOS". Otherwise an `introduced` version above `D` makes
  it not yet introduced, or unavailable, "introduced in macOS V", with
  `strict`. Otherwise an `obsoleted` version at or below `D` makes it
  unavailable, "obsoleted in macOS V". A `message` follows the reason after
  ` - `. Deprecation has no effect here; Apple clang warns. The `unavailable`
  attribute makes the entity unavailable with its optional message.
- A use of an unavailable entity is `E2962` (`unavailable-declaration`) at the
  use: an identifier that names a function, an object or an enumerator, in an
  evaluated or unevaluated operand; a typedef name; a reference to a tag other
  than its definition or a declaration of the tag alone; and a member access.
  The use is reported in a function that the unit drops too, as Apple clang
  reports it.
- A use is not refused when its context is unavailable, as in Apple clang. The
  context of a use in a declarator, its parameters, its array bounds and its
  initializer is the entity of that declarator. At file scope and in a record,
  the declaration specifiers take the context of the first declarator; in a
  block they take the context of the enclosing function. The context of a
  member is its record and then the function that encloses the record, as C
  records do not nest; the context of a body is its function. The entity of a
  context is unavailable when its attributes are complete, so a later
  redeclaration does not change it.
- A function or object declaration that is not yet introduced, or that
  `weak_import` marks, is a weak import: it is the weak declaration of
  Section 5.2, emitted as `.weak_reference`, unless the unit defines it or
  gives it internal linkage. Example: with the deployment target 11.0, a use
  of the SDK's `strchrnul`, introduced in macOS 15.4, is a weak reference, and
  `strchrnul != NULL` is tested at run time, as with Apple clang.

**Clang extensions.** On `aarch64-apple-darwin` the SDK and Clang's resource
headers use Clang extensions that EvoCC does not implement. Each is a named
refusal at its first use; none is approximated. Linux keeps the diagnostics of
GCC 14's dialect, which has none of them.

- Blocks: a `^` that begins a declarator, an abstract declarator in a type
  name, or a unary operand, such as `void (^handler)(int)` or `^{ ... }`, is
  `E2963` (`unsupported-blocks`) at the `^`. EvoCC defines no `__BLOCKS__`, so
  it compiles the SDK as Apple clang does with `-fno-blocks`, and it refuses
  at the positions where that compiler refuses. A header that needs a type
  that the SDK declares only with `__BLOCKS__`, such as `os_block_t`, stops
  at the unknown name, as with `-fno-blocks`.
- `overloadable`: the attribute, in the GNU spelling or as
  `[[clang::overloadable]]`, is `E2964` (`unsupported-overloadable-attribute`)
  at its name, in every position. Clang lets declarations of one name differ
  in type and gives each a mangled symbol name, so no ordinary function can
  stand in for one.
- A list attribute of the convention rule above after the `*` of a function
  declarator's result type belongs to the function, as Apple clang moves it
  there, and its declaration settles it; an object or a type name with one
  keeps `E2952`. Clang's `<arm_acle.h>` writes
  `void *__attribute__((target("gcs"))) __gcsss(void *)`.
- The definition of an internal function with such an attribute is refused
  only when the unit keeps the function, as a use in a body is: Clang's
  resource headers define their intrinsics as `static __inline__` functions
  with `target`, and a unit that does not call one compiles.
- On Darwin, a function's `target` attribute is accepted without effect when
  its single string argument is a comma-separated list whose items, after
  removal of surrounding ASCII whitespace, are each in this exact list:
  `aes`, `bf16`, `dotprod`, `faminmax`, `fp16fml`, `fp8`, `fp8dot2`,
  `fp8dot4`, `fp8fma`, `fullfp16`, `i8mm`, `neon`, `sha2`, `sha3`, `sm4`,
  `v8.1a`, `v8.3a`, `v8.5a`. These are the feature names in Apple's NEON
  headers. Empty items and all other names keep `E2952`, including a leading
  `+`, `sve`, `sme`, a `no` form, and architecture, CPU or tuning selections
  (`arch=`, `cpu=`, `tune=`). This permission does not admit an unsupported
  operation or builtin in the function body. Under `-mgeneral-regs-only` the
  attribute keeps `E2952`:
  EvoCC cannot re-enable SIMD registers for one function, whereas Apple clang
  accepts `target("neon")` in that mode. Linux keeps the original refusal.

**Scalar storage order.** `scalar_storage_order`, with or without surrounding
underscores, is `E2940` (`unsupported-scalar-storage-order`); it is never
preserved as an ignored attribute.

**Other attributes.** Every attribute that this section neither refuses nor
gives an effect is preserved and not applied. In particular
`noreturn`, `unused`, `no_instrument_function`,
`cold`, `pure` and `const` are preserved without effect here; they carry
effect or backend semantics that belong to later stages, and applying
a partial reading now would create a second source of truth for each.
`always_inline` has the effect stated below; `weak` and `alias` are linkage
marks (Section 5.2); `aligned` on a function is code alignment (Section 5.2);
`section` is placement metadata for function definitions (Section 5.2) and
for objects (Section 2.1).
`cleanup` on an automatic object has the scope-exit effect stated below.
`alloc_size(1)` / `alloc_size(1,2)` and other valid parameter indices may
describe a returned allocation's byte extent for the object-size queries below.
Indices are one-based; the indicated integer
arguments, after their ordinary call conversions, give the size or its product.
This adds no alias, alignment, lifetime or bounds-check exemption; the original
call and argument effects still execute exactly once.

**Unsupported execution and calling conventions.** The GNU attributes `naked`,
`interrupt`, `interrupt_handler`, `pcs`, `aarch64_vector_pcs`,
`aarch64_sve_pcs`, `preserve_all`, `preserve_most`, `preserve_none`,
`no_callee_saved_registers`, `no_caller_saved_registers`, `ms_abi`, `sysv_abi`,
`stdcall`, `fastcall`, `thiscall`, `vectorcall`, `regcall`, `swiftcall`,
`swiftasynccall`, `regparm`, `sseregparm`, `callee_pop_aggregate_return`,
`returns_twice`, `target`, `target_clones`, `target_version` and `optimize`
are `E2952` (`unsupported-function-attribute`) where this paragraph does not
accept them. This also applies to names enclosed in `__` and to the `gnu` and
`__gnu__` bracket namespaces, on both targets. A warning option cannot
suppress this error. No ordinary frame or default calling convention may
stand in for one of these contracts. Unrelated bracket namespaces do not
denote these GNU attributes.

One of these attributes on a function declaration that is not a definition
is accepted among the declaration specifiers and directly before or after
the declarator. The front end records it on that function. For example,
glibc's `<math.h>` declares its vector functions with `aarch64_vector_pcs`,
and Clang's `bounds_safety_soft_traps.h` declares two functions with
`preserve_all`. Every other placement is `E2952` at the attribute: a type
name, a typedef, a function pointer declarator, an object, a parameter, a
member and a statement. A function that any declaration marks is checked
when the unit is complete, in any declaration order:

- Each definition is `E2952`, at the attribute when the definition carries
  it and at the defined name otherwise. `returns_twice` is the exception, and
  on Darwin an internal definition that the unit drops is not refused (Clang
  extensions, below).
- Each use of the function's designator is `E2952`: a call, an address or
  any other operand, evaluated or not. A call through a pointer cannot be
  checked, so a call is not the only use that counts. A use outside a
  function body is reported at once. A use in a function body is a deferred
  front-end profile refusal. It is not reported for a function that the
  unreferenced internal function rule of 10.1.16 drops. Neither interpreter
  enters such a body, and the lowering does not emit it.
- A use of a function marked `returns_twice`, `aarch64_vector_pcs`,
  `preserve_all` or `preserve_most` is accepted. A callee marked with one of
  the last three takes its arguments as the base AAPCS64 does and preserves a
  superset of the base callee-saved registers. A caller relies only on the
  base set, so a call of it is an ordinary base-PCS call. `aarch64_sve_pcs`,
  `preserve_none` and every other attribute of the list keep `E2952` at each
  use.
- The message names the first attribute of the list, in the order above,
  that refuses the definition or the use.

**`returns_twice`.** A function returns twice when a declaration of it
carries `returns_twice`, or by GCC 14's name rule (`special_function_p` in
`calls.cc`). By that rule, the function has external linkage, and its C
identifier has at most 11 bytes and either equals `setjmp` or `sigsetjmp`
after one leading `__` or `_` is removed, or equals `savectx`, `vfork` or
`getcontext`. The identifier counts, not an assembler name: after
`int h(void) __asm__("setjmp");` a call `h()` is ordinary. A `static`
function does not count. GCC 14 counts only a file-scope declaration. EvoCC
also counts a block-scope declaration with external linkage; this addition is
conservative, because it only gives more calls the treatment of 3.10. A
definition of a function that returns twice and every use of its designator
are accepted. A direct call of it lowers to `call.returns_twice` (3.10). A
call through `*f`, `&f` or any chain of them on its designator `f` is direct,
because GCC folds them. A call through a pointer object is an ordinary call. No function that
makes such a call is substituted for a call of it, and no function that
returns twice is substituted either. An `always_inline` function that makes
such a call is `E2957` (`always-inline-returns-twice`) at that call:
substitution would move the second return into the caller, and an ordinary
call would return from the frame that the jump needs. The result of a
function that returns twice is never a known constant. glibc's
`pthread_cleanup_push` and `pthread_cleanup_pop` macros call
`__sigsetjmp_cancel`, which its prototype marks, so they compile. The
interpreters refuse its external call (`E3039`), as they refuse `setjmp`,
`vfork` and `getcontext`.

The sole accepted `optimize` form has one or more ordinary string arguments
whose comma-separated options are all `no-tree-vectorize`,
`no-tree-loop-vectorize` or `no-tree-slp-vectorize`, optionally prefixed by
`-f`. String concatenation and escapes have their ordinary C meaning. Empty
options and all other options remain `E2952`, including a mixture of supported
and unsupported options. EvoCC does not automatically vectorize scalar code,
so these restrictions require no code change. They do not disable explicit
vector operations or permit any other per-function semantic option.

**`cleanup`.** `cleanup(function)` on an automatic object
resolves a function at the declaration and calls it with the object's address
when execution leaves that object's scope. The callback must accept that
pointer as its sole argument; its return value is discarded. Invalid arguments
or a non-function callback are `E2903`; an unresolved name is `E2001`.
The attribute on static/external objects, parameters, members, or a pointer type
does not create a scope-exit callback. The written attribute remains in C-HIR.
Callbacks execute in reverse declaration order, before their objects' lifetimes
end, on normal exit, return, break, continue and a direct or indirect goto that
actually leaves the scope. A declaration not yet reached on an early-return
path has no callback; reserving its storage does not activate cleanup.
The return expression is evaluated, and an aggregate return copied, before any
callback can change its source. Inlined bodies preserve this ordering and
remap the callback's object address to the new local object.
These are ordinary C-HIR calls with ordinary EvoIR call effects and explicit
edge/lifetime handling, not a new instruction or an optimizer hint.

**GNU inline definition roles.**
The complete declaration chain distinguishes an inline body from this unit's
linkable definition. GNU89, `-fgnu89-inline`, or a function's `gnu_inline`
attribute selects GNU inline semantics; a per-function attribute does not
change the unit's language mode or predefined inline-mode macros. A pure
GNU `extern inline` body supplies no independent definition, including when
its address is taken. A non-extern inline declaration in the chain can request
the independent definition; an inline-only GNU body may precede a distinct
ordinary definition, but not follow it or replace duplicate-definition checks.
When that real definition is present, its body supersedes the earlier inline
body for direct substitution too, including call sites preceding the real
definition.
Default GNU11 ISO inline and `static inline` retain their separate rules.
Remaining calls, addresses and relocations name the actual external symbol,
never a fabricated local, weak or COMDAT copy. Interpreters may execute a
substituted body, but an unexpanded external call uses the existing external
execution boundary and top effect summary, not the saved inline body.

**`always_inline`.** A call whose callee is a function
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
inside an inlined body is substituted too. Taking the function's address does
not prohibit direct-call substitution or give an inline-only body an address.
A variadic callee and a cycle of `always_inline` calls are not
substituted: the former is lowered as an ordinary call, the cycle is
`E2932` (`always-inline-cycle`). The exception is a variadic callee whose
body forwards its anonymous arguments only through the GNU argument pack
and uses no `va_start`, `va_arg`, `va_end` or `va_copy`; it is substituted
like a fixed callee. A callee that makes a call that returns twice is not
substituted; it is `E2957` (`returns_twice` below). Each anonymous argument of the call, after the default
argument promotions, becomes a fresh automatic object of its promoted type,
initialized after the named parameters in argument order. Each
`__builtin_va_arg_pack ()` in the body becomes those objects, in order, and
each `__builtin_va_arg_pack_len ()` becomes their count as an `int`
constant. After substitution the front end folds constants again, so an
`asm` operand under an immediate constraint (`i`, `n`, `I`, `J`, `K`, `L`)
whose expression was a parameter now bound to an integer constant
expression is a constant, and 3.11 admits it; the operand that is still
not constant stays `E3021`. The out-of-line body of a `static` `always_inline`
function is **not lowered** when every call to it was substituted and the
unit takes no address of it: the reference compilers never emit such a
body, and lowering it would report `E3021` for an immediate operand that is
only constant after substitution. A body that keeps a use (an address taken,
a call the rules above did not substitute) is lowered, internal, and any
immediate operand still not constant there is `E3021`.

**Optional substitution and runtime recursion.** Optional inline selection
reuses the same parameter conversion, fresh-object, lifetime, label-renaming,
static-object and effect rules. It honors `noinline`, `noipa` and `noclone`
barriers on the complete declaration chain. A runtime call-graph cycle is not
itself an inline-substitution cycle: an inlined visitor may call an out-of-line
walker which in turn executes the visitor. The expansion graph must remain
finite, and the recursive runtime call must remain an ordinary call.
The bounded single-site implementation admits only an unaddressed internal
inline body with one source call in a non-inline caller which is not otherwise
substituted. It bounds the unexpanded body at 1024 syntax nodes and fixed local
storage at 256 bytes, and excludes variadics, activation-sensitive builtins,
dynamic allocation, static locals, assembly and address-taken labels. These
are optimization admission limits, not new C-language refusals or permission
to change an object's identity or read uninitialized bytes. A nonmatching
optional call remains a call; mandatory `always_inline` cycle handling is
unchanged. No EvoIR instruction, EqSat rule or implicit cross-block state is
introduced by this selection.

**Bracketed attributes.** `[[gnu::name]]` and `[[__gnu__::__name__]]` have
the meaning of the GNU attribute `name` at declaration, member and tag-keyword
positions. An unscoped or other-namespace bracket attribute gets no GNU layout,
linkage or inlining effect, and C-HIR keeps each spelling and namespace.
Malformed placement is `E2903`, including a GNU `__attribute__` written before a
bracket attribute in one specifier sequence and `aligned` on an array suffix or
pointer layer. A bracket attribute on a function declarator suffix stays on the
function type and does not raise the function's alignment.

**`transparent_union`.** A parameter of a `transparent_union` type accepts an
argument that matches one member, including through the ordinary pointer and
null-pointer conversions; that member is constructed exactly once and the union
is passed in the record-call form of 10.1.3. An argument already of the union
type is passed unchanged, and an unannotated union gets no such conversion. The
attribute applies only when every member is a general-register integer or
pointer of equal size and alignment, passed as the first member's
representation. An incompatible argument is `E2202`; any other member
representation is `E2903`.

**Diagnostics introduced.** Messages live in Section 6.3, the single authority;
this table gives conditions only. `E2902` is withdrawn: Section 10.1.13 covers
compound literals, and a front end must not emit it.

| Code | Condition |
|---|---|
| `E2903` | argument list does not parse, position not in the table, `aligned` argument not a representable power of two, or an attribute with an effect after the `(` of a parenthesized declarator |
| `E2904` | a `typeof` type-name bound requires a runtime capture outside an admitted function-body scope |
| `E2905` | `({ ... })` at file scope or where a constant expression is required |
| `E2906` | designator names a member the type lacks, a member ambiguous between two anonymous members, or an index at or beyond a fixed bound |
| `E2907` | two `_Generic` associations name compatible types, or `default` appears twice |
| `E2908` | `_Generic` has no association compatible with the controlling type and no `default` |
| `E2909` | `__builtin_expect`, `__builtin_constant_p` or `__alignof__` called with the wrong number of operands, or `__builtin_tgmath` with fewer than two functions and their arguments (10.1.14) |
| `E2910` | a resolved builtin name used without a call |
| `E2911` | `_Static_assert` condition evaluates to zero |
| `E2912` | `_Static_assert` condition is not an integer constant expression |
| `E2913` | `__builtin_expect` operand of non-arithmetic type, or an atomic builtin operand outside its domain (10.1.14) |
| `E2914` | `asm` template names an unknown operand, or an output operand is not a modifiable lvalue |
| `E2915` | `__auto_type` with no initializer, several declarators, or an array/function declarator |
| `E2916` | `case a ... b` with `a > b` or overlapping another case |
| `E2917` | designator index or range bound not an integer constant expression, range with `a > b`, or a range initializer that is not constant |
| `E2918` | `goto` into a statement expression |
| `E2919` | initializer for a flexible array member outside file scope |
| `E2920` | `goto` names a label no enclosing function scope declares |
| `E2922` | a volatile union outside the exact-width/alignment rule, or a volatile record copy with bit-field members |
| `E2923` | array bound folds to a negative value or is not of integer type |
| `E2924` | an attribute specifier precedes a statement that is not a null statement |
| `E2925` | `__builtin_object_size` mode is not a constant in `0..3` |
| `E2926` | `va_start` in a non-variadic function, or naming a parameter other than the last named one |
| `E2927` | a library builtin's non-constant form used where a visible declaration of the library function has an incompatible type |
| `E2928` | the address of a `register` object is taken |
| `E2929` | `__builtin_va_arg` names a type the default argument promotions change (`char`, `short`, `_Bool`, `float`, an enumeration narrower than `int`) |
| `E2930` | a call or address-of names a function whose type contains an enumeration still incomplete at that point |
| `E2931` | a function declared without a prototype and not defined in the unit is called with promoted argument types that differ from those of its first call |
| `E2932` | a cycle of `always_inline` calls, which no substitution order can eliminate |
| `E2933` | `__builtin_exit` or `__builtin__Exit` (10.1.14) |
| `E2934` | `_Noreturn` on an object or a typedef (10.1.18) |
| `E2935` | `__builtin_puts`, a library builtin this profile does not provide, or an `__atomic_is_lock_free` or `__c11_atomic_is_lock_free` answer that needs the run-time library (10.1.14, 10.1.21) |
| `E2937` | a called expression that is not of pointer-to-function type (10.1.18) |
| `E2938` | an invalid `_Alignas` request or placement (10.1.18) |
| `E2939` | a wide bit-field use that 10.1.16 does not admit |
| `E2940` | `scalar_storage_order` in any spelling |
| `E2941` | `__builtin_classify_type` of `_Bool` or of a target type with no class |
| `E2942` | a cast to a union type with no compatible member |
| `E2944` | a recognized ordinary `alloca` call whose name the unit defines or renames |
| `E2945` | a jump into the scope of a variably modified declaration |
| `E2946` | a nonliteral `__builtin_strcmp` with a visible same-named `static` function, or lowering a library symbol that collides with a linked object |
| `E2947` | a constant memory order that an atomic builtin does not admit (10.1.14, 10.1.21) |
| `E2949` | a call of an undeclared identifier that begins with `__builtin_`, `__atomic_` or `__sync_`, or with `__c11_atomic_` on Darwin; a retained Darwin NEON builtin call without an implemented operation (10.1.14) |
| `E2950` | `__builtin_va_arg_pack ()` other than as the last anonymous argument of a call to a variadic function, either pack builtin outside a variadic function, or a lowered body that keeps one (10.1.14) |
| `E2951` | a floating comparison or classification builtin with an operand of the wrong type, a class argument of `__builtin_fpclassify` that is not an integer constant expression (3.13.1), or a `__builtin_tgmath` function list or argument that GCC 14 rejects (10.1.14) |
| `E2952` | an execution or calling-convention attribute where 10.1.9 does not accept it, or a definition or use of a function that it marks (10.1.9) |
| `E2953` | a cast to a struct type from an incompatible operand type (10.1.3) |
| `E2954` | a declaration makes a previously folded string object's binding weak, aliased or assembly-renamed (10.1.17) |
| `E2955` | `_Atomic` names an array, function, atomic or qualified type, or an atomic bit-field (10.1.21) |
| `E2956` | an evaluated member access of an atomic record or union (10.1.21) |
| `E2957` | an `always_inline` function that makes a call that returns twice (10.1.9) |
| `E2958` | `_Bool`, `__int128`, an atomic type or an aligned typedef as the fixed underlying type of an enumeration (10.1.14) |
| `E2959` | a `__builtin_tgmath` call that selects a complex function, or has a complex argument, of a type without admitted values: `_Float16 _Complex` and the other `_FloatN _Complex` and `_FloatNx _Complex` types (10.1.14) |
| `E2960` | a translation-time product or quotient of floating complex values on a target whose complex helpers have no model, Darwin in this revision (3.13.5, 10.1.14) |
| `E2961` | an arithmetic, relational, equality or conditional operator, or a compound assignment, with a `_Float16` operand and a `__bf16` operand (3.13.6) |
| `E2962` | a use of a declaration that is unavailable for the macOS deployment target, outside an unavailable context (10.1.9) |
| `E2963` | on Darwin, a `^` that begins a declarator or a unary operand: Clang's Blocks (10.1.9) |
| `E2964` | on Darwin, the attribute `overloadable` (10.1.9) |
| `E2965` | on Darwin, a call of `__builtin_os_log_format` or `__builtin_os_log_format_buffer_size` (10.1.14) |
| `E2966` | on Darwin, a call of a Clang ACLE builtin or a use of an ACLE intrinsic that `<arm_acle.h>` declares (10.1.14) |

#### 10.1.10 `typeof` and `_Generic`

`typeof(type-name)` and `typeof(expression)` denote a type. `__typeof__` and
`__typeof` are the same construct. The result of `typeof(expression)` is the type of the expression
before lvalue conversion: an array-typed lvalue yields the array type, not a
pointer, and qualifiers are retained. An expression operand is evaluated if and
only if its type is variably modified. This includes a pointer to a VLA formed
by address-of, array conversion, cast, conditional or statement expression.
Otherwise it is not evaluated: `typeof(i++)` names `int` and leaves `i` unchanged.
An evaluated operand runs exactly once when its declaration, typedef or cast
executes, before a declaration's initializer. Its value is discarded, and its
local declarations keep their own scope. The resulting type keeps its bound
captures; bounds written inside the operand are captured during that evaluation.
For a parameter of a function definition, these captures execute at function
entry before the body. Function prototypes, including nested callback
prototypes, do not execute parameter captures.
An enclosing unevaluated context, such as `sizeof` of a non-VLA type or an
unselected `__builtin_choose_expr` arm, suppresses this evaluation. An identifier
of the resulting variably modified type keeps the jump restriction E2945.
In a `typeof` type name, variable bounds are captured once when the declaration
executes (10.1.19); a bound that needs such a capture outside an admitted
function-body scope is E2904.

`typeof` produces a type and is fully resolved by the front end; nothing of it
survives into EvoIR. A statement expression inside a `typeof` operand is a
block of its own: every declaration it makes — objects, tags, and
declarations with linkage such as `extern void c(void)` — is scoped to it and
leaves nothing behind. When its operand is a function designator the result is a
function type, and that type may be used as a declaration specifier exactly as
a typedef naming a function type may (10.1.18): `extern typeof(g) g;` declares
`g` a function and may never define one.

`__typeof_unqual__` is `typeof` with the top-level qualifiers removed: `const`,
`volatile`, `restrict` and `_Atomic` are dropped from the resulting type, and
nothing else changes. Its operand is a type name or an expression, with the
same evaluation rule as `typeof`; it is fully resolved by the front end.
`const int`, `volatile int`, `const volatile int` and `_Atomic int` all give
`int`, and the result is writable; `int *const` gives `int *`, while
`const int *` and `const char *const` keep the pointee qualifier; a `const`
member of a record stays `const`. For an array such as `const int arr[3]`,
the profile keeps the element qualifier, so a store through
`__typeof_unqual__(arr)` is refused (the reference compilers disagree; the
profile takes the direction that refuses a write). Only the
`__typeof_unqual__` spelling exists in this dialect; the C23 keyword
`typeof_unqual` is not admitted under `gnu11`.

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
An aggregate value is copied into a fresh temporary before those objects end.
The copy precedes every `lifetime.end` for an object declared inside the
expression, and the statement expression's value is the temporary. The
temporary lives at least until the end of the enclosing full expression.
`break`, `continue`, `goto` and `return` may leave a statement expression;
jumping into one is `E2918` (`jump-into-statement-expression`) at the `goto`.

A statement expression is permitted **anywhere inside a function definition**
and nowhere outside one; outside, it is `E2905`
(`statement-expression-outside-function`). That is the whole rule. In
particular it is permitted inside `typeof`, `sizeof` and `_Alignof`, where the
operand is unevaluated (unless it has variable-length array type, 10.1.19) and
only its type is taken, even when that operand sits
inside a `_Static_assert` condition.

This profile does not fold a statement expression with a constant result where
a constant is required (`_Static_assert(({ 1; }), "")`, `int a[({ 4; })]`):
where the *value* is required as an integer constant expression the context
reports its own diagnostic (`E2912` in a static assertion, `E2923` or `E2203`
in an array bound). A later revision can fold it.

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
subobject that follows it, as it would after an undesignated one:
`{ [1] = 1, 9, 9 }` has four elements. When two
actions name overlapping paths, the later one wins and the earlier is dropped
from the plan; the written form still records both, so the dump shows what the
source said. A designator naming a member the type does not have, an index at
or beyond a fixed bound, or any designator applied to a scalar, reports `E2906`
at the designator. An index or range bound that is not an integer constant
expression, or a range with `a > b`, is `E2917`
(`designator-index-not-constant`) at that expression. For an array of
unknown bound the bound becomes one more than the largest index any
designator or positional element reaches: `[5] = 1` gives 6, `[2 ... 4] = 7`
gives 5. One written initializer serves every element of a range, so it
must be a constant, as a static initializer is: an arithmetic constant
expression, an address constant or a label difference, and each element of a
braced list likewise. A nonconstant one would run once per element; it is
`E2917` at the expression.

Nothing changes below the plan: lowering consumes `InitAction`s exactly as
before, so an initializer that arrives through designators is
indistinguishable, in EvoIR, from one written positionally.

**Range designators and inferred bounds.** `[lo ... hi]` designates every
element from `lo` to `hi` inclusive; `lo` and `hi` are integer constant
expressions with `lo <= hi`. An array declared without a bound takes as its
size the larger of the positional count and one more than the largest index
any designator or range names, at any nesting depth of the same array. A
later initializer for an element already initialized replaces it.

**Obsolete GNU designators.** `member: value` means `.member = value`, and a
single `[index]` or `[first ... last]` designator can omit `=`. These forms
resolve subobjects, infer bounds and evaluate exactly as the standard
spellings, and an unknown member or out-of-bounds index is `E2906`. A
designation of more than one component must keep `=`.

**Empty union initializers.** An empty initializer for a union initializes its
first member only. The union's bytes beyond that member stay uninitialized, and
a read of them is `E3001`.

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
symbol is generated from its allocation-site ordinal, `__evocc.compound.<origin>`,
so two lowerings of one unit agree byte for byte, and validation checks the
spelling as for the other generated kinds (5.2). A file-scope literal of
`const`-qualified type is a read-only object exactly as a string-literal global
is (5.2), and a store through it reports `E3014`; an unqualified one is
writable. A file-scope *pointer* initializer that takes a literal's address
(`int *r = (int []){5, 6};`) is a relocation to the literal's global
(Section 2.1).

A static object initialized from a compound literal receives a copy of the
literal's value.

#### 10.1.14 Kernel builtins resolved by the front end

The front end resolves the following GNU builtins; none survives into EvoIR,
and the lowered module is byte-identical to the one obtained by writing the
resolved form. Where the reference compilers disagree, the profile takes the
rule both document as permitted.

- `__builtin_expect(e, c)`: both operands are converted to `long`; the result
  is the converted `e` with type `long`. Parsing, type checking and conversion
  legality apply to both operands. At source resolution, if `e` is itself an
  integer constant expression under this profile, the result is its constant
  projection and `c` is not executed; this result is an integer constant
  expression. Otherwise this is a runtime expression: evaluate and save the
  converted `e`, then evaluate converted `c` once, then return the saved value.
  This `e`-then-`c` order is a project choice, not a GNU-wide ordering claim.
  `c` need not be constant. Unevaluated contexts and control transfers retain
  their ordinary semantics; a second operand that does not return does not
  produce a builtin result. Substitution, inlining and later constant discovery
  must not change the source-selected evaluation contract. No expectation
  instruction reaches EvoIR: only the selected computation and effects remain.
  This bounded source-ICE policy deliberately differs from GCC's nested-expect
  fold and from Clang's non-ICE const-local projection.
- `__builtin_constant_p(e)`: `e` is parsed and type-checked but never
  evaluated. The result has type `int` and is an integer constant expression.
  It is `1` when `e` is an integer constant expression or a string literal,
  and may also be `1` when optimization proves its value constant. The
  proof may use immutable dominating initializers and substituted call
  arguments; it may not execute the operand, assume an unknown object value,
  ignore volatile reads, or discard effects from the original initialization
  or argument evaluation. Without such a proof the answer is `0`, including
  symbolic address constants and expressions with side effects. This does not
  make an object or a statement expression a source integer constant expression.
  The shared predicate answers from the typed operand; C-HIR execution and
  lowering apply the same proved substitutions before answering runtime
  queries. Required source-constant contexts use the facts available when
  resolved.
- `__alignof__(type-name)` and `__alignof__(expression)`: the alignment of the
  type, or of the expression's type before lvalue conversion, as an integer
  constant expression of type `size_t`; the expression is not evaluated. It
  is the C11 `_Alignof` with an expression operand admitted, and the same
  target layout answers both. `_Alignof(void)` and `__alignof__(void)` are
  `1`; this does not make `void` an object type. `__alignof` is the same operator.

**`__builtin_tgmath`.** `__builtin_tgmath(functions..., arguments...)` calls
the function that the rules of `<tgmath.h>` select, as GCC 14 documents and
implements it; glibc's `<tgmath.h>` defines its macros with it. The leading
operands are pointers to prototyped functions with fixed parameters and one
parameter count, at least two of them. That count, taken from the first
function, says how many trailing operands are arguments. Each parameter
position and the return type either have one type in every function, or have
in each function a real or complex floating type: the type-generic type `t` of
that function or its real type. The first such parameter is the type-generic
one, and its types are distinct. The type-generic arguments select `t`: an
integer argument counts as `double`, the usual arithmetic conversions combine
them, and a complex argument or a list of complex-only functions makes `t`
complex. The call goes to the function whose `t` is that type. When no
function matches and every function returns one floating type, it goes to the
first function whose format has at least the range and precision of that
type, as GCC 14 does for a narrowing macro. The result is an ordinary call of
that function, so each argument is evaluated once and converts to its
parameter type, and the other function operands are not evaluated. A function
list or an argument that GCC 14 rejects is `E2951`, and too few operands are
`E2909`. The functions can take GCC's `_FloatN` and `_FloatNx` types and their
complex types, as glibc's lists do with `_GNU_SOURCE`; a `float`, `double` or
`long double` argument still selects the function of that type. A selection
of a `float`, `double` or `long double` complex function is an ordinary call.
A selection of another complex function, or a complex argument of another
type, is `E2959` (`unsupported-complex-type-generic-call`), because those
types have no admitted values. Example: `__builtin_tgmath(sqrtf, sqrt, sqrtl,
2)` calls `sqrt`, `__builtin_tgmath(powf, pow, powl, 2.0f, 3.0L)` calls
`powl`, and `__builtin_tgmath(sqrtf, sqrt, sqrtl, csqrtf, csqrt, csqrtl,
1.0i)` calls `csqrt`.

Note: Darwin's `<tgmath.h>`, in the SDK and in Clang's resource directory,
selects with `overloadable` functions instead. This profile does not
implement `overloadable`, so that header stops at `E2002`.

**`packed` on an enumeration.** `packed` on an enum type selects the smallest
integer type that represents every enumerator: by value range `0..255` one
unsigned byte, `0..65535` two, otherwise four; with a negative enumerator the
smallest *signed* type instead: `-1..6` is 1 byte, and `-1..200` is 2,
because 200 does not fit a signed byte. Alignment equals size. The
enumeration *constants* keep type `int` regardless (`sizeof(A1)` is 4), and
the underlying type's signedness is observable (`(enum e5)-1 < 0` is 1). The
attribute is accepted before the tag and after the closing brace alike.
`packed` and `aligned(4)` together on an enum are refused (`E2903`), because
the reference compilers give them different layouts.

**Enumeration underlying types.** A non-packed enumeration whose values all
lie in `0..UINT_MAX` has underlying type `unsigned int`, and one with a
negative value whose range fits `int` has `int`, also as a bit-field base. Its
constants keep type `int` when the whole range fits `int`. A complete
enumeration type is compatible with its underlying type, including through
pointers and function parameters. Separately declared enumeration tags are not
compatible with each other, nested pointer qualifiers must still match, and a
pointer conditional keeps the compatible pointee's signedness and the union of
pointee qualifiers.

**Enumerators outside `int`.** An enumerator whose value does not fit `int` is
a GNU (and C23) extension: the enumeration's underlying type becomes the
smallest of `unsigned int`, `long`, `unsigned long` that represents every
enumerator, signed when any enumerator is negative: `0xffffffff00000000`
gives an 8-byte enum whose constant has type `unsigned long`; `-1` with
`0x100000000` gives an 8-byte enum whose wide constant has type `long`. A
value that fits no such type is `E1303`. Every constant of such an enum takes
the underlying type, as in GCC and C23 (all enumerators take the enumerated
type once the list is complete).

**Fixed underlying types.** `enum E : T { ... }`, `enum E : T;` and
`typedef enum E : T { ... } name;` follow C23 6.7.2.2 in every language mode,
as GCC 14 and Apple clang accept them before C23. `T` is a
specifier-qualifier list that names an integer type, and `const` and
`volatile` in it are dropped. As in GCC 14, only a colon that a type name
follows begins the enum type specifier, so a member `enum E : 3;` stays a
bit-field. The enumeration is complete from the declaration that fixes `T`.
It has the size, alignment, range, promotions, conversions and call ABI of
`T`, and it is compatible with `T` and with no other type. Each enumerator
has the enumerated type, during the list and after it. Its value, written or
the previous value plus one, must be representable in `T`; a value outside
that range is `E1306` (`enumerator-outside-underlying-type`). `packed` on such
an enumeration has no effect, as in both reference compilers. These are
`E1307` (`invalid-enumeration-underlying-type`):

- a `T` that is not an integer type, such as an enumeration, a floating, a
  record or a vector type, and a `restrict`-qualified `T`;
- `enum E : T` without a list anywhere but a declaration `enum E : T;` that
  has no other specifier: a declarator, a parameter, a cast, a type name, a
  storage class or a qualifier;
- a declaration that disagrees with an earlier declaration of the
  enumeration, by another `T` or by the presence of `T`.

A second list is `E1101`. `_Bool`, `__int128`, an atomic type and an aligned
typedef as `T` are `E2958` (`unsupported-enumeration-underlying-type`): GCC 14
converts to a `_Bool`-based enumeration as to `_Bool`, Apple clang truncates,
and they also disagree on an atomic `T`. `aligned` on such an enumeration and
any attribute in `T` are `E2903`, because GCC 14 ignores `aligned` there and
Apple clang applies it. Example: `enum E8 : unsigned char { A8 = 1, B8 = 200 };`
has size 1, `sizeof(A8)` is 1, `A8 - 2` is `-1`, and `(enum E8)300` is `44`;
`enum E8 : unsigned char { X = 256 };` is `E1306`.

Note: Apple clang accepts a negative value in an unsigned `T`, a declarator
after `enum E : T` and a cast to it. GCC 14 refuses them, and so does this
profile, because C23 makes them constraint violations. Darwin's
`kern/kcdata.h` and `malloc/malloc.h` declare such enumerations.

**`__attribute` spelling.** `__attribute` is a synonym for `__attribute__`.
It is read wherever `__attribute__` is.

Pointer-declarator attributes (10.1.9, position after `*`) are preserved and
not applied: the reference compilers disagree on `aligned` in that position,
so this profile applies nothing there.

Errors: a builtin used with the wrong arity is `E2909`
(`builtin-arity`); a builtin name used other than as a call is `E2910`
(`builtin-not-called`); an `__builtin_expect` operand that is not of
arithmetic type is `E2913` (`builtin-operand-not-convertible`); a floating
operand converts to `long`.

`_Static_assert(condition, message)` and the C23 one-operand form: the
condition must be an integer constant expression, else `E2912`
(`static-assertion-not-constant`) at the condition's span; a condition that
evaluates to zero is `E2911` (`static-assertion-failed`) at the `_Static_assert`
keyword. `__builtin_constant_p` and `__builtin_types_compatible_p` are integer
constant expressions in that position; `__builtin_*_overflow` is not.

**Scalar atomic builtins.** `__atomic_load_n(p, order)` returns the unqualified
pointee type; `__atomic_store_n(p, value, order)` converts `value` to that type
and returns `void`. The pointee is an integer of 1/2/4/8-byte size or a
target-width pointer. A store requires a modifiable pointee; const loads and
volatile pointers preserve their ordinary meaning. Unsupported types report
`E2913`, wrong arity `E2909`, and a builtin not called `E2910`. These accesses
are never ICEs. An `_Atomic`-qualified pointee means its non-atomic type for
every `__atomic` builtin; Section 10.1.21 admits the C operators on `_Atomic`
objects. The admission does not include 128-bit accesses or MMIO; the
generic forms on memory follow below.

Source constant order values map 0/1/2/3/4/5 to
relaxed/consume/acquire/release/acq_rel/seq_cst. Loads admit 0/1/2/5 and stores
0/3/5; an invalid constant is `E2947` at that operand. For a non-ICE order,
on the defined domain of legal source orders the implementation strengthens
to seq_cst, evaluating the actual order expression exactly once. Operand
evaluation uses the project's deterministic left-to-right policy: address,
then store value if any, then order, then the access. This preserves earlier
values across later effects and does not evaluate operands in an unevaluated
or unreached context. The source constant/dynamic distinction is fixed before
optimization or inlining. Lowering uses Section 3.8's atomic operations,
never ordinary memory access or an undeclared external helper.

**GNU read-modify-write builtins.** These builtins follow GCC 14 on both
targets. `__atomic_fetch_OP(p, v, order)` returns the old value and
`__atomic_OP_fetch(p, v, order)` the new one, for `OP` in `add`, `sub`,
`and`, `or`, `xor` and `nand`; `nand` is `~(old & v)`. The pointee is an
integer of 1, 2, 4 or 8 bytes other than `_Bool`, and `v` converts to its
type; arithmetic wraps at that width. On an object pointer, `add` and `sub`
move the address by `v` bytes, unscaled, as GCC documents: `v` is an integer
converted to `unsigned long`. `__atomic_exchange_n(p, v, order)` stores `v`
and returns the old value; it also admits `_Bool` and any pointer pointee.
`__atomic_compare_exchange_n(p, e, d, weak, success, failure)` evaluates its
six operands once each, left to right. `e` points to an unqualified object of
the pointee type. An ordinary load reads `*e` before the atomic operation; the
operation stores `d` when the object equals that value, integers by value and
pointers by address, and returns 1. Otherwise an ordinary store writes the
observed value to `*e`, and the result is 0. `weak` is evaluated and a weak
request is strong: the interpreters never fail spuriously. The result has
type `_Bool`. `__atomic_test_and_set(p, order)` exchanges the first byte of
the object with 1 and returns whether it was nonzero; `__atomic_clear(p,
order)` stores the byte 0. Both admit any object pointer.

Every order admits the constants 0 to 5 except: `__atomic_clear` admits
relaxed, release and seq_cst; the failure order of a compare-exchange is not
release or acq_rel and not numerically greater than the success order. These
are exactly the pairs for which GCC 14 does not warn with
`-Winvalid-memory-model`. Every other constant is `E2947` at its operand,
including a constant outside 0..5 where GCC sometimes stays silent. A
run-time order is evaluated once and makes the operation seq_cst; for a
compare-exchange either run-time order makes both halves seq_cst. EvoIR keeps
one order for a compare-exchange: the success order, or acq_rel for a release
success with a consume or acquire failure. Each half is at least as strong as
its source order. A const pointee, a floating pointee, a record, a 16-byte
integer, a function-pointer or `_Bool` arithmetic operand, bitwise arithmetic
on a pointer and a qualified `*e` are `E2913`.

**GNU generic forms.** `__atomic_load(p, ret, order)`, `__atomic_store(p,
val, order)`, `__atomic_exchange(p, val, ret, order)` and
`__atomic_compare_exchange(p, expected, desired, weak, success, failure)`
follow GCC 14 on both targets for an object `*p` of type `T`: an integer,
enumeration, pointer, floating, record or union type, `_Atomic` or not, whose
size is 1, 2, 4 or 8 bytes and whose alignment, the atomic layout of 10.1.21
included, is at least its size. Each other operand except the orders and
`weak` points to a complete object of a type compatible with the non-atomic
`T`, ignoring qualifiers, and of the same size. The object of a store,
exchange or compare-exchange, `ret` and `expected` are not const. The
operands are evaluated once each, left to right, and the orders follow the
scalar forms: a load admits the load orders, a store the store orders, an
exchange every order, and a compare-exchange the pairs above. Lowering uses
the `atomic.copy` forms of Section 3.8, so the forms move object
representations: padding bytes and pointer provenance survive. A
compare-exchange compares every byte, padding included (C23 7.17.7.4p3), and
an uninitialized compared byte is `E3001` in both interpreters, as their
`memcmp` is. This is the profile's conservative boundary; C gives such a
comparison an unspecified result, not undefined behavior. On success the
desired bytes replace the object and the result is 1. On failure the object
is unchanged, a `copy` moves the observed bytes into `*expected`, and the
result is 0; `*expected` is never written on success. A weak request is
strong. The result has type `_Bool`; the other forms are `void`. An array,
function, vector or complex `T`, a variably modified `T`, a size outside 1,
2, 4 and 8 (16-byte records and `__int128` included), an alignment below the
size, a mismatched buffer, and a const buffer or object that the form writes
are `E2913`, at every warning level and language mode. EvoCC never calls the run-time library for
these forms. Clang refuses them on an `_Atomic` object and uses its
`__c11_atomic_*` builtins; the crossed tests show the two interfaces
updating one object.

**GNU fences.** `__atomic_thread_fence(order)` and
`__atomic_signal_fence(order)` are library builtins of type `void (int)`.
A relaxed constant order has no effect: only the operand is evaluated. Any
other thread fence lowers, like `__sync_synchronize`, to an implicit
`asm volatile` with a `memory` clobber, no operands and the current
`mem`/`world` tokens: template `dmb ishld` for consume and acquire and
`dmb ish` otherwise, and `dmb ish` for a run-time order. A signal fence has
the empty template, so it orders the compiler only. Both interpreters report
`E3021` on reaching one of these barriers.

**GNU lock-free queries.** In both queries `size` must be an integer constant
expression, or the query is `E2913`. `__atomic_always_lock_free(size, 0)` is
an integer constant expression of type `_Bool`: 1 for sizes 1, 2, 4 and 8,
and for 16 on Darwin only, as Apple clang answers; 0 otherwise. Its pointer
must be a null pointer constant (`E2913`). `__atomic_is_lock_free(size, p)`
evaluates `p` and is 1 when that query is 1 and `p` is a null pointer
constant or points to a type whose alignment is at least `size`. Any other
answer needs the run-time library and is `E2935`. 16-byte atomic operations
stay `E2913` on both targets. `__has_builtin` answers 1 for every builtin of
this section.

**Interpreter scope.** Both interpreters run one thread. They show values,
operand effects, the compare-exchange write-back and the order of atomic
events. They do not show inter-thread ordering; the crossed native tests of
`crates/evocc/tests/gnu_atomic_builtins.rs` and
`crates/evocc/tests/generic_atomic_records.rs` exercise it against GCC 14,
Clang 19 and Apple clang, and a passing run of them is not a memory-model
proof.

**Library builtins.** `__builtin_memset`, `__builtin_memcpy`,
`__builtin_memcmp`, `__builtin_strcmp`, `__builtin_strncmp` and
`__builtin_strlen`, and `__builtin_memmove` (`void *(void *, const void *, size_t)`), have the prototypes of the library functions they name; `__builtin_prefetch(addr[, rw[, locality]])` is `void` with one to three arguments; `__builtin_extract_return_addr(p)` returns `void *`
(`memset` returns `void *`, `strlen` returns `size_t`); `__builtin_alloca(n)`
returns `void *`; `__builtin_va_list`, `__builtin_va_start`,
`__builtin_va_arg(ap, T)` (of type `T`) and `__builtin_va_end` are typed by the
front end and lowered as 10.1.17 states. `__builtin_isdigit`,
`__builtin_isalpha`, `__builtin_isspace`, `__builtin_toupper` and
`__builtin_tolower` are GCC builtins, and `__has_builtin` answers for them as
GCC does. They return `int` with C-locale semantics over the value of their
`int` argument; the answer for -1 and 0x130 is 0.
`__builtin_isdigit(arg)` specifically resolves to the ordinary integer
expression `((unsigned int)(int)(arg) - 48u) <= 9u`, with one argument
evaluation and an `int` result of zero or one, not an external call or a new
EvoIR intrinsic. A compatible visible `isdigit` definition does not replace
this builtin operation; ordinary calls to `isdigit` retain the external-call
contract. The other four ctype builtin spellings retain that call contract.

**Hosted library builtins.** These are also library builtins in the builtin
namespace of 10.1.3, with the prototypes of the functions they name:
`__builtin_printf` (`int printf(const char *, ...)`), `__builtin_sprintf`,
`__builtin_snprintf`, `__builtin_malloc` (`void *malloc(size_t)`),
`__builtin_free` (`void free(void *)`), `__builtin_strcpy`
(`char *strcpy(char *, const char *)`), `__builtin_strncpy` and
`__builtin_strchr`. Anonymous formatting arguments take the default argument
promotions, and each argument is evaluated once. No intrinsic or interpreter
model is added: wrong arity is `E2909`, an incompatible argument `E2913`, an
incompatible visible declaration `E2927`, and a reached external call `E3039`.
`__builtin_puts` is `E2935` (`unsupported-library-builtin`); an ordinary
declared `puts` is unaffected. The address argument of `__builtin_prefetch`
accepts an object pointer with any pointee qualifiers and converts it to
`const void *` without reading the object; the address expression is evaluated
exactly once, and ordinary pointer conversions are not loosened.

**Literal string comparison.** `__builtin_strcmp` of two narrow string literals
folds to the `int` -1, 0 or 1 by unsigned code-unit order up to the first NUL,
before any same-named declaration is consulted. Explicit `__builtin_` forms
keep this folding, and that of `__builtin_strlen` (10.1.17), under
`-fno-builtin`. A nonliteral `__builtin_strcmp` with a visible same-named
`static` function is `E2946` (`builtin-local-binding-conflict`).

**Dynamic stack allocation.** `__builtin_alloca(n)` converts `n` to `size_t`,
evaluates it once and lowers to `alloca.dynamic` (3.7) at alignment 16. The
storage is uninitialized and lives until the containing function returns, not
until its block exits. With builtin recognition enabled, a call of an
externally linked `alloca` with the compatible prototype `void *(size_t)` has
the same meaning; GNU89 implicit-call mode supplies that prototype, and GNU11
requires a declaration. `-fno-builtin` disables this recognition but not
`__builtin_alloca`, and a same-named `static` function stays ordinary. A
recognized call when the unit defines an external `alloca` or renames it by an
assembly label is `E2944` (`ordinary-builtin-binding-conflict`), even when the
definition follows the call.

**Absolute-value, termination and prefetch builtins.** `__builtin_abs`,
`__builtin_labs` and `__builtin_llabs` (`int(int)`, `long(long)`,
`long long(long long)`) evaluate the converted operand once and return its
absolute value; a minimum operand reports `E3004`, also in wrapping mode. They
lower to shift, xor and subtract under that obligation and are never integer
constant expressions. A compatible direct call of the external `abs`, `labs` or
`llabs` is the same operation unless `-fno-builtin` is in effect (the last of
`-fno-builtin` and `-fbuiltin` wins); internal, pointer and assembly-renamed
calls stay ordinary calls. `__builtin_abort()` (`void (void)`) is an ordinary
external call of `abort`; `__builtin_exit` and `__builtin__Exit` are `E2933`
(`unsupported-termination-builtin`). `__builtin_prefetch` emits no access and
no call but evaluates its address argument once.

**Target types in otherwise unused declarations.** The
GNU target-header profile admits the builtin type spellings `_Float16`,
`_Float32`, `_Float64`, `_Float128`, `_Float32x`, `_Float64x`, `__fp16`,
`__Float32x4_t`, `__Float64x2_t`, `__SVFloat32_t`, `__SVFloat64_t` and
`__SVBool_t` in typedefs and prototypes. Each has its own canonical identity,
preserves qualifiers, and is not a same-size alias of `float`, `double`,
`long double`, or a GNU `vector_size` type. Their size/alignment
pairs are respectively 2/2, 4/4, 8/8, 16/16, 8/8, 16/16, 2/2, 16/16 and
16/16; the last three are sizeless and never receive a fabricated object
layout. Values of `_Float16` and `__fp16`, and of `__bf16`, execute as 3.13.6
states. For the other types only the declaration/type-query capability is
added: an unused prototype needs no EvoIR declaration, while a required value
or call with these types remains at the existing unsupported-type
execution/lowering boundary. This does not admit other floating-point or
vector execution. Seven of these spellings exist only in GCC; the profile
follows the GCC target headers.

**Integer-vector profile.**
`vector_size(B)` creates a canonical vector of B / sizeof(element) lanes,
not a layout annotation on a scalar. Integer lane count is a positive power
of two; layout is B bytes with alignment min(B,16). Qualifiers and vector
identity survive typedefs, arrays, records and signatures. GNU vectors and
GCC's nominal `__Int8x8_t`/`__Uint32x4_t` family are distinct source types
even when their representations match; the latter admits all 64/128-bit
integer combinations of 8/16/32/64-bit lanes, with long/unsigned long for
64-bit lanes. Floating, polynomial and architecture-intrinsic types/builtins
remain separate capabilities, not implied by these integer names.

Arithmetic has no scalar integer promotion of lanes. Integer-vector assignment
can preserve equal-width lane representations across signedness; this does
not make their declaration types compatible. Comparisons yield
same-width integer masks (all ones/zero); scalar broadcast requires a legal
element conversion. Compound assignments evaluate the place and RHS once.
Signed shifts retain the GNU truncating rule (6.1) and full-width count
check; other integer obligations and wrapping options apply per lane.
An element can be assigned but its address is refused; vector increment/
decrement is also outside the consensus extension set. GNU dynamic
`__builtin_shuffle` uses lane indices modulo the input lane count, while
`__builtin_shufflevector` has constant indices (-1 produces an unspecified
lane); conversion and equal-size reinterpretation are distinct operations.
Integer vectors participate in fixed/variadic calls and homogeneous vector
aggregates through EvoMIR's real ABI, not scalar-parameter substitution. A
homogeneous vector aggregate is the AAPCS64 one of the floating-vector
profile below: its members have one vector size, whatever their lane types.
General-register-only mode still forbids executed SIMD values.

**Tiny vectors at a call boundary.** A tiny vector is an integer vector of
1, 2 or 4 bytes. Its call convention is fixed by the target triple; no option
changes it. Lane `i` of a vector with `w`-byte lanes occupies bits `8*i*w` to
`8*(i+1)*w - 1` of its integer image, and bits above the vector are
unspecified.

- `aarch64-unknown-linux-gnu` follows GCC 14. A parameter, a result and an
  anonymous argument are the unsigned integer image of the same size: the
  next general register or an 8-byte stack slot, and a result in `w0`. No
  SIMD register is involved, so `-mgeneral-regs-only` admits them. Clang 19
  returns tiny vectors in `v0`, so a call that returns a tiny vector between
  EvoCC and Clang code is a recorded incompatibility. Arguments, anonymous
  arguments and records agree with Clang.
- `aarch64-apple-darwin` follows Apple clang. A parameter is the 4-byte
  integer image: the next general register, or a 4-byte slot of the named
  stack area. An anonymous argument takes an 8-byte slot. A result returns in
  `v0` with each lane widened to fill 64 bits: four 8-bit lanes as 16-bit
  lanes, two lanes as 32-bit lanes, and one lane in lane 0. The caller reads
  only the low bits of each widened lane. Under `-mgeneral-regs-only` a
  function or call with a tiny-vector result is E4002.

A record that contains a tiny vector follows the ordinary composite rules; a
tiny vector is never an HVA member (EvoMIR 6.2 and 6.6).

**Floating-vector profile.** The 2026-10-13 decision on L05 admits GNU
`vector_size` vectors of `float` and `double` lanes on both targets, with the
layout of the integer profile. A vector of 8 or 16 bytes is a `vec<L, f32>` or
`vec<L, f64>` value (3.8); a wider vector is an object, as an integer vector
is. Lanes of `long double` stay `E3050`. So do the nominal
`arm_neon.h` floating types, such as `__Float32x4_t`, whose intrinsics belong
to L05b. A `vector_size` attribute on a half format is `E2903` (3.13.6).

- `+`, `-`, `*`, `/`, unary `-` and unary `+` are the scalar operations of 3.13
  on each lane: rounding, NaN selection, signed zeros and subnormals follow the
  scalar rule. No multiply and add contract into one operation. A scalar
  operand broadcasts when the integer profile's rule admits it, and so does an
  exact finite `long double` constant, as GCC 14, Clang 19 and Apple clang
  accept it.
- A comparison gives a vector of signed integer lanes of the lane width, all
  ones for true and zero for false. A relational lane is a signaling
  comparison and an equality lane is a quiet one (3.13.1). An 8-bit mask lane
  is `signed char` on Linux, as in GCC 14, and `char` on Darwin, as in Apple
  clang. A 64-bit mask lane is `long` on Linux, as in GCC 14, and `long long`
  on Darwin, as in Apple clang, for integer and floating lanes alike. These
  mask types do not depend on `-fsigned-char` or `-funsigned-char`. The mask
  lane types have one representation and one ABI.
- Lanes execute in ascending order. In a strict region (3.13.1, 3.13.3) each
  lane is a strict operation, so the vector operation raises the union of the
  flags of its lanes, and each lane rounds in the current direction.
- `__builtin_convertvector` is the C cast of each lane. A floating lane that
  converts to an integer outside the range of the integer type, or a NaN lane,
  is `E3063` at that lane, as a scalar cast is. An integer lane converts to a
  floating lane, and a `float` lane to a `double` lane or back, with one
  rounding.
- Shuffles, lane reads and writes, copies, loads, stores and equal-size casts
  between floating and integer vectors move bits. `__builtin_shufflevector` and
  `__builtin_shuffle` keep the index rules of the integer profile.
- `!`, `&&`, `||`, `%`, `~`, `&`, `|`, `^`, the shifts and a cast between a
  floating scalar and a vector are `E2102`, and a vector condition of `?:` is
  `E2202`, as GCC 14 and Clang 19 refuse them in C. As in GCC 14, `++` and `--`
  add or subtract one in each lane, and `&v[i]` is the address of lane `i` of
  the vector object.
- A short floating vector passes and returns in a SIMD register and uses the
  shared SIMD cursor (EvoMIR 6.3). A record of one to four short vectors of one
  size is an HVA whatever its lane types, as AAPCS64 states, and it passes in
  vector registers. Its `hva` mark records the member count and the lane count
  and lane width of its first member. A record whose size is not the size of
  its members has no HVA shape and follows the composite rules. A vector wider
  than 16 bytes is a composite passed by reference.
- A 4-byte floating vector is no AAPCS64 short vector. GCC 14 and Clang 19 pass
  it differently, so a parameter, result, argument or `va_arg` of that type is
  `E4002`; inside a function it is a value. Under `-mgeneral-regs-only` every
  floating vector value is `E4002`, a copy included.

**Legacy `__sync` builtins.** `__sync_synchronize()` has type `void`, takes
no arguments and is a full memory barrier; it is not a constant expression and
declares no external function. It lowers to an implicit `asm volatile` with
template `dmb ish`, a `memory` clobber, no operands and the current
`mem`/`world` tokens, so both interpreters report `E3021` on reaching it. On a
naturally aligned 1/2/4/8-byte integer or pointer object, the fetch-and-op and
op-and-fetch forms of `add`, `sub`, `and`, `or`, `xor` and `nand`, the
compare-and-swap forms and `__sync_lock_test_and_set` evaluate each operand once
and lower to Section 3.8's `atomic.rmw`; `__sync_lock_release` is a `release`
atomic store of zero. All use `seq_cst` except `acquire` for lock-test-and-set.
Wrong arity is `E2909`; the existing type and alignment diagnostics apply.

**More library builtins.** `__builtin_strcat` (`char *(char *, const char *)`),
`__builtin_strncat` (`char *(char *, const char *, size_t)`) and
`__builtin_memchr` (`void *(const void *, int, size_t)`) are library builtins:
each is an external call through the unit's builtin namespace with each
argument evaluated once, and `E2909`, `E2913` and `E2927` apply.

**Object-size checked builtins.** The fortified headers of glibc and of the
macOS SDK select the checked builtins below. Each one
names a C library function and its checked entry point, and has the
prototype of that entry point:

| Builtin | Checked entry point prototype |
|---|---|
| `__builtin___memccpy_chk` | `void *(void *, const void *, int, size_t len, size_t size)` |
| `__builtin___strlcpy_chk`, `__builtin___strlcat_chk` | `size_t (char *, const char *, size_t len, size_t size)` |
| `__builtin___memcpy_chk`, `__builtin___memmove_chk`, `__builtin___mempcpy_chk` | `void *(void *, const void *, size_t len, size_t size)` |
| `__builtin___memset_chk` | `void *(void *, int, size_t len, size_t size)` |
| `__builtin___strcpy_chk`, `__builtin___stpcpy_chk`, `__builtin___strcat_chk` | `char *(char *, const char *, size_t size)` |
| `__builtin___strncpy_chk`, `__builtin___stpncpy_chk` | `char *(char *, const char *, size_t len, size_t size)` |
| `__builtin___strncat_chk` | `char *(char *, const char *, size_t, size_t size)` |
| `__builtin___sprintf_chk` | `int (char *, int flag, size_t size, const char *, ...)` |
| `__builtin___vsprintf_chk` | `int (char *, int flag, size_t size, const char *, va_list)` |
| `__builtin___snprintf_chk` | `int (char *, size_t len, int flag, size_t size, const char *, ...)` |
| `__builtin___vsnprintf_chk` | `int (char *, size_t len, int flag, size_t size, const char *, va_list)` |
| `__builtin___printf_chk` | `int (int flag, const char *, ...)` |
| `__builtin___vprintf_chk` | `int (int flag, const char *, va_list)` |
| `__builtin___fprintf_chk` | `int (FILE *, int flag, const char *, ...)` |
| `__builtin___vfprintf_chk` | `int (FILE *, int flag, const char *, va_list)` |

The checked entry point is `__X_chk` for the builtin `__builtin___X_chk`, and
the plain function is `X`. Its prototype is the checked one without `flag`
and `size`. A call evaluates each argument once and is an external call
through the unit's builtin namespace: of the plain function when the check
cannot fail, otherwise of the checked entry point, which stops the program
when the destination is too small. The check cannot fail when `size` is the
constant `(size_t)-1`, which means that no size is known, or when the
constant `len` is no greater than the constant `size`. A formatting builtin
also needs the constant `flag` 0, because a nonzero flag asks the C library
for format checks as well. The plain call drops the constant `flag` and
`size` arguments. The front end applies this rule to source constants. An
object-size query of 10.1.17 that substitution can still answer, such as the
size behind a parameter, is not yet a constant, and substitution applies the
rule again after it resolves the query. `E2909`, `E2913` and `E2927` apply
to both prototypes. Example: in `char b[8];`,
`__builtin___memcpy_chk(b, s, 8, __builtin_object_size(b, 0))` calls
`memcpy`, and `__builtin___memcpy_chk(b, s, n, __builtin_object_size(b, 0))`
calls `__memcpy_chk(b, s, n, 8)`. `__builtin___memccpy_chk`,
`__builtin___strlcpy_chk` and `__builtin___strlcat_chk` use the Clang/macOS
extension prototypes; the same bound rule applies to them. The `len` of
`strlcat` bounds the total destination capacity, unlike that of `strncat`.

For `strcpy` and `stpcpy`, a narrow string literal supplies a second sufficient
condition: its byte count through the first NUL is no greater than the constant
destination size. The implicit terminating NUL counts. An embedded NUL stops
the count, and adjacent literal fragments retain their C decoding. This condition
does not apply to `strcat`, which also reads the destination's current length.

The four stream-output builtins have no size argument. They retain the checked
entry point and the flag, including flag zero. The stream type uses the
file-scope `FILE` typedef if it is present, otherwise an opaque `void *`.
No library runtime is added by these builtins: the selected target library must
provide any checked entry point that remains. Unsupported external calls in
the interpreters remain refusals; native success does not extend that model.

**Instruction cache refresh.** `__builtin___clear_cache` has the prototype
`void (void *begin, void *end)`. It evaluates each argument once and calls the
target runtime's `__clear_cache` for the half-open range `[begin, end)`. It is
an effectful external call, not a prefetch hint or a no-op. The target runtime
owns the cache maintenance and synchronization. An interpreter without that
external model refuses the call. Arity and operand-type errors use E2909 and
E2913, as for other library builtins.

**GNU argument pack.** `__builtin_va_arg_pack ()` stands for the anonymous
arguments of the enclosing variadic function. It is valid only as the last
argument of a call to a function with a variadic prototype, after at least
as many arguments as that prototype names. `__builtin_va_arg_pack_len ()` is
the `int` count of those arguments, valid only in a variadic function. Both
have meaning only where the `always_inline` substitution of 10.1.9 replaces
them; glibc's fortified `printf` family uses them in this way. Any other
position is `E2950` (`invalid-va-arg-pack`), and so is a body that keeps them
when it is lowered, for example because the unit takes its address. The
interpreters report `E2950` when they reach one.

**Unknown builtins.** `__builtin_`, `__atomic_` and `__sync_` begin the GNU
builtin namespaces. A call of an undeclared identifier that begins with one of
them is `E2949` (`unknown-builtin`) at that identifier, in every language mode
and at every warning level, `-w` included. On `aarch64-apple-darwin`, whose
`<stdatomic.h>` is Clang's, `__c11_atomic_` is such a namespace as well
(10.1.21). An implicit declaration cannot
supply a compiler builtin, so such a call is never an external call. For an
identifier from a macro expansion, the location is the macro use, as for every
expanded token.

**Clang builtins without a model.** On `aarch64-apple-darwin`:

- A call of `__builtin_os_log_format` or `__builtin_os_log_format_buffer_size`
  is `E2965` (`unsupported-os-log-builtin`) at the builtin. `__has_builtin`
  answers 1 for both, as Apple clang does, because `<os/log.h>` stops at an
  `#error` without them; the header compiles and its other declarations are
  usable. Example: `os_signpost_event_emit` calls them and is refused at the
  macro use.
- A call of a `__builtin_arm_` builtin is `E2966` (`unsupported-acle-builtin`)
  at the builtin, and so is a use of `__dmb`, `__dsb`, `__isb`, `__wfi`,
  `__wfe`, `__sev`, `__sevl` or `__yield`, which `<arm_acle.h>` declares as
  functions and Clang compiles to instructions. In a function body the
  refusal is deferred, so a body that the unit drops is not refused; such a
  call of a builtin that `<arm_acle.h>` or `<arm64/mte.h>` calls takes the
  type that Apple clang 21 gives it. Any other one is refused at once. The
  target predefines `__ARM_ACLE` as `202420` and `__ARM_ACLE_VERSION` as
  Apple clang does, and none of the `__ARM_FEATURE_` macros of the optional
  instructions, so `<arm_acle.h>` and `<arm64/mte.h>` compile.

**Integer complex.** GNU integer `_Complex T` has two adjacent components of
integer type `T`, real then imaginary. Arithmetic works per component at
component width, conversions convert each component, the real and imaginary
projections are lvalues, and conjugation negates the imaginary component. It
passes as an ordinary composite under AAPCS64, not as a vector or floating
aggregate. Overflow, initializedness, qualification and object/provenance
checks apply per component.

**Integer-complex division.** Division uses GCC's wide algorithm in the
common component type `U` on both targets. `U` is the usual arithmetic
conversion of the operand component types. A complex operand keeps its
component type; a real integer operand is promoted and gets a zero imaginary
part. For `x = a + bi` and `y = c + di`, with `|v|` the negation of a negative
signed `v`:

```text
if |c| < |d|:  ratio = c / d;  div = c * ratio + d
               r = (a * ratio + b) / div;  i = (b * ratio - a) / div
else:          ratio = d / c;  div = d * ratio + c
               r = (b * ratio + a) / div;  i = (b - a * ratio) / div
```

Each step is the C operation on `U`, in the written order, with its existing
obligation. Signed overflow, `|v|` of the minimum value included, is `E3004`.
Division by zero is `E3002`; a zero divisor reaches `d / c` with `c == 0`.
Unsigned steps wrap. Only the selected branch executes, so the lowering
branches on `|c| < |d|` and never selects between two computed quotients.
The result has type `_Complex U`. When both operands are integer constant
expressions, the steps use the constant arithmetic of 10.1.19, and a step
that divides by zero or computes `MIN / -1` makes the expression
non-constant (`E2208` in a static initializer). `-fcx-limited-range` and
`-ffast-math` do not change this rule. Apple clang uses the textbook formula
instead, so Darwin results can differ from Apple clang's.

**Integer-complex updates.** For `x op= y` with `op` one of `+`, `-`, `*`
and `/`, where either side is integer complex and the other has an integer
type, `y` converts to `_Complex U` for the component type of `x` and of `y`.
A complex `x` gets each component of the result converted back to its
component type. A real `x` is evaluated once as an object and gets the result
converted to its type, so a `_Bool` target tests both components. When either
side has a floating type, the update follows the floating complex rules
below. A pointer target is `E2102`.

`++` and `--` add or subtract one in the real part and leave the imaginary
part unchanged. A component narrower than `int` is promoted before the
operation and converted back afterwards; an `int` or wider signed component
keeps the existing `E3004` overflow obligation. The prefix form yields the
updated complex value, and the postfix form yields the old value.

**Imaginary integer constants.** A GNU imaginary integer constant is an
integer constant with exactly one `i`, `I`, `j` or `J` letter anywhere in its
integer suffix. The suffix starts after the last digit of the constant's base
and follows GCC's grammar: at most one `u` or `U`, at most one imaginary
letter, and one `l`/`L` or two adjacent `l`/`L` of the same case; it holds
nothing else. Thus `4uil` is valid, `4lil` is not, and `4lL` is not. The type
is `_Complex U`, where `U` is the type the underlying integer constant has
without the letter; its real part is zero and its imaginary part is the
constant value. `4i` and `4j` are `_Complex int`; `4il` and `4li` are
`_Complex long`; `4ill` and `4lli` are `_Complex long long`; `4ui` and `4iu`
are `_Complex unsigned int`; `4uli` is `_Complex unsigned long`; and `0x10i`
is `_Complex int`. A floating imaginary constant follows the floating rules
below, and two imaginary letters are not a constant. In `#if`, every
imaginary integer constant and every invalid integer suffix is `E0111`.

**Floating complex.** `float _Complex`, `double _Complex` and `long double
_Complex` have two adjacent parts of the real type, real then imaginary, so
the size is twice that of the real type and the alignment is the same. Plain
`_Complex` is `double _Complex`, as GCC and Clang read it. `long double` has
the target's format: binary128 on Linux, binary64 on Darwin. A value passes
under AAPCS64 as an HFA of its two parts, also as a variadic argument and in
a record, with the HFA rules of 3.13. Lowering keeps a value in memory, as
for integer complex; it is not a vector.

These operations work per part with the scalar rules of 3.13, and their
strict forms in strict regions:

- Unary `+` copies, and unary `-` negates each part. `~` and the `conj`
  builtins negate the imaginary part.
- `__real__` and `__imag__` of a complex lvalue are lvalues of the part and
  access only that part. Of a real operand, `__real__` is its value and
  `__imag__` is a zero of its type. The `creal` and `cimag` builtins give the
  parts as values.
- `__builtin_complex(x, y)` takes two operands of one real floating type,
  after lvalue conversion and without promotion, and copies them exactly: no
  arithmetic and no status. A floating imaginary constant, such as `2.5i`, has
  a positive zero real part. `x + y * I` is arithmetic: `0 * inf` gives NaN.
  The Darwin SDK defines `CMPLX`, `CMPLXF` and `CMPLXL` only when `__clang__`
  is defined. EvoCC does not define that macro, so those spellings remain
  undeclared on Darwin; `__builtin_complex` is the supported replacement.
- Addition and subtraction add or subtract each part. A real operand is not
  converted to complex (C11 6.3.1.8, G.5.1), as GCC and Clang compute: the
  usual arithmetic conversions give the real type of each part, `r - x` has
  the negated imaginary part of `x`, and multiplication or division by a real
  operand works per part.
- Conversion between complex types converts each part. Conversion to a real
  or integer type converts the real part; the imaginary part takes no part
  and raises no status. Conversion from a real or integer type gives a
  positive zero imaginary part.
- `==` and `!=` compare both parts with the quiet `oeq`. An ordered
  comparison is `E2102`. A truth test (`!`, `&&`, `||`, a condition, a
  conversion to `_Bool`) compares the real part with zero by the quiet `une`,
  and the imaginary part only when the real part is zero, as GCC builds it.
  Strict code branches, so the second comparison executes only then.
- A compound assignment converts its operands as the binary operator does,
  then converts the result to the type of the target. GNU `++` and `--` add
  one to the real part or subtract one from it.
- A volatile complex object is read and written as two volatile accesses, one
  of each part, real part first. Integer complex follows the same rule.

A product of two complex values, a quotient of two complex values and a
quotient of a real by a complex value lower to the parts of 3.13.5, after
both operands convert to the common complex type; the real dividend gets a
positive zero imaginary part. GCC's `-fcx-limited-range`, `-fcx-fortran-rules`,
`-fcomplex-arithmetic=basic` and `-ffast-math` stay refused.

**Floating-complex constants.** Translation-time evaluation, as in a static
initializer, computes a product or quotient with the helper model of 3.13.5
in RNE, and the other operations per part with the scalar rules. GCC 14 folds
a constant product or quotient correctly rounded with MPC, so an inexact
constant can differ from GCC's static value in the last bit; it is equal to
the run-time result of both compilers. Example: `(1.0 + 1.0i) / (0.2 + 0.5i)`
has the imaginary part `-0x1.08d3dcb08d3ddp+0` here and at run time, and
`-0x1.08d3dcb08d3dcp+0` in a static initializer of GCC 14. On Darwin, a
translation-time product or quotient is `E2960`.

**Interchange complex types.** `_Float16 _Complex`, `_Float32 _Complex`,
`_Float64 _Complex`, `_Float128 _Complex`, `_Float32x _Complex` and
`_Float64x _Complex`, in either word order, are the types that GCC 14 admits
on aarch64 Linux. Each is distinct from the complex type of the standard type
of the same format, and has twice the size and the alignment of its real
type. They are admitted in declarations, so glibc's `<complex.h>` and
`<tgmath.h>` compile with `_GNU_SOURCE`. Their values are refused: an
initializer, an argument, a returned value or an operand of one is `E2202` or
`E2102`. A function definition with such a parameter is `E3049` where it is
lowered, and a cast of such a value is `E3101`, as for the real `_FloatN`
types. `__fp16 _Complex` and `__bf16 _Complex` are `E1007`, as GCC rejects
them.

**Scalar broadcast.** An integer scalar no wider than an unsigned lane converts
to the lane's unsigned representation before broadcast, negative values
included; a signed lane accepts a source only when the source's whole range
fits. A wider constant broadcasts when its value fits the lane range, or, for
an unsigned lane, when it is negative and at least that width's signed
minimum. Every other broadcast is `E2102`.

**AArch64 target-header profile.** The GCC 14 target-header profile defines, as
real canonical types and records, the nominal 64/128-bit half, bfloat, floating
and polynomial vector types, polynomial scalars of 1/2/8/16 bytes, and the 90
`val`-array tuple typedefs that `#pragma GCC aarch64 "arm_neon.h"` introduces.
Builtin prototypes are exactly the versioned table
`crates/evocc-types/src/neon-signatures.tsv`, pragma-injected reinterpret
declarations included, and declaration-only floating bodies are type-checked.
Polynomial scalars keep their own type identity but use ordinary scalar storage
and unsigned bit representation; polynomial and ordinary integer types are never
compatible.

**Darwin NEON header types.** On Darwin, `neon_vector_type(N)` and
`neon_polyvector_type(N)` count lanes. The total size must be 8 or 16 bytes,
and alignment equals that size. The ordinary form accepts the supported
integer and floating element types, including the declarable half and opaque
types. The polynomial form accepts unsigned 8-, 16- and 64-bit elements.
Integer and `float`/`double` vectors with the same element type and lane count
are compatible with the corresponding GNU `vector_size` type. Lanes follow
the target's little-endian order; the existing short-vector and HVA ABI applies.
Both attributes report `__has_attribute == 1` on Darwin. The nominal half
and opaque vector value restrictions remain in force.

Darwin's `__mfp8` is a nominal opaque type with size and alignment 1. It
admits typedefs, pointers, size/alignment queries and one-byte storage copies,
loads and stores. It is not an integer or arithmetic type. Numeric casts are
`E2102`; arithmetic and comparison keep the existing `E2202` refusal, and a
truth operand is `E2103`. Its by-value call interface, including a vector or
composite that contains it, is `E3049`; a pointer to it is admitted. Linux
does not admit this spelling (`E1007`).

The measured Apple clang builtin declaration table is
`crates/evocc-types/src/neon-apple-signatures.tsv`. Its prototypes let unused
header bodies type-check. A prototype does not admit execution: a retained
call without an implemented operation is `E2949`, and a builtin absent from
the declaration table remains a named unknown-builtin refusal (`E2949`).
Darwin defines `__ARM_FEATURE_DIRECTED_ROUNDING=1` so that Apple's header
declares its rounding functions with their vector return types. This does not
admit their builtin execution.
The same declaration-only rule applies to `__builtin_roundeven` and
`__builtin_roundevenf`, which Apple's unused ACLE rounding wrappers call.

Darwin's `__builtin_bit_cast(T, value)` preserves the complete image when the
source and destination have the same size. This subset accepts scalar
integer, floating and target-storage types and short vectors. Pointer,
record and union operands are refused (`E2102`), and the existing restrictions
on executable formats still apply. It adds no initialization or provenance.

The generic forms `__builtin_neon_vld1q_v`, `__builtin_neon_vst1q_v`,
`__builtin_neon_vzipq_v` and `__builtin_neon_vuzpq_v` admit the source-constant
selectors 32, 33 and 34 (signed 8-, 16- and 32-bit lanes). Other selectors keep
`E2949`. Loads and stores transfer 16 bytes with byte alignment, ordinary
bounds and initializedness checks. Zip stores the lower and upper interleaved
halves of the two vectors; unzip stores their even and odd lanes, respectively.
The two results occupy byte offsets 0 and 16 of the destination object. Each
value operand is evaluated once in source order. These forms lower through
existing vector and memory instructions and never become external calls.

**Target intrinsics.** A registered integer or polynomial intrinsic evaluates
each operand once after its declared-parameter conversion and lowers to
existing vector, integer and memory instructions, never to an external call:

- `ld1`/`st1` access element-aligned memory; `ld2`/`ld3`/`ld4` fill the
  canonical tuple with `result.val[j][i] = source[i*count+j]`, with ordinary
  faults and initializedness and no atomicity or vector-alignment claim;
- immediate left shifts take counts 0..N-1 and right shifts 1..N; left/right
  insertion keeps the first operand's unshifted low/high bits (0..N-1 / 1..N);
  `shrn_n` shifts right logically and keeps the low half-width bits (1 to the
  destination lane width); a full-width right shift still consumes the lane's
  initializedness;
- single-table byte lookup yields zero for an index outside the table;
  equal-representation reinterpretation keeps the bits; `get_low`/`get_high`
  select the lower/upper 64 bits in lane order (the `v2di` form returns its
  scalar `long`); `combine` puts its first operand in the low half;
- byte polynomial multiply yields the low 8 bits of the carryless product;
  `crypto_pmulldi_ppp` and `crypto_pmullv2di_ppp` yield the full 128-bit
  carryless product of two 64-bit polynomials (the `v2di` form uses lane 1);
  `rbitv8qi`/`rbitv16qi` reverse the bits of each byte lane;
- the four AES round interfaces lower to Section 3.8's `vec.aese`, `vec.aesd`,
  `vec.aesmc` and `vec.aesimc`.

Immediate counts must be source integer constant expressions after parameter
conversion; an optimizer-discovered constant does not qualify. Each consumed
lane keeps its initializedness check, and selection never initializes unread
lanes. Any other registered operation, a floating variant or an unsupported
immediate is `E3052` naming the builtin; wrong arity or operand type is
`E2909`/`E2913`.

#### 10.1.15 Checked arithmetic builtins

`__builtin_add_overflow(a, b, p)`, `__builtin_sub_overflow(a, b, p)` and
`__builtin_mul_overflow(a, b, p)`: `a` and `b` have integer types, including
signed and unsigned `__int128`, which may differ in width and signedness; `p`
is a pointer to a non-`_Bool` integer type. The operation is computed in
infinite precision, the result is converted to the pointee type as if by
assignment (wrapping) and stored through `p`, and the call yields `_Bool` `1`
when the infinite-precision result is not representable in the pointee type,
else `0`. Both operands and `p` are evaluated exactly once, left to right. The
call is never an integer constant expression. A `p` whose pointee is
`const`-qualified is `E2913` at the third operand. `__builtin_*_overflow_p` is
outside this profile. Examples: `mul(70000, 70000) -> int` is `1` with
`605032704` stored, and `-> long` is `0` with `4900000000`;
`add(200, 100) -> unsigned char` is `1` with `44`;
`sub(4000000000u, -1) -> long long` is `0`; `add(-1, 0) -> unsigned` is `1`.

The front end keeps the call in C-HIR as a checked arithmetic node, and the
C-HIR interpreter implements the rule above. Lowering computes the exact
result and then asks whether it fits. The exact arithmetic is **unsigned**
when the operation is `mul` and both operands are unsigned, and **signed**
otherwise. The required width is one more than the wider operand for `add` and
`sub`, and the sum of the two widths for `mul`, counting an unsigned operand
of a signed operation as one bit wider. When it is at most 128, `W` is the
smallest of 32, 64 and 128 that holds it. Each operand is `sext` or `zext` to
`bits<W>` by its own signedness, the operation is performed in `bits<W>`, and
the result is converted to the destination type `R` and stored. The builtin's
value is `icmp.ne` between the `bits<W>` result and the conversion of the
stored value back to `bits<W>`, with two corrections. A negative exact result
always overflows an unsigned `R`, even when `R` is as wide as `W` or wider. An
exact unsigned `bits<W>` result with its high bit set overflows a signed `R`
only when `R` is no wider than `W`; a wider signed `R` holds it.

When the required width exceeds 128, the exact result is an exact sign, a
128-bit magnitude and a magnitude-overflow flag. It is tested against the range
of `R` and stored modulo the width of `R`, and a zero result is nonnegative.
For `mul` the magnitude overflows exactly when `a > UMAX / b`, with `b` taken
as 1 when it is zero, so existing unsigned operations and the 128-bit division
of 10.1.3 suffice. Neither path needs a predicate or an obligation: the
arithmetic cannot overflow by construction, and no division by zero occurs.

#### 10.1.16 GNU `asm` at the front-end boundary

Syntax accepted, in every position GNU C accepts it:

- basic `asm("template");` at file scope and as a statement;
- extended `asm` with up to four colon-separated sections: outputs, inputs,
  clobbers, and (for `asm goto`) labels; each operand is `[symbolic-name]`
  optional, a constraint string, and a parenthesised expression;
- qualifiers `volatile`, `inline`, `goto` in any order, each at most once;
- `__asm__`, `__asm` and `__volatile__` spellings; `__asm` is recognized after
  preprocessing in every assembly position, and C-HIR keeps the written
  spelling;
- an `asm("name")` label after a declarator (`int x asm("sym");`), which
  renames the object or function for the assembler and is otherwise inert.

The front end checks that output operands are modifiable lvalues, that every
`%[name]` in the template names an operand, that `asm goto` label operands are
labels in scope, and that string literals concatenate; it keeps the statement
in C-HIR losslessly (template text, operands with constraints, clobbers,
labels, qualifiers, spans). Constraint strings are preserved as text and not
validated in this revision. `asm` has the effects of Section 4.2: it reads
and writes `mem<all>` and `world`, may not be reordered, duplicated or
removed, and an
`asm goto` is a terminator whose successors are its labels plus fall-through.
Lowering produces the `asm` instruction of Section 3.11 when every operand
constraint, clobber and template reference is in the vocabulary that section
admits, and reports `E3021` (`not-lowered: asm`) naming the offending
constraint, clobber or reference otherwise. A file-scope `asm("template")`
lowers to one entry of the module assembly table (Section 2.1) and is never
`E3021`. An output operand lowers to a
result of the operand's lowered type followed by a store to the operand's
place; an input operand lowers to the loaded value; a `+` operand lowers to
both, tied; a `Q` operand lowers to the address of its place, and the place
is not otherwise loaded or stored, so the memory effect of Section 3.11 is
what covers it. Both interpreters report `E3021` when they reach an `asm`:
it is executed by the native run and by nothing else.

Errors (`E2914`, `malformed-asm`), each at the span named: an output operand
that is not a modifiable lvalue, at the operand expression; a `%[name]` that
names no operand or label, at the template string literal; a qualifier written
twice, at the second qualifier; an `asm goto` label that does not resolve, at
the label operand; a labels section without `goto`, at the fourth colon.

**Statement placement.** An attribute specifier may precede a **null statement**
and no other statement: `__attribute__((__fallthrough__));` is admitted, and an
attribute before any other statement is `E2924`
(`attribute-on-non-null-statement`) at the attribute. The attribute is
preserved in C-HIR with its placement and applied to nothing: a null statement
has no semantics to refine, so `fallthrough` in particular does not change
control flow, suppress a diagnostic, or reach EvoIR.

Lossless means: the qualifier spellings and `asm`/`__asm__` spelling as
written, the template as the single concatenated string the assembler will
see (the individual string-literal pieces are not kept), every operand's
symbolic name, constraint text and expression, clobber strings in order, and
label names. The keyword-alias and piece spellings are the only things the
representation drops.

**Wide bit-fields in arithmetic.** For a bit-field whose declared type is
wider than `int`, the reference compilers disagree: GCC computes arithmetic in
the field's own precision and Clang in the declared type. `E2939` marks the
uses where the two give different values, as **Bit-field promotions** below
states.

**Bit-field promotions.** Integer promotions use a bit-field's width, whatever
its declared base, including `long` and `long long`: below 32 bits, or signed
at 32 bits, it promotes to `int`; unsigned at 32 bits, to `unsigned int`.
Layout is unchanged, and nothing is promoted outside arithmetic. A *wide
field*, wider than 32 bits and narrower than its declared type, computes in
the declared type in these uses: truth tests; right shifts; comparisons,
except an unsigned field against a signed operand that is neither a
nonnegative constant nor wider than the declared type; arithmetic whose other
operand is not a wide field and is at least as wide as the declared type,
except an unsigned field with a signed operand of that width; a conditional
whose other arm fits the common field precision; compound assignment under
these rules, and any unsigned `+=`, `-=`, `*=`, `&=`, `|=`, `^=`, `++` or
`--`; and a direct, possibly parenthesized, read as the left operand of `<<`
or `<<=`, where the storage width governs only the store and the GNU shift
rule of 6.1 applies (the count keeps its full width, and an invalid count is
`E3005`, never `E3006`).

Every other arithmetic use of a wide field, such as unary `~` or `-` or a
signed `++`, is `E2939` (`unsupported-wide-bit-field-promotion`). The result
of unary `+`, a comma, a conditional, a right shift, an assignment or an
update stays a wide-field value, and a wide shift count never widens a shifted
value. A negative arm converted to an unsigned field-precision result is
`E2939`, as is arithmetic or a conditional that mixes an unsigned field with
a signed operand of the declared width. Two forms clear only their own
operation's `E2939`, keeping its evaluation, side effects and other refusals:
an add, subtract, multiply or bitwise result, unsigned in both precisions and
not of that mixed form, compared with integer zero only for its sign; and a
direct unsigned wide-field read ANDed with a nonnegative constant mask within
the field's width, consumed by an explicit cast to a non-Boolean unsigned type
no wider than the field. An uncast type query keeps `E2939`.

**Unreferenced internal functions.** A function with
internal linkage that nothing in the unit references by call, address,
alias or module assembly is not lowered, and no lowering refusal
(`E30xx`) or deferred front-end profile refusal (such as `E2939`) is raised for its
body; the front end still parses and checks it. This extends the
`always_inline` rule of 10.1.9 to every unused internal function and matches
the reference compilers, which emit no code for such a function. A unit is
therefore refused only for constructs in code the reference build also
contains. The drop is computed to a fixed point before lowering, as EvoMIR
does after it.

**Alignment of an incomplete array member.** `__alignof__`
and `_Alignof` of an expression or type whose type is an array, including
a flexible array member, answer the element type's alignment raised by an
`aligned` attribute on the member (`__alignof__(tfm->__crt_ctx)`); no layout of the incomplete array is
needed, and `E3101` is not raised for the query. The query
answers an expression or a member; `_Alignof` of an incomplete array type name,
such as `_Alignof(int[])`, is not admitted, and its requesting context refuses
it.

#### 10.1.17 Compile-time builtins, `__auto_type` and small GNU forms

- `__builtin_choose_expr(c, a, b)`: `c` is an integer constant expression;
  the result *is* `a` when `c` is nonzero, else `b`: same type (no conversion),
  same value category (an lvalue stays assignable), same constant-expression
  status. The unselected operand is parsed and type-checked but never
  evaluated. Non-constant `c` is `E2912` at `c`.
- `__builtin_types_compatible_p(T1, T2)`: integer constant expression of type
  `int`, `1` when the two type names are compatible after dropping top-level
  qualifiers (`const int` vs `int` is `1`; `int[4]` vs `int[]` is `1`).
- `__builtin_offsetof(T, designator)`: the designator is a member path with `.`
  and `[index]` steps. A
  path the type lacks is `E2906`. With every index an integer constant
  expression the result is an integer constant expression of type `size_t`.

  An index need not be constant. `__builtin_offsetof(T, m[e])` is then the
  runtime value `offsetof(T, m) + e * sizeof(*m)`, `e` is evaluated exactly
  once, and the result is **not** an integer constant expression, so each
  context that requires one reports its own diagnostic (`E2912` in a static
  assertion, `E2923` or `E2203` in an array bound). The index is not bounds
  checked: a negative or past-the-end index yields the arithmetic value, which
  a flexible array member requires (`arr[-1]` is 0 and `arr[9]` is 40 for
  `struct { int head; int arr[4]; }`).
- `__builtin_ffs`, `__builtin_ffsl`, `__builtin_ffsll`, `__builtin_clz`,
  `__builtin_clzl`, `__builtin_clzll`, `__builtin_ctz`/`l`/`ll`,
  `__builtin_popcount`/`l`/`ll`: result type `int`; `ffs(0)` is `0`; `clz`/`ctz`
  of `0` is undefined (`E3004`-class runtime report `E3022`,
  `bit-builtin-zero-operand`); an integer-constant-expression operand makes the
  call an integer constant expression. The front end folds constant operands;
  non-constant operands become a C-HIR intrinsic that lowers to the `ctlz`,
  `cttz`, `ctpop` and `bswap` instructions of Section 3.4 (the zero-operand
  obligation is `ub.assume.bit_builtin_zero_operand`).
- `__builtin_clrsb`, `__builtin_clrsbl`, `__builtin_clrsbll`: the operand is
  converted once to `int`, `long` or `long long`; the `int` result counts the
  leading bits equal to the sign bit, excluding the sign bit, so `0` and `-1`
  give the width minus one. `__builtin_parity`, `__builtin_parityl`,
  `__builtin_parityll`: the operand is converted once to `unsigned int`,
  `unsigned long` or `unsigned long long`; the `int` result is the population
  count modulo 2. All six are total, fold constant operands, use the
  diagnostics of the bit builtins above, and lower to `ctlz`, `ctpop` and
  width-typed arithmetic with no zero-operand obligation.
- `__builtin_unreachable()`: a statement-expression of type `void`; reaching
  it is undefined behaviour reported as `E3024` (`unreachable-reached`) by the
  interpreters and lowered to `trap.unreachable_reached` (Section 4.2).
- `__builtin_trap()`: no arguments, type `void`, no constant value. It lowers
  to `trap.builtin_trap` (Section 4.2), and reaching it is `E3047`
  (`builtin-trap-reached`) in both interpreters, an explicit termination, not
  a UB report. The AArch64 emitter uses `brk #1000`. An unevaluated or
  unselected occurrence does not terminate.
- `__builtin_classify_type(expr)`: an `int` integer constant expression; `expr`
  is not evaluated. Integer and enumeration types give 1, pointers (including
  decayed arrays and functions) 5, real floating types 8, complex types 9,
  structures and `__builtin_va_list` 12, and unions 13. `_Bool` and target
  types with no agreed class are `E2941` (`unsupported-type-classification`);
  a `void` operand is `E2913`, zero or several arguments `E2909`, and a type
  name `E1003`.
- `__builtin_object_size(p, mode)`: `mode` is an integer constant expression in
  `0..3`; any other value, or a non-constant mode, is `E2925`
  (`builtin-mode-out-of-range`) at the mode operand.
  The operand is parsed and type-checked but **never evaluated**. The builtin
  returns the remaining byte extent when it can be proved from a declared object or
  subobject, a string literal, and constant address offsets. Even modes refer
  to the complete object; odd modes refer to the nearest surrounding subobject.
  Unknown complete-object identity does not prevent proving a declared fixed
  member's odd-mode extent: `p->name` for `char name[16]` gives 16 in modes 1
  and 3 without reading `p` or the member. One-past gives zero; an offset
  outside a proved extent gives zero. These results remain integer constant
  expressions and resolve before EvoIR; parser and consumers share the query.

  Unproved extents, variable bounds, dynamic offsets and effectful operands
  retain `(size_t)-1` in modes 0/1 or `0` in modes 2/3. The analysis may decline
  multi-object flow rather than invent a bound; casts never create an allocation
  extent. In particular, side-effecting operands stay unknown and unevaluated.
  For a multidimensional array row (`char m[3][4]; m[1]`), the odd-mode row
  extent remains unknown, because the reference compilers disagree.
- `__builtin_dynamic_object_size(p, mode)` has the same mode, type and unknown
  answers, but may return a runtime extent proved from allocation-time argument
  captures, captured array bounds, subobject offsets and selected pointer flows.
  It does not reevaluate the allocation, a mutated size variable or an effectful
  query operand, and is not a source integer constant expression. A static
  `__builtin_object_size` may use these facts only when the answer is constant.
  All answers are resolved in the shared C-HIR path before EvoIR; when analysis
  cannot establish an extent it retains the unknown answer, not an invented size.
  This is the narrow `alloc_size` exception of 10.1.9, not general permission to
  apply other preserved attributes.
- `__builtin_has_attribute(operand, attribute)`: a GCC builtin, available
  because EvoCPP presents itself as GCC (10.5). The result is an integer
  constant expression of type `_Bool`, usable in `_Static_assert`. The operand is a type name or an expression; an
  expression is not evaluated, and the answer is `1` exactly when the named
  attribute was written on the declaration of the entity the expression
  designates — for `nonstring`, on the array member it names, through `.` or
  `->` alike — and `0` otherwise, including for a type name and for an entity
  declared without it. Because 10.1.9 preserves every attribute losslessly, the
  answer is read from that inventory; no attribute needs a meaning to be
  queried.
- **Variable arguments at the front end.** `__builtin_va_start(ap, param)`,
  `__builtin_va_end(ap)` and `__builtin_va_copy(dst, src)` are accepted with
  type `void`: `ap`, `dst` and `src` must have type `__builtin_va_list`
  (10.1.5), `va_start` may appear only in a function whose parameter list ends
  in `...` (`E2926`, `va-start-outside-variadic-function`), and `param` must
  name the last named parameter (the standard makes another parameter
  undefined, so this profile refuses it, `E2926` at `param`).
  `__builtin_va_arg(ap, T)` is admitted at the front end: `ap` must
  have type `__builtin_va_list` and `T` must be an object type that is not an
  array or function type; the expression is an rvalue of type `T`, kept in
  C-HIR as a `VaArg` node holding the list place and the type, with no
  constant value. Lowering: a function type whose
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
  `__builtin_va_copy(dst, src)` to a target-sized `copy` (Section 3.12). A
  `va_arg` of a record or union type lowers to `va_arg.byval`, and such an
  anonymous argument carries the call-site `byval` mark (3.10, 3.12); a shape
  that 3.10 does not admit stays `E3030` (`not-lowered: varargs`). `E3030` is no longer reported for a variadic declaration or call.
- **Empty records.** A struct or union whose member list declares no member
  (for instance one containing only a `_Static_assert`) is a GNU extension with
  `sizeof` 0 and alignment 1.
- `__builtin_strlen(s)`: a constant string designator gives an extended integer
  constant expression of type `size_t`. Its value counts bytes from the
  designated position to the first NUL in the innermost character array.
  The operand is not evaluated and no function is called. This rule applies
  on both targets, at every optimization level and in every ICE context.
  Parentheses and casts to character pointers preserve admission. The forms are:
  a narrow literal (no prefix or `u8`); decay of an immutable file-scope
  character array, including its constant-index rows and record-member arrays;
  an immutable file-scope character pointer; `d + k`, `k + d` and `&d[k]`
  with an integer-constant index and character-element stride; and a conditional
  with an integer-constant condition and an admitted selected arm. Every
  intermediate position must stay inside the same innermost array; one-past
  positions and a missing terminating NUL do not qualify.
  An admitted object has an explicit initializer before the use, static
  storage duration at file scope, and a const-qualified type (including an
  array's element type), without volatile or atomic access. It is not
  thread-local, weak, aliased or assembly-renamed. Its designated array bytes
  come only from narrow string literals, integer constant expressions and
  initialization zero fill; an inactive union member or aggregate copy does
  not supply those bytes. A character pointer object has type `T *const` and
  an admitted address initializer; at most one pointer object is read. A
  preceding const integer object's value does not become an ICE in an index
  or byte initializer. A later declaration that changes an admitted object's
  binding to weak, alias or an assembly name is E2954, not a stale constant.
  Automatic objects, block-scope statics, `__func__`, pointer-array elements,
  pointer members, comma expressions and statement expressions do not qualify.
  Every other operand keeps an ordinary call of `strlen`, with each operand
  effect executed once and with the builtin prototype
  (10.1.14, `size_t (const char *)`), whether or not a declaration is visible.
  A visible incompatible declaration on that call path is E2927. The plain
  spelling `strlen(...)` remains an ordinary call.
  Except for the explicitly resolved `__builtin_isdigit` and
  `__builtin_extract_return_addr` operations, the same
  holds for every library builtin of 10.1.14: the builtin's
  prototype is the call's type, and the call names the external function.
  No intrinsic reaches EvoIR. A user-defined `strlen` is called for an
  operand outside the constant-string rule.
- `__builtin_frame_address(n)` and `__builtin_return_address(n)` have type
  `void *`. Their operand must be an integer constant expression, converted
  to `unsigned int`. A converted depth above 65535 is E2913. A wider constant
  that narrows to an admitted depth uses that depth, including narrowing to zero.
  Depth zero lowers to `frame_address` or `return_address`, respectively
  (Section 3.7). A depth from 1 to 65535 yields null with `Null` provenance on
  both targets and in both interpreters. No ancestor frame is read. The builtin
  expression is not an integer constant expression, a null pointer constant
  or an address constant. This source rule does not change the low-level
  `frame_address depth=n` operation for independently supplied EvoIR.
- `__builtin_extract_return_addr(p)`: evaluates `p` exactly once and returns
  the same address with the same provenance; it is not an external call.
- `__auto_type`: exactly one declarator, which must be a plain identifier (no
  pointer, array or function declarator), and an initializer is required; the
  declared type is the initializer's type after lvalue conversion (qualifiers
  dropped, arrays and functions decay: `const int` gives `int`, `int[3]` gives
  `int *`). The declared object may not appear in its own initializer.
  Violations, including a pointer declarator (`__auto_type *p = &l;`), are
  `E2915` (`auto-type-misuse`) at the declarator.
- A stray `;` where an external declaration or a block item may appear is an
  empty declaration and has no effect. It is not a diagnostic in this dialect.
- `a ?: b`: `a` is evaluated once; the result is `a` when nonzero, else `b`,
  with the usual conditional-operator typing.
- `__extension__`: one or more prefixes on an expression leave its type,
  value, constant-expression status and evaluation unchanged, including in a
  `for` initializer expression. A prefixed declaration in a `for` initializer
  is `E1003`.
- `case a ... b`: inclusive range in a `switch` whose bounds are integer
  constant expressions. A non-constant bound, `a > b`, or a range overlapping
  another case is `E2916` (`case-range-invalid`).
- `__label__ a, b;` at the start of a block declares each listed name as a
  distinct label in that block's scope; nested blocks do not share them. A
  `goto` naming a label that no enclosing function scope declares is `E2920`
  (`undeclared-label`) at the label name.
- Label addresses: `&&name` is the address of a label in the current function,
  of type `void *`; it is an address constant (10.1.18 "Label addresses"), so
  a static initializer
  may use it, and `&&name` where no enclosing function scope declares that
  label reports `E2920`. `goto *expr` transfers control to such an address.
  Both forms are parsed, type-checked and carried in C-HIR. Label addresses
  lower to `block_addr`, integer casts to `ptr.to_int`, and indirect jumps to
  the complete edge-binding form of 3.9. Static label tables use relocations
  (2.1); no cross-block SSA visibility exception is implied.

**Function names.** Inside every function body the identifiers `__func__`,
`__FUNCTION__` and `__PRETTY_FUNCTION__` denote one implicitly declared object of type
`static const char[N]` holding the function's name: `sizeof __func__` is N,
`_Generic` sees `const char *` after decay, `&__func__` is
`const char (*)[N]`, and its address is an address constant (`static const
char *p = __func__;` is accepted). `__builtin_constant_p(__func__[0])`
follows the constant rule of this section and answers 0. `__PRETTY_FUNCTION__`
holds the bare function name, as GCC spells it for C. `E2921`
(`pretty-function-unsupported`) is withdrawn.

**Labeled last statement.** The value and type of a statement expression whose
last statement is a labeled statement are those of the statement the label
prefixes, recursively; `__label__` declarations inside the statement
expression do not change its type (`r = ({ __label__ out; ...; out: q; })`
is valid).

#### 10.1.18 C11 declaration forms

The front end accepts the complete C11 declaration grammar. The following
forms are named so that their acceptance is checkable:

- **Specifier order is free.** Type qualifiers, storage-class specifiers and
  function specifiers may appear before or after a type specifier, including
  after a record or enum specifier: `struct midr_range const *ranges` and
  `enum e const x` mean exactly what `const struct midr_range *ranges` and
  `const enum e x` mean.
- **A typedef may name a function type.** `typedef int fn(void);` then
  `extern fn f;` declares `f` a function, and `fn` may also be written
  parenthesised (`typedef int (fn)(void);`). Such a typedef may not be used to
  define a function body, and an object declared with it is a function
  declaration, never an object.
- **A declarator list may declare several functions.** `fn a, b;` with `fn`
  a typedef naming a function type, and `int f(void), g(void);`, each declare
  every listed name a function; none of them may be a definition.
- **`typeof` of a dereferenced function pointer.** `*fp`, with `fp` a pointer
  to function, is a function designator, so `typeof(*fp)` is that function
  type and `extern typeof(*fp) h;` declares `h` a function exactly as
  `extern typeof(g) h;` does (10.1.10).
- **Redeclaration.** An identifier with linkage — a function, or an object
  declared `extern` — may be declared any number of times, in the same scope
  or in nested ones, provided every declaration's type is compatible with the
  previous ones (C11 6.2.7, 6.7p4); the declared type is their composite.
  Attributes on a redeclaration accumulate. Declaring it again with an
  incompatible type is `E2002` (`incompatible-redeclaration`) at the later
  declarator, as is a second definition of an object or function, or a second
  declaration of an identifier without linkage in the same scope: repeated
  compatible block-scope `extern` declarations are accepted, and
  `extern int g(int)` then `extern long g(int)` is refused.
- **Block-scope function declarations.** A function first declared in a
  block names the same non-builtin symbol as a later file-scope definition
  with that linkage; a definition inside the block stays invalid.
- **Identifier lists and empty lists.** An identifier-list definition keeps
  its parameters in that order with their adjusted declared types; its call
  interface applies the default argument promotions unless a preceding
  prototype gives types compatible with each declared type or its promotion.
  In the default language mode each listed parameter has exactly one
  declaration, without storage class or with `register`; a missing or extra
  declaration, an initializer or an implicit `int` is `E1001`, and a duplicate
  declaration or an incompatible preceding prototype is `E2002`. A definition
  written with `()` has zero parameters and is still not a prototype.
- **Call targets.** After the ordinary conversions the called expression must
  have pointer-to-function type; a visible non-callable object is never
  replaced by an implicit declaration, and the call is `E2937`
  (`invalid-call-target`).
- **`_Noreturn`.** It is accepted on function declarations and definitions
  and kept as the `noreturn` attribute with its written spelling, giving no
  control-flow licence; on an object or a typedef it is `E2934`
  (`function-specifier-on-non-function`).
- **Static assertions are member declarations.** `_Static_assert` may appear in
  a struct or union member list, including an anonymous one, where it declares
  no member and contributes nothing to layout.

**Array parameters.** A parameter declared with array type, whatever the
bound and whatever qualifiers or `static` stand inside the brackets, has the
adjusted pointer type in the function's type and at every use of the
parameter name. The adjustment happens once, when the declarator is built,
and no other code path sees the array type.

**Parameter adjustment and identity.** A parameter written with function type,
directly or through a typedef, is adjusted to a pointer to that exact function
type; an array typedef adjusts as an array does. Checks of the preserved
written declarator against the canonical type undo only this outermost
adjustment and compare everything inside it unadjusted. Top-level parameter
qualifiers do not affect these checks or compatibility, but the parameter
object keeps them.

**Pointer sign in conversions.** In initialization, assignment, argument
passing, `return`, `==` and `!=`, a pointer to an integer type converts to a pointer to
the same type with the other signedness, or between `char`, `signed char`
and `unsigned char`; the conversion is accepted without a diagnostic.
Pointers to otherwise incompatible types are `E2202`.

**Function pointers and `void *`.** A pointer to function converts to and
from `void *` in initialization, assignment, argument passing, `return`,
casts and both arms of the conditional operator; `(fn_t)0` and `(void *)0`
are null pointer constants there, and `cond ? impl : (void *)0` has the
function pointer type. Arithmetic on a `void *` operand follows 10.1.3 and
its result may be converted to a function pointer the same way.

**Qualified `void *` and function pointers.** The conversion between a
pointer to function and `void *` of 10.1.18 holds for `void *` with any
qualifiers on the pointee (`const void *data` to `fn_t`, and back) at every
site named there.

**Conditional address constants.** A conditional expression is an address
constant when its condition is an integer constant expression or an address
constant (a function or object address, which is never null) and the arm
selected is an address constant: `(impl == (fn_t)0) ? impl : impl`,
`1 ? impl : (void *)0`, `impl ? impl : 0`.

**Label addresses.** `&&label` is an address constant of type `void *`
usable in static initializers, including range designators
(`[0 ... 255] = &&dflt`); `goto *expr` is accepted by the front end with
`expr` of pointer type. A label block that no direct or indirect (3.9)
terminator edge reaches cannot satisfy 2.5: lowering reports `E3101` for it
and never fabricates an edge. A used `&&label` lowers to `block_addr` of the
label's block (Section 3.7), and as a static initializer to a relocation
against a local code symbol the backend emits for that block (2.1);
`goto *expr` uses the explicit edges of 3.9.

**`_Pragma`, spellings and attribute placement.** `_Pragma("...")` is handled
exactly where `#pragma` would be, including at file scope between
declarations and inside a macro expansion; `__inline` and `__inline__` spell
`inline`; `__restrict` and `__restrict__` spell `restrict`; `__const` and
`__const__` spell `const`, `__volatile` and `__volatile__` spell `volatile`,
and `__signed` and `__signed__` spell `signed`, under the ordinary type rules;
one or more `__extension__` prefixes on a record member declaration, including
a bit-field, an anonymous record or a declarator list, have no effect; an attribute may
stand between any two declaration specifiers, as in `static const
__attribute__((unused)) struct r`.

**Global register variables.** A file-scope declaration
`register T name asm("reg")` is a global register variable (GNU). It
declares no object and supplies no storage: it has no allocation site, and
the allocation-site manifest of 10.1.3 passes over it rather than failing to
derive one. Its type is `T`, `sizeof` and `_Alignof` are `T`'s, and `&name`
is `E2928`. A read yields the
current value of the named machine register and a write sets it. EvoIR has
no machine-register value, and does not need one: a read lowers to the
`asm` instruction of Section 3.11 with the template `mov %0, reg`, one
`=r` output of `T`'s lowered type, no inputs and the `volatile` qualifier;
a write lowers to `asm volatile "mov reg, %0"` with one `r` input. The asm is *implicit*: it is a lowering artefact, not
source text, and it carries every property of Section 3.11, so the value
is opaque to the equality database, ordered by the effect tokens, and
executed only by the machine; both interpreters report `E3021` when they
reach it, as for any assembly. A block-scope
`register T name asm("reg")` is an ordinary automatic object whose register
binding matters only when it is an `asm` operand, where `E3021` already
applies; its value behaves as any automatic object's.

**Lowering diagnostics name their class.** A lowering failure carries a code
of its own, as a front-end diagnostic does; `E3101`
(`evoir-lowering-failed`) is the wrapper for a failure that has no code of
its own, such as the unreachable label block above. A recurring class of
lowering failure must be given a code of its own.

**Storage class `register`, va_list objects, 128-bit places.** `register` is
accepted on parameters and block-scope objects and has no effect beyond
forbidding `&` on the object, which is `E2928` (`address-of-register-object`)
at the `&`. `__builtin_va_list` is an object type that may
be a member, an array element or a typedef target; it occupies
32 bytes on Linux and 8 on Darwin, at alignment 8. A structure holding one
pointer and one `va_list` is 40 bytes on Linux and 16 on Darwin, at alignment
8. An element of an array of
`__uint128_t` or `__int128_t` is a modifiable lvalue like any element, and
`*__builtin_va_arg(ap, T *)` is a modifiable lvalue of type `T`; both
execute (Section 3.12, 10.1.3).

**Parameter compatibility.** Top-level qualifiers on parameter types do not
affect compatibility (C11 6.7.6.3p15): `int (*)(const clockid_t, struct ts *)`
and `int (*)(clockid_t, struct ts *)` are compatible. Function pointer
arithmetic `p + n`, `p - n` and the compound forms scale by one (GNU). These
operations, including the prefix and postfix forms, lower to
`ptr.to_int`, 64-bit integer arithmetic at byte scale one and
`ptr.from_int.fnptr<Sig>` (3.7): the lvalue and the integer operand are each
evaluated once, and no interior address gains Function provenance. The
difference of two function pointers is their byte-address difference, of type
`long`; one that `long` cannot represent reports `E3004`. A call through a
rebuilt pointer keeps the `E3015`, `E3016` and `E3019` checks, and `E3034` is
not reported for these forms.

**The `\e` escape.** `'\e'` is the GNU escape for 27.

**Qualified array pointees.** In initialization, assignment, argument passing
and `return`, the non-atomic qualification conversion also reaches the element
type through consecutive array layers: `int (*)[4]` converts to
`const int (*)[4]`. A known extent combines with an incomplete one; different
known extents stay incompatible. The conversion stops at an intervening pointer,
keeps atomic distinctions, and changes neither `__builtin_types_compatible_p`,
mutability nor layout. The excluded forms are `E2202` (`E2102` in assignment).

**Parameters.** A function definition can omit a parameter name; the
parameter keeps its type and call-interface slot and its argument is evaluated
and converted normally, but no identifier or parameter object is introduced.
An outermost `[*]` in a prototype parameter, named or abstract and optionally
bracket-qualified, adjusts to the corresponding pointer and has no bound to
evaluate. `[*]` on an object, in a definition, with `static` or in an inner
qualifier position is `E1005` (`invalid-declarator`).

**Variable array parameters.** An outer array parameter with a nonconstant
bound adjusts to a pointer like any array parameter. A prototype-style
definition evaluates each variable dimension of its parameters once on entry,
after binding the parameters and before the body, including inner dimensions
kept through the adjustment and pointer-to-array declarators; the captures
follow 10.1.19. Prototypes at file or block scope and nested callback
prototypes never evaluate their bounds. Inlining does not reevaluate them, and
the outer bound is not the adjusted parameter's `sizeof`. When array bounds
compose, a constant size wins (two different constants are incompatible), then
a specified capture wins over an unspecified variable or incomplete bound;
element and qualifier compatibility are unchanged. A variable bound in an
identifier-list definition is not admitted, and a noninteger bound is `E2923`.

**Function pointer equality.** `==` and `!=` admit a function pointer against a
pointer to qualified or unqualified `void`, in either order and including null
values. Each operand is evaluated once and the function identity is compared
through `ptr.to_int`; no ordering or dereference follows from this rule.

**Alignment specifiers.** `_Alignas(type-name)` and
`_Alignas(constant-expression)` among declaration specifiers set the alignment
of the declared object or member without changing its type. Zero is ignored,
and several nonzero specifiers take the largest. These are `E2938`
(`invalid-alignment-specifier`): a request that is not a power of two, is not
representable, is weaker than the natural alignment or exceeds 2^28; a weaker
`_Alignas` together with GNU `aligned`; and any specifier on a parameter,
function, typedef, `register` object, bit-field or type name. A request on an
array of unknown bound is checked against the element type's alignment without
a bound. A specifier on a standalone tag declaration is kept and does not
change the type, and `__builtin_has_attribute(x, aligned)` answers 1 for a
nonzero `_Alignas`. An explicitly aligned external declaration of an
incomplete record stays valid when the later definition is weaker or stronger.

#### 10.1.19 Integer constant expressions and array bounds

Every place that requires an integer constant expression (`_Static_assert`,
`case`, array bounds, bit-field widths, designator indexes, `aligned`,
enumeration constants, the builtins of 10.1.14 and 10.1.17) uses the C11
definition with this profile's extensions, and the same evaluator:

- integer, character and enumeration constants;
- `sizeof` and `_Alignof` applied **to a type name or to an expression**; the
  operand expression is not evaluated (except a `sizeof` operand of
  variable-length array type, below), only its type is taken, so
  `sizeof(*(int *)8)` is `4` and `sizeof(i++)` leaves `i` alone;
- `__builtin_offsetof`, `__builtin_types_compatible_p`, `__builtin_constant_p`,
  `__builtin_choose_expr` and the constant-operand bit builtins (10.1.14,
  10.1.17);
- `__builtin_strlen` of a constant string designator (10.1.17), as an
  extended integer constant expression;
- casts to an integer type, the unary and binary arithmetic, bitwise,
  relational and logical operators, and the conditional operator, applied to
  the above.

`sizeof(void)` is `1`, as is `sizeof` of a function type; both are GNU
extensions.

**Signed arithmetic in a constant expression** is evaluated in the operand
type's width with two's-complement wrap-around, not in host arithmetic and not
as a refusal: `1 << 31` is the `int` value `-2147483648`, `(1 << 31) < 0`
holds in a static assertion, and an enumerator `X = 1 << 31` has type `int` in
a 4-byte enumeration. This is the
constant evaluator's arithmetic rule, and it also applies outside contexts
that require a constant: an add, subtract, multiply or negation of integer
constant expressions wraps in its promoted type before any surrounding cast or
runtime operation. Object reads do not become integer constants; the bounded
string-initializer rule above is an explicit exception for that builtin. Signed
add/subtract/multiply of runtime values, division by zero and executed signed
`MIN / -1` retain their obligations; signed shifts follow the GNU truncation
rule of 6.1, so executed `1 << 31` needs only a valid count. IR modules that
use `signed_shl_defined` and explicit E3006 obligations keep their meaning.

`__int128_t` and `__uint128_t` are compiler-provided spellings of `__int128`
and `unsigned __int128`, not distinct types: they have size 16, and
`__builtin_types_compatible_p(__uint128_t, unsigned __int128)` is `1`. They are type specifiers wherever the
underlying spelling is one, and carry its value semantics unchanged.

**Null pointer constants.** An integer constant expression with value zero, or
such an expression cast to `void *`, is a null pointer constant (C11 6.3.2.3p3)
and converts, in assignment, initialization, argument passing, return and
comparison, to **any** pointer type — object pointers and function pointers
alike. The resulting value has `Null` provenance.

**Address constants through integer casts.** An address constant cast to an
integer type and back to a pointer type, `(void *)(uintptr_t)&f`, is an address
constant. This admits the *form*; whether the initializer can be lowered is
Section 2.1's relocation question (`E3026` when it is not a relocation).

**Conditional operator with a null pointer constant.** In `c ? a : b` where
one of `a` and `b` is a null pointer constant and the other is a pointer, the
result type is the *other* operand's pointer type, not a composite with `void *`
(C11 6.5.15p6). Example: the Linux kernel writes

```c
#define __is_constexpr(x) \
    (sizeof(int) == sizeof(*(8 ? ((void *)((long)(x) * 0l)) : (int *)8)))
```

which reads `4 == 4` when `x` is an integer constant expression, because
`(void *)((long)(x) * 0l)` is then a null pointer constant and the conditional
has type `int *`, and reads `4 == 1` otherwise, because the conditional is then
`void *` and `sizeof(void)` is `1`. A wrong rule produces no diagnostic; it
produces the opposite answer in every size and overflow check that uses the
macro.

**Array bounds.** A bound is evaluated wherever it is written, not only when an
initializer follows.

A bound of **zero** is valid: a zero-length array is a GNU extension with size
`0` that contributes no bytes and no alignment, may appear anywhere in a record
including before other members, and does not make the record flexible:
`struct { int a; char d[0]; int b; }` is 8, `struct { int a; char d[0]; }` is
4, `char pad[20 - 2*sizeof(long) - sizeof(unsigned)]` is a valid empty member
whose successor is at offset 0, and a file-scope `int g[0]` has `sizeof` 0.

A noninteger bound is `E2923` (`invalid-array-bound`) in every context. This
type check precedes the constant-expression and storage checks. An integer
bound that folds to a **negative** value is also `E2923`. A remaining integer
bound that is not an integer constant expression makes the type variably
modified. That is `E2203` (`variable-length-object`) at file scope, on a static
or linked object, on any declaration with linkage, and on a record member whose
canonical type is variably modified, including through a typedef or a pointer
to a variable array. Dimensions inside a member's function-pointer parameter
prototype do not make the member variably modified. `E2203` has one message,
`variable-length-object`, at every site.

**Variable-length arrays.** At block scope a variable bound is admitted. Each
variable bound written in an executed declaration, typedef, pointer
declarator, `typeof` type name or cast type name is evaluated once when that
declaration or cast executes, before any initializer, into a distinct
compiler-owned size object; an unselected expression evaluates none. Later
`sizeof`, subscripts, pointer addition in either order, updates and pointer
differences use the captured values, also through typedef chains, multiple
declarators, recursion and separate inline expansions; inlining renames the
captures in types and declarators. Captured sizes are not integer constant
expressions, alignment is the element type's, a pointer to a variable array
keeps a fixed size, and a variable bound alone does not make otherwise
compatible array types incompatible. Checked pointer execution keeps the full
scaled index and reports `E3011` for an unrepresentable delta or a zero or
overflowing captured stride.

A variable-length array object is allocated by `alloca.dynamic` (3.7) at its
declaration and ended by `lifetime.end` when its scope is left, including by a
backward jump to before the declaration; a later access is `E3010`.
`-ftrivial-auto-var-init=zero` also zeroes it, padding included, with Null
provenance for typed pointer subobjects. A `sizeof` operand of variable array
type is executed before its captured extent is read, `sizeof(type-name)`
evaluates only the bounds the queried extent needs, and a fixed-size operand,
including a pointer, stays unevaluated. A block-scope `static` pointer to a
variable array, or a fixed array of such pointers, is one object initialized
once, while its pointed-to dimensions are captured on each execution of the
declaration. Entering the scope of a variably modified declaration without
passing it, by `goto`, `asm goto` or `switch`, is `E2945`
(`jump-into-variable-type-scope`).

**Address constants.** `&E` is an address constant when `E` designates an
object of static storage duration or a function, or an element or member of
such an object reached through constant subscripts and member access
(`&vals[1]`, `&s.m`, `&s.a[2]`); so are an array or function name that
decays, a string literal, and `__func__`. An address constant converted by a
cast to any pointer type or to an integer type at least as wide as a pointer
stays an address constant. This holds in every static initializer position,
nested aggregate members included.

**Integer constants as pointers; decay in static objects.** An integer
constant expression converted by a cast to a pointer type is an address
constant (`(void *) 2`). The decay of any
array lvalue that designates part of an object with static storage duration
is an address constant as well: `init_s.x`, `arr2[1]`,
`init_files.close_on_exec_init`.

**More address constants.** `&"literal"` is an address constant (the address
of the literal's array), so `(kernel_ulong_t)&"PARPORT_SERIAL"` is one under
the integer-cast rule. A comparison of an address constant with a null
pointer constant is a constant expression (`impl == (fn_t)0` is 0,
`impl != 0` is 1), so the `_OF_DECLARE` conditional
`(fn == (fn_type)NULL) ? fn : fn` is an address constant under the
conditional rule of 10.1.18.

**Initializer plans have no size cap.** A plan records the entries an
initializer writes and the zero fill of the rest; its cost is bounded by the
number of entries, never by the object's size: a range designator `[lo ... hi]` is one entry
of the plan (its expansion to elements in 10.1.12 is a description of
meaning, not of representation), and no diagnostic refuses an
object for being large (`infos[4096]` with two designated entries).
`E2207` (`initializer-plan-too-large`) is withdrawn.

**Numeric address constants.** In a static address initializer, `&*p` with
`p` a numeric pointer constant is that numeric value, and `&p->m[i]` on such a
base, a zero base included, adds the projection's target-layout offsets without
reading storage. The result has no allocation provenance; a load, a bit-field
address and a nonconstant subscript are not admitted. In an integer case label,
an integer constant cast to a pointer and back keeps its target-width value. In
a static integer initializer, a comparison of pure numeric address constants
uses their unsigned target-width bits. None of these makes a symbolic address a
known integer, admits a pointer read or call, or makes a pointer comparison a
source integer constant expression.

**Const objects in static initializers.** In a static object's integer
initializer, a preceding explicitly initialized non-volatile, non-atomic
`const` integer object, automatic or thread-local included, supplies its
initializer's value converted to its declared type. An unknown value or a
declaration without a written initializer remains `E2208`. Object reads do not
become source integer constant expressions, and scope exit or a failed
declaration removes the binding.

#### 10.1.20 Source locations

**Restricted raw source bytes.**
Main input, stdin and included contents share a byte-preserving lexer.
Invalid UTF-8 data bytes in unprefixed ordinary narrow literals retain their
byte values; comments containing them follow the usual whitespace and line
splicing rules. Ordinary macro transport retains the token's raw spelling.
Original byte offsets and source digests are not based on lossy decoding,
Latin-1 transcoding or replacement characters. Character constants retain
the target's single-byte interpretation; this rule does not select signed char.
Invalid encoding in identifiers, other tokens, prefixed literals, include
names, stringization/prefix-changing paste, narrow/wide concatenation or
UTF-8-only assembly/attribute consumers remains E0002. Legal UTF-8 and existing
escapes are unchanged; arbitrary charset conversion is not admitted.
Lossless C escape serialization of narrow literal data is permitted, while
the original token spelling and physical locations remain available.

In an unprefixed ordinary narrow literal, a backslash followed directly by a
raw non-UTF-8 byte is a GNU unknown escape that yields that byte; a pair of
backslashes still yields one backslash. A UTF-8-safe re-spelling given to a
literal decoder must keep escape boundaries, so an introduced numeric escape is
never read as literal digits. Prefixed literals, invalid-encoding metadata and
stringization gain nothing from this rule.

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
spelling span as a note. Example: in the three-line file `#define TAG s` /
`struct s { int x; };` / `struct TAG v;`, the tokens of `struct TAG v` have
expansion spans in the main file and `s` has its spelling span in line 1.

#### 10.1.21 `_Atomic` objects

**Types.** `_Atomic` is a qualifier and, when `(` follows it, the type
specifier `_Atomic ( type-name )` of C11 6.7.2.4. The type name of the
specifier must not be an array, function, atomic or qualified type, and the
qualifier must not apply to an array or function type (C11 6.7.3p3). Each
violation is `E2955` (`invalid-atomic-type`). An atomic bit-field is also
`E2955`: C11 6.7.2.1p5 leaves it implementation-defined, and GCC and Clang
refuse it. An evaluated `.` or `->` that designates a member of an atomic
record or union is `E2956` (`atomic-member-access`), because C11 6.5.2.3p5
makes the access undefined. The operand of `sizeof` is not evaluated, so a
member access there is allowed. A pointer conversion that adds or removes
`_Atomic` without a cast stays `E2202`.

**Layout.** On `aarch64-unknown-linux-gnu` an atomic type keeps the size of
its non-atomic type. A size of 1, 2, 4, 8 or 16 bytes raises the alignment
to that size, as GCC 14 does. On `aarch64-apple-darwin` a size of at most 16
bytes is rounded up to a power of two, and the alignment is that size, as
Apple clang does; the added bytes are padding. A larger atomic type keeps its
layout on both targets. Record members, arrays of atomic elements and calls
use this layout. Clang 19 on Linux uses the Apple rule. Its objects of an
atomic record whose size is not a power of two, or whose alignment is below
its size, do not match GCC's or EvoCC's (llvm-project issue #26836).

**Accesses.** The admitted atomic objects are integers, `_Bool`,
enumerations, pointers and binary32 or binary64 floating objects of 1, 2, 4
or 8 bytes. Lvalue conversion of one is a `seq_cst` `atomic.load` (3.8). A
simple assignment converts the right operand to the type of the object and
stores it with a `seq_cst` `atomic.store`; its value is the converted value.
A floating object moves as `bits<32>` or `bits<64>` through `float.to_bits`
and `float.from_bits`, without a numeric conversion. A compound assignment
or a prefix or postfix `++` or `--` is one `seq_cst` read-modify-write. The
place and the right operand are evaluated first. Then a `seq_cst` atomic load
reads the old value, and a retry block computes the new value with the
ordinary C computation and stores it with `atomic.rmw.cmpxchg seq_cst`. A
failed comparison branches back to the retry block with the observed value.
The computation keeps the obligations of the ordinary operator: signed
overflow is `E3004`, division by zero `E3002` and an invalid shift `E3005`,
and a pointer moves by whole elements under the pointer-formation rules. A
`_Bool` result is 0 or 1. A compound assignment and a prefix form yield the
new value; a postfix form yields the old value. `volatile` adds the volatile
marker to the same atomic accesses, so each access is one event. A read of a
`const` atomic object is an atomic load. Initialization by a declaration or a
compound literal is not an atomic access.

**Refusals.** A compound assignment, `++` or `--` of a floating atomic
object, or one whose computation is floating or complex, is `E2702`. The
rule for the floating flags of a discarded iteration (C11 footnote 113) is
not part of this revision. A read, assignment or update of an atomic record,
union, complex or vector object, or of a 16-byte atomic object (Linux `long
double`, `__int128`), is `E2702` at the read or the target. Declarations,
initializers, `&` and `sizeof` of these objects stay admitted.

**Values.** The value of an atomic lvalue has the non-atomic type (C11
6.3.2.1p2). `typeof` of an expression that is not an lvalue, `__auto_type`
and the controlling expression of `_Generic` see the non-atomic type;
`typeof` of an atomic lvalue keeps `_Atomic`.

**Interpreter scope.** Both interpreters run one thread. Each atomic access
appends one atomic event, so they show values, obligations and the order of
the accesses, and the first compare-exchange of an update succeeds. They do
not show inter-thread behavior. The crossed native tests of
`crates/evocc/tests/atomic_objects.rs` share one object among units built by
EvoCC and GCC 14 on Linux, or by EvoCC and Apple clang on Darwin. Every unit
must see the same layout, and two threads update the object through C
operators. A passing run of them is not a memory-model proof.

**Headers.** EvoCC uses the target's `<stdatomic.h>` unchanged. The GCC
header of the Linux sysroot calls the generic forms of 10.1.14. On
`aarch64-apple-darwin` the header is Clang's resource header, which the SDK
does not replace. It calls the Clang builtins below and reads the ten
`__CLANG_ATOMIC_*_LOCK_FREE` macros, which EvoCC predefines on Darwin only,
with Apple clang's value 2. Apple clang defines the `char8_t` one only for
C23, which EvoCC does not provide. The compiler identity stays GCC's.

**Clang builtins.** On `aarch64-apple-darwin` the front end provides exactly
the fourteen `__c11_atomic_*` builtins that Clang's `<stdatomic.h>` uses:
`init`, `load`, `store`, `exchange`, `compare_exchange_strong`,
`compare_exchange_weak`, `fetch_add`, `fetch_sub`, `fetch_and`, `fetch_or`,
`fetch_xor`, `thread_fence`, `signal_fence` and `is_lock_free`. Their
operands and orders are Apple clang's, as measured with Apple clang 21.
`__has_builtin` answers 1 for them on Darwin. On Linux they are ordinary
identifiers, as in GCC 14. The first operand of a value form points to an
`_Atomic` object of a type that the Accesses paragraph admits. Its non-atomic
type `T` is the value type, and the object is const only for `load`. Each
form is one access of Section 3.8, and its operands are evaluated once each,
left to right:

- `__c11_atomic_init(p, v)` converts `v` to `T` as by assignment and stores
  it with a relaxed `atomic.store`, as GCC's `atomic_init` does. A relaxed
  store refines the initialization of C11 7.17.2.2, which is not atomic.
- `__c11_atomic_load(p, order)` is an `atomic.load` of type `T`.
  `__c11_atomic_store(p, v, order)` converts `v` to `T` and is an
  `atomic.store` of type `void`. `__c11_atomic_exchange(p, v, order)` is an
  `atomic.rmw.exchange` that yields the old value.
- `__c11_atomic_compare_exchange_strong(p, e, d, success, failure)` and the
  `weak` form have the write-back of `__atomic_compare_exchange_n` (10.1.14):
  `e` points to an unqualified object of type `T`, `d` converts to `T`, the
  result is `_Bool`, and a weak request is strong.
- `__c11_atomic_fetch_OP(p, v, order)` for `OP` in `add`, `sub`, `and`, `or`
  and `xor` yields the old value. On an integer other than `_Bool`, `v`
  converts to `T`, and the arithmetic wraps at the width of `T` (C11
  7.17.7.5p3). On a pointer to a complete object type, `add` and `sub` take
  `v` as a `ptrdiff_t` count of elements. The count times the element size,
  wrapping at 64 bits as Apple clang computes it, is the byte count of the
  `atomic.rmw.add` or `atomic.rmw.sub`.
- A binary32 or binary64 object moves as `bits<32>` or `bits<64>` through
  `fp.to_bits` and `fp.from_bits`. A compare-exchange therefore compares
  object representations, as Apple clang's does: `-0.0` and `0.0` differ,
  and a NaN equals the same NaN.
- `__c11_atomic_thread_fence(order)` and `__c11_atomic_signal_fence(order)`
  are the GNU fences of 10.1.14.
- `__c11_atomic_is_lock_free(size)` is an integer constant expression of type
  `_Bool`. It is 1 for the sizes for which `__atomic_always_lock_free` is 1.
  For another constant size Apple clang calls the run-time library, so that
  size is `E2935`. A size that is not an integer constant expression is
  `E2913`.

Constant orders follow Apple clang's `-Watomic-memory-ordering` diagnostics.
A load admits relaxed, consume, acquire and seq_cst, and a store relaxed,
release and seq_cst. A compare-exchange admits every success order and a
failure order that is not release or acq_rel. Unlike GCC 14, Apple clang
admits a failure order stronger than the success order; the compare-exchange
then carries the failure order, which is at least as strong as both halves.
Every other constant, and every constant outside 0 to 5, is `E2947`. Apple
clang does not diagnose a fence constant outside 0 to 5; EvoCC refuses it, as
for the GNU fences. A run-time order is evaluated once and makes the
operation seq_cst.

These shapes are `E2913`: a first operand that does not point to an `_Atomic`
object; a const object that the form writes; a record, union, complex,
vector or 16-byte object; arithmetic on `_Bool`, which C11 7.17.7.5p1
excludes; arithmetic on a floating object; arithmetic on a pointer to `void`,
to a function, or to an incomplete or variably modified type; bitwise
arithmetic on a pointer; an `e` that does not point to an unqualified object
of type `T`; and a value that does not convert to `T`. Apple clang admits the
`_Bool`, floating and function-pointer arithmetic, and its `fetch_add` on
`_Bool` stores 2. Clang's other `__c11_atomic_*` spellings, such as
`fetch_nand`, `fetch_max` and `fetch_min`, are `E2949` on Darwin (10.1.14).
The crossed native tests of `crates/evocc/tests/darwin_stdatomic.rs` update
one object from units built by EvoCC and Apple clang through the header's
generic functions. A passing run of them is not a memory-model proof.

### 10.2 EvoIR to EvoMIR Extraction Contract

Informative. After EqSat optimization and extraction, the chosen EvoIR
representations are lowered to EvoMIR. `spec/evomir.md` holds the contract
of that phase. EvoMIR representations do not enter e-classes in this revision
(EvoMIR Section 1), so the invariant below binds no current code. A revision
that lets them enter must first make it normative and checked:

> **Every EvoMIR representation added to an EClass shares the EvoIR-level
> semantics of that EClass.** The EvoIR-to-EvoMIR lowering agent must not
> add a representation to an EClass unless it faithfully implements the
> abstract semantics of that EClass for all defined executions.

### 10.3 Agent Contract Summary

Informative. This table summarizes rules that other sections state; it adds
none.

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
   forwarded exactly once. A `cond_br`, `asm.goto` or `br.indirect` may place that same current token once
   on each mutually exclusive successor edge; this is control-flow transport,
   not two simultaneous token uses.
8. Executable memory operations use only `mem<all>`; each guard observes
   the current token and advances it to a fresh identity
9. Every load/store consumes a matching `access_in_bounds` output for the same
   pointer and exact target-derived size/alignment, with no intervening memory
   effect. The access alignment is a property of the designated subobject:
   a packed member uses its target member layout, not the natural alignment
   of its scalar type. Validation requires the exact scalar byte width and a
   nonzero power-of-two alignment no greater than its natural alignment;
   ordinary dereferences still carry their type's natural alignment.
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
`mem<R>`/`world` threading without guessing a callee signature. Lowering uses
the conservative `mem<all>` region.

The reference interpreter executes `bits<N>` values for `1 <= N <= 64` and
`bits<128>` (10.1.3), and can forward opaque `mem<R>` and `world` tokens at
block/function boundaries. It advances explicit block and call-frame program counters and
charges one shared step budget for every instruction and terminator across
direct calls and recursion. Module validation resolves every direct callee and
checks the ordinary arguments/results plus the exact trailing effect-token
interface of every call/effect participant, including callers. The
interpreter also executes `ptr<generic>`, globals, automatic object allocation and
lifetime, pointer offset/equality, memory guards, and ordinary/volatile
load/store through the shared object model. It returns immutable memory/world
snapshots and the exact volatile trace; every invalid access reports the typed
diagnostic in Section 6.1. Every other instruction family, including scalar
FP in 3.13, uses that same object/state model; forms outside their
specified boundaries remain typed unsupported and never produce placeholders.

**Hosted `main`.** The C-HIR and EvoIR command-line interpreter entry points
accept `main(void)`, `main(int, char **)` and `main(int, char *[])`. With no
program arguments, `argc` is 1, `argv[0]` is the source path as `evo run`
passes it, and `argv[1]` is a null pointer. Other `main` signatures keep
`E3007` (`unsupported-interpreter-input`, with the
`unsupported-interpreter-function-signature` message).

**Fuel.** An interpreter that exhausts its shared step budget stops with
`E3008` (`interpreter-fuel-exhausted`). That stop is a limit of the host, not a
property of the program: it is neither undefined behaviour nor a comparison
result. A differential trial that hits it on either side is counted on its own
and is never folded into the undefined-behaviour discard count or the
mismatch count.

### 10.5 EvoCPP Target Profile

EvoCPP presents itself to headers as the GNU C dialect of the reference
compiler, GCC 14.2 for `aarch64-unknown-linux-gnu`, so that every `#if` a
header takes is the branch that compiler takes. Nothing here changes what the front end
accepts; it changes only which lines reach the front end.

**Predefined macros.** The predefined set is the list
`crates/evocc-cpp/predefined/aarch64-unknown-linux-gnu.h` (421 macros, the
output of `gcc-14 -std=gnu11 -dM -E - </dev/null`, SHA-256
`74801974f845be4d198bbedc2f20130ddd98645550c05e3453d0294d47bc5a95`), defined
verbatim before any source token, in that order. `__STDC_VERSION__`
is `201112L` as in the list. The list matches `-O0`; the driver options below set the optimization macros. The
driver's `-D`/`-U` apply after the list. The list is the authority;
`crates/evocc-cpp/tests/target_profile.rs` checks its digest. `__CHAR_UNSIGNED__ 1` is in the default list and agrees with the type
profile (plain `char` is unsigned on this target by default). `-fsigned-char`
and `-funsigned-char` (including their inverse spellings) select plain-char
representation in written order, without making its C type identical to
either explicitly signed character type. Ordinary single-byte character
constants and preprocessor character operands use that representation;
signed-char mode removes `__CHAR_UNSIGNED__` before the user's `-D`/`-U` actions.

**The Darwin target.** With `--target=aarch64-apple-darwin` the predefined set
is the list above, then the definitions of
`crates/evocc/src/aarch64-apple-darwin.h`, which follow Apple clang. The C data
layout of this target differs from the Linux one in these facts, which follow
Apple clang:

- Plain `char` is signed by default.
- `va_list` is one pointer: size 8 and alignment 8.
- `long double` stays a distinct C type, for compatibility and `_Generic`,
  with the IEEE binary64 format, size 8 and alignment 8. A `long double` value,
  constant, conversion, HFA member, variadic argument and library argument is
  a binary64 one, EvoIR `f64`; no binary128 operation or helper serves it.
  `__SIZEOF_LONG_DOUBLE__` is 8, and the `__LDBL_*__` macros describe binary64,
  so `<float.h>` agrees. `long double _Complex` has size 16 and alignment 8.
- Narrow integers cross calls as Section 10.1.3 states.
- `wchar_t` and `wint_t` are `int`. A wide character constant and the
  elements of a wide string literal have type `int`. `__WCHAR_TYPE__` is
  `int`, `__WCHAR_MAX__` is `2147483647`, and `__WINT_MAX__` is
  `2147483647`; neither `__WCHAR_MIN__` nor `__WINT_MIN__` is defined. With
  `-fshort-wchar`, `wchar_t` is `unsigned short`, `__WCHAR_MAX__` is `65535`
  and `__WCHAR_UNSIGNED__` is `1`, and `wint_t` stays `int`.

The EvoIR text of this target names it (Section 9.1).

The macOS deployment target is the last `-mmacosx-version-min=V` or
`-mmacos-version-min=V` option, then a nonempty `MACOSX_DEPLOYMENT_TARGET`,
as Apple clang selects them. Without both, EvoCC assumes macOS 11.0, the first
release for arm64; Apple clang infers its default from the SDK and the host
instead, so a comparison names the target for both compilers. `V` has one to
three decimal components, each below 100, and a major version of at least 10;
any other value is a command-line error, "invalid version number". A target
below 11.0 is kept for the compilation, and the platform tools give the
object a minimum of 11.0, as for Apple clang. `__ENVIRONMENT_MAC_OS_X_VERSION_MIN_REQUIRED__`
and `__ENVIRONMENT_OS_VERSION_MIN_REQUIRED__` encode the target as Clang does:
12.3.1 is `120301`, and a version before 10.10 has four digits, so 10.9.5 is
`1095`. The cc command line gives the option to the Darwin assembler and
linker drivers too. For Linux, the development command line refuses the
options, and the cc command line ignores them with a warning, as it ignores
other `-m` options. The availability rules of Section 10.1.9 read the
target.

**Language modes.** The last language-mode option wins, and every macro it
selects is set before the user's `-D`/`-U` actions.

- `-std=gnu11` is the default above. `-std=gnu99` and `-std=gnu17` keep it with
  `__STDC_VERSION__` `199901L` or `201710L`. `-std=c99` and `-std=c11`
  (`199901L`, `201112L`) also define `__STRICT_ANSI__=1` and make bare `asm`
  and `typeof` identifiers.
- `-std=c89`, `-std=c90` and `-ansi` leave `__STDC_VERSION__` undefined, define
  `__STRICT_ANSI__=1` and make bare `inline`, `restrict`, `asm` and `typeof`
  identifiers. `-std=gnu89` also leaves `__STDC_VERSION__` undefined and makes
  only bare `restrict` an identifier.
- GNU89 and the C89 modes use GNU inline semantics: `__GNUC_STDC_INLINE__` is
  removed and `__GNUC_GNU_INLINE__=1` is defined. Combined with a final
  `-fno-gnu89-inline` they are refused at the driver; a later mode option that
  ends the combination is accepted.
- Only GNU89 and the C89 modes admit three legacy forms: an omitted declaration
  type means `int`; undeclared identifier-list parameters are `int`; a call of
  an undeclared name declares a block-scope `extern int name()` with linkage and
  default argument promotions. Undeclared non-call names and incompatible
  redeclarations keep their diagnostics; every other mode rejects an undeclared
  call.
- Before C11 modes, `u`, `U` and `u8` are identifiers, not literal prefixes, in
  every input, command-line replacement and paste check, and macro replacement
  applies to them; `L` stays a wide prefix. GNU89 and C89 refuse a declaration
  in a `for` initializer.
- Reserved spellings such as `__asm__`, `__typeof__` and `__restrict` stay
  available, and the preprocessor treats every keyword as an identifier. C23 is
  not admitted.

**Driver options.** These options take effect before the user's `-D`/`-U`
actions, whatever their position:

- The final optimization option defines `__NO_INLINE__=1` for `-O0`,
  `__OPTIMIZE__=1` for any positive level, and also `__OPTIMIZE_SIZE__=1` for
  `-Os`. These macros change no C definedness and promise no optimization.
- `-pthread` defines `_REENTRANT=1`; repeating it changes nothing, and it adds
  no linker, thread or interpreter behavior.
- `-ffreestanding` sets `__STDC_HOSTED__=0` and disables ordinary-name library
  builtin substitution; `-fhosted` restores both. A later `-fbuiltin` or
  `-fno-builtin` sets substitution alone; `__builtin_*` spellings stay available.
- `-ftrivial-auto-var-init=zero|uninitialized` selects 10.1.6's automatic zero
  initialization. `-fstack-protector-strong`, `-fno-stack-protector`,
  `-fpatchable-function-entry` and `-fmin-function-alignment` select the
  function header fields of Section 5.2. Options that GCC 14 reflects in
  predefined macros (`__SSP_STRONG__`, `__ARM_FEATURE_PAC_DEFAULT`) set them
  with GCC 14's values.
- The last of `-fpic`, `-fPIC`, `-fpie`, `-fPIE` and their `-fno-` forms
  selects position independence. A `-fno-` form selects direct page-relative
  addresses for strong undefined symbols, keeps a nullable GOT address for weak
  ones, and undefines `__PIC__`, `__pic__`, `__PIE__` and `__pie__`. Otherwise
  `__PIC__`/`__pic__` (and, for a PIE option, `__PIE__`/`__pie__`) are 1 for the
  lowercase and 2 for the uppercase spelling. Without such an option, EvoMIR's
  PIE-safe addressing applies.
- `-mcmodel=tiny` uses short PC-relative symbol addressing, nullable GOT
  references included. Requested unwind tables describe the actual frame, saved
  registers and return-address signing state.

**Dynamic macros.** `__FILE__`, `__LINE__`, `__BASE_FILE__`, `__DATE__`,
`__TIME__` as ISO C; `__COUNTER__` expands to `0`, `1`, `2`, ... per expansion
in translation order; a translation unit that expands it
2^32 times is `E0109` (`counter-overflow`).

**Line control.** `#line` macro-expands its operands and sets the presumed
decimal line number of the next physical line, and optionally the presumed
filename, for `__LINE__` and `__FILE__`. Leading zeros are decimal and line
zero is accepted. A number-only form keeps the presumed filename, each included
file has its own line state, and an inactive `#line` has no effect. Physical
source identity, include lookup and token byte offsets do not change. A
malformed line number or filename, including a number above `4294967295`, is
`E2936` (`invalid-line-directive`).

**Token pasting.** The operands of `##` are concatenated and the result is
re-lexed as one *preprocessing* token (C11 6.4, with pp-numbers per 6.4.8): an identifier, a
punctuator, a string or character literal, or a pp-number, which is a digit
followed by any sequence of digits, identifier characters, `.`, and `e`/`E`/`p`/`P`
followed by a sign, and may begin with `.` followed by a digit (`.5foo` and
`1.2.3e+x` are each one pp-number). `1_MODULE`, `1x` and `1.5` are therefore
valid results: `CAT(1,_MODULE)` gives `1_MODULE`. A result that is not one
preprocessing token, such as `CAT(1,+)`, is `E0110` (`invalid-token-paste`)
at the `##` operator. A header-name is never a paste result, and two
non-empty operands never form an *other* token (`C(@,@)` and `C(\\,x)` are
refused); `$`
is an identifier character in this dialect, so a lone `$` is an identifier,
not a stray character. A pp-number that is not a valid integer constant is an error
only where it is consumed: `E0111` (`conditional-operand-not-integer`) in a
`#if`, or the C lexer's diagnostic in the translation unit. GNU comma elision
before an empty `__VA_ARGS__` is honoured. In `, ##__VA_ARGS__`, a named GNU
variadic parameter included, a present argument is substituted without argument
prescan; rescanning may then pass its raw tokens to a nested macro's `#` or `##`
operand.

**Other characters.** Any non-white-space character that does not begin another
preprocessing token (`\\`, `@`, `` ` ``, `$` outside identifiers) is a single
preprocessing token of its own (C11 6.4 *each non-white-space character that
cannot be one of the above*). It passes through macro expansion and is kept by
`#` stringification (`S(\\sreg)` is `"\\sreg"`). It is an error only
when it reaches the C lexer as a token: `E0112` (`stray-token`) at that token,
the same code every preprocessing token that has no C token form reports at
conversion (a pp-number that is not a C constant included).

**`#if` arithmetic.** Every pp-number in a controlling expression, including
one produced by `##` or by macro expansion, is re-lexed as an integer constant:
any of the suffixes `u`, `l`, `ul`, `ll`, `ull` in either case, decimal, octal
and hexadecimal forms. Evaluation is in `intmax_t`/`uintmax_t` with the usual
arithmetic conversions; an identifier that is not a macro is `0`; `defined`
and the feature queries are evaluated before expansion. A `defined` produced
by a macro replacement list is evaluated before that replacement list is
rescanned. A character constant has the value it has in a C expression,
including wide prefixes,
octal and hexadecimal escapes and `\e`; a malformed one is `E1006` before
evaluation and never reads as zero. `#if CAT(300,) > 100` and
`#if CAT(0x1,00) > 100` are true.

**Feature queries.** All five are defined as macros, so `#ifdef __has_attribute`
is true for each of `__has_attribute`, `__has_builtin`, `__has_feature`,
`__has_include` and `__has_include_next`. Their arguments are one token or one
header name; anything else is `E0106` (`feature-query-malformed`).

- `__has_attribute(name)`: `name` with a surrounding `__` pair stripped. The
  answer is the GCC 14 answer, recorded in
  `crates/evocc-cpp/predefined/has-attribute.tsv`: `202311` for `fallthrough` and `noreturn`; `1`
  for `aligned`, `alloc_size`, `always_inline`, `assume_aligned`, `cleanup`, `cold`, `const`, `copy`, `designated_init`, `error`, `externally_visible`, `format`, `gnu_inline`, `malloc`, `no_instrument_function`, `no_profile_instrument_function`, `no_sanitize_coverage`, `no_stack_protector`, `noclone`, `noinline`, `noipa`, `nonnull`, `nonstring`, `packed`, `patchable_function_entry`, `pure`, `retain`, `section`, `uninitialized`, `unused`, `used`, `warn_unused_result`, `warning`, `weak`; `0` for `btf_type_tag`, `diagnose_as_builtin`, `disable_sanitizer_instrumentation`, `ection`, `no_caller_saved_registers`, `overloadable`, `pass_dynamic_object_size`, `pass_object_size`, `preserve_most`, `x`;
  `0` for any name not in that table. The table is data, not code, and
  `crates/evocc-cpp/tests/target_profile.rs` checks its digest.
- `__has_builtin(name)`: `1` exactly for the builtins that the front end
  implements (10.1.14, 10.1.15, 10.1.17 and `__builtin_va_list`), as
  `crates/evocc-cpp/predefined/has-builtin.txt` lists them; `0` otherwise
  (`__builtin_alloca_uninitialized`). The refused `__builtin_puts`,
  `__builtin_exit` and `__builtin__Exit` answer `0`. An AArch64 target
  intrinsic answers `1` when it lowers to a target operation and its
  prototype needs no tuple type of the `arm_neon.h` pragma.
  `crates/evocc-parser/tests/builtin_inventory.rs` checks the list against
  the front end in both directions. On `aarch64-apple-darwin` the answer is
  also 1 for the fourteen `__c11_atomic_*` builtins of 10.1.21 and for the two
  os_log builtins, whose calls are `E2965` (10.1.14).
- `__has_feature(name)`: `name` with a surrounding `__` pair stripped. The
  answer is `0` for every name, as in GCC 14, except on Darwin.
- On `aarch64-apple-darwin`, `__has_attribute` answers `1` for
  `availability`, `unavailable` and `weak_import`, and `__has_feature` answers
  `1` for `attribute_availability`, `attribute_availability_with_message`,
  `attribute_availability_app_extension`,
  `attribute_availability_with_version_underscores`,
  `attribute_availability_tvos`, `attribute_availability_watchos`,
  `attribute_availability_driverkit`, `attribute_availability_with_strict`,
  `attribute_availability_with_replacement` and
  `attribute_availability_swift`, as Apple clang 21 does. Section 10.1.9
  gives each of these forms its meaning; the SDK takes its attribute paths
  only with these answers.
- `__has_include(<h>)` and `__has_include("h")`: `1` when the include search
  the directive would perform finds the file, else `0`.
- `__has_include_next(<h>)` and `__has_include_next("h")`: `1` when the search
  of `#include_next` finds the file, else `0`.

**`#include_next`.** The search starts at the directory after the one that
found the current file. The quoted form does not search the directory of the
current file. A file found beside its includer keeps the position of that
includer, as in Clang; GCC 14 restarts at the first directory in that case. A
file named by `-include` searches from the first directory. In the main file,
`#include_next` searches as `#include` does and `__has_include_next` answers
as `__has_include` does, as in GCC 14 and Clang. Example: on Darwin, Clang's
resource `<limits.h>` includes the SDK `<limits.h>` only when
`__has_include_next(<limits.h>)` holds, and the SDK header defines `PATH_MAX`.
A header that the subframework search below finds has no search position, so
an `#include_next` in it starts at the first directory, as in Clang.

**`#import`.** `#import` searches as `#include` does. It enters the file only
when no directive entered that file before, and every later `#include`,
`#include_next` or `#import` of it is skipped, as GCC 14 and Clang do on both
targets. `#pragma once` and `#import` mark a file the same way.

**Framework headers.** On `aarch64-apple-darwin` a search directory is a
normal directory or a framework directory, as in Apple clang. `-F DIR` names a
framework directory in the position of an `-I` directory, and `-iframework
DIR` names one in the position of an `-isystem` directory; the joined forms
`-FDIR` and `-iframeworkDIR` are the same options. Both are refused on Linux.
A framework directory serves a name `Name/Rest` and no other name:

- The first framework directory of the search list that holds a directory
  `Name.framework` owns `Name`; a later framework directory never serves it,
  even when the owner lacks `Rest`.
- In the owner, `Name.framework/Headers/Rest` is found first, then
  `Name.framework/PrivateHeaders/Rest`; only a regular file is found.
- When the whole list finds no file, the search tries the subframeworks of the
  current file and then of each open includer, innermost first. A file whose
  path has `Outer.framework/` at the first `.framework` of the path offers
  `Outer.framework/Frameworks/Name.framework`, with the same two directories.

`__has_include` and `__has_include_next` use the same search. Without
`-nostdinc`, the search list of this target is the -I and -F directories in
command-line order, then `/usr/local/include` when no `--sysroot` or
`-isysroot` names the SDK, then the -isystem and -iframework directories in
command-line order, then the SDK's `usr/local/include`, Clang's resource
headers, the SDK's `usr/include` and the toolchain's `usr/include`, and last
the SDK's `System/Library/Frameworks` and `System/Library/SubFrameworks`
framework directories; a directory that does not exist is left out. This is
the order of the `-cc1` arguments of Apple clang 21, and `xcrun clang -v -E`
prints the directories of it that exist. The cc command
line also gives each -F and -iframework directory to the linker, and
`-framework Name` links the framework. Example: `#include
<CoreFoundation/CoreFoundation.h>` and `-framework CoreFoundation` compile and
link a CoreFoundation program, and `#include <CarbonCore/CarbonCore.h>` in
`CoreServices.framework/Headers/CoreServices.h` finds the subframework
`CarbonCore`.

**Pragmas.** `_Pragma("text")` destringizes and is processed as `#pragma text`.
`#pragma once` is honoured. `#pragma GCC diagnostic ...`, `#pragma GCC
visibility ...`, `#pragma GCC poison`, `#pragma pack(...)` are accepted:
`diagnostic` is ignored-safe; `visibility push(hidden)` / `push(default)` /
`push(protected)` / `push(internal)`
and `visibility pop` maintain a translation-unit stack, including across
includes and `_Pragma` expansion. The current visibility supplies a default
for externally linked object/function declarations, not for types or automatic
objects; an explicit attribute on that declaration, or an earlier bound
visibility of the same symbol, takes precedence. The default is preserved as
pragma-origin metadata and uses the existing hidden-symbol linkage path:
strong hidden addresses are direct, weak hidden addresses retain the GOT.
Malformed/unsupported values and unmatched `pop` are E2943; so is a conflicting
later explicit visibility. The effective visibility follows the same
hidden/default/protected and internal-to-hidden rules as for
the explicit attribute. `-E` preserves these semantic directives for
re-parsing. `poison` is honoured, and
`pack(N)` sets the maximum member alignment of every record defined while it
is in effect to `N` (a power of two); `pack()` restores the default;
`pack(push[, N])` and `pack(pop)` maintain a stack. Example: `pack(1)` on `{char; int}` gives size 5 alignment 1, `pack(2)` on
`{char; int; long}` gives 14/2, `pack()` restores 8/4. The preprocessor turns
the directive into a layout marker the parser applies as a record-level
`max_align`: member alignment is `min(natural, N)`, and a member `aligned(M)`
with `M > N` does **not** raise it while the pack is active, in all three
attribute placements (`pack(1)` with `int __attribute__((aligned(4)))` gives
size 5 alignment 1). `pack` directives are honoured at file scope and at block
scope; a `pack` directive inside a record's member list is `E0107`, because
the reference compilers disagree on it. `pack(pop)` on an empty stack is
ignored. `pack(show)` and any other
argument form are `E0107` (`pragma-pack-unsupported`). A poisoned identifier
used later is `E0108` (`poisoned-identifier-used`) at the use.

**`push_macro` and `pop_macro`.** `#pragma push_macro("NAME")`
saves the current definition state of the macro `NAME` on a stack that belongs
to that name: the definition, with its parameter list and replacement list, or
the fact that `NAME` is not defined. `#pragma pop_macro("NAME")` removes the
top state of that stack and makes it current, so a name that was pushed while
undefined becomes undefined again, and a name pushed with a definition regains
it, whatever `#define` and `#undef` happened in between. A pop on an empty
stack changes nothing. The stacks are per name, nest,
and are independent of the `pack` stack. The `_Pragma` spelling is the same
directive. Ignoring these two pragmas is not safe, so they are not in the
ignored-safe set. An argument that is not exactly one string literal is
handled as any other unrecognized pragma content: ignored-safe.

`#pragma clang attribute` is not implemented. On Darwin, a `push` or an
attribute list of it is `E0114` (`unsupported-attribute-pragma`) when it
would apply `unavailable`, `weak_import` or an `availability` attribute that
makes a declaration unavailable or a weak import for the deployment target,
because ignoring it would change which declarations Section 10.1.9 refuses or
imports weakly. Other `clang attribute` pragmas, such as the deprecation range
of the SDK's `<dns_util.h>`, are ignored-safe, and on Linux all of them are.

Any other pragma is ignored-safe.

**Whitespace flag and stringization.** Every pp-token carries one flag:
preceded by white space. A token spelled in a file has the flag when white
space, a comment, or a line break inside a macro argument separates it from
the previous token. A token produced by macro expansion keeps the flag its
spelling had in the replacement list or argument, except that the first token
of an expansion takes the flag of the macro name that began the invocation;
when an expansion produces no tokens, the token that follows the invocation
takes the flag if the macro name had it. Stringization (`#`) writes the
spellings of the argument's tokens with exactly one space before each token
whose flag is set, dropping leading and trailing white space. Example: with `A` = `a`, `B` = `b`, `E` empty and
`F(x)` = `x`, `__stringify(A/B)` is `"a/b"`, `__stringify(A / B)` is
`"a / b"`, `__stringify(A E B)` is `"a b"`, `__stringify(A E/B)` is `"a /b"`,
`__stringify(F(A)B)` is `"ab"`, and `__stringify(F( A )/B)` is `"a/b"`. The
same flag decides the spelling of `-E` output; that output is a convenience,
not an acceptance surface, because the parser consumes the token stream.

**The `_Pragma` operator.** `_Pragma ( string-literal )` is processed wherever
it stands after macro expansion, including when the string literal was
produced by stringization inside an expansion: the literal is destringized
(C11 6.10.9) and handled exactly as a `#pragma` directive at that point,
under the pragma rules of this section (`pack` applied, others ignored-safe
and counted). The operand must be exactly one string literal after
expansion; anything else is `E0112` (`stray-token`). A `;` that follows the
operator at file scope is an empty declaration and is accepted.

**Whitespace of a substituted argument.** When an argument is substituted
for a parameter in a replacement list, the first token of the argument takes
the whitespace flag of that occurrence of the parameter, and the remaining
tokens keep the flags of their spelling (with `STR(GCC diagnostic s)`,
`D(push)` is `"GCC diagnostic push"`, `E(x)` with `STR(a s b)` is `"a x b"`,
and `G(name)` with `STR(s.h)` is `"name.h"`).

**One conditional stack.** Conditional directives form one stack whether they
are met at directive level or while the arguments of a function-like macro
are being collected: a branch opened before an invocation may close inside
its arguments and the reverse, and the tokens of the branch not taken never
reach the argument collector (for example
`#ifdef ... pr_warn( #else pr_err( #endif`).

**Empty operands of `##`.** When either operand of `##` is empty, the result
is the other operand unchanged, whether that operand is a parameter or a
token of the replacement list (`mod##SNMP_INC` with `mod` empty is
`SNMP_INC`).

**Comments across continued definition lines.** A block comment inside a
`#define` whose lines continue with `\` is white space, including when the
comment itself spans continued lines.

**`__VA_OPT__`.** `__VA_OPT__ ( tokens )` in a variadic replacement list
expands to `tokens` when the variable arguments contain at least one token
and to nothing otherwise (C23 6.10.4.1; also in `gnu11`).

**Parameter names.** A macro parameter is any identifier, keywords included
(for example `register`); the preprocessor knows no keywords.

**Wide strings under `-fshort-wchar`.** With `-fshort-wchar`, `wchar_t` is
`unsigned short`, `L"..."` has type `unsigned short[N]` counting a terminating
zero, and a wide literal concatenated with a narrow one is wide (`L"" "abc"` is
`unsigned short[4]`, 8 bytes).

**Wide characters.** The target layout fixes wide character values, wide
string element types, decoding, initialization and the `wchar_t` predefined
macros. By default a wide literal holds UTF-32 units, and `wchar_t` is
`unsigned int` on Linux and `int` on Darwin. `-fshort-wchar` selects
`unsigned short` and UTF-16 units on both targets, with a code point above
U+FFFF written as a surrogate pair.

A wide character constant has the value of the target `wchar_t`. On Linux a
hexadecimal escape above the type's range is the unsigned value; under
`-fshort-wchar` GCC 14 truncates it to the low 16 bits. On Darwin a value above
`INT_MAX` sign-extends through the `int` result, and under `-fshort-wchar`
Apple clang refuses every value above `0xffff` with `E1006`. A universal
character name above the short range stays `E1006` on both targets. In `#if`
the constant has the same value, width and signedness as in an ordinary
expression. Plain, `u`, `U` and `u8` character constants keep their existing
rules.

**Multi-character constants.** An unprefixed character constant with two or
more elements has type `int` on both targets when each element is an ASCII
character or an escape sequence with a value of at most 255. Apple's SDK
headers use such constants as four-character codes. The value starts at 0,
and each element in turn shifts it left by eight bits and adds its byte. The
value keeps its low 32 bits as an `int`, and plain char signedness does not
apply. `'ab'` is 24930, `'\xff\xff'` is 65535, `'\xff\xff\xff\xff'` is -1 and
`'abcde'` is 1650680933, as GCC 14 and Apple clang 21 compute them. In `#if`
the constant has the same value. A multi-character constant with a prefix, an
element outside ASCII, a universal character name or an escape above 255
stays `E1006`, because the two compilers read it differently or refuse it.
Fixture: `tests/fixtures/multicharacter-constants/`.

**UTF-8 plain character constants.** A plain character constant whose text is
non-ASCII UTF-8 is the GCC 14 multi-character value of its source bytes:
`'é'` is `0xC3A9`, `'€'` is `0xE282AC`, `'aé'` is `0x61C3A9` and `'😀'` is
`0xF09F9880`. Apple clang refuses every such constant with `E1006`; the
lowered value exists only on Linux. A prefixed constant and a constant with
an escape keep the rules above.

`u'😀'` follows the target: Linux keeps GCC 14's low UTF-16 surrogate
`0xDE00` in a `char16_t`, and Darwin refuses it with `E1006`. GCC prints a
warning for the Linux value; EvoCC does not add a warning.

**Rescanning.** After a macro's replacement list has its arguments
substituted, it is rescanned for further macro names together with the
tokens that follow it in the sequence it was expanded in: the rest of the
source file, the rest of the enclosing replacement list, or the rest of the
enclosing argument. A function-like macro name that ends an expansion is
invoked when the next token of that outer sequence is `(`, across line
breaks. The name of the macro being replaced is not expanded during its own
rescan (C11 6.10.3.4). Example: with
`smp()` = `Z` and `G` = `smp`, `G()` is `Z`; `G` followed by `()` on the next
line is `Z`; `H(1)()` with `H(x)` = `x smp` is `1 Z`; `K(G)()` with `K(x)` =
`x` is `Z`.

**Directives inside macro arguments.** A conditional directive (`#if`,
`#ifdef`, `#ifndef`, `#elif`, `#else`, `#endif`) met while the arguments of a
function-like macro are being collected is processed as a directive, and the
tokens of the branch taken join the argument; `#define` and `#undef` there are
processed as well. `#pragma` inside an argument is `E0113`
(`directive-in-macro-argument`), because the reference compilers disagree on
it. Any other directive there is `E0113` too.

**Empty arguments and variadic forwarding.** An argument may be empty. A
named variadic parameter (`args...`) and `__VA_ARGS__` denote the exact
sequence of the remaining arguments, empty ones and their separating commas
included; when that sequence is substituted into a nested invocation and
rescanned, the nested invocation's arguments are collected from the
substituted tokens exactly as spelled. Example: `E(,)` is `[|]`, `V(,2)` is
`[|2]`, `N()` is `[|]`, and `__MAP(3,__SC_ARGS,,r0,,r1,,r2)` through a
`__MAP` chain yields `, r0 , r1 , r2`.

Diagnostics of this section are front-end (preprocessor) diagnostics: `E0102`
`unmatched-conditional`, `E0104` `error-directive`, `E0105`
`include-not-found`, `E0106` `feature-query-malformed`, `E0107`
`pragma-pack-unsupported`, `E0108` `poisoned-identifier-used`, `E0109`
`counter-overflow`, `E0110` `invalid-token-paste`, `E0111`
`conditional-operand-not-integer`, `E0112` `stray-token`, `E0113`
`directive-in-macro-argument`, `E0114` `unsupported-attribute-pragma`.

---

## 11. Proof Checking Interface

### 11.1 Names

The names of the checker are fixed here, so that no implementation chooses
them:

| Role | Name |
|---|---|
| Cargo package | `evocc-proofcheck` |
| Installed binary | `evo-proofcheck` |
| Argument interface | `evo-proofcheck before.evo after.evo proof.evp` |
| Legacy input (text without the `fenv` field, 4.6) | `evo-proofcheck --legacy-pre-fenv before.evo after.evo proof.evp` |
| Capability probe | `evo-proofcheck --probe` |
| Packaged copy (package `evocc`) | `evocc-checker`: the same program, with the same interface and exit statuses |

The package `evocc-proofcheck` also installs `evo-facts-vectors`, a
correspondence runner. It is not a checker interface.

The compiler driver does **not** answer a `proofcheck` capability probe of its
own. A driver probe would suggest the driver can discharge the check, and the
value of the check is that it runs somewhere the driver's code cannot reach.
`evo-proofcheck --probe` exits zero with empty stdout and stderr, matching the
repository's exact-capability convention. The driver's broad `proof --probe`,
`eqsat --probe`, `-Oeq --probe`, and unrelated broad probes remain exit 69 with
`NOT_IMPLEMENTED`.

### 11.2 Interface

The checker reads three files and takes nothing else from its environment
except the optional leading `--legacy-pre-fenv` flag. It uses the one target
authority in Section 9.1; no target flag or host default can change the
meaning of these bytes:

```text
before.evo    the canonical rendering of the module as it entered optimization
after.evo     the canonical rendering of the module as it left
proof.evp     the certificate
```

It performs, in order:

1. Read the target profile from `before.evo` under Section 9.1, then parse
   both files with that target's reader (with `--legacy-pre-fenv`, use the
   Linux-only legacy reader of Section 4.6). Reject either file under
   Section 9.5 rather than repairing it or changing its target.
2. Verify the certificate's input digest against `before.evo`'s bytes.
3. Recompute the ordered eligible regions and their witness and seed manifests
   from parsed `before.evo`, per Sections 8.5 through 8.7, using its own
   implementation. Verify that the certificate names a duplicate-free
   subsequence and that every named manifest is exact; an omitted eligible
   region is a permitted decision not to optimize it. A reader may derive only
   the named roots, in module order, if it independently checks eligibility and
   the complete closure for each. An identity certificate needs no region or
   seed-fact analysis. Input size must not become recursive traversal depth;
   a manifest count cannot allocate more entries than its input can contain.
4. Replay every merge against the trusted registry of Section 11.6, recomputing
   constant folds rather than accepting claimed results.
5. Verify the structural rules of the extraction map (Section 11.3:
   topological order, one entry per final class, the closure from `root` equal
   to the entries, the extracted root not the original root node) and that its
   root is the final class of the original root node. The checker does not
   judge cost (Section 11.3).
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

It must nonetheless carry every fact a replay needs: without node identity,
owner class, children, and the substitution a rule was applied under, there is
nothing to re-derive.

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
                 | "divisor_nonzero" | "signed_div_rem_no_overflow"
                 | "signed_add_defined" | "signed_sub_defined" | "signed_mul_defined"
                 | "shift_count_defined.signed" | "shift_count_defined.unsigned"
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

Node and e-class identifiers in a certificate are certificate-local and restart in every
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
exactly as for a named constant.

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

The checker does not judge cost. It verifies that every replacement is equivalent by
the recorded events, that the extracted root is not the original root node
itself (a region must change something), and every structural rule of this
section. Whether the replacement is an improvement is decided by the
producer's policy (Section 8, "Cost is a policy") and is producer conformance the
checker cannot observe, like the ceiling-stop rule of 8.11. The default
policy's cost, for the record: a seed, a `const` node, or a non-root
before-witness reuse costs zero; the replacement root's non-constant before
node and every non-constant derived node cost one plus the sum of selected
child costs, repeated child positions counted repeatedly; constants cost
zero so that `icmp.eq x, x` and its `true` replacement do not tie. A producer
admits a region block only when the extracted root has strictly lower cost
than the original witness root under its own policy, ties retaining the
original; that is producer conformance, not a checker rule.

**Versions.** The producer writes `evp2`, and `evo-proofcheck` reads `evp1`
and `evp2`. `evp1` is the earlier format and remains readable: it carries no budget line, no epoch markers and no premises, so a checker
validates it without a scheduler claim and admits only rules that need no
premise. `evp2` adds the three. A reader that does not implement a version
rejects it as malformed rather than guessing. A Tier B rewrite requires `evp2`,
because a fact-driven rewrite without its premise is not a claim a checker can
verify. Note (informative): the Lean reader `lean/EvoIR/Rewrite/Certificate.lean`
is partial. It reads `evp2` only, ignores the budget line, epoch markers and
digests, and admits only `input` seeds and `bits<N>` sorts.

**Premises.** Each premise names a class and the known-bits fact the rule relied
on. Mask and value have the same digit count, which is the class's width in
bits rounded up to a whole hexadecimal digit; a premise whose two fields differ
in length, or whose value has a one where the mask has a zero, is malformed. The checker recomputes the fact from the region's nodes with its own
implementation of the Section 8.10 transfers and refuses the rewrite unless
the recomputed fact is at or below the recorded premise in the order of 8.10,
that is, unless every bit the premise claims is known to the checker with the
same value; a premise claiming a bit the checker cannot confirm is refused
(status 3). Facts are recomputed
rather than trusted for the reason Section 11.6 gives: a shared analysis that
was wrong would make producer and checker agree on an unsound rewrite.

**Epoch markers.** The scheduler runs per transaction and a certificate is
organised per region, so markers are written per region: within a region block,
an `epoch k` line precedes the events that epoch produced for that region, and
`k` is strictly increasing within the block. A region touched in no epoch has no
markers. The checker verifies monotonicity within each block and that no `k`
exceeds the recorded epoch count; it does not require every epoch to appear in
every region, since most epochs touch few regions. Epochs are numbered from 1: with `epochs=c/1024` the legal markers are `1..=c`, so the inequality `k <= c` is tight, and a marker `0` is malformed (status 2). A congruence that holds as its two nodes are inserted was produced by no epoch and precedes every marker, like the node inserts; a congruence a merge enabled follows the marker of the epoch whose merge enabled it. A rewrite whose union would join a class with itself, because an earlier merge already placed the root where the rewrite would, is not recorded; the replacement it would have justified stands on the recorded merges alone, which the checker verifies as it verifies any replacement.

**Budget.** Every `quantity` is written consumed `/` bound, in that order:
`budget nodes=3/100000 epochs=1/1024 fuel=0/256 n=0 stop=saturated` is a
transaction whose regions record three nodes of a bound of one hundred
thousand, whose highest epoch marker is 1 of a possible 1024, and which
stopped because nothing new could be proposed. The `nodes` and `epochs`
bounds are the profile's constants (8.11), and the checker verifies that the
recorded bounds equal them. `fuel` is the per-rule, per-epoch quantity of 8.11: `n` is the number
of nodes the region held when the transaction began, the fuel bound is
`max(256, 8 * n)`, and the fuel consumed is the largest number of firings any
single rule made in any single epoch of one region. The checker verifies
that each consumed does not exceed its bound, that the fuel bound equals `max(256, 8 * n)` for
the recorded `n`, that no epoch marker exceeds the consumed epoch count, and
that a stop reason of `epochs`, `nodes` or `fuel` coincides with that
quantity's consumed equalling its bound. `n` itself is declared, not
re-derived: a producer that overstates it only widens a box the checker
still holds it to. `saturated` and `no-new-best` are not verified; they would
require re-running the scheduler and are telemetry. A producer that stops by a rule of its own, neither at a profile ceiling nor at saturation, records `stop=policy`; it is telemetry like `saturated` and `no-new-best`. `saturated` has exactly the meaning of 8.11: the producer had no further rewrite to propose. It says nothing about a fresh transaction over the output, which may admit regions because 11.6 lets a left-hand `const_like` match only a constant present in the before witness closure, never one an earlier epoch derived; such *residual regions* are a quality metric of the registry, not a false record. For a certificate covering several regions, `n` is the largest starting node count among them. With P0 regions the fuel bound's floor of 256 is not reachable, so `stop=fuel` is reserved for a profile with larger regions; the grammar keeps it. Hitting any ceiling keeps every region admitted before the stop (8.11): a producer that answers its own budget exhaustion with a zero-region certificate is non-conforming, because it discards work the checker would have verified. An `evp2` header without a budget line, or an `evp1` header with one or with epoch markers, is malformed (status 2).

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
rendering.

**Bindings are recomputed from the recorded children.** The substitution a
rewrite line records must equal the one the checker derives from the
operation and children of the node the rewrite names, after the merges
recorded before it; a producer therefore computes each binding from the
node stream it emitted, never from an operand remapped by a later
replacement in the same witness closure. A class equal in content to the
child's class but never unified with it is not that child's class.

### 11.4 Exit Statuses

```text
0    valid (also the --probe answer)
2    malformed input or invocation -- wrong argument count, an unreadable file,
                            or bytes that are not a module or not a certificate
3    well-formed but invalid -- it parses and the claim does not hold
```

Status 3 also covers a failed self-check of the checker's own seed-fact table
(Section 8.10): no premise is confirmed against a table that fails it.

The two failure statuses are distinct because they are acted on differently: a
malformed file is a producer or transport defect, while a well-formed invalid
one is a claim that was checked and refused. Collapsing them would hide the
second inside the first during a campaign, which is precisely when the
difference matters. Any other status is itself a defect; the checker does not
exit non-zero for reasons outside this table.

### 11.5 Dependency Boundary

This is a normative constraint, not a convention. The Cargo test
`crates/evocc-proofcheck/tests/normal_dependencies.rs` reads Cargo's normal-edge
dependency closure with all features and targets. Dev-only dependencies do not
enter this closure.

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

**Shared, and not fail-safe: the renderer, the validator, `inst_results` and
the certificate codec.** The checker renders its reassembled module with the
Section 9 renderer `render_module`, validates it with `validate_module`,
enumerates instruction results with `evocc_evoir::inst_results`, and reads
the certificate with `evocc_proof::parse_evp1`, whose writer `render_evp1`
the producer uses. Its reassembly renumbers operands with `all_values_mut`, a
copy of the producer's function. On the ordinary `-Oeq-scalar` path the
backend compiles the producer's in-memory module, and the checker sees only
its rendering: a field that the renderer drops is absent from both texts, so
the checker can accept a region whose eligibility depends on that field. A
defect in any of these parts is common to both sides and can make the
checker accept; Section 11.7 lists them in the trusted base.

**Not shared: eligibility and seed derivation.** A common defect here makes both
sides agree that an ineligible region was eligible, and the certificate is
*accepted*. Sharing fails dangerous. The checker implements Sections 8.5 through
8.7 for itself, but it enumerates results through the shared `inst_results`
(above).

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
the fact-driven folds for the shifts and width changes of Section 8.5. Its digest is `aca1f15b47e602ce18c593e06d77008862947c7c7252a9f76bb823c71f53fb15`, computed as for the earlier registries:

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

**Reviewed registry `g0.ub.v1`.** This separate guard-only registry is normative
for `evp2`. Its digest is
`c49a0caac284fe276cf1605b312f45a8ddb5378b61dc0aaf352e03a41d21cc84`,
covering the following exact bytes through the final newline. It does not
include any P0 rule; P0 witness operations use their existing transfer rows.
Each rule reads one exact-true result-class fact through `fully_known`.
False/unknown results are not folds. Sorts, independent N/M, operand order
and source signedness are fixed by the left pattern, not certificate claims.

```text
g0_001_nonzero divisor_nonzero(?a) => known_const when fully_known
g0_002_div_rem signed_div_rem_no_overflow(?a,?b) => known_const when fully_known
g0_003_add signed_add_defined(?a,?b) => known_const when fully_known
g0_004_sub signed_sub_defined(?a,?b) => known_const when fully_known
g0_005_mul signed_mul_defined(?a,?b) => known_const when fully_known
g0_006_unsigned_count shift_count_defined.unsigned(?a,?k) => known_const when fully_known
g0_007_signed_count shift_count_defined.signed(?a,?k) => known_const when fully_known
```

**Registries and revisions.** A registry is admissible only when this
specification prints its rule block and digest; a producer and a checker may
not agree on a registry between themselves, because a certificate checked
against rules it supplied establishes only that its merges follow from its own
claims. A revision includes every rule of the registry it revises, unchanged
and in the same order, before its new rules, so a rule id keeps its meaning
across revisions. `evp1` accepts `p0.scalar.v1` only; `evp2` accepts
`p0.scalar.v1` through `p0.scalar.v4`, or the independent `g0.ub.v1` under its
own eligibility profile (Section 8.12).

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
the facts of seeds (8.10) make possible for a block argument; a coverage claim
counts them as reached when such an input exists. Premise classes are resolved through the
partition at the rewrite that consumes them, like binding classes. A premise
on a rule whose side condition reads no class is invalid (status 3), and so is
a premise whose digit count disagrees with its class's width.

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

No EVP version accepts a registry name or digest other than those this section
prints. The producer and checker each
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
policy's decision (Section 8, "Cost is a policy").

`fold.add`, `fold.sub`, and `fold.mul` compute modulo `2^N`; the bitwise folds
operate on the exact `N` raw bits. Equality and unsigned comparisons use those
raw values. Signed comparisons decode each raw operand as an `N`-bit two's
complement integer. Comparison builders return `bits<1>`. The checker performs
this calculation itself and compares the resulting derived node's sort and
`value` attribute; the certificate never supplies a trusted fold result.

**Residual risk, recorded rather than implied.** The certificate format
definition and its reader and writer (`crates/evocc-proof/src/evp1.rs`) remain
shared between producer and checker, so a defect in that definition is
invisible to both; the partial Lean reader of Section 11.3 does not remove
this risk. The property that would bound it is that **no mutant of the
optimizer produces a semantically different module together with an accepted
certificate** — not that every mutant is rejected, since a mutant that merely
disables a rewrite emits a valid empty certificate and should pass. No test
measures that property. The campaign that exists,
`crates/evocc-proofcheck/tests/day8_proof_contract.rs`, runs the real producer
once and then edits its output by hand: five semantic corruptions (an
after-module change with a recomputed output digest, a wrong rule name, a
wrong fold payload, a wrong binding and a hidden witness) each exit 3, and a
certificate with no region, which stands for a mutant that only disables
rewrites, exits 0. The after-module forgeries of
`crates/evocc-proofcheck/tests/copy_is_opaque.rs` are of the same kind. No
test mutates the optimizer's code, the shared certificate reader and writer,
or the shared parts of `evocc-evoir` (Section 11.7, A4).

**Proof coverage.** A machine-checked proof of a registry
covers *instances*: a rule together with every type parameter the registry
admits for it (the result width, the operand widths of a comparison, the two
widths of a shift, the two widths of a width change) and its side condition.
Coverage is stated in two classes, because they fail independently: the
*rule* class, where a registry counts as proved only when every instance the
registry text admits has a proof; and the *analysis* class, where every
Section 8.10 transfer, the merge, the join and the side-condition predicates
that produce or consume a premise are proved over the parameter range the
registry's rules can reach, including a shift amount of any width. The only
place a coverage constraint may live is the registry text itself: a registry
whose proof coverage is incomplete stays usable by producer and checker
exactly as before and its receipt says which instances are unproved; a
configuration that requires proof either completes the instance space or
publishes a new registry whose side conditions exclude the unproved
instances, under a new digest that both sides interpret identically. No
enabling condition is ever implied under an old digest, and an independent
proof gate may fail without failing any build. The receipt is
`tests/contracts/lean-p0-receipt.json`; the plan is `docs/archive/lean-plan.md`.
The receipt binds the revision line and the SHA-256 of this document, and
`python3 tools/render-status.py --check` fails when either differs from the
pin of this document in `spec/pins.json`.

### 11.7 Trusted Base

What a certificate establishes rests on the following, each stated so that a
reader can see which are checked by a machine and which are believed. The
Lean receipt `tests/contracts/lean-p0-receipt.json` does not use these
numbers: its `trust` field lists the assumptions of the Lean layer in its own
words.

| Item | Trusted object | Guarantor |
|---|---|---|
| A1 | Section 8.5 eligibility and taint: every value the e-graph touches is total, poison-free and effect-free | The producer's and the checker's separate Rust implementations; both enumerate results through the shared `inst_results` (A4) |
| A2 | The function-level seed-fact fixed point of Section 8.10: entry arguments top, every edge including back edges | The checker's own recomputation; an inductive-invariant check of the final table, where present, adds independent evidence |
| A3 | The checker's replay, extraction and reassembly, including the association of facts with the right class in the pre-merge state | Rust, Section 11.2; the reassembly renumbers operands with `all_values_mut`, a copy of the producer's function |
| A4 | The code that producer and checker share (Section 11.6): the Section 9 parser and renderer, `validate_module` and `inst_results` of `evocc-evoir`, and the certificate reader and writer of `evocc-proof` | Rust; the field sentinel and the traversal test of `crates/evocc-evoir/tests/section9_completeness.rs` (10 of the 59 instruction variants) are evidence, not proof |
| A5 | The Lean kernel, and on SAT-backed proofs the per-theorem native reflection axioms and the compiler behind them | Lean toolchain pinned in `lean/lean-toolchain` |
| A6 | The registry decoder, the Lean semantics definitions and the audit script | Reviewed code; round-trip comparison of decoded data against the digest-bound text |
| A7 | Agreement between the Rust semantics (interpreter, folds, transfers) and the Lean model | Differential vectors only; evidence, not proof |
| A8 | On the ordinary `-Oeq-scalar` path, the completeness of the renderer (Section 9.1, "Total"): the backend compiles the in-memory module, and the checker sees only its rendering | The field sentinel of `section9_completeness.rs`; evidence, not proof |

A proof of the registry replaces two things that were previously believed on
review alone: that each registry rule is a valid equality of the Section 3
semantics, and that the Section 8.10 transfers, merge, join and predicates are
sound abstractions of it. It replaces nothing else in this table. The claim a
proved registry licenses is therefore: "the listed rules and analysis rows are
machine-checked against the Lean model of the EvoIR scalar semantics, within
the stated coverage, with the stated axioms"; it is not a verified compiler.

## Appendix A: Instruction Quick Reference

Section 3 defines each instruction and Section 9.3 gives its exact spelling.

| Category | Instruction | Result | Operands | May Trap |
|---|---|---|---|---|
| Constants | `const.bits<N>` | `bits<N>` | integer literal | no |
| Arithmetic | `add.bits<N>`, `sub.bits<N>`, `mul.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | no |
| | `sdiv.bits<N>`, `udiv.bits<N>`, `srem.bits<N>`, `urem.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | yes |
| Predicates | `divisor_nonzero` | `bits<1>` | `%divisor` : `bits<N>` | no |
| | `signed_add_defined`, `signed_sub_defined`, `signed_mul_defined` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| | `signed_div_rem_no_overflow` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| | `shift_count_defined.signed` / `.unsigned` | `bits<1>` | `%shifted` : `bits<N>`, `%amount` : `bits<M>` | no |
| | `signed_shl_defined.signed` / `.unsigned` | `bits<1>` | `%shifted` : `bits<N>`, `%amount` : `bits<M>` | no |
| Obligation | `ub.assume.<reason> %cond` | — | `%cond` : `bits<1>` | yes |
| Bitwise | `and.bits<N>`, `or.bits<N>`, `xor.bits<N>` | `bits<N>` | `%a`, `%b` : `bits<N>` | no |
| | `shl.bits<N>`, `lshr.bits<N>`, `ashr.bits<N>` | `bits<N>` | `%a` : `bits<N>`, `%b` : `bits<M>` | no |
| | `ctlz.bits<N>`, `cttz.bits<N>`, `ctpop.bits<N>`, `bswap.bits<N>` | `bits<N>` | `%a` : `bits<N>` | no |
| Comparison | `icmp.eq`, `icmp.ne` | `bits<1>` | `%a`, `%b` : same `bits<N>` or same pointer type | no |
| | `icmp.slt`, `icmp.sle`, `icmp.ult`, `icmp.ule` | `bits<1>` | `%a`, `%b` : `bits<N>` | no |
| Extension | `sext.bits<N>`, `zext.bits<N>` | `bits<N>` | `%a` : `bits<W>` (W <= N) | no |
| | `trunc.bits<N>` | `bits<N>` | `%a` : `bits<W>` (W >= N) | no |
| Floating | `const.f16`, `const.bf16`, `const.f32`, `const.f64`, `const.f128` | `f16`, `bf16`, `f32`, `f64`, `f128` | 4, 4, 8, 16 or 32 hexadecimal digits | no |
| | `fp.<op> ... : T` | `T` or `bits<1>` | per operation (3.13); a half format admits only conversions and representation moves (3.13.6) | no |
| | `fp.strict.<op> %w, ... : T` | result, `world` | world token, operands (3.13.1) | no; `fenv` effect |
| Pointer | `ptr.null` | `ptr<AS>` or `fnptr<Sig>` | — | no |
| | `global_addr @symbol` | `ptr<generic>` | global symbol | no |
| | `func_addr @symbol` | `fnptr<Sig>` | function symbol | no |
| | `tls_addr @symbol` | `ptr<generic>`, `mem<all>`, `world` | TLS symbol, `%m`, `%w` | yes |
| | `ptr.offset.<direction>.<signedness>` | `ptr<AS>` | `%base` : `ptr<AS>`, `%index` : `bits<N>`, positive scale | yes |
| | `ptr.to_int %p` | `bits<64>` | pointer | no |
| | `ptr.from_int.<type> %i` | `ptr<AS>` or `fnptr<Sig>` | `bits<64>` | on use (`E3013`, `E3016`) |
| | `block_addr ^b` | `ptr<generic>` | block label | on dereference (`E3013`) |
| | `return_address`, `frame_address`, `frame_address depth=N` | `ptr<generic>` | none; N in 1..65535 | on dereference (`E3013`); interpreters `E3031` for N > 0 |
| Memory | `alloca` | `ptr<generic>`, `mem<all>`, `world` | `%m`, `%w`, origin, size, align | yes |
| | `alloca.dynamic` | `ptr<generic>`, `mem<all>`, `world` | `%m`, `%w`, `%size` : `bits<64>`, origin, align | yes |
| | `lifetime.end` | `mem<all>`, `world` | `%m`, `%w`, `%ptr` | yes |
| | `pointer_valid` | `mem<all>` | `%m`, `%ptr` | yes |
| | `access_in_bounds` | `mem<all>` | `%m`, `%ptr`, size, align | yes |
| | `load [volatile]` | Type, `mem<all>`, `world` | `%m`, `%w`, `%ptr` | yes |
| | `store [volatile]` | `mem<all>`, `world` | `%m`, `%w`, `%ptr`, `%val` : Type | yes |
| | `load ... [mask=M]` | `bits<N>`, `mem<all>`, `world` | `%m`, `%w`, `%ptr`, mask | yes; `E3001` only for a selected bit |
| | `store ... [mask=M]` | `mem<all>`, `world` | `%m`, `%w`, `%ptr`, `%val` : `bits<N>`, mask | yes |
| | `copy`, `copy.volatile.read`, `copy.volatile.write`, `copy.volatile.readwrite` | `mem<all>`, `world` | `%m`, `%w`, `%dst`, `%src`, size, align | yes; `E3027` for a partial overlap |
| | `atomic.load order [volatile]` | Type, `mem<all>`, `world` | `%m`, `%w`, `%ptr` | yes |
| | `atomic.store order [volatile]` | `mem<all>`, `world` | `%m`, `%w`, `%ptr`, `%val` | yes |
| | `atomic.rmw.<op> order [volatile]` | old value, `mem<all>`, `world` | `%m`, `%w`, `%ptr`, `%val` [, `%expected` for `cmpxchg`] | yes |
| | `atomic.copy.<op> order [volatile]` | [`bits<1>` for `cmpxchg`,] `mem<all>`, `world` | `%m`, `%w`, `%ptr`, buffers, size, align | yes; `E3001` for an uninitialized compared byte |
| Vectors | `vec.pack` | `vec<N, T>`, `T` = `bits<B>`, `f32` or `f64` | N lane values of `T` | no |
| | `vec.extract` | the lane type `T` | vector, full-width integer index | yes; `E3011` for an invalid index |
| | `vec.bitcast` | same-size integer, floating or vector representation | source value | no |
| | `vec.aese`, `vec.aesd` / `vec.aesmc`, `vec.aesimc` | `vec<16, bits<8>>` | data and key / one value | no |
| Calls | `call @callee(args..., %m, %w)` | ret values, `mem`, `world` | callee, args, memory, world | depends |
| | `call.returns_twice @callee(args..., %m, %w)` | ret values, `mem`, `world` | callee, args, memory, world | depends; top effects (3.10) |
| | `call %fptr(args..., %m, %w)` | ret values, `mem`, `world` | callee value, args, memory, world | depends |
| Varargs | `va_start %ap, %m, %w` | `mem<all>`, `world` | list object, memory, world | `E3037` at validation |
| | `va_arg.Type %ap, %m, %w` | Type, `mem<all>`, `world` | list object, memory, world | yes; `E3038` |
| | `va_arg.byval %ap, %m, %w, %dst` | `mem<all>`, `world` | list object, memory, world, destination, size, align | yes; `E3038` |
| | `va_end %ap, %m, %w` | `mem<all>`, `world` | list object, memory, world | yes; `E3038` |
| Assembly | `asm [volatile]` | register outputs, `mem<all>`, `world` | template, constrained operands, clobbers, input tokens | yes; interpreter `E3021` |
| | `asm.goto [volatile]` (terminator) | register outputs and tokens, bound on successor edges | same payload, named label edges, fallthrough edge | yes; interpreter `E3021` |
| Control | `br ^target(args...)` | — | block label, values | n/a |
| | `cond_br %c, ^t(targs), ^e(eargs)` | — | condition, blocks, values | n/a |
| | `br.indirect %p [^b(args), ...]` | — | code pointer, complete ordered edges and values | yes; `E3048` for an invalid target |
| | `ret values...` | — | return values | n/a |
| | `trap.<reason>` | — | `scalar_function_fell_off_end`, `unreachable_reached`, `builtin_trap` | yes |
| Poison | `uninitialized.bits<N>` | `bits<N>` | — | on consumption (`E3001`) |
| | `uninitialized.ptr<AS>`, `uninitialized.fnptr<Sig>` | that type | — | on dereference (`E3013`) |

---

## Appendix B: Type Grammar (Formal)

```
Type       := "bits" "<" Integer ">"
            | "f16" | "bf16" | "f32" | "f64" | "f128"
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

Section 9.3 adds the variadic marker `...` and the `byval`/`sret` record marks
to the signature spelling.

---

## Revision

**Revision 2.32.0, 2026-10-24.** Admit the Darwin NEON header subset: opaque
FP8 storage with a refused by-value interface; 64/128-bit NEON vector
attributes; typed declarations for unused Apple header bodies; equal-image
bit casts and four generic integer load/store/zip/unzip forms. State the
bounded Darwin target-feature attribute rule and its general-registers-only
exception. Other builtin operations and nominal half-vector execution remain
outside this subset.

**Revision 2.31.4, 2026-10-24.** The diagnostic authority gives `E3007` one
meaning, `unsupported-interpreter-input`: an interpreter or the execution entry
cannot execute the input. It is a refusal, not a finding. The
`scalar_function_fell_off_end` trap reason stays for hand-written modules.

**Revision 2.31.3, 2026-10-24.** The shift lowering may give
`shift_count_defined` the canonical `const.bits<1> 1` result when the amount
is a compile-time constant in `[0, N)`, so that checked MIR omits the branch
under 8.12/9.4; a constant amount outside the range and a nonconstant amount
keep the ordinary guard.

**Revision 2.31.2, 2026-10-24.** The complex paragraph records that the Darwin
SDK defines `CMPLX`, `CMPLXF` and `CMPLXL` only for `__clang__`, which EvoCC
does not define, so those spellings stay undeclared on Darwin and
`__builtin_complex` is the replacement.

**Revision 2.31.1, 2026-10-23.** The interpreter paragraph lists the hosted
`main` signatures and the no-argument `argc`/`argv` values shared by both
interpreters and `evo run`.

**Revision 2.31.0, 2026-10-23.** Section 6.3 adds the `cpp` lint for
`#warning` and its option levels, and the diagnostic table adds `W0010` and
the pedantic driver refusal `E2968` for `-pedantic-errors`,
`--pedantic-errors` and `-Werror=pedantic`. It also states that
`-Wno-error=NAME` does not enable a lint and that `-Werror` promotes only
EvoCC's own lints.

**Revision 2.30.0, 2026-10-23.** Section 6.3 gives pointer-sign,
pointer-arith, discarded-qualifiers and discarded-array-qualifiers their four
new lints, codes and default levels. The checks cover assignment,
initialization, argument passing, return, pointer arithmetic and pointer
subscripts as measured against GCC 14.

**Revision 2.29.2, 2026-10-23.** The vector comparison paragraph gives 8-bit
character lanes `signed char` masks on Linux and `char` masks on Darwin, and
gives every 64-bit lane type the target's 64-bit mask type, including integer
lanes. Both mask rules are independent of `-fsigned-char` and
`-funsigned-char`. The representation and ABI are unchanged.

**Revision 2.29.1, 2026-10-23.** Section 10.1.9 refuses type-, value-,
layout-, linkage- and ABI-changing Clang attributes on Darwin with `E2967`,
including the measured vector, counted/sized-family, `ms_struct` and
external-linkage `internal_linkage` names; `internal_linkage` on a `static`
declaration is accepted. It accepts diagnostic-only attributes and pointer
nullability keywords without effect, refuses a second nullability keyword, and
refuses `#pragma ms_struct on`. Linux keeps GCC 14's warning-and-ignore
behavior for unknown attributes; EvoCC ignores them without the warning.
`__has_attribute` answers 0 for the refused group.

**Revision 2.29.0, 2026-10-23.** The integer-complex paragraph admits `++`
and `--` on integer complex values and GNU imaginary integer constants with
the letter anywhere in the integer suffix. It states GCC's suffix groups,
rejects the mixed-case `lL` spelling, and refuses every imaginary integer
constant in `#if`. Both targets take the values and types of GCC 14 and Apple
clang; signed component overflow keeps its `E3004` obligation.

**Revision 2.28.4, 2026-10-22.** Section 10.5 evaluates a `defined` produced
by a macro replacement list before that list is rescanned, and refuses a
`defined` whose operand is not an identifier. A plain UTF-8 character
constant, including the mixed `'aé'`, has GCC 14's multi-character value on
Linux; Apple clang refuses it. `u'😀'` follows the target: GCC 14's low
surrogate `0xDE00` on Linux, `E1006` on Darwin.

**Revision 2.28.3, 2026-10-22.** Section 10.5 gives a wide character constant
the value, width and signedness of the target `wchar_t` in `#if` and in
ordinary expressions. Linux accepts the unsigned value and, under
`-fshort-wchar`, truncates an out-of-range hexadecimal escape as GCC 14 does;
a universal character name above that range stays `E1006`. Darwin sign-extends
through `int` and refuses an out-of-range value under `-fshort-wchar`.

**Revision 2.28.2, 2026-10-22.** Section 10.1.11 copies an aggregate
statement-expression value into a fresh temporary before the objects declared
inside end. The copy precedes their `lifetime.end` sequence, and the temporary
lives at least until the end of the enclosing full expression.

**Revision 2.28.1, 2026-10-22.** Section 10.1.8 requires a scalar controlling
expression and adds `E2103` (`controlling-expression-not-scalar`) at the
operand for a record, union, vector or void operand. A vector operand of `!`,
`&&` or `||` keeps `E2102`, and a vector condition of `?:` keeps `E2202`.

**Revision 2.28.0, 2026-10-21.** Section 10.5 admits the multi-character
constants of GCC 14 and Apple clang 21 on both targets: an unprefixed
constant of two or more ASCII characters or escapes of at most 255 is an
`int`, in C expressions and in `#if`. The other multi-character forms stay
`E1006`. No grammar production, registry or Lean rule domain changes.

**Revision 2.27.0, 2026-10-21.** Sections 10.1.9 and 10.1.14 refuse the Clang
extensions of Apple's SDK by name on Darwin, at their first use: Blocks are the
new `E2963`, `overloadable` the new `E2964`, the os_log builtins the new
`E2965`, and the ACLE builtins and intrinsics of `<arm_acle.h>` the new
`E2966`. `__has_builtin` answers 1 for the two os_log builtins, and the
target predefines `__ARM_ACLE`. On Darwin, a list attribute after the `*` of a
function's result type belongs to the function, and the definition of an
internal function with one is refused only when the unit keeps it. Linux does
not change. No grammar production, registry or Lean rule domain changes.

**Revision 2.26.0, 2026-10-21.** Section 10.1.9 gives Clang's `availability`,
`unavailable` and `weak_import` attributes the meaning of Apple clang on
Darwin: a use of a declaration that is unavailable for the macOS deployment
target is the new `E2962` outside an unavailable context, and a declaration
introduced after the target is a weak import. Section 10.5 selects the
deployment target from `-mmacosx-version-min`, `-mmacos-version-min` and
`MACOSX_DEPLOYMENT_TARGET`, assumes 11.0 without them, and refuses an invalid
version. Darwin's `__has_attribute` and `__has_feature` answer the
availability queries as Apple clang does, and a `#pragma clang attribute`
that would apply such an attribute with an effect is the new `E0114`. Linux
keeps the attributes without effect. No grammar production, registry or Lean
rule domain changes.

**Revision 2.25.0, 2026-10-21.** Section 10.5 adds the framework headers of
Darwin: `-F` and `-iframework` directories, the SDK's framework and
subframework directories, the ownership of a framework name by its first
framework directory, `Headers` before `PrivateHeaders`, and the subframework
search from the frameworks of the open files. It states the whole Darwin
search order of Apple clang 21, which puts `/usr/local/include` before the
-isystem directories and adds the toolchain's `usr/include`. `#import` is
admitted on both targets with the meaning of GCC 14 and Clang. The cc command
line gives the framework directories to the linker. Linux search lists do not
change. No grammar production, registry or Lean rule domain changes.

**Revision 2.24.0, 2026-10-21.** Section 3.8 admits the mask on a volatile
`load` or `store`, which stays one volatile event; its event records the
initializedness of each byte when a byte is not completely initialized.
Section 10.1.6 gives a volatile bit-field the rule of D23: the unit read of a
store demands no bit, the store initializes only the field's bits, and a read
demands only the field's bits, in the lowering and in both interpreters. A
store to a volatile bit-field with an uninitialized neighbour is no longer
`E3001`; a read of a member that was never stored still is. Native code does
not change. The Section 9 grammar already spells the volatile masked forms,
and no registry or Lean rule domain changes; the `core-v1` path admits no
bit-field.

**Revision 2.23.0, 2026-10-21.** Section 10.1.21 provides the `<stdatomic.h>`
of Darwin, which is Clang's resource header. On `aarch64-apple-darwin` the
front end has the fourteen `__c11_atomic_*` builtins that the header uses,
with Apple clang's operands and orders, and predefines the ten
`__CLANG_ATOMIC_*_LOCK_FREE` macros with the value 2. `__has_builtin` answers
1 for the builtins. Each form is a scalar access of Section 3.8: `init` is a
relaxed store, a pointer `fetch_add` or `fetch_sub` moves by whole elements,
and a floating object moves as its bits, so its compare-exchange compares
representations. A compare-exchange admits a failure order stronger than its
success order and carries the stronger one. Other shapes are `E2913`, Clang's
other `__c11_atomic_*` spellings are `E2949`, and a size that is not lock-free
is `E2935`. Linux is unchanged, because GCC 14 has none of these names.
Section 10.1.14 follows. No grammar production, registry or Lean rule domain
changes; the `core-v1` path admits no atomic.

**Revision 2.22.0, 2026-10-21.** Section 3.13.6 admits the half storage
formats `f16` and `bf16` of the 2026-10-13 decision on L03. `_Float16` and
`__fp16` values are `f16` values, and `__bf16` values are `bf16` values. A
half format admits only constants, conversions and representation moves, and
each conversion rounds once. A C operation on half operands evaluates in
binary32 and rounds once to the half format at an assignment, a cast, a
return, an argument or the value of a statement expression, as GCC 14 does on
Armv8-A; an `__fp16` operand promotes to `float`. Constant evaluation and both
interpreters follow the same rules. A half value crosses a call in the low 16
bits of a vector register, and a binary16 record of one to four members is an
HFA. An HFA of `__bf16` members is `E3049`. A record that mixes the two half
formats is no HFA: it passes as a composite on Linux. On Darwin it is
`E3049`, and so are an anonymous `_Float16` or `__bf16` argument and its
`va_arg`. The `bf16` and `BF16` constant suffixes and the `__BFLT16_*` macros
are Linux only. Operands of `_Float16` and `__bf16` together are the new
`E2961`. Sections 1.1, 1.7, 3.12, 3.13, 6.3, 10.1.1, 10.1.7 and 10.1.14 and
the appendices follow. The grammar of 9.3 adds the types `f16` and `bf16`, their
constants of four hexadecimal digits and `hfa=Cxf16`. The `core-v1` path
admits no half value, and no Lean rule domain changes.

**Revision 2.21.0, 2026-10-21.** Section 3.8 admits floating vectors
`vec<L, f32>` and `vec<L, f64>` in `vec.pack`, `vec.extract` and
`vec.bitcast`, which move bits. Section 10.1.14 adds the floating-vector
profile of the 2026-10-13 decision on L05: GNU `float` and `double` vectors
execute lane by lane with the scalar rules of 3.13, comparisons give integer
masks, conversions are the C cast of each lane with `E3063`, and `++`, `--` and
`&v[i]` follow GCC 14. An HVA is one to four short vectors of one size,
whatever their lane types, so a record of floating vectors passes in vector
registers and is no longer `E3049` (10.1.7); this also makes a record of two
integer vectors of different lane shapes an HVA. A 4-byte floating vector at a
call boundary and any floating vector under `-mgeneral-regs-only` are `E4002`.
Sections 3.10, 3.12 and 3.13 and Appendix A follow. No grammar production,
registry or Lean rule domain changes; the `core-v1` path admits no vector.

**Revision 2.20.0, 2026-10-21.** Section 3.13.5 adds the parts of a complex
product and quotient: `fp.cmul.real`, `fp.cmul.imag`, `fp.cdiv.real` and
`fp.cdiv.imag`, with strict forms. On Linux each part is the part that
libgcc's helper of GCC 14 computes, NaN payloads and status flags included.
One model gives them, and both interpreters and translation-time evaluation
use it. On Darwin, compiler-rt has no model: the interpreters stop with
`E3067`, and translation-time evaluation reports `E2960`. Section 10.1.14
admits `float`, `double` and `long double` complex values: per-part
operations, HFA calls, mixed real operands, truth and equality tests,
volatile part accesses and constants. Plain `_Complex` is `double _Complex`.
A volatile integer complex object is also accessed part by part, and an
integer-complex update with a floating side follows the floating rules.
GCC's `_FloatN _Complex` types are admitted in declarations, and
`__builtin_tgmath` selects `float`, `double` and `long double` complex
functions. Section 9.3 adds the four mnemonics, and Section 6.3 adds `E2960`
and `E3067`. No registry or Lean rule domain changes; the `core-v1` path
admits no floating operation.

**Revision 2.19.0, 2026-10-21.** Section 5.2 tracks initializedness per bit,
and Section 3.8 adds an optional `mask` field to `load` and `store` of
`bits<N>`. A load mask selects the bits that must be initialized; a store
mask selects the bits that become initialized, and the other bits keep their
state. `copy` moves the state of each bit. Section 10.1.6 lowers a
non-volatile bit-field access with these masks, so a store to a bit-field is
not a read of its neighbours, and a read of a never-stored member stays
`E3001`. Sections 9.3, 9.3.1 and 9.5 give the syntax and its rejections.
Native code does not change. No registry or Lean rule domain changes; the
`core-v1` path admits no masked access.

**Revision 2.18.1, 2026-10-21.** Section 10.5 corrects the wide character
types of the Darwin target. They follow Apple clang: `wchar_t` and `wint_t`
are `int`, wide literals have `int` elements, and the `wchar_t` and `wint_t`
macros have Apple clang's spellings. Linux keeps GCC 14's `unsigned int`. No
EvoIR operation, registry or Lean rule domain changes.

**Revision 2.18.0, 2026-10-21.** Section 10.1.14 admits `__builtin_tgmath` as
GCC 14 documents it: the call goes to the function that the `<tgmath.h>` rules
select, and each argument is evaluated once. `__has_builtin` reports it.
Section 6.3 adds `E2959` for a selection of a complex function. This repairs
the type-generic macros of glibc's `<tgmath.h>` for real floating and integer
arguments. No EvoIR operation, registry or Lean rule domain changes.

**Revision 2.17.0, 2026-10-21.** Section 10.1.9 accepts GNU attributes after
the `(` of a parenthesized declarator, where GCC 14 and Apple clang accept
them. In a named declarator, such an attribute attaches to the declared
entity and is preserved without effect. An attribute with an effect is
`E2903` there, and a refused attribute keeps its code. This repairs 53 libxml
and libexslt headers of the Darwin SDK. No EvoIR operation, registry or Lean
rule domain changes.

**Revision 2.16.0, 2026-10-21.** Section 10.1.14 admits enumerations with a
fixed underlying type (C23 6.7.2.2) in every language mode. The type sets the
size, alignment, range, conversions and call ABI, and each enumerator has the
enumerated type. Section 6.3 adds `E1306`, `E1307` and `E2958`. This repairs
Darwin's `kern/kcdata.h` and `malloc/malloc.h`. No EvoIR operation, registry or
Lean rule domain changes.

**Revision 2.15.0, 2026-10-21.** Section 3.13.1 adds `__builtin_fpclassify`
as GCC 14 documents it. It returns the class argument that names the class of
`x`, with no promotion of `x`, and `__has_builtin` reports it. A class
argument that is not an integer constant expression, or an `x` that is not
`float`, `double` or `long double`, is `E2951`. This repairs `fpclassify` of
glibc's `<math.h>`. No EvoIR operation, registry or Lean rule domain changes.

**Revision 2.14.0, 2026-10-21.** Section 10.5 adds `__has_include_next`, the
fifth feature query, and states the search of `#include_next`. The search
continues after the directory that found the current file. In the main file,
the directive and the query search as `#include` and `__has_include` do, as in
GCC 14 and Clang. On Darwin, Clang's resource `<limits.h>`, `<stdint.h>`,
`<tgmath.h>` and `<unwind.h>` now include the SDK headers of the same name, as
with Apple clang, so `PATH_MAX` exists after `<limits.h>`. No EvoIR operation,
registry or Lean rule domain changes.

**Revision 2.13.0, 2026-10-20.** Section 10.1.9 accepts every execution and
calling-convention attribute of its list on a function declaration that is not
a definition, and records it. A definition and each use of a function that
such a mark records stay `E2952`, in any declaration order. A use of a
function marked `aarch64_vector_pcs`, `preserve_all` or `preserve_most` is
accepted, and a call of it is an ordinary base-PCS call. Every other placement
keeps `E2952` at the attribute. This repairs every Linux unit that includes
glibc's `<math.h>` or `<tgmath.h>`. No EvoIR operation, registry or Lean rule
domain changes.

**Revision 2.12.0, 2026-10-20.** Section 3.10 adds `call.returns_twice`, a
direct call that can return a second time, with the top effect summary.
Section 10.1.9 marks a function by its `returns_twice` attribute or by GCC
14's name rule, which EvoCC also applies to block-scope declarations. It
accepts the definition and the uses of such a function and lowers its direct
calls to the new form. A function that makes such a call, or that returns
twice, is never substituted; an `always_inline` one is `E2957`, which is new.
Section 9 adds the grammar, the shape and the field; text without the mark is
unchanged. No registry or Lean rule domain changes.

**Revision 2.11.1, 2026-10-20.** Section 10.1.9 accepts `returns_twice` on a
function declaration that is not a definition and records it. `E2952` moves
to each definition and each use of the designator of a marked function, in
any declaration order. A use in a dropped internal function is not reported.
Every other placement keeps `E2952` at the attribute. This repairs every Linux
unit that includes glibc's `<pthread.h>`. No EvoIR operation, registry or
Lean rule domain changes.

**Revision 2.11.0, 2026-10-20.** Section 10.1.14 admits the generic
`__atomic_load`, `__atomic_store`, `__atomic_exchange` and
`__atomic_compare_exchange` forms on 1-, 2-, 4- and 8-byte objects whose
alignment is at least their size. Section 3.8 adds the `atomic.copy` forms:
one atomic access of the object and byte transfers of ordinary buffers, with
`E3001` for an uninitialized compared byte. 16-byte and misaligned objects
stay `E2913`. No registry or Lean rule domain changes.

**Revision 2.10.0, 2026-10-20.** Section 10.1.21 admits `_Atomic` objects.
The layout follows GCC 14 on Linux and Apple clang on Darwin. A read of a 1-,
2-, 4- or 8-byte atomic scalar is a `seq_cst` `atomic.load`, a simple
assignment a `seq_cst` `atomic.store`, and a compound assignment, `++` or
`--` one `seq_cst` compare-exchange loop with the ordinary obligations.
Floating updates, whole-record accesses and 16-byte objects stay `E2702`.
`E2955` and `E2956` are new. No new EvoIR instruction, registry or Lean rule
domain.

**Revision 2.9.0, 2026-10-20.** Section 10.1.14 admits the GNU `__atomic`
fetch-op, op-fetch, exchange, compare-exchange, test-and-set, clear, fence and
lock-free builtins on 1-, 2-, 4- and 8-byte scalars, with E2947 for exactly
the constant orders that GCC 14 warns about and every constant outside 0..5.
Section 3.8 admits `bits<1>` exchange and compare-exchange and a byte count
for pointer `add` and `sub`. Fences use the barrier assembly of
`__sync_synchronize`. No new EvoIR instruction, registry or Lean rule domain.

**Revision 2.8.0, 2026-10-20.** Section 10.1.14 admits integer vectors of
1, 2 and 4 bytes at a call boundary. Linux follows GCC 14 in general
registers; Darwin follows Apple clang's 4-byte argument image and widened
`v0` result. The E4002 refusal of these signatures is removed. No EvoIR
operation, registry or Lean rule domain changes.

**Revision 2.7.0, 2026-10-20.** Section 10.1.14 admits integer-complex
division with GCC's wide algorithm in the component type on both targets.
Each step keeps its E3002 or E3004 obligation, and only the selected branch
executes. Compound updates convert the value to the common complex type; a
real integer target keeps the converted result, and a floating or pointer
target is E2102. E2948 is withdrawn. No EvoIR operation, registry or Lean
rule domain changes.

**Revision 2.6.8, 2026-10-19.** Section 10.1.9 admits only the three disabling
automatic-vectorization options in `optimize`. All other options still fail
closed, including semantic options mixed into the same attribute.

**Revision 2.6.5, 2026-10-16.** Section 10.1.10 evaluates every variably
modified `typeof` expression operand once, including pointer types, under
the existing scope and unevaluated-context rules. Section 10.1.17 gives both
activation-address builtins a runtime null result at converted depths 1..65535
on both targets. Depth zero and independently supplied EvoIR retain their
semantics. No registry, Core subset or Lean theorem domain changes.

**Revision 2.6.4, 2026-10-16.** Sections 10.1.17 and 10.1.19 admit the reviewed
file-scope string-constant rule. The query uses written initializers, preserves
ordinary calls outside the rule and refuses late interposition with E2954.
No interpreter, IR operation, registry or Lean rule domain changes.

**Revision 2.6.3, 2026-10-16.** Section 10.1.14 adds three Clang checked-string
builtins, four GNU checked-output builtins, literal copy bounds and the runtime
cache-flush call. External library availability remains a target requirement.
Section 11.5 identifies the normal-dependency test; all five printed registries
have digest tests. No registry bytes or Lean rule domain change.

**Revision 2.6.2, 2026-10-16.** Source diagnostics use E2918 for jumps into
statement expressions and E0107 for pack directives inside records. Member
lookup reports E2906 at an absent or ambiguous name. Section 10.1.19 checks
the bound's integer type before storage restrictions. Section 10.1.3 names
E2953 for incompatible struct casts and the E2946 lowering limit for a library
symbol that collides with a linked source object. Builtin namespaces and
same-type record casts retain their existing meaning.

**Revision 2.6.1, 2026-10-16.** Section 10.1.3 names the pointer subtraction
and ordering obligations that EvoIR execution does not check. Completed checks
report only findings from implemented checks. The C-HIR refusal for these
unsupported relations is E3007, not E3013; this is not full C subobject checking.

**Revision 2.6.0, 2026-10-15.** Sections 9.1 and 11.2 admit target-bound Darwin
rewrite chains beside the unchanged implicit Linux format. The five integer
registries are layout-independent; their printed bytes and digests do not
change. The checker binds the input, output and reconstruction to one profile.
The Lean `core-v1` compilation profile remains Linux-only.
The checker derives only claimed regions and uses iterative witness traversal.
Typed parse errors preserve the malformed/invalid distinction of Sections 9.5
and 11.4; input counts and class references are checked before allocation or use.

**Revision 2.5.1, 2026-10-15.** Section 10.1.9 names the execution and
calling-convention attributes that the front end refuses with `E2952`.
In particular, a `naked` function cannot silently receive an ordinary frame.
Section 6.3 adds the diagnostic authority row.
Section 10.1.3 states the division obligation for zero-sized pointer differences.

**Revision 2.5.0, 2026-10-14.** The text of equality saturation and of the
checker states what the code does. Reaching a bound keeps the admitted
regions, a failed transaction returns the original module, and the driver's
chain then stops with exit 1 (Sections 8, 8.9 and 8.11). Section 8.11 states
what a certificate records and describes the current producer as
informative, and Section 8.9 states the transaction and the producer's
history self-check. Sections 4.4, 6.4, 6.5, 7.2, 8.1 to 8.4 and 10.2 mark as
informative the design that no code applies, and Section 10.3 is marked as a
summary. Section 11.6 names the code that producer and checker share or copy
and the mutation campaign that exists, and Section 11.7 adds that code and
the renderer to the trusted base. Pointer equality compares abstract
addresses (Sections 3.5 and 5.3). Smaller corrections: `f128` anonymous
arguments (Sections 3.10 and 3.12), the effects of `asm` (Sections 3.11 and
10.1.16), the reason table and the fault location of Section 6.1, the
summary of the Section 9.8 example and the attribute answers of Section
10.5. No registry block, grammar production or row of the Section 6.3 table
changes. The errata note is `docs/archive/notebook/spec-errata-20261014.md`.
Revision 2.4.0 of 2026-10-14 stated the Darwin target profile (Sections
3.11, 9.1, 9.3, 10.1.3, 10.1.12 and 10.5). Revision 2.3.0 of 2026-10-13
gave `builtin-argument-type` its own code, `E2951` (Sections 3.13.1 and 6.3).
Revision 2.2.0 of 2026-10-13
added the object-size checked builtins, the GNU argument pack, `E2949`,
`E2950` and `E3066` (Sections 6.3, 10.1.9, 10.1.14 and 10.5). Revision 2.1.0
of 2026-10-13 attached `used` and `retain` after a pointer `*` (Section
10.1.9). Revision 2.0.1 of
2026-10-07 added the front-end warning levels (Section 6.3). Revision 2.0.0 of
2026-10-06 folded revision 1.9.116 and its rulings into one pinned text; the
review transcript is archived, not normative, in
`docs/archive/notebook/evoir-spec-review-20261001.md`. No registry byte changes.

The complete history is in git: `git log -- docs/evoir-spec.md spec/evoir.md`.

**Change procedure.** A change to this document must follow these steps:

1. Edit the normative text. Keep the machine-read parts listed in "Status
   and scope" in their shape. A printed registry block must not change;
   Section 11.6 defines how a new registry revises an older one.
2. Bump the revision in the header: the patch number for a correction that
   changes no requirement, the minor number for a compatible addition, the
   major number for an incompatible change. Replace the paragraph above with
   the new revision, its date and what it changes.
3. Update the digest of this file in `spec/pins.json`.
4. Run the gate. Every command must pass:
   `cargo build -p evocc`;
   `cargo test -p evocc-evoir --test spec_grammar_contract -p evocc-eqopt --test registry_digests -p evocc --test check_product`;
   `python3 tools/check-textual-grammar.py`;
   `python3 -m unittest tools.lean.tests.test_registry_to_lean`.

**What the 1.x series added** (informative):

| Revisions | Content |
|---|---|
| 1.0 | Types, block-argument SSA, instruction set, effects, memory, UB, equality criterion, persistent equality database. |
| 1.1 to 1.4 | Formal parameters as call-interface binders; explicit `mem` and `world` token positions; the trailing `mem<R>, world` interface; UB predicates with reasons `E3002` to `E3006`. |
| 1.5 to 1.6 | Scalar poison (`uninitialized`), `trap` terminators, `--emit=evoir` as the canonical text. |
| 1.7 | Object and provenance model, `mem<all>` snapshots, globals, automatic objects, pointer and access guards, volatile world traces. |
| 1.8 | Textual syntax as a byte grammar with rejection rules; P0 eligible regions and extraction. |
| 1.9.1 to 1.9.34 | GNU C profile (attributes, `typeof`, `_Generic`, statement expressions, builtins, checked arithmetic, front-end `asm`), EvoCPP target profile, `copy`, constant expressions, source locations. |
| 1.9.35 to 1.9.56 | Certificates `evp1` and `evp2`, registries `p0.scalar.v1` and `p0.scalar.v2`, analysis lattices, bounded scheduler, fuel. |
| 1.9.57 to 1.9.79 | Whole-kernel lowering: inline assembly, external tables, relocations, module assembly, block addresses, linkage, `byval` and `sret`, `always_inline`. |
| 1.9.80 to 1.9.105 | Assembly constraints, integer modes, computed `goto` edges, object-size queries, visibility, TLS and lifecycle tables, scalar atomics, function sections, 128-bit division. |
| 1.9.106 to 1.9.115 | Scalar floating point: strict operations, FMA, the floating-environment library boundary, `FENV_ACCESS`, binary128. |
| 1.9.116 | Guard registry `g0.ub.v1` and certified total-integer guard folding. |
