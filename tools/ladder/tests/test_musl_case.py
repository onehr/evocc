"""Explicit identities and output transport preserve upstream test verdicts."""
from pathlib import Path
import os
import signal
import subprocess
import sys
import tempfile
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

    def invoke_supervisor(self, body, case='src/regression/daemon-failure.exe'):
        with tempfile.TemporaryDirectory() as directory:
            supervisor = Path(directory) / 'reference-runtest'
            supervisor.write_text('#!' + sys.executable + '\n' + body)
            supervisor.chmod(0o755)
            return subprocess.run(
                [sys.executable, musl_case.__file__, str(supervisor),
                 '-w', musl_case.__file__, case], capture_output=True, timeout=10,
            )

    def test_daemon_supervisor_gets_local_file_and_exact_output_survives(self):
        for suffix in ('.exe', '-static.exe'):
            done = self.invoke_supervisor(
                'import os,stat,sys\n'
                'assert stat.S_ISREG(os.fstat(1).st_mode)\n'
                'assert sys.argv[1] == "-w" and len(sys.argv) == 4\n'
                'os.write(1, b"before\\x00\\xffafter\\n")\n',
                'src/regression/daemon-failure' + suffix,
            )
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertEqual(done.stdout, b'before\x00\xffafter\n')

    def test_supervisor_failure_and_diagnostic_are_not_hidden(self):
        done = self.invoke_supervisor(
            'import os,stat,sys\n'
            'assert stat.S_ISREG(os.fstat(1).st_mode)\n'
            'os.write(1,b"FAIL original [timed out]\\n")\n'
            'os.write(2,b"watchdog detail\\n")\n'
            'sys.exit(23)\n'
        )
        self.assertEqual(done.returncode, 23, done.stderr)
        self.assertEqual(done.stdout, b'FAIL original [timed out]\n')
        self.assertEqual(done.stderr, b'watchdog detail\n')

    @unittest.skipUnless(os.name == 'posix', 'signal return status is POSIX')
    def test_supervisor_signal_is_not_converted_into_success(self):
        done = self.invoke_supervisor(
            'import os,signal,stat\n'
            'assert stat.S_ISREG(os.fstat(1).st_mode)\n'
            'os.write(1,b"interrupted\\n")\n'
            'os.kill(os.getpid(), signal.SIGTERM)\n'
        )
        self.assertEqual(done.returncode, -signal.SIGTERM, done.stderr)
        self.assertEqual(done.stdout, b'interrupted\n')

    def test_other_supervised_cases_keep_the_original_stdout(self):
        for case in ('src/regression/other.exe', 'elsewhere/daemon-failure.exe'):
            done = self.invoke_supervisor(
                'import os,stat\nassert stat.S_ISFIFO(os.fstat(1).st_mode)\n', case,
            )
            self.assertEqual(done.returncode, 0, done.stderr)

    def test_spool_setup_failure_does_not_execute_or_claim_success(self):
        argv = ['reference-runtest', '-w', 'wrapper',
                'src/regression/daemon-failure.exe']
        with patch.object(musl_case.tempfile, 'TemporaryFile',
                          side_effect=OSError('no guest storage')), \
                patch.object(musl_case.subprocess, 'call') as child:
            with self.assertRaises(OSError):
                musl_case.run_case(argv)
            child.assert_not_called()

    def test_only_exact_supervisor_shapes_are_spooled(self):
        good = ['src/common/runtest.exe', '-w', 'wrapper',
                'src/regression/daemon-failure-static.exe']
        self.assertTrue(musl_case.needs_local_stdout(good))
        for argv in ([good[3]], ['different-runner', *good[1:]],
                     [good[0], '-t', '20', good[3]], [*good, 'extra'],
                     [good[0], '-w', 'wrapper', 'elsewhere/daemon-failure.exe']):
            self.assertFalse(musl_case.needs_local_stdout(argv), argv)


if __name__ == '__main__':
    unittest.main()
