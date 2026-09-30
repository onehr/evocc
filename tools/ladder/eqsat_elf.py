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
