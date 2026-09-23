#!/usr/bin/env python3
"""Run selected upstream kselftests in two existing kernels, without rebuilding either."""

from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / "tests/contracts/kernel-selftest-contract.json"
RECEIPT = ROOT / "tests/contracts/kernel-selftest-receipt.json"


def read(path):
    return json.loads(path.read_text())


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def normalize_printk_replays(text):
    """Recover explicitly replayed console records, never inferred TAP results.

    Keep the raw serial capture; callers record these exact removals alongside
    the upstream KUnit parser's derived output. Unknown/truncated replays fail
    closed instead of dropping a possibly meaningful record.
    """
    lines, output, replays = text.splitlines(), [], []
    for index, line in enumerate(lines):
        if line != '** replaying previous printk message **':
            output.append(line)
            continue
        prefix = output[-1] if output else ''
        complete = lines[index + 1] if index + 1 < len(lines) else ''
        if not re.match(r'^\[\s*\d+\.\d+\] ', prefix) or not complete.startswith(prefix):
            raise ValueError(f'unverifiable printk replay at line {index + 1}')
        output.pop()
        replays.append({'line': index + 1, 'prefix': prefix, 'complete': complete})
    return output, replays


def ublk_inner_results(output, cases):
    """Retain the frozen scripts' child verdicts, not expected device I/O errors.

    Background ERR_CODE assignments cannot update the parent's shell variable;
    its final [PASS] and a zero exit do not erase an earlier worker failure.
    test_common.sh can also exit before the caller prints its [FAIL] record.
    These are review records, not extra TAP cases or a worker-count denominator.
    """
    programs = {}
    for case in cases:
        name = re.fullmatch(r'selftests:\s+ublk:\s+test_([a-z]+_\d+)\.sh(?:\s+#.*)?', case['name'])
        if name:
            programs[name[1]] = case['outcome']
    failed, skipped, errors, seen = [], [], [], set()
    if not programs:
        return failed, skipped, errors
    for line in output.splitlines():
        comment = re.fullmatch(r'\s*(?:#\s*)+(.+?)\s*', line)
        if not comment:
            continue
        text = comment[1]
        summary = re.match(r'([a-z]+_\d+)\s*:', text)
        if summary and summary[1] in programs:
            complete = re.fullmatch(r'([a-z]+_\d+)\s*:\s*\[(PASS|FAIL|SKIP)\]', text)
            if not complete:
                errors.append({'lines': [line], 'reason': 'incomplete or malformed ublk summary'})
                continue
            seen.add(complete[1])
            if complete[2] == 'FAIL':
                failed.append({'line': line, 'failed': 1, 'errors': 0})
            elif complete[2] == 'SKIP':
                skipped.append({'line': line, 'skipped': 1})
        elif (re.fullmatch(r"/dev/ublkc\d+ isn't removed(?: res.*)?", text)
              or re.fullmatch(r"device isn't dead\(.*\) after killing daemon", text)
              or re.fullmatch(r'delete dev \d+ failed', text)):
            failed.append({'line': line, 'failed': 0, 'errors': 1})
    for name, outcome in programs.items():
        if outcome == 'PASS' and name not in seen:
            errors.append({'lines': [], 'reason': f'missing ublk summary for {name}'})
    return failed, skipped, errors


def parse_tap(output):
    plans = re.findall(r'^1\.\.(\d+)(?:\s+.*)?$', output, re.M)
    results = re.findall(r'^(not ok|ok)\s+(\d+)\b(.*)$', output, re.M)
    cases = [{'number': int(n), 'name': rest.strip(),
              'outcome': 'SKIP' if re.search(r'#\s*SKIP\b', rest, re.I)
              else 'PASS' if ok == 'ok' else 'FAIL'} for ok, n, rest in results]
    planned = int(plans[0]) if len(plans) == 1 else None
    valid = planned is not None and [r['number'] for r in cases] == list(range(1, planned + 1))
    valid = valid and 'Bail out!' not in output
    counts = {key: sum(r['outcome'] == key for r in cases) for key in ('PASS', 'FAIL', 'SKIP')}
    # The upstream wrapper can return zero even when an inner program prints
    # failed cases (mremap_test's miscellaneous checks are one measured example).
    # Keep its verdict, but never promote it to an all-subtests claim. A nested
    # self-test may intentionally exercise failure, so these require review,
    # not a guessed automatic reclassification of the original program.
    nested, skipped, summary_errors, bpf_summaries = [], [], [], []
    bpf_cases = [case for case in cases if re.fullmatch(
        r'selftests:\s+bpf:\s+test_progs(?:-(?:no_alu32|cpuv4|bpf_gcc))?', case['name'])]
    for line in output.splitlines():
        bpf = re.fullmatch(r'\s*(?:#\s*)+Summary: (\d+)/(\d+) PASSED, (\d+) SKIPPED, (\d+) FAILED\s*', line)
        if bpf:
            passed_tests, passed_subtests, skipped_count, failed_tests = map(int, bpf.groups())
            bpf_summaries.append({'line': line, 'passedTests': passed_tests,
                                 'passedSubtests': passed_subtests, 'skipped': skipped_count,
                                 'failedTests': failed_tests})
            if not (passed_tests or failed_tests or skipped_count):
                summary_errors.append({'lines': [line], 'reason': 'BPF summary reports no tests'})
            if failed_tests:
                nested.append({'line': line, 'failed': failed_tests, 'errors': 0})
            if skipped_count:
                # test_progs counts successful tests and successful subtests
                # separately; its skip counter is neither a TAP denominator
                # nor disjoint from the successful-test count.
                skipped.append({'line': line, 'passedTests': passed_tests,
                                'passedSubtests': passed_subtests, 'skipped': skipped_count})
        elif bpf_cases and re.match(r'\s*(?:#\s*)+Summary:', line):
            summary_errors.append({'lines': [line], 'reason': 'incomplete or malformed BPF summary'})
        if re.match(r'^\s*(?:#\s*)+Totals:', line):
            totals = {key: int(value) for key, value in re.findall(r'\b(pass|fail|skip|error):\s*(\d+)', line)}
            if totals.get('fail', 0) or totals.get('error', 0):
                nested.append({'line': line, 'failed': totals.get('fail', 0), 'errors': totals.get('error', 0)})
            if totals.get('skip', 0):
                skipped.append({'line': line, 'passed': totals.get('pass', 0), 'skipped': totals['skip']})
    # test_progs replays failed case logs before its sole final summary. Those
    # replayed lines are not new tests, and its skip counter overlaps successful
    # root tests. Retain all four counters without inventing a TAP denominator.
    if len(bpf_summaries) > 1:
        summary_errors.append({'lines': [item['line'] for item in bpf_summaries],
                               'reason': 'multiple BPF summaries'})
    if any(case['outcome'] == 'PASS' for case in bpf_cases) and not bpf_summaries:
        summary_errors.append({'lines': [], 'reason': 'missing BPF summary'})
    # sched_ext's runner reports failures in this three-line summary but can
    # still exit zero. Preserve the outer TAP result while making its inner
    # failures/skips visible to the existing acceptance review. An incomplete
    # recognized summary is invalid evidence, not an empty list of failures.
    # An unrelated RESULTS heading is not enough to identify this protocol.
    scheduler_pass = any(case['outcome'] == 'PASS'
                         and re.fullmatch(r'selftests:\s+sched_ext:\s+runner', case['name'])
                         for case in cases)
    scheduler_summaries = 0
    summary_lines = [line for line in output.splitlines()
                     if not re.fullmatch(r'\s*(?:#\s*)+', line)]
    for index, line in enumerate(summary_lines):
        if not re.fullmatch(r'\s*(?:#\s*)+RESULTS:\s*', line):
            continue
        block = summary_lines[index + 1:index + 4]
        if not scheduler_pass and not any(
                re.match(r'\s*(?:#\s*)+(?:PASSED|SKIPPED|FAILED):', item) for item in block):
            continue
        scheduler_summaries += 1
        summary_counts = [re.fullmatch(r'\s*(?:#\s*)+' + label + r':\s*(\d+)\s*', item)
                          for label, item in zip(('PASSED', 'SKIPPED', 'FAILED'), block)]
        if len(summary_counts) != 3 or not all(summary_counts):
            summary_errors.append({'lines': [line, *block],
                                   'reason': 'incomplete or malformed sched_ext summary'})
            continue
        passed, skip_count, failed = [int(match[1]) for match in summary_counts]
        if passed + skip_count + failed == 0:
            summary_errors.append({'lines': [line, *block],
                                   'reason': 'sched_ext summary reports no tests'})
        if failed:
            nested.append({'line': block[2], 'failed': failed, 'errors': 0})
        if skip_count:
            skipped.append({'line': block[1], 'passed': passed, 'skipped': skip_count})
    if scheduler_pass and not scheduler_summaries:
        summary_errors.append({'lines': [], 'reason': 'missing sched_ext summary'})
    ublk_failed, ublk_skipped, ublk_errors = ublk_inner_results(output, cases)
    nested.extend(ublk_failed)
    skipped.extend(ublk_skipped)
    summary_errors.extend(ublk_errors)
    valid = valid and not summary_errors
    return {'planned': planned, 'valid': valid, 'subtests': counts, 'cases': cases,
            'nestedFailureSummaries': nested, 'nestedSkipSummaries': skipped,
            'nestedSummaryErrors': summary_errors, 'bpfSummaries': bpf_summaries}


def parse_transcript(text, nonce, programs):
    text = text.replace('\r', '')
    if text.splitlines().count(f'EVO_BOOT_NONCE {nonce}') != 1:
        raise ValueError('missing or repeated current boot nonce')
    events = list(re.finditer(r'^KSELFTEST_(BEGIN|END) (\S+) (\S+)(?: (\d+))?$', text, re.M))
    rows = {name: {'id': name, 'outcome': 'NOT_RUN', 'subtests': {'PASS': 0, 'FAIL': 0, 'SKIP': 0}, 'cases': []} for name in programs}
    active, start, seen = None, None, []
    for event in events:
        kind, observed, name, status = event.groups()
        if observed != nonce or name not in rows:
            raise ValueError('foreign test or nonce in serial transcript')
        if kind == 'BEGIN':
            if active or name in seen or name != programs[len(seen)] or status is not None:
                raise ValueError('duplicate or out-of-order test execution')
            active, start = name, event.end()
            seen.append(name)
            rows[name]['outcome'] = 'INCOMPLETE'
        else:
            if active != name or status is None:
                raise ValueError('test result without its corresponding execution')
            tap = parse_tap(text[start:event.start()])
            rc = int(status)
            outcome = ('TIMEOUT' if rc == 124 else 'FAIL' if rc not in (0, 4) or tap['subtests']['FAIL']
                       else 'INVALID_OUTPUT' if not tap['valid'] or not tap['cases'] and rc != 4
                       else 'SKIP' if rc == 4 or tap['subtests']['PASS'] == 0 else 'PASS')
            rows[name].update(tap, outcome=outcome, exitCode=rc)
            active = None
    return list(rows.values())


def compare(reference, subject):
    if [r['id'] for r in reference] != [r['id'] for r in subject]:
        raise ValueError('reference and subject test denominators differ')
    regressions, baseline_failures = [], []
    for before, after in zip(reference, subject):
        if (before['outcome'] not in ('PASS', 'SKIP')
                or before.get('nestedFailureSummaries') or before.get('nestedSkipSummaries')):
            baseline_failures.append(before['id'])
        old_cases = {r['number']: r['outcome'] for r in before['cases']}
        new_cases = {r['number']: r['outcome'] for r in after['cases']}
        lost_pass = any(value == 'PASS' and new_cases.get(n) != 'PASS' for n, value in old_cases.items())
        if (after['outcome'] not in ('PASS', 'SKIP') or lost_pass
                or before['outcome'] == 'PASS' and after['outcome'] != 'PASS'
                or before.get('planned') != after.get('planned')
                or after.get('nestedFailureSummaries') or after.get('nestedSkipSummaries')):
            regressions.append(after['id'])
    return {'status': 'PASS' if reference and not regressions and not baseline_failures
            and any(r['outcome'] == 'PASS' for r in subject) else 'INCOMPLETE',
            'regressions': regressions, 'baselineFailures': baseline_failures,
            'outcomes': dict(Counter(r['outcome'] for r in subject))}


def build_payload(work, contract, run):
    environment = read(ROOT / 'contracts/build-environment.json')
    image = environment['buildImage']['sealedImageTag']
    installed = json.loads(subprocess.check_output(
        ['/usr/local/bin/container', 'image', 'inspect', image], text=True))
    image_digest = installed[0]['configuration']['descriptor']['digest']
    if image_digest != environment['buildImage']['executionReference']:
        raise ValueError('installed selftest build image differs from the sealed image')
    programs = contract['programs']
    ids = [name.replace('/', '.') for name in programs]
    ramfs = work / 'root'
    ramfs.mkdir(parents=True, exist_ok=True)
    (ramfs / 'list').write_text(''.join(f'{name} {contract["perProgramTimeoutSeconds"]}\n' for name in ids))
    (ramfs / 'huge-pages').write_text(str(contract['hugePages']) + '\n')
    spec = ['dir /dev 755 0 0', 'dir /proc 755 0 0', 'dir /sys 755 0 0',
            'dir /tmp 1777 0 0', 'dir /scratch 755 0 0', 'dir /tests 755 0 0',
            'nod /dev/console 600 0 0 c 5 1',
            'file /init /work/selftest/root/init 755 0 0',
            'file /tests/huge-pages /work/selftest/root/huge-pages 644 0 0',
            'file /tests/list /work/selftest/root/list 644 0 0']
    spec += [f'file /tests/{name} /work/selftest/root/{name} 755 0 0' for name in ids]
    (work / 'spec.txt').write_text('\n'.join(spec) + '\n')
    commands = [['make', '-s', '-C', '/work/src', 'O=/work/selftest/headers-build',
                 'ARCH=arm64', 'HOSTCC=gcc-14', 'headers_install',
                 'INSTALL_HDR_PATH=/work/selftest/headers']]
    for name, ident in zip(programs, ids):
        directory, binary = name.rsplit('/', 1)
        output = f'/work/selftest/build/{directory}'
        (work / 'build' / directory).mkdir(parents=True, exist_ok=True)
        commands.append(['make', '-s', '-C', f'/work/src/tools/testing/selftests/{directory}',
                         f'OUTPUT={output}', 'ARCH=arm64', 'CC=gcc-14',
                         'KHDR_INCLUDES=-isystem /work/selftest/headers/include',
                         'LDFLAGS=-static', f'{output}/{binary}'])
        commands.append(['cp', f'{output}/{binary}', f'/work/selftest/root/{ident}'])
    commands.append(['gcc-14', '-static', '-O2', '-Wall', '-Wextra', '-Werror',
                     '/work/harness/selftest-init.c', '-o', '/work/selftest/root/init'])
    shell = 'set -eu\n' + '\n'.join(shlex.join(c) for c in commands)
    shell += '\n/work/bin/gen_init_cpio -t ' + str(environment['reproducibility']['SOURCE_DATE_EPOCH'])
    shell += ' /work/selftest/spec.txt > /work/selftest/initramfs.cpio\n'
    cmd = ['/usr/local/bin/container', 'run', '--rm', '--platform', 'linux/arm64', '--network', 'none',
           '-v', f'{ROOT / ".build/linux-src"}:/work/src:ro',
           '-v', f'{work}:/work/selftest', '-v', f'{ROOT / "tools/k1"}:/work/harness:ro',
           '-v', f'{ROOT / ".build/reference/gcc-lane-1/usr/gen_init_cpio"}:/work/bin/gen_init_cpio:ro',
           image, 'sh', '-c', shell]
    with (run / 'build.log').open('w') as log:
        subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=600)
        formatter = Path('/opt/homebrew/opt/e2fsprogs/sbin/mke2fs')
        format_command = [str(formatter), '-q', '-F', '-t', 'ext4', '-b', '1024',
                          '-E', 'lazy_itable_init=0,lazy_journal_init=0',
                          str(work / 'scratch.img'), '65536']
        subprocess.run(format_command, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=30)
    return {'compiler': 'gcc-14', 'image': image_digest, 'commands': commands,
            'initSourceSha256': sha(ROOT / 'tools/k1/selftest-init.c'),
            'filesystemCommand': format_command, 'filesystemToolSha256': sha(formatter),
            'initramfsSha256': sha(work / 'initramfs.cpio'), 'scratchSha256': sha(work / 'scratch.img'),
            'programs': [{'id': ident, 'source': f'tools/testing/selftests/{name}.c',
                          'sourceSha256': sha(ROOT / f'.build/linux-src/tools/testing/selftests/{name}.c'),
                          'binarySha256': sha(ramfs / ident)} for name, ident in zip(programs, ids)]}


def flight(name, image, work, run, contract, ids, extra_env=None):
    directory = run / name
    env = {**os.environ, **(extra_env or {}), 'K1_INITRAMFS': str(work / 'initramfs.cpio'),
           'K1_TEST_DISK': str(work / 'scratch.img'),
           'K1_CONSOLE_LOGLEVEL': str(contract['consoleLoglevel']),
           'K1_BOOT_TIMEOUT': str(contract['bootOverheadSeconds'] + len(ids) * contract['perProgramTimeoutSeconds'])}
    expected_image = sha(image)
    print(f'{name}: running {len(ids)} upstream programs on Image {expected_image[:12]}', flush=True)
    done = subprocess.run([str(ROOT / 'tools/k1/boot.sh'), str(image), str(directory)],
                          env=env, check=False)
    boot = read(directory / 'boot.json')
    if sha(image) != expected_image or boot['imageSha256'] != expected_image:
        raise ValueError('kernel Image changed during selftests')
    log = Path(boot['serialLog'])
    if sha(log) != boot['serialLogSha256']:
        raise ValueError('serial capture differs from boot receipt')
    rows = parse_transcript(log.read_text(errors='replace'), boot['nonce'], ids)
    for row in rows:
        print(f"  {row['id']}: {row['outcome']} {row['subtests']}", flush=True)
    return {'boot': boot, 'exitCode': done.returncode, 'programs': rows,
            'consoleLoglevel': contract['consoleLoglevel'],
            'hugePages': int(env.get('K1_SELFTEST_HUGEPAGES', contract['hugePages'])),
            'outcomes': dict(Counter(r['outcome'] for r in rows)),
            'subtests': dict(sum((Counter(r['subtests']) for r in rows), Counter()))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--allow-qemu-drift', action='store_true')
    args = parser.parse_args()
    if args.allow_qemu_drift:
        os.environ['K1_ALLOW_QEMU_DRIFT'] = '1'
    contract = read(CONTRACT)
    kernel_receipt = ROOT / 'tests/contracts/kernel-full-receipt.json'
    kernel = read(kernel_receipt)
    subject = ROOT / kernel['boot']['image']
    if (kernel.get('status') != 'PASS' or not kernel.get('completeComposition')
            or kernel['counts'] != {'substituted': kernel['requestedUnits']}
            or kernel['boot']['verdict'] != 'PASS' or sha(subject) != kernel['imageSha256']
            or kernel['boot']['imageSha256'] != kernel['imageSha256']):
        raise ValueError('a verified complete booted EvoCC kernel is required')
    work = ROOT / '.build/kselftest'
    run_id = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    run = ROOT / '.evidence/kselftest' / run_id
    run.mkdir(parents=True)
    receipt = {'schemaVersion': 1, 'run': run_id, 'status': 'RUNNING',
               'scope': contract['scope'], 'contractSha256': sha(CONTRACT),
               'kernelReceiptSha256': sha(kernel_receipt), 'kernelImageSha256': kernel['imageSha256'],
               'compilerComposition': kernel.get('compilerComposition', []),
               'programs': len(contract['programs']), 'evidence': str(run.relative_to(ROOT)),
               'kunit': contract['kunit']}
    def save():
        value = json.dumps(receipt, indent=2) + '\n'
        (run / 'summary.json').write_text(value)
        RECEIPT.write_text(value)
    save()
    try:
        receipt['payload'] = build_payload(work, contract, run)
        save()
        ids = [r['id'] for r in receipt['payload']['programs']]
        receipt['reference'] = flight('reference', ROOT / '.build/reference/gcc-lane-1/arch/arm64/boot/Image', work, run, contract, ids)
        save()
        receipt['subject'] = flight('subject', subject, work, run, contract, ids)
        receipt.update(compare(receipt['reference']['programs'], receipt['subject']['programs']))
        if any(receipt[name]['boot']['verdict'] != 'PASS' for name in ('reference', 'subject')):
            receipt['status'] = 'INCOMPLETE'
        if (sha(kernel_receipt) != receipt['kernelReceiptSha256']
                or sha(work / 'initramfs.cpio') != receipt['payload']['initramfsSha256']
                or sha(work / 'scratch.img') != receipt['payload']['scratchSha256']):
            raise ValueError('measured kernel receipt or common test inputs changed during the flights')
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        detail = (f'payload command exited {error.returncode}; see build.log'
                  if isinstance(error, subprocess.CalledProcessError) else str(error))
        receipt.update(status='ERROR', error=detail)
        save()
        print(f'kernel-selftest: {detail}; evidence: {run}', file=sys.stderr)
        return 2
    save()
    print(f"kernel-selftest: {receipt['status']}; {receipt['outcomes']}; evidence: {run}", flush=True)
    # Like the other lanes, measured FAIL is a receipt verdict, not a runner error.
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
