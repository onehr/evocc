"""Quiet, CPU-pinned Linux steady-state batches; startup is not the metric."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import statistics
import time

from .eqsat_chain import digest, keys, natural, require, unique
from .eqsat_native import Capture, pair_order
from .eqsat_pilot import pilot_statistics
from .eqsat_twins import admit_unchecked, canonical, native_identity

PROTOCOL = dict(cpu=0, seed=20260930, target_ns=2000000000, minimum_ns=1000000000,
    maximum_ns=8000000000, maximum_iterations=4194304, probe_rounds=4,
    pair_counts=[12, 24, 48], comparison_family=10, calibration_looks=3,
    effect_floor_percent=5, precision_percent=5, retries=0,
    metric='HARNESS_CLOCK_MONOTONIC_RAW_HOT_LOOP_ONLY', warmup='ITERATIONS_DIV_32_PLUS_1',
    host_collection='QUIET_BATCH_BOUNDARIES_ONLY', fitness='NOT_USED_FOR_SEARCH')


def result_value(raw, work, iterations):
    value = json.loads(raw, object_pairs_hook=unique)
    keys(value, 'result work iterations elapsed_ns')
    require(value['result'] == 'validated' and value['work'] == work
            and natural(value['iterations']) == iterations, 'steady result/input mismatch')
    require(0 < natural(value['elapsed_ns']) < 1 << 63, 'invalid steady clock observation')
    return value


def next_iterations(iterations, elapsed):
    natural(iterations); natural(elapsed)
    require(0 < iterations <= PROTOCOL['maximum_iterations'] and elapsed > 0, 'invalid duration probe')
    # Integer arithmetic, upward quantization; no candidate-B timing participates.
    value = (iterations*PROTOCOL['target_ns']*105 + elapsed*100-1)//(elapsed*100)
    value = max(iterations+1, ((value+255)//256)*256)
    require(value <= PROTOCOL['maximum_iterations'], 'duration requires more than frozen repeat cap')
    return value


def aa_order(seed):
    # Every permitted look is balanced, not merely the final 48-pair sequence.
    return pair_order(12, seed)+pair_order(12, seed+1)+pair_order(24, seed+2)


def interval(pairs, calibration):
    family = PROTOCOL['comparison_family']*(PROTOCOL['calibration_looks'] if calibration else 1)
    row = pilot_statistics(pairs, PROTOCOL, family)
    row['metric'] = PROTOCOL['metric']
    row['inference'] = 'LINUX_VM_STEADY_AA_CALIBRATION' if calibration else 'LINUX_VM_FROZEN_AB_PILOT'
    row['sample_cv'] = {label: statistics.stdev(p[label] for p in pairs)/
        statistics.mean(p[label] for p in pairs) for label in ('A0', 'A1')}
    bounds = row['ci_percent_of_control_median']
    row['outcome'] = 'INCONCLUSIVE'
    if not calibration and row['precision_target_met']:
        if bounds[1] < -PROTOCOL['effect_floor_percent']:
            row['outcome'] = 'MATERIAL_IMPROVEMENT'
        elif bounds[0] > PROTOCOL['effect_floor_percent']:
            row['outcome'] = 'MATERIAL_REGRESSION'
        elif all(abs(x) <= PROTOCOL['effect_floor_percent'] for x in bounds):
            row['outcome'] = 'NO_MATERIAL_EFFECT_AT_DECLARED_FLOOR'
    return row


def run(request_path, directory):
    require(platform.system() == 'Linux' and platform.machine() == 'aarch64', 'wrong timing guest')
    request = json.loads(Path(request_path).read_bytes(), object_pairs_hook=unique)
    require(canonical(request['protocol']) == canonical(PROTOCOL) and request['mode'] in ('AA', 'AB'),
            'unregistered guest timing protocol')
    require(platform.release() == request['guest_kernel'], 'guest kernel changed')
    require([(t['case'], t['work']) for t in request['tasks']] ==
            [('zlib', 'compress'), ('zlib', 'decompress'), ('zstd', 'compress'),
             ('zstd', 'decompress'), ('cjson', 'json')], 'wrong steady workload order')
    directory = Path(directory).resolve(); directory.mkdir()
    cpu = PROTOCOL['cpu']
    require(cpu in os.sched_getaffinity(0), 'requested guest CPU is not available')
    os.sched_setaffinity(0, {cpu})
    require(os.sched_getaffinity(0) == {cpu}, 'guest CPU pinning failed')
    receipt = dict(schema='evocc-r02-steady-guest-v1', status='STARTED', attempts=[],
        mode=request['mode'], request_digest=digest(Path(request_path).read_bytes()),
        cpu_affinity=[cpu], kernel=platform.release(), comparisons=[], probes=[], admissions=[],
        protocol=PROTOCOL, pmu='NOT_MEASURED', host_interference='UNCONTROLLED_OUTSIDE_GUEST_VM')
    def save():
        temporary = directory/'receipt.json.pending'
        temporary.write_bytes(canonical(receipt)+b'\n'); temporary.replace(directory/'receipt.json')
    resources = dict(timeout_seconds=15, output_bytes=16384,
        min_free_bytes=8*1024**3, max_retained_bytes=64*1024**2)
    capture = Capture(directory, resources, dict(PATH='/usr/bin:/bin', LANG='C', LC_ALL='C', TMPDIR='/tmp'),
                      receipt, save, measure_resources=True)
    deadline = time.monotonic()+request['campaign_seconds']
    identities = {p['executable']: p['executable_digest'] for t in request['tasks']
                  for variant in t['variants'].values() for p in variant.values()}
    def stable():
        require(os.sched_getaffinity(0) == {cpu}, 'timing affinity changed')
        require(all(digest(Path(p).read_bytes()) == h for p, h in identities.items()), 'timing artifact changed')
    def execute(task, variant, profile, count, stage):
        require(time.monotonic() < deadline, 'steady batch watchdog expired')
        require(os.sched_getaffinity(0) == {cpu}, 'timing affinity changed')
        program = task['variants'][variant][profile]
        row = capture.run(stage, [program['executable'], task['work'], str(count)], quiet=True)
        value = result_value((directory/row['stdout']).read_bytes(), task['work'], count)
        require(row['stderr_bytes'] == 0, 'unexpected steady diagnostic')
        input_hash = digest(canonical(dict(harness=request['harness_digest'], work=task['work'], iterations=count)))
        identity = native_identity(program['binding'], profile, 'linux-aarch64',
            request['backend_digest'], program['executable_digest'], input_hash)
        row.update(status='RESULT_CHECKED', native_identity=identity['native_identity'],
                   result_contract='STEADY_TYPED_RESULT_V1')
        return dict(attempt=len(receipt['attempts'])-1, value=value, identity=identity)
    def admit(task, variant, count, purpose):
        checked = execute(task, variant, 'checked', count, purpose+':checked')
        program = task['variants'][variant]['unchecked']
        unchecked = native_identity(program['binding'], 'unchecked', 'linux-aarch64',
            request['backend_digest'], program['executable_digest'], checked['identity']['input_digest'])
        row = receipt['attempts'][checked['attempt']]
        admission = admit_unchecked(checked['identity'], unchecked, row,
            {k: row[k] for k in ('exit_code', 'stdout_sha256', 'stderr_sha256')})
        receipt['admissions'].append(dict(task=task['id'], variant=variant, iterations=count,
                                         checked=checked, admission=admission))
        return checked
    save()
    try:
        stable()
        require(len(request['tasks']) == 5, 'not the declared three-library/five-mode population')
        for task_index, task in enumerate(request['tasks']):
            if request['mode'] == 'AA':
                count = {'zlib': 16, 'zstd': 256, 'cjson': 8192}[task['case']]
                for probe in range(PROTOCOL['probe_rounds']):
                    checked = admit(task, 'A', count, f'probe:{task_index}:{probe}')
                    unchecked = execute(task, 'A', 'unchecked', count, f'probe:{task_index}:{probe}:unchecked')
                    receipt['probes'].append(dict(task=task['id'], iterations=count, checked=checked, unchecked=unchecked))
                    elapsed = min(checked['value']['elapsed_ns'], unchecked['value']['elapsed_ns'])
                    if elapsed >= PROTOCOL['minimum_ns']:
                        break
                    count = next_iterations(count, elapsed)
                else:
                    raise ValueError('A-only duration calibration exhausted its frozen rounds')
            else:
                count = request['calibration'][task['id']]['iterations']
                for variant in ('A', 'B'):
                    admit(task, variant, count, f'admit:{task_index}:{variant}')
            for profile in ('checked', 'unchecked'):
                comparison = dict(task=task['id'], case=task['case'], work=task['work'], profile=profile,
                    iterations=count, samples=[], pairs=[], looks=[], status='STARTED')
                receipt['comparisons'].append(comparison)
                seed = PROTOCOL['seed']+2*task_index+(profile == 'unchecked')
                orders = aa_order(seed) if request['mode'] == 'AA' else pair_order(
                    request['calibration'][task['id']]['pairs'][profile], seed+100)
                for n, order in enumerate(orders):
                    pair = {}
                    for label in order:
                        variant = 'A' if request['mode'] == 'AA' or label == 'A0' else 'B'
                        sample = execute(task, variant, profile, count, f'sample:{task_index}:{profile}:{n}:{label}')
                        comparison['samples'].append(dict(pair=n, label=label, variant=variant, **sample))
                        pair[label] = sample['value']['elapsed_ns']
                    comparison['pairs'].append(pair)
                    if request['mode'] == 'AA' and n+1 in PROTOCOL['pair_counts']:
                        stats = interval(comparison['pairs'], True)
                        duration = all(PROTOCOL['minimum_ns'] <= s['value']['elapsed_ns'] <=
                                       PROTOCOL['maximum_ns'] for s in comparison['samples'])
                        comparison['looks'].append(dict(pairs=n+1, statistics=stats, duration_gate=duration))
                        if stats['calibration'] == 'WITHIN_EFFECT_FLOOR' and duration:
                            break
                comparison['statistics'] = interval(comparison['pairs'], request['mode'] == 'AA')
                comparison['duration_gate'] = all(PROTOCOL['minimum_ns'] <= s['value']['elapsed_ns'] <=
                    PROTOCOL['maximum_ns'] for s in comparison['samples'])
                comparison['status'] = ('ADMITTED_AA' if comparison['statistics']['calibration'] ==
                    'WITHIN_EFFECT_FLOOR' and comparison['duration_gate'] else 'INCONCLUSIVE_AA') if request['mode'] == 'AA' else (
                    'OBSERVED_AB' if comparison['duration_gate'] else 'INCONCLUSIVE_DURATION')
                save()
        stable()
        receipt['status'] = 'OBSERVED_'+request['mode']
    except (OSError, ValueError, KeyError, TypeError) as error:
        receipt.update(status='FAILED', error=str(error))
    finally:
        save()
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('request', type=Path); parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    result = run(args.request, args.directory)
    print(json.dumps(dict(status=result['status'], comparisons=len(result['comparisons']))))
    return 0 if result['status'].startswith('OBSERVED_') else 1


if __name__ == '__main__':
    raise SystemExit(main())
