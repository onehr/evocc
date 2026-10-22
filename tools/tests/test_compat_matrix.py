"""Channel metadata preserves retained evidence and names documented versions."""

from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tools/gen-compat-matrix.py"
CHAPTER = Path("book/src/compatibility.md")
RECEIPTS = ROOT / "tests/contracts"


class CompatibilityMatrixTests(unittest.TestCase):
    @contextmanager
    def fixture(self):
        self.assertTrue(SCRIPT.is_file(), "tools/gen-compat-matrix.py is missing")
        with tempfile.TemporaryDirectory(prefix="evocc-compat-matrix-") as directory:
            root = Path(directory)
            for name in ("tools", "tests/contracts", "book/src"):
                (root / name).mkdir(parents=True)
            for name in ("gen-compat-matrix.py", "bench.py"):
                shutil.copyfile(ROOT / "tools" / name, root / "tools" / name)
            for path in sorted(RECEIPTS.glob("ladder-*-receipt.json")):
                shutil.copyfile(path, root / "tests/contracts" / path.name)
            yield root

    def run_generator(self, root, *args):
        return subprocess.run(
            [sys.executable, str(root / "tools/gen-compat-matrix.py"), *args],
            cwd=root, capture_output=True, text=True, timeout=15,
        )

    def generate(self, root):
        result = self.run_generator(root)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((root / CHAPTER).is_file(), f"missing generated {CHAPTER}")
        return (root / CHAPTER).read_text(encoding="utf-8")

    def row(self, chapter, receipt):
        lines = [line for line in chapter.splitlines() if receipt in line]
        self.assertTrue(lines, f"missing receipt row: {receipt}")
        return " ".join(lines).replace(",", "")

    def test_generator_budget_and_every_current_receipt_is_linked(self):
        with self.fixture() as root:
            self.assertLessEqual(len(SCRIPT.read_text().splitlines()), 150)
            chapter = self.generate(root)
            expected = {path.name for path in RECEIPTS.glob("ladder-*-receipt.json")}
            self.assertTrue(expected)
            linked = set(re.findall(r"ladder-[a-z0-9-]+-receipt\.json", chapter))
            self.assertEqual(linked, expected)
            for name in expected:
                with self.subTest(receipt=name):
                    self.assertRegex(chapter, r"\]\([^\n)]*" + re.escape(name) + r"\)")

    def test_output_is_deterministic_and_generation_preserves_receipts(self):
        with self.fixture() as root:
            paths = sorted((root / "tests/contracts").glob("ladder-*-receipt.json"))
            before = {path.name: path.read_bytes() for path in paths}
            first = self.generate(root)
            self.assertEqual(self.generate(root), first)
            self.assertEqual({path.name: path.read_bytes() for path in paths}, before)

    def test_check_accepts_current_output_and_refuses_stale_without_writing(self):
        with self.fixture() as root:
            missing = self.run_generator(root, "--check")
            self.assertNotEqual(missing.returncode, 0)
            self.assertFalse((root / CHAPTER).exists())
            self.generate(root)
            current = self.run_generator(root, "--check")
            self.assertEqual(current.returncode, 0, current.stdout + current.stderr)
            stale = b"# Deliberately stale retained compatibility matrix\n"
            (root / CHAPTER).write_bytes(stale)
            result = self.run_generator(root, "--check")
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual((root / CHAPTER).read_bytes(), stale)

    def test_retained_results_keep_original_denominators_and_claim_ceilings(self):
        with self.fixture() as root:
            chapter = self.generate(root)
            self.assertIn("retained", chapter.lower())
            self.assertIn("not revalidated", chapter.lower())
            for name, passed, total, status in (
                ("e0", 1522, 1692, "PROFILE_CLEAN"),
                ("csmith", 932, 1000, "INCOMPLETE"),
                ("llvm-test-suite", 1039, 1735, "INCOMPLETE"),
                ("musl", 1271, 1339, "INCOMPLETE"),
                ("toybox", 203, 203, "INCOMPLETE"),
            ):
                with self.subTest(receipt=name):
                    row = self.row(chapter, f"ladder-{name}-receipt.json")
                    self.assertRegex(row, rf"\b{passed}\s*/\s*{total}\b")
                    self.assertIn(status, row)
            doom = self.row(chapter, "ladder-doom-receipt.json")
            self.assertRegex(doom, r"\b81\s*/\s*81\b")
            self.assertIn("PASS", doom)
            self.assertIn("headless", chapter.lower())

    def test_invalid_denominator_is_not_published_as_a_compatibility_result(self):
        with self.fixture() as root:
            self.generate(root)
            before = (root / CHAPTER).read_bytes()
            receipt = root / "tests/contracts/ladder-csmith-receipt.json"
            data = json.loads(receipt.read_text())
            data["latest"]["programs"] = 0
            receipt.write_text(json.dumps(data))
            result = self.run_generator(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual((root / CHAPTER).read_bytes(), before)


class ChannelWorkflowTests(unittest.TestCase):
    def test_nightly_reuses_release_build_instead_of_duplicating_it(self):
        path = ROOT / ".github/workflows/nightly.yml"
        self.assertTrue(path.is_file(), "nightly workflow is missing")
        nightly = path.read_text()
        release = (ROOT / ".github/workflows/release.yml").read_text()
        self.assertLessEqual(len(nightly.splitlines()), 60)
        self.assertIn("schedule:", nightly)
        self.assertIn("cron:", nightly)
        self.assertIn("./.github/workflows/release.yml", nightly)
        self.assertRegex(nightly, r"channel:\s*['\"]?nightly")
        self.assertIn("workflow_call:", release)
        self.assertIn('tags: ["v*"]', release)
        for command in ("tools/release.sh", "./bin/evoir-sem --selftest",
                        "--interpret-c-hir hello.c", 'test "$(./cross-hello)"'):
            self.assertIn(command, release)


class ReleaseChangelogTests(unittest.TestCase):
    @contextmanager
    def fixture(self, changelog, inherited_version=False):
        with tempfile.TemporaryDirectory(prefix="evocc-release-version-") as directory:
            root = Path(directory)
            target = "aarch64-apple-darwin"
            (root / "tools").mkdir()
            shutil.copyfile(ROOT / "tools/release.sh", root / "tools/release.sh")
            for name in ("README.md", "LICENSE", "NOTICE", "THIRD-PARTY-NOTICES.md"):
                (root / name).write_text("Synthetic packaging fixture.\n")
            # release.sh pins the runtime license texts and refuses an archive without them.
            shutil.copytree(ROOT / "licenses/runtime", root / "licenses/runtime")
            (root / "CHANGELOG.md").write_text(changelog)
            manifest = root / "crates/evocc/Cargo.toml"
            manifest.parent.mkdir(parents=True)
            if inherited_version:
                manifest.write_text('[package]\nversion.workspace = true\n')
                (root / "Cargo.toml").write_text(
                    '[workspace.package]\nversion = "0.1.0"\n')
            else:
                manifest.write_text('[package]\nversion = "0.1.0"\n')
            # --skip-build reads these tiny stand-ins; no Cargo or Lake runs.
            for relative in ("prebuilt/dist", f"prebuilt/{target}/dist",
                             "lean/.lake/build/bin"):
                directory = root / relative
                directory.mkdir(parents=True)
                names = ("evoir-sem",) if relative.startswith("lean/") else (
                    "evo", "evocc", "evocc-checker")
                for name in names:
                    binary = directory / name
                    binary.write_text("#!/bin/sh\nexit 0\n" if inherited_version else
                                      "#!/bin/sh\nprintf '%s\\n' 'evocc 0.1.0 (fixture)'\n")
                    binary.chmod(0o755)
            env = {**os.environ, "CARGO_TARGET_DIR": str(root / "prebuilt"),
                   "SOURCE_DATE_EPOCH": "0"}
            result = subprocess.run(
                ["sh", str(root / "tools/release.sh"), "--skip-build",
                 "--target", target, "--out", str(root / "dist")],
                cwd=root, env=env, capture_output=True, text=True, timeout=15,
            )
            yield root, result

    def test_release_packages_a_version_with_its_own_changelog_heading(self):
        with self.fixture("# Changelog\n\n## [0.1.0] - Unreleased\n") as (root, result):
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue((root / "dist/evocc-0.1.0-aarch64-apple-darwin.tar.gz").is_file())

    def test_release_falls_back_to_the_inherited_workspace_version(self):
        with self.fixture("# Changelog\n\n## [0.1.0] - Unreleased\n",
                          inherited_version=True) as (root, result):
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue((root / "dist/evocc-0.1.0-aarch64-apple-darwin.tar.gz").is_file())

    def test_release_refuses_a_version_without_its_own_changelog_heading(self):
        for changelog in (
            "# Changelog\n\n## [9.9.9] - Unreleased\n",
            "# Changelog\n\n## [9.9.9] - Unreleased\n\nText mentions 0.1.0 only.\n",
            "# Changelog\n\n## [0.1.00] - Unreleased\n",
        ):
            with self.subTest(changelog=changelog), self.fixture(changelog) as (root, result):
                self.assertNotEqual(result.returncode, 0, "undocumented version was packaged")
                self.assertIn("changelog", (result.stdout + result.stderr).lower())
                self.assertEqual(list((root / "dist").glob("*.tar.gz")), [])
                self.assertFalse((root / "dist/SHA256SUMS").exists())


if __name__ == "__main__":
    unittest.main()
