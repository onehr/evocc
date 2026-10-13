# N06 design: EqSat memory and effect domain

This note is the design of N06 in `TODO.md` ("EqSat memory and effect
domain"). The owner asked on 2026-10-13 to finish every open item, N06
included. The TODO entry says that CSE, dead code and alias-aware forwarding
need a state and effect model and a proof design of their own. This note gives
both. It changes no code, specification, receipt or task file. An
implementation agent works through the milestones of Section 6 in order. Each
milestone names its tests from Section 5.

## Base, scope and evidence

- Base: commit `ff966284`. `crates/evocc-evoir/src/lib.rs` and
  `spec/evoir.md` had uncommitted edits by other agents when they were read,
  so their line numbers can drift. This note cites the specification by
  section, not by line.
- Specifications read: EvoIR 2.3.0 (Sections 1 to 11), EvoMIR 1.1.0
  (Section 5.1), `spec/verification.md` 1.0.0, and the T03 audit
  (`docs/archive/notebook/spec-audit-20261013.md`, row A23).
- Code read: `crates/evocc-egraph`, `crates/evocc-eqopt`, `crates/evocc-proof`,
  `crates/evocc-proofcheck`, `crates/evocc-randgate`, `crates/evocc-passes`,
  the interpreter and validator in `crates/evocc-evoir`, `crates/evocc-memory`,
  and the Lean sources under `lean/EvoIR/` (`Rule.lean`, `Proofs/Rules.lean`,
  `Rewrite/`, `Core/IR.lean`, `Core/IRExecution.lean`).
- Run: only the frozen compiler `.build/frozen/366fd796/evocc-darwin`
  (`evocc 0.1.0 (366fd796613e 2026-10-13)`). It emitted EvoIR text and
  assembly for small probes, the five kernel reductions and the six S5 units.
  A read-only Python census then read that text (Appendix A). No Cargo, no
  Lake, no build.
- History read: N01 to N07 and N08 to N11 in `TODO.md`, the N03b/N03e
  negative preflight, the S1 to S5 EqSat size packages, the certified guard
  slice and the codegen quality plan.

**Claim ceiling.** This is a design with census estimates. The census counts
syntactic opportunities in EvoIR text. It is not a validated transformation,
not a size result and not a proof.

## 1. Decisions

| # | Decision | Section |
|---|---|---|
| D1 | A memory snapshot becomes an e-graph value of sort `mem<all>`. A class means Kleene equality of partial values: its members fault together or yield the same value. | 3.2 |
| D2 | The module after a memory transaction is the module before it, with some instructions deleted and their results renamed. N06 never emits, moves or reorders a memory instruction. Token chains stay linear. | 3.1, 3.7 |
| D3 | An instruction is deleted only when every result class holds a defined, available representative. A deleted instruction therefore cannot fault. Every trap, diagnostic code and volatile event stays. | 3.7 |
| D4 | The proof of a root uses only earlier instructions of its block. A rule that needs "this instruction ran" applies only to a witness before the root. | 3.2 |
| D5 | Alias facts are local and syntactic: one SSA root with an exact byte offset, distinct globals, and fresh unexposed allocas against older pointers. No escape analysis and no type-based alias analysis in the first version. | 3.4 |
| D6 | Calls that write, `asm`, volatile and atomic accesses, fences, `va_*`, `tls_addr`, `alloca.dynamic` and `lifetime.end` are barriers. Their memory result is an opaque seed. Strict floating point never touches memory, and no rule merges a world token. | 3.11 |
| D7 | Dead stores use a separate checked scan, not e-graph equality. Deleting a store changes every snapshot between the store and its killer. | 3.8 |
| D8 | A separate certified transformation first merges straight-line block chains. Memory regions stay inside one block. | 3.9 |
| D9 | Effect results that can never be poison (loads, divisions, UB predicates) become opaque scalar seeds in new registries. This is the first milestone. It lets certified guard folding see loaded operands. | 3.10 |
| D10 | Memory knowledge lives for one transaction. The persistent equality state keeps scalar classes only and receives no memory union. | 3.7 |
| D11 | Each family has its own registry, digest and Lean proofs: `p1.seeded.v1`, `g1.seeded.v1`, `c0.chain.v1`, `m0.mem.v1`, `m1.dead.v1`. A new certificate version `evp3` carries `c0`, `m0` and `m1`. | 4.3 |
| D12 | Lean proves the rules against a new byte-level memory model. Differential vectors tie the model to `evocc-memory`, and a bridge lemma ties it to `Core.IR`. The proof-required path refuses the new registries during N06. | 4.1, 4.6 |

The names of registries, records and files in this note are proposals. The
specification revision of each milestone fixes the final spelling.

## 2. The current state

### 2.1 What the e-graph represents

- **Pure total scalars only.** The P0 allowlist of EvoIR §8.5 is `const`,
  `add`, `sub`, `mul`, `and`, `or`, `xor`, six `icmp` forms, `shl`, `lshr`,
  `ashr`, `zext`, `sext` and `trunc`, at `bits<1..64>`. The producer and the
  checker each implement it, as §11.6 requires:
  `crates/evocc-eqopt/src/eligibility.rs:77` and
  `crates/evocc-proofcheck/src/eligibility.rs:427`.
- **One root per region, inside one block.** A region has one replacement
  root and a witness closure of earlier allowlisted instructions in the same
  block (§8.5). Its seeds all have origin `input` (§8.7). The format and the
  checker already parse origin `definition`, but no profile produces it.
- **Registries.** `p0.scalar.v1` to `v4` hold 55 rules. `g0.ub.v1` folds seven
  total UB predicates to true (§8.12). A g0 root may be a predicate, but its
  operands still obey P0 exclusion.
- **Engine.** `crates/evocc-egraph/src/lib.rs` keys a node by operator and
  children (`ENode` at :119, hash-cons key at :269). The node carries no sort.
- **Persistent state.** `crates/evocc-eqopt/src/persistent.rs` keeps classes
  across transactions, keyed by `(function, ValueId)` (§8.9). Its history
  replays P0 rules only.
- **Chain.** `crates/evocc-eqopt/src/chain.rs` runs at most eight
  transactions. The standalone checker replays each certificate before the
  chain publishes anything.
- **Driver.** `-Oeq-scalar` works for the Linux target only;
  `crates/evocc/src/main.rs:1138` refuses Darwin because its EvoIR text has no
  checker.

### 2.2 How effects and memory stay out

The IR already threads state explicitly. The frozen compiler lowers
`p->a = x; y = p->a;` to:

```text
%v4 = access_in_bounds %v2, %v0 [size=4, align=4]
(%v5, %v6) = store %v4, %v3, %v0, %v1
%v7 = access_in_bounds %v5, %v0 [size=4, align=4]
(%v8, %v9, %v10) = load %v7, %v6, %v0 : bits<32>
```

1. Every memory operation consumes and produces `mem<all>` and `world`
   (§1.4, §1.5, §3.8). The executable profile admits `mem<all>` only.
2. A load or a store consumes the output of a matching `access_in_bounds`.
   Every `ptr.offset` result goes at once to `pointer_valid` (§3.7). The
   interpreter keys a guard by the runtime identity of its token
   (`matching_guard` in `crates/evocc-evoir/src/lib.rs`).
3. Tokens are linear. The validator rejects a stale `mem` or `world` token,
   and the interpreter keeps one current memory.
4. SSA is block-local (§2.4). A value crosses a block only as an edge
   argument, so every region is inside one block.
5. §8.5 excludes a value when its sort is `mem`, `world` or a pointer, or when
   its instruction is not allowlisted, may trap or has an effect. Exclusion
   then flows to every value that uses it and across edges
   (`crates/evocc-eqopt/src/eligibility.rs:153`, `:179`). So
   `add %loaded, 0` is no region, and a loaded value cannot even be a seed.
6. §4.5 states this as an invariant. §5.5 puts forwarding, redundant-load
   elimination and every `NoAlias` rewrite outside the revision. §3.8 forbids
   any rewrite family to introduce, remove or reorder `copy`. §3.7 requires a
   future memory profile to treat every `ptr.to_int` as an escape. §7.2 lists
   alias facts as "not in this revision".
7. EvoMIR lowers `pointer_valid` and `access_in_bounds` to nothing
   (`spec/evomir.md` §5.1). Native code checks no memory fault, so deleting a
   memory guard changes no native instruction.

### 2.3 How a rewrite is justified now

- **Producer.** `crates/evocc-eqopt/src/evp1.rs` derives per-region events
  from the e-graph bridge (`scalar_eq.rs:535`) and writes `evp2` (§11.3).
- **Rust checker.** `crates/evocc-proofcheck` recomputes eligibility,
  manifests and seed facts with its own code, replays every event against its
  own rule table (`replay.rs`), reassembles the module (`reassemble.rs`) and
  compares canonical bytes. It exits 0, 2 or 3.
- **Lean rules.** A rule is data (`lean/EvoIR/Rule.lean`). One fixed statement,
  `RuleSound` and `RuleCovered`, speaks about bit-vector denotations. Each
  registry rule has a theorem in `lean/EvoIR/Proofs/Rules.lean`. Agreement
  between that model and the Rust semantics rests on differential vectors
  (§11.7, A7).
- **Lean rewrite checker.** `lean/EvoIR/Rewrite/Check.lean` runs only in
  proof-required mode. It admits pure roots only (`isPure`, :36), refuses the
  root-deletion form (`spec/verification.md` §9), and returns `none` for an
  unknown registry (`Rewrite/Rules.lean:87`).
- **Lean memory today.** The `core-v1` semantics already has snapshots with
  per-byte initialization (`lean/EvoIR/Core/IR.lean:108`) and executes
  `allocate`, `access`, `load` (32 or 64 bits), `store` and `endLifetime`
  (`Core/IRExecution.lean`; a store of poison faults with E3001 at :116). It
  has no `ptr.offset`, global, read-only object, stored pointer, `copy` or
  exposure.

### 2.4 The A23 width problem

Audit row A23: `zext.bits<32> %x` and `zext.bits<64> %x` share one class.
`OP_ZEXT` (`crates/evocc-eqopt/src/scalar_eq.rs:34`) has no width, and the
hash-cons key has no sort. The defect is latent today, because the certificate
carries `sort=` and the checker checks it.

N06 would make it live. A load's type, access size and alignment are not among
its children. Two loads of different widths from one address would hash-cons
into one class. Task T03v keys persistent nodes by result sort. N06 depends on
T03v, and its own graph must key every node by operator, sort, attributes and
children from the start.

### 2.5 Where the memory cost is

The kernel profile (`tests/fixtures/kernel-tcg-instruction-profile.out`)
ranks the causes of EvoCC's excess on K02. Block-argument transport is 29.6%
of executed kernel instructions (N08). Time-scaled waits are about 23%.
Out-of-line helpers cost 5.4% (N09). One-word structs live in stack slots
(N10). Return signing runs on every function (N11). Constant shift-count
guards stay unfolded. N06 addresses none of the first two causes. The profile
has no class for loads and stores through pointers.

The frozen compiler's assembly for `local-array-constant-index.c` (`theta`) at
`-O2`, checked profile, shows what N06 can reach:

- `bc[]` is zero-filled by five `str`, then each element is stored again.
- Each `bc[k]` address is built as `movz; sxtw; lsl; add`.
- Each `rol64` by a constant still runs `movz w5, #63; cmp w5, #64; b.hs`.
  This is the shift-count guard of a loaded operand. Even with `g0.ub.v1`
  enabled, the guard stays, because §8.12 keeps the loaded operand excluded.

Census of EvoIR text (method and caveats in Appendix A). "Merged chains"
treats a run of single-predecessor `br` successors as one block (Section 3.9).
Alias facts are the local facts of Section 3.4.

| Input | Loads | Redundant loads, per block | Redundant loads, merged chains | Of these, reuse distance ≤ 64 | `access_in_bounds` implied | `pointer_valid` implied | P0 roots eligible now → with definition seeds |
|---|---:|---:|---:|---:|---:|---:|---|
| theta | 60 | 2 | 35 | 1 | 65 / 95 | 44 / 73 | 0 → 65 of 65 |
| struct-value-through-stack | 5 | 0 | 0 | 0 | 0 / 9 | 0 / 1 | 5 → 22 of 23 |
| loop-carried-transport | 2 | 0 | 0 | 0 | 1 / 3 | 0 / 3 | 7 → 12 of 14 |
| asm-goto-live-transport | 16 | 0 | 0 | 0 | 0 / 36 | 19 / 54 | 6 → 6 of 6 |
| leaf-helper-call | 3 | 0 | 0 | 0 | 0 / 3 | 0 / 4 | 0 → 13 of 13 |
| zlib inflate | 444 | 33 | 33 | 33 | 76 / 726 | 57 / 729 | 67 → 792 of 861 |
| zlib deflate | 1,180 | 63 | 63 | 63 | 364 / 1,610 | 230 / 1,478 | 177 → 1,193 of 1,260 |
| cJSON | 539 | 9 | 9 | 9 | 88 / 867 | 41 / 699 | 257 → 659 of 996 |
| zstd compress | 1,857 | 101 | 160 | 160 | 256 / 2,937 | 357 / 3,392 | 1,691 → 3,324 of 4,272 |
| zstd decompress | 410 | 11 | 11 | 11 | 38 / 681 | 42 / 675 | 413 → 825 of 1,167 |

Dead zero-fill stores: five of 35 stores in `theta` and one of three in the
struct reduction (both emitted with `-ftrivial-auto-var-init=zero`). The
library units were emitted without that option and show none. `integer-mix`
has no load and is left out of the table.

What the table says:

- Redundant loads are 1.7% to 8.6% of loads in the library units. Almost all
  reuse a value at most 64 instructions back. The native gain is therefore a
  few hundred bytes per unit, not a step toward the 1.4x target.
- In `theta`, chain merging raises the count from 2 to 35. But 34 of the 35
  reuse a value more than 64 instructions back. Reusing the 25 `st[k]` values
  would hold 25 extra registers live, and EvoCC's allocator would spill them.
  The ten `bc` loads need only the five stored values, so they are worth
  deleting (Section 3.12).
- Definition seeds widen the scalar domain two- to twelvefold in eligible
  roots. The S4 diagnostic bounds the guard bytes that this can recover
  (Section 3.10).
- The three transport and call reductions have no memory redundancy. They
  belong to N08, N09 and N11.

## 3. The memory and effect model

### 3.1 Principles

1. **UB is a report, not a licence.** EvoIR §4.2, §4.5 and §7.1 already say
   this for P0. N06 never erases, advances, delays or changes a trap, a
   diagnostic code or a volatile event. The randomized gate today discards
   programs that the interpreter reports as undefined; N06 adds a strict
   oracle that compares their diagnostics too (Section 5.2).
2. **Subsequence.** A memory transaction deletes instructions, renames their
   results, recomputes function effect summaries and renumbers values. It
   emits no instruction. Scalar emission stays with the P0 and P1
   transactions.
3. **Linear tokens.** A deleted instruction's token results are renamed to its
   own token operands. Every surviving instruction still consumes the current
   token.
4. **Earlier knowledge only.** The proof of a root uses only witnesses that
   precede it in its block.
5. **One block.** A region stays inside one block. Reuse across blocks needs
   the chain merge of Section 3.9 or a later design.
6. **Equality does not force extraction.** A rule can merge two pointer
   classes while both pointer instructions stay. The policy decides what to
   delete.
7. **No persistent memory knowledge.** Memory classes die with the
   transaction.
8. **One family, one registry, one proof set.**

### 3.2 Semantic domain

**Sorts.** `bits<N>` (memory values up to `N = 128`; scalar arithmetic stays
at `N <= 64`), `ptr<generic>`, `fnptr<Sig>`, `mem<all>`, and the proof-only
sort `meta`. A `meta` value is the object table without bytes: identities,
sizes, alignments, owners, read-only marks and lifetimes. No rule merges a
`world` token, and world tokens do not enter the graph.

**Kleene equality.** At the root of a region, in the execution of the module
before the transaction, each class denotes a partial value: a value or a
fault. Two members are equal when both fault, or both yield the same value.
Congruence holds, because every node operator is a deterministic partial
function of its children.

**Ran witnesses.** A witness is an instruction of the region that precedes
the root. In that execution it ran without a fault, so its nodes denote
values. Some equalities hold only for an instruction that succeeded, for
example "the output token of a guard equals its input token". Such a rule
carries the side condition `ran` and applies only to a non-root witness node.

**Root success.** If a root's result class holds a member that denotes a
value, the root denotes that value too. Then the root cannot fault, and
deleting it changes no observation (D3).

### 3.3 Witnesses, barriers and seeds

| Instruction | Role in `m0` | Nodes |
|---|---|---|
| `access_in_bounds` | witness or root | its `mem` result |
| `pointer_valid` | witness or root | its `mem` result |
| `load`, not volatile, not atomic | witness or root | value and `mem` results |
| `store`, not volatile | witness | `mem` result |
| `copy` with static size, not volatile | witness | `mem` result |
| `alloca` with static size | witness | pointer and `mem` results |
| `ptr.offset.*.*` | witness or root, with its `pointer_valid` | pointer result |
| `global_addr` | witness or root | pointer result |
| `const` | witness | value |
| `call` whose callee writes nothing (Section 3.4) | witness | `mem` result; value results are seeds |
| every other instruction that carries `mem` or `world` | barrier | none; its results are seeds |
| `ptr.to_int` | not a node; see "exposure" in Section 3.4 | none |

A seed is an operand of a witness that no witness of the region defines: a
block argument, or a result of an earlier instruction. Seeds may now have any
sort, including `mem<all>` and pointers. A barrier's `mem` result is an
opaque snapshot seed. That is how a barrier stops every rule.

A multi-result instruction gets one node per result that the table lists.
Each node line names its result index. For the memory profile this replaces
the rule of §8.8 that a multi-result instruction is never a root.

### 3.4 Facts

The producer and the checker each compute these facts from the module before
the transaction. They do not share the code (§11.6).

1. **Maybe-poison.** Taint of §8.5, extended: results of `uninitialized`, of
   `call` (a callee can return poison) and of `va_arg`, and every block
   argument that receives one of them on any edge. Arithmetic on poison
   faults with E3001 in the EvoIR interpreter and in the Lean `Core.IR`
   semantics, so taint needs no operand propagation beyond §8.5.
2. **Pointer root and offset.** One forward pass over each block, from its
   entry. A pointer's root is a fresh `alloca` at a known position in the
   block, a global symbol after alias resolution (§5.2: an alias and its
   target are one provenance), or else an opaque root: a block argument or the
   result of any other instruction (a pointer load, a call, `ptr.from_int`).
   `ptr.offset` keeps the root of its base. The offset
   is exact when every `ptr.offset` on the way has a constant index;
   `index * scale` is computed in mathematical integers, as §3.7 does.
   Otherwise the offset is unknown. Facts are computed for the whole block,
   not for a region window, so a pointer seed keeps its fact.
3. **Fresh and unexposed.** An `alloca` at position `i` is fresh against a
   pointer whose root is a block argument, a global, another `alloca`, or a
   value defined before `i`. The rule holds only if no `ptr.to_int` in the
   block consumes a pointer rooted at that `alloca`. Such a pointer could
   otherwise be reached through `ptr.from_int` (§3.7).
4. **Exposure.** `ptr.to_int` changes the exposed set without a token. Only a
   pointer with exposed-address provenance resolves at access time
   (`resolve_pointer` in `crates/evocc-memory/src/lib.rs`), and growth of the
   exposed set can make that resolution ambiguous (two zero-sized objects at
   one address). Then a later access faults where an earlier one did not. So
   every `load`, `access_in_bounds` and `pointer_valid` node over a pointer
   with an opaque root carries the attribute `epoch`: the number of
   `ptr.to_int` instructions before it in the block. Nodes of different epochs
   never hash-cons, and no rule relates them. Fresh and global roots have
   object provenance and need no epoch.
5. **Disjoint.** `disjoint(p, n, q, m)` holds when the two roots are the same
   and the exact byte ranges `[off_p, off_p + n)` and `[off_q, off_q + m)` do
   not meet; or the roots are two different global symbols after alias
   resolution; or one root is a fresh unexposed `alloca` and the other is one
   of the pointers of item 3.
   Otherwise it does not hold. No type-based alias fact exists: the kernel
   builds with `-fno-strict-aliasing`, and EvoIR has no effective types.
6. **Same address.** `same(p, q)` holds when the roots are the same and both
   exact offsets are equal.
7. **Callee reads and writes.** The least fixed point of §4.6, restricted to
   the `reads` and `writes` components and computed again by the checker from
   the module. The recorded `effects` clause is not trusted: the validator
   that compares it is shared code, and a shared defect here would fail
   dangerous. An external or indirect callee reads and writes everything.
8. **Writable.** A store cannot fault with E3014 when its pointer's root is an
   `alloca` or a global that is neither `readonly=true` nor a string literal.
9. **Guard facts.** §6.5 already keys the fact of a successful guard by its
   output token, its pointer, and its size and alignment. N06 uses that fact
   twice: as the token rule of a guard that ran (`m0_001`, `m0_002`), and as
   the premise of a later implied guard (`m0_200`, `m0_201`).

An alias class is therefore a pair of a root and an exact offset. Two
pointers with the same pair are one class (`m0_303`). Two classes are
disjoint only by item 5. Every other pair may alias.

### 3.5 Which operations commute

The question for a later load of `p` with size `n`, at the end of a run of
instructions: does its value equal the value that an earlier access read or
wrote?

| Instruction in the run | The load's value is unchanged when |
|---|---|
| `access_in_bounds`, `pointer_valid` | always |
| ordinary `load` | always |
| ordinary `store` to `q`, size `m` | `disjoint(p, n, q, m)` |
| ordinary `copy` into `d`, size `N` | `disjoint(p, n, d, N)` |
| `alloca` | `p` is one of the pointers of Section 3.4, item 3 |
| `call` whose callee writes nothing | always |
| pure scalar, UB predicate, `ub.assume`, `ptr.offset`, `global_addr` | always; a fault there ends both executions alike |
| `ptr.to_int` | always for a root that is fresh or global; never for an opaque root |
| writing `call`, `asm`, volatile or atomic access, fence, `va_*`, `tls_addr`, `alloca.dynamic`, `lifetime.end` | never in the first version |

A later guard on `(p, n2, a2)` is implied by an earlier guard on
`(p, n1, a1)` when `n2 <= n1`, `a2` divides `a1`, and the run between them has
only metadata-preserving instructions: guards, loads, stores, copies, pure
instructions and calls that write nothing. For a pointer with an opaque root,
the run also holds no `ptr.to_int` (the epoch of Section 3.4, item 4). A
`pointer_valid` on `p` is implied by an earlier `pointer_valid` or
`access_in_bounds` on `p` under the same run condition.

### 3.6 Rule families

Draft text of `m0.mem.v1`, in the style of §11.6. `[...]` holds attributes
that are part of the node key. `ran` names the non-root witnesses whose
success the rule uses. The digest is computed when the specification freezes
the text.

```text
m0_001_aib_token     access_in_bounds[n,a](?m,?p) => ?m when ran
m0_002_valid_token   pointer_valid(?m,?p) => ?m when ran
m0_003_load_token    load.mem[T,n,a](?m,?p) => ?m when ran
m0_004_call_token    call.mem[@f](?m,...) => ?m when ran,writes_nothing(@f)
m0_010_meta_store    meta(store.mem[n,a](?m,?p,?v)) => meta(?m) when ran
m0_011_meta_copy     meta(copy.mem[N,A](?m,?d,?s)) => meta(?m) when ran
m0_100_load_store    load[T,n,a](store.mem[n,a](?m,?p,?v),?p) => ?v when ran,sort(?v,T)
m0_101_load_store_x  load[T,n,a](store.mem[k,b](?m,?q,?v),?p) => load[T,n,a](?m,?p) when ran,disjoint(?p,n,?q,k)
m0_102_load_copy_x   load[T,n,a](copy.mem[N,A](?m,?d,?s),?p) => load[T,n,a](?m,?p) when ran,disjoint(?p,n,?d,N)
m0_103_load_alloca_x load[T,n,a](alloca.mem[S,A](?m),?p) => load[T,n,a](?m,?p) when ran,fresh_against(?p,alloca)
m0_200_aib_implied   access_in_bounds[n,a](?m,?p) => ?m when implied_access(?m,?p,n,a)
m0_201_valid_implied pointer_valid(?m,?p) => ?m when implied_valid(?m,?p)
m0_300_offset_bytes  ptr.offset.<d>.<s>[k](?p,?i) => ptr.offset.add.signed[1](?p,const64(delta)) when const(?i),fits_i64(delta)
m0_301_offset_join   ptr.offset.add.signed[1](ptr.offset.add.signed[1](?p,?c1),?c2) => ptr.offset.add.signed[1](?p,const64(c1+c2)) when ran,const(?c1),const(?c2),fits_i64(c1+c2)
m0_302_offset_zero   ptr.offset.add.signed[1](?p,const64(0)) => ?p when ran
m0_303_same_address  ?p => ?q when same(?p,?q)
m0_400_global_alias  global_addr[@a] => global_addr[@t] when alias_of(@a,@t)
```

Notes on the side conditions:

- A load node `load[T,n,a](M, P)` keys the type, the size and the alignment of
  its guard. Its children are the snapshot and the pointer. The world token is
  not a child: an ordinary load's value does not depend on it.
- `m0_100` needs the store to have run. A store of poison faults with E3001 in
  the EvoIR interpreter (`crates/evocc-evoir/src/lib.rs`, the `Store` arm) and
  in `Core.IR` (`Core/IRExecution.lean:116`), so a stored value that ran is
  never poison.
  The same type is required: no type punning, no partial overlap.
- `m0_101` to `m0_103` need the earlier instruction to have run. Without it
  the left side faults and the right side may not.
- `m0_103` needs item 3 of Section 3.4, not only a different root. A pointer
  loaded after the `alloca` may hold the new object's address. Then the left
  side faults with E3001 (uninitialized) and the right side with E3013
  (unknown object).
- `m0_200` and `m0_201` read a premise naming the implying guard node. That
  node is a non-root witness, and the two snapshots have one `meta` class.
- `m0_300` gives every constant `ptr.offset` spelling one canonical form: the
  mathematical byte delta (index decoded by its own signedness, times the
  scale, times the direction) as a signed 64-bit constant with scale 1. Both
  sides fault on a null base (E3009) and on an `i64` overflow of the final
  offset (E3011) under the same condition, so the equality is Kleene.
  `m0_301` and `m0_302` need `ran`: the inner `ptr.offset` can fault where
  the joined one does not, and `p + 0` faults on a null base where `p` does
  not.
- `m0_303` merges two SSA pointers with the same root and the same exact
  offset. It needs no `ran`: both are values.

What the families enable:

| Optimization | Rules | Deletion |
|---|---|---|
| Store-to-load forwarding | `m0_100`, then `m0_101` to `m0_103` past the run | the load, value renamed to the stored value |
| Redundant-load elimination and load CSE | congruence of two `load` nodes over one snapshot class and one pointer class | the later load, value renamed to the earlier value |
| Guard deletion | `m0_200`, `m0_201`, `m0_010`, `m0_011` | the guard, token renamed to its input |
| Pointer CSE | `m0_300` to `m0_303`, congruence | the `ptr.offset` with its `pointer_valid`, or the `global_addr` |
| Dead-store elimination | separate scan, Section 3.8 | the store |

### 3.7 Deletion units and reassembly

The validator ties some instructions together, so a deletion removes a unit.

| Unit | Instructions | Condition | Result map |
|---|---|---|---|
| U-load | one ordinary `load` | its value class holds a seed, a constant or the value of an earlier witness | value to that representative; `mem` and `world` to the load's own operands |
| U-access | `access_in_bounds` and the `load` that consumes its token | the guard's class holds its input token (`m0_200`), and U-load holds | as U-load; the guard's token to its input |
| U-guard | one `access_in_bounds` whose token no load or store consumes, or one `pointer_valid` that does not validate a `ptr.offset` result (§3.7 requires that pair) | its class holds its input token | token to its input |
| U-pointer | `ptr.offset` and the `pointer_valid` that follows it | the pointer class holds an earlier pointer value; the guard is implied | pointer to that value; token to the guard's input |
| U-global | one `global_addr` | its class holds an earlier `global_addr` | pointer to that value |

A representative must be defined earlier in the same block or be a block
argument. If it is itself deleted, its own representative applies. Positions
only decrease, so the map is acyclic.

Reassembly of a memory certificate:

1. Parse the module before (shared parser, which also validates).
2. Delete every root of every admitted region.
3. Rename each result through the region maps, transitively.
4. Recompute every function's effect summary by §4.6. Deleting the last load,
   guard or store of a function changes `reads`, `writes` or `may_trap`, and
   the validator accepts only the exact summary.
5. Renumber by §8.6 and render by §9.4.
6. Validate the result: guard pairing, token linearity and summaries.
7. Compare canonical bytes with the module after.

The persistent equality state (§8.9) gets no union from a memory
transaction. A deleted value loses its key, as §8.9 already says of a deleted
root, and its uses take the representative's existing binding. The P0 history
stays replayable by P0 rules alone.

### 3.8 Dead stores (`m1.dead.v1`)

Deleting a store `S` to `p` with size `n` changes the bytes of
`X = [p, p + n)` in every snapshot until a killer overwrites `X` or ends its
object. No equality of the e-graph states that. The checker therefore runs a
scan of the block for each claim:

1. **`S` cannot fault.** Its own guard precedes it on the same token. Its
   value is not maybe-poison. Its root is writable (Section 3.4, item 8). The
   guard stays.
2. **A killer exists.** Later in the block: stores with the same root and
   exact offsets whose union covers `X`; or the `ret` terminator when the root
   is an `alloca` of this function (a return ends every automatic object of
   the frame, §3.7). `lifetime.end` is a barrier in the first version, so it
   kills nothing.
3. **No observer runs before the killer.** Every instruction between `S` and
   the killer is a guard, a pure instruction, a store (any target), a load
   with `disjoint` from `X`, a copy whose source is disjoint from `X`, an
   `alloca`, or a call that writes nothing and whose callee reads nothing.
   A barrier stops the scan. A fault in that run ends both executions with
   the same diagnostic, and a fault exposes no memory (Section 5.2).

The claim record names `S` and its killers. A killer must survive the
transaction: a store that is itself claimed dead cannot kill another store in
the same certificate. Chains of dead stores go away in successive
transactions. The main targets are the zero-fill stores of
`-ftrivial-auto-var-init=zero` and the stores that forwarding has left without
a reader.

### 3.9 Block-chain merge (`c0.chain.v1`)

Inlined helpers end in a `br` to a continuation block with one predecessor.
In `theta`, five such edges cut the function into six blocks, and the
census shows that this hides 33 of the 35 redundant loads. The merge appends
the instructions of block `B` to block `A` when:

- `A` ends in `br ^B(args)`;
- `B` is not the entry block and is not `A`;
- exactly one edge in the function targets `B`, counting both edges of every
  `cond_br`, every `br.indirect` target, every `asm.goto` label and fall-through;
- no `block_addr` and no relocation names `B`.

`B`'s parameters become the edge arguments, `B` disappears, and values are
renumbered by §8.6. No instruction changes, so no effect summary changes. The
certificate lists the merged pairs; the checker counts the edges itself. The
transformation also removes the edge transports that out-of-SSA would make
for that edge, which N08 measures.

### 3.10 Definition seeds (`p1.seeded.v1`, `g1.seeded.v1`)

A value defined by an instruction outside the allowlist may be an opaque
scalar seed when its sort is `bits<1..64>` and it is not maybe-poison. The
seedable instructions are ordinary and volatile `load` (the value of a
completed load is never poison), `sdiv`, `udiv`, `srem`, `urem`, the seven
total UB predicates and `ptr.to_int`. Call results, `va_arg`, `vec.extract`,
`asm` results and atomic results stay excluded. Exclusion then applies to
instructions, not transitively to values. A seed's known-bits fact is top.

This is a semantic extension with a proof, not a relaxation for coverage.
The rule texts equal `p0.scalar.v4` and `g0.ub.v1`. New registry names and
digests select the new eligibility profile, as `g0.ub.v1` already selects its
own (§8.12, §11.6). A certificate with a definition seed names a seeded
registry; the old registries keep their meaning.

The S4 diagnostic (`docs/archive/notebook/eqsat-size-work-packages-20261001.md`)
used all value facts without the eligibility of §8.5. It bounds the guard
bytes that `g1` can certify: zlib 952, zstd 1,608, cJSON 80, against 256,
984 and 28 certified by `g0`. The constant shift counts of `rol64` in `theta`
are of this kind.

### 3.11 Barriers

- **Volatile.** Volatile loads, stores and `copy.volatile.*` are barriers. No
  memory rule names them, no unit deletes them, and their tokens are opaque
  seeds. The value of a volatile load may still be a scalar seed of `p1`
  (Section 3.10); the access itself never changes. The single-thread
  semantics would allow CSE across a volatile load, as GCC does. The first
  version does not, because the kernel's marked accesses are not modeled
  (owner decision OD4).
- **Atomic.** Every `atomic.*` access and fence is a barrier, relaxed ones
  included. §3.8 requires their order metadata to survive, and no N06 rule
  touches them.
- **`asm` and `asm.goto`.** Barriers. `asm.goto` ends a block anyway. §3.11
  keeps their results out of every class.
- **Calls.** A call is a barrier unless the checker's own fixed point shows
  that the callee writes nothing. Then its `mem` result equals its `mem`
  operand (`m0_004`). A callee that writes nothing allocates nothing, because
  `alloca` writes `{All}`.
- **Strict floating point.** `fp.strict.*` threads `world` and the floating
  environment, not memory. No rule merges a `world` token, and no unit deletes
  an operation whose world effect is observable (volatile, atomic, `io` or
  `fenv` in §4.2). A deleted ordinary load renames its world result to its own
  world operand. The order of every remaining world operation is therefore
  unchanged. Floating values are not seeds.
- **Others.** `va_start`, `va_arg`, `va_end`, `tls_addr`, `alloca.dynamic` and
  `lifetime.end` are barriers in the first version.

### 3.12 Extraction policy

The certificate proves equalities. The policy chooses deletions (§8, "Cost is
a policy"). The default policy `mem-default-v1`:

- deletes every implied `access_in_bounds` and `pointer_valid` (no native
  effect; fewer interpreter steps and smaller IR);
- deletes a load whose representative is a constant, a stored value or an
  earlier loaded value, while at most eight values are live only because of a
  deletion at any point, nearest representatives first;
- deletes a `ptr.offset` pair only for a variable index. For a constant
  index, immediate addressing in selection is the better fix. EvoMIR §5.4
  admits `[base, #offset]` only when nothing but constant materializations
  lies between the `ptr.offset` and the access after lowering (the guards
  lower to nothing), so in `theta` the loads of `st[k]` use
  `ldr x, [x0, #imm]` while the stores still build the address. CSE there would
  only lengthen a live range;
- reuses a `global_addr` under the same live budget.

Two measurement arms bound the policy: `mem-guards-only` and `mem-unbounded`.
The budget is a producer choice. The checker does not see it.

### 3.13 Outside the first version

Each item needs its own design note and owner decision:

- escape analysis, and with it loads past writing calls for private
  `alloca` objects (§3.7 then counts every `ptr.to_int` as an escape);
- forwarding through `copy` and deleting dead copies, which §3.8 forbids
  today; this overlaps N10;
- removing an `alloca`: `crates/evocc-memory/src/lib.rs` gives each object an
  address in allocation order (`pointer_address`), so removing an object
  changes the `ptr.to_int` result of later objects; N10 has the same problem
  (OD7);
- reuse across blocks with new edge arguments (dominator scopes);
- symbolic offsets (`a[i]` against `a[i+1]`);
- `f32`, `f64`, `f128` and vector values (each needs a round-trip lemma);
- CSE across volatile loads; relaxed atomics;
- loop-invariant loads.

## 4. Proof design

### 4.1 The Lean model and its anchors

A new namespace `EvoIR.Mem` holds a byte-level model of §5:

- `lean/EvoIR/Mem/Model.lean`: objects with identity, origin, size,
  alignment, owner, read-only mark, lifetime and bytes of
  `Option (BitVec 8 × Option PtrFrag)`; pointers as object identity and byte
  offset, plus exposed address; the exposed set; `access`, `valid`, `load`,
  `store`, `copy`, `alloc`, `endLifetime`, `offset` and `resolve`, each
  returning `Except Fault _` with the §5.4 order of faults.
- `Mem/Algebra.lean`: the lemmas of Section 4.2.
- `Mem/Rule.lean`, `Mem/Registry.lean`, `Mem/Proofs.lean`: the rule language,
  the registry generated from the specification text by an extension of
  `tools/lean/registry_to_lean.py`, and one theorem per rule.
- `Mem/Facts.lean`: soundness of the local pointer facts.
- `Mem/Bridge.lean`: on the overlap that `Core.IR` has (`allocate`, `access`,
  32- and 64-bit `load` and `store` at an object's offset), the model and
  `Core/IRExecution.lean` compute the same result.
- `Rewrite/Merge.lean`: the chain merge theorem over `Core.IR`.

The link to Rust is evidence, as A7 is for P0: a new binary
`evo-mem-vectors` in `crates/evocc-proofcheck` (like `evo-facts-vectors`)
runs random operation sequences on `evocc-memory`, and Lean replays them.
`tools/gate-lean.sh` runs the group. `tools/lean/audit.py` binds the new
digests in the receipt.

### 4.2 Statements per family

Sketches; names are indicative.

```lean
namespace EvoIR.Mem
-- T: tokens and metadata
theorem store_meta : store m p v = .ok m' → m'.meta = m.meta
theorem copy_meta  : copy m d s n = .ok m' → m'.meta = m.meta
-- G: guard implication (exposed set monotone, metadata equal)
theorem access_mono : access m p n a = .ok () → m'.meta = m.meta → m.exposed ⊆ m'.exposed →
    NoAmbiguity m' p → n' ≤ n → a' ∣ a → access m' p n' a' = .ok ()
theorem valid_of_access : access m p n a = .ok () → 0 < n → m'.meta = m.meta → valid m' p = .ok ()
-- R: reads (Kleene: equal `Except` results)
theorem load_store_same : store m p v = .ok m' → v.ty = t → load m' p t = .ok v
theorem load_store_other : store m q v = .ok m' → Disjoint p (size t) q (size v.ty) →
    load m' p t = load m p t
theorem load_copy_other : copy m d s k = .ok m' → Disjoint p (size t) d k → load m' p t = load m p t
theorem load_alloc_other : alloc m sz al o = (m', a) → p.obj ≠ a.obj → load m' p t = load m p t
-- P: pointers
theorem offset_bytes : offset p dir sgn scale i = offsetBytes p (decode sgn i * scale * dir)
theorem offset_join  : offsetBytes p c₁ = .ok q → offsetBytes q c₂ = offsetBytes p (c₁ + c₂)
-- D: dead stores
theorem store_cover : store m p v₁ = .ok m₁ → store m₁ p v₂ = .ok m₂ → v₁.size = v₂.size →
    store m p v₂ = .ok m₂
theorem insensitive : AgreeExcept X m₁ m₂ → InstrAvoids i X → exec i m₁ ≈[X] exec i m₂
-- S: definition seeds
theorem seedable_bits : evalOperation o op args = .ok (r :: _) → Seedable op →
    ∃ w x, r = .bits w x
```

The fixed statement of a memory rule mirrors `RuleSound`:

```lean
/-- In every environment where the `ran` nodes denote values and the side
condition holds of the runtime values, both sides denote the same partial
value. -/
def MRuleSound (r : MRule) : Prop :=
  ∀ σ : Var → MVal, Ran σ r → CondHolds σ r.cond → denote σ r.lhs = denote σ r.rhs
def MRuleCovered (r : MRule) : Prop := ∀ Γ : Var → MSort, Typed Γ r → MRuleSound r
```

The deletion lemma states D3: if every recorded merge is an instance of a
covered rule or a congruence, all members of a class denote one partial
value; so a root in a class with a defined member is defined and equal to
it. The fact lemmas state that a local root and an exact offset describe the
runtime pointer, and that a fresh unexposed `alloca` differs from every older
pointer. The chain merge theorem has the shape of `checkRewrite_refines`:
when `checkMerge before after cert` holds, `after` refines `before` with the
same entry state.

### 4.3 Certificate format `evp3`

`evp3` is `evp2` plus the records below. A registry admits only its own
record kinds. Every quantity stays bounded (T03l).

```text
version_line    := "evp3" newline
merge_line      := "merge " ident "/" ident "/" ident newline          ; c0: function/A/B, in §8.6 order
mregion_line    := "mregion " region_id " unit " evp_int " window " evp_int
                   " witnesses " evp_int " seeds " evp_int newline     ; m0: first root, unit size, window start
insert_line     := as evp2 ... " children=" children " result=" evp_int newline
delete_line     := "delete " evp_int " " region_id " " extraction newline   ; one per root; value or pointer representative
dead_line       := "dead " region_id " killer " (region_id ("," region_id)* | "ret") newline   ; m1
premise         := eclass ":" hex_digit+ ":" hex_digit+                 ; known bits (evp2)
                 | eclass ":ptr:" root ":" offset                       ; pointer fact
                 | "implies:" node_id                                   ; implying guard
root            := "s" evp_int | "a" evp_int | "g:" ident               ; seed, alloca position, global
offset          := "?" | ("+" | "-") evp_int
sort            := packed_type                                          ; now also mem<all>, ptr<generic>, fnptr<...>
effect_class    := "pure" | "partial" | "guard" | "effectful"            ; fixed by the operator table
```

`ran` is not recorded. The checker derives it from `source=before` and the
witness position. Token renames of a deleted root come from the operator
table, not from the certificate. Taint, callee summaries and read-only marks
are recomputed and never claimed.

### 4.4 What the independent checker verifies

1. Parse both modules; check the input digest and the registry name and
   digest.
2. Recompute maybe-poison, block pointer facts, exposure epochs, callee read
   and write sets (its own fixed point), global aliases and read-only marks.
3. Recompute each region's manifest from its unit and window: every
   allowlisted instruction in `[window, root)`, then the unit's roots.
4. Replay events: node shapes against witness instructions (operator, sort,
   attributes, children, result index); rule instances against its own
   `m0` table; `ran` only on non-root witnesses; premises against its own
   facts; congruence.
5. Check each deletion: the unit's shape, the root's success (a defined
   representative in the class; for a guard, its input token in its class),
   and the representative's position.
6. Check each `dead` claim with its own scan, and each `merge` with its own
   edge count and `block_addr` search.
7. Reassemble (Section 3.7), recompute summaries, validate, compare bytes.
8. Exit 2 for malformed or oversized input and 3 for a claim that fails.

### 4.5 Trusted base

New rows for §11.7:

| Item | Trusted object | Guarantor |
|---|---|---|
| A8 | Maybe-poison, pointer facts, exposure epochs and callee read and write sets | The producer's and the checker's separate implementations |
| A9 | The `evp3` deletion reassembly and summary recomputation | The checker; byte comparison with the validator's check |
| A10 | The Lean memory model and its agreement with `evocc-memory` | Differential vectors only (evidence, not proof) |
| A11 | The bridge between the memory model and `Core.IR` | Lean proof on the overlap; nothing outside it |

The shared parser stays shared, because a parser defect fails safe. The
summary fixed point of §4.6 is shared code that fails dangerous for `m0_004`
and for the dead-store scan, so the checker computes read and write sets
itself.

### 4.6 The proof-required path

N06 leaves it unchanged. The driver must refuse `-Oeq-mem` and the seeded
profiles with `--proof-required` as a refused admission (exit 1,
`spec/verification.md` §7.6), and §2.1 of that document must name the
refusal. This needs explicit code: audit row A52 shows that the driver admits
every EqSat option once `-Oeq-scalar` is present. `Rewrite.registryRules`
already returns `none` for an unknown registry, and the Lean checker refuses
root deletion. So a memory certificate fails closed even if it reaches the
stage.

`core-v1` has no pointer, array or aggregate, so `m0` and `m1` have nothing to
do on that path. Two families could enter it later, by owner decision (OD5):
`c0`, whose theorem is over `Core.IR` from the start, and `p1`/`g1`, whose Lean
change is a split of `badValues` (`lean/EvoIR/Rewrite/Flow.lean:123`) into
poison and exclusion plus `seedable_bits`. `m0` needs pointers in `core-v1`
first, and then a deletion theorem for `Rewrite.checkRewrite`.

## 5. Tests and measurement

### 5.1 Focused tests

Every negative test is checked twice: the producer declines, and a forged or
rehashed certificate makes the standalone checker exit 3 (or 2 when the
bytes are malformed). Fixtures go to `tests/fixtures/n06/`; Rust tests go to
`crates/evocc-eqopt/tests/n06_*.rs` and `crates/evocc-proofcheck/tests/n06_*.rs`.

| Family | Positive | Negative |
|---|---|---|
| T03v precondition | `zext.bits<32> %x` and `zext.bits<64> %x` stay two classes | loads of `bits<32>` and `bits<64>` at one address stay two classes |
| `p1`/`g1` | a load result seeds `and %x, -1 => %x`; `g1` folds `shift_count_defined %loaded, 63` | a call result, a `va_arg` result, an `uninitialized` value or a block argument fed by a call is never a seed |
| `c0` | a chain of three `br` blocks merges; an inlined helper's continuation merges | two predecessors; both `cond_br` edges to one block; a `block_addr` target; a `br.indirect` or `asm.goto` edge; the entry block; a self-loop |
| `m0` forwarding | store then load of the same pointer and type; forwarding past a store at a disjoint offset of the same root; past a store to another global; past a fresh `alloca` | different type or width; partial overlap; two parameters (may alias); a `ptr.to_int` of the `alloca` in the block; a writing call, `asm`, volatile or atomic access, fence or `lifetime.end` in the run |
| `m0` CSE | two loads over one snapshot class; the canonical `ptr.offset` of two spellings | snapshots split by a barrier; a `ran` rule on the root node; a `ran` rule on a volatile load |
| `m0` guards | implied `access_in_bounds` and `pointer_valid`; a smaller size and alignment | a guard deleted while its load stays, or a `pointer_valid` deleted while its `ptr.offset` stays (the module after fails validation); a larger size; a `lifetime.end`, a writing call, or a `ptr.to_int` with an opaque root between |
| `m0` reassembly | function summaries recomputed after the last load goes | a representative after the root; a representative from another block; a stale summary in the module after |
| `m1` | zero-fill store killed by full overwrite; killed by a covering pair of stores; killed by `ret` for an `alloca` | a store to a `readonly=true` global or a string literal; a load or copy source that may overlap; a call in the run; a killer that covers part of the range; a `ret` killer for a global; a poison value |

A mutation campaign covers the format and the optimizer, as §11.6 asks: one
rehashed mutant for each new status-2 and status-3 rule, and producer mutants
that ignore `disjoint`, apply `ran` to the root, or keep a guard pairing. The
property is the one of §11.6: no optimizer mutant yields a semantically
different module with an accepted certificate.

### 5.2 Randomized differential gate

`crates/evocc-randgate` gets a third generator mode, `Memory`, with its own
seed space tag, so the two pinned seed spaces do not change. A seed draws:
static `alloca` objects of 8 to 64 bytes, two to four globals (one read-only,
one string literal), pointers to them through `global_addr` and helper
parameters, `ptr.offset` with constant and variable indices, guarded loads
and stores of `bits<8..64>` and pointers, copies, zero-fill runs, calls to
helpers that write nothing and to helpers that write a global, volatile
accesses, `ptr.to_int` and `ptr.from_int`, `lifetime.end`, chains of
single-predecessor blocks, diamonds and short loops. About one seed in five
faults on purpose (out of bounds, uninitialized read, dangling access).

The oracle is stricter than today's, which discards undefined programs:

- the before run is defined: the after run is defined with equal returns,
  volatile trace and statics;
- the before run faults: the after run faults with the same diagnostic code
  and the same volatile trace prefix (`Observation::Undefined` gains the
  trace);
- the before run exhausts fuel or an object limit: the trial is discarded and
  counted, because deletions lower resource use.

Every certificate goes to the standalone checker binary. Campaign per family:
20,000 seeds with zero mismatches, and at least 30% of seeds with at least one
deletion, so the campaign is not vacuous. The shrinker reduces any failure.
The 384-case Linux native corpus runs OFF and ON. Darwin has no EqSat lane.

### 5.3 Measuring the benefit without timing

Each milestone reports these deterministic counts. No timing run measures a
benefit. The compile-cost bound of the N03 slices stays a resource gate only.

1. **EvoIR counts** per unit from the producer report: loads, stores, guards,
   `ptr.offset` before and after; deletions by unit kind; declines by reason
   (budget, barrier, window, policy).
2. **S1 object bytes** for the six S5 units, checked and unchecked, by family
   and never as a cross-family average. N01 category deltas for `memory`,
   `compute`, `constants` and `branch`. Zero regression per unit.
3. **Interpreter steps** on the N05 measurement inputs, in trend receipts.
4. **Kernel reductions:** `tools/k1/perf/reduction-counts.py` with `tbprof`
   gives executed instructions per call for the five reductions, built with
   the kernel's flags plus `-Oeq-scalar -Oeq-mem`, against OFF and GCC 14.
   Estimate for `theta`: about 329 down to about 240 (ten `bc` loads, ten
   stores, about 28 guard instructions, about 40 address instructions). The
   three transport and call reductions should not move.
5. **K02 profile:** only after OD6. Rerun the equal-work demand phase and add
   two classes to `tools/k1/perf/attribute.py`: `mem` (loads and stores whose
   base is not `sp` or `x29`) and `ubguard` (compare and branch to a trap
   block).
6. **Ladder programs** (zlib, zstd, cJSON and the rest under `.build/ladder`):
   result checks unchanged, S1 bytes, and executed instructions under QEMU
   user mode with the same plugin after an A/A check of two equal runs.
7. **Certificate cost:** nodes, events, certificate bytes and checker replay
   events per unit.

## 6. Milestones

Each milestone follows the working agreement of the codegen quality plan: a
frozen plan `tests/contracts/codegen-n06-<m>-YYYYMMDD.json`, the RED, the code
behind a flag, the gates, a one-page result
`docs/archive/notebook/codegen-n06-<topic>-YYYYMMDD.md`, a compact receipt and
the TODO update in one commit. Default output stays unchanged; `-Oeq-mem` is
opt-in. Line counts are estimates of new or changed lines.

| M | Scope | Rust | Rust tests | Lean | Spec | Depends on |
|---|---|---:|---:|---:|---:|---|
| M0 | Preconditions: T03v (sort keys), T03l (bounded checker), T03m (closure test), T03w (§8 and §11 errata) | owned by T03 | | | | — |
| M1 | Definition seeds: `p1.seeded.v1`, `g1.seeded.v1` | 450 | 650 | 300 | 150 | M0 |
| M2 | Block-chain merge: `c0.chain.v1`, `evp3` merge records | 500 | 450 | 450 | 120 | M0 |
| M3a | Memory preflight: diagnostic producer and stop rule | 600 | 150 | 0 | 60 | M2 |
| M3 | `m0.mem.v1`: forwarding, CSE, guards, pointers; `evp3` regions; Lean model | 4,650 | 2,600 | 2,700 | 650 | M1, M2, M3a passed |
| M4 | `m1.dead.v1`: dead stores | 800 | 600 | 700 | 150 | M2 (zero-fill), M3 (stores left by forwarding) |
| M5 | Later families of Section 3.13 | one design note each | | | | owner |
| M6 | Proof-required path: `c0`, then `p1`/`g1`, then `m0` | 300 | 300 | 1,500–2,500 | 100 | owner, `core-v1` pointers for `m0` |

M1 to M4 total about 7,000 Rust lines, 4,450 test lines, 4,150 Lean lines and
1,130 specification lines.

**Files.** Other agents work in the same tree. N06 creates new modules and
touches shared files only where its milestone needs them.

| Kind | Paths |
|---|---|
| New, owned by N06 | `crates/evocc-eqopt/src/memory/`, `crates/evocc-eqopt/src/chain_merge.rs`, `crates/evocc-proofcheck/src/memory/`, `crates/evocc-proofcheck/src/chain_merge.rs`, `crates/evocc-proofcheck/src/bin/evo-mem-vectors.rs`, `lean/EvoIR/Mem/`, `lean/EvoIR/Rewrite/Merge.lean`, `tests/fixtures/n06/`, `crates/*/tests/n06_*.rs` |
| Shared, coordinate first | `crates/evocc-proof/src/evp1.rs` (format; T03l), both `eligibility.rs` files (M1), `crates/evocc-egraph/src/lib.rs` and `crates/evocc-eqopt/src/persistent.rs` (T03v), `crates/evocc-eqopt/src/chain.rs`, `crates/evocc/src/main.rs` (driver flag and proof-required refusal), `crates/evocc-randgate/src/generate.rs` (new seed space), `lean/EvoIR/Rewrite/Flow.lean` and `Rules.lean` (M1), `spec/evoir.md` and `spec/verification.md` (after T03w), `tools/lean/registry_to_lean.py`, `tools/lean/audit.py`, `tools/gate-lean.sh`, `tools/k1/perf/attribute.py` |

**M1 exit.** The EvoIR revision prints both registries with digests and the
eligibility profile. Producer and checker compute maybe-poison and definition
seeds independently. Lean: the `badValues` split, `seedable_bits`, and
`registryRules` entries that reuse the existing `RuleCovered` theorems;
`tools/gate-lean.sh` green. The existing seed spaces run the `p1`/`g1` chains
with zero mismatch. The six S5 units report S1 bytes for the arms `p0.v4`,
`p1`, `g0` and `g1`, with the `g1` gain shown against the S4 ceiling. Stop
rule, frozen before the run: if `g1` recovers less than 10% of the gap between
`g0` and the S4 ceiling in every family and `p1` changes no unit, close M1
`NO_OBSERVABLE_EFFECT`.

**M2 exit.** The checker counts edges itself; all `c0` negatives pass;
`Rewrite/Merge.lean` proves the merge theorem; randomized chains agree; block
counts before and after for the six units and five reductions; S1 bytes and
reduction counts recorded.

**M3a exit.** A diagnostic producer, marked `UNVERIFIED_DIAGNOSTIC` and never
linked, executed or installed (the S4 rule), reports deletable units and S1
bytes. Stop rule, frozen before the run: if every library family saves less
than 0.25% of its text and `theta` loses less than 15% of its static loads and
stores, close `m0` as `NOT_ADMITTED` and go to M4 for zero-fill stores only.

**M3 exit.** Every `m0` rule has an `MRuleCovered` theorem; at least 100,000
memory vector sequences with zero differences; the bridge lemma; the hostile
certificate matrix; the memory campaign of Section 5.2; the native corpus and
the 72 executions of the six unit harnesses with twins; ladder result checks;
the counts of Section 5.3 with zero size regression per unit; the compile-cost
bound.

**M4 exit.** `m1` scan and Lean insensitivity lemmas; the `m1` negatives; the
zero-fill stores of `theta` and the struct reduction gone; reduction counts
rerun.

## 7. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| The Lean memory model drifts from `evocc-memory` | A proof about the wrong semantics | Vectors in the Lean gate; bridge lemma to `Core.IR`; exposure and pointer fragments in the model from M3 |
| Shared code that fails dangerous | Producer and checker agree on an unsound deletion | Independent facts and write sets in the checker (A8); mutation campaign |
| Exposed-address pointers | A forwarding hides an ambiguity fault | The `epoch` attribute of Section 3.4, item 4; the strict oracle; `ptr.to_int` and `ptr.from_int` in the generator |
| Register pressure | CSE spills more than it saves | The live budget of `mem-default-v1`; zero-regression rule; `mem-unbounded` arm |
| Certificate size on large kernel blocks | Slow replay, large evidence | Window cap per region; bounded parsing (T03l); certificate bytes reported |
| Little native gain in library code | Effort without product effect | M3a stop rule; library ceiling of a few hundred bytes per unit is stated now |
| Overlap with N08, N09, N10 | Duplicate work or file conflicts | N06 owns new modules only; it measures after N09 and N10 land, or in both arms |
| Concurrency in the kernel | CSE of a racy ordinary load | Volatile and atomic accesses are barriers; `asm` memory barriers are barriers |
| Specification churn | Conflicting edits with T03w | N06 edits §4.5, §5.5, §7.2, §8.13 onward and §11 after T03w lands |

## 8. Owner decisions

- **OD1.** Approve definition seeds as a semantic extension in new
  registries. The codegen plan froze `p0.scalar.v4`, and the guard slice said
  that no admission relaxation follows.
- **OD2.** Approve the EvoIR revision that lifts §5.5 for the named families,
  rewrites §4.5 and §7.2, adds §8.13 to §8.16, `evp3`, five registries and
  four trusted-base rows. Choose a major or a minor revision.
- **OD3.** Keep strict trap preservation (recommended) or switch to
  refinement modulo memory UB, which would let N06 delete loads that may
  fault.
- **OD4.** Keep volatile accesses as full barriers (recommended), or allow
  CSE across volatile loads as GCC does.
- **OD5.** Keep `c0` inside N06 or make it an item that N08 and N09 can use
  earlier. Decide whether `c0` and then `p1`/`g1` enter the proof-required
  path.
- **OD6.** Decide whether kernel builds enable `-Oeq-mem` (today
  `protects_codegen()` turns opt-in variants off, and R01/G2 owns kernel
  EqSat), and whether to spend a serial VM-lane K02 rerun.
- **OD7.** Keep allocation-order object addresses (then no `alloca` removal in
  N06, and N10's promotion faces the same question) or make addresses
  nondeterministic in the semantics.
- **OD8.** Accept a producer-side live budget now and leave pressure-aware
  extraction to R03e.
- **OD9.** Raise `max_transactions` from 8 to 16 for mixed chains
  (`c0`, `p1`, `g1`, `m0`, `m1`).
- **OD10.** Set the N10 boundary: N10 promotes one-word records and
  constant-indexed arrays; N06 does not forward through `copy` in its first
  version.

## Appendix A: census method

Commands (frozen compiler, macOS host, `--target=aarch64-unknown-linux-gnu`,
`--sysroot=sysroot/aarch64-unknown-linux-gnu`, `-O2`):

- zlib 1.3.2 `inflate.c`, `deflate.c` with `-I` of the source directory;
  cJSON 1.7.19 `cJSON.c`; zstd 1.5.7 `zstd_compress.c` and
  `zstd_decompress.c` with `-I lib -I lib/common -DZSTD_DISABLE_ASM`; sources
  from `.build/ladder/`.
- The five reductions of `tests/fixtures/kernel-codegen-reductions/` with
  `-ftrivial-auto-var-init=zero -fno-strict-aliasing`; the struct reduction
  also with `-fstack-protector-strong`.

These are not the frozen S5 build profiles or the kernel's full flag set. The
counts estimate the shape of the opportunity, not its size in a receipt.

The census parses each block, computes the local facts of Section 3.4, and
walks back from each ordinary load along the block (or the merged chain) until
it finds a store of the same address and type (forwarding), a load of the
same address and type (CSE), or an instruction of Section 3.5 that may change
the value. Guards are implied when an earlier guard covers the same address
with no metadata barrier between. Definition seeds follow Section 3.10. The
script stayed in the scratch directory; M3a re-implements it as a frozen
preflight.

EvoIR text identities (SHA-256, first 16 hex digits): `theta`
`3bbc2a53df6c4d3d`, struct reduction `9bdc2f32ceacce13`, loop
`a5a9570a9617ead1`, `asm goto` `c4330c86ef3d3193`, leaf helper
`e3c5bff4316d744c`, inflate `22920005746e4934`, deflate `6f820e4afd2c4131`,
cJSON `5ef426e33adc5ab5`, zstd compress `cf2f3d72839d2bfd`, zstd decompress
`86603093a58e45d8`.

## Appendix B: worked example, `theta`

After `c0`, the function is one block. A reduced excerpt:

```text
(%v3, %v4, %v5) = alloca %v1, %v2 [origin=1, size=40, align=8]       ; bc
...
%v41 = ptr.offset.add.signed %v0, %v38 [scale=8]                      ; st + 0, %v38 = 0
%v42 = pointer_valid %v40, %v41
%v43 = access_in_bounds %v42, %v41 [size=8, align=8]
(%v44, %v45, %v46) = load %v43, %v33, %v41 : bits<64>                  ; L1 = st[0]
...
(%v80, %v81) = store %v79, %v77, %v39, %v78                            ; bc[0] = ...
...
%v285 = ptr.offset.add.signed %v0, %v284 [scale=8]                    ; st + 0 again
%v286 = pointer_valid %v281, %v285
%v287 = access_in_bounds %v286, %v285 [size=8, align=8]
(%v288, %v289, %v290) = load %v287, %v282, %v285 : bits<64>           ; L2 = st[0]
```

- `m0_300` gives `%v41` and `%v285` one canonical node, so their classes
  merge. Under `mem-default-v1` both `ptr.offset` instructions stay, because
  the index is constant.
- Every `bc` store between L1 and L2 has a fresh unexposed root (`%v3`), and
  `%v0` is a block argument, so `m0_101` carries the load past each of them.
  The guards and loads in the run merge their tokens by `m0_001` and `m0_003`.
- L2's node is then congruent with L1's. The policy declines most of these
  deletions: the 25 `st[k]` values would exceed the live budget of eight. The
  `mem-unbounded` arm measures the other choice.
- Both loads of each `bc[k]` forward from the `bc[k]` store by `m0_100`. They
  need five values, within the budget, so U-access deletes each load with its
  guard. The store's guard implies the load's guard by `m0_200`, because the
  two `ptr.offset` spellings share one canonical class.
- After forwarding, no load reads `bc`. M4 then deletes the five zero-fill
  stores (killed by the `bc[k]` stores) and the five `bc[k]` stores (killed by
  `ret`). The `alloca` stays (OD7).
- M1 lets `g1` fold the two shift-count guards of each `rol64`, because the
  shifted operand is now a load seed.
