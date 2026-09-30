import itertools
import unittest

from tools.ladder.eqsat_chain import AXES
from tools.ladder.eqsat_preflight import enumerate_policies, rule_verdict


class PreflightTests(unittest.TestCase):
    def test_difference_constraints_match_independent_small_rank_enumeration(self):
        pairs = [('add', None), ('mul', None), ('shl', None),
                 ('mul', 'add'), ('shl', 'add'), ('add', 'shl')]
        result = enumerate_policies(pairs, 1024)
        ordered = result['comparisons']
        expected = set()
        for a, m, s in itertools.product(range(4), repeat=3):
            values = {'add': a, 'mul': m, 'shl': s, None: 0}
            expected.add(tuple(values[x] > values[y] for x, y in ordered))
        self.assertEqual(result['status'], 'ENUM_COMPLETE')
        self.assertEqual({tuple(c['signature']) for c in result['candidates']}, expected)
        for candidate in result['candidates']:
            values = dict(zip(AXES, candidate['weights'])); values[None] = 0
            self.assertEqual(tuple(values[x] > values[y] for x, y in ordered), tuple(candidate['signature']))

    def test_zero_weight_quota_choices_are_not_deleted(self):
        result = enumerate_policies([('add', None), ('and', None)], 16)
        self.assertEqual(len(result['candidates']), 4)
        self.assertIn([False, False], [c['signature'] for c in result['candidates']])

    def test_no_pairs_is_one_policy_and_limit_is_not_completeness(self):
        self.assertEqual(len(enumerate_policies([], 1)['candidates']), 1)
        result = enumerate_policies([('add', None)], 1)
        self.assertEqual(result['status'], 'ENUM_BUDGET_EXHAUSTED')

    def test_underfunded_incomplete_or_unproved_rule_track_never_becomes_searchable(self):
        cell = dict(status='ENUM_BUDGET_EXHAUSTED', allowed=100, consumed=100)
        self.assertEqual(rule_verdict(cell, ai_budget=100, multiplier=1,
            useful_proved_witness=True, unresolved=False), 'SEARCHABLE')
        for change, kwargs in ((dict(allowed=99, consumed=99), {}),
                               (dict(consumed=90), {}), ({}, dict(unresolved=True)),
                               ({}, dict(useful_proved_witness=False)), ({}, dict(ai_budget=0))):
            args = dict(ai_budget=100, multiplier=1, useful_proved_witness=True, unresolved=False)
            args.update(kwargs)
            self.assertEqual(rule_verdict({**cell, **change}, **args), 'INCONCLUSIVE')
        self.assertEqual(rule_verdict(dict(cell, status='ENUM_COMPLETE'), ai_budget=100,
            multiplier=1, useful_proved_witness=True, unresolved=False), 'FINITE_ENUMERATION_SUFFICIENT')


if __name__ == '__main__':
    unittest.main()
