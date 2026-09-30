"""Reconstruct steady-state timings and native twin admission from raw capture."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from .eqsat_chain import digest, require
from .eqsat_elf import inspect_elf
from .eqsat_native import pair_order
from .eqsat_pilot_audit import audit_usage
from .eqsat_steady import (GUEST_TOOLS, PROTOCOL, RESOURCES, TASKS, admitted_calibration,
                          audit_plan, build_population, check_build_inputs, load, timing_tasks)
from .eqsat_steady_guest import aa_order, interval, next_iterations, result_value
from .eqsat_twins import admit_unchecked, canonical, native_identity


def audit_capture(directory, root, manifest_digest):
    directory, root = Path(directory).resolve(), Path(root).resolve()
    raw = (directory/'manifest.json').read_bytes()
    require(digest(raw) == manifest_digest, 'wrong steady manifest')
    choices = [p for p in (root/'tests/contracts').glob('*.json') if p.read_bytes() == raw]
    require(len(choices) == 1, 'steady plan is not uniquely committed')
    mode = load(directory/'manifest.json')['mode']
    m, _ = audit_plan(choices[0], root, mode)
    tools = {k: (root/v['path']).resolve() for k, v in m['tools'].items()}
    r = load(directory/'receipt.json')
    require(r['schema'] == 'evocc-r02-steady-capture-v1' and r['mode'] == mode
        and r['manifest_digest'] == manifest_digest and r['status'] == 'OBSERVED_'+mode
        and r['cleanup']['exit_code'] == 0 and r['pmu'] == 'NOT_MEASURED', 'incomplete steady capture')
    pinned = {}
    def read(name):
        file = directory/name
        require(file.resolve().is_relative_to(directory) and not file.is_symlink(), 'unowned steady artifact')
        raw = file.read_bytes()
        require(str(name) not in pinned or pinned[str(name)] == digest(raw), 'steady artifact changed')
        pinned[str(name)] = digest(raw)
        return raw
    read('manifest.json'); read('receipt.json')
    attempts = {}
    for n, row in enumerate(r['attempts']):
        require(row['stage'] not in attempts and row['stdout'] == f'{n:03}.stdout'
            and row['stderr'] == f'{n:03}.stderr' and row['status'] == 'COMPLETED'
            and row['exit_code'] == 0, 'unfinished/duplicated steady command')
        audit_usage(row.get('resources'), 'Darwin')
        for stream in ('stdout', 'stderr'):
            content = read(row[stream])
            require(digest(content) == row[stream+'_sha256'] and len(content) == row[stream+'_bytes'], 'steady stream changed')
        attempts[row['stage']] = row
    used = set()
    def one(stage, argv):
        row = attempts[stage]
        require(stage not in used and row['argv'] == list(map(str, argv)), 'wrong/reused steady command')
        used.add(stage)
        return row
    container = tools['container']; vm = 'evocc-r02-steady-'+directory.name
    image = one('image-identity', [container, 'image', 'inspect', m['image']['name']])
    require(json.loads(read(image['stdout']))[0]['id'] == m['image']['digest'], 'wrong timing image')
    one('start-owned-vm', [container, 'run', '--rm', '-d', '--name', vm, '--cpus', str(RESOURCES['vm_cpus']),
        '--memory', RESOURCES['vm_memory'], '--read-only', '--tmpfs', '/tmp', '--no-dns',
        '--mount', f'type=bind,source={root},target=/work,readonly',
        '--mount', f'type=bind,source={directory},target=/runwork', m['image']['name'],
        '/bin/sleep', str(RESOURCES['campaign_seconds'])])
    row = one('guest-tools', [container, 'exec', vm, '/usr/bin/sha256sum', *GUEST_TOOLS])
    require({s.split()[1]: s.split()[0] for s in read(row['stdout']).decode().splitlines()} == m['guest_tools'],
            'timing tool receipt mismatch')
    def guest(stage, argv):
        return one(stage, [container, 'exec', vm, *argv])
    def mounted(file):
        return '/work/'+Path(file).resolve().relative_to(root).as_posix()
    if mode == 'build':
        expected = build_population(m, root)
        check_build_inputs(m, root, expected)
        require(len(r['programs']) == len(expected), 'steady build population changed')
        from .eqsat_pilot import ROOTS
        for actual, original in zip(r['programs'], expected):
            name = original['case']; folder = directory/name
            for field in ('case', 'units', 'dependencies', 'reference_objects'):
                require(actual[field] == original[field], 'steady source/chain composition changed')
            header = root/ROOTS[name]/('lib' if name == 'zstd' else '')
            define = ['-DUSE_CJSON'] if name == 'cjson' else ['-DUSE_ZSTD'] if name == 'zstd' else []
            harness = folder/'harness.o'
            guest('harness:'+name, ['/usr/bin/gcc-14', '-std=gnu11', '-O2', '-fPIC', *define,
                '-I'+mounted(header), '-c', '/work/tests/fixtures/eqsat-performance/steady-workload.c',
                '-o', '/runwork/'+harness.relative_to(directory).as_posix()])
            require(actual['harness_object'] == harness.relative_to(root).as_posix()
                and actual['harness_digest'] == digest(read(harness.relative_to(directory))), 'harness object changed')
            for variant in ('A', 'B', 'reference'):
                for profile in (('checked', 'unchecked') if variant != 'reference' else ('reference',)):
                    objects = ([root/u['objects'][variant][profile] for u in original['units']]+
                        [root/p for p in original['dependencies']]) if variant != 'reference' else [
                        root/p for p in original['reference_objects']]
                    exe = folder/(variant+'-'+profile)
                    guest('link:'+name+':'+variant+':'+profile, ['/usr/bin/gcc-14', *map(mounted, objects),
                        '/runwork/'+harness.relative_to(directory).as_posix(), '-Wl,--build-id=none', '-lm',
                        '-o', '/runwork/'+exe.relative_to(directory).as_posix()])
                    metrics = inspect_elf(read(exe.relative_to(directory)))
                    value = actual['reference'] if variant == 'reference' else actual['variants'][variant][profile]
                    require(value['executable'] == exe.relative_to(root).as_posix()
                        and value['executable_digest'] == metrics['file_digest'] and value['metrics'] == metrics,
                        'steady executable/metrics changed')
                    if variant != 'reference':
                        require(value['binding'] == original['variants'][variant][profile]['binding'],
                                'steady twins do not share their verified input chain')
            for _, work in [t for t in TASKS if t[0] == name]:
                row = guest('reference:'+name+':'+work, [mounted(root/actual['reference']['executable']), work, '1'])
                result_value(read(row['stdout']), work, 1)
                require(row['stderr_bytes'] == 0, 'reference harness failed')
        result = dict(status='STEADY_CAPTURE_CONSISTENT', mode=mode, programs=3, reference_modes=5,
            timing='NOT_RUN', scope=r['scope'])
    else:
        source = root/m['source']['directory']
        if mode == 'AB':
            calibration = admitted_calibration(load(source/'guest/receipt.json'))
            build_dir = root/load(source/'manifest.json')['source']['directory']
        else:
            calibration, build_dir = None, source
        build = load(build_dir/'receipt.json')
        request = dict(mode=mode, protocol=PROTOCOL, tasks=timing_tasks(build, root),
            backend_digest=m['tools']['ir_backend']['sha256'],
            harness_digest=m['files']['tests/fixtures/eqsat-performance/steady-workload.c'],
            campaign_seconds=RESOURCES['campaign_seconds'], calibration=calibration, guest_kernel=m['guest_kernel'])
        require(read('request.json') == canonical(request)+b'\n', 'timing request/data population changed')
        row = guest('quiet-'+mode, ['/usr/bin/python3', '-B', '-m', 'tools.ladder.eqsat_steady_guest',
            '/runwork/request.json', '/runwork/guest'])
        require(row.get('storage_watch') == 'BATCH_BOUNDARIES_ONLY_PREDECLARED_CHILD_ENVELOPE',
                'host collector was not a quiet batch')
        observed_raw = read('guest/receipt.json'); observed = json.loads(observed_raw)
        require(digest(observed_raw) == r['guest_receipt_digest'], 'guest receipt changed')
        result = audit_guest(observed, request, read)
        require(r.get('calibration') == ('ADMITTED' if all(c['status'] == 'ADMITTED_AA' for c in
            observed['comparisons']) else 'INCONCLUSIVE') if mode == 'AA' else
            r.get('calibration') == 'ALREADY_ADMITTED_AA', 'host calibration verdict changed')
    require(set(attempts) == used, 'unaccounted steady command')
    for name, expected in pinned.items():
        require(digest((directory/name).read_bytes()) == expected, 'steady capture changed during audit')
    audit_plan(choices[0], root, mode)
    return result


def audit_guest(g, request, read):
    mode = request['mode']
    require([(t['case'], t['work']) for t in request['tasks']] == TASKS,
            'guest workload population/order changed')
    require(g['schema'] == 'evocc-r02-steady-guest-v1' and g['status'] == 'OBSERVED_'+mode
        and g['mode'] == mode and g['request_digest'] == digest(canonical(request)+b'\n')
        and g['cpu_affinity'] == [PROTOCOL['cpu']] and g['kernel'] == request['guest_kernel']
        and canonical(g['protocol']) == canonical(PROTOCOL) and g['pmu'] == 'NOT_MEASURED'
        and len(g['comparisons']) == 10, 'incomplete/unpinned guest timing')
    checked = set()
    for n, row in enumerate(g['attempts']):
        require(row['stdout'] == f'{n:03}.stdout' and row['stderr'] == f'{n:03}.stderr'
            and row['status'] == 'RESULT_CHECKED' and row['exit_code'] == 0
            and row.get('storage_watch') == 'BATCH_BOUNDARIES_ONLY_PREDECLARED_CHILD_ENVELOPE'
            and row.get('result_contract') == 'STEADY_TYPED_RESULT_V1', 'invalid guest result/quiet mode')
        audit_usage(row.get('resources'), 'Linux')
        for stream in ('stdout', 'stderr'):
            content = read('guest/'+row[stream])
            require(digest(content) == row[stream+'_sha256'] and len(content) == row[stream+'_bytes'],
                    'guest raw output changed')
        require(row['stderr_bytes'] == 0, 'timed child diagnostic')
    chronology = []
    def execution(task, variant, profile, count, value, stage, chronological=True):
        n = value['attempt']; require(type(n) is int and 0 <= n < len(g['attempts']), 'missing guest attempt')
        if chronological:
            require(n == len(chronology), 'raw attempt chronology contradicts the frozen execution order')
            chronology.append(n)
        row = g['attempts'][n]; p = task['variants'][variant][profile]
        actual = result_value(read('guest/'+row['stdout']), task['work'], count)
        identity = native_identity(p['binding'], profile, 'linux-aarch64', request['backend_digest'],
            p['executable_digest'], digest(canonical(dict(harness=request['harness_digest'], work=task['work'], iterations=count))))
        require(row['argv'] == [p['executable'], task['work'], str(count)] and row['stage'] == stage
            and actual == value['value'] and identity == value['identity']
            and row['native_identity'] == identity['native_identity'], 'sample executable/input/clock identity changed')
        checked.add(n)
        return actual['elapsed_ns']
    admission_keys = set()
    task_ids = {t['id'] for t in request['tasks']}
    require(all(p['task'] in task_ids for p in g['probes']), 'unaccounted duration probe')
    for a in g['admissions']:
        require(a['task'] in task_ids, 'unaccounted admission task')
        task_index = next(i for i, t in enumerate(request['tasks']) if t['id'] == a['task'])
        task = request['tasks'][task_index]
        count, variant = a['iterations'], a['variant']
        row = g['attempts'][a['checked']['attempt']]
        # The probe/admit stage is also checked below in its chronological slot.
        execution(task, variant, 'checked', count, a['checked'], row['stage'], chronological=False)
        p = task['variants'][variant]['unchecked']
        identity = native_identity(p['binding'], 'unchecked', 'linux-aarch64', request['backend_digest'],
            p['executable_digest'], a['checked']['identity']['input_digest'])
        actual = admit_unchecked(a['checked']['identity'], identity, row,
            {k: row[k] for k in ('exit_code', 'stdout_sha256', 'stderr_sha256')})
        key = (a['task'], variant, count)
        require(key not in admission_keys and a['admission'] == actual, 'duplicate/forged checked-twin admission')
        admission_keys.add(key)
    sample_count = 0; outcomes = []
    for i, task in enumerate(request['tasks']):
        if mode == 'AA':
            probes = [p for p in g['probes'] if p['task'] == task['id']]
            count = {'zlib': 16, 'zstd': 256, 'cjson': 8192}[task['case']]
            require(1 <= len(probes) <= PROTOCOL['probe_rounds'], 'duration probes missing/unbounded')
            for n, p in enumerate(probes):
                require(p['iterations'] == count, 'duration selection does not follow A-only frozen rule')
                low = min(execution(task, 'A', 'checked', count, p['checked'], f'probe:{i}:{n}:checked'),
                          execution(task, 'A', 'unchecked', count, p['unchecked'], f'probe:{i}:{n}:unchecked'))
                require((task['id'], 'A', count) in admission_keys and p['checked']['attempt'] <
                    p['unchecked']['attempt'], 'unchecked duration probe preceded its checked twin')
                if n+1 < len(probes):
                    require(low < PROTOCOL['minimum_ns'], 'unregistered extra duration probe')
                    count = next_iterations(count, low)
                else:
                    require(low >= PROTOCOL['minimum_ns'], 'duration probe never reached steady-state floor')
        else:
            count = request['calibration'][task['id']]['iterations']
            for v in ('A', 'B'):
                matches = [a for a in g['admissions'] if (a['task'], a['variant'], a['iterations']) == (task['id'], v, count)]
                require(len(matches) == 1, 'A/B admission missing')
                execution(task, v, 'checked', count, matches[0]['checked'], f'admit:{i}:{v}:checked')
        expected_admissions = {(task['id'], 'A', p['iterations']) for p in probes} if mode == 'AA' else {
            (task['id'], v, count) for v in ('A', 'B')}
        require({k for k in admission_keys if k[0] == task['id']} == expected_admissions,
                'unregistered extra checked-twin admission')
        for profile_index, profile in enumerate(('checked', 'unchecked')):
            c = g['comparisons'][2*i+profile_index]
            require((c['task'], c['case'], c['work'], c['profile'], c['iterations']) ==
                (task['id'], task['case'], task['work'], profile, count), 'comparison population/input changed')
            seed = PROTOCOL['seed']+2*i+profile_index
            orders = aa_order(seed) if mode == 'AA' else pair_order(request['calibration'][task['id']]['pairs'][profile], seed+100)
            require(len(c['samples']) == 2*len(c['pairs']) and len(c['pairs']) in PROTOCOL['pair_counts']
                and (mode == 'AA' or len(c['pairs']) == len(orders)), 'sample/pair count changed')
            pairs, looks = [], []
            for n, order in enumerate(orders[:len(c['pairs'])]):
                pair = {}
                for offset, label in enumerate(order):
                    s = c['samples'][2*n+offset]
                    v = 'A' if mode == 'AA' or label == 'A0' else 'B'
                    require((s['pair'], s['label'], s['variant']) == (n, label, v), 'sample order/variant changed')
                    pair[label] = execution(task, v, profile, count, s, f'sample:{i}:{profile}:{n}:{label}')
                    admitted = [a for a in g['admissions'] if (a['task'], a['variant'], a['iterations']) == (task['id'], v, count)]
                    require(len(admitted) == 1 and admitted[0]['checked']['attempt'] < s['attempt'],
                            'timed execution preceded its checked input admission')
                pairs.append(pair)
                if mode == 'AA' and n+1 in PROTOCOL['pair_counts']:
                    stat = interval(pairs, True)
                    duration = all(PROTOCOL['minimum_ns'] <= p[x] <= PROTOCOL['maximum_ns']
                        for p in pairs for x in ('A0', 'A1'))
                    looks.append(dict(pairs=n+1, statistics=stat, duration_gate=duration))
                    require(not (stat['calibration'] == 'WITHIN_EFFECT_FLOOR' and duration) or
                            n+1 == len(c['pairs']), 'calibration continued after its stopping rule')
            stat = interval(pairs, mode == 'AA')
            duration = all(PROTOCOL['minimum_ns'] <= p[x] <= PROTOCOL['maximum_ns'] for p in pairs for x in ('A0', 'A1'))
            status = ('ADMITTED_AA' if stat['calibration'] == 'WITHIN_EFFECT_FLOOR' and duration else
                'INCONCLUSIVE_AA') if mode == 'AA' else ('OBSERVED_AB' if duration else 'INCONCLUSIVE_DURATION')
            require(c['pairs'] == pairs and c['looks'] == looks and c['statistics'] == stat
                and c['duration_gate'] == duration and c['status'] == status, 'timing statistics/qualification forged')
            sample_count += len(c['samples']); outcomes.append(dict(task=c['task'], profile=profile,
                pairs=len(pairs), iterations=count, status=status, statistics=stat))
    require(chronology == list(range(len(g['attempts']))) and
            checked == set(range(len(g['attempts']))) and (mode != 'AB' or not g['probes']),
            'unaccounted guest attempt/probe')
    return dict(status='STEADY_CAPTURE_CONSISTENT', mode=mode, timed_samples=sample_count,
        comparisons=outcomes, admitted_aa=(all(o['status'] == 'ADMITTED_AA' for o in outcomes) if mode == 'AA' else None),
        metric=PROTOCOL['metric'], pmu='NOT_MEASURED',
        scope='THREE_LIBRARIES_FIVE_MODES_MIXED_LINK_LINUX_VM_NOT_FINAL_Q2_OR_AI')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path); parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--manifest-sha256', required=True)
    args = parser.parse_args()
    print(json.dumps(audit_capture(args.directory, args.root, args.manifest_sha256), sort_keys=True))


if __name__ == '__main__':
    main()
