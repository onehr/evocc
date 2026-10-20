#!/usr/bin/env python3
"""Count executed instructions of C reductions built by EvoCC and GCC 14.

Each reduction file holds the measured function(s) and, under
REDUCTION_DRIVER, a GCC-built main that calls them and prints a checksum.
The measured part is compiled once by frozen EvoCC (host, --target) and
once by gcc-14 in the sealed image, both with compare_asm.KERNEL_FLAGS.
Each part is linked statically with the same driver object, booted as
/init's single program on one Image under the tbprof plugin (one CPU), and
the executed instructions inside each measured symbol are summed. The two
checksums must match.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import re
import shlex
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[3]
NM = '/opt/homebrew/opt/llvm/bin/llvm-nm'
HERE = Path(__file__).parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


compare = load('compare_asm', HERE / 'compare_asm.py')
attribute = load('attribute', HERE / 'attribute.py')


def container(work, script, sources):
    cmd = ['/usr/local/bin/container', 'run', '--rm', '--platform', 'linux/arm64', '--network', 'none',
           '-v', f'{sources}:/src:ro', '-v', f'{work}:/work', '-v', f'{ROOT / "tools/k1"}:/harness:ro',
           '-v', f'{ROOT / ".build/reference/gcc-lane-1/usr/gen_init_cpio"}:/tools/gen_init_cpio:ro',
           compare.IMAGE, 'sh', '-c', 'set -eu\n' + script]
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL)


def symbols(binary, names):
    text = subprocess.run([NM, '-S', '--defined-only', str(binary)], capture_output=True,
                          text=True, check=True).stdout
    found = {}
    for line in text.splitlines():
        parts = line.split()
        if len(parts) == 4 and parts[3] in names:
            found[parts[3]] = (int(parts[0], 16), int(parts[0], 16) + int(parts[1], 16))
    return found


def executable_segments(path):
    """Read the load ranges used by this static AArch64 profiling lane."""
    data = path.read_bytes()
    if len(data) < 64 or data[:7] != b'\x7fELF\x02\x01\x01':
        raise ValueError('profiling requires ELF64 little-endian')
    kind, machine = struct.unpack_from('<HH', data, 16)
    offset, = struct.unpack_from('<Q', data, 32)
    stride, count = struct.unpack_from('<HH', data, 54)
    if (kind != 2 or machine != 183 or stride != 56 or not count
            or offset + count * stride > len(data)):
        raise ValueError('profiling requires a static AArch64 executable')
    ranges = []
    for i in range(count):
        ty, flags, _, address, _, _, size, _ = struct.unpack_from('<IIQQQQQQ', data, offset + i*stride)
        if ty == 1 and flags & 1 and size:
            ranges.append((address, address + size))
    if not ranges:
        raise ValueError('executable has no code segment')
    return ranges


def require_disjoint_code(binary, init):
    # tbprof has code words but no process identity. Normal static binaries
    # share 0x400000 with /init, so a PC-only sum includes unrelated init code.
    for lo, hi in executable_segments(binary):
        for other_lo, other_hi in executable_segments(init):
            if lo < other_hi and other_lo < hi:
                raise ValueError('reduction and init executable addresses overlap')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', action='append', required=True, metavar='FILE.c:FUNC[,FUNC]')
    parser.add_argument('--work', required=True)
    parser.add_argument('--image', default=str(ROOT / '.build/reference/gcc-lane-1/arch/arm64/boot/Image'))
    parser.add_argument('--plugin', required=True)
    args = parser.parse_args()
    work = Path(args.work).resolve()
    work.mkdir(parents=True, exist_ok=False)
    jobs, script = [], []
    for item in args.source:
        path, names = item.split(':')
        source = Path(path).resolve()
        stem = source.stem
        subprocess.run([str(compare.EVOCC), '--target=aarch64-unknown-linux-gnu',
                        f'--sysroot={compare.SYSROOT}', *compare.KERNEL_FLAGS, '-w', '-c', str(source),
                        '-o', str(work / f'{stem}.evocc.o')], check=True)
        flags = shlex.join(compare.KERNEL_FLAGS + compare.GCC_ONLY)
        script += [f'gcc-14 {flags} -c /src/{source.name} -o /work/{stem}.gcc.o',
                   f'gcc-14 -O2 -fno-PIE -DREDUCTION_DRIVER -c /src/{source.name} -o /work/{stem}.driver.o']
        for compiler in ('evocc', 'gcc'):
            script.append(f'gcc-14 -static -Wl,-Ttext-segment=0x10000000 /work/{stem}.{compiler}.o /work/{stem}.driver.o '
                          f'-o /work/root/{stem}.{compiler}')
            jobs.append((stem, compiler, names.split(','), source))
    (work / 'root').mkdir()
    ids = [f'{stem}.{compiler}' for stem, compiler, _, _ in jobs]
    (work / 'root/list').write_text(''.join(f'{i} 60\n' for i in ids))
    (work / 'root/huge-pages').write_text('1\n')
    spec = ['dir /dev 755 0 0', 'dir /proc 755 0 0', 'dir /sys 755 0 0', 'dir /tmp 1777 0 0',
            'dir /scratch 755 0 0', 'dir /tests 755 0 0', 'nod /dev/console 600 0 0 c 5 1',
            'file /init /work/root/init 755 0 0', 'file /tests/huge-pages /work/root/huge-pages 644 0 0',
            'file /tests/list /work/root/list 644 0 0']
    spec += [f'file /tests/{i} /work/root/{i} 755 0 0' for i in ids]
    (work / 'spec.txt').write_text('\n'.join(spec) + '\n')
    script += ['gcc-14 -static -O2 -Wall -Wextra -Werror /harness/selftest-init.c -o /work/root/init',
               '/tools/gen_init_cpio -t 0 /work/spec.txt > /work/initramfs.cpio']
    sources = {job[3].parent for job in jobs}
    if len(sources) != 1:
        raise SystemExit('keep all reductions in one directory')
    container(work, '\n'.join(script), sources.pop())
    for ident in ids:
        require_disjoint_code(work / 'root' / ident, work / 'root/init')
    subprocess.run(['/opt/homebrew/opt/e2fsprogs/sbin/mke2fs', '-q', '-F', '-t', 'ext4', '-b', '1024',
                    str(work / 'scratch.img'), '8192'], check=True)
    system_map = Path(args.image).parents[3] / 'System.map'
    results = []
    for (stem, compiler, names, _), ident in zip(jobs, ids):
        run = work / f'run-{ident}'
        subprocess.run(['python3', str(HERE / 'profile.py'), '--image', args.image,
                        '--system-map', str(system_map), '--initramfs', str(work / 'initramfs.cpio'),
                        '--disk', str(work / 'scratch.img'), '--selftest', ident, '--smp', '1',
                        '--plugin', args.plugin, '--plugin-arg', 'discon=off', '--timeout', '300',
                        '--out', str(run)], check=True, stdout=subprocess.DEVNULL)
        profile = attribute.load_run(run)
        ranges = symbols(work / 'root' / ident, set(names))
        executed = {name: 0 for name in names}
        entries = {name: 0 for name in names}
        binary = (work / 'root' / ident).read_bytes()
        for snap in profile['snaps']:
            for index, count in snap['counts'].items():
                pc, words = profile['blocks'][index]
                for name, (lo, hi) in ranges.items():
                    for offset in range(len(words)):
                        if lo <= pc + 4 * offset < hi:
                            executed[name] += count
                    if pc == lo:
                        entries[name] += count
        text = (run / 'boot.log').read_bytes().decode('latin-1')
        checksum = re.findall(r'REDUCTION_CHECKSUM (\S+)', text)
        results.append({'reduction': stem, 'compiler': compiler, 'executed': executed,
                        'calls': entries, 'checksum': checksum, 'binaryBytes': len(binary),
                        'ranges': {k: [hex(v[0]), v[1] - v[0]] for k, v in ranges.items()}})
        print(json.dumps(results[-1]), flush=True)
    (work / 'counts.json').write_text(json.dumps(results, indent=2) + '\n')


if __name__ == '__main__':
    main()
