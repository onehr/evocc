"""Acceptance tool defaults must name existing inputs or the planned campaign layout."""

import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest

TOOLS = Path(__file__).resolve().parents[1]
REPO = TOOLS.parents[1]
sys.path.append(str(TOOLS))
import kunit  # noqa: E402
import layout  # noqa: E402
import selftest_full  # noqa: E402

# Retired on 2026-10-07, or damaged by the case-insensitive host volume.
STALE = ('.build/k7-coverage', '.build/kselftest-full', '.build/kselftest/headers', '.build/linux-src',
         '.build/k7/', '.evidence/kunit/expanded-reference-builtin-20260922')


def defaults(parser):
    return {action.dest: action.default for action in parser._actions if isinstance(action.default, Path)}


class DefaultPathTest(unittest.TestCase):
    def assert_not_stale(self, values):
        for name, path in values.items():
            for stale in STALE:
                self.assertNotIn(stale, str(path), name)

    def test_kunit_defaults_name_the_audited_source_and_the_payload_layout(self):
        values = defaults(kunit.parser())
        self.assert_not_stale(values)
        self.assertEqual(values['source_root'], layout.SOURCE)
        self.assertEqual(values['reference_runtime'], layout.KUNIT_RUNTIME / 'reference')
        self.assertEqual(values['subject_runtime'], layout.KUNIT_RUNTIME / 'subject')
        self.assertIsNone(kunit.parser().parse_args([]).reference_boot)
        self.assertTrue(values['kernel_receipt'].is_file())
        self.assertTrue(values['builtin_initramfs'].is_file())

    def test_full_selftest_defaults_name_the_audited_source_and_the_selftest_layout(self):
        values = defaults(selftest_full.arguments())
        self.assert_not_stale(values)
        self.assertEqual(values['source_root'], layout.SOURCE)
        self.assertEqual(values['work_root'], layout.SELFTEST_WORK)
        self.assertEqual(values['headers_root'], layout.SELFTEST_HEADERS)

    def test_layout_inputs_exist_and_outputs_stay_on_the_case_volume(self):
        for path in (layout.KERNEL_TARGET, layout.BUILD_ENVIRONMENT, layout.GEN_INIT_CPIO):
            self.assertTrue(path.is_file(), path)
        for path in (layout.SOURCE, layout.SELFTEST_WORK, layout.SELFTEST_HEADERS, layout.KUNIT_RUNTIME):
            self.assertTrue(path.is_relative_to(layout.CASE_VOLUME), path)
        if not (layout.SOURCE / 'Makefile').is_file():
            self.skipTest('the case-sensitive volume is not attached')
        self.assertTrue((layout.SOURCE / 'scripts/kconfig/merge_config.sh').is_file())


class SelectedReplayTest(unittest.TestCase):
    def test_a_targeted_replay_never_replaces_the_complete_receipt(self):
        self.assertEqual(selftest_full.receipt_destination([]), selftest_full.FULL_RECEIPT)
        selected = selftest_full.receipt_destination(['pidfd:pidfd_test'])
        self.assertEqual(selected, selftest_full.SELECTED_RECEIPT)
        self.assertNotEqual(selected, selftest_full.FULL_RECEIPT)
        self.assertEqual(selected.parent, selftest_full.FULL_RECEIPT.parent)

    def test_bench_reads_the_same_replay_receipt(self):
        spec = importlib.util.spec_from_file_location('bench_for_k1_defaults', TOOLS.parent / 'bench.py')
        bench = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(bench)
        self.assertEqual(bench.ROOT / bench.SELECTED_SELFTEST_RECEIPT, selftest_full.SELECTED_RECEIPT)


class SupplementalSourceTest(unittest.TestCase):
    def test_a_separate_work_root_needs_its_own_source(self):
        # A separate process: substitute.py puts tools/k1 first on its path.
        work = REPO / '.build/t04g-absent-work-root'
        command = [sys.executable, str(TOOLS / 'substitute.py'), '--all', '--k1-root', str(work)]
        result = subprocess.run(command, capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 2)
        self.assertIn('own --src-root', result.stderr)
        result = subprocess.run(command + ['--src-root', str(layout.SOURCE)],
                                capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 2)
        self.assertIn('reproduce-reference.sh', result.stderr)
        self.assertFalse(work.exists())


if __name__ == '__main__':
    unittest.main()
