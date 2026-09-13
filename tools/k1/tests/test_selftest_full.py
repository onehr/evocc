"""The full selftest run must account for build and execution gaps."""

import importlib.util
import copy
from pathlib import Path
import unittest


PATH = Path(__file__).resolve().parents[1] / 'selftest_full.py'
lane = None
if PATH.exists():
    spec = importlib.util.spec_from_file_location('kernel_selftest_full', PATH)
    lane = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(lane)


class FullSelftest(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(lane, 'full upstream selftest execution is not implemented')

    def test_manifest_preserves_nested_collections_and_missing_binaries(self):
        self.assertEqual(lane.test_manifest('net/forwarding:a.sh\nmm:test\nmm:missing\n'),
                         ['net/forwarding:a.sh', 'mm:test', 'mm:missing'])
        with self.assertRaises(ValueError):
            lane.test_manifest('mm:test\nmm:test\n')

    def test_build_failure_and_no_tests_cannot_disappear(self):
        builds = [{'id': 'mm', 'buildExit': 0, 'tests': ['mm:test']},
                  {'id': 'bpf', 'buildExit': 2, 'tests': []},
                  {'id': 'x86', 'buildExit': 0, 'tests': []}]
        runtime = [{'id': 'mm', 'cases': [{'outcome': 'PASS'}], 'valid': True}]
        result = lane.summarize(builds, runtime)
        self.assertEqual(result['collections'], 3)
        self.assertEqual(result['buildFailures'], ['bpf'])
        self.assertEqual(result['noRunnableTests'], ['bpf', 'x86'])
        self.assertEqual(result['outcomes'], {'PASS': 1})
        self.assertNotEqual(result['status'], 'PASS')

    def test_incomplete_execution_retains_the_declared_denominator(self):
        builds = [{'id': 'mm', 'buildExit': 0, 'tests': ['mm:a', 'mm:b']}]
        result = lane.summarize(builds, [{'id': 'mm', 'valid': False,
                                         'cases': [{'outcome': 'PASS'}]}])
        self.assertEqual(result['programs'], 2)
        self.assertEqual(result['outcomes'], {'PASS': 1, 'NOT_RUN': 1})
        self.assertNotEqual(result['status'], 'PASS')

    def test_one_program_timeout_cannot_erase_later_program_results(self):
        self.assertTrue(hasattr(lane, 'program_case'), 'per-program VM accounting is missing')
        timeout = {'boot': {'verdict': 'TIMEOUT'}, 'programs': [{'outcome': 'INCOMPLETE', 'cases': []}]}
        passed = {'boot': {'verdict': 'PASS'}, 'programs': [{'outcome': 'PASS', 'valid': True,
                                                         'cases': [{'outcome': 'PASS'}]}]}
        cases = [lane.program_case('mm:a', timeout), lane.program_case('mm:b', passed)]
        self.assertEqual([c['outcome'] for c in cases], ['TIMEOUT', 'PASS'])
        self.assertEqual([c['id'] for c in cases], ['mm:a', 'mm:b'])

    def test_boot_or_incomplete_tap_cannot_become_a_program_pass(self):
        self.assertTrue(hasattr(lane, 'program_case'), 'per-program VM accounting is missing')
        for verdict, outcome, valid in [('MARKERS_MISSING', 'PASS', True),
                                         ('PASS', 'INVALID_OUTPUT', False)]:
            flight = {'boot': {'verdict': verdict}, 'programs': [{'outcome': outcome, 'valid': valid,
                                                               'cases': [{'outcome': 'PASS'}]}]}
            self.assertNotEqual(lane.program_case('mm:a', flight)['outcome'], 'PASS')

    def test_kernel_failure_before_test_begin_stays_not_run(self):
        self.assertTrue(hasattr(lane, 'program_case'), 'per-program VM accounting is missing')
        flight = {'boot': {'verdict': 'MARKERS_MISSING'},
                  'programs': [{'outcome': 'NOT_RUN', 'cases': []}]}
        self.assertEqual(lane.program_case('mm:a', flight)['outcome'], 'NOT_RUN')

    def test_reference_reuse_preserves_every_result_and_rejects_changed_inputs(self):
        self.assertTrue(hasattr(lane, 'reusable_reference'), 'explicit reference reuse is missing')
        current = {'build': {'builds': [{'id': 'mm', 'available': ['mm:a', 'mm:b']}]},
                   'referenceConfigSha256': 'config', 'initramfsSha256': 'init',
                   'rootfsSha256': 'disk', 'runtimePolicy': {'workers': 2}}
        cases = [{'id': 'mm:a', 'outcome': 'PASS'}, {'id': 'mm:b', 'outcome': 'FAIL'}]
        previous = {**current, 'status': 'INCOMPLETE', 'kernelImageSha256': 'old-subject',
                    'execution': {'reference': {'collections': [{'id': 'mm', 'cases': cases}]},
                                  'subject': {'collections': []}}}
        self.assertEqual(lane.reusable_reference(previous, current), cases)
        for key in ('referenceConfigSha256', 'initramfsSha256', 'rootfsSha256', 'runtimePolicy', 'build'):
            with self.subTest(changed=key), self.assertRaises(ValueError):
                lane.reusable_reference(previous, {**current, key: 'changed'})
        for status in ('RUNNING', 'BLOCKED'):
            with self.subTest(status=status), self.assertRaises(ValueError):
                lane.reusable_reference({**previous, 'status': status}, current)
        for bad in (cases[:1], [cases[0], cases[0]]):
            incomplete = copy.deepcopy(previous)
            incomplete['execution']['reference']['collections'][0]['cases'] = bad
            with self.assertRaises(ValueError):
                lane.reusable_reference(incomplete, current)


if __name__ == '__main__':
    unittest.main()
