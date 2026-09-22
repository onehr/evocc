#!/usr/bin/env python3
"""Mbed TLS 4.2.0: unchanged upstream selftest and CTest, mixed-object ramp.

The official release includes TF-PSA-Crypto, the framework and generated
sources. Kitware's portable CMake is mounted beside the source, so the
sealed GCC/Clang image is not modified. This measures the default release
configuration, not ssl-opt.sh/compat.sh or every configuration of all.sh.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import urllib.request

_spec = importlib.util.spec_from_file_location(
    'ladder_subject', Path(__file__).resolve().parent / 'subject.py'
)
ladder = importlib.util.module_from_spec(_spec)
sys.modules['ladder_subject'] = ladder
_spec.loader.exec_module(ladder)

CMAKE_TREE = 'cmake-3.31.8-linux-aarch64'
CMAKE_URL = f'https://github.com/Kitware/CMake/releases/download/v3.31.8/{CMAKE_TREE}.tar.gz'
CMAKE_SHA256 = '609735983e3bdf24b6ab379d918458d64196fe72b98226f62dd5e9fe7b2997cc'
TOOLS = f'/work/mbedtls/{CMAKE_TREE}/bin'
CMAKE = f'{TOOLS}/cmake'
BUILD = f'{CMAKE} --build build --parallel 2 --verbose'

MBEDTLS = ladder.Subject(
    name='mbedtls',
    rung='E4',
    pin='4.2.0 (2026-07-07, github.com/Mbed-TLS/mbedtls)',
    url='https://github.com/Mbed-TLS/mbedtls/releases/download/mbedtls-4.2.0/mbedtls-4.2.0.tar.bz2',
    sha256='2bed9d713b4668f76553b097e72b8aa30bc8f112a940d7ae228d524bbde6ffea',
    tree='mbedtls-4.2.0',
    configure=(f'{CMAKE} -S . -B build -DCMAKE_C_COMPILER=gcc-14 '
               '-DCMAKE_BUILD_TYPE=Release -DENABLE_TESTING=ON -DENABLE_PROGRAMS=ON '
               '-DUSE_SHARED_MBEDTLS_LIBRARY=OFF -DCMAKE_EXE_LINKER_FLAGS=-no-pie'),
    build=BUILD,
    relink=BUILD,
    test=(f'(build/programs/test/selftest && {TOOLS}/ctest --test-dir build '
          '--output-on-failure --no-tests=error)'),
    refresh_generated_inputs=True,
    notes=(
        'official release archive, including TF-PSA-Crypto and framework submodules; no source rewrites',
        'default Release configuration, static libraries and non-PIE test links; '
        'programs/test/selftest plus all registered CTest tests, not ssl-opt.sh/compat.sh/all.sh',
        f'portable CMake 3.31.8 linux-aarch64, archive SHA-256 {CMAKE_SHA256}; '
        'build tool mounted beside source, sealed compiler image unchanged',
        'library, example and generated test translation units are all captured from the verbose build; '
        'a passing mixed build is not a complete EvoCC build',
        'a library relink regenerates test C sources; replaced test objects are reinstalled '
        'only after recompiling the current input to identical EvoCC assembly, then relinked and '
        'checked again before CTest; changed assembly or external assembler inputs stop reuse',
    ),
)


def prepare_cmake() -> None:
    directory = ladder.BUILD / MBEDTLS.name
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory / f'{CMAKE_TREE}.tar.gz'
    if not archive.is_file():
        with urllib.request.urlopen(CMAKE_URL) as response:
            archive.write_bytes(response.read())
    if ladder.sha256_file(archive) != CMAKE_SHA256:
        raise SystemExit('portable CMake archive does not match its release checksum')
    result = ladder.run(['tar', '-xf', str(archive), '-C', str(directory)])
    if result.returncode:
        raise SystemExit(f'cannot unpack portable CMake: {result.stderr}')


if __name__ == '__main__':
    prepare_cmake()
    sys.exit(ladder.main(MBEDTLS))
