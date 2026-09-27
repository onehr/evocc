"""A shared failed prefix is not a complete toybox test run."""

from pathlib import Path
import os
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import toybox  # noqa: E402


class ToyboxVerdicts(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.tree = Path(self.directory.name)
        (self.tree / "scripts").mkdir()
        (self.tree / "tests").mkdir()
        (self.tree / "lib").mkdir()
        (self.tree / "lib/toyflags.h").write_text("/* fixture */\n")
        (self.tree / "configure").write_text("# fixture\n")
        (self.tree / ".config").write_text("# fixture\n")
        (self.tree / "generated/unstripped").mkdir(parents=True)
        executable = self.tree / "toybox"
        executable.write_text("#!/bin/sh\nexit 0\n")
        executable.chmod(0o755)
        (self.tree / "commands.log").write_bytes(
            b"/work/toybox\0-c\0one.c\0-o\0unit.o\0\x01\n"
            b"/work/toybox\0unit.o\0-o\0generated/unstripped/toybox\0\x01\n"
        )

    def run_suite(self, source, *, link_status=0, expected=None, changed_object=False):
        (self.tree / "scripts/test.sh").write_text(
            "mkdir -p generated/testdir\ncp toybox generated/testdir/toybox\n" + source,
        )

        def local_container(mounts, script, timeout, **kwargs):
            self.assertEqual(mounts, {toybox.MOUNT: self.tree})
            self.runtime_options = kwargs
            # Exercise the real shell adapter, not a mock completion marker.
            prelude = f"""
gcc-14() {{
  [ {link_status} -eq 0 ] || return {link_status}
  cp {'unit.o' if changed_object else 'toybox'} generated/unstripped/toybox
  chmod +x generated/unstripped/toybox
}}
strip() {{ cp generated/unstripped/toybox toybox; }}
"""
            script = script.replace(toybox.CONTAINER_ROOT, shlex.quote(str(self.tree)))
            return subprocess.run(
                ["bash", "-c", prelude + script], capture_output=True, text=True,
                timeout=min(timeout, 10),
            )

        with patch.object(toybox.mixed, "container", side_effect=local_container):
            return toybox.relink_and_test(self.tree, expected)

    def test_duplicate_names_keep_every_occurrence(self):
        log = self.tree / "tests.log"
        log.write_text("PASS: base64 simple -d\nFAIL: base64 simple -d\n")
        self.assertEqual(list(toybox.outcomes(log).values()), ["PASS", "FAIL"])

    def test_stale_executable_cannot_hide_failed_relink(self):
        verdict, _ = self.run_suite("echo 'PASS: old executable'\n", link_status=7)
        self.assertEqual(verdict, "relink-failed")

    def test_prefix_exit_is_not_a_completed_suite(self):
        verdict, _ = self.run_suite("echo 'FAIL: chattr immutable'\nexit 1\n")
        self.assertEqual(verdict, "no-verdict")

    def test_upstream_continue_option_reaches_the_tail_after_failure(self):
        verdict, _ = self.run_suite("""
echo 'FAIL: first'
[ "$VERBOSE" != all ] && exit 1
echo 'PASS: tail'
false
""")
        self.assertEqual(verdict, "tests-done")
        self.assertEqual(list(toybox.outcomes(self.tree / "tests.log").values()), ["FAIL", "PASS"])

    def test_successful_empty_run_is_not_a_suite(self):
        verdict, _ = self.run_suite("true\n")
        self.assertEqual(verdict, "no-verdict")

    def test_zero_exit_with_failed_assertion_is_inconsistent(self):
        verdict, _ = self.run_suite("echo 'FAIL: hidden status'\ntrue\n")
        self.assertEqual(verdict, "inconsistent-verdict")

    def test_nonzero_exit_without_failed_assertion_is_inconsistent(self):
        verdict, _ = self.run_suite("echo 'PASS: first'\nfalse\n")
        self.assertEqual(verdict, "inconsistent-verdict")

    def test_successful_suite(self):
        verdict, _ = self.run_suite("echo 'PASS: first'\necho 'SKIP: optional'\ntrue\n")
        self.assertEqual(verdict, "tests-done")

    def test_recorded_link_runs_when_objects_are_not_make_prerequisites(self):
        # The pinned Makefile lists sources/.config, not generated objects.
        (self.tree / ".config").write_text("configured\n")
        os.utime(self.tree / ".config", (1, 1))
        (self.tree / "Makefile").write_text("toybox: .config\n\tcp unit.o toybox\n")
        (self.tree / "unit.o").write_text("#!/bin/sh\necho new\n")
        status, found = self.run_suite("""
if [ "$(./toybox)" = new ]; then echo 'PASS: installed code'; true
else echo 'FAIL: installed code'; false; fi
""", changed_object=True)
        self.assertEqual(status, "tests-done")
        self.assertEqual(list(found.values()), ["PASS"])

    def test_original_test_recipe_keeps_asan_environment(self):
        status, found = self.run_suite("""
if [ "$ASAN" = 1 ]; then echo 'PASS: recipe environment'; true
else echo 'FAIL: recipe environment'; false; fi
""")
        self.assertEqual(status, "tests-done")
        self.assertEqual(list(found.values()), ["PASS"])

    def test_runtime_uses_private_filesystem_not_the_mounted_build(self):
        status, found = self.run_suite(f"""
if [ "$PWD" != {shlex.quote(str(self.tree))} ] && [ -f lib/toyflags.h ]; then
  echo 'PASS: local runtime'
else echo 'FAIL: local runtime'; false; fi
""")
        self.assertEqual(status, "tests-done")
        self.assertEqual(list(found.values()), ["PASS"])
        self.assertEqual(self.runtime_options, {"capabilities": ("CAP_LINUX_IMMUTABLE", "CAP_SYS_ADMIN")})

    def test_runtime_excludes_target_objects_and_audits_the_executable(self):
        (self.tree / "generated/obj").mkdir()
        (self.tree / "generated/obj/one.o").write_bytes(b"compiler object")
        status, found = self.run_suite("""
if [ ! -e generated/obj ]; then echo 'PASS: no duplicate build'; else
  echo 'FAIL: no duplicate build'; false; fi
""")
        self.assertEqual(status, "tests-done")
        self.assertEqual(list(found.values()), ["PASS"])
        status, _ = self.run_suite("echo damaged > toybox\necho 'PASS: misleading'\n")
        self.assertNotEqual(status, "tests-done")
        status, _ = self.run_suite(
            "echo damaged > generated/testdir/toybox\necho 'PASS: misleading'\n",
        )
        self.assertNotEqual(status, "tests-done")

    def test_runtime_has_real_user_and_loop_name_fits_the_kernel_field(self):
        with patch.dict(os.environ, {"USER": "not-the-executing-user"}):
            status, found = self.run_suite('''
name="$PWD/generated/testdir/testdir/blah.img"
if [ "$USER" = "$(id -un)" ] && [ ${#name} -lt 64 ]; then
  echo 'PASS: runtime prerequisites'; true
else echo 'FAIL: runtime prerequisites'; false; fi
''')
        self.assertEqual(status, "tests-done")
        self.assertEqual(list(found.values()), ["PASS"])

    def test_preexisting_log_is_erased_even_when_link_fails(self):
        log = self.tree / "tests.log"
        log.write_text("PASS: old\nEVOCC_TOYBOX_TESTS_COMPLETE status=0\n")
        self.run_suite("true\n", link_status=7)
        self.assertEqual(log.read_text(), "")

    def test_wrong_object_before_test_prevents_execution(self):
        with patch.object(toybox.mixed, "container") as run:
            status, _ = toybox.relink_and_test(self.tree, {"unit.o": "absent"})
        self.assertEqual(status, "object-mismatch-before-test")
        run.assert_not_called()

    def test_object_rebuilt_while_testing_cannot_pass(self):
        obj = self.tree / "unit.o"
        obj.write_bytes(b"evocc")
        expected = {"unit.o": toybox.mixed.sha256(obj)}
        status, _ = self.run_suite(
            f"echo gcc > {shlex.quote(str(obj))}\necho 'PASS: first'\n", expected=expected,
        )
        self.assertEqual(status, "object-mismatch-after-test")

    def test_timeout_is_not_completion(self):
        with patch.object(toybox.mixed, "container", side_effect=subprocess.TimeoutExpired("suite", 5400)):
            self.assertEqual(toybox.relink_and_test(self.tree)[0], "test-timeout")

    def test_empty_name_and_duplicate_completion_markers_fail_closed(self):
        status, _ = self.run_suite("echo 'PASS: '\n")
        self.assertEqual(status, "inconsistent-verdict")
        status, _ = self.run_suite("echo 'PASS: one'\necho 'EVOCC_TOYBOX_TESTS_COMPLETE status=0'\n")
        self.assertEqual(status, "no-verdict")

    def test_comparison_detects_missing_repeated_occurrences_and_new_skips(self):
        baseline = {("repeat", 1): "PASS", ("repeat", 2): "PASS", ("known", 1): "FAIL"}
        for changed in (
            {("repeat", 1): "PASS", ("known", 1): "FAIL"},
            {**baseline, ("repeat", 2): "SKIP"},
            {**baseline, ("known", 1): "SKIP"},
            {**baseline, ("new", 1): "PASS"},
        ):
            self.assertTrue(toybox.regressions(baseline, changed))
        self.assertFalse(toybox.regressions(baseline, baseline))
        self.assertFalse(toybox.regressions(baseline, {**baseline, ("known", 1): "PASS"}))

    def test_final_selected_build_is_replayed_uncached_and_restored(self):
        subject = Mock(root=self.tree)
        log = self.tree / "tests.log"
        log.write_text("PASS: final candidate\n")
        summary = {"logSha256": toybox.mixed.sha256(log), "completed": True}
        narrowed = {"passingSet": ["a.o"], "failingUnits": ["b.o"], "budgetExhausted": True}
        with patch.object(toybox.mixed, "narrow", return_value=narrowed), \
             patch.object(toybox, "probe_candidate", return_value=(False, "no-regression", summary)) as probe:
            selected, _, verdict, _, _ = toybox.select_and_confirm(
                subject, ["a.o", "b.o"], self.tree, {}, self.tree, budget=1, no_bisect=False,
            )
        self.assertEqual(selected, ["a.o"])
        self.assertEqual(verdict, "bisection-budget-exhausted")
        probe.assert_called_once_with(subject, ["a.o"], self.tree, {})
        self.assertEqual((self.tree / "substituted-tests.log").read_bytes(), log.read_bytes())
        subject.restore_all.assert_called_once()

    def test_final_failed_replay_cannot_inherit_cached_pass(self):
        subject = Mock(root=self.tree)
        narrowed = {"passingSet": ["a.o"], "failingUnits": [], "budgetExhausted": False}
        with patch.object(toybox.mixed, "narrow", return_value=narrowed), \
             patch.object(toybox, "probe_candidate", return_value=(True, "no-verdict", {})):
            selected, _, verdict, _, _ = toybox.select_and_confirm(
                subject, ["a.o"], self.tree, {}, self.tree, budget=2, no_bisect=False,
            )
        self.assertEqual(selected, [])
        self.assertEqual(verdict, "final-replay-failed:no-verdict")
        subject.restore_all.assert_called_once()

    def test_exception_still_restores_reference_objects(self):
        subject = Mock(root=self.tree)
        with patch.object(toybox, "probe_candidate", side_effect=RuntimeError("failed tool")):
            with self.assertRaisesRegex(RuntimeError, "failed tool"):
                toybox.select_and_confirm(
                    subject, ["a.o"], self.tree, {}, self.tree, budget=1, no_bisect=True,
                )
        subject.restore_all.assert_called_once()

    def test_link_record_keeps_all_flags_and_object_order(self):
        log = self.tree / "commands.log"
        contents = log.read_bytes().replace(b"/work/toybox\0unit.o", b"/work/toybox\0-fPIC\0unit.o\0-lm")
        log.write_bytes(contents)
        self.assertEqual(toybox.link_command(self.tree), [
            "gcc-14", "-fPIC", "unit.o", "-lm", "-o", "generated/unstripped/toybox",
        ])

    def test_missing_or_changed_link_population_and_source_rebuild_are_rejected(self):
        log = self.tree / "commands.log"
        original = log.read_bytes()
        link = original.split(b"\x01\n")[1]
        for contents in (
            original.split(b"\x01\n")[0],
            original.replace(b"/work/toybox\0unit.o", b"/work/toybox\0other.o"),
            original.replace(b"/work/toybox\0unit.o", b"/work/toybox\0unit.o\0unit.o"),
            original.replace(b"/work/toybox\0unit.o", b"/work/toybox\0one.c\0unit.o"),
            original + link.replace(b"unit.o", b"other.o") + b"\x01\n",
        ):
            with self.subTest(contents=contents):
                log.write_bytes(contents)
                with patch.object(toybox.mixed, "container") as run:
                    self.assertEqual(toybox.relink_and_test(self.tree)[0], "invalid-link-record")
                run.assert_not_called()

    def test_missing_candidate_cannot_be_silently_dropped(self):
        subject = Mock(root=self.tree)
        with patch.object(toybox.mixed, "substitute", return_value=[]), \
             patch.object(toybox, "relink_and_test") as run:
            failed, note, _ = toybox.probe_candidate(subject, ["a.o"], self.tree, {})
        self.assertTrue(failed)
        self.assertEqual(note, "missing-object")
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
