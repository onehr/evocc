"""The full selftest run must account for build and execution gaps."""

import importlib.util
import copy
from pathlib import Path
import tempfile
import subprocess
import unittest


PATH = Path(__file__).resolve().parents[1] / 'selftest_full.py'
lane = None
if PATH.exists():
    spec = importlib.util.spec_from_file_location('kernel_selftest_full', PATH)
    lane = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(lane)


class FullSelftest(unittest.TestCase):
    def test_boot_lane_uses_only_a_declared_machine_profile(self):
        library = PATH.parent / 'lib.sh'
        contract = PATH.parents[2] / 'contracts/kernel-target.json'
        command = ['bash', '-c', 'source "$1"; k1_boot_lane_args "$2" "$3"',
                   'lane-test', str(library), str(contract)]
        for name, expected in (('hvf-host', ['-accel', 'hvf', '-cpu', 'host']),
                               ('tcg-cortex-a72', ['-accel', 'tcg', '-cpu', 'cortex-a72'])):
            done = subprocess.run(command + [name], text=True, capture_output=True, check=False)
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertEqual(done.stdout.splitlines(), expected)
        self.assertNotEqual(subprocess.run(command + ['unknown'], capture_output=True).returncode, 0)

    def setUp(self):
        self.assertIsNotNone(lane, 'full upstream selftest execution is not implemented')

    def test_manifest_preserves_nested_collections_and_missing_binaries(self):
        self.assertEqual(lane.test_manifest('net/forwarding:a.sh\nmm:test\nmm:missing\n'),
                         ['net/forwarding:a.sh', 'mm:test', 'mm:missing'])
        with self.assertRaises(ValueError):
            lane.test_manifest('mm:test\nmm:test\n')

    def test_runtime_install_preserves_ublk_helpers_without_changing_programs(self):
        self.assertTrue(hasattr(lane, 'install_runtime_helpers'), 'ublk runtime helpers are not installed')
        with tempfile.TemporaryDirectory() as temp:
            source, installed = Path(temp) / 'source', Path(temp) / 'installed'
            (source / 'ublk/trace').mkdir(parents=True)
            (installed / 'ublk').mkdir(parents=True)
            (source / 'ublk/test_common.sh').write_text('helper\n')
            (source / 'ublk/trace/seq_io.bt').write_text('trace\n')
            (installed / 'ublk/test_generic_01.sh').write_text('original program\n')
            lane.install_runtime_helpers(source, installed)
            self.assertEqual((installed / 'ublk/test_common.sh').read_text(), 'helper\n')
            self.assertEqual((installed / 'ublk/trace/seq_io.bt').read_text(), 'trace\n')
            self.assertEqual((installed / 'ublk/test_generic_01.sh').read_text(), 'original program\n')

    def test_runtime_dependency_packages_cover_observed_missing_tools(self):
        self.assertTrue(hasattr(lane, 'RUNTIME_PACKAGES'), 'runtime packages are not declared')
        self.assertTrue({'procps', 'psmisc', 'udev', 'tcpdump', 'fio', 'kmod'} <= set(lane.RUNTIME_PACKAGES))

    def test_targeted_replay_keeps_the_full_inventory_and_rejects_unknown_names(self):
        self.assertTrue(hasattr(lane, 'selected_programs'), 'targeted replay is missing')
        builds = [{'id': 'mm', 'available': ['mm:a', 'mm:b']},
                  {'id': 'ublk', 'available': ['ublk:c']}]
        original = copy.deepcopy(builds)
        self.assertEqual(lane.selected_programs(builds, ['mm', 'ublk:c']), ['mm:a', 'mm:b', 'ublk:c'])
        self.assertEqual(lane.selected_programs(builds, ['mm:b']), ['mm:b'])
        self.assertEqual(builds, original)
        with self.assertRaises(ValueError):
            lane.selected_programs(builds, ['mispelled'])

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

    def test_nonzero_exit_cannot_pass_even_when_the_tap_case_says_ok(self):
        flight = {'boot': {'verdict': 'PASS'}, 'programs': [
            {'outcome': 'FAIL', 'exitCode': 1, 'valid': True, 'cases': [{'outcome': 'PASS'}]}]}
        self.assertEqual(lane.program_case('mm:a', flight)['outcome'], 'FAIL')

    def test_upstream_timeout_tap_stays_timeout_after_normal_guest_shutdown(self):
        name = 'selftests: mm: run_vmtests.sh # TIMEOUT 900 seconds'
        for exit_code in (0, 1):
            with self.subTest(exit_code=exit_code):
                flight = {'boot': {'verdict': 'PASS'}, 'programs': [
                    {'outcome': 'FAIL', 'exitCode': exit_code, 'valid': True,
                     'cases': [{'outcome': 'FAIL', 'name': name}]}]}
                self.assertEqual(lane.program_case('mm:run_vmtests.sh', flight)['outcome'], 'TIMEOUT')
        flight['programs'][0].update(outcome='PASS', exitCode=0)
        flight['programs'][0]['cases'][0]['outcome'] = 'PASS'
        self.assertEqual(lane.program_case('mm:run_vmtests.sh', flight)['outcome'], 'PASS')

    def test_reference_reuse_preserves_every_result_and_rejects_changed_inputs(self):
        self.assertTrue(hasattr(lane, 'reusable_reference'), 'explicit reference reuse is missing')
        current = {'build': {'builds': [{'id': 'mm', 'available': ['mm:a', 'mm:b']}]},
                   'referenceConfigSha256': 'config', 'initramfsSha256': 'init',
                   'rootfsSha256': 'disk', 'runtimePolicy': {'workers': 2}}
        cases = [{'id': 'mm:a', 'outcome': 'PASS', 'exitCode': 0},
                 {'id': 'mm:b', 'outcome': 'FAIL', 'exitCode': 1}]
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
        contradictory = copy.deepcopy(previous)
        contradictory['execution']['reference']['collections'][0]['cases'][0]['exitCode'] = 1
        with self.assertRaises(ValueError):
            lane.reusable_reference(contradictory, current)


if __name__ == '__main__':
    unittest.main()
