"""N07 E3 must not turn a bounded translator escape into a proof."""
import unittest

from tools.isa_model_smt import QUERY, property_source, solver_verdict


class BoundedModelTest(unittest.TestCase):
    def test_query_disallows_observed_overflow_and_assumption_escapes(self):
        self.assertIn("~(exist overflow)", QUERY)
        self.assertIn("all assumption", QUERY)
        self.assertIn("~(exist match_failure)", QUERY)
        self.assertNotIn("|", QUERY)

    def test_only_successful_exact_unsat_is_a_proof(self):
        self.assertEqual(solver_verdict(0, b"unsat\n"), "UNSAT")
        self.assertEqual(solver_verdict(0, b"sat\n"), "SAT")
        self.assertEqual(solver_verdict(0, b"unknown\n"), "UNKNOWN")
        for code, output in [(1, b"unsat\n"), (0, b""),
                             (0, b"unsat\nsat\n"), (0, b"(error bad)\nunsat\n")]:
            with self.subTest(code=code, output=output):
                self.assertEqual(solver_verdict(code, output), "SOLVER_ERROR")
        self.assertEqual(solver_verdict(124, b""), "TIMEOUT")

    def test_symbolic_state_and_source_domain_are_explicit(self):
        source = property_source("model_shl_2", [0x1ac12000])
        self.assertIn("unsigned(bv) < 2", source)
        self.assertIn("b[1..0]", source)
        self.assertIn("PSTATE == initial_pstate", source)
        for register in range(2, 31):
            self.assertIn(f"R{register} == initial_gprs[", source)
        self.assertIn("catch { _ => false }", source)
        self.assertIn("__DecodeA64(0, 0x1ac12000)", source)

    def test_preservation_conjunction_avoids_observed_overload_explosion(self):
        source = property_source("model_add_64", [0x8b010000])
        self.assertEqual(source.count(" : bool = R"), 30)
        self.assertEqual(source.count("correct = and_bool(correct, preserved_"), 30)
        self.assertIn("let preserved_29 : bool = PSTATE == initial_pstate;", source)
        self.assertNotIn("R2 == initial_gprs[63..0] &", source)


if __name__ == "__main__":
    unittest.main()
