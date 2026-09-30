"""Steady protocol REDs. Synthetic records are never performance evidence."""
import copy
import unittest
from unittest.mock import patch

from tools.ladder.eqsat_chain import digest
from tools.ladder.eqsat_native import pair_order
from tools.ladder.eqsat_steady import FILES, GUEST_TOOLS, RESOURCES, TASKS, admitted_calibration, validate_plan
from tools.ladder.eqsat_steady_audit import audit_guest
from tools.ladder.eqsat_steady_guest import PROTOCOL, aa_order, interval, next_iterations, result_value
from tools.ladder.eqsat_twins import admit_unchecked, canonical, native_identity


def synthetic(mode='AA'):
    """Build input-bound raw streams independently of the guest runner."""
    binding = {k: digest(k.encode()) for k in ('ir_digest', 'chain_record_digest', 'bundle_digest')}
    tasks = [dict(id=c+':'+w, case=c, work=w, variants={v: {p: dict(binding=binding,
        executable='/work/'+c+'-'+v+'-'+p, executable_digest=digest((c+v+p).encode()))
        for p in ('checked', 'unchecked')} for v in ('A', 'B')}) for c, w in TASKS]
    calibration = {t['id']: dict(iterations={'zlib': 16, 'zstd': 256, 'cjson': 8192}[t['case']],
        pairs=dict(checked=12, unchecked=12)) for t in tasks}
    request = dict(mode=mode, tasks=tasks, protocol=PROTOCOL, backend_digest=digest(b'backend'),
        harness_digest=digest(b'harness'), campaign_seconds=7200, guest_kernel='6.18.15',
        calibration=calibration if mode == 'AB' else None)
    g = dict(schema='evocc-r02-steady-guest-v1', status='OBSERVED_'+mode, mode=mode,
        request_digest=digest(canonical(request)+b'\n'), cpu_affinity=[0], kernel='6.18.15',
        protocol=PROTOCOL, pmu='NOT_MEASURED', comparisons=[], probes=[], admissions=[], attempts=[])
    streams = {}
    def execution(t, v, p, n, stage):
        i = len(g['attempts']); program = t['variants'][v][p]
        value = dict(result='validated', work=t['work'], iterations=n, elapsed_ns=2000000000)
        raw = canonical(value)+b'\n'
        identity = native_identity(program['binding'], p, 'linux-aarch64', request['backend_digest'],
            program['executable_digest'], digest(canonical(dict(harness=request['harness_digest'], work=t['work'], iterations=n))))
        row = dict(stage=stage, argv=[program['executable'], t['work'], str(n)], status='RESULT_CHECKED', exit_code=0,
            stdout=f'{i:03}.stdout', stderr=f'{i:03}.stderr', stdout_bytes=len(raw), stderr_bytes=0,
            stdout_sha256=digest(raw), stderr_sha256=digest(b''), native_identity=identity['native_identity'],
            storage_watch='BATCH_BOUNDARIES_ONLY_PREDECLARED_CHILD_ENVELOPE', result_contract='STEADY_TYPED_RESULT_V1',
            resources=dict(user_ns=1, system_ns=1, peak_rss_bytes=1024, peak_rss_raw=1, rss_unit='bytes',
                scope='wait4-direct-child-including-waited-descendants', wall_scope='spawn-to-stdio-drain'))
        streams['guest/'+row['stdout']] = raw; streams['guest/'+row['stderr']] = b''
        g['attempts'].append(row)
        return dict(attempt=i, value=value, identity=identity)
    def admit(t, v, n, stage):
        checked = execution(t, v, 'checked', n, stage)
        row = g['attempts'][checked['attempt']]; p = t['variants'][v]['unchecked']
        identity = native_identity(p['binding'], 'unchecked', 'linux-aarch64', request['backend_digest'],
                                  p['executable_digest'], checked['identity']['input_digest'])
        a = admit_unchecked(checked['identity'], identity, row,
            {k: row[k] for k in ('exit_code', 'stdout_sha256', 'stderr_sha256')})
        g['admissions'].append(dict(task=t['id'], variant=v, iterations=n, checked=checked, admission=a))
        return checked
    for i, t in enumerate(tasks):
        n = calibration[t['id']]['iterations']
        if mode == 'AA':
            c = admit(t, 'A', n, f'probe:{i}:0:checked')
            u = execution(t, 'A', 'unchecked', n, f'probe:{i}:0:unchecked')
            g['probes'].append(dict(task=t['id'], iterations=n, checked=c, unchecked=u))
        else:
            for v in ('A', 'B'):
                admit(t, v, n, f'admit:{i}:{v}:checked')
        for pindex, p in enumerate(('checked', 'unchecked')):
            c = dict(task=t['id'], case=t['case'], work=t['work'], profile=p, iterations=n,
                     samples=[], pairs=[], looks=[])
            seed = PROTOCOL['seed']+2*i+pindex
            orders = aa_order(seed)[:12] if mode == 'AA' else pair_order(12, seed+100)
            for j, order in enumerate(orders):
                pair = {}
                for label in order:
                    v = 'A' if mode == 'AA' or label == 'A0' else 'B'
                    sample = execution(t, v, p, n, f'sample:{i}:{p}:{j}:{label}')
                    c['samples'].append(dict(pair=j, label=label, variant=v, **sample))
                    pair[label] = sample['value']['elapsed_ns']
                c['pairs'].append(pair)
            c.update(statistics=interval(c['pairs'], mode == 'AA'), duration_gate=True,
                     status='ADMITTED_AA' if mode == 'AA' else 'OBSERVED_AB')
            if mode == 'AA':
                c['looks'].append(dict(pairs=12, statistics=c['statistics'], duration_gate=True))
            g['comparisons'].append(c)
    return g, request, streams


class SteadyProtocolTests(unittest.TestCase):
    def test_every_allowed_calibration_look_is_balanced(self):
        for n in PROTOCOL['pair_counts']:
            order = aa_order(20260930)[:n]
            self.assertEqual(sum(x[0] == 'A0' for x in order), n//2)

    def test_result_validation_does_not_trust_exit_zero_or_boolean_counts(self):
        value = dict(result='validated', work='compress', iterations=1, elapsed_ns=1)
        self.assertEqual(result_value(canonical(value), 'compress', 1), value)
        for field, replacement in (('result', 'unchecked'), ('work', 'decompress'),
                                   ('iterations', True), ('elapsed_ns', 0), ('elapsed_ns', -1)):
            with self.subTest(field=field), self.assertRaises(ValueError):
                result_value(canonical(dict(value, **{field: replacement})), 'compress', 1)

    def test_repeat_choice_is_bounded_and_depends_on_a_only_duration(self):
        self.assertGreater(next_iterations(16, 1000000), 16)
        for n, elapsed in ((0, 1), (1, 0), (True, 1), (4194304, 1)):
            with self.assertRaises(ValueError):
                next_iterations(n, elapsed)

    def test_unqualified_or_partial_aa_cannot_admit_ab(self):
        g, _, _ = synthetic()
        self.assertEqual(len(admitted_calibration(g)), 5)
        for mutate in (lambda x: x['comparisons'].pop(),
                       lambda x: x['comparisons'][0].update(status='INCONCLUSIVE_AA'),
                       lambda x: x.update(status='FAILED')):
            changed = copy.deepcopy(g); mutate(changed)
            with self.assertRaises(ValueError):
                admitted_calibration(changed)

    @patch('tools.ladder.eqsat_steady.host_identity', return_value={'host': 'test'})
    def test_control_or_resource_mutation_is_not_a_registered_plan(self, _):
        m = dict(schema='evocc-r02-steady-plan-v1', mode='AA', implementation_commit='0'*40,
            source=dict(kind='STEADY_BUILD', directory='build', files={'artifact': '0'*64}),
            files=dict.fromkeys(FILES, '0'*64), tools=dict.fromkeys(
                ('container', 'collector_python', 'checker', 'historical_checker', 'ir_backend'), {}),
            image={}, guest_tools=dict.fromkeys(GUEST_TOOLS, '0'*64), host={'host': 'test'}, guest_kernel='6.18.15',
            resources=copy.deepcopy(RESOURCES), protocol=copy.deepcopy(PROTOCOL))
        validate_plan(m, 'AA')
        for mutate in (lambda x: x['protocol'].update(cpu=1), lambda x: x['protocol'].update(minimum_ns=0),
                       lambda x: x['protocol'].update(retries=1), lambda x: x['resources'].update(jobs=True),
                       lambda x: x['files'].pop('tools/ladder/eqsat_steady_audit.py')):
            changed = copy.deepcopy(m); mutate(changed)
            with self.assertRaises(ValueError):
                validate_plan(changed, 'AA')


class SteadyAuditMutationTests(unittest.TestCase):
    def test_synthetic_raw_aa_and_ab_are_fully_accounted_for(self):
        for mode in ('AA', 'AB'):
            g, request, streams = synthetic(mode)
            report = audit_guest(g, request, streams.__getitem__)
            self.assertEqual(report['timed_samples'], 240)
            self.assertEqual(report['status'], 'STEADY_CAPTURE_CONSISTENT')

    def test_metric_identity_cpu_quiet_mode_and_twin_forgeries_fail(self):
        g, request, streams = synthetic()
        for mutate in (
            lambda x: x.update(cpu_affinity=[0, 1]),
            lambda x: x['attempts'][0].pop('storage_watch'),
            lambda x: x['attempts'][0].update(exit_code=1),
            lambda x: x['attempts'][0]['argv'].__setitem__(2, '999'),
            lambda x: x['admissions'][0]['admission'].update(status='BYPASSED'),
            lambda x: x['comparisons'][0]['samples'][0]['value'].update(elapsed_ns=1),
            lambda x: x['comparisons'][0]['statistics'].update(outcome='MATERIAL_IMPROVEMENT'),
            lambda x: x['comparisons'][0].update(status='INCONCLUSIVE_AA'),
            lambda x: x['comparisons'][0]['samples'][0].update(label='A1'),
            lambda x: x['probes'][0].update(iterations=4096),
            lambda x: x['attempts'].append(copy.deepcopy(x['attempts'][0])),
        ):
            changed = copy.deepcopy(g); mutate(changed)
            with self.subTest(mutate=mutate), self.assertRaises((ValueError, KeyError)):
                audit_guest(changed, request, streams.__getitem__)


if __name__ == '__main__':
    unittest.main()
