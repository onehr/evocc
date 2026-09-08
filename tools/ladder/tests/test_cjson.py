"""The cJSON test commands must execute the installed library objects."""

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


PATH = Path(__file__).resolve().parents[1] / "cjson.py"
spec = importlib.util.spec_from_file_location("cjson_lane", PATH)
lane = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = lane
spec.loader.exec_module(lane)


class ObjectConsumptionTest(unittest.TestCase):
    def test_relink_observes_each_changed_library_object(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "cJSON.h").write_text("int core(void); int utils(void);\n")
            (root / "cJSON.c").write_text("int core(void) { return 42; }\n")
            (root / "cJSON_Utils.c").write_text("int utils(void) { return 7; }\n")
            (root / "test.c").write_text(
                '#include "cJSON.h"\nint main(void) { return core() != 42; }\n'
            )
            tests = root / "tests"
            (tests / "unity/src").mkdir(parents=True)
            (tests / "unity/src/unity.c").write_text("/* test runner fixture */\n")
            for name in ("cjson_add", "compare_tests", "readme_examples", "minify_tests",
                         "old_utils_tests", "misc_utils_tests"):
                (tests / f"{name}.c").write_text(
                    '#include "../cJSON.h"\n'
                    "int main(void) { return core() != 42 || utils() != 7; }\n"
                )
            (root / "Makefile").write_text(
                "CJSON_TEST_SRC = cJSON.c test.c\n"
                ".PHONY: all static tests test\n"
                "all: cJSON.o cJSON_Utils.o cJSON_test\n"
                "static: cJSON.o cJSON_Utils.o\n"
                "tests: cJSON_test\n"
                "test: tests\n\t./cJSON_test\n"
                "cJSON_test: $(CJSON_TEST_SRC)\n"
                "\t$(CC) $(CJSON_TEST_SRC) -o $@\n"
            )

            def run(command):
                return subprocess.run(
                    command, shell=True, cwd=root, text=True, capture_output=True,
                    check=False, timeout=30,
                )

            built = run(lane.CJSON.build)
            self.assertEqual(built.returncode, 0, built.stdout + built.stderr)
            for symbol, object_name in [("core", "cJSON.o"), ("utils", "cJSON_Utils.o")]:
                with self.subTest(object=object_name):
                    originals = {name: (root / name).read_bytes()
                                 for name in ("cJSON.o", "cJSON_Utils.o")}
                    linked = run(lane.CJSON.relink)
                    self.assertEqual(linked.returncode, 0, linked.stdout + linked.stderr)
                    self.assertEqual(run(lane.CJSON.test).returncode, 0)
                    changed = subprocess.run(
                        ["cc", "-x", "c", "-c", "-o", object_name, "-"], cwd=root,
                        input=f"int {symbol}(void) {{ return -1; }}\n", text=True,
                        capture_output=True, check=False,
                    )
                    self.assertEqual(changed.returncode, 0, changed.stderr)
                    linked = run(lane.CJSON.relink)
                    self.assertEqual(linked.returncode, 0, linked.stdout + linked.stderr)
                    self.assertNotEqual(run(lane.CJSON.test).returncode, 0,
                                        f"the tests bypassed changed {object_name}")
                    for name, contents in originals.items():
                        (root / name).write_bytes(contents)
