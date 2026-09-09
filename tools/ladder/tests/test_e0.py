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
    def test_irrelevant_foreign_predicates_do_not_block_a_selector(self):
        self.assertFalse(e0.target_matches('nvptx-*-* && { ! nvptx_softstack }'))
        self.assertTrue(e0.target_matches('aarch64*-*-* || unknown_foreign_predicate'))
        with self.assertRaises(ValueError):
            e0.target_matches('aarch64*-*-* && unknown_required_predicate')

    def test_measured_target_requirements_do_not_stand_for_compiler_coverage(self):
        requirements = [
            'trampolines', 'fileio', 'mmap', 'signal', 'fpic', 'c99_runtime',
            'stdint_types', 'double64plus', 'unwrapped', 'run_expensive_tests', 'dfp', 'dfprt',
        ]
        for requirement in requirements:
            with self.subTest(requirement=requirement):
                self.assertTrue(e0.target_matches(requirement))
        self.assertFalse(e0.target_matches('newlib_nano_io'))
        self.assertFalse(e0.target_matches('stack_size'))

    def test_original_board_requirements_and_annotations_are_consumed(self):
        source = '''/* { dg-add-options stack_size } */
/* { dg-add-options ieee } */
/* { dg-require-stack-size "100000 * 4 + 1024" } */
/* { dg-require-weak "" } */
/* { dg-require-alias "" } */
/* { dg-xfail-run-if "foreign platform" { nvptx-*-* } } */
/* { dg-prune-output "unused warning" } */
int main(void){return 0;}
'''
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'probe.c'
            path.write_text(source)
            options = e0.program_options(path)
        self.assertEqual(options['unhandled'], [])
        self.assertEqual(options.get('applicability', []), [])
        self.assertEqual(options['applied'], [])
        self.assertIn('unused warning', str(options.get('annotations')))
        self.assertIn('stack_size', str(options.get('annotations')))

    def test_stack_requirements_are_bounded_arithmetic_not_executable_expressions(self):
        for amount, status in [('0x80100', None), ('8*100*100', None), ('1 + 8388608', False), ('__import__("os")', True)]:
            with self.subTest(amount=amount), tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp) / 'probe.c'
                path.write_text('/* { dg-require-stack-size {' + amount + '} } */')
                options = e0.program_options(path)
                reasons = options.get('applicability', [])
                if status is None:
                    self.assertEqual(reasons, [])
                else:
                    self.assertEqual(len(reasons), 1)
                    self.assertEqual(bool(reasons[0].get('unresolved')), status)

    def test_expected_failure_never_turns_a_failed_execution_into_pass_or_exclusion(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            (work / 'src').mkdir()
            (work / 'asm').mkdir()
            (work / 'src/probe.c').write_text('/* { dg-xfail-run-if "known issue" { aarch64*-*-* } } */\nint main(void){return 1;}')
            with patch.object(e0, 'sh', return_value=SimpleNamespace(returncode=0, stdout='ret\n', stderr='')):
                row = e0.emit(Path('/compiler'), work, ['probe'], [])['probe']
            self.assertEqual(row['emit'], 'emitted')
            self.assertIn('known issue', str(row['options'].get('annotations')))
            with patch.object(e0, 'sh', return_value=SimpleNamespace(returncode=0, stdout='probe fail rc=1\n', stderr='')):
                self.assertEqual(e0.run_in_container(work)['probe'], {'run': 'fail', 'exit': 1})

    def test_declared_timeout_factor_reaches_every_execution_without_changing_defaults(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            (work / 'src').mkdir()
            (work / 'asm').mkdir()
            (work / 'src/probe.c').write_text('/* { dg-timeout-factor 8 } */\nint main(void){return 0;}')
            with patch.object(e0, 'sh', return_value=SimpleNamespace(returncode=0, stdout='ret\n', stderr='')) as run:
                row = e0.emit(Path('/compiler'), work, ['probe'], [])['probe']
                self.assertEqual(row['emit'], 'emitted')
                self.assertEqual(run.call_args.kwargs['timeout'], 480)
                e0.interpreter_verdict(Path('/compiler'), work, 'probe', [])
                self.assertEqual(run.call_args.kwargs['timeout'], 240)
            with patch.object(e0, 'sh', return_value=SimpleNamespace(returncode=0, stdout='probe pass\n', stderr='')):
                e0.run_in_container(work)
            self.assertEqual((work / 'src/probe.timeout').read_text().strip(), '80')
            self.assertIn('timeout "$limit"', (work / 'run.sh').read_text())

    def test_inapplicable_requirements_cannot_be_reported_as_passes(self):
        for directive, reason in [
            ('/* { dg-options "-fwrapv -fnot-an-option" } */', '-fnot-an-option'),
            ('/* { dg-require-effective-target imaginary_target_feature } */', 'imaginary_target_feature'),
            ('/* { dg-skip-if "not this target" { aarch64*-*-* } } */', 'not this target'),
            ('/* { dg-do run { target x86_64-*-* } } */', 'x86_64-*-*'),
            ('/* { dg-do run { target { { aarch64*-*-* || x86_64-*-* } && x86_64-*-* } } } */', 'target selector'),
            ('/* { dg-do run { target { { aarch64*-*-* || x86_64-*-* } && x86_64-*-* || riscv*-*-* } } } */', 'target selector'),
        ]:
            with self.subTest(directive=directive), tempfile.TemporaryDirectory() as tmp:
                work = Path(tmp)
                (work / "src").mkdir()
                (work / "asm").mkdir()
                source = directive + "\nint main(void){return 0;}\n"
                (work / "src/probe.c").write_text(source)
                with patch.object(e0, "sh", return_value=SimpleNamespace(returncode=0, stdout="ret\n", stderr="")) as run:
                    row = e0.emit(Path("/compiler"), work, ["probe"], [])["probe"]
                expected = 'requirement-unresolved' if 'imaginary_target_feature' in directive else 'not-applicable'
                self.assertEqual(row.get("emit"), expected)
                self.assertIn(reason, str(row.get("applicability", {})))
                self.assertEqual(row.get("sourceSha256"), e0.hashlib.sha256(source.encode()).hexdigest())
                run.assert_not_called()

    def test_target_selected_options_and_skips_are_consumed(self):
        for directives, expected in [
            ('/* { dg-options "-fwrapv" { target aarch64*-*-* } } */', ['-fwrapv']),
            ('/* { dg-options "-fnot-an-option" { target x86_64-*-* } } */', []),
            ('/* { dg-skip-if "small alignment" { pdp11-*-* } } */', []),
            ('/* { dg-do run { target { aarch64*-*-* || x86_64-*-* } } } */', []),
            ('/* { dg-require-effective-target int128 } */', []),
            ('/* { dg-require-effective-target int32plus } */', []),
        ]:
            with self.subTest(directives=directives), tempfile.TemporaryDirectory() as tmp:
                work = Path(tmp)
                (work / "src").mkdir()
                (work / "asm").mkdir()
                (work / "src/probe.c").write_text(directives + "\nint main(void){return 0;}\n")
                with patch.object(e0, "sh", return_value=SimpleNamespace(returncode=0, stdout="ret\n", stderr="")) as run:
                    row = e0.emit(Path("/compiler"), work, ["probe"], [])["probe"]
                self.assertEqual(row.get("emit"), "emitted", row)
                self.assertEqual(row["options"]["applied"], expected)
                self.assertEqual([arg for arg in run.call_args.args[0] if arg.startswith('-f')], expected)

    def test_original_support_directory_reaches_emission_and_diagnosis(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp) / "run"
            inputs = Path(tmp) / "upstream/gcc/testsuite/gcc.c-torture/execute"
            inputs.mkdir(parents=True)
            (inputs / "helper.h").write_bytes(b"#define RESULT 7\n")
            (inputs / "probe.c").write_bytes(b'#include "helper.h"\nint main(void){return RESULT != 7;}\n')
            subject = Path(tmp) / "evocc"
            subject.write_bytes(b"subject")
            options = {"applied": [], "unhandled": []}
            with (
                patch.object(e0, "BUILD", work.parent),
                patch.object(e0, "EVIDENCE", Path(tmp) / "evidence"),
                patch.object(e0, "RECEIPT", Path(tmp) / "receipt.json"),
                patch.object(e0, "ROOT", Path(tmp)),
                patch.object(e0, "fetch", return_value=inputs),
                patch.object(e0, "sysroot", return_value=Path("/sysroot")),
                patch.object(e0, "sh", return_value=SimpleNamespace(stdout="source-commit")),
                patch.object(e0, "emit", return_value={"probe": {"emit": "emitted", "options": options}}) as emit,
                patch.object(e0, "run_in_container", return_value={"probe": {"run": "fail", "exit": 1}}),
                patch.object(e0, "interpreter_verdict", return_value="diagnostic") as diagnose,
                patch("sys.argv", ["e0", "--suite", "torture", "--evocc", str(subject)]),
            ):
                e0.main()
                self.assertIn(f"-I{inputs}", emit.call_args.args[3])
                self.assertIn(f"-I{inputs}", diagnose.call_args.args[3])
            work = work.parent / "run-torture"
            self.assertEqual((work / "src/probe.c").read_bytes(), (inputs / "probe.c").read_bytes())

    def test_sparse_checkout_supplies_quoted_cross_directory_sources(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "torture"
            (root / e0.PINS["torture"]["subdir"]).mkdir(parents=True)
            payload = b"/* same pinned repository, exact bytes */\nint helper;\n"
            with (
                patch.object(e0, "BUILD", Path(tmp)),
                patch.object(e0.subprocess, "run", return_value=SimpleNamespace(stdout=payload)),
            ):
                e0.fetch("torture")
            helper = root / "gcc/testsuite/gcc.dg/tree-ssa/pr105777.c"
            self.assertTrue(helper.is_file(), "the included file is outside the sparse subtree")
            self.assertEqual(helper.read_bytes(), payload)

    def test_language_mode_is_preserved_for_emission_and_diagnosis(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            (work / "src").mkdir()
            (work / "asm").mkdir()
            (work / "src/probe.c").write_text('/* { dg-options "-std=gnu89" } */\nmain(){return 0;}\n')
            with patch.object(e0, "sh", return_value=SimpleNamespace(returncode=0, stdout="ret\n", stderr="")) as run:
                rows = e0.emit(Path("/compiler"), work, ["probe"], [])
                self.assertIn("-std=gnu89", run.call_args.args[0])
            self.assertEqual(rows["probe"]["options"], {"applied": ["-std=gnu89"], "unhandled": []})
            with patch.object(e0, "sh", return_value=SimpleNamespace(returncode=0, stdout="0\n", stderr="")) as run:
                e0.interpreter_verdict(Path("/compiler"), work, "probe", [])
                self.assertIn("-std=gnu89", run.call_args.args[0])

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
            ('/* { dg-options "-fshort-wchar -fno-short-wchar" } */', ["-fshort-wchar", "-fno-short-wchar"], []),
            ('/* { dg-options { "-fwrapv" } } */', ["-fwrapv"], []),
            ('/* { dg-additional-options "-fno-strict-overflow" } */', ["-fno-strict-overflow"], []),
            ('/* { dg-options "-Wno-psabi -fwrapv" } */', ["-fwrapv"], ["-Wno-psabi"]),
            ('/* { dg-options "-fwrapv" { target x86_64-*-* } } */', [], []),
            ('char *s="/* { dg-options \\\"-fwrapv\\\" } */";', [], []),
        ]:
            with self.subTest(directive=directive), tempfile.TemporaryDirectory() as tmp:
                work = Path(tmp)
                (work / "src").mkdir()
                (work / "asm").mkdir()
                (work / "src/probe.c").write_text(directive + "\nint main(void){return 0;}\n")
                with patch.object(e0, "sh", return_value=SimpleNamespace(returncode=0, stdout="ret\n", stderr="")) as run:
                    rows = e0.emit(Path("/compiler"), work, ["probe"], [])
                options = rows["probe"].get("options", {})
                self.assertEqual(options.get("applied"), applied)
                if options.get('applicability'):
                    self.assertEqual(rows['probe']['emit'], 'not-applicable')
                    run.assert_not_called()
                else:
                    argv = run.call_args.args[0]
                    self.assertEqual([arg for arg in argv if arg.startswith("-f")], applied)
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
