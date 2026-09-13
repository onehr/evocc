"""The full selftest run must account for build and execution gaps."""

import importlib.util
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


if __name__ == '__main__':
    unittest.main()
