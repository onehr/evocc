"""Kernel tests must execute; boot markers alone cannot satisfy the lane."""

import importlib.util
from pathlib import Path
import unittest


PATH = Path(__file__).resolve().parents[1] / "selftest.py"
lane = None
if PATH.exists():
    spec = importlib.util.spec_from_file_location("kernel_selftest", PATH)
    lane = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(lane)


class KernelSelftest(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(lane, "kernel-selftest execution is not implemented")

    def transcript(self, output="1..2\nok 1 memory\nok 2 huge # SKIP unavailable\n", rc=0):
        return ("EVO_BOOT_NONCE flight\nKSELFTEST_BEGIN flight memory\n"
                + output + f"KSELFTEST_END flight memory {rc}\nEVO_INIT_DONE\n")

    def test_actual_results_preserve_subtest_skips(self):
        rows = lane.parse_transcript(self.transcript(), "flight", ["memory"])
        self.assertEqual(rows[0]["outcome"], "PASS")
        self.assertEqual(rows[0]["subtests"], {"PASS": 1, "FAIL": 0, "SKIP": 1})

    def test_zero_exit_cannot_hide_failing_or_truncated_tap(self):
        for output in ("1..1\nnot ok 1 wrong\n", "1..2\nok 1 only\n",
                       "1..2\nok 1 repeated\nok 1 repeated\n", ""):
            with self.subTest(output=output):
                row = lane.parse_transcript(self.transcript(output), "flight", ["memory"])[0]
                self.assertNotEqual(row["outcome"], "PASS")

    def test_timeout_and_missing_program_stay_in_denominator(self):
        rows = lane.parse_transcript(self.transcript(rc=124), "flight", ["memory", "signal"])
        self.assertEqual([r["outcome"] for r in rows], ["TIMEOUT", "NOT_RUN"])
        rows = lane.parse_transcript("EVO_BOOT_NONCE flight\nKSELFTEST_BEGIN flight memory\n",
                                     "flight", ["memory", "signal"])
        self.assertEqual(len(rows), 2)
        self.assertNotEqual(rows[0]["outcome"], "PASS")

    def test_wrong_nonce_or_duplicate_execution_is_rejected(self):
        with self.assertRaises(ValueError):
            lane.parse_transcript(self.transcript(), "different", ["memory"])
        with self.assertRaises(ValueError):
            lane.parse_transcript(self.transcript() + self.transcript(), "flight", ["memory"])

    def test_reference_pass_must_not_become_skip_or_failure(self):
        reference = lane.parse_transcript(self.transcript("1..1\nok 1 memory\n"), "flight", ["memory"])
        for output, rc in (("1..1\nnot ok 1 memory\n", 1),
                           ("1..1\nok 1 memory # SKIP missing\n", 4)):
            subject = lane.parse_transcript(self.transcript(output, rc), "flight", ["memory"])
            self.assertNotEqual(lane.compare(reference, subject)["status"], "PASS")
        self.assertEqual(lane.compare(reference, reference)["status"], "PASS")

    def test_reference_failure_is_not_subject_success(self):
        rows = lane.parse_transcript(self.transcript("1..1\nnot ok 1 memory\n", 1), "flight", ["memory"])
        self.assertNotEqual(lane.compare(rows, rows)["status"], "PASS")

    def test_skip_exit_cannot_erase_a_recorded_failure(self):
        rows = lane.parse_transcript(self.transcript("1..1\nnot ok 1 broken\n", 4), "flight", ["memory"])
        self.assertEqual(rows[0]['outcome'], 'FAIL')


if __name__ == "__main__":
    unittest.main()
