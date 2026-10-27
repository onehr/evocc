import EvoIR.Proofs.Bits

/-!
# The carry rows `add` and `sub` of the transfer table of Section 8.10

Each row is enumerated over `N` in `1 .. 64` by `bv_decide`; each is 64
theorems named `<row>_1` through `<row>_64`.
Each `bv_decide` sets a SAT timeout of 60 s, not the default 10 s: a parallel
build on a busy host can stretch a 3 s query past 10 s.

The rewrite checker does not use these rows, so `evoir-sem` does not import this
module. The umbrella `EvoIR` imports it for the audit. The `mul` row is in
`Proofs/TransferMul.lean`.

Section 8.5 admits `bits<N>` with `1 <= N <= 64` for a root or witness, so each
enumerated row covers exactly the instance space the profile admits.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR

/-! ## The carry rows, enumerated over the widths Section 8.5 admits

The mask is the trailing run of bits known in both inputs; the value is the
sum or difference of the known values inside that run.  The argument is carry
propagation, which `bv_decide` settles at each width. -/

each_width transfer_add over (Op.bin BinOp.add) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_add, carryMask, Fact.mk', denoteOp, Op.res] at *
  bv_decide (timeout := 60)

each_width transfer_sub over (Op.bin BinOp.sub) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_sub, carryMask, Fact.mk', denoteOp, Op.res] at *
  bv_decide (timeout := 60)

end EvoIR
