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

    def test_nested_scheduler_summary_keeps_the_pair_incomplete(self):
        for failed, skipped in ((1, 0), (0, 1)):
            output = ('1..1\n# RESULTS:\n#\n# PASSED:  1\n'
                      f'# SKIPPED: {skipped}\n# FAILED:  {failed}\nok 1 wrapper\n')
            rows = lane.parse_transcript(self.transcript(output), 'flight', ['memory'])
            self.assertEqual(rows[0]['outcome'], 'PASS', 'wrapper result is preserved')
            self.assertNotEqual(lane.compare(rows, rows)['status'], 'PASS')

    def test_skip_exit_cannot_erase_a_recorded_failure(self):
        rows = lane.parse_transcript(self.transcript("1..1\nnot ok 1 broken\n", 4), "flight", ["memory"])
        self.assertEqual(rows[0]['outcome'], 'FAIL')

    def test_truncated_scheduler_summary_cannot_be_an_all_tests_pass(self):
        output = (PATH.parents[2] / 'tests/fixtures/kselftest-scheduler-truncated.tap').read_text()
        parsed = lane.parse_tap(output)
        self.assertFalse(parsed['valid'], 'missing nested FAILED count was silently ignored')
        self.assertEqual(parsed['subtests'], {'PASS': 1, 'FAIL': 0, 'SKIP': 0},
                         'the observed outer verdict must remain intact')
        self.assertTrue(parsed['nestedSummaryErrors'])
        rows = lane.parse_transcript(self.transcript(output), 'flight', ['memory'])
        self.assertEqual(rows[0]['outcome'], 'INVALID_OUTPUT')
        self.assertEqual(lane.compare(rows, rows)['status'], 'INCOMPLETE')

    def test_scheduler_summary_requires_ordered_nonnegative_counts_and_a_test(self):
        for counts in (
            '# PASSED: 22\n',
            '# PASSED: 22\n# SKIPPED: 0\n# FAILED: -1\n',
            '# PASSED: 22\n# SKIPPED: 0\n# FAILED: unknown\n',
            '# PASSED: 22\n# PASSED: 22\n# FAILED: 0\n',
            '# FAILED: 0\n# SKIPPED: 0\n# PASSED: 22\n',
            '# PASSED: 0\n# SKIPPED: 0\n# FAILED: 0\n',
        ):
            with self.subTest(counts=counts):
                output = '1..1\n# RESULTS:\n' + counts + 'ok 1 selftests: sched_ext: runner\n'
                self.assertFalse(lane.parse_tap(output)['valid'])

    def test_unrelated_results_heading_is_not_scheduler_protocol(self):
        output = '1..1\n# RESULTS:\n# bytes: 42\nok 1 unrelated\n'
        self.assertTrue(lane.parse_tap(output)['valid'])

    def test_scheduler_wrapper_requires_a_summary_only_when_it_claims_pass(self):
        output = '1..1\nok 1 selftests: sched_ext: runner\n'
        self.assertFalse(lane.parse_tap(output)['valid'])
        skipped = output.replace('runner\n', 'runner # SKIP unavailable\n')
        self.assertTrue(lane.parse_tap(skipped)['valid'])
        healthy = output.replace('ok 1', '# RESULTS:\n# PASSED: 23\n# SKIPPED: 0\n# FAILED: 0\nok 1')
        parsed = lane.parse_tap(healthy)
        self.assertTrue(parsed['valid'])
        self.assertEqual(parsed['nestedSummaryErrors'], [])

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

    def test_bpf_wrapper_cannot_hide_a_truncated_or_missing_summary(self):
        output = (PATH.parents[2] / 'tests/fixtures/kselftest-bpf-truncated.tap').read_text()
        for sample in (output, '1..1\nok 1 selftests: bpf: test_progs\n'):
            with self.subTest(sample=sample):
                parsed = lane.parse_tap(sample)
                self.assertFalse(parsed['valid'])
                self.assertEqual(parsed['subtests'], {'PASS': 1, 'FAIL': 0, 'SKIP': 0})
                self.assertTrue(parsed['nestedSummaryErrors'])
                rows = lane.parse_transcript(self.transcript(sample), 'flight', ['memory'])
                self.assertEqual(rows[0]['outcome'], 'INVALID_OUTPUT')
                self.assertEqual(lane.compare(rows, rows)['status'], 'INCOMPLETE')

    def test_bpf_summary_requires_one_complete_nonempty_counter_record(self):
        for summary in (
            '# Summary: 0/0 PASSED, 0 SKIPPED, 0 FAILED\n',
            '# Summary: 1/1 PASSED, 0 SKIPPED, -1 FAILED\n',
            '# Summary: 1/1 PASSED, 0 SKIPPED, unknown FAILED\n',
            '# Summary: 1/1 PASSED, 0 SKIPPED, 0 FAILED\n' * 2,
        ):
            with self.subTest(summary=summary):
                self.assertFalse(lane.parse_tap(
                    '1..1\n' + summary + 'ok 1 selftests: bpf: test_progs-no_alu32\n')['valid'])

    def test_bpf_error_log_replays_do_not_duplicate_summary_or_outer_counts(self):
        line = '# Summary: 651/5644 PASSED, 127 SKIPPED, 11 FAILED'
        output = ('1..1\n# #31 bpf_tcp_ca:FAIL\n# All error logs:\n'
                  '# #31 bpf_tcp_ca:FAIL\n' + line + '\nnot ok 1 selftests: bpf: test_progs\n')
        parsed = lane.parse_tap(output)
        self.assertTrue(parsed['valid'])
        self.assertEqual(parsed['subtests'], {'PASS': 0, 'FAIL': 1, 'SKIP': 0})
        self.assertEqual(parsed['bpfSummaries'], [{'line': line, 'passedTests': 651,
                         'passedSubtests': 5644, 'skipped': 127, 'failedTests': 11}])
        for name in ('test_progs', 'test_progs-no_alu32', 'test_progs-cpuv4'):
            skipped = f'1..1\nok 1 selftests: bpf: {name} # SKIP unavailable\n'
            self.assertTrue(lane.parse_tap(skipped)['valid'])
        unrelated = '1..1\n# Summary: 3 bytes\nok 1 unrelated\n'
        self.assertTrue(lane.parse_tap(unrelated)['valid'])

    def test_printk_replay_requires_a_matching_timestamp_and_prefix(self):
        for text in ('** replaying previous printk message **\n',
                     '[   1.0] not ok\n** replaying previous printk message **\n[   1.0] ok 1\n',
                     '[   1.0] ok\n** replaying previous printk message **\n[   2.0] ok 1\n'):
            with self.assertRaises(ValueError):
                lane.normalize_printk_replays(text)


if __name__ == "__main__":
    unittest.main()
