"""Doom verdicts require all original frame positions and independent repeats."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "doom.py"
spec = importlib.util.spec_from_file_location("ladder_doom_test", PATH)
doom = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = doom
spec.loader.exec_module(doom)


class DoomTest(unittest.TestCase):
    def rows(self):
        return [{"compiler": cc, "demo": demo, "repeat": repeat, "exitCode": 0,
                 "complete": True, "frames": [hashlib.sha256(f"{demo}:{i}".encode()).hexdigest()
                                               for i in range(350)]}
                for cc in ("gcc-14", "clang-19", "evocc") for demo in ("demo1", "demo2")
                for repeat in range(2)]

    def test_full_matrix_passes(self):
        self.assertTrue(doom.valid_runtime(self.rows()))

    def test_missing_duplicate_failed_or_partial_execution_fails(self):
        rows = self.rows()
        for changed in ([], rows[:-1], rows + [rows[0]], rows[:-1] + [rows[0]]):
            self.assertFalse(doom.valid_runtime(changed))
        for field, value in (("exitCode", -5), ("complete", False), ("frames", [])):
            changed = copy.deepcopy(rows)
            changed[-1][field] = value
            self.assertFalse(doom.valid_runtime(changed))

    def test_first_frame_repeat_or_compiler_difference_fails(self):
        for index in (0, 1, 4, 8, 11):
            changed = self.rows()
            changed[index]["frames"][0] = "f" * 64
            self.assertFalse(doom.valid_runtime(changed))

    def test_static_black_screen_is_not_gameplay_pass(self):
        rows = self.rows()
        for row in rows:
            row["frames"] = ["0" * 64] * 350
        self.assertFalse(doom.valid_runtime(rows))

    def test_common_build_has_explicit_wrapping(self):
        self.assertIn("-fwrapv", doom.FLAGS)
        self.assertNotIn("-ffast-math", doom.FLAGS)


if __name__ == "__main__":
    unittest.main()
