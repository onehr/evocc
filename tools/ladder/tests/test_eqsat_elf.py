import struct
import unittest

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


if __name__ == '__main__':
    unittest.main()
