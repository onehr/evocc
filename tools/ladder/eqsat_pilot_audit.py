"""Independent pilot artifact/result accounting; timings remain collector telemetry.

Replay required EVP bundles, recompute ELF sizes and compare every checked twin
and sample to captured guest executions. Do not rerun programs or repair receipts.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from .eqsat_chain import AXES, digest, require, unique, natural
from .eqsat_elf import inspect_elf
from .eqsat_native import pair_order
from .eqsat_pilot import validate_plan, composite_binding, pilot_statistics, GUEST_TOOLS, tree_identity
from .eqsat_census import chain_census
from .eqsat_preflight import selected_pairs
from .eqsat_twins import bind_bundle, canonical, native_identity, admit_unchecked
from .eqsat_opportunities import (diagnostic_expressions, enumerate_rule_cell,
                                  llvm_function, llvm_functions, lean_candidate, residual_candidate)
from .eqsat_preflight import enumerate_policies
from .eqsat_preflight_run import section_identity, shortcut_census
import re


def audit_pilot(directory, root, expected_manifest_digest):
    directory, root = Path(directory).resolve(), Path(root).resolve()
    observed = {}
    def read(relative):
        path = directory/relative
        require(path.resolve().is_relative_to(directory) and not path.is_symlink(), 'unowned capture path')
        raw = path.read_bytes()
        require(str(relative) not in observed or observed[str(relative)] == digest(raw), 'capture changed')
        observed[str(relative)] = digest(raw)
        return raw
    def load(relative):
        return json.loads(read(relative), object_pairs_hook=unique)
    m = load('manifest.json')
    require(observed['manifest.json'] == expected_manifest_digest, 'wrong frozen manifest')
    validate_plan(m, root)
    for name, expected in m['files'].items():
        require(digest((root/name).read_bytes()) == expected, 'auditor/input changed since freeze: '+name)
    for item in m['tools'].values():
        require(digest((root/item['path']).read_bytes()) == item['sha256'], 'audit tool identity changed')
    for item in m['trees'].values():
        require(tree_identity(root/item['path']) == {k: item[k] for k in ('sha256', 'files')},
                'audit source/header tree changed')
    r = load('receipt.json')
    require(r['schema'] == 'evocc-r02-linux-pilot-receipt-v1' and r['status'] == 'OBSERVED',
            'failed or incomplete pilot cannot be accepted')
    require(r['manifest_digest'] == expected_manifest_digest
            and r['implementation_commit'] == m['implementation_commit'] and r['host'] == m['host'],
            'pilot identity mismatch')
    require(r['scope'] == 'DECLARED_MIXED_LINK_LINUX_HARDWARE_VM'
            and r['performance'] == 'PILOT_NOT_FINAL_Q2'
            and r['controller_timing'] == 'NOT_PRODUCT_TIME'
            and r['interference'] == 'HOST_AFFINITY_UNCONTROLLED'
            and r['cleanup']['exit_code'] == 0, 'scope or ownership changed')
    checker = (root/m['tools']['checker']['path']).resolve()
    require(digest(checker.read_bytes()) == m['tools']['checker']['sha256'], 'wrong replay checker')
    unknown_templates = {p.get('attempt_stdout') for p in
        r.get('preflight', {}).get('rule', {}).get('proofs', []) if p['status'] == 'UNKNOWN'}
    attempts, guests, stages = {}, {}, {}
    for index, row in enumerate(r['attempts']):
        template_unknown = row['stdout'] in unknown_templates and row['stage'].startswith('preflight:template:')
        require(natural(row['wall_ns']) > 0 and
                (template_unknown or (row['status'] in ('COMPLETED', 'RESULT_CHECKED')
                 and type(row['exit_code']) is int and row['exit_code'] == 0)), 'unfinished/unclassified attempt')
        require(row['stage'] not in stages, 'duplicate stage')
        stages.setdefault(row['stage'], []).append(row)
        for stream in ('stdout', 'stderr'):
            require(row[stream] == f'{index:03}.{stream}', 'attempt output order mismatch')
            raw = read(row[stream])
            require(digest(raw) == row[stream+'_sha256'] and len(raw) == row[stream+'_bytes'], 'output changed')
        attempts[row['stdout']] = row
        if 'guest_receipt' in row:
            raw = read(row['guest_receipt'])
            require(digest(raw) == row['guest_receipt_digest'], 'guest capture changed')
            g = json.loads(raw, object_pairs_hook=unique)
            require(g['status'] == 'OBSERVED' and g['runtime'] == 'Linux-aarch64-hardware-VM'
                    and len(g['attempts']) == 1, 'guest execution incomplete')
            child = g['attempts'][0]
            require(child['exit_code'] == 0 and child['status'] == 'COMPLETED'
                    and g['executable_digest'] == row['guest_executable_digest']
                    and child['wall_ns'] == row['guest_wall_ns']
                    and child['resources'] == row['guest_resources'], 'guest metric mismatch')
            usage = child['resources']
            for field in ('user_ns', 'system_ns', 'peak_rss_bytes'):
                natural(usage[field])
            require(usage['peak_rss_bytes'] > 0 and usage['peak_rss_bytes'] == usage['peak_rss_raw']*1024
                    and usage['scope'] == 'wait4-direct-child-including-waited-descendants', 'guest resource units changed')
            for stream in ('stdout', 'stderr'):
                raw = read(str(Path(row['guest_receipt']).parent/child[stream]))
                require(digest(raw) == child[stream+'_sha256'] == row[stream+'_sha256'], 'guest output changed')
            require(row['argv'][-len(child['argv']):] == child['argv'], 'guest argv substitution')
            guests[row['guest_receipt']] = (row, child)

    def one(stage):
        require(stage in stages and len(stages[stage]) == 1, 'missing/duplicate required stage: '+stage)
        return stages[stage][0]
    def result(row, expected):
        require(row['status'] == 'RESULT_CHECKED' and all(row[k] == expected[k] for k in expected),
                'missing exact result check')
    def guest_path(path):
        path = Path(path).resolve()
        if path.is_relative_to(directory):
            return '/runwork/'+path.relative_to(directory).as_posix()
        require(path.is_relative_to(root), 'unmounted audited input')
        return '/work/'+path.relative_to(root).as_posix()
    def guest_command(stage, argv):
        row = one(stage)
        require(row.get('guest_receipt') in guests and guests[row['guest_receipt']][1]['argv'] == argv,
                'guest build/input argv changed: '+stage)
        return row
    tools = {k: str((root/v['path']).resolve()) for k, v in m['tools'].items()}
    require(one('image-identity')['argv'] == [tools['container'], 'image', 'inspect', m['image']['name']],
            'image identity command changed')
    require(load(one('image-identity')['stdout'])[0]['id'] == m['image']['digest'], 'wrong measured image')
    startup = one('start-owned-vm')
    require(startup['argv'] == [tools['container'], 'run', '--rm', '-d', '--name', 'evocc-r02-'+directory.name,
        '--cpus', str(m['resources']['vm_cpus']), '--memory', m['resources']['vm_memory'],
        '--read-only', '--tmpfs', '/tmp', '--no-dns', '--mount',
        f'type=bind,source={root},target=/work,readonly', '--mount',
        f'type=bind,source={directory},target=/runwork', m['image']['name'],
        '/bin/sleep', str(m['resources']['campaign_seconds'])], 'VM ownership/resources changed')
    require(one('guest-tool-identities')['argv'] == [tools['container'], 'exec',
        'evocc-r02-'+directory.name, '/usr/bin/sha256sum', *GUEST_TOOLS], 'tool census argv changed')
    require({line.split()[1]: line.split()[0] for line in
        read(one('guest-tool-identities')['stdout']).decode().splitlines()} == m['guest_tools'], 'guest tools changed')

    require([p['id'] for p in r['cases']] == [c['id'] for c in m['cases']], 'changed case population')
    proof_counts, twin_count, samples = 0, 0, 0
    used_timed = set()
    for c, product in zip(m['cases'], r['cases']):
        case = c['id']
        require(product['status'] == 'OBSERVED'
                and [u['source'] for u in product['units']] == c['subject_sources'], 'incomplete subject population')
        require(product['coverage'] == dict(subject_sources=c['subject_sources'],
            reference_sources=[s for s in c['sources'] if s not in c['subject_sources']],
            reference_harness=None if case == 'p0' else c['harness'],
            runtime_libraries='sealed-glibc/libm/GCC-runtime-reference-not-EvoCC'), 'coverage changed')
        folder = root/c['source_root']
        app_includes = [folder, folder/'lib'] if case != 'p0' else []
        flags = ['-std=gnu11', '-O2', '-fPIC', *c['flags']]
        guest_flags = [*flags, *['-I'+guest_path(p) for p in app_includes]]
        includes = [root/m['trees']['sysroot']['path']/p for p in
            ('usr/lib/gcc/aarch64-linux-gnu/14/include', 'usr/include/aarch64-linux-gnu', 'usr/include')]
        host_flags = [*flags, '-nostdinc', *['-I'+str(p) for p in app_includes+includes]]
        for n, source in enumerate(c['sources']):
            guest_command(case+':reference:'+source, ['/usr/bin/gcc-14', *guest_flags, '-c',
                guest_path(folder/source), '-o', guest_path(directory/case/f'ref-{n:02}.o')])
            read(Path(case)/f'ref-{n:02}.o')
        if case != 'p0':
            guest_command(case+':harness', ['/usr/bin/gcc-14', *guest_flags,
                *(['-DUSE_ZSTD'] if case == 'zstd' else []), '-c', guest_path(root/c['harness']),
                '-o', guest_path(directory/case/'harness.o')])
            read(Path(case)/'harness.o')
        else:
            for mode in ('--interpret-c-hir', '--interpret-evoir'):
                row = one(case+':'+mode)
                require(row['argv'] == [tools['compiler'], mode, *host_flags, str(root/c['sources'][0])],
                        'interpreter control changed')
                result(row, dict(exit_code=0, stdout_sha256=digest(b'0\n'), stderr_sha256=digest(b'')))
        reference_objects = [directory/case/f'ref-{n:02}.o' for n in range(len(c['sources']))]
        harness_objects = [] if case == 'p0' else [directory/case/'harness.o']
        dependencies = [reference_objects[n] for n, s in enumerate(c['sources']) if s not in c['subject_sources']]
        dependencies += harness_objects
        guest_command(case+':reference-link', ['/usr/bin/gcc-14',
            *map(guest_path, reference_objects+harness_objects), '-Wl,--build-id=none', '-lm', '-o',
            guest_path(directory/case/'reference')])
        reference_digest = digest(read(Path(case)/'reference'))
        bindings = {'A': {}, 'B': {}}
        for unit in product['units']:
            require(unit['status'] == 'OBSERVED' and set(unit['variants']) == {'A', 'B'}, 'missing unit variant')
            number = c['sources'].index(unit['source'])
            for variant in ('A', 'B'):
                detail = unit['variants'][variant]
                chain = Path(case)/f'{number:02}-{variant}-chain'
                binding = bind_bundle(directory/chain, checker if variant == 'B' else None)
                require(canonical(binding) == canonical(detail['binding']), 'replay differs from retained binding')
                for name in binding['inventory']:
                    read(chain/name)
                bindings[variant][unit['source']] = binding
                proof_counts += binding['audit']['proof_counts']['proof_successes']
                front = one(case+':frontend:'+unit['source']+':'+variant)
                optimizer = ([] if variant == 'A' else ['-Oeq-scalar', '--eq-checker='+tools['checker'],
                    '--eq-checker-sha256='+m['tools']['checker']['sha256'], '--eq-checker-timeout-ms=300000'])
                require(front['argv'] == [tools['compiler'], '--emit=evoir', *host_flags, *optimizer,
                    '--eq-proof-dir='+str(directory/chain), str(folder/unit['source'])], 'frontend controls changed')
                require(detail['frontend_stdout'] == front['stdout']
                        and read(front['stdout']) == read(chain/'output.evo'), 'wrong emitted frontend IR')
                if variant == 'B':
                    record = load(chain/'chain.json')
                    require(canonical(detail['default_caps']) == canonical(chain_census(record)),
                            'default cap summary forged')
                    require(canonical(detail['chain_resources']) == canonical(load(chain/'resources.json')),
                            'chain resource summary forged')
                    require(record['input_digest'] == bindings['A'][unit['source']]['ir_digest'],
                            'A/B lowering mismatch')
                    for tx in record['transactions']:
                        trace = chain/f"{tx['index']:03}.policy.json"
                        read(trace)
                        selected_pairs(directory/trace, tx, record['policy']['cost_weights'])
                for profile in ('checked', 'unchecked'):
                    native = detail['native'][profile]
                    obj = read(Path(case)/native['object'])
                    require(canonical(inspect_elf(obj)) == canonical(native['metrics']), 'object metrics forged')
                    backend = one(case+':backend:'+unit['source']+':'+variant+':'+profile)
                    expected_argv = [str((root/m['tools']['ir_backend']['path']).resolve()), 'linux-aarch64',
                        binding['ir_digest'], str(directory/chain/'output.evo'), '--ub-checks='+profile]
                    assembly = read(backend['stdout'])
                    require(backend['argv'] == expected_argv and digest(assembly) == native['assembly_digest']
                            and assembly.startswith(('// evocc-native-ub-checks='+profile+'\n').encode()),
                            'backend profile/input mismatch')
                    require(native['object'] == f'{number:02}-{variant}-{profile}.o'
                            and native['native_ub_profile'] == profile
                            and native['ub_brk_sites'] == len(re.findall(
                                rb'\bbrk #(?:300[2456]|3063)\b', assembly)), 'UB profile metrics changed')
                    guest_command(case+':assemble:'+unit['source']+':'+variant+':'+profile,
                        ['/usr/bin/gcc-14', '-c', '-x', 'assembler', guest_path(directory/backend['stdout']),
                         '-o', guest_path(directory/case/native['object'])])
        for variant in ('A', 'B'):
            common = composite_binding(bindings[variant])
            identities = {}
            for profile in ('checked', 'unchecked'):
                v = product['variants'][variant][profile]
                executable = read(Path(case)/v['executable'])
                require(v['executable'] == variant+'-'+profile and digest(executable) == v['executable_digest']
                        and canonical(inspect_elf(executable)) == canonical(v['metrics']), 'native artifact mismatch')
                require(len(read(Path(case)/(variant+'-'+profile+'.stripped'))) == v['stripped_file_bytes'],
                        'stripped metric mismatch')
                subject_objects = [directory/case/unit['variants'][variant]['native'][profile]['object']
                                   for unit in product['units']]
                guest_command(case+':link:'+variant+':'+profile, ['/usr/bin/gcc-14',
                    *map(guest_path, subject_objects+dependencies), '-Wl,--build-id=none', '-lm', '-o',
                    guest_path(directory/case/v['executable'])])
                guest_command(case+':strip:'+variant+':'+profile, ['/usr/bin/strip', '--strip-all', '-o',
                    guest_path(directory/case/(variant+'-'+profile+'.stripped')),
                    guest_path(directory/case/v['executable'])])
                require(len(v['inputs']) == len(c['modes']), 'missing input admission')
                identities[profile] = []
                for mode, entry in zip(c['modes'], v['inputs']):
                    input_digest = digest(canonical(dict(harness=m['files'][c['harness']], argv=mode)))
                    identity = native_identity(common, profile, 'linux-aarch64', m['tools']['ir_backend']['sha256'],
                                               v['executable_digest'], input_digest)
                    require(identity == entry['identity'] and entry['argv'] == mode, 'input identity mismatch')
                    identities[profile].append(identity)
            for i, mode in enumerate(c['modes']):
                reference_row = guest_command(case+':reference-result:'+str(mode),
                    [guest_path(directory/case/'reference'), *mode])
                result(reference_row, c['result'])
                require(reference_row['guest_executable_digest'] == reference_digest, 'wrong reference binary')
                checked = one(case+':checked-admission:'+variant+':'+str(mode))
                unchecked = one(case+':unchecked-admission:'+variant+':'+str(mode))
                result(checked, c['result']); result(unchecked, c['result'])
                for profile, row in (('checked', checked), ('unchecked', unchecked)):
                    v = product['variants'][variant][profile]
                    child = guests[row['guest_receipt']][1]
                    require(child['argv'] == ['/runwork/'+case+'/'+v['executable'], *mode]
                            and row['guest_executable_digest'] == v['executable_digest']
                            and row['native_identity'] == identities[profile][i]['native_identity'],
                            'admission executed the wrong binary/input')
                require(r['attempts'].index(checked) < r['attempts'].index(unchecked),
                        'unchecked ran before its checked admission')
                admission = admit_unchecked(identities['checked'][i], identities['unchecked'][i], checked, c['result'])
                for profile in ('checked', 'unchecked'):
                    require(product['variants'][variant][profile]['inputs'][i]['checked_twin'] == admission,
                            'checked-twin receipt mismatch')
                twin_count += 1
        expected_comparisons = [(mode, profile, kind) for mode in c['modes']
                               for profile in ('checked', 'unchecked') for kind in ('AA', 'AB')]
        require([(v['argv'], v['profile'], v['kind']) for v in product['comparisons']] == expected_comparisons,
                'comparison population mismatch')
        for comparison in product['comparisons']:
            expected_pairs, expected_samples = [], []
            profile, kind, mode = comparison['profile'], comparison['kind'], comparison['argv']
            for repeat in range(m['protocol']['warmups']):
                for label in ('A0', 'A1'):
                    stage = case+f':warmup:{profile}:{kind}:{repeat}:{label}:'+str(mode)
                    row = one(stage)
                    result(row, c['result'])
                    used_timed.add(stage)
                    variant = 'B' if label == 'A1' and kind == 'AB' else 'A'
                    v = product['variants'][variant][profile]
                    child = guests[row['guest_receipt']][1]
                    require(child['argv'] == ['/runwork/'+case+'/'+v['executable'], *mode]
                            and row['guest_executable_digest'] == v['executable_digest'], 'warmup mismatch')
            for pair, order in enumerate(pair_order(m['protocol']['pairs'], m['protocol']['seed'])):
                values = {}
                for label in order:
                    sample = comparison['samples'][len(expected_samples)]
                    row, child = guests[sample['guest_receipt']]
                    variant = 'B' if label == 'A1' and kind == 'AB' else 'A'
                    executable = product['variants'][variant][profile]
                    result(row, c['result'])
                    require(row['stage'] == case+f':sample:{profile}:{kind}:{pair}:{label}:'+str(mode)
                            and child['argv'] == ['/runwork/'+case+'/'+executable['executable'], *mode]
                            and row['guest_executable_digest'] == executable['executable_digest'], 'sample identity mismatch')
                    require(row['stage'] not in used_timed, 'sample reused')
                    used_timed.add(row['stage'])
                    expected_samples.append(dict(pair=pair, label=label, variant=variant,
                        guest_receipt=row['guest_receipt'], guest_wall_ns=row['guest_wall_ns'],
                        guest_resources=row['guest_resources'], executable_digest=executable['executable_digest']))
                    values[label] = row['guest_wall_ns']
                expected_pairs.append(values)
            stats = pilot_statistics(expected_pairs, m['protocol'], sum(len(c['modes'])*4 for c in m['cases']))
            require(canonical(comparison['samples']) == canonical(expected_samples)
                    and comparison['pairs'] == expected_pairs and canonical(comparison['statistics']) == canonical(stats),
                    'sample/summary population changed')
            samples += len(expected_samples)
    require(used_timed == {s for s in stages if ':sample:' in s or ':warmup:' in s},
            'unaccounted timed sample or warmup')
    preflight = audit_preflight(m, r, directory, root, read, load, one, result, guest_command, guest_path, checker, guests)
    for name in tuple(observed):
        read(name)
    return dict(schema='evocc-r02-linux-pilot-audit-v1', status='CAPTURE_CONSISTENT',
                manifest_digest=expected_manifest_digest, proof_successes=proof_counts,
                checked_twins=twin_count, timed_executions=samples,
                preflight=preflight,
                execution_telemetry='TRUSTED_COLLECTOR_NOT_EXECUTION_ATTESTATION',
                performance='PILOT_NOT_FINAL_Q2')


def audit_preflight(m, r, directory, root, read, load, one, result, guest_command, guest_path, checker, guests):
    """Recompute the bounded census; LLVM and producer traces are not proof authorities."""
    p = r['preflight']
    bounds = m['preflight']
    require(p['schema'] == 'evocc-r02-preflight-v1' and p['status'] == 'BOUNDED_PREFLIGHT_COMPLETED'
            and p['bounds'] == bounds and p['campaign_admission'] == 'NO_AI_CAMPAIGN_AUTHORIZED_BY_THIS_CAPTURE',
            'preflight incomplete or campaign scope promoted')
    require(canonical(p) == canonical(load('preflight/report.json')), 'preflight report changed')
    tools = {k: str((root/v['path']).resolve()) for k, v in m['tools'].items()}
    units, comparisons, proofs, caps = [], set(), [], []
    for case, product in zip(m['cases'], r['cases']):
        for unit in product['units']:
            n = case['sources'].index(unit['source'])
            chain = directory/case['id']/f'{n:02}-B-chain'
            record = load(chain.relative_to(directory)/'chain.json')
            caps.append(dict(case=case['id'], source=unit['source'], **chain_census(record)))
            units.append(dict(case=case, source=unit['source'], number=n,
                input=directory/case['id']/f'{n:02}-A-chain/output.evo',
                baseline=chain/'output.evo', record=record))
            for tx in record['transactions']:
                trace = selected_pairs(chain/f"{tx['index']:03}.policy.json", tx, record['policy']['cost_weights'])
                comparisons.update(tuple(pair) for pair in trace['comparisons'])
                proofs.append((f"{case['id']}/{unit['source']}/{tx['index']}",
                    read(chain.relative_to(directory)/f"{tx['index']:03}.proof.evp").decode()))
    require(canonical(caps) == canonical(p['default_caps']) == canonical(load('preflight/default-caps.json')),
            'default cap table changed')
    inventory, patterns = [], {}
    for index, u in enumerate(units):
        for phase in ('input', 'baseline'):
            source = u[phase]
            row = one(f'preflight:census:{index}:{phase}')
            input_digest = digest(read(source.relative_to(directory)))
            require(row['argv'] == [tools['census'], 'census', input_digest, str(source)], 'census command changed')
            data = load(row['stdout'])
            require(data['input_digest'] == input_digest, 'census digest changed')
            tx = u['record']['transactions'][0 if phase == 'input' else -1]
            if phase == 'input' or tx['input_digest'] == tx['output_digest']:
                require(data['eligible'] == tx['roots']['eligible'], 'census denominator changed')
            entry = dict(case=u['case']['id'], source=u['source'], phase=phase,
                eligible=data['eligible'], excluded=data['excluded'], artifact=row['stdout'])
            inventory.append(entry)
            if phase == 'baseline':
                diagnostic = diagnostic_expressions(data, bounds['diagnostic_nodes'])
                entry['mapping_exclusions'] = diagnostic['mapping_exclusions']
                for ident, pattern in diagnostic['expressions'].items():
                    item = patterns.setdefault(ident, dict(**pattern, units=[]))
                    item['units'].append([u['case']['id'], u['source']])
    require(canonical(inventory) == canonical(p['inventory']) == canonical(load('preflight/inventory.json')),
            'eligible/mapping inventory changed')
    selected = sorted(patterns)[:bounds['diagnostic_patterns']]
    oracle = dict(patterns=len(patterns), selected=len(selected), out_of_budget=len(patterns)-len(selected),
        authority='OPPORTUNITY_ONLY_NOT_EQUIVALENCE',
        mapping='INITIALIZED_BITS_NO_OVERFLOW_FLAGS_CONSTANT_IN_RANGE_SHIFTS_ONLY', residuals=[])
    if selected:
        source = directory/'preflight/opportunities.ll'
        require(read('preflight/opportunities.ll').decode() ==
            '\n'.join(llvm_function(i, patterns[i]['expression']) for i in selected), 'translator input changed')
        output = directory/'preflight/instcombine.ll'
        guest_command('preflight:instcombine', ['/usr/bin/opt-19', '-passes=instcombine', '-S',
            guest_path(source), '-o', guest_path(output)])
        after = llvm_functions(read('preflight/instcombine.ll').decode())
        require(set(after) == set(selected), 'opportunity function population changed')
        for ident in selected:
            count = len(re.findall(r'^\s*%[^\n=]+\s*=', after[ident], re.M))
            if count < patterns[ident]['operator_nodes']:
                oracle['residuals'].append(dict(id=ident, **patterns[ident], reference_nodes=count,
                    reference_body=after[ident], qualification='UNPROVED_NOT_INSTALLABLE'))
    require(canonical(oracle) == canonical(p['oracle']) == canonical(load('preflight/oracle.json')),
            'opportunity output/qualification changed')
    shortcut = shortcut_census(proofs)
    require(canonical(shortcut) == canonical(p['shortcut']) == canonical(load('preflight/shortcut.json')),
            'shortcut count or unmeasured savings promoted')

    survivors, cells = [], []
    for size in bounds['rule_cells']:
        cell = enumerate_rule_cell(size, syntax_cap=bounds['syntax_visits_per_cell'])
        require(canonical(cell) == canonical(load(f'preflight/rule-cell-{size}.json')), 'rule enumeration changed')
        survivors.extend(cell['survivors'])
        cells.append({k: v for k, v in cell.items() if k != 'survivors'})
    rule = p['rule']
    candidates = {s['candidate_digest']: s for s in survivors}
    useful, unresolved = {}, []
    for residual in oracle['residuals']:
        candidate = residual_candidate(residual)
        if candidate.get('candidate_digest') in candidates:
            useful.setdefault(candidate['candidate_digest'], []).append(residual['id'])
        else:
            unresolved.append(dict(pattern=residual['id'], reason=candidate.get('outside_grammar',
                'NO_ENUMERATED_FUZZ_SURVIVOR')))
    controls = [(('add', ('x',), ('c', 0)), ('x',)), (('mul', ('x',), ('c', 1)), ('x',))]
    control_ids = [digest(canonical([lhs, rhs])) for lhs, rhs in controls]
    identities = list(sorted(useful)) + sorted(set(control_ids)-set(useful))
    require(canonical(rule['cells']) == canonical(cells) and rule['fuzz_survivors'] == len(survivors)
        and rule['residual_candidates'] == useful and rule['unresolved_residuals'] == unresolved
        and rule['ai_budget'] == bounds['ai_budget'] and rule['enumeration_floor'] ==
            bounds['ai_budget']*bounds['enumeration_multiplier']
        and rule['omitted_relevant_templates'] == max(0, len(identities)-bounds['proof_attempts'])
        and [proof['candidate'] for proof in rule['proofs']] == identities[:bounds['proof_attempts']],
        'rule qualification population changed')
    proved, unknown = 0, 0
    for proof in rule['proofs']:
        identity = proof['candidate']
        candidate = candidates[identity]
        source = directory/'preflight'/(identity+'.lean')
        require(read(source.relative_to(directory)).decode() == lean_candidate(candidate['lhs'], candidate['rhs']),
                'untrusted template/source substituted')
        row = one('preflight:template:'+identity)
        require(row['argv'] == [tools['collector_python'], str(root/'tools/ladder/eqsat_template.py'),
            tools['lean'], str(root/'lean'), str(source)] and proof['attempt_stdout'] == row['stdout']
            and proof['template'] == bounds['template'], 'template command changed')
        output = read(row['stdout']).decode()
        accepted = (row['status'] == 'COMPLETED' and row['exit_code'] == 0 and
            'EvoIR.preflight_sound' in output and 'RuleCovered preflight_candidate' in output
            and 'sorryAx' not in output and 'ofReduceBool' not in output)
        require(proof['status'] == ('PROVED_TEMPLATE_ONLY_NOT_INSTALLED' if accepted else 'UNKNOWN'),
                'unknown proof promoted to success')
        if accepted:
            require(proof['output'] == row['stdout'], 'wrong proof output')
            proved += 1
        else:
            require(type(proof.get('reason')) is str and proof['reason'], 'unknown proof lacks reason')
            unknown += 1
    require(rule['unattempted_templates'] == len(survivors)-len(rule['proofs'])
        and rule['full_suite_evaluations'] == 0 and rule['verdict'] == 'INCONCLUSIVE'
        and rule['reason'] == 'NO_BOUNDED_NEW_RULE_INSTALLATION_OR_UTILITY_PROOF'
        and canonical(rule) == canonical(load('preflight/rule.json')), 'rule claim exceeds qualification')

    space = enumerate_policies(comparisons, bounds['signature_visits'])
    require(canonical(space) == canonical(p['policy']['space']) ==
            canonical(load('preflight/policy-space.json')), 'policy space/coverage changed')
    expected = [(candidate['weights'], schedule) for candidate in space['candidates'] for schedule in bounds['schedules']]
    evaluations = p['policy']['evaluations']
    require(len(evaluations) == min(len(expected), bounds['whole_suite_evaluations']), 'policy evaluation population changed')
    for index, (evaluation, (weights, schedule)) in enumerate(zip(evaluations, expected)):
        require(evaluation['status'] == 'OBSERVED' and evaluation['weights'] == weights
                and evaluation['max_transactions'] == schedule and len(evaluation['units']) == len(units),
                'policy evaluation incomplete/changed')
        folder = directory/'preflight'/f'policy-{index:03}'
        label = folder.name
        bindings, objects, hits = {}, {}, []
        policy_weights = ','.join(f'{axis}={weight}' for axis, weight in zip(AXES, weights))
        for n, (u, detail) in enumerate(zip(units, evaluation['units'])):
            chain = folder/f'{n:02}-chain'
            binding = bind_bundle(chain, checker)
            for name in binding['inventory']:
                read(chain.relative_to(directory)/name)
            require(detail['case'] == u['case']['id'] and detail['source'] == u['source']
                    and canonical(detail['binding']) == canonical(binding), 'policy unit/replay changed')
            row = one(f'preflight:{label}:chain:{n}')
            require(row['argv'] == [tools['census'], 'policy', digest(u['input'].read_bytes()), str(u['input']),
                str(checker), m['tools']['checker']['sha256'], policy_weights, str(schedule), str(chain)]
                and read(row['stdout']) == read(chain.relative_to(directory)/'output.evo'), 'policy chain command changed')
            record = load(chain.relative_to(directory)/'chain.json')
            require(record['input_digest'] == digest(u['input'].read_bytes()) and
                    record['policy']['cost_weights'] == weights and record['policy']['schedule']['max_transactions'] == schedule,
                    'policy identity/input changed')
            new_pairs = set()
            for tx in record['transactions']:
                trace_path = chain/f"{tx['index']:03}.policy.json"
                read(trace_path.relative_to(directory))
                trace = selected_pairs(trace_path, tx, weights)
                new_pairs.update(tuple(pair) for pair in trace['comparisons'] if tuple(pair) not in comparisons)
                hits.extend(tx['budget_hits'])
            require(canonical(detail['new_comparisons']) == canonical(sorted(new_pairs, key=str))
                and detail['scoped_counters'] == [tx['scoped_counters'] for tx in record['transactions']]
                and detail['resources'] == load(chain.relative_to(directory)/'resources.json'), 'policy telemetry changed')
            bindings.setdefault(u['case']['id'], {})[u['source']] = binding
            for profile in ('checked', 'unchecked'):
                native = detail['native'][profile]
                obj = folder/f'{n:02}-{profile}.o'
                metrics = inspect_elf(read(obj.relative_to(directory)))
                require(native == dict(object=obj.name, metrics=metrics, code_digest=section_identity(metrics),
                    assembly_digest=native['assembly_digest'], native_ub_profile=profile), 'policy object metric changed')
                backend = one(f'preflight:{label}:asm:{n}:{profile}')
                assembly = read(backend['stdout'])
                require(backend['argv'] == [tools['ir_backend'], 'linux-aarch64', binding['ir_digest'],
                    str(chain/'output.evo'), '--ub-checks='+profile]
                    and digest(assembly) == native['assembly_digest']
                    and assembly.startswith(('// evocc-native-ub-checks='+profile+'\n').encode()),
                    'policy native fan-out changed')
                guest_command(f'preflight:{label}:assemble:{n}:{profile}', ['/usr/bin/gcc-14', '-c', '-x',
                    'assembler', guest_path(directory/backend['stdout']), '-o', guest_path(obj)])
                objects.setdefault((u['case']['id'], profile), []).append(obj)
        require(evaluation['cap_hits'] == hits, 'policy cap denominator changed')
        require([program['case'] for program in evaluation['programs']] == [c['id'] for c in m['cases']],
                'policy program population changed')
        for case, program in zip(m['cases'], evaluation['programs']):
            dependencies = [directory/case['id']/f'ref-{n:02}.o' for n, source in enumerate(case['sources'])
                            if source not in case['subject_sources']]
            if case['id'] != 'p0':
                dependencies.append(directory/case['id']/'harness.o')
            common = composite_binding(bindings[case['id']])
            for profile in ('checked', 'unchecked'):
                exe = folder/(case['id']+'-'+profile)
                metrics = inspect_elf(read(exe.relative_to(directory)))
                require(program['profiles'][profile] == dict(executable=exe.name,
                    executable_digest=metrics['file_digest'], metrics=metrics, code_digest=section_identity(metrics)),
                    'policy program metrics changed')
                guest_command(f'preflight:{label}:link:{case["id"]}:{profile}', ['/usr/bin/gcc-14',
                    *map(guest_path, objects[case['id'], profile]+dependencies), '-Wl,--build-id=none', '-lm',
                    '-o', guest_path(exe)])
            require(len(program['inputs']) == len(case['modes']), 'policy input population changed')
            for mode, entry in zip(case['modes'], program['inputs']):
                input_digest = digest(canonical(dict(harness=m['files'][case['harness']], argv=mode)))
                identities = {profile: native_identity(common, profile, 'linux-aarch64',
                    m['tools']['ir_backend']['sha256'], program['profiles'][profile]['executable_digest'], input_digest)
                    for profile in ('checked', 'unchecked')}
                checked = guest_command(f'preflight:{label}:checked:{case["id"]}:{mode}',
                    [guest_path(folder/program['profiles']['checked']['executable']), *mode])
                unchecked = guest_command(f'preflight:{label}:unchecked:{case["id"]}:{mode}',
                    [guest_path(folder/program['profiles']['unchecked']['executable']), *mode])
                result(checked, case['result']); result(unchecked, case['result'])
                for profile, row in (('checked', checked), ('unchecked', unchecked)):
                    require(row['native_identity'] == identities[profile]['native_identity']
                        and row['guest_executable_digest'] == program['profiles'][profile]['executable_digest'],
                        'policy admission binary/identity changed')
                require(r['attempts'].index(checked) < r['attempts'].index(unchecked), 'unchecked policy ran first')
                admission = admit_unchecked(identities['checked'], identities['unchecked'], checked, case['result'])
                require(entry == dict(argv=mode, identities=identities, checked_twin=admission), 'policy twin binding changed')
    completed = space['status'] == 'ENUM_COMPLETE' and len(evaluations) == len(expected)
    new_pairs = sorted({tuple(pair) for e in evaluations for u in e['units'] for pair in u['new_comparisons']}, key=str)
    code_classes = len({digest(canonical([dict(case=program['case'],
        profiles={k: v['code_digest'] for k, v in program['profiles'].items()}) for program in e['programs']]))
        for e in evaluations})
    require(p['policy']['whole_suite_evaluations'] == len(evaluations)
        and canonical(p['policy']['observed_new_comparisons']) == canonical(new_pairs)
        and p['policy']['code_classes'] == code_classes
        and p['policy']['verdict'] == ('FINITE_ENUMERATION_SUFFICIENT' if completed and not new_pairs else 'INCONCLUSIVE')
        and p['policy']['native_fitness'] == 'NOT_USED'
        and p['policy']['scope'] == 'FROZEN_CONSULTED_PAIRS_AND_SCHEDULE_GRID_ONLY', 'policy verdict promoted')
    return dict(status='BOUNDED_PREFLIGHT_CONSISTENT', policy_evaluations=len(evaluations),
        policy_verdict=p['policy']['verdict'], rule_verdict=rule['verdict'],
        shortcut_verdict=shortcut['verdict'], template_proofs=proved, template_unknowns=unknown,
        enumeration_authority='SYNTACTIC_ONLY_NOT_SEMANTIC_PROOF', ai_campaign='NOT_ADMITTED')


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('directory', type=Path)
    p.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    p.add_argument('--manifest-sha256', required=True)
    args = p.parse_args()
    try:
        print(json.dumps(audit_pilot(args.directory, args.root, args.manifest_sha256), sort_keys=True, indent=2))
    except (OSError, ValueError, KeyError, TypeError, IndexError) as error:
        p.exit(1, f'pilot audit rejected: {error}\n')
