"""The common entry point must report benchmark verdicts, not runner exits."""

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

PATH = Path(__file__).resolve().parents[2] / "bench.py"
bench = None
if PATH.exists():
    spec = importlib.util.spec_from_file_location("compiler_bench", PATH)
    bench = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = bench
    spec.loader.exec_module(bench)


class BenchmarkEntryTest(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(bench, "the benchmark entry point is not implemented")

    def test_catalog_and_multiple_selections(self):
        self.assertEqual(bench.select(["csmith,cjson", "csmith"]), ["csmith", "cjson"])
        self.assertIn("kernel", bench.select(["all"]))
        self.assertIn("mbedtls", bench.CATALOG)
        with self.assertRaises(ValueError):
            bench.select(["typo"])

    def test_command_uses_one_explicit_driver(self):
        args = bench.parser().parse_args(["csmith", "--programs", "17", "--jobs", "2"])
        command, env = bench.command("csmith", args, Path("/tmp/compiler"))
        self.assertIn("/tmp/compiler", command)
        self.assertEqual(command[command.index("--programs") + 1], "17")
        _, env = bench.command("kernel", args, Path("/tmp/compiler"))
        self.assertEqual(env["K1_EVOCC"], "/tmp/compiler")

    def test_program_failure_is_not_runner_success(self):
        receipt = {"suites": {"torture": {"programs": 4, "outcomes": {"pass": 2, "fail": 1, "refused": 1}}}}
        row = bench.metrics("torture", receipt)
        self.assertEqual((row["status"], row["passed"], row["total"]), ("INCOMPLETE", 2, 4))

    def test_oracle_unavailable_stays_in_csmith_denominator(self):
        row = bench.metrics("csmith", {"programs": 10, "agreeing": 9, "campaigns": {
            "base": {"outcomes": {"agree": 9, "oracle-unusable": 1}}
        }})
        self.assertEqual(row["status"], "INCOMPLETE")
        self.assertEqual(row["outcomes"]["oracle-unusable"], 1)

    def test_cjson_reference_test_is_not_subject_validation(self):
        row = bench.metrics("cjson", {"unitsTotal": 2, "unitsInPassingBuild": 2,
                                      "test": {"passed": True, "relinkStatus": 0}})
        self.assertEqual(row["status"], "VALIDATION_GAP")
        self.assertEqual(row["passed"], 0)

    def test_bisection_exhaustion_and_baseline_failures_are_not_complete(self):
        for verdict in ["bisection-budget-exhausted", "libc-test-clean"]:
            row = bench.metrics("musl", {"units": 5, "unitsInPassingBuild": 5,
                                         "testResult": verdict,
                                         "baselineTest": {"libcTestFailingCases": 1}})
            self.assertNotEqual(row["status"], "PASS")

    def test_fresh_receipt_and_compiler_identity_are_required(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            receipt = root / "tests/contracts/ladder-e0-receipt.json"
            receipt.parent.mkdir(parents=True)
            value = {"latest": {"run": "new", "driverSha256": "wrong", "suites": {
                "ctest": {"programs": 1, "outcomes": {"pass": 1}}
            }}}
            receipt.write_text(json.dumps(value))
            with patch.object(bench, "ROOT", root):
                with self.assertRaisesRegex(ValueError, "unchanged"):
                    bench.read_result("ctest", receipt.read_bytes(), "wanted")
                with self.assertRaisesRegex(ValueError, "compiler"):
                    bench.read_result("ctest", b"old", "wanted")

    def test_empty_or_inconsistent_counts_cannot_pass(self):
        for total, passed in [(0, 0), (1, 2)]:
            with self.assertRaises(ValueError):
                bench.metrics("torture", {"suites": {"torture": {
                    "programs": total, "outcomes": {"pass": passed}
                }}})


if __name__ == "__main__":
    unittest.main()
