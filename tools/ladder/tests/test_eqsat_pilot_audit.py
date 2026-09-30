import copy
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch

from tools.ladder.eqsat_chain import digest
from tools.ladder.eqsat_twins import canonical
from tools.ladder.eqsat_opportunities import enumerate_rule_cell, lean_candidate
from tools.ladder.eqsat_preflight import enumerate_policies
from tools.ladder.eqsat_preflight_run import CONTRACT, shortcut_census, section_identity
from tools.ladder.eqsat_pilot_audit import audit_preflight, audit_pilot, audit_usage


def control_capture():
    """An empty helper-level census, not an admissible whole-pilot manifest."""
    root, directory = Path('/unit/repo'), Path('/unit/capture')
    bounds = copy.deepcopy(CONTRACT)
    m = dict(preflight=bounds, cases=[], tools={
        k: dict(path='/unit/'+k, sha256='0'*64) for k in
        ('compiler', 'checker', 'census', 'ir_backend', 'collector_python', 'lean')})
    artifacts = {}
    def artifact(name, data):
        artifacts['preflight/'+name] = canonical(data)+b'\n'
    survivors, cells = [], []
    for size in bounds['rule_cells']:
        cell = enumerate_rule_cell(size, syntax_cap=bounds['syntax_visits_per_cell'])
        artifact(f'rule-cell-{size}.json', cell)
        survivors.extend(cell['survivors'])
        cells.append({k: v for k, v in cell.items() if k != 'survivors'})
    controls = [(('add', ('x',), ('c', 0)), ('x',)), (('mul', ('x',), ('c', 1)), ('x',))]
    ids = sorted(digest(canonical([lhs, rhs])) for lhs, rhs in controls)
    candidates = {s['candidate_digest']: s for s in survivors}
    proofs, attempts = [], {}
    for n, identity in enumerate(ids):
        source = directory/'preflight'/(identity+'.lean')
        candidate = candidates[identity]
        artifacts['preflight/'+source.name] = lean_candidate(candidate['lhs'], candidate['rhs']).encode()
        output = f'{n:03}.stdout'
        artifacts[output] = b''
        stage = 'preflight:template:'+identity
        attempts[stage] = dict(stdout=output, status='FAILED', exit_code=1, failure_kind='COMMAND_EXIT',
            argv=['/unit/collector_python', str(root/'tools/ladder/eqsat_template.py'),
                  '/unit/lean', str(root/'lean'), str(source)])
        proofs.append(dict(candidate=identity, status='UNKNOWN', template=bounds['template'],
            attempt_stdout=output, reason='fixed template failed'))
    rule = dict(cells=cells, fuzz_survivors=len(survivors), proofs=proofs,
        full_suite_evaluations=0, ai_budget=bounds['ai_budget'],
        enumeration_floor=bounds['ai_budget']*bounds['enumeration_multiplier'],
        verdict='INCONCLUSIVE', reason='NO_BOUNDED_NEW_RULE_INSTALLATION_OR_UTILITY_PROOF',
        scope='FIXED_ONE_VARIABLE_GRAMMAR_NOT_COMPLETE_P0_RULE_SPACE',
        residual_candidates={}, unresolved_residuals=[], omitted_relevant_templates=0,
        unattempted_templates=len(survivors)-len(proofs))
    oracle = dict(patterns=0, selected=0, out_of_budget=0,
        authority='OPPORTUNITY_ONLY_NOT_EQUIVALENCE',
        mapping='INITIALIZED_BITS_NO_OVERFLOW_FLAGS_CONSTANT_IN_RANGE_SHIFTS_ONLY', residuals=[])
    space = enumerate_policies([], bounds['signature_visits'])
    evaluations = [dict(weights=space['candidates'][0]['weights'], max_transactions=schedule,
        status='OBSERVED', units=[], programs=[], cap_hits=[]) for schedule in bounds['schedules']]
    policy = dict(space=space, evaluations=evaluations, observed_new_comparisons=[],
        objective='ALLOCATABLE_AND_RELOCATION_SECTION_IDENTITY_AND_SIZE', native_fitness='NOT_USED',
        verdict='FINITE_ENUMERATION_SUFFICIENT', whole_suite_evaluations=2,
        scope='FROZEN_CONSULTED_PAIRS_AND_SCHEDULE_GRID_ONLY', code_classes=1)
    p = dict(schema='evocc-r02-preflight-v1', status='BOUNDED_PREFLIGHT_COMPLETED',
        bounds=bounds, default_caps=[], inventory=[], oracle=oracle, shortcut=shortcut_census([]),
        rule=rule, policy=policy, attempts=[], campaign_admission='NO_AI_CAMPAIGN_AUTHORIZED_BY_THIS_CAPTURE')
    for name, data in (('default-caps.json', []), ('inventory.json', []), ('oracle.json', oracle),
                       ('shortcut.json', p['shortcut']), ('rule.json', rule), ('policy-space.json', space),
                       ('report.json', p)):
        artifact(name, data)
    return m, dict(cases=[], preflight=p), directory, root, artifacts, attempts


def audit_fixture(f):
    m, r, directory, root, artifacts, attempts = f
    def read(path):
        return artifacts[str(path)]
    import json
    def load(path):
        return json.loads(read(path))
    def unused(*args):
        raise AssertionError('empty census must not execute or silently add a program')
    return audit_preflight(m, r, directory, root, read, load, lambda s: attempts[s],
                           unused, unused, unused, Path('/unit/checker'), {})


class PilotAuditTests(unittest.TestCase):
    def test_resource_units_and_invalid_measurements_fail_closed(self):
        usage = dict(user_ns=1, system_ns=2, peak_rss_bytes=3072, peak_rss_raw=3,
            rss_unit='bytes', scope='wait4-direct-child-including-waited-descendants',
            wall_scope='spawn-to-stdio-drain')
        audit_usage(usage, 'Linux')
        audit_usage(dict(usage, peak_rss_bytes=3), 'Darwin')
        for invalid in (dict(usage, user_ns=True), dict(usage, system_ns=-1),
                        dict(usage, peak_rss_bytes=3), dict(usage, scope='heap-only')):
            with self.assertRaises(ValueError):
                audit_usage(invalid, 'Linux')

    def test_failed_template_is_unknown_not_a_proof_success(self):
        result = audit_fixture(control_capture())
        self.assertEqual(result['template_proofs'], 0)
        self.assertEqual(result['template_unknowns'], 2)
        self.assertEqual(result['rule_verdict'], 'INCONCLUSIVE')
        self.assertEqual(result['ai_campaign'], 'NOT_ADMITTED')

    def test_integrity_or_resource_failure_cannot_be_laundered_as_template_unknown(self):
        for reason in ('RETENTION_LIMIT', 'ARTIFACT_CHANGED', 'OUTPUT_LIMIT',
                       'WATCHDOG', 'DESCENDANT_LIVE', 'PROCESS_LAUNCH'):
            fixture = control_capture()
            for row in fixture[-1].values():
                row.update(failure_kind=reason, error=reason)
            with self.subTest(reason=reason), self.assertRaises(ValueError):
                audit_fixture(fixture)

    def test_mutating_summary_and_matching_json_does_not_bypass_recomputation(self):
        for change in (
            lambda p: p['rule'].update(verdict='SEARCHABLE'),
            lambda p: p['rule'].update(fuzz_survivors=0),
            lambda p: p['policy'].update(code_classes=2),
            lambda p: p['policy']['evaluations'].pop(),
            lambda p: p['shortcut'].update(shortcut_fuel_saving=100),
            lambda p: p['rule']['proofs'][0].update(status='PROVED_TEMPLATE_ONLY_NOT_INSTALLED'),
        ):
            f = control_capture()
            p = f[1]['preflight']
            change(p)
            for name in ('rule', 'shortcut'):
                f[4]['preflight/'+name+'.json'] = canonical(p[name])+b'\n'
            f[4]['preflight/report.json'] = canonical(p)+b'\n'
            with self.assertRaises(ValueError):
                audit_fixture(f)

    def test_object_materialization_ignores_debug_not_relocations_or_allocated_bytes(self):
        text = dict(name='.text', flags=6, type=1, data_digest='1'*64)
        relocation = dict(name='.rela.text', flags=0, type=4, data_digest='2'*64)
        debug = dict(name='.debug_info', flags=0, type=1, data_digest='3'*64)
        base = section_identity(dict(sections=[text, relocation, debug]))
        self.assertEqual(base, section_identity(dict(sections=[text, relocation, dict(debug, data_digest='4'*64)])))
        self.assertNotEqual(base, section_identity(dict(sections=[dict(text, data_digest='4'*64), relocation, debug])))
        self.assertNotEqual(base, section_identity(dict(sections=[text, dict(relocation, data_digest='4'*64), debug])))

    def test_shortcuts_count_verified_compositions_without_inventing_fuel_savings(self):
        text = ('region 0\nrewrite 1 first 0 0\nrewrite 2 second 0 0\nendregion\n'
                'region 1\nrewrite 3 first 0 0\nendregion\n')
        result = shortcut_census([('unit', text)])
        self.assertEqual(result['regions'], 2)
        self.assertEqual(result['multi_rewrite_regions'], 1)
        self.assertEqual(result['compositions'][0]['rules'], ['first', 'second'])
        self.assertEqual(result['compositions'][0]['shortcut_fuel_saving'], 'NOT_MEASURED')
        self.assertEqual(result['verdict'], 'INCONCLUSIVE')


@unittest.skipUnless(os.environ.get('EVOCC_R02_CAPTURE'), 'requires a retained real pilot capture')
class PilotCaptureMutationTests(unittest.TestCase):
    def test_retained_capture_and_read_only_mutations(self):
        root = Path(__file__).resolve().parents[3]
        directory = Path(os.environ['EVOCC_R02_CAPTURE']).resolve()
        expected = os.environ['EVOCC_R02_MANIFEST_SHA256']
        receipt_path = directory/'receipt.json'
        original_read = Path.read_bytes
        original = original_read(receipt_path)
        healthy = json.loads(original)
        self.assertEqual(audit_pilot(directory, root, expected)['status'], 'CAPTURE_CONSISTENT')
        def attempt(r, marker):
            return next(a for a in r['attempts'] if marker in a['stage'])
        mutations = (
            lambda r: r.update(status='INCOMPLETE'),
            lambda r: r['cases'][0]['units'][0]['variants']['A']['native']['checked'].update(native_ub_profile='unchecked'),
            lambda r: attempt(r, ':frontend:')['argv'].remove('-O2'),
            lambda r: attempt(r, ':backend:')['argv'].__setitem__(-1, '--ub-checks=unchecked'),
            lambda r: attempt(r, ':checked-admission:').update(exit_code=5),
            lambda r: r['cases'][0]['variants']['A']['unchecked']['inputs'][0]['identity'].update(ir_digest='0'*64),
            lambda r: r['cases'][0]['comparisons'][0]['samples'].pop(),
            lambda r: r['preflight']['policy'].update(code_classes=1000000),
            lambda r: r['preflight']['rule'].update(verdict='SEARCHABLE'),
            lambda r: r['preflight']['shortcut'].update(verdict='SEARCHABLE'),
        )
        for mutate in mutations:
            changed = copy.deepcopy(healthy)
            mutate(changed)
            replacements = {receipt_path: canonical(changed)+b'\n'}
            # Change both summary copies. Independent reconstruction, not
            # duplicate-JSON equality, must still reject the forged claim.
            replacements[directory/'preflight/report.json'] = canonical(changed['preflight'])+b'\n'
            for name in ('rule', 'shortcut'):
                replacements[directory/'preflight'/(name+'.json')] = canonical(changed['preflight'][name])+b'\n'
            def read(path):
                return replacements[path] if path in replacements else original_read(path)
            with self.subTest(mutation=mutations.index(mutate)), patch.object(Path, 'read_bytes', read):
                with self.assertRaises((ValueError, KeyError, IndexError)):
                    audit_pilot(directory, root, expected)
        self.assertEqual(original_read(receipt_path), original, 'negative tests must not edit evidence')


if __name__ == '__main__':
    unittest.main()
