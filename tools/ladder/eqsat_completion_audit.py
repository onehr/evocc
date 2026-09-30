"""Independently replay the deterministic R02 completion, never its timings."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from .eqsat_chain import AXES, digest, require, unique
from .eqsat_completion import BOUNDS, audit_plan, load
from .eqsat_elf import inspect_elf
from .eqsat_opportunities import lean_candidate, residual_candidate
from .eqsat_pilot_audit import audit_usage, completed_template_failure
from .eqsat_preflight import selected_pairs
from .eqsat_preflight_run import section_identity
from .eqsat_twins import bind_bundle, canonical


def audit_completion(directory, root, expected_manifest):
    directory, root = Path(directory).resolve(), Path(root).resolve()
    raw = (directory/'manifest.json').read_bytes()
    require(digest(raw) == expected_manifest, 'wrong completion manifest')
    # The committed manifest copy, not the capture's status label, is authority.
    paths = [p for p in (root/'tests/contracts').glob('*.json') if p.read_bytes() == raw]
    require(len(paths) == 1, 'completion manifest is not uniquely committed')
    m, _, source, original, previous, units, space, candidates = audit_plan(paths[0], root)
    tools = {name: str((root/item['path']).resolve()) for name, item in m['tools'].items()}
    r = load(directory/'receipt.json')
    require(r['schema'] == m['schema'] and r['manifest_digest'] == expected_manifest
        and r['status'] == 'OBSERVED_DETERMINISTIC_COMPLETION'
        and r['cleanup']['exit_code'] == 0 and r['native_timing'] == 'NOT_RUN'
        and r['ai_campaign'] == 'NOT_ADMITTED' and r['rule_installation'] == 'NOT_AUTHORIZED',
        'completion scope/execution changed')
    observed = {'manifest.json': digest(raw), 'receipt.json': digest((directory/'receipt.json').read_bytes())}
    def read(name):
        path = directory/name
        require(path.resolve().is_relative_to(directory) and not path.is_symlink(), 'unowned completion artifact')
        raw = path.read_bytes()
        require(str(name) not in observed or observed[str(name)] == digest(raw), 'completion artifact changed')
        observed[str(name)] = digest(raw)
        return raw
    def data(name):
        return json.loads(read(name), object_pairs_hook=unique)
    attempts = {}
    used = set()
    unknowns = {p['attempt_stdout'] for p in r['rule']['proofs'] if p['status'] == 'UNKNOWN'}
    for n, row in enumerate(r['attempts']):
        require(row['stage'] not in attempts and row['stdout'] == f'{n:03}.stdout'
            and row['stderr'] == f'{n:03}.stderr', 'attempt order/identity changed')
        if row['stdout'] in unknowns:
            completed_template_failure(row)
        else:
            require(row['status'] == 'COMPLETED' and row['exit_code'] == 0, 'unfinished completion command')
        audit_usage(row.get('resources'), 'Darwin')
        for stream in ('stdout', 'stderr'):
            content = read(row[stream])
            require(digest(content) == row[stream+'_sha256'] and len(content) == row[stream+'_bytes'], 'command output changed')
        attempts[row['stage']] = row
    def one(stage, argv):
        row = attempts[stage]
        require(stage not in used and row['argv'] == list(map(str, argv)), 'wrong/reused completion command')
        used.add(stage)
        return row
    container, checker = tools['container'], Path(tools['checker'])
    image = one('image-identity', [container, 'image', 'inspect', m['image']['name']])
    require(json.loads(read(image['stdout']))[0]['id'] == m['image']['digest'], 'image receipt mismatch')
    vm = 'evocc-r02-'+directory.name
    one('start-owned-vm', [container, 'run', '--rm', '-d', '--name', vm,
        '--cpus', str(m['resources']['vm_cpus']), '--memory', m['resources']['vm_memory'],
        '--read-only', '--tmpfs', '/tmp', '--no-dns',
        '--mount', f'type=bind,source={root},target=/work,readonly',
        '--mount', f'type=bind,source={directory},target=/runwork',
        m['image']['name'], '/bin/sleep', str(m['resources']['campaign_seconds'])])
    from .eqsat_pilot import GUEST_TOOLS
    row = one('guest-tools', [container, 'exec', vm, '/usr/bin/sha256sum', *GUEST_TOOLS])
    require({s.split()[1]: s.split()[0] for s in read(row['stdout']).decode().splitlines()} == m['guest_tools'],
            'guest tools do not match plan')
    one('zombie-only-group', [container, 'exec', vm, '/usr/bin/python3', '-B', '-m', 'tools.ladder.eqsat_guest',
        '--directory', '/runwork/zombie-regression', '--timeout', '10', '--output-bytes', '1024',
        '--reserve', str(m['resources']['min_free_bytes']), '--',
        '/usr/bin/python3', '/work/tests/fixtures/eqsat-performance/zombie-only-group.py'])
    zombie = data('zombie-regression/receipt.json')
    require(zombie['status'] == 'OBSERVED' and len(zombie['attempts']) == 1
        and zombie['attempts'][0]['exit_code'] == 0, 'zombie-only group treated as live failure')
    for u in units:
        bind_bundle(source/u['baseline'], Path(tools['historical_checker']))
    expected = [(c, schedule) for c in space['candidates'] for schedule in BOUNDS['schedules']]
    evaluations = r['policy']['evaluations']
    require(len(evaluations) == len(expected) == BOUNDS['whole_suite_evaluations'], 'policy census incomplete')
    pairs, code_keys, proofs, caps = set(map(tuple, space['comparisons'])), set(), 0, 0
    for i, (evaluation, (candidate, schedule)) in enumerate(zip(evaluations, expected)):
        require(evaluation['weights'] == candidate['weights'] and evaluation['max_transactions'] == schedule
            and evaluation['status'] == 'OBSERVED_CODE_SIZE_ONLY' and len(evaluation['units']) == len(units),
            'policy evaluation altered/missing')
        weights = ','.join(f'{a}={w}' for a, w in zip(AXES, candidate['weights']))
        by_case = {}
        for n, (u, detail) in enumerate(zip(units, evaluation['units'])):
            folder = Path('policies')/f'{i:04}'/f'{n:02}-chain'
            bundle = directory/folder
            require(detail['case'] == u['case'] and detail['source'] == u['source']
                and detail['bundle'] == folder.as_posix(), 'policy source/TU swapped')
            row = one(f'policy:{i}:chain:{n}', [tools['census'], 'policy', u['input_digest'], source/u['input'],
                checker, m['tools']['checker']['sha256'], weights, str(schedule), bundle])
            binding = bind_bundle(bundle, checker)
            for name in binding['inventory']:
                read(folder/name)
            require(detail['binding'] == {k: binding[k] for k in ('ir_digest', 'chain_record_digest', 'bundle_digest')}
                and detail['proof_counts'] == binding['audit']['proof_counts']
                and read(row['stdout']) == read(folder/'output.evo'), 'policy chain/replay/output changed')
            record = data(folder/'chain.json')
            require(record['input_digest'] == u['input_digest'] and record['max_transactions'] == schedule
                and record['policy']['cost_weights'] == candidate['weights'], 'policy identity differs from candidate')
            optimizer = BOUNDS['optimizer']
            require(all(record['policy'][k] == optimizer[k] for k in ('registry', 'extractor', 'counter_mapping'))
                and record['policy']['schedule'] == dict(max_transactions=schedule,
                    max_nodes=optimizer['max_nodes'], max_epochs=optimizer['max_epochs']),
                'policy does not consume the frozen default caps/extractor/registry')
            actual_pairs = []
            for tx in record['transactions']:
                read(folder/f"{tx['index']:03}.policy.json")
                actual_pairs.extend(selected_pairs(bundle/f"{tx['index']:03}.policy.json", tx,
                                                   candidate['weights'])['comparisons'])
            hits = [h for t in record['transactions'] for h in t['budget_hits']]
            require(detail['hits'] == hits and detail['comparisons'] == actual_pairs, 'gate/cap telemetry changed')
            pairs.update(map(tuple, actual_pairs)); caps += bool(hits)
            proofs += binding['audit']['proof_counts']['proof_successes']
            for profile in ('checked', 'unchecked'):
                ident = digest(canonical((binding['ir_digest'], profile)))
                require(detail['native'][profile] == ident and ident in r['code'], 'wrong backend memoization key')
                code = r['code'][ident]
                if ident not in code_keys:
                    code_keys.add(ident)
                    asm = one('backend:'+ident, [tools['ir_backend'], 'linux-aarch64', binding['ir_digest'],
                        bundle/'output.evo', '--ub-checks='+profile])
                    assembly = read(Path('code')/(ident+'.s'))
                    require(assembly == read(asm['stdout']) and digest(assembly) == code['assembly_digest'], 'assembly changed')
                    one('assemble:'+ident, [container, 'exec', vm, '/usr/bin/gcc-14', '-c', '-x', 'assembler',
                        '/runwork/code/'+ident+'.s', '-o', '/runwork/code/'+ident+'.o'])
                    metrics = inspect_elf(read(code['object']))
                    require(code['object'] == 'code/'+ident+'.o' and code['profile'] == profile
                        and code['ir_digest'] == binding['ir_digest'] and code['object_digest'] == metrics['file_digest']
                        and code['metrics'] == metrics and code['text_bytes'] == metrics['executable_section_bytes']
                        and code['code_digest'] == section_identity(metrics), 'object size/code-class metrics forged')
            by_case.setdefault(u['case'], []).append(detail)
        programs = [dict(case=c['id'], profiles={profile:
            dict(code_digest=digest(canonical([r['code'][d['native'][profile]]['code_digest'] for d in by_case[c['id']]])),
                 subject_text_bytes=sum(r['code'][d['native'][profile]]['text_bytes'] for d in by_case[c['id']]))
            for profile in ('checked', 'unchecked')}) for c in original['cases']]
        require(evaluation['programs'] == programs, 'suite object-size fitness forged')
    new_pairs = sorted(pairs-set(map(tuple, space['comparisons'])), key=str)
    classes = len({digest(canonical(e['programs'])) for e in evaluations})
    require(r['policy']['observed_new_comparisons'] == new_pairs and r['policy']['code_classes'] == classes
        and r['policy']['verdict'] == ('INCONCLUSIVE' if new_pairs else 'FINITE_ENUMERATION_SUFFICIENT'),
        'policy ceiling exceeds finite census')
    require(set(r['code']) == code_keys, 'unreferenced/missing code classes')
    proof_rows = r['rule']['proofs']
    require([p['candidate'] for p in proof_rows] == sorted(candidates), 'template population/order changed')
    proved = set()
    for p in proof_rows:
        identity, candidate = p['candidate'], candidates[p['candidate']]
        file = directory/'templates'/(identity+'.lean')
        require(read(file.relative_to(directory)).decode() == lean_candidate(candidate['lhs'], candidate['rhs']),
                'untrusted proof source/template')
        row = one('preflight:template:'+identity, [tools['collector_python'], root/'tools/ladder/eqsat_template.py',
            tools['lean'], root/'lean', file])
        require(p['template'] == BOUNDS['template'] and p['attempt_stdout'] == row['stdout'], 'template binding changed')
        output = read(row['stdout']).decode()
        if p['status'] == 'PROVED_TEMPLATE_ONLY_NOT_INSTALLED':
            require(row['status'] == 'COMPLETED' and p['output'] == row['stdout']
                and 'EvoIR.preflight_sound' in output and 'RuleCovered preflight_candidate' in output
                and 'sorryAx' not in output and 'ofReduceBool' not in output, 'invalid template proof promoted')
            proved.add(identity)
        else:
            require(p['status'] == 'UNKNOWN', 'unknown template outcome')
            completed_template_failure(row)
    relevant = {v['id']: residual_candidate(v) for v in previous['preflight']['oracle']['residuals']}
    require(r['rule']['proved_templates'] == len(proved) and r['rule']['unknown_templates'] == len(candidates)-len(proved)
        and r['rule']['unattempted_templates'] == 0 and r['rule']['residual_mapping'] == relevant
        and r['rule']['proved_residuals'] == [i for i, c in relevant.items() if c.get('candidate_digest') in proved]
        and r['rule']['verdict'] == 'INCONCLUSIVE', 'template proof mistaken for useful new-rule qualification')
    require(used == set(attempts), 'unexpected or unused completion command')
    for name, expected in observed.items():
        require(digest((directory/name).read_bytes()) == expected, 'completion mutated during audit')
    audit_plan(paths[0], root)
    return dict(status='DETERMINISTIC_CAPTURE_CONSISTENT', policy_evaluations=len(evaluations),
        verified_chains=len(evaluations)*len(units), proof_successes=proofs, code_classes=classes,
        unique_backend_objects=len(code_keys), cap_hit_chains=caps, template_proofs=len(proved),
        template_unknowns=len(candidates)-len(proved), unattempted_templates=0,
        policy_verdict=r['policy']['verdict'], rule_verdict=r['rule']['verdict'],
        shortcut_verdict='INCONCLUSIVE_NO_NEW_SHORTCUT_COST_EVIDENCE', ai_campaign='NOT_ADMITTED',
        fitness='SUBJECT_OBJECT_ALLOCATABLE_AND_RELOCATION_CODE_ONLY_NOT_LATENCY')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--manifest-sha256', required=True)
    args = parser.parse_args()
    print(json.dumps(audit_completion(args.directory, args.root, args.manifest_sha256), sort_keys=True))


if __name__ == '__main__':
    main()
