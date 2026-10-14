"""The Lean receipt must bind the pinned text of spec/evoir.md.

verification.md section 5.4: `tools/render-status.py --check` fails when the
revision or the SHA-256 that `tests/contracts/lean-p0-receipt.json` binds
differs from the `spec/evoir.md` pin in `spec/pins.json`.
"""

from __future__ import annotations

from contextlib import redirect_stderr, redirect_stdout
import importlib.util
import io
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("render_status", ROOT / "tools" / "render-status.py")
status = importlib.util.module_from_spec(spec)
spec.loader.exec_module(status)

PINNED = "a" * 64
OTHER = "b" * 64
PINS = {
    "format": "evocc-spec-pins-v1",
    "pins": {"spec/evoir.md": {"revision": "2.5.0", "sha256": PINNED}},
}


def receipt(revision: str = "2.5.0", digest: str = PINNED) -> dict:
    line = f"**Revision {revision}** | **Pinned ground truth** | Some change on 2026-10-14"
    return {"binding": {"spec_revision": line, "spec_sha256": digest}}


class LeanBindingTests(unittest.TestCase):
    def test_a_receipt_that_binds_the_pin_passes(self):
        self.assertEqual(status.lean_binding_errors(receipt(), PINS), [])

    def test_a_stale_digest_fails(self):
        errors = status.lean_binding_errors(receipt(digest=OTHER), PINS)
        self.assertEqual(len(errors), 1, errors)
        self.assertIn(f"SHA-256 {OTHER}", errors[0])
        self.assertIn(f"pins {PINNED}", errors[0])
        self.assertIn("tools/gate-lean.sh", errors[0])

    def test_a_stale_revision_fails_with_the_pinned_digest(self):
        errors = status.lean_binding_errors(receipt(revision="2.4.0"), PINS)
        self.assertEqual(len(errors), 1, errors)
        self.assertIn("revision 2.4.0", errors[0])
        self.assertIn("pins 2.5.0", errors[0])

    def test_a_stale_revision_and_digest_give_two_findings(self):
        self.assertEqual(len(status.lean_binding_errors(receipt("2.3.0", OTHER), PINS)), 2)

    def test_missing_evidence_fails_closed(self):
        unmarked = {"binding": {"spec_revision": "Revision 2.5.0", "spec_sha256": PINNED}}
        incomplete = {"pins": {"spec/evoir.md": {"revision": "2.5.0"}}}
        for label, bound, pins in (
            ("no receipt", {}, PINS),
            ("no binding fields", {"binding": {}}, PINS),
            ("revision line without its marker", unmarked, PINS),
            ("no pins", receipt(), {}),
            ("incomplete pin", receipt(), incomplete),
        ):
            with self.subTest(label):
                self.assertTrue(status.lean_binding_errors(bound, pins))

    def test_check_mode_reports_a_stale_binding_and_accepts_a_current_one(self):
        versions = {
            "evoir": status.revision(ROOT / "spec" / "evoir.md"),
            "evomir": status.revision(ROOT / "spec" / "evomir.md"),
        }
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "status.md"
            for label, bound, expected in (("current", receipt(), 0), ("stale", receipt(digest=OTHER), 1)):
                receipts = {status.LEAN_RECEIPT: bound}
                # docs/status.md matches, so only the binding decides.
                output.write_text(status.render_document(receipts, **versions))
                errors = io.StringIO()
                with self.subTest(label), \
                        patch.object(status, "load_receipts", return_value=receipts), \
                        patch.object(status, "spec_pins", return_value=PINS), \
                        patch.object(status, "OUT", output), \
                        redirect_stdout(io.StringIO()), redirect_stderr(errors):
                    self.assertEqual(status.main(["--check"]), expected)
                    if expected:
                        self.assertIn(f"SHA-256 {OTHER}", errors.getvalue())
                    else:
                        self.assertEqual(errors.getvalue(), "")


if __name__ == "__main__":
    unittest.main()
