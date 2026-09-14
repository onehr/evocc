"""The full selftest run must account for build and execution gaps."""

import importlib.util
import copy
import io
import hashlib
import json
import inspect
from pathlib import Path
import shutil
import tempfile
import subprocess
import tarfile
import unittest


PATH = Path(__file__).resolve().parents[1] / 'selftest_full.py'
lane = None
if PATH.exists():
    spec = importlib.util.spec_from_file_location('kernel_selftest_full', PATH)
    lane = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(lane)


class FullSelftest(unittest.TestCase):
    def test_expanded_kernel_selection_uses_the_audited_images_not_the_frozen_pair(self):
        select = getattr(lane, 'kernel_selection', None)
        self.assertTrue(callable(select), 'full selftests cannot yet select the expanded audited kernel')
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            def digest(path):
                return hashlib.sha256(path.read_bytes()).hexdigest()
            for side in ('out', 'pristine'):
                folder = root / '.build/k7' / side
                (folder / 'arch/arm64/boot').mkdir(parents=True)
                (folder / 'arch/arm64/boot/Image').write_bytes(side.encode())
                (folder / '.config').write_bytes(b'CONFIG_KUNIT=y\n')
            capture = root / 'capture'
            capture.mkdir()
            objects = capture / 'objects.jsonl'
            obj = root / '.build/k7/out/a.o'
            obj.write_bytes(b'EvoCC object')
            objects.write_text(json.dumps({'unit': 'a.o', 'outcome': 'substituted',
                                          'installedSha256': digest(obj)}) + '\n')
            image = root / '.build/k7/out/arch/arm64/boot/Image'
            reference = root / '.build/k7/pristine/arch/arm64/boot/Image'
            receipt = {'config': {'sha256': digest(root / '.build/k7/out/.config')},
                'builtInComposition': {
                'capture': 'capture', 'unitsFile': 'objects.jsonl', 'unitsSha256': digest(objects),
                'targetCBuiltIns': 1, 'installedAndHashVerified': 1, 'gccFallback': 0,
                'generatedInputsStable': True, 'generatedExportTableStable': True,
                'linkExitCode': 0, 'imageSha256': digest(image)},
                'subject': {'runtime': {'bootVerdict': 'PASS', 'imageSha256': digest(image)}},
                'reference': {'bootVerdict': 'PASS', 'imageSha256': digest(reference)}}
            receipt_path = root / 'tests/contracts/kernel-kunit-receipt.json'
            receipt_path.parent.mkdir(parents=True)
            receipt_path.write_text(json.dumps(receipt))
            selected = select(True, root)
            self.assertEqual(selected['images'], {'reference': reference, 'subject': image})
            self.assertEqual(selected['receipt'], receipt_path)
            self.assertIn('receipt_path', inspect.signature(select).parameters,
                          'a second configuration must not overwrite the active kernel pair')
            shutil.copytree(root / '.build/k7', root / '.build/coverage')
            supplemental = copy.deepcopy(receipt)
            supplemental['outputs'] = {'reference': '.build/coverage/pristine',
                                       'subject': '.build/coverage/out'}
            alternative = root / 'tests/contracts/kernel-coverage-receipt.json'
            alternative.write_text(json.dumps(supplemental))
            second = select(True, root, receipt_path=alternative)
            self.assertEqual(second['images']['subject'],
                             root / '.build/coverage/out/arch/arm64/boot/Image')
            self.assertEqual(second['receipt'], alternative)
            with self.assertRaises(ValueError):
                select(False, root, receipt_path=alternative)
            (root / '.build/k7/out/nested').mkdir()
            objects.write_text(json.dumps({'unit': 'nested/../a.o', 'outcome': 'substituted',
                                          'installedSha256': digest(obj)}) + '\n')
            receipt['builtInComposition']['unitsSha256'] = digest(objects)
            receipt_path.write_text(json.dumps(receipt))
            self.assertEqual(select(True, root)['images']['subject'], image)
            (root / '.build/k7/pristine/.config').write_bytes(b'other config')
            with self.assertRaises(ValueError):
                select(True, root)
            (root / '.build/k7/pristine/.config').write_bytes(b'CONFIG_KUNIT=y\n')
            obj.write_bytes(b'GCC replacement')
            with self.assertRaises(ValueError):
                select(True, root)

    def test_upstream_install_dependencies_are_built_without_becoming_test_collections(self):
        self.assertTrue(hasattr(lane, 'enumerate_collections'),
                        'the build enumeration drops INSTALL_DEP_TARGETS such as net/lib')
        with tempfile.TemporaryDirectory() as temp:
            (Path(temp) / 'Makefile').write_text(
                'TARGETS := net mm\nTARGETS_HOTPLUG := cpu-hotplug\n'
                'INSTALL_DEP_TARGETS := net/lib net\n')
            targets, dependencies = lane.enumerate_collections(['make', '-s', '-C', temp])
            self.assertEqual(targets, ['net', 'mm', 'cpu-hotplug'])
            self.assertEqual(dependencies, ['net/lib'])
        dependency = {'id': 'net/lib', 'buildExit': 2, 'installExit': 0, 'buildErrors': []}
        result = lane.summarize([], [], [dependency])
        self.assertEqual(result['dependencyBuildFailures'], ['net/lib'])
        self.assertEqual(result['programs'], 0)
        self.assertNotEqual(result['status'], 'PASS')

    def test_long_stressor_uses_its_declared_test_timeout_not_the_collection_default(self):
        self.assertTrue(hasattr(lane, 'program_timeout'))
        build = {'programTimeoutSeconds': 60}
        self.assertEqual(lane.program_timeout(build, 'filesystems:file_stressor'), 1815)
        self.assertEqual(lane.program_timeout(build, 'filesystems:devpts_pts'), 60)

    def test_explicit_timeout_reaches_one_selected_program_without_changing_other_options(self):
        command = ['bash', '-c', 'source "$1"; k1_selftest_cmdline "$2" "$3" "$4"',
                   'timeout-test', str(PATH.parent / 'lib.sh')]
        done = subprocess.run(command + ['filesystems:file_stressor', '', '1800'],
                              text=True, capture_output=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stdout.strip(),
                         'evo_selftest=filesystems:file_stressor kselftest_override_timeout=1800')
        for program, value in [('filesystems', '1800'), ('', '1800'),
                               ('filesystems:file_stressor', '0'),
                               ('filesystems:file_stressor', '99999999999999999'),
                               ('filesystems:file_stressor', '1800 init=/bin/sh')]:
            self.assertNotEqual(subprocess.run(command + [program, '', value],
                                               capture_output=True).returncode, 0)

    def test_multiphase_ublk_stress_has_an_explicit_budget_without_changing_neighbors(self):
        build = {'programTimeoutSeconds': 60}
        self.assertEqual(lane.program_timeout(build, 'ublk:test_stress_05.sh'), 615)
        self.assertEqual(lane.program_timeout(build, 'ublk:test_stress_04.sh'), 60)
        policy = lane.PROGRAM_TIMEOUT_OVERRIDES['ublk:test_stress_05.sh']
        self.assertIn('test_stress_05.sh', policy['source'])
        self.assertIn('40', policy['reason'])

    def test_complete_rseq_injection_matrix_has_an_explicit_measured_budget(self):
        build = {'programTimeoutSeconds': 315}
        self.assertEqual(lane.program_timeout(build, 'rseq:run_param_test.sh'), 7215)
        self.assertEqual(lane.program_timeout(build, 'rseq:run_syscall_errors_test.sh'), 315)
        policy = lane.PROGRAM_TIMEOUT_OVERRIDES['rseq:run_param_test.sh']
        self.assertIn('timeout=0', policy['reason'])
        self.assertIn('1184', policy['reason'])

    def test_kunit_filter_is_explicit_and_cannot_add_kernel_options(self):
        command = ['bash', '-c', 'source "$1"; k1_kunit_cmdline "$2"',
                   'kunit-test', str(PATH.parent / 'lib.sh')]
        done = subprocess.run(command + ['pm_runtime_test_cases.*'], text=True, capture_output=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stdout, ' kunit.filter_glob=pm_runtime_test_cases.*')
        for value in ['suite.* init=/bin/sh', '$(false)', 'suite\n', 'a' * 257]:
            self.assertNotEqual(subprocess.run(command + [value], capture_output=True).returncode, 0)

    def test_kunit_autorun_override_is_explicit_for_separate_selftest_boots(self):
        command = ['bash', '-c', 'source "$1"; k1_kunit_cmdline "$2" "$3"',
                   'kunit-test', str(PATH.parent / 'lib.sh')]
        for value, expected in [('', ''), ('0', ' kunit.autorun=0'), ('1', ' kunit.autorun=1')]:
            done = subprocess.run(command + ['', value], text=True, capture_output=True)
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertEqual(done.stdout, expected)
        for value in ['2', '0 init=/bin/sh']:
            self.assertNotEqual(subprocess.run(command + ['', value], capture_output=True).returncode, 0)

    def test_module_kunit_execution_is_separate_from_upstream_program_results(self):
        source = (PATH.parent / 'selftest-init.c').read_text()
        self.assertIn('"/tests/kunit-modules"', source)
        self.assertIn('"/sys/module/kunit/parameters/filter_glob"', source)
        self.assertIn('KUNIT_MODULE_BEGIN %s %s', source)
        self.assertIn('KUNIT_MODULE_END %s %s %d', source)
        self.assertIn('KUNIT_MODULE_COMPLETE %s %u', source)
        self.assertIn('dmesg --read-clear --color=never', source)
        self.assertIn('fatal("read module KUnit records")', source)

    def test_post_setup_kunit_runs_use_debugfs_and_keep_the_exact_results(self):
        source = (PATH.parent / 'selftest-init.c').read_text()
        begin = source.find('static unsigned run_kunit_suites(')
        self.assertNotEqual(begin, -1, 'KUnit cannot yet run after its module prerequisites load')
        end = source.index('\nint main(void)', begin)
        function = source[begin:end]
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'suite').mkdir()
            (root / 'suite/run').write_text('')
            results = 'KTAP version 1\n1..1\nnot ok 1 deliberately_failing_case\n'
            (root / 'suite/results').write_text(results)
            manifest = root / 'list'
            manifest.write_text('suite\n')
            program = root / 'probe.c'
            program.write_text('#include <errno.h>\n#include <fcntl.h>\n#include <stdio.h>\n'
                               '#include <stdlib.h>\n#include <string.h>\n#include <unistd.h>\n'
                               'static void fatal(const char *s) { perror(s); exit(7); }\n'
                               + function + '\nint main(int argc, char **argv) { (void)argc;\n'
                               'return run_kunit_suites(argv[1], "nonce", argv[2]) != 1; }\n')
            subprocess.run(['cc', '-Wall', '-Wextra', '-Werror', str(program),
                            '-o', str(root / 'probe')], check=True, capture_output=True)
            command = [str(root / 'probe'), str(manifest), str(root)]
            done = subprocess.run(command, text=True, capture_output=True)
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertEqual(done.stdout, 'KUNIT_RERUN_BEGIN nonce suite\n' + results
                             + 'KUNIT_RERUN_END nonce suite\n')
            self.assertEqual((root / 'suite/run').read_text(), '1\n')
            (root / 'suite/run').unlink()
            done = subprocess.run(command, text=True, capture_output=True)
            self.assertEqual(done.returncode, 7)
            self.assertFalse((root / 'suite/run').exists(), 'an init-only suite must not become a fake run')
            self.assertNotIn('KUNIT_RERUN_END', done.stdout)
            manifest.write_text('../suite\n')
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 7)

    def test_targeted_arguments_use_the_upstream_environment_without_shell_code(self):
        command = ['bash', '-c', 'source "$1"; k1_selftest_cmdline "$2" "$3"',
                   'args-test', str(PATH.parent / 'lib.sh')]
        done = subprocess.run(command + ['mm:run_vmtests.sh', '-t cow'],
                              text=True, capture_output=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stdout.strip(),
                         'evo_selftest=mm:run_vmtests.sh KSELFTEST_RUN_VMTESTS_SH_ARGS="-t cow"')
        done = subprocess.run(command + ['mm:sub/test+name.sh', '-n 1'],
                              text=True, capture_output=True, check=True)
        self.assertIn('KSELFTEST_TEST_NAME_SH_ARGS="-n 1"', done.stdout)
        for program, args in [('mm', '-t cow'), ('mm:../cow', ''),
                              ('mm:run_vmtests.sh', '" evil=1'),
                              ('mm:run_vmtests.sh', '$(false)'),
                              ('mm:run_vmtests.sh', '-t cow\n')]:
            done = subprocess.run(command + [program, args], capture_output=True)
            self.assertNotEqual(done.returncode, 0, (program, args))

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

    def test_fat_runtime_includes_the_formatter_and_privilege_wrapper(self):
        self.assertTrue({'dosfstools', 'sudo'} <= set(lane.RUNTIME_PACKAGES))

    def test_mount_runtime_includes_the_ext4_formatter(self):
        self.assertIn('e2fsprogs', lane.RUNTIME_PACKAGES)

    def test_network_runtime_includes_observed_packet_and_device_tools(self):
        self.assertTrue({'netsniff-ng', 'libteam-utils', 'iperf3', 'traceroute',
                         'conntrack', 'fuse3'} <= set(lane.RUNTIME_PACKAGES))

    def test_runtime_rejects_a_partial_network_library_install(self):
        self.assertTrue(hasattr(lane, 'check_network_runtime'))
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            files = ('py/ynl.py', 'ynl/pyynl/__init__.py', 'ynl/pyynl/lib/ynl.py',
                     'specs/netdev.yaml', 'csum', 'xdp_helper')
            for name in files:
                path = root / 'tests/net/lib' / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('installed upstream file\n')
            lane.check_network_runtime(root)
            (root / 'tests/net/lib/ynl/pyynl/__init__.py').unlink()
            with self.assertRaisesRegex(ValueError, 'ynl/pyynl/__init__.py'):
                lane.check_network_runtime(root)

    def test_guest_archive_uses_root_ownership_without_changing_modes_or_links(self):
        self.assertTrue(hasattr(lane, 'write_rootfs_tar'))
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'etc').mkdir()
            (root / 'etc/sudoers').write_text('root ALL=(ALL:ALL) ALL\n')
            (root / 'etc/sudoers').chmod(0o440)
            (root / 'etc/shadow').write_text('fixture')
            (root / 'sudo').write_text('executable fixture')
            (root / 'sudo').chmod(0o755)
            (root / 'link').symlink_to('sudo')
            output = io.BytesIO()
            metadata = {'sudo': [0, 0, 0o4755], 'etc/shadow': [0, 42, 0o640]}
            lane.write_rootfs_tar(root, output, metadata)
            output.seek(0)
            with tarfile.open(fileobj=output) as archive:
                for member in archive.getmembers():
                    expected = metadata.get(member.name.removeprefix('./'), [0, 0])[:2]
                    self.assertEqual([member.uid, member.gid], expected, member.name)
                self.assertEqual(archive.getmember('./etc/sudoers').mode, 0o440)
                self.assertEqual(archive.getmember('./sudo').mode, 0o4755)
                self.assertEqual(archive.getmember('./etc/shadow').mode, 0o640)
                self.assertEqual(archive.getmember('./link').linkname, 'sudo')
                self.assertEqual(archive.extractfile('./etc/sudoers').read(),
                                 b'root ALL=(ALL:ALL) ALL\n')

    def test_mount_tests_have_their_standard_mountpoint(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            lane.install_runtime_paths(root)
            self.assertTrue((root / 'mnt').is_dir())

    def test_runtime_paths_share_the_run_directory_inside_the_guest_root(self):
        self.assertTrue(hasattr(lane, 'install_runtime_paths'), 'runtime directory links are missing')
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            lane.install_runtime_paths(root)
            lane.install_runtime_paths(root)
            self.assertEqual((root / 'var/run').readlink(), Path('../run'))
            self.assertEqual((root / 'var/lock').readlink(), Path('../run/lock'))
            (root / 'run/netns').mkdir()
            (root / 'var/run/netns/example').write_text('guest namespace mount point')
            self.assertEqual((root / 'run/netns/example').read_text(), 'guest namespace mount point')
            self.assertEqual((root / 'var/lock').resolve(), (root / 'run/lock').resolve())

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

    def test_successful_wrapper_cannot_hide_a_nested_failure_summary(self):
        spec = importlib.util.spec_from_file_location('selftest_summary_probe', PATH.parent / 'selftest.py')
        selftest = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(selftest)
        output = ('TAP version 13\n1..1\n'
                  '# # Totals: pass:22 fail:1 xfail:3 xpass:0 skip:0 error:0\n'
                  'ok 1 selftests: mm: run_vmtests.sh\n')
        parsed = selftest.parse_tap(output)
        self.assertEqual(parsed.get('nestedFailureSummaries'), [
            {'line': '# # Totals: pass:22 fail:1 xfail:3 xpass:0 skip:0 error:0',
             'failed': 1, 'errors': 0}])
        flight = {'boot': {'verdict': 'PASS'}, 'programs': [
            dict(parsed, outcome='PASS', exitCode=0)]}
        case = lane.program_case('mm:run_vmtests.sh', flight)
        self.assertEqual(case['outcome'], 'PASS', 'retain the original wrapper verdict')
        summary = lane.summarize([{'id': 'mm', 'buildExit': 0, 'tests': ['mm:run_vmtests.sh']}],
                                 [{'id': 'mm', 'valid': True, 'cases': [case]}])
        self.assertEqual(summary['nestedFailureReview'], ['mm:run_vmtests.sh'])
        self.assertNotEqual(summary['status'], 'PASS')
        clean = output.replace('fail:1', 'fail:0')
        self.assertEqual(selftest.parse_tap(clean)['nestedFailureSummaries'], [])

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
