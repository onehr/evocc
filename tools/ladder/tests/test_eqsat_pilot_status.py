"""The pilot checkpoint must not manufacture final performance acceptance."""

import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location('eqsat_pilot_status', ROOT / 'tools/render-status.py')
status = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(status)
NAME = 'eqsat-r02-pilot-receipt-20260930.json'


def checkpoint():
    return json.loads((ROOT / 'tests/contracts' / NAME).read_bytes())


class PilotStatusTests(unittest.TestCase):
    def render(self, receipt):
        return '\n'.join(status.eqsat_pilot_status({NAME: receipt}))

    def test_missing_checkpoint_is_not_run(self):
        self.assertEqual(status.eqsat_pilot_status({}), ['R02a/b/d bounded Linux pilot: NOT_RUN'])

    def test_lean_rules_use_separate_registry_denominators(self):
        receipts = {
            'lean-p0-receipt.json': {
                'binding': {
                    'registry_name': 'p0.scalar.v4', 'registry_rules': 2,
                    'guard_registry': {'name': 'g0.ub.v1', 'rules': 1},
                },
                'segments': {'A': {'targets': [
                    {}, {'registry': 'p0.scalar.v4'}, {'registry': 'g0.ub.v1'},
                ]}},
            },
            'lean-correspondence-receipt.json': {
                'total': {'compared': 17, 'differences': 0},
            },
        }
        output = status.render_ascii(receipts, evoir='1.9.116', evomir='0.55')
        self.assertIn('Lean rules: p0.scalar.v4 2/2; g0.ub.v1 1/1', output)
        self.assertIn('Rust/Lean model differential: 17 comparisons; 0 differences', output)
        self.assertNotIn('3/2', output)

    def test_qualified_pilot_does_not_promote_parent_gates(self):
        output = status.render_ascii({NAME: checkpoint()}, evoir='1.9.115', evomir='0.54')
        self.assertIn('ARTIFACT_AUDIT_PASS; 576 timed; 138 EVP certificates; 108 checked twins', output)
        self.assertIn('Timing protocol: NOT_QUALIFIED', output)
        self.assertIn('6 TU / 12 transactions', output)
        self.assertIn('INCONCLUSIVE / INCONCLUSIVE / INCONCLUSIVE', output)
        self.assertIn('2/12 A/A within floor; 12/12 A/B inconclusive', output)
        self.assertIn('AI campaign NOT_ADMITTED', output)
        for gate in ('G2 EqSat', 'Q2 GCC', 'Q3 measured'):
            self.assertTrue(any(line.strip().startswith(gate) and 'NOT_ACCEPTED' in line
                                for line in output.splitlines()), gate)
        self.assertTrue(output.isascii())

    def test_incomplete_or_forged_denominators_are_unverified(self):
        mutations = (
            lambda r: r.update(status='INCOMPLETE'),
            lambda r: r['timing'].update(timed_executions=575),
            lambda r: r['timing'].update(aa_within_effect_floor=13),
            lambda r: r['timing'].update(ab_inconclusive=13),
            lambda r: r['proof_accounting'].update(proof_successes=139),
            lambda r: r['proof_accounting'].update(completed_proof_rejections=1),
            lambda r: r['checked_twins'].update(admitted=107),
            lambda r: r['checked_twins'].update(identity='SAME_SOURCE_ONLY'),
            lambda r: r['preflight']['policy'].update(observed_evaluations=15),
            lambda r: r['preflight']['rule'].update(verdict='FUZZ_PROVED_COMPLETE'),
            lambda r: r['retention'].update(owned_vm_cleanup_exit_code=1),
            lambda r: r['provenance'].update(checker_sha256='unrecorded'),
        )
        for index, mutate in enumerate(mutations):
            with self.subTest(mutation=index):
                data = checkpoint()
                mutate(data)
                self.assertIn('UNVERIFIED', self.render(data))
                self.assertNotIn('QUALIFIED;', self.render(data))

    def test_failed_or_missing_gate_cannot_hide_behind_qualified(self):
        for gate in ('rust_workspace', 'python_collector', 'real_capture_mutations'):
            data = checkpoint()
            data['gates'][gate]['status'] = 'FAIL'
            with self.subTest(gate=gate):
                self.assertIn('UNVERIFIED', self.render(data))
        data = checkpoint()
        data['gates']['real_capture_mutations']['evidence_unchanged'] = False
        self.assertIn('UNVERIFIED', self.render(data))
        del data['gates']
        self.assertIn('UNVERIFIED', self.render(data))

    def test_counter_types_and_negative_values_fail_closed(self):
        for invalid in (True, -1, '576', None):
            data = checkpoint()
            data['timing']['timed_executions'] = invalid
            with self.subTest(value=invalid):
                self.assertIn('UNVERIFIED', self.render(data))

    def test_checkpoint_binds_tracked_manifest_and_does_not_mutate_receipts(self):
        data = checkpoint()
        before = copy.deepcopy(data)
        manifest_bytes = (ROOT / data['provenance']['manifest']).read_bytes()
        self.assertEqual(hashlib.sha256(manifest_bytes).hexdigest(), data['provenance']['manifest_sha256'])
        manifest = json.loads(manifest_bytes)
        self.assertEqual(manifest['implementation_commit'], data['provenance']['implementation_commit'])
        for receipt_key, tool in (('compiler_sha256', 'compiler'), ('checker_sha256', 'checker'),
                                  ('ir_backend_sha256', 'ir_backend')):
            self.assertEqual(data['provenance'][receipt_key], manifest['tools'][tool]['sha256'])
        self.render(data)
        self.assertEqual(data, before)


if __name__ == '__main__':
    unittest.main()
