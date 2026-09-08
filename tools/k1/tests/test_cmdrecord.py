"""The Kbuild command decoder must reproduce the recorded compile exactly."""

from __future__ import annotations

from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import cmdrecord  # noqa: E402

RECORD = (
    "savedcmd_lib/bitrev.o := gcc-14 -Wp,-MMD,lib/.bitrev.o.d -nostdinc "
    "-I/work/src/arch/arm64/include -I./arch/arm64/include/generated "
    "-include /work/src/include/linux/kconfig.h -D__KERNEL__ -mlittle-endian "
    "-DKASAN_SHADOW_SCALE_SHIFT= -fmacro-prefix-map=/work/src/= -std=gnu11 -O2 "
    "-Wa,-march=armv8.5-a -DARM64_ASM_ARCH='\"armv8.5-a\"' "
    "-c -o lib/bitrev.o /work/src/lib/bitrev.c\n"
    "\n"
    "source_lib/bitrev.o := /work/src/lib/bitrev.c\n"
    "\n"
    "deps_lib/bitrev.o := \\\n"
    "  /work/src/lib/bitrev.c \\\n"
)


class DecodeTest(unittest.TestCase):
    def setUp(self) -> None:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.tmp = Path(directory.name)
        self.out = self.tmp / "out"
        (self.out / "lib").mkdir(parents=True)
        (self.out / "arch/arm64/include/generated").mkdir(parents=True)
        self.src = self.tmp / "src"
        (self.src / "lib").mkdir(parents=True)
        (self.src / "lib/bitrev.c").write_text("int x;\n", encoding="utf-8")
        self.record_file = self.out / "lib/.bitrev.o.cmd"
        self.record_file.write_text(RECORD, encoding="utf-8")

    def record(self) -> cmdrecord.Record:
        return cmdrecord.parse_record(self.record_file)

    def test_record_path_matches_kbuild_naming(self) -> None:
        self.assertEqual(
            cmdrecord.record_path(self.out, "lib/bitrev.o"), self.record_file
        )
        self.assertEqual(
            cmdrecord.record_path(self.out, "a/b/c.o"), self.out / "a/b/.c.o.cmd"
        )

    def test_unit_and_source_come_from_the_record(self) -> None:
        record = self.record()
        self.assertEqual(record.unit, "lib/bitrev.o")
        self.assertEqual(record.source_token, "/work/src/lib/bitrev.c")
        self.assertEqual(record.tool, "gcc-14")
        self.assertTrue(cmdrecord.is_target_c_compile(record))

    def test_a_missing_saved_command_is_refused(self) -> None:
        broken = self.out / "lib/.broken.o.cmd"
        broken.write_text("no command here\n", encoding="utf-8")
        with self.assertRaises(cmdrecord.RecordError):
            cmdrecord.parse_record(broken)

    def test_an_empty_record_is_refused(self) -> None:
        empty = self.out / "lib/.empty.o.cmd"
        empty.write_text("", encoding="utf-8")
        with self.assertRaises(cmdrecord.RecordError):
            cmdrecord.parse_record(empty)

    def test_a_source_operand_is_found_without_a_declaration(self) -> None:
        generated = self.out / "lib/.gen.o.cmd"
        generated.write_text(
            "savedcmd_lib/gen.o := gcc-14 -c -o lib/gen.o lib/gen.c\n", encoding="utf-8"
        )
        record = cmdrecord.parse_record(generated)
        self.assertEqual(record.source_token, "lib/gen.c")

    def test_a_host_compile_is_not_a_target_compile(self) -> None:
        host = self.out / "lib/.host.o.cmd"
        host.write_text(
            "savedcmd_lib/host.o := gcc -c -o lib/host.o /work/src/lib/host.c\n",
            encoding="utf-8",
        )
        self.assertFalse(cmdrecord.is_target_c_compile(cmdrecord.parse_record(host)))

    def test_a_link_step_is_not_a_target_compile(self) -> None:
        link = self.out / "lib/.link.o.cmd"
        link.write_text(
            "savedcmd_lib/link.o := ld -r -o lib/link.o lib/a.o lib/b.o\n", encoding="utf-8"
        )
        with self.assertRaises(cmdrecord.RecordError):
            cmdrecord.parse_record(link)

    def test_container_paths_become_host_paths(self) -> None:
        self.assertEqual(
            cmdrecord.translate_path("/work/src/include", self.src, self.out),
            str(self.src / "include"),
        )
        self.assertEqual(
            cmdrecord.translate_path("./arch/arm64/include/generated", self.src, self.out),
            str((self.out / "arch/arm64/include/generated").resolve()),
        )
        self.assertEqual(cmdrecord.translate_path("/usr/lib", self.src, self.out), "/usr/lib")

    def test_a_relative_path_escaping_the_output_tree_is_refused(self) -> None:
        with self.assertRaises(cmdrecord.RecordError):
            cmdrecord.translate_path("../elsewhere", self.src, self.out)

    def test_the_front_end_command_keeps_only_what_it_consumes(self) -> None:
        argv, omitted = cmdrecord.evocc_argv(
            self.record(), Path("/bin/evocc"), self.src, self.out
        )
        self.assertEqual(argv[:4], [
            "/bin/evocc", "--emit=asm", "--survey", "--target=aarch64-unknown-linux-gnu",
        ])
        self.assertEqual(argv[-1], str(self.src / "lib/bitrev.c"))
        self.assertIn("-nostdinc", argv)
        self.assertIn("-D__KERNEL__", argv)
        self.assertIn("-DKASAN_SHADOW_SCALE_SHIFT=", argv)
        self.assertIn('-DARM64_ASM_ARCH="armv8.5-a"', argv)
        self.assertIn("-std=gnu11", argv)
        self.assertIn(f"-I{self.src / 'arch/arm64/include'}", argv)
        self.assertIn(f"-I{(self.out / 'arch/arm64/include/generated').resolve()}", argv)
        self.assertIn("-include", argv)
        self.assertIn(str(self.src / "include/linux/kconfig.h"), argv)
        self.assertIn(f"-fmacro-prefix-map={self.src}/=", argv)
        # The source operand appears once, as the final argument.
        self.assertEqual(argv.count(str(self.src / "lib/bitrev.c")), 1)
        # Code generation flags the front end has no use for are named, not guessed at.
        self.assertIn("-O2", omitted)
        self.assertIn("-mlittle-endian", omitted)
        self.assertIn("-c", omitted)
        self.assertIn("-Wp,-MMD,lib/.bitrev.o.d", omitted)

    def test_explicit_arithmetic_modes_reach_the_subject_in_written_order(self) -> None:
        flags = ["-fwrapv", "-fno-wrapv", "-fno-strict-overflow", "-fstrict-overflow"]
        for written in (flags, flags[::-1]):
            with self.subTest(flags=written):
                self.record_file.write_text(
                    RECORD.replace(" -O2 ", f" -O2 {' '.join(written)} "),
                    encoding="utf-8",
                )
                argv, omitted = cmdrecord.evocc_argv(
                    self.record(), Path("/bin/evocc"), self.src, self.out
                )
                self.assertEqual([flag for flag in argv if flag in flags], written)
                self.assertFalse(set(flags).intersection(omitted))

    def test_builtin_selection_reaches_the_subject_in_written_order(self) -> None:
        flags = ["-fno-builtin", "-fbuiltin"]
        for written in (flags, flags[::-1]):
            self.record_file.write_text(RECORD.replace(" -O2 ", f" -O2 {' '.join(written)} "))
            argv, omitted = cmdrecord.evocc_argv(self.record(), Path("/bin/evocc"), self.src, self.out)
            self.assertEqual([flag for flag in argv if flag in flags], written)
            self.assertFalse(set(flags).intersection(omitted))

    def test_a_dangling_option_value_is_refused(self) -> None:
        dangling = self.out / "lib/.dangling.o.cmd"
        dangling.write_text(
            "savedcmd_lib/d.o := gcc-14 -c -o lib/d.o /work/src/lib/bitrev.c -I\n",
            encoding="utf-8",
        )
        with self.assertRaises(cmdrecord.RecordError):
            cmdrecord.evocc_argv(
                cmdrecord.parse_record(dangling), Path("/bin/evocc"), self.src, self.out
            )

    def test_only_assembler_directed_flags_reach_the_assembler(self) -> None:
        flags = cmdrecord.assembler_flags(self.record())
        self.assertEqual(flags, ["-Wa,-march=armv8.5-a"])

    def test_the_assemble_command_names_input_and_output(self) -> None:
        argv = cmdrecord.assemble_argv(
            self.record(), "gcc-14", "lib/bitrev.s", "lib/bitrev.o"
        )
        self.assertEqual(
            argv,
            [
                "gcc-14", "-Wa,-march=armv8.5-a", "-c", "-x", "assembler",
                "lib/bitrev.s", "-o", "lib/bitrev.o",
            ],
        )


if __name__ == "__main__":
    unittest.main()
