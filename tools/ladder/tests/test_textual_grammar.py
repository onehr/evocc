"""The shape hint must distinguish a complete summary from a reader prefix."""

import importlib.util
from pathlib import Path
import unittest


path = Path(__file__).resolve().parents[2] / "check-textual-grammar.py"
spec = importlib.util.spec_from_file_location("textual_grammar", path)
grammar = importlib.util.module_from_spec(spec)
spec.loader.exec_module(grammar)


class TextualGrammarTest(unittest.TestCase):
    def test_effect_prefix_is_not_an_emitted_summary(self):
        self.assertEqual(grammar.renderer_shapes('line.starts_with("    effects { ")'), set())

    def test_complete_effect_summary_is_still_a_shape(self):
        form = "effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}"
        self.assertEqual(len(grammar.renderer_shapes(f'"{form}"')), 1)

    def test_fixed_instruction_without_a_placeholder_is_not_lost(self):
        self.assertEqual(grammar.renderer_shapes('"ret"'), {"ret"})


if __name__ == "__main__":
    unittest.main()
