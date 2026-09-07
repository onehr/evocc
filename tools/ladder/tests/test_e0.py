"""Benchmark preparation must not change the program being tested."""

import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "ladder_e0", Path(__file__).resolve().parents[1] / "e0.py"
)
e0 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(e0)


class SourceIdentityTest(unittest.TestCase):
    def test_preparation_preserves_original_bytes_and_function_definitions(self):
        for source in [
            b'unsigned char g[] = "\\0\xff"; int main(void){return 0;}\n',
            b"int main(){return 0;}\n",
            b'/* main() { is only a comment */\nchar *p="main() {";\n',
        ]:
            with self.subTest(source=source), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                inputs = root / "input"
                inputs.mkdir()
                (inputs / "probe.c").write_bytes(source)
                (inputs / "probe.c.expected").write_bytes(b"answer\n")
                work = root / "work"
                names, adjusted = e0.prepare("torture", inputs, work, None)
                self.assertEqual(names, ["probe"])
                self.assertEqual((work / "src/probe.c").read_bytes(), source)
                self.assertEqual((work / "src/probe.expected").read_bytes(), b"answer\n")
                self.assertEqual(adjusted, 0)
