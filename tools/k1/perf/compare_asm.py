#!/usr/bin/env python3
"""Compile one C reduction with frozen EvoCC and sealed GCC 14; count instructions.

Both compilers get the kernel's code-generation flags that change emitted
code (see KERNEL_FLAGS). EvoCC runs on the host with --target and --sysroot;
GCC runs in the sealed image without network. For each named function the
report gives the static instruction count and the instruction classes used
by tools/k1/perf/attribute.py. A loop body is the span from a branch target
to the last backward branch that reaches it; its count is per iteration only
when that span has no internal taken branch, so check the listing.
"""

from __future__ import annotations

import argparse
from collections import Counter
import importlib.util
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]
EVOCC = ROOT / '.build/frozen/366fd796/evocc-darwin'
SYSROOT = ROOT / 'sysroot/aarch64-unknown-linux-gnu'
KERNEL_FLAGS = ['-O2', '-std=gnu11', '-fno-strict-aliasing', '-fno-strict-overflow',
                '-fno-delete-null-pointer-checks', '-mgeneral-regs-only', '-mbranch-protection=pac-ret',
                '-fno-omit-frame-pointer', '-fno-optimize-sibling-calls', '-fstack-protector-strong',
                '-ftrivial-auto-var-init=zero', '-funsigned-char', '-fno-common', '-fno-PIE']
GCC_ONLY = ['-fno-asynchronous-unwind-tables', '-fno-unwind-tables', '-fno-allow-store-data-races',
            '-fconserve-stack', '-fmin-function-alignment=4']
IMAGE = 'evo-nexlab:sealed'
spec = importlib.util.spec_from_file_location('attribute', Path(__file__).with_name('attribute.py'))
attribute = importlib.util.module_from_spec(spec)
spec.loader.exec_module(attribute)


def compile_both(source, out):
    out.mkdir(parents=True, exist_ok=True)
    evocc = out / f'{source.stem}.evocc.s'
    gcc = out / f'{source.stem}.gcc.s'
    subprocess.run([str(EVOCC), '--target=aarch64-unknown-linux-gnu', f'--sysroot={SYSROOT}',
                    *KERNEL_FLAGS, '-S', str(source), '-o', str(evocc)], check=True)
    inner = ['gcc-14', *KERNEL_FLAGS, *GCC_ONLY, '-S', f'/in/{source.name}', '-o', f'/out/{gcc.name}']
    subprocess.run(['/usr/local/bin/container', 'run', '--rm', '--platform', 'linux/arm64',
                    '--network', 'none', '-v', f'{source.parent.resolve()}:/in:ro',
                    '-v', f'{out.resolve()}:/out', IMAGE, *inner],
                   check=True, stdout=subprocess.DEVNULL)
    return {'EvoCC': evocc, 'GCC': gcc}


def functions(path):
    """Map each function to its instruction lines, in emission order."""
    result, current = {}, None
    for raw in path.read_text().splitlines():
        line = raw.split('//')[0].rstrip()
        stripped = line.strip()
        if not stripped:
            continue
        label = re.fullmatch(r'([A-Za-z_][\w.$]*):', stripped)
        if label and not stripped.startswith('.L'):
            current = label.group(1)
            result[current] = []
        elif current and (stripped.startswith('.L') and stripped.endswith(':')):
            result[current].append(stripped)
        elif current and not stripped.startswith('.'):
            result[current].append(stripped)
        elif current and stripped.startswith(('.size', '.cfi_endproc')) and current in stripped:
            current = None
    return result


def encode_classes(text):
    """Classify one assembly line by mnemonic, mirroring attribute.classify."""
    parts = text.replace(',', ' ').split()
    op = parts[0].lower()
    found = ['insn']
    if re.match(r'(ld|st)(r|ur|p|xr|axr|lxr|ar|lr|add|set|clr|eor|smax|umax|smin|umin|swp)', op) or op.startswith(('cas', 'swp', 'prfm')):
        found.append('mem')
        if re.search(r'\[(sp|x29)\b', text):
            found.append('frame')
    if op in ('bl', 'blr'):
        found.append('call')
    if op in ('b', 'br', 'cbz', 'cbnz', 'tbz', 'tbnz') or op.startswith('b.'):
        found.append('branch')
    if op in ('paciasp', 'autiasp', 'hint') and (op != 'hint' or parts[1] in ('#25', '#29', '25', '29')):
        found.append('pac')
    if op == 'mov' and len(parts) == 3 and re.fullmatch(r'[wx]\d+|[wx]zr|sp', parts[2]) and parts[2] != 'sp' and parts[1] != 'sp':
        found.append('move')
    return found


def loop_body(lines):
    """Instructions from the outermost backward-branch target to that branch."""
    positions = {line[:-1]: i for i, line in enumerate(lines) if line.endswith(':')}
    best = None
    for i, line in enumerate(lines):
        target = line.split()[-1] if line.split() else ''
        if target in positions and positions[target] < i and line.split()[0].lower().startswith(('b', 'cb', 'tb')):
            span = (positions[target], i)
            if best is None or span[1] - span[0] > best[1] - best[0]:
                best = span
    if best is None:
        return None
    return [line for line in lines[best[0]:best[1] + 1] if not line.endswith(':')]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source')
    parser.add_argument('function', nargs='+')
    parser.add_argument('--out', required=True)
    parser.add_argument('--show', action='store_true', help='print both listings')
    args = parser.parse_args()
    outputs = compile_both(Path(args.source), Path(args.out))
    print(f'flags: {" ".join(KERNEL_FLAGS)} (GCC also {" ".join(GCC_ONLY)})')
    for name in args.function:
        for compiler, path in outputs.items():
            body = functions(path).get(name)
            if body is None:
                print(f'{name:<28} {compiler:<6} missing (inlined or renamed)')
                continue
            insns = [line for line in body if not line.endswith(':')]
            classes = Counter(k for line in insns for k in encode_classes(line))
            loop = loop_body(body)
            loop_text = f' loop-span {len(loop)}' if loop else ''
            print(f'{name:<28} {compiler:<6} static {len(insns):>4}{loop_text}  '
                  + ' '.join(f'{k}={classes[k]}' for k in ('mem', 'frame', 'move', 'call', 'branch', 'pac')))
            if args.show:
                print('\n'.join('    ' + line for line in body))


if __name__ == '__main__':
    main()
