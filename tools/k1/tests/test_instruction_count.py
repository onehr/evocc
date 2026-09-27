"""The diagnostic must not double-count aliases or borrow adjacent symbols."""

import importlib.util
from pathlib import Path
import re
import tempfile
import unittest

_spec = importlib.util.spec_from_file_location(
    "kvm_instruction_count",
    Path(__file__).resolve().parents[3] / "tests/fixtures/kvm-instruction-count.py",
)
profile = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(profile)


class InstructionRangesTest(unittest.TestCase):
    def ranges(self, text, expression="kvm"):
        with tempfile.TemporaryDirectory() as directory:
            table = Path(directory) / "System.map"
            table.write_text(text)
            return profile.ranges(table, re.compile(expression))

    def test_aliases_form_one_range(self):
        self.assertEqual(
            self.ranges("100 T kvm_a\n100 t kvm_b\n110 T next\n"),
            "100 110 kvm_a|kvm_b\n",
        )

    def test_nonselected_and_data_symbols_bound_ranges(self):
        self.assertEqual(
            self.ranges(
                "100 T kvm_a\n104 D data\n108 T other\n" "10c T kvm_b\n110 D end\n"
            ),
            "100 104 kvm_a\n10c 110 kvm_b\n",
        )

    def test_data_names_do_not_become_code(self):
        with self.assertRaisesRegex(ValueError, "no selected"):
            self.ranges("100 D kvm_data\n108 T other\n110 D end\n")

    def test_unbounded_last_symbol_is_not_invented(self):
        with self.assertRaisesRegex(ValueError, "no selected"):
            self.ranges("100 T kvm_last\n")

    def test_implausible_extent_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "unbounded"):
            self.ranges("100 T kvm_a\n200000 D end\n")

    def test_unsorted_input_keeps_address_order(self):
        self.assertEqual(
            self.ranges("110 D end\n100 T kvm_a\n108 T kvm_b\n"),
            "100 108 kvm_a\n108 110 kvm_b\n",
        )

    def test_root_entries_must_match_independent_trace(self):
        trace = "# kvm_handle_guest_abort() {\n# kvm_handle_guest_abort() {\n"
        rows = [{"name": "kvm_handle_guest_abort", "entries": 2}]
        self.assertTrue(profile.root_counts_match(trace, rows))
        self.assertFalse(profile.root_counts_match(trace, []))
        self.assertFalse(profile.root_counts_match("", rows))
        self.assertFalse(profile.root_counts_match(trace, [dict(rows[0], entries=3)]))

    def test_both_memslot_roots_must_match(self):
        trace = (
            "one-mode memslot diagnostic\n# kvm_arch_flush_shadow_memslot();\n"
            "# kvm_free_stage2_pgd();\n"
        )
        rows = [
            {"name": "kvm_arch_flush_shadow_memslot", "entries": 1},
            {"name": "alias|kvm_free_stage2_pgd", "entries": 1},
        ]
        self.assertTrue(profile.root_counts_match(trace, rows))
        self.assertFalse(profile.root_counts_match(trace, rows[:1]))


if __name__ == "__main__":
    unittest.main()
