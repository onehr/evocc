#!/usr/bin/env python3
"""Explicit libc-test execution prerequisites, shared by both compiler lanes.

RLIMIT_NPROC does not constrain root. The atfork errno regression deliberately
sets that limit to zero to induce fork failure; run that case unprivileged.
No test source, assertion, timeout, or failure verdict is changed here.

The daemon descriptor-exhaustion cases must not duplicate a host virtiofs log
descriptor a million times: exit then waits on filesystem requests rather than
the daemon behavior under test. Spool only those supervisors' stdout to a
guest-local regular file, outside the unchanged watchdog, and copy every byte
and preserve its status afterward. Inner case invocation still uses exec.
"""
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile

UNPRIVILEGED_CASES = {
    ('src', 'regression', 'pthread_atfork-errno-clobber.exe'),
    ('src', 'regression', 'pthread_atfork-errno-clobber-static.exe'),
}

DESCRIPTOR_CASES = {
    ('src', 'regression', 'daemon-failure.exe'),
    ('src', 'regression', 'daemon-failure-static.exe'),
}


def needs_local_stdout(argv: list[str]) -> bool:
    return (
        len(argv) == 4 and argv[1] == '-w'
        and Path(argv[3]).parts[-3:] in DESCRIPTOR_CASES
        and (Path(argv[0]).name == 'reference-runtest'
             or Path(argv[0]).parts[-3:] == ('src', 'common', 'runtest.exe'))
    )


def run_supervisor(argv: list[str]) -> None:
    # /tmp is guest-local in the sealed lane; do not inherit TMPDIR pointing
    # into a host mount. A regular file preserves the original output type.
    with tempfile.TemporaryFile(dir='/tmp') as output:
        status = subprocess.call(argv, stdout=output)
        output.seek(0)
        shutil.copyfileobj(output, sys.stdout.buffer)
        sys.stdout.buffer.flush()
    if status < 0:
        signum = -status
        if signum not in (signal.SIGKILL, signal.SIGSTOP):
            signal.signal(signum, signal.SIG_DFL)
        signal.pthread_sigmask(signal.SIG_UNBLOCK, {signum})
        os.kill(os.getpid(), signum)
        raise SystemExit(128 + signum)  # fail closed if delivery is intercepted
    raise SystemExit(status)


def run_case(argv: list[str]) -> None:
    if not argv:
        raise ValueError('missing libc-test executable')
    if needs_local_stdout(argv):
        run_supervisor(argv)
    if Path(argv[0]).parts[-3:] in UNPRIVILEGED_CASES and os.geteuid() == 0:
        os.setgroups([])
        os.setgid(65534)
        os.setuid(65534)
    os.execv(argv[0], argv)


if __name__ == '__main__':
    try:
        run_case(sys.argv[1:])
    except (OSError, ValueError) as error:
        print(f'libc-test environment: {error}', file=sys.stderr)
        sys.exit(125)
