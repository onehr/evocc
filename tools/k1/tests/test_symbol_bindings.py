"""Keep linker binding facts when comparing the original and emitted objects."""

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

_spec = importlib.util.spec_from_file_location(
    "k1_substitute", Path(__file__).resolve().parents[1] / "substitute.py"
)
substitute = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(substitute)


class SymbolBindingsTest(unittest.TestCase):
    def test_failed_or_incomplete_symbol_read_is_not_empty_symbol_parity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            objects = [("mine/a.o", root / "a.o"), ("theirs/a.o", root / "a.o")]
            for rc, output in [(1, ""), (0, "## mine/a.o\nT f\n")]:
                with self.subTest(rc=rc, output=output):
                    done = subprocess.CompletedProcess([], rc, output, "symbol reader failed")
                    with patch.object(substitute.subprocess, "run", return_value=done):
                        with self.assertRaises(RuntimeError):
                            substitute.defined_symbols(objects, root)

    def test_successful_empty_objects_remain_valid_symbol_measurements(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            done = subprocess.CompletedProcess([], 0, "## mine/a.o\n## theirs/a.o\n", "")
            with patch.object(substitute.subprocess, "run", return_value=done):
                self.assertEqual(substitute.defined_symbols(
                    [("mine/a.o", root / "a.o"), ("theirs/a.o", root / "a.o")], root
                ), {"mine/a.o": [], "theirs/a.o": []})

    def test_symbol_parity_retains_the_measured_binding_kind(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("asm", "reference"):
                (root / name).mkdir()
                (root / name / "a.o").write_bytes(b"object fixture")
            rows = [{"unit": "a.o", "outcome": "assembled"}]
            table = {"mine/a.o": ["T strong", "W weak", "V object"],
                     "theirs/a.o": ["T strong", "W weak", "V object"]}
            with patch.object(substitute, "defined_symbols", return_value=table):
                substitute.check_symbol_parity(
                    rows, root / "asm", root / "reference", root / "symbols"
                )
            self.assertEqual(rows[0].get("definedSymbolKinds"),
                             {"strong": "T", "weak": "W", "object": "V"})
            self.assertEqual(rows[0]["missingSymbols"], [])
            self.assertEqual(rows[0]["extraSymbols"], [])
