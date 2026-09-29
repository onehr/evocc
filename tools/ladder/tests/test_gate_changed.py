"""Exercise the real shell gate with tiny, offline Git/Cargo stand-ins."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


GATE = Path(__file__).resolve().parents[2] / "gate-changed.sh"


class ChangedGateTest(unittest.TestCase):
    def run_gate(self, changed="", status="", fail_test=False, sources=None, full=False, extra=()):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "tools").mkdir()
            shutil.copy2(GATE, root / "tools/gate-changed.sh")
            for name in ("leaf", "consumer", "unrelated"):
                (root / "crates" / name).mkdir(parents=True)
            for path, source in (sources or {}).items():
                destination = root / path
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(source)
            binaries = root / "bin"
            binaries.mkdir()
            git = binaries / "git"
            git.write_text(
                f"#!{sys.executable}\n"
                "import os, sys\n"
                "print(os.environ['GATE_DIFF' if sys.argv[1] == 'diff' else 'GATE_STATUS'])\n"
            )
            cargo = binaries / "cargo"
            cargo.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "from pathlib import Path\n"
                "with Path(os.environ['GATE_CALLS']).open('a') as log:\n"
                "    log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "if sys.argv[1] == 'metadata':\n"
                "    print(json.dumps({'packages': [\n"
                "        {'name': 'leaf', 'dependencies': []},\n"
                "        {'name': 'consumer', 'dependencies': [{'name': 'leaf'}]},\n"
                "        {'name': 'unrelated', 'dependencies': []}]}))\n"
                "if sys.argv[1] == 'test' and os.environ['GATE_FAIL_TEST'] == '1':\n"
                "    sys.exit(7)\n"
            )
            git.chmod(0o755)
            cargo.chmod(0o755)
            calls = root / "calls.jsonl"
            env = dict(os.environ, PATH=f"{binaries}{os.pathsep}{os.environ['PATH']}",
                       GATE_DIFF=changed, GATE_STATUS=status,
                       GATE_CALLS=str(calls), GATE_FAIL_TEST=str(int(fail_test)))
            result = subprocess.run(["sh", str(root / "tools/gate-changed.sh"),
                                     "--full-workspace" if full else "baseline", "--offline", "-j", "2", *extra],
                                    env=env, capture_output=True, text=True, timeout=10)
            commands = [json.loads(line) for line in calls.read_text().splitlines()] \
                if calls.exists() else []
            return result, commands

    def test_full_workspace_gate_cannot_select_away_unrelated_crates(self):
        result, calls = self.run_gate(full=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [
            ['test', '--workspace', '--no-fail-fast', '--offline', '-j', '2'],
            ['clippy', '--workspace', '--all-targets', '--', '-D', 'warnings'],
            ['fmt', '--all', '--', '--check']])
        failed, calls = self.run_gate(full=True, fail_test=True)
        self.assertNotEqual(failed.returncode, 0)
        self.assertEqual(len(calls), 1)

    def test_full_workspace_cannot_be_relabelled_after_excluding_tests(self):
        for extra in [('--test', 'only-one'), ('-p', 'leaf'), ('--exclude', 'unrelated'),
                      ('--', 'a_test_name'), ('--lib',), ('--no-run',)]:
            result, calls = self.run_gate(full=True, extra=extra)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(calls, [])

    def assert_packages(self, calls, names):
        self.assertEqual([call[0] for call in calls], ["metadata", "test", "clippy", "fmt"])
        for command in calls[1:3]:
            self.assertEqual({command[i + 1] for i, arg in enumerate(command) if arg == "-p"}, names)
        self.assertEqual(calls[1][-3:], ["--offline", "-j", "2"])
        self.assertEqual(calls[2][-4:], ["--all-targets", "--", "-D", "warnings"])
        self.assertEqual(calls[3], ["fmt", "--all", "--", "--check"])

    def test_manifest_only_change_runs_every_package(self):
        for path in ("Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml"):
            for committed in (True, False):
                with self.subTest(path=path, committed=committed):
                    result, calls = self.run_gate(
                        changed=path if committed else "",
                        status="" if committed else f" M {path}")
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assert_packages(calls, {"leaf", "consumer", "unrelated"})

    def test_repository_toolchain_matches_frozen_build_contract(self):
        import re
        root = GATE.parents[1]
        expected = json.loads((root / 'contracts/build-environment.json').read_text())['toolchain']['rust']['version']
        path = root / 'rust-toolchain.toml'
        self.assertTrue(path.is_file(), 'ambient stable must not select the acceptance compiler')
        channels = re.findall(r'^channel\s*=\s*"([^"]+)"\s*$', path.read_text(), re.M)
        self.assertEqual(channels, [expected])

    def test_crate_change_includes_reverse_dependencies(self):
        result, calls = self.run_gate(changed="crates/leaf/src/lib.rs")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_packages(calls, {"leaf", "consumer"})

    def test_non_cargo_change_does_not_invent_a_cargo_run(self):
        result, calls = self.run_gate(changed="docs/roadmap.md", status=" M tools/bench.py")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [])
        self.assertNotIn("gate-changed: green", result.stdout)

    def test_manifest_test_failure_is_not_green(self):
        result, calls = self.run_gate(changed="Cargo.lock", fail_test=True)
        self.assertEqual(result.returncode, 7, result.stdout)
        self.assertEqual([call[0] for call in calls], ["metadata", "test"])
        self.assertNotIn("gate-changed: green", result.stdout)

    def test_external_literal_fixture_selects_owner_and_reverse_dependencies(self):
        for macro in ("include_str", "include_bytes", "include"):
            for spelling in ('"../../../tests/fixtures/input.c"',
                             'r#"../../../tests/fixtures/input.c"#'):
                with self.subTest(macro=macro, spelling=spelling):
                    result, calls = self.run_gate(
                        changed="tests/fixtures/input.c",
                        sources={"crates/leaf/src/lib.rs": f"{macro}!(\n{spelling}\n);"})
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assert_packages(calls, {"leaf", "consumer"})

    def test_dirty_or_deleted_fixture_failure_is_not_green(self):
        for status in (" M", " D", "??"):
            with self.subTest(status=status):
                result, calls = self.run_gate(
                    status=f"{status} tests/fixtures/input.c", fail_test=True,
                    sources={"crates/leaf/tests/probe.rs":
                             'include_str!("../../../tests/fixtures/input.c");'})
                self.assertEqual(result.returncode, 7, result.stdout)
                self.assertEqual([call[0] for call in calls], ["metadata", "test"])
                self.assertNotIn("gate-changed: green", result.stdout)

    def test_external_nested_rust_include_tracks_its_data(self):
        result, calls = self.run_gate(
            changed="tests/fixtures/input.c",
            sources={
                "crates/leaf/tests/probe.rs": 'include!("../../../tests/shared.rs");',
                "tests/shared.rs": 'include_bytes!("fixtures/input.c");',
            })
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_packages(calls, {"leaf", "consumer"})

    def test_dynamic_include_fails_closed_for_external_changes(self):
        result, calls = self.run_gate(
            changed="tests/fixtures/input.c",
            sources={"crates/leaf/tests/probe.rs":
                     'include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../tests/fixtures/input.c"));'})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_packages(calls, {"leaf", "consumer"})

    def test_unreferenced_fixture_does_not_schedule_rust(self):
        result, calls = self.run_gate(
            changed="tests/fixtures/manual.c",
            sources={"crates/leaf/tests/probe.rs":
                     'include_str!("../../../tests/fixtures/input.c");'})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [])

    def test_include_cycle_is_bounded_and_keeps_remaining_dependencies(self):
        result, calls = self.run_gate(
            changed="tests/fixtures/input.c",
            sources={
                "crates/leaf/tests/probe.rs": 'include!("../../../tests/shared.rs");',
                "tests/shared.rs": 'include!("shared.rs"); include_str!{r"fixtures/input.c",}',
            })
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_packages(calls, {"leaf", "consumer"})

    def test_nonliteral_escape_and_missing_nested_source_fail_closed(self):
        for source in ('include_str!("../../../tests/fixtures/\\x69nput.c");',
                       'include!("../../../tests/missing.rs");'):
            with self.subTest(source=source):
                result, calls = self.run_gate(
                    changed="tests/fixtures/input.c",
                    sources={"crates/leaf/tests/probe.rs": source})
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assert_packages(calls, {"leaf", "consumer"})

    def test_actual_cli_fixture_consumer_is_selected(self):
        # Keep the real consumer's depth and contents; the stand-in package has
        # no Rust build cost, while the actual CLI suite is run separately.
        source = (GATE.parents[1] / "crates/evocc/tests/compared_value_selection.rs").read_text()
        for path in ("tests/fixtures/compared-value-selection.c",
                     "tests/fixtures/compared-value-csel.c"):
            with self.subTest(path=path):
                result, calls = self.run_gate(
                    changed=path, sources={"crates/leaf/tests/probe.rs": source})
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assert_packages(calls, {"leaf", "consumer"})


if __name__ == "__main__":
    unittest.main()
