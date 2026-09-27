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
from unittest.mock import MagicMock, patch


PATH = Path(__file__).resolve().parents[1] / 'selftest_full.py'
lane = None
if PATH.exists():
    spec = importlib.util.spec_from_file_location('kernel_selftest_full', PATH)
    lane = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(lane)


class FullSelftest(unittest.TestCase):
    def test_timer_runtime_dependency_requires_a_pinned_archive_and_explicit_patch(self):
        prepare = getattr(lane, 'prepare_runtime_alsa', None)
        self.assertTrue(callable(prepare), 'the timer dependency repair must survive runtime rebuilds')
        with tempfile.TemporaryDirectory() as temp:
            work = Path(temp)
            archive = work / 'runtime-tools/alsa-lib-1.2.14.tar.bz2'
            archive.parent.mkdir()
            archive.write_bytes(b'wrong archive')
            with patch.object(lane.subprocess, 'run') as run, self.assertRaises(ValueError):
                prepare(work)
            run.assert_not_called()
        source = inspect.getsource(lane.install_runtime_alsa)
        self.assertIn('alsa-lib-1.2.14-timer-async.patch', source)
        self.assertIn('alsa-timer-async.c', source)
        self.assertIn('patchSha256', source)
        self.assertIn('binarySha256', source)
        self.assertNotIn('NDEBUG', source, 'assertions must remain enabled')

    def test_iommufd_provisions_its_original_128m_hugetlb_fixture_on_both_images(self):
        for expanded in (False, True):
            environment = lane.program_environment('iommu:iommufd', expanded)
            self.assertEqual(environment.get('K1_SELFTEST_HUGEPAGES'), '64')
        self.assertNotIn('K1_SELFTEST_HUGEPAGES',
                         lane.program_environment('mm:hugepage-shm', True))

    def test_udmabuf_provisions_the_unchanged_two_gib_hugetlb_workload(self):
        for expanded in (False, True):
            environment = lane.program_environment('drivers/dma-buf:udmabuf', expanded)
            self.assertEqual(environment.get('K1_SELFTEST_HUGEPAGES'), '1024')
            self.assertEqual(environment.get('K1_BOOT_LANE'), 'hvf-host')
            contract = Path(environment['K1_KERNEL_CONTRACT'])
            self.assertNotEqual(contract, lane.ROOT / 'contracts/kernel-target.json')
            machine = json.loads(contract.read_text())
            self.assertEqual(machine['qemu']['fixedArgv']['memoryMiB'], 4096)
            self.assertEqual(machine['qemu']['machine'], 'virt-11.0')
            self.assertNotIn('K1_SELFTEST_TIMEOUT', environment)
        ordinary = lane.program_environment('drivers/dma-buf:unrelated', True)
        self.assertNotIn('K1_KERNEL_CONTRACT', ordinary)
        self.assertNotIn('K1_SELFTEST_HUGEPAGES', ordinary)

    def test_udmabuf_hugepage_count_survives_the_real_command_line(self):
        command = ['bash', '-c', 'source "$1"; k1_selftest_cmdline "$2" "" "" "$3"',
                   'hugepage-test', str(PATH.parent / 'lib.sh')]
        done = subprocess.run(command + ['drivers/dma-buf:udmabuf', '1024'],
                              text=True, capture_output=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stdout.strip(),
                         'evo_selftest=drivers/dma-buf:udmabuf kselftest_hugepages=1024')

    def test_hugepage_override_is_bounded_and_bound_to_one_program(self):
        command = ['bash', '-c', 'source "$1"; k1_selftest_cmdline "$2" "" "" "$3"',
                   'hugepage-test', str(PATH.parent / 'lib.sh')]
        done = subprocess.run(command + ['iommu:iommufd', '64'],
                              text=True, capture_output=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stdout.strip(),
                         'evo_selftest=iommu:iommufd kselftest_hugepages=64')
        for program, value in [('iommu', '64'), ('', '64'), ('iommu:iommufd', '0'),
                               ('iommu:iommufd', '1025'), ('iommu:iommufd', '-1'),
                               ('iommu:iommufd', '64 init=/bin/sh'),
                               ('iommu:iommufd', '99999999999999999999')]:
            with self.subTest(program=program, value=value):
                self.assertNotEqual(subprocess.run(command + [program, value],
                                                   capture_output=True).returncode, 0)

    def test_init_hugepage_override_preserves_default_and_rejects_invalid_values(self):
        source = (PATH.parent / 'selftest-init.c').read_text()
        self.assertIn('static unsigned huge_page_count(', source)
        function = source.split('static unsigned huge_page_count(', 1)[1].split('\n}\n', 1)[0]
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            program = root / 'probe.c'
            program.write_text('#include <errno.h>\n#include <stdio.h>\n'
                               '#include <stdlib.h>\n#include <string.h>\n'
                               'static void fatal(const char *s) { perror(s); exit(7); }\n'
                               + 'static unsigned huge_page_count(' + function + '\n}\n'
                               'int main(int argc, char **argv) {\n'
                               'if (argc > 1) setenv("kselftest_hugepages", argv[1], 1);\n'
                               'else unsetenv("kselftest_hugepages");\n'
                               'printf("%u\\n", huge_page_count(1)); return 0; }\n')
            subprocess.run(['cc', '-Wall', '-Wextra', '-Werror', str(program),
                            '-o', str(root / 'probe')], check=True, capture_output=True)
            for args, expected in [([], '1\n'), (['64'], '64\n'), (['512'], '512\n'),
                                   (['1024'], '1024\n')]:
                done = subprocess.run([str(root / 'probe'), *args], text=True, capture_output=True)
                self.assertEqual(done.returncode, 0, done.stderr)
                self.assertEqual(done.stdout, expected)
            for value in ('', '0', '-1', '1025', '64 ', '64x', '99999999999999999999'):
                self.assertEqual(subprocess.run([str(root / 'probe'), value],
                                               capture_output=True).returncode, 7)
        self.assertIn('pages = huge_page_count(pages);', source)

    def test_runtime_preloads_installed_character_device_modules(self):
        with tempfile.TemporaryDirectory() as temp:
            rootfs = Path(temp)
            (rootfs / 'tests').mkdir()
            modules = {'modules': [{'unit': name} for name in (
                'fs/fuse/fuse.ko', 'sound/core/snd-timer.ko',
                'drivers/misc/ntsync.ko', 'drivers/iommu/iommufd/iommufd.ko',
                'drivers/unrelated.ko')]}
            actual = lane.install_runtime_module_prerequisites(rootfs, modules)
            self.assertEqual(actual, ['fuse', 'snd_timer', 'ntsync', 'iommufd'])
            self.assertEqual((rootfs / 'tests/modules').read_text().splitlines(), actual)

    def test_virtual_nci_device_is_created_without_udev_only_when_installed(self):
        with tempfile.TemporaryDirectory() as temp:
            rootfs = Path(temp)
            (rootfs / 'tests').mkdir()
            modules = {'modules': [{'unit': 'net/nfc/nfc.ko'},
                                   {'unit': 'net/nfc/nci/nci.ko'}]}
            self.assertEqual(lane.install_runtime_module_prerequisites(rootfs, modules), [])
            modules['modules'].append({'unit': 'drivers/nfc/virtual_ncidev.ko'})
            self.assertEqual(lane.install_runtime_module_prerequisites(rootfs, modules),
                             ['virtual_ncidev'])
            self.assertEqual((rootfs / 'tests/modules').read_text(), 'virtual_ncidev\n')

    def test_installed_fuse_is_loaded_before_mounting_its_control_filesystem(self):
        prepare = getattr(lane, 'install_runtime_module_prerequisites', None)
        self.assertTrue(callable(prepare), 'fusectl sysfs mountpoint exists only after loading fuse')
        with tempfile.TemporaryDirectory() as temp:
            rootfs = Path(temp)
            (rootfs / 'tests').mkdir()
            modules = {'status': 'PASS', 'modules': [{'unit': 'fs/fuse/fuse.ko'},
                                                    {'unit': 'drivers/other.ko'}]}
            self.assertEqual(prepare(rootfs, modules), ['fuse'])
            self.assertEqual((rootfs / 'tests/modules').read_text(), 'fuse\n')
            self.assertEqual(prepare(rootfs, None), [])
            self.assertEqual((rootfs / 'tests/modules').read_text(), '')
        init = (PATH.parent / 'selftest-init.c').read_text()
        self.assertLess(init.index('load_runtime_modules("/tests/modules", NULL)'),
                        init.index('"/sys/fs/fuse/connections"'))

    def test_kvm_selftests_use_a_separate_el2_machine_profile(self):
        environment = lane.program_environment('kvm:guest_print_test', True)
        self.assertEqual(environment.get('K1_BOOT_LANE'), 'tcg-cortex-a72-el2',
                         'ordinary EL1-only guests cannot exercise /dev/kvm')
        contract = Path(environment['K1_KERNEL_CONTRACT'])
        machine = json.loads(contract.read_text())
        self.assertNotEqual(contract, lane.ROOT / 'contracts/kernel-target.json')
        self.assertEqual(machine['qemu']['machine'], 'virt-11.0,virtualization=on,gic-version=3',
                         'upstream arm64 default irqchip requires VGICv3')
        self.assertEqual(machine['qemu']['lanes'][0]['argv'],
                         ['-accel', 'tcg', '-cpu', 'cortex-a72'])
        self.assertGreaterEqual(machine['qemu']['fixedArgv']['memoryMiB'], 4096,
                                'unchanged mmu_stress_test pre-faults a 2 GiB memslot '
                                'before allocating its page tables and kernel overhead')
        self.assertEqual(lane.program_environment('kvm:dirty_log_test', True)
                         ['K1_KERNEL_CONTRACT'], str(contract))
        self.assertNotIn('K1_KERNEL_CONTRACT', lane.program_environment('bpf:test_progs', True))

    def test_boot_policy_identity_tracks_machine_contract_changes(self):
        policies = getattr(lane, 'boot_policies', None)
        self.assertTrue(callable(policies), 'runtime policy must bind supplemental machine bytes')
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / 'machine.json'
            path.write_text('{"machine":"virt-11.0"}')
            profiles = {'kvm': {'lane': 'fixed', 'contract': 'machine.json'}}
            with patch.object(lane, 'ROOT', Path(temp)), patch.object(lane, 'PROGRAM_BOOT_LANES', profiles):
                before = policies()
                path.write_text('{"machine":"virt-11.0,virtualization=on"}')
                after = policies()
            self.assertNotEqual(before, after)
            self.assertEqual(after['kvm']['contractSha256'], hashlib.sha256(path.read_bytes()).hexdigest())
            self.assertNotIn('contractSha256', profiles['kvm'])

    def test_kvm_timer_clock_control_preserves_the_original_test_contract(self):
        environment = lane.program_environment('kvm:arch_timer', True)
        self.assertEqual(environment.get('K1_BOOT_LANE'), 'tcg-cortex-a72-el2-icount')
        machine = json.loads(Path(environment['K1_KERNEL_CONTRACT']).read_text())
        profile = next(p for p in machine['qemu']['lanes']
                       if p['name'] == environment['K1_BOOT_LANE'])
        self.assertEqual(profile['argv'], ['-accel', 'tcg,thread=single', '-cpu',
                                          'cortex-a72', '-icount', 'shift=0'])
        self.assertNotIn('K1_SELFTEST_ARGS', environment)
        self.assertNotIn('K1_SELFTEST_TIMEOUT', environment)
        self.assertEqual(lane.program_environment('kvm:dirty_log_test', True)['K1_BOOT_LANE'],
                         'tcg-cortex-a72-el2', 'do not change unrelated performance tests')

    def test_kvm_host_sve_uses_a_fixed_cpu_with_the_tested_capability(self):
        environment = lane.program_environment('kvm:host_sve', True)
        self.assertEqual(environment.get('K1_BOOT_LANE'), 'tcg-neoverse-n2-el2')
        machine = json.loads(Path(environment['K1_KERNEL_CONTRACT']).read_text())
        profile = next(p for p in machine['qemu']['lanes']
                       if p['name'] == environment['K1_BOOT_LANE'])
        self.assertEqual(profile['argv'], ['-accel', 'tcg', '-cpu', 'neoverse-n2'])
        self.assertNotIn('K1_SELFTEST_ARGS', environment)
        self.assertNotIn('K1_SELFTEST_TIMEOUT', environment)
        self.assertEqual(lane.program_environment('kvm:dirty_log_test', True)['K1_BOOT_LANE'],
                         'tcg-cortex-a72-el2')

    def test_kvm_aarch32_id_registers_require_an_aarch64_only_cpu(self):
        environment = lane.program_environment('kvm:aarch32_id_regs', True)
        self.assertEqual(environment.get('K1_BOOT_LANE'), 'tcg-a64fx-el2')
        machine = json.loads(Path(environment['K1_KERNEL_CONTRACT']).read_text())
        profile = next(p for p in machine['qemu']['lanes']
                       if p['name'] == environment['K1_BOOT_LANE'])
        self.assertEqual(profile['argv'], ['-accel', 'tcg', '-cpu', 'a64fx'])
        self.assertNotIn('K1_SELFTEST_ARGS', environment)
        self.assertNotIn('K1_SELFTEST_TIMEOUT', environment)

    def test_kvm_demand_paging_uses_the_completed_fixed_cpu_control(self):
        for expanded in (False, True):
            environment = lane.program_environment('kvm:demand_paging_test', expanded)
            self.assertEqual(environment.get('K1_BOOT_LANE'), 'tcg-neoverse-n2-el2')
            self.assertNotIn('K1_SELFTEST_ARGS', environment)
            self.assertNotIn('K1_SELFTEST_TIMEOUT', environment)
            self.assertNotIn('K1_KVM_MODE', environment)
        for ident in ('kvm:memslot_perf_test', 'kvm:rseq_test'):
            self.assertEqual(lane.program_environment(ident, True)['K1_BOOT_LANE'],
                             'tcg-cortex-a72-el2', 'failed controls do not change the runtime lane')

    def test_external_test_modules_are_rebuilt_against_the_actual_kernel(self):
        rebuild = getattr(lane, 'rebuild_test_modules', None)
        self.assertTrue(callable(rebuild), 'BPF and livepatch helpers need the actual BTF and symbol versions')
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            work, kernel = base / 'work', base / 'kernel'
            source = work / 'source/tools/testing/selftests/bpf/test_kmods'
            source.mkdir(parents=True)
            (source / 'Makefile').write_text('obj-m += bpf_testmod.o\n')
            (source / 'bpf_testmod.c').write_text('/* unchanged test support */\n')
            installed = work / 'rootfs/tests/bpf/bpf_testmod.ko'
            installed.parent.mkdir(parents=True)
            installed.write_bytes(b'reference helper')
            alias = installed.parent / 'no_alu32/bpf_testmod.ko'
            alias.parent.mkdir()
            alias.symlink_to('../bpf_testmod.ko')
            for name in ('.config', 'vmlinux', 'Module.symvers', 'arch/arm64/boot/Image'):
                path = kernel / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(name.encode())
            calls = []

            def build(argv, **kwargs):
                calls.append(argv[-1])
                self.assertEqual(argv[:3], ['make', '-C', str(kernel)])
                self.assertIn(f'M={source}', argv)
                self.assertIn(f'O={kernel}', argv, 'override the reference build mount stored in Makefile')
                self.assertIn('CC=gcc-14', argv)
                if argv[-1] == 'modules':
                    (source / 'modules.order').write_text(str(source / 'bpf_testmod.o') + '\n')
                    (source / 'bpf_testmod.ko').write_bytes(b'actual-kernel helper')
                return subprocess.CompletedProcess(argv, 0, 'original module recipe\n')

            with patch.object(lane.subprocess, 'run', side_effect=build):
                result = rebuild(work, kernel)
            self.assertEqual(calls, ['clean', 'modules'])
            self.assertEqual(installed.read_bytes(), b'actual-kernel helper')
            self.assertEqual(alias.read_bytes(), installed.read_bytes())
            self.assertEqual(result['helperAliases'], [
                {'path': 'bpf/no_alu32/bpf_testmod.ko', 'target': 'bpf/bpf_testmod.ko'}])
            self.assertEqual(len(result['modules']), 1, 'a flavor alias is not another module build')
            self.assertEqual(result['status'], 'PASS')
            self.assertEqual(result['kernelSha256']['vmlinux'],
                             hashlib.sha256(b'vmlinux').hexdigest())
            self.assertEqual(result['modules'][0]['path'], 'bpf/bpf_testmod.ko')
            self.assertEqual(result['modules'][0]['sha256'],
                             hashlib.sha256(installed.read_bytes()).hexdigest())
            self.assertIn('reference-built', result['role'])

    def test_module_refresh_rejects_untracked_flavor_copies_or_redirects(self):
        with tempfile.TemporaryDirectory() as temp:
            work = Path(temp)
            primary = work / 'rootfs/tests/bpf/bpf_testmod.ko'
            primary.parent.mkdir(parents=True)
            primary.write_bytes(b'current helper')
            alias = primary.parent / 'no_alu32/bpf_testmod.ko'
            alias.parent.mkdir()
            alias.write_bytes(b'stale copy')
            with self.assertRaisesRegex(ValueError, 'unaccounted external'):
                lane.rebuild_test_modules(work, work / 'kernel')
            alias.unlink()
            alias.symlink_to('../../untracked.ko')
            with self.assertRaisesRegex(ValueError, 'unaccounted external'):
                lane.rebuild_test_modules(work, work / 'kernel')

    def test_module_runtime_rejects_stale_external_test_helpers(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            image = folder / 'Image'
            image.write_bytes(b'new Image')
            (folder / 'scratch.img').write_bytes(b'filesystem with stale test helpers')
            metadata = {'builds': [{'id': 'bpf', 'available': ['bpf:test_progs']}],
                        'kernelBuild': {'sha256': {'.config': 'cfg', 'vmlinux': 'reference BTF',
                                                  'Module.symvers': 'versions'}},
                        'kernelModules': {'status': 'PASS', 'imageSha256': lane.file_sha256(image),
                                          'configSha256': 'cfg', 'vmlinuxSha256': 'subject BTF',
                                          'moduleSymversSha256': 'versions'},
                        'rootfsSha256': lane.file_sha256(folder / 'scratch.img')}
            (folder / 'build.json').write_text(json.dumps(metadata))
            with self.assertRaisesRegex(ValueError, 'test module.*kernel'):
                lane.check_module_runtime(folder, image, None)
            metadata['kernelTestModules'] = {'status': 'PASS', 'kernelSha256': {
                '.config': 'cfg', 'vmlinux': 'subject BTF', 'Module.symvers': 'versions'}}
            (folder / 'build.json').write_text(json.dumps(metadata))
            lane.check_module_runtime(folder, image, None)

    def test_configuration_audit_keeps_disabled_features_and_modules_distinct(self):
        audit = getattr(lane, 'configuration_gaps', None)
        self.assertTrue(callable(audit), 'unavailable test configuration is not hardware inapplicability')
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            source = base / 'source'
            (source / 'tools/testing/selftests/net').mkdir(parents=True)
            fragment = source / 'tools/testing/selftests/net/config'
            fragment.write_text('CONFIG_A=y\nCONFIG_B=y\nCONFIG_C=m\nCONFIG_D=n\n')
            config = base / '.config'
            config.write_text('# CONFIG_A is not set\nCONFIG_B=m\nCONFIG_C=y\nCONFIG_D=y\n')
            builds = [{'id': 'net', 'available': ['net:test']},
                      {'id': 'no-fragment', 'available': ['no-fragment:test']}]
            original = copy.deepcopy(builds)
            result = audit(source, config, builds)
            self.assertEqual(result['configSha256'], hashlib.sha256(config.read_bytes()).hexdigest())
            self.assertEqual(result['missingFragments'], ['no-fragment'])
            self.assertEqual(result['collections'][0]['differences'], [
                {'option': 'CONFIG_A', 'requested': 'y', 'actual': 'n', 'kind': 'disabled'},
                {'option': 'CONFIG_B', 'requested': 'y', 'actual': 'm', 'kind': 'module'},
                {'option': 'CONFIG_C', 'requested': 'm', 'actual': 'y', 'kind': 'built-in'},
                {'option': 'CONFIG_D', 'requested': 'n', 'actual': 'y', 'kind': 'different'},
            ])
            self.assertEqual(result['collections'][0]['fragmentSha256'],
                             hashlib.sha256(fragment.read_bytes()).hexdigest())
            self.assertEqual(builds, original, 'configuration evidence must not rewrite results')

    def test_configuration_audit_includes_arm64_fragment_without_erasing_generic_requests(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            source = base / 'source'
            directory = source / 'tools/testing/selftests/bpf'
            directory.mkdir(parents=True)
            (directory / 'config').write_text('CONFIG_BPF=y\nCONFIG_A=m\n')
            arm = directory / 'config.aarch64'
            arm.write_text('CONFIG_A=y\nCONFIG_CGROUP_NET_CLASSID=y\n')
            (directory / 'config.x86_64').write_text('CONFIG_X86_ONLY=y\n')
            config = base / '.config'
            config.write_text('CONFIG_ARM64=y\nCONFIG_BPF=y\nCONFIG_A=m\n')
            result = lane.configuration_gaps(source, config, [{'id': 'bpf'}])
            self.assertEqual(result['collections'][0]['differences'], [])
            self.assertEqual(result.get('architectureFragments'), [{
                'id': 'bpf', 'path': 'tools/testing/selftests/bpf/config.aarch64',
                'fragmentSha256': hashlib.sha256(arm.read_bytes()).hexdigest(),
                'differences': [
                    {'option': 'CONFIG_A', 'requested': 'y', 'actual': 'm', 'kind': 'module'},
                    {'option': 'CONFIG_CGROUP_NET_CLASSID', 'requested': 'y', 'actual': 'n',
                     'kind': 'disabled'},
                ],
            }])
            config.write_text('CONFIG_X86_64=y\nCONFIG_BPF=y\nCONFIG_A=m\n')
            self.assertEqual(lane.configuration_gaps(source, config, [{'id': 'bpf'}])
                             .get('architectureFragments'), [],
                             'arm64-only audit must not apply its fragment to another target')

    def test_configuration_audit_records_reviewed_prerequisites_without_a_fragment(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            source = base / 'source'
            source.mkdir()
            kconfig = source / 'connector.Kconfig'
            kconfig.write_text('config PROC_EVENTS\n\tdepends on CONNECTOR=y\n')
            test_source = source / 'proc_filter.c'
            test_source.write_text('socket(AF_NETLINK, SOCK_DGRAM, NETLINK_CONNECTOR);\n')
            pins = {p.name: lane.file_sha256(p) for p in (kconfig, test_source)}
            review = {'connector': {
                'sources': pins, 'requirements': {'CONFIG_CONNECTOR': 'y', 'CONFIG_PROC_EVENTS': 'y'},
                'reason': 'process events require built-in connector',
            }}
            config = base / '.config'
            builds = [{'id': 'connector', 'available': ['connector:proc_filter']}]
            unchanged = copy.deepcopy(builds)
            with patch.object(lane, 'REVIEWED_CONFIG_PREREQUISITES', review, create=True):
                for connector in ('n', 'm', 'y'):
                    config.write_text(f'CONFIG_CONNECTOR={connector}\nCONFIG_PROC_EVENTS=n\n')
                    result = lane.configuration_gaps(source, config, builds)
                    self.assertEqual(result['missingFragments'], ['connector'])
                    entries = result.get('reviewedPrerequisites', [])
                    self.assertEqual(len(entries), 1, 'missing upstream fragment hides known requirements')
                    entry = entries[0]
                    self.assertEqual(entry['status'], 'VERIFIED_SOURCE')
                    self.assertEqual(entry['sourceSha256'], pins)
                    differences = entry['differences']
                    self.assertEqual(len(differences), 1 if connector == 'y' else 2)
                    self.assertEqual(differences[-1]['option'], 'CONFIG_PROC_EVENTS')
                    self.assertEqual(differences[-1]['kind'], 'disabled')
                    if connector == 'm':
                        self.assertEqual(differences[0]['kind'], 'module')
                config.write_text('CONFIG_CONNECTOR=y\nCONFIG_PROC_EVENTS=y\n')
                self.assertEqual(lane.configuration_gaps(source, config, builds)
                                 ['reviewedPrerequisites'][0]['differences'], [])
                kconfig.write_text('changed dependency\n')
                entry = lane.configuration_gaps(source, config, builds)['reviewedPrerequisites'][0]
                self.assertEqual(entry['status'], 'SOURCE_CHANGED')
                self.assertNotIn('differences', entry, 'unreviewed sources must not inherit the conclusion')
                test_source.unlink()
                entry = lane.configuration_gaps(source, config, builds)['reviewedPrerequisites'][0]
                self.assertEqual(entry['status'], 'SOURCE_CHANGED')
                self.assertIsNone(entry['sourceSha256']['proc_filter.c'])
                self.assertEqual(lane.configuration_gaps(source, config, [{'id': 'unrelated'}])
                                 ['reviewedPrerequisites'], [])
            self.assertEqual(builds, unchanged, 'prerequisites never rewrite runtime outcomes')

    def test_module_installation_preserves_the_linked_body_and_records_the_payload(self):
        install = getattr(lane, 'install_kernel_modules', None)
        self.assertTrue(callable(install), 'a new Image must not silently reuse reference modules')
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            source, kernel, rootfs = (base / name for name in ('source', 'kernel', 'rootfs'))
            (source / 'scripts').mkdir(parents=True)
            (source / 'scripts/Makefile.modinst').write_text('# original installer\n')
            (kernel / 'include/config').mkdir(parents=True)
            (kernel / 'include/config/kernel.release').write_text('6.18.44\n')
            (kernel / '.config').write_text('CONFIG_MODULES=y\nCONFIG_MODULE_SIG_ALL=y\n')
            (kernel / 'arch/arm64/boot').mkdir(parents=True)
            (kernel / 'arch/arm64/boot/Image').write_bytes(b'new Image')
            (kernel / 'vmlinux').write_bytes(b'new BTF base')
            (kernel / 'Module.symvers').write_bytes(b'new symbol versions')
            (kernel / 'System.map').write_bytes(b'new symbols')
            (kernel / 'modules.order').write_text('a.o\n')
            (kernel / 'a.ko').write_bytes(b'new linked ELF body')
            rootfs.mkdir()
            installed = rootfs / 'usr/lib/modules/6.18.44/kernel/a.ko'

            def modinst(argv, **kwargs):
                self.assertEqual(argv[:3], ['make', '-f', str(source / 'scripts/Makefile.modinst')])
                self.assertEqual(kwargs['cwd'], kernel)
                self.assertIn('INSTALL_MOD_STRIP=', argv)
                installed.parent.mkdir(parents=True, exist_ok=True)
                installed.write_bytes((kernel / 'a.ko').read_bytes()
                    + b'sig' + bytes([0, 0, 2, 0, 0, 0, 0, 0]) + (3).to_bytes(4, 'big')
                    + b'~Module signature appended~\n')
                (installed.parent.parent / 'modules.dep').write_text('kernel/a.ko:\n')
                (installed.parent.parent / 'modules.dep.bin').write_bytes(b'index')
                return subprocess.CompletedProcess(argv, 0)

            with patch.object(lane.subprocess, 'run', side_effect=modinst):
                result = install(source, kernel, rootfs)
            self.assertEqual(result['status'], 'PASS')
            self.assertEqual(result['release'], '6.18.44')
            self.assertEqual(result['imageSha256'], hashlib.sha256(b'new Image').hexdigest())
            self.assertEqual(result['modules'][0]['unit'], 'a.ko')
            self.assertEqual(result['modules'][0]['linkedSha256'],
                             hashlib.sha256(b'new linked ELF body').hexdigest())
            self.assertEqual(result['modules'][0]['installedSha256'],
                             hashlib.sha256(installed.read_bytes()).hexdigest())
            def unresolved(argv, **kwargs):
                result = modinst(argv, **kwargs)
                result.stderr = 'depmod: WARNING: a.ko needs unknown symbol missing\n'
                return result
            with patch.object(lane.subprocess, 'run', side_effect=unresolved):
                with self.assertRaisesRegex(ValueError, 'unresolved module'):
                    install(source, kernel, rootfs)
            with patch.object(lane.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)):
                installed.write_bytes(b'old GCC ELF body~Module signature appended~\n')
                with self.assertRaisesRegex(ValueError, 'module body changed'):
                    install(source, kernel, rootfs)
                installed.write_bytes((kernel / 'a.ko').read_bytes())
                with self.assertRaisesRegex(ValueError, 'signature missing or malformed'):
                    install(source, kernel, rootfs)
                (kernel / '.config').write_text('CONFIG_MODULES=y\n# CONFIG_MODULE_SIG is not set\n')
                unsigned = install(source, kernel, rootfs)
                self.assertIn('unsigned', unsigned['signatures'],
                              'an unsigned configuration must not claim a signer or trailer check')
            (rootfs / 'usr/lib/modules/6.18.44/kernel').rename(rootfs / 'old-kernel')
            (rootfs / 'usr/lib/modules/6.18.44/kernel').symlink_to(kernel, target_is_directory=True)
            with patch.object(lane.subprocess, 'run') as run:
                with self.assertRaisesRegex(ValueError, 'module installation path'):
                    install(source, kernel, rootfs)
                run.assert_not_called()

    def test_module_runtime_rejects_a_reference_payload_with_the_subject_image(self):
        check = getattr(lane, 'check_module_runtime', None)
        self.assertTrue(callable(check), 'hashing a stale rootfs does not bind it to the new kernel')
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            image = folder / 'Image'
            image.write_bytes(b'new Image')
            (folder / 'scratch.img').write_bytes(b'new filesystem')
            metadata = {'kernelModules': {'status': 'PASS', 'imageSha256':
                hashlib.sha256(image.read_bytes()).hexdigest(),
                'modules': [{'unit': 'a.ko', 'linkedSha256': 'new object'}]},
                'rootfsSha256': hashlib.sha256(b'new filesystem').hexdigest()}
            (folder / 'build.json').write_text(json.dumps(metadata))
            check(folder, image, {'modules': [{'unit': 'a.ko', 'sha256': 'new object'}]})
            metadata['kernelModules']['imageSha256'] = 'old GCC image'
            (folder / 'build.json').write_text(json.dumps(metadata))
            with self.assertRaisesRegex(ValueError, 'module runtime Image'):
                check(folder, image, None)
            metadata['kernelModules']['imageSha256'] = hashlib.sha256(image.read_bytes()).hexdigest()
            (folder / 'build.json').write_text(json.dumps(metadata))
            with self.assertRaisesRegex(ValueError, 'linked module inventory'):
                check(folder, image, {'modules': [{'unit': 'a.ko', 'sha256': 'another object'}]})
            (folder / 'scratch.img').write_bytes(b'old reference filesystem')
            with self.assertRaisesRegex(ValueError, 'module runtime filesystem'):
                check(folder, image, None)

    def test_livepatch_runtime_installs_real_prepared_module_headers(self):
        install = getattr(lane, 'install_module_build_tree', None)
        self.assertTrue(callable(install), 'prebuilt livepatch modules still need a valid runtime KDIR')
        with tempfile.TemporaryDirectory() as temp, \
             tempfile.TemporaryDirectory(prefix='kselftest-module-headers.', dir='/tmp') as staging:
            base = Path(temp)
            source, kernel, rootfs = (base / name for name in ('source', 'kernel', 'rootfs'))
            (source / 'scripts/package').mkdir(parents=True)
            script = source / 'scripts/package/install-extmod-build'
            script.write_text('#!/bin/sh\n')
            (kernel / 'include/config').mkdir(parents=True)
            (kernel / 'include/config/kernel.release').write_text('6.18.44\n')
            (kernel / '.config').write_text('CONFIG_MODULES=y\n')
            (kernel / 'Module.symvers').write_text('reference symbol versions\n')
            (kernel / 'samples/livepatch').mkdir(parents=True)
            (kernel / 'samples/livepatch/livepatch-sample.ko').write_bytes(b'original linked sample')
            link = rootfs / 'usr/lib/modules/6.18.44/build'
            link.parent.mkdir(parents=True)
            link.symlink_to('/work/kernel')

            def package(argv, **kwargs):
                if argv[0] == 'mktemp':
                    return subprocess.CompletedProcess(argv, 0, staging + '\n')
                if argv[0] == 'rsync':
                    shutil.copytree(argv[-2], argv[-1], dirs_exist_ok=True, symlinks=True)
                    return subprocess.CompletedProcess(argv, 0)
                self.assertEqual(argv[0], str(script))
                self.assertEqual(kwargs['cwd'], kernel)
                self.assertEqual(kwargs['env']['srctree'], str(source))
                self.assertEqual(kwargs['env']['SRCARCH'], 'arm64')
                destination = Path(argv[1])
                destination.mkdir(parents=True, exist_ok=True)
                (destination / 'Makefile').write_text('original external-module makefile\n')
                shutil.copy2(kernel / 'Module.symvers', destination / 'Module.symvers')
                return subprocess.CompletedProcess(argv, 0)

            with patch.object(lane.subprocess, 'run', side_effect=package) as run:
                result = install(source, kernel, rootfs)
            self.assertEqual(run.call_count, 3)
            self.assertEqual(link.readlink(), Path('/usr/src/linux-headers-6.18.44'))
            self.assertTrue((rootfs / 'usr/src/linux-headers-6.18.44/Makefile').is_file())
            sample = rootfs / 'usr/src/linux-headers-6.18.44/samples/livepatch/livepatch-sample.ko'
            self.assertTrue(sample.is_file(), 'the original BPF test loads the configured livepatch sample')
            self.assertEqual(sample.read_bytes(), b'original linked sample')
            self.assertEqual(result['release'], '6.18.44')
            self.assertIn('scriptSha256', result)

    def test_rootfs_capacity_includes_matching_modules_without_copying_the_archive(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / 'root'
            root.mkdir()
            # Sparse input exercises sizing without allocating a 3 GiB fixture.
            with (root / 'modules.ko').open('wb') as module:
                module.truncate(3 << 30)
            child = MagicMock()
            child.__enter__.return_value = child
            child.wait.return_value = 0
            with patch.object(lane.subprocess, 'Popen', return_value=child) as spawn, \
                 patch.object(lane, 'write_rootfs_tar') as archive:
                lane.format_rootfs(root, Path(temp) / 'scratch.img')
            self.assertEqual(spawn.call_args.args[0][-1], '4096M')
            command = spawn.call_args.args[0]
            self.assertEqual(command[command.index('-O') + 1], 'verity')
            archive.assert_called_once_with(root, child.stdin, None)

    def test_full_runtime_keeps_disk_backed_temporary_files_for_fsverity(self):
        source = (PATH.parent / 'selftest-init.c').read_text()
        self.assertIn('if (!full_suite && mount("tmpfs", "/tmp"', source,
                      'tmpfs cannot exercise the original fs-verity signature test')
        self.assertIn('KSELFTEST_SETUP /tmp ext4', source)

    def test_disk_backed_tmp_is_also_a_mountpoint_for_namespace_tests(self):
        source = (PATH.parent / 'selftest-init.c').read_text()
        full_setup = source.split('if (full_suite) {\n        if (chmod("/tmp"', 1)
        self.assertEqual(len(full_setup), 2, 'full-suite mode must keep the ext4 directory')
        full_setup = full_setup[1].split('\n    }', 1)[0]
        self.assertIn('mount("/tmp", "/tmp", NULL, MS_BIND, NULL)', full_setup,
                      'MS_PRIVATE on a plain ext4 directory returns EINVAL before openat2 runs')
        self.assertIn('fatal("bind-mount disk-backed tmp")', full_setup,
                      'do not silently continue after a failed setup mount')

    def test_btf_build_checks_capability_and_uses_same_kernel_without_mutating_it(self):
        prepare = getattr(lane, 'prepare_btf_input', None)
        self.assertTrue(callable(prepare), 'pahole version alone silently loses arena attributes')
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            kernel, source, work = (base / name for name in ('kernel', 'source', 'work'))
            kernel.mkdir()
            source.mkdir()
            work.mkdir()
            (kernel / '.config').write_text('CONFIG_DEBUG_INFO_BTF=y\n')
            (kernel / 'vmlinux').write_bytes(b'original kernel with DWARF')
            encoder = base / 'pahole'
            encoder.write_bytes(b'sealed pahole 1.30')
            commands = []

            def run(argv, **kwargs):
                commands.append((argv, kwargs))
                output = ''
                if argv[-1] == '--supported_btf_features':
                    output = ('decl_tag,type_tag,attributes\n' if 'LD_LIBRARY_PATH' in kwargs.get('env', {})
                              else 'decl_tag,type_tag\n')
                elif argv[0] == 'make':
                    library = Path(argv[-1])
                    library.parent.mkdir(parents=True, exist_ok=True)
                    library.write_bytes(b'qualified source-built libbpf')
                else:
                    dest = next(a.split('=', 1)[1] for a in argv if a.startswith('--btf_encode_detached='))
                    Path(dest).write_bytes(b'detached attributes BTF')
                return subprocess.CompletedProcess(argv, 0, output)

            with patch.object(lane.shutil, 'which', return_value=str(encoder)), \
                 patch.object(lane.subprocess, 'run', side_effect=run), \
                 patch.dict(lane.os.environ, {}, clear=True):
                result = prepare(source, kernel, work, 2)
            self.assertEqual((kernel / 'vmlinux').read_bytes(), b'original kernel with DWARF')
            self.assertIn('attributes', result['features'])
            self.assertEqual(result['sourceKernelSha256'], hashlib.sha256(b'original kernel with DWARF').hexdigest())
            self.assertEqual(result['outputSha256'], hashlib.sha256(b'detached attributes BTF').hexdigest())
            self.assertEqual(result['libbpfSha256'], hashlib.sha256(b'qualified source-built libbpf').hexdigest())
            build = next(args for args, _ in commands if args[0] == 'make')
            self.assertIn(str(source / 'tools/lib/bpf'), build)
            self.assertIn('-j2', build)
            encode = commands[-1][0]
            self.assertIn(str(kernel / 'vmlinux'), encode)
            self.assertTrue(any(a.startswith('--btf_features_strict=') for a in encode))
            options = lane.collection_build_options('bpf', work, kernel, btf_input=Path(result['path']))
            self.assertIn('VMLINUX_BTF=' + result['path'], options)
            self.assertIn('KDIR=' + str(kernel), options)
            self.assertIn('O=' + str(kernel), options)

    def test_btf_missing_capability_is_not_credited_even_after_library_build(self):
        prepare = getattr(lane, 'prepare_btf_input', None)
        self.assertTrue(callable(prepare), 'missing BTF attributes currently go unnoticed')
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            (base / '.config').write_text('CONFIG_DEBUG_INFO_BTF=y\n')
            (base / 'vmlinux').write_bytes(b'kernel')
            (base / 'pahole').write_bytes(b'tool')
            commands = []
            def run(argv, **kwargs):
                commands.append(argv)
                return subprocess.CompletedProcess(argv, 0, 'type_tag,decl_tag\n')
            with patch.object(lane.shutil, 'which', return_value=str(base / 'pahole')), \
                 patch.object(lane.subprocess, 'run', side_effect=run):
                with self.assertRaisesRegex(ValueError, 'attributes'):
                    prepare(base, base, base / 'work', 2)
            self.assertFalse(any(any(a.startswith('--btf_encode_detached=') for a in cmd) for cmd in commands))

    def test_btf_preparation_does_not_manufacture_support_when_kernel_disables_btf(self):
        prepare = getattr(lane, 'prepare_btf_input', None)
        self.assertTrue(callable(prepare))
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            (base / '.config').write_text('# CONFIG_DEBUG_INFO_BTF is not set\n')
            with patch.object(lane.subprocess, 'run') as run:
                self.assertIsNone(prepare(base, base, base, 2))
            run.assert_not_called()

    def test_bpf_build_uses_the_configured_kernel_and_versioned_llvm_tools(self):
        self.assertTrue(hasattr(lane, 'collection_build_options'))
        options = lane.collection_build_options('bpf', Path('/work/full'), Path('/work/kernel'))
        self.assertIn('O=/work/kernel', options)
        self.assertIn('KDIR=/work/kernel', options)
        self.assertIn('LLVM_STRIP=llvm-strip-19', options)
        self.assertIn('LLVM_CONFIG=llvm-config-19', options)
        self.assertIn('VMLINUX_BTF=/work/kernel/vmlinux', options)
        self.assertNotIn('O=/work/full/obj', options)
        other = lane.collection_build_options('mm', Path('/work/full'), Path('/work/kernel'))
        self.assertIn('O=/work/full/obj', other)
        self.assertIn('KDIR=/work/kernel', other)
        livepatch = lane.collection_build_options('livepatch', Path('/work/full'), Path('/work/kernel'))
        self.assertIn('KDIR=/work/kernel', livepatch)
        self.assertIn('O=/work/kernel', livepatch)
        self.assertNotIn('O=/work/full/obj', livepatch)
        self.assertTrue({'openssl', 'python3-docutils', 'xxd'} <= set(lane.BUILD_PACKAGES))

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

    def test_supplemental_full_receipt_keeps_object_and_boot_audits(self):
        self.assertIn('reference_boot_path', inspect.signature(lane.kernel_selection).parameters,
                      'the full composition receipt must not be rewritten as a KUnit result')
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            def digest(path):
                return hashlib.sha256(path.read_bytes()).hexdigest()
            outputs = {side: root / side for side in ('reference', 'subject')}
            boots = {}
            for side, folder in outputs.items():
                (folder / 'arch/arm64/boot').mkdir(parents=True)
                (folder / '.config').write_bytes(b'CONFIG_KUNIT=y\n')
                image = folder / 'arch/arm64/boot/Image'
                image.write_bytes(side.encode())
                (folder / 'vmlinux').write_bytes(b'ELF-' + side.encode())
                serial = root / (side + '.log')
                serial.write_text('nonce-bound boot and shutdown\n')
                boots[side] = {'verdict': 'PASS', 'image': str(image), 'imageSha256': digest(image),
                               'serialLog': str(serial), 'serialLogSha256': digest(serial)}
            obj = outputs['subject'] / 'a.o'
            obj.write_bytes(b'EvoCC object')
            rows = root / 'capture/units.jsonl'
            rows.parent.mkdir()
            rows.write_text(json.dumps({'unit': 'a.o', 'outcome': 'substituted',
                'installedSha256': digest(obj), 'objectSha256AfterRelink': digest(obj)}) + '\n')
            data = {'mode': 'full-kernel', 'status': 'PASS', 'completeComposition': True,
                    'installed': True, 'requestedUnits': 1, 'counts': {'substituted': 1},
                    'substitutedUnits': ['a.o'], 'relinkExitCode': 0,
                    'preparation': {'prepareExitCode': 0, 'verifyPrepareExitCode': 0,
                                    'preparedInputsStable': True, 'failedUnits': [],
                                    'exportTable': {'required': True, 'ready': True,
                                                    'unchangedAfterLink': True}},
                    'referenceRoot': str(outputs['reference']), 'outputRoot': str(outputs['subject']),
                    'evidence': 'capture', 'unitsSha256': digest(rows),
                    'configSha256': digest(outputs['subject'] / '.config'),
                    'imageSha256': boots['subject']['imageSha256'],
                    'vmlinuxSha256': digest(outputs['subject'] / 'vmlinux'), 'boot': boots['subject']}
            receipt, reference_boot = root / 'full.json', root / 'reference-boot.json'
            receipt.write_text(json.dumps(data))
            reference_boot.write_text(json.dumps(boots['reference']))
            def select():
                return lane.kernel_selection(True, root, receipt_path=receipt,
                                             reference_boot_path=reference_boot)
            self.assertEqual(select()['images']['subject'], outputs['subject'] / 'arch/arm64/boot/Image')
            with self.assertRaises(ValueError):
                lane.kernel_selection(True, root, receipt_path=receipt)
            for change in ({'status': 'INCOMPLETE'}, {'completeComposition': False},
                           {'counts': {'substituted': 0, 'rebuilt-by-kbuild': 1}},
                           {'requestedUnits': 2}, {'relinkExitCode': 1},
                           {'unitsSha256': 'changed'}, {'configSha256': 'changed'},
                           {'vmlinuxSha256': 'changed'},
                           {'moduleLink': {'required': True, 'status': 'INCOMPLETE'}},
                           {'preparation': {**data['preparation'], 'preparedInputsStable': False}}):
                with self.subTest(change=change):
                    receipt.write_text(json.dumps({**data, **change}))
                    with self.assertRaises(ValueError):
                        select()
            receipt.write_text(json.dumps(data))
            obj.write_bytes(b'GCC replacement')
            with self.assertRaises(ValueError):
                select()
            obj.write_bytes(b'EvoCC object')
            reference_boot.write_text(json.dumps({**boots['reference'], 'verdict': 'FAIL'}))
            with self.assertRaises(ValueError):
                select()
            reference_boot.write_text(json.dumps(boots['reference']))
            (root / 'subject.log').write_text('modified serial output')
            with self.assertRaises(ValueError):
                select()

    def test_supplemental_module_inventory_is_checked_against_the_linked_files(self):
        check = getattr(lane, 'check_composed_modules', None)
        self.assertTrue(callable(check), 'a boot receipt must not authorize stale reference modules')
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            reference, subject = root / 'reference', root / 'subject'
            reference.mkdir()
            subject.mkdir()
            (reference / 'modules.order').write_text('a.o\n')
            (subject / 'a.ko').write_bytes(b'EvoCC linked module with BTF')
            expected = hashlib.sha256((subject / 'a.ko').read_bytes()).hexdigest()
            data = {'vmlinuxSha256': 'elf', 'configSha256': 'config',
                    'moduleLink': {'required': True, 'status': 'PASS',
                                   'inputHashesUnchanged': True, 'btfBaseSha256': 'elf',
                                   'configSha256': 'config',
                                   'modules': [{'unit': 'a.ko', 'sha256': expected}]}}
            check(data, reference, subject)
            for change in ({'modules': []}, {'status': 'INCOMPLETE'},
                           {'inputHashesUnchanged': False}, {'btfBaseSha256': 'other'},
                           {'modules': [{'unit': '../a.ko', 'sha256': expected}]}):
                with self.subTest(change=change), self.assertRaises(ValueError):
                    check({**data, 'moduleLink': {**data['moduleLink'], **change}}, reference, subject)
            with self.assertRaises(ValueError):
                check({**data, 'moduleLink': None}, reference, subject)
            (subject / 'a.ko').write_bytes(b'reference overwrite')
            with self.assertRaises(ValueError):
                check(data, reference, subject)

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

    def test_cpu_cgroup_sequence_is_not_cut_off_before_its_source_duration(self):
        build = {'programTimeoutSeconds': 60}
        self.assertEqual(lane.program_timeout(build, 'cgroup:test_cpu'), 135)
        self.assertEqual(lane.program_timeout(build, 'cgroup:test_kmem'), 60)
        policy = lane.PROGRAM_TIMEOUT_OVERRIDES['cgroup:test_cpu']
        self.assertIn('46', policy['reason'])
        self.assertIn('test_cpu.c', policy['source'])

    def test_sched_ext_hotplug_uses_an_explicit_fixed_cpu_lane_and_program_budget(self):
        environment = getattr(lane, 'program_environment', None)
        self.assertTrue(callable(environment), 'HVF cpu_die is not a sched_ext correctness verdict')
        self.assertEqual(environment('sched_ext:runner', True), {
            'K1_SELFTEST_COLLECTION': 'sched_ext:runner', 'K1_KUNIT_AUTORUN': '0',
            'K1_SELFTEST_TIMEOUT': '120', 'K1_BOOT_LANE': 'tcg-cortex-a72'})
        self.assertEqual(lane.program_timeout({'programTimeoutSeconds': 60}, 'sched_ext:runner'), 135)
        self.assertEqual(environment('cgroup:test_kmem', False),
                         {'K1_SELFTEST_COLLECTION': 'cgroup:test_kmem'})
        self.assertEqual(environment('cgroup:test_cpu', True), {
            'K1_SELFTEST_COLLECTION': 'cgroup:test_cpu', 'K1_KUNIT_AUTORUN': '0',
            'K1_SELFTEST_TIMEOUT': '120'})

    def test_kunit_filter_is_explicit_and_cannot_add_kernel_options(self):
        command = ['bash', '-c', 'source "$1"; k1_kunit_cmdline "$2"',
                   'kunit-test', str(PATH.parent / 'lib.sh')]
        done = subprocess.run(command + ['pm_runtime_test_cases.*'], text=True, capture_output=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(done.stdout, ' kunit.filter_glob=pm_runtime_test_cases.*')
        for value in ['suite.* init=/bin/sh', '$(false)', 'suite\n', 'a' * 257]:
            self.assertNotEqual(subprocess.run(command + [value], capture_output=True).returncode, 0)

    def test_nested_kvm_boot_mode_is_explicit_and_default_is_unchanged(self):
        command = ['bash', '-c', 'source "$1"; k1_kvm_cmdline "$2"',
                   'kvm-mode-test', str(PATH.parent / 'lib.sh')]
        for value, expected in [('', ''), ('nested', ' kvm-arm.mode=nested')]:
            done = subprocess.run(command + [value], text=True, capture_output=True)
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertEqual(done.stdout, expected)
        for value in ['none', 'protected', 'nested init=/bin/sh', 'nested\n']:
            self.assertNotEqual(subprocess.run(command + [value], capture_output=True).returncode, 0)
        self.assertIn('k1_kvm_cmdline "${K1_KVM_MODE:-}"',
                      (PATH.parent / 'lib.sh').read_text())
        self.assertIn('kvmMode:$kvmMode', (PATH.parent / 'boot.sh').read_text())

    def test_nested_kvm_mode_is_in_the_declared_program_policy(self):
        profile = {'lane': 'diagnostic', 'kvmMode': 'nested', 'reason': 'original nested EL2 test'}
        with patch.object(lane, 'PROGRAM_BOOT_LANES', {'kvm:hello_el2': profile}):
            self.assertEqual(lane.program_environment('kvm:hello_el2', True)['K1_KVM_MODE'],
                             'nested')
            self.assertNotIn('K1_KVM_MODE', lane.program_environment('kvm:dirty_log_test', True))
            self.assertEqual(lane.boot_policies()['kvm:hello_el2']['kvmMode'], 'nested')

    def test_kvm_nested_el2_uses_the_tested_cpu_and_its_documented_boot_mode(self):
        environment = lane.program_environment('kvm:hello_el2', True)
        self.assertEqual(environment.get('K1_BOOT_LANE'), 'tcg-neoverse-n2-el2')
        self.assertEqual(environment.get('K1_KVM_MODE'), 'nested')
        self.assertNotIn('K1_SELFTEST_ARGS', environment)
        self.assertNotIn('K1_SELFTEST_TIMEOUT', environment)
        for ident in ('kvm:host_sve', 'kvm:aarch32_id_regs', 'kvm:dirty_log_test'):
            self.assertNotIn('K1_KVM_MODE', lane.program_environment(ident, True))

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
        self.assertIn('setenv("KBUILD_OUTPUT", build, 1)', source)
        self.assertIn('"/lib/modules/%s/build", kernel.release', source)
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
        self.assertIn('bpftrace', lane.RUNTIME_PACKAGES)
        self.assertIn('keyutils', lane.RUNTIME_PACKAGES)
        self.assertIn('fsverity', lane.RUNTIME_PACKAGES)

    def test_ovpn_runtime_installs_original_peer_tables_and_key(self):
        with tempfile.TemporaryDirectory() as temp:
            source, installed = Path(temp) / 'source', Path(temp) / 'installed'
            (source / 'ublk/trace').mkdir(parents=True)
            (source / 'ublk/test_common.sh').write_text('ublk helper\n')
            (installed / 'ublk').mkdir(parents=True)
            (source / 'net/ovpn').mkdir(parents=True)
            (installed / 'net/ovpn').mkdir(parents=True)
            program = installed / 'net/ovpn/test.sh'
            program.write_bytes(b'original script\n')
            files = {'udp_peers.txt': b'1 10.10.1.1 1 10.10.1.2 2\n',
                     'tcp_peers.txt': b'1 5.5.5.2\n', 'data64.key': b'original key\n'}
            for name, contents in files.items():
                (source / 'net/ovpn' / name).write_bytes(contents)
            helpers = lane.install_runtime_helpers(source, installed)
            for name, contents in files.items():
                target = installed / 'net/ovpn' / name
                self.assertTrue(target.is_file(), f'missing OVPN helper {name}')
                self.assertEqual(target.read_bytes(), contents)
                self.assertIn('net/ovpn/' + name, helpers)
            self.assertEqual(program.read_bytes(), b'original script\n')
            # An installed suite cannot silently acquire an invented table/key
            # if one of the frozen source inputs is absent.
            (source / 'net/ovpn/data64.key').unlink()
            with self.assertRaises(FileNotFoundError):
                lane.install_runtime_helpers(source, installed)

    def test_tcp_selftests_have_a_real_netcat_dependency(self):
        self.assertIn('netcat-openbsd', lane.RUNTIME_PACKAGES,
                      'the original BPF TCP scripts execute nc, not socat')

    def test_runtime_keeps_alsa_configuration_data(self):
        self.assertIn("'usr/share/alsa'", inspect.getsource(lane.refresh_runtime),
                      'libasound cannot open the original hw: timer without alsa.conf')

    def test_runtime_ethtool_supplement_preserves_pinned_upstream_identity(self):
        install = getattr(lane, 'install_runtime_ethtool', None)
        self.assertTrue(callable(install), 'ethtool 6.14 cannot answer the original JSON channels query')
        with tempfile.TemporaryDirectory() as temp:
            work = Path(temp)
            archive = work / 'runtime-tools/ethtool-6.15.tar.xz'
            archive.parent.mkdir()
            archive.write_bytes(b'pinned archive')
            source = archive.parent / 'ethtool-6.15'
            calls = []

            def build(argv, **kwargs):
                calls.append(argv)
                if argv[0] == 'tar':
                    source.mkdir()
                elif argv[0] == 'make':
                    (source / 'ethtool').write_bytes(b'original upstream ethtool 6.15')
                return subprocess.CompletedProcess(argv, 0)

            with patch.object(lane, 'RUNTIME_ETHTOOL_SHA256', lane.file_sha256(archive)), \
                    patch.object(lane.subprocess, 'run', side_effect=build), \
                    patch.object(lane.subprocess, 'check_output', return_value='ethtool version 6.15\n'):
                result = install(work, work / 'rootfs')
            self.assertEqual(result['version'], '6.15')
            self.assertEqual(result['archiveSha256'], lane.file_sha256(archive))
            self.assertEqual(result['binarySha256'], lane.file_sha256(work / 'rootfs/usr/sbin/ethtool'))
            self.assertEqual(calls[-1], ['make', '-j1'])
            self.assertEqual(result['commands'], calls)
            archive.write_bytes(b'changed archive')
            with self.assertRaisesRegex(ValueError, 'ethtool.*checksum'):
                install(work, work / 'rootfs')

    def test_original_bpf_tool_is_available_on_the_guest_path(self):
        with tempfile.TemporaryDirectory() as temp:
            rootfs = Path(temp)
            original = rootfs / 'tests/bpf/bpftool'
            original.parent.mkdir(parents=True)
            original.write_bytes(b'original upstream tool')
            lane.install_runtime_paths(rootfs)
            alias = rootfs / 'usr/sbin/bpftool'
            self.assertTrue(alias.is_symlink(), 'drivers/net invokes bpftool through PATH')
            self.assertEqual(alias.readlink(), Path('../../tests/bpf/bpftool'))
            self.assertEqual(alias.read_bytes(), original.read_bytes())

    def test_bpf_flavors_reuse_the_matching_runtime_helpers(self):
        with tempfile.TemporaryDirectory() as temp:
            source, installed = Path(temp) / 'source', Path(temp) / 'installed'
            (source / 'ublk/trace').mkdir(parents=True)
            (source / 'ublk/test_common.sh').write_text('original ublk helper\n')
            (installed / 'ublk').mkdir(parents=True)
            primary = installed / 'bpf'
            primary.mkdir()
            helpers = ('urandom_read', 'liburandom_read.so', 'xdp_synproxy',
                       'sign-file', 'uprobe_multi', 'ima_setup.sh', 'verify_sig_setup.sh',
                       'bpf_testmod.ko', 'btf_dump_test_case_multidim.c')
            for name in helpers:
                (primary / name).write_text('current runtime ' + name)
            for flavor in ('no_alu32', 'cpuv4'):
                (primary / flavor).mkdir()
                (primary / flavor / 'program.bpf.o').write_text('original flavor BPF')
            lane.install_runtime_helpers(source, installed)
            for flavor in ('no_alu32', 'cpuv4'):
                for name in helpers:
                    alias = primary / flavor / name
                    self.assertTrue(alias.is_symlink(), f'{flavor} is missing {name}')
                    self.assertEqual(alias.readlink(), Path('..') / name)
                    self.assertEqual(alias.read_bytes(), (primary / name).read_bytes())
                self.assertEqual((primary / flavor / 'program.bpf.o').read_text(),
                                 'original flavor BPF')
            # A helper rebuilt for a new vmlinux/BTF base must not leave stale
            # flavor-specific copies from the reference kernel behind.
            (primary / 'bpf_testmod.ko').write_bytes(b'new kernel helper')
            self.assertEqual((primary / 'no_alu32/bpf_testmod.ko').read_bytes(),
                             b'new kernel helper')

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

    def test_ublk_worker_failure_survives_program_and_campaign_accounting(self):
        spec = importlib.util.spec_from_file_location('ublk_worker_probe', PATH.parent / 'selftest.py')
        selftest = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(selftest)
        for inner in ('# stress_04 : [FAIL]\n', "# /dev/ublkc7 isn't removed res \n"):
            with self.subTest(inner=inner):
                output = ('1..1\n' + inner + '# stress_04 : [PASS]\n'
                          'ok 1 selftests: ublk: test_stress_04.sh\n')
                parsed = selftest.parse_tap(output)
                case = lane.program_case('ublk:test_stress_04.sh', {
                    'boot': {'verdict': 'PASS'}, 'programs': [dict(parsed, outcome='PASS', exitCode=0)]})
                self.assertEqual(case['outcome'], 'PASS', 'retain the observed wrapper verdict')
                summary = lane.summarize([
                    {'id': 'ublk', 'buildExit': 0, 'tests': ['ublk:test_stress_04.sh']}],
                    [{'id': 'ublk', 'valid': True, 'cases': [case]}])
                self.assertEqual(summary['nestedFailureReview'], ['ublk:test_stress_04.sh'])
                self.assertEqual(summary['status'], 'INCOMPLETE')

    def test_successful_wrapper_cannot_claim_skipped_inner_assertions(self):
        spec = importlib.util.spec_from_file_location('selftest_skip_probe', PATH.parent / 'selftest.py')
        selftest = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(selftest)
        output = ('TAP version 13\n1..1\n'
                  '# # Totals: pass:0 fail:0 xfail:0 xpass:0 skip:1 error:0\n'
                  'ok 1 selftests: filesystems/fuse: fusectl_test\n')
        parsed = selftest.parse_tap(output)
        self.assertEqual(parsed.get('nestedSkipSummaries'), [
            {'line': '# # Totals: pass:0 fail:0 xfail:0 xpass:0 skip:1 error:0', 'passed': 0, 'skipped': 1}])
        case = lane.program_case('filesystems/fuse:fusectl_test', {
            'boot': {'verdict': 'PASS'}, 'programs': [dict(parsed, outcome='PASS', exitCode=0)]})
        self.assertEqual(case['outcome'], 'PASS', 'retain the original program wrapper result')
        summary = lane.summarize([{'id': 'filesystems/fuse', 'buildExit': 0,
                                   'tests': ['filesystems/fuse:fusectl_test']}],
                                 [{'id': 'filesystems/fuse', 'valid': True, 'cases': [case]}])
        self.assertEqual(summary['nestedSkipReview'], ['filesystems/fuse:fusectl_test'])
        self.assertEqual(summary['status'], 'INCOMPLETE')
        self.assertEqual(selftest.parse_tap(output.replace('skip:1', 'skip:0'))['nestedSkipSummaries'], [])

    def test_bpf_summary_preserves_its_distinct_test_and_subtest_counts(self):
        spec = importlib.util.spec_from_file_location('selftest_bpf_probe', PATH.parent / 'selftest.py')
        selftest = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(selftest)
        line = '# Summary: 652/5645 PASSED, 127 SKIPPED, 10 FAILED'
        output = f'TAP version 13\n1..1\n{line}\nok 1 selftests: bpf: test_progs\n'
        parsed = selftest.parse_tap(output)
        self.assertEqual(parsed.get('nestedFailureSummaries'), [
            {'line': line, 'failed': 10, 'errors': 0}])
        self.assertEqual(parsed.get('nestedSkipSummaries'), [
            {'line': line, 'passedTests': 652, 'passedSubtests': 5645, 'skipped': 127}])
        self.assertEqual(parsed['subtests'], {'PASS': 1, 'FAIL': 0, 'SKIP': 0},
                         'BPF counters are not additional top-level TAP cases')
        case = lane.program_case('bpf:test_progs', {
            'boot': {'verdict': 'PASS'}, 'programs': [dict(parsed, outcome='PASS', exitCode=0)]})
        self.assertEqual(case['bpfSummaries'], parsed['bpfSummaries'])
        summary = lane.summarize([{'id': 'bpf', 'buildExit': 0, 'tests': ['bpf:test_progs']}],
                                 [{'id': 'bpf', 'valid': True, 'cases': [case]}])
        self.assertEqual(summary['nestedFailureReview'], ['bpf:test_progs'])
        self.assertEqual(summary['nestedSkipReview'], ['bpf:test_progs'])
        self.assertEqual(summary['status'], 'INCOMPLETE')
        clean = selftest.parse_tap(output.replace('127 SKIPPED, 10 FAILED', '0 SKIPPED, 0 FAILED'))
        self.assertEqual(clean['nestedFailureSummaries'], [])
        self.assertEqual(clean['nestedSkipSummaries'], [])

    def test_truncated_bpf_summary_remains_invalid_at_program_boundary(self):
        spec = importlib.util.spec_from_file_location('bpf_truncated', PATH.parent / 'selftest.py')
        selftest = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(selftest)
        output = (PATH.parents[2] / 'tests/fixtures/kselftest-bpf-truncated.tap').read_text()
        parsed = selftest.parse_tap(output)
        case = lane.program_case('bpf:test_progs', {
            'boot': {'verdict': 'PASS'}, 'programs': [dict(parsed, outcome='PASS', exitCode=0)]})
        self.assertEqual(case['outcome'], 'INVALID_OUTPUT')
        self.assertTrue(case['nestedSummaryErrors'])
        summary = lane.summarize([{'id': 'bpf', 'buildExit': 0, 'tests': ['bpf:test_progs']}],
                                 [{'id': 'bpf', 'valid': False, 'cases': [case]}])
        self.assertEqual(summary['status'], 'INCOMPLETE')
        self.assertEqual(summary['outcomes'], {'INVALID_OUTPUT': 1})

    def test_sched_ext_summary_cannot_be_hidden_by_its_zero_exit_wrapper(self):
        spec = importlib.util.spec_from_file_location('selftest_scx_probe', PATH.parent / 'selftest.py')
        selftest = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(selftest)
        output = ('TAP version 13\n1..1\n# RESULTS:\n# \n'
                  '# PASSED:  22\n# SKIPPED: 0\n# FAILED:  1\n'
                  'ok 1 selftests: sched_ext: runner\n')
        parsed = selftest.parse_tap(output)
        self.assertEqual(parsed['nestedFailureSummaries'], [
            {'line': '# FAILED:  1', 'failed': 1, 'errors': 0}])
        self.assertEqual(parsed['subtests'], {'PASS': 1, 'FAIL': 0, 'SKIP': 0})
        case = lane.program_case('sched_ext:runner', {
            'boot': {'verdict': 'PASS'}, 'programs': [dict(parsed, outcome='PASS', exitCode=0)]})
        self.assertEqual(case['outcome'], 'PASS', 'retain the original outer wrapper verdict')
        summary = lane.summarize([{'id': 'sched_ext', 'buildExit': 0,
                                   'tests': ['sched_ext:runner']}],
                                 [{'id': 'sched_ext', 'valid': True, 'cases': [case]}])
        self.assertEqual(summary['nestedFailureReview'], ['sched_ext:runner'])
        self.assertEqual(summary['status'], 'INCOMPLETE')
        skipped = selftest.parse_tap(output.replace('FAILED:  1', 'FAILED:  0')
                                    .replace('SKIPPED: 0', 'SKIPPED: 1'))
        self.assertEqual(skipped['nestedFailureSummaries'], [])
        self.assertEqual(skipped['nestedSkipSummaries'], [
            {'line': '# SKIPPED: 1', 'passed': 22, 'skipped': 1}])
        clean = selftest.parse_tap(output.replace('FAILED:  1', 'FAILED:  0'))
        self.assertEqual(clean['nestedFailureSummaries'], [])
        self.assertEqual(clean['nestedSkipSummaries'], [])
        # Do not turn arbitrary diagnostic text into a suite result.
        unrelated = selftest.parse_tap(output.replace('# RESULTS:\n# \n', ''))
        self.assertEqual(unrelated['nestedFailureSummaries'], [])

    def test_truncated_scheduler_summary_survives_program_and_campaign_accounting(self):
        spec = importlib.util.spec_from_file_location('selftest_scx_truncated', PATH.parent / 'selftest.py')
        selftest = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(selftest)
        output = (PATH.parents[2] / 'tests/fixtures/kselftest-scheduler-truncated.tap').read_text()
        parsed = selftest.parse_tap(output)
        case = lane.program_case('sched_ext:runner', {
            'boot': {'verdict': 'PASS'}, 'programs': [dict(parsed, outcome='PASS', exitCode=0)]})
        self.assertEqual(case['outcome'], 'INVALID_OUTPUT')
        self.assertEqual(case['nestedSummaryErrors'], parsed['nestedSummaryErrors'])
        summary = lane.summarize([{'id': 'sched_ext', 'buildExit': 0,
                                   'tests': ['sched_ext:runner']}],
                                 [{'id': 'sched_ext', 'valid': False, 'cases': [case]}])
        self.assertEqual(summary['outcomes'], {'INVALID_OUTPUT': 1})
        self.assertEqual(summary['status'], 'INCOMPLETE')

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

    def test_reference_reuse_binds_reference_payload_not_new_subject_modules(self):
        cases = [{'id': 'mm:a', 'outcome': 'PASS', 'exitCode': 0}]
        previous = {
            'status': 'INCOMPLETE', 'build': {'builds': [{'available': ['mm:a']}]},
            'referenceConfigSha256': 'same-config', 'runtimePolicy': {'workers': 1},
            'initramfsSha256': 'old-subject-init', 'rootfsSha256': 'old-subject-disk',
            'payloads': {'reference': {'root': '/reference', 'initramfsSha256': 'ref-init',
                                        'rootfsSha256': 'ref-disk'},
                         'subject': {'rootfsSha256': 'old-subject-disk'}},
            'execution': {'reference': {'collections': [{'cases': cases}]}},
        }
        current = copy.deepcopy(previous)
        current.update(initramfsSha256='new-subject-init', rootfsSha256='new-subject-disk')
        current['payloads']['subject']['rootfsSha256'] = 'new-subject-disk'
        self.assertEqual(lane.reusable_reference(previous, current), cases)
        for key in ('initramfsSha256', 'rootfsSha256'):
            changed = copy.deepcopy(current)
            changed['payloads']['reference'][key] = 'changed-reference'
            with self.subTest(key=key), self.assertRaises(ValueError):
                lane.reusable_reference(previous, changed)
        for missing in ('payloads', 'reference'):
            changed = copy.deepcopy(current)
            if missing == 'payloads':
                del changed['payloads']
            else:
                del changed['payloads']['reference']
            with self.subTest(missing=missing), self.assertRaises(ValueError):
                lane.reusable_reference(previous, changed)


if __name__ == '__main__':
    unittest.main()
