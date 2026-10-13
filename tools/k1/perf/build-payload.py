#!/usr/bin/env python3
"""Build a selected-program selftest payload in the sealed image.

The steps follow tools/k1/selftest.py build_payload: the sealed image,
headers_install, upstream selftest Makefiles with gcc-14 and LDFLAGS=-static,
the existing selftest-init.c, gen_init_cpio and a 64 MiB ext4 scratch disk.
One difference: the KVM Makefile writes generated sysreg headers and try-run
probes into its own tree, so `tools/`, `arch/arm64/tools/` and
`scripts/subarch.include` are first copied into the work directory. A read-only tree would silently drop the
-no-pie probe. The pinned source tree itself is mounted read-only.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work', required=True, help='new work directory')
    parser.add_argument('--source-root', default=str(ROOT / '.build/case/kbuild-probe/src'))
    parser.add_argument('--program', action='append', required=True, help='e.g. kvm/demand_paging_test')
    parser.add_argument('--timeout', type=int, required=True, help='per-program limit, seconds')
    parser.add_argument('--huge-pages', type=int, default=1)
    parser.add_argument('--wrapper', action='append', default=[],
                        metavar='ID=PROGRAM_ID:ARG,ARG', help='fixed-argument diagnostic entry')
    parser.add_argument('--wrapper-timeout', type=int, help='per-wrapper and per-extra limit, seconds')
    parser.add_argument('--extra', action='append', default=[], metavar='ID=perf/FILE.c',
                        help='static diagnostic program from tools/k1, built with the UAPI headers')
    parser.add_argument('--bin', action='append', default=[], metavar='NAME=perf/FILE.c',
                        help='static helper installed as /bin/NAME (a runtime prerequisite)')
    args = parser.parse_args()
    extras = [tuple(item.split('=', 1)) for item in args.extra]
    bins = [tuple(item.split('=', 1)) for item in args.bin]
    wrappers = []
    for item in args.wrapper:
        ident, rest = item.split('=', 1)
        target, arguments = rest.split(':', 1)
        wrappers.append((ident, target, arguments.split(',')))
    if (wrappers or extras) and not args.wrapper_timeout:
        raise SystemExit('--wrapper and --extra need --wrapper-timeout')
    spec = importlib.util.spec_from_file_location('kernel_selftest', ROOT / 'tools/k1/selftest.py')
    lane = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(lane)
    work, source = Path(args.work).resolve(), Path(args.source_root).resolve()
    if work.exists():
        raise SystemExit('keep each payload; choose a new work directory')
    if shutil.disk_usage(ROOT).free < 25 << 30:
        raise SystemExit('less than 25 GiB free')
    environment = lane.read(ROOT / 'contracts/build-environment.json')
    image = environment['buildImage']['sealedImageTag']
    installed = json.loads(subprocess.check_output(
        ['/usr/local/bin/container', 'image', 'inspect', image], text=True))
    digest = installed[0]['configuration']['descriptor']['digest']
    if digest != environment['buildImage']['executionReference']:
        raise SystemExit('installed build image differs from the sealed image')
    ids = [name.replace('/', '.') for name in args.program]
    (work / 'root').mkdir(parents=True)
    (work / 'root/list').write_text(''.join(f'{name} {args.timeout}\n' for name in ids)
                                    + ''.join(f'{w[0]} {args.wrapper_timeout}\n' for w in wrappers)
                                    + ''.join(f'{e[0]} {args.wrapper_timeout}\n' for e in extras))
    (work / 'root/huge-pages').write_text(f'{args.huge_pages}\n')
    spec_lines = ['dir /dev 755 0 0', 'dir /proc 755 0 0', 'dir /sys 755 0 0',
                  'dir /tmp 1777 0 0', 'dir /scratch 755 0 0', 'dir /tests 755 0 0',
                  'nod /dev/console 600 0 0 c 5 1',
                  'file /init /work/selftest/root/init 755 0 0',
                  'file /tests/huge-pages /work/selftest/root/huge-pages 644 0 0',
                  'file /tests/list /work/selftest/root/list 644 0 0']
    spec_lines += [f'file /tests/{name} /work/selftest/root/{name} 755 0 0'
                   for name in ids + [w[0] for w in wrappers] + [e[0] for e in extras]]
    if bins:
        spec_lines.append('dir /bin 755 0 0')
        spec_lines += [f'file /bin/{name} /work/selftest/root/bin-{name} 755 0 0' for name, _ in bins]
    (work / 'spec.txt').write_text('\n'.join(spec_lines) + '\n')
    commands = [['mkdir', '-p', '/work/selftest/srcw/arch/arm64', '/work/selftest/srcw/scripts'],
                ['cp', '-a', '/work/src/scripts/subarch.include', '/work/selftest/srcw/scripts/'],
                ['cp', '-a', '/work/src/tools', '/work/selftest/srcw/tools'],
                ['cp', '-a', '/work/src/arch/arm64/tools', '/work/selftest/srcw/arch/arm64/tools'],
                ['make', '-s', '-C', '/work/src', 'O=/work/selftest/headers-build', 'ARCH=arm64',
                 'HOSTCC=gcc-14', 'headers_install', 'INSTALL_HDR_PATH=/work/selftest/headers']]
    for name, ident in zip(args.program, ids):
        directory, binary = name.rsplit('/', 1)
        output = f'/work/selftest/build/{directory}'
        (work / 'build' / directory).mkdir(parents=True, exist_ok=True)
        commands.append(['make', '-s', '-C', f'/work/selftest/srcw/tools/testing/selftests/{directory}',
                         f'OUTPUT={output}', 'ARCH=arm64', 'CC=gcc-14',
                         'KHDR_INCLUDES=-isystem /work/selftest/headers/include',
                         'LDFLAGS=-static', f'{output}/{binary}'])
        commands.append(['cp', f'{output}/{binary}', f'/work/selftest/root/{ident}'])
    for ident, target, arguments in wrappers:
        quoted = ', '.join('"' + a + '"' for a in arguments)
        commands.append(['gcc-14', '-static', '-O2', '-Wall', '-Wextra', '-Werror',
                         f'-DPROGRAM="/tests/{target}"', f'-DARGUMENTS={quoted}',
                         '/work/harness/perf/exec-args.c', '-o', f'/work/selftest/root/{ident}'])
    for name, path in bins:
        commands.append(['gcc-14', '-static', '-O2', '-Wall', '-Wextra', '-Werror',
                         f'/work/harness/{path}', '-o', f'/work/selftest/root/bin-{name}'])
    for ident, path in extras:
        commands.append(['gcc-14', '-static', '-O2', '-Wall', '-Wextra', '-Werror',
                         '-isystem', '/work/selftest/headers/include', f'/work/harness/{path}',
                         '-o', f'/work/selftest/root/{ident}'])
    commands.append(['gcc-14', '-static', '-O2', '-Wall', '-Wextra', '-Werror',
                     '/work/harness/selftest-init.c', '-o', '/work/selftest/root/init'])
    shell = 'set -eu\n' + '\n'.join(shlex.join(c) for c in commands)
    shell += f'\n/work/bin/gen_init_cpio -t {environment["reproducibility"]["SOURCE_DATE_EPOCH"]}'
    shell += ' /work/selftest/spec.txt > /work/selftest/initramfs.cpio\n'
    cmd = ['/usr/local/bin/container', 'run', '--rm', '--platform', 'linux/arm64', '--network', 'none',
           '-v', f'{source}:/work/src:ro', '-v', f'{work}:/work/selftest',
           '-v', f'{ROOT / "tools/k1"}:/work/harness:ro',
           '-v', f'{ROOT / ".build/reference/gcc-lane-1/usr/gen_init_cpio"}:/work/bin/gen_init_cpio:ro',
           image, 'sh', '-c', shell]
    with (work / 'build.log').open('w') as log:
        subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=1800)
        formatter = Path('/opt/homebrew/opt/e2fsprogs/sbin/mke2fs')
        format_command = [str(formatter), '-q', '-F', '-t', 'ext4', '-b', '1024',
                          '-E', 'lazy_itable_init=0,lazy_journal_init=0',
                          str(work / 'scratch.img'), '65536']
        subprocess.run(format_command, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=30)
    record = {'compiler': 'gcc-14', 'image': digest, 'commands': commands, 'container': cmd,
              'initSourceSha256': lane.sha(ROOT / 'tools/k1/selftest-init.c'),
              'filesystemCommand': format_command, 'filesystemToolSha256': lane.sha(formatter),
              'initramfsSha256': lane.sha(work / 'initramfs.cpio'),
              'scratchSha256': lane.sha(work / 'scratch.img'), 'sourceRoot': str(source),
              'perProgramTimeoutSeconds': args.timeout, 'hugePages': args.huge_pages,
              'wrappers': [{'id': i, 'program': t, 'arguments': a, 'timeoutSeconds': args.wrapper_timeout,
                            'binarySha256': lane.sha(work / 'root' / i)} for i, t, a in wrappers],
              'extras': [{'id': i, 'source': f'tools/k1/{path}', 'sourceSha256': lane.sha(ROOT / 'tools/k1' / path),
                          'timeoutSeconds': args.wrapper_timeout, 'binarySha256': lane.sha(work / 'root' / i)}
                         for i, path in extras],
              'bins': [{'path': f'/bin/{name}', 'source': f'tools/k1/{path}',
                        'sourceSha256': lane.sha(ROOT / 'tools/k1' / path),
                        'binarySha256': lane.sha(work / 'root' / f'bin-{name}')} for name, path in bins],
              'programs': [{'id': ident, 'source': f'tools/testing/selftests/{name}.c',
                            'sourceSha256': lane.sha(source / f'tools/testing/selftests/{name}.c'),
                            'binarySha256': lane.sha(work / 'root' / ident)}
                           for name, ident in zip(args.program, ids)]}
    (work / 'payload.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps({'initramfs': record['initramfsSha256'], 'scratch': record['scratchSha256'],
                      'programs': [(p['id'], p['binarySha256']) for p in record['programs']]}, indent=2))


if __name__ == '__main__':
    main()
