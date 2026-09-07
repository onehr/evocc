"""The generator options and the checksum reading must be the intended ones."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import tempfile
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
    def test_printing_a_checksum_does_not_hide_failed_execution(self):
        report = "report()" + csmith.RUNNER.split("report()", 1)[1].split("\n}\n", 1)[0] + "\n}\n"
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output.txt"
            output.write_text("checksum = ab\n")
            for status, expected in ((0, "ran ab"), (1, "run-fail 1"), (133, "run-fail 133"), (124, "timeout 124")):
                with self.subTest(status=status):
                    result = subprocess.run(
                        ["sh", "-c", 'b=probe\n' + report + '\nreport evocc "$1" "$2"', "sh", str(status), str(output)],
                        capture_output=True, text=True, check=True,
                    )
                    self.assertEqual(result.stdout.strip(), f"probe evocc {expected}")

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


class RuntimeTest(unittest.TestCase):
    def test_subtraction_guard_uses_a_defined_sign_mask(self):
        self.assertTrue(
            callable(getattr(csmith, "repair_runtime_header", None)),
            "The shared Csmith runtime still shifts signed one into the sign bit",
        )
        source = "\n".join(
            f"#define mask{width} (((int{width}_t)1) << (sizeof(int{width}_t)*CHAR_BIT-1))"
            for width in (32, 64)
        ) + "\n/* untouched */\n"
        repaired = csmith.repair_runtime_header(source)
        self.assertIn("#define mask32 (~INT32_MAX)", repaired)
        self.assertIn("#define mask64 (~INT64_MAX)", repaired)
        self.assertTrue(repaired.endswith("/* untouched */\n"))
        with self.assertRaises(ValueError):
            csmith.repair_runtime_header(source + source)


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


class InterestingTest(unittest.TestCase):
    def test_a_reduction_keeps_the_outcome_it_started_from(self):
        row = VerdictTest.runs("ac", "ab")
        self.assertTrue(csmith.interesting(row, "mismatch"))
        self.assertFalse(csmith.interesting(row, "no-checksum"))

    def test_a_candidate_that_stopped_disagreeing_is_not_interesting(self):
        self.assertFalse(csmith.interesting(VerdictTest.runs("ab", "ab"), "mismatch"))

    def test_a_candidate_whose_oracles_stopped_agreeing_is_not_interesting(self):
        row = VerdictTest.runs("ac", "ab", split="cd")
        self.assertFalse(csmith.interesting(row, "mismatch"))


class SummaryTest(unittest.TestCase):
    ROWS = [
        {"program": "s1", "outcome": "agree"},
        {"program": "s2", "outcome": "agree"},
        {"program": "s3", "outcome": "refused", "code": "E2102", "message": "invalid-update-operands"},
        {"program": "s4", "outcome": "refused", "code": "E2102", "message": "invalid-update-operands"},
        {"program": "s5", "outcome": "refused", "code": "E3101", "message": "bit-field zero initialization"},
        {"program": "s6", "outcome": "mismatch", "reference": "ab", "evocc": "ac"},
        {"program": "s7", "outcome": "no-checksum", "reference": "ab", "runs": {"evocc": {"exit": csmith.TRAP_EXIT}}},
        {"program": "s8", "outcome": "oracle-split"},
    ]

    def test_only_matching_checksums_count_as_agreement(self):
        self.assertEqual(csmith.summarise("base", self.ROWS)["agreeing"], 2)

    def test_every_program_is_accounted_for(self):
        summary = csmith.summarise("base", self.ROWS)
        self.assertEqual(summary["programs"], len(self.ROWS))
        self.assertEqual(sum(summary["outcomes"].values()), len(self.ROWS))

    def test_refusals_rank_by_code_and_by_message(self):
        summary = csmith.summarise("base", self.ROWS)
        self.assertEqual(summary["refusalsByCode"], {"E2102": 2, "E3101": 1})
        self.assertEqual(
            list(summary["refusalsByMessage"])[0], "E2102 invalid-update-operands"
        )

    def test_only_a_checksum_disagreement_is_a_miscompile(self):
        summary = csmith.summarise("base", self.ROWS)
        self.assertEqual([m["program"] for m in summary["miscompiles"]], ["s6"])

    def test_a_run_that_trapped_is_tracked_apart_from_a_miscompile(self):
        summary = csmith.summarise("base", self.ROWS)
        self.assertEqual(summary["trapExit133Programs"], ["s7"])
        self.assertNotIn("trappedOnUndefinedBehaviour", summary)

    def test_a_run_that_printed_nothing_without_trapping_is_neither(self):
        rows = self.ROWS + [
            {"program": "s9", "outcome": "no-checksum", "runs": {"evocc": {"exit": 0}}}
        ]
        summary = csmith.summarise("base", rows)
        self.assertNotIn("s9", summary["trapExit133Programs"])
        self.assertNotIn("s9", [m["program"] for m in summary["miscompiles"]])

    def test_an_oracle_split_is_neither_agreement_nor_miscompile(self):
        summary = csmith.summarise("base", self.ROWS)
        self.assertEqual(summary["outcomes"]["oracle-split"], 1)
        self.assertNotIn("s8", [m["program"] for m in summary["miscompiles"]])


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
