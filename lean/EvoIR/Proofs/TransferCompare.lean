import EvoIR.Proofs.Bits

/-!
# The comparison rows of the transfer table of Section 8.10

The rows `icmp.eq`, `icmp.ne`, `icmp.ult`, `icmp.ule`, `icmp.slt` and
`icmp.sle` are each enumerated over `N` in `1 .. 64` by `bv_decide`; each is 64
theorems named `<row>_1` through `<row>_64`.
Each `bv_decide` sets a SAT timeout of 60 s, not the default 10 s: a parallel
build on a busy host can stretch a 3 s query past 10 s.

The rewrite checker does not use these rows, so `evoir-sem` does not import this
module. The umbrella `EvoIR` imports it for the audit.

Section 8.5 admits `bits<N>` with `1 <= N <= 64` for a root or witness, so each
enumerated row covers exactly the instance space the profile admits.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR

/-! ## The comparison rows, enumerated over the widths Section 8.5 admits

Equality is decided by a bit known in both with different values, or by both
operands being fully known; the ordered comparisons by the interval rule, the
signed ones only when both sign bits are known.  Each `split` is one branch of
that procedure, and the `top` branch is sound because `top` says nothing. -/

each_width transfer_icmp_eq over (Op.cmp CmpOp.eq) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i h1
  · bv_decide (timeout := 60)
  · split <;> rename_i h2
    · obtain ⟨e1, e2⟩ := h2
      subst e1; subst e2
      bv_decide (timeout := 60)
    · bv_decide (timeout := 60)

each_width transfer_icmp_ne over (Op.cmp CmpOp.ne) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i h1
  · bv_decide (timeout := 60)
  · split <;> rename_i h2
    · obtain ⟨e1, e2⟩ := h2
      subst e1; subst e2
      bv_decide (timeout := 60)
    · bv_decide (timeout := 60)

each_width transfer_icmp_ult over (Op.cmp CmpOp.ult) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i h1
  · bv_decide (timeout := 60)
  · split <;> rename_i h2
    · bv_decide (timeout := 60)
    · bv_decide (timeout := 60)

each_width transfer_icmp_ule over (Op.cmp CmpOp.ule) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i h1
  · bv_decide (timeout := 60)
  · split <;> rename_i h2
    · bv_decide (timeout := 60)
    · bv_decide (timeout := 60)

each_width transfer_icmp_slt over (Op.cmp CmpOp.slt) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i hs
  · obtain ⟨s1, s2⟩ := hs
    split <;> rename_i h1
    · bv_decide (timeout := 60)
    · split <;> rename_i h2
      · bv_decide (timeout := 60)
      · bv_decide (timeout := 60)
  · bv_decide (timeout := 60)

each_width transfer_icmp_sle over (Op.cmp CmpOp.sle) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i hs
  · obtain ⟨s1, s2⟩ := hs
    split <;> rename_i h1
    · bv_decide (timeout := 60)
    · split <;> rename_i h2
      · bv_decide (timeout := 60)
      · bv_decide (timeout := 60)
  · bv_decide (timeout := 60)

end EvoIR
