"""README output panels preserve text and fail on changed captured assets."""
import importlib.util
import base64
import copy
from html.parser import HTMLParser
from pathlib import Path
import tempfile
import struct
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET
import zlib

SCRIPT = Path(__file__).resolve().parents[2] / "showcase.py"
SPEC = importlib.util.spec_from_file_location("showcase", SCRIPT)
showcase = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(showcase)
VIS_SPEC = importlib.util.spec_from_file_location("showcase_visuals", SCRIPT.with_name("showcase_visuals.py"))
visuals = importlib.util.module_from_spec(VIS_SPEC)
VIS_SPEC.loader.exec_module(visuals)


class ShowcaseTests(unittest.TestCase):
    def test_stdout_is_text_not_svg_markup(self):
        card = visuals.Card("<Title>", "A & B", "stdout")
        card.text(1, 1, '<script>alert("x")</script>')
        root = ET.fromstring(card.finish())
        self.assertNotIn("script", [node.tag.rsplit("}", 1)[-1] for node in root.iter()])
        self.assertIn('<script>alert("x")</script>', "".join(root.itertext()))

    def test_rgb_embedding_is_lossless(self):
        pixels = bytes((0, 127, 255, 23, 45, 67))
        encoded = base64.b64decode(visuals.png_uri(2, 1, pixels).split(",")[1])
        self.assertEqual(encoded[:8], b"\x89PNG\r\n\x1a\n")
        position, compressed = 8, b""
        while position < len(encoded):
            length = struct.unpack(">I", encoded[position:position+4])[0]
            kind = encoded[position+4:position+8]
            if kind == b"IDAT":
                compressed += encoded[position+8:position+8+length]
            position += 12+length
        self.assertEqual(zlib.decompress(compressed), b"\0" + pixels)
        with self.assertRaisesRegex(ValueError, "dimensions"):
            visuals.png_uri(2, 2, pixels)

    def test_roundtrip_picture_must_match_measured_bytes(self):
        records = showcase.read_json(showcase.OUT / "captures.json")
        corrupted = copy.deepcopy(records["programs"]["zstd"])
        corrupted["roundTrip"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "differs from Lua"):
            visuals.draw("zstd", corrupted, records["programs"])

    def test_sprite_picture_must_match_compressed_bytes(self):
        record = copy.deepcopy(showcase.read_json(showcase.OUT / "captures.json")["programs"]["zlib"])
        record["picture"]["rgb"] = "00" + record["picture"]["rgb"][2:]
        with self.assertRaisesRegex(ValueError, "sprite sheet differs"):
            visuals.draw("zlib", record)

    def test_bundle_rejects_changed_hashes_and_false_negative_control(self):
        original = showcase.read_json(showcase.OUT / "captures.json")["programs"]["toybox"]
        record = copy.deepcopy(original)
        record["bundle"]["files"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "manifest differs"):
            visuals.draw("toybox", record)
        record = copy.deepcopy(original)
        negative, = [c for c in record["commands"] if c.get("stage") == "detect-change"]
        negative["exitCode"] = 0
        with self.assertRaisesRegex(ValueError, "exit code"):
            visuals.draw("toybox", record)

    def test_bundle_requires_both_restore_and_change_detection(self):
        original = showcase.read_json(showcase.OUT / "captures.json")["programs"]["toybox"]
        for stage, message in (("verify", "did not all verify"), ("detect-change", "not detected")):
            record = copy.deepcopy(original)
            command, = [c for c in record["commands"] if c.get("stage") == stage]
            command["stdout"] = "raytrace.c:  OK\nraytrace:  OK\n"  # Missing image result.
            with self.assertRaisesRegex(ValueError, message):
                visuals.draw("toybox", record)

    def test_fpu_capture_rejects_missing_worker_module_or_nonce(self):
        record = showcase.read_json(showcase.OUT / "captures.json")["linux"]["fpu"]
        log = (showcase.OUT / "linux-fpu.stdout.txt").read_text()
        self.assertEqual(showcase.fpu_facts(log, record["nonce"]), record["facts"])
        for changed in (log.replace("# [OK]\ttest_fpu\n", "", 1),
                        log.replace("module test_fpu loaded", "module test_fpu missing"),
                        log.replace("EVO_BOOT_NONCE", "WRONG_NONCE"),
                        log.replace("reboot: Power down", "not powered off")):
            with self.assertRaises(ValueError):
                showcase.fpu_facts(changed, record["nonce"])

    def test_boot_facts_require_ordered_milestones_and_bound_nonce(self):
        facts = showcase.read_json(showcase.OUT / "captures.json")["linux"]["facts"]
        lines = [facts[k]["line"] for k in ("reset", "cpus", "memory", "devices", "init")]
        lines += ["EVO_INIT_ALIVE", "EVO_BOOT_NONCE " + facts["nonce"], "EVO_INIT_DONE", facts["poweroff"]["line"]]
        log = "\n".join(lines) + "\n"
        self.assertEqual(showcase.boot_facts(log, facts["nonce"]), facts)
        with self.assertRaisesRegex(ValueError, "missing/out of order"):
            showcase.boot_facts(log, "different-boot")
        with self.assertRaisesRegex(ValueError, "missing boot milestone"):
            showcase.boot_facts(log.replace("devtmpfs: initialized", "not initialized"), facts["nonce"])
        with self.assertRaisesRegex(ValueError, "missing/out of order"):
            showcase.boot_facts(log.replace("EVO_INIT_ALIVE", "EVO_INIT_DONE").replace("\nEVO_INIT_DONE\n[", "\nEVO_INIT_ALIVE\n["), facts["nonce"])

    def test_sprite_fixture_is_well_formed(self):
        scene = showcase.demo_inputs().sprite_sheet()
        self.assertEqual(len(bytes.fromhex(scene["rgb"])), scene["width"] * scene["height"] * 3)

    def test_committed_captures_match_assets_and_output(self):
        showcase.check()

    def test_missing_asset_is_not_accepted(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(showcase, "OUT", Path(directory)), patch.object(showcase, "read_json", return_value={"assets": {}}):
                with self.assertRaisesRegex(ValueError, "missing showcase assets"):
                    showcase.check()

    def test_readme_gallery_has_four_rows_of_three(self):
        class Gallery(HTMLParser):
            images = 0
            cells = 0
            rows = []
            current = 0

            def handle_starttag(self, tag, attrs):
                if tag == "img":
                    self.images += 1
                if tag == "td":
                    self.cells += 1
                    self.current += 1
                for name, value in attrs:
                    if name in ("href", "src"):
                        self.assert_exists(value)

            def handle_endtag(self, tag):
                if tag == "tr":
                    self.rows.append(self.current)
                    self.current = 0

        gallery = Gallery()
        gallery.assert_exists = lambda value: self.assertTrue((showcase.ROOT / value.split("#")[0]).is_file(), value)
        section = (showcase.ROOT / "README.md").read_text().split("## Built with EvoCC\n", 1)[1].split("## Status", 1)[0]
        gallery.feed(section)
        self.assertEqual((gallery.images, gallery.cells), (12, 12))
        self.assertEqual(gallery.rows, [3, 3, 3, 3])


if __name__ == "__main__":
    unittest.main()
