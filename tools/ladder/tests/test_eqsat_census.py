"""Scope/cap denominators must not conflate replay, quota and matcher work."""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

from tools.ladder.eqsat_census import chain_census, report_census, run_smoke, validate_smoke
from tools.ladder.eqsat_chain import digest

FIXTURE = Path(__file__).resolve().parents[3] / 'tests/fixtures/eqsat-chains/verified-chain/chain.json'


def chain():
    return json.loads(FIXTURE.read_bytes())


def report():
    c = chain()
    p = dict(name='control', inputDigest=c['input_digest'], outputDigest=c['output_digest'],
             outcome='measured', outcomeDetail='', stepsBefore=10, stepsAfter=5,
             instructionsBefore=20, instructionsAfter=10,
             regions=sum(t['roots']['accepted'] for t in c['transactions']),
             transactions=len(c['transactions']), certificates=len(c['transactions']),
             certificatesReplayed=len(c['transactions']), optimizerWallNanos=None, chainRecord=c)
    return dict(schema='evocc-policy-report-v2',
                policy=dict(name='instruction-count', registry='p0.scalar.v4',
                            costFunction='InstructionCount', schedule=dict(
                                maxNodes=100000, maxEpochs=1024, maxTransactions=8)),
                corpusSource='unit-test', corpusDigest='a' * 64, seed=0, stepBudget=200000,
                programs=[p], aggregate=dict(
                    programs=1, measured=1, undefinedInput=0, fuelExhausted=0, refused=0,
                    programsOptimized=1, **{k: p[k] for k in (
                        'stepsBefore', 'stepsAfter', 'instructionsBefore', 'instructionsAfter',
                        'regions', 'transactions', 'certificates', 'certificatesReplayed',
                        'optimizerWallNanos')}))


def resource_report():
    r = report()
    r['schema'] = 'evocc-policy-report-v3'
    resources = dict(scope='shared-chain-v1', totalWallNanos=100,
                     checkerWallNanos=70, identityWallNanos=15, wrapperWallNanos=5)
    for row in (r['programs'][0], r['aggregate']):
        row.update(optimizerWallNanos=10, chainResources=copy.deepcopy(resources))
    return r


class CensusTests(unittest.TestCase):
    def test_v3_separates_producer_replay_identity_and_wrapper_cost(self):
        r = resource_report()
        result = report_census(r)
        self.assertEqual(result['chain_resources'], r['aggregate']['chainResources'])
        self.assertEqual(result['rows'][0]['chain_resources'], r['programs'][0]['chainResources'])
        self.assertEqual(result['default_caps_observation'], 'NO_HITS_IN_THIS_POPULATION')

    def test_v3_rejects_overlapping_missing_or_fabricated_resource_cost(self):
        for mutate in (
            lambda r: r['programs'][0]['chainResources'].update(checkerWallNanos=101),
            lambda r: r['programs'][0]['chainResources'].pop('identityWallNanos'),
            lambda r: r['aggregate']['chainResources'].update(totalWallNanos=99),
            lambda r: r['programs'][0]['chainResources'].update(wrapperWallNanos=-1),
            lambda r: r['programs'][0]['chainResources'].update(checkerWallNanos=None),
            lambda r: r['programs'][0].update(chainResources=None),
            lambda r: r['programs'][0]['chainResources'].update(scope='cpu-time'),
        ):
            r = resource_report()
            mutate(r)
            with self.subTest(r=r), self.assertRaises(ValueError):
                report_census(r)

    def test_v3_elided_resources_are_unknown_not_zero(self):
        r = resource_report()
        for row in (r['programs'][0], r['aggregate']):
            row['optimizerWallNanos'] = None
            for key in row['chainResources']:
                if key != 'scope':
                    row['chainResources'][key] = None
        result = report_census(r)
        self.assertIsNone(result['chain_resources']['checkerWallNanos'])
        self.assertIsNone(result['chain_resources']['identityWallNanos'])

    def test_v3_no_attempt_retains_null_and_cannot_invent_cost(self):
        r = resource_report()
        r['stepBudget'] = 0
        p = r['programs'][0]
        p.update(outcome='fuelExhausted', chainRecord=None, chainResources=None,
                 optimizerWallNanos=0, transactions=0, certificates=0, certificatesReplayed=0,
                 regions=0, outputDigest=p['inputDigest'], stepsBefore=0, stepsAfter=0,
                 instructionsAfter=p['instructionsBefore'])
        r['aggregate'].update(measured=0, fuelExhausted=1, programsOptimized=0)
        r['aggregate']['instructionsAfter'] = p['instructionsAfter']
        for key in ('optimizerWallNanos', 'transactions', 'certificates', 'certificatesReplayed',
                    'regions', 'stepsBefore', 'stepsAfter'):
            r['aggregate'][key] = 0
        for key in r['aggregate']['chainResources']:
            if key != 'scope':
                r['aggregate']['chainResources'][key] = 0
        result = report_census(r)
        self.assertEqual(result['default_caps_observation'], 'INCOMPLETE')
        self.assertEqual(result['rows'][0]['status'], 'NO_CHAIN')
        self.assertIsNone(result['rows'][0]['chain_resources'])
        p['chainResources'] = copy.deepcopy(r['aggregate']['chainResources'])
        with self.assertRaisesRegex(ValueError, 'cost without a chain attempt'):
            report_census(r)

    def test_zero_fuel_cannot_be_reported_as_a_successful_chain(self):
        r = resource_report()
        r['stepBudget'] = 0
        with self.assertRaisesRegex(ValueError, 'zero-fuel execution'):
            report_census(r)

    def test_retained_real_report_recomputes_without_reclassifying_scope(self):
        directory = FIXTURE.parent.parent / 'cap-census-20260929'
        raw = (directory / 'policy.json').read_bytes()
        receipt = json.loads((directory / 'receipt.json').read_bytes())
        self.assertEqual(digest(raw), receipt['policy_report_digest'])
        self.assertEqual(report_census(json.loads(raw)), receipt['census'])

    def test_no_hit_is_scoped_and_never_a_preflight_verdict(self):
        got = report_census(report())
        self.assertEqual(got['default_caps_observation'], 'NO_HITS_IN_THIS_POPULATION')
        self.assertEqual(got['attempted_programs'], 1)
        self.assertEqual(got['attempted_transactions'], 2)
        self.assertIsNone(got['matching_work'])
        for track in ('policy', 'rule', 'shortcut'):
            self.assertEqual(got[track + '_preflight'], 'NOT_RUN')
        self.assertEqual(got['rows'][0]['proof_accounting']['proof_successes'], 2)

    def test_missing_scope_stale_schema_and_invalid_units_reject(self):
        for mutate in (
            lambda r: r.update(schema='evocc-policy-report-v1'),
            lambda r: r['aggregate'].update(programs=0),
            lambda r: r['aggregate'].update(programs=True),
            lambda r: r['programs'][0].update(chainRecord=None),
            lambda r: r['programs'][0].update(certificatesReplayed=1),
            lambda r: r['programs'][0].update(inputDigest='f' * 64),
            lambda r: r['programs'][0].update(stepsAfter=-1),
            lambda r: r['programs'][0].update(optimizerWallNanos=float('nan')),
            lambda r: r['programs'][0]['chainRecord']['transactions'][0]['scoped_counters'].pop('search'),
            lambda r: r['programs'][0]['chainRecord']['transactions'][0]['scoped_counters'].update(matching_work=0),
            lambda r: r['policy']['schedule'].update(maxEpochs=999),
        ):
            r = report()
            mutate(r)
            with self.subTest(r=r), self.assertRaises(ValueError):
                report_census(r)

    def test_interpreter_failure_is_not_zero_hits(self):
        r = report()
        p = r['programs'][0]
        p.update(outcome='fuelExhausted')
        r['aggregate'].update(measured=0, fuelExhausted=1)
        got = report_census(r)
        self.assertEqual(got['default_caps_observation'], 'INCOMPLETE')
        self.assertEqual(got['observed_programs'], 0)
        self.assertIsNone(got['rows'][0]['hit_transactions'])

    def test_rejected_chain_keeps_exit_classes_without_no_hit_claim(self):
        for reason, bucket in (('CERTIFICATE_MALFORMED', 'bundle_obligation_failures'),
                               ('PROOF_INVALID', 'completed_proof_rejections'),
                               ('CHECKER_TIMEOUT', 'unknown_proof_outcomes')):
            c = chain()
            c.update(publication='REJECTED', rejection_reason=reason, output_digest=None,
                     stop='ERROR', verified_prefix=1)
            c['transactions'][-1].update(verification='REJECTED', rejection_reason=reason)
            row = chain_census(c)
            self.assertEqual(row['status'], 'CHAIN_REJECTED')
            self.assertTrue(all(value is None for value in row['hit_transactions'].values()))
            self.assertEqual(row['proof_accounting'][bucket], 1)

    def test_actual_search_hit_does_not_become_certificate_hit(self):
        c = chain()
        c['transactions'] = [copy.deepcopy(c['transactions'][0])]
        t = c['transactions'][0]
        c.update(attempted_transactions=1, verified_prefix=1,
                 output_digest=t['output_digest'], stop='TRANSACTION_BUDGET')
        t['stop'] = 'POLICY_BUDGET'
        t['scoped_counters']['search'].update(bridge_nodes=100001, nodes_hit=True)
        t['budget_hits'] = [dict(scope='search.bridge_nodes', unit='bridge_nodes',
                                consumed=100001, limit=100000, next=0,
                                reason='CONSUMPTION_EXCEEDED')]
        row = chain_census(c)
        self.assertEqual(row['hit_transactions']['search.bridge_nodes'], 1)
        self.assertEqual(row['hit_transactions']['admission.proof_nodes'], 0)
        self.assertFalse(row['chain_limit_hit'])
        self.assertIsNone(row['matching_work'])

    def test_verified_recovery_is_separate_from_success_and_never_no_hits(self):
        c = chain()
        tx = c['transactions'][-1]
        tx.update(result='ROLLED_BACK_ERROR', stop='INTERNAL_ERROR')
        c.update(publication='REJECTED', rejection_reason='PRODUCER_ERROR',
                 output_digest=None, stop='ERROR', verified_prefix=1)
        row = chain_census(c)
        self.assertEqual(row['proof_accounting']['proof_successes'], 1)
        self.assertEqual(row['proof_accounting']['recovery_proof_successes'], 1)
        self.assertEqual(row['status'], 'CHAIN_REJECTED')
        self.assertEqual(row['chain_rejection_reason'], 'PRODUCER_ERROR')
        self.assertTrue(all(hit is None for hit in row['hit_transactions'].values()))
        c['rejection_reason'] = 'PROOF_INVALID'
        with self.assertRaisesRegex(ValueError, 'hides transaction cause'):
            chain_census(c)
        c['rejection_reason'] = 'NONE'
        with self.assertRaisesRegex(ValueError, 'rejected chain'):
            chain_census(c)

    def test_zero_or_omitted_population_cannot_be_clean(self):
        r = report()
        r['programs'] = []
        with self.assertRaises(ValueError):
            report_census(r)
        r = report()
        r['programs'].append(copy.deepcopy(r['programs'][0]))
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            report_census(r)


class SmokeBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='evocc-cap-collector-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.manifest = dict(
            schema='evocc-cap-smoke-manifest-v1', scope='generated-ir-regression-only',
            implementation_commit='a' * 40,
            evaluator=dict(path='bin/evo-policy-eval', sha256='0' * 64),
            checker=dict(path='bin/evo-proofcheck', sha256='0' * 64),
            generator=dict(path='crates/evocc-randgate/src/generate.rs', sha256='0' * 64),
            generated=dict(first=9300, count=1), seed=0, step_budget=200000,
            policy='instruction-count', resources=dict(timeout_seconds=5,
                output_bytes=1024 * 1024, min_free_bytes=1024 ** 3),
            claim_boundary='not-R02-performance-or-track-admission')
        for field in ('evaluator', 'checker', 'generator'):
            p = self.root / self.manifest[field]['path']
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text('# test fixture, never a production checker\n')
            self.manifest[field]['sha256'] = digest(p.read_bytes())
        self.output = report()
        self.output['programs'][0]['name'] = 'seed-00000000000000009300'
        self.output['programs'][0]['chainRecord']['checker_digest'] = self.manifest['checker']['sha256']
        self.path = self.root / 'tests/contracts/cap-smoke.json'
        self.path.parent.mkdir(parents=True)
        self.command('import json; print(' + repr(json.dumps(self.output)) + ')')

    def command(self, source):
        p = self.root / self.manifest['evaluator']['path']
        p.write_text('#!/usr/bin/env python3\n' + source + '\n')
        p.chmod(0o700)
        self.manifest['evaluator']['sha256'] = digest(p.read_bytes())

    def invoke(self, *, committed=True, source_matches=True):
        self.path.write_text(json.dumps(self.manifest))
        # Only source-control queries are mocked. The collector starts/reaps the real fixture
        # child and uses the real shared file lock and bounded capture files.
        def git_query(argv, **kwargs):
            self.assertEqual(argv[0], 'git')
            if argv[1] == 'show':
                return SimpleNamespace(returncode=0 if committed else 1,
                    stdout=self.path.read_bytes() if committed else b'')
            if argv[1] == 'rev-parse':
                return SimpleNamespace(returncode=0, stdout=self.manifest['implementation_commit'].encode())
            return SimpleNamespace(returncode=0 if source_matches else 1, stdout=b'')
        with patch('tools.ladder.eqsat_census.subprocess.run', side_effect=git_query):
            return run_smoke(self.path, self.root)

    def test_completed_diagnostic_retains_the_exact_input_and_output(self):
        directory, receipt = self.invoke()
        self.assertEqual(receipt['status'], 'OBSERVED')
        self.assertEqual(receipt['native'], 'NOT_RUN')
        self.assertEqual(receipt['census']['rule_preflight'], 'NOT_RUN')
        self.assertEqual(receipt['policy_report_digest'], digest((directory / 'policy.json').read_bytes()))
        old = (directory / 'receipt.json').read_bytes()
        next_dir, _ = self.invoke()
        self.assertNotEqual(next_dir, directory)
        self.assertEqual((directory / 'receipt.json').read_bytes(), old)

    def test_uncommitted_manifest_stale_binary_and_missing_subject_refuse(self):
        with self.assertRaisesRegex(ValueError, 'commit'):
            self.invoke(committed=False)
        with self.assertRaisesRegex(ValueError, 'live build inputs'):
            self.invoke(source_matches=False)
        self.manifest['checker']['sha256'] = 'f' * 64
        with self.assertRaisesRegex(ValueError, 'identity changed'):
            self.invoke()
        self.assertFalse((self.root / '.evidence').exists())

    def test_budget_profile_units_and_unknown_fields_fail_closed(self):
        for mutate in (
            lambda m: m.update(arbitrary_command='echo forged'),
            lambda m: m['generated'].update(count=0),
            lambda m: m['generated'].update(count=65),
            lambda m: m['generated'].update(count=True),
            lambda m: m['resources'].update(timeout_seconds=1000),
            lambda m: m['resources'].update(min_free_bytes=0),
            lambda m: m['resources'].update(output_bytes=1 << 50),
            lambda m: m['checker'].update(path='../evo-proofcheck'),
            lambda m: m.update(claim_boundary='full-performance-PASS'),
        ):
            m = copy.deepcopy(self.manifest)
            mutate(m)
            with self.subTest(m=m), self.assertRaises(ValueError):
                validate_smoke(m)

    def test_lock_contention_and_disk_shortage_create_no_capture(self):
        import fcntl
        (self.root / '.build').mkdir()
        with (self.root / '.build/bench.lock').open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'shared lock'):
                self.invoke()
        with patch('tools.ladder.eqsat_census.shutil.disk_usage', return_value=SimpleNamespace(free=1)):
            with self.assertRaisesRegex(ValueError, 'disk'):
                self.invoke()
        self.assertFalse((self.root / '.evidence').exists())

    def test_missing_program_is_preserved_as_failure_not_smaller_denominator(self):
        self.manifest['generated']['count'] = 2
        directory, receipt = self.invoke()
        self.assertEqual(receipt['status'], 'COLLECTION_FAILED')
        self.assertIn('missing programs', receipt['error'])
        self.assertTrue((directory / 'policy.json').exists())

    def test_timeout_reaps_child_and_retains_failed_attempt(self):
        self.manifest['resources']['timeout_seconds'] = 1
        self.command('import os,time; print(os.getpid(), flush=True); time.sleep(30)')
        directory, receipt = self.invoke()
        self.assertEqual(receipt['status'], 'COLLECTION_FAILED')
        self.assertIn('watchdog', receipt['error'])
        pid = int((directory / 'policy.json').read_text())
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)

    def test_output_limit_is_hard_not_an_unbounded_error_log(self):
        self.manifest['resources']['output_bytes'] = 1024
        self.command('import os; os.write(1, b"x" * 1000000)')
        directory, receipt = self.invoke()
        self.assertEqual(receipt['status'], 'COLLECTION_FAILED')
        self.assertLessEqual(sum((directory / name).stat().st_size for name in ('policy.json', 'stderr.txt')), 1024)

    def test_grandchild_scratch_proof_is_not_subject_to_capture_file_limit(self):
        # Exceeds both the old 4 MiB inherited limit and this test's capture cap.
        self.command('import subprocess,sys\n'
                     'subprocess.run([sys.executable,"-c",'
                     + repr('from pathlib import Path; Path("scratch.evp").write_bytes(b"x" * (8 * 1024 * 1024))')
                     + '], check=True)\nprint(' + repr(json.dumps(self.output)) + ')')
        directory, receipt = self.invoke()
        self.assertEqual(receipt['status'], 'OBSERVED', receipt)
        self.assertEqual((self.root / 'scratch.evp').stat().st_size, 8 * 1024 * 1024)
        self.assertLess((directory / 'policy.json').stat().st_size, self.manifest['resources']['output_bytes'])

    def test_exited_leader_with_live_pipe_descendant_is_bounded(self):
        self.manifest['resources']['timeout_seconds'] = 1
        self.command('import subprocess,sys\n'
                     'p=subprocess.Popen([sys.executable,"-c","import time; time.sleep(30)"])\n'
                     'print(p.pid, flush=True)')
        _, receipt = self.invoke()
        self.assertEqual(receipt['status'], 'COLLECTION_FAILED')
        self.assertIn('watchdog', receipt['error'])


if __name__ == '__main__':
    unittest.main()
