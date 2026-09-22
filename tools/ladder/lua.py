#!/usr/bin/env python3
"""Lua 5.5.1 with its matching public test suite (no _U/soft skips)."""
import importlib.util
from pathlib import Path
import sys

_spec = importlib.util.spec_from_file_location('ladder_subject', Path(__file__).with_name('subject.py'))
ladder = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = ladder
_spec.loader.exec_module(ladder)

TEST_URL = 'https://www.lua.org/tests/lua-5.5.1-tests.tar.gz'
TEST_SHA256 = 'da07b543872dc0bb2ff12aabd0c248578d78df3eb6b67efdc537a46d455c7f31'
TEST_TREE = 'lua-5.5.1-tests'
BUILD = 'make linux CC="gcc-14 -std=gnu99" MYLDFLAGS=-no-pie -j2'
LUA = ladder.Subject(
    name='lua', rung='E11', pin='5.5.1', tree='lua-5.5.1',
    url='https://www.lua.org/ftp/lua-5.5.1.tar.gz',
    sha256='1c4b4068d67061f2a2231ad2b5422e77acea1487ea9890f6320af614f4373dce',
    build=(f'({BUILD} && make -C ../{TEST_TREE}/libs CC=gcc-14 '
           'LUA_DIR=../../lua-5.5.1/src)'),
    relink=BUILD,
    test=('(mkdir -p /tmp/lua-readline && '
          'ln -sf /lib/aarch64-linux-gnu/libreadline.so.8 /tmp/lua-readline/libreadline.so && '
          'export LD_LIBRARY_PATH=/tmp/lua-readline; '
          f'cd ../{TEST_TREE} && : | ../lua-5.5.1/src/lua all.lua > ../lua-tests.out 2>&1; '
          's=$?; cat ../lua-tests.out; test "$s" = 0 && grep -q "final OK" ../lua-tests.out)'),
    notes=('Unmodified public all.lua, no _U, _soft or _port overrides; optional internal T module not built.',
           'Expose the sealed runtime libreadline.so.8 under its development dlopen name libreadline.so; '
           'otherwise the upstream -E REPL check fails even with GCC.',
           'Standard input is an empty pipe, not the container default /dev/null: '
           'the public files.lua test requires stdin to reject seeking.',
           f'Matching official tests archive SHA-256 {TEST_SHA256}.',
           'The five dlopen test helper libraries remain reference-built; coverage counts the Lua source tree only.'),
)


def prepare_tests():
    subject = ladder.dataclasses.replace(LUA, url=TEST_URL, sha256=TEST_SHA256, tree=TEST_TREE)
    tree = ladder.unpack(subject)
    # all.lua/attrib.lua locate the selected executable by ../lua, and
    # upstream libs/P1 is intentionally an empty search directory.
    executable = tree.parent / 'lua'
    if not executable.exists():
        executable.symlink_to('lua-5.5.1/src/lua')


if __name__ == '__main__':
    prepare_tests()
    sys.exit(ladder.main(LUA))
