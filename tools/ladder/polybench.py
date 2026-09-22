#!/usr/bin/env python3
"""Roadmap E8: released PolyBench/C kernels and their original live-out arrays.

MINI_DATASET is an explicit correctness configuration, not a performance claim.
The upstream allocation/instrumentation utility stays reference-built, like
the CRT and test driver; every one of the 30 listed kernel sources is attempted.
"""
import argparse
import collections
import datetime as dt
import importlib.util
import json
from pathlib import Path, PurePosixPath
import sys

_spec = importlib.util.spec_from_file_location('ladder_subject', Path(__file__).with_name('subject.py'))
ladder = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = ladder
_spec.loader.exec_module(ladder)

# The released utility uses POSIX allocation declarations exposed in GNU C.
# Compare the approved noncontracting FP profile in every compiler: an FMA
# reference is not an exact-value oracle for separate multiply/add operations.
FLAGS = ('-std=gnu99', '-O2', '-ffp-contract=off',
         '-DMINI_DATASET', '-DPOLYBENCH_DUMP_ARRAYS')
POLYBENCH = ladder.Subject(
    name='polybench', rung='E8', pin='4.2.1-beta (2016-05-20)',
    tree='polybench-c-4.2.1-beta',
    url='https://downloads.sourceforge.net/project/polybench/polybench-c-4.2.1-beta.tar.gz',
    sha256='426519ee8443a5f2175de6a3e9328cda8917a5e33053d0e8f59855ab56d689a4',
    configure='', build='', relink='', test='original POLYBENCH_DUMP_ARRAYS',
)


def inventory(tree):
    sources = []
    names = set()
    for line in (tree / 'utilities/benchmark_list').read_text().splitlines():
        path = PurePosixPath(line.strip())
        if (path.is_absolute() or '..' in path.parts or path.suffix != '.c'
                or path.stem in names or not (tree / str(path)).is_file()):
            raise ValueError(f'invalid or duplicate upstream kernel: {line}')
        sources.append(str(path))
        names.add(path.stem)
    return sources


def program_outcome(compilation, gcc, clang, subject):
    def valid(result):
        return result is not None and result['exit'] == 0 and result['hasArrayDump']

    def same(left, right):
        return all(left[k] == right[k] for k in ('stdoutSha256', 'stderrSha256'))

    if not valid(gcc) or not valid(clang) or not same(gcc, clang):
        return 'oracle-unusable'
    if compilation != 'assembled':
        return {'refused': 'compiler-refused', 'timeout': 'compiler-timeout',
                'assembler-refused': 'assembler-failure'}.get(compilation, 'compiler-error')
    if subject is None:
        return 'link-failure'
    if subject['exit'] in (124, 137):
        return 'runtime-timeout'
    return 'PASS' if valid(subject) and same(gcc, subject) else 'runtime-failure'


def native_script(sources, run_id, *, subject=False):
    """Use the same released utility and array printer in every native variant."""
    tree = f'/work/polybench/{POLYBENCH.tree}'
    output = f'/work/polybench/native/{run_id}'
    flags = ' '.join(FLAGS)
    lines = [f'cd {tree} || exit 2', f'mkdir -p {output}']
    variants = [('evocc', 'gcc-14')] if subject else [('gcc', 'gcc-14'), ('clang', 'clang-19')]
    for variant, cc in variants:
        directory = f'{output}/{variant}'
        lines.append(f'mkdir -p {directory}')
        support = f'{output}/gcc/utility.o' if subject else f'{directory}/utility.o'
        if not subject:
            lines.append(f'{cc} {flags} -I utilities -c utilities/polybench.c -o {support} '
                         f'> {directory}/utility.stdout 2> {directory}/utility.stderr; '
                         f'echo $? > {directory}/utility.status')
        for source in sources:
            name = PurePosixPath(source).stem
            prefix = f'{directory}/{name}'
            inputs = f'/work/polybench/evocc-objects/obj/{name}.o' if subject else source
            command = (f'{cc} {flags} -I utilities -I {PurePosixPath(source).parent} '
                       f'{inputs} {support} -lm -no-pie -o {prefix}.exe')
            lines.append(f'{command} > {prefix}.compile.stdout 2> {prefix}.compile.stderr; '
                         f'status=$?; echo "$status" > {prefix}.compile.status; '
                         f'if [ "$status" -eq 0 ]; then '
                         f'timeout -s KILL 60 {prefix}.exe > {prefix}.stdout 2> {prefix}.stderr; '
                         f'echo $? > {prefix}.status; fi')
    return '\n'.join(lines)


def read_run(prefix):
    status = prefix.with_suffix('.status')
    stdout = prefix.with_suffix('.stdout')
    stderr = prefix.with_suffix('.stderr')
    if not all(path.is_file() for path in (status, stdout, stderr)):
        return None
    data = stderr.read_bytes()
    return {'exit': int(status.read_text()),
            'stdoutSha256': ladder.sha256_file(stdout),
            'stderrSha256': ladder.sha256_file(stderr),
            'hasArrayDump': b'==BEGIN DUMP_ARRAYS==' in data and b'==END   DUMP_ARRAYS==' in data}


def measure(args):
    driver = Path(args.evocc).resolve()
    run_id = args.run_id or dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    evidence = ladder.EVIDENCE / POLYBENCH.name / run_id
    evidence.mkdir(parents=True, exist_ok=False)
    tree = ladder.unpack(POLYBENCH)
    sources = inventory(tree)
    if len(sources) != 30:
        raise RuntimeError('the pinned upstream 30-kernel inventory changed')
    work = ladder.BUILD / POLYBENCH.name
    native = work / 'native' / run_id
    if native.exists():
        raise RuntimeError('native output directory already exists')
    reference_command = native_script(sources, run_id)
    (evidence / 'reference-command.sh').write_text(reference_command + '\n')
    reference = ladder.container(work, POLYBENCH.name, reference_command, timeout=2400)
    if reference.returncode:
        raise RuntimeError(f'reference container failed: {reference.stderr}')
    sysroot, adjustments = ladder.adjust_sysroot()
    prefix = f'/work/polybench/{POLYBENCH.tree}'
    units = [ladder.Unit(f'obj/{PurePosixPath(source).stem}.o', source, '',
                        ('gcc-14', *FLAGS, '-I', f'{prefix}/utilities',
                         '-I', f'{prefix}/{PurePosixPath(source).parent}',
                         '-c', f'{prefix}/{source}', '-o', f'obj/{PurePosixPath(source).stem}.o'))
             for source in sources]
    (evidence / 'compile-commands.json').write_text(json.dumps(
        [ladder.dataclasses.asdict(unit) for unit in units], indent=1) + '\n')
    asm = work / 'evocc-objects'
    rows = ladder.emit(POLYBENCH, units, driver, tree,
                       ladder.e0.include_flags(sysroot), asm, timeout=60)
    ladder.assemble(rows, asm)
    compiled = [r['source'] for r in rows if r['outcome'] == 'assembled']
    subject_command = native_script(compiled, run_id, subject=True)
    (evidence / 'subject-command.sh').write_text(subject_command + '\n')
    result = ladder.container(work, POLYBENCH.name, subject_command, timeout=2400)
    if result.returncode:
        raise RuntimeError(f'subject container failed: {result.stderr}')
    original_arguments = {unit.obj: list(unit.argv) for unit in units}
    for row in rows:
        name = PurePosixPath(row['source']).stem
        observations = {variant: read_run(native / variant / name) for variant in ('gcc', 'clang', 'evocc')}
        row.update(sourceSha256=ladder.sha256_file(tree / row['source']),
                   originalArguments=original_arguments[row['unit']], observations=observations,
                   programOutcome=program_outcome(row['outcome'], *observations.values()))
    counts = dict(collections.Counter(r['programOutcome'] for r in rows))
    summary = {
        'schemaVersion': 1, 'rung': 'E8', 'subject': 'polybench', 'run': run_id,
        'pin': POLYBENCH.pin, 'sourceSha256': POLYBENCH.sha256,
        'driverSha256': ladder.sha256_file(driver), 'programsTotal': len(rows),
        'unitsTotal': len(rows), 'unitsEvoccCompiled': len(compiled),
        'programOutcomes': counts, 'outcomes': dict(collections.Counter(r['outcome'] for r in rows)),
        'refusalsByCode': dict(collections.Counter(r.get('code', 'other') for r in rows
                                                  if r['outcome'] == 'refused')),
        'refusals': [{k: r.get(k, '') for k in ('source', 'code', 'message', 'at')}
                     for r in rows if r['outcome'] == 'refused'],
        'test': {'passed': counts.get('PASS', 0) == len(rows), 'outcomes': counts},
        'configuration': {'flags': list(FLAGS), 'timeoutSeconds': 60, 'jobs': 1,
                          'comparison': 'exact original live-out stdout/stderr; both reference dumps must agree'},
        'runtimeSupport': {'source': 'utilities/polybench.c',
                           'sourceSha256': ladder.sha256_file(tree / 'utilities/polybench.c'),
                           'compiler': 'gcc-14 for subject; respective compiler for each reference',
                           'role': 'upstream allocation/instrumentation harness; excluded from the 30-kernel denominator'},
        'adjustments': adjustments, 'headerRoot': str(sysroot),
        'evidence': str(evidence.relative_to(ladder.ROOT)),
        'nativeEvidence': str(native.relative_to(ladder.ROOT)),
        'summaryLine': f'PolyBench: {counts.get("PASS", 0)}/{len(rows)} original kernels PASS; {counts}',
    }
    (evidence / 'results.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in rows))
    (evidence / 'summary.json').write_text(json.dumps(summary, indent=1) + '\n')
    ladder.receipt_path(POLYBENCH).write_text(json.dumps(summary, indent=1) + '\n')
    print(summary['summaryLine'], flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evocc', default=str(ladder.default_evocc()))
    parser.add_argument('--run-id')
    measure(parser.parse_args())
    return 0


if __name__ == '__main__':
    sys.exit(main())
