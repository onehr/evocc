"""Release binaries must not carry paths of the build machine."""

from contextlib import contextmanager
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class ReleasePathTests(unittest.TestCase):
    @contextmanager
    def fixture(self):
        with tempfile.TemporaryDirectory(prefix="evocc-release-paths-") as directory:
            root = Path(directory)
            target = "aarch64-apple-darwin"
            home = Path(tempfile.mkdtemp(prefix="evocc-release-home-"))
            (root / "tools").mkdir()
            shutil.copyfile(ROOT / "tools/release.sh", root / "tools/release.sh")
            for name in ("README.md", "LICENSE", "NOTICE", "THIRD-PARTY-NOTICES.md"):
                (root / name).write_text("Synthetic packaging fixture.\n")
            shutil.copytree(ROOT / "licenses/runtime", root / "licenses/runtime")
            (root / "CHANGELOG.md").write_text("# Changelog\n\n## [0.1.0] - Unreleased\n")
            manifest = root / "crates/evocc/Cargo.toml"
            manifest.parent.mkdir(parents=True)
            manifest.write_text('[package]\nversion = "0.1.0"\n')
            for relative in ("prebuilt/dist", f"prebuilt/{target}/dist",
                             "lean/.lake/build/bin"):
                directory = root / relative
                directory.mkdir(parents=True)
                names = ("evoir-sem",) if relative.startswith("lean/") else (
                    "evo", "evocc", "evocc-checker")
                for name in names:
                    binary = directory / name
                    binary.write_text("#!/bin/sh\nprintf '%s\\n' 'evocc 0.1.0 (fixture)'\n")
                    binary.chmod(0o755)
            env = {**os.environ, "CARGO_TARGET_DIR": str(root / "prebuilt"),
                   "HOME": str(home), "SOURCE_DATE_EPOCH": "0"}
            try:
                yield root, home, env
            finally:
                shutil.rmtree(home, ignore_errors=True)

    def run_release(self, root, env):
        return subprocess.run(
            ["sh", str(root / "tools/release.sh"), "--skip-build",
             "--target", "aarch64-apple-darwin", "--out", str(root / "dist")],
            cwd=root, env=env, capture_output=True, text=True, timeout=15,
        )

    def test_checkout_root_in_a_binary_refuses_the_archive(self):
        with self.fixture() as (root, _, env):
            binary = root / "prebuilt/dist/evo"
            binary.write_text(binary.read_text() + f"# {root}\n")
            result = self.run_release(root, env)
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("build path", result.stdout + result.stderr)
            self.assertEqual(list((root / "dist").glob("*.tar.gz")), [])

    def test_home_in_a_binary_refuses_the_archive(self):
        with self.fixture() as (root, home, env):
            binary = root / "prebuilt/dist/evocc"
            binary.write_text(binary.read_text() + f"# {home}\n")
            result = self.run_release(root, env)
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("build path", result.stdout + result.stderr)
            self.assertEqual(list((root / "dist").glob("*.tar.gz")), [])

    def test_evoir_sem_home_is_allowed(self):
        with self.fixture() as (root, home, env):
            binary = root / "lean/.lake/build/bin/evoir-sem"
            binary.write_text(binary.read_text() + f"# {home}\n")
            result = self.run_release(root, env)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue((root / "dist/evocc-0.1.0-aarch64-apple-darwin.tar.gz").is_file())

    def test_evoir_sem_checkout_root_refuses_the_archive(self):
        with self.fixture() as (root, _, env):
            binary = root / "lean/.lake/build/bin/evoir-sem"
            binary.write_text(binary.read_text() + f"# {root}\n")
            result = self.run_release(root, env)
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("build path", result.stdout + result.stderr)
            self.assertEqual(list((root / "dist").glob("*.tar.gz")), [])

    def test_clean_binaries_write_the_archive(self):
        with self.fixture() as (root, _, env):
            result = self.run_release(root, env)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue((root / "dist/evocc-0.1.0-aarch64-apple-darwin.tar.gz").is_file())


if __name__ == "__main__":
    unittest.main()
