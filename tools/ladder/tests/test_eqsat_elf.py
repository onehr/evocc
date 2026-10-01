import struct
import unittest

from tools.ladder import eqsat_elf
from tools.ladder.eqsat_elf import inspect_elf


def fixture():
    names = b'\0.shstrtab\0.text\0.rodata\0.data\0.bss\0'
    payloads = [b'', names, b'ABCD', b'ro', b'write', b'']
    data = bytearray(64 + 6*64)
    data[:7] = b'\x7fELF\x02\x01\x01'
    struct.pack_into('<HHIQQQIHHHHHH', data, 16, 1, 183, 1, 0, 0, 64, 0, 64, 0, 0, 64, 6, 1)
    for i, (name, ty, flags, size) in enumerate([
            ('', 0, 0, 0), ('.shstrtab', 3, 0, len(names)), ('.text', 1, 6, 4),
            ('.rodata', 1, 2, 2), ('.data', 1, 3, 5), ('.bss', 8, 3, 4096)]):
        struct.pack_into('<IIQQQQIIQQ', data, 64+i*64, names.index(name.encode()+b'\0'), ty,
                         flags, 0, len(data), size, 0, 0, 1, 0)
        data.extend(payloads[i])
    return bytes(data)


def size_fixture(pool_in_text=True, text_flags=6):
    """Two functions, one real NOP, an alignment gap, and a mapped data pool."""
    names = b'\0.shstrtab\0.text\0.text.cold\0.rodata\0.strtab\0.symtab\0'
    strings = b'\0f\0g\0$x.0\0$x.1\0$d.0\0'
    nop, ret = bytes.fromhex('1f2003d5'), bytes.fromhex('c0035fd6')
    text = nop + ret + nop*2 + ret
    symbols = [(0, 0, 0, 0, 0, 0), (strings.index(b'f\0'), 2, 0, 2, 0, 8),
               (strings.index(b'g\0'), 2, 0, 2, 16, 4),
               (strings.index(b'$x.0\0'), 0, 0, 2, 0, 0),
               (strings.index(b'$x.1\0'), 0, 0, 3, 0, 0)]
    if pool_in_text:
        symbols.append((strings.index(b'$d.0\0'), 0, 0, 2, len(text), 0))
        text += b'abcdefgh'
    payloads = [b'', names, text, ret, b'' if pool_in_text else b'abcdefgh', strings,
                b''.join(struct.pack('<IBBHQQ', *s) for s in symbols)]
    data = bytearray(64 + len(payloads)*64)
    data[:7] = b'\x7fELF\x02\x01\x01'
    struct.pack_into('<HHIQQQIHHHHHH', data, 16, 1, 183, 1, 0, 0, 64, 0,
                     64, 0, 0, 64, len(payloads), 1)
    for i, (name, ty, flags, link, ent) in enumerate([
            ('', 0, 0, 0, 0), ('.shstrtab', 3, 0, 0, 0),
            ('.text', 1, text_flags, 0, 0), ('.text.cold', 1, 6, 0, 0),
            ('.rodata', 1, 2, 0, 0), ('.strtab', 3, 0, 0, 0),
            ('.symtab', 2, 0, 5, 24)]):
        struct.pack_into('<IIQQQQIIQQ', data, 64+i*64, names.index(name.encode()+b'\0'),
                         ty, flags, 0, len(data), len(payloads[i]), link, 0, 4, ent)
        data.extend(payloads[i])
    return bytes(data)


class ElfTests(unittest.TestCase):
    def test_sections_are_not_file_size_or_instruction_count(self):
        result = inspect_elf(fixture())
        self.assertEqual([result[k] for k in ('executable_section_bytes', 'readonly_alloc_bytes',
            'writable_alloc_bytes', 'zerofill_alloc_bytes', 'debug_bytes')], [4, 2, 5, 4096, 0])
        self.assertEqual(result['instruction_padding_pool_split'], 'NOT_MEASURED')
        self.assertLess(result['file_bytes'], result['zerofill_alloc_bytes'])

    def test_wrong_target_truncation_and_magic_are_rejected(self):
        for value in (b'', fixture()[:120], b'X'+fixture()[1:], fixture()[:18]+b'\x3e\0'+fixture()[20:]):
            with self.subTest(size=len(value)), self.assertRaises(ValueError):
                inspect_elf(value)


class ObjectSizeContractTests(unittest.TestCase):
    def test_padding_is_not_every_nop_and_a_forged_partition_is_rejected(self):
        obj = size_fixture()
        result = eqsat_elf.inspect_object_size(obj)
        self.assertEqual(result['text_bytes'], 32)
        self.assertEqual(result['instruction_bytes'], 16)
        self.assertEqual(result['static_instruction_slots'], 4)
        self.assertEqual(result['padding_bytes'], 8)
        self.assertEqual(result['literal_pool_or_intext_data_bytes'], 8)
        forged = dict(result, instruction_bytes=24, padding_bytes=0)
        with self.assertRaisesRegex(ValueError, 'size record'):
            eqsat_elf.validate_object_size(obj, forged)

    def test_pool_migration_is_reported_not_sold_as_instruction_elimination(self):
        before = eqsat_elf.inspect_object_size(size_fixture())
        after = eqsat_elf.inspect_object_size(size_fixture(pool_in_text=False))
        self.assertEqual(before['text_bytes'] - after['text_bytes'], 8)
        self.assertEqual(after['rodata_bytes'] - before['rodata_bytes'], 8)
        self.assertEqual(before['static_instruction_slots'], after['static_instruction_slots'])
        self.assertEqual(before['text_plus_rodata_bytes'], after['text_plus_rodata_bytes'])
        with self.assertRaisesRegex(ValueError, 'size record'):
            eqsat_elf.validate_object_size(size_fixture(pool_in_text=False), before)

    def test_named_text_requires_executable_readonly_allocated_progbits(self):
        for flags in (0, 2, 7):
            with self.subTest(flags=flags), self.assertRaisesRegex(ValueError, 'text section'):
                eqsat_elf.inspect_object_size(size_fixture(text_flags=flags))

    def test_two_builds_must_be_identical_not_just_the_same_size(self):
        obj = size_fixture()
        eqsat_elf.require_identical_objects(obj, obj)
        changed = obj[:-1] + bytes([obj[-1] ^ 1])
        with self.assertRaisesRegex(ValueError, 'reproducible'):
            eqsat_elf.require_identical_objects(obj, changed)

    def test_missing_symbols_does_not_invent_an_instruction_count(self):
        result = eqsat_elf.inspect_object_size(fixture())
        self.assertEqual(result['unclassified_text_bytes'], 4)
        self.assertIsNone(result['static_instruction_slots'])


if __name__ == '__main__':
    unittest.main()
