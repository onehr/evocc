"""The registry decoder must refuse anything the specification does not say.

The generator is trusted (A6 of the plan's section 5): it decides which rule
the Lean theorems are about.  These tests exercise the two checks that make it
worth trusting -- the digest the specification prints beside the block, and the
byte-for-byte round trip from decoded values back to the block -- plus the
refusals that keep an operator outside Section 8.5 out of the data.
"""

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location(
    "registry_to_lean", ROOT / "tools" / "lean" / "registry_to_lean.py"
)
gen = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = gen
spec.loader.exec_module(gen)

SPEC = (ROOT / "docs" / "evoir-spec.md").read_text(encoding="utf-8")


class ExtractionTest(unittest.TestCase):
    def test_the_block_hashes_to_the_digest_the_specification_prints(self):
        import hashlib

        block, stated = gen.extract_block(SPEC, "p0.scalar.v4")
        self.assertEqual(hashlib.sha256(block.encode("utf-8")).hexdigest(), stated)

    def test_every_reviewed_registry_of_the_specification_hashes(self):
        import hashlib

        for name in ["p0.scalar.v1", "p0.scalar.v2", "p0.scalar.v3", "p0.scalar.v4"]:
            block, stated = gen.extract_block(SPEC, name)
            self.assertEqual(
                hashlib.sha256(block.encode("utf-8")).hexdigest(),
                stated,
                f"{name} does not hash to its stated digest",
            )

    def test_an_unprinted_registry_is_refused(self):
        with self.assertRaises(gen.RegistryError):
            gen.extract_block(SPEC, "p0.scalar.v9")


class RoundTripTest(unittest.TestCase):
    def test_the_decoded_rules_render_back_to_the_block(self):
        block, _ = gen.extract_block(SPEC, "p0.scalar.v4")
        lines = [line for line in block.split("\n") if line]
        self.assertEqual(len(lines), 55)
        rendered = "".join(gen.parse_rule(line).render() + "\n" for line in lines)
        self.assertEqual(rendered, block)

    def test_each_rule_shape_survives_the_round_trip(self):
        samples = [
            "p0_009_double_neg sub(const_like(?x,0),sub(const_like(?x,0),?x)) => ?x when true",
            "p0_010_icmp_eq_self icmp.eq(?x,?x) => bool(1) when true",
            "p0_106_fold_icmp_eq icmp.eq(?a,?b) => fold.icmp.eq(?a,?b) when all_const",
            "p1_206_known_shl shl(?a,?k) => known_const when fully_known",
            "p1_230_known_zext zext(?a) => known_const when fully_known",
            "p1_220_and_cover_rhs and(?x,?m) => ?x when covers(?m,?x)",
            "p1_224_xor_known_zero_rhs xor(?x,?z) => ?x when known_zero(?z)",
            "p2_302_add_self_shl add(?x,?x) => shl(?x,const_like(?x,1)) when width_min(?x,2)",
        ]
        for line in samples:
            self.assertEqual(gen.parse_rule(line).render(), line)


class RefusalTest(unittest.TestCase):
    def test_an_operator_outside_the_allowlist_is_refused(self):
        for line in [
            "p9_000_div sdiv(?a,?b) => known_const when fully_known",
            "p9_001_ctlz ctlz(?a) => known_const when fully_known",
        ]:
            with self.assertRaises(gen.RegistryError):
                gen.parse_rule(line)

    def test_a_fold_builder_for_a_shift_is_refused(self):
        with self.assertRaises(gen.RegistryError):
            gen.parse_rule("p9_002 shl(?a,?k) => fold.shl(?a,?k) when all_const")

    def test_an_unknown_side_condition_is_refused(self):
        with self.assertRaises(gen.RegistryError):
            gen.parse_rule("p9_003 add(?a,?b) => ?a when nonzero(?b)")

    def test_a_non_canonical_integer_is_refused(self):
        with self.assertRaises(gen.RegistryError):
            gen.parse_rule("p9_004 add(?x,const_like(?x,00)) => ?x when true")

    def test_a_wrong_arity_is_refused(self):
        with self.assertRaises(gen.RegistryError):
            gen.parse_rule("p9_005 add(?a) => ?a when true")


FIRST = "p0_001_add_zero_rhs add(?x,const_like(?x,0)) => ?x when true\n"
SECOND = "p0_002_add_zero_lhs add(const_like(?x,0),?x) => ?x when true\n"


def digest_of(text: str) -> str:
    import hashlib

    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def synthetic_spec(prev: str, later: str, prev_digest=None, later_digest=None) -> str:
    """A specification printing two reviewed registries, in the shape Section
    11.6 uses."""

    def block(name: str, body: str, stated: str) -> str:
        return (
            "**Reviewed registry `"
            + name
            + "`.** Its digest is `"
            + stated
            + "`, computed as for the earlier registries:\n\n```text\n"
            + body
            + "```\n\n"
        )

    return block("p0.test.prev", prev, prev_digest or digest_of(prev)) + block(
        "p0.test", later, later_digest or digest_of(later)
    )


class DigestGuardTest(unittest.TestCase):
    """A block whose bytes were changed must stop the generator."""

    def run_generator(self, spec_text: str, out: Path) -> int:
        with tempfile.TemporaryDirectory() as tmp:
            spec_path = Path(tmp) / "spec.md"
            spec_path.write_text(spec_text, encoding="utf-8")
            return gen.main(
                [
                    "--spec",
                    str(spec_path),
                    "--registry",
                    "p0.test",
                    "--prefix-registry",
                    "p0.test.prev",
                    "--out",
                    str(out),
                ]
            )

    def test_a_changed_rule_stops_the_generator(self):
        later = FIRST + SECOND
        tampered = later.replace("const_like(?x,0)", "const_like(?x,1)")
        text = synthetic_spec(FIRST, tampered, later_digest=digest_of(later))
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "Registry.lean"
            self.assertEqual(self.run_generator(text, out), 2)
            self.assertFalse(out.exists())

    def test_an_earlier_registry_that_is_not_a_prefix_stops_the_generator(self):
        text = synthetic_spec(SECOND, FIRST + SECOND)
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "Registry.lean"
            self.assertEqual(self.run_generator(text, out), 2)
            self.assertFalse(out.exists())

    def test_a_matching_block_is_written(self):
        text = synthetic_spec(FIRST, FIRST + SECOND)
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "Registry.lean"
            self.assertEqual(self.run_generator(text, out), 0)
            written = out.read_text(encoding="utf-8")
        self.assertIn("def rule_p0_001_add_zero_rhs : Rule :=", written)
        self.assertIn('.bin .add false (.var "x") (.constLike "x" 0)', written)
        self.assertIn("def registry : List Rule :=", written)
        self.assertIn('def registryPrefixName : String := "p0.test.prev"', written)
        self.assertIn("def registryPrefixCount : Nat := 1", written)
        self.assertNotIn("theorem", written)
        self.assertNotIn("Prop", written)

    def test_a_second_digest_beside_the_block_is_refused(self):
        block = "p0_001_add_zero_rhs add(?x,const_like(?x,0)) => ?x when true\n"
        zero = "0" * 64
        text = (
            "**Reviewed registry `p0.test`.** Its digest is `"
            + zero
            + "` or maybe `"
            + zero
            + "`:\n\n```text\n"
            + block
            + "```\n"
        )
        with self.assertRaises(gen.RegistryError):
            gen.extract_block(text, "p0.test")


class GeneratedFileTest(unittest.TestCase):
    def test_the_checked_in_registry_matches_the_generator(self):
        code = gen.main(
            [
                "--spec",
                str(ROOT / "docs" / "evoir-spec.md"),
                "--registry",
                "p0.scalar.v4",
                "--out",
                str(ROOT / "lean" / "EvoIR" / "Registry.lean"),
                "--check",
            ]
        )
        self.assertEqual(code, 0)


if __name__ == "__main__":
    unittest.main()
