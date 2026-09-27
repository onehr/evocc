#!/usr/bin/env python3
"""Explicit libc-test execution prerequisites, shared by both compiler lanes.

RLIMIT_NPROC does not constrain root. The atfork errno regression deliberately
sets that limit to zero to induce fork failure; run that case unprivileged.
No test source, assertion, timeout, or failure verdict is changed here.
"""
import os
from pathlib import Path
import sys

UNPRIVILEGED_CASES = {
    ('src', 'regression', 'pthread_atfork-errno-clobber.exe'),
    ('src', 'regression', 'pthread_atfork-errno-clobber-static.exe'),
}


def run_case(argv: list[str]) -> None:
    if not argv:
        raise ValueError('missing libc-test executable')
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
