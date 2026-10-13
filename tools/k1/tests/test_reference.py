"""The reference build must keep its solved configuration and build identity."""

import io
import json
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
REPO = TOOLS.parents[1]
sys.path.append(str(TOOLS))
import layout  # noqa: E402
import reference  # noqa: E402


def fake_source(root):
    (root / 'scripts/kconfig').mkdir(parents=True, exist_ok=True)
    (root / 'Makefile').write_text('# Linux\n')
    return root


class ReferenceScriptTest(unittest.TestCase):
    def setUp(self):
        self.environment = json.loads(layout.BUILD_ENVIRONMENT.read_text())

    def test_build_identity_is_the_frozen_contract(self):
        identity = reference.build_identity(self.environment)
        values = self.environment['reproducibility']
        for name in ('KBUILD_BUILD_TIMESTAMP', 'SOURCE_DATE_EPOCH', 'KBUILD_BUILD_USER', 'KBUILD_BUILD_HOST'):
            self.assertEqual(identity[name], values[name])
        self.assertEqual(identity['KBUILD_BUILD_HOST'], 'nexlab')
        self.assertEqual(identity['KBUILD_BUILD_VERSION'], '1')

    def test_configuration_must_be_a_fixed_point_before_and_after_the_build(self):
        identity = reference.build_identity(self.environment)
        script = reference.build_script(identity, '022', 4, False)
        lines = script.splitlines()
        make = 'make -C /work/src O=/work/out ARCH=arm64 CC=gcc-14 HOSTCC=gcc-14'
        self.assertEqual(lines[:2], ['set -eu', 'umask 022'])
        self.assertIn(f'{make} olddefconfig', lines)
        self.assertEqual(lines.index('cmp /work/out/.config /work/config'), lines.index(f'{make} olddefconfig') + 1)
        self.assertEqual(lines[-2:], [f'{make} -j4 Image modules', 'cmp /work/out/.config /work/config'])
        self.assertIn("KBUILD_BUILD_TIMESTAMP='Sun Aug 9 18:25:35 UTC 2026'", script)
        configure_only = reference.build_script(identity, '022', 4, True)
        self.assertNotIn('Image modules', configure_only)
        self.assertTrue(configure_only.endswith('cmp /work/out/.config /work/config\n'))

    def test_container_reads_the_source_and_configuration_only(self):
        command = reference.container_command('image', Path('/src'), Path('/out'), Path('/c6.config'), 4, '10G')
        self.assertIn('/src:/work/src:ro', command)
        self.assertIn('/c6.config:/work/config:ro', command)
        self.assertIn('/out:/work/out', command)
        self.assertEqual(command[command.index('--network') + 1], 'none')
        self.assertEqual(command[command.index('--cpus') + 1], '4')


class ReferenceTreeTest(unittest.TestCase):
    def test_new_tree_copies_the_configuration_and_another_tree_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            config = Path(tmp) / 'c6.config'
            config.write_text('CONFIG_A=y\n')
            output = Path(tmp) / 'pristine'
            self.assertEqual(reference.prepare_output(output, config), 'new')
            self.assertEqual((output / '.config').read_text(), 'CONFIG_A=y\n')
            (output / 'vmlinux.o').write_text('partial')
            self.assertEqual(reference.prepare_output(output, config), 'continue')
            (output / '.config').write_text('CONFIG_B=y\n')
            with self.assertRaisesRegex(ValueError, 'another tree'):
                reference.prepare_output(output, config)
            (output / '.config').unlink()
            with self.assertRaisesRegex(ValueError, 'another tree'):
                reference.prepare_output(output, config)

    def test_configured_source_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = fake_source(Path(tmp))
            reference.check_source(source)
            (source / 'include/config').mkdir(parents=True)
            with self.assertRaisesRegex(ValueError, 'clean tree'):
                reference.check_source(source)

    def test_inventory_compares_units_and_modules_with_a_retained_capture(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree, capture = Path(tmp) / 'tree', Path(tmp) / 'capture'
            tree.mkdir()
            capture.mkdir()
            (tree / 'modules.order').write_text('fs/fuse/fuse.o\nlib/test.o\n')
            (capture / 'units.jsonl').write_text('{"unit": "a.o"}\n{"unit": "b.o"}\n')
            (capture / 'module-link.json').write_text(json.dumps(
                {'modules': [{'unit': 'lib/test.ko'}, {'unit': 'fs/fuse/fuse.ko'}]}))
            fake = types.SimpleNamespace(target_c_records=lambda root: [('b.o', None), ('a.o', None)])
            with patch.dict(sys.modules, {'substitute': fake}):
                result = reference.inventory(tree, capture)
                self.assertTrue(result['unitsEqual'] and result['modulesEqual'])
                fake.target_c_records = lambda root: [('a.o', None), ('c.o', None)]
                result = reference.inventory(tree, capture)
            self.assertFalse(result['unitsEqual'])
            self.assertEqual((result['missingUnits'], result['extraUnits']), (['b.o'], ['c.o']))


class ReferenceCommandTest(unittest.TestCase):
    def run_main(self, tmp, argv, container):
        source = fake_source(Path(tmp) / 'src')
        config = Path(tmp) / 'c6.config'
        config.write_text('CONFIG_A=y\n')
        output = Path(tmp) / 'pristine'
        with patch.object(reference.layout, 'verify_sealed_image', return_value=('image', 'digest')), \
             patch.object(reference.subprocess, 'run', side_effect=container) as run, \
             redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            code = reference.main(['--config', str(config), '--output', str(output),
                                   '--source-root', str(source), *argv])
        record = Path(tmp) / 'pristine-reference.json'
        return code, (json.loads(record.read_text()) if record.exists() else None), run

    @staticmethod
    def built(command, **_):
        out = Path(next(part for part in command if part.endswith(':/work/out')).rsplit(':', 1)[0])
        (out / 'arch/arm64/boot').mkdir(parents=True)
        (out / 'arch/arm64/boot/Image').write_bytes(b'image')
        (out / 'vmlinux').write_bytes(b'vmlinux')
        (out / 'modules.order').write_text('fs/fuse/fuse.o\n')
        return subprocess.CompletedProcess(command, 0)

    def test_completed_build_records_artifacts_and_unchanged_configuration(self):
        with tempfile.TemporaryDirectory() as tmp:
            code, record, _ = self.run_main(tmp, [], self.built)
            self.assertEqual(code, 0)
            self.assertEqual(record['status'], 'PASS')
            self.assertTrue(record['configUnchanged'])
            self.assertEqual(record['artifacts']['moduleCount'], 1)
            self.assertIn('arch/arm64/boot/Image', record['artifacts'])
            self.assertIn('Image modules', record['script'])

    def test_changed_configuration_or_failed_build_is_an_error(self):
        def rewrites(command, **_):
            out = Path(next(part for part in command if part.endswith(':/work/out')).rsplit(':', 1)[0])
            (out / '.config').write_text('CONFIG_A=y\nCONFIG_NEW=y\n')
            return subprocess.CompletedProcess(command, 0)
        with tempfile.TemporaryDirectory() as tmp:
            code, record, _ = self.run_main(tmp, ['--configure-only'], rewrites)
            self.assertEqual((code, record['status'], record['configUnchanged']), (2, 'ERROR', False))
        with tempfile.TemporaryDirectory() as tmp:
            code, record, _ = self.run_main(tmp, [], lambda command, **_: subprocess.CompletedProcess(command, 2))
            self.assertEqual((code, record['status']), (2, 'ERROR'))

    def test_inventory_difference_is_reported_separately(self):
        with tempfile.TemporaryDirectory() as tmp:
            capture = Path(tmp) / 'capture'
            capture.mkdir()
            (capture / 'units.jsonl').write_text('{"unit": "a.o"}\n')
            (capture / 'module-link.json').write_text(json.dumps({'modules': [{'unit': 'fs/fuse/fuse.ko'}]}))
            fake = types.SimpleNamespace(target_c_records=lambda root: [('b.o', None)])
            with patch.dict(sys.modules, {'substitute': fake}):
                code, record, _ = self.run_main(tmp, ['--inventory', str(capture)], self.built)
            self.assertEqual((code, record['status']), (1, 'DIFFERENT_INVENTORY'))
            self.assertEqual(record['inventory']['missingUnits'], ['a.o'])

    def test_dry_run_and_contradictory_options_start_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            code, record, run = self.run_main(tmp, ['--dry-run'], self.built)
            self.assertEqual((code, record), (0, None))
            run.assert_not_called()
            self.assertFalse((Path(tmp) / 'pristine').exists())
            code, record, run = self.run_main(tmp, ['--configure-only', '--inventory', tmp], self.built)
            self.assertEqual((code, record), (2, None))
            run.assert_not_called()


class CaseVolumeTest(unittest.TestCase):
    def test_outputs_on_a_detached_volume_are_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            volume = Path(tmp) / 'case'
            volume.mkdir()
            with self.assertRaisesRegex(ValueError, 'not attached'):
                layout.require_attached(volume / 'final-s/pristine', volume)
            layout.require_attached(Path(tmp) / 'elsewhere', volume)
            with patch.object(layout.os.path, 'ismount', return_value=True):
                layout.require_attached(volume / 'final-s/pristine', volume)


if __name__ == '__main__':
    unittest.main()
