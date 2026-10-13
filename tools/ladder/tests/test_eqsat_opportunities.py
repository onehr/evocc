import unittest
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from tools.ladder.eqsat_opportunities import (grammar_terms, enumerate_rule_cell, evaluate,
    lean_candidate, diagnostic_expressions, llvm_function, llvm_functions, residual_candidate)


class OpportunityTests(unittest.TestCase):
    def test_fixed_template_compiles_with_the_pinned_project_when_built(self):
        root = Path(__file__).resolve().parents[3]
        lean = shutil.which('lean')
        if lean is None or not (root/'lean/.lake/build/lib/lean/EvoIR/Proofs/Rules.olean').is_file():
            self.skipTest('pinned Lean project has not been built')
        with tempfile.TemporaryDirectory(prefix='evocc-lean-control-') as scratch:
            source = Path(scratch)/'Control.lean'
            source.write_text(lean_candidate(('add', ('x',), ('c', 0)), ('x',)))
            env = dict(os.environ, LEAN_PATH=str(root/'lean/.lake/build/lib/lean'), LEAN_NUM_THREADS='1')
            result = subprocess.run([lean, str(source)], cwd=root/'lean', env=env,
                                    capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
            self.assertIn('RuleCovered preflight_candidate', result.stdout)
            self.assertNotIn('sorryAx', result.stdout)
            self.assertNotIn('ofReduceBool', result.stdout)

    def test_syntax_coverage_follows_independent_tree_recurrence(self):
        counts = [3]
        for n in range(1, 4):
            counts.append(6*sum(counts[i]*counts[n-1-i] for i in range(n)))
        for n in range(3):
            terms = grammar_terms(n)
            self.assertEqual(len(terms), counts[n])
            self.assertEqual(len(terms), len(set(terms)))
        result = enumerate_rule_cell(1, syntax_cap=162)
        self.assertEqual(result['status'], 'ENUM_COMPLETE')
        self.assertEqual(result['visits'], 162)
        self.assertEqual(result['visits'], result['counterexamples']+len(result['survivors']))
        self.assertEqual(enumerate_rule_cell(1, syntax_cap=161)['status'], 'ENUM_INCOMPLETE_ERROR')

    def test_fuzz_is_not_proof_and_template_is_fixed(self):
        r = enumerate_rule_cell(1, syntax_cap=162)
        self.assertTrue(r['survivors'])
        self.assertTrue(all(s['proof'] == 'NOT_ATTEMPTED' for s in r['survivors']))
        t = lean_candidate(('add', ('x',), ('c', 0)), ('x',))
        self.assertIn('RuleCovered preflight_candidate', t)
        self.assertIn('rule_one preflight_candidate', t)
        self.assertNotIn('bv_decide', t)
        with self.assertRaises(ValueError):
            lean_candidate(('arbitrary tactic', ('x',), ('x',)), ('x',))
        self.assertEqual(evaluate(('sub', ('c', 0), ('x',)), 8, 1), 255)

    def test_mapping_refuses_variable_shift_and_never_adds_overflow_flags(self):
        row = dict(function=0, name='f', block=0, root=2, width=32, op='shl',
            operands=[dict(id=0, width=32), dict(id=1, width=64)], constant=None)
        census = dict(schema='evocc-opportunity-census-v1', authority='PRODUCER_DIAGNOSTIC_NOT_PROOF',
            input_digest='0'*64, eligible=1, excluded={}, roots=[row])
        result = diagnostic_expressions(census)
        self.assertEqual(result['mapping_exclusions'], {'LLVM_SHIFT_DOMAIN_NOT_TOTAL': 1})
        row['op'] = 'add'
        row['operands'][1]['width'] = 32
        result = diagnostic_expressions(census)
        ident, entry = next(iter(result['expressions'].items()))
        llvm = llvm_function(ident, entry['expression'])
        self.assertIn('add i32', llvm)
        self.assertNotIn('nsw', llvm)
        self.assertNotIn('nuw', llvm)
        self.assertEqual(set(llvm_functions(llvm)), {ident})

    def test_residual_grammar_does_not_launder_multi_variable_rules(self):
        r = dict(expression=('xor', 32, ('v', 32, 0), ('v', 32, 0)), reference_body='ret i32 0')
        self.assertEqual(residual_candidate(r)['rhs'], ('c', 0))
        r['expression'] = ('xor', 32, ('v', 32, 0), ('v', 32, 1))
        self.assertEqual(residual_candidate(r)['outside_grammar'], 'VARIABLES_OUTSIDE_GRAMMAR')


if __name__ == '__main__':
    unittest.main()
