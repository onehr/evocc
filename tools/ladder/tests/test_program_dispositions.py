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
    def measure(self, rule=None, *, evidence=True, raw='fail'):
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
                patch.object(e0, 'emit', return_value={'probe': {'emit': 'emitted', 'options': {'applied': [], 'unhandled': []}}}),
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
        row, summary = self.measure({'disposition': 'FP-deferred', 'reason': 'executed arithmetic proved by separate probe'})
        self.assertEqual(row['outcome'], 'fail')
        self.assertEqual(row['disposition'], 'FP-deferred')
        self.assertEqual(summary['dispositionCounts']['FP-deferred'], 1)
        row, _ = self.measure(raw='pass')
        self.assertEqual(row['disposition'], 'PASS')

