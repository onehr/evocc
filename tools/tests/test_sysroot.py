"""The header export preserves files and records their actual byte hashes."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
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
    if len(actual) != len({path.lower() for path in actual}):
        raise AssertionError("the sysroot contains case-colliding paths")


class HeaderExportTests(unittest.TestCase):
    def test_release_workflow_exports_headers_and_runs_the_cross_object(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        for command in ("tools/export-sysroot.sh", "linux-headers.tar.gz",
                        "cross-hello.o", 'test "$(./cross-hello)"'):
            self.assertIn(command, workflow)

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

    def test_excluded_collisions_are_sorted_and_not_shipped(self):
        contract = json.loads(CONTRACT.read_text())
        excluded = contract["excluded_case_collisions"]
        self.assertEqual(excluded, sorted(set(excluded)))
        for path in excluded:
            self.assertNotEqual(Path(path).name, Path(path).name.lower())
            self.assertNotIn(path, contract["files"])

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

    def test_release_archive_carries_complete_headers_beside_bin(self):
        archive = os.environ.get("EVOCC_RELEASE_ARCHIVE")
        if not archive:
            self.skipTest("no release archive was supplied")
        contract = json.loads(CONTRACT.read_text())
        with tarfile.open(archive) as package:
            files = {member.name: member for member in package.getmembers() if member.isfile()}
            sums = [name for name in files if name.endswith("/SHA256SUMS")]
            self.assertEqual(len(sums), 1)
            prefix = sums[0].removesuffix("SHA256SUMS")
            for binary in ("evo", "evocc", "evocc-checker", "evoir-sem"):
                self.assertIn(prefix + "bin/" + binary, files)
            listed = {}
            for line in package.extractfile(files[sums[0]]).read().decode().splitlines():
                digest, name = line.split("  ", 1)
                data = package.extractfile(files[prefix + name]).read()
                self.assertEqual(hashlib.sha256(data).hexdigest(), digest, name)
                listed[name] = digest
            self.assertEqual(set(files), {prefix + name for name in listed} | {sums[0]})
            header_prefix = "sysroot/aarch64-unknown-linux-gnu/"
            headers = {name.removeprefix(header_prefix): digest for name, digest in listed.items()
                       if name.startswith(header_prefix)}
            self.assertEqual(headers, contract["files"])
            self.assertEqual(len(headers), len({path.lower() for path in headers}))


if __name__ == "__main__":
    unittest.main()
