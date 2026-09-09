"""Keep linker binding facts when comparing the original and emitted objects."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

_spec = importlib.util.spec_from_file_location(
    "k1_substitute", Path(__file__).resolve().parents[1] / "substitute.py"
)
substitute = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(substitute)


class SymbolBindingsTest(unittest.TestCase):
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
