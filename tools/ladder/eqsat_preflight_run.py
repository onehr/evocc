"""Finite R02b preflight inside the existing, frozen pilot capture.

Rule utility hints never reach native code. Policy evaluations use the ordinary
shared proof chain, common-IR twins and original frozen workload inputs.
"""
from __future__ import annotations

import json
from pathlib import Path
import re

from .eqsat_chain import AXES, digest, require, unique
from .eqsat_elf import inspect_elf
from .eqsat_opportunities import (diagnostic_expressions, enumerate_rule_cell, lean_candidate,
                                  llvm_function, llvm_functions, residual_candidate)
from .eqsat_preflight import selected_pairs, enumerate_policies
from .eqsat_twins import bind_bundle, canonical, native_identity, admit_unchecked

CONTRACT = dict(schema='evocc-preflight-bounds-v1', schedules=[1, 8],
    signature_visits=1024, whole_suite_evaluations=32,
    diagnostic_nodes=3, diagnostic_patterns=4096, rule_cells=[1, 2],
    syntax_visits_per_cell=6000, proof_attempts=16, ai_budget=16, enumeration_multiplier=1,
    evaluation_unit='ONE_CANONICAL_CANDIDATE_ON_FULL_FROZEN_SUITE',
    rule_grammar='ORDERED_TREE_ONE_VARIABLE_CONST_0_1_SIX_BITWISE_ARITHMETIC_OPS_RHS_LEAF',
    template='UNIVERSAL_RULE_ONE_SIMP_V1', template_heartbeats=200000,
    template_selection='OBSERVED_RESIDUALS_THEN_ADD_ZERO_AND_MUL_ONE_CONTROLS_DIGEST_ORDER',
    stop='FROZEN_ORDER_NO_RETRY_NO_ADAPTIVE_NATIVE_FITNESS',
    new_rule_installation='NOT_AUTHORIZED_BY_OPPORTUNITY_ORACLE')


def section_identity(metrics):
    # Object filenames, debug and symbol-table strings are not machine-code
    # materialization. Retain allocatable and relocation section identities.
    return digest(canonical([r for r in metrics['sections'] if r['flags'] & 2 or r['type'] in (4, 9)]))


def shortcut_census(proofs):
    compositions, regions, multi = {}, 0, 0
    for source, text in proofs:
        for body in re.findall(r'^region .*?\n(.*?)^endregion$', text, re.M | re.S):
            regions += 1
            rules = re.findall(r'^rewrite \d+ (\S+) ', body, re.M)
            if len(rules) < 2:
                continue
            multi += 1
            key = tuple(rules)
            entry = compositions.setdefault(key, dict(rules=rules, occurrences=0,
                expansion='EXISTING_REPLAYED_EVP_EVENTS_WITH_ORIGINAL_BEFORE_PREMISES',
                shortcut_fuel_saving='NOT_MEASURED', shortcut_matcher_cost='NOT_MEASURED', sources=[]))
            entry['occurrences'] += 1
            entry['sources'].append(source)
    return dict(regions=regions, multi_rewrite_regions=multi, compositions=list(compositions.values()),
        verdict='INCONCLUSIVE' if multi else 'NO_OBSERVABLE_EFFECT',
        reason='NO_SHORTCUT_INTEGRATION_COST_MEASUREMENT' if multi else 'NO_MULTI_REWRITE_PROOF_IN_THIS_POPULATION',
        scope='OBSERVED_VERIFIED_BASELINE_COMPOSITIONS_NOT_ALL_POSSIBLE_SHORTCUTS')


def run_preflight(m, receipt, directory, root, tools, command, guest, guest_path, pin, save):
    from .eqsat_pilot import composite_binding
    bounds = m['preflight']
    require(bounds == CONTRACT, 'unregistered preflight bounds')
    work = directory/'preflight'
    work.mkdir()
    report = dict(schema='evocc-r02-preflight-v1', status='STARTED', bounds=bounds,
                  default_caps=[], policy={}, rule={}, shortcut={}, attempts=[])
    receipt['preflight'] = report

    def artifact(name, value):
        path = work/name
        path.write_bytes(canonical(value)+b'\n')
        pin(path)
        return path

    units, comparisons, proofs = [], set(), []
    for case, product in zip(m['cases'], receipt['cases']):
        if product['status'] != 'OBSERVED':
            report['default_caps'].append(dict(case=case['id'], status='UNAVAILABLE', reason=product.get('error')))
            continue
        for unit in product['units']:
            n = case['sources'].index(unit['source'])
            folder = directory/case['id']
            chain = folder/f'{n:02}-B-chain'
            record = json.loads((chain/'chain.json').read_bytes(), object_pairs_hook=unique)
            report['default_caps'].append(dict(case=case['id'], source=unit['source'],
                **unit['variants']['B']['default_caps']))
            units.append(dict(case=case, source=unit['source'], number=n, folder=folder,
                              input=folder/f'{n:02}-A-chain/output.evo', baseline=chain/'output.evo',
                              record=record))
            for tx in record['transactions']:
                trace = selected_pairs(chain/f"{tx['index']:03}.policy.json", tx, record['policy']['cost_weights'])
                comparisons.update(tuple(p) for p in trace['comparisons'])
                proofs.append((f"{case['id']}/{unit['source']}/{tx['index']}",
                               (chain/f"{tx['index']:03}.proof.evp").read_text()))
    # This immutable table is the first result, before policy/rule exploration.
    artifact('default-caps.json', report['default_caps'])
    save()
    if len(units) != sum(len(c['subject_sources']) for c in m['cases']):
        report.update(status='INCOMPLETE', reason='BASELINE_POPULATION_NOT_ADMITTED')
        for track in ('policy', 'rule', 'shortcut'):
            report[track] = dict(verdict='INCONCLUSIVE', reason='BASELINE_POPULATION_NOT_ADMITTED')
        artifact('report.json', report)
        save()
        return

    # Read-only producer census before/after B, cross-checked against the chain
    # denominator when the last transaction is unchanged. Never proof authority.
    patterns = {}
    inventory = []
    for index, u in enumerate(units):
        for phase in ('input', 'baseline'):
            source = u[phase]
            row = command(f'preflight:census:{index}:{phase}',
                [tools['census'], 'census', digest(source.read_bytes()), source])
            data = json.loads((directory/row['stdout']).read_bytes(), object_pairs_hook=unique)
            require(data['input_digest'] == digest(source.read_bytes()), 'census input mismatch')
            tx = u['record']['transactions'][0 if phase == 'input' else -1]
            if phase == 'input' or tx['input_digest'] == tx['output_digest']:
                require(data['eligible'] == tx['roots']['eligible'], 'producer census denominator mismatch')
            inventory.append(dict(case=u['case']['id'], source=u['source'], phase=phase,
                eligible=data['eligible'], excluded=data['excluded'], artifact=row['stdout']))
            if phase == 'baseline':
                diagnostic = diagnostic_expressions(data, bounds['diagnostic_nodes'])
                inventory[-1]['mapping_exclusions'] = diagnostic['mapping_exclusions']
                for ident, pattern in diagnostic['expressions'].items():
                    item = patterns.setdefault(ident, dict(**pattern, units=[]))
                    item['units'].append([u['case']['id'], u['source']])
    report['inventory'] = inventory
    artifact('inventory.json', inventory)
    selected = sorted(patterns)[:bounds['diagnostic_patterns']]
    report['oracle'] = dict(patterns=len(patterns), selected=len(selected),
        out_of_budget=len(patterns)-len(selected), authority='OPPORTUNITY_ONLY_NOT_EQUIVALENCE',
        mapping='INITIALIZED_BITS_NO_OVERFLOW_FLAGS_CONSTANT_IN_RANGE_SHIFTS_ONLY')
    if selected:
        source = work/'opportunities.ll'
        source.write_text('\n'.join(llvm_function(i, patterns[i]['expression']) for i in selected))
        pin(source)
        output = work/'instcombine.ll'
        guest('preflight:instcombine', ['/usr/bin/opt-19', '-passes=instcombine', '-S',
            guest_path(source), '-o', guest_path(output)])
        pin(output)
        after = llvm_functions(output.read_text())
        require(set(after) == set(selected), 'opportunity oracle dropped a function')
        misses = []
        for ident in selected:
            count = len(re.findall(r'^\s*%[^\n=]+\s*=', after[ident], re.M))
            if count < patterns[ident]['operator_nodes']:
                misses.append(dict(id=ident, **patterns[ident], reference_nodes=count,
                    reference_body=after[ident], qualification='UNPROVED_NOT_INSTALLABLE'))
        report['oracle']['residuals'] = misses
    else:
        report['oracle']['residuals'] = []
    artifact('oracle.json', report['oracle'])
    report['shortcut'] = shortcut_census(proofs)
    artifact('shortcut.json', report['shortcut'])
    save()

    # One finite rule grammar. Syntactic/fuzz counts are not full-suite or proof
    # evaluations. Unresolved external patterns prevent a no-opportunity claim.
    survivors, cells = [], []
    for size in bounds['rule_cells']:
        cell = enumerate_rule_cell(size, syntax_cap=bounds['syntax_visits_per_cell'])
        survivors.extend(cell['survivors'])
        cells.append({k: v for k, v in cell.items() if k != 'survivors'})
        artifact(f'rule-cell-{size}.json', cell)
    report['rule'] = dict(cells=cells, fuzz_survivors=len(survivors), proofs=[],
        full_suite_evaluations=0, ai_budget=bounds['ai_budget'],
        enumeration_floor=bounds['ai_budget']*bounds['enumeration_multiplier'],
        verdict='INCONCLUSIVE', reason='NO_BOUNDED_NEW_RULE_INSTALLATION_OR_UTILITY_PROOF',
        scope='FIXED_ONE_VARIABLE_GRAMMAR_NOT_COMPLETE_P0_RULE_SPACE')
    candidates = {s['candidate_digest']: s for s in survivors}
    useful = {}
    unresolved = []
    for residual in report['oracle']['residuals']:
        candidate = residual_candidate(residual)
        if candidate.get('candidate_digest') in candidates:
            useful.setdefault(candidate['candidate_digest'], []).append(residual['id'])
        else:
            unresolved.append(dict(pattern=residual['id'], reason=candidate.get('outside_grammar',
                'NO_ENUMERATED_FUZZ_SURVIVOR')))
    controls = [dict(lhs=('add', ('x',), ('c', 0)), rhs=('x',)),
                dict(lhs=('mul', ('x',), ('c', 1)), rhs=('x',))]
    control_ids = [digest(canonical([c['lhs'], c['rhs']])) for c in controls]
    selected_candidates = list(sorted(useful)) + sorted(set(control_ids)-set(useful))
    report['rule'].update(residual_candidates=useful, unresolved_residuals=unresolved,
        omitted_relevant_templates=max(0, len(selected_candidates)-bounds['proof_attempts']))
    # Template attempts are reserved for observed opportunities and two fixed
    # admission controls. Fuzz survivors outside that population remain unproved.
    for identity in selected_candidates[:bounds['proof_attempts']]:
        candidate = candidates[identity]
        source = work/(identity+'.lean')
        source.write_text(lean_candidate(candidate['lhs'], candidate['rhs']))
        pin(source)
        proof = dict(candidate=identity, status='UNKNOWN', template=bounds['template'])
        report['rule']['proofs'].append(proof)
        try:
            row = command('preflight:template:'+identity,
                [tools['collector_python'], root/'tools/ladder/eqsat_template.py',
                 tools['lean'], root/'lean', source])
            output = (directory/row['stdout']).read_text()
            require('EvoIR.preflight_sound' in output and 'RuleCovered preflight_candidate' in output
                    and 'sorryAx' not in output and 'ofReduceBool' not in output,
                    'template output lacks exact theorem/axiom admission')
            proof.update(status='PROVED_TEMPLATE_ONLY_NOT_INSTALLED', output=row['stdout'])
        except (OSError, ValueError) as error:
            proof['reason'] = str(error)
        stage = 'preflight:template:'+identity
        attempt = next((a for a in reversed(receipt['attempts']) if a['stage'] == stage), None)
        if attempt is not None:
            proof['attempt_stdout'] = attempt['stdout']
        save()
    report['rule']['unattempted_templates'] = len(survivors)-len(report['rule']['proofs'])
    artifact('rule.json', report['rule'])

    policy_inventory = enumerate_policies(comparisons, bounds['signature_visits'])
    artifact('policy-space.json', policy_inventory)
    report['policy'] = dict(space=policy_inventory, evaluations=[], observed_new_comparisons=[],
        objective='ALLOCATABLE_AND_RELOCATION_SECTION_IDENTITY_AND_SIZE',
        native_fitness='NOT_USED', verdict='INCONCLUSIVE')
    expected_evaluations = len(policy_inventory['candidates'])*len(bounds['schedules'])
    for candidate in policy_inventory['candidates']:
        for transactions in bounds['schedules']:
            if len(report['policy']['evaluations']) == bounds['whole_suite_evaluations']:
                break
            evaluation = dict(weights=candidate['weights'], max_transactions=transactions,
                              status='STARTED', units=[], programs=[], cap_hits=[])
            index = len(report['policy']['evaluations'])
            report['policy']['evaluations'].append(evaluation)
            folder = work/f'policy-{index:03}'
            folder.mkdir()
            try:
                evaluate_policy(evaluation, units, folder, m, directory, tools, command,
                                guest, guest_path, pin, comparisons, composite_binding)
                evaluation['status'] = 'OBSERVED'
            except (OSError, ValueError) as error:
                evaluation.update(status='FAILED', reason=str(error))
                save()
                break
            save()
        if any(e['status'] == 'FAILED' for e in report['policy']['evaluations']):
            break
    completed = (policy_inventory['status'] == 'ENUM_COMPLETE'
        and len(report['policy']['evaluations']) == expected_evaluations
        and all(e['status'] == 'OBSERVED' for e in report['policy']['evaluations']))
    new_pairs = sorted({tuple(p) for e in report['policy']['evaluations']
                        for u in e['units'] for p in u.get('new_comparisons', [])}, key=str)
    report['policy']['observed_new_comparisons'] = new_pairs
    report['policy']['whole_suite_evaluations'] = len(report['policy']['evaluations'])
    report['policy']['verdict'] = 'FINITE_ENUMERATION_SUFFICIENT' if completed and not new_pairs else 'INCONCLUSIVE'
    report['policy']['scope'] = 'FROZEN_CONSULTED_PAIRS_AND_SCHEDULE_GRID_ONLY'
    report['policy']['code_classes'] = len({digest(canonical([
        dict(case=p['case'], profiles={k: v['code_digest'] for k, v in p['profiles'].items()})
        for p in e['programs']]))
        for e in report['policy']['evaluations'] if e['status'] == 'OBSERVED'})
    report.update(status='BOUNDED_PREFLIGHT_COMPLETED',
                  campaign_admission='NO_AI_CAMPAIGN_AUTHORIZED_BY_THIS_CAPTURE')
    artifact('report.json', report)
    save()


def evaluate_policy(evaluation, units, folder, m, directory, tools, command,
                    guest, guest_path, pin, comparisons, composite_binding):
    bundles, objects = {}, {}
    weights = ','.join(f'{axis}={weight}' for axis, weight in zip(AXES, evaluation['weights']))
    label = folder.name
    for index, u in enumerate(units):
        chain = folder/f'{index:02}-chain'
        command(f'preflight:{label}:chain:{index}', [tools['census'], 'policy',
            digest(u['input'].read_bytes()), u['input'], tools['checker'],
            m['tools']['checker']['sha256'], weights, str(evaluation['max_transactions']), chain])
        binding = bind_bundle(chain, tools['checker'])
        for file in chain.iterdir():
            pin(file)
        record = json.loads((chain/'chain.json').read_bytes(), object_pairs_hook=unique)
        new_pairs = set()
        for tx in record['transactions']:
            trace = selected_pairs(chain/f"{tx['index']:03}.policy.json", tx, evaluation['weights'])
            new_pairs.update(tuple(p) for p in trace['comparisons'] if tuple(p) not in comparisons)
        detail = dict(case=u['case']['id'], source=u['source'], binding=binding,
            new_comparisons=sorted(new_pairs, key=str), scoped_counters=[t['scoped_counters'] for t in record['transactions']],
            resources=json.loads((chain/'resources.json').read_bytes()), native={})
        evaluation['units'].append(detail)
        evaluation['cap_hits'].extend(h for t in record['transactions'] for h in t['budget_hits'])
        bundles.setdefault(u['case']['id'], {})[u['source']] = binding
        for profile in ('checked', 'unchecked'):
            row = command(f'preflight:{label}:asm:{index}:{profile}', [tools['ir_backend'],
                'linux-aarch64', binding['ir_digest'], chain/'output.evo', '--ub-checks='+profile])
            assembly = directory/row['stdout']
            obj = folder/f'{index:02}-{profile}.o'
            guest(f'preflight:{label}:assemble:{index}:{profile}', ['/usr/bin/gcc-14', '-c', '-x',
                'assembler', guest_path(assembly), '-o', guest_path(obj)])
            pin(obj)
            objects.setdefault((u['case']['id'], profile), []).append(obj)
            metrics = inspect_elf(obj.read_bytes())
            detail['native'][profile] = dict(object=obj.name, metrics=metrics, code_digest=section_identity(metrics),
                assembly_digest=digest(assembly.read_bytes()), native_ub_profile=profile)
    for case in m['cases']:
        original = directory/case['id']
        dependencies = [original/f'ref-{i:02}.o' for i, s in enumerate(case['sources']) if s not in case['subject_sources']]
        if case['id'] != 'p0':
            dependencies.append(original/'harness.o')
        common = composite_binding(bundles[case['id']])
        programs = {}
        for profile in ('checked', 'unchecked'):
            exe = folder/(case['id']+'-'+profile)
            guest(f'preflight:{label}:link:{case["id"]}:{profile}', ['/usr/bin/gcc-14',
                *[guest_path(p) for p in objects[case['id'], profile]+dependencies],
                '-Wl,--build-id=none', '-lm', '-o', guest_path(exe)])
            programs[profile] = exe, pin(exe)
        inputs = []
        for mode in case['modes']:
            input_digest = digest(canonical(dict(harness=m['files'][case['harness']], argv=mode)))
            identities = {profile: native_identity(common, profile, 'linux-aarch64',
                m['tools']['ir_backend']['sha256'], programs[profile][1], input_digest)
                for profile in programs}
            checked = guest(f'preflight:{label}:checked:{case["id"]}:{mode}',
                [guest_path(programs['checked'][0]), *mode], case['result'])
            checked['native_identity'] = identities['checked']['native_identity']
            admission = admit_unchecked(identities['checked'], identities['unchecked'], checked, case['result'])
            unchecked = guest(f'preflight:{label}:unchecked:{case["id"]}:{mode}',
                [guest_path(programs['unchecked'][0]), *mode], case['result'])
            unchecked['native_identity'] = identities['unchecked']['native_identity']
            inputs.append(dict(argv=mode, identities=identities, checked_twin=admission))
        evaluation['programs'].append(dict(case=case['id'], profiles={profile:
            dict(executable=exe.name, executable_digest=identity,
                 code_digest=section_identity(inspect_elf(exe.read_bytes())), metrics=inspect_elf(exe.read_bytes()))
            for profile, (exe, identity) in programs.items()}, inputs=inputs))
