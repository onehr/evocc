import EvoIR.Proofs.Bits

/-!
# The carry row `mul` of the transfer table of Section 8.10

The row is enumerated over `N` in `1 .. 64` by `bv_decide`: 64 theorems named
`transfer_mul_1` through `transfer_mul_64`.
Each `bv_decide` sets a SAT timeout of 60 s, not the default 10 s: a parallel
build on a busy host can stretch a 3 s query past 10 s.

This module elaborates its theorems one at a time (`Elab.async false`). Their
certificates are about 1.5 GB. With parallel elaboration on four threads, the
module needed about 13 GiB of memory to build. One at a time, it needs about
7 GiB, and its build takes about 140 s instead of 90 s.

The rewrite checker does not use these rows, so `evoir-sem` does not import this
module. The umbrella `EvoIR` imports it for the audit.

Section 8.5 admits `bits<N>` with `1 <= N <= 64` for a root or witness, so each
enumerated row covers exactly the instance space the profile admits.
-/

set_option linter.unusedSimpArgs false

-- Elaborate the 64 theorems one at a time. See the module docstring.
set_option Elab.async false

namespace EvoIR

/-! ## The carry rows, enumerated over the widths Section 8.5 admits

The mask is the trailing run of bits known in both inputs; the value is the
sum or difference of the known values inside that run.  The argument is carry
propagation, which `bv_decide` settles at each width. -/

each_width transfer_mul over (Op.bin BinOp.mul) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_mul, lowbit0, Fact.mk', denoteOp, Op.res] at *
  bv_decide (timeout := 60)

end EvoIR
