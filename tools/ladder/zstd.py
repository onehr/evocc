#!/usr/bin/env python3
"""Rung E2 of the evaluation ladder: zstd v1.5.7.

Pinned in docs/roadmap.md to v1.5.7 (2025-02-19, github.com/facebook/zstd).
The oracle is the release's own ``make check``: it builds the command line
tool and runs ``tests/playTests.sh``, which compresses and decompresses
through every CLI path and compares the results byte for byte.

Three things about how this subject is driven, all recorded in the receipt:

* The build is the one ``make check`` itself performs, not the default
  build. zstd names its object directory after a hash of the flags, so
  objects compiled with the default flags would be ignored by the test
  configuration; ``make -C tests zstd datagen`` puts them where the suite
  looks.
* The build is serial. zstd's top level runs the library and the programs
  sub-makes at once, and their interleaved output cannot be attributed to a
  directory afterwards.
* Relinking and testing happen on a copy of the tree inside the container.
  playTests.sh checks the permission bits of a file it creates, and the
  shared mount does not preserve them; on the container's own filesystem the
  suite passes. The copy keeps timestamps, so no object is recompiled, which
  the receipt verifies from the relink log. The suite also runs ``file`` on
  the binary before starting, and the sealed container has no ``file``, so a
  shim that prints its argument stands in for that one decorative line.

Usage: python3 tools/ladder/zstd.py
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

WORKSPACE = "/tmp/zstd-check"
SHIM = "mkdir -p /tmp/shim && ln -sf /bin/echo /tmp/shim/file"
COPY = f"rm -rf {WORKSPACE} && cp -a /work/zstd/zstd-1.5.7 {WORKSPACE} && cd {WORKSPACE}"

ZSTD = ladder.Subject(
    name="zstd",
    rung="E2",
    pin="v1.5.7 (2025-02-19, github.com/facebook/zstd)",
    url="https://github.com/facebook/zstd/releases/download/v1.5.7/zstd-1.5.7.tar.gz",
    sha256="eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3",
    tree="zstd-1.5.7",
    build="make -C tests zstd datagen V=1",
    relink=f"{COPY} && make -C tests zstd datagen",
    test=f"{SHIM} && {COPY} && PATH=/tmp/shim:$PATH make check",
    workspace=WORKSPACE,
    notes=(
        "built the way `make check` builds: zstd names its object directory "
        "after a hash of the flags, so the default build's objects are not "
        "the ones the suite links",
        "built serially: the top level runs two sub-makes at once and their "
        "output cannot be attributed to a directory afterwards",
        "relinked and tested on a copy of the tree inside the container: "
        "playTests.sh checks the permission bits of a file it creates and "
        "the shared mount does not preserve them",
        "a shim stands in for file(1), which the sealed container does not "
        "carry and which `make check` runs once for decoration",
    ),
    defects=(
        {
            "unit": "programs/obj/*/timefn.o",
            "failure": "playTests.sh dies with a segmentation fault as soon "
            "as the CLI takes a timestamp",
            "cause": "an aggregate of sixteen bytes or fewer is passed and "
            "returned indirectly, through a hidden pointer in x0, where "
            "AAPCS64 passes and returns it in general registers. A GCC "
            "caller therefore hands the callee the struct's value where the "
            "callee expects the address to write through.",
            "repro": (
                "typedef struct { unsigned long long t; } T;\n"
                "T get(void) { T r; r.t = 42; return r; }\n"
                "unsigned long long span(T a, T b) { return b.t - a.t; }\n"
            ),
            "shownBy": "compile the three lines with EvoCC, call get() and "
            "span() from a GCC-compiled main, link and run: it faults where "
            "the GCC-compiled unit prints 42",
        },
    ),
)


if __name__ == "__main__":
    sys.exit(ladder.main(ZSTD))
