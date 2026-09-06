"""A survey answer may only be reused when nothing it depends on has moved."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest

_spec = importlib.util.spec_from_file_location(
    "k1_substitute", Path(__file__).resolve().parents[1] / "substitute.py"
)
substitute = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(substitute)

ARGV = ["/bin/evocc", "--emit=asm", "-Ifoo", "/src/lib/bitrev.c"]
DRIVER = "a" * 64
SOURCE = "b" * 64


class CacheKeyTest(unittest.TestCase):
    def key(self, **overrides) -> str:
        fields = {
            "unit": "lib/bitrev.o",
            "evocc_sha256": DRIVER,
            "source_sha256": SOURCE,
            "argv": ARGV,
        }
        fields.update(overrides)
        return substitute.cache_key(**fields)

    def test_the_same_inputs_give_the_same_key(self) -> None:
        self.assertEqual(self.key(), self.key())

    def test_a_new_driver_binary_is_a_new_key(self) -> None:
        self.assertNotEqual(self.key(), self.key(evocc_sha256="c" * 64))

    def test_a_changed_source_is_a_new_key(self) -> None:
        self.assertNotEqual(self.key(), self.key(source_sha256="c" * 64))

    def test_a_changed_compile_command_is_a_new_key(self) -> None:
        self.assertNotEqual(self.key(), self.key(argv=ARGV + ["-DEXTRA"]))

    def test_a_different_unit_is_a_new_key(self) -> None:
        self.assertNotEqual(self.key(), self.key(unit="lib/ctype.o"))

    def test_two_units_never_share_an_entry_through_their_argv(self) -> None:
        # The unit is part of the key in its own right, not only through the
        # command, so a record whose argv happens to match cannot answer for
        # the wrong object path.
        self.assertNotEqual(self.key(unit="lib/a.o"), self.key(unit="lib/b.o"))


class ReusableTest(unittest.TestCase):
    def setUp(self) -> None:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.asm_root = Path(directory.name)

    def emitted_entry(self) -> dict:
        return {"unit": "lib/bitrev.o", "outcome": "emitted", "assemblyBytes": 12}

    def write_assembly(self, unit: str) -> None:
        path = substitute.assembly_path(self.asm_root, unit)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(".text\n", encoding="utf-8")

    def test_a_refusal_is_reused_as_it_stands(self) -> None:
        entry = {"unit": "lib/x.o", "outcome": "refused", "diagnosticCodes": ["E3101"]}
        row = substitute.reusable(entry, "lib/x.o", self.asm_root)
        self.assertEqual(row["outcome"], "refused")
        self.assertEqual(row["diagnosticCodes"], ["E3101"])
        self.assertTrue(row["fromCache"])

    def test_an_emitted_answer_needs_its_assembly_on_disk(self) -> None:
        self.assertIsNone(
            substitute.reusable(self.emitted_entry(), "lib/bitrev.o", self.asm_root)
        )
        self.write_assembly("lib/bitrev.o")
        self.assertIsNotNone(
            substitute.reusable(self.emitted_entry(), "lib/bitrev.o", self.asm_root)
        )

    def test_a_timeout_is_never_reused(self) -> None:
        entry = {"unit": "lib/x.o", "outcome": "timeout", "seconds": 180}
        self.assertIsNone(substitute.reusable(entry, "lib/x.o", self.asm_root))

    def test_an_outcome_from_a_later_step_is_never_reused(self) -> None:
        for outcome in ("assembled", "substituted", "symbol-drift", "assembler-refused",
                        "rebuilt-by-kbuild", "invented"):
            self.assertIsNone(
                substitute.reusable({"outcome": outcome}, "lib/x.o", self.asm_root)
            )

    def test_a_missing_entry_is_a_miss(self) -> None:
        self.assertIsNone(substitute.reusable(None, "lib/x.o", self.asm_root))

    def test_the_reused_row_is_a_copy(self) -> None:
        entry = {"unit": "lib/x.o", "outcome": "refused", "diagnosticCodes": ["E3101"]}
        row = substitute.reusable(entry, "lib/x.o", self.asm_root)
        row["outcome"] = "substituted"
        self.assertEqual(entry["outcome"], "refused")
        self.assertNotIn("fromCache", entry)


class CacheFileTest(unittest.TestCase):
    def setUp(self) -> None:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.path = Path(directory.name) / "nested" / "survey-cache.json"

    def test_what_is_saved_is_what_is_loaded(self) -> None:
        entries = {"k": {"unit": "lib/x.o", "outcome": "refused"}}
        substitute.save_cache(self.path, entries)
        self.assertEqual(substitute.load_cache(self.path), entries)

    def test_an_absent_cache_is_empty_rather_than_an_error(self) -> None:
        self.assertEqual(substitute.load_cache(self.path), {})

    def test_a_damaged_cache_is_ignored(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text("{not json", encoding="utf-8")
        self.assertEqual(substitute.load_cache(self.path), {})

    def test_a_cache_of_another_version_is_ignored(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text('{"version": 0, "entries": {"k": {}}}', encoding="utf-8")
        self.assertEqual(substitute.load_cache(self.path), {})


if __name__ == "__main__":
    unittest.main()
