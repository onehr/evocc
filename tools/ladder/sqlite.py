#!/usr/bin/env python3
"""SQLite amalgamation plus its public Tcl veryquick suite, not TH3."""
import importlib.util
from pathlib import Path
import shutil
import sys

_spec = importlib.util.spec_from_file_location('ladder_subject', Path(__file__).with_name('subject.py'))
ladder = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = ladder
_spec.loader.exec_module(ladder)

TCL_URL = 'https://downloads.sourceforge.net/project/tcl/Tcl/8.6.16/tcl8.6.16-src.tar.gz'
TCL_SHA256 = '91cb8fa61771c63c262efb553059b7c7ad6757afa5857af6265e4b0bdc2a14a5'
SQLITE = ladder.Subject(
    name='sqlite', rung='E12', pin='3.53.4 public source + testfixture',
    tree='sqlite-src-3530400', url='https://www.sqlite.org/2026/sqlite-src-3530400.zip',
    sha256='d18fa15aec74d8c17e1463f861095adc01b5ad190256acb4f91d22f0368d232b',
    configure=('./configure CC=gcc-14 --with-tcl=/work/sqlite/tcl-install/lib '
               '--disable-shared LDFLAGS=-no-pie'),
    build=('make -j2 testfixture '
           "'T.link.tcl=. ./.tclenv.sh; python3 /work/sqlite/sqlite_fixture.py $(T.link)'"),
    relink='python3 /work/sqlite/sqlite_fixture.py --relink',
    test=('(cp -a . /tmp/sqlite-test && chown -R nobody:nogroup /tmp/sqlite-test && '
          'cd /tmp/sqlite-test && runuser -u nobody -- '
          './testfixture test/testrunner.tcl --jobs 2 veryquick; '
          'runner_status=$?; python3 /work/sqlite/sqlite_fixture.py '
          '--test-verdict /tmp/sqlite-test/testrunner.db "$runner_status")'),
    test_objects=('fixture-objects/085-sqlite3.o',),
    notes=(
        'Official canonical source of the roadmap release; the autoconf archive omits the Tcl tests.',
        'Public veryquick permutation through upstream testrunner.tcl, not TH3 or the multi-configuration release suite. '
        'It isolates test files; the serial veryquick.test left symlink.test directory x behind, '
        'changing zipfile-25.0 from cannot-open to fread error even in the GCC baseline.',
        'Permission tests run as nobody on a fresh container-local copy of the installed tree; '
        'the upstream suite explicitly refuses root and no permission checks are disabled.',
        'The test verdict also reads upstream testrunner.db: every job must finish done, '
        'with zero errors and a nonzero assertion count; the runner alone may exit zero on failure.',
        'The unchanged upstream testfixture command is split into one compile per C file and one link; '
        'all flags and sources are preserved. Relinks consume installed objects, not fresh GCC source compilations.',
        f'Local static Tcl 8.6.16 harness dependency, source SHA-256 {TCL_SHA256}; not part of EvoCC coverage.',
    ),
)


def prepare_tcl():
    directory = ladder.BUILD / 'sqlite'
    directory.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(Path(__file__).with_name('sqlite_fixture.py'), directory / 'sqlite_fixture.py')
    if (directory / 'tcl-install/bin/tclsh8.6').is_file():
        return
    source = ladder.dataclasses.replace(SQLITE, tree='tcl8.6.16', url=TCL_URL, sha256=TCL_SHA256)
    ladder.unpack(source)
    result = ladder.container(directory, 'sqlite',
        'cd /work/sqlite/tcl8.6.16/unix && '
        './configure --prefix=/work/sqlite/tcl-install --disable-shared CC=gcc-14 && '
        'make -j2 binaries libraries && make install-binaries install-libraries', timeout=900)
    if result.returncode:
        raise SystemExit(f'Tcl harness build failed: {result.stderr[-2000:]}')


if __name__ == '__main__':
    prepare_tcl()
    sys.exit(ladder.main(SQLITE))
