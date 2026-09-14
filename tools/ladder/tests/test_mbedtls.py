"""The TLS milestone must execute upstream tests against installed objects."""

import importlib.util
from pathlib import Path
import sys
import unittest


ROOT = Path(__file__).resolve().parents[3]


class MbedTlsAdapterTest(unittest.TestCase):
    def adapter(self):
        path = ROOT / 'tools/ladder/mbedtls.py'
        self.assertTrue(path.is_file(), 'the pinned TLS benchmark has no adapter')
        spec = importlib.util.spec_from_file_location('ladder_mbedtls', path)
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        return module

    def test_release_includes_submodules_and_generated_sources(self):
        subject = self.adapter().MBEDTLS
        self.assertEqual(subject.tree, 'mbedtls-4.2.0')
        self.assertEqual(subject.sha256, '2bed9d713b4668f76553b097e72b8aa30bc8f112a940d7ae228d524bbde6ffea')
        self.assertIn('/releases/download/mbedtls-4.2.0/', subject.url)

    def test_original_tests_run_after_relink_and_empty_ctest_is_an_error(self):
        subject = self.adapter().MBEDTLS
        self.assertIn('-DENABLE_TESTING=ON', subject.configure)
        self.assertIn('--verbose', subject.build)
        self.assertIn('--build build', subject.relink)
        self.assertIn('build/programs/test/selftest', subject.test)
        self.assertIn('--no-tests=error', subject.test)
        self.assertIn('--output-on-failure', subject.test)
        self.assertIn('&&', subject.test)
        self.assertNotIn('--build', subject.test)
        self.assertTrue(subject.test.startswith('('))
        self.assertTrue(subject.test.endswith(')'))

    def test_cmake_commands_keep_library_and_generated_test_objects(self):
        module = self.adapter()
        tree = '/work/mbedtls/mbedtls-4.2.0'
        log = (
            f'cd {tree}/build/library && /usr/bin/gcc-14 -I{tree}/include '
            f'-o CMakeFiles/mbedtls.dir/ssl_tls.c.o -c {tree}/library/ssl_tls.c\n'
            f'cd {tree}/build/tests && /usr/bin/gcc-14 -I{tree}/include '
            f'-o CMakeFiles/test_suite_ssl.dir/test_suite_ssl.c.o -c {tree}/build/tests/test_suite_ssl.c\n'
        )
        units = module.ladder.parse_build_log(log, tree)
        self.assertEqual([u.obj for u in units], [
            'build/library/CMakeFiles/mbedtls.dir/ssl_tls.c.o',
            'build/tests/CMakeFiles/test_suite_ssl.dir/test_suite_ssl.c.o',
        ])


if __name__ == '__main__':
    unittest.main()
