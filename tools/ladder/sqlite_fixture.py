#!/usr/bin/env python3
"""Split SQLite's upstream compile-and-link recipe into replaceable objects.

The arguments still come from main.mk's T.link.tcl and TESTFIXTURE_SRC.
No source/define is changed. Relinking reads the saved original command and
uses the installed objects, never recompiles the amalgamation behind us.
"""
import json
from pathlib import Path
import shlex
import sqlite3
import subprocess
import sys


def commands(argv):
    sources = [a for a in argv[1:] if a.endswith('.c')]
    if not sources or '-o' not in argv:
        raise ValueError('expected the upstream fixture compile-and-link command')
    output = argv[argv.index('-o') + 1]
    objects = {src: f'fixture-objects/{index:03d}-{Path(src).stem}.o'
               for index, src in enumerate(sources)}
    common = []
    skip = False
    for arg in argv:
        if skip:
            skip = False
        elif arg == '-o':
            skip = True
        elif arg not in sources:
            common.append(arg)
    builds = [common + ['-c', source, '-o', obj] for source, obj in objects.items()]
    link = [objects.get(arg, arg) for arg in argv]
    return builds, link, output


def execute(argv):
    print(shlex.join(argv), flush=True)
    return subprocess.call(argv)


def test_verdict(database, status):
    """testrunner.tcl can exit zero after reporting failed jobs; inspect its ledger."""
    try:
        with sqlite3.connect(Path(database).resolve().as_uri() + '?mode=ro', uri=True) as db:
            rows = db.execute('SELECT displayname, state, nerr, ntest, output FROM jobs').fetchall()
    except sqlite3.Error as error:
        return {'passed': False, 'status': status, 'error': str(error)}
    errors = sum(row[2] or 0 for row in rows)
    tests = sum(row[3] or 0 for row in rows)
    failed = [dict(zip(('name', 'state', 'errors', 'tests', 'output'), row))
              for row in rows if row[1] != 'done' or (row[2] or 0) != 0]
    return {'passed': status == 0 and bool(rows) and tests > 0 and not failed,
            'status': status, 'jobs': len(rows), 'errors': errors, 'tests': tests,
            'failedJobs': failed}


def main(argv):
    if len(argv) == 3 and argv[0] == '--test-verdict':
        result = test_verdict(argv[1], int(argv[2]))
        print(json.dumps(result, sort_keys=True), flush=True)
        return 0 if result['passed'] else 1
    manifest = Path('fixture-objects/link.json')
    if argv == ['--relink']:
        return execute(json.loads(manifest.read_text())['link'])
    builds, link, output = commands(argv)
    manifest.parent.mkdir(exist_ok=True)
    for command in builds:
        if status := execute(command):
            return status
    manifest.write_text(json.dumps({'compile': builds, 'link': link, 'output': output}))
    return execute(link)


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
