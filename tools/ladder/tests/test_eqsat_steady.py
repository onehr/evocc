"""Steady protocol REDs. Synthetic records are never performance evidence."""
import copy
import unittest
from unittest.mock import patch

from tools.ladder.eqsat_chain import digest
from tools.ladder.eqsat_native import pair_order
from tools.ladder.eqsat_steady import (DEFAULT_CONTROL, FILES, GUEST_TOOLS, RESOURCES, TASKS,
                                     admitted_calibration, build_population, check_build_inputs, validate_plan)
from tools.ladder.eqsat_steady_audit import audit_capture, audit_guest
from tools.ladder.eqsat_steady_guest import PROTOCOL, aa_order, interval, next_iterations, result_value
from tools.ladder.eqsat_steady_plan import make_plan, owned_files
from tools.ladder.eqsat_twins import admit_unchecked, bind_bundle, canonical, native_identity


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
        i = len(g['attempts'])
        program = t['variants'][v][p]
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
        streams['guest/'+row['stdout']] = raw
        streams['guest/'+row['stderr']] = b''
        g['attempts'].append(row)
        return dict(attempt=i, value=value, identity=identity)
    def admit(t, v, n, stage):
        checked = execution(t, v, 'checked', n, stage)
        row = g['attempts'][checked['attempt']]
        p = t['variants'][v]['unchecked']
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


class SteadyCaptureHeaderTests(unittest.TestCase):
    def header(self, mode, status, **fields):
        """An incomplete capture tests header admission, not native acceptance."""
        from pathlib import Path
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as temp:
            root = Path(temp)
            directory = root/'capture'
            directory.mkdir()
            contracts = root/'tests/contracts'
            contracts.mkdir(parents=True)
            raw = canonical(dict(mode=mode))+b'\n'
            (contracts/'steady.json').write_bytes(raw)
            (directory/'manifest.json').write_bytes(raw)
            receipt = dict(
                schema='evocc-r02-steady-capture-v1', mode=mode, status=status,
                manifest_digest=digest(raw), cleanup=dict(exit_code=0),
                pmu='NOT_MEASURED', attempts=[],
                scope='THREE_LIBRARIES_FIVE_MODES_MIXED_LINK_LINUX_HARDWARE_VM_NO_AI',
                native_timing='NOT_RUN' if mode == 'build' else PROTOCOL['metric'])
            receipt.update(fields)
            (directory/'receipt.json').write_bytes(canonical(receipt)+b'\n')
            plan = dict(tools=dict(container=dict(path='container')), image=dict(name='fixture'))
            with patch('tools.ladder.eqsat_steady_audit.audit_plan', return_value=(plan, raw)):
                return audit_capture(directory, root, digest(raw))

    def test_uppercase_stage_status_reaches_command_replay(self):
        for mode, status in (('build', 'OBSERVED_BUILD'), ('AA', 'OBSERVED_AA'), ('AB', 'OBSERVED_AB')):
            # There is deliberately no image command. A valid header must
            # reach independent command replay and still refuse this fixture.
            with self.subTest(mode=mode), self.assertRaisesRegex(KeyError, 'image-identity'):
                self.header(mode, status)

    def test_wrong_stage_or_unfinished_status_fails_header_admission(self):
        for mode, expected in (('build', 'OBSERVED_BUILD'), ('AA', 'OBSERVED_AA'), ('AB', 'OBSERVED_AB')):
            for status in ('OBSERVED_build', 'OBSERVED_BUILD', 'OBSERVED_AA', 'OBSERVED_AB', 'STARTED', 'FAILED'):
                if status == expected:
                    continue
                with self.subTest(mode=mode, status=status), self.assertRaisesRegex(ValueError, 'incomplete steady'):
                    self.header(mode, status)

    def test_receipt_cannot_promote_scope_or_change_the_registered_timing_metric(self):
        for mode, status in (('build', 'OBSERVED_BUILD'), ('AA', 'OBSERVED_AA'), ('AB', 'OBSERVED_AB')):
            for field, forged in (('scope', 'ALL_COMPILER_AND_AI_ACCEPTED'),
                                  ('native_timing', 'QUALIFIED_SEARCH_WINNER'), ('pmu', 'MEASURED')):
                with self.subTest(mode=mode, field=field), self.assertRaisesRegex(ValueError, 'incomplete steady'):
                    self.header(mode, status, **{field: forged})


class SteadyProtocolTests(unittest.TestCase):
    def test_build_plan_pins_baseline_before_the_fresh_default_exists(self):
        from pathlib import Path
        from tempfile import TemporaryDirectory
        from tools.ladder.eqsat_pilot import ROOTS
        with TemporaryDirectory() as temp:
            root = Path(temp)
            complete = root/'complete'
            complete.mkdir()
            historical = root/'historical'
            historical.mkdir()
            tools = {k: dict(path='tool', sha256='0'*64) for k in
                ('container', 'collector_python', 'checker', 'historical_checker', 'ir_backend', 'census')}
            (complete/'manifest.json').write_bytes(canonical(dict(source=dict(directory='historical'),
                tools=tools, image=dict(name='synthetic', digest='0'*64),
                guest_tools=dict.fromkeys(GUEST_TOOLS, '0'*64))))
            (complete/'receipt.json').write_bytes(canonical(dict(status='OBSERVED_DETERMINISTIC_COMPLETION')))
            (complete/'audit.json').write_bytes(canonical(dict(status='DETERMINISTIC_CAPTURE_CONSISTENT')))
            (historical/'manifest.json').write_bytes(b'{}')
            programs, inputs = [], []
            for case in ('zlib', 'zstd', 'cjson'):
                chain = 'historical/'+case+'/A-chain'
                objects = {p: 'complete/'+case+'-'+p+'.o' for p in ('checked', 'unchecked')}
                reference = 'historical/'+case+'/ref.o'
                programs.append(dict(case=case, reference_objects=[reference],
                    units=[dict(chains=dict(A=chain), objects=dict(A=objects))]))
                inputs.extend([chain+'/output.evo', reference, *objects.values(), ROOTS[case]+'/public.h'])
            for name in (*FILES, *inputs):
                p = root/name
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_bytes(name.encode())
            with patch('tools.ladder.eqsat_steady_plan.build_population', return_value=programs), \
                 patch('tools.ladder.eqsat_steady_plan.subprocess.check_output', return_value='0'*40), \
                 patch('tools.ladder.eqsat_steady_plan.host_identity', return_value={'host': 'test'}), \
                 patch('tools.ladder.eqsat_steady.host_identity', return_value={'host': 'test'}):
                plan = make_plan(root, 'build', complete)
            self.assertEqual(plan['optimizer'], DEFAULT_CONTROL)
            self.assertEqual(plan['tools']['census'], tools['census'])
            self.assertTrue(set(inputs) <= set(plan['source']['files']))
            self.assertIn('historical/manifest.json', plan['source']['files'])
            self.assertFalse(any('B-chain' in p for p in plan['source']['files']))

    def test_default_control_does_not_substitute_a_signature_representative(self):
        from pathlib import Path
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as temp:
            root = Path(temp)
            complete = root/'complete'
            complete.mkdir()
            historical = root/'historical'
            historical.mkdir()
            (complete/'manifest.json').write_bytes(canonical(dict(source=dict(directory='historical'))))
            (complete/'audit.json').write_bytes(canonical(dict(status='DETERMINISTIC_CAPTURE_CONSISTENT',
                policy_evaluations=1024, verified_chains=6144, unattempted_templates=0)))
            code, cases = {}, []
            for case in ('zlib', 'zstd', 'cjson'):
                source = case+'.c'
                cases.append(dict(id=case, sources=[source], subject_sources=[source]))
                chain = historical/case/'00-A-chain'
                chain.mkdir(parents=True)
                ir = case.encode()
                identity = digest(ir)
                record = dict(schema='evocc-identity-chain-v1', mode='eqsat-off',
                    publication='IDENTITY_VERIFIED', attempted_transactions=0, transactions=[],
                    input_digest=identity, output_digest=identity,
                    chain_id=digest(f'identity-chain-v1\n{identity}\n'.encode()))
                (chain/'chain.json').write_bytes(canonical(record))
                for name in ('input.evo', 'output.evo'):
                    (chain/name).write_bytes(ir)
                binding = bind_bundle(chain)
                for profile in ('checked', 'unchecked'):
                    obj = complete/(case+'-'+profile+'.o')
                    obj.write_bytes(b'synthetic object')
                    key = digest(canonical((binding['ir_digest'], profile)))
                    code[key] = dict(ir_digest=binding['ir_digest'], profile=profile,
                                     object=obj.name, object_digest=digest(obj.read_bytes()))
            (historical/'manifest.json').write_bytes(canonical(dict(cases=cases)))
            # The minimal positive representative has mul=2. It is not the
            # compiler's all-one default, even if old consulted gates agree.
            (complete/'receipt.json').write_bytes(canonical(dict(status='OBSERVED_DETERMINISTIC_COMPLETION',
                ai_campaign='NOT_ADMITTED', code=code,
                policy=dict(evaluations=[dict(weights=[1, 1, 2]+[1]*10, max_transactions=8)]))))
            m = dict(source=dict(directory='complete'), tools=dict(historical_checker=dict(path='checker')))
            programs = build_population(m, root)
            self.assertEqual([p['case'] for p in programs], ['zlib', 'zstd', 'cjson'])
            for program in programs:
                self.assertEqual(program['variants']['B'], {})
                self.assertEqual(set(program['units'][0]['chains']), {'A'})
            self.assertEqual(DEFAULT_CONTROL['cost_weights'], [1]*13)

    def test_shared_resource_auditor_has_a_frozen_source_identity(self):
        import inspect
        from pathlib import Path
        from tools.ladder.eqsat_pilot_audit import audit_usage
        root = Path(__file__).resolve().parents[3]
        source = Path(inspect.getsourcefile(audit_usage)).resolve().relative_to(root).as_posix()
        self.assertIn(source, FILES)

    def test_unpinned_link_object_proof_or_header_is_not_a_complete_build_plan(self):
        from pathlib import Path
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as temp:
            root = Path(temp)
            paths = ('complete/manifest.json', 'complete/receipt.json', 'complete/audit.json', 'historical/manifest.json',
                     'ref.o', 'chain/output.evo', 'subject.o', 'include/public.h')
            for p in paths:
                file = root/p
                file.parent.mkdir(exist_ok=True)
                file.write_bytes(p.encode())
            (root/'complete/manifest.json').write_bytes(canonical(dict(source=dict(directory='historical'))))
            m = dict(source=dict(directory='complete', files={p: digest((root/p).read_bytes()) for p in paths}))
            programs = [dict(case='cjson', reference_objects=['ref.o'], units=[dict(
                chains=dict(A='chain', B='chain'), objects=dict(A=dict(checked='subject.o', unchecked='subject.o')))])]
            with patch('tools.ladder.eqsat_steady.ROOTS', dict(cjson='include')):
                check_build_inputs(m, root, programs)
                for p in paths:
                    changed = copy.deepcopy(m)
                    changed['source']['files'].pop(p)
                    with self.subTest(path=p), self.assertRaisesRegex(ValueError, 'unbound'):
                        check_build_inputs(changed, root, programs)

    def test_plan_snapshot_retains_exact_owned_artifacts_and_refuses_pending_publish(self):
        from pathlib import Path
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as temp:
            root = Path(temp)
            source = root/'capture'
            source.mkdir()
            (source/'receipt.json').write_bytes(b'complete\n')
            self.assertEqual(owned_files(source, root), {'capture/receipt.json': digest(b'complete\n')})
            (source/'receipt.json.pending').write_bytes(b'incomplete\n')
            with self.assertRaisesRegex(ValueError, 'published'):
                owned_files(source, root)

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
            changed = copy.deepcopy(g)
            mutate(changed)
            with self.assertRaises(ValueError):
                admitted_calibration(changed)

    @patch('tools.ladder.eqsat_steady.host_identity', return_value={'host': 'test'})
    def test_control_or_resource_mutation_is_not_a_registered_plan(self, _):
        m = dict(schema='evocc-r02-steady-plan-v1', mode='AA', implementation_commit='0'*40,
            source=dict(kind='STEADY_BUILD', directory='build', files={'artifact': '0'*64}),
            files=dict.fromkeys(FILES, '0'*64), tools=dict.fromkeys(
                ('container', 'collector_python', 'checker', 'historical_checker', 'ir_backend', 'census'), {}),
            image={}, guest_tools=dict.fromkeys(GUEST_TOOLS, '0'*64), host={'host': 'test'}, guest_kernel='6.18.15',
            resources=copy.deepcopy(RESOURCES), protocol=copy.deepcopy(PROTOCOL),
            optimizer=copy.deepcopy(DEFAULT_CONTROL))
        validate_plan(m, 'AA')
        for mutate in (lambda x: x['protocol'].update(cpu=1), lambda x: x['protocol'].update(minimum_ns=0),
                       lambda x: x['protocol'].update(retries=1), lambda x: x['resources'].update(jobs=True),
                       lambda x: x['files'].pop('tools/ladder/eqsat_steady_audit.py'),
                       lambda x: x['files'].pop('tools/ladder/eqsat_pilot_audit.py'),
                       lambda x: x['optimizer']['cost_weights'].__setitem__(2, 2),
                       lambda x: x['optimizer']['schedule'].update(max_transactions=1),
                       lambda x: x['tools'].pop('census')):
            changed = copy.deepcopy(m)
            mutate(changed)
            with self.assertRaises(ValueError):
                validate_plan(changed, 'AA')


class SteadyAuditMutationTests(unittest.TestCase):
    def test_raw_attempt_chronology_must_match_the_frozen_pair_order(self):
        g, request, streams = synthetic()
        samples = g['comparisons'][0]['samples'][:2]
        a, b = [s['attempt'] for s in samples]
        g['attempts'][a], g['attempts'][b] = g['attempts'][b], g['attempts'][a]
        for n in (a, b):
            row = g['attempts'][n]
            row.update(stdout=f'{n:03}.stdout', stderr=f'{n:03}.stderr')
        for s in samples:
            s['attempt'] = b if s['attempt'] == a else a
        # Outputs are identical A/A observations; hashes, stages, identities,
        # pairs and statistics all remain internally consistent. Only the raw
        # execution order now contradicts the preregistered pair order.
        with self.assertRaisesRegex(ValueError, 'chronology'):
            audit_guest(g, request, streams.__getitem__)

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
            lambda x: x.update(cpu_affinity=[False]),
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
            changed = copy.deepcopy(g)
            mutate(changed)
            with self.subTest(mutate=mutate), self.assertRaises((ValueError, KeyError)):
                audit_guest(changed, request, streams.__getitem__)


if __name__ == '__main__':
    unittest.main()
