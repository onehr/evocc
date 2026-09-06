"""The mixed-link machinery must decode a build and narrow a failure exactly.

Three things here are worth a test because getting them wrong is silent. A
misparsed command log drops units from the denominator; a mistranslated argv
hands the front end a different program than the reference compiler saw; and
a narrowing that stops at the first minimal failing set reports a passing
build that does not pass.
"""

from __future__ import annotations

from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import mixed  # noqa: E402


def record(cwd: str, *argv: str) -> bytes:
    return b"\x00".join(part.encode() for part in (cwd, *argv)) + b"\x00\x01\n"


class ParseCommands(unittest.TestCase):
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
        self.assertEqual(omitted, ["-O2", "-Wall", "-fPIC"])

    def test_the_output_operand_never_reaches_the_front_end(self):
        unit = self.unit("-c", "a.c", "-o", "obj/a.o", "-MF", "dep.d")
        command, omitted = mixed.evocc_command(unit, Path("/evocc"), "t", Path("/host"))
        self.assertNotIn("obj/a.o", command)
        self.assertNotIn("dep.d", command)
        self.assertEqual(omitted, ["-o", "-MF"])


class SymbolDrift(unittest.TestCase):
    def test_units_sharing_a_drifted_symbol_cannot_both_be_installed(self):
        keep, excluded = mixed.compatible_units(
            {"a.o": ["is_space"], "b.o": ["is_space"], "c.o": ["other"]}
        )
        self.assertEqual(keep, ["a.o", "c.o"])
        self.assertIn("b.o", excluded)
        self.assertIn("a.o", excluded["b.o"])

    def test_a_unit_without_drift_is_always_kept(self):
        keep, excluded = mixed.compatible_units({"a.o": [], "b.o": []})
        self.assertEqual(keep, ["a.o", "b.o"])
        self.assertEqual(excluded, {})


class Specifiers(unittest.TestCase):
    def test_a_declaration_loses_the_specifier_but_data_keeps_it(self):
        with tempfile.TemporaryDirectory() as directory:
            tree = Path(directory)
            (tree / "a.c").write_text(
                '_Noreturn void die(void);\n'
                'const char *name = "_Noreturn";\n'
                '#define _Noreturn __attribute__((__noreturn__))\n'
            )
            self.assertEqual(mixed.strip_specifiers(tree), ["a.c"])
            text = (tree / "a.c").read_text()
            self.assertTrue(text.startswith(" void die(void);"))
            self.assertIn('"_Noreturn"', text)
            self.assertIn("#define _Noreturn __attribute__", text)


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


class PageBreaks(unittest.TestCase):
    def test_form_feeds_and_vertical_tabs_are_removed(self):
        with tempfile.TemporaryDirectory() as directory:
            tree = Path(directory)
            (tree / "a.c").write_text("int main(void)\n{\n\f\v  return 0;\n}\n")
            (tree / "b.c").write_text("int f(void);\n")
            touched = mixed.strip_page_breaks(tree)
            self.assertEqual(touched, ["a.c"])
            self.assertNotIn("\f", (tree / "a.c").read_text())


if __name__ == "__main__":
    unittest.main()
