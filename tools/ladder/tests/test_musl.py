"""The libc under test must not also provide its own timeout supervisor."""

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

path = Path(__file__).resolve().parents[1] / "musl.py"
spec = importlib.util.spec_from_file_location("bench_musl", path)
musl = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = musl
spec.loader.exec_module(musl)


class SupervisorTest(unittest.TestCase):
    def test_missing_report_is_not_an_empty_failure_set(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "REPORT"
            with self.assertRaises(FileNotFoundError):
                musl.report_failures(report)
            report.write_text("")
            self.assertEqual(musl.report_failures(report), set())

    def test_reference_build_preserves_the_supervisor(self):
        self.assertIn("cp src/common/runtest.exe reference-runtest", musl.BUILD_SCRIPT)

    def test_subject_cases_use_the_preserved_supervisor(self):
        self.assertIn("RUN_TEST=\"/work/libc-test/reference-runtest -w ''\"", musl.RELINK_SCRIPT)
        self.assertIn("test -x /work/libc-test/reference-runtest", musl.RELINK_SCRIPT)
        # Only supervision changes; cases still relink against the subject libc.
        self.assertIn("make install", musl.RELINK_SCRIPT)
        self.assertIn("rm -f src/*/*.exe", musl.RELINK_SCRIPT)


if __name__ == "__main__":
    unittest.main()
