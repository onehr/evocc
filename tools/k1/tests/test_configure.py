"""The configuration solver must reproduce the recorded identities, not new ones."""

import hashlib
import io
import json
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
REPO = TOOLS.parents[1]
sys.path.append(str(TOOLS))
import configure  # noqa: E402


def digest(text):
    return hashlib.sha256(text.encode()).hexdigest()


def fake_source(root):
    """A clean source tree with every fragment that the recipe names."""
    (root / 'scripts/kconfig').mkdir(parents=True)
    (root / 'scripts/kconfig/merge_config.sh').write_text('#!/bin/sh\n')
    for step in configure.STEPS:
        for kind, name in step['fragments']:
            if kind == 'upstream':
                path = root / configure.SELFTESTS / name / 'config'
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('CONFIG_EXAMPLE=y\n')
    return root


class ConfigureRecipeTest(unittest.TestCase):
    def setUp(self):
        self.steps = {step['name']: step for step in configure.STEPS}

    def test_identities_and_requests_come_from_the_retained_records(self):
        kunit = json.loads((REPO / 'tests/contracts/kernel-kunit-receipt.json').read_text())
        self.assertEqual(self.steps['c1']['sha256'], kunit['config']['sha256'])
        self.assertEqual(list(configure.KUNIT_REQUESTS), kunit['config']['requested'])
        contract = json.loads((REPO / 'tests/contracts/kernel-expanded-kunit-contract.json').read_text())
        self.assertEqual(self.steps['c2']['sha256'], contract['configSha256'])
        preflight = (REPO / 'tests/fixtures/kernel-connector-fpu-preflight.out').read_text()
        self.assertIn('previous config ' + self.steps['c5']['sha256'], preflight)
        self.assertIn('candidate config ' + self.steps['c6']['sha256'], preflight)
        self.assertEqual(len(configure.CONNECTOR_FPU_REQUESTS), len(configure.CONNECTOR_FPU_CHANGES))
        for request in configure.CONNECTOR_FPU_REQUESTS:
            option, value = request.split('=')
            self.assertEqual(configure.CONNECTOR_FPU_CHANGES[option], ['n', value])
            self.assertRegex(preflight, rf'{option}\s+n -> {value}\n')
        self.assertEqual(configure.LINES, {'kunit': 'c2', 'selftest': 'c6'})

    def test_base_is_the_contract_pinned_k6_configuration(self):
        base, recorded = configure.base_config()
        self.assertEqual(base, REPO / '.toolchain/records/frozen-kernel-gcc.config')
        self.assertEqual(configure.sha256(base), recorded)

    def test_kunit_fragments_are_the_exact_recorded_command(self):
        record = REPO / '.evidence/kunit/coverage-config-20260914/reference-network.json'
        if not record.is_file():
            self.skipTest('retained C2 record unavailable')
        data = json.loads(record.read_text())
        upstream = [name for kind, name in self.steps['c2']['fragments'] if kind == 'upstream']
        self.assertEqual([f'{configure.SELFTESTS}/{name}/config' for name in upstream], data['fragments'])
        self.assertEqual(self.steps['c2']['fragments'][-1], ('overlay', 'kernel-selftest-coverage.config'))
        self.assertEqual(configure.sha256(REPO / 'tests/configs/kernel-selftest-coverage.config'),
                         data['overlaySha256'])
        self.assertIn('merge_config.sh -y -m', data['script'])
        self.assertEqual(data['configSha256'], self.steps['c2']['sha256'])

    def test_bpf_identity_matches_its_retained_audit(self):
        summary = REPO / '.evidence/kselftest-full/20260922T033617Z/summary.json'
        if not summary.is_file():
            self.skipTest('retained C3 audit unavailable')
        audit = json.loads(summary.read_text())['configurationAudit']
        self.assertEqual(audit['configSha256'], self.steps['c3']['sha256'])

    def test_each_step_extends_the_previous_configuration(self):
        names = [step['name'] for step in configure.STEPS]
        self.assertEqual(names, ['c1', 'c2', 'c3', 'c5', 'c6'])
        self.assertEqual([step['base'] for step in configure.STEPS], ['c0', 'c1', 'c2', 'c3', 'c5'])

    def test_runtime_step_keeps_its_merge_form_and_order(self):
        script = configure.solve_script()
        merges = [line for line in script.splitlines() if 'merge_config.sh' in line]
        self.assertEqual(len(merges), 5)
        for line, step in zip(merges, configure.STEPS):
            self.assertIn(' -y -m -O ' if step['yes'] else ' -m -O ', line)
            self.assertIn(f"-O /tmp/{step['name']} ", line)
        c5 = merges[3]
        self.assertNotIn(' -y ', c5)
        self.assertLess(c5.index('/nci/config'), c5.index('/selftests/net/config'))
        self.assertLess(c5.index('/selftests/net/config'), c5.index('/pstore/config'))
        self.assertTrue(c5.split(' > ')[0].endswith(
            '/work/configs/kernel-bpf-coverage.config /work/configs/kernel-selftest-runtime.config'))
        self.assertIn('make -C /work/src O=/tmp/c2 ARCH=arm64 CC=gcc-14 olddefconfig', script)
        self.assertIn('diffconfig c5.config c6.config > c5-c6.diff', script)

    def test_container_mounts_the_source_and_inputs_read_only(self):
        command = configure.container_command('image', Path('/src'), Path('/base.config'), Path('/out'))
        self.assertIn('/src:/work/src:ro', command)
        self.assertIn('/base.config:/work/base.config:ro', command)
        self.assertIn('/out:/work/cap', command)
        self.assertEqual(command[command.index('--network') + 1], 'none')


class ConfigureEvaluationTest(unittest.TestCase):
    def write(self, folder, name, text):
        (folder / name).write_text(text)
        return digest(text)

    def test_identity_and_connector_changes_are_exact(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            c5 = 'CONFIG_A=y\n# CONFIG_CONNECTOR is not set\n'
            c6 = 'CONFIG_A=y\nCONFIG_CONNECTOR=y\nCONFIG_PROC_EVENTS=y\nCONFIG_TEST_FPU=m\n# CONFIG_FB_UVESA is not set\n'
            steps = [{'name': 'c5', 'base': 'c3', 'yes': False, 'fragments': (), 'record': 'r',
                      'sha256': self.write(folder, 'c5.config', c5)},
                     {'name': 'c6', 'base': 'c5', 'yes': True, 'fragments': (), 'record': 'r',
                      'sha256': self.write(folder, 'c6.config', c6)}]
            result = configure.evaluate(folder, steps)
            self.assertEqual(result['status'], 'REPRODUCED')
            self.assertTrue(result['connectorFpuExact'])
            self.assertTrue(result['lines']['selftest']['matches'])
            self.write(folder, 'c6.config', c6 + 'CONFIG_EXTRA=m\n')
            result = configure.evaluate(folder, steps)
            self.assertEqual(result['status'], 'DIFFERENT')
            self.assertFalse(result['steps'][1]['matches'])
            self.assertEqual(result['connectorFpuChanges']['CONFIG_EXTRA'], ['n', 'm'])
            self.assertFalse(result['connectorFpuExact'])

    def test_missing_configuration_is_not_a_match(self):
        with tempfile.TemporaryDirectory() as tmp:
            result = configure.evaluate(Path(tmp))
            self.assertEqual(result['status'], 'DIFFERENT')
            self.assertTrue(all(row['sha256'] is None and not row['matches'] for row in result['steps']))


class ConfigureCommandTest(unittest.TestCase):
    def test_dry_run_writes_nothing_and_starts_no_container(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = fake_source(Path(tmp) / 'src')
            output = Path(tmp) / 'out'
            with patch.object(configure.subprocess, 'run') as run, redirect_stdout(io.StringIO()) as text:
                self.assertEqual(configure.main(['--output', str(output), '--source-root', str(source),
                                                 '--dry-run']), 0)
            run.assert_not_called()
            self.assertFalse(output.exists())
            self.assertIn('merge_config.sh -y -m', text.getvalue())

    def test_refuses_a_configured_source_missing_fragment_or_existing_record(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = fake_source(Path(tmp) / 'src')
            output = Path(tmp) / 'out'
            output.mkdir()
            (output / 'configure.json').write_text('{}')
            with patch.object(configure.layout, 'verify_sealed_image') as image, \
                 redirect_stderr(io.StringIO()) as errors:
                self.assertEqual(configure.main(['--output', str(output), '--source-root', str(source)]), 2)
                self.assertIn('already holds', errors.getvalue())
                (output / 'configure.json').unlink()
                (source / '.config').write_text('CONFIG_X=y\n')
                self.assertEqual(configure.main(['--output', str(output), '--source-root', str(source)]), 2)
                (source / '.config').unlink()
                (source / configure.SELFTESTS / 'net/ovpn/config').unlink()
                self.assertEqual(configure.main(['--output', str(output), '--source-root', str(source)]), 2)
            image.assert_not_called()

    def test_run_records_inputs_and_each_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = fake_source(Path(tmp) / 'src')
            output = Path(tmp) / 'out'
            texts = {step['name']: f"CONFIG_STEP_{step['name'].upper()}=y\n" for step in configure.STEPS}
            texts['c6'] = texts['c5'] + 'CONFIG_CONNECTOR=y\nCONFIG_PROC_EVENTS=y\nCONFIG_TEST_FPU=m\n'
            steps = tuple({**step, 'sha256': digest(texts[step['name']])} for step in configure.STEPS)

            def container(command, *, input, **_):
                self.assertIn('set -eu', input)
                cap = Path(next(part for part in command if part.endswith(':/work/cap')).rsplit(':', 1)[0])
                for name, text in texts.items():
                    (cap / f'{name}.config').write_text(text)
                return subprocess.CompletedProcess(command, 0)

            with patch.object(configure, 'STEPS', steps), \
                 patch.object(configure.layout, 'verify_sealed_image', return_value=('image', 'digest')), \
                 patch.object(configure.subprocess, 'run', side_effect=container), \
                 redirect_stdout(io.StringIO()):
                self.assertEqual(configure.main(['--output', str(output), '--source-root', str(source)]), 0)
            record = json.loads((output / 'configure.json').read_text())
            self.assertEqual(record['status'], 'REPRODUCED')
            self.assertEqual(record['exitCode'], 0)
            self.assertEqual(record['requests']['kunit-base.config'],
                             configure.sha256(output / 'kunit-base.config'))
            self.assertEqual((output / 'kunit-base.config').read_text(),
                             'CONFIG_KUNIT=y\nCONFIG_KUNIT_ALL_TESTS=y\nCONFIG_BLK_DEV_UBLK=y\n')
            self.assertIn(f'/work/src/{configure.SELFTESTS}/net/config', record['fragments'])
            self.assertEqual(record['base']['sha256'], configure.base_config()[1])
            self.assertTrue(all(row['matches'] for row in record['steps']))

    def test_container_failure_is_an_error_even_with_some_configurations(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = fake_source(Path(tmp) / 'src')
            output = Path(tmp) / 'out'
            with patch.object(configure.layout, 'verify_sealed_image', return_value=('image', 'digest')), \
                 patch.object(configure.subprocess, 'run',
                              return_value=subprocess.CompletedProcess([], 2)), \
                 redirect_stdout(io.StringIO()):
                self.assertEqual(configure.main(['--output', str(output), '--source-root', str(source)]), 2)
            self.assertEqual(json.loads((output / 'configure.json').read_text())['status'], 'ERROR')


class ConfigureSolverReplayTest(unittest.TestCase):
    """The retained C5 audit is an option-level oracle for the merge form."""

    def test_audit_requests_agree_with_the_runtime_merge_order(self):
        summary = REPO / '.evidence/kselftest-full/20260927T223104Z/summary.json'
        if not summary.is_file():
            self.skipTest('retained C5 audit unavailable')
        audit = json.loads(summary.read_text())['configurationAudit']
        self.assertEqual(audit['configSha256'], configure.STEPS[3]['sha256'])
        rows = {row['id']: {d['option']: d['actual'] for d in row['differences']} for row in audit['collections']}
        # net/config follows bonding, virtio_net and ovpn without -y: its requests win.
        self.assertNotIn('CONFIG_BONDING', rows['net'])
        self.assertEqual(rows['drivers/net/bonding']['CONFIG_BONDING'], 'm')
        self.assertEqual(rows['net/ovpn']['CONFIG_CRYPTO_CHACHA20POLY1305'], 'm')
        self.assertEqual(rows['drivers/net/virtio_net']['CONFIG_NET_VRF'], 'y')
        # tc-testing follows net/config: its module requests win.
        self.assertEqual(rows['net']['CONFIG_NET_SCH_NETEM'], 'm')
        self.assertTrue(re.search(r'\bnet\b', ' '.join(
            name for kind, name in configure.STEPS[3]['fragments'] if kind == 'upstream')))


if __name__ == '__main__':
    unittest.main()
