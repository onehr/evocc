"""Regression for the observed whole-body theorem audit mismatch."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import audit


class WholeBodyAuditTests(unittest.TestCase):
    name = "EvoIR.Core.AbsCorrespondence.pilot_pair_refines"
    # Actual pinned Lean output: inferable existential binder types are omitted.
    printed = (
        "∀ (s : EvoIR.Core.Source.Program) (t : EvoIR.Core.IR.Program), "
        "EvoIR.Core.AbsCorrespondence.checkPilotPair s t = true → ∀ (x : BitVec 32), "
        "∃ s₀ t₀, "
        "EvoIR.Core.Source.initial s 0 [{ type := EvoIR.Core.Source.IntType.int, bits := x }] = Except.ok s₀ ∧ "
        "EvoIR.Core.IR.initial t \"abs\" [EvoIR.Core.IR.Value.bits 32 x] = Except.ok t₀ ∧ "
        "EvoIR.Core.LoweringRefines s t EvoIR.Core.Source.IntType.int s₀ t₀"
    )

    def check(self, proposition):
        return audit.target_entry(
            {self.name: {"type": proposition, "axioms": []}}, self.name,
            dict(audit.CORE_PRIMITIVE_TARGETS)[self.name],
        )

    def test_pinned_whole_body_type(self):
        self.assertEqual(self.check(self.printed)["proof_kind"], "kernel-only")

    def test_changed_obligation_still_refuses(self):
        for changed in (
            self.printed.replace("= true →", "= false →"),
            self.printed.replace("BitVec 32", "BitVec 64"),
            self.printed.replace('t "abs"', 't "other"'),
            self.printed.replace("LoweringRefines s t", "LoweringRefines s s"),
        ):
            with self.subTest(proposition=changed), self.assertRaises(audit.AuditFailure):
                self.check(changed)


if __name__ == "__main__":
    unittest.main()
