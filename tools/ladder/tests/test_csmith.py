"""The generator options and the checksum reading must be the intended ones."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest

_spec = importlib.util.spec_from_file_location(
    "ladder_csmith", Path(__file__).resolve().parents[1] / "csmith.py"
)
csmith = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(csmith)


class OptionTest(unittest.TestCase):
    def test_base_campaign_refuses_float(self):
        self.assertIn("--no-float", csmith.csmith_options("base"))

    def test_base_campaign_refuses_the_constructs_outside_the_profile(self):
        options = csmith.csmith_options("base")
        for flag in ("--no-packed-struct", "--no-bitfields", "--no-volatiles"):
            self.assertIn(flag, options)

    def test_the_second_campaign_only_turns_bitfields_back_on(self):
        base = csmith.csmith_options("base")
        second = csmith.csmith_options("bitfields")
        self.assertIn("--bitfields", second)
        self.assertNotIn("--no-bitfields", second)
        self.assertEqual(
            [o for o in base if o != "--no-bitfields"],
            [o for o in second if o != "--bitfields"],
        )

    def test_the_safe_math_option_matches_the_runtime_define(self):
        self.assertIn("--math-notmp", csmith.csmith_options("base"))
        self.assertIn("-DUSE_MATH_MACROS_NOTMP", csmith.RUNTIME_DEFINES)

    def test_options_are_a_copy_the_caller_cannot_corrupt(self):
        first = csmith.csmith_options("base")
        first.append("--float")
        self.assertNotIn("--float", csmith.csmith_options("base"))

    def test_an_unknown_campaign_is_refused(self):
        with self.assertRaises(KeyError):
            csmith.csmith_options("volatile")

    def test_the_command_carries_the_seed_and_the_output(self):
        command = csmith.csmith_command(Path("/csmith"), "base", 17, Path("/tmp/s17.c"))
        self.assertEqual(command[0], "/csmith")
        self.assertEqual(command[-4:], ["--seed", "17", "--output", "/tmp/s17.c"])
        self.assertIn("--no-float", command)


class ChecksumTest(unittest.TestCase):
    def test_the_checksum_line_is_read(self):
        self.assertEqual(csmith.parse_checksum("checksum = 4A1BF2C\n"), "4a1bf2c")

    def test_spacing_does_not_matter(self):
        self.assertEqual(csmith.parse_checksum("checksum=ff"), "ff")

    def test_leading_zeros_do_not_matter(self):
        self.assertEqual(csmith.parse_checksum("checksum = 000ff"), "ff")

    def test_a_zero_checksum_survives_normalisation(self):
        self.assertEqual(csmith.parse_checksum("checksum = 0"), "0")

    def test_the_last_line_wins(self):
        text = "checksum = 1\nchecksum = 2\n"
        self.assertEqual(csmith.parse_checksum(text), "2")

    def test_output_without_a_checksum_reads_as_nothing(self):
        self.assertIsNone(csmith.parse_checksum("g_4 7\n"))
        self.assertIsNone(csmith.parse_checksum(""))
        self.assertIsNone(csmith.parse_checksum(None))


class VerdictTest(unittest.TestCase):
    @staticmethod
    def runs(evocc: str | None, oracle: str, split: str | None = None) -> dict:
        row = {}
        if evocc is not None:
            row["evocc"] = {"status": "ran", "checksum": evocc}
        for cc, opt in csmith.ORACLES:
            row[f"{cc}.{opt}"] = {"status": "ran", "checksum": oracle}
        if split is not None:
            row["clang-19.O2"] = {"status": "ran", "checksum": split}
        return row

    def test_all_three_agreeing_is_an_agreement(self):
        self.assertEqual(verdict_of(self.runs("ab", "ab")), "agree")

    def test_a_different_checksum_is_a_mismatch(self):
        outcome, detail = csmith.verdict(self.runs("ac", "ab"))
        self.assertEqual(outcome, "mismatch")
        self.assertEqual(detail, {"reference": "ab", "evocc": "ac"})

    def test_oracles_that_disagree_exclude_the_program(self):
        self.assertEqual(verdict_of(self.runs("ab", "ab", split="cd")), "oracle-split")

    def test_a_timing_out_program_is_not_a_mismatch(self):
        row = self.runs(None, "ab")
        row["evocc"] = {"status": "timeout"}
        self.assertEqual(verdict_of(row), "timeout")

    def test_an_oracle_that_will_not_build_is_not_a_mismatch(self):
        row = self.runs("ab", "ab")
        row["gcc.O2"] = {"status": "compile-fail"}
        self.assertEqual(verdict_of(row), "oracle-unusable")


def verdict_of(row: dict) -> str:
    return csmith.verdict(row)[0]


class DiagnosticTest(unittest.TestCase):
    def test_the_first_code_is_taken(self):
        text = "a.c:3:1: error[E2202]: invalid-initializer-type\nb.c: error[E1001]: x\n"
        self.assertEqual(csmith.diagnostic_code(text), ("E2202", "invalid-initializer-type"))

    def test_a_panic_is_its_own_bucket(self):
        self.assertEqual(csmith.diagnostic_code("thread panicked at ...")[0], "panic")

    def test_silence_falls_into_other(self):
        self.assertEqual(csmith.diagnostic_code("")[0], "other")


if __name__ == "__main__":
    unittest.main()
