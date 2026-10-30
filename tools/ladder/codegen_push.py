"""N05 pre-push orchestration over the existing bench adapter and shared target.

Missing historical measurements are reported, not made up. Only the current
clean checkout can be built automatically; this command never changes revision.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from .codegen_trend import compare, coverage, read_observations
from .eqsat_chain import digest, require


ROOT = Path(__file__).resolve().parents[2]


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def pushed_commits(root, lines):
    """First-parent implementation commits since the N05 plan, plus each tip."""
    admitted = git(root, 'log', '--diff-filter=A', '--format=%H', '--',
                   'tests/contracts/codegen-n05-20261002.json').splitlines()
    require(len(admitted) == 1, 'cannot find the frozen N05 starting revision')
    result = []
    for line in lines:
        fields = line.split()
        require(len(fields) == 4, 'malformed pre-push update')
        _, local, _, remote = fields
        if local == '0' * 40:
            continue
        tip = git(root, 'rev-parse', '--verify', local + '^{commit}')
        history = git(root, 'rev-list', '--first-parent', tip).splitlines()
        require(admitted[0] in history, 'pushed revision predates the N05 measurement contract')
        end = history.index(remote) if remote in history else history.index(admitted[0])
        for commit in reversed(history[:end]):
            paths = git(root, 'diff-tree', '--no-commit-id', '--name-only', '-r', commit).splitlines()
            changed = any(path.startswith(('crates/', 'targets/')) or path in
                          {'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo/config.toml'} for path in paths)
            if (changed or commit == tip) and commit not in result:
                result.append(commit)
    return result


def missing_measurements(commits, records, plan_hash, variant):
    return [commit for commit in commits if not any(row['source_commit'] == commit and
            row['plan_sha256'] == plan_hash and row['variant'] == variant and
            coverage(row)['status'] == 'OBSERVED' for row in records)]


def run(root, plan, updates, *, execute=True):
    root, plan = Path(root).resolve(), Path(plan).resolve()
    raw = plan.read_bytes()
    config = json.loads(raw)
    require(config['schema'] == 'evocc-codegen-measure-plan-v1', 'not an N05 live measurement plan')
    commits = pushed_commits(root, updates)
    history = root / config['history']
    records = read_observations(history)
    missing = missing_measurements(commits, records, digest(raw), config['variant'])
    head = git(root, 'rev-parse', 'HEAD')
    if execute and head in missing:
        require(shutil.disk_usage(root).free >= 25 * 1024**3, 'pre-push build requires 25 GiB free')
        require(not git(root, 'status', '--porcelain'), 'pre-push measurement requires a clean worktree')
        env = dict(os.environ, CARGO_TARGET_DIR='target', CARGO_BUILD_JOBS='2')
        # Cargo's own target lock serializes this single invocation; the bench
        # adapter additionally serializes the VM lane using .build/bench.lock.
        subprocess.run(['cargo', 'build', '--release', '--offline', '-p', 'evocc', '-p', 'evocc-proofcheck',
                        '-p', 'evocc-passes', '--bins', '--example', 'corpus'], cwd=root, env=env, check=True)
        subprocess.run([sys.executable, 'tools/bench.py', '--codegen-measure', str(plan)],
                       cwd=root, env=env, check=True)
        records = read_observations(history)
        missing = missing_measurements(commits, records, digest(raw), config['variant'])
    comparisons = []
    for commit in commits:
        if commit in missing:
            continue
        candidates = [r for r in records if r['source_commit'] == commit and
                      r['plan_sha256'] == digest(raw) and r['variant'] == config['variant'] and
                      coverage(r)['status'] == 'OBSERVED']
        current = candidates[0]
        for retry in candidates[1:]:
            def sizes(row):
                return [{k: v for k, v in unit.items() if k != 'compile_wall_ns'} for unit in row['units']]
            require(sizes(retry) == sizes(current) and retry['steps'] == current['steps'],
                    'inconsistent repeated push measurements; select an explicit disposition')
        ancestors = git(root, 'rev-list', '--first-parent', commit).splitlines()[1:]
        prior = next((r for parent in ancestors for r in records if r['source_commit'] == parent and
                      r['plan_sha256'] == digest(raw) and r['variant'] == config['variant'] and
                      coverage(r)['status'] == 'OBSERVED'), None)
        if prior is not None:
            comparisons.extend(dict(commit=commit, capture_sha256=candidate['capture_sha256'],
                                    result=compare(prior, candidate)) for candidate in candidates)
    blocked = any(row['result']['status'] not in {'NO_REGRESSION'} for row in comparisons)
    return dict(status='NOT_RUN' if missing else 'REGRESSION' if blocked else 'OBSERVED',
                requested_commits=commits, missing_commits=missing, comparisons=comparisons,
                note='Missing historical revisions require the same bench plan in an exclusive clean checkout; no checkout was changed.')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--plan', type=Path)
    parser.add_argument('--no-run', action='store_true', help='read-only admission preview')
    args = parser.parse_args(argv)
    try:
        plan = args.plan
        if plan is None:
            plan = args.root / git(args.root, 'config', '--get', 'evocc.codegenPlan')
        result = run(args.root, plan, sys.stdin.read().splitlines(), execute=not args.no_run)
    except (KeyError, ValueError, OSError, subprocess.SubprocessError) as error:
        print('codegen pre-push: ' + str(error), file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result['status'] == 'OBSERVED' else 1


if __name__ == '__main__':
    raise SystemExit(main())
