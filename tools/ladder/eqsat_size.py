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
import shutil
import subprocess
import tempfile

from .eqsat_chain import digest, require, unique
from .eqsat_elf import inspect_object_size, inspect_elf, require_identical_objects
from .eqsat_native import Capture
from .eqsat_twins import bind_bundle, canonical, stable_bundle
from .eqsat_pilot import recipes


def load(path):
    return json.loads(Path(path).read_bytes(), object_pairs_hook=unique)


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
