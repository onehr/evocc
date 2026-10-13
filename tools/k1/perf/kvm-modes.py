#!/usr/bin/env python3
"""Run original KVM selftests through the unchanged tools/k1/boot.sh.

Each flight boots one Image on the fixed Neoverse-N2 EL2 lane with the given
payload and one program, no arguments and the payload's per-program limit.
The summary counts guest modes started ("Testing guest mode") and modes
completed ("All vCPU threads joined"). No plugin and no kernel argument is
added; QEMU drift must be accepted explicitly with --allow-qemu-drift.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]


def sha(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', action='append', required=True, metavar='SIDE=IMAGE')
    parser.add_argument('--program', action='append', required=True, help='e.g. kvm.demand_paging_test')
    parser.add_argument('--payload', required=True, help='directory with initramfs.cpio and scratch.img')
    parser.add_argument('--out', required=True)
    parser.add_argument('--allow-qemu-drift', action='store_true')
    args = parser.parse_args()
    out, payload = Path(args.out), Path(args.payload)
    out.mkdir(parents=True, exist_ok=False)
    images = dict(item.split('=', 1) for item in args.image)
    protected = {str(p): sha(p) for p in [*images.values(), payload / 'initramfs.cpio', payload / 'scratch.img']}
    rows = []
    for program in args.program:
        for side, image in images.items():
            run = out / f'{side}-{program}'
            env = {**os.environ, 'K1_KERNEL_CONTRACT': str(ROOT / 'contracts/kernel-selftest-kvm-target.json'),
                   'K1_BOOT_LANE': 'tcg-neoverse-n2-el2', 'K1_KUNIT_AUTORUN': '0',
                   'K1_INITRAMFS': str(payload / 'initramfs.cpio'), 'K1_TEST_DISK': str(payload / 'scratch.img'),
                   'K1_SELFTEST_COLLECTION': program, 'K1_BOOT_TIMEOUT': '240'}
            if args.allow_qemu_drift:
                env['K1_ALLOW_QEMU_DRIFT'] = '1'
            uptime = subprocess.run(['uptime'], capture_output=True, text=True).stdout.strip()
            done = subprocess.run([str(ROOT / 'tools/k1/boot.sh'), image, str(run)], env=env,
                                  capture_output=True, text=True)
            boot = json.loads((run / 'boot.json').read_text())
            text = Path(boot['serialLog']).read_bytes().decode('latin-1')
            end = re.findall(rf'KSELFTEST_END {re.escape(boot["nonce"])} {re.escape(program)} (\d+)', text)
            rows.append({'side': side, 'program': program, 'bootExit': done.returncode,
                         'testExit': int(end[0]) if len(end) == 1 else None,
                         'modesStarted': text.count('Testing guest mode:'),
                         'modesCompleted': text.count('All vCPU threads joined'),
                         'modes': re.findall(r'Testing guest mode: ([^\n]+)', text),
                         'uptimeBefore': uptime, 'boot': boot})
            print(json.dumps({k: rows[-1][k] for k in ('side', 'program', 'testExit', 'modesStarted',
                                                       'modesCompleted')}), flush=True)
    after = {path: sha(path) for path in protected}
    record = {'rows': rows, 'protectedBefore': protected, 'protectedAfter': after,
              'inputsUnchanged': after == protected, 'payload': str(payload)}
    (out / 'runtime.json').write_text(json.dumps(record, indent=2) + '\n')
    return 0 if record['inputsUnchanged'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
