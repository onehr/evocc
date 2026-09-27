"""The mixed-link machinery must decode a build and narrow a failure exactly.

Three things here are worth a test because getting them wrong is silent. A
misparsed command log drops units from the denominator; a mistranslated argv
hands the front end a different program than the reference compiler saw; and
a narrowing that stops at the first minimal failing set reports a passing
build that does not pass.
"""

from __future__ import annotations

from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import mixed  # noqa: E402
import musl  # noqa: E402
import tinycc  # noqa: E402
import toybox  # noqa: E402


def record(cwd: str, *argv: str) -> bytes:
    return b"\x00".join(part.encode() for part in (cwd, *argv)) + b"\x00\x01\n"


class ParseCommands(unittest.TestCase):
    def test_container_capability_is_explicit_and_scoped(self):
        with patch.object(mixed.subprocess, "run") as run:
            mixed.container({}, "true")
            self.assertNotIn("--cap-add", run.call_args.args[0])
            mixed.container({}, "true", capabilities=("CAP_LINUX_IMMUTABLE",))
            argv = run.call_args.args[0]
            self.assertEqual(argv[argv.index("--cap-add") + 1], "CAP_LINUX_IMMUTABLE")
            self.assertNotIn("ALL", argv)

    def test_recorder_locks_the_entire_multi_write_record(self):
        # O_APPEND makes individual writes atomic, not a shell printf group.
        # The sealed image provides flock; the fd must span both writes.
        script = mixed.RECORDER
        self.assertIn('} 9>> "$EVO_CMDLOG"', script)
        self.assertLess(script.index('flock -x 9'), script.index("printf"))
        self.assertIn("|| exit", script)
        self.assertIn(">&9", script)

    def log(self, *records: bytes) -> Path:
        handle = tempfile.NamedTemporaryFile(suffix=".log", delete=False)
        handle.write(b"".join(records))
        handle.close()
        return Path(handle.name)

    def test_keeps_one_c_compile(self):
        log = self.log(record("/work/t", "-c", "a.c", "-o", "a.o", "-I.", "-DX=1"))
        units = mixed.parse_commands(log, "/work/t")
        self.assertEqual([unit.name for unit in units], ["a.o"])
        self.assertEqual(units[0].source, "a.c")

    def test_names_are_relative_to_the_mount(self):
        log = self.log(record("/work/t/sub", "-c", "b.c", "-o", "../obj/b.o"))
        units = mixed.parse_commands(log, "/work/t")
        self.assertEqual([unit.name for unit in units], ["obj/b.o"])

    def test_drops_links_preprocessing_and_probes(self):
        log = self.log(
            record("/work/t", "conftest.c", "-o", "a.out"),
            record("/work/t", "-E", "-c", "x.c", "-o", "x.o"),
            record("/work/t", "-c", "x.S", "-o", "x.o"),
            record("/work/t", "-c", "keep.c", "-o", "keep.o"),
        )
        units = mixed.parse_commands(log, "/work/t")
        self.assertEqual([unit.name for unit in units], ["keep.o"])

    def test_a_rebuild_supersedes_the_record_it_replaced(self):
        log = self.log(
            record("/work/t", "-c", "a.c", "-o", "a.o", "-DFIRST"),
            record("/work/t", "-c", "a.c", "-o", "a.o", "-DSECOND"),
        )
        units = mixed.parse_commands(log, "/work/t")
        self.assertEqual(len(units), 1)
        self.assertIn("-DSECOND", units[0].argv)

    def test_a_parallel_build_writes_whole_records(self):
        log = self.log(
            record("/work/t", "-c", "a.c", "-o", "a.o"),
            record("/work/t", "-c", "b.c", "-o", "b.o"),
        )
        self.assertEqual(
            [unit.name for unit in mixed.parse_commands(log, "/work/t")], ["a.o", "b.o"]
        )


class TranslateCommand(unittest.TestCase):
    def unit(self, *argv: str) -> mixed.Unit:
        return mixed.Unit(
            cwd="/work/t", argv=argv, source="a.c", obj="a.o", root="/work/t"
        )

    def test_include_paths_are_rewritten_to_the_host(self):
        unit = self.unit("-c", "a.c", "-o", "a.o", "-I/work/t/include", "-Isub")
        command, _ = mixed.evocc_command(unit, Path("/evocc"), "t", Path("/host/tree"))
        self.assertIn("-I/host/tree/include", command)
        self.assertIn("-I/host/tree/sub", command)
        self.assertEqual(command[-1], "/host/tree/a.c")

    def test_isystem_and_iquote_become_include_paths_in_order(self):
        unit = self.unit("-c", "a.c", "-iquote", "q", "-isystem", "/work/t/s")
        command, _ = mixed.evocc_command(unit, Path("/evocc"), "t", Path("/host/tree"))
        paths = [
            command[index + 1] for index, word in enumerate(command) if word == "-I"
        ]
        self.assertEqual(paths, ["/host/tree/q", "/host/tree/s"])

    def test_definitions_survive_and_unknown_options_are_named(self):
        unit = self.unit("-c", "a.c", "-DX=1", "-U", "Y", "-O2", "-Wall", "-fPIC")
        command, omitted = mixed.evocc_command(unit, Path("/evocc"), "t", Path("/host"))
        self.assertIn("-DX=1", command)
        self.assertEqual(command[command.index("-U") + 1], "Y")
        self.assertEqual(omitted, ["-Wall"])
        self.assertLess(command.index("-O2"), command.index("-fPIC"))

    def test_supported_language_and_machine_options_keep_written_overrides(self):
        flags = [
            '-fno-strict-aliasing',
            '-fexcess-precision=standard',
            '-ffunction-sections', '-fno-function-sections',
            '-fdata-sections', '-fno-data-sections',
            '-fPIC', '-fno-pic', '-fwrapv', '-fno-wrapv',
            '-std=gnu89', '-fno-gnu89-inline', '-fsigned-char', '-funsigned-char',
            '-fstack-protector-strong', '-fno-stack-protector',
            '-mbranch-protection=pac-ret', '-mbranch-protection=none',
            '-ftls-model=global-dynamic', '-mstrict-align',
            '-mstack-protector-guard-reg=sp_el0', '-mstack-protector-guard-offset=1240',
            '-fmacro-prefix-map=/build=.',
        ]
        command, omitted = mixed.evocc_command(
            self.unit('-c', 'a.c', *flags), Path('/evocc'), 't', Path('/host'))
        self.assertEqual([word for word in command if word in flags], flags)
        self.assertEqual(omitted, [])

    def test_an_unsupported_language_mode_reaches_the_driver_instead_of_changing_dialect(self):
        command, omitted = mixed.evocc_command(
            self.unit('-c', 'a.c', '-std=c99'), Path('/evocc'), 't', Path('/host'))
        self.assertIn('-std=c99', command)
        self.assertEqual(omitted, [])

    def test_the_output_operand_never_reaches_the_front_end(self):
        unit = self.unit("-c", "a.c", "-o", "obj/a.o", "-MF", "dep.d")
        command, omitted = mixed.evocc_command(unit, Path("/evocc"), "t", Path("/host"))
        self.assertNotIn("obj/a.o", command)
        self.assertNotIn("dep.d", command)
        self.assertEqual(omitted, ["-o", "-MF"])

    def test_assembler_options_keep_both_spellings_and_written_override_order(self):
        flags = ['-Xassembler', '--execstack', '-Wa,--noexecstack',
                 '-Xassembler', '--defsym', '-Xassembler', 'guard=1']
        unit = self.unit('-c', 'a.c', *flags, '-o', 'a.o')
        self.assertEqual(mixed.assembler_flags(unit), flags)

    def test_missing_assembler_operand_is_not_silently_discarded(self):
        with self.assertRaises(ValueError):
            mixed.assembler_flags(self.unit('-c', 'a.c', '-Xassembler'))

    def test_assembly_plan_preserves_exact_arguments_without_shell_splitting(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            flags = ['-Xassembler', '--defsym', '-Xassembler', 'label with space=1',
                     '-Wa,--noexecstack']
            unit = self.unit('-c', 'a.c', *flags)

            def run(*args, **kwargs):
                plan = json.loads((root / 'plan.json').read_text())
                self.assertEqual(plan[0]['flags'], flags)
                return subprocess.CompletedProcess([], 0, 'a.o ok\n', '')

            with patch.object(mixed, 'container', side_effect=run):
                result = mixed.assemble([{'unit': 'a.o', 'outcome': 'emitted',
                                          'assembly': 'a.s'}], {'a.o': unit}, root)
            self.assertEqual(result, {'a.o': 'ok'})

    def test_failed_assembly_runner_cannot_publish_partial_success(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(mixed, 'container', return_value=
                              subprocess.CompletedProcess([], 7, 'a.o ok\n', 'failed')):
                result = mixed.assemble([{'unit': 'a.o', 'outcome': 'emitted',
                                          'assembly': 'a.s'}], {'a.o': self.unit('-c', 'a.c')},
                                        Path(directory))
            self.assertEqual(result, {})

    def test_assembly_runner_delivers_literal_argv_to_the_tool(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            flags = ['-Xassembler', 'label with space=1', '-Xassembler',
                     '$(touch injected)', '-Wa,--noexecstack']
            (root / 'plan.json').write_text(json.dumps([
                {'unit': 'a.o', 'assembly': 'input file.s', 'object': 'a.o',
                 'flags': flags}]))
            tool = root / 'gcc-14'
            tool.write_text(f'#!{sys.executable}\nimport json, sys\n'
                            'from pathlib import Path\n'
                            'Path("argv.json").write_text(json.dumps(sys.argv[1:]))\n')
            tool.chmod(0o755)
            script = mixed.ASSEMBLE.replace('cd /work/asm || exit 1', '')
            done = subprocess.run(['sh', '-c', script], cwd=root,
                                  env={**os.environ, 'PATH': directory + os.pathsep + os.environ['PATH']},
                                  text=True, capture_output=True, timeout=10)
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertEqual(json.loads((root / 'argv.json').read_text()),
                             [*flags, '-c', '-x', 'assembler', 'input file.s', '-o', 'obj/a.o'])
            self.assertEqual(mixed._assembly_verdicts(done.stdout), {'a.o': 'ok'})
            self.assertFalse((root / 'injected').exists())


def parity(added=(), missing=()):
    return {"added": list(added), "missing": list(missing)}


class SymbolParity(unittest.TestCase):
    def test_units_adding_the_same_symbol_cannot_both_be_installed(self):
        keep, excluded = mixed.compatible_units(
            {
                "a.o": parity(added=["is_space"]),
                "b.o": parity(added=["is_space"]),
                "c.o": parity(added=["other"]),
            }
        )
        self.assertEqual(keep, ["a.o", "c.o"])
        self.assertIn("a.o", excluded["b.o"])

    def test_a_unit_that_loses_a_symbol_is_set_aside(self):
        keep, excluded = mixed.compatible_units(
            {"a.o": parity(missing=["environ"]), "b.o": parity()}
        )
        self.assertEqual(keep, ["b.o"])
        self.assertEqual(excluded["a.o"], "does not define environ")

    def test_a_unit_at_parity_is_always_kept(self):
        keep, excluded = mixed.compatible_units({"a.o": parity(), "b.o": parity()})
        self.assertEqual(keep, ["a.o", "b.o"])
        self.assertEqual(excluded, {})


class PreparationPreservesSource(unittest.TestCase):
    def test_exported_headers_keep_include_next_and_whitespace(self):
        self.assertTrue(callable(getattr(mixed, 'exported_sysroot', None)),
                        'mixed lanes must reuse the unmodified shared export')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            wrapper = root / "gccinc/stdint.h"
            header = root / "usr/include/example.h"
            types = root / 'usr/include/aarch64-linux-gnu/bits/floatn-common.h'
            vectors = root / 'usr/include/aarch64-linux-gnu/bits/math-vector.h'
            wrapper.parent.mkdir(parents=True)
            header.parent.mkdir(parents=True)
            types.parent.mkdir(parents=True)
            wrapper.write_bytes(b'#include_next <stdint.h>\n')
            header.write_bytes(b'unsigned\flong number;\n')
            types.write_bytes(b'#define __HAVE_FLOAT32 1\n#define __HAVE_FLOAT64 1\n')
            vectors.write_bytes(b'#if __GNUC_PREREQ(9, 0)\n#include <arm_neon.h>\n#endif\n')
            with patch.object(mixed, 'exported_sysroot', return_value=root):
                actual, flags = mixed.ladder_sysroot()
            self.assertEqual(actual, root)
            self.assertEqual(flags, mixed.include_flags(root))
            self.assertEqual(mixed.SYSROOT_ADJUSTMENTS, [])
            self.assertTrue(wrapper.is_file())
            self.assertEqual(wrapper.read_bytes(), b'#include_next <stdint.h>\n')
            self.assertEqual(header.read_bytes(), b'unsigned\flong number;\n')
            self.assertEqual(types.read_bytes(), b'#define __HAVE_FLOAT32 1\n#define __HAVE_FLOAT64 1\n')
            self.assertEqual(vectors.read_bytes(), b'#if __GNUC_PREREQ(9, 0)\n#include <arm_neon.h>\n#endif\n')

    def test_variable_length_profile_macro_comes_from_the_compiler(self):
        self.assertFalse(any('__STDC_NO_VLA__' in flag for flag in mixed.SYSROOT_DEFINES))
        self.assertFalse(any('__extension__' in flag for flag in mixed.SYSROOT_DEFINES))

    def test_supported_specifiers_and_whitespace_are_not_deleted(self):
        contents = (
            b'_Noreturn void stop(void);\n'
            b'unsigned\flong number;\n'
            b'const char *spelling = "_Noreturn";\n'
            b'int function(void) {\v return 0; }\n'
            b'/* raw \xff */ const char *bytes = "\xff\xc0\xaf";\n'
        )
        for adapter in [musl, tinycc, toybox]:
            with self.subTest(adapter=adapter.__name__):
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    source, build, tests = root / "source", root / "build", root / "tests"
                    for path in [source, build, tests]:
                        path.mkdir()
                    (source / "fixture.c").write_bytes(contents)
                    with patch.object(mixed, "install_recorder"):
                        args = (build, source, tests) if adapter is musl else (build, source)
                        prepared = adapter.prepare(*args)[0]
                    self.assertEqual((prepared / "fixture.c").read_bytes(), contents)
                    self.assertEqual((source / "fixture.c").read_bytes(), contents)


class TinyccVerdict(unittest.TestCase):
    def test_rebuilt_or_missing_subject_objects_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            obj = root / 'compiler.o'
            obj.write_bytes(b'subject')
            expected = {'compiler.o': mixed.sha256(obj)}
            done = subprocess.CompletedProcess([], 0, 'tests-passed\n', '')
            with patch.object(mixed, 'container', return_value=done):
                self.assertEqual(tinycc.run_tests(root, expected), (True, 'tests-passed'))

            def rebuild(*args, **kwargs):
                obj.write_bytes(b'reference')
                return done

            with patch.object(mixed, 'container', side_effect=rebuild):
                self.assertEqual(tinycc.run_tests(root, expected), (False, 'object-mismatch-after-test'))
            obj.unlink()
            with patch.object(mixed, 'container') as run:
                self.assertEqual(tinycc.run_tests(root, expected), (False, 'object-mismatch-before-test'))
                run.assert_not_called()

    def test_container_failure_overrides_a_successful_inner_marker(self):
        done = subprocess.CompletedProcess([], 9, 'tests-passed\n', '')
        with patch.object(mixed, 'container', return_value=done):
            self.assertEqual(tinycc.run_tests(Path('/unused')), (False, 'runner-exit-9'))

    def test_success_marker_does_not_override_relink_or_test_exit_status(self):
        for relink_status, test_status, marker, passed in [
            (0, 0, '------- ALL TESTS PASSED --------', True),
            (2, 0, '------- ALL TESTS PASSED --------', False),
            (0, 7, '------- ALL TESTS PASSED --------', False),
            (0, 0, 'prefix ALL TESTS PASSED but truncated', False),
            (0, 0, '', False),
        ]:
            with self.subTest(relink=relink_status, tests=test_status, marker=marker), \
                    tempfile.TemporaryDirectory() as directory:
                root = Path(directory)

                def local_container(mounts, script, timeout):
                    stub = f'''make() {{
                        if [ "$1" = tcc ]; then
                            touch tcc; chmod +x tcc; return {relink_status}
                        fi
                        printf '%s\\n' '{marker}'
                        return {test_status}
                    }}
                    '''
                    return subprocess.run(['bash', '-c', stub + script.replace('/work/tinycc', str(root))],
                                          capture_output=True, text=True, timeout=10, check=False)

                with patch.object(mixed, 'container', side_effect=local_container):
                    self.assertEqual(tinycc.run_tests(root)[0], passed)


class Substitution(unittest.TestCase):
    def test_every_reference_object_comes_back_whatever_its_suffix(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "tree"
            (root / "obj").mkdir(parents=True)
            (root / "obj" / "a.lo").write_bytes(b"reference-lo")
            (root / "obj" / "b.o").write_bytes(b"reference-o")
            asm = Path(directory) / "asm"
            (asm / "obj").mkdir(parents=True)
            (asm / "obj" / "obj__a.lo.o").write_bytes(b"evocc-lo")
            (asm / "obj" / "obj__b.o.o").write_bytes(b"evocc-o")
            tree = mixed.Tree(root, Path(directory) / "pristine")
            installed = mixed.substitute(tree, ["obj/a.lo", "obj/b.o"], asm)
            self.assertEqual(installed, ["obj/a.lo", "obj/b.o"])
            self.assertEqual((root / "obj" / "a.lo").read_bytes(), b"evocc-lo")
            mixed.substitute(tree, [], asm)
            self.assertEqual((root / "obj" / "a.lo").read_bytes(), b"reference-lo")
            self.assertEqual((root / "obj" / "b.o").read_bytes(), b"reference-o")


class FlatNames(unittest.TestCase):
    def test_two_unit_names_can_flatten_to_the_same_spelling(self):
        self.assertEqual(
            mixed.flat_name("obj/src/env/__environ.lo"),
            mixed.flat_name("obj/src/env____environ.lo"),
        )

    def test_an_assembly_verdict_is_read_back_by_unit_not_by_object(self):
        verdicts = mixed._assembly_verdicts(
            "obj/src/env/__environ.lo ok\nobj/src/foo.lo assembler-refused\n"
        )
        self.assertEqual(
            verdicts,
            {"obj/src/env/__environ.lo": "ok", "obj/src/foo.lo": "assembler-refused"},
        )


class Narrow(unittest.TestCase):
    def journal(self) -> Path:
        handle = tempfile.NamedTemporaryFile(suffix=".jsonl", delete=False)
        handle.close()
        return Path(handle.name)

    def test_a_passing_set_is_returned_untouched(self):
        result = mixed.narrow(["a", "b"], lambda units: (False, "ok"), self.journal())
        self.assertEqual(result["passingSet"], ["a", "b"])
        self.assertEqual(result["failingUnits"], [])
        self.assertEqual(len(result["rounds"]), 1)

    def test_one_guilty_unit_is_found_and_the_rest_are_kept(self):
        def fails(units):
            return ("guilty" in units), "boom" if "guilty" in units else "ok"

        result = mixed.narrow(
            ["a", "b", "guilty", "c"], fails, self.journal()
        )
        self.assertEqual(result["failingUnits"], ["guilty"])
        self.assertEqual(sorted(result["passingSet"]), ["a", "b", "c"])

    def test_two_independent_culprits_are_both_removed(self):
        def fails(units):
            bad = [unit for unit in units if unit.startswith("bad")]
            return bool(bad), ",".join(bad) or "ok"

        result = mixed.narrow(
            ["a", "bad1", "b", "bad2", "c"], fails, self.journal()
        )
        self.assertEqual(sorted(result["failingUnits"]), ["bad1", "bad2"])
        self.assertEqual(sorted(result["passingSet"]), ["a", "b", "c"])

    def test_an_interaction_between_two_units_is_reported_as_a_pair(self):
        def fails(units):
            together = {"x", "y"} <= set(units)
            return together, "pair" if together else "ok"

        result = mixed.narrow(["x", "p", "y", "q"], fails, self.journal())
        self.assertEqual(sorted(result["failingUnits"]), ["x", "y"])
        self.assertNotIn("x", result["passingSet"])

    def test_the_budget_stops_a_runaway_search(self):
        result = mixed.narrow(
            [f"u{index}" for index in range(64)],
            lambda units: (True, "always"),
            self.journal(),
            budget=3,
        )
        self.assertTrue(result["budgetExhausted"])
        self.assertEqual(result["passingSet"], [])

    def test_an_exhausted_budget_still_reports_the_largest_passing_set(self):
        def fails(units):
            bad = [unit for unit in units if unit != "good"]
            return bool(bad), "always" if bad else "ok"

        result = mixed.narrow(
            [f"u{index}" for index in range(64)] + ["good"],
            fails,
            self.journal(),
            budget=6,
        )
        self.assertTrue(result["budgetExhausted"])
        self.assertNotIn("u0", result["passingSet"])


class LinkDiagnostics(unittest.TestCase):
    def test_multiple_definition_symbols_are_extracted(self):
        log = (
            "/usr/bin/ld: libtcc.a(arm64-asm.o): in function `is_space':\n"
            "(.text+0xa0): multiple definition of `is_space'; tcc.o:(.text+0xa0):"
            " first defined here\n"
        )
        self.assertEqual(mixed.multiple_definition_symbols(log), {"is_space"})


if __name__ == "__main__":
    unittest.main()
