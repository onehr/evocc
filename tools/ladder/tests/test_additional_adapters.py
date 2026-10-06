"""The remaining application rungs need real upstream test consumers."""

import importlib.util
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


class AdditionalAdapterTests(unittest.TestCase):
    def adapter(self, name):
        path = ROOT / 'tools/ladder' / f'{name}.py'
        self.assertTrue(path.is_file(), f'{name} must have an executable adapter')
        return load(f'ladder_test_{name}', path)

    def test_all_four_are_registered(self):
        bench = load('additional_bench', ROOT / 'tools/bench.py')
        for name in ('llvm-test-suite', 'jq', 'lua', 'sqlite'):
            with self.subTest(name=name):
                self.assertIsNotNone(bench.CATALOG[name][1])

    def test_jq_tests_include_regex_and_the_original_check_target(self):
        subject = self.adapter('jq').JQ
        self.assertIn('--with-oniguruma=builtin', subject.configure)
        self.assertIn('make check', subject.test)
        self.assertFalse(subject.skip_objects)
        tree = '/work/jq/jq-1.8.2'
        units = self.adapter('jq').ladder.parse_build_log(
            'libtool: compile: gcc-14 -Isrc -c src/jv.c -o src/jv.o\n', tree)
        self.assertEqual([u.obj for u in units], ['src/jv.o'])

    def test_lua_uses_matching_official_tests_without_soft_skips(self):
        module = self.adapter('lua')
        self.assertIn('5.5.1-tests', module.TEST_URL)
        self.assertIn('all.lua', module.LUA.test)
        self.assertNotIn('_U=true', module.LUA.test)
        self.assertNotIn('_soft=true', module.LUA.test)
        self.assertIn('final OK', module.LUA.test)
        self.assertIn('libreadline.so.8', module.LUA.test)
        self.assertTrue(module.LUA.build.startswith('('))

    def test_sqlite_tests_consume_the_installed_fixture(self):
        subject = self.adapter('sqlite').SQLITE
        self.assertIn('sqlite-src-3530400', subject.url)
        self.assertIn('./testfixture test/testrunner.tcl --jobs 2 veryquick', subject.test)
        self.assertNotIn('make test', subject.test)
        self.assertIn('--relink', subject.relink)
        self.assertNotIn('#', subject.relink, 'a shell comment would swallow the status capture')
        self.assertIn('runuser -u nobody', subject.test)
        self.assertIn('--test-verdict /tmp/sqlite-test/testrunner.db', subject.test)
        self.assertIn('"$runner_status"', subject.test)

    def test_sqlite_split_preserves_all_sources_flags_and_relink_objects(self):
        helper = self.adapter('sqlite_fixture')
        argv = ['gcc-14', '-DSQLITE_TEST=1', '-DSQLITE_PRIVATE=', '-I../tcl', '-O2',
                '-o', 'testfixture', 'test/a.c', 'sqlite3.c', '-ltcl8.6', '-lm']
        builds, link, output = helper.commands(argv)
        self.assertEqual(len(builds), 2)
        self.assertEqual(output, 'testfixture')
        self.assertEqual(link[-4:], ['fixture-objects/000-a.o', 'fixture-objects/001-sqlite3.o', '-ltcl8.6', '-lm'])
        for build in builds:
            self.assertIn('-DSQLITE_TEST=1', build)
            self.assertIn('-DSQLITE_PRIVATE=', build)
            self.assertIn('-I../tcl', build)
            self.assertEqual(sum(a.endswith('.c') for a in build), 1)
            self.assertIn('-c', build)

    def test_sqlite_runner_zero_exit_does_not_override_failed_or_incomplete_jobs(self):
        helper = self.adapter('sqlite_fixture')
        for state, errors, tests, status, passed in [
            ('done', 0, 4, 0, True),
            ('failed', 1, 4, 0, False),
            ('failed', 0, 0, 0, False),
            ('running', 0, 0, 0, False),
            ('ready', 0, 0, 0, False),
            ('omit', 0, 0, 0, False),
            ('done', 1, 4, 0, False),
            ('done', 0, 0, 0, False),
            ('done', 0, 4, 1, False),
        ]:
            with self.subTest(state=state, errors=errors, tests=tests, status=status):
                with tempfile.TemporaryDirectory() as directory:
                    database = Path(directory) / 'testrunner.db'
                    with sqlite3.connect(database) as db:
                        db.execute('CREATE TABLE jobs(displayname, state, nerr, ntest, output)')
                        db.execute('INSERT INTO jobs VALUES(?,?,?,?,?)',
                                   ('test/original.test', state, errors, tests, 'original failure'))
                    result = helper.test_verdict(database, status)
                    self.assertEqual(result['passed'], passed)
                    self.assertEqual(result['tests'], tests)
                    self.assertEqual(result['errors'], errors)

    def test_sqlite_missing_result_database_is_not_a_pass(self):
        helper = self.adapter('sqlite_fixture')
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / 'missing.db'
            self.assertFalse(helper.test_verdict(database, 0)['passed'])
            self.assertFalse(database.exists(), 'validation must not invent a result database')

    def test_llvm_keeps_reference_output_checks_and_original_population(self):
        module = self.adapter('llvm-test-suite')
        self.assertEqual(module.COMMIT, '2919e55176a572ee085cb46b291c47b7092c363e')
        self.assertIn('-DTEST_SUITE_SUBDIRS=SingleSource', module.CONFIGURE)
        self.assertIn('-DTEST_SUITE_BENCHMARKING_ONLY=OFF', module.CONFIGURE)
        self.assertIn('-DTEST_SUITE_RUN_BENCHMARKS=ON', module.CONFIGURE)
        self.assertIn('HashProgramOutput.sh', module.LLVM.archive_members)

    def test_llvm_population_excludes_language_not_floating_point(self):
        helper = self.adapter('llvm_single_source')
        data = helper.inventory([
            {'output': 'SingleSource/UnitTests/CMakeFiles/integer.dir/int.c.o', 'file': '/source/int.c'},
            {'output': 'SingleSource/UnitTests/CMakeFiles/floating.dir/fp.c.o', 'file': '/source/fp.c'},
            {'output': 'SingleSource/UnitTests/CMakeFiles/cxx.dir/cxx.cpp.o', 'file': '/source/cxx.cpp'},
            {'output': 'tools/CMakeFiles/fpcmp.dir/fpcmp.c.o', 'file': '/source/tools/fpcmp.c'},
        ])
        self.assertEqual([r['target'] for r in data['selected']], ['floating', 'integer'])
        self.assertEqual([r['target'] for r in data['excluded']], ['cxx'])

    def test_llvm_build_shares_the_cmake_dependency_graph(self):
        helper = self.adapter('llvm_single_source')
        self.assertTrue(hasattr(helper, 'build_command'))
        data = helper.inventory([
            {'output': 'SingleSource/UnitTests/CMakeFiles/example.dir/example.c.o',
             'file': '/source/example.c'},
        ])
        argv = helper.build_command(data)
        self.assertIn('CMakeFiles/Makefile2', argv)
        self.assertIn('SingleSource/UnitTests/CMakeFiles/example.dir/all', argv)
        self.assertIn('-k', argv)

    def test_llvm_uses_the_pinned_lit_console_entry_point(self):
        helper = self.adapter('llvm_single_source')
        argv = helper.test_command({'selected': [{'test': 'build/original.test'}]})
        self.assertEqual(argv[1:3], ['-c', 'from lit.main import main; main()'])
        self.assertEqual(argv[-1], 'build/original.test')
        self.assertIn('--timeout=60', argv)

    def test_llvm_counts_independent_programs_without_reference_fallback_passes(self):
        module = self.adapter('llvm-test-suite')
        self.assertTrue(hasattr(module, 'program_outcome'))
        cases = [
            ('PASS', 'assembled', 'PASS', True, True, 'PASS'),
            ('PASS', 'refused', 'PASS', True, True, 'compiler-refused'),
            ('FAIL', 'assembled', 'PASS', True, True, 'oracle-unusable'),
            ('PASS', 'assembled', 'PASS', False, True, 'link-failure'),
            ('PASS', 'assembled', 'PASS', True, False, 'rebuilt'),
            ('PASS', 'assembled', 'FAIL', True, True, 'runtime-failure'),
            ('PASS', 'assembled', 'TIMEOUT', True, True, 'runtime-timeout'),
        ]
        for *args, expected in cases:
            self.assertEqual(module.program_outcome(*args), expected)

    def test_llvm_summary_counts_only_original_evaluated_programs(self):
        bench = load('additional_bench', ROOT / 'tools/bench.py')
        row = bench.metrics('llvm-test-suite', {
            'programsTotal': 4, 'unitsEvoccCompiled': 3,
            'programOutcomes': {'PASS': 1, 'compiler-refused': 1,
                                'runtime-failure': 1, 'oracle-unusable': 1},
            'nonCPrograms': [{'target': 'cxx', 'reason': 'non-C'}],
        })
        self.assertEqual((row['passed'], row['total'], row['kind']), (1, 4, 'programs'))
        self.assertEqual(row['status'], 'INCOMPLETE')
        self.assertEqual(row['baselineFailures'], 1)
        self.assertEqual(row['excluded'], 1)

    def test_unified_llvm_summary_preserves_the_five_way_account(self):
        bench = load('additional_bench', ROOT / 'tools/bench.py')
        data = {
            'programsTotal': 4, 'unitsEvoccCompiled': 2,
            'programOutcomes': {'PASS': 2, 'compiler-refused': 1, 'oracle-unusable': 1},
            'dispositionCounts': {'PASS': 1, 'compiler-failure': 0, 'FP-deferred': 1,
                                  'not-applicable': 1, 'oracle-unusable': 1},
        }
        row = bench.metrics('llvm-test-suite', data)
        self.assertEqual(row['passed'], 1)
        self.assertEqual(row['outcomes'], data['programOutcomes'])
        self.assertEqual(row['dispositions'], data['dispositionCounts'])
        self.assertEqual(row['status'], 'PROFILE_CLEAN')
        data['dispositionCounts']['PASS'] = 4
        with self.assertRaises(ValueError):
            bench.metrics('llvm-test-suite', data)

    def test_llvm_dispositions_keep_raw_outcomes_and_require_source_bound_evidence(self):
        module = self.adapter('llvm-test-suite')
        self.assertTrue(hasattr(module, 'program_disposition'))
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'example.c'
            source.write_text('int main(void){return 0;}\n')
            row = {'source': 'SingleSource/example.c', 'programOutcome': 'compiler-refused',
                   'reference': 'PASS', 'code': 'E3101',
                   'originalArguments': ['clang-19', 'example.c']}
            key = 'llvm-test-suite:SingleSource/example.c'
            rule = {'disposition': 'FP-deferred', 'sourceSha256': module.ladder.sha256_file(source),
                    'reason': 'source-specific reviewed requirement',
                    'deferredFeatures': ['binary16'],
                    'evidence': ['spec/evoir.md'],
                    'originalArguments': row['originalArguments']}
            self.assertEqual(module.program_disposition(row, source, {}, 'baseline.json')['disposition'],
                             'compiler-failure', 'E3101 alone is not floating-point evidence')
            result = module.program_disposition(row, source, {key: rule}, 'baseline.json')
            self.assertEqual(result['disposition'], 'FP-deferred')
            self.assertEqual(row['programOutcome'], 'compiler-refused')
            stale = {k: v for k, v in rule.items() if k != 'deferredFeatures'}
            self.assertEqual(module.program_disposition(row, source, {key: stale}, 'baseline.json')['disposition'],
                             'compiler-failure')
            for outcome, expected in [('PASS', 'PASS'), ('runtime-failure', 'compiler-failure'),
                                      ('link-failure', 'compiler-failure'),
                                      ('compiler-timeout', 'compiler-failure')]:
                result = module.program_disposition(
                    row | {'programOutcome': outcome}, source, {key: rule}, 'baseline.json')
                self.assertEqual(result['disposition'], expected)
            source.write_text('int main(void){return 1;}\n')
            self.assertEqual(module.program_disposition(row, source, {key: rule}, 'baseline.json')['disposition'],
                             'compiler-failure')
            missing = rule | {'sourceSha256': module.ladder.sha256_file(source),
                              'evidence': ['does-not-exist.txt']}
            self.assertEqual(module.program_disposition(row, source, {key: missing}, 'baseline.json')['disposition'],
                             'compiler-failure')
            self.assertEqual(module.program_disposition(row | {'programOutcome': 'PASS'}, source, {}, 'baseline.json')['disposition'],
                             'PASS')

    def test_llvm_reference_failure_is_unusable_not_subject_pass(self):
        module = self.adapter('llvm-test-suite')
        self.assertTrue(hasattr(module, 'program_disposition'))
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'example.c'
            source.write_text('int main(void){return 0;}\n')
            row = {'source': 'SingleSource/example.c', 'programOutcome': 'oracle-unusable',
                   'reference': 'TIMEOUT', 'native': 'PASS'}
            result = module.program_disposition(row, source, {}, 'capture/lit-baseline.json')
            self.assertEqual(result['disposition'], 'oracle-unusable')
            self.assertEqual(result['dispositionEvidence'], ['capture/lit-baseline.json'])
            self.assertIn('TIMEOUT', result['dispositionReason'])
            self.assertEqual(result['sourceSha256'], module.ladder.sha256_file(source))

    def test_llvm_disposition_cannot_survive_a_dialect_or_wrapping_change(self):
        module = self.adapter('llvm-test-suite')
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'example.c'
            source.write_text('int main(void){return 0;}\n')
            row = {'source': 'SingleSource/example.c', 'programOutcome': 'compiler-refused',
                   'reference': 'PASS', 'originalArguments': ['clang-19', '-std=gnu11', 'example.c']}
            key = 'llvm-test-suite:SingleSource/example.c'
            rule = {'disposition': 'not-applicable', 'sourceSha256': module.ladder.sha256_file(source),
                    'reason': 'measured original-mode boundary', 'evidence': ['spec/evoir.md'],
                    'originalArguments': row['originalArguments']}
            self.assertEqual(module.program_disposition(row, source, {key: rule}, 'baseline.json')['disposition'],
                             'not-applicable')
            for arguments in ([], ['clang-19', '-std=gnu89', 'example.c'],
                              ['clang-19', '-std=gnu11', '-fwrapv', 'example.c']):
                changed = row | {'originalArguments': arguments}
                result = module.program_disposition(changed, source, {key: rule}, 'baseline.json')
                self.assertEqual(result['disposition'], 'compiler-failure')
                self.assertIn('dispositionError', result)


if __name__ == '__main__':
    unittest.main()
