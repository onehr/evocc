#!/usr/bin/env python3
"""Pinned SingleSource, original lit reference-output checks, C object ramp.

C++ programs are excluded by language and the suite's measuring tools remain
reference-built. Both populations are reported separately from EvoCC coverage.
"""
import argparse
import collections
import datetime as dt
import importlib.util
import json
import os
from pathlib import Path
import shlex
import shutil
import sys

_spec = importlib.util.spec_from_file_location('ladder_subject', Path(__file__).with_name('subject.py'))
ladder = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = ladder
_spec.loader.exec_module(ladder)
mbed = ladder._module('llvm_cmake_pin', Path(__file__).with_name('mbedtls.py'))

COMMIT = '2919e55176a572ee085cb46b291c47b7092c363e'
TREE = f'llvm-test-suite-{COMMIT}'
CMAKE = f'/work/llvm-test-suite/{mbed.CMAKE_TREE}/bin/cmake'
LIT_URL = ('https://files.pythonhosted.org/packages/fc/89/'
           'ba0b96c97d4a78bdaeeabdc2d17177ba2d0e899e8457d85b758e95543699/'
           'lit-23.1.1-py3-none-any.whl')
LIT_SHA256 = '201adf4f4c73ef6ac6e5e62b01a51f0bf9cb229cc607e186ea96471506bf3e43'
PSUTIL_URL = ('https://files.pythonhosted.org/packages/63/65/'
              '37648c0c158dc222aba51c089eb3bdfa238e621674dc42d48706e639204f/'
              'psutil-7.2.2-cp36-abi3-manylinux2014_aarch64.manylinux_2_17_aarch64.manylinux_2_28_aarch64.whl')
PSUTIL_SHA256 = 'b0726cecd84f9474419d67252add4ac0cd9811b04d61123054b9fb6f57df6e9e'
CONFIGURE = (f'{CMAKE} -S . -B build -DCMAKE_C_COMPILER=clang-19 '
             '-DCMAKE_CXX_COMPILER=clang++-19 -DCMAKE_BUILD_TYPE=Release '
             '-DCMAKE_EXE_LINKER_FLAGS=-no-pie -DCMAKE_EXPORT_COMPILE_COMMANDS=ON '
             '-DTEST_SUITE_SUBDIRS=SingleSource -DTEST_SUITE_BENCHMARKING_ONLY=OFF '
             '-DTEST_SUITE_RUN_BENCHMARKS=ON -DTEST_SUITE_COLLECT_CODE_SIZE=OFF '
             '-DTEST_SUITE_COLLECT_COMPILE_TIME=OFF')
HELPER = 'python3 /work/llvm-test-suite/llvm_single_source.py'
BUILD = f'{HELPER} build {CMAKE}'
LLVM = ladder.Subject(
    name='llvm-test-suite', rung='E9', pin=f'llvmorg-23.1.0 ({COMMIT})', tree=TREE,
    url=f'https://codeload.github.com/llvm/llvm-test-suite/tar.gz/{COMMIT}',
    sha256='415106c9962e07a24326ad9173bbecd4215f6dee49bce9aec5181d75d07db770',
    archive_members=('CMakeLists.txt', 'cmake', 'tools', 'litsupport', 'lit.cfg',
                     'lit.site.cfg.in', 'HashProgramOutput.sh', 'SingleSource'),
    source_prefixes=('SingleSource/',),
    configure=f'({CONFIGURE} && {HELPER} prepare)', build=BUILD, relink=BUILD,
    test=f'PYTHONPATH=/work/llvm-test-suite/lit {HELPER} test',
    notes=(
        'Official SingleSource CMake population with upstream default inputs; no FP prefilter.',
        'Clang-19 reference build; language classification comes from the complete CMake compilation database. '
        'Non-C programs are listed as excluded, not run or counted as passes; measurement tools stay reference-built.',
        'Original lit output comparison, two workers, 60-second per-test wall limit; '
        'code-size/compile-time collection disabled because this is a correctness measurement.',
        'CMake-generated Makefile2 recipes build the selected targets in one shared dependency graph; '
        'keep-going records independent build failures without aborting the other programs.',
        'Only SingleSource and its build/lit support extracted; MultiSource/External data are not used.',
        f'lit 23.1.1 wheel SHA-256 {LIT_SHA256}; its timeout dependency psutil 7.2.2 '
        f'linux/arm64 abi3 wheel SHA-256 {PSUTIL_SHA256}; portable CMake as pinned by the TLS adapter.',
    ),
)


def prepare_tools():
    directory = ladder.BUILD / LLVM.name
    directory.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(Path(__file__).with_name('llvm_single_source.py'), directory / 'llvm_single_source.py')
    archive = directory / COMMIT
    shared = ladder.BUILD / 'dl/llvm-test-suite-2919e551.tar.gz'
    if not archive.exists() and shared.exists():
        os.link(shared, archive)
    cmake = ladder.dataclasses.replace(LLVM, url=mbed.CMAKE_URL, sha256=mbed.CMAKE_SHA256,
                                      tree=mbed.CMAKE_TREE, archive_members=())
    old = ladder.BUILD / 'mbedtls' / f'{mbed.CMAKE_TREE}.tar.gz'
    cached = directory / old.name
    if not cached.exists() and old.exists():
        os.link(old, cached)
    ladder.unpack(cmake)
    for url, digest in ((LIT_URL, LIT_SHA256), (PSUTIL_URL, PSUTIL_SHA256)):
        package = ladder.dataclasses.replace(LLVM, url=url, sha256=digest)
        wheel, _ = ladder.fetch(package)
        result = ladder.run(['unzip', '-oq', str(wheel), '-d', str(directory / 'lit')])
        if result.returncode:
            raise SystemExit(f'cannot unpack lit dependency: {result.stderr}')


def program_outcome(reference, compilation, result, linked, unchanged):
    if reference != 'PASS':
        return 'oracle-unusable'
    if compilation != 'assembled':
        return {'refused': 'compiler-refused', 'timeout': 'compiler-timeout',
                'assembler-refused': 'assembler-failure'}.get(compilation, 'compiler-error')
    if not linked:
        return 'link-failure'
    if not unchanged:
        return 'rebuilt'
    if result == 'PASS':
        return 'PASS'
    return 'runtime-timeout' if result == 'TIMEOUT' else 'runtime-failure'


def program_disposition(row, source, reviewed, baseline_evidence):
    """Bind the existing ledger to source and command; never infer FP from a code."""
    if row['programOutcome'] == 'oracle-unusable':
        return {'disposition': 'oracle-unusable',
                'dispositionReason': f"original reference result is {row['reference']}; no usable passing control",
                'dispositionEvidence': [baseline_evidence],
                'sourceSha256': ladder.sha256_file(source)}
    rule = reviewed.get(f"llvm-test-suite:{row['source']}")
    if rule is not None and (
            not isinstance(rule, dict) or not rule.get('originalArguments')
            or rule['originalArguments'] != row.get('originalArguments')):
        return {'disposition': 'compiler-failure',
                'dispositionError': 'reviewed disposition has missing or changed original command'}
    return ladder.e0.classify_program(
        {'suite': 'llvm-test-suite', 'program': row['source'],
         'outcome': {'PASS': 'pass', 'compiler-refused': 'refused'}.get(
             row['programOutcome'], 'fail')},
        source, reviewed)


def lit_results(evidence, tag):
    output = ladder.BUILD / LLVM.name / 'lit-results.json'
    output.unlink(missing_ok=True)
    result = ladder.run_tests(LLVM, evidence, tag)
    if not output.is_file():
        raise RuntimeError(f'lit produced no program results: {evidence}/test-{tag}.log')
    shutil.copyfile(output, evidence / f'lit-{tag}.json')
    tests = json.loads(output.read_text())['tests']
    return result, {t['name'].split(' :: ', 1)[-1]: t for t in tests}


def measure(args):
    """SingleSource programs are independent; do not bisect one giant suite.

    A reference failure stays in the denominator and has its own disposition.
    Before relinking a substituted program, remove its reference executable:
    a failed link must not leave an old passing executable to be tested.
    """
    driver = Path(args.evocc).resolve()
    run_id = args.run_id or dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    evidence = ladder.EVIDENCE / LLVM.name / run_id
    evidence.mkdir(parents=True, exist_ok=False)
    tree = ladder.unpack(LLVM)
    disposition_file = ladder.ROOT / 'tests/contracts/benchmark-program-dispositions.json'
    reviewed = json.loads(disposition_file.read_text())['programs']
    sysroot, adjustments = ladder.adjust_sysroot()
    build = ladder.reference_build(LLVM, tree, evidence)
    inventory_path = tree / 'c-programs.json'
    if not inventory_path.is_file():
        raise RuntimeError(f'reference configuration failed: {evidence}')
    programs = json.loads(inventory_path.read_text())
    shutil.copyfile(inventory_path, evidence / 'c-programs.json')
    entries = json.loads((tree / 'build/compile_commands.json').read_text())
    wanted = {o for p in programs['selected'] for o in p['objects']}
    prefix = f'/work/{LLVM.name}/{LLVM.tree}/'
    units = [ladder.Unit('build/' + e['output'], e['file'].removeprefix(prefix),
                        e['directory'].removeprefix(prefix),
                        tuple(e.get('arguments') or shlex.split(e['command'])))
             for e in entries if e['output'] in wanted]
    if len(units) != len(wanted):
        raise RuntimeError('C program inventory and compilation database disagree')
    (evidence / 'compile-commands.json').write_text(json.dumps(
        [ladder.dataclasses.asdict(u) for u in units], indent=1) + '\n')
    symlinks = ladder.portable_build_symlinks(tree, prefix.rstrip('/'))
    baseline, reference = lit_results(evidence, 'baseline')
    asm_root = ladder.BUILD / LLVM.name / 'evocc-objects'
    if asm_root.is_dir():
        shutil.rmtree(asm_root)
    asm_root.mkdir()
    rows = ladder.emit(LLVM, units, driver, tree, ladder.e0.include_flags(sysroot), asm_root)
    ladder.assemble(rows, asm_root)
    assembled = [r['unit'] for r in rows if r['outcome'] == 'assembled']
    ladder.install(tree, assembled, asm_root)
    for p in programs['selected']:
        if all('build/' + o in assembled for o in p['objects']):
            executable = tree / p['test'][:-5]
            executable.resolve().relative_to(tree.resolve())
            executable.unlink(missing_ok=True)
    link = ladder.relink(LLVM, evidence, 'final')
    final, measured = lit_results(evidence, 'final')
    rebuilt = ladder.rebuilt_by_make(tree, assembled, asm_root)
    by_object = {r['unit']: r for r in rows}
    original_arguments = {unit.obj: list(unit.argv) for unit in units}
    results = []
    for p in programs['selected']:
        objects = ['build/' + o for o in p['objects']]
        # This pinned SingleSource inventory has one object per program.
        if len(objects) != 1:
            raise RuntimeError('new multi-object program needs explicit program aggregation')
        row = by_object[objects[0]]
        test = p['test'].removeprefix('build/')
        before = reference.get(test, {}).get('code', 'NOT_RUN')
        after = measured.get(test, {}).get('code', 'NOT_RUN')
        outcome = program_outcome(before, row['outcome'], after,
                                  (tree / p['test'][:-5]).is_file(), objects[0] not in rebuilt)
        result = row | {'program': test, 'reference': before, 'native': after,
                        'programOutcome': outcome,
                        'originalArguments': original_arguments[objects[0]]}
        result.update(program_disposition(
            result, tree / row['source'], reviewed,
            str((evidence / 'lit-baseline.json').relative_to(ladder.ROOT))))
        results.append(result)
    counts = collections.Counter(r['programOutcome'] for r in results)
    refusals = collections.Counter(r['code'] for r in rows if r['outcome'] == 'refused')
    passing = [r['unit'] for r in results if r['programOutcome'] == 'PASS']
    summary = {
        'schemaVersion': 1, 'rung': LLVM.rung, 'subject': LLVM.name,
        'run': run_id, 'pin': LLVM.pin, 'tarballSha256': LLVM.sha256,
        'driverSha256': ladder.sha256_file(driver), 'target': ladder.TARGET,
        'unitsTotal': len(units), 'programsTotal': len(results),
        'unitsEvoccCompiled': len(assembled), 'unitsSubstituted': len(assembled),
        'unitsInPassingBuild': len(passing), 'substitutedUnits': assembled,
        'unitsRebuiltByMake': rebuilt, 'unitsExcluded': 0, 'excludedUnits': [],
        'nonCPrograms': programs['excluded'], 'programOutcomes': dict(counts),
        'dispositionCounts': {label: sum(r['disposition'] == label for r in results)
                              for label in ladder.e0.DISPOSITIONS},
        'outcomes': dict(collections.Counter(r['outcome'] for r in rows)),
        'refusalsByCode': dict(refusals),
        'refusals': [{k: r.get(k, '') for k in ('unit', 'code', 'message', 'at')}
                     for r in rows if r['outcome'] == 'refused'],
        'failingUnits': [r for r in results if r['programOutcome'] in
                         ('link-failure', 'rebuilt', 'runtime-failure', 'runtime-timeout')],
        'baselineTest': {'passed': baseline['passed'], 'status': baseline['status'],
                         'buildStatus': build['status'],
                         'outcomes': dict(collections.Counter(t['code'] for t in reference.values()))},
        'test': {'passed': len(passing) == len(results), 'status': final['status'],
                 'relinkStatus': link['status'], 'command': LLVM.test,
                 'outcomes': dict(collections.Counter(t['code'] for t in measured.values()))},
        'adjustments': {'subject': list(LLVM.notes), 'sysroot': adjustments,
                        'portableBuildSymlinks': symlinks,
                        'omittedFlags': sorted({f for r in rows for f in r['omittedFlags']})},
        'evidence': str(evidence.relative_to(ladder.ROOT)),
        'summaryLine': f'LLVM SingleSource: {len(passing)}/{len(results)} C programs compiled by EvoCC and PASS; {dict(counts)}',
    }
    (evidence / 'results.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in results))
    (evidence / 'summary.json').write_text(json.dumps(summary, indent=1) + '\n')
    ladder.receipt_path(LLVM).write_text(json.dumps(summary, indent=1) + '\n')
    print(summary['summaryLine'], flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evocc', default=str(ladder.default_evocc()))
    parser.add_argument('--run-id')
    args = parser.parse_args()
    prepare_tools()
    measure(args)
    return 0


if __name__ == '__main__':
    sys.exit(main())
