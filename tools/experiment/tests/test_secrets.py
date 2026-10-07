"""Scan result files and full transcripts without reading any credential file."""
import gzip
import io
import os
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
MARKER = b"sk-or-"


def check_bytes(data, name):
    if MARKER in data:
        raise AssertionError("credential prefix found in " + name)


def scan(path):
    data = path.read_bytes()
    check_bytes(data, str(path))
    if path.name.endswith(".tar.gz"):
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            for member in archive:
                if not member.isfile():
                    continue
                content = archive.extractfile(member).read()
                name = str(path) + ":" + member.name
                check_bytes(content, name)
                if member.name.endswith(".gz"):
                    check_bytes(gzip.decompress(content), name)
    elif path.suffix == ".gz":
        check_bytes(gzip.decompress(data), str(path))


class SecretTests(unittest.TestCase):
    def test_all_committed_results_and_external_transcripts(self):
        paths = set()
        for match in (ROOT / "tests/contracts").glob("agent-repair-*"):
            paths.update(p for p in match.rglob("*") if p.is_file()) if match.is_dir() else paths.add(match)
        external = os.environ.get("EVOCC_REPAIR_TRANSCRIPTS")
        if external:
            directory = Path(external)
            self.assertTrue(directory.is_dir(), "transcript directory is missing")
            paths.update(p for p in directory.rglob("*") if p.is_file())
        for path in sorted(paths):
            with self.subTest(path=path):
                scan(path)

    def test_compressed_transcripts_cannot_hide_a_prefix(self):
        with tempfile.TemporaryDirectory() as work:
            path = Path(work) / "transcript.json.gz"
            path.write_bytes(gzip.compress(MARKER + b"test-only"))
            with self.assertRaisesRegex(AssertionError, "credential prefix"):
                scan(path)

    def test_nested_transcript_archive_is_scanned(self):
        with tempfile.TemporaryDirectory() as work:
            path = Path(work) / "transcripts.tar.gz"
            payload = gzip.compress(MARKER + b"test-only")
            with tarfile.open(path, "w:gz") as archive:
                info = tarfile.TarInfo("0000.json.gz")
                info.size = len(payload)
                archive.addfile(info, io.BytesIO(payload))
            with self.assertRaisesRegex(AssertionError, "credential prefix"):
                scan(path)


if __name__ == "__main__":
    unittest.main()
