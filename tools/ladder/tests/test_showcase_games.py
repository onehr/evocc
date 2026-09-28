"""A screenshot cannot substitute for a complete C-object or runtime receipt."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("showcase_games", Path(__file__).resolve().parents[2] / "showcase_games.py")
games = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(games)


class GameCaptureTests(unittest.TestCase):
    def setUp(self):
        self.record = games.read(games.OUT / "games.json")["games"]["neverball"]

    def test_committed_images_are_the_recorded_frames(self):
        games.check()

    def test_refused_missing_or_duplicate_c_unit_is_not_a_capture_pass(self):
        for mutation in ("refused", "missing", "duplicate", "installed"):
            record = copy.deepcopy(self.record)
            if mutation == "refused":
                record["emit"]["units"][0]["exitCode"] = 1
            elif mutation == "missing":
                record["emit"]["units"].pop()
            elif mutation == "duplicate":
                record["emit"]["units"].append(record["emit"]["units"][0])
            else:
                record["native"]["objects"].pop(next(iter(record["native"]["objects"])))
            with self.assertRaisesRegex(ValueError, "C-object provenance"):
                games.validate(record)

    def test_runtime_failure_or_a_reference_binary_cannot_supply_subject_picture(self):
        for mutation in ("exit", "binary", "frame", "compiler", "adapter"):
            record = copy.deepcopy(self.record)
            run = record["runs"]["evocc"]
            if mutation == "exit":
                run["exitCode"] = 133
            elif mutation == "binary":
                run["binarySha256"] = record["runs"]["gcc"]["binarySha256"]
            elif mutation == "frame":
                run["frames"].pop("frame-600.ppm")
            elif mutation == "adapter":
                run["adapterSha256"] = "0" * 64
            else:
                run["compiler"] = "gcc"
            with self.assertRaises(ValueError):
                games.validate(record)

    def test_pixels_reconstruct_original_ppm_not_a_repainted_window(self):
        image = self.record["image"]
        original = games.captured_ppm(games.OUT / image["file"])
        self.assertEqual(hashlib.sha256(original).hexdigest(), self.record["runs"]["evocc"]["frames"]["frame-600.ppm"])
        rgb = original.split(b"\n255\n", 1)[1]
        self.assertEqual(hashlib.sha256(rgb).hexdigest(), image["rgbSha256"])

    def test_successful_exit_does_not_become_pixel_equivalence(self):
        record = copy.deepcopy(self.record)
        record["exactMatchingFrames"] = list(record["runs"]["evocc"]["frames"])
        with self.assertRaisesRegex(ValueError, "frame-equivalence"):
            games.validate(record)


if __name__ == "__main__":
    unittest.main()
