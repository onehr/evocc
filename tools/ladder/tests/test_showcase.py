"""README output panels preserve text and fail on changed captured assets."""
import importlib.util
from html.parser import HTMLParser
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET

SCRIPT = Path(__file__).resolve().parents[2] / "showcase.py"
SPEC = importlib.util.spec_from_file_location("showcase", SCRIPT)
showcase = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(showcase)


class ShowcaseTests(unittest.TestCase):
    def test_stdout_is_text_not_svg_markup(self):
        svg = showcase.panel("<Title>", "A & B", ['<script>alert("x")</script>'], "stdout")
        root = ET.fromstring(svg)
        self.assertNotIn("script", [node.tag.rsplit("}", 1)[-1] for node in root.iter()])
        self.assertIn('<script>alert("x")</script>', "".join(root.itertext()))

    def test_long_lines_are_wrapped_inside_panel(self):
        root = ET.fromstring(showcase.panel("Test", "stdout", ["x" * 200], "captured"))
        body = [n for n in root if n.attrib.get("fill") == "#d8e4ef"]
        self.assertEqual("".join(n.text or "" for n in body), "x" * 200)
        for node in body:
            self.assertLessEqual(len(node.text) * float(node.attrib["font-size"]) * 0.65, 744.01)
            self.assertLess(float(node.attrib["y"]), 445)

    def test_committed_captures_match_assets_and_output(self):
        showcase.check()

    def test_missing_asset_is_not_accepted(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(showcase, "OUT", Path(directory)), patch.object(showcase, "read_json", return_value={"assets": {}}):
                with self.assertRaisesRegex(ValueError, "missing showcase assets"):
                    showcase.check()

    def test_readme_gallery_has_eight_images_and_local_links(self):
        class Gallery(HTMLParser):
            images = 0
            cells = 0

            def handle_starttag(self, tag, attrs):
                if tag == "img":
                    self.images += 1
                if tag == "td":
                    self.cells += 1
                for name, value in attrs:
                    if name in ("href", "src"):
                        self.assert_exists(value)

        gallery = Gallery()
        gallery.assert_exists = lambda value: self.assertTrue((showcase.ROOT / value).is_file(), value)
        section = (showcase.ROOT / "README.md").read_text().split("## Built with EvoCC\n", 1)[1].split("## Status", 1)[0]
        gallery.feed(section)
        self.assertEqual((gallery.images, gallery.cells), (8, 8))


if __name__ == "__main__":
    unittest.main()
