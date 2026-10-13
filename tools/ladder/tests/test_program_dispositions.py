"""Reviewed applicability is separate from an original program's raw outcome."""

import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('e0_dispositions', Path(__file__).resolve().parents[1] / 'e0.py')
e0 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(e0)


class ProgramDispositionTest(unittest.TestCase):
    def test_repaired_scalar_llvm_cases_and_timeout_have_no_obsolete_fp_waiver(self):
        root = Path(__file__).resolve().parents[3]
        programs = json.loads((root / 'tests/contracts/benchmark-program-dispositions.json').read_text())['programs']
        originals = [
            'Benchmarks/Polybench/linear-algebra/solvers/lu/lu.c',
            'Benchmarks/Polybench/linear-algebra/solvers/ludcmp/ludcmp.c',
            'Benchmarks/Polybench/medley/floyd-warshall/floyd-warshall.c',
            'Regression/C/gcc-c-torture/execute/20020720-1.c',
            'Regression/C/gcc-c-torture/execute/ieee/compare-fp-1.c',
            'Regression/C/gcc-c-torture/execute/ieee/compare-fp-3.c',
            'Regression/C/gcc-c-torture/execute/ieee/compare-fp-4.c',
            'Regression/C/gcc-c-torture/execute/ieee/pr50310.c',
        ]
        for source in originals:
            with self.subTest(source=source):
                self.assertNotIn('llvm-test-suite:SingleSource/' + source, programs)

    def test_pr23135_source_ub_review_binds_the_unchanged_original_command(self):
        # L10 under the B01 source-UB rule: the recorded command has no -fwrapv.
        # A -fwrapv replay is a labeled control, never the original's PASS.
        root = Path(__file__).resolve().parents[3]
        programs = json.loads((root / 'tests/contracts/benchmark-program-dispositions.json').read_text())['programs']
        rule = programs['llvm-test-suite:SingleSource/Regression/C/gcc-c-torture/execute/pr23135.c']
        self.assertEqual(rule['disposition'], 'not-applicable')
        self.assertEqual(rule['sourceSha256'], '55b64d3f85e48564b7275634e277d9c3a86bb9ba9181ebb9aed0e64f64295693')
        self.assertEqual(rule['originalArguments'][:12], [
            '/usr/bin/clang-19', '-DNDEBUG', '-O3', '-DNDEBUG', '-w', '-Werror=date-time',
            '-Wno-implicit-int', '-Wno-int-conversion', '-Wno-implicit-function-declaration',
            '-w', '-Wno-psabi', '-Wno-error=incompatible-pointer-types'])
        self.assertNotIn('-fwrapv', rule['originalArguments'])
        self.assertIn('tests/fixtures/llvm-pr23135-definedness.out', rule['evidence'])
        for path in rule['evidence']:
            with self.subTest(path=path):
                self.assertTrue((root / path).is_file())

    def measure(self, rule=None, *, evidence=True, raw='fail', emission='emitted'):
        source = 'float declaration_only; int main(void){return 1;}\n'
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = root / 'original'
            inputs.mkdir()
            (inputs / 'probe.c').write_text(source)
            subject = root / 'evocc'
            subject.write_bytes(b'compiler')
            contracts = root / 'tests/contracts'
            contracts.mkdir(parents=True)
            if rule is not None:
                rule = {'disposition': 'not-applicable', 'reason': 'measured oracle conflict',
                        'sourceSha256': hashlib.sha256(source.encode()).hexdigest(),
                        'evidence': ['probe.out'], **rule}
                (contracts / 'benchmark-program-dispositions.json').write_text(json.dumps({'programs': {'torture:probe': rule}}))
            if evidence:
                (root / 'probe.out').write_text('independent probe results\n')
            with (
                patch.object(e0, 'ROOT', root),
                patch.object(e0, 'BUILD', root / 'build'),
                patch.object(e0, 'EVIDENCE', root / 'evidence'),
                patch.object(e0, 'RECEIPT', root / 'receipt.json'),
                patch.object(e0, 'fetch', return_value=inputs),
                patch.object(e0, 'sysroot', return_value=Path('/sysroot')),
                patch.object(e0, 'sh', return_value=SimpleNamespace(stdout='source-commit')),
                patch.object(e0, 'emit', return_value={'probe': {'emit': emission, 'code': 'E3050', 'options': {'applied': [], 'unhandled': []}}}),
                patch.object(e0, 'run_in_container', return_value={'probe': {'run': raw, 'exit': 1 if raw == 'fail' else 0}}),
                patch.object(e0, 'interpreter_verdict', return_value='diagnostic'),
                patch('sys.argv', ['e0', '--suite', 'torture', '--evocc', str(subject)]),
            ):
                e0.main()
            row = json.loads(next((root / 'evidence').glob('*/results.jsonl')).read_text())
            receipt = json.loads((root / 'receipt.json').read_text())
            return row, receipt['latest']['suites']['torture']

    def test_reviewed_exclusion_preserves_raw_failure_and_source_identity(self):
        row, summary = self.measure({})
        self.assertEqual(row['outcome'], 'fail')
        self.assertEqual(row['disposition'], 'not-applicable')
        self.assertEqual(row['dispositionEvidence'], ['probe.out'])
        self.assertEqual(summary['outcomes'], {'fail': 1})
        self.assertEqual(summary['dispositionCounts']['not-applicable'], 1)
        self.assertEqual(summary['dispositionCounts']['compiler-failure'], 0)

    def test_missing_evidence_or_changed_source_cannot_hide_a_failure(self):
        for rule, exists in [({}, False), ({'sourceSha256': 'changed'}, True), ({'disposition': 'PASS'}, True)]:
            with self.subTest(rule=rule, exists=exists):
                row, _ = self.measure(rule, evidence=exists)
                self.assertEqual(row['disposition'], 'compiler-failure')
                self.assertIn('dispositionError', row)

    def test_floating_point_requires_program_specific_execution_evidence(self):
        row, _ = self.measure()
        self.assertEqual(row['disposition'], 'compiler-failure')
        row, summary = self.measure({'disposition': 'FP-deferred', 'reason': 'executed arithmetic proved by separate probe',
                                     'deferredFeatures': ['binary16']}, emission='refused')
        self.assertEqual(row['outcome'], 'refused')
        self.assertEqual(row['disposition'], 'FP-deferred')
        self.assertEqual(summary['dispositionCounts']['FP-deferred'], 1)
        row, _ = self.measure(raw='pass')
        self.assertEqual(row['disposition'], 'PASS')

    def test_unusable_reference_control_never_becomes_a_subject_pass(self):
        row, summary = self.measure({
            'disposition': 'oracle-unusable',
            'reason': 'recorded fixed reference compiler cannot compile the original',
        })
        self.assertEqual(row['outcome'], 'fail')
        self.assertEqual(row['disposition'], 'oracle-unusable')
        self.assertEqual(summary['dispositionCounts']['PASS'], 0)
        self.assertEqual(summary['dispositionCounts']['oracle-unusable'], 1)

    def test_a_fresh_executed_pass_resolves_only_the_old_fp_deferral(self):
        row, summary = self.measure({'disposition': 'FP-deferred'}, raw='pass')
        self.assertEqual(row['outcome'], 'pass')
        self.assertEqual(row['disposition'], 'PASS')
        self.assertEqual(summary['dispositionCounts']['FP-deferred'], 0)
        for disposition in ['not-applicable', 'oracle-unusable']:
            row, _ = self.measure({'disposition': disposition}, raw='pass')
            self.assertEqual(row['disposition'], disposition)

    def test_old_fp_deferrals_cannot_hide_new_link_or_runtime_failures(self):
        for outcome in ['fail', 'asm-fail', 'link-fail', 'timeout', 'not-run']:
            with self.subTest(outcome=outcome):
                row, summary = self.measure({'disposition': 'FP-deferred'}, raw=outcome)
                self.assertEqual(row['outcome'], outcome)
                self.assertEqual(row['disposition'], 'compiler-failure')
                self.assertEqual(summary['dispositionCounts']['compiler-failure'], 1)

    def test_fp_refusal_reviews_must_name_a_still_deferred_feature(self):
        for features in [None, [], ['binary64'], ['binary128'], ['strict-fenv'],
                         ['binary16', 'unknown']]:
            with self.subTest(features=features):
                row, _ = self.measure({'disposition': 'FP-deferred',
                                       'deferredFeatures': features}, emission='refused')
                self.assertEqual(row['disposition'], 'compiler-failure')
                self.assertIn('dispositionError', row)
