"""The header export preserves files and records their actual byte hashes."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tools/export-sysroot.sh"
CONTRACT = ROOT / "contracts/sysroot-aarch64-unknown-linux-gnu.json"


def check_files(root, contract):
    # An export made before a directory was excluded still holds it. Those
    # files never ship, so the check ignores them.
    excluded = tuple(path + "/" for path in contract.get("excluded_directories", []))
    actual = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
              for p in root.rglob("*") if p.is_file()}
    actual = {path: digest for path, digest in actual.items() if not path.startswith(excluded)}
    if actual != contract["files"]:
        raise AssertionError("the sysroot files differ from the contract; "
                             "export again into an empty directory")
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
        self.assertIn("evo-nexlab:sealed", result.stdout)

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

    def test_excluded_directories_are_not_shipped(self):
        contract = json.loads(CONTRACT.read_text())
        excluded = contract["excluded_directories"]
        self.assertEqual(excluded, sorted(set(excluded)))
        for directory in ("usr/include/c++", "usr/include/clang",
                          "usr/include/aarch64-linux-gnu/c++", "usr/include/x86_64-linux-gnu"):
            self.assertIn(directory, excluded)
        for path in contract["files"]:
            self.assertFalse(path.startswith(tuple(d + "/" for d in excluded)), path)
        # The C search path of the driver stays: the multiarch and GCC directories.
        self.assertIn("usr/include/aarch64-linux-gnu/asm/unistd.h", contract["files"])
        self.assertIn("usr/lib/gcc/aarch64-linux-gnu/14/include/stddef.h", contract["files"])

    def test_an_older_export_may_hold_excluded_directories_only(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "usr/include").mkdir(parents=True)
            (root / "usr/include/stdio.h").write_bytes(b"stdio")
            contract = {"excluded_directories": ["usr/include/c++"],
                        "files": {"usr/include/stdio.h": hashlib.sha256(b"stdio").hexdigest()}}
            (root / "usr/include/c++/14").mkdir(parents=True)
            (root / "usr/include/c++/14/vector").write_bytes(b"vector")
            check_files(root, contract)
            (root / "usr/include/extra.h").write_bytes(b"extra")
            with self.assertRaises(AssertionError):
                check_files(root, contract)

    def test_default_image_is_the_contract_image(self):
        contract = json.loads(CONTRACT.read_text())
        script = SCRIPT.read_text()
        image = re.search(r'^pinned_image = "([^"]+)"$', script, re.M)
        digest = re.search(r'^pinned_digest = "([^"]+)"$', script, re.M)
        self.assertEqual(image and image[1], contract["image"])
        self.assertEqual(digest and digest[1], contract["image_digest"])

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
            for text in ("musl/COPYRIGHT", "libuv/LICENSE", "gmp/lgpl-3.0.txt", "gmp/gpl-3.0.txt"):
                self.assertIn(prefix + "licenses/runtime/" + text, files)
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
