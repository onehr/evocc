"""Run one fixed generated Lean template with the frozen project search path."""
import os
from pathlib import Path
import subprocess
import sys


def main():
    lean, project, source = map(Path, sys.argv[1:])
    # The caller pins the actual Lean executable and all project source files.
    # No user/AI tactic script or shell command is accepted by the collector.
    env = dict(os.environ, LEAN_PATH=str(project/'.lake/build/lib/lean'), LEAN_NUM_THREADS='1')
    return subprocess.call([str(lean), str(source)], cwd=project, env=env)


if __name__ == '__main__':
    sys.exit(main())
