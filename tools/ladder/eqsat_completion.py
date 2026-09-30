"""Complete the frozen R02 selected-pair and fixed-template preflight.

This extends the bench/capture path, not the optimizer or rule registry. Content
addressing deduplicates immutable evidence; every policy still runs the shared
chain and every emitted certificate is independently checked. No timing fitness,
model invocation, new rule installation or source/workspace clone is involved.
"""
from __future__ import annotations

import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

from .eqsat_chain import AXES, digest, keys, require, unique, validate_record
from .eqsat_census import check_source_revision
from .eqsat_elf import inspect_elf
from .eqsat_native import Capture
from .eqsat_opportunities import enumerate_rule_cell, lean_candidate, residual_candidate
from .eqsat_pilot import COLLECTORS, LEAN_SOURCES, LEAN_IMPORTS, GUEST_TOOLS
from .eqsat_preflight import enumerate_policies, selected_pairs
from .eqsat_preflight_run import CONTRACT, fixed_template_attempt, section_identity
from .eqsat_twins import bind_bundle, canonical

BOUNDS = dict(signature_visits=1024, whole_suite_evaluations=1024, schedules=[1, 8],
    optimizer=dict(registry='p0.scalar.v4', extractor='selected-pair-v1',
        counter_mapping='legacy-coupled-v1', max_nodes=100000, max_epochs=1024, proof_required=True),
    rule_cells=[1, 2], syntax_visits_per_cell=6000, proof_attempts=1222,
    template=CONTRACT['template'], template_heartbeats=200000,
    order='POLICY_SIGNATURE_THEN_SCHEDULE_AND_TEMPLATE_DIGEST', retries=0,
    fitness='ALLOCATABLE_AND_RELOCATION_BYTES_ONLY', native_timing='NOT_RUN',
    rule_installation='NOT_AUTHORIZED', candidate_budget_unit=CONTRACT['evaluation_unit'],
    retention='UNIQUE_INODE_FULL_SCAN_MINIMUM_COMPLETION_INTERVAL_1S_AND_FINAL_SCAN')
FILES = (*COLLECTORS, 'tools/ladder/eqsat_completion.py',
    'tools/ladder/eqsat_completion_audit.py', *LEAN_SOURCES, *LEAN_IMPORTS,
    'docs/evoir-spec.md', 'docs/evomir-spec.md',
    'tests/fixtures/eqsat-performance/zombie-only-group.py')


def load(path):
    return json.loads(Path(path).read_bytes(), object_pairs_hook=unique)


def populations(source):
    """Read the retained, pinned canonical input population; no new lowering."""
    source = Path(source)
    manifest, receipt = load(source/'manifest.json'), load(source/'receipt.json')
    require(receipt['status'] == 'OBSERVED' and receipt['manifest_digest'] ==
            digest((source/'manifest.json').read_bytes()), 'historical source capture incomplete')
    require(receipt['preflight']['bounds'] == CONTRACT, 'different historical grammar/population')
    comparisons, units = set(), []
    for case, product in zip(manifest['cases'], receipt['cases']):
        require(product['id'] == case['id'] and product['status'] == 'OBSERVED', 'source case incomplete')
        require([u['source'] for u in product['units']] == case['subject_sources'], 'source TU population changed')
        for unit in product['units']:
            n = case['sources'].index(unit['source'])
            identity = source/case['id']/f'{n:02}-A-chain'
            baseline = source/case['id']/f'{n:02}-B-chain'
            record = load(baseline/'chain.json')
            validate_record(record)
            require(record['publication'] == 'VERIFIED' and record['input_digest'] ==
                unit['variants']['A']['binding']['ir_digest'], 'historical input/chain mismatch')
            require(digest((identity/'output.evo').read_bytes()) == record['input_digest']
                and digest((baseline/'output.evo').read_bytes()) == record['output_digest'], 'historical IR changed')
            units.append(dict(case=case['id'], source=unit['source'], number=n,
                input=(identity/'output.evo').relative_to(source).as_posix(),
                input_digest=record['input_digest'], baseline=baseline.relative_to(source).as_posix()))
            for tx in record['transactions']:
                comparisons.update(tuple(p) for p in selected_pairs(
                    baseline/f"{tx['index']:03}.policy.json", tx, record['policy']['cost_weights'])['comparisons'])
    require(len(manifest['cases']) == len(receipt['cases']) == 4 and len(units) == 6,
            'not the frozen six-TU library pilot')
    policy = enumerate_policies(comparisons, BOUNDS['signature_visits'])
    require(policy['status'] == 'ENUM_COMPLETE' and len(policy['candidates']) == 512,
            'selected-pair signature population changed')
    candidates = {}
    for nodes in BOUNDS['rule_cells']:
        cell = enumerate_rule_cell(nodes, syntax_cap=BOUNDS['syntax_visits_per_cell'])
        require(cell['status'] == 'ENUM_COMPLETE', 'historical syntax cell incomplete')
        candidates.update((s['candidate_digest'], s) for s in cell['survivors'])
    require(len(candidates) == BOUNDS['proof_attempts'], 'fixed template population changed')
    return manifest, receipt, units, policy, candidates


def historical_files(source, manifest, units):
    """Pin proofs, IR, reference objects and inputs actually reused by this run."""
    source = Path(source)
    names = {'manifest.json', 'receipt.json', 'preflight/report.json', 'preflight/oracle.json'}
    for unit in units:
        names.add(unit['input'])
        names.update(p.relative_to(source).as_posix() for p in (source/unit['baseline']).iterdir())
    for case in manifest['cases']:
        names.update(f"{case['id']}/ref-{n:02}.o" for n, s in enumerate(case['sources'])
                     if s not in case['subject_sources'])
        if case['id'] != 'p0':
            names.add(case['id']+'/harness.o')
    return {name: digest((source/name).read_bytes()) for name in sorted(names)}


def validate_plan(m):
    keys(m, 'schema implementation_commit source tools files image guest_tools bounds resources')
    require(m['schema'] == 'evocc-r02-preflight-completion-v1' and canonical(m['bounds']) == canonical(BOUNDS),
            'unregistered deterministic completion plan')
    keys(m['source'], 'directory files')
    keys(m['tools'], 'checker historical_checker census ir_backend collector_python lean container')
    require(set(m['files']) == set(FILES), 'missing completion collector/semantic input')
    r = m['resources']
    keys(r, 'jobs timeout_seconds output_bytes min_free_bytes max_retained_bytes campaign_seconds vm_cpus vm_memory')
    require(canonical(r) == canonical(dict(jobs=1, timeout_seconds=300, output_bytes=16*1024**2,
        min_free_bytes=8*1024**3, max_retained_bytes=3*1024**3,
        campaign_seconds=14400, vm_cpus=2, vm_memory='3G')), 'unregistered completion resource caps')
    require(set(m['guest_tools']) == set(GUEST_TOOLS), 'missing guest tool identities')


def audit_plan(path, root):
    path, root = Path(path).resolve(), Path(root).resolve()
    raw, m = path.read_bytes(), load(path)
    validate_plan(m)
    relative = path.relative_to(root).as_posix()
    committed = subprocess.run(['git', 'show', 'HEAD:'+relative], cwd=root,
                               capture_output=True, check=False)
    require(committed.returncode == 0 and committed.stdout == raw, 'commit the completion manifest before execution')
    check_source_revision(root, m['implementation_commit'])
    require(Path(sys.executable).resolve() == (root/m['tools']['collector_python']['path']).resolve(),
            'wrong collector interpreter')
    for item in m['tools'].values():
        require(digest((root/item['path']).read_bytes()) == item['sha256'], 'completion tool changed')
    for name, expected in m['files'].items():
        require(digest((root/name).read_bytes()) == expected, 'completion input changed: '+name)
    source = root/m['source']['directory']
    manifest, receipt, units, policy, candidates = populations(source)
    require(historical_files(source, manifest, units) == m['source']['files'], 'historical input changed')
    require(m['tools']['historical_checker']['sha256'] == manifest['tools']['checker']['sha256'],
            'wrong historical replay checker')
    return m, raw, source, manifest, receipt, units, policy, candidates


def deduplicate(folder, store):
    """Hard-link equal immutable bytes, never discard the only retained artifact.

    Only a completed, run-owned bundle enters this store. Before replacing its
    duplicate file, compare exact bytes and create the replacement link first.
    Neither input snapshots nor historical evidence is modified.
    """
    for path in sorted(Path(folder).iterdir()):
        require(path.is_file() and not path.is_symlink(), 'unowned bundle member')
        raw = path.read_bytes()
        target = Path(store)/digest(raw)
        if target.exists():
            require(not target.is_symlink() and target.read_bytes() == raw, 'content store collision/mutation')
        else:
            with target.open('xb') as file:
                file.write(raw)
            target.chmod(0o444)
        link = path.with_name(path.name+'.dedup-pending')
        os.link(target, link)
        os.replace(link, path)


def run_completion(path, root):
    root = Path(root).resolve()
    m, raw, source, original, previous, units, space, candidates = audit_plan(path, root)
    tools = {name: (root/item['path']).resolve() for name, item in m['tools'].items()}
    (root/'.build').mkdir(exist_ok=True)
    with (root/'.build/bench.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError('another benchmark owns the shared lock') from error
        require(shutil.disk_usage(root).free >= m['resources']['min_free_bytes'], 'disk reserve unavailable')
        base = root/'.evidence/eqsat-completion'
        base.mkdir(exist_ok=True)
        directory = Path(tempfile.mkdtemp(prefix='complete-', dir=base))
        (directory/'manifest.json').write_bytes(raw)
        for name in ('store', 'policies', 'templates', 'code'):
            (directory/name).mkdir()
        receipt = dict(schema=m['schema'], status='STARTED', manifest_digest=digest(raw),
            attempts=[], policy=dict(evaluations=[], verdict='INCONCLUSIVE'),
            rule=dict(proofs=[], verdict='INCONCLUSIVE'), code={}, native_timing='NOT_RUN',
            ai_campaign='NOT_ADMITTED', rule_installation='NOT_AUTHORIZED')
        def save():
            pending = directory/'receipt.json.pending'
            pending.write_bytes(canonical(receipt)+b'\n')
            pending.replace(directory/'receipt.json')
        env = dict(PATH='/usr/bin:/bin:/usr/local/bin', LANG='C', LC_ALL='C', TMPDIR=str(directory))
        resources = {k: v for k, v in m['resources'].items() if k not in
                     ('jobs', 'campaign_seconds', 'vm_cpus', 'vm_memory')}
        runner = Capture(directory, resources, env, receipt, save, measure_resources=True,
                         unique_inodes=True, watch_interval_seconds=1)
        started = time.monotonic()
        vm, vm_started = 'evocc-r02-'+directory.name, False
        def stable():
            require(time.monotonic()-started < m['resources']['campaign_seconds'], 'completion campaign watchdog expired')
            require(Path(path).read_bytes() == raw, 'completion plan changed')
        def stable_tools():
            for item in m['tools'].values():
                require(digest((root/item['path']).read_bytes()) == item['sha256'], 'completion tool changed')
        def command(stage, argv, expected=None):
            stable()
            row = runner.run(stage, argv, expected)
            stable()
            return row
        def guest_path(file):
            file = Path(file).resolve()
            if file.is_relative_to(directory):
                return '/runwork/'+file.relative_to(directory).as_posix()
            require(file.is_relative_to(root), 'unmounted guest artifact')
            return '/work/'+file.relative_to(root).as_posix()
        def guest(stage, argv):
            return command(stage, [tools['container'], 'exec', vm, *argv])
        code = {}
        save()
        try:
            # Revalidate the prior exact baseline proofs with their original
            # checker before reusing canonical IR; do not reclassify old timing.
            for n, u in enumerate(units):
                binding = bind_bundle(source/u['baseline'], tools['historical_checker'])
                require(binding['ir_digest'] == load(source/u['baseline']/'chain.json')['output_digest'],
                        'historical baseline replay mismatch')
            row = command('image-identity', [tools['container'], 'image', 'inspect', m['image']['name']])
            require(load(directory/row['stdout'])[0]['id'] == m['image']['digest'], 'sealed image changed')
            vm_started = True
            command('start-owned-vm', [tools['container'], 'run', '--rm', '-d', '--name', vm,
                '--cpus', str(m['resources']['vm_cpus']), '--memory', m['resources']['vm_memory'],
                '--read-only', '--tmpfs', '/tmp', '--no-dns',
                '--mount', f'type=bind,source={root},target=/work,readonly',
                '--mount', f'type=bind,source={directory},target=/runwork',
                m['image']['name'], '/bin/sleep', str(m['resources']['campaign_seconds'])])
            row = guest('guest-tools', ['/usr/bin/sha256sum', *GUEST_TOOLS])
            hashes = {line.split()[1]: line.split()[0] for line in (directory/row['stdout']).read_text().splitlines()}
            require(hashes == m['guest_tools'], 'guest tools changed')
            guest('zombie-only-group', ['/usr/bin/python3', '-B', '-m', 'tools.ladder.eqsat_guest',
                '--directory', '/runwork/zombie-regression', '--timeout', '10',
                '--output-bytes', '1024', '--reserve', str(resources['min_free_bytes']), '--',
                '/usr/bin/python3', '/work/tests/fixtures/eqsat-performance/zombie-only-group.py'])
            require(load(directory/'zombie-regression/receipt.json')['status'] == 'OBSERVED',
                    'zombie-only group regression did not finish')
            observed_pairs = set(tuple(p) for p in space['comparisons'])
            for candidate in space['candidates']:
                for schedule in BOUNDS['schedules']:
                    stable_tools()
                    index = len(receipt['policy']['evaluations'])
                    evaluation = dict(weights=candidate['weights'], max_transactions=schedule,
                                      status='STARTED', units=[], programs=[])
                    receipt['policy']['evaluations'].append(evaluation)
                    weights = ','.join(f'{a}={w}' for a, w in zip(AXES, candidate['weights']))
                    folder = directory/'policies'/f'{index:04}'
                    folder.mkdir()
                    by_case = {}
                    for n, u in enumerate(units):
                        bundle = folder/f'{n:02}-chain'
                        row = command(f'policy:{index}:chain:{n}', [tools['census'], 'policy',
                            u['input_digest'], source/u['input'], tools['checker'],
                            m['tools']['checker']['sha256'], weights, str(schedule), bundle])
                        binding = bind_bundle(bundle, tools['checker'])
                        require((directory/row['stdout']).read_bytes() == (bundle/'output.evo').read_bytes(),
                                'policy adapter emitted different codegen IR')
                        record = load(bundle/'chain.json')
                        detail = dict(case=u['case'], source=u['source'], bundle=bundle.relative_to(directory).as_posix(),
                            binding={k: binding[k] for k in ('ir_digest', 'chain_record_digest', 'bundle_digest')},
                            proof_counts=binding['audit']['proof_counts'],
                            hits=[h for t in record['transactions'] for h in t['budget_hits']],
                            comparisons=[], native={})
                        for tx in record['transactions']:
                            trace = selected_pairs(bundle/f"{tx['index']:03}.policy.json", tx, candidate['weights'])
                            detail['comparisons'].extend(trace['comparisons'])
                            observed_pairs.update(tuple(p) for p in trace['comparisons'])
                        deduplicate(bundle, directory/'store')
                        # stdout is canonical IR already retained in the bundle.
                        deduplicate_one(directory/row['stdout'], directory/'store')
                        for profile in ('checked', 'unchecked'):
                            key = (binding['ir_digest'], profile)
                            if key not in code:
                                ident = digest(canonical(key))
                                assembly = directory/'code'/(ident+'.s')
                                row = command('backend:'+ident, [tools['ir_backend'], 'linux-aarch64',
                                    binding['ir_digest'], bundle/'output.evo', '--ub-checks='+profile])
                                assembly.write_bytes((directory/row['stdout']).read_bytes())
                                deduplicate_one(assembly, directory/'store')
                                deduplicate_one(directory/row['stdout'], directory/'store')
                                obj = directory/'code'/(ident+'.o')
                                guest('assemble:'+ident, ['/usr/bin/gcc-14', '-c', '-x', 'assembler',
                                    guest_path(assembly), '-o', guest_path(obj)])
                                metrics = inspect_elf(obj.read_bytes())
                                receipt['code'][ident] = dict(object=obj.relative_to(directory).as_posix(),
                                    object_digest=digest(obj.read_bytes()), code_digest=section_identity(metrics),
                                    text_bytes=metrics['executable_section_bytes'], metrics=metrics,
                                    assembly_digest=digest(assembly.read_bytes()), profile=profile,
                                    ir_digest=binding['ir_digest'])
                                code[key] = ident
                            detail['native'][profile] = code[key]
                        evaluation['units'].append(detail)
                        by_case.setdefault(u['case'], []).append(detail)
                    # Deterministic object code size is the fitness. There is no
                    # latency/RSS result or unchecked-execution admission here.
                    for case in original['cases']:
                        pieces = by_case[case['id']]
                        evaluation['programs'].append(dict(case=case['id'], profiles={profile:
                            dict(code_digest=digest(canonical([receipt['code'][d['native'][profile]]['code_digest'] for d in pieces])),
                                 subject_text_bytes=sum(receipt['code'][d['native'][profile]]['text_bytes'] for d in pieces))
                            for profile in ('checked', 'unchecked')}))
                    evaluation['status'] = 'OBSERVED_CODE_SIZE_ONLY'
                    stable_tools()
                    save()
            policy = receipt['policy']
            require(len(policy['evaluations']) == BOUNDS['whole_suite_evaluations'], 'incomplete policy population')
            policy.update(observed_new_comparisons=sorted(observed_pairs-set(map(tuple, space['comparisons'])), key=str),
                code_classes=len({digest(canonical(e['programs'])) for e in policy['evaluations']}),
                verdict='FINITE_ENUMERATION_SUFFICIENT' if observed_pairs == set(map(tuple, space['comparisons'])) else 'INCONCLUSIVE',
                scope='FIXED_SELECTED_PAIR_AND_SCHEDULE_GRID_OBJECT_CODE_SIZE_NOT_LATENCY')
            for identity, candidate in sorted(candidates.items()):
                stable_tools()
                file = directory/'templates'/(identity+'.lean')
                file.write_text(lean_candidate(candidate['lhs'], candidate['rhs']))
                proof = fixed_template_attempt(identity, BOUNDS['template'],
                    [tools['collector_python'], root/'tools/ladder/eqsat_template.py',
                     tools['lean'], root/'lean', file], directory, command)
                proof['attempt_stdout'] = receipt['attempts'][-1]['stdout']
                receipt['rule']['proofs'].append(proof)
                stable_tools()
                save()
            relevant = {r['id']: residual_candidate(r) for r in previous['preflight']['oracle']['residuals']}
            proved = {p['candidate'] for p in receipt['rule']['proofs'] if p['status'].startswith('PROVED_')}
            receipt['rule'].update(proved_templates=len(proved), unknown_templates=len(candidates)-len(proved),
                unattempted_templates=0, residual_mapping=relevant,
                proved_residuals=[ident for ident, c in relevant.items() if c.get('candidate_digest') in proved],
                verdict='INCONCLUSIVE', scope='FIXED_SIMP_TEMPLATE_PROOFS_NOT_NEW_RULE_UTILITY',
                reason='EXTERNAL_RESIDUALS_OUTSIDE_GRAMMAR_OR_NO_INTEGRATED_USEFUL_NEW_RULE')
            runner.last_watch = float('-inf')
            runner.watch()
            audit_plan(path, root)
            receipt['status'] = 'OBSERVED_DETERMINISTIC_COMPLETION'
        except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
            receipt.update(status='FAILED', error=str(error))
        except BaseException:
            receipt['status'] = 'INTERRUPTED'
            raise
        finally:
            if vm_started:
                try:
                    cleanup = subprocess.run([tools['container'], 'stop', vm], capture_output=True,
                        timeout=30, check=False)
                    receipt['cleanup'] = dict(exit_code=cleanup.returncode,
                        stdout=cleanup.stdout.decode(errors='backslashreplace'),
                        stderr=cleanup.stderr.decode(errors='backslashreplace'))
                    if cleanup.returncode != 0:
                        receipt['status'] = 'FAILED'
                except (OSError, subprocess.SubprocessError) as error:
                    receipt.update(status='FAILED', cleanup=dict(error=str(error)))
            save()
        return directory, receipt


def deduplicate_one(path, store):
    raw = Path(path).read_bytes()
    target = Path(store)/digest(raw)
    if target.exists():
        require(target.read_bytes() == raw and not target.is_symlink(), 'content store mutation')
    else:
        with target.open('xb') as file:
            file.write(raw)
        target.chmod(0o444)
    pending = Path(path).with_name(Path(path).name+'.dedup-pending')
    os.link(target, pending)
    os.replace(pending, path)
