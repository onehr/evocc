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
        gallery.assert_exists = lambda value: self.assertTrue((showcase.ROOT / value).is_file(), value)
        section = (showcase.ROOT / "README.md").read_text().split("## Built with EvoCC\n", 1)[1].split("## Status", 1)[0]
        gallery.feed(section)
        self.assertEqual((gallery.images, gallery.cells), (12, 12))
        self.assertEqual(gallery.rows, [3, 3, 3, 3])


if __name__ == "__main__":
    unittest.main()
