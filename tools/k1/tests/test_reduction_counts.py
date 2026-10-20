"""The profiler's address filter must not count /init as the reduction."""
import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    'reduction_counts', Path(__file__).resolve().parents[1] / 'perf/reduction-counts.py')
counts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(counts)


class CodeAddressTest(unittest.TestCase):
    def test_static_programs_require_disjoint_executable_load_ranges(self):
        def elf(address):
            data = bytearray(120)
            data[:7] = b'\x7fELF\x02\x01\x01'
            struct.pack_into('<HH', data, 16, 2, 183)
            struct.pack_into('<Q', data, 32, 64)
            struct.pack_into('<HH', data, 54, 56, 1)
            struct.pack_into('<IIQQQQQQ', data, 64, 1, 5, 0, address, address, 120, 0x1000, 0x10000)
            return data

        with tempfile.TemporaryDirectory() as directory:
            init, binary = Path(directory) / 'init', Path(directory) / 'subject'
            init.write_bytes(elf(0x400000))
            for address in (0x400000, 0x400ffc, 0x3ffffc):
                binary.write_bytes(elf(address))
                with self.subTest(address=address), self.assertRaisesRegex(ValueError, 'overlap'):
                    counts.require_disjoint_code(binary, init)
            for address in (0x401000, 0x3ff000, 0x10000000):
                binary.write_bytes(elf(address))
                counts.require_disjoint_code(binary, init)
            binary.write_bytes(b'not an ELF')
            with self.assertRaises(ValueError):
                counts.require_disjoint_code(binary, init)
