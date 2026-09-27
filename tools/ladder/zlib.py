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

_HERE = os.path.dirname(os.path.realpath(__file__))
_codec_paths = [entry for entry in sys.path if os.path.realpath(entry or ".") != _HERE]

if __name__ == "zlib":
    # A fresh shutil/gzip import can reach this file before the shared ladder
    # helpers. It wants Python's codec, not a benchmark Subject. Resolve the
    # codec outside this directory without changing the caller's search path
    # or leaving a partly initialized benchmark in sys.modules['zlib'].
    import importlib.machinery
    import importlib.util

    _spec = importlib.machinery.PathFinder.find_spec("zlib", _codec_paths)
    if _spec is None or _spec.loader is None:
        raise ImportError("Python's zlib codec is unavailable outside tools/ladder")
    _codec = importlib.util.module_from_spec(_spec)
    sys.modules[__name__] = _codec
    _spec.loader.exec_module(_codec)
else:
    # The historical CLI (or a distinctly named path import) still loads the
    # benchmark. Do not place its subject-named directory on the codec path.
    sys.path[:] = _codec_paths

    import importlib.util
    from pathlib import Path

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
        defects=(
            {
                "units": ["deflate.o", "infback.o", "inflate.o"],
                "failure": "refused, E3101: dereference operand did not lower to "
                "a pointer",
                "cause": "a call written through an explicit dereference of a "
                "function pointer. zlib's ZALLOC and ZFREE are (*((strm)->zalloc"
                "))(...); the same call written strm->zalloc(...) compiles.",
                "repro": "void g(void (*f)(void)) { (*f)(); }\n",
            },
            {
                "units": ["trees.o", "zutil.o"],
                "failure": "refused, E3101: generated invalid EvoIR, a value used "
                "in a later block is not a prior local definition",
                "cause": "a subscript whose index expression branches. Any "
                "conditional or short-circuit operator inside [] does it, on an "
                "array or on a pointer.",
                "repro": "int g(int *a, int e) { return a[e ? 1 : 0]; }\n",
            },
        ),
    )

    if __name__ == "__main__":
        sys.exit(ladder.main(ZLIB))
