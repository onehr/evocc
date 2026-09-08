#!/usr/bin/env python3
"""Rung E3 of the evaluation ladder: cJSON v1.7.19.

Pinned in docs/roadmap.md to v1.7.19 (2025-09-09, github.com/DaveGamble/cJSON).

The public Makefile test and five upstream public-API Unity programs link
the installed objects. The default Makefile compiles the reference cJSON.c
into its test binary, while Unity's common.h includes that implementation;
neither can validate substituted objects unchanged. The adaptations below
change only test linkage and preserve the selected test sources. They are
not a claim to run the complete CMake/private-helper Unity suite.

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
RELINK = """(
set -e
rm -f cJSON_test
make 'CJSON_TEST_SRC=test.c cJSON.o' static tests
for test in %s; do
  "${CC:-gcc-14}" -DCJSON_TESTS_COMMON_H -include ./cJSON.h -include stdbool.h -include string.h tests/$test.c tests/unity/src/unity.c cJSON.o cJSON_Utils.o -lm -o tests/$test
done
)
""" % " ".join(PUBLIC_TESTS)
TEST = """(
set -e
./cJSON_test
for test in %s; do
  ./tests/$test
done
)
""" % " ".join(PUBLIC_TESTS)

CJSON = ladder.Subject(
    name="cjson",
    rung="E3",
    pin="v1.7.19 (2025-09-09, github.com/DaveGamble/cJSON)",
    url="https://github.com/DaveGamble/cJSON/archive/refs/tags/v1.7.19.tar.gz",
    sha256="7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562",
    tree="cJSON-1.7.19",
    # Serial: the release's Makefile does not declare that the utils shared
    # library needs cJSON.o, so a parallel build races against itself.
    build="(set -e\nmake\n" + RELINK + ")",
    test=TEST,
    # ``make all`` is not idempotent here: it recreates the shared library's
    # symbolic links and fails because they exist. The relink asks for the
    # targets that consume the objects instead.
    relink=RELINK,
    test_objects=("cJSON.o", "cJSON_Utils.o"),
    notes=(
        "the release Makefile builds the libraries; its public test is "
        "relinked from test.c and the installed cJSON.o on every trial",
        "five unchanged public-API Unity programs (cjson_add, compare_tests, "
        "minify_tests, old_utils_tests, misc_utils_tests) link both installed "
        "objects; CJSON_TESTS_COMMON_H suppresses common.h's embedded cJSON.c, "
        "with cJSON.h, stdbool.h and string.h supplied explicitly",
        "this is the public Makefile test plus 54 public-API Unity assertions, "
        "not the full CMake/private-helper Unity suite; CMake is unavailable "
        "and private helpers cannot test an independently compiled object",
        "cJSON's numbers are doubles, and the back end refuses floating "
        "point, so these units are not expected to pass before the "
        "floating-point rung",
        "both units stop in <math.h> before any of their own code is read: "
        "the driver defines __GNUC__ 14, so glibc uses GCC 14's built-in "
        "vector types, which the front end does not have. With those types "
        "supplied by hand the units refuse on floating point instead "
        "(cJSON.c: __builtin_nanf undeclared; cJSON_Utils.c: E3101, a "
        "function value that is neither an integer nor an object pointer)",
    ),
)


if __name__ == "__main__":
    sys.exit(ladder.main(CJSON))
