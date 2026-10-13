"""The KUnit runtime payload must follow the retained flights and pass the adapter checks."""

import hashlib
import io
import json
from contextlib import redirect_stderr, redirect_stdout
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
REPO = TOOLS.parents[1]
sys.path.append(str(TOOLS))
import kunit  # noqa: E402
import kunit_payload  # noqa: E402

CONTRACT = json.loads(kunit_payload.CONTRACT.read_text())


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def executable(path, data=b'\x7fELF'):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    path.chmod(0o755)
    return path


def link(path, target):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.symlink_to(target)


def fake_system(root):
    """A merged-/usr layout like the sealed image."""
    executable(root / 'usr/bin/dash', b'dash')
    link(root / 'usr/bin/sh', 'dash')
    executable(root / 'usr/bin/dmesg', b'dmesg')
    executable(root / 'usr/bin/kmod', b'kmod')
    link(root / 'usr/sbin/modprobe', '../bin/kmod')
    executable(root / 'usr/sbin/ip', b'ip')
    link(root / 'lib', 'usr/lib')
    executable(root / 'usr/lib/aarch64-linux-gnu/libc.so.6', b'libc')
    executable(root / 'usr/lib/aarch64-linux-gnu/ld-linux-aarch64.so.1', b'loader')
    link(root / 'usr/lib/ld-linux-aarch64.so.1', 'aarch64-linux-gnu/ld-linux-aarch64.so.1')
    return root


def fake_tree(root, units):
    executable(root / 'arch/arm64/boot/Image', b'image')
    (root / '.config').write_text('CONFIG_MODULES=y\n')
    (root / 'modules.order').write_text(''.join(str(Path(unit).with_suffix('.o')) + '\n' for unit in units))
    return root


def installed_modules(tree, units):
    return {'status': 'PASS', 'release': '6.18.44', 'imageSha256': sha(tree / 'arch/arm64/boot/Image'),
            'configSha256': sha(tree / '.config'), 'vmlinuxSha256': 'vmlinux', 'moduleSymversSha256': 'symvers',
            'modules': [{'unit': unit, 'linkedSha256': 'linked-' + unit, 'installedSha256': 'installed'}
                        for unit in units]}


UNITS = [row['unit'] for row in CONTRACT['suiteModules']] + ['fs/fuse/fuse.ko', 'drivers/net/veth.ko']


class RetainedRecipeTest(unittest.TestCase):
    def test_setup_list_and_module_order_follow_the_retained_flights(self):
        names = [row['name'] for row in CONTRACT['suiteModules']]
        checked = 0
        for capture in ('expanded-reference-modules-20260922', 'expanded-subject-query-modules-20260922'):
            log = REPO / '.evidence/kunit' / capture / 'boot.log'
            if not log.is_file():
                continue
            text = log.read_text(errors='replace')
            with self.subTest(capture=capture):
                self.assertEqual(re.findall(r'^KSELFTEST_SETUP module (\S+) loaded$', text, re.M),
                                 [name for _, name in kunit_payload.SETUP_MODULES])
                self.assertEqual(re.findall(r'^KUNIT_MODULE_BEGIN \S+ (\S+)$', text, re.M), names)
                # No module build tree, no rerun list and no failed pseudo-filesystem mount.
                for absent in ('KUNIT_RERUN', 'KSELFTEST_SETUP KBUILD_OUTPUT', 'KSELFTEST_SETUP unavailable'):
                    self.assertNotIn(absent, text)
            checked += 1
        if not checked:
            self.skipTest('retained KUnit module flights unavailable')

    def test_init_runs_exactly_the_four_command_tools(self):
        source = (TOOLS / 'selftest-init.c').read_text()
        commands = set(re.findall(r'system\("([\w-]+)', source)) | set(re.findall(r'execlp\("([\w-]+)"', source))
        self.assertEqual(commands, {'dmesg', 'ip', 'modprobe'})
        self.assertEqual(set(kunit_payload.COMMAND_TOOLS), commands | {'sh'})
        self.assertIn('"/tests/kunit-modules"', source)
        self.assertIn('"/tests/modules"', source)


class RuntimeFileTest(unittest.TestCase):
    def test_guest_paths_follow_the_merged_usr_layout(self):
        self.assertEqual(kunit_payload.guest_path('/bin/sh'), '/usr/bin/sh')
        self.assertEqual(kunit_payload.guest_path('/sbin/ip'), '/usr/sbin/ip')
        self.assertEqual(kunit_payload.guest_path('/lib/ld-linux-aarch64.so.1'), '/usr/lib/ld-linux-aarch64.so.1')
        self.assertEqual(kunit_payload.guest_path('/usr/bin/dash'), '/usr/bin/dash')
        self.assertEqual(kunit_payload.guest_path('/library/x'), '/library/x')

    def test_ldd_paths_include_the_loader_and_reject_unresolved_libraries(self):
        output = ('\tlinux-vdso.so.1 (0x0000ffff9a3f0000)\n'
                  '\tlibc.so.6 => /lib/aarch64-linux-gnu/libc.so.6 (0x0000ffff9a1e0000)\n'
                  '\t/lib/ld-linux-aarch64.so.1 (0x0000ffff9a3b0000)\n')
        with patch.object(kunit_payload.subprocess, 'check_output', return_value=output):
            self.assertEqual(kunit_payload.libraries('/usr/bin/dash'),
                             ['/lib/aarch64-linux-gnu/libc.so.6', '/lib/ld-linux-aarch64.so.1'])
        for text in ('\tlibmnl.so.0 => not found\n', '\tlinux-vdso.so.1 (0x1)\n'):
            with self.subTest(text=text), patch.object(kunit_payload.subprocess, 'check_output', return_value=text):
                with self.assertRaises(ValueError):
                    kunit_payload.libraries('/usr/sbin/ip')

    def test_symlink_chains_and_libraries_are_copied_to_guest_paths(self):
        with tempfile.TemporaryDirectory() as tmp:
            system, rootfs, metadata = fake_system(Path(tmp) / 'system'), Path(tmp) / 'rootfs', {}
            rootfs.mkdir()
            self.assertEqual(kunit_payload.copy_into(rootfs, '/usr/sbin/modprobe', metadata, system),
                             ['/usr/sbin/modprobe', '/usr/bin/kmod'])
            self.assertEqual(os.readlink(rootfs / 'usr/sbin/modprobe'), '../bin/kmod')
            self.assertEqual((rootfs / 'usr/bin/kmod').read_bytes(), b'kmod')
            self.assertEqual(kunit_payload.copy_into(rootfs, '/lib/ld-linux-aarch64.so.1', metadata, system),
                             ['/usr/lib/ld-linux-aarch64.so.1', '/usr/lib/aarch64-linux-gnu/ld-linux-aarch64.so.1'])
            self.assertEqual((rootfs / 'usr/lib/aarch64-linux-gnu/ld-linux-aarch64.so.1').read_bytes(), b'loader')
            link(system / 'usr/bin/first', 'second')
            link(system / 'usr/bin/second', 'first')
            with self.assertRaisesRegex(ValueError, 'loop'):
                kunit_payload.copy_into(rootfs, '/usr/bin/first', metadata, system)
            with self.assertRaisesRegex(ValueError, 'missing'):
                kunit_payload.copy_into(rootfs, '/usr/bin/absent', metadata, system)

    def test_four_tools_carry_their_libraries(self):
        with tempfile.TemporaryDirectory() as tmp:
            system, rootfs = fake_system(Path(tmp) / 'system'), Path(tmp) / 'rootfs'
            rootfs.mkdir()
            with patch.object(kunit_payload, 'libraries',
                              return_value=['/lib/aarch64-linux-gnu/libc.so.6', '/lib/ld-linux-aarch64.so.1']):
                tools = kunit_payload.install_command_tools(rootfs, {}, system)
            self.assertEqual(list(tools), list(kunit_payload.COMMAND_TOOLS))
            self.assertEqual(tools['sh']['guestPaths'], ['/usr/bin/sh', '/usr/bin/dash'])
            self.assertEqual(tools['ip']['command'], '/usr/sbin/ip')
            self.assertIn('/usr/lib/aarch64-linux-gnu/libc.so.6', tools['dmesg']['libraries'])
            self.assertTrue((rootfs / 'usr/lib/aarch64-linux-gnu/libc.so.6').is_file())
            (system / 'usr/sbin/ip').unlink()
            with self.assertRaisesRegex(ValueError, 'install-deps'), \
                 patch.object(kunit_payload, 'libraries', return_value=['/lib/ld-linux-aarch64.so.1']):
                kunit_payload.install_command_tools(Path(tmp) / 'other', {}, system)


class WorkerTest(unittest.TestCase):
    def worker(self, tmp, units):
        payload, harness = Path(tmp) / 'payload', Path(tmp) / 'harness'
        payload.mkdir()
        harness.mkdir()
        (harness / 'selftest-init.c').write_text('int main(void) { return 0; }\n')
        tree = fake_tree(Path(tmp) / 'kernel', units)
        calls = []

        def run(command, **kwargs):
            calls.append(command)
            if command[0] == 'gcc-14':
                Path(command[command.index('-o') + 1]).write_bytes(b'init')
            else:
                kwargs['stdout'].write(b'cpio')
            return subprocess.CompletedProcess(command, 0)

        def format_rootfs(rootfs, image, metadata):
            image.write_bytes(b'ext4:' + (rootfs / 'tests/kunit-modules').read_bytes())

        with patch.object(kunit_payload, 'install_command_tools', return_value={'sh': {}}), \
             patch.object(kunit_payload, 'tool_packages', return_value={}), \
             patch.object(kunit_payload.selftest_full, 'install_kernel_modules',
                          return_value=installed_modules(tree, units)), \
             patch.object(kunit_payload.selftest_full, 'format_rootfs', side_effect=format_rootfs), \
             patch.object(kunit_payload.subprocess, 'run', side_effect=run):
            kunit_payload.worker('reference', payload, Path(tmp) / 'src', tree, harness,
                                 kunit_payload.CONTRACT, Path('/work/bin/gen_init_cpio'))
        return payload, tree, calls

    def test_payload_has_the_recorded_lists_and_passes_the_adapter_checks(self):
        with tempfile.TemporaryDirectory() as tmp:
            payload, tree, calls = self.worker(tmp, UNITS)
            tests = payload / 'rootfs/tests'
            self.assertEqual((tests / 'kunit-modules').read_text().splitlines(),
                             [row['name'] for row in CONTRACT['suiteModules']])
            self.assertEqual((tests / 'modules').read_text(), 'fuse\n')
            self.assertFalse((tests / 'kunit-rerun').exists())
            self.assertIn('file /full-suite /', (payload / 'spec.txt').read_text())
            self.assertEqual(calls[0][:6], list(kunit_payload.INIT_COMMAND))
            self.assertEqual(calls[1][1:3], ['-t', '0'])
            build = json.loads((payload / 'build.json').read_text())
            self.assertEqual(build['suiteModules'], CONTRACT['suiteModules'])
            self.assertEqual(build['setupModules'], ['fuse'])
            self.assertNotIn('builds', build)
            hashes = kunit.runtime_inputs(payload, tree / 'arch/arm64/boot/Image', tree / '.config', None, CONTRACT)
            self.assertEqual(hashes['scratch.img'], build['rootfsSha256'])

    def test_missing_contract_or_setup_module_stops_the_payload(self):
        for missing in (CONTRACT['suiteModules'][0]['unit'], 'fs/fuse/fuse.ko'):
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as tmp:
                with self.assertRaisesRegex(ValueError, 'not installed'):
                    self.worker(tmp, [unit for unit in UNITS if unit != missing])
                self.assertFalse((Path(tmp) / 'payload/build.json').exists())

    def test_stale_staging_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / 'payload/rootfs').mkdir(parents=True)
            with self.assertRaisesRegex(ValueError, 'not empty'):
                kunit_payload.worker('subject', Path(tmp) / 'payload', contract_path=kunit_payload.CONTRACT)


class HostTest(unittest.TestCase):
    def selection(self, tmp):
        tree = fake_tree(Path(tmp) / 'tree', UNITS)
        image, config = tree / 'arch/arm64/boot/Image', tree / '.config'
        return {'images': {'reference': image, 'subject': image}, 'configs': {'reference': config, 'subject': config},
                'moduleLink': {'modules': []}}

    def args(self, tmp):
        return types.SimpleNamespace(output_root=Path(tmp) / 'runtime', source_root=Path(tmp) / 'src',
                                     jobs=1, install_deps=False)

    @staticmethod
    def container(command, **_):
        staging = Path(next(part for part in command if part.endswith(':/work/payload')).rsplit(':', 1)[0])
        (staging / 'rootfs/usr').mkdir(parents=True)
        for name in ('build.json', 'initramfs.cpio', 'scratch.img'):
            (staging / name).write_text(name)
        return subprocess.CompletedProcess(command, 0)

    def test_payload_is_checked_then_moved_into_place_without_its_staging_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            selected, args = self.selection(tmp), self.args(tmp)
            with patch.object(kunit_payload.subprocess, 'run', side_effect=self.container), \
                 patch.object(kunit, 'runtime_inputs') as check, redirect_stdout(io.StringIO()):
                for side in kunit_payload.SIDES:
                    final = kunit_payload.build_side(side, selected, CONTRACT, args, 'image')
                    self.assertEqual(final, args.output_root / side)
                    self.assertTrue((final / 'build.json').is_file())
                    self.assertFalse((final / 'rootfs').exists())
                    self.assertFalse((args.output_root / f'.{side}.partial').exists())
            self.assertIsNone(check.call_args_list[0].args[3])
            self.assertEqual(check.call_args_list[1].args[3], selected['moduleLink'])
            with self.assertRaisesRegex(ValueError, 'never replaced'):
                kunit_payload.build_side('reference', selected, CONTRACT, args, 'image')

    def test_failed_container_keeps_the_log_and_removes_only_the_root(self):
        def fails(command, **kwargs):
            self.container(command)
            raise subprocess.CalledProcessError(1, command)
        with tempfile.TemporaryDirectory() as tmp:
            selected, args = self.selection(tmp), self.args(tmp)
            with patch.object(kunit_payload.subprocess, 'run', side_effect=fails), \
                 patch.object(kunit, 'runtime_inputs') as check, redirect_stdout(io.StringIO()):
                with self.assertRaises(subprocess.CalledProcessError):
                    kunit_payload.build_side('subject', selected, CONTRACT, args, 'image')
                staging = args.output_root / '.subject.partial'
                self.assertTrue((staging / 'build.log').is_file())
                self.assertFalse((staging / 'rootfs').exists())
                self.assertFalse((args.output_root / 'subject').exists())
                with self.assertRaisesRegex(ValueError, 'earlier run'):
                    kunit_payload.build_side('subject', selected, CONTRACT, args, 'image')
            check.assert_not_called()

    def test_container_has_no_network_unless_dependencies_are_requested(self):
        command = kunit_payload.container_command('subject', Path('/tree'), Path('/stage'), Path('/src'),
                                                  'image', 2, False)
        self.assertEqual(command[command.index('--network') + 1], 'none')
        for mount in ('/tree:/work/kernel:ro', '/src:/work/src:ro', '/stage:/work/payload'):
            self.assertIn(mount, command)
        self.assertTrue(command[-1].startswith('exec python3 /work/harness/kunit_payload.py --worker --side subject'))
        command = kunit_payload.container_command('subject', Path('/tree'), Path('/stage'), Path('/src'),
                                                  'image', 2, True)
        self.assertNotIn('--network', command)
        self.assertIn('apt-get install -y --no-install-recommends iproute2 e2fsprogs', command[-1])

    def test_tree_must_link_every_contract_and_setup_module(self):
        with tempfile.TemporaryDirectory() as tmp:
            kunit_payload.check_tree(fake_tree(Path(tmp) / 'full', UNITS), CONTRACT)
            with self.assertRaisesRegex(ValueError, 'fuse'):
                kunit_payload.check_tree(fake_tree(Path(tmp) / 'short', UNITS[:-2]), CONTRACT)

    def test_check_audits_both_trees_and_builds_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            selected = self.selection(tmp)
            contract = dict(CONTRACT, configSha256=sha(selected['configs']['subject']))
            contract_path = Path(tmp) / 'contract.json'
            contract_path.write_text(json.dumps(contract))
            source = Path(tmp) / 'src/scripts'
            source.mkdir(parents=True)
            (source / 'Makefile.modinst').write_text('')
            argv = ['--check', '--reference-boot', 'boot.json', '--source-root', str(source.parent),
                    '--output-root', str(Path(tmp) / 'runtime')]
            with patch.object(kunit_payload, 'CONTRACT', contract_path), \
                 patch.object(kunit_payload.selftest_full, 'kernel_selection', return_value=selected) as select, \
                 patch.object(kunit_payload.subprocess, 'run') as run, \
                 redirect_stdout(io.StringIO()) as output:
                self.assertEqual(kunit_payload.main(argv), 0)
                self.assertIn('preflight PASS', output.getvalue())
                self.assertEqual(output.getvalue().count('--worker --side'), 2)
                run.assert_not_called()
                self.assertEqual(select.call_args.kwargs['reference_boot_path'], Path('boot.json'))
                contract_path.write_text(json.dumps(dict(contract, configSha256='other')))
                with redirect_stderr(io.StringIO()) as errors:
                    self.assertEqual(kunit_payload.main(argv), 2)
                self.assertIn('differs from the KUnit contract', errors.getvalue())
            self.assertFalse((Path(tmp) / 'runtime').exists())

    def test_reference_boot_is_required_before_any_audit(self):
        with patch.object(kunit_payload.selftest_full, 'kernel_selection') as select, \
             redirect_stderr(io.StringIO()) as errors:
            self.assertEqual(kunit_payload.main(['--check']), 2)
        select.assert_not_called()
        self.assertIn('--reference-boot is required', errors.getvalue())

    def test_default_output_is_the_adapter_default_input(self):
        defaults = kunit.parser().parse_args([])
        output = kunit_payload.parser().parse_args([]).output_root
        self.assertEqual(defaults.reference_runtime, output / 'reference')
        self.assertEqual(defaults.subject_runtime, output / 'subject')


if __name__ == '__main__':
    unittest.main()
