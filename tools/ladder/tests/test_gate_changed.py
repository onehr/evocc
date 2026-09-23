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
    def run_gate(self, changed="", status="", fail_test=False):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "tools").mkdir()
            shutil.copy2(GATE, root / "tools/gate-changed.sh")
            for name in ("leaf", "consumer", "unrelated"):
                (root / "crates" / name).mkdir(parents=True)
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
                                     "baseline", "--offline", "-j", "2"],
                                    env=env, capture_output=True, text=True, timeout=10)
            commands = [json.loads(line) for line in calls.read_text().splitlines()] \
                if calls.exists() else []
            return result, commands

    def assert_packages(self, calls, names):
        self.assertEqual([call[0] for call in calls], ["metadata", "test", "clippy", "fmt"])
        for command in calls[1:3]:
            self.assertEqual({command[i + 1] for i, arg in enumerate(command) if arg == "-p"}, names)
        self.assertEqual(calls[1][-3:], ["--offline", "-j", "2"])
        self.assertEqual(calls[2][-4:], ["--all-targets", "--", "-D", "warnings"])
        self.assertEqual(calls[3], ["fmt", "--all", "--", "--check"])

    def test_manifest_only_change_runs_every_package(self):
        for path in ("Cargo.toml", "Cargo.lock"):
            for committed in (True, False):
                with self.subTest(path=path, committed=committed):
                    result, calls = self.run_gate(
                        changed=path if committed else "",
                        status="" if committed else f" M {path}")
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assert_packages(calls, {"leaf", "consumer", "unrelated"})

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


if __name__ == "__main__":
    unittest.main()
