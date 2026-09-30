"""Finite completion and immutable deduplication, not a new AI campaign."""
import copy
import os
from pathlib import Path
import tempfile
import unittest

from tools.ladder.eqsat_chain import AXES, digest
from tools.ladder.eqsat_completion import BOUNDS, FILES, deduplicate, deduplicate_one, validate_plan
from tools.ladder.eqsat_native import Capture
from tools.ladder.eqsat_opportunities import enumerate_rule_cell
from tools.ladder.eqsat_pilot import GUEST_TOOLS
from tools.ladder.eqsat_preflight import enumerate_policies


def plan():
    return dict(schema='evocc-r02-preflight-completion-v1', implementation_commit='0'*40,
        source=dict(directory='historical', files={}), tools={k: {} for k in
            ('checker', 'historical_checker', 'census', 'ir_backend', 'collector_python', 'lean', 'container')},
        files=dict.fromkeys(FILES, '0'*64), image={}, guest_tools=dict.fromkeys(GUEST_TOOLS, '0'*64),
        bounds=copy.deepcopy(BOUNDS), resources=dict(jobs=1, timeout_seconds=300,
            output_bytes=16*1024**2, min_free_bytes=8*1024**3, max_retained_bytes=3*1024**3,
            campaign_seconds=14400, vm_cpus=2, vm_memory='3G'))


class CompletionContractTests(unittest.TestCase):
    def test_the_two_finite_populations_are_not_search_or_proof_claims(self):
        policies = enumerate_policies([(a, None) for a in AXES[:9]], BOUNDS['signature_visits'])
        self.assertEqual(policies['status'], 'ENUM_COMPLETE')
        self.assertEqual(len(policies['candidates'])*len(BOUNDS['schedules']), 1024)
        cells = [enumerate_rule_cell(n, syntax_cap=BOUNDS['syntax_visits_per_cell']) for n in BOUNDS['rule_cells']]
        self.assertTrue(all(c['status'] == 'ENUM_COMPLETE' for c in cells))
        self.assertEqual(sum(len(c['survivors']) for c in cells), 1222)
        self.assertTrue(all(c['proof'] == 'NOT_ATTEMPTED' for cell in cells for c in cell['survivors']))

    def test_changed_budget_template_population_or_fitness_is_rejected(self):
        m = plan()
        validate_plan(m)
        for mutate in (
            lambda x: x['bounds'].update(whole_suite_evaluations=16),
            lambda x: x['bounds'].update(proof_attempts=2),
            lambda x: x['bounds'].update(native_timing='SEARCH_FITNESS'),
            lambda x: x['bounds'].update(rule_installation='ENABLED'),
            lambda x: x['bounds'].update(template='AI_SCRIPT'),
            lambda x: x['resources'].update(min_free_bytes=0),
            lambda x: x['resources'].update(jobs=True),
            lambda x: x['bounds']['optimizer'].update(proof_required=1),
            lambda x: x['files'].pop('tools/ladder/eqsat_completion_audit.py'),
        ):
            changed = copy.deepcopy(m)
            mutate(changed)
            with self.assertRaises(ValueError):
                validate_plan(changed)


class ContentAddressedEvidenceTests(unittest.TestCase):
    def test_equal_artifacts_keep_all_names_and_exact_bytes_without_copies(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); bundle = root/'bundle'; store = root/'store'
            bundle.mkdir(); store.mkdir()
            for name in ('input.evo', 'output.evo', '000.before.evo'):
                (bundle/name).write_bytes(b'canonical IR\n')
            (bundle/'chain.json').write_bytes(b'chain identity\n')
            deduplicate(bundle, store)
            self.assertEqual(len(list(bundle.iterdir())), 4)
            self.assertEqual(len(list(store.iterdir())), 2)
            self.assertEqual((bundle/'input.evo').stat().st_ino, (bundle/'output.evo').stat().st_ino)
            self.assertEqual((bundle/'input.evo').read_bytes(), b'canonical IR\n')
            capture = Capture(root, dict(max_retained_bytes=28), {}, {}, lambda: None, unique_inodes=True)
            capture.watch()
            legacy = Capture(root, dict(max_retained_bytes=28), {}, {}, lambda: None)
            with self.assertRaisesRegex(ValueError, 'retention'):
                legacy.watch()

    def test_corrupt_store_does_not_replace_the_original_evidence(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); store = root/'store'; store.mkdir()
            file = root/'output.evo'; file.write_bytes(b'canonical')
            target = store/digest(b'canonical'); target.write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError, 'mutation'):
                deduplicate_one(file, store)
            self.assertEqual(file.read_bytes(), b'canonical')

    def test_a_symbolic_link_is_not_an_owned_bundle_file(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); bundle = root/'bundle'; store = root/'store'
            bundle.mkdir(); store.mkdir()
            (root/'external').write_bytes(b'external')
            os.symlink(root/'external', bundle/'aliased')
            with self.assertRaisesRegex(ValueError, 'unowned'):
                deduplicate(bundle, store)


if __name__ == '__main__':
    unittest.main()
