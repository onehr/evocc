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


def load(path):
    return json.loads(Path(path).read_bytes(), object_pairs_hook=unique)


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
