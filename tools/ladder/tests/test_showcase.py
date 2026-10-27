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

    def test_boot_facts_require_ordered_milestones_and_bound_nonce(self):
        facts = showcase.read_json(showcase.OUT / "captures.json")["logs"]["linux"]["facts"]
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

    def test_log_card_quotes_text_verbatim_not_as_markup(self):
        record = {"title": "<Log> & card", "segments": [{"line": 1, "text": ["PASS: <script>x</script>"]},
                                                        {"line": 9, "text": ["\tkey:\tvalue"]}]}
        root = ET.fromstring(visuals.log_card("toybox", record))
        self.assertNotIn("script", [node.tag.rsplit("}", 1)[-1] for node in root.iter()])
        rows = ["".join(node.itertext()) for node in root.iter() if node.tag.endswith("}text")]
        self.assertEqual(rows, ["<Log> & card", "PASS: <script>x</script>", "\u2026", "\tkey:\tvalue".expandtabs()])

    def test_log_excerpt_needs_one_exact_start_and_enough_lines(self):
        text = "a\nPASS: one\nPASS: two\nend\n"
        self.assertEqual(showcase.excerpt(text, [(r"^PASS: one$", 2)]),
                         [{"line": 2, "text": ["PASS: one", "PASS: two"]}])
        for segments in ([(r"^PASS", 1)], [(r"^missing$", 1)], [(r"^end$", 2)]):
            with self.assertRaisesRegex(ValueError, "missing or ambiguous"):
                showcase.excerpt(text, segments)

    def test_log_excerpt_can_end_at_its_matched_line(self):
        text = "a\nPASS: one\nPASS: two\nend\n"
        self.assertEqual(showcase.excerpt(text, [(r"^end$", 2, 1)]),
                         [{"line": 3, "text": ["PASS: two", "end"]}])
        for segments in ([(r"^PASS: one$", 3, 2)], [(r"^end$", 2, 2)]):
            with self.assertRaisesRegex(ValueError, "missing or ambiguous"):
                showcase.excerpt(text, segments)

    def test_sqlite_card_ends_at_the_summary_of_each_run(self):
        # The progress times differ from run to run. The six quoted lines end at the summary.
        tail = ["Test database is /tmp/sqlite-test/testrunner.db",
                "Test log is /tmp/sqlite-test/testrunner.log"]
        runs = {
            "2026-09-19": ["02:11 tcl(1079/1238) r2 ETC 00:16", "02:13 tcl(1108/1238) r2 ETC 00:12",
                           "02:15 tcl(1182/1238) r2 ETC 00:06", "02:16 tcl(1238/1238) ETC 00:00", *tail,
                           "0 errors out of 329803 tests in 02:16 on 4dc872e9 Linux 64-bit"],
            "2026-10-25": ["01:00 tcl(1096/1238) r2 ETC 00:11", "01:02 tcl(1152/1238) r2 ETC 00:07",
                           "01:04 tcl(1235/1238) r2 ETC 00:02", "01:06 tcl(1238/1238) ETC 00:00", *tail,
                           "0 errors out of 329803 tests in 01:06 on de39b8c5 Linux 64-bit"],
        }
        head = ["splitting work across 2 cores", "built testset in 76ms..", "00:00 tcl(0/1238) r2"]
        for run, lines in runs.items():
            with self.subTest(run=run):
                text = "\n".join(head + lines) + "\n"
                self.assertEqual(showcase.excerpt(text, showcase.LOGS["sqlite"][2]),
                                 [{"line": 1, "text": head[:2]}, {"line": 5, "text": lines[1:]}])

    def test_log_card_wraps_a_long_line_without_changing_its_text(self):
        banner = ("[    0.000000] Linux version 6.18.44 (evo@nexlab) (evocc 0.1.0 (fb05debf29c4 2026-10-13), "
                  "GNU ld (GNU Binutils for Debian) 2.44) #1 SMP PREEMPT Sun Aug 9 18:25:35 UTC 2026")
        record = {"title": "Linux", "segments": [{"line": 2, "text": [banner]}]}
        root = ET.fromstring(visuals.log_card("linux", record))
        rows = [node for node in root.iter() if node.tag.endswith("}text")][1:]
        texts = ["".join(node.itertext()) for node in rows]
        self.assertEqual("".join(texts), banner)
        self.assertTrue(all(len(text) <= visuals.LOG_WRAP["linux"] for text in texts))
        self.assertTrue(all(text.endswith(" ") for text in texts[:-1]), texts)
        self.assertEqual(visuals.wrap_spans([("abcdefgh", "text", False)], 3),
                         [[("abc", "text", False)], [("def", "text", False)], [("gh", "text", False)]])
        green = [span.text for row in rows for span in row
                 if span.get("fill") == visuals.TERMINAL["green"] and span.get("font-weight") == "700"]
        self.assertEqual("".join(green), "evocc 0.1.0 (fb05debf29c4 2026-10-13)")

    def test_boot_facts_accept_markers_that_the_console_interleaves(self):
        facts = showcase.read_json(showcase.OUT / "captures.json")["logs"]["linux"]["facts"]
        nonce = facts["nonce"]
        early = [facts[k]["line"] for k in ("reset", "cpus", "memory", "devices")]
        log = "\n".join(early + ["EVO_INIT_ALIVE[    0.09] driver line", f"EVO_BOOT_NONCE {nonce}[    0.10] more",
                                 "EVO_INIT_DONE[    0.11] last", facts["init"]["line"], facts["poweroff"]["line"]]) + "\n"
        self.assertEqual(showcase.boot_facts(log, nonce), facts)
        with self.assertRaisesRegex(ValueError, "missing/out of order"):
            showcase.boot_facts(log.replace(f"NONCE {nonce}[", f"NONCE {nonce}x["), nonce)

    def test_changed_log_excerpt_is_rejected(self):
        records = showcase.read_json(showcase.OUT / "captures.json")
        records["logs"]["sqlite"]["segments"][1]["text"][-1] = "1 errors out of 329803 tests"
        with patch.object(showcase, "read_json", return_value=records):
            with self.assertRaisesRegex(ValueError, "differs from its recorded excerpt"):
                showcase.check()

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

    def test_readme_gallery_has_five_rows_of_three(self):
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
        section = (showcase.ROOT / "README.md").read_text().split("## What it compiles correctly\n", 1)[1].split("\n## ", 1)[0]
        gallery.feed(section)
        self.assertEqual((gallery.images, gallery.cells), (15, 15))
        self.assertEqual(gallery.rows, [3, 3, 3, 3, 3])


if __name__ == "__main__":
    unittest.main()
