"""A patched dependency supplement must not manufacture complete acceptance."""
import copy
import importlib.util
from pathlib import Path
import unittest

path = Path(__file__).resolve().parents[1] / 'musl_patched.py'
spec = importlib.util.spec_from_file_location('musl_patched', path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SupplementVerdict(unittest.TestCase):
    def complete(self):
        return {'inputsUnchanged': True, 'patched': True, 'suite': 'numerics', 'rows': [
            dict(compiler=cc, opt=opt, library=lib, subject=name,
                 build={'exit': 0}, link={'exit': 0},
                 run={'exit': 0, 'stdout': 'checks=5920 failures=0\n' if name == 'probe' else ''},
                 binarySha256='binary', objectSha256='object', linkMapVerified=True,
                 replacementObjects={'floatscan': 'a', 'powf': 'b'})
            for cc in ('gcc-14', 'clang-19', 'evocc') for opt in ('0', '2')
            for lib in ('reference', 'mixed') for name in ('probe', *module.SUBJECTS)]}

    def test_exact_paired_population_passes(self):
        self.assertEqual(len(self.complete()['rows']), 192)
        self.assertTrue(module.passed(self.complete()))

    def test_time_suite_has_its_own_population_and_probe_contract(self):
        data = self.complete()
        data['suite'] = 'time'
        data['rows'] = [dict(compiler=cc, opt=opt, library=lib, subject=name,
            build={'exit': 0}, link={'exit': 0},
            run={'exit': 0, 'stdout': 'checks=45 failures=0\n' if name == 'probe' else ''},
            binarySha256='binary', objectSha256='object', linkMapVerified=True,
            replacementObjects={'strptime': 'object'})
            for cc in ('gcc-14', 'clang-19', 'evocc') for opt in ('0', '2')
            for lib in ('reference', 'mixed') for name in ('probe', *module.SUITES['time']['subjects'])]
        self.assertEqual(len(data['rows']), 36)
        self.assertTrue(module.passed(data))
        data['rows'][0]['run']['stdout'] = 'checks=5920 failures=0\n'
        self.assertFalse(module.passed(data))

    def test_memory_fault_suite_is_twelve_instrumented_probes_not_original_acceptance(self):
        suite = module.SUITES['wordexp-memory']
        self.assertEqual(suite['subjects'], ())
        self.assertEqual(suite['patch'], 'musl-patched-wordexp.patch')
        self.assertEqual(suite['source_flags']['src/misc/wordexp.c'], (
            '-Dgetdelim=evo_getdelim', '-Drealloc=evo_realloc',
            '-Dcalloc=evo_calloc', '-Dfree=evo_free'))
        self.assertNotIn(suite['fixture'], suite['source_flags'])
        data = {'inputsUnchanged': True, 'patched': True, 'suite': 'wordexp-memory',
            'rows': [dict(compiler=cc, opt=opt, library=lib, subject='probe',
                build={'exit': 0}, link={'exit': 0}, run={'exit': 0,
                    'stdout': 'checks=192 failures=0\n'},
                binarySha256='binary', objectSha256='object', linkMapVerified=True,
                replacementObjects={'wordexp': 'object'})
                for cc in ('gcc-14', 'clang-19', 'evocc') for opt in ('0', '2')
                for lib in ('reference', 'mixed')]}
        self.assertEqual(len(data['rows']), 12)
        self.assertTrue(module.passed(data))
        data['suite'] = 'wordexp'
        self.assertFalse(module.passed(data))
        data['suite'] = 'wordexp-memory'
        data['rows'][0]['run']['stdout'] = 'checks=192 failures=1\n'
        self.assertFalse(module.passed(data))

    def test_wordexp_suite_needs_the_grammar_probe_and_both_original_entries(self):
        suite = module.SUITES['wordexp']
        self.assertEqual(suite['checks'], 133)
        self.assertEqual(suite['subjects'], ('functional/wordexp', 'functional/wordexp-static'))
        data = {'inputsUnchanged': True, 'patched': True, 'suite': 'wordexp', 'rows': [
            dict(compiler=cc, opt=opt, library=lib, subject=name,
                 build={'exit': 0}, link={'exit': 0},
                 run={'exit': 0, 'stdout': 'checks=133 failures=0\n' if name == 'probe' else ''},
                 binarySha256='binary', objectSha256='object', linkMapVerified=True,
                 replacementObjects={'wordexp': 'object'})
            for cc in ('gcc-14', 'clang-19', 'evocc') for opt in ('0', '2')
            for lib in ('reference', 'mixed') for name in ('probe', *suite['subjects'])]}
        self.assertEqual(len(data['rows']), 36)
        self.assertTrue(module.passed(data))
        # The historical transport-only probe is not the current contract.
        transport = copy.deepcopy(data)
        for row in transport['rows']:
            if row['subject'] == 'probe':
                row['run']['stdout'] = 'checks=48 failures=0\n'
        self.assertFalse(module.passed(transport))
        # One unchanged original entry that still fails keeps the supplement at FAIL.
        failing = copy.deepcopy(data)
        next(row for row in failing['rows']
             if row['subject'] == 'functional/wordexp-static')['run']['exit'] = 1
        self.assertFalse(module.passed(failing))
        missing = copy.deepcopy(data)
        missing['rows'] = [row for row in missing['rows'] if row['subject'] != 'functional/wordexp']
        self.assertFalse(module.passed(missing))

    def test_missing_duplicate_failing_or_changed_inputs_never_pass(self):
        for change in ('missing', 'duplicate', 'extra', 'build', 'link', 'run',
                       'timeout', 'missing-hash', 'inputs-changed', 'probe-count', 'probe-errors',
                       'unpatched', 'wrong-object', 'bad-map', 'unknown-suite', 'wrong-suite'):
            data = copy.deepcopy(self.complete())
            row = data['rows'][0]
            if change == 'missing':
                data['rows'].pop()
            elif change == 'duplicate':
                data['rows'].append(copy.deepcopy(row))
            elif change == 'extra':
                row['subject'] = 'not-tested'
            elif change in ('build', 'link', 'run'):
                row[change]['exit'] = 1
            elif change == 'timeout':
                row['run'] = {'exit': None, 'timeout': True}
            elif change == 'missing-hash':
                row.pop('binarySha256')
            elif change == 'inputs-changed':
                data['inputsUnchanged'] = False
            elif change == 'probe-count':
                row['run']['stdout'] = 'checks=1 failures=0\n'
            elif change == 'unpatched':
                data['patched'] = False
            elif change == 'wrong-object':
                row['replacementObjects'].pop('floatscan')
            elif change == 'bad-map':
                row['linkMapVerified'] = False
            elif change == 'unknown-suite':
                data['suite'] = 'invented'
            elif change == 'wrong-suite':
                data['suite'] = 'time'
            else:
                row['run']['stdout'] = 'checks=5920 failures=1\n'
            with self.subTest(change=change):
                self.assertFalse(module.passed(data))

    def test_conf_requires_original_api_compile_and_full_object_population(self):
        suite = module.SUITES['conf']
        data = {'inputsUnchanged': True, 'patched': True, 'suite': 'conf', 'rows': [
            dict(compiler=cc, opt=opt, library=lib, subject=name,
                build={'exit': 0}, link={'exit': 0}, run={'exit': 0,
                    'stdout': 'checks=27 failures=0\n' if name == 'probe' else ''},
                binarySha256='binary', objectSha256='object', linkMapVerified=True,
                replacementObjects={Path(p).stem: 'object' for p in suite['sources']},
                apiBuild={'exit': 0}, testObjects={**{f'{i}.o': 'hash' for i in range(78)},
                    'unistd.o': 'hash'})
            for cc in ('gcc-14', 'clang-19', 'evocc') for opt in ('0', '2')
            for lib in ('reference', 'mixed') for name in ('probe', 'api/main')]}
        self.assertTrue(module.passed(data))
        original = copy.deepcopy(data)
        data['rows'][1]['apiBuild']['exit'] = 1
        self.assertFalse(module.passed(data))
        data = copy.deepcopy(original)
        del data['rows'][1]['testObjects']['0.o']
        self.assertFalse(module.passed(data))
        data = copy.deepcopy(original)
        del data['rows'][1]['testObjects']['unistd.o']
        data['rows'][1]['testObjects']['unexpected.o'] = 'hash'
        self.assertFalse(module.passed(data))

    def test_link_maps_must_name_both_replacements_without_archive_fallback(self):
        objects = [Path('/work/out/floatscan.o'), Path('/work/out/powf.o')]
        text = ''.join(f'LOAD {p}\n' for p in objects)
        self.assertTrue(module.audit_map(text, objects))
        self.assertFalse(module.audit_map(text.replace('LOAD /work/out/powf.o\n', ''), objects))
        self.assertFalse(module.audit_map(text + '/work/libc.a(floatscan.lo)\n', objects))


if __name__ == '__main__':
    unittest.main()
