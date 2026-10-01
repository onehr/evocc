"""Exact ELF64 AArch64 artifact metrics; not an instruction decoder.

SHF_EXECINSTR size includes alignment/pools. A separate disassembler would be
needed to classify each byte as instructions/padding; do not invent that split.
"""
import struct

from .eqsat_chain import digest, require


def inspect_elf(data):
    require(len(data) >= 64 and data[:7] == b'\x7fELF\x02\x01\x01', 'not ELF64 little-endian')
    header = struct.unpack_from('<HHIQQQIHHHHHH', data, 16)
    kind, machine, _, _, _, offset, _, _, _, _, entry_size, count, names_index = header
    require(kind in (1, 2, 3) and machine == 183 and entry_size == 64 and 0 < names_index < count,
            'unsupported ELF section table')
    require(offset + count*64 <= len(data), 'truncated ELF section table')
    entries = [struct.unpack_from('<IIQQQQIIQQ', data, offset+i*64) for i in range(count)]

    def bytes_at(offset, size):
        require(offset + size <= len(data), 'truncated ELF section')
        return data[offset:offset+size]

    names = bytes_at(entries[names_index][4], entries[names_index][5])
    rows = []
    totals = dict(executable_section_bytes=0, readonly_alloc_bytes=0,
                  writable_alloc_bytes=0, zerofill_alloc_bytes=0, debug_bytes=0)
    for name, ty, flags, address, start, size, link, info, align, ent in entries:
        require(name < len(names), 'invalid section name')
        end = names.find(b'\0', name)
        require(end != -1, 'unterminated ELF section name')
        label = names[name:end].decode('ascii')
        payload = b'' if ty == 8 else bytes_at(start, size)
        if flags & 2:
            field = ('zerofill_alloc_bytes' if ty == 8 else 'executable_section_bytes' if flags & 4
                     else 'writable_alloc_bytes' if flags & 1 else 'readonly_alloc_bytes')
            totals[field] += size
        if label.startswith(('.debug', '.zdebug')):
            totals['debug_bytes'] += size
        rows.append(dict(name=label, type=ty, flags=flags, address=address, size=size,
                         alignment=align, data_digest=digest(payload)))
    return dict(schema='evocc-elf-size-v1', file_digest=digest(data), file_bytes=len(data),
                **totals, sections=rows, instruction_padding_pool_split='NOT_MEASURED',
                materialization_comparison='EXACT_OBJECT_BYTES_INCLUDING_RELOCATIONS')


def inspect_object_size(data):
    """S1 TU metric, separate from the frozen historical ELF-v1 receipts.

    Mapping symbols distinguish instruction words from in-text data, not data
    provenance (a jump table is not distinguishable from a literal pool here).
    Only fill between sized STT_FUNC extents is called layout padding. NOPs
    inside a function remain instruction slots, including any internal alignment
    NOPs: an object alone cannot tell them from intentionally emitted NOPs.
    """
    old = inspect_elf(data)
    header = struct.unpack_from('<HHIQQQIHHHHHH', data, 16)
    require(header[0] == 1, 'TU size requires a relocatable object')
    offset, count = header[5], header[11]
    entries = [struct.unpack_from('<IIQQQQIIQQ', data, offset+i*64) for i in range(count)]
    symbols = []
    for section in entries:
        _, ty, _, _, start, size, link, _, _, stride = section
        if ty != 2:
            continue
        require(stride == 24 and size % stride == 0 and link < count,
                'invalid object symbol table')
        strings = entries[link]
        require(strings[1] == 3, 'invalid symbol string table')
        names = data[strings[4]:strings[4]+strings[5]]
        for pos in range(start, start+size, stride):
            name, info, _, index, value, length = struct.unpack_from('<IBBHQQ', data, pos)
            end = names.find(b'\0', name)
            require(name < len(names) and end >= name, 'invalid symbol name')
            symbols.append((names[name:end], info & 15, index, value, length))

    totals = dict(text_bytes=0, instruction_bytes=0, padding_bytes=0,
                  literal_pool_or_intext_data_bytes=0, unclassified_text_bytes=0,
                  rodata_bytes=0)
    rows = []
    nop = bytes.fromhex('1f2003d5')
    for index, (raw, old_row) in enumerate(zip(entries, old['sections'])):
        name, ty, flags, _, start, size, _, _, _, _ = raw
        label = old_row['name']
        is_text = label == '.text' or label.startswith('.text.')
        is_rodata = label == '.rodata' or label.startswith('.rodata.')
        if is_rodata:
            require(ty == 1 and flags & 7 == 2, 'misclassified rodata section: '+label)
            totals['rodata_bytes'] += size
        if not is_text:
            continue
        require(ty == 1 and flags & 7 == 6, 'misclassified text section: '+label)
        row = dict(name=label, size=size, instruction_bytes=0, padding_bytes=0,
                   literal_pool_or_intext_data_bytes=0, unclassified_text_bytes=0)
        local = [s for s in symbols if s[2] == index]
        functions = [(s[3], s[3]+s[4]) for s in local if s[1] == 2 and s[4]]
        require(all(0 <= a < b <= size for a, b in functions), 'invalid function extent')
        mappings = {}
        for symbol, _, _, value, _ in local:
            if symbol in (b'$x', b'$d') or symbol.startswith((b'$x.', b'$d.')):
                require(value <= size, 'mapping symbol outside section')
                kind = symbol[1:2]
                require(value not in mappings or mappings[value] == kind, 'conflicting mapping symbols')
                mappings[value] = kind
        boundaries = sorted({0, size, *mappings, *(p for f in functions for p in f)})
        mode = None
        for a, b in zip(boundaries, boundaries[1:]):
            mode = mappings.get(a, mode)
            chunk = data[start+a:start+b]
            field = 'unclassified_text_bytes'
            if mode == b'd':
                field = 'literal_pool_or_intext_data_bytes'
            elif mode == b'x' and a % 4 == 0 and (b-a) % 4 == 0:
                inside = any(lo <= a and b <= hi for lo, hi in functions)
                between = (not inside and any(hi <= a for _, hi in functions)
                           and any(lo >= b for lo, _ in functions))
                other_symbol = any(a <= s[3] < b and not s[0].startswith(b'$') for s in local)
                fill = all(chunk[i:i+4] in (nop, b'\0'*4) for i in range(0, len(chunk), 4))
                field = ('padding_bytes' if between and fill and not other_symbol
                         else 'instruction_bytes')
            row[field] += b-a
        totals['text_bytes'] += size
        for field in row.keys() - {'name', 'size'}:
            totals[field] += row[field]
        rows.append(row)
    require(totals['text_bytes'] == sum(totals[f] for f in ('instruction_bytes', 'padding_bytes',
            'literal_pool_or_intext_data_bytes', 'unclassified_text_bytes')), 'invalid text partition')
    return dict(schema='evocc-tu-size-v1', object_digest=old['file_digest'], **totals,
                static_instruction_slots=(None if totals['unclassified_text_bytes'] else
                                          totals['instruction_bytes']//4),
                text_plus_rodata_bytes=totals['text_bytes']+totals['rodata_bytes'],
                other_executable_bytes=old['executable_section_bytes']-totals['text_bytes'],
                other_readonly_alloc_bytes=old['readonly_alloc_bytes']-totals['rodata_bytes'],
                writable_alloc_bytes=old['writable_alloc_bytes'],
                zerofill_alloc_bytes=old['zerofill_alloc_bytes'], debug_bytes=old['debug_bytes'],
                file_bytes=old['file_bytes'],
                padding_scope='INTER_FUNCTION_FILL_ONLY; INTERNAL_NOPS_ARE_INSTRUCTION_SLOTS',
                data_scope='MAPPING_SYMBOL_DATA; LITERAL_POOLS_AND_OTHER_IN_TEXT_DATA', sections=rows)


def validate_object_size(data, record):
    require(inspect_object_size(data) == record, 'forged object size record')


def require_identical_objects(first, second):
    require(first == second, 'object builds are not byte-reproducible')
