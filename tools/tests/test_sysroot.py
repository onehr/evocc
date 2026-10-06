"""The header export preserves files and records their actual byte hashes."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tools/export-sysroot.sh"
CONTRACT = ROOT / "contracts/sysroot-aarch64-unknown-linux-gnu.json"


def check_files(root, contract):
    actual = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
              for p in root.rglob("*") if p.is_file()}
    if actual != contract["files"]:
        raise AssertionError("the sysroot files differ from the contract")


class HeaderExportTests(unittest.TestCase):
    def test_script_exists_and_documents_its_destination(self):
        result = subprocess.run(["sh", str(SCRIPT), "--help"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("DESTINATION", result.stdout)
        self.assertIn("evo-flight-lab:sealed", result.stdout)

    def test_missing_header_and_changed_bytes_fail_the_contract(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            header = root / "header.h"
            header.write_bytes(b"original")
            contract = {"files": {"header.h": hashlib.sha256(b"original").hexdigest()}}
            check_files(root, contract)
            header.write_bytes(b"changed")
            with self.assertRaises(AssertionError):
                check_files(root, contract)
            header.unlink()
            with self.assertRaises(AssertionError):
                check_files(root, contract)

    def test_case_insensitive_destination_is_refused_before_export(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            probe = root / "Header"
            probe.touch()
            if not (root / "header").exists():
                self.skipTest("this volume is case-sensitive")
            probe.unlink()
            result = subprocess.run(["sh", str(SCRIPT), str(root)],
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn("case-sensitive", result.stderr)
            self.assertEqual(list(root.iterdir()), [])

    def test_real_export_matches_the_tracked_contract_when_present(self):
        root = Path(os.environ.get("EVOCC_SYSROOT", ROOT / "sysroot/aarch64-unknown-linux-gnu"))
        if not root.exists():
            self.skipTest("the exported sysroot is absent")
        contract = json.loads(CONTRACT.read_text())
        self.assertEqual(contract["target"], "aarch64-unknown-linux-gnu")
        self.assertRegex(contract["image_digest"], r"^sha256:[0-9a-f]{64}$")
        self.assertIn("14.2.0", contract["gcc_version"])
        self.assertIn("usr/include/stdio.h", contract["files"])
        check_files(root, contract)


if __name__ == "__main__":
    unittest.main()
