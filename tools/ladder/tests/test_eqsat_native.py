"""Native measurement primitives: failed/mutated attempts never become samples."""
import io
import copy
import fcntl
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

from tools.ladder.eqsat_chain import digest
from tools.ladder.eqsat_native import (Capture, pair_order, summarize, execution_plan,
                                       run_calibration, FRONTEND, REFERENCE, LINK, SDK_LINK)
from tools.ladder.tests.test_eqsat_manifest import manifest


def native_plan():
    m = manifest()
    m.update(schema='evocc-performance-plan-v2', native=dict(adapter='darwin-p0-aa-v1', eqsat='off',
        codegen_target='darwin-arm64', source_profile='linux-lp64-header-free-integer',
        collector_files={name: '0'*64 for name in ('tools/bench.py', 'tools/ladder/eqsat_native.py',
            'tools/ladder/eqsat_manifest.py', 'tools/ladder/eqsat_census.py', 'tools/ladder/eqsat_chain.py')}))
    m['tools']['ir_backend'] = copy.deepcopy(m['tools']['compiler'])
    m['tools']['collector_python'] = dict(path=str(Path(sys.executable).resolve()),
                                         sha256=digest(Path(sys.executable).read_bytes()))
    m['environment'] = dict(PATH='/usr/bin:/bin', LANG='C', LC_ALL='C')
    m['controls'].update(target='aarch64-unknown-linux-gnu', isa='baseline',
        abi='linux-lp64-source/darwin-arm64-backend', semantic_flags=[])
    m['protocol'].update(pairs=6)
    case = m['cases'][0]
    case.update(inputs=[], compiler_argv=FRONTEND.copy(), reference_argv=REFERENCE.copy(),
                link_argv=LINK.copy(), coverage=dict(subject_objects=['program.o'], reference_objects=[]))
    case['source']['path'] = 'tests/fixtures/verified-eqsat-chain.c'
    return m


class NativePrimitivesTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.receipt = dict(attempts=[])
        self.resources = dict(timeout_seconds=2, output_bytes=1024, min_free_bytes=0,
                              max_retained_bytes=1024*1024)
        self.runner = Capture(self.root, self.resources, {}, self.receipt, lambda: None)

    def run_code(self, code, expected=None):
        return self.runner.run('unit', [sys.executable, '-c', code], expected=expected)

    def test_exact_result_is_checked_for_the_timed_attempt(self):
        row = self.run_code('print("result")', dict(exit_code=0,
            stdout_sha256=digest(b'result\n'), stderr_sha256=digest(b'')))
        self.assertEqual(row['status'], 'RESULT_CHECKED')

    def test_waited_child_cpu_and_peak_rss_are_observed_not_parent_deltas(self):
        self.runner.measure_resources = True
        row = self.run_code('sum(i*i for i in range(100000))')
        usage = row['resources']
        self.assertGreater(usage['user_ns'] + usage['system_ns'], 0)
        self.assertGreater(usage['peak_rss_bytes'], 0)
        self.assertEqual(usage['scope'], 'wait4-direct-child-including-waited-descendants')
        self.assertGreater(row['wall_ns'], 0)
        self.assertEqual(row['argv'][0], sys.executable)
        self.assertEqual(self.receipt['attempts'], [row])

    def test_wrong_output_nonzero_exit_and_overflow_are_retained(self):
        for code in ('print("wrong")', 'raise SystemExit(2)', 'print("x"*2048)'):
            with self.subTest(code=code), self.assertRaises(ValueError):
                self.run_code(code, dict(exit_code=0, stdout_sha256=digest(b''), stderr_sha256=digest(b'')))
            self.assertEqual(self.receipt['attempts'][-1]['status'], 'FAILED')
            self.assertNotIn('sample_ns', self.receipt['attempts'][-1])

    def test_watchdog_reaps_owned_process_and_retains_attempt(self):
        self.resources['timeout_seconds'] = 0.05
        with self.assertRaisesRegex(ValueError, 'watchdog'):
            self.run_code('import time; time.sleep(10)')
        self.assertEqual(self.receipt['attempts'][-1]['status'], 'FAILED')

    def test_surviving_same_group_descendant_is_not_a_completed_run(self):
        with self.assertRaisesRegex(ValueError, 'descendants'):
            self.run_code('import subprocess,sys; subprocess.Popen([sys.executable,"-c",'
                          '"import time;time.sleep(5)"], stdout=subprocess.DEVNULL, '
                          'stderr=subprocess.DEVNULL)')

    def test_work_files_are_bounded_without_a_descendant_fsize_limit(self):
        self.resources['max_retained_bytes'] = 2048
        with self.assertRaisesRegex(ValueError, 'retention'):
            self.run_code('from pathlib import Path; import time; Path("big").write_bytes(b"x"*4096); time.sleep(1)')
        self.assertEqual((self.root / 'big').stat().st_size, 4096)

    def test_checker_scratch_disappearing_during_scan_is_not_a_failed_attempt(self):
        scratch = self.root / 'evocc-proof-owned'
        scratch.mkdir()
        proof = scratch / 'proof.evp'
        proof.write_bytes(b'transient')
        original = Path.stat
        def concurrent_cleanup(path, *args, **kwargs):
            value = original(path, *args, **kwargs)
            if path == proof:
                shutil.rmtree(scratch)
            return value
        with patch.object(Path, 'stat', concurrent_cleanup):
            self.runner.watch()
        self.assertFalse(scratch.exists())
        # A disappeared scratch entry never licenses an oversized retained file.
        (self.root / 'retained').write_bytes(b'x' * 2048)
        self.resources['max_retained_bytes'] = 1024
        with self.assertRaisesRegex(ValueError, 'retention'):
            self.runner.watch()

    def test_retention_scan_cannot_hide_permission_errors_or_missing_capture_root(self):
        with patch('os.scandir', side_effect=PermissionError('unreadable capture')):
            with self.assertRaises(PermissionError):
                self.runner.watch()
        self.runner.directory = self.root / 'missing-root'
        with self.assertRaisesRegex(ValueError, 'capture directory'):
            self.runner.watch()

    def test_scratch_directory_vanishing_before_scandir_is_tolerated(self):
        scratch = self.root / 'evocc-proof-owned'
        scratch.mkdir()
        (scratch / 'proof.evp').write_bytes(b'transient')
        original = os.scandir
        def concurrent_cleanup(path):
            if not isinstance(path, int) and Path(path) == scratch and scratch.exists():
                shutil.rmtree(scratch)
            return original(path)
        with patch('os.scandir', side_effect=concurrent_cleanup):
            self.runner.watch()
        self.assertFalse(scratch.exists())

    def test_pair_order_and_statistics_are_frozen_and_complete(self):
        order = pair_order(6, 7)
        self.assertEqual(order, pair_order(6, 7))
        self.assertEqual(sorted(order), [('A0', 'A1')]*3 + [('A1', 'A0')]*3)
        pairs = [dict(A0=100+i, A1=102+i) for i in range(6)]
        result = summarize(pairs)
        self.assertEqual(result['paired_median_delta_ns'], 2)
        self.assertEqual(result['median_delta_ci95_ns'], [2, 2])
        self.assertIsNone(summarize(pairs[:2])['median_delta_ci95_ns'])
        for bad in ([dict(A0=1)], [dict(A0=0, A1=1)], [dict(A0=True, A1=2)]):
            with self.assertRaises(ValueError):
                summarize(bad)

    def test_native_recipe_rejects_unimplemented_or_unfrozen_controls(self):
        m = native_plan()
        execution_plan(m, m['host'])
        for change in (
            lambda p: p['host'].update(release='another'),
            lambda p: p['controls'].update(semantic_flags=['-ffast-math']),
            lambda p: p['cases'][0]['compiler_argv'].append('-Oeq-scalar'),
            lambda p: p['cases'][0]['source'].update(path='new-unreviewed.c'),
            lambda p: p['protocol'].update(pairs=3),
            lambda p: p['resources'].update(timeout_seconds=600),
            lambda p: p['environment'].update(DYLD_INSERT_LIBRARIES='/uncontrolled'),
        ):
            p = copy.deepcopy(m)
            change(p)
            with self.assertRaises(ValueError):
                execution_plan(p, m['host'])


class NativePipelineTests(unittest.TestCase):
    """Real child supervision with fixture tools, NOT compiler/native evidence."""
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.m = native_plan()
        self.path = self.root / 'plan.json'
        self.script = self.root / 'fixture-tool'
        for name in self.m['native']['collector_files']:
            p = self.root / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text('unit collector pin')
            self.m['native']['collector_files'][name] = digest(p.read_bytes())
        for item in self.m['specs'].values():
            p = self.root / item['path']
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text('unit fixture')
            item['sha256'] = digest(p.read_bytes())
        source = self.m['cases'][0]['source']
        p = self.root / source['path']
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text('/* unit fixture only */')
        source['sha256'] = digest(p.read_bytes())
        self.tool()

    def tool(self, subject='exit 0', require_sdk=False):
        # This fixture does not assert any C or machine semantics.
        ir = 'unit IR identity only\n'
        ih = digest(ir.encode())
        chain = dict(schema='evocc-identity-chain-v1', mode='eqsat-off',
            publication='IDENTITY_VERIFIED', attempted_transactions=0, transactions=[],
            input_digest=ih, output_digest=ih,
            chain_id=digest(('identity-chain-v1\n'+ih+'\n').encode()))
        text = f'''#!{sys.executable}
import sys, json, os
from pathlib import Path
a=sys.argv[1:]
if a[0].startswith('--interpret-'):
    print(0)
elif a[0]=='--emit=evoir':
    d=Path(a[2].split('=',1)[1]); d.mkdir()
    for name in ('input.evo','output.evo'): (d/name).write_text({ir!r})
    (d/'chain.json').write_text({json.dumps(chain,sort_keys=True,separators=(',', ':'))!r})
    print({ir!r}, end='')
elif a[0]=='darwin-arm64':
    print('fixture assembly')
elif '-c' in a:
    Path(a[a.index('-o')+1]).write_text('fixture object')
else:
    if {require_sdk!r} and (not os.environ.get('SDKROOT') or not a[0].startswith('-fuse-ld=')):
        sys.exit(9)
    p=Path(a[a.index('-o')+1])
    body={subject!r} if p.name=='program' else 'exit 0'
    p.write_text('#!/bin/sh\\n'+body+'\\n'); p.chmod(0o700)
'''
        self.script.write_text(text)
        self.script.chmod(0o700)
        for name, item in self.m['tools'].items():
            if name != 'collector_python':
                item.update(path='fixture-tool', sha256=digest(self.script.read_bytes()))

    def run_plan(self):
        self.path.write_text(json.dumps(self.m))
        with (patch('tools.ladder.eqsat_native.audit_manifest',
                    return_value=dict(manifest_digest=digest(self.path.read_bytes()))),
              patch('tools.ladder.eqsat_native.host_identity', return_value=self.m['host']),
              patch('tools.ladder.eqsat_native.check_source_revision')):
            return run_calibration(self.path, self.root)

    def test_complete_pair_population_and_unique_owned_receipts(self):
        directory, r = self.run_plan()
        self.assertEqual(r['status'], 'OBSERVED_AA', r)
        self.assertEqual(r['performance'], 'NOT_CLAIMED')
        case = r['cases'][0]
        self.assertEqual(len(case['samples']), 12)
        self.assertEqual(len(case['warmups']), 2)
        self.assertEqual(len({p['executable_digest'] for p in case['samples']}), 1)
        for sample in case['samples']:
            self.assertEqual(r['attempts'][sample['attempt']]['status'], 'RESULT_CHECKED')
        old = (directory/'receipt.json').read_bytes()
        again, _ = self.run_plan()
        self.assertNotEqual(directory, again)
        self.assertEqual((directory/'receipt.json').read_bytes(), old)

    def test_bad_result_or_self_mutation_cannot_reach_samples(self):
        for subject in ('exit 3', 'echo "# changed" >> "$0"\nexit 0'):
            self.tool(subject)
            _, r = self.run_plan()
            self.assertEqual(r['status'], 'FAILED', r)
            self.assertEqual(r['cases'][0]['samples'], [])

    def test_existing_benchmark_lock_prevents_launch_and_evidence(self):
        (self.root/'.build').mkdir()
        with (self.root/'.build/bench.lock').open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'shared lock'):
                self.run_plan()
        self.assertFalse((self.root/'.evidence').exists())

    def test_sdk_recipe_requires_explicit_pinned_inputs_and_closed_environment(self):
        self.tool(require_sdk=True)
        sdk = self.root/'SDK'
        (sdk/'usr/lib').mkdir(parents=True)
        settings, system = sdk/'SDKSettings.json', sdk/'usr/lib/libSystem.tbd'
        settings.write_bytes(b'fixture SDK settings')
        system.write_bytes(b'fixture system library stub')
        self.m['native'].update(adapter='darwin-p0-aa-v2', sdk=dict(
            settings=dict(path=str(settings), sha256=digest(settings.read_bytes())),
            system_library=dict(path=str(system), sha256=digest(system.read_bytes()))))
        self.m['tools']['link_editor'] = copy.deepcopy(self.m['tools']['linker'])
        self.m['environment']['SDKROOT'] = str(sdk.resolve())
        self.m['cases'][0]['link_argv'] = SDK_LINK.copy()
        directory, result = self.run_plan()
        self.assertEqual(result['status'], 'OBSERVED_AA', result)
        from tools.ladder.eqsat_native_audit import audit_calibration
        audit_calibration(directory, self.root, digest((directory/'manifest.json').read_bytes()))
        for change in (
            lambda p: p['environment'].pop('SDKROOT'),
            lambda p: p['environment'].update(SDKROOT='/another-sdk'),
            lambda p: p['native']['sdk']['system_library'].update(path='/another/libSystem.tbd'),
            lambda p: p['tools'].pop('link_editor'),
        ):
            p = copy.deepcopy(self.m)
            change(p)
            with self.assertRaises(ValueError):
                execution_plan(p, p['host'])

    def test_read_only_audit_recomputes_population_results_artifacts_and_statistics(self):
        from tools.ladder.eqsat_native_audit import audit_calibration
        directory, r = self.run_plan()
        expected = digest((directory/'manifest.json').read_bytes())
        report = audit_calibration(directory, self.root, expected)
        self.assertEqual(report['status'], 'CAPTURE_CONSISTENT')
        self.assertEqual(report['timed_executions'], 12)
        self.assertEqual(report['performance'], 'NOT_CLAIMED')
        mutations = (
            lambda x: x.update(status='FAILED'),
            lambda x: x['attempts'].pop(),
            lambda x: x['attempts'][-1].update(exit_code=1),
            lambda x: x['attempts'][-1].update(stdout='../outside'),
            lambda x: x['attempts'][-1].update(argv=['another-program']),
            lambda x: x['cases'][0]['samples'][0].update(wall_ns=1),
            lambda x: x['cases'][0]['samples'][0].update(attempt=0),
            lambda x: x['cases'][0]['samples'][0].update(pair=False),
            lambda x: x['cases'][0]['samples'].pop(),
            lambda x: x['cases'][0]['warmups'].reverse(),
            lambda x: x['cases'][0]['statistics'].update(paired_median_delta_ns=-999),
            lambda x: x['cases'][0]['chain'].update(accepted=False),
            lambda x: x.update(performance='PASS'),
        )
        for mutate in mutations:
            bad = copy.deepcopy(r)
            mutate(bad)
            (directory/'receipt.json').write_text(json.dumps(bad))
            with self.subTest(mutation=mutate), self.assertRaises(ValueError):
                audit_calibration(directory, self.root, expected)
        (directory/'receipt.json').write_text(json.dumps(r))
        with self.assertRaisesRegex(ValueError, 'manifest'):
            audit_calibration(directory, self.root, '0'*64)
        (directory/self.m['cases'][0]['id']/'program').write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError, 'artifact'):
            audit_calibration(directory, self.root, expected)
