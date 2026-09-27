#!/usr/bin/env python3
"""Opt-in numerical dependency repair supplement, NOT the frozen musl rung.

Reuse the sealed ladder executor, installed archives, original test objects and
watchdog. Only two copied source files are patched. Their replacement objects
are explicit link inputs; original archives, source and verdicts never change.
No successful supplement result can upgrade the original benchmark receipt.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCES = {
    'src/internal/floatscan.c': '13a007ee0be8682efa12c80e088a1758a425d8be1842980609f20c0f9d6d9ad5',
    'src/math/powf.c': '74069b3a380cf954361e0243b38de2094ae680e2ecfb3e50af61ab576f13babf',
}
PRIVATE = ('arch/aarch64', 'arch/generic', 'obj/src/internal', 'src/include',
           'src/internal', 'obj/include', 'include', 'src/math')
FLAGS = ('-std=c99', '-nostdinc', '-ffreestanding', '-fexcess-precision=standard',
         '-frounding-math', '-fno-strict-aliasing', '-D_XOPEN_SOURCE=700',
         '-fno-unwind-tables', '-fno-asynchronous-unwind-tables',
         '-ffunction-sections', '-fdata-sections', '-fPIC')
SUBJECTS = tuple('functional/' + name + suffix for name in (
    'strtold', 'strtof', 'strtod', 'strtod_long', 'strtod_simple', 'sscanf', 'sscanf_long'
) for suffix in ('', '-static')) + ('math/powf',)
FIXTURE = 'musl-patched-numerics.c'


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def execute(argv: list[str], timeout: int = 90) -> dict:
    try:
        p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
        return dict(argv=argv, exit=p.returncode, stdout=p.stdout, stderr=p.stderr)
    except subprocess.TimeoutExpired as error:
        return dict(argv=argv, exit=None, timeout=True,
                    stdout=str(error.stdout), stderr=str(error.stderr))


def passed(data: dict) -> bool:
    """Exact population and successful builds/runs, not a stdout PASS marker."""
    expected = {(cc, opt, lib, name) for cc in ('gcc-14', 'clang-19', 'evocc')
                for opt in ('0', '2') for lib in ('reference', 'mixed')
                for name in ('probe', *SUBJECTS)}
    if data.get('patched') is not True:
        return False
    seen = set()
    for row in data.get('rows', []):
        key = tuple(row.get(x) for x in ('compiler', 'opt', 'library', 'subject'))
        if key not in expected or key in seen:
            return False
        seen.add(key)
        if any(row.get(stage, {}).get('exit') != 0 for stage in ('build', 'link', 'run')):
            return False
        if not row.get('binarySha256') or not row.get('objectSha256'):
            return False
        if row.get('replacementObjects', {}).keys() != {'floatscan', 'powf'} or not row.get('linkMapVerified'):
            return False
        if key[-1] == 'probe' and row['run'].get('stdout') != 'checks=5920 failures=0\n':
            return False
    return seen == expected and data.get('inputsUnchanged') is True


def audit_map(text: str, objects: list[Path]) -> bool:
    """Both explicit definitions must be present, neither old archive member."""
    return all(f'LOAD {obj}\n' in text and
               f'({obj.stem}.lo)' not in text and f'({obj.stem}.o)' not in text
               for obj in objects)


def inside(output: Path) -> int:
    plan = json.loads((output / 'plan.json').read_text())
    rows = []
    sources = output / 'source'
    includes = ['-I/work/musl/' + p for p in PRIVATE]
    libraries = {'reference': Path('/work/reference/libc-reference.a'),
                 'mixed': Path('/work/musl/lib/libc.a')}
    identities = {str(p): digest(p) for p in libraries.values()}
    data = {'rows': rows, 'libraryHashes': identities, 'patched': plan['patched']}
    for opt in ('0', '2'):
        for cc in ('gcc-14', 'clang-19', 'evocc'):
            directory = output / (cc + '-' + opt)
            directory.mkdir()
            objects, builds = [], []
            for source in (*SOURCES, FIXTURE):
                name = Path(source).stem
                obj = directory / (name + '.o')
                path = sources / source if source in SOURCES else Path('/work/fixtures') / source
                argv = (['gcc-14', '-c', str(output / f'{name}-{opt}.s'), '-o', str(obj)]
                        if cc == 'evocc' else
                        [cc, *FLAGS, '-O' + opt, *includes, '-c', str(path), '-o', str(obj)])
                built = execute(argv)
                builds.append(built)
                objects.append(obj)
            (directory / 'builds.json').write_text(json.dumps(builds, indent=2) + '\n')
            if any(b['exit'] != 0 for b in builds):
                data['buildFailure'] = builds
                (output / 'executions.json').write_text(json.dumps(data, indent=2) + '\n')
                return 1
            replacements = {p.stem: digest(p) for p in objects[:-1]}
            for library, archive in libraries.items():
                for subject in ('probe', *SUBJECTS):
                    obj = (objects[-1] if subject == 'probe' else
                           Path('/work/tests/src') / (subject.removesuffix('-static') + '.o'))
                    exe = directory / (library + '-' + subject.replace('/', '-'))
                    argv = ['gcc-14', '-nostdlib', '-static', '/work/pristine/obj/crt/crt1.o',
                            '/work/musl/lib/crti.o', str(obj)]
                    if subject != 'probe':
                        argv.append('/work/tests/src/common/libtest.a')
                    if plan['patched']:
                        argv.extend(str(p) for p in objects[:-1])
                    argv += ['-Wl,--start-group', str(archive), '-lgcc', '-lgcc_eh',
                             '-Wl,--end-group', '/work/musl/lib/crtn.o', '-o', str(exe),
                             '-Wl,-Map=' + str(exe) + '.map']
                    row = dict(compiler=cc, opt=opt, library=library, subject=subject,
                               objectSha256=digest(obj), build={'exit': 0}, link=execute(argv))
                    if row['link']['exit'] == 0:
                        row['replacementObjects'] = replacements if plan['patched'] else {}
                        row['linkMapVerified'] = audit_map(Path(str(exe) + '.map').read_text(), objects[:-1])
                        row['binarySha256'] = digest(exe)
                        row['run'] = execute(['/work/tests/reference-runtest', str(exe)], 10)
                    rows.append(row)
                    (output / 'executions.json').write_text(json.dumps(data, indent=2) + '\n')
                    print(cc, opt, library, subject, row.get('run', row['link'])['exit'], flush=True)
            if any(digest(p) != replacements[p.stem] for p in objects[:-1]):
                raise ValueError('replacement object changed during execution')
    data['inputsUnchanged'] = all(digest(Path(p)) == value for p, value in identities.items())
    data['status'] = 'PASS' if passed(data) else 'FAIL'
    (output / 'executions.json').write_text(json.dumps(data, indent=2) + '\n')
    return 0 if data['inputsUnchanged'] else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inside', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--evocc', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--unpatched-control', action='store_true')
    args = parser.parse_args()
    if args.inside:
        return inside(args.inside)
    if not args.evocc or not args.output:
        parser.error('--evocc and --output are required')
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import mixed
    tree = ROOT / '.build/ladder/musl-work'
    tests = ROOT / '.build/ladder/libc-test-work'
    reference = ROOT / '.build/floating-targeted/strict-20260927/musl-thread-ssa'
    patch = ROOT / 'tests/fixtures/musl-patched-numerics.patch'
    compiler = args.evocc.resolve()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    paths = [compiler, Path(__file__).resolve(), patch, ROOT / 'tests/fixtures' / FIXTURE,
             tree / 'lib/libc.a', reference / 'libc-reference.a', tests / 'reference-runtest',
             tests / 'src/common/libtest.a', *(tree / name for name in SOURCES),
             *(tests / 'src' / (name.removesuffix('-static') + '.o') for name in SUBJECTS)]
    identity = {str(p): digest(p) for p in paths}
    for name, wanted in SOURCES.items():
        if digest(tree / name) != wanted:
            raise ValueError('unexpected dependency source: ' + name)
        target = out / 'source' / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(tree / name, target)
    # patch(1) operates only on the two copied files, not the installed tree.
    applied = execute(['patch', '--batch', '-p1', '-d', str(out / 'source'), '-i', str(patch)])
    (out / 'patch.json').write_text(json.dumps(applied, indent=2) + '\n')
    if applied['exit'] != 0:
        raise ValueError('dependency patch failed')
    plan = dict(configuration='musl-1.2.6-numerical-repair-supplement',
                patched=not args.unpatched_control, image=mixed.IMAGE,
                originalBenchmarkUnchanged=True, identity=identity,
                patchedSources={name: digest(out / 'source' / name) for name in SOURCES},
                scope='two replacement library objects; 15 original subjects and focused matrix',
                objectMode='explicit link inputs before original archive; no installed replacement')
    (out / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    emissions = []
    for opt in ('0', '2'):
        for source in (*SOURCES, FIXTURE):
            path = out / 'source' / source if source in SOURCES else ROOT / 'tests/fixtures' / source
            argv = [str(compiler), '--target=aarch64-unknown-linux-gnu', '--emit=asm',
                    *FLAGS, '-O' + opt, *['-I' + str(tree / p) for p in PRIVATE], str(path)]
            row = execute(argv)
            (out / (Path(source).stem + '-' + opt + '.s')).write_text(row.pop('stdout'))
            emissions.append(row)
            (out / 'emissions.json').write_text(json.dumps(emissions, indent=2) + '\n')
            if row['exit'] != 0:
                print(row['stderr'], file=sys.stderr)
                return 1
    mounts = {'out': out, 'musl': tree, 'tests': tests, 'reference': reference,
              'pristine': ROOT / '.build/ladder/musl-pristine',
              'fixtures': ROOT / 'tests/fixtures', 'tools': ROOT / 'tools/ladder'}
    result = mixed.container(mounts, 'python3 /work/tools/musl_patched.py --inside /work/out', timeout=600)
    (out / 'container.json').write_text(json.dumps(dict(exit=result.returncode,
        stdout=result.stdout, stderr=result.stderr), indent=2) + '\n')
    print(result.stdout, result.stderr)
    if any(digest(Path(p)) != value for p, value in identity.items()):
        raise ValueError('original input changed during supplement')
    if result.returncode != 0:
        return 1
    data = json.loads((out / 'executions.json').read_text())
    print('Patched supplement:', data['status'], '(never updates the original rung)')
    return 0 if passed(data) else 1


if __name__ == '__main__':
    raise SystemExit(main())
