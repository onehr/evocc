"""The ladder's decoders must read a build log the way the build meant it."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

# Loaded by path: the lane's directory holds files named for their subjects,
# and ``zlib`` is a standard library module, so that directory must not go on
# the search path.
_spec = importlib.util.spec_from_file_location(
    "ladder_subject", Path(__file__).resolve().parents[1] / "subject.py"
)
ladder = importlib.util.module_from_spec(_spec)
sys.modules["ladder_subject"] = ladder
_spec.loader.exec_module(ladder)

TREE = "/work/zstd/zstd-1.5.7"


class ParseBuildLogTest(unittest.TestCase):
    def test_a_flat_build_names_every_object_and_source(self) -> None:
        log = (
            "gcc -O3 -DHAVE_HIDDEN -I. -c -o adler32.o adler32.c\n"
            "gcc -O3 -I. -c -o example.o test/example.c\n"
            "ar rc libz.a adler32.o\n"
            "gcc -O3 -o example example.o -L. libz.a\n"
        )
        units = ladder.parse_build_log(log, TREE)
        self.assertEqual([unit.obj for unit in units], ["adler32.o", "example.o"])
        self.assertEqual(units[1].source, "test/example.c")
        # Neither the archive nor the link is a compilation.
        self.assertNotIn("libz.a", [unit.obj for unit in units])

    def test_a_recursive_build_resolves_paths_against_the_directory(self) -> None:
        log = (
            f"make[1]: Entering directory '{TREE}/lib'\n"
            "gcc -c -o common/entropy_common.o common/entropy_common.c\n"
            f"make[1]: Leaving directory '{TREE}/lib'\n"
            f"make[1]: Entering directory '{TREE}/programs'\n"
            "gcc -c -o fileio.o fileio.c\n"
            f"make[1]: Leaving directory '{TREE}/programs'\n"
        )
        units = ladder.parse_build_log(log, TREE)
        self.assertEqual(
            [(unit.obj, unit.source) for unit in units],
            [
                ("lib/common/entropy_common.o", "lib/common/entropy_common.c"),
                ("programs/fileio.o", "programs/fileio.c"),
            ],
        )

    def test_a_cd_in_the_recipe_moves_the_command(self) -> None:
        log = "cd lib && gcc -c -o zstd_compress.o compress/zstd_compress.c\n"
        (unit,) = ladder.parse_build_log(log, TREE)
        self.assertEqual(unit.obj, "lib/zstd_compress.o")
        self.assertEqual(unit.source, "lib/compress/zstd_compress.c")
        self.assertEqual(unit.directory, "lib")

    def test_absolute_paths_are_named_relative_to_the_tree(self) -> None:
        log = f"gcc -c -o {TREE}/lib/common/pool.o {TREE}/lib/common/pool.c\n"
        (unit,) = ladder.parse_build_log(log, TREE)
        self.assertEqual((unit.obj, unit.source), ("lib/common/pool.o", "lib/common/pool.c"))

    def test_a_recompiled_object_is_recorded_once_with_the_last_command(self) -> None:
        log = (
            "gcc -DFIRST -c -o a.o a.c\n"
            "gcc -DSECOND -c -o a.o a.c\n"
        )
        (unit,) = ladder.parse_build_log(log, TREE)
        self.assertIn("-DSECOND", unit.argv)

    def test_an_implicit_output_names_the_object_the_way_make_does(self) -> None:
        log = "gcc -std=c89 -c -fPIC -Wall cJSON.c\n"
        (unit,) = ladder.parse_build_log(log, TREE)
        self.assertEqual((unit.obj, unit.source), ("cJSON.o", "cJSON.c"))

    def test_an_implicit_output_lands_where_the_command_ran(self) -> None:
        log = "cd lib && gcc -c common/pool.c\n"
        (unit,) = ladder.parse_build_log(log, TREE)
        self.assertEqual((unit.obj, unit.source), ("lib/pool.o", "lib/common/pool.c"))

    def test_what_is_not_a_target_c_compilation_is_not_a_unit(self) -> None:
        log = (
            "gcc -o cjson_test cJSON.c test.c -lm\n"       # two sources, no -c
            "gcc -c -o crt.o crt.S\n"                      # assembly
            "gcc -c -o tool.o tool.cpp\n"                  # not C
            "gcc -E -o pp.i pp.c\n"                        # not a compilation
            "gcc -shared -o libz.so.1 adler32.o\n"         # a link
            "ranlib libz.a\n"
            "echo '  CC      lib/entropy_common.o'\n"      # a progress line
        )
        self.assertEqual(ladder.parse_build_log(log, TREE), [])

    def test_an_unparsable_line_is_skipped_rather_than_guessed_at(self) -> None:
        log = "gcc -c -o a.o 'unterminated\ngcc -c -o b.o b.c\n"
        self.assertEqual([unit.obj for unit in ladder.parse_build_log(log, TREE)], ["b.o"])


class EvoccArgvTest(unittest.TestCase):
    def setUp(self) -> None:
        self.subject = ladder.Subject(
            name="zstd", rung="E2", pin="v1.5.7", url="http://example/zstd.tar.gz",
            sha256="0" * 64, tree="zstd-1.5.7", build="make", test="make check",
            relink="make",
        )
        self.tree = Path("/host/zstd-1.5.7")

    def command(self, log: str) -> tuple[list[str], list[str]]:
        (unit,) = ladder.parse_build_log(log, TREE)
        return ladder.evocc_argv(unit, Path("/bin/evocc"), self.subject, self.tree, ["-I/sysroot"])

    def test_preprocessor_flags_are_carried_and_paths_translated(self) -> None:
        argv, omitted = self.command(
            f"gcc -O3 -Wall -DZSTD_MULTITHREAD -I{TREE}/lib/common -Ilib -c -o a.o a.c\n"
        )
        self.assertIn("-DZSTD_MULTITHREAD", argv)
        self.assertIn(f"-I{self.tree}/lib/common", argv)
        self.assertIn(f"-I{self.tree}/lib", argv)
        self.assertEqual(argv[-1], f"{self.tree}/a.c")
        self.assertEqual(omitted, ["-O3", "-Wall"])

    def test_the_object_and_the_source_do_not_reach_the_front_end_twice(self) -> None:
        argv, _ = self.command("gcc -c -o a.o a.c\n")
        self.assertNotIn("-o", argv)
        self.assertNotIn("a.o", argv)
        self.assertEqual(argv.count(f"{self.tree}/a.c"), 1)

    def test_a_separate_include_value_is_translated(self) -> None:
        argv, _ = self.command(f"gcc -include {TREE}/lib/zstd.h -c -o a.o a.c\n")
        self.assertIn("-include", argv)
        self.assertIn(f"{self.tree}/lib/zstd.h", argv)


class IncludeNextTest(unittest.TestCase):
    def test_the_directive_resolves_to_the_next_header_in_the_search_order(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first, second = root / "gcc", root / "usr"
            first.mkdir()
            second.mkdir()
            (second / "stdint.h").write_text("int from_libc;\n")
            wrapper = first / "stdint.h"
            wrapper.write_text("#include_next <stdint.h>\n")
            text, resolved = ladder._resolve_include_next(
                wrapper.read_text(), wrapper, [first, second]
            )
            self.assertEqual(resolved, 1)
            self.assertEqual(text.strip(), f'#include "{second / "stdint.h"}"')

    def test_a_directive_with_nothing_after_it_is_left_alone(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            last = root / "usr"
            last.mkdir()
            header = last / "limits.h"
            header.write_text("#include_next <limits.h>\n")
            text, resolved = ladder._resolve_include_next(header.read_text(), header, [last])
            self.assertEqual((text, resolved), ("#include_next <limits.h>\n", 0))


class CollisionTest(unittest.TestCase):
    def rows(self, table: dict[str, list[str]]) -> list[dict]:
        return [
            {"unit": unit, "outcome": "assembled", "extraSymbols": symbols}
            for unit, symbols in table.items()
        ]

    def test_units_that_publish_the_same_static_cannot_share_a_link(self) -> None:
        keep, excluded = ladder.without_collisions(
            self.rows({"a.o": ["__bswap_16"], "b.o": ["__bswap_16"], "c.o": []})
        )
        self.assertEqual(keep, ["a.o", "c.o"])
        self.assertEqual([row["unit"] for row in excluded], ["b.o"])
        self.assertEqual(excluded[0]["reason"], "visibility-drift")
        self.assertEqual(excluded[0]["collisions"][0]["with"], "a.o")

    def test_the_kept_units_keep_the_order_the_build_had(self) -> None:
        keep, _ = ladder.without_collisions(
            self.rows({"z.o": [], "a.o": ["one", "two"], "m.o": []})
        )
        self.assertEqual(keep, ["z.o", "a.o", "m.o"])

    def test_nothing_is_excluded_when_no_symbol_is_claimed_twice(self) -> None:
        keep, excluded = ladder.without_collisions(
            self.rows({"a.o": ["one"], "b.o": ["two"]})
        )
        self.assertEqual((keep, excluded), (["a.o", "b.o"], []))


class LinkErrorTest(unittest.TestCase):
    def test_the_colliding_symbols_are_read_out_of_the_linker_message(self) -> None:
        log = (
            "/usr/bin/ld: libz.a(gzclose.o): in function `__bswap_16':\n"
            "gzclose.c:(.text+0x0): multiple definition of `__bswap_16';"
            " compress.o:(.text+0x0): first defined here\n"
            "/usr/bin/ld: gzlib.o:(.text+0x2c): multiple definition of `__bswap_32'\n"
        )
        self.assertEqual(ladder.multiple_definitions(log), ["__bswap_16", "__bswap_32"])

    def test_a_clean_link_names_nothing(self) -> None:
        self.assertEqual(ladder.multiple_definitions("ar rc libz.a adler32.o\n"), [])


class BisectionTest(unittest.TestCase):
    """The rung bisects with the kernel lane's search, so it must behave here."""

    def test_one_guilty_unit_is_isolated_by_halving(self) -> None:
        units = [f"u{index}.o" for index in range(16)]
        calls: list[tuple[str, ...]] = []

        def fails(subset):
            calls.append(tuple(subset))
            return "u9.o" in subset

        self.assertEqual(ladder.ddmin(units, fails), ["u9.o"])
        self.assertLess(len(calls), 16)

    def test_a_pair_that_only_fails_together_is_found_together(self) -> None:
        units = [f"u{index}.o" for index in range(8)]
        guilty = {"u2.o", "u6.o"}

        def fails(subset):
            return guilty.issubset(set(subset))

        self.assertEqual(set(ladder.ddmin(units, fails)), guilty)


if __name__ == "__main__":
    unittest.main()
