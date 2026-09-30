"""Linux-specific steady-state R02 timing, separate from deterministic preflight.

The registered harness excludes setup/startup, repeats in process, uses a guest
CPU affinity and captures one quiet batch. A/A selects repeats and sample counts
before a separately committed A/B plan. Millisecond fresh-process data is never
promoted to this protocol or used to choose a candidate.
"""
from __future__ import annotations

import fcntl
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from .eqsat_chain import digest, keys, require, unique
from .eqsat_census import check_source_revision
from .eqsat_elf import inspect_elf
from .eqsat_native import Capture, host_identity
from .eqsat_pilot import GUEST_TOOLS, ROOTS, composite_binding
from .eqsat_steady_guest import PROTOCOL
from .eqsat_twins import bind_bundle, canonical

FILES = ('tools/bench.py', 'tools/ladder/eqsat_steady.py', 'tools/ladder/eqsat_steady_guest.py',
    'tools/ladder/eqsat_steady_audit.py', 'tools/ladder/eqsat_native.py', 'tools/ladder/eqsat_census.py',
    'tools/ladder/eqsat_chain.py', 'tools/ladder/eqsat_twins.py', 'tools/ladder/eqsat_pilot.py',
    'tools/ladder/eqsat_elf.py', 'tests/fixtures/eqsat-performance/steady-workload.c',
    'docs/evoir-spec.md', 'docs/evomir-spec.md')
RESOURCES = dict(jobs=1, timeout_seconds=300, output_bytes=16*1024**2,
    min_free_bytes=8*1024**3, max_retained_bytes=256*1024**2, campaign_seconds=7200,
    batch_timeout_seconds=7200, vm_cpus=2, vm_memory='3G')
TASKS = [('zlib', 'compress'), ('zlib', 'decompress'), ('zstd', 'compress'),
         ('zstd', 'decompress'), ('cjson', 'json')]


def load(path):
    return json.loads(Path(path).read_bytes(), object_pairs_hook=unique)


def validate_plan(m, mode):
    keys(m, 'schema mode implementation_commit source tools files image guest_tools host guest_kernel resources protocol')
    require(mode in ('build', 'AA', 'AB') and m['schema'] == 'evocc-r02-steady-plan-v1'
            and m['mode'] == mode, 'wrong steady-state plan/mode')
    keys(m['source'], 'kind directory files')
    require(m['source']['kind'] == {'build': 'DETERMINISTIC_COMPLETION', 'AA': 'STEADY_BUILD',
        'AB': 'ADMITTED_STEADY_AA'}[mode], 'wrong prerequisite stage')
    require(canonical(m['resources']) == canonical(RESOURCES) and
            canonical(m['protocol']) == canonical(PROTOCOL), 'unregistered timing bounds/controls')
    require(set(m['files']) == set(FILES) and set(m['guest_tools']) == set(GUEST_TOOLS),
            'missing timing collector/harness/runtime identity')
    keys(m['tools'], 'container collector_python checker historical_checker ir_backend')
    require(m['source']['files'], 'unbound timing prerequisite')
    require(m['host'] == host_identity() and m['guest_kernel'] == '6.18.15',
            'host/guest identity differs from the registered Linux lane')


def audit_plan(path, root, mode):
    path, root = Path(path).resolve(), Path(root).resolve()
    raw, m = path.read_bytes(), load(path)
    validate_plan(m, mode)
    relative = path.relative_to(root).as_posix()
    saved = subprocess.run(['git', 'show', 'HEAD:'+relative], cwd=root, capture_output=True, check=False)
    require(saved.returncode == 0 and saved.stdout == raw, 'commit timing plan before execution')
    check_source_revision(root, m['implementation_commit'])
    require(Path(sys.executable).resolve() == (root/m['tools']['collector_python']['path']).resolve(),
            'wrong timing collector interpreter')
    for item in m['tools'].values():
        require(digest((root/item['path']).read_bytes()) == item['sha256'], 'timing tool changed')
    for files in (m['files'], m['source']['files']):
        for name, expected in files.items():
            require((root/name).resolve().is_relative_to(root) and
                    digest((root/name).read_bytes()) == expected, 'timing input changed: '+name)
    if mode in ('AA', 'AB'):
        previous = root/m['source']['directory']
        required = {p.relative_to(root).as_posix() for p in previous.rglob('*') if p.is_file()}
        inherited = load(previous/'manifest.json')['source']['files']
        require(required <= set(m['source']['files']) and all(m['source']['files'].get(k) == v
                for k, v in inherited.items()), 'timing prerequisite inventory omitted/changed')
    return m, raw


def build_population(m, root):
    """Use the default policy, never a winner selected from native timing."""
    completion = root/m['source']['directory']
    r, cm = load(completion/'receipt.json'), load(completion/'manifest.json')
    audit = load(completion/'audit.json')
    require(r['status'] == 'OBSERVED_DETERMINISTIC_COMPLETION' and audit['status'] ==
        'DETERMINISTIC_CAPTURE_CONSISTENT' and audit['policy_evaluations'] == 1024
        and audit['verified_chains'] == 6144 and audit['unattempted_templates'] == 0
        and r['ai_campaign'] == 'NOT_ADMITTED', 'deterministic prerequisite incomplete/unaudited')
    selected = [e for e in r['policy']['evaluations'] if e['weights'] == [1]*13
                and e['max_transactions'] == 8]
    require(len(selected) == 1, 'default-policy composition missing/ambiguous')
    historical = root/cm['source']['directory']
    old = load(historical/'manifest.json')
    programs = []
    for case in old['cases']:
        if case['id'] == 'p0':
            continue  # P0 is a no-change plumbing control, not a timed library.
        units = [u for u in selected[0]['units'] if u['case'] == case['id']]
        require([u['source'] for u in units] == case['subject_sources'], 'steady subject coverage changed')
        program = dict(case=case['id'], units=[], dependencies=[], reference_objects=[], variants={})
        for n, source in enumerate(case['sources']):
            obj = historical/case['id']/f'ref-{n:02}.o'
            program['reference_objects'].append(obj.relative_to(root).as_posix())
            if source not in case['subject_sources']:
                program['dependencies'].append(obj.relative_to(root).as_posix())
        for unit in units:
            number = case['sources'].index(unit['source'])
            chains = dict(A=historical/case['id']/f'{number:02}-A-chain', B=completion/unit['bundle'])
            bindings = {v: bind_bundle(chain, root/m['tools']['historical_checker' if v == 'A' else 'checker']['path'])
                        for v, chain in chains.items()}
            require(bindings['A']['ir_digest'] == load(chains['B']/'chain.json')['input_digest']
                    and bindings['B']['ir_digest'] == unit['binding']['ir_digest'], 'A/B verified inputs changed')
            objects = {}
            for variant, binding in bindings.items():
                objects[variant] = {}
                for profile in ('checked', 'unchecked'):
                    key = digest(canonical((binding['ir_digest'], profile)))
                    code = r['code'][key]
                    obj = completion/code['object']
                    require(code['ir_digest'] == binding['ir_digest'] and code['profile'] == profile
                        and code['object_digest'] == digest(obj.read_bytes()), 'wrong steady backend object')
                    objects[variant][profile] = obj.relative_to(root).as_posix()
            program['units'].append(dict(source=unit['source'], chains={v: p.relative_to(root).as_posix()
                for v, p in chains.items()}, bindings=bindings, objects=objects))
        for variant in ('A', 'B'):
            binding = composite_binding({u['source']: u['bindings'][variant] for u in program['units']})
            program['variants'][variant] = {p: dict(binding=binding) for p in ('checked', 'unchecked')}
        programs.append(program)
    require([p['case'] for p in programs] == ['zlib', 'zstd', 'cjson'], 'wrong library population')
    return programs


def check_build_inputs(m, root, programs):
    """Every recipe read/link/include must have a predeclared immutable pin."""
    root = Path(root).resolve(); required = set()
    source = root/m['source']['directory']
    required.update(source/name for name in ('manifest.json', 'receipt.json', 'audit.json'))
    for program in programs:
        required.update(root/p for p in program['reference_objects'])
        for unit in program['units']:
            for chain in unit['chains'].values():
                required.update(p for p in (root/chain).iterdir() if p.is_file())
            for profiles in unit['objects'].values():
                required.update(root/p for p in profiles.values())
        header_root = root/ROOTS[program['case']]
        require(header_root.is_dir(), 'missing declared library headers')
        headers = list(header_root.rglob('*.h'))
        require(headers, 'empty declared header inventory')
        required.update(headers)
    pins = m['source']['files']
    for p in required:
        require(p.resolve().is_relative_to(root) and not p.is_symlink(), 'unowned steady build input')
        name = p.relative_to(root).as_posix()
        require(name in pins and pins[name] == digest(p.read_bytes()), 'unbound steady build input: '+name)


def timing_tasks(build, root):
    tasks = []
    for case, work in TASKS:
        program = next(p for p in build['programs'] if p['case'] == case)
        variants = {v: {p: dict(x, executable='/work/'+x['executable']) for p, x in profiles.items()}
                    for v, profiles in program['variants'].items()}
        tasks.append(dict(id=case+':'+work, case=case, work=work, variants=variants))
    return tasks


def admitted_calibration(guest):
    require(guest['status'] == 'OBSERVED_AA' and len(guest['comparisons']) == 10
        and all(c['status'] == 'ADMITTED_AA' for c in guest['comparisons']),
        'Linux A/A did not admit every frozen comparison; A/B forbidden')
    result = {}
    for c in guest['comparisons']:
        row = result.setdefault(c['task'], dict(iterations=c['iterations'], pairs={}))
        require(row['iterations'] == c['iterations'], 'profiles use different input repetition')
        row['pairs'][c['profile']] = len(c['pairs'])
    require(set(result) == {a+':'+b for a, b in TASKS} and
            all(set(c['pairs']) == {'checked', 'unchecked'} for c in result.values()), 'incomplete A/A population')
    return result


def run_plan(path, root, mode):
    root = Path(root).resolve()
    m, raw = audit_plan(path, root, mode)
    tools = {k: (root/v['path']).resolve() for k, v in m['tools'].items()}
    with (root/'.build/bench.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError('another benchmark owns the shared lock') from error
        require(shutil.disk_usage(root).free >= RESOURCES['min_free_bytes'], 'timing disk reserve unavailable')
        base = root/'.evidence/eqsat-steady'; base.mkdir(exist_ok=True)
        directory = Path(tempfile.mkdtemp(prefix=mode.lower()+'-', dir=base))
        (directory/'manifest.json').write_bytes(raw)
        receipt = dict(schema='evocc-r02-steady-capture-v1', status='STARTED', mode=mode,
            manifest_digest=digest(raw), attempts=[], programs=[], pmu='NOT_MEASURED',
            scope='THREE_LIBRARIES_FIVE_MODES_MIXED_LINK_LINUX_HARDWARE_VM_NO_AI',
            native_timing='NOT_RUN' if mode == 'build' else PROTOCOL['metric'])
        def save():
            tmp = directory/'receipt.json.pending'
            tmp.write_bytes(canonical(receipt)+b'\n'); tmp.replace(directory/'receipt.json')
        environment = dict(PATH='/usr/bin:/bin:/usr/local/bin', LANG='C', LC_ALL='C', TMPDIR=str(directory))
        runner = Capture(directory, {k: v for k, v in RESOURCES.items() if k not in
            ('jobs', 'campaign_seconds', 'vm_cpus', 'vm_memory')}, environment, receipt, save, measure_resources=True)
        vm, started = 'evocc-r02-steady-'+directory.name, False
        def guest_path(p):
            return '/work/'+Path(p).resolve().relative_to(root).as_posix()
        def guest(stage, argv, quiet=False):
            return runner.run(stage, [tools['container'], 'exec', vm, *argv], quiet=quiet)
        save()
        try:
            row = runner.run('image-identity', [tools['container'], 'image', 'inspect', m['image']['name']])
            require(load(directory/row['stdout'])[0]['id'] == m['image']['digest'], 'timing image changed')
            started = True
            runner.run('start-owned-vm', [tools['container'], 'run', '--rm', '-d', '--name', vm,
                '--cpus', str(RESOURCES['vm_cpus']), '--memory', RESOURCES['vm_memory'], '--read-only',
                '--tmpfs', '/tmp', '--no-dns', '--mount', f'type=bind,source={root},target=/work,readonly',
                '--mount', f'type=bind,source={directory},target=/runwork', m['image']['name'],
                '/bin/sleep', str(RESOURCES['campaign_seconds'])])
            row = guest('guest-tools', ['/usr/bin/sha256sum', *GUEST_TOOLS])
            require({s.split()[1]: s.split()[0] for s in (directory/row['stdout']).read_text().splitlines()} ==
                    m['guest_tools'], 'timing guest tools changed')
            if mode == 'build':
                receipt['programs'] = build_population(m, root)
                check_build_inputs(m, root, receipt['programs'])
                for program in receipt['programs']:
                    name = program['case']; folder = directory/name; folder.mkdir()
                    header = root/ROOTS[name]/('lib' if name == 'zstd' else '')
                    harness = folder/'harness.o'
                    define = ['-DUSE_CJSON'] if name == 'cjson' else ['-DUSE_ZSTD'] if name == 'zstd' else []
                    guest('harness:'+name, ['/usr/bin/gcc-14', '-std=gnu11', '-O2', '-fPIC', *define,
                        '-I'+guest_path(header), '-c', '/work/tests/fixtures/eqsat-performance/steady-workload.c',
                        '-o', '/runwork/'+harness.relative_to(directory).as_posix()])
                    program['harness_object'] = harness.relative_to(root).as_posix()
                    program['harness_digest'] = digest(harness.read_bytes())
                    for variant in ('A', 'B', 'reference'):
                        for profile in (('checked', 'unchecked') if variant != 'reference' else ('reference',)):
                            objects = ([root/u['objects'][variant][profile] for u in program['units']]+
                                [root/p for p in program['dependencies']]) if variant != 'reference' else [
                                root/p for p in program['reference_objects']]
                            executable = folder/(variant+'-'+profile)
                            guest('link:'+name+':'+variant+':'+profile, ['/usr/bin/gcc-14', *map(guest_path, objects),
                                '/runwork/'+harness.relative_to(directory).as_posix(), '-Wl,--build-id=none', '-lm',
                                '-o', '/runwork/'+executable.relative_to(directory).as_posix()])
                            value = dict(executable=executable.relative_to(root).as_posix(),
                                executable_digest=digest(executable.read_bytes()), metrics=inspect_elf(executable.read_bytes()))
                            if variant == 'reference':
                                program['reference'] = value
                            else:
                                program['variants'][variant][profile].update(value)
                    for _, work in [t for t in TASKS if t[0] == name]:
                        # A new clock harness still needs an original-library
                        # reference correctness check before any EvoCC sample.
                        row = guest('reference:'+name+':'+work, [guest_path(root/program['reference']['executable']), work, '1'])
                        from .eqsat_steady_guest import result_value
                        result_value((directory/row['stdout']).read_bytes(), work, 1)
                        require(row['stderr_bytes'] == 0, 'reference harness diagnostic')
                receipt['status'] = 'OBSERVED_BUILD'
            else:
                source = root/m['source']['directory']
                if mode == 'AB':
                    previous = load(source/'receipt.json')
                    require(previous['status'] == 'OBSERVED_AA' and load(source/'audit.json')['status'] ==
                            'STEADY_CAPTURE_CONSISTENT', 'unaudited A/A prerequisite')
                    calibration = admitted_calibration(load(source/'guest/receipt.json'))
                    build_directory = root/load(source/'manifest.json')['source']['directory']
                else:
                    build_directory, calibration = source, None
                build = load(build_directory/'receipt.json')
                require(build['status'] == 'OBSERVED_BUILD' and load(build_directory/'audit.json')['status'] ==
                        'STEADY_CAPTURE_CONSISTENT', 'unaudited build prerequisite')
                request = dict(mode=mode, protocol=PROTOCOL, tasks=timing_tasks(build, root),
                    backend_digest=m['tools']['ir_backend']['sha256'],
                    harness_digest=m['files']['tests/fixtures/eqsat-performance/steady-workload.c'],
                    campaign_seconds=RESOURCES['campaign_seconds'], calibration=calibration,
                    guest_kernel=m['guest_kernel'])
                (directory/'request.json').write_bytes(canonical(request)+b'\n')
                # Only pipe/deadline checks remain during this entire batch.
                # Hashes, recursive retention scans and source checks occur
                # before and after it, never concurrently with C hot loops.
                runner.resources['timeout_seconds'] = RESOURCES['batch_timeout_seconds']
                row = guest('quiet-'+mode, ['/usr/bin/python3', '-B', '-m', 'tools.ladder.eqsat_steady_guest',
                    '/runwork/request.json', '/runwork/guest'], quiet=True)
                runner.resources['timeout_seconds'] = RESOURCES['timeout_seconds']
                observed = load(directory/'guest/receipt.json')
                require(observed['status'] == 'OBSERVED_'+mode, 'guest timing batch incomplete')
                receipt.update(status='OBSERVED_'+mode, guest_receipt_digest=digest((directory/'guest/receipt.json').read_bytes()),
                    calibration='ADMITTED' if mode == 'AA' and all(c['status'] == 'ADMITTED_AA' for c in
                        observed['comparisons']) else 'INCONCLUSIVE' if mode == 'AA' else 'ALREADY_ADMITTED_AA')
            audit_plan(path, root, mode)
            runner.watch()
        except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
            receipt.update(status='FAILED', error=str(error))
        except BaseException:
            receipt['status'] = 'INTERRUPTED'
            raise
        finally:
            if started:
                try:
                    clean = subprocess.run([tools['container'], 'stop', vm], capture_output=True, timeout=30, check=False)
                    receipt['cleanup'] = dict(exit_code=clean.returncode, stdout=clean.stdout.decode(errors='backslashreplace'),
                                             stderr=clean.stderr.decode(errors='backslashreplace'))
                    if clean.returncode != 0:
                        receipt['status'] = 'FAILED'
                except (OSError, subprocess.SubprocessError) as error:
                    receipt.update(status='FAILED', cleanup=dict(error=str(error)))
            save()
        return directory, receipt
