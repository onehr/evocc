"""Fresh processes must not mistake the zlib benchmark for Python's codec."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


LADDER = Path(__file__).resolve().parents[1]


class ImportIsolation(unittest.TestCase):
    def fresh(self, statement, directory=LADDER):
        source = (
            "import sys\n"
            f"sys.path.insert(0, {str(directory)!r})\n"
            "sys.modules.pop('zlib', None)\n"
            "before = list(sys.path)\n" + statement + "\nimport zlib\n"
            "data = bytes(range(256)) * 3\n"
            "assert zlib.decompress(zlib.compress(data)) == data\n"
            f"assert zlib.__spec__.origin != {str(LADDER / 'zlib.py')!r}\n"
        )
        result = subprocess.run(
            [sys.executable, "-c", source],
            capture_output=True,
            text=True,
            timeout=20,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_e0_import_preserves_the_callers_search_path(self):
        self.fresh("import e0\nassert sys.path == before")

    def test_mixed_import_does_not_depend_on_e0_being_preloaded(self):
        self.fresh("import mixed\nassert callable(mixed.container)")

    def test_standard_library_consumers_get_the_real_zlib(self):
        self.fresh(
            "import shutil, gzip\nassert sys.path == before\n"
            "assert gzip.decompress(gzip.compress(b'codec')) == b'codec'"
        )

    def test_symlinked_ladder_directory_does_not_reenter_the_proxy(self):
        with tempfile.TemporaryDirectory() as directory:
            alias = Path(directory) / "ladder"
            alias.symlink_to(LADDER, target_is_directory=True)
            self.fresh("import zlib\nassert sys.path == before", alias)

    def test_named_benchmark_import_still_exposes_its_subject(self):
        self.fresh(
            "import importlib.util\n"
            f"spec = importlib.util.spec_from_file_location('ladder_zlib_probe', {str(LADDER / 'zlib.py')!r})\n"
            "module = importlib.util.module_from_spec(spec)\n"
            "spec.loader.exec_module(module)\n"
            "assert module.ZLIB.name == 'zlib'"
        )

    def test_legacy_zlib_lane_entry_point_still_has_its_cli(self):
        result = subprocess.run(
            [sys.executable, str(LADDER / "zlib.py"), "--help"],
            capture_output=True,
            text=True,
            timeout=20,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--evocc", result.stdout)


if __name__ == "__main__":
    unittest.main()
