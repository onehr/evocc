"""A substitution witness may never borrow the next section's bytes."""

import importlib.util
from pathlib import Path
import tempfile
import unittest

_spec = importlib.util.spec_from_file_location(
    "k1_prove", Path(__file__).resolve().parents[1] / "prove.py"
)
prove = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(prove)


class SymbolExtentTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.object = Path(self.temp.name) / "object"
        self.object.write_bytes(b"HEADCODECOMMENT")
        self.table = {
            "symbols": {"f": {"value": 0, "size": 4, "ndx": 1}},
            "sections": {1: {"addr": 0, "offset": 4, "size": 4, "type": "PROGBITS"}},
        }

    def test_subject_extent_reads_exactly_its_own_bytes(self):
        self.assertEqual(prove.slice_symbol(self.object, self.table, "f", False, 4), b"CODE")

    def test_a_longer_reference_extent_is_not_a_subject_extent(self):
        self.assertIsNone(prove.slice_symbol(self.object, self.table, "f", False, 8))

    def test_a_shorter_reference_extent_cannot_hide_changed_tail_bytes(self):
        self.assertIsNone(prove.slice_symbol(self.object, self.table, "f", False, 2))

    def test_a_malformed_symbol_cannot_extend_into_the_next_section(self):
        self.table["symbols"]["f"]["size"] = 8
        self.assertIsNone(prove.slice_symbol(self.object, self.table, "f", False, 8))

    def test_an_unsized_symbol_is_not_measured_using_another_compiler(self):
        self.table["symbols"]["f"]["size"] = 0
        self.assertIsNone(prove.slice_symbol(self.object, self.table, "f", False, 4))

    def test_executable_addresses_are_relative_to_their_section(self):
        self.table["sections"][1]["addr"] = 4096
        self.table["symbols"]["f"]["value"] = 4096
        self.assertEqual(prove.slice_symbol(self.object, self.table, "f", True, 4), b"CODE")


if __name__ == "__main__":
    unittest.main()
