"""A real paired loader result cannot be inferred from a static-lane PASS."""

import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import mixed  # noqa: E402,F401

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import musl_shared  # noqa: E402


class SharedVerdict(unittest.TestCase):
    def complete(self):
        rows = []
        for side in ("reference", "candidate"):
            rows += [
                dict(library=side, plugin=name, compile={"exit": 0}, build={"exit": 0})
                for name in musl_shared.PLUGINS
            ]
            rows += [
                dict(
                    library=side,
                    subject=name,
                    build={"exit": 0},
                    run={"exit": 0},
                    elf={
                        "exit": 0,
                        "stdout": f"[Requesting program interpreter: /work/shared/{side}/libc.so]",
                    },
                    binarySha256="binary",
                    objectSha256="object",
                )
                for name in musl_shared.SUBJECTS
            ]
        return dict(
            rows=rows, libraryHashes={"reference": "ref", "candidate": "candidate"}
        )

    def test_exact_paired_population_passes(self):
        self.assertTrue(musl_shared.validate_executions(self.complete()))

    def test_missing_duplicate_unknown_and_failed_rows_fail(self):
        for change in (
            "missing",
            "duplicate",
            "unknown",
            "failed-build",
            "failed-run",
            "timeout",
            "failed-plugin",
            "static",
            "different-loader",
            "no-identity",
            "no-library",
        ):
            data = copy.deepcopy(self.complete())
            row = data["rows"][-1]
            if change == "missing":
                data["rows"].pop()
            elif change == "duplicate":
                data["rows"].append(copy.deepcopy(row))
            elif change == "unknown":
                row["subject"] = "different-program"
            elif change == "failed-build":
                row["build"]["exit"] = 1
            elif change == "failed-run":
                row["run"]["exit"] = 1
            elif change == "timeout":
                row["run"] = {"exit": None, "timeout": True}
            elif change == "failed-plugin":
                data["rows"][0]["compile"]["exit"] = 1
            elif change == "static":
                row["elf"]["stdout"] = ""
            elif change == "different-loader":
                row["elf"]["stdout"] = (
                    "[Requesting program interpreter: /lib/ld-linux.so]"
                )
            elif change == "no-identity":
                row.pop("binarySha256")
            else:
                data["libraryHashes"].pop("candidate")
            with self.subTest(change=change):
                self.assertFalse(musl_shared.validate_executions(data))


if __name__ == "__main__":
    unittest.main()
