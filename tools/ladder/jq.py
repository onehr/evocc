#!/usr/bin/env python3
"""jq's release configuration and original make check, including Oniguruma."""
import importlib.util
from pathlib import Path
import sys

_spec = importlib.util.spec_from_file_location('ladder_subject', Path(__file__).with_name('subject.py'))
ladder = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = ladder
_spec.loader.exec_module(ladder)

JQ = ladder.Subject(
    name='jq', rung='E10', pin='jq-1.8.2', tree='jq-1.8.2',
    url='https://github.com/jqlang/jq/releases/download/jq-1.8.2/jq-1.8.2.tar.gz',
    sha256='71b8d6e8f5fe81f6c6d0d110e3892251f6ce76ed095abd315e26e6e1193af3af',
    configure='./configure CC=gcc-14 --disable-shared --disable-docs --with-oniguruma=builtin LDFLAGS=-no-pie',
    # The release ships src/version.h but not its .remake-version-h stamp. Without the
    # stamp, every later make regenerates the header and recompiles src/main.c with GCC.
    build='make -j2 V=1 && touch -d @0 .remake-version-h', relink='make -j2 V=1', test='make check V=1',
    notes=('Official release, static libjq and bundled Oniguruma; original nine make-check scripts.',
           'Mixed-object coverage includes Oniguruma; FP sources are attempted, not prefiltered.',
           'The build creates the missing .remake-version-h stamp with an old time. Otherwise every '
           'later make regenerates src/version.h and recompiles src/main.c with GCC, which would '
           'replace the substituted object before make check.'),
)

if __name__ == '__main__':
    sys.exit(ladder.main(JQ))
