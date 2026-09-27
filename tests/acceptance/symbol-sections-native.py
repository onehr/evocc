#!/usr/bin/env python3
"""Check section flags/aliases/GC in the sealed ARM64 lab, using prepared assembly.

Usage: python3 tests/acceptance/symbol-sections-native.py PREPARED_DIRECTORY
The directory has sections-evo-O{0,2}-{pic,static,patched}.s, emitted on the host.
No source expectations, compiler thresholds or upstream tests are rewritten.
"""

from pathlib import Path
import importlib.util
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('section_prove', ROOT / 'tools/k1/prove.py')
prove = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(prove)


def run(*argv):
    return subprocess.run(argv, check=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, text=True).stdout


def inspect(path):
    sections = run('readelf', '-SW', str(path))
    symbols = run('readelf', '-sW', str(path))
    text = '@@FILE unit\n' + sections + '@@SYMS unit\n' + symbols
    table = prove.parse_readelf(text)['unit']
    for line in sections.splitlines():
        match = re.match(r'\s*\[\s*(\d+)\]\s+(\S+)\s+(\S+)\s+'
                         r'[0-9a-f]+\s+[0-9a-f]+\s+[0-9a-f]+\s+[0-9a-f]+'
                         r'\s+([A-Z]+)\s+\d+\s+\d+\s+(\d+)\s*$', line)
        if match:
            table['sections'][int(match[1])].update(flags=match[4], align=int(match[5]))
    return table


DEAD = ('dead_function', 'dead_data', 'dead_zero', 'dead_readonly', 'dead_tls', 'dead_tls_zero')


def check_object(path, pic, subject, patched):
    table = inspect(path)
    symbols = table['symbols']
    sections = table['sections']
    seen = set()
    for symbol, kind, flags, size, alignment in [
        ('initialized', 'data', 'AW', 4, 4),
        ('zeroed', 'bss', 'AW', 4, 4),
        ('readonly', 'rodata', 'A', 4, 4),
        ('readonly_zero', 'rodata', 'A', 4, 4),
        ('relocation', 'data.rel.ro' if pic else 'rodata', 'WA' if pic else 'A', 8, 8),
        ('tls_data', 'tdata', 'AWT', 4, 4),
        ('tls_zero', 'tbss', 'AWT', 4, 4),
        ('dead_data', 'data', 'AW', 4, 4),
        ('dead_zero', 'bss', 'AW', 4, 4),
        ('dead_readonly', 'rodata', 'A', 4, 4),
        ('dead_tls', 'tdata', 'AWT', 4, 4),
        ('dead_tls_zero', 'tbss', 'AWT', 4, 4),
    ]:
        item = symbols[symbol]
        section = sections[item['ndx']]
        assert item['size'] == size, (path, symbol, item)
        assert section['align'] >= alignment, (path, symbol, section)
        assert set(section['flags']) == set(flags), (path, symbol, section)
        assert section['type'] == ('NOBITS' if kind in ('bss', 'tbss') else 'PROGBITS')
        assert item['type'] == ('TLS' if 'T' in flags else 'OBJECT')
        assert item['ndx'] not in seen, (path, symbol, 'shared storage section')
        seen.add(item['ndx'])
        if subject:
            assert section['name'] == f'.{kind}.{symbol}', (path, symbol, section)
    for symbol in ('live_function', 'dead_function', 'dispatch', 'main'):
        item = symbols[symbol]
        section = sections[item['ndx']]
        assert section['type'] == 'PROGBITS' and set(section['flags']) == set('AX')
        assert item['ndx'] not in seen, (path, symbol, 'shared code section')
        seen.add(item['ndx'])
        if subject:
            assert section['name'] == f'.text.{symbol}'
    for symbol, name in [('selected_function', '.text.selected'),
                         ('explicit_data', '.data.selected'),
                         ('explicit_zero', '.bss.selected'),
                         ('explicit_tls', '.tdata.selected')]:
        assert sections[symbols[symbol]['ndx']]['name'] == name
    assert symbols['live_alias'] == symbols['live_function'], (path, 'alias identity')
    assert any(s['type'] == 'INIT_ARRAY' for s in sections.values())
    assert any(s['type'] == 'FINI_ARRAY' for s in sections.values())
    if patched:
        records = [s for s in sections.values() if s['name'] == '__patchable_function_entries']
        assert records and all('L' in s['flags'] for s in records), records
        assert any(s['name'] == '.eh_frame' for s in sections.values())
    return table


def check_links(compiler, obj, directory, identity):
    for gc in (False, True):
        binary = directory / f'{identity}-gc{int(gc)}'
        run(compiler, '-no-pie', str(obj), *(['-Wl,--gc-sections'] if gc else []), '-o', str(binary))
        run(str(binary))
        symbols = inspect(binary)['symbols']
        for name in DEAD:
            assert (name not in symbols) == gc, (binary, name, gc)
    print(f'{identity}: object sections + ordinary/GC execution PASS', flush=True)


def main():
    directory = Path(sys.argv[1]).resolve()
    source = ROOT / 'tests/fixtures/driver-symbol-sections.c'
    for compiler in ('gcc-14', 'clang-19'):
        for level in (0, 2):
            for mode in ('pic', 'static', 'patched'):
                pic = mode != 'static'
                patch = mode == 'patched'
                flags = ['-fPIC' if pic else '-fno-pic', '-ffunction-sections', '-fdata-sections']
                if patch:
                    flags += ['-funwind-tables', '-fpatchable-function-entry=4,2']
                identity = f'{compiler}-O{level}-{mode}'
                reference = directory / f'{identity}.o'
                run(compiler, f'-O{level}', '-std=gnu11', *flags, '-c', str(source), '-o', str(reference))
                check_object(reference, pic, False, patch)
                check_links(compiler, reference, directory, identity)
                identity = f'evo-O{level}-{mode}-{compiler}'
                subject = directory / f'{identity}.o'
                run(compiler, '-c', str(directory / f'sections-evo-O{level}-{mode}.s'), '-o', str(subject))
                check_object(subject, pic, True, patch)
                check_links(compiler, subject, directory, identity)
    print('12 reference + 12 subject objects, 48 native executions PASS')


if __name__ == '__main__':
    main()
