"""Size-only attribution using existing verified IR, capture and native tools.

This is not a new auditor, rule search or timing lane. S2 disables the already
present common baseline DCE only in its attribution control. Production DCE and
the verified IR/certificates remain unchanged.
"""
from __future__ import annotations

import fcntl
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from .eqsat_chain import digest, proof_bucket, require, unique, validate_record, verify_bundle
from .eqsat_elf import inspect_object_size, inspect_elf, require_identical_objects
from .eqsat_native import Capture
from .eqsat_twins import BundleReplayRejected, bind_bundle, canonical, stable_bundle
from .eqsat_pilot import recipes


def load(path):
    return json.loads(Path(path).read_bytes(), object_pairs_hook=unique)


def codegen_compile_argv(compiler, checker, checker_hash, case, source, root, profile, chain,
                         extra_flags=(), *, eqsat=True):
    """Fresh original C to assembly; never label frozen old IR as new frontend work."""
    from .eqsat_pilot import ROOTS
    root = Path(root)
    folder = root / case['source_root']
    includes = [root / ROOTS['sysroot'] / p for p in
                ('usr/lib/gcc/aarch64-linux-gnu/14/include', 'usr/include/aarch64-linux-gnu', 'usr/include')]
    if case['id'] != 'p0':
        includes = [folder, folder / 'lib', *includes]
    proof_flags = ['-Oeq-scalar',
            '--eq-checker=' + str(checker), '--eq-checker-sha256=' + checker_hash,
            '--eq-checker-timeout-ms=300000'] if eqsat else []
    return [compiler, '--emit=asm', '-std=gnu11', '-O2', '-fPIC', *case['flags'],
            '-nostdinc', *['-I' + str(p) for p in includes], *proof_flags, '--eq-proof-dir=' + str(chain),
            '--ub-checks=' + profile, *extra_flags, folder / source]


def codegen_selection_counts(assembly):
    """Retain producer counters before discarding assembly, never as proof evidence."""
    counts = dict(selected={}, declined={})
    for bucket, key, count in re.findall(
            rb'^// evocc-local-(selected|declined) ([a-z0-9_/-]+)=([0-9]+)$',
            assembly, flags=re.MULTILINE):
        values = counts[bucket.decode('ascii')]
        key = key.decode('ascii')
        values[key] = values.get(key, 0) + int(count)
    return counts


def codegen_disk_preflight(root, resources, *, start):
    require(resources['jobs'] == 1 and resources['min_free_bytes'] >= 25 * 1024**3 and
            resources['max_retained_bytes'] <= 500 * 1024**2, 'N05 disk/job envelope')
    floor = resources['min_free_bytes'] if start else 15 * 1024**3
    require(shutil.disk_usage(root).free >= floor,
            'N05 requires 25 GiB free before starting' if start else 'N05 free space below stop reserve')


def cleanup_codegen_products(directory, products, *, retain=()):
    """Only the explicitly owned new outputs; never reference dependencies."""
    directory = Path(directory).resolve()
    paths = [Path(path) for path in products]
    require(all(path.resolve().is_relative_to(directory) for path in paths),
            'N05 cleanup target is outside this capture')
    retained = {Path(path).resolve() for path in retain}
    require(retained <= {path.resolve() for path in paths} and
            all(path.suffix == '.o' for path in retained), 'retained object is not an owned object')
    for path in paths:
        if path.resolve() not in retained:
            path.unlink(missing_ok=True)


class ObjectBaselineMismatch(ValueError):
    """Only a full-byte mismatch authorizes retaining a failure object."""


def compare_codegen_object_baseline(root, baseline_receipt, unit, profile, candidate):
    """Compare full ELF bytes with the one retained, already measured object."""
    root = Path(root).resolve()
    record = load(baseline_receipt)
    require(record.get('schema') == 'evocc-codegen-capture-v1' and record.get('status') == 'OBSERVED',
            'object baseline is not a completed capture')
    matching = [row for row in record.get('retained_objects', [])
                if row['unit'] == unit and row['profile'] == profile]
    require(len(matching) == 1, 'missing or duplicate object baseline for unit/profile')
    row = matching[0]
    obj = root / row['path']
    require(obj.is_file() and not obj.is_symlink() and obj.resolve().is_relative_to(root),
            'missing or aliased retained baseline object')
    original = obj.read_bytes()
    require(digest(original) == row['object_sha256'], 'retained baseline object changed')
    try:
        require_identical_objects(original, candidate)
    except ValueError as error:
        raise ObjectBaselineMismatch(str(error)) from error
    return dict(unit=unit, profile=profile, baseline_object_sha256=row['object_sha256'],
                candidate_object_sha256=digest(candidate), status='BYTE_IDENTICAL')


def codegen_proof_failure(receipt, chain, error, phase):
    """Classify failures using existing dispositions, without replaying again."""
    event = dict(phase=phase, chain=Path(chain).name)
    if isinstance(error, BundleReplayRejected):
        audit = error.audit
        receipt['proof']['completed_rejections'] += audit['proof_counts']['completed_proof_rejections']
        receipt['proof']['bundle_failures'] += audit['proof_counts']['bundle_obligation_failures']
        event.update(evidence='INDEPENDENT_REPLAY', replay=audit['replay'],
                     proof_counts=audit['proof_counts'])
    elif phase == 'compile':
        record_loaded, identity_audit = False, None
        try:
            record = load(Path(chain) / 'chain.json')
            record_loaded = True
            if isinstance(record, dict) and record.get('schema') == 'evocc-identity-chain-v1':
                identity_audit = verify_bundle(Path(chain))
            else:
                validate_record(record)
        except FileNotFoundError as missing:
            # A frontend/driver failure before the chain exists is not evidence
            # of either an invalid equation or a malformed certificate.
            if record_loaded:
                receipt['proof']['bundle_failures'] += 1
                event.update(evidence='UNREADABLE_IDENTITY_BUNDLE', reason='BUNDLE_MISMATCH', detail=str(missing))
            else:
                event.update(evidence='NOT_AVAILABLE', reason='CHAIN_NOT_EMITTED')
        except (OSError, ValueError, TypeError, KeyError) as malformed:
            receipt['proof']['bundle_failures'] += 1
            event.update(evidence='UNREADABLE_CHAIN_RECORD', reason='BUNDLE_MISMATCH', detail=str(malformed))
        else:
            if identity_audit is not None:
                event.update(evidence=identity_audit['evidence'], publication=record['publication'],
                             reason='COMPILE_FAILED_AFTER_IDENTITY_CHAIN')
            else:
                transactions = []
                for tx in record['transactions']:
                    bucket = proof_bucket(tx['verification'], tx['rejection_reason'], tx['result'])
                    receipt['proof']['completed_rejections'] += int(bucket == 'completed_proof_rejections')
                    receipt['proof']['bundle_failures'] += int(bucket == 'bundle_obligation_failures')
                    transactions.append(dict(index=tx['index'], result=tx['result'], bucket=bucket,
                                             reason=tx['rejection_reason']))
                event.update(evidence='PRODUCER_TELEMETRY_NOT_PROVED', publication=record['publication'],
                             reason=record.get('rejection_reason', 'COMPILE_FAILED_AFTER_VERIFIED_CHAIN'),
                             transactions=transactions)
    else:
        receipt['proof']['bundle_failures'] += 1
        event.update(evidence='BUNDLE_ADMISSION_FAILURE', reason='BUNDLE_MISMATCH', detail=str(error))
    receipt['failure_events'].append(event)


def run_codegen_measurement(plan, root):
    """N05 fresh six-TU capture through the existing Capture/S1/S5 paths.

    This entry point consumes an already built release compiler. The pre-push
    orchestrator builds once, serially. VM execution stays under bench.lock.
    N02 step commands may be absent during integration, which returns NOT_RUN,
    never a completed N05 observation.
    """
    from .codegen_trend import (UNITS, PROFILES, append_observation, coverage,
                               object_row, steps_from_reports)
    root, plan = Path(root).resolve(), Path(plan).resolve()
    raw, m = plan.read_bytes(), load(plan)
    require(m['schema'] == 'evocc-codegen-measure-plan-v1', 'not an N05 live measurement plan')
    eqsat = m.get('eqsat', True)
    retain_objects = m.get('retain_object_baseline', False)
    require(type(eqsat) is bool and type(retain_objects) is bool, 'invalid object baseline option')
    require(subprocess.check_output(['git', 'show', 'HEAD:' + plan.relative_to(root).as_posix()], cwd=root) == raw,
            'commit the N05 live plan before execution')
    require(not subprocess.check_output(['git', 'status', '--porcelain'], cwd=root),
            'N05 current-commit measurement requires a clean worktree')
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    tools = {name: (root / path).resolve() for name, path in m['tools'].items()}
    require(all(tools[name].is_relative_to(root / 'target/frontend/release')
                for name in ('compiler', 'checker')), 'N05 requires the shared release target')
    tool_hashes = {name: digest(path.read_bytes()) for name, path in tools.items()}
    for path, expected in m['inputs'].items():
        require(digest((root / path).read_bytes()) == expected, 'N05 input changed: ' + path)
    object_baseline = m.get('object_baseline_receipt')
    if object_baseline is not None:
        require(object_baseline in m['inputs'], 'object baseline receipt must be a frozen input')
        baseline_record = load(root / object_baseline)
        require(baseline_record.get('status') == 'OBSERVED' and
                len(baseline_record.get('retained_objects', [])) == len(UNITS) * len(PROFILES) and
                {(row['unit'], row['profile']) for row in baseline_record['retained_objects']}
                == {(unit, profile) for unit in UNITS for profile in PROFILES},
                'object baseline does not contain the completed six-unit/profile population')
        baseline_raw = (root / object_baseline).parent.joinpath('manifest.json').read_bytes()
        require(digest(baseline_raw) == baseline_record['manifest_sha256'], 'object baseline plan changed')
        baseline_plan = json.loads(baseline_raw, object_pairs_hook=unique)
        require(baseline_plan.get('eqsat', True) == eqsat and
                baseline_plan.get('extra_flags', []) == m.get('extra_flags', []) and
                all(baseline_plan[field] == m[field] for field in
                    ('source_plan', 'program_plan', 'reference_receipt', 'image', 'container_sha256', 'tools')),
                'object baseline compiler controls differ')
        require(all(m['inputs'].get(path) == expected for path, expected in baseline_plan['inputs'].items()
                    if path != baseline_plan.get('object_baseline_receipt')),
                'object baseline source/runtime inputs differ')
    require(m['source_plan'] in m['inputs'], 'N05 S5 source plan must be frozen')
    for path, expected in load(root / m['source_plan'])['inputs'].items():
        if path.endswith(('.c', '.h')):
            require(digest((root / path).read_bytes()) == expected, 'N05 source/header changed: ' + path)
    require(tool_hashes['container'] == m['container_sha256'], 'N05 container tool changed')
    require(all(path in m['inputs'] for path in (m['program_plan'], m['reference_receipt'])),
            'N05 reused program/reference receipts must be frozen inputs')
    programs = load(root / m['program_plan'])['programs']
    reference = load(root / m['reference_receipt'])
    resources = m['resources']
    codegen_disk_preflight(root, resources, start=True)
    cases = {case['id']: case for case in recipes(root)}
    expected = [case['id'] + ':' + source for case in cases.values() for source in case['subject_sources']]
    require(expected == list(UNITS), 'N05 six-unit recipe changed')
    require(all((Path(case['source_root']) / source).as_posix() in m['inputs']
                for case in cases.values() for source in case['subject_sources']),
            'N05 original source files must be frozen inputs')
    require(all(path in m['inputs'] for program in programs for path in
                program['dependencies'] + ([program['harness']] if program['harness'] else [])),
            'N05 reference dependencies must be frozen inputs')
    with (root / '.build/bench.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        parent = root / '.evidence/codegen-trend'
        parent.mkdir(parents=True, exist_ok=True)
        directory = Path(tempfile.mkdtemp(prefix='measure-', dir=parent))
        (directory / 'manifest.json').write_bytes(raw)
        free_before = shutil.disk_usage(root).free
        receipt = dict(schema='evocc-codegen-capture-v1', status='STARTED', source_commit=head,
                       manifest_sha256=digest(raw), tools=tool_hashes, attempts=[], units=[], ladder=[],
                       bindings={}, failure_events=[],
                       proof=dict(completed_rejections=0, bundle_failures=0), timing='NOT_RUN',
                       scope='SIX_SELECTED_TUS_WITH_REFERENCE_DEPENDENCIES_NOT_FULL_APPLICATIONS')
        if 'eqsat' in m:
            receipt['eqsat'] = eqsat
        if retain_objects:
            receipt['retained_objects'] = []
        if object_baseline:
            receipt['object_baseline_receipt'] = object_baseline
            receipt['object_baseline_comparisons'] = []

        def save():
            (directory / 'receipt.json').write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')

        runner = Capture(directory, resources, dict(os.environ), receipt, save, watch_interval_seconds=1)
        vm, started = directory.name.lower(), False
        trend = None
        owned_objects, owned_executables = [], []
        retention_candidates, failure_objects = [], []

        def run(stage, argv, expected=None):
            codegen_disk_preflight(root, resources, start=False)
            return runner.run(stage, argv, expected=expected)

        def guest_path(path):
            path = Path(path).resolve()
            return '/runwork/' + path.relative_to(directory).as_posix() if path.is_relative_to(directory) else \
                   '/work/' + path.relative_to(root).as_posix()

        def guest(stage, argv, expected=None):
            return run(stage, [tools['container'], 'exec', vm, *argv], expected)

        try:
            image = run('image', [tools['container'], 'image', 'inspect', m['image']['name']])
            require(load(directory / image['stdout'])[0]['id'] == m['image']['digest'], 'N05 sealed image changed')
            run('start-size-vm', [tools['container'], 'run', '--rm', '-d', '--name', vm,
                '--cpus', '2', '--memory', '3G', '--read-only', '--tmpfs', '/tmp', '--no-dns',
                '--mount', f'type=bind,source={root},target=/work,readonly',
                '--mount', f'type=bind,source={directory},target=/runwork', m['image']['name'], '/bin/sleep', '3600'])
            started = True
            live_objects, frozen_bindings = {}, {}
            for number, name in enumerate(UNITS):
                family, source = name.split(':', 1)
                case = cases[family]
                ref = next(row for row in reference['units'] if row['case'] == family and row['source'] == source)
                gcc_size = ref['references']['gcc-14-O2']['metrics']['text_bytes']
                for profile in PROFILES:
                    objects, compile_times, assembly_hashes = [], [], []
                    for repeat in range(3):
                        chain = directory / f'{number}-{profile}-{repeat}-chain'
                        argv = codegen_compile_argv(tools['compiler'], tools['checker'], tool_hashes['checker'],
                            case, source, root, profile, chain, m.get('extra_flags', []), eqsat=eqsat)
                        try:
                            result = run(f'compile:{name}:{profile}:{repeat}', argv)
                        except Exception as error:
                            codegen_proof_failure(receipt, chain, error, 'compile')
                            raise
                        compile_times.append(result['wall_ns'])
                        asm = directory / result['stdout']
                        assembly_hashes.append(result['stdout_sha256'])
                        if repeat == 0:
                            receipt.setdefault('producer_selection_counts', []).append(dict(
                                unit=name, profile=profile,
                                claim='PRODUCER_TELEMETRY_NOT_PROVED',
                                **codegen_selection_counts(asm.read_bytes())))
                        try:
                            binding = bind_bundle(chain, tools['checker'] if eqsat else None)
                        except Exception as error:
                            codegen_proof_failure(receipt, chain, error, 'replay')
                            raise
                        compact_binding = {field: binding[field] for field in
                                           ('ir_digest', 'chain_record_digest', 'bundle_digest')}
                        if name in frozen_bindings:
                            require(frozen_bindings[name] == compact_binding,
                                    'N05 checked/unchecked compilations do not share the same verified IR/bundle')
                        else:
                            frozen_bindings[name] = compact_binding
                            receipt['bindings'][name] = compact_binding
                        if repeat < 2:
                            obj = directory / f'{number}-{profile}-{repeat}.o'
                            owned_objects.append(obj)
                            guest(f'assemble:{name}:{profile}:{repeat}', ['/usr/bin/gcc-14', '-c', '-x',
                                  'assembler', guest_path(asm), '-o', guest_path(obj)])
                            objects.append(obj)
                        # Commands and hashes are retained. The same fresh source
                        # compilation recreates the now-replayed intermediate bundle.
                        shutil.rmtree(chain)
                        asm.unlink()
                    require(len(set(assembly_hashes)) == 1, 'N05 three source compilations differ')
                    row = object_row(name, profile, objects[0].read_bytes(), objects[1].read_bytes(),
                                     gcc_o2_text_bytes=gcc_size, compile_wall_ns=compile_times)
                    retained = dict(unit=name, profile=profile, path=objects[0].relative_to(root).as_posix(),
                                    object_sha256=row['object_sha256'])
                    if object_baseline:
                        try:
                            comparison = compare_codegen_object_baseline(
                                root, root / object_baseline, name, profile, objects[0].read_bytes())
                        except ObjectBaselineMismatch:
                            # A single already-replayed candidate is the minimal
                            # failure artifact; FAILED receipts cannot be baselines.
                            receipt['failure_object'] = retained
                            failure_objects.append(objects[0])
                            raise
                        receipt['object_baseline_comparisons'].append(comparison)
                    if retain_objects:
                        retention_candidates.append(retained)
                    receipt['units'].append(row)
                    live_objects[name, profile] = objects[0]
                    objects[1].unlink()
                    save()
            for program in programs:
                for profile in PROFILES:
                    objs = [live_objects[name, profile] for name in UNITS if name.startswith(program['case'] + ':')]
                    objs += [root / path for path in program['dependencies']]
                    if program['harness']:
                        objs.append(root / program['harness'])
                    exe = directory / (program['case'] + '-' + profile)
                    owned_executables.append(exe)
                    guest('link:' + exe.name, ['/usr/bin/gcc-14', *map(guest_path, objs), '-Wl,--build-id=none', '-lm',
                                               '-o', guest_path(exe)])
                    for mode in program['modes']:
                        case_name = {'p0': 'integer-mix', 'cjson': 'cjson-roundtrip'}.get(program['case'])
                        if case_name is None:
                            case_name = program['case'] + '-' + mode[0]
                        result = guest('correctness:' + case_name + ':' + profile,
                                       [guest_path(exe), *mode], program['result'])
                        # Both profiles ran; the stable output identity is shared.
                        entry = dict(case=case_name, status='PASS', result_sha256=digest(canonical({
                            k: result[k] for k in ('exit_code', 'stdout_sha256', 'stderr_sha256')})))
                        prior = next((r for r in receipt['ladder'] if r['case'] == case_name), None)
                        require(prior is None or prior == entry, 'checked/unchecked replay differs')
                        if prior is None:
                            receipt['ladder'].append(entry)
                    exe.unlink()
                    save()
            reports = []
            steps_plan = m.get('steps')
            if steps_plan:
                for number, command in enumerate(steps_plan['commands']):
                    report = directory / f'n02-{number}.json'
                    args = []
                    for arg in command:
                        arg = str(arg).replace('{report}', str(report)).replace('{root}', str(root))
                        for name, tool in tools.items():
                            arg = arg.replace('{' + name + '}', str(tool))
                        args.append(arg)
                    result = run('n02-steps:' + str(number), args)
                    reports.append(load(report if report.exists() else directory / result['stdout']))
            steps = steps_from_reports(reports, steps_plan['plan_sha256'], steps_plan['expected_cases'],
                                       steps_plan.get('aliases')) if steps_plan else None
            require(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip() == head,
                    'N05 source commit changed during run')
            require(all(digest(path.read_bytes()) == tool_hashes[name] for name, path in tools.items()),
                    'N05 measured binary changed during run')
            receipt.update(status='OBSERVED', steps=steps)
            trend = dict(schema='evocc-codegen-trend-v1', source_commit=head,
                compiler_sha256=tool_hashes['compiler'], plan_sha256=digest(raw),
                inputs_sha256=digest(canonical(m['inputs'])), capture_sha256='0' * 64,
                variant=m['variant'], units=receipt['units'], ladder=receipt['ladder'], steps=steps, proof=receipt['proof'])
            receipt['coverage'] = coverage(trend)
            receipt['status'] = 'OBSERVED' if receipt['coverage']['status'] == 'OBSERVED' else 'NOT_RUN'
        except BaseException as error:
            receipt.update(status='FAILED', error=str(error))
            raise
        finally:
            try:
                if started:
                    runner.run('stop-size-vm', [tools['container'], 'stop', vm])
            except BaseException as error:
                receipt.update(status='FAILED', cleanup_error=str(error))
                raise
            finally:
                retained = retention_candidates if retain_objects and receipt['status'] == 'OBSERVED' else []
                if retain_objects:
                    receipt['retained_objects'] = retained
                cleanup_codegen_products(directory, owned_objects + owned_executables,
                                         retain=[root / row['path'] for row in retained] + failure_objects)
                receipt['owned_retained_bytes'] = sum(p.stat().st_size for p in directory.rglob('*') if p.is_file())
                receipt['disk_free_delta_bytes'] = shutil.disk_usage(root).free - free_before
                save()
        if trend is not None:
            trend['capture_sha256'] = digest((directory / 'receipt.json').read_bytes())
            append_observation(root / m['history'], trend)
        return directory, receipt


def run_guard_certification(plan, root):
    """R02e first slice. Reuse the existing chain replay and S1 ELF contract.

    The old diagnostic ceiling and P0/DCE captures remain frozen. The current
    B baseline includes the common MIR true-constant selection in both arms.
    No checker, identity or audit layer is introduced here.
    """
    root, plan = Path(root).resolve(), Path(plan).resolve()
    raw, m = plan.read_bytes(), load(plan)
    require(m['schema']=='evocc-certified-guard-size-plan-v1' and m['registry']=='g0.ub.v1',
            'not the approved first guard slice')
    require(subprocess.check_output(['git','show','HEAD:'+plan.relative_to(root).as_posix()],cwd=root)==raw,
            'commit the guard size plan before execution')
    tools = {k: Path(v['path']).resolve() if Path(v['path']).is_absolute() else
             (root/v['path']).resolve() for k,v in m['tools'].items()}
    for k,p in tools.items():
        require(digest(p.read_bytes())==m['tools'][k]['sha256'],'guard size tool changed: '+k)
    for p,h in m['inputs'].items():
        require(digest((root/p).read_bytes())==h,'guard size input changed: '+p)
    resources = m['resources']
    require(resources['jobs']==1 and resources['min_free_bytes']>=8*1024**3
            and resources['max_retained_bytes']<=256*1024**2 and resources['timeout_seconds']<=300,
            'unbounded guard size run')
    require(shutil.disk_usage(root).free>=resources['min_free_bytes'],'guard disk reserve unavailable')
    with (root/'.build/bench.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        directory = Path(tempfile.mkdtemp(prefix='g0-',dir=root/'.evidence/eqsat-size'))
        (directory/'manifest.json').write_bytes(raw)
        receipt = dict(schema='evocc-certified-guard-size-receipt-v1',status='STARTED',
            manifest_digest=digest(raw),attempts=[],units=[],programs=[],timing='NOT_RUN',
            claim='g0-only-guard-savings-after-P0-and-common-baseline-DCE')

        def save():
            pending = directory/'receipt.json.pending'
            pending.write_bytes(json.dumps(receipt,indent=2,sort_keys=True).encode()+b'\n')
            pending.replace(directory/'receipt.json')

        runner = Capture(directory,resources,dict(os.environ),receipt,save)
        vm, started = directory.name.lower(), False

        def guest_path(p):
            p = Path(p).resolve()
            return '/runwork/'+p.relative_to(directory).as_posix() if p.is_relative_to(directory) else '/work/'+p.relative_to(root).as_posix()

        def guest(argv,stage,expected=None):
            return runner.run(stage,[tools['container'],'exec',vm,*argv],expected=expected)

        def predicates(path):
            text = Path(path).read_text()
            names = ('divisor_nonzero','signed_div_rem_no_overflow','signed_add_defined',
                     'signed_sub_defined','signed_mul_defined','shift_count_defined.unsigned',
                     'shift_count_defined.signed')
            return sum(any((' = '+name+' ') in line for name in names) for line in text.splitlines())

        save()
        try:
            image = runner.run('image',[tools['container'],'image','inspect',m['image']['name']])
            require(load(directory/image['stdout'])[0]['id']==m['image']['digest'],'guard image changed')
            runner.run('start-object-vm',[tools['container'],'run','--rm','-d','--name',vm,
                '--cpus','2','--memory','3G','--read-only','--tmpfs','/tmp','--no-dns',
                '--mount',f'type=bind,source={root},target=/work,readonly',
                '--mount',f'type=bind,source={directory},target=/runwork',
                m['image']['name'],'/bin/sleep','3600'])
            started = True
            for index,unit in enumerate(m['units']):
                ir = root/unit['ir']
                chain = directory/f'{index:02}-chain'
                runner.run('g0-chain:'+unit['source'],[tools['producer'],'guards',digest(ir.read_bytes()),ir,
                    tools['checker'],m['tools']['checker']['sha256'],'guard-known-true','8',chain])
                binding = bind_bundle(chain,tools['checker'])
                record = load(chain/'chain.json')
                require(record['policy']['registry']=='g0.ub.v1' and record['publication']=='VERIFIED',
                        'guard chain not independently verified')
                row = dict(case=unit['case'],source=unit['source'],binding=binding,
                    total_predicates=predicates(ir),retained_predicates=predicates(chain/'output.evo'),
                    certified_roots=sum(t['roots']['accepted'] for t in record['transactions']),
                    proof_accounting=[dict(result=t['result'],verification=t['verification'],reason=t['rejection_reason'])
                                      for t in record['transactions']],
                    diagnostic_ceiling=unit['diagnostic_ceiling'],arms={})
                require(row['total_predicates']-row['retained_predicates']==row['certified_roots'],
                        'guard root accounting mismatch')
                for arm,path in [('B',ir),('G',chain/'output.evo')]:
                    row['arms'][arm] = {}
                    for profile in ('checked','unchecked'):
                        objects, assemblies = [], []
                        for repeat in (0,1):
                            stem = f'{index:02}-{arm}-{profile}-{repeat}'
                            asm = runner.run('asm:'+stem,[tools['backend'],'linux-aarch64',digest(path.read_bytes()),path,
                                                        '--ub-checks='+profile])
                            assemblies.append((directory/asm['stdout']).read_bytes())
                            obj = directory/(stem+'.o')
                            guest(['/usr/bin/gcc-14','-c','-x','assembler',guest_path(directory/asm['stdout']),
                                   '-o',guest_path(obj)],'assemble:'+stem)
                            objects.append(obj)
                        require(assemblies[0]==assemblies[1],'guard lowering not reproducible')
                        require_identical_objects(objects[0].read_bytes(),objects[1].read_bytes())
                        row['arms'][arm][profile] = dict(metrics=inspect_object_size(objects[0].read_bytes()),
                            object=objects[0].relative_to(root).as_posix(),two_builds='BYTE_IDENTICAL')
                old = inspect_object_size((root/unit['old_baseline_object']).read_bytes())['text_bytes']
                baseline = row['arms']['B']['checked']['metrics']['text_bytes']
                after = row['arms']['G']['checked']['metrics']['text_bytes']
                row.update(common_baseline_delta=old-baseline,certified_saving=baseline-after,
                           certified_percent=100*(baseline-after)/baseline)
                receipt['units'].append(row)
                save()
            # Original S2 mixed-link correctness recipes; no timing loops and
            # no all-Evo application or kernel claim from these six TUs.
            for program in m['programs']:
                units = [u for u in receipt['units'] if u['case']==program['case']]
                for arm in ('B','G'):
                    for profile in ('checked','unchecked'):
                        objects = [root/u['arms'][arm][profile]['object'] for u in units]
                        objects += [root/p for p in program['dependencies']]
                        if program['harness']: objects.append(root/program['harness'])
                        exe = directory/f"{program['case']}-{arm}-{profile}"
                        guest(['/usr/bin/gcc-14',*map(guest_path,objects),'-Wl,--build-id=none','-lm',
                               '-o',guest_path(exe)],'link:'+exe.name)
                        checks = [guest([guest_path(exe),*mode],'correctness:'+exe.name+':'+str(i),program['result'])
                                  for i,mode in enumerate(program['modes'])]
                        receipt['programs'].append(dict(case=program['case'],arm=arm,profile=profile,
                            correctness=[r['stage'] for r in checks],scope='S2_MIXED_LINK_REPLAY'))
                        save()
            receipt['status'] = 'VERIFIED_GUARD_TU_SIZES'
        except BaseException as error:
            receipt.update(status='FAILED',error=str(error))
            raise
        finally:
            if started: runner.run('stop-object-vm',[tools['container'],'stop',vm])
            save()
        return directory,receipt


def run_dce_attribution(plan, root):
    root, plan = Path(root).resolve(), Path(plan).resolve()
    raw, m = plan.read_bytes(), load(plan)
    require(m['schema'] == 'evocc-size-attribution-plan-v1' and m['phase'] == 'S2',
            'not the registered S2 size plan')
    saved = subprocess.check_output(['git', 'show', 'HEAD:'+plan.relative_to(root).as_posix()], cwd=root)
    require(saved == raw, 'commit the size plan before execution')
    tools = {k: Path(v['path']).resolve() if Path(v['path']).is_absolute() else
             (root/v['path']).resolve() for k, v in m['tools'].items()}
    for name, path in tools.items():
        require(digest(path.read_bytes()) == m['tools'][name]['sha256'], 'size tool changed: '+name)
    for name, expected in m['inputs'].items():
        require(digest((root/name).read_bytes()) == expected, 'size input changed: '+name)
    resources = m['resources']
    require(resources['jobs'] == 1 and resources['min_free_bytes'] >= 8*1024**3
            and resources['max_retained_bytes'] <= 256*1024**2
            and resources['timeout_seconds'] <= 300, 'unbounded size run')
    require(shutil.disk_usage(root).free >= resources['min_free_bytes'], 'size disk reserve unavailable')
    with (root/'.build/bench.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        directory = Path(tempfile.mkdtemp(prefix='S2-', dir=root/'.evidence/eqsat-size'))
        (directory/'manifest.json').write_bytes(raw)
        receipt = dict(schema='evocc-dce-size-receipt-v1', status='STARTED',
                       manifest_digest=digest(raw), attempts=[], units=[], programs=[],
                       timing='NOT_RUN', claim='BASELINE_DCE_ATTRIBUTION_NOT_VERIFIED_EQSAT_GAIN')

        def save():
            pending = directory/'receipt.json.pending'
            pending.write_bytes(json.dumps(receipt, indent=2, sort_keys=True).encode()+b'\n')
            pending.replace(directory/'receipt.json')

        runner = Capture(directory, resources, dict(os.environ), receipt, save)
        vm = directory.name.lower()

        def guest(argv, stage):
            return runner.run(stage, [tools['container'], 'exec', vm, *argv])

        def guest_path(path):
            return '/work/'+Path(path).resolve().relative_to(root).as_posix()

        started = False
        save()
        try:
            image = runner.run('image', [tools['container'], 'image', 'inspect', m['image']['name']])
            require(load(directory/image['stdout'])[0]['id'] == m['image']['digest'], 'size image changed')
            runner.run('start-size-vm', [tools['container'], 'run', '--rm', '-d', '--name', vm,
                       '--cpus', '2', '--memory', '3G', '--read-only', '--tmpfs', '/tmp', '--no-dns',
                       '--mount', f'type=bind,source={root},target=/work,readonly',
                       '--mount', f'type=bind,source={directory},target=/runwork',
                       m['image']['name'], '/bin/sleep', '3600'])
            started = True
            versions = guest(['/usr/bin/gcc-14', '--version'], 'assembler-toolchain')
            receipt['guest_toolchain'] = (directory/versions['stdout']).read_text().splitlines()[0]
            for n, unit in enumerate(m['units']):
                result = dict(case=unit['case'], source=unit['source'], eligible_roots=unit['eligible_roots'], arms={})
                receipt['units'].append(result)
                for arm in ('A', 'B'):
                    chain = root/unit['chains'][arm]
                    binding = bind_bundle(chain, tools[unit['checkers'][arm]]
                                          if load(chain/'chain.json')['schema'] != 'evocc-identity-chain-v1' else None)
                    result['arms'][arm] = dict(binding=binding, profiles={})
                    for profile in ('checked', 'unchecked'):
                        values = {}
                        result['arms'][arm]['profiles'][profile] = values
                        for dce in ('off', 'on'):
                            stem = f'{n:02}-{arm}-{profile}-{dce}'
                            objects, assemblies, telemetry = [], [], []
                            for repeat in (0, 1):
                                args = [tools['backend'], 'linux-aarch64', binding['ir_digest'],
                                        chain/'output.evo', '--ub-checks='+profile, '--dce-report']
                                if dce == 'off':
                                    args.append('--retain-dead-computations')
                                row = runner.run('backend:'+stem+f':{repeat}', args)
                                assembly = (directory/row['stdout']).read_bytes()
                                assemblies.append(assembly)
                                telemetry.append(load(directory/row['stderr']))
                                asm, obj = directory/(stem+'.s'), directory/(stem+f'-{repeat}.o')
                                asm.write_bytes(assembly)
                                guest(['/usr/bin/gcc-14', '-c', '-x', 'assembler', '/runwork/'+asm.name,
                                       '-o', '/runwork/'+obj.name], 'assemble:'+stem+f':{repeat}')
                                objects.append(obj)
                            require(assemblies[0] == assemblies[1] and telemetry[0] == telemetry[1],
                                    'backend build is not reproducible')
                            require_identical_objects(objects[0].read_bytes(), objects[1].read_bytes())
                            values[dce] = dict(object=objects[0].relative_to(root).as_posix(),
                                               metrics=inspect_object_size(objects[0].read_bytes()),
                                               telemetry=telemetry[0], two_builds='BYTE_IDENTICAL')
                    stable_bundle(chain, binding)
                require(result['arms']['A']['binding']['inventory']['output.evo'] ==
                        result['arms']['B']['binding']['inventory']['input.evo'], 'A/B input IR mismatch')
                save()
            # Original correctness harnesses, without in-process timing loops.
            for program in m['programs']:
                result = dict(case=program['case'], variants=[])
                receipt['programs'].append(result)
                units = [u for u in receipt['units'] if u['case'] == program['case']]
                for arm in ('A', 'B'):
                    for profile in ('checked', 'unchecked'):
                        for dce in ('off', 'on'):
                            objects = [root/u['arms'][arm]['profiles'][profile][dce]['object'] for u in units]
                            objects.extend(root/name for name in program['dependencies'])
                            if program['harness']:
                                objects.append(root/program['harness'])
                            exe = directory/f"{program['case']}-{arm}-{profile}-{dce}"
                            guest(['/usr/bin/gcc-14', *map(guest_path, objects), '-Wl,--build-id=none', '-lm',
                                   '-o', '/runwork/'+exe.name], 'link:'+exe.name)
                            rows = [runner.run('correctness:'+exe.name, [tools['container'], 'exec', vm,
                                    '/runwork/'+exe.name, *mode], expected=program['result']) for mode in program['modes']]
                            result['variants'].append(dict(arm=arm, profile=profile, dce=dce,
                                executable=exe.relative_to(root).as_posix(), metrics=inspect_elf(exe.read_bytes()),
                                correctness_attempts=[row['stage'] for row in rows]))
                save()
            receipt['status'] = 'OBSERVED_SIZE_ATTRIBUTION'
        except BaseException as error:
            receipt.update(status='FAILED', error=str(error))
            raise
        finally:
            if started:
                runner.run('stop-size-vm', [tools['container'], 'stop', vm])
            save()
        return directory, receipt


def run_guard_attribution(plan, root):
    """S4 object-size diagnostic only: no link, execution or installation."""
    root, plan = Path(root).resolve(), Path(plan).resolve()
    raw, m = plan.read_bytes(), load(plan)
    require(m['schema'] == 'evocc-guard-size-plan-v1' and m['phase'] == 'S4',
            'not the committed S4 diagnostic plan')
    require(subprocess.check_output(['git', 'show', 'HEAD:'+plan.relative_to(root).as_posix()], cwd=root) == raw,
            'commit the guard plan before execution')
    tools = {k: (root/v['path']).resolve() for k, v in m['tools'].items()}
    for name, path in tools.items():
        require(digest(path.read_bytes()) == m['tools'][name]['sha256'], 'guard tool changed: '+name)
    for name, expected in m['inputs'].items():
        require(digest((root/name).read_bytes()) == expected, 'guard input changed: '+name)
    resources = m['resources']
    require(resources['jobs'] == 1 and resources['min_free_bytes'] >= 8*1024**3
            and resources['max_retained_bytes'] <= 256*1024**2, 'unbounded guard diagnostic')
    require(shutil.disk_usage(root).free >= resources['min_free_bytes'], 'guard disk reserve unavailable')
    with (root/'.build/bench.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        directory = Path(tempfile.mkdtemp(prefix='S4-', dir=root/'.evidence/eqsat-size'))
        (directory/'manifest.json').write_bytes(raw)
        receipt = dict(schema='evocc-guard-size-receipt-v1', status='STARTED',
            manifest_digest=digest(raw), attempts=[], units=[], families=[],
            installable=False, pass_eligible=False, native_executions=0, timing='NOT_RUN')

        def save():
            pending = directory/'receipt.json.pending'
            pending.write_bytes(json.dumps(receipt, indent=2, sort_keys=True).encode()+b'\n')
            pending.replace(directory/'receipt.json')

        runner = Capture(directory, resources, dict(os.environ), receipt, save)
        vm, started = directory.name.lower(), False
        save()
        try:
            image = runner.run('image', [tools['container'], 'image', 'inspect', m['image']['name']])
            require(load(directory/image['stdout'])[0]['id'] == m['image']['digest'], 'guard image changed')
            runner.run('start-object-vm', [tools['container'], 'run', '--rm', '-d', '--name', vm,
                '--cpus', '2', '--memory', '3G', '--read-only', '--tmpfs', '/tmp', '--no-dns',
                '--mount', f'type=bind,source={root},target=/work,readonly',
                '--mount', f'type=bind,source={directory},target=/runwork',
                m['image']['name'], '/bin/sleep', '3600'])
            started = True
            for n, unit in enumerate(m['units']):
                ir = root/unit['ir']
                control = runner.run('unchanged-production:'+unit['source'],
                    [tools['backend'], 'linux-aarch64', digest(ir.read_bytes()), ir, '--ub-checks=checked'])
                require((directory/control['stdout']).read_bytes() == (root/unit['baseline_assembly']).read_bytes(),
                        'ordinary checked assembly changed before guard attribution')
                assemblies, reports, objects = [], [], []
                for repeat in (0, 1):
                    row = runner.run('diagnostic:'+unit['source']+':'+str(repeat),
                        [tools['guard_backend'], digest(ir.read_bytes()), ir])
                    assemblies.append((directory/row['stdout']).read_bytes())
                    reports.append(load(directory/row['stderr']))
                    require(reports[-1]['status'] == 'UNVERIFIED_DIAGNOSTIC'
                            and reports[-1]['installable'] is False and reports[-1]['pass_eligible'] is False,
                            'diagnostic artifact lost its non-installable scope')
                    asm, obj = directory/f'{n:02}-{repeat}.s', directory/f'{n:02}-{repeat}.o'
                    asm.write_bytes(assemblies[-1])
                    runner.run('assemble-diagnostic:'+unit['source']+':'+str(repeat),
                        [tools['container'], 'exec', vm, '/usr/bin/gcc-14', '-c', '-x', 'assembler',
                         '/runwork/'+asm.name, '-o', '/runwork/'+obj.name])
                    objects.append(obj)
                require(assemblies[0] == assemblies[1] and reports[0] == reports[1], 'unstable diagnostic lowering')
                require_identical_objects(objects[0].read_bytes(), objects[1].read_bytes())
                baseline = inspect_object_size((root/unit['baseline_object']).read_bytes())
                measured = inspect_object_size(objects[0].read_bytes())
                recovered = baseline['text_bytes']-measured['text_bytes']
                receipt['units'].append(dict(case=unit['case'], source=unit['source'],
                    baseline=baseline, diagnostic=measured, report=reports[0],
                    object=objects[0].relative_to(root).as_posix(), two_builds='BYTE_IDENTICAL',
                    recovered_text_bytes=recovered, recovered_percent=100*recovered/baseline['text_bytes']))
                save()
            for case, eqsat_saving in m['post_s2_checked_savings'].items():
                units = [u for u in receipt['units'] if u['case'] == case]
                recovered = sum(u['recovered_text_bytes'] for u in units)
                receipt['families'].append(dict(case=case, recovered_text_bytes=recovered,
                    post_s2_eqsat_saving=eqsat_saving, ratio=recovered/eqsat_saving))
            receipt['stop_decision'] = 'CLOSE_NEGATIVE' if all(
                f['ratio'] < 0.1 for f in receipt['families']) else 'REQUIRES_REVIEWED_SEMANTIC_SLICE'
            receipt['status'] = 'UNVERIFIED_DIAGNOSTIC'
        except BaseException as error:
            receipt.update(status='FAILED', error=str(error))
            raise
        finally:
            if started:
                runner.run('stop-object-vm', [tools['container'], 'stop', vm])
            save()
        return directory, receipt


def run_reference_sizes(plan, root):
    """S5 TU sizes only; no timing or application-correctness claim."""
    root, plan = Path(root).resolve(), Path(plan).resolve()
    raw, m = plan.read_bytes(), load(plan)
    require(m['schema'] == 'evocc-reference-size-plan-v1' and m['phase'] == 'S5', 'not the S5 size plan')
    require(subprocess.check_output(['git', 'show', 'HEAD:'+plan.relative_to(root).as_posix()], cwd=root) == raw,
            'commit S5 before execution')
    container = Path(m['container']['path'])
    require(digest(container.read_bytes()) == m['container']['sha256'], 'container changed')
    for name, expected in m['inputs'].items():
        require(digest((root/name).read_bytes()) == expected, 'S5 input changed: '+name)
    resources = m['resources']
    require(resources['jobs'] == 1 and resources['max_retained_bytes'] <= 256*1024**2
            and shutil.disk_usage(root).free >= resources['min_free_bytes'], 'S5 resource limit')
    cases = {p['id']: p for p in recipes(root)}
    with (root/'.build/bench.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        directory = Path(tempfile.mkdtemp(prefix='S5-', dir=root/'.evidence/eqsat-size'))
        (directory/'manifest.json').write_bytes(raw)
        receipt = dict(schema='evocc-reference-size-receipt-v1', status='STARTED', attempts=[], units=[],
            manifest_digest=digest(raw), timing='NOT_RUN', native_executions=0,
            claim='TU_SIZE_ONLY_NO_PARITY_OR_APPLICATION_PASS')

        def save():
            p = directory/'receipt.json.pending'
            p.write_bytes(json.dumps(receipt, indent=2, sort_keys=True).encode()+b'\n')
            p.replace(directory/'receipt.json')

        runner = Capture(directory, resources, dict(os.environ), receipt, save)
        vm, started = directory.name.lower(), False
        try:
            row = runner.run('image', [container, 'image', 'inspect', m['image']['name']])
            require(load(directory/row['stdout'])[0]['id'] == m['image']['digest'], 'S5 image changed')
            runner.run('start-size-vm', [container, 'run', '--rm', '-d', '--name', vm,
                '--cpus', '2', '--memory', '3G', '--read-only', '--tmpfs', '/tmp', '--no-dns',
                '--mount', f'type=bind,source={root},target=/work,readonly',
                '--mount', f'type=bind,source={directory},target=/runwork',
                m['image']['name'], '/bin/sleep', '3600'])
            started = True
            for compiler in ('gcc-14', 'clang-19'):
                runner.run('version:'+compiler, [container, 'exec', vm, '/usr/bin/'+compiler, '--version'])
            previous = load(root/m['baseline_receipt'])
            for n, unit in enumerate(previous['units']):
                case = cases[unit['case']]
                folder = Path('/work')/case['source_root']
                includes = [] if case['id'] == 'p0' else [folder, folder/'lib']
                source = folder/unit['source']
                result = dict(case=unit['case'], source=unit['source'], eligible_roots=unit['eligible_roots'],
                    evo={}, references={})
                receipt['units'].append(result)
                for arm in ('A', 'B'):
                    result['evo'][arm] = {}
                    for profile in ('checked', 'unchecked'):
                        product = unit['arms'][arm]['profiles'][profile]['on']
                        obj = root/product['object']
                        twin = obj.with_name(obj.name.replace('-0.o', '-1.o'))
                        require_identical_objects(obj.read_bytes(), twin.read_bytes())
                        result['evo'][arm][profile] = dict(metrics=inspect_object_size(obj.read_bytes()),
                            object=product['object'], two_builds='BYTE_IDENTICAL')
                for compiler in ('gcc-14', 'clang-19'):
                    for level in ('Os', 'O2'):
                        objects = []
                        for repeat in (0, 1):
                            obj = directory/f'{n:02}-{compiler}-{level}-{repeat}.o'
                            runner.run(f'compile:{n}:{compiler}:{level}:{repeat}', [container, 'exec', vm,
                                '/usr/bin/'+compiler, '-std=gnu11', '-'+level, '-fPIC', *case['flags'],
                                *['-I'+str(p) for p in includes], '-c', source, '-o', '/runwork/'+obj.name])
                            objects.append(obj)
                        require_identical_objects(objects[0].read_bytes(), objects[1].read_bytes())
                        result['references'][compiler+'-'+level] = dict(
                            object=objects[0].relative_to(root).as_posix(),
                            metrics=inspect_object_size(objects[0].read_bytes()), two_builds='BYTE_IDENTICAL')
                save()
            receipt['status'] = 'OBSERVED_TU_SIZE_COMPARISON'
        except BaseException as error:
            receipt.update(status='FAILED', error=str(error))
            raise
        finally:
            if started:
                runner.run('stop-size-vm', [container, 'stop', vm])
            save()
        return directory, receipt
