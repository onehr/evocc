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

    def test_zero_exit_with_only_skipped_cases_is_not_a_pass(self):
        # The upstream runner returns zero even when its sole program skips.
        rows = lane.parse_transcript(self.transcript(
            "1..1\nok 1 selftests: kvm: guest_print_test # SKIP\n"), "flight", ["memory"])
        self.assertEqual(rows[0]['outcome'], 'SKIP')
        self.assertEqual(rows[0]['exitCode'], 0)

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

    def test_timeout_marker_has_its_own_line_after_partial_child_output(self):
        source = (PATH.parent / 'selftest-init.c').read_text()
        self.assertTrue(
            'dprintf(1, "\\nKSELFTEST_END %s %s %d\\n", nonce, selected, rc);' in source,
            'the init-owned end marker must begin with a newline')
        # mmu_stress_test can be killed while printing a progress line without
        # its final newline. The init marker must still expose the real 124.
        output = ('EVO_BOOT_NONCE flight\nKSELFTEST_BEGIN flight memory\n'
                  '1..1\n# 3 vCPUs have not rendezvoused...'
                  '\nKSELFTEST_END flight memory 124\nEVO_INIT_DONE\n')
        row = lane.parse_transcript(output, 'flight', ['memory'])[0]
        self.assertEqual(row['outcome'], 'TIMEOUT')
        self.assertEqual(row['exitCode'], 124)
        self.assertFalse(row['valid'])

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

    def test_printk_replay_removes_only_the_identical_interrupted_prefix(self):
        text = ('[   30.710764]    \n** replaying previous printk message **\n'
                '[   30.710764]         # Subtest: intlog10_test\n'
                '[   30.711000]         not ok 1 broken\n')
        normalized, replays = lane.normalize_printk_replays(text)
        self.assertEqual(normalized, text.splitlines()[2:])
        self.assertEqual(replays, [{'line': 2, 'prefix': text.splitlines()[0],
                                   'complete': text.splitlines()[2]}])
        self.assertEqual(lane.normalize_printk_replays('not ok 1 broken\n'),
                         (['not ok 1 broken'], []))

    def test_printk_replay_requires_a_matching_timestamp_and_prefix(self):
        for text in ('** replaying previous printk message **\n',
                     '[   1.0] not ok\n** replaying previous printk message **\n[   1.0] ok 1\n',
                     '[   1.0] ok\n** replaying previous printk message **\n[   2.0] ok 1\n'):
            with self.assertRaises(ValueError):
                lane.normalize_printk_replays(text)


if __name__ == "__main__":
    unittest.main()
