"""PolyBench inventories kernels, and compares observable arrays, not timers."""
import importlib.util
from pathlib import Path
import tempfile
import unittest


class PolybenchTests(unittest.TestCase):
    def adapter(self):
        path = Path(__file__).resolve().parents[1] / 'polybench.py'
        self.assertTrue(path.is_file(), 'the roadmap PolyBench adapter is missing')
        spec = importlib.util.spec_from_file_location('polybench_adapter_test', path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_inventory_uses_the_upstream_list_and_keeps_float_sources(self):
        module = self.adapter()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'utilities').mkdir()
            (root / 'utilities/benchmark_list').write_text('./integer/i.c\n./floating/f.c\n')
            for path, text in [('integer/i.c', 'int main(void){return 0;}'),
                               ('floating/f.c', 'double x; int main(void){return x;}')]:
                file = root / path
                file.parent.mkdir()
                file.write_text(text)
            self.assertEqual(module.inventory(root), ['integer/i.c', 'floating/f.c'])
            for bad in ['./integer/i.c\n./integer/i.c\n', '../outside.c\n', '/absolute.c\n']:
                (root / 'utilities/benchmark_list').write_text(bad)
                with self.assertRaises(ValueError):
                    module.inventory(root)

    def test_only_completed_matching_reference_and_subject_dumps_pass(self):
        module = self.adapter()
        good = {'exit': 0, 'stdoutSha256': 'out', 'stderrSha256': 'arrays', 'hasArrayDump': True}
        for compilation, gcc, clang, subject, expected in [
            ('assembled', good, good, good, 'PASS'),
            ('refused', good, good, None, 'compiler-refused'),
            ('assembled', good, good, None, 'link-failure'),
            ('assembled', good, good, good | {'exit': 137}, 'runtime-timeout'),
            ('assembled', good, good, good | {'stderrSha256': 'wrong'}, 'runtime-failure'),
            ('assembled', good, good | {'stderrSha256': 'different'}, good, 'oracle-unusable'),
            ('assembled', good | {'exit': 1}, good, good, 'oracle-unusable'),
            ('assembled', good | {'hasArrayDump': False}, good | {'hasArrayDump': False}, good, 'oracle-unusable'),
        ]:
            self.assertEqual(module.program_outcome(compilation, gcc, clang, subject), expected)

    def test_configuration_preserves_types_and_uses_upstream_array_dump(self):
        module = self.adapter()
        self.assertIn('-DPOLYBENCH_DUMP_ARRAYS', module.FLAGS)
        self.assertIn('-std=gnu99', module.FLAGS)
        self.assertIn('-DMINI_DATASET', module.FLAGS)
        self.assertNotIn('-DDATA_TYPE_IS_INT', module.FLAGS)
        self.assertNotIn('-DPOLYBENCH_TIME', module.FLAGS)
        self.assertEqual(module.POLYBENCH.sha256,
                         '426519ee8443a5f2175de6a3e9328cda8917a5e33053d0e8f59855ab56d689a4')

    def test_dump_requires_status_and_both_original_markers(self):
        module = self.adapter()
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory) / 'kernel'
            prefix.with_suffix('.stdout').write_bytes(b'')
            prefix.with_suffix('.stderr').write_bytes(b'==BEGIN DUMP_ARRAYS==\ndata\n')
            self.assertIsNone(module.read_run(prefix))
            prefix.with_suffix('.status').write_text('0\n')
            self.assertFalse(module.read_run(prefix)['hasArrayDump'])
            prefix.with_suffix('.stderr').write_bytes(
                b'==BEGIN DUMP_ARRAYS==\ndata\n==END   DUMP_ARRAYS==\n')
            self.assertTrue(module.read_run(prefix)['hasArrayDump'])
            prefix.with_suffix('.status').write_text('137\n')
            self.assertEqual(module.read_run(prefix)['exit'], 137)

    def test_reference_and_subject_use_the_same_noncontracting_fp_profile(self):
        module = self.adapter()
        self.assertIn('-ffp-contract=off', module.FLAGS)
        unit = module.ladder.Unit('mvt.o', 'mvt.c', '',
                                  ('gcc-14', *module.FLAGS, '-c', 'mvt.c'))
        command, omitted = module.ladder.evocc_argv(
            unit, Path('/compiler'), module.POLYBENCH, Path('/source'), [])
        self.assertIn('-ffp-contract=off', command)
        self.assertNotIn('-ffp-contract=off', omitted)
        source = ['linear-algebra/kernels/mvt/mvt.c']
        for subject in [False, True]:
            script = module.native_script(source, 'fp-profile', subject=subject)
            commands = [line for line in script.splitlines()
                        if line.startswith(('gcc-14 ', 'clang-19 '))]
            self.assertTrue(commands)
            self.assertTrue(all(' -ffp-contract=off ' in line for line in commands))

    def test_subject_links_only_the_emitted_object_with_reference_utility(self):
        module = self.adapter()
        script = module.native_script(['medley/nussinov/nussinov.c'], 'test', subject=True)
        self.assertIn('/evocc-objects/obj/nussinov.o', script)
        self.assertIn('/native/test/gcc/utility.o', script)
        self.assertNotIn('nussinov.c ', script)
        self.assertIn('if [ "$status" -eq 0 ]', script)
        self.assertIn('timeout -s KILL 60', script)

    def test_unified_entrypoint_counts_original_programs_not_utility_objects(self):
        path = Path(__file__).resolve().parents[2] / 'bench.py'
        spec = importlib.util.spec_from_file_location('polybench_entrypoint_test', path)
        bench = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(bench)
        self.assertIsNotNone(bench.CATALOG['polybench'][1])
        row = bench.metrics('polybench', {
            'programsTotal': 30, 'unitsEvoccCompiled': 2,
            'programOutcomes': {'PASS': 2, 'compiler-refused': 28},
            'runtimeSupport': {'role': 'reference-built upstream allocation utility'},
        })
        self.assertEqual((row['passed'], row['total'], row['kind']), (2, 30, 'programs'))
        self.assertEqual(row['status'], 'INCOMPLETE')
        self.assertIn('reference-built', row['runtimeSupport']['role'])


if __name__ == '__main__':
    unittest.main()
