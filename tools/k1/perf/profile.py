#!/usr/bin/env python3
"""Boot one existing Image under a QEMU TCG lane with the tbprof plugin.

The QEMU argv matches tools/k1/lib.sh k1_boot. Two changes are diagnostic:
the plugin is loaded, and `nokaslr` keeps the kernel at its System.map
addresses. The plugin reads guest code only. This is a measurement of
executed guest instructions, not a boot or selftest acceptance verdict.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
TIMEOUT = '/opt/homebrew/bin/timeout'


def sha(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def symbol_address(system_map, name):
    found = [line.split()[0] for line in Path(system_map).read_text().splitlines()
             if line.split()[2:3] == [name] and line.split()[1] in 'tTwW']
    if len(found) != 1:
        raise SystemExit(f'{name}: expected one text symbol, found {len(found)}')
    return found[0]


def predicate(log, nonce):
    """Return the k1_boot verdict word for a completed QEMU run."""
    text = log.read_bytes().decode('latin-1')
    alive, mark, done = (text.find(s) for s in ('EVO_INIT_ALIVE', f'EVO_BOOT_NONCE {nonce}', 'EVO_INIT_DONE'))
    if min(alive, mark, done) < 0:
        return 'MARKERS_MISSING'
    if not alive < mark < done:
        return 'MARKERS_OUT_OF_ORDER'
    if text.count(f'EVO_BOOT_NONCE {nonce}') != 1:
        return f'NONCE_COUNT_{text.count(f"EVO_BOOT_NONCE {nonce}")}'
    return 'PASS' if 'reboot: Power down' in text else 'NO_POWERDOWN'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', required=True)
    parser.add_argument('--system-map', required=True)
    parser.add_argument('--initramfs', required=True)
    parser.add_argument('--disk')
    parser.add_argument('--contract', default=str(ROOT / 'contracts/kernel-selftest-kvm-target.json'))
    parser.add_argument('--lane', default='tcg-neoverse-n2-el2')
    parser.add_argument('--selftest', default='')
    parser.add_argument('--mark', action='append', default=[], help='kernel symbol for a snapshot')
    parser.add_argument('--plugin', required=True)
    parser.add_argument('--timeout', type=int, default=240)
    parser.add_argument('--smp', type=int, help='diagnostic override of the contract CPU count')
    parser.add_argument('--plugin-arg', action='append', default=[], help='extra tbprof argument')
    parser.add_argument('--out', required=True)
    args = parser.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=False)
    contract = json.loads(Path(args.contract).read_text())
    qemu = contract['qemu']
    lanes = [lane['argv'] for lane in qemu['lanes'] if lane['name'] == args.lane]
    if len(lanes) != 1:
        raise SystemExit(f'unknown lane {args.lane}')
    marks = {name: symbol_address(args.system_map, name) for name in args.mark}
    nonce = f'k1-{time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())}-{secrets.token_hex(4)}'
    cmdline = f'console=ttyAMA0 panic=-1 evo_nonce={nonce}'
    if args.selftest:
        if not re.fullmatch(r'[A-Za-z0-9_.:+-]+', args.selftest):
            raise SystemExit('invalid selftest id')
        cmdline += f' evo_selftest={args.selftest}'
    cmdline += ' kunit.autorun=0 nokaslr'
    plugin = ','.join([args.plugin, f'out={out / "tbprof"}'] + [f'mark={value}' for value in marks.values()]
                      + args.plugin_arg)
    smp = args.smp or qemu['fixedArgv']['smp']
    argv = [TIMEOUT, '--foreground', str(args.timeout), qemu['binaryPath'],
            '-M', f'{qemu["machine"]},dtb-randomness=off', *lanes[0],
            '-smp', str(smp), '-m', str(qemu['fixedArgv']['memoryMiB']),
            '-kernel', args.image, '-initrd', args.initramfs]
    if args.disk:
        argv += ['-drive', f'file={args.disk},format=raw,if=none,id=selftest,snapshot=on',
                 '-device', 'virtio-blk-device,drive=selftest']
    argv += ['-append', cmdline, '-nographic', '-no-reboot',
             '-plugin', plugin, '-d', 'plugin', '-D', str(out / 'plugin.log')]
    inputs = {'image': args.image, 'systemMap': args.system_map, 'initramfs': args.initramfs,
              'plugin': args.plugin, 'qemu': os.path.realpath(qemu['binaryPath'])}
    if args.disk:
        inputs['disk'] = args.disk
    before = {key: sha(path) for key, path in inputs.items()}
    record = {'argv': argv, 'nonce': nonce, 'lane': args.lane, 'marks': marks, 'smp': smp,
              'smpMatchesContract': smp == qemu['fixedArgv']['smp'],
              'inputsBefore': before, 'contractQemuSha256': qemu['binarySha256'],
              'qemuMatchesContract': before['qemu'] == qemu['binarySha256'],
              'uptimeBefore': subprocess.run(['uptime'], capture_output=True, text=True).stdout.strip()}
    log = out / 'boot.log'
    started = time.monotonic()
    with log.open('wb') as handle:
        rc = subprocess.run(argv, stdout=handle, stderr=subprocess.STDOUT, check=False).returncode
    record['elapsedSeconds'] = round(time.monotonic() - started, 3)
    record['uptimeAfter'] = subprocess.run(['uptime'], capture_output=True, text=True).stdout.strip()
    record['qemuExit'] = rc
    record['verdict'] = 'TIMEOUT' if rc == 124 else f'QEMU_EXIT_{rc}' if rc else predicate(log, nonce)
    text = log.read_bytes().decode('latin-1')
    record['selftestEnds'] = re.findall(rf'KSELFTEST_END {re.escape(nonce)} (\S+) (\d+)', text)
    record['inputsAfter'] = {key: sha(path) for key, path in inputs.items()}
    record['inputsUnchanged'] = record['inputsAfter'] == before
    record['outputs'] = {path.name: sha(path) for path in sorted(out.iterdir()) if path.is_file()}
    (out / 'result.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps({key: record[key] for key in ('verdict', 'qemuExit', 'elapsedSeconds',
                                                   'selftestEnds', 'inputsUnchanged')}))
    return 0 if record['verdict'] == 'PASS' and record['inputsUnchanged'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
