#!/usr/bin/env python3
"""Build the GCC reference kernel of one solved configuration in the sealed image.

The build uses the frozen build identity of contracts/build-environment.json,
gcc-14 as CC and HOSTCC, and the read-only Linux source. olddefconfig must
leave the solved configuration byte-identical before and after the build;
otherwise the run fails. The output becomes the pristine reference tree of a
T04 composition. A later run continues an interrupted build in the same tree.

Exit 0: the requested steps completed and every identity holds.
Exit 1: the build completed, but its inventory differs from --inventory.
Exit 2: an input, image or build error.
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import time

import layout

ARTIFACTS = ('.config', 'arch/arm64/boot/Image', 'vmlinux', 'System.map', 'Module.symvers', 'modules.order')
# KBUILD_BUILD_VERSION is not in the contract; every retained reference
# build record exports 1.
BUILD_VERSION = '1'


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with Path(path).open('rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def build_identity(environment: dict) -> dict[str, str]:
    values = environment['reproducibility']
    names = ('KBUILD_BUILD_TIMESTAMP', 'SOURCE_DATE_EPOCH', 'KBUILD_BUILD_USER', 'KBUILD_BUILD_HOST',
             'LC_ALL', 'TZ')
    identity = {name: values[name] for name in names}
    identity['KBUILD_BUILD_VERSION'] = BUILD_VERSION
    return identity


def build_script(identity: dict[str, str], umask: str, jobs: int, configure_only: bool) -> str:
    make = 'make -C /work/src O=/work/out ARCH=arm64 CC=gcc-14 HOSTCC=gcc-14'
    exports = ' '.join(f'{name}={shlex.quote(value)}' for name, value in identity.items())
    lines = ['set -eu', f'umask {umask}', f'export {exports}',
             f'{make} olddefconfig', 'cmp /work/out/.config /work/config']
    if not configure_only:
        lines += [f'{make} -j{jobs} Image modules', 'cmp /work/out/.config /work/config']
    return '\n'.join(lines) + '\n'


def container_command(tag: str, source: Path, output: Path, config: Path, jobs: int, memory: str) -> list[str]:
    return [layout.CONTAINER, 'run', '--rm', '-i', '--platform', 'linux/arm64', '--network', 'none',
            '--cpus', str(jobs), '--memory', memory,
            '-v', f'{source}:/work/src:ro', '-v', f'{output}:/work/out', '-v', f'{config}:/work/config:ro',
            '-w', '/work/out', tag, 'sh', '-e', '-s']


def prepare_output(output: Path, config: Path) -> str:
    """Start an empty tree, or continue one that holds the same configuration."""
    if output.exists() and any(output.iterdir()):
        existing = output / '.config'
        if not existing.is_file() or existing.read_bytes() != config.read_bytes():
            raise ValueError(f'{output} holds another tree; use an empty or absent directory')
        return 'continue'
    output.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(config, output / '.config')
    return 'new'


def check_source(source: Path) -> None:
    if not (source / 'Makefile').is_file() or not (source / 'scripts/kconfig').is_dir():
        raise ValueError(f'not a Linux source tree: {source}')
    if (source / '.config').exists() or (source / 'include/config').exists():
        raise ValueError(f'the source tree is configured; Kbuild needs a clean tree: {source}')


def inventory(tree: Path, capture: Path) -> dict:
    """Runbook check V7: the same target-C units and modules as a retained capture."""
    import substitute

    units = sorted(unit for unit, _ in substitute.target_c_records(tree))
    retained = sorted(json.loads(line)['unit'] for line in (capture / 'units.jsonl').read_text().splitlines())
    order = tree / 'modules.order'
    modules = sorted(line[:-2] + '.ko' for line in order.read_text().splitlines()) if order.is_file() else []
    linked = json.loads((capture / 'module-link.json').read_text())
    retained_modules = sorted(row['unit'] for row in linked['modules'])
    return {'capture': str(capture), 'units': len(units), 'retainedUnits': len(retained),
            'unitsEqual': units == retained,
            'missingUnits': sorted(set(retained) - set(units)), 'extraUnits': sorted(set(units) - set(retained)),
            'modules': len(modules), 'retainedModules': len(retained_modules),
            'modulesEqual': modules == retained_modules,
            'missingModules': sorted(set(retained_modules) - set(modules)),
            'extraModules': sorted(set(modules) - set(retained_modules))}


def artifacts(tree: Path) -> dict:
    result = {name: sha256(tree / name) for name in ARTIFACTS if (tree / name).is_file()}
    release = tree / 'include/config/kernel.release'
    if release.is_file():
        result['kernelRelease'] = release.read_text().strip()
    order = tree / 'modules.order'
    if order.is_file():
        result['moduleCount'] = len(order.read_text().splitlines())
    return result


def parser():
    result = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    result.add_argument('--config', type=Path, required=True, help='solved configuration, for example c6.config')
    result.add_argument('--output', type=Path, required=True, help='reference tree, for example .build/case/final-s/pristine')
    result.add_argument('--source-root', type=Path, default=layout.SOURCE,
                        help='clean case-sensitive Linux source, mounted read-only')
    result.add_argument('--jobs', type=int, default=4, help='make jobs and container CPUs')
    result.add_argument('--memory', default='10G', help='container memory')
    result.add_argument('--configure-only', action='store_true', help='olddefconfig and the comparison only; no build')
    result.add_argument('--inventory', type=Path,
                        help='retained capture with units.jsonl and module-link.json to compare after the build')
    result.add_argument('--record', type=Path, help='record path; default <output>-reference.json beside the tree')
    result.add_argument('--dry-run', action='store_true', help='print the container command and script only')
    return result


def main(argv=None) -> int:
    args = parser().parse_args(argv)
    config, source, output = args.config.resolve(), args.source_root.resolve(), args.output.resolve()
    record_path = (args.record or output.with_name(output.name + '-reference.json')).resolve()
    try:
        if args.jobs < 1:
            raise ValueError('--jobs must be positive')
        if args.inventory and args.configure_only:
            raise ValueError('--inventory needs a completed build, not --configure-only')
        if not config.is_file():
            raise ValueError(f'missing configuration: {config}')
        check_source(source)
        environment = json.loads(layout.BUILD_ENVIRONMENT.read_text())
        tag, digest = layout.sealed_image(environment)
        identity = build_identity(environment)
        script = build_script(identity, environment['reproducibility']['umask'], args.jobs, args.configure_only)
        command = container_command(tag, source, output, config, args.jobs, args.memory)
        if args.dry_run:
            print('+ ' + shlex.join(command))
            print(script, end='')
            return 0
        layout.require_attached(output)
        layout.require_attached(record_path)
        layout.verify_sealed_image(environment)
        state = prepare_output(output, config)
    except (OSError, KeyError, ValueError, subprocess.SubprocessError) as error:
        print(f'reference: {error}', file=sys.stderr)
        return 2
    record = {'schemaVersion': 1, 'tool': 'tools/k1/reference.py', 'toolSha256': sha256(Path(__file__)),
              'startedAt': datetime.now(timezone.utc).isoformat(), 'image': {'tag': tag, 'digest': digest},
              'source': str(source), 'config': str(config), 'configSha256': sha256(config),
              'output': str(output), 'outputState': state, 'buildIdentity': identity,
              'configureOnly': args.configure_only, 'jobs': args.jobs, 'command': command, 'script': script}
    started = time.monotonic()
    log = record_path.with_suffix('.log')
    record_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with log.open('w') as handle:
            process = subprocess.run(command, input=script, text=True, stdout=handle,
                                     stderr=subprocess.STDOUT, timeout=6 * 3600)
        record['exitCode'] = process.returncode
    except (OSError, subprocess.SubprocessError) as error:
        record.update(exitCode=None, error=str(error))
    record.update(seconds=round(time.monotonic() - started, 1), log=str(log),
                  artifacts=artifacts(output))
    record['configUnchanged'] = record['artifacts'].get('.config') == record['configSha256']
    status = 'PASS' if record['exitCode'] == 0 and record['configUnchanged'] else 'ERROR'
    if status == 'PASS' and args.inventory:
        try:
            record['inventory'] = inventory(output, args.inventory.resolve())
            if not (record['inventory']['unitsEqual'] and record['inventory']['modulesEqual']):
                status = 'DIFFERENT_INVENTORY'
        except (OSError, KeyError, ValueError) as error:
            record['inventoryError'] = str(error)
            status = 'ERROR'
    record['status'] = status
    record_path.write_text(json.dumps(record, indent=2) + '\n')
    print(f"reference {status}: exit {record['exitCode']}; config unchanged {record['configUnchanged']}; "
          f"record {record_path}")
    return {'PASS': 0, 'DIFFERENT_INVENTORY': 1}.get(status, 2)


if __name__ == '__main__':
    raise SystemExit(main())
