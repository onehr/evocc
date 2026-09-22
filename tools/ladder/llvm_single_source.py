#!/usr/bin/env python3
"""Select C programs from the upstream CMake inventory, then use CMake/lit.

Language, not compiler success or presence of FP, defines this C lane. The
complete configured population and every non-C exclusion remain in JSON.
"""
import collections
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys


def inventory(commands):
    programs = collections.defaultdict(list)
    for entry in commands:
        match = re.fullmatch(r'(SingleSource/.+)/CMakeFiles/([^/]+)\.dir/.+', entry['output'])
        if match:
            programs[(match[1], match[2])].append(entry)
    selected, excluded = [], []
    for (directory, target), entries in sorted(programs.items()):
        row = {'target': target, 'test': f'build/{directory}/{target}.test',
               'sources': [entry['file'] for entry in entries],
               'objects': [entry['output'] for entry in entries]}
        if all(Path(entry['file']).suffix == '.c' for entry in entries):
            selected.append(row)
        else:
            excluded.append(row | {'reason': 'non-C program; EvoCC is a C compiler'})
    if not selected:
        raise ValueError('CMake inventory contains no SingleSource C programs')
    return {'selected': selected, 'excluded': excluded}


def execute(argv):
    shown = shlex.join(argv if len(argv) <= 20 else argv[:9])
    print(shown + (f' ... ({len(argv)} arguments)' if len(argv) > 20 else ''), flush=True)
    return subprocess.call(argv)


def build_command(data):
    # CMake's outer Makefile invokes an entire recursive build per goal.
    # Its generated dependency graph has the same recipes and shares support
    # targets across all C programs, instead of rechecking 1,735 times.
    support = ['litsupport/CMakeFiles/build-litsupport.dir/all',
               'tools/CMakeFiles/build-HashProgramOutput.sh.dir/all',
               'tools/CMakeFiles/fpcmp-target.dir/all',
               'tools/CMakeFiles/timeit-target.dir/all', 'tools/CMakeFiles/not.dir/all']
    return ['gmake', '-C', 'build', '-f', 'CMakeFiles/Makefile2', '-j2', '-k', 'VERBOSE=1',
            *support, *[row['objects'][0].split('.dir/')[0] + '.dir/all'
                        for row in data['selected']]]


def test_command(data):
    # The pinned wheel's console entry point is lit.main:main; it has no
    # lit.__main__ module. Use that entry point without installing globally.
    return [sys.executable, '-c', 'from lit.main import main; main()',
            '-v', '-j2', '--timeout=60',
            '-o', '/work/llvm-test-suite/lit-results.json',
            *[row['test'] for row in data['selected']]]


def main(argv):
    destination = Path('c-programs.json')
    if argv == ['prepare']:
        destination.write_text(json.dumps(inventory(json.loads(
            Path('build/compile_commands.json').read_text())), indent=1))
        return 0
    data = json.loads(destination.read_text())
    if argv[0] == 'build':
        return execute(build_command(data))
    if argv == ['test']:
        return execute(test_command(data))
    raise ValueError('expected prepare, build CMAKE, or test')


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
