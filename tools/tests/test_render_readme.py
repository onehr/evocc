"""The README and the paper show the proof lines that tools/render-readme.py expects."""

import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("render_readme", ROOT / "tools/render-readme.py")
render = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(render)


class ProvePanelTests(unittest.TestCase):
    def test_documents_show_the_expected_lines_of_the_checked_abs(self):
        expected = render.PROVE_ASSEMBLY.splitlines()
        self.assertEqual(len(expected), 3)
        for name in ("README.md", "docs/paper.md"):
            with self.subTest(document=name):
                text = (ROOT / name).read_text(encoding="utf-8")
                block = text.split("```asm\n", 1)[1].split("```", 1)[0].splitlines()
                self.assertEqual([line for line in block if line in expected], expected)


if __name__ == "__main__":
    unittest.main()
