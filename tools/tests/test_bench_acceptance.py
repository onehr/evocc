"""The bench entry point must keep retained results and plans honest."""

from contextlib import redirect_stdout
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

PATH = Path(__file__).resolve().parents[1] / 'bench.py'
spec = importlib.util.spec_from_file_location('bench_acceptance', PATH)
bench = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = bench
spec.loader.exec_module(bench)

DRIVER = 'a' * 64


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data))
    return path


def e0_summary(run, suite, programs=2):
    return {'run': run, 'driverSha256': DRIVER,
            'suites': {suite: {'programs': programs, 'outcomes': {'pass': programs}}}}


class SharedE0StatusTest(unittest.TestCase):
    """ctest and torture share ladder-e0-receipt.json, which keeps only the last suite."""

    def tree(self, root, *, ctest_run='20260927T082713Z', evidence=None, driver=DRIVER):
        write(root / 'tests/contracts/ladder-e0-receipt.json',
              {'latest': e0_summary('20260927T084053Z', 'torture', 3)})
        write(root / '.evidence/ladder/e0/20260927T082713Z/summary.json', e0_summary('20260927T082713Z', 'ctest'))
        write(root / bench.RECEIPT, {'benchmarks': {'ctest': {
            'run': ctest_run, 'evidence': evidence or '.evidence/ladder/e0/20260927T082713Z',
            'measurement': {'driverSha256': driver}}}})

    def status(self, root, *names):
        with patch.object(bench, 'ROOT', root), redirect_stdout(io.StringIO()) as output:
            code = bench.main(['--status', *names])
        return code, output.getvalue()

    def test_status_reads_the_suite_from_its_own_retained_run(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.tree(root)
            code, text = self.status(root, 'ctest', 'torture')
            self.assertEqual(code, 0)
            self.assertIn('ctest: retained PASS 2/2; run=20260927T082713Z', text)
            self.assertIn('from .evidence/ladder/e0/20260927T082713Z/summary.json', text)
            self.assertIn('torture: retained PASS 3/3; run=20260927T084053Z', text)
            self.assertNotIn('INVALID_RECEIPT', text)

    def test_a_mismatched_run_compiler_or_location_stays_invalid(self):
        for change in ({'ctest_run': 'another'}, {'driver': 'b' * 64}, {'evidence': 'tests/contracts'}):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                self.tree(root, **change)
                code, text = self.status(root, 'ctest', 'torture')
                self.assertEqual(code, 2)
                self.assertIn('ctest: INVALID_RECEIPT', text)
                # One invalid receipt does not hide the following lanes.
                self.assertIn('torture: retained PASS 3/3', text)

    def test_suite_without_a_retained_run_is_not_run(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write(root / 'tests/contracts/ladder-e0-receipt.json', {'latest': e0_summary('r', 'torture')})
            code, text = self.status(root, 'ctest')
            self.assertEqual(code, 0)
            self.assertIn('ctest: NOT_RUN (the shared e0 receipt holds torture)', text)


class SelectedReplayTest(unittest.TestCase):
    def test_replay_uses_its_own_receipt_and_compares_with_the_complete_one(self):
        complete = {'status': 'INCOMPLETE', 'kernelImageSha256': 'image', 'build': {'targets': ['pidfd']},
                    'execution': {'subject': {'summary': {'programs': 970, 'outcomes': {'PASS': 900, 'FAIL': 70}}}}}
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            full = write(root / 'tests/contracts/kernel-selftest-full-receipt.json', complete)
            before = full.read_bytes()
            seen = {}

            def result(name, previous, driver, survey=None, *, receipt_path, kernel_receipt_path):
                seen['path'] = receipt_path
                return {'rung': 'K7', 'status': 'INCOMPLETE', 'passed': 1, 'total': 970, 'kind': 'programs'}

            with patch.object(bench, 'ROOT', root), patch.object(bench, 'execute', return_value=(0, [])), \
                 patch.object(bench, 'read_result', side_effect=result), \
                 patch.object(bench.subprocess, 'check_output', return_value='checkout'), \
                 patch.object(bench.subprocess, 'run', return_value=SimpleNamespace(returncode=0)), \
                 redirect_stdout(io.StringIO()):
                bench.main(['kernel-selftest-all', '--reuse-selftest-build', '--selftest-select', 'pidfd'])
            self.assertEqual(seen['path'], root / bench.SELECTED_SELFTEST_RECEIPT)
            self.assertEqual(full.read_bytes(), before)
            row = json.loads((root / bench.RECEIPT).read_text())['benchmarks']['kernel-selftest-all']
            self.assertEqual((row['previous']['passed'], row['previous']['total']), (900, 970))

    def test_status_and_receipt_paths_keep_the_two_populations_apart(self):
        args = bench.parser().parse_args(['kernel-selftest-all'])
        self.assertEqual(bench.lane_receipt('kernel-selftest-all', args),
                         bench.ROOT / 'tests/contracts/kernel-selftest-full-receipt.json')
        args = bench.parser().parse_args(['kernel-selftest-all', '--selftest-select', 'pidfd'])
        self.assertEqual(bench.lane_receipt('kernel-selftest-all', args), bench.ROOT / bench.SELECTED_SELFTEST_RECEIPT)
        self.assertEqual(bench.lane_receipt('kernel-selftest-all'),
                         bench.ROOT / 'tests/contracts/kernel-selftest-full-receipt.json')


class SupplementalRootTest(unittest.TestCase):
    def test_a_supplemental_work_root_needs_an_explicit_source(self):
        for argv in (['kernel-full', '--kernel-full-receipt', 'tests/contracts/kernel-bpf-full-receipt.json'],
                     ['kernel', '--survey-only']):
            with self.subTest(argv=argv):
                args = bench.parser().parse_args([*argv, '--kernel-work-root', '.build/case/final-s'])
                with self.assertRaisesRegex(ValueError, 'own --kernel-source-root'):
                    bench.command(argv[0], args, Path('/compiler'))
                args.kernel_source_root = Path('.build/case/kbuild-probe/src')
                command, _ = bench.command(argv[0], args, Path('/compiler'))
                self.assertEqual(command[command.index('--src-root') + 1],
                                 str(Path('.build/case/kbuild-probe/src').resolve()))


class DryRunInputTest(unittest.TestCase):
    def dry_run(self, root, argv):
        with patch.object(bench, 'ROOT', root), patch.object(bench, 'execute') as execute, \
             redirect_stdout(io.StringIO()) as output:
            code = bench.main([*argv, '--dry-run'])
        execute.assert_not_called()
        return code, output.getvalue()

    def test_plan_reports_absent_inputs_without_running_or_writing(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            runtime = root / 'runtime'
            for label in ('reference', 'subject'):
                for name in ('build.json', 'initramfs.cpio', 'scratch.img'):
                    (runtime / label).mkdir(parents=True, exist_ok=True)
                    (runtime / label / name).write_text(name)
            (root / 'source/tools/testing/kunit').mkdir(parents=True)
            (root / 'source/tools/testing/kunit/kunit_parser.py').write_text('')
            write(root / 'tests/contracts/kernel-expanded-kunit-full-receipt.json', {})
            write(root / 'boot.json', {})
            (root / 'initramfs.cpio').write_text('')
            argv = ['kernel-kunit', '--kunit-source-root', 'source', '--kunit-reference-boot', 'boot.json',
                    '--kunit-reference-runtime', 'runtime/reference', '--kunit-subject-runtime', 'runtime/subject',
                    '--kunit-builtin-initramfs', 'initramfs.cpio']
            before = sorted(path.relative_to(root) for path in root.rglob('*'))
            code, text = self.dry_run(root, argv)
            self.assertEqual(code, 0)
            self.assertNotIn('missing input', text)
            (runtime / 'subject/scratch.img').unlink()
            code, text = self.dry_run(root, argv[:3] + argv[5:])
            self.assertEqual(code, 0)
            self.assertIn('missing input: reference boot (required; not given)', text)
            self.assertIn('missing input: subject KUnit runtime runtime/subject/scratch.img', text)
            self.assertEqual(sorted(path.relative_to(root) for path in root.rglob('*')), [
                path for path in before if path != Path('runtime/subject/scratch.img')])

    def test_each_absent_input_is_reported_once(self):
        with tempfile.TemporaryDirectory() as tmp:
            code, text = self.dry_run(Path(tmp), [
                'kernel-full', '--evocc', '/absent/evocc', '--kernel-work-root', '.build/case/final-s',
                '--kernel-source-root', 'source', '--kernel-full-receipt', 'tests/contracts/kernel-bpf-full-receipt.json'])
            self.assertEqual(code, 0)
            lines = [line for line in text.splitlines() if 'missing input' in line]
            self.assertEqual(lines, ['  missing input: compiler /absent/evocc',
                                     '  missing input: reference tree .build/case/final-s/pristine/.config',
                                     '  missing input: source source/Makefile',
                                     '  missing input: working tree .build/case/final-s/out/.config'])
            code, text = self.dry_run(Path(tmp), ['ctest', 'torture', '--evocc', '/absent/evocc'])
            self.assertEqual(text.count('missing input: compiler /absent/evocc'), 1)

    def test_an_absent_payload_directory_is_reported_once(self):
        with tempfile.TemporaryDirectory() as tmp:
            code, text = self.dry_run(Path(tmp), ['kernel-kunit', '--kunit-subject-runtime', 'runtime/subject'])
            self.assertEqual(code, 0)
            self.assertIn('  missing input: subject KUnit runtime runtime/subject\n', text)
            self.assertNotIn('runtime/subject/build.json', text)


if __name__ == '__main__':
    unittest.main()
