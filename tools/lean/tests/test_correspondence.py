"""A selected vector run must preserve the other groups' recorded evidence."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location(
    "correspondence", ROOT / "tools/lean/correspondence.py"
)
correspondence = importlib.util.module_from_spec(spec)
spec.loader.exec_module(correspondence)


class SelectedRunTest(unittest.TestCase):
    def run_guards(self, root):
        with mock.patch.multiple(correspondence, ROOT=root, VECTORS=root / "vectors",
                                 RECEIPT=root / "receipt.json"), \
             mock.patch.object(correspondence, "run"), \
             mock.patch.object(correspondence, "target_dir", return_value=root / "target"), \
             mock.patch.object(correspondence.subprocess, "run",
                               return_value=SimpleNamespace(stdout="current-commit\n")), \
             mock.patch("sys.argv", ["correspondence.py", "--skip-build", "--set", "guards"]), \
             contextlib.redirect_stdout(io.StringIO()):
            return correspondence.main()

    def fixture(self, root, differences=0):
        (root / "lean").mkdir()
        (root / "lean/lean-toolchain").write_text("leanprover/lean4:v4.32.2\n")
        (root / "vectors").mkdir()
        for side in ["lean", *correspondence.RUNNERS]:
            (root / f"vectors/guards.{side}.jsonl").write_text('{"result": 1}\n')
        files = {name: {side: dict(vectors=1, compared=1, agreements=1,
                                  differences=0, abstentions=0)
                        for side in correspondence.RUNNERS}
                 for name in correspondence.FILES if name != "guards"}
        files["named"]["producer"].update(agreements=1-differences, differences=differences)
        previous = dict(schemaVersion=1, segment="C", run="previous-run",
                        commit="previous-commit", leanToolchain="leanprover/lean4:v4.32.2",
                        files=files, total=dict(compared=15, agreements=15-differences,
                                               differences=differences))
        (root / "receipt.json").write_text(json.dumps(previous))
        return previous

    def test_selected_run_keeps_five_groups_and_replaces_its_own_counts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            previous = self.fixture(root)
            for _ in range(2):
                self.assertEqual(self.run_guards(root), 0)
                receipt = json.loads((root / "receipt.json").read_text())
                self.assertEqual(set(receipt["files"]), set(correspondence.FILES))
                for name, counts in previous["files"].items():
                    self.assertEqual(receipt["files"][name], counts)
                    self.assertEqual(receipt["file_runs"][name]["commit"], "previous-commit")
                self.assertEqual(receipt["file_runs"]["guards"]["commit"], "current-commit")
                self.assertEqual(receipt["total"], dict(compared=18, agreements=18, differences=0))

    def test_passing_selected_run_does_not_hide_a_retained_difference(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root, differences=1)
            self.assertEqual(self.run_guards(root), 1)
            receipt = json.loads((root / "receipt.json").read_text())
            self.assertEqual(receipt["total"], dict(compared=18, agreements=17, differences=1))


if __name__ == "__main__":
    unittest.main()
