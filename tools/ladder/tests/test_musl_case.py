"""The fork-limit regression must execute outside root's limit exemption."""
from pathlib import Path
import sys
import unittest
from unittest.mock import patch, call

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import musl_case  # noqa: E402


class CaseEnvironment(unittest.TestCase):
    def test_only_exact_regression_paths_drop_privilege(self):
        for suffix in ('.exe', '-static.exe'):
            argv = ['src/regression/pthread_atfork-errno-clobber' + suffix]
            with patch.object(musl_case, 'os') as os:
                os.geteuid.return_value = 0
                musl_case.run_case(argv)
                self.assertEqual(os.mock_calls, [call.geteuid(), call.setgroups([]),
                                                call.setgid(65534), call.setuid(65534),
                                                call.execv(argv[0], argv)])

    def test_other_cases_keep_their_environment_and_arguments(self):
        for name in ('src/regression/daemon-failure.exe',
                     'elsewhere/pthread_atfork-errno-clobber.exe',
                     'src/common/runtest.exe'):
            argv = [name, 'literal argument']
            with patch.object(musl_case, 'os') as os:
                musl_case.run_case(argv)
                self.assertEqual(os.mock_calls, [call.execv(name, argv)])

    def test_already_unprivileged_identity_is_not_replaced(self):
        argv = ['/work/libc-test/src/regression/pthread_atfork-errno-clobber.exe']
        with patch.object(musl_case, 'os') as os:
            os.geteuid.return_value = 1000
            musl_case.run_case(argv)
            self.assertEqual(os.mock_calls, [call.geteuid(), call.execv(argv[0], argv)])

    def test_failed_privilege_drop_never_executes_as_root(self):
        argv = ['src/regression/pthread_atfork-errno-clobber.exe']
        with patch.object(musl_case, 'os') as os:
            os.geteuid.return_value = 0
            os.setuid.side_effect = PermissionError('denied')
            with self.assertRaises(PermissionError):
                musl_case.run_case(argv)
            os.execv.assert_not_called()


if __name__ == '__main__':
    unittest.main()
