import copy
import struct
import unittest

from tools.ladder import codegen_census as census


def object_bytes(words, functions=None, mappings=None):
    functions = functions if functions is not None else [('f', 0, len(words)*4)]
    mappings = mappings if mappings is not None else [('$x.0', 0)]
    names = b'\0.shstrtab\0.text\0.strtab\0.symtab\0'
    strings = b'\0'
    symbols = [(0, 0, 0, 0, 0, 0)]
    for name, start, size in functions:
        symbols.append((len(strings), 2, 0, 2, start, size))
        strings += name.encode() + b'\0'
    for name, start in mappings:
        symbols.append((len(strings), 0, 0, 2, start, 0))
        strings += name.encode() + b'\0'
    payloads = [b'', names, b''.join(struct.pack('<I', w) for w in words), strings,
                b''.join(struct.pack('<IBBHQQ', *s) for s in symbols)]
    data = bytearray(64+64*len(payloads))
    data[:7] = b'\x7fELF\x02\x01\x01'
    struct.pack_into('<HHIQQQIHHHHHH', data, 16, 1, 183, 1, 0, 0, 64, 0,
                     64, 0, 0, 64, len(payloads), 1)
    for i, (name, ty, flags, link, stride) in enumerate([
            ('', 0, 0, 0, 0), ('.shstrtab', 3, 0, 0, 0),
            ('.text', 1, 6, 0, 0), ('.strtab', 3, 0, 0, 0),
            ('.symtab', 2, 0, 3, 24)]):
        struct.pack_into('<IIQQQQIIQQ', data, 64+64*i,
                         names.index(name.encode()+b'\0'), ty, flags, 0,
                         len(data), len(payloads[i]), link, 0, 4, stride)
        data.extend(payloads[i])
    return bytes(data)


def disassembly(rows):
    return 'Disassembly of section .text:\n\n0000 <f>:\n' + '\n'.join(
        f'{i*4:x}: {word:08x}  {text}' for i, (word, text) in enumerate(rows))


class CensusTests(unittest.TestCase):
    def analyze(self, rows, **kwargs):
        return census.analyze(object_bytes([w for w, _ in rows], **kwargs), disassembly(rows))

    def test_precedence_and_exact_partition(self):
        rows = [(0xa9bf7bfd, 'stp x29, x30, [sp, #-0x10]!'),
                (0x910003fd, 'mov x29, sp'), (0xf94003e0, 'ldr x0, [sp]'),
                (0xaa0003e1, 'mov x1, x0'), (0xd2800020, 'mov x0, #1'),
                (0x90000000, 'adrp x0, 0x0'), (0x92401c00, 'and x0, x0, #0xff'),
                (0x94000000, 'bl 0x1c <f+0x1c>'), (0xeb00001f, 'cmp x0, x0'),
                (0x17fffff7, 'b 0x0 <f>'), (0xf9400000, 'ldr x0, [x0]'),
                (0x91000400, 'add x0, x0, #1'), (0xd503201f, 'nop'),
                (0xd65f03c0, 'ret')]
        result = self.analyze(rows)
        expected = dict.fromkeys(census.CATEGORIES, 4)
        expected.update(guard=0, prologue_epilogue=12)
        self.assertEqual(result['category_bytes'], expected)
        self.assertEqual(sum(result['category_bytes'].values()), result['size']['instruction_bytes'])

    def test_forged_category_missing_duplicate_or_wrong_bytes_rejected(self):
        rows = [(0xd65f03c0, 'ret')]
        data, text = object_bytes([rows[0][0]]), disassembly(rows)
        real = census.analyze(data, text)
        forged = copy.deepcopy(real)
        forged['category_bytes']['move'] += 4
        with self.assertRaises(ValueError):
            census.validate_summary(data, text, forged)
        for bad in (text.replace('d65f03c0', 'd503201f'), text + '\n0: d65f03c0 ret',
                    'Disassembly of section .text:\n', text.replace('ret', '<unknown>')):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                census.analyze(data, bad)

    def test_padding_pool_and_internal_nop_are_separate(self):
        rows = [(0xd503201f, 'nop'), (0xd65f03c0, 'ret'),
                (0xd503201f, 'nop'), (0xd503201f, 'nop'),
                (0xd65f03c0, 'ret'), (0x12345678, '.word 0x12345678')]
        r = self.analyze(rows, functions=[('f', 0, 8), ('g', 16, 4)],
                         mappings=[('$x.0', 0), ('$d.0', 20)])
        self.assertEqual(r['size']['padding_bytes'], 8)
        self.assertEqual(r['size']['literal_pool_or_intext_data_bytes'], 4)
        self.assertEqual(r['category_bytes']['other'], 4)
        self.assertEqual(sum(r['category_bytes'].values()), 12)

    def test_unmapped_and_overlap_fail_aliases_count_once(self):
        rows = [(0xd65f03c0, 'ret'), (0xd65f03c0, 'ret')]
        for functions in ([], [('f', 0, 4)], [('f', 0, 8), ('g', 4, 4)]):
            with self.subTest(functions=functions), self.assertRaises(ValueError):
                self.analyze(rows, functions=functions)
        r = self.analyze(rows, functions=[('f', 0, 8), ('alias', 0, 8)])
        self.assertEqual(sum(r['category_bytes'].values()), 8)
        self.assertEqual(r['functions'][0]['aliases'], ['alias', 'f'])

    def test_guard_flags_only_when_exclusively_consumed(self):
        rows = [(0xf100001f, 'cmp x0, #0'), (0x54000060, 'b.eq 0x10 <f+0x10>'),
                (0xd65f03c0, 'ret'), (0xd503201f, 'nop'), (0xd4200020, 'brk #1')]
        r = self.analyze(rows)
        self.assertEqual(r['category_bytes']['guard'], 12)
        rows[2] = (0x9a810000, 'csel x0, x0, x1, eq')
        r = self.analyze(rows)
        self.assertEqual(r['category_bytes']['guard'], 8)
        self.assertEqual(r['category_bytes']['compare_select'], 8)

    def test_relocation_call_is_not_placeholder_loop_and_local_counts_are_exact(self):
        rows = [(0x94000000, 'bl 0x0 <f>'), (0x14000000, 'b 0x4 <f+0x4>'),
                (0xd65f03c0, 'ret')]
        data = object_bytes([w for w, _ in rows], functions=[('f', 0, 8), ('g', 8, 4)])
        text = disassembly(rows) + '\n0: R_AARCH64_CALL26 g\n4: R_AARCH64_JUMP26 external\n'
        r = census.analyze(data, text)
        self.assertEqual(r['category_bytes']['call'], 8)
        self.assertEqual(next(f for f in r['functions'] if f['name'] == 'g')['direct_calls'], 1)

    def test_relocation_low12_and_got_load(self):
        rows = [(0x91000000, 'add x0, x0, #0'), (0xf9400000, 'ldr x0, [x0]'),
                (0xd65f03c0, 'ret')]
        text = disassembly(rows) + '\n0: R_AARCH64_ADD_ABS_LO12_NC obj\n4: R_AARCH64_LD64_GOT_LO12_NC obj\n'
        r = census.analyze(object_bytes([w for w, _ in rows]), text)
        self.assertEqual(r['category_bytes']['address'], 8)

    def test_written_callee_saved_register_spill_is_not_a_prologue_save(self):
        rows = [(0xf90003f3, 'str x19, [sp]'),
                (0xaa0003f3, 'mov x19, x0'), (0xf90007f3, 'str x19, [sp, #8]'),
                (0xf94007f3, 'ldr x19, [sp, #8]'), (0x91000673, 'add x19, x19, #1'),
                (0xf94003f3, 'ldr x19, [sp]'), (0xd65f03c0, 'ret')]
        r = self.analyze(rows)
        self.assertEqual(r['category_bytes']['prologue_epilogue'], 12)
        self.assertEqual(r['category_bytes']['stack_access'], 8)


if __name__ == '__main__':
    unittest.main()
