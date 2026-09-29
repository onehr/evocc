"""R02a frozen-plan boundary. This validates plans, never grants measurement PASS.

V1 freezes native checked A/A calibration inputs. Runtime controls, actual object
coverage, fresh result checks and codegen/chain binding still belong to the
execution adapter. A schema-valid or hash-matching plan cannot substitute for
those gates. No builders, workloads, model calls or receipt writers run here.
"""
from __future__ import annotations

import json
from pathlib import Path
import re
import subprocess

from .eqsat_chain import digest, keys, natural, require, sha256_identity, unique
from .eqsat_census import check_source_revision


def strings(values, *, empty=False):
    require(type(values) is list and (empty or values), 'missing string list')
    require(all(type(v) is str and v and '\0' not in v for v in values), 'invalid string list')


def artifact(item, external_tool=False):
    keys(item, 'path sha256')
    require(type(item['path']) is str and item['path'], 'missing artifact path')
    path = Path(item['path'])
    require((external_tool or not path.is_absolute()) and path.parts and '..' not in path.parts
            and path.as_posix() == item['path'], 'unscoped artifact path')
    sha256_identity(item['sha256'])


def artifacts(m):
    for item in m['tools'].values():
        yield item, True
    for item in m['specs'].values():
        yield item, False
    for name, identity in m.get('native', {}).get('collector_files', {}).items():
        yield dict(path=name, sha256=identity), False
    for item in m.get('native', {}).get('sdk', {}).values():
        yield item, True
    for case in m['cases']:
        yield case['source'], False
        for item in case['inputs']:
            yield item, False


def validate_manifest(m):
    v2 = m.get('schema') == 'evocc-performance-plan-v2'
    keys(m, 'schema scope implementation_commit tools specs host environment controls protocol resources cases'
         + (' native' if v2 else ''))
    require(m['schema'] in ('evocc-performance-plan-v1', 'evocc-performance-plan-v2')
            and m['scope'] == 'r02-plan-not-admission', 'unknown plan schema or claim scope')
    require(type(m['implementation_commit']) is str
            and re.fullmatch('[0-9a-f]{40}', m['implementation_commit']), 'missing implementation commit')
    sdk = v2 and type(m.get('native')) is dict and m['native'].get('adapter') == 'darwin-p0-aa-v2'
    keys(m['tools'], 'compiler checker reference assembler linker' + (' ir_backend collector_python' if v2 else '')
         + (' link_editor' if sdk else ''))
    if v2:
        keys(m['native'], 'adapter eqsat codegen_target source_profile collector_files' + (' sdk' if sdk else ''))
        require({k: v for k, v in m['native'].items() if k not in ('collector_files', 'sdk')} ==
                dict(adapter='darwin-p0-aa-v2' if sdk else 'darwin-p0-aa-v1', eqsat='off', codegen_target='darwin-arm64',
                     source_profile='linux-lp64-header-free-integer'),
                'unknown native recipe or source/backend boundary')
        keys(m['native']['collector_files'], 'tools/bench.py tools/ladder/eqsat_native.py '
             'tools/ladder/eqsat_manifest.py tools/ladder/eqsat_census.py tools/ladder/eqsat_chain.py')
        if sdk:
            keys(m['native']['sdk'], 'settings system_library')
            for item in m['native']['sdk'].values():
                artifact(item, True)
                require(Path(item['path']).is_absolute(), 'SDK artifact needs an absolute path')
    keys(m['specs'], 'evoir evomir')
    h = m['host']
    keys(h, 'system release machine runtime cpu libc affinity')
    require(all(type(v) is str and v for v in h.values()), 'incomplete host/runtime controls')
    require(h['runtime'] == 'native', 'V1 is native calibration, not VM/TCG inference')
    require(type(m['environment']) is dict and all(
        type(k) is str and re.fullmatch('[A-Za-z_][A-Za-z0-9_]*', k) and type(v) is str and '\0' not in v
        for k, v in m['environment'].items()), 'invalid environment controls')
    c = m['controls']
    keys(c, 'target isa abi ub_assume semantic_flags lowering optimizer')
    require(all(type(c[k]) is str and c[k] for k in ('target', 'isa', 'abi')), 'missing target profile')
    keys(c['ub_assume'], 'requested effective')
    require(c['ub_assume'] == dict(requested='checked', effective='checked'),
            'unchecked R02d is not implemented; no inferred or mismatched UB profile')
    strings(c['semantic_flags'], empty=True)
    require(not any(flag in ('-ffast-math', '-Ofast') for flag in c['semantic_flags']),
            'calibration cannot introduce fast math')
    lowering = c['lowering']
    keys(lowering, 'optimization_level macros integer_constant_folding immutable_global_propagation '
                   'pure_value_reuse constant_edge_pruning linear_block_joining')
    require(natural(lowering['optimization_level']) <= 3, 'unsupported lowering level')
    strings(lowering['macros'], empty=True)
    require(all(type(value) is bool for key, value in lowering.items()
                if key not in ('optimization_level', 'macros')), 'lowering passes need explicit booleans')
    optimizer = c['optimizer']
    keys(optimizer, 'chain extractor counter_mapping registry max_nodes max_epochs max_transactions proof_required')
    require(optimizer['chain'] == 'scalar-chain-v1' and optimizer['extractor'] == 'selected-pair-v1'
            and optimizer['counter_mapping'] == 'legacy-coupled-v1'
            and optimizer['registry'] == 'p0.scalar.v4' and optimizer['proof_required'] is True,
            'unknown or unchecked optimization pipeline')
    require(natural(optimizer['max_nodes']) <= 100000 and natural(optimizer['max_epochs']) <= 1024
            and 1 <= natural(optimizer['max_transactions']) <= 8, 'optimizer limits exceed admitted profile')
    p = m['protocol']
    keys(p, 'phase variants primary boundary clock warmups pairs seed order estimator claims retries')
    require(p['phase'] == 'calibration' and p['variants'] == ['A0', 'A1']
            and p['claims'] == 'calibration-only', 'V1 does not admit A/B, search or performance claims')
    require(p['primary'] == 'process_wall_ns' and p['boundary'] == 'spawn-to-stdio-drain'
            and p['clock'] == 'monotonic_ns' and p['order'] == 'balanced-interleaved'
            and p['estimator'] == 'paired-median', 'unknown measurement boundary or estimator')
    require(0 <= natural(p['warmups']) <= 8 and 2 <= natural(p['pairs']) <= 100
            and natural(p['retries']) == 0, 'unbounded protocol or implicit retries')
    natural(p['seed'])
    r = m['resources']
    keys(r, 'jobs timeout_seconds output_bytes min_free_bytes max_retained_bytes')
    require(natural(r['jobs']) == 1 and 1 <= natural(r['timeout_seconds']) <= 600,
            'one bounded heavy job is required')
    require(1024 <= natural(r['output_bytes']) <= 16 * 1024**2
            and natural(r['min_free_bytes']) >= 2 * 1024**3
            and r['output_bytes'] <= natural(r['max_retained_bytes']) <= 1024**3,
            'invalid output/retention/reserve bound')
    require(type(m['cases']) is list and 1 <= len(m['cases']) <= 128, 'empty or unbounded population')
    names = set()
    for case in m['cases']:
        keys(case, 'id family split source inputs compiler_argv reference_argv link_argv run_argv '
                   'coverage result regime')
        require(type(case['id']) is str and re.fullmatch('[a-z0-9][a-z0-9._-]*', case['id'])
                and case['id'] not in names, 'invalid or duplicate case identity')
        names.add(case['id'])
        require(case['family'] in ('p0', 'zlib', 'zstd', 'cjson', 'jq', 'lua', 'sqlite')
                and case['split'] == 'development', 'calibration cannot consume held-out data')
        require(type(case['inputs']) is list, 'missing runtime-input inventory')
        for name in ('compiler_argv', 'reference_argv', 'link_argv', 'run_argv'):
            strings(case[name])
        require('{source}' in case['compiler_argv'] and '{source}' in case['reference_argv']
                and '{object}' in case['link_argv'] and '{executable}' in case['link_argv']
                and case['run_argv'][0] == '{executable}', 'argv must bind the named subject/artifacts')
        require(case['regime'] == 'fresh-process-no-cache-claim', 'unknown startup/cache regime')
        coverage = case['coverage']
        keys(coverage, 'subject_objects reference_objects')
        strings(coverage['subject_objects'])
        strings(coverage['reference_objects'], empty=True)
        objects = coverage['subject_objects'] + coverage['reference_objects']
        require(len(objects) == len(set(objects)), 'duplicate or overlapping object coverage')
        result = case['result']
        keys(result, 'exit_code stdout_sha256 stderr_sha256')
        require(natural(result['exit_code']) <= 255, 'invalid result exit code')
        sha256_identity(result['stdout_sha256'])
        sha256_identity(result['stderr_sha256'])
    by_path = {}
    for item, external_tool in artifacts(m):
        artifact(item, external_tool)
        require(item['path'] not in by_path or by_path[item['path']] == item['sha256'],
                'one artifact path has conflicting identities')
        by_path[item['path']] = item['sha256']
    return dict(schema='evocc-performance-plan-audit-v1', status='PLAN_VALIDATED_EXECUTION_NOT_RUN',
                cases=len(names), artifacts_declared=len(by_path), correctness='NOT_RUN',
                proof='NOT_RUN', performance='NOT_RUN', effective_controls='NOT_VERIFIED')


def check_committed(path, root, raw):
    relative = path.relative_to(root)
    require(relative.is_relative_to('tests/contracts'), 'plan must be frozen under tests/contracts')
    result = subprocess.run(['git', 'show', 'HEAD:' + relative.as_posix()], cwd=root,
                            capture_output=True, check=False)
    require(result.returncode == 0 and result.stdout == raw, 'commit the frozen plan before audit')


def audit_manifest(path, root):
    root, path = Path(root).resolve(), Path(path).resolve()
    require(path.is_relative_to(root), 'plan is outside workspace')
    raw = path.read_bytes()
    m = json.loads(raw, object_pairs_hook=unique)
    report = validate_manifest(m)
    check_committed(path, root, raw)
    check_source_revision(root, m['implementation_commit'])
    paths = {}
    for item, external_tool in artifacts(m):
        resolved = (root / item['path']).resolve()
        external = external_tool and Path(item['path']).is_absolute()
        require(external or resolved.is_relative_to(root), 'artifact symlink leaves workspace')
        require(digest(resolved.read_bytes()) == item['sha256'], 'artifact identity changed: ' + item['path'])
        paths[item['path']] = (item['sha256'], external)
    require(path.read_bytes() == raw, 'plan changed during audit')
    check_source_revision(root, m['implementation_commit'])
    # Detect mutation while later artifacts/source metadata were checked too.
    for name, (expected, external) in paths.items():
        resolved = (root / name).resolve()
        require((external or resolved.is_relative_to(root)) and digest(resolved.read_bytes()) == expected,
                'artifact identity changed during audit: ' + name)
    report.update(status='FROZEN_PLAN_AUDITED_EXECUTION_NOT_RUN', manifest_digest=digest(raw),
                  artifacts_checked=len(paths), host_controls='NOT_VERIFIED')
    return report
