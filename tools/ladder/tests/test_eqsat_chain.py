"""R01a archive mutation gate, using the actual driver and standalone checker."""
import copy
import importlib.util
import itertools
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('eqsat_chain', ROOT / 'tools/ladder/eqsat_chain.py')
chain = importlib.util.module_from_spec(spec)
spec.loader.exec_module(chain)


class IdentityArchive(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='evocc-identity-contract-')
        cls.root = Path(cls.temp.name)
        target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
        cls.driver = target / 'debug/evocc'
        cls.source = ROOT / 'tests/fixtures/verified-eqsat-chain.c'
        cls.bundle = cls.root / 'identity'
        out = subprocess.run([str(cls.driver), '--emit=evoir', '-O2',
                              f'--eq-proof-dir={cls.bundle}', str(cls.source)], capture_output=True, check=False)
        if out.returncode:
            raise AssertionError(out.stderr.decode())
        cls.ir = out.stdout
        cls.record = json.loads((cls.bundle / 'chain.json').read_bytes())

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_actual_driver_exports_identity_without_claiming_a_proof(self):
        with patch.object(chain.subprocess, 'run', side_effect=AssertionError('no checker for identity')):
            audit = chain.verify_bundle(self.bundle)
        self.assertTrue(audit['accepted'])
        self.assertEqual(audit['identity_checks'], 1)
        self.assertEqual(audit['proof_counts']['proof_successes'], 0)
        self.assertEqual(audit['replay'], [])
        self.assertEqual((self.bundle / 'input.evo').read_bytes(), self.ir)
        self.assertEqual((self.bundle / 'output.evo').read_bytes(), self.ir)
        out = subprocess.run(['python3', str(ROOT / 'tools/bench.py'), '--verify-eqsat-chain', str(self.bundle)],
                             capture_output=True, check=False)
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertEqual(json.loads(out.stdout)['proof_counts']['proof_successes'], 0)

    def test_asm_and_ir_paths_bind_the_same_unoptimized_module(self):
        directory = self.root / 'assembly'
        args = [str(self.driver), '--emit=asm', '-O2', str(self.source)]
        plain = subprocess.run(args, capture_output=True, check=False)
        retained = subprocess.run([*args, f'--eq-proof-dir={directory}'], capture_output=True, check=False)
        self.assertEqual(plain.returncode, 0, plain.stderr)
        self.assertEqual(retained.returncode, 0, retained.stderr)
        self.assertTrue(plain.stdout)
        self.assertEqual(plain.stdout, retained.stdout)
        self.assertEqual((directory / 'input.evo').read_bytes(), self.ir)
        self.assertEqual((directory / 'chain.json').read_bytes(), (self.bundle / 'chain.json').read_bytes())

    def test_identity_one_field_mutations_and_changed_bytes_are_rejected(self):
        mutations = dict(mode='eqsat-on', attempted_transactions=True, transactions=[{}],
                         publication='VERIFIED', schema='evocc-verified-chain-v1',
                         input_digest='f' * 64, output_digest='f' * 64, chain_id='f' * 64,
                         checker_digest='f' * 64)
        for key, value in mutations.items():
            directory = self.root / ('mutated-' + key)
            shutil.copytree(self.bundle, directory)
            record = {**self.record, key: value}
            (directory / 'chain.json').write_text(json.dumps(record))
            with self.subTest(key=key), self.assertRaises(ValueError):
                chain.verify_bundle(directory)
        directory = self.root / 'changed-bytes'
        shutil.copytree(self.bundle, directory)
        changed = self.ir + b'\n'
        (directory / 'output.evo').write_bytes(changed)
        (directory / 'chain.json').write_text(json.dumps({**self.record, 'output_digest': chain.digest(changed)}))
        with self.assertRaisesRegex(ValueError, 'changed the IR'):
            chain.verify_bundle(directory)

    def test_no_implicit_checker_or_fake_evp_file(self):
        with self.assertRaisesRegex(ValueError, 'does not use'):
            chain.verify_bundle(self.bundle, Path('/missing/checker'))
        directory = self.root / 'fake-proof'
        shutil.copytree(self.bundle, directory)
        (directory / '000.proof.evp').write_text('fake')
        with self.assertRaisesRegex(ValueError, 'no EVP'):
            chain.verify_bundle(directory)

    def test_malformed_top_level_is_a_contract_error(self):
        directory = self.root / 'array-record'
        shutil.copytree(self.bundle, directory)
        (directory / 'chain.json').write_text('[]')
        with self.assertRaisesRegex(ValueError, 'must be an object'):
            chain.verify_bundle(directory)

    def test_existing_directory_and_inactive_flags_refuse_without_stdout(self):
        for options in ([f'--eq-proof-dir={self.bundle}'],
                        ['--eq-checker=/missing'], ['--eq-policy=instruction-count']):
            out = subprocess.run([str(self.driver), '--emit=evoir', *options, str(self.source)],
                                 capture_output=True, check=False)
            self.assertNotEqual(out.returncode, 0)
            self.assertFalse(out.stdout)
        for mode in ('-E', '--parse-only', '--emit=c-hir', '--interpret-c-hir'):
            directory = self.root / ('invalid-' + mode.replace('/', '_'))
            out = subprocess.run([str(self.driver), mode, f'--eq-proof-dir={directory}', str(self.source)],
                                 capture_output=True, check=False)
            self.assertNotEqual(out.returncode, 0)
            self.assertFalse(directory.exists())


class ChainArchive(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='evocc-chain-contract-')
        cls.root = Path(cls.temp.name)
        target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
        cls.driver, cls.checker = target / 'debug/evocc', target / 'debug/evo-proofcheck'
        cls.bundle = cls.root / 'original'
        cls.source = ROOT / 'tests/fixtures/verified-eqsat-chain.c'
        result = subprocess.run([str(cls.driver), '--emit=evoir', '-Oeq-scalar',
                                 f'--eq-checker={cls.checker}',
                                 f'--eq-checker-sha256={chain.digest(cls.checker.read_bytes())}',
                                 f'--eq-proof-dir={cls.bundle}', str(cls.source)], capture_output=True, check=False)
        if result.returncode:
            raise AssertionError(result.stderr.decode())
        cls.record = json.loads((cls.bundle / 'chain.json').read_bytes())

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_independent_replay_and_common_entry(self):
        result = chain.verify_bundle(self.bundle, self.checker)
        self.assertTrue(result['accepted'])
        self.assertEqual(result['proof_counts']['proof_successes'], len(self.record['transactions']))
        self.assertGreaterEqual(len(result['replay']), 2)
        out = subprocess.run(['python3', str(ROOT / 'tools/bench.py'), '--verify-eqsat-chain', str(self.bundle),
                              '--checker', str(self.checker)], capture_output=True, check=False)
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertEqual(json.loads(out.stdout), result)

    def test_sealed_chain_replays_without_relabeling_its_native_subject(self):
        directory = ROOT / 'tests/fixtures/eqsat-chains/verified-chain'
        raw = (directory / 'chain.json').read_bytes()
        record = json.loads(raw, object_pairs_hook=chain.unique)
        chain.validate_record(record)
        native = json.loads((directory / 'native.json').read_bytes())
        self.assertEqual(native['chain_record_digest'], chain.digest(raw))
        self.assertEqual(native['verified_ir_digest'], record['output_digest'])
        for tx in record['transactions']:
            paths = []
            for suffix, key in (('before.evo', 'input_digest'), ('after.evo', 'output_digest'), ('proof.evp', 'proof_digest')):
                path = directory / f"{tx['index']:03}.{suffix}"
                self.assertEqual(chain.digest(path.read_bytes()), tx[key])
                paths.append(str(path))
            # Current checker semantic replay is not a new native acceptance.
            result = subprocess.run([str(self.checker), *paths], capture_output=True, timeout=30, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_closed_product_and_unknown_enums(self):
        sets = (('CHANGED', 'UNCHANGED', 'ROLLED_BACK_ERROR'), ('NORMAL', 'POLICY_BUDGET', 'INTERNAL_ERROR'),
                ('EMITTED', 'NONE'), ('NOT_CHECKED', 'VERIFIED', 'REJECTED'), sorted(chain.REASONS))
        accepted = sum(chain.valid_state(*row) for row in itertools.product(*sets))
        self.assertEqual(accepted, 45)
        self.assertEqual(3 * 3 * 2 * 3 * 8 - accepted, 387)
        self.assertFalse(chain.valid_state('CHANGED', 'NORMAL', 'EMITTED', 'REJECTED', 'MADE_UP'))
        self.assertTrue(chain.valid_state('UNCHANGED', 'NORMAL', 'NONE', 'REJECTED', 'CERTIFICATE_MISSING'))
        self.assertTrue(chain.valid_state('ROLLED_BACK_ERROR', 'INTERNAL_ERROR', 'NONE', 'NOT_CHECKED', 'NONE'))

    def test_mutated_identity_states_and_counters(self):
        mutations = (
            lambda r: r.update(chain_id='0' * 64),
            lambda r: r.update(output_digest='0' * 64),
            lambda r: r.update(verified_prefix=0),
            lambda r: r.update(max_transactions=1),
            lambda r: r.update(stop='TRANSACTION_LIMIT'),
            lambda r: r['transactions'].reverse(),
            lambda r: r['transactions'].pop(),
            lambda r: r['policy']['cost_weights'].__setitem__(0, 999),
            lambda r: r['transactions'][0].update(verification='NOT_CHECKED'),
            lambda r: r['transactions'][0].update(result='ROLLED_BACK_ERROR'),
            lambda r: r['transactions'][0].update(rejection_reason='PROOF_INVALID'),
            lambda r: r['transactions'][0]['roots'].update(accepted=0),
            lambda r: r['transactions'][0]['roots'].update(eligible=999999),
            lambda r: r['transactions'][0]['scoped_counters'].update(matching_work=0),
            lambda r: r['transactions'][0]['scoped_counters']['search'].update(nodes_hit=True),
            lambda r: r['transactions'][0].update(unknown=0),
            lambda r: r['transactions'][0].update(index=False),
            lambda r: r['transactions'][0].update(proof_digest='not-a-hash'),
        )
        for mutate in mutations:
            record = copy.deepcopy(self.record)
            mutate(record)
            with self.assertRaises(ValueError):
                chain.validate_record(record)

    def test_absent_or_modified_bundle_does_not_replay_as_success(self):
        for name in ('000.before.evo', '000.after.evo', '000.proof.evp'):
            with tempfile.TemporaryDirectory(dir=self.root) as temp:
                bundle = Path(temp) / 'bundle'
                shutil.copytree(self.bundle, bundle)
                path = bundle / name
                path.write_bytes(path.read_bytes() + b' ')
                result = chain.verify_bundle(bundle, self.checker)
                self.assertFalse(result['accepted'])
                self.assertEqual(result['replay'][0]['rejection_reason'], 'BUNDLE_MISMATCH')
                path.unlink()
                result = chain.verify_bundle(bundle, self.checker)
                self.assertFalse(result['accepted'])
                self.assertEqual(result['replay'][0]['rejection_reason'], 'CERTIFICATE_MISSING' if name.endswith('.evp') else 'BUNDLE_MISMATCH')

    def test_exit_two_and_three_are_not_one_rejection_bucket(self):
        for malformed in (True, False):
            with tempfile.TemporaryDirectory(dir=self.root) as temp:
                bundle = Path(temp) / 'bundle'
                shutil.copytree(self.bundle, bundle)
                path = bundle / '000.proof.evp'
                proof = b'not a proof' if malformed else re.sub(rb'^output [0-9a-f]{64}', b'output ' + b'0' * 64, path.read_bytes(), flags=re.M)
                self.assertNotEqual(proof, path.read_bytes())
                path.write_bytes(proof)
                record = copy.deepcopy(self.record)
                record['transactions'][0]['proof_digest'] = chain.digest(proof)
                (bundle / 'chain.json').write_text(json.dumps(record))
                result = chain.verify_bundle(bundle, self.checker)
                self.assertFalse(result['accepted'])
                bucket = 'bundle_obligation_failures' if malformed else 'completed_proof_rejections'
                self.assertEqual(result['proof_counts'][bucket], 1)
                self.assertEqual(result['proof_counts']['unknown_proof_outcomes'], 0)

    def test_outages_are_unknown_not_proof_rejections(self):
        for reason in ('CHECKER_TIMEOUT', 'CHECKER_UNAVAILABLE', 'CHECKER_ERROR'):
            self.assertEqual(chain.proof_bucket('REJECTED', reason, 'UNCHANGED'), 'unknown_proof_outcomes')
        with self.assertRaises(ValueError):
            json.loads('{"x": 1, "x": 2}', object_pairs_hook=chain.unique)

    def test_accounting_keeps_not_checked_separate_from_unknown(self):
        self.assertEqual(chain.proof_bucket('NOT_CHECKED', 'NONE', 'UNCHANGED'), 'not_checked')
        self.assertEqual(chain.proof_bucket('VERIFIED', 'NONE', 'UNCHANGED'), 'proof_successes')
        self.assertEqual(chain.proof_bucket('VERIFIED', 'NONE', 'ROLLED_BACK_ERROR'), 'recovery_proof_successes')
        self.assertEqual(chain.proof_bucket('REJECTED', 'PROOF_INVALID', 'UNCHANGED'), 'completed_proof_rejections')
        for state, reason in (('VERIFIED', 'PROOF_INVALID'), ('NOT_CHECKED', 'CHECKER_ERROR'),
                              ('REJECTED', 'NONE'), ('MISSING', 'NONE')):
            with self.assertRaises(ValueError):
                chain.proof_bucket(state, reason, 'UNCHANGED')

    def test_process_outages_are_retained_as_unknown(self):
        # Process stubs exercise the adapter only, not equality soundness.
        for body, mode, reason in (('exec /bin/sleep 1', 0o700, 'CHECKER_TIMEOUT'),
                                   ('exit 0', 0o600, 'CHECKER_UNAVAILABLE'),
                                   ('exit 1', 0o700, 'CHECKER_ERROR')):
            with tempfile.TemporaryDirectory(dir=self.root) as temp:
                bundle = Path(temp) / 'bundle'
                shutil.copytree(self.bundle, bundle)
                checker = Path(temp) / 'checker'
                checker.write_text('#!/bin/sh\n' + body + '\n')
                checker.chmod(mode)
                record = copy.deepcopy(self.record)
                record.update(checker_digest=chain.digest(checker.read_bytes()),
                              checker_timeout_ms=30 if reason == 'CHECKER_TIMEOUT' else 1000)
                (bundle / 'chain.json').write_text(json.dumps(record))
                audit = chain.verify_bundle(bundle, checker)
                self.assertFalse(audit['accepted'])
                self.assertEqual(audit['replay'][0]['rejection_reason'], reason)
                self.assertEqual(audit['proof_counts']['unknown_proof_outcomes'], 1)
                self.assertEqual(audit['proof_counts']['completed_proof_rejections'], 0)

    def test_missing_certificate_success_and_rollback_are_distinct(self):
        for rollback in (False, True):
            record = copy.deepcopy(self.record)
            record['transactions'] = [record['transactions'][0]]
            record.update(attempted_transactions=1, verified_prefix=0, publication='REJECTED',
                          rejection_reason='PRODUCER_ERROR' if rollback else 'CERTIFICATE_MISSING', stop='ERROR', output_digest=None)
            tx = record['transactions'][0]
            tx.update(certificate_state='NONE', proof_digest=None)
            if rollback:
                tx.update(result='ROLLED_BACK_ERROR', stop='INTERNAL_ERROR', verification='NOT_CHECKED',
                          rejection_reason='NONE', output_digest=tx['input_digest'])
                tx['roots']['accepted'] = 0
            else:
                tx.update(verification='REJECTED', rejection_reason='CERTIFICATE_MISSING')
            chain.validate_record(record)
            tx['verification'] = 'VERIFIED'
            with self.assertRaises(ValueError):
                chain.validate_record(record)


if __name__ == '__main__':
    unittest.main()
