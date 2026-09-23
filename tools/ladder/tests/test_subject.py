"""The ladder's decoders must read a build log the way the build meant it."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

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
    def test_multiline_commands_cannot_replace_failure_with_redirection_status(self) -> None:
        for operation in ('reference_build', 'relink', 'run_tests'):
            for suffix in ('', '\n', '\n# adapter end\n'):
                with self.subTest(operation=operation, suffix=suffix), tempfile.TemporaryDirectory() as tmp:
                    root = Path(tmp)
                    mount = root / 'probe'
                    (mount / 'source').mkdir(parents=True)
                    evidence = root / 'evidence'
                    evidence.mkdir()
                    command = "(printf '%s\\n' actual-test-output; exit 7)" + suffix
                    subject = ladder.Subject(name='probe', rung='test', pin='', url='',
                                             sha256='', tree='source', build=command,
                                             test=command, relink=command)

                    def local_container(directory, name, script, timeout=30):
                        self.assertEqual(directory, mount)
                        return subprocess.run(['bash', '-c', script.replace(f'/work/{name}', str(mount))],
                                              capture_output=True, text=True, timeout=timeout, check=False)

                    with mock.patch.object(ladder, 'BUILD', root), \
                            mock.patch.object(ladder, 'container', side_effect=local_container):
                        function = getattr(ladder, operation)
                        result = (function(subject, mount / 'source', evidence)
                                  if operation == 'reference_build'
                                  else function(subject, evidence, 'red'))
                    self.assertEqual(result['status'], 7, 'a redirection is not the tested command')
                    self.assertIn('actual-test-output', result.get('log', result.get('tail', '')))

    def test_configure_errexit_is_not_disabled_by_the_wrapper(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            mount = root / 'probe'
            (mount / 'source').mkdir(parents=True)
            evidence = root / 'evidence'
            evidence.mkdir()
            subject = ladder.Subject(name='probe', rung='test', pin='', url='', sha256='',
                                     tree='source', build='echo build-must-not-run', test='', relink='',
                                     configure='(set -e; echo configure; false; echo unreachable)\n')
            def local_container(directory, name, script, timeout=30):
                return subprocess.run(['bash', '-c', script.replace(f'/work/{name}', str(mount))],
                                      capture_output=True, text=True, timeout=timeout, check=False)
            with mock.patch.object(ladder, 'BUILD', root), \
                    mock.patch.object(ladder, 'container', side_effect=local_container):
                result = ladder.reference_build(subject, mount / 'source', evidence)
            self.assertNotEqual(result['status'], 0)
            self.assertEqual((evidence / 'configure.log').read_text(), 'configure\n')
            self.assertFalse((mount / 'build.log').exists())

    def test_final_acceptance_does_not_repeat_the_same_completed_passing_trial(self) -> None:
        passed = {'passed': True, 'units': ['a.o', 'b.o']}
        rerun = mock.Mock(return_value={'passed': False})
        self.assertIs(ladder.final_trial(passed, ['a.o', 'b.o'], rerun), passed)
        rerun.assert_not_called()
        for previous, subset in [(passed, ['a.o']),
                                 (passed | {'passed': False}, ['a.o', 'b.o'])]:
            self.assertIs(ladder.final_trial(previous, subset, rerun), rerun.return_value)
        self.assertEqual(rerun.call_count, 2)

    def test_receipt_reports_only_current_measured_front_end_gaps(self) -> None:
        self.assertEqual(ladder.front_end_gaps([{'unit': 'ok.o', 'outcome': 'assembled'}]), [])
        current = {'unit': 'bad.o', 'outcome': 'refused', 'code': 'E0002',
                   'message': 'unsupported source byte', 'at': 'bad.h:3:2'}
        self.assertEqual(ladder.front_end_gaps([
            current, {'unit': 'fp.o', 'outcome': 'refused', 'code': 'E3050'}]),
            [{key: current[key] for key in ('unit', 'code', 'message', 'at')}])

    def test_cmake_gmake_build_directory_applies_to_unprefixed_recipes(self) -> None:
        log = (
            f"gmake[1]: Entering directory '{TREE}/build'\n"
            f"gmake[2]: Entering directory '{TREE}/build'\n"
            f"gmake[2]: Entering directory '{TREE}/build'\n"
            f"/usr/bin/gcc-14 -o CMakeFiles/helper.dir/hash.c.o -c {TREE}/hash.c\n"
            f"cd {TREE}/build/tests && /usr/bin/gcc-14 -o suite.c.o -c {TREE}/suite.c\n"
            f"gmake[2]: Leaving directory '{TREE}/build'\n"
            f"/usr/bin/gcc-14 -o CMakeFiles/helper.dir/random.c.o -c {TREE}/random.c\n"
            f"gmake[2]: Leaving directory '{TREE}/build'\n"
            f"gmake[1]: Leaving directory '{TREE}/build'\n"
            "gcc-14 -o top.o -c top.c\n"
        )
        units = ladder.parse_build_log(log, TREE)
        self.assertEqual([unit.obj for unit in units], [
            'build/CMakeFiles/helper.dir/hash.c.o', 'build/tests/suite.c.o',
            'build/CMakeFiles/helper.dir/random.c.o', 'top.o',
        ])

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
        self.assertIn("-O3", argv)
        self.assertEqual(omitted, ["-Wall"])

    def test_supported_target_semantics_are_not_lost_in_projection(self) -> None:
        flags = [
            "-O3", "-O0", "-Os", "-fshort-wchar", "-fno-short-wchar",
            "-fPIC", "-fno-pic", "-fPIE", "-ffreestanding", "-fhosted",
            "-ftls-model=local-exec", "-ftls-model=global-dynamic",
            "-mgeneral-regs-only", "-mno-general-regs-only",
            "-mstrict-align", "-mno-strict-align",
            "-ftrivial-auto-var-init=zero", "-ftrivial-auto-var-init=uninitialized",
            "-mbranch-protection=pac-ret", "-mbranch-protection=none",
            "-fstack-protector-strong", "-fno-stack-protector",
            "-mstack-protector-guard=sysreg", "-mstack-protector-guard-reg=sp_el0",
            "-mstack-protector-guard-offset=1240",
        ]
        for written in (flags, flags[::-1]):
            argv, omitted = self.command(f"gcc {' '.join(written)} -c -o a.o a.c\n")
            self.assertEqual([flag for flag in argv if flag in flags], written)
            self.assertEqual(omitted, [])

    def test_unknown_options_remain_named_not_reinterpreted_as_supported(self) -> None:
        argv, omitted = self.command(
            "gcc -Ofast -fno-inline -fpermissive -std=gnu11 -c -o a.o a.c\n"
        )
        self.assertIn("-std=gnu11", argv)
        self.assertEqual(omitted, ["-Ofast", "-fno-inline", "-fpermissive"])

    def test_rejected_standard_is_not_silently_replaced_with_the_default(self) -> None:
        (unit,) = ladder.parse_build_log('gcc -std=c99 -c -o a.o a.c\n', TREE)
        argv, omitted = ladder.evocc_argv(
            unit, Path('/bin/evocc'), self.subject, self.tree, [], frozenset({'-std=c99'}))
        self.assertIn('-std=c99', argv)
        self.assertNotIn('-std=c99', omitted)

    def test_the_object_and_the_source_do_not_reach_the_front_end_twice(self) -> None:
        argv, _ = self.command("gcc -c -o a.o a.c\n")
        self.assertNotIn("-o", argv)
        self.assertNotIn("a.o", argv)
        self.assertEqual(argv.count(f"{self.tree}/a.c"), 1)

    def test_explicit_arithmetic_modes_reach_the_subject_in_written_order(self) -> None:
        flags = ["-fwrapv", "-fno-wrapv", "-fno-strict-overflow", "-fstrict-overflow"]
        for written in (flags, flags[::-1]):
            with self.subTest(flags=written):
                argv, omitted = self.command(f"gcc {' '.join(written)} -c -o a.o a.c\n")
                self.assertEqual([flag for flag in argv if flag in flags], written)
                self.assertFalse(set(flags).intersection(omitted))

    def test_builtin_selection_reaches_the_subject_in_written_order(self) -> None:
        flags = ["-fno-builtin", "-fbuiltin"]
        for written in (flags, flags[::-1]):
            argv, omitted = self.command(f"gcc {' '.join(written)} -c -o a.o a.c\n")
            self.assertEqual([flag for flag in argv if flag in flags], written)
            self.assertFalse(set(flags).intersection(omitted))

    def test_a_separate_include_value_is_translated(self) -> None:
        argv, _ = self.command(f"gcc -include {TREE}/lib/zstd.h -c -o a.o a.c\n")
        self.assertIn("-include", argv)
        self.assertIn(f"{self.tree}/lib/zstd.h", argv)


class BuildSymlinkTest(unittest.TestCase):
    def test_internal_absolute_links_are_portable_without_changing_source(self) -> None:
        self.assertTrue(hasattr(ladder, 'portable_build_symlinks'))
        with tempfile.TemporaryDirectory() as directory:
            tree = Path(directory)
            (tree / 'source').mkdir()
            (tree / 'build').mkdir()
            source = tree / 'source/query.c'
            source.write_bytes(b'int value = 7;\n')
            link = tree / 'build/query.c'
            link.symlink_to(f'{TREE}/source/query.c')
            relative = tree / 'build/relative.c'
            relative.symlink_to('../source/query.c')
            external = tree / 'build/external.c'
            external.symlink_to('/usr/include/external.c')
            escaped = tree / 'build/escaped.c'
            escaped.symlink_to(f'{TREE}/../outside.c')

            changes = ladder.portable_build_symlinks(tree, TREE)

            self.assertEqual(changes, {'build/query.c': '../source/query.c'})
            self.assertEqual(link.read_bytes(), b'int value = 7;\n')
            self.assertEqual(source.read_bytes(), b'int value = 7;\n')
            self.assertEqual(relative.readlink(), Path('../source/query.c'))
            self.assertEqual(external.readlink(), Path('/usr/include/external.c'))
            self.assertEqual(escaped.readlink(), Path(f'{TREE}/../outside.c'))
            self.assertEqual(ladder.portable_build_symlinks(tree, TREE), {})


class SysrootSourceTest(unittest.TestCase):
    def test_every_adapter_uses_the_same_original_sysroot_export(self) -> None:
        with mock.patch.object(ladder.e0, 'sysroot', return_value=Path('/explicit/headers')) as load:
            self.assertEqual(ladder.exported_sysroot(), Path('/explicit/headers'))
            load.assert_called_once_with()

    def test_supported_source_tokens_reach_the_compiler_unchanged(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "original"
            source.mkdir()
            header = source / "header.h"
            contents = (
                b"#include_next <header.h>\n"
                b"struct S { __extension__ int x; };\n"
                b"\f\vint f(void){return __extension__ 0;}\n"
            )
            header.write_bytes(contents)
            with mock.patch.object(ladder, "exported_sysroot", return_value=source), \
                    mock.patch.object(ladder, "BUILD", root):
                for force in (False, True):
                    prepared, counts = ladder.adjust_sysroot(force=force)
                    self.assertEqual((prepared / "header.h").read_bytes(), contents)
                    self.assertEqual(prepared, source)
                    self.assertEqual(sum(counts.values()), 0)


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


class InstalledObjectTest(unittest.TestCase):
    def test_reuse_recompiles_with_the_identical_command_and_checks_assembly(self):
        argv = ['evocc', '--emit=asm', '-std=gnu11', '-include', 'generated.h', '-DF=3', 'a.c']
        for status, output, expected in ((0, b'ret\n', ladder.sha256_bytes(b'ret\n')),
                                         (1, b'ret\n', None), (0, b'', None),
                                         (0, b'.incbin "bytes"\n', None),
                                         (0, b'  .include "assembly.inc"\n', None)):
            completed = ladder.subprocess.CompletedProcess([], status, output, b'')
            with mock.patch.object(ladder.subprocess, 'run', return_value=completed) as run:
                self.assertEqual(ladder.emission_digest(argv), expected)
                self.assertEqual(run.call_args.args[0], argv)

    def test_regenerated_inputs_must_match_before_reinstalling_subject_objects(self):
        self.assertTrue(hasattr(ladder, 'emission_digest'), 'regenerated sources must be recompiled')
        subject = ladder.Subject(name='example', rung='test', pin='1', url='',
                                 sha256='', tree='source', build='', test='', relink='')
        for unchanged, rebuild_twice in ((True, False), (False, False), (True, True)):
            with self.subTest(unchanged=unchanged, rebuild_twice=rebuild_twice), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                tree, assembly, pristine = (root / name for name in ('tree', 'asm', 'pristine'))
                for path in (tree, assembly, pristine):
                    path.mkdir()
                for path in (tree, pristine):
                    (path / 'a.o').write_bytes(b'reference')
                (assembly / 'a.o').write_bytes(b'subject')
                links = []

                def regenerate(*_):
                    links.append(1)
                    if len(links) == 1 or rebuild_twice:
                        (tree / 'a.o').write_bytes(b'reference')
                    return {'status': 0, 'log': ''}

                with mock.patch.object(ladder, 'relink', side_effect=regenerate), \
                        mock.patch.object(ladder, 'emission_digest', return_value='same' if unchanged else 'changed'), \
                        mock.patch.object(ladder, 'run_tests', return_value={'passed': True, 'status': 0, 'tail': ''}) as tests:
                    result = ladder.trial(
                        subject, tree, ['a.o'], ['a.o'], assembly, pristine, root, 'check',
                        generated_inputs={'a.o': (['evocc', '--emit=asm', 'a.c'], 'same')})
                passed = unchanged and not rebuild_twice
                self.assertEqual(result['passed'], passed)
                self.assertEqual(len(links), 2 if unchanged else 1)
                self.assertEqual(tests.call_count, int(passed))
                self.assertEqual(result['stage'], 'rebuilt' if rebuild_twice else 'pass' if unchanged else 'inputs-changed')
                if passed:
                    self.assertEqual((tree / 'a.o').read_bytes(), b'subject')
                    self.assertEqual(result['regeneratedInputs'], ['a.o'])

    def test_relink_replacement_is_checked_before_tests_and_not_bisected(self):
        subject = ladder.Subject(name='example', rung='test', pin='1', url='',
                                 sha256='', tree='source', build='', test='', relink='')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tree, assembly, pristine = (root / name for name in ('tree', 'asm', 'pristine'))
            for path in (tree, assembly, pristine):
                path.mkdir()
            for path in (tree, pristine):
                (path / 'a.o').write_bytes(b'reference')
            (assembly / 'a.o').write_bytes(b'subject')

            def regenerate(*_):
                (tree / 'a.o').write_bytes(b'reference')
                return {'status': 0, 'log': ''}

            with mock.patch.object(ladder, 'relink', side_effect=regenerate), \
                    mock.patch.object(ladder, 'run_tests') as tests:
                result = ladder.trial(subject, tree, ['a.o'], ['a.o'], assembly,
                                      pristine, root, 'check')
            self.assertEqual(result['stage'], 'rebuilt')
            tests.assert_not_called()
            self.assertFalse(ladder.is_compiler_failure(result))
            self.assertFalse(ladder.is_compiler_failure({'passed': False, 'stage': 'inputs-changed'}))
            self.assertTrue(ladder.is_compiler_failure({'passed': False, 'stage': 'test'}))
            self.assertTrue(ladder.is_compiler_failure({'passed': False, 'stage': 'relink'}))

    def test_a_test_target_rebuilding_the_subject_is_not_a_subject_pass(self):
        subject = ladder.Subject(name='example', rung='test', pin='1', url='',
                                 sha256='', tree='source', build='', test='', relink='')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tree, assembly, pristine = (root / name for name in ('tree', 'asm', 'pristine'))
            for path in (tree, assembly, pristine):
                path.mkdir()
            (tree / 'a.o').write_bytes(b'reference')
            (pristine / 'a.o').write_bytes(b'reference')
            (assembly / 'a.o').write_bytes(b'subject')

            def reference_rebuild(*_):
                (tree / 'a.o').write_bytes(b'reference')
                return {'passed': True, 'tail': 'all tests pass'}

            with mock.patch.object(ladder, 'relink', return_value={'status': 0, 'log': ''}), \
                    mock.patch.object(ladder, 'run_tests', side_effect=reference_rebuild):
                result = ladder.trial(subject, tree, ['a.o'], ['a.o'], assembly,
                                      pristine, root, 'check')
            self.assertFalse(result['passed'])
            self.assertEqual(result['stage'], 'rebuilt')


if __name__ == "__main__":
    unittest.main()
