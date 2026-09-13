#!/usr/bin/env python3
"""Build every upstream kselftest collection and run installed tests in isolated VMs."""

from __future__ import annotations

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]


def test_manifest(text):
    rows = [line for line in text.splitlines()
            if re.fullmatch(r'[\w./+-]+:[\w./+-]+', line)]
    if len(set(rows)) != len(rows) or any('..' in r.replace(':', '/').split('/') for r in rows):
        raise ValueError('duplicate or unsafe upstream test name')
    return rows


def summarize(builds, runtime):
    actual = {row['id']: row for row in runtime}
    counts = Counter()
    incomplete = []
    for build in builds:
        available = build.get('available', build['tests'])
        counts['BUILD_UNAVAILABLE'] += len(build['tests']) - len(available)
        row = actual.get(build['id'], {})
        cases = row.get('cases', [])
        if len(cases) > len(available):
            raise ValueError('runtime denominator exceeds installed test list')
        counts.update(case['outcome'] for case in cases)
        counts['NOT_RUN'] += len(available) - len(cases)
        if available and (not row.get('valid') or row.get('boot', {}).get('verdict', 'PASS') != 'PASS'):
            incomplete.append(build['id'])
    counts = +counts
    failed = [b['id'] for b in builds if b['buildExit'] != 0 or b.get('buildErrors')]
    empty = [b['id'] for b in builds if not b.get('available', b['tests'])]
    return {'collections': len(builds), 'programs': sum(len(b['tests']) for b in builds),
            'completedCollections': sum(bool(b.get('available', b['tests']))
                and len(actual.get(b['id'], {}).get('cases', [])) == len(b.get('available', b['tests']))
                and all(c.get('started', True) for c in actual.get(b['id'], {}).get('cases', []))
                for b in builds),
            'outcomes': dict(counts), 'buildFailures': failed, 'noRunnableTests': empty,
            'incompleteCollections': incomplete,
            'timedOutCollections': [r['id'] for r in runtime if r.get('outcome') == 'TIMEOUT'
                                    or r.get('boot', {}).get('verdict') == 'TIMEOUT'],
            'status': 'PASS' if counts and set(counts) == {'PASS'} and not failed
            and not empty and not incomplete else 'INCOMPLETE'}


def program_case(ident, result):
    """A VM watchdog failure is one measured program, not the rest of its collection."""
    boot = result['boot']['verdict']
    row = result['programs'][0]
    outcome = row['outcome']
    if outcome == 'NOT_RUN':
        outcome = 'NOT_RUN'
    elif boot == 'TIMEOUT' or outcome == 'TIMEOUT':
        outcome = 'TIMEOUT'
    elif boot != 'PASS':
        outcome = 'INCOMPLETE'
    elif not row.get('valid') or len(row['cases']) != 1:
        outcome = 'INVALID_OUTPUT'
    elif outcome == 'FAIL':
        outcome = 'FAIL'
    else:
        outcome = row['cases'][0]['outcome']
        if '# TIMEOUT ' in row['cases'][0].get('name', ''):
            outcome = 'TIMEOUT'
    return {'id': ident, 'outcome': outcome, 'bootVerdict': boot,
            'started': row['outcome'] != 'NOT_RUN',
            'valid': row.get('valid', False) and boot == 'PASS',
            'exitCode': row.get('exitCode')}


def reusable_reference(previous, current):
    """Reuse a completed control measurement, never the old subject results."""
    if previous['status'] not in ('PASS', 'INCOMPLETE'):
        raise ValueError('reference run is not finished')
    for key in ('build', 'referenceConfigSha256', 'initramfsSha256', 'rootfsSha256', 'runtimePolicy'):
        if previous[key] != current[key]:
            raise ValueError(f'reference reuse input changed: {key}')
    cases = [case for row in previous['execution']['reference']['collections'] for case in row['cases']]
    wanted = [ident for b in current['build']['builds'] for ident in b['available']]
    if sorted(c['id'] for c in cases) != sorted(wanted):
        raise ValueError('reference run does not account for every installed program exactly once')
    if any(c['outcome'] == 'PASS' and c.get('exitCode') != 0 for c in cases):
        raise ValueError('reference PASS disagrees with its process exit')
    return cases


def save(path, data):
    path.write_text(json.dumps(data, indent=2) + '\n')


def build_command(command, handle, timeout):
    process = subprocess.Popen(command, stdout=handle, stderr=subprocess.STDOUT, start_new_session=True)
    try:
        return process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait()
        return 124


def build_worker(jobs):
    """Runs only inside the disposable Linux build container, never on the host."""
    work, original = Path('/work/full'), Path('/work/src')
    source = work / 'source'
    source.mkdir(exist_ok=True)
    # Several upstream Makefiles ignore OUTPUT. Stage only tools (90 MiB), not
    # the kernel tree; preserve test sources and use their unmodified recipes.
    (source / 'tools').mkdir(exist_ok=True)
    subprocess.run(['rsync', '-a', str(original / 'tools') + '/', str(source / 'tools') + '/'], check=True)
    for path in original.iterdir():
        if path.name != 'tools' and not (source / path.name).exists():
            (source / path.name).symlink_to(path)
    logs, rootfs = work / 'logs', work / 'rootfs'
    logs.mkdir(exist_ok=True)
    (work / 'obj').mkdir(exist_ok=True)
    tests = rootfs / 'tests'
    if tests.exists():
        shutil.rmtree(tests)  # This lane's generated install only, never source or kernel objects.
    tests.mkdir(parents=True, exist_ok=True)
    base = ['make', '--no-print-directory', '-C', str(source / 'tools/testing/selftests'),
            'ARCH=arm64', 'SKIP_TARGETS=']
    enumeration = subprocess.check_output(base + ['-s', '-f', 'Makefile', '-f', '-', 'print_collections'],
        input='print_collections:\n\t@printf "%s\\n" $(TARGETS) $(TARGETS_HOTPLUG)\n', text=True)
    targets = list(dict.fromkeys(enumeration.split()))
    if not targets or any(not re.fullmatch(r'[\w/-]+', t) for t in targets):
        raise ValueError('invalid upstream TARGETS enumeration')
    metadata = {'targets': targets, 'enumerationCommand': base,
                'packages': subprocess.check_output(['dpkg-query', '-W'], text=True),
                'compiler': subprocess.check_output(['gcc-14', '--version'], text=True), 'builds': []}
    save(work / 'build.json', metadata)
    for target in targets:
        if shutil.disk_usage(work).free < 8 << 30:
            raise RuntimeError('less than 8 GiB free; stopping before filling the host disk')
        directory = source / 'tools/testing/selftests' / target
        output = directory
        cmd = ['make', '--no-print-directory', '-C', str(directory), '-o', 'headers', 'ARCH=arm64',
               'CC=gcc-14', 'HOSTCC=gcc-14', 'CLANG=clang-19', 'LLC=llc-19',
               f'OUTPUT={output}', f'O={work / "obj"}',
               'KHDR_INCLUDES=-isystem /work/headers/include',
               f'CPPFLAGS=-I{source / "tools/testing/selftests"} -isystem /work/headers/include',
               'CFLAGS_NOLIBC_TEST=-isystem /work/headers/include']
        log = logs / (target.replace('/', '.') + '.log')
        started = time.monotonic()
        with log.open('w') as handle:
            # The arm64 signal Makefile copies all outputs from multiple rules;
            # its parallel cp race is unrelated to kernel/compiler behavior.
            target_jobs = 1 if target == 'arm64' else jobs
            build_rc = build_command(cmd + ['-k', '-j', str(target_jobs), 'all'], handle, 600)
            build_errors = [line for line in log.read_text(errors='replace').splitlines()
                            if re.search(r'make(?:\[\d+\])?: \*\*\*', line)]
            install = cmd + ['-k', '-o', 'all', 'install', f'INSTALL_PATH={tests / target}',
                             f'SRC_PATH={source / "tools/testing/selftests"}', f'OBJ_PATH={tests}']
            try:
                install_rc = build_command(install, handle, 120)
                emitted = subprocess.run(cmd + ['-s', f'COLLECTION={target}', 'emit_tests'],
                                         capture_output=True, text=True, timeout=60)
                names = test_manifest(emitted.stdout)
                handle.write(emitted.stderr)
            except subprocess.TimeoutExpired:
                install_rc, names = 124, []
        available = [name for name in names if (tests / name.replace(':', '/', 1)).is_file()]
        row = {'id': target, 'command': cmd + ['-k', '-j', str(target_jobs), 'all'],
               'buildExit': build_rc, 'buildErrors': build_errors, 'installExit': install_rc, 'tests': names, 'available': available,
               'seconds': round(time.monotonic() - started, 2), 'log': log.name}
        settings = tests / target / 'settings'
        timeout = re.search(r'^timeout=(\d+)$', settings.read_text() if settings.exists() else '', re.M)
        row['upstreamTimeoutSeconds'] = int(timeout[1]) if timeout else 45
        row['programTimeoutSeconds'] = (row['upstreamTimeoutSeconds'] or 300) + 15
        row['collectionTimeoutSeconds'] = max(120, len(available) * (row['upstreamTimeoutSeconds'] or 300) + 60)
        metadata['builds'].append(row)
        save(work / 'build.json', metadata)
        print(f'build {len(metadata["builds"])}/{len(targets)} {target}: rc={build_rc}, '
              f'installed={len(available)}/{len(names)}', flush=True)
    for filename in ('run_kselftest.sh',):
        shutil.copy2(source / 'tools/testing/selftests' / filename, tests / filename)
    shutil.copytree(source / 'tools/testing/selftests/kselftest', tests / 'kselftest', dirs_exist_ok=True)
    # Shared network/Python helpers are an upstream installation dependency.
    shutil.copytree(source / 'tools/testing/selftests/net/lib', tests / 'net/lib', dirs_exist_ok=True)
    all_tests = [name for row in metadata['builds'] for name in row['available']]
    (tests / 'kselftest-list.txt').write_text('\n'.join(all_tests) + '\n')
    (tests / 'list').write_text(''.join(f'{b["id"]} {b["programTimeoutSeconds"]}\n'
                                       for b in metadata['builds'] if b['available']))
    (tests / 'huge-pages').write_text('1\n')
    # A runtime root, not another kernel tree or complete compiler installation.
    for part in ('usr/bin', 'usr/sbin', 'usr/lib/aarch64-linux-gnu', 'usr/lib/python3',
                 'usr/lib/python3.13', 'usr/lib/locale', 'usr/libexec', 'usr/share/perl5',
                 'usr/share/perl', 'usr/share/zoneinfo', 'etc'):
        if Path('/' + part).exists():
            dest = rootfs / part
            dest.mkdir(parents=True, exist_ok=True)
            subprocess.run(['rsync', '-a', '--exclude=*.a', '--exclude=*.o', '/' + part + '/',
                            str(dest) + '/'], check=True)
    for name, dest in (('bin', 'usr/bin'), ('sbin', 'usr/sbin'), ('lib', 'usr/lib')):
        if not (rootfs / name).is_symlink():
            (rootfs / name).symlink_to(dest)
    shutil.copy2('/usr/lib/ld-linux-aarch64.so.1', rootfs / 'usr/lib/ld-linux-aarch64.so.1', follow_symlinks=True)
    for part in ('dev', 'proc', 'sys', 'tmp', 'scratch', 'run', 'root', 'var/tmp', 'var/log'):
        (rootfs / part).mkdir(parents=True, exist_ok=True)
    subprocess.run(['gcc-14', '-static', '-O2', '-Wall', '-Wextra', '-Werror',
                    '/work/harness/selftest-init.c', '-o', str(work / 'init')], check=True)
    (work / 'full-suite').touch()
    (work / 'spec.txt').write_text('dir /dev 755 0 0\ndir /newroot 755 0 0\n'
        'nod /dev/console 600 0 0 c 5 1\nfile /init /work/full/init 755 0 0\n'
        'file /full-suite /work/full/full-suite 644 0 0\n')
    with (work / 'initramfs.cpio').open('wb') as handle:
        subprocess.run(['/work/bin/gen_init_cpio', '-t', '0', str(work / 'spec.txt')], stdout=handle, check=True)
    metadata['complete'] = True
    save(work / 'build.json', metadata)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--jobs', type=int, default=2)
    parser.add_argument('--build-worker', action='store_true', help=argparse.SUPPRESS)
    parser.add_argument('--build-only', action='store_true')
    parser.add_argument('--reuse-build', action='store_true')
    parser.add_argument('--reference-run', type=Path,
                        help='reuse a finished reference summary after checking the unchanged Image and payload; execute every subject program')
    parser.add_argument('--install-deps', action='store_true', help='install additional selftest dependencies only in the disposable build container')
    parser.add_argument('--allow-qemu-drift', action='store_true')
    args = parser.parse_args()
    if args.reference_run and not args.reuse_build:
        parser.error('--reference-run requires --reuse-build')
    if args.build_worker:
        return build_worker(args.jobs)
    import selftest
    if args.allow_qemu_drift:
        os.environ['K1_ALLOW_QEMU_DRIFT'] = '1'
    work = ROOT / '.build/kselftest-full'
    work.mkdir(exist_ok=True)
    kernel_path = ROOT / 'tests/contracts/kernel-full-receipt.json'
    kernel = selftest.read(kernel_path)
    subject = ROOT / kernel['boot']['image']
    if kernel['status'] != 'PASS' or not kernel['completeComposition'] or selftest.sha(subject) != kernel['imageSha256']:
        raise ValueError('unchanged complete booted kernel required')
    environment = selftest.read(ROOT / 'contracts/build-environment.json')['buildImage']
    image = environment['sealedImageTag']
    if not args.reuse_build:
        installed = json.loads(subprocess.check_output(['/usr/local/bin/container', 'image', 'inspect', image], text=True))
        if installed[0]['configuration']['descriptor']['digest'] != environment['executionReference']:
            raise ValueError('selftest seed image differs from the frozen build environment')
        cmd = ['/usr/local/bin/container', 'run', '--rm', '--platform', 'linux/arm64',
               '--cpus', str(args.jobs), '--memory', '4G',
               '-v', f'{ROOT / ".build/linux-src"}:/work/src:ro', '-v', f'{work}:/work/full',
               '-v', f'{ROOT / ".build/kselftest/headers"}:/work/headers:ro',
               '-v', f'{ROOT / "tools/k1"}:/work/harness:ro',
               '-v', f'{ROOT / ".build/reference/gcc-lane-1/usr/gen_init_cpio"}:/work/bin/gen_init_cpio:ro',
               *([] if args.install_deps else ['--network', 'none']), image]
        worker = f'exec python3 /work/harness/selftest_full.py --build-worker --jobs {args.jobs}'
        if args.install_deps:
            packages = ('pkgconf libcap-dev libnuma-dev libmnl-dev libasound2-dev libaio-dev liburing-dev '
                        'libnl-3-dev libnl-genl-3-dev libcap-ng-dev libunwind-dev libtraceevent-dev '
                        'libbpf-dev libpopt-dev libfuse-dev libfuse3-dev libjson-c-dev libpcap-dev libkeyutils-dev '
                        'iproute2 iputils-ping iptables nftables ethtool socat numactl bc strace jq '
                        'python3-pyroute2 python3-yaml python3-pytest python3-scapy python3-pexpect '
                        'gawk python3-jsonschema libmbedtls-dev')
            worker = ('apt-get -o Acquire::Check-Valid-Until=false update > /work/full/dependencies.log 2>&1 && '
                      'DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends '
                      + packages + ' >> /work/full/dependencies.log 2>&1 && ' + worker)
        cmd += ['sh', '-c', worker]
        subprocess.run(cmd, check=True)
        formatter = '/opt/homebrew/opt/e2fsprogs/sbin/mke2fs'
        subprocess.run([formatter, '-q', '-F', '-t', 'ext4', '-d', str(work / 'rootfs'),
                        '-E', 'lazy_itable_init=0,lazy_journal_init=0', str(work / 'scratch.img'), '2097152'], check=True)
    if args.build_only:
        return 0
    build = selftest.read(work / 'build.json')
    if ([row['id'] for row in build['builds']] != build['targets']
            or not all((work / name).is_file() for name in ('initramfs.cpio', 'scratch.img'))):
        raise ValueError('selftest build has not completed its enumeration and payload')
    run_id = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    run = ROOT / '.evidence/kselftest-full' / run_id
    run.mkdir(parents=True)
    shutil.copytree(work / 'logs', run / 'build')
    if (work / 'dependencies.log').exists():
        shutil.copy2(work / 'dependencies.log', run / 'dependencies.log')
    receipt_path = ROOT / 'tests/contracts/kernel-selftest-full-receipt.json'
    receipt = {'schemaVersion': 1, 'run': run_id, 'status': 'RUNNING',
               'scope': 'Every upstream TARGETS plus hotplug collection, SKIP_TARGETS empty; unchanged arm64 configuration',
               'evidence': str(run.relative_to(ROOT)), 'kernelImageSha256': selftest.sha(subject),
               'kernelReceiptSha256': selftest.sha(kernel_path), 'build': build,
               'kernelConfigSha256': selftest.sha(ROOT / '.build/k1/out/.config'),
               'referenceConfigSha256': selftest.sha(ROOT / '.build/reference/gcc-lane-1/.config'),
               'initramfsSha256': selftest.sha(work / 'initramfs.cpio'),
               'rootfsSha256': selftest.sha(work / 'scratch.img'), 'execution': {}}
    receipt['runtimePolicy'] = {'isolation': 'fresh VM per program', 'workers': min(args.jobs, 2),
                                'upstreamTimeouts': 'unchanged; outer watchdog adds 15 seconds cleanup and 60 seconds boot',
                                'unlimitedProgramWatchdogSeconds': 300,
                                'modules': 'not built or installed; frozen Images and configuration unchanged'}
    if receipt['kernelConfigSha256'] != receipt['referenceConfigSha256']:
        raise ValueError('reference and subject kernel configurations differ')
    def checkpoint():
        save(run / 'summary.json', receipt)
        save(receipt_path, receipt)
    checkpoint()
    images = {'reference': ROOT / '.build/reference/gcc-lane-1/arch/arm64/boot/Image', 'subject': subject}
    receipt['referenceImageSha256'] = selftest.sha(images['reference'])
    rows = {label: {b['id']: {'id': b['id'], 'cases': [], 'valid': False} for b in build['builds'] if b['available']}
            for label in images}
    for label in images:
        receipt['execution'][label] = {'collections': list(rows[label].values()),
                                       'summary': summarize(build['builds'], [])}

    def execute(label, b, number, ident):
        capture = label + '/' + ident.replace(':', '/')
        contract = {'consoleLoglevel': 5, 'hugePages': 1, 'bootOverheadSeconds': 60,
                    'perProgramTimeoutSeconds': b['programTimeoutSeconds']}
        try:
            result = selftest.flight(capture, images[label], work, run, contract, [ident],
                                    extra_env={'K1_SELFTEST_COLLECTION': ident})
            case = program_case(ident, result)
        except ValueError as error:
            case = {'id': ident, 'outcome': 'NOT_RUN', 'started': False, 'valid': False, 'error': str(error)}
        case.update(number=number, name=ident, capture=str((run / capture).relative_to(ROOT)))
        return label, b, case

    def record(label, b, case):
        row = rows[label][b['id']]
        row['cases'].append(case)
        row['cases'].sort(key=lambda c: c['number'])
        row['valid'] = len(row['cases']) == len(b['available']) and all(c['valid'] for c in row['cases'])
        receipt['execution'][label]['summary'] = summarize(build['builds'], list(rows[label].values()))
        checkpoint()

    active_images = list(images)
    if args.reference_run:
        previous = selftest.read(args.reference_run)
        cases = reusable_reference(previous, receipt)
        qemu = selftest.sha(Path(kernel['boot']['qemu']['path']))
        builds = {b['id']: b for b in build['builds']}
        for case in cases:
            boot = selftest.read(ROOT / case['capture'] / 'boot.json')
            if (boot['imageSha256'] != receipt['referenceImageSha256']
                    or boot['qemu']['sha256'] != qemu
                    or selftest.sha(Path(boot['serialLog'])) != boot['serialLogSha256']):
                raise ValueError('reference Image, QEMU or captured output changed')
            b = builds[case['id'].split(':', 1)[0]]
            rows['reference'][b['id']]['cases'].append(case)
        for row in rows['reference'].values():
            row['valid'] = all(case['valid'] for case in row['cases'])
        receipt['execution']['reference']['summary'] = summarize(build['builds'], list(rows['reference'].values()))
        receipt['referenceReuse'] = {'run': previous['run'], 'summary': str(args.reference_run),
                                     'summarySha256': selftest.sha(args.reference_run),
                                     'programs': len(cases), 'subjectResultsReused': 0}
        active_images = ['subject']
        checkpoint()
        print(f'Reusing {len(cases)} unchanged reference measurements from {previous["run"]}; every subject program will run', flush=True)

    tasks = [(label, b, number, ident) for b in build['builds']
             for number, ident in enumerate(b['available'], 1) for label in active_images]
    with ThreadPoolExecutor(max_workers=min(args.jobs, 2)) as pool:
        # A guest which cannot start the test runtime would repeat the same boot
        # failure hundreds of times. Measure it once and retain every NOT_RUN.
        bootstrap = list(pool.map(lambda task: execute(*task), tasks[:len(active_images)]))
        for result in bootstrap:
            record(*result)
        if any(not case['started'] for _, _, case in bootstrap):
            receipt.update(status='BLOCKED', blocker='Kernel could not initialize the selftest runtime; tests not reached',
                           bootstrap={label: case for label, _, case in bootstrap})
            checkpoint()
            print(receipt['blocker'], flush=True)
            return 0
        futures = [pool.submit(execute, *task) for task in tasks[len(active_images):]]
        for future in as_completed(futures):
            record(*future.result())
    receipt['status'] = ('PASS' if all(e['summary']['status'] == 'PASS' for e in receipt['execution'].values())
                         else 'INCOMPLETE')
    receipt['referenceStatus'] = receipt['execution']['reference']['summary']['status']
    previous = rows['reference']
    receipt['regressions'] = []
    for row in receipt['execution']['subject']['collections']:
        old = previous[row['id']]
        now = {c['number']: c['outcome'] for c in row['cases']}
        receipt['regressions'].extend(f'{row["id"]}:{c["name"]}' for c in old['cases']
                                      if c['outcome'] == 'PASS' and now.get(c['number']) != 'PASS')
    if (selftest.sha(subject) != receipt['kernelImageSha256']
            or selftest.sha(images['reference']) != receipt['referenceImageSha256']
            or selftest.sha(work / 'initramfs.cpio') != receipt['initramfsSha256']
            or selftest.sha(work / 'scratch.img') != receipt['rootfsSha256']):
        raise ValueError('test images changed during execution')
    checkpoint()
    print(f'full selftest receipt: {receipt_path}', flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
