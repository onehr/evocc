#!/usr/bin/env python3
"""Attribute tbprof block counts to kernel functions and compare two kernels.

Each executed block is split into its 4-byte instructions. Kernel addresses
are mapped to the vmlinux FUNC symbol that contains them; other kernel text
falls back to the preceding symbol (marked asm:). Addresses below the kernel
half are one bucket for L1 userspace and the KVM guest. GCC clone suffixes
(.isra, .part, .constprop, .cold) are folded into the base name, and copies
of one static function in several objects are summed by name.
"""

from __future__ import annotations

import argparse
import bisect
from collections import Counter, defaultdict
import json
from pathlib import Path
import re
import subprocess

READELF = '/opt/homebrew/opt/llvm/bin/llvm-readelf'
SYMBOLIZER = '/opt/homebrew/opt/llvm/bin/llvm-symbolizer'
KERNEL = 1 << 63
LOW = '[user+guest]'
CLASSES = ('insn', 'mem', 'frame', 'load', 'store', 'call', 'ret', 'branch', 'pac', 'bti',
           'barrier', 'sysreg', 'atomic', 'move')
PAC_HINTS = {0xd503233f, 0xd50323bf, 0xd503237f, 0xd50323ff, 0xd503231f, 0xd503239f,
             0xd503235f, 0xd50323df, 0xd503211f, 0xd503219f, 0xd503215f, 0xd50321df,
             0xd50320ff, 0xd65f0bff, 0xd65f0fff}


def classify(word):
    """Return the instruction classes of one A64 word (approximate decode)."""
    found = ['insn']
    if word & 0x0a000000 == 0x08000000:
        found.append('mem')
        if (word >> 5) & 31 in (29, 31):
            found.append('frame')
        if word & 0x3b000000 == 0x39000000 or word & 0x3b200000 in (0x38000000, 0x38200000):
            found.append('load' if (word >> 22) & 3 else 'store')
        else:
            found.append('load' if (word >> 22) & 1 else 'store')
        if (word & 0x3f800000 == 0x08000000 or word & 0x3b200c00 == 0x38200000
                or word & 0x3fa07c00 == 0x08a07c00):
            found.append('atomic')
    if word & 0xfc000000 == 0x94000000 or word & 0xfffffc1f == 0xd63f0000 or word & 0xfefff800 == 0xd63f0800:
        found.append('call')
    if word & 0xfffffc1f == 0xd65f0000 or word in (0xd65f0bff, 0xd65f0fff):
        found.append('ret')
    if (word & 0x7c000000 == 0x14000000 or word & 0xff000010 == 0x54000000
            or word & 0x7e000000 in (0x34000000, 0x36000000) or word & 0xfffffc1f == 0xd61f0000):
        found.append('branch')
    if word in PAC_HINTS or word & 0xffffc000 == 0xdac10000:
        found.append('pac')
    if word & 0xffffff3f == 0xd503241f:
        found.append('bti')
    if word & 0xfffff0ff in (0xd50330bf, 0xd503309f, 0xd50330df):
        found.append('barrier')
    if (word & 0xffd00000 in (0xd5300000, 0xd5100000) or word & 0xfff8f01f == 0xd500401f
            or word & 0xfff80000 == 0xd5080000):
        found.append('sysreg')
    if word & 0x7fe0ffe0 == 0x2a0003e0:
        found.append('move')
    return found


def base_name(name):
    return re.sub(r'\.(isra|part|constprop|cold|lto_priv)(\.\d+)?', '', name)


class Symbols:
    def __init__(self, vmlinux):
        text = subprocess.run([READELF, '-s', '-W', vmlinux], capture_output=True, text=True,
                              check=True).stdout
        funcs, labels = [], []
        for line in text.splitlines():
            parts = line.split()
            if (len(parts) < 8 or not parts[0][:-1].isdigit() or not parts[0].endswith(':')
                    or parts[6] in ('UND', 'ABS')):
                continue
            address, size, kind = int(parts[1], 16), int(parts[2]), parts[3]
            if address < KERNEL:
                continue
            if kind == 'FUNC' and size:
                funcs.append((address, address + size, parts[7]))
            elif kind in ('NOTYPE', 'FUNC') and not parts[7].startswith('$'):
                labels.append((address, parts[7]))
        funcs.sort()
        labels.sort()
        self.starts = [f[0] for f in funcs]
        self.funcs = funcs
        self.label_starts = [label[0] for label in labels]
        self.labels = labels
        self.entries = {f[0]: f[2] for f in funcs}

    def name(self, address):
        if address < KERNEL:
            return LOW
        index = bisect.bisect_right(self.starts, address) - 1
        if index >= 0 and address < self.funcs[index][1]:
            return base_name(self.funcs[index][2])
        index = bisect.bisect_right(self.label_starts, address) - 1
        return 'asm:' + (self.labels[index][1] if index >= 0 else '?')


def load_run(directory):
    blocks = {}
    for line in (Path(directory) / 'tbprof.blocks').read_text().splitlines():
        if line.startswith('#'):
            header = line
            continue
        parts = line.split()
        blocks[int(parts[0])] = (int(parts[1], 16), [int(w, 16) for w in parts[3:]])
    snaps, current = [], None
    with (Path(directory) / 'tbprof.snaps').open() as handle:
        for line in handle:
            if line.startswith('S '):
                _, seq, label, vcpu, stamp, count = line.split()
                current = {'seq': int(seq), 'label': label, 'time': float(stamp), 'counts': {}}
            elif line.startswith('E'):
                snaps.append(current)
            else:
                index, delta = line.split()
                current['counts'][int(index)] = int(delta)
    return {'header': header, 'blocks': blocks, 'snaps': snaps}


def phases(run):
    """Name each snapshot interval by the marker that closes it."""
    rows, start = [], 0.0
    for snap in run['snaps']:
        insns = sum(len(run['blocks'][i][1]) * c for i, c in snap['counts'].items())
        kernel = sum(len(run['blocks'][i][1]) * c for i, c in snap['counts'].items()
                     if run['blocks'][i][0] >= KERNEL)
        rows.append({'seq': snap['seq'], 'closedBy': snap['label'], 'insns': insns,
                     'kernelInsns': kernel, 'seconds': round(snap['time'] - start, 3) if start else None})
        start = snap['time']
    return rows


def executed_addresses(run, seqs):
    found = set()
    for snap in run['snaps']:
        if snap['seq'] in seqs:
            for index in snap['counts']:
                pc, words = run['blocks'][index]
                if pc >= KERNEL:
                    found.update(pc + 4 * i for i in range(len(words)))
    return found


def inline_resolver(vmlinux, addresses, wanted, symbols):
    """Map each address to the innermost DWARF inline frame named in `wanted`.

    GCC folds helpers into callers; EvoCC keeps many of them out of line.
    Attributing GCC code to the innermost frame that EvoCC also executed as a
    function compares like with like. Other code keeps its symbol name.
    """
    ordered = sorted(addresses)
    text = subprocess.run([SYMBOLIZER, f'--obj={vmlinux}', '--inlining', '--functions=linkage'],
                          input=''.join(f'{a:#x}\n' for a in ordered), capture_output=True,
                          text=True, check=True).stdout
    chains = [block.splitlines()[0::2] for block in text.split('\n\n') if block.strip()]
    if len(chains) != len(ordered):
        raise SystemExit(f'symbolizer returned {len(chains)} chains for {len(ordered)} addresses')
    resolved = {}
    for address, chain in zip(ordered, chains):
        names = [base_name(name) for name in chain if name != '??']
        resolved[address] = next((name for name in names if name in wanted), symbols.name(address))
    return resolved


def profile(run, symbols, seqs, resolved=None):
    """Per-function class counts and entry counts over the selected snapshots."""
    totals = defaultdict(Counter)
    entries = Counter()
    cache = {}
    for snap in run['snaps']:
        if snap['seq'] not in seqs:
            continue
        for index, count in snap['counts'].items():
            pc, words = run['blocks'][index]
            if pc in symbols.entries:
                entries[base_name(symbols.entries[pc])] += count
            for offset, word in enumerate(words):
                address = pc + 4 * offset
                if address < KERNEL:
                    name = LOW
                elif resolved is not None:
                    name = resolved[address]
                else:
                    name = symbols.name(address)
                key = (name, word)
                if key not in cache:
                    cache[key] = classify(word)
                for kind in cache[key]:
                    totals[name][kind] += count
    return totals, entries


def transport_runs(run, seqs, minimum=6):
    """Kernel instructions inside runs of frame loads/stores and register moves.

    A run ends at a block boundary, so this is a lower bound for spill and
    block-argument transport sequences. Returns (in-run, all kernel).
    """
    inside = total = 0
    for snap in run['snaps']:
        if snap['seq'] not in seqs:
            continue
        for index, count in snap['counts'].items():
            pc, words = run['blocks'][index]
            if pc < KERNEL:
                continue
            flags = []
            for word in words:
                kinds = classify(word)
                flags.append('move' in kinds or ('frame' in kinds and 'atomic' not in kinds))
            length = 0
            for flag in flags + [False]:
                if flag:
                    length += 1
                    continue
                if length >= minimum:
                    inside += length * count
                length = 0
            total += len(words) * count
    return inside, total


def table(left, right, labels, top):
    """Rank functions by their share of the left kernel's excess instructions."""
    names = set(left) | set(right)
    excess_total = sum(left[n]['insn'] for n in names) - sum(right[n]['insn'] for n in names)
    rows = []
    for name in names:
        a, b = left[name]['insn'], right[name]['insn']
        rows.append((a - b, name, a, b))
    rows.sort(reverse=True)
    lines = [f'{"function":<40} {labels[0]:>14} {labels[1]:>14} {"ratio":>7} {"excess":>14} {"share":>7}']
    for delta, name, a, b in rows[:top]:
        ratio = f'{a / b:.2f}' if b else 'only'
        share = f'{100 * delta / excess_total:.1f}%' if excess_total else '-'
        lines.append(f'{name[:40]:<40} {a:>14,} {b:>14,} {ratio:>7} {delta:>14,} {share:>7}')
    return lines, excess_total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--left', nargs=2, metavar=('RUN', 'VMLINUX'), required=True)
    parser.add_argument('--right', nargs=2, metavar=('RUN', 'VMLINUX'), required=True)
    parser.add_argument('--labels', nargs=2, default=('EvoCC', 'GCC'))
    parser.add_argument('--left-seqs', default='all', help='comma list of snapshot numbers or all')
    parser.add_argument('--right-seqs', default='all')
    parser.add_argument('--top', type=int, default=40)
    parser.add_argument('--per', metavar='SYMBOL',
                        help='scale the left counts to the right side\'s entries of SYMBOL')
    parser.add_argument('--right-inline', action='store_true',
                        help='attribute right-side code to DWARF inline frames the left side executed')
    parser.add_argument('--json')
    args = parser.parse_args()

    results = {}
    for side, (run_dir, vmlinux), seq_text in (('left', args.left, args.left_seqs),
                                               ('right', args.right, args.right_seqs)):
        run = load_run(run_dir)
        symbols = Symbols(vmlinux)
        seqs = ({s['seq'] for s in run['snaps']} if seq_text == 'all'
                else {int(s) for s in seq_text.split(',')})
        resolved = None
        if side == 'right' and args.right_inline:
            wanted = {name for name, counts in results['left']['totals'].items() if counts['insn']}
            resolved = inline_resolver(vmlinux, executed_addresses(run, seqs), wanted, symbols)
        totals, entries = profile(run, symbols, seqs, resolved)
        inside, kernel_total = transport_runs(run, seqs)
        print(f'{side}: kernel instructions in runs of >=6 frame ldr/str or moves: '
              f'{inside:,} of {kernel_total:,} ({100 * inside / max(kernel_total, 1):.1f}%)')
        results[side] = {'phases': phases(run), 'totals': totals, 'entries': entries,
                         'header': run['header'], 'seqs': sorted(seqs)}
    left, right = results['left'], results['right']
    if args.per:
        events = (left['entries'][args.per], right['entries'][args.per])
        if not all(events):
            raise SystemExit(f'{args.per}: no entries on one side')
        factor = events[1] / events[0]
        for counts in left['totals'].values():
            for kind in counts:
                counts[kind] = round(counts[kind] * factor)
        for name in left['entries']:
            left['entries'][name] = round(left['entries'][name] * factor)
        print(f'{args.labels[0]} counts scaled by {factor:.4f}: {args.per} entries {events[0]:,} -> {events[1]:,}; '
              f'per-event values are counts / {events[1]:,}')
    for side, label in (('left', args.labels[0]), ('right', args.labels[1])):
        print(f'{label}: {results[side]["header"]}; snapshots {results[side]["seqs"]}')
        for row in results[side]['phases']:
            print(f'  seq {row["seq"]:>3} closed by {row["closedBy"]:<6} insns {row["insns"]:>15,} '
                  f'kernel {row["kernelInsns"]:>15,} host-s {row["seconds"]}')
    sums = {side: Counter() for side in results}
    for side in results:
        for counts in results[side]['totals'].values():
            sums[side].update(counts)
    print(f'\n{"class":<10} {args.labels[0]:>16} {args.labels[1]:>16} {"ratio":>7}')
    for kind in CLASSES:
        a, b = sums['left'][kind], sums['right'][kind]
        print(f'{kind:<10} {a:>16,} {b:>16,} {a / b if b else 0:>7.2f}')
    kernel = {side: sum(c['insn'] for n, c in results[side]['totals'].items() if n != LOW)
              for side in results}
    print(f'kernel insns {kernel["left"]:,} / {kernel["right"]:,}; '
          f'{LOW} {results["left"]["totals"][LOW]["insn"]:,} / {results["right"]["totals"][LOW]["insn"]:,}')
    lines, excess = table(left['totals'], right['totals'], args.labels, args.top)
    print(f'\nTotal excess {excess:,} instructions')
    print('\n'.join(lines))
    shown = ('insn', 'mem', 'frame', 'move', 'call', 'pac', 'branch', 'sysreg')
    print(f'\nPer-function classes, {args.labels[0]}/{args.labels[1]} (thousands); entries = block entries at the symbol')
    print(f'{"function":<32} ' + ' '.join(f'{k:>15}' for k in (*shown, 'entries')))
    for line in lines[1:min(len(lines), 21)]:
        name = line[:40].rstrip()
        a, b = left['totals'].get(name, Counter()), right['totals'].get(name, Counter())
        cells = [f'{a[k] // 1000:>7}/{b[k] // 1000:<7}' for k in shown]
        cells.append(f'{left["entries"][name] // 1000:>7}/{right["entries"][name] // 1000:<7}')
        print(f'{name[:32]:<32} ' + ' '.join(cells))
    if args.json:
        payload = {side: {'phases': results[side]['phases'], 'seqs': results[side]['seqs'],
                          'functions': {n: dict(c) for n, c in results[side]['totals'].items()},
                          'entries': dict(results[side]['entries'])} for side in results}
        Path(args.json).write_text(json.dumps(payload) + '\n')


if __name__ == '__main__':
    main()
