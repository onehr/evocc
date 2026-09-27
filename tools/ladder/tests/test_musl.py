"""The libc under test must not also provide its own timeout supervisor."""

import importlib.util
from pathlib import Path
import os
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

path = Path(__file__).resolve().parents[1] / "musl.py"
spec = importlib.util.spec_from_file_location("bench_musl", path)
musl = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = musl
spec.loader.exec_module(musl)


class SupervisorTest(unittest.TestCase):
    def completed_tree(self, directory):
        tree = Path(directory) / 'musl'
        tests = Path(directory) / 'tests'
        (tree / 'lib').mkdir(parents=True)
        (tree / 'sysroot/lib').mkdir(parents=True)
        (tree / 'lib/libc.a').write_bytes(b'new-archive')
        (tree / 'sysroot/lib/libc.a').write_bytes(b'new-archive')
        (tests / 'src/sample').mkdir(parents=True)
        (tests / 'src/sample/one.err').write_text('')
        (tests / 'src/sample/two.err').write_text('FAIL src/sample/two.exe [status 1]\n')
        (tests / 'src/REPORT').write_text('FAIL src/sample/two.exe [status 1]\n')
        (tests / 'inventory.log').write_text('ERRS src/sample/one.err src/sample/two.err\n')
        (tests / 'reference-runtest').write_bytes(b'original-supervisor')
        return tree, tests

    def test_nonzero_runner_cannot_publish_a_stale_success_marker(self):
        for status, stdout in [(7, 'tests-done\n'), (0, 'prefix-tests-done\n'),
                               (0, 'tests-done\ntrailing unexpected output\n')]:
            with self.subTest(status=status, stdout=stdout), tempfile.TemporaryDirectory() as tmp:
                tree, tests = self.completed_tree(tmp)
                with patch.object(musl.mixed, 'container', return_value=
                                  subprocess.CompletedProcess([], status, stdout, '')):
                    verdict, _ = musl.relink_and_test(tree, tests)
                self.assertNotEqual(verdict, 'tests-done')

    def test_partial_runtime_inventory_is_not_a_completed_test(self):
        for broken in ('missing-case', 'lost-failure', 'missing-inventory', 'wrong-install',
                       'empty-inventory', 'duplicate-case', 'outside-case'):
            with self.subTest(broken=broken), tempfile.TemporaryDirectory() as tmp:
                tree, tests = self.completed_tree(tmp)
                if broken == 'missing-case':
                    (tests / 'src/sample/one.err').unlink()
                elif broken == 'lost-failure':
                    (tests / 'src/REPORT').write_text('')
                elif broken == 'missing-inventory':
                    (tests / 'inventory.log').unlink()
                elif broken == 'wrong-install':
                    (tree / 'sysroot/lib/libc.a').write_bytes(b'stale-library')
                elif broken == 'empty-inventory':
                    (tests / 'inventory.log').write_text('ERRS\n')
                elif broken == 'duplicate-case':
                    (tests / 'inventory.log').write_text('ERRS src/sample/one.err src/sample/one.err\n')
                else:
                    (tests / 'inventory.log').write_text('ERRS ../outside.err\n')
                with patch.object(musl.mixed, 'container', return_value=
                                  subprocess.CompletedProcess([], 0, 'tests-done\n', '')):
                    verdict, _ = musl.relink_and_test(tree, tests)
                self.assertNotEqual(verdict, 'tests-done')

    def test_a_complete_report_keeps_reference_failures_as_failures(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree, tests = self.completed_tree(tmp)
            with patch.object(musl.mixed, 'container', return_value=
                              subprocess.CompletedProcess([], 0, 'tests-done\n', '')):
                verdict, failed = musl.relink_and_test(tree, tests)
            self.assertEqual(verdict, 'tests-done')
            self.assertEqual(failed, {'src/sample/two.exe'})

    def test_failed_make_stops_before_reusing_an_old_archive_and_tests(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree, tests = self.completed_tree(tmp)
            bindir = Path(tmp) / 'bin'
            bindir.mkdir()
            make = bindir / 'make'
            make.write_text('#!/bin/sh\nexit 7\n')
            make.chmod(0o755)
            (tests / 'reference-runtest').chmod(0o755)
            script = musl.RELINK_SCRIPT.replace('/work/musl', str(tree)).replace('/work/libc-test', str(tests))
            done = subprocess.run(['/bin/sh', '-c', script], capture_output=True, text=True,
                                  env={**os.environ, 'PATH': f'{bindir}:{os.environ["PATH"]}'})
            self.assertNotEqual(done.returncode, 0)
            self.assertNotIn('tests-done', done.stdout)

    def test_rebuilt_objects_and_replaced_supervisors_are_rejected(self):
        for change in ('before', 'after', 'supervisor'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as tmp:
                tree, tests = self.completed_tree(tmp)
                obj = tree / 'target.lo'
                obj.write_bytes(b'evocc-object')
                expected = {'target.lo': musl.mixed.sha256(obj)}
                if change == 'before':
                    obj.write_bytes(b'other-object')

                def run(*args, **kwargs):
                    if change == 'after':
                        obj.write_bytes(b'rebuilt-object')
                    elif change == 'supervisor':
                        (tests / 'reference-runtest').write_bytes(b'subject-supervisor')
                    return subprocess.CompletedProcess([], 0, 'tests-done\n', '')

                with patch.object(musl.mixed, 'container', side_effect=run) as launch:
                    verdict, _ = musl.relink_and_test(tree, tests, expected)
                    if change == 'before':
                        launch.assert_not_called()
                self.assertNotEqual(verdict, 'tests-done')

    def test_reference_build_requires_exact_marker_and_successful_process(self):
        for status, output in [(1, 'reference-build-ok\n'), (0, 'prefix-reference-build-ok\n')]:
            with self.subTest(status=status), tempfile.TemporaryDirectory() as tmp:
                tree, tests = self.completed_tree(tmp)
                with patch.object(musl.mixed, 'container', return_value=
                                  subprocess.CompletedProcess([], status, output, '')):
                    verdict, _ = musl.reference_build(tree, tests)
                self.assertNotEqual(verdict, 'ok')

    def test_a_cached_passing_set_needs_a_real_complete_final_trial(self):
        for mode in ('pass', 'runner-fail', 'missing-object'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as tmp:
                build = Path(tmp)
                tree, tests = self.completed_tree(tmp)
                tree.rename(build / 'musl-work')
                tests.rename(build / 'libc-test-work')
                tree, tests = build / 'musl-work', build / 'libc-test-work'
                (build / 'musl').mkdir()
                (build / 'libc-test').mkdir()
                (tree / 'a.lo').write_bytes(b'reference-object')
                compiler = build / 'evocc'
                compiler.write_bytes(b'compiler-identity')
                unit = musl.mixed.Unit('/work/musl', ('-c', 'a.c', '-o', 'a.lo'),
                                       'a.c', 'a.lo', '/work/musl')
                calls = []

                def runtime(*args, **kwargs):
                    calls.append(1)
                    status = 9 if mode == 'runner-fail' and len(calls) > 1 else 0
                    return subprocess.CompletedProcess([], status, 'tests-done\n', '')

                def emit(*args, **kwargs):
                    objects = build / 'musl-asm/obj'
                    objects.mkdir(parents=True)
                    if mode != 'missing-object':
                        (objects / 'a.lo.o').write_bytes(b'evocc-object')
                    return [{'unit': 'a.lo', 'outcome': 'emitted', 'omitted': []}]

                options = SimpleNamespace(evocc=compiler, jobs=1, budget=2, skip_reference=True)
                cached = {'passingSet': ['a.lo'], 'failingUnits': [], 'budgetExhausted': False}
                with patch.object(musl.argparse.ArgumentParser, 'parse_args', return_value=options), \
                        patch.object(musl.mixed, 'BUILD', build), \
                        patch.object(musl.mixed, 'EVIDENCE', build / 'evidence'), \
                        patch.object(musl.mixed, 'container', side_effect=runtime), \
                        patch.object(musl.mixed, 'parse_commands', return_value=[unit]), \
                        patch.object(musl.mixed, 'emit_all', side_effect=emit), \
                        patch.object(musl.mixed, 'assemble', return_value={'a.lo': 'ok'}), \
                        patch.object(musl.mixed, 'symbol_parity', return_value={}), \
                        patch.object(musl.mixed, 'compatible_units', return_value=(['a.lo'], [])), \
                        patch.object(musl.mixed, 'narrow', return_value=cached), \
                        patch.object(musl.mixed, 'write_receipt', return_value=build / 'receipt') as publish:
                    self.assertEqual(musl.main(), 0)
                    receipt = publish.call_args.args[1]
                self.assertEqual(receipt['unitsInPassingBuild'], 1 if mode == 'pass' else 0)
                self.assertEqual(len(receipt['trialAudits']), 1)
                self.assertEqual(receipt['trialAudits'][0]['units'], [] if mode == 'missing-object' else ['a.lo'])
                self.assertEqual(len(calls), 1 if mode == 'missing-object' else 2)
                self.assertEqual((tree / 'a.lo').read_bytes(), b'reference-object')

    def test_missing_report_is_not_an_empty_failure_set(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "REPORT"
            with self.assertRaises(FileNotFoundError):
                musl.report_failures(report)
            report.write_text("")
            self.assertEqual(musl.report_failures(report), set())

    def test_reference_build_preserves_the_supervisor(self):
        self.assertIn("cp src/common/runtest.exe reference-runtest", musl.BUILD_SCRIPT)

    def test_subject_cases_use_the_preserved_supervisor(self):
        self.assertIn(f'RUN_TEST="/work/libc-test/reference-runtest -w {musl.CASE_WRAPPER}"', musl.RELINK_SCRIPT)
        self.assertIn(f'RUN_WRAP={musl.CASE_WRAPPER}', musl.BUILD_SCRIPT)
        self.assertIn("test -x /work/libc-test/reference-runtest", musl.RELINK_SCRIPT)

    def test_case_wrapper_is_installed_exactly(self):
        with tempfile.TemporaryDirectory() as directory:
            tests = Path(directory)
            musl.install_case_environment(tests)
            wrapper = tests / 'evo-case-environment'
            self.assertEqual(wrapper.read_bytes(), musl.CASE_RUNNER.read_bytes())
            self.assertTrue(os.access(wrapper, os.X_OK))
        # Only supervision changes; cases still relink against the subject libc.
        self.assertIn("make install", musl.RELINK_SCRIPT)
        self.assertIn("rm -f src/*/*.exe", musl.RELINK_SCRIPT)


if __name__ == "__main__":
    unittest.main()
