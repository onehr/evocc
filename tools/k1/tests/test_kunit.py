"""KUnit acceptance must not confuse a boot, a module load, and case results."""

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
sys.path.append(str(TOOLS))
SPEC = importlib.util.spec_from_file_location('kernel_kunit', TOOLS / 'kunit.py')
kunit = None
if SPEC and Path(SPEC.origin).exists():
    kunit = importlib.util.module_from_spec(SPEC)
    SPEC.loader.exec_module(kunit)


class KUnitAcceptanceTest(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(kunit, 'the common KUnit acceptance adapter is missing')

    @staticmethod
    def parsed(outcomes=('PASS', 'PASS')):
        return {'parserStatus': 'SUCCESS', 'parserErrors': 0, 'parserCrashed': 0,
                'parserTests': len(outcomes), 'consoleReplays': [],
                'cases': [{'path': 'group/repeated', 'outcome': value} for value in outcomes]}

    def transcript(self, *, end='0', complete='1', nonce='flight', body='KTAP'):
        return (f'EVO_BOOT_NONCE flight\nKUNIT_MODULE_BEGIN {nonce} example\n{body}\n'
                f'KUNIT_MODULE_END {nonce} example {end}\n'
                f'KUNIT_MODULE_COMPLETE {nonce} {complete}\nEVO_INIT_DONE\n')

    def test_module_load_and_duplicate_case_multiplicity_are_separate(self):
        result = kunit.parse_modules(self.transcript(), 'flight', ['example'],
                                     lambda _: self.parsed())
        self.assertTrue(result['complete'])
        self.assertEqual(result['filesLoaded'], 1)
        self.assertEqual(result['tests'], 2)
        self.assertEqual(result['outcomes'], {'PASS': 2})
        self.assertEqual([row['occurrence'] for row in result['cases']], [1, 2])

    def test_module_load_success_does_not_override_a_failed_case(self):
        result = kunit.parse_modules(self.transcript(), 'flight', ['example'],
                                     lambda _: self.parsed(('PASS', 'FAIL')))
        self.assertEqual(result['status'], 'FAIL')
        self.assertTrue(result['complete'])

    def test_missing_module_boundary_is_not_complete_even_with_passing_tap(self):
        for text in (self.transcript().replace('KUNIT_MODULE_END flight example 0\n', ''),
                     self.transcript().replace('KUNIT_MODULE_COMPLETE flight 1\n', ''),
                     self.transcript(complete='0'), self.transcript(nonce='old'),
                     self.transcript() + 'KUNIT_MODULE_BEGIN flight example\n',
                     self.transcript().replace('KUNIT_MODULE_END flight example 0',
                                               'KUNIT_MODULE_END flight different 0')):
            with self.subTest(text=text), self.assertRaises(ValueError):
                kunit.parse_modules(text, 'flight', ['example'], lambda _: self.parsed())

    def test_module_order_and_population_are_not_silently_reduced(self):
        with self.assertRaises(ValueError):
            kunit.parse_modules(self.transcript(), 'flight', ['first', 'example'],
                                lambda _: self.parsed())
        with self.assertRaises(ValueError):
            kunit.parse_modules(self.transcript(), 'flight', ['example', 'example'],
                                lambda _: self.parsed())

    def test_failed_load_empty_output_and_parser_errors_remain_incomplete(self):
        for end, parsed in (('1', self.parsed()), ('0', self.parsed(())),
                            ('0', {**self.parsed(), 'parserErrors': 1}),
                            ('0', {**self.parsed(), 'parserCrashed': 1})):
            with self.subTest(end=end, parsed=parsed):
                result = kunit.parse_modules(self.transcript(end=end), 'flight', ['example'],
                                             lambda _: parsed)
                self.assertFalse(result['complete'])
                self.assertEqual(result['status'], 'INCOMPLETE')

    def test_pair_does_not_turn_equal_failures_or_skips_into_passes(self):
        for outcomes, expected in ((('PASS', 'PASS'), 'PASS'),
                                   (('PASS', 'FAIL'), 'FAIL'),
                                   (('PASS', 'SKIP'), 'INCOMPLETE')):
            cases = kunit.number_cases(self.parsed(outcomes)['cases'])
            side = {'complete': True, 'cases': cases}
            comparison = kunit.compare_cases(side, side, 2)
            self.assertEqual(comparison['status'], expected)
            self.assertTrue(comparison['executionComplete'])

    def test_pair_rejects_lost_duplicate_and_wrong_case_with_unchanged_count(self):
        cases = kunit.number_cases(self.parsed()['cases'])
        reference = {'complete': True, 'cases': cases}
        for rows in (cases[:1], [{**cases[0], 'path': 'other'}, cases[1]]):
            result = kunit.compare_cases(reference, {'complete': True, 'cases': rows}, 2)
            self.assertFalse(result['executionComplete'])
            self.assertEqual(result['status'], 'INCOMPLETE')

    def test_contract_keeps_builtin_and_module_populations_distinct(self):
        contract = json.loads((TOOLS.parents[1] / 'tests/contracts/kernel-expanded-kunit-contract.json').read_text())
        self.assertEqual(contract['builtInCases'], 1504)
        self.assertEqual(contract['moduleCases'], 765)
        self.assertEqual(len(contract['suiteModules']), 31)
        self.assertEqual(len({row['name'] for row in contract['suiteModules']}), 31)

    def test_runtime_rejects_a_different_image_disk_init_or_module_population(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            image, config = folder / 'Image', folder / '.config'
            image.write_bytes(b'kernel')
            config.write_bytes(b'config')
            (folder / 'initramfs.cpio').write_bytes(b'init')
            (folder / 'scratch.img').write_bytes(b'modules')
            contract = {'suiteModules': [{'name': 'one', 'unit': 'one.ko'}]}
            modules = {'status': 'PASS', 'imageSha256': kunit.selftest.sha(image),
                       'configSha256': kunit.selftest.sha(config),
                       'modules': [{'unit': 'one.ko', 'linkedSha256': 'body'}]}
            build = {'kernelModules': modules, **contract,
                     'rootfsSha256': kunit.selftest.sha(folder / 'scratch.img'),
                     'initramfsSha256': kunit.selftest.sha(folder / 'initramfs.cpio')}
            (folder / 'build.json').write_text(json.dumps(build))
            self.assertEqual(len(kunit.runtime_inputs(folder, image, config, None, contract)), 3)
            for filename in ('Image', '.config', 'scratch.img', 'initramfs.cpio'):
                path = folder / filename
                old = path.read_bytes()
                path.write_bytes(b'changed')
                with self.subTest(filename=filename), self.assertRaises(ValueError):
                    kunit.runtime_inputs(folder, image, config, None, contract)
                path.write_bytes(old)
            with self.assertRaises(ValueError):
                kunit.runtime_inputs(folder, image, config, {'modules': [{'unit': 'one.ko', 'sha256': 'other'}]}, contract)
            with self.assertRaises(ValueError):
                kunit.runtime_inputs(folder, image, config, None, {'suiteModules': []})

    def test_readonly_check_does_not_boot_or_write_an_acceptance_receipt(self):
        with patch.object(kunit, 'upstream_parser', return_value=None), \
             patch.object(kunit.selftest_full, 'kernel_selection', side_effect=ValueError('stale Image')), \
             patch.object(kunit, 'flight') as flight:
            self.assertEqual(kunit.main(['--check']), 2)
            flight.assert_not_called()

    def test_upstream_parser_keeps_fail_skip_and_rejects_truncation_and_extra_results(self):
        source = kunit.parser().parse_args([]).source_root
        if not (source / 'tools/testing/kunit/kunit_parser.py').exists():
            self.skipTest('frozen Linux parser unavailable; exercised by the provisioned KUnit lane')
        contract = json.loads(kunit.CONTRACT.read_text())
        parse = kunit.upstream_parser(source, contract)
        text = ('KTAP version 1\n1..1\n    # Subtest: example\n    1..3\n'
                '    ok 1 repeated\n    not ok 2 repeated\n'
                '    ok 3 unavailable # SKIP no device\nnot ok 1 example\n')
        result = kunit.summarize(parse(text))
        self.assertTrue(result['complete'])
        self.assertEqual(result['outcomes'], {'PASS': 1, 'FAIL': 1, 'SKIP': 1})
        self.assertFalse(kunit.summarize(parse(text.replace('not ok 1 example\n', '')))['complete'])
        for extra in ('ok 2 forged\n', 'KTAP version 1\n1..1\nok 1 forged\n'):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                parse(text + extra)
        with self.assertRaises(ValueError):
            parse('no test output')


if __name__ == '__main__':
    unittest.main()
