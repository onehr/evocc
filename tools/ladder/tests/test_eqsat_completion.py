"""Finite completion and immutable deduplication, not a new AI campaign."""
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from tools.ladder.eqsat_chain import AXES, digest
from tools.ladder.eqsat_completion import (BOUNDS, FILES, REAUDIT_FILES, REAUDIT_PURPOSE,
    deduplicate, deduplicate_one, reaudit_files, validate_plan, validate_reaudit)
from tools.ladder.eqsat_completion_audit import require_comparisons
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


class ComparisonTelemetryTests(unittest.TestCase):
    def test_json_arrays_match_recomputed_pairs_including_null(self):
        require_comparisons([['icmp', None], ['mul', 'add']], [('icmp', None), ('mul', 'add')])
        require_comparisons([['add', 'shl']], [('add', 'shl')])
        require_comparisons([], [])

    def test_a_changed_order_value_or_multiplicity_is_rejected(self):
        original = [('icmp', None), ('mul', 'add')]
        for changed in ([['mul', 'add'], ['icmp', None]], [['icmp', None], ['mul', 'shl']],
                        [['icmp', None]], [['icmp', None], ['mul', 'add'], ['mul', 'add']]):
            with self.assertRaises(ValueError):
                require_comparisons(changed, original)


class ReauditContractTests(unittest.TestCase):
    def test_public_binding_requires_committed_supplement_and_both_code_revisions(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            def git(*args):
                return subprocess.check_output(['git', '-c', 'user.name=Fixture', '-c',
                    'user.email=fixture@example.invalid', *args], cwd=root, stderr=subprocess.DEVNULL)
            def commit():
                git('add', '.')
                git('commit', '-qm', 'fixture')
                return git('rev-parse', 'HEAD').decode().strip()
            git('init', '-q')
            m = plan()
            raw = b'original manifest'
            receipt = b'original receipt'
            for name in REAUDIT_FILES:
                path = root/name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'original code')
                m['files'][name] = digest(path.read_bytes())
            m['implementation_commit'] = commit()
            for name in REAUDIT_FILES:
                (root/name).write_bytes(b'repaired code')
            revision = commit()
            capture = root/'.evidence/original'
            capture.mkdir(parents=True)
            (capture/'manifest.json').write_bytes(raw)
            (capture/'receipt.json').write_bytes(receipt)
            path = root/'tests/contracts/reaudit.json'
            path.parent.mkdir(parents=True)
            s = dict(schema='evocc-r02-completion-reaudit-v1', purpose=REAUDIT_PURPOSE,
                implementation_commit=revision, manifest_sha256=digest(raw),
                capture=dict(directory='.evidence/original', receipt_sha256=digest(receipt)),
                repairs={name: dict(before_sha256=m['files'][name], after_sha256=digest(b'repaired code'))
                         for name in REAUDIT_FILES})
            path.write_text(json.dumps(s))
            with self.assertRaisesRegex(ValueError, 'commit the re-audit supplement'):
                reaudit_files(path, root, m, raw, capture)
            commit()
            self.assertEqual(reaudit_files(path, root, m, raw, capture),
                             {name: digest(b'repaired code') for name in REAUDIT_FILES})
            (capture/'receipt.json').write_bytes(b'changed receipt')
            with self.assertRaisesRegex(ValueError, 'capture identity changed'):
                reaudit_files(path, root, m, raw, capture)
            (capture/'receipt.json').write_bytes(receipt)
            (capture/'manifest.json').write_bytes(b'changed manifest')
            with self.assertRaisesRegex(ValueError, 'manifest differs'):
                reaudit_files(path, root, m, raw, capture)
            (capture/'manifest.json').write_bytes(raw)
            s['implementation_commit'] = m['implementation_commit']
            path.write_text(json.dumps(s))
            commit()
            with self.assertRaisesRegex(ValueError, 'declared commit'):
                reaudit_files(path, root, m, raw, capture)

    def test_supplement_binds_original_manifest_capture_and_exact_repair_files(self):
        m = plan()
        s = dict(schema='evocc-r02-completion-reaudit-v1', purpose=REAUDIT_PURPOSE,
            implementation_commit='1'*40, manifest_sha256='2'*64,
            capture=dict(directory='.evidence/original', receipt_sha256='3'*64),
            repairs={name: dict(before_sha256=m['files'][name], after_sha256='4'*64)
                     for name in REAUDIT_FILES})
        validate_reaudit(s, m, '2'*64, '.evidence/original', '3'*64)
        for mutate in (
            lambda x: x.update(manifest_sha256='5'*64),
            lambda x: x['capture'].update(directory='.evidence/other'),
            lambda x: x['capture'].update(receipt_sha256='5'*64),
            lambda x: x.update(bounds={}),
            lambda x: x.update(purpose='NEW_EXPERIMENT'),
            lambda x: x['repairs'].update({'tools/ladder/eqsat_chain.py':
                dict(before_sha256='0'*64, after_sha256='4'*64)}),
            lambda x: x['repairs'].pop(REAUDIT_FILES[0]),
            lambda x: x['repairs'][REAUDIT_FILES[0]].update(before_sha256='5'*64),
            lambda x: x['repairs'][REAUDIT_FILES[0]].update(after_sha256='0'*64),
            lambda x: x['repairs'][REAUDIT_FILES[0]].update(after_sha256='not-a-digest'),
        ):
            changed = copy.deepcopy(s)
            mutate(changed)
            with self.assertRaises(ValueError):
                validate_reaudit(changed, m, '2'*64, '.evidence/original', '3'*64)


class ContentAddressedEvidenceTests(unittest.TestCase):
    def test_equal_artifacts_keep_all_names_and_exact_bytes_without_copies(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = root/'bundle'
            store = root/'store'
            bundle.mkdir()
            store.mkdir()
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
            root = Path(temp)
            store = root/'store'
            store.mkdir()
            file = root/'output.evo'
            file.write_bytes(b'canonical')
            target = store/digest(b'canonical')
            target.write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError, 'mutation'):
                deduplicate_one(file, store)
            self.assertEqual(file.read_bytes(), b'canonical')

    def test_a_symbolic_link_is_not_an_owned_bundle_file(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = root/'bundle'
            store = root/'store'
            bundle.mkdir()
            store.mkdir()
            (root/'external').write_bytes(b'external')
            os.symlink(root/'external', bundle/'aliased')
            with self.assertRaisesRegex(ValueError, 'unowned'):
                deduplicate(bundle, store)


if __name__ == '__main__':
    unittest.main()
