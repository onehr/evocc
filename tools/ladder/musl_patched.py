#!/usr/bin/env python3
"""Opt-in dependency repair supplements, NOT the frozen musl rung.

Reuse the sealed ladder executor, installed archives, original test objects and
watchdog. Only the selected copied source files are patched. Replacement objects
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
SUITES = {
    'numerics': dict(sources=SOURCES, subjects=SUBJECTS, fixture=FIXTURE, checks=5920),
    'time': dict(sources={'src/time/strptime.c':
        '9d3eb656b897a82c3b43a6ef973239a014ee9ee89361c1b308b0a2429a4766ff'},
        subjects=('functional/strptime', 'functional/strptime-static'),
        fixture='musl-patched-time.c', checks=45),
    'wordexp': dict(sources={'src/misc/wordexp.c':
        '018c97c999cb60966a0376b71f2c8c187179ef31cf5ddde47b959e8f440e08f8'},
        subjects=('functional/wordexp', 'functional/wordexp-static'),
        fixture='musl-patched-wordexp.c', checks=133),
    'wordexp-memory': dict(sources={'src/misc/wordexp.c':
        '018c97c999cb60966a0376b71f2c8c187179ef31cf5ddde47b959e8f440e08f8'},
        subjects=(), fixture='musl-wordexp-memory.c', checks=192,
        patch='musl-patched-wordexp.patch', source_flags={'src/misc/wordexp.c': (
            '-Dgetdelim=evo_getdelim', '-Drealloc=evo_realloc',
            '-Dcalloc=evo_calloc', '-Dfree=evo_free')}),
    'conf': dict(sources={
        'src/conf/fpathconf.c': '301cf7c0ecd13a12fd090ee8f6fa6de90af2c1bcea85df2e387a48b7d7baf4f9',
        'src/conf/pathconf.c': 'ede2994972d93f1951dbddf4194d6764f9e4f1c5ac9bfa56dbdd4f8d64455848',
        'src/conf/sysconf.c': '803fa64a3da9fac2377d96929d964e9e39c825fbd382c101885ea96693db61ec'},
        headers={'include/unistd.h':
            'ab2782955f7f5cd22b4feaa7dd48195a4e6d86c6d022bfd39e14ebb610b165e1'},
        subjects=('api/main',), fixture='musl-patched-conf.c', checks=27),
}


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
    suite = SUITES.get(data.get('suite'))
    if suite is None:
        return False
    expected = {(cc, opt, lib, name) for cc in ('gcc-14', 'clang-19', 'evocc')
                for opt in ('0', '2') for lib in ('reference', 'mixed')
                for name in ('probe', *suite['subjects'])}
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
        if (row.get('replacementObjects', {}).keys() != {Path(p).stem for p in suite['sources']}
                or not row.get('linkMapVerified')):
            return False
        if key[-1] == 'probe' and row['run'].get('stdout') != f"checks={suite['checks']} failures=0\n":
            return False
        if key[-1] == 'api/main' and (row.get('apiBuild', {}).get('exit') != 0 or
                len(row.get('testObjects', {})) != 79 or
                'unistd.o' not in row.get('testObjects', {})):
            return False
    return seen == expected and data.get('inputsUnchanged') is True


def audit_map(text: str, objects: list[Path]) -> bool:
    """Both explicit definitions must be present, neither old archive member."""
    return all(f'LOAD {obj}\n' in text and
               f'({obj.stem}.lo)' not in text and f'({obj.stem}.o)' not in text
               for obj in objects)


def inside(output: Path) -> int:
    plan = json.loads((output / 'plan.json').read_text())
    suite = SUITES[plan['suite']]
    rows = []
    sources = output / 'source'
    includes = ['-I/work/musl/' + p for p in PRIVATE]
    if suite.get('headers'):
        includes.insert(0, '-I' + str(sources / 'include'))
    libraries = {'reference': Path('/work/reference') / plan.get('referenceArchive', 'libc-reference.a'),
                 'mixed': Path('/work/musl/lib/libc.a')}
    identities = {str(p): digest(p) for p in libraries.values()}
    data = {'rows': rows, 'libraryHashes': identities, 'patched': plan['patched'], 'suite': plan['suite']}
    for opt in ('0', '2'):
        for cc in ('gcc-14', 'clang-19', 'evocc'):
            directory = output / (cc + '-' + opt)
            directory.mkdir()
            objects, builds = [], []
            for source in (*suite['sources'], suite['fixture']):
                name = Path(source).stem
                obj = directory / (name + '.o')
                path = sources / source if source in suite['sources'] else Path('/work/fixtures') / source
                argv = (['gcc-14', '-c', str(output / f'{name}-{opt}.s'), '-o', str(obj)]
                        if cc == 'evocc' else
                        [cc, *FLAGS, '-O' + opt, *suite.get('source_flags', {}).get(source, ()),
                         *includes, '-c', str(path), '-o', str(obj)])
                built = execute(argv)
                builds.append(built)
                objects.append(obj)
            (directory / 'builds.json').write_text(json.dumps(builds, indent=2) + '\n')
            if any(b['exit'] != 0 for b in builds):
                data['buildFailure'] = builds
                (output / 'executions.json').write_text(json.dumps(data, indent=2) + '\n')
                return 1
            replacements = {p.stem: digest(p) for p in objects[:-1]}
            api_objects, api_build = [], None
            if 'api/main' in suite['subjects']:
                original = Path('/work/tests/src/api/unistd.c')
                api_obj = directory / 'unistd.o'
                argv = (['gcc-14', '-c', str(output / f'unistd-{opt}.s'), '-o', str(api_obj)]
                        if cc == 'evocc' else [cc, *FLAGS, '-O' + opt, *includes,
                        '-I/work/tests/src/common', '-pedantic-errors', '-Werror', '-Wno-unused',
                        # The original GCC API gate accepts C99 f(). Clang's
                        # extra deprecation warning is not part of that gate;
                        # -pedantic-errors overrides -Wno-error for this warning.
                        *(['-Wno-strict-prototypes'] if cc == 'clang-19' else []),
                        '-c', str(original), '-o', str(api_obj)])
                api_build = execute(argv)
                api_objects = [p.with_suffix('.o') for p in
                    sorted(original.parent.glob('*.c')) if p.name != 'unistd.c'] + [api_obj]
            for library, archive in libraries.items():
                for subject in ('probe', *suite['subjects']):
                    obj = (objects[-1] if subject == 'probe' else
                           Path('/work/tests/src') / (subject.removesuffix('-static') + '.o'))
                    exe = directory / (library + '-' + subject.replace('/', '-'))
                    argv = ['gcc-14', '-nostdlib', '-static', '/work/pristine/obj/crt/crt1.o',
                            '/work/musl/lib/crti.o', str(obj)]
                    if subject == 'api/main':
                        argv[-1:] = [str(p) for p in api_objects]
                    elif subject != 'probe':
                        argv.append('/work/tests/src/common/libtest.a')
                    if plan['patched']:
                        argv.extend(str(p) for p in objects[:-1])
                    argv += ['-Wl,--start-group', str(archive), '-lgcc', '-lgcc_eh',
                             '-Wl,--end-group', '/work/musl/lib/crtn.o', '-o', str(exe),
                             '-Wl,-Map=' + str(exe) + '.map']
                    row = dict(compiler=cc, opt=opt, library=library, subject=subject,
                               objectSha256=digest(obj), build={'exit': 0}, link=execute(argv))
                    if subject == 'api/main':
                        row['apiBuild'] = api_build
                        row['testObjects'] = {p.name: digest(p) for p in api_objects if p.is_file()}
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
    parser.add_argument('--suite', choices=SUITES, default='numerics')
    parser.add_argument('--build', type=Path, default=ROOT / '.build/ladder',
                        help='musl lane work root (musl-work, libc-test-work, musl-pristine)')
    parser.add_argument('--reference-archive', type=Path,
                        help='reference libc archive (default: BUILD/libc-reference.a)')
    parser.add_argument('--patch', type=Path,
                        help='retained patch for a RED replay (default: the suite patch)')
    args = parser.parse_args()
    if args.inside:
        return inside(args.inside)
    if not args.evocc or not args.output:
        parser.error('--evocc and --output are required')
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import mixed
    suite = SUITES[args.suite]
    build = args.build.resolve()
    tree = build / 'musl-work'
    tests = build / 'libc-test-work'
    archive = (args.reference_archive or build / 'libc-reference.a').resolve()
    reference = archive.parent
    patch = args.patch.resolve() if args.patch else (
        ROOT / 'tests/fixtures' / suite.get('patch', Path(suite['fixture']).with_suffix('.patch')))
    compiler = args.evocc.resolve()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    paths = [compiler, Path(__file__).resolve(), patch, ROOT / 'tests/fixtures' / suite['fixture'],
             tree / 'lib/libc.a', archive, tests / 'reference-runtest',
             tests / 'src/common/libtest.a', *(tree / name for name in suite['sources']),
             *(tests / 'src' / (name.removesuffix('-static') + '.o') for name in suite['subjects'])]
    if 'api/main' in suite['subjects']:
        paths += [tests / 'src/api/unistd.c', tests / 'src/common/options.h']
        paths += list((tests / 'src/api').glob('*.o'))
    paths += [tree / name for name in suite.get('headers', {})]
    identity = {str(p): digest(p) for p in paths}
    for name, wanted in (suite['sources'] | suite.get('headers', {})).items():
        if digest(tree / name) != wanted:
            raise ValueError('unexpected dependency source: ' + name)
        target = out / 'source' / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(tree / name, target)
    # patch(1) operates only on copied sources/headers, not the installed tree.
    applied = execute(['patch', '--batch', '-p1', '-d', str(out / 'source'), '-i', str(patch)])
    (out / 'patch.json').write_text(json.dumps(applied, indent=2) + '\n')
    if applied['exit'] != 0:
        raise ValueError('dependency patch failed')
    plan = dict(configuration='musl-1.2.6-' + args.suite + '-repair-supplement', suite=args.suite,
                patched=not args.unpatched_control, image=mixed.IMAGE,
                referenceArchive=archive.name,
                originalBenchmarkUnchanged=True, identity=identity,
                patchedSources={name: digest(out / 'source' / name)
                    for name in suite['sources'] | suite.get('headers', {})},
                scope=f"{len(suite['sources'])} replacement objects; {len(suite['subjects'])} original entries",
                objectMode='explicit link inputs before original archive; no installed replacement')
    (out / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    emissions = []
    for opt in ('0', '2'):
        original_api = ('api/unistd.c',) if 'api/main' in suite['subjects'] else ()
        for source in (*suite['sources'], suite['fixture'], *original_api):
            path = out / 'source' / source if source in suite['sources'] else ROOT / 'tests/fixtures' / source
            includes = ['-I' + str(tree / p) for p in PRIVATE]
            if suite.get('headers'):
                includes.insert(0, '-I' + str(out / 'source/include'))
            if source in original_api:
                path = tests / 'src' / source
                includes.append('-I' + str(tests / 'src/common'))
            argv = [str(compiler), '--target=aarch64-unknown-linux-gnu', '--emit=asm',
                    *FLAGS, '-O' + opt, *suite.get('source_flags', {}).get(source, ()),
                    *includes, str(path)]
            row = execute(argv)
            (out / (Path(source).stem + '-' + opt + '.s')).write_text(row.pop('stdout'))
            emissions.append(row)
            (out / 'emissions.json').write_text(json.dumps(emissions, indent=2) + '\n')
            if row['exit'] != 0:
                print(row['stderr'], file=sys.stderr)
                return 1
    mounts = {'out': out, 'musl': tree, 'tests': tests, 'reference': reference,
              'pristine': build / 'musl-pristine',
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
