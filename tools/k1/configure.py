#!/usr/bin/env python3
"""Solve the recorded acceptance kernel configurations in the sealed image.

Each step merges ordered fragments onto the configuration of the previous
step with the upstream merge_config.sh. Then olddefconfig resolves the
result. The fragments, their order and the merge forms reproduce the
recorded identities C1, C2, C3, C5 and C6 of the acceptance runbook of
2026-10-13 (section 3.1). C2 is the final KUnit configuration. C6 is the
final selftest/BPF configuration. The Linux source is mounted read-only.

Exit 0: every configuration has its recorded identity.
Exit 1: a configuration differs. configure.json names each difference.
Exit 2: an input, image or container error.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

import layout

SELFTESTS = 'tools/testing/selftests'
MERGE = '/work/src/scripts/kconfig/merge_config.sh'
# The requests of the base KUnit configuration: config.requested in
# tests/contracts/kernel-kunit-receipt.json. Its command was not recorded.
# merge_config.sh gives the recorded identity with and without -y.
KUNIT_REQUESTS = ('CONFIG_KUNIT=y', 'CONFIG_KUNIT_ALL_TESTS=y', 'CONFIG_BLK_DEV_UBLK=y')
# The three changes of tests/fixtures/kernel-connector-fpu-preflight.out.
CONNECTOR_FPU_REQUESTS = ('CONFIG_CONNECTOR=y', 'CONFIG_PROC_EVENTS=y', 'CONFIG_TEST_FPU=m')
REQUESTS = {'kunit-base.config': KUNIT_REQUESTS, 'connector-fpu.config': CONNECTOR_FPU_REQUESTS}


def upstream(*collections):
    return tuple(('upstream', name) for name in collections)


STEPS = (
    {'name': 'c1', 'base': 'c0', 'yes': True, 'fragments': (('request', 'kunit-base.config'),),
     'sha256': '6afbd2125fb1d182b77350077bc888f87323f4cd0de0d4da618cfa5afd159c23',
     'record': 'config in tests/contracts/kernel-kunit-receipt.json'},
    # The exact command of .evidence/kunit/coverage-config-20260914/reference-network.json.
    {'name': 'c2', 'base': 'c1', 'yes': True,
     'fragments': upstream('net/af_unix', 'net/can', 'net', 'net/forwarding', 'net/hsr',
                           'net/mptcp', 'net/netfilter', 'net/openvswitch', 'net/ovpn',
                           'net/packetdrill', 'net/tcp_ao', 'mm')
     + (('overlay', 'kernel-selftest-coverage.config'),),
     'sha256': '9e28277bea95477ff8ccd9a88809ef18368ecc9db57552b16fa06f9aa056def0',
     'record': 'configSha256 in tests/contracts/kernel-expanded-kunit-contract.json'},
    {'name': 'c3', 'base': 'c2', 'yes': True,
     'fragments': upstream('bpf', 'hid', 'sched_ext') + (('overlay', 'kernel-bpf-coverage.config'),),
     'sha256': '2f938cda8169ed22e90e46b4e43a9b3752daa6c457497fb13e14a3d66abdfa28',
     'record': 'configurationAudit in .evidence/kselftest-full/20260922T033617Z/summary.json'},
    # The merge record was deleted. Without -y, a later module request wins
    # over an earlier built-in request. net/config follows the other
    # collections in name order. This form matches the retained C5 audit for
    # every requested option, and the recorded SHA-256.
    {'name': 'c5', 'base': 'c3', 'yes': False,
     'fragments': upstream('damon', 'dmabuf-heaps', 'drivers/net', 'drivers/net/bonding',
                           'drivers/net/team', 'drivers/net/virtio_net', 'filesystems/binderfs',
                           'firmware', 'ftrace', 'gpio', 'iommu', 'ipc', 'kvm', 'lib', 'livepatch',
                           'memory-hotplug', 'mm', 'nci', 'net', 'pstore', 'splice', 'static_keys',
                           'sync', 'sysctl', 'tc-testing', 'user_events', 'zram')
     + (('overlay', 'kernel-bpf-coverage.config'), ('overlay', 'kernel-selftest-runtime.config')),
     'sha256': 'f2a06079b52ed50a40416b470417ffdbc104696c06eaac4ef9f62621e4f1ecbd',
     'record': 'configurationAudit in .evidence/kselftest-full/20260927T223104Z/summary.json'},
    {'name': 'c6', 'base': 'c5', 'yes': True, 'fragments': (('request', 'connector-fpu.config'),),
     'sha256': '95853321d8dd6ae0b2d779abc094659a327936cb557244bed45b484ccf4c0e09',
     'record': 'tests/fixtures/kernel-connector-fpu-preflight.out'},
)
LINES = {'kunit': 'c2', 'selftest': 'c6'}
CONNECTOR_FPU_CHANGES = {'CONFIG_CONNECTOR': ['n', 'y'], 'CONFIG_PROC_EVENTS': ['n', 'y'],
                         'CONFIG_TEST_FPU': ['n', 'm']}


def sha256(path: Path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def fragment_path(kind: str, name: str) -> str:
    """The container path of one fragment."""
    if kind == 'upstream':
        return f'/work/src/{SELFTESTS}/{name}/config'
    if kind == 'overlay':
        return f'/work/configs/{name}'
    if kind == 'request':
        return f'/work/cap/{name}'
    raise ValueError(f'unknown fragment kind: {kind}')


def host_fragment(kind: str, name: str, source: Path, output: Path) -> Path:
    if kind == 'upstream':
        return source / SELFTESTS / name / 'config'
    if kind == 'overlay':
        return layout.ROOT / 'tests/configs' / name
    return output / name


def solve_script(steps=None) -> str:
    """One shell script: each step writes its configuration and two logs."""
    steps = STEPS if steps is None else steps
    lines = ['set -eu', 'umask 022', 'export LC_ALL=C TZ=UTC']
    for step in steps:
        name = step['name']
        base = '/work/base.config' if step['base'] == 'c0' else f"/work/cap/{step['base']}.config"
        out = f'/tmp/{name}'
        form = ['-y', '-m'] if step['yes'] else ['-m']
        merge = [MERGE, *form, '-O', out, base, *(fragment_path(*item) for item in step['fragments'])]
        lines += [f'rm -rf {out}', f'mkdir -p {out}',
                  f'{shlex.join(merge)} > /work/cap/{name}.merge.log 2>&1',
                  f'make -C /work/src O={out} ARCH=arm64 CC=gcc-14 olddefconfig'
                  f' > /work/cap/{name}.olddefconfig.log 2>&1',
                  f'cp {out}/.config /work/cap/{name}.config']
    names = ' '.join(step['name'] + '.config' for step in steps)
    lines += ['cd /work/cap', f'sha256sum {names} > SHA256SUMS']
    if {'c5', 'c6'} <= {step['name'] for step in steps}:
        lines.append('/work/src/scripts/diffconfig c5.config c6.config > c5-c6.diff')
    return '\n'.join(lines) + '\n'


def container_command(tag: str, source: Path, base: Path, output: Path) -> list[str]:
    return [layout.CONTAINER, 'run', '--rm', '-i', '--platform', 'linux/arm64', '--network', 'none',
            '--cpus', '2', '--memory', '4G',
            '-v', f'{source}:/work/src:ro', '-v', f'{layout.ROOT / "tests/configs"}:/work/configs:ro',
            '-v', f'{base}:/work/base.config:ro', '-v', f'{output}:/work/cap',
            tag, 'sh', '-e', '-s']


def effective(path: Path) -> dict[str, str]:
    """Set options only; an absent or unset option is n."""
    result = {}
    for line in Path(path).read_text().splitlines():
        match = re.fullmatch(r'(CONFIG_\w+)=(.*)', line.strip())
        if match:
            result[match[1]] = match[2]
    return result


def changes(before: Path, after: Path) -> dict[str, list[str]]:
    old, new = effective(before), effective(after)
    return {name: [old.get(name, 'n'), new.get(name, 'n')]
            for name in sorted(old.keys() | new.keys()) if old.get(name, 'n') != new.get(name, 'n')}


def base_config() -> tuple[Path, str]:
    """C0: the frozen GCC virtconfig that contracts/kernel-target.json pins."""
    lane = json.loads(layout.KERNEL_TARGET.read_text())['config']['lanes']['gcc']
    return layout.ROOT / lane['record'], lane['rawSha256']


def check_inputs(source: Path, output: Path) -> dict:
    """Hash every input before the solver reads it."""
    if not (source / 'scripts/kconfig/merge_config.sh').is_file():
        raise ValueError(f'not a Linux source tree: {source}')
    if (source / '.config').exists() or (source / 'include/config').exists():
        raise ValueError(f'the source tree is configured; Kbuild needs a clean tree: {source}')
    base, recorded = base_config()
    if sha256(base) != recorded:
        raise ValueError(f'base configuration differs from contracts/kernel-target.json: {base}')
    fragments = {}
    for step in STEPS:
        for kind, name in step['fragments']:
            if kind == 'request':
                continue
            path = host_fragment(kind, name, source, output)
            if not path.is_file():
                raise ValueError(f'missing configuration fragment: {path}')
            fragments[fragment_path(kind, name)] = sha256(path)
    return {'base': {'path': str(base.relative_to(layout.ROOT)), 'sha256': recorded},
            'fragments': fragments}


def evaluate(output: Path, steps=None) -> dict:
    """Compare each solved configuration with its recorded identity."""
    steps = STEPS if steps is None else steps
    rows = []
    for step in steps:
        path = output / (step['name'] + '.config')
        actual = sha256(path) if path.is_file() else None
        rows.append({'name': step['name'], 'base': step['base'],
                     'mergeForm': ['-y', '-m'] if step['yes'] else ['-m'],
                     'fragments': [fragment_path(*item) for item in step['fragments']],
                     'sha256': actual, 'recordedSha256': step['sha256'], 'recordedBy': step['record'],
                     'matches': actual == step['sha256']})
    result = {'steps': rows}
    if (output / 'c5.config').is_file() and (output / 'c6.config').is_file():
        found = changes(output / 'c5.config', output / 'c6.config')
        result['connectorFpuChanges'] = found
        result['connectorFpuExact'] = found == CONNECTOR_FPU_CHANGES
    matched = {row['name']: row['matches'] for row in rows}
    result['lines'] = {line: {'config': name + '.config', 'matches': matched.get(name, False)}
                       for line, name in LINES.items()}
    exact = all(matched.values()) and result.get('connectorFpuExact', True)
    result['status'] = 'REPRODUCED' if exact else 'DIFFERENT'
    return result


def parser():
    result = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    result.add_argument('--output', type=Path, required=True,
                        help='new directory for configurations, logs and configure.json')
    result.add_argument('--source-root', type=Path, default=layout.SOURCE,
                        help='clean case-sensitive Linux source, mounted read-only')
    result.add_argument('--dry-run', action='store_true', help='print the container command and script only')
    return result


def main(argv=None) -> int:
    args = parser().parse_args(argv)
    source, output = args.source_root.resolve(), args.output.resolve()
    try:
        inputs = check_inputs(source, output)
        environment = json.loads(layout.BUILD_ENVIRONMENT.read_text())
        tag, digest = layout.sealed_image(environment)
        base = layout.ROOT / inputs['base']['path']
        command = container_command(tag, source, base, output)
        script = solve_script()
        if args.dry_run:
            print('+ ' + shlex.join(command))
            print(script, end='')
            return 0
        if (output / 'configure.json').exists():
            raise ValueError(f'{output} already holds a solved record; choose a new --output')
        layout.verify_sealed_image(environment)
        output.mkdir(parents=True, exist_ok=True)
    except (OSError, KeyError, ValueError, subprocess.SubprocessError) as error:
        print(f'configure: {error}', file=sys.stderr)
        return 2
    for name, requests in REQUESTS.items():
        (output / name).write_text(''.join(line + '\n' for line in requests))
    (output / 'solve.sh').write_text(script)
    started = time.monotonic()
    record = {'schemaVersion': 1, 'tool': 'tools/k1/configure.py',
              'toolSha256': sha256(Path(__file__)), 'startedAt': datetime.now(timezone.utc).isoformat(),
              'image': {'tag': tag, 'digest': digest}, 'source': str(source), **inputs,
              'requests': {name: sha256(output / name) for name in REQUESTS},
              'script': 'solve.sh', 'scriptSha256': sha256(output / 'solve.sh'), 'command': command}
    try:
        with (output / 'configure.log').open('w') as log:
            process = subprocess.run(command, input=script, text=True, stdout=log,
                                     stderr=subprocess.STDOUT, timeout=1800)
        record['exitCode'] = process.returncode
    except (OSError, subprocess.SubprocessError) as error:
        record.update(exitCode=None, error=str(error))
    record['seconds'] = round(time.monotonic() - started, 1)
    record.update(evaluate(output))
    if record['exitCode'] != 0:
        record['status'] = 'ERROR'
    (output / 'configure.json').write_text(json.dumps(record, indent=2) + '\n')
    for row in record['steps']:
        state = 'recorded identity' if row['matches'] else f"DIFFERENT, recorded {row['recordedSha256']}"
        print(f"{row['name']}: {row['sha256']} ({state})")
    if 'connectorFpuExact' in record:
        print(f"c5 -> c6: {len(record['connectorFpuChanges'])} effective changes; "
              f"exact={record['connectorFpuExact']}")
    print(f"configure {record['status']}: {output / 'configure.json'}")
    return {'REPRODUCED': 0, 'DIFFERENT': 1}.get(record['status'], 2)


if __name__ == '__main__':
    raise SystemExit(main())
