"""Bounded Linux guest execution using the existing capture/resource collector.

The timed interval is the child spawn to stdio drain inside the VM. Container
RPC and this Python process's startup are outside it, never native CPU claims.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import resource
import sys

from .eqsat_chain import digest, require
from .eqsat_native import Capture


def run(directory, argv, timeout, output_bytes, reserve):
    require(platform.system() == 'Linux' and platform.machine() == 'aarch64', 'wrong guest target')
    directory = Path(directory).resolve()
    directory.mkdir()  # no reusing a failed or stale measurement directory
    executable = Path(argv[0]).resolve(strict=True)
    require(executable.is_file(), 'missing measured executable')
    identity = digest(executable.read_bytes())
    receipt = dict(schema='evocc-linux-guest-execution-v1', status='STARTED', attempts=[],
                   executable_digest=identity, runtime='Linux-aarch64-hardware-VM',
                   kernel=platform.release(), environment=dict(PATH='/usr/bin:/bin', LANG='C', LC_ALL='C'))

    def save():
        pending = directory/'receipt.json.pending'
        pending.write_text(json.dumps(receipt, sort_keys=True, indent=2)+'\n')
        pending.replace(directory/'receipt.json')

    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    runner = Capture(directory, dict(timeout_seconds=timeout, output_bytes=output_bytes,
        min_free_bytes=reserve, max_retained_bytes=output_bytes+65536),
        {**receipt['environment'], 'TMPDIR': '/tmp'}, receipt, save, measure_resources=True)
    code = 1
    try:
        row = runner.run('guest-child', argv)
        require(digest(executable.read_bytes()) == identity, 'executable changed during execution')
        receipt['status'] = 'OBSERVED'
        code = row['exit_code']
    except (OSError, ValueError) as error:
        receipt.update(status='FAILED', error=str(error))
    finally:
        save()
        for name, stream in (('stdout', sys.stdout.buffer), ('stderr', sys.stderr.buffer)):
            path = directory / ('000.'+name)
            if path.exists():
                stream.write(path.read_bytes())
    return code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', required=True)
    parser.add_argument('--timeout', type=int, required=True)
    parser.add_argument('--output-bytes', type=int, required=True)
    parser.add_argument('--reserve', type=int, required=True)
    parser.add_argument('argv', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    require(1 <= args.timeout <= 600 and 1024 <= args.output_bytes <= 16*1024**2
            and args.reserve >= 2*1024**3 and args.argv and args.argv[0] == '--', 'unbounded guest invocation')
    sys.exit(run(args.directory, args.argv[1:], args.timeout, args.output_bytes, args.reserve))


if __name__ == '__main__':
    main()
