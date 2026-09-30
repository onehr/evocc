"""Print a steady plan for review/commit; never starts or overwrites a campaign."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess

from .eqsat_chain import digest, require
from .eqsat_native import host_identity
from .eqsat_steady import (DEFAULT_CONTROL, FILES, PROTOCOL, RESOURCES, admitted_calibration,
                          build_population, check_build_inputs, load, validate_plan)
from .eqsat_twins import canonical


def owned_files(directory, root):
    directory, root = Path(directory).resolve(), Path(root).resolve()
    require(directory.is_relative_to(root), 'prerequisite is outside the repository')
    files = {}
    for p in sorted(directory.rglob('*')):
        require(not p.is_symlink(), 'symlink is not an immutable owned capture')
        if p.is_file():
            require(not p.name.endswith('.pending'), 'prerequisite is still being published')
            files[p.relative_to(root).as_posix()] = digest(p.read_bytes())
    require(files, 'empty prerequisite capture')
    return files


def make_plan(root, mode, source):
    root, source = Path(root).resolve(), Path(source).resolve()
    require(mode in ('build', 'AA', 'AB') and source.is_relative_to(root), 'invalid steady-plan source')
    sm, sr, sa = load(source/'manifest.json'), load(source/'receipt.json'), load(source/'audit.json')
    files = {}
    if mode == 'build':
        require(sr['status'] == 'OBSERVED_DETERMINISTIC_COMPLETION' and sa['status'] ==
                'DETERMINISTIC_CAPTURE_CONSISTENT', 'incomplete/unaudited deterministic preflight')
        tools = {k: sm['tools'][k] for k in ('container', 'collector_python', 'checker', 'historical_checker', 'ir_backend', 'census')}
        # Pin only artifacts the steady composition actually consumes, not all
        # thousands of nonselected policy bundles a second time.
        for name in ('manifest.json', 'receipt.json', 'audit.json'):
            files[(source/name).relative_to(root).as_posix()] = digest((source/name).read_bytes())
        historical_manifest = root/sm['source']['directory']/'manifest.json'
        files[historical_manifest.relative_to(root).as_posix()] = digest(historical_manifest.read_bytes())
        provisional = dict(source=dict(directory=source.relative_to(root).as_posix()), tools=tools)
        programs = build_population(provisional, root)
        for program in programs:
            for name in program['reference_objects']:
                files[name] = digest((root/name).read_bytes())
            for unit in program['units']:
                for variant, relative in unit['chains'].items():
                    chain = root/relative
                    files.update(owned_files(chain, root))
                    for name in unit['objects'][variant].values():
                        files[name] = digest((root/name).read_bytes())
        # Includes are real compiler inputs even though library objects are
        # already frozen. The harness consumes the unchanged public headers.
        from .eqsat_pilot import ROOTS
        for name in ('zlib', 'zstd', 'cjson'):
            for header in sorted((root/ROOTS[name]).rglob('*.h')):
                require(not header.is_symlink(), 'unbound include symlink')
                files[header.relative_to(root).as_posix()] = digest(header.read_bytes())
    else:
        require(sr['status'] == 'OBSERVED_'+('BUILD' if mode == 'AA' else 'AA')
                and sa['status'] == 'STEADY_CAPTURE_CONSISTENT', 'incomplete/unaudited timing prerequisite')
        if mode == 'AB':
            admitted_calibration(load(source/'guest/receipt.json'))
            require(sa['admitted_aa'] is True, 'independent A/A audit did not admit timing')
        tools = sm['tools']
        files.update(sm['source']['files'])
        files.update(owned_files(source, root))
    plan = dict(schema='evocc-r02-steady-plan-v1', mode=mode,
        implementation_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
        source=dict(kind={'build': 'DETERMINISTIC_COMPLETION', 'AA': 'STEADY_BUILD', 'AB': 'ADMITTED_STEADY_AA'}[mode],
            directory=source.relative_to(root).as_posix(), files=files),
        tools=tools, files={p: digest((root/p).read_bytes()) for p in FILES}, image=sm['image'],
        guest_tools=sm['guest_tools'], host=host_identity(), guest_kernel='6.18.15',
        resources=RESOURCES, protocol=PROTOCOL, optimizer=DEFAULT_CONTROL)
    validate_plan(plan, mode)
    if mode == 'build':
        check_build_inputs(plan, root, programs)
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('build', 'AA', 'AB')); parser.add_argument('source', type=Path)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    print(json.dumps(make_plan(args.root, args.mode, args.source), sort_keys=True, indent=2))


if __name__ == '__main__':
    main()
