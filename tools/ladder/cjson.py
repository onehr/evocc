#!/usr/bin/env python3
"""Rung E3 of the evaluation ladder: cJSON v1.7.19.

Pinned in docs/roadmap.md to v1.7.19 (2025-09-09, github.com/DaveGamble/cJSON).

Two things about this subject are not what the roadmap assumed, and both are
recorded in the receipt rather than worked around. The Unity suite the
roadmap names as the oracle is driven by CMake, and the sealed container has
no cmake; the release's own Makefile is therefore what runs, and its ``make
test`` builds ``cJSON_test`` by compiling ``cJSON.c`` and ``test.c`` together
in one command instead of linking ``cJSON.o``. So the suite cannot judge a
substituted object even when there is one: the two objects this build
produces go into ``libcjson.a`` and ``libcjson.so``, which the suite does not
use.

The measurement is still worth taking, because it says what the front end
does with the two units, and that is the number the ladder wants.

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

CJSON = ladder.Subject(
    name="cjson",
    rung="E3",
    pin="v1.7.19 (2025-09-09, github.com/DaveGamble/cJSON)",
    url="https://github.com/DaveGamble/cJSON/archive/refs/tags/v1.7.19.tar.gz",
    sha256="7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562",
    tree="cJSON-1.7.19",
    # Serial: the release's Makefile does not declare that the utils shared
    # library needs cJSON.o, so a parallel build races against itself.
    build="make",
    test="make test",
    # ``make all`` is not idempotent here: it recreates the shared library's
    # symbolic links and fails because they exist. The relink asks for the
    # targets that consume the objects instead.
    relink="make static tests",
    notes=(
        "the Unity suite the roadmap names is built by CMake, which the "
        "sealed container does not have; the release's own Makefile runs "
        "instead",
        "that Makefile's `make test` compiles cJSON.c and test.c together "
        "into one binary rather than linking cJSON.o, so the suite does not "
        "judge a substituted object; the units are measured, the suite "
        "result is the reference build's",
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
