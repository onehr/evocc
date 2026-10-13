"""Default locations of the final acceptance campaigns.

The acceptance runbook of 2026-10-13 (section 4.5) names these paths. The
audited Linux source, the frozen base configuration and the reference
gen_init_cpio are existing inputs. The other paths are outputs of the
configuration, reference build and payload steps; the tools in this
directory create them. No default names a tree that the October 7
retirement deleted, or the case-damaged .build/linux-src.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
CASE_VOLUME = ROOT / '.build/case'
# Linux 6.18.44 on the case-sensitive volume. Its 91,117 regular files match
# the release archive (sourceAudit in tests/contracts/kernel-direct-virt-receipt.json).
SOURCE = CASE_VOLUME / 'kbuild-probe/src'
SELFTEST_LINE = CASE_VOLUME / 'final-s'
KUNIT_LINE = CASE_VOLUME / 'final-k'
SELFTEST_WORK = SELFTEST_LINE / 'selftests'
SELFTEST_HEADERS = SELFTEST_LINE / 'headers'
KUNIT_RUNTIME = KUNIT_LINE / 'kunit-runtime'
KERNEL_TARGET = ROOT / 'contracts/kernel-target.json'
BUILD_ENVIRONMENT = ROOT / 'contracts/build-environment.json'
GEN_INIT_CPIO = ROOT / '.build/reference/gcc-lane-1/usr/gen_init_cpio'
CONTAINER = '/usr/local/bin/container'


def require_attached(path: Path, volume: Path = CASE_VOLUME) -> None:
    """Refuse an output on the case volume while the volume is detached.

    A detached volume leaves an empty mount point on the host volume. A tree
    of several GiB written there would fill the host disk instead.
    """
    if Path(os.path.abspath(path)).is_relative_to(os.path.abspath(volume)) and not os.path.ismount(volume):
        raise ValueError(f'{volume} is not attached; refusing to write {path} on the host volume')


def sealed_image(environment: dict) -> tuple[str, str]:
    """Return the sealed image tag and the digest that it must resolve to."""
    image = environment['buildImage']
    return image['sealedImageTag'], image['executionReference']


def verify_sealed_image(environment: dict) -> tuple[str, str]:
    """Refuse a tag that no longer resolves to the frozen build image."""
    tag, digest = sealed_image(environment)
    installed = json.loads(subprocess.check_output([CONTAINER, 'image', 'inspect', tag], text=True))
    if installed[0]['configuration']['descriptor']['digest'] != digest:
        raise ValueError(f'{tag} differs from the frozen build environment')
    return tag, digest
