#!/usr/bin/env python3
"""Rung E1 of the evaluation ladder: zlib 1.3.2.

Pinned in docs/roadmap.md to 1.3.2 (2026-02-17, zlib.net). The subject's own
oracle is ``make test``: ``example`` exercises deflate, inflate, dictionaries
and gzip streams and prints "zlib test OK", and ``minigzip`` round trips a
file through the gzip format.

The build is configured ``--static`` so that every unit has exactly one
object; the shared library asks for position-independent code, which is a
back-end rung of its own and would measure something other than the front
end. The adjustment is recorded in the receipt.

Usage: python3 tools/ladder/zlib.py
"""

from __future__ import annotations

import os
import sys

# Running this file puts its directory first on the module search path, where
# a file named for its subject shadows the standard library's own ``zlib``.
# Drop the directory before anything else is imported; the lane's shared
# module is loaded by path just below.
_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path[:] = [entry for entry in sys.path if os.path.abspath(entry or ".") != _HERE]

import importlib.util  # noqa: E402
from pathlib import Path  # noqa: E402

_spec = importlib.util.spec_from_file_location(
    "ladder_subject", Path(__file__).resolve().parent / "subject.py"
)
ladder = importlib.util.module_from_spec(_spec)
sys.modules["ladder_subject"] = ladder
_spec.loader.exec_module(ladder)

#: The subject's own flags, with ``-fPIC`` dropped and ``-no-pie`` added.
#: zlib's Makefile links its test programs with ``CFLAGS``, so this is where
#: the link mode has to be said.
CFLAGS = "-O3 -D_LARGEFILE64_SOURCE=1 -DHAVE_HIDDEN -no-pie"

ZLIB = ladder.Subject(
    name="zlib",
    rung="E1",
    pin="1.3.2 (2026-02-17, zlib.net)",
    url="https://zlib.net/zlib-1.3.2.tar.gz",
    sha256="bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16",
    tree="zlib-1.3.2",
    configure="CC=gcc ./configure --static",
    build=f"make -j4 CFLAGS='{CFLAGS}'",
    test=f"make test CFLAGS='{CFLAGS}'",
    relink=f"make -j4 CFLAGS='{CFLAGS}'",
    notes=(
        "configured --static: the shared library wants position-independent "
        "code, which is a back end rung of its own",
        "CFLAGS replaced to drop -fPIC and link -no-pie: EvoCC emits "
        "position-dependent code, and Debian's default PIE link refuses an "
        "ADRP against an interposable data symbol such as stderr",
    ),
)


if __name__ == "__main__":
    sys.exit(ladder.main(ZLIB))
