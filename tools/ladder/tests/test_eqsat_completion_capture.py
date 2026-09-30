"""Optional real completion replay and early one-field adversarial mutations.

No retained evidence is modified. A patched reader supplies a candidate receipt
to the independent auditor; streams, objects and certificates remain original.
"""
import copy
import os
from pathlib import Path
import unittest
from unittest.mock import patch

from tools.ladder import eqsat_completion_audit as auditor
from tools.ladder.eqsat_chain import digest


@unittest.skipUnless(os.environ.get('EVOCC_EQSAT_COMPLETION_CAPTURE'), 'real completion capture not selected')
class CompletionCaptureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = Path(__file__).resolve().parents[3]
        cls.directory = Path(os.environ['EVOCC_EQSAT_COMPLETION_CAPTURE']).resolve()
        cls.identity = digest((cls.directory/'manifest.json').read_bytes())
        cls.receipt = auditor.load(cls.directory/'receipt.json')

    def test_actual_complete_capture_replays(self):
        result = auditor.audit_completion(self.directory, self.root, self.identity)
        self.assertEqual(result['policy_evaluations'], 1024)
        self.assertEqual(result['verified_chains'], 6144)
        self.assertEqual(result['unattempted_templates'], 0)

    def test_incomplete_population_cannot_claim_complete(self):
        self.reject(lambda r: r['policy']['evaluations'].pop())

    def test_policy_signature_is_not_a_receipt_label(self):
        self.reject(lambda r: r['policy']['evaluations'][0]['weights'].__setitem__(0, 999))

    def test_checked_object_metrics_are_recomputed(self):
        def mutate(r):
            key = r['policy']['evaluations'][0]['units'][0]['native']['checked']
            r['code'][key]['text_bytes'] += 4
        self.reject(mutate)

    def test_shared_chain_identity_cannot_be_substituted(self):
        self.reject(lambda r: r['policy']['evaluations'][0]['units'][0]['binding'].update(ir_digest='0'*64))

    def test_cap_hits_are_not_inferred_from_a_status_label(self):
        self.reject(lambda r: r['policy']['evaluations'][0]['units'][0].update(hits=['FORGED']))

    def test_template_unknown_cannot_be_promoted_to_proved(self):
        def mutate(r):
            p = next(p for p in r['rule']['proofs'] if p['status'] == 'UNKNOWN')
            p.update(status='PROVED_TEMPLATE_ONLY_NOT_INSTALLED', output=p['attempt_stdout'])
        self.reject(mutate)

    def test_native_timing_claim_is_outside_the_completion(self):
        self.reject(lambda r: r.update(native_timing='PASS'))

    def reject(self, mutate):
        candidate = copy.deepcopy(self.receipt)
        mutate(candidate)
        original = auditor.load
        def altered(path):
            return candidate if Path(path).resolve() == self.directory/'receipt.json' else original(path)
        with patch.object(auditor, 'load', side_effect=altered), self.assertRaises((ValueError, KeyError)):
            auditor.audit_completion(self.directory, self.root, self.identity)


if __name__ == '__main__':
    unittest.main()
