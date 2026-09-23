#!/usr/bin/env python3
"""Rung E3 of the evaluation ladder: cJSON v1.7.19.

Pinned in docs/roadmap.md to v1.7.19 (2025-09-09, github.com/DaveGamble/cJSON).

The public Makefile test and five public-API controls link installed library
objects. In addition, every program in the pinned CMake Unity selection is
compiled as its own measured translation unit, including common.h's embedded
cJSON.c and private helpers. Relinking never recompiles those test objects.
Unity itself remains a separately identified reference-built assertion harness.

Usage: python3 tools/ladder/cjson.py
"""

from __future__ import annotations

import importlib.util
from pathlib import Path
import sys

_spec = importlib.util.spec_from_file_location(
    "ladder_subject", Path(__file__).resolve().parent / "subject.py"
)
ladder = importlib.util.module_from_spec(_spec)
sys.modules["ladder_subject"] = ladder
_spec.loader.exec_module(ladder)

PUBLIC_TESTS = ("cjson_add", "compare_tests", "minify_tests",
                "old_utils_tests", "misc_utils_tests")
# Exact unity_tests + cjson_utils_tests in v1.7.19 tests/CMakeLists.txt.
UPSTREAM_TESTS = (
    "parse_examples", "parse_number", "parse_hex4", "parse_string",
    "parse_array", "parse_object", "parse_value", "print_string",
    "print_number", "print_array", "print_object", "print_value",
    "misc_tests", "parse_with_opts", "compare_tests", "cjson_add",
    "readme_examples", "minify_tests", "json_patch_tests",
    "old_utils_tests", "misc_utils_tests",
)
FULL_BUILD = """(
set -e
mkdir -p tests/evocc-full
printf '%%s\\n' "${CC:-gcc-14} -std=c89 -O2 -c tests/unity/src/unity.c -o tests/evocc-full/unity.o"
"${CC:-gcc-14}" -std=c89 -O2 -c tests/unity/src/unity.c -o tests/evocc-full/unity.o
for test in %s; do
  printf '%%s\\n' "${CC:-gcc-14} -std=c89 -O2 -c tests/$test.c -o tests/evocc-full/$test.o"
  "${CC:-gcc-14}" -std=c89 -O2 -c tests/$test.c -o tests/evocc-full/$test.o
done
)
""" % " ".join(UPSTREAM_TESTS)
RELINK = """(
set -e
rm -f cJSON_test
make 'CJSON_TEST_SRC=test.c cJSON.o' static tests
for test in %s; do
  "${CC:-gcc-14}" -DCJSON_TESTS_COMMON_H -include ./cJSON.h -include stdbool.h -include string.h tests/$test.c tests/unity/src/unity.c cJSON.o cJSON_Utils.o -lm -o tests/$test
done
for test in %s; do
  "${CC:-gcc-14}" tests/evocc-full/$test.o tests/evocc-full/unity.o libcjson_utils.a libcjson.a -lm -o tests/evocc-full/$test
done
)
""" % (" ".join(PUBLIC_TESTS), " ".join(UPSTREAM_TESTS))
TEST = """(
set -e
./cJSON_test
for test in %s; do
  ./tests/$test
done
cd tests
for test in %s; do
  ./evocc-full/$test
done
)
""" % (" ".join(PUBLIC_TESTS), " ".join(UPSTREAM_TESTS))

CJSON = ladder.Subject(
    name="cjson",
    rung="E3",
    pin="v1.7.19 (2025-09-09, github.com/DaveGamble/cJSON)",
    url="https://github.com/DaveGamble/cJSON/archive/refs/tags/v1.7.19.tar.gz",
    sha256="7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562",
    tree="cJSON-1.7.19",
    # Serial: the release's Makefile does not declare that the utils shared
    # library needs cJSON.o, so a parallel build races against itself.
    build="(set -e\nmake\n" + FULL_BUILD + RELINK + ")",
    test=TEST,
    # ``make all`` is not idempotent here: it recreates the shared library's
    # symbolic links and fails because they exist. The relink asks for the
    # targets that consume the objects instead.
    relink=RELINK,
    test_objects=("cJSON.o", "cJSON_Utils.o",
                  *(f"tests/evocc-full/{name}.o" for name in UPSTREAM_TESTS)),
    skip_objects=("tests/evocc-full/unity.o",),
    notes=(
        "the release Makefile builds the libraries; its public test is "
        "relinked from test.c and the installed cJSON.o on every trial",
        "five unchanged public-API Unity programs (cjson_add, compare_tests, "
        "minify_tests, old_utils_tests, misc_utils_tests) link both installed "
        "objects; CJSON_TESTS_COMMON_H suppresses common.h's embedded cJSON.c, "
        "with cJSON.h, stdbool.h and string.h supplied explicitly",
        "all 21 programs selected by v1.7.19 tests/CMakeLists.txt, including "
        "private helpers embedded through common.h, are additionally compiled "
        "as measured test translation units and linked from those installed "
        "objects; no assertions or includes are rewritten; Unity alone is "
        "reference-built and excluded from the subject population",
        "the full Unity selection uses explicit -std=c89 -O2 commands beside "
        "the release Makefile build, not a claim to reproduce CMake's optional "
        "sanitizer/Valgrind configurations; tests run from the original tests "
        "directory so the original inputs and json-patch-tests are consumed",
        "The release Makefile explicitly selects -std=c89. The subject "
        "receives that option unchanged; a language-mode refusal is distinct "
        "from an observed floating-point or header-type refusal. Current "
        "first errors are recorded in refusals, not inferred from cJSON's "
        "use of double or historical adjusted-header runs.",
    ),
)


if __name__ == "__main__":
    sys.exit(ladder.main(CJSON))
