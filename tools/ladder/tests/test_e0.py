"""Benchmark preparation must not change the program being tested."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "ladder_e0", Path(__file__).resolve().parents[1] / "e0.py"
)
e0 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(e0)


class SourceIdentityTest(unittest.TestCase):
    def test_refusals_preserve_the_first_error_source_location(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            (work / "src").mkdir()
            (work / "asm").mkdir()
            (work / "src/probe.c").write_text("int main(void){return 0;}\n")
            first = "/sysroot/include/header.h:12:9: error[E1001]: expected-declaration"
            stderr = "warning: unrelated\n" + first + "\nprobe.c:3:2: error[E2001]: later\n"
            with patch.object(e0, "sh", return_value=SimpleNamespace(returncode=1, stdout="", stderr=stderr)):
                row = e0.emit(Path("/compiler"), work, ["probe"], [])["probe"]
            self.assertEqual(row["code"], "E1001")
            self.assertEqual(row.get("firstDiagnostic"), first)

    def test_emission_consumes_only_supported_unconditional_options(self):
        for directive, applied, unhandled in [
            ('/* { dg-options "-fwrapv" } */', ["-fwrapv"], []),
            ('/* { dg-options "-fno-builtin -fbuiltin" } */', ["-fno-builtin", "-fbuiltin"], []),
            ('/* { dg-options "-fbuiltin -fno-builtin" } */', ["-fbuiltin", "-fno-builtin"], []),
            ('/* { dg-options { "-fwrapv" } } */', ["-fwrapv"], []),
            ('/* { dg-additional-options "-fno-strict-overflow" } */', ["-fno-strict-overflow"], []),
            ('/* { dg-options "-Wno-psabi -fwrapv" } */', ["-fwrapv"], ["-Wno-psabi"]),
            ('/* { dg-options "-fwrapv" { target x86_64-*-* } } */', [], ["target"]),
            ('char *s="/* { dg-options \\\"-fwrapv\\\" } */";', [], []),
        ]:
            with self.subTest(directive=directive), tempfile.TemporaryDirectory() as tmp:
                work = Path(tmp)
                (work / "src").mkdir()
                (work / "asm").mkdir()
                (work / "src/probe.c").write_text(directive + "\nint main(void){return 0;}\n")
                with patch.object(e0, "sh", return_value=SimpleNamespace(returncode=0, stdout="ret\n", stderr="")) as run:
                    rows = e0.emit(Path("/compiler"), work, ["probe"], [])
                argv = run.call_args.args[0]
                self.assertEqual([arg for arg in argv if arg.startswith("-f")], applied)
                options = rows["probe"].get("options", {})
                self.assertEqual(options.get("applied"), applied)
                for item in unhandled:
                    self.assertIn(item, str(options.get("unhandled", [])))
                if not unhandled:
                    self.assertEqual(options.get("unhandled"), [])

    def test_failure_interpretation_uses_the_same_arithmetic_options(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            (work / "src").mkdir()
            (work / "src/probe.c").write_text('/* { dg-additional-options "-fwrapv" } */\nint main(void){return 0;}\n')
            with patch.object(e0, "sh", return_value=SimpleNamespace(returncode=0, stdout="0\n", stderr="")) as run:
                e0.interpreter_verdict(Path("/compiler"), work, "probe", [])
            self.assertIn("-fwrapv", run.call_args.args[0])

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
