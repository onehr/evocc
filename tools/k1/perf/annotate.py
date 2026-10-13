#!/usr/bin/env python3
"""Print one function's disassembly with tbprof execution counts per instruction.

Counts come from the selected snapshots of one run. All copies and GCC clones
of the named function are printed. Disassembly uses the host llvm-objdump on
the matching vmlinux, so the listing is the linked code before boot-time
patching; the count column shows the code the guest actually entered.
"""

from __future__ import annotations

import argparse
from collections import Counter
import importlib.util
from pathlib import Path
import subprocess

OBJDUMP = '/opt/homebrew/opt/llvm/bin/llvm-objdump'
spec = importlib.util.spec_from_file_location('attribute', Path(__file__).with_name('attribute.py'))
attribute = importlib.util.module_from_spec(spec)
spec.loader.exec_module(attribute)


def counts(run, seqs):
    per_address = Counter()
    for snap in run['snaps']:
        if seqs is None or snap['seq'] in seqs:
            for index, count in snap['counts'].items():
                pc, words = run['blocks'][index]
                for offset in range(len(words)):
                    per_address[pc + 4 * offset] += count
    return per_address


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run')
    parser.add_argument('vmlinux')
    parser.add_argument('function')
    parser.add_argument('--seqs', default='all')
    parser.add_argument('--min-count', type=int, default=0)
    args = parser.parse_args()
    run = attribute.load_run(args.run)
    seqs = None if args.seqs == 'all' else {int(s) for s in args.seqs.split(',')}
    executed = counts(run, seqs)
    symbols = attribute.Symbols(args.vmlinux)
    ranges = [(lo, hi, name) for lo, hi, name in symbols.funcs
              if attribute.base_name(name) == args.function]
    if not ranges:
        raise SystemExit(f'{args.function}: no FUNC symbol')
    for lo, hi, name in ranges:
        total = sum(executed[a] for a in range(lo, hi, 4))
        entries = executed[lo]
        print(f'== {name} {lo:#x}-{hi:#x} bytes {hi - lo} executed-insns {total:,} entries {entries:,}')
        if not total:
            continue
        text = subprocess.run([OBJDUMP, '-d', '--no-show-raw-insn', f'--start-address={lo:#x}',
                               f'--stop-address={hi:#x}', args.vmlinux],
                              capture_output=True, text=True, check=True).stdout
        for line in text.splitlines():
            head = line.strip().split(':', 1)
            try:
                address = int(head[0], 16)
            except ValueError:
                continue
            if len(head) == 2 and executed[address] >= args.min_count:
                print(f'{executed[address]:>12,}  {line.strip()}')


if __name__ == '__main__':
    main()
