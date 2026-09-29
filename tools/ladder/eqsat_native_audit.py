"""Read-only A/A capture consistency, not execution or performance attestation.

No tools are launched, no measurements are retried and no receipts are written.
The expected plan digest must come from the independently frozen plan. Timing,
exit status and host observations remain trusted collector telemetry.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from .eqsat_chain import digest, keys, natural, require, sha256_identity, unique, verify_bundle
from .eqsat_native import execution_plan, pair_order, summarize


def audit_calibration(directory, root, expected_manifest_digest):
    directory, root = Path(directory).resolve(), Path(root).resolve()
    sha256_identity(expected_manifest_digest)
    observed_files = {}

    def exact(left, right):
        # Python otherwise equates false/0 and 1.0/1 in nested receipts.
        return json.dumps(left, sort_keys=True, allow_nan=False) == json.dumps(
            right, sort_keys=True, allow_nan=False)

    def data(path):
        path = directory / path
        require(path.resolve().is_relative_to(directory), 'capture path leaves owned directory')
        raw = path.read_bytes()
        identity = digest(raw)
        require(path not in observed_files or observed_files[path] == identity,
                'capture changed during audit')
        observed_files[path] = identity
        return raw

    manifest_bytes = data('manifest.json')
    require(digest(manifest_bytes) == expected_manifest_digest, 'frozen manifest mismatch')
    m = json.loads(manifest_bytes, object_pairs_hook=unique)
    receipt_bytes = data('receipt.json')
    r = json.loads(receipt_bytes, object_pairs_hook=unique)
    keys(r, 'schema status manifest_digest implementation_commit host scope python attempts cases '
         'performance collector_files correctness proof counters runtime_cache effective_controls')
    require(r['schema'] == 'evocc-native-calibration-v1' and r['status'] == 'OBSERVED_AA',
            'incomplete or failed capture cannot be admitted')
    require(r['manifest_digest'] == expected_manifest_digest
            and r['implementation_commit'] == m['implementation_commit']
            and r['collector_files'] == m['native']['collector_files'], 'capture identity mismatch')
    execution_plan(m, r['host'])
    require(r['scope'] == 'Darwin-P0-A/A-pipeline-only-not-Q2-Q3-or-Linux'
            and r['performance'] == 'NOT_CLAIMED'
            and r['correctness'] == 'RESULT_CHECKED_EVERY_EXECUTION'
            and r['proof'] == 'IDENTITY_ONLY_NO_EVP_REWRITE'
            and r['counters'] == 'NOT_MEASURED'
            and r['runtime_cache'] == 'NOT_CONTROLLED'
            and r['effective_controls'] == 'REGISTERED_RECIPE_ONLY', 'capture claim boundary changed')
    tools = {name: (root / item['path']).resolve() for name, item in m['tools'].items()}
    keys(r['python'], 'executable version')
    require(r['python']['executable'] == str(tools['collector_python'])
            and type(r['python']['version']) is str and r['python']['version'], 'collector identity mismatch')
    require(type(r['attempts']) is list and type(r['cases']) is list
            and len(r['cases']) == len(m['cases']), 'incomplete capture population')
    position = 0

    def attempt(stage, argv, expected=None):
        nonlocal position
        require(position < len(r['attempts']), 'missing attempt')
        index = position
        row = r['attempts'][index]
        position += 1
        keys(row, 'stage argv status stdout stderr exit_code wall_ns stdout_sha256 '
                  'stderr_sha256 stdout_bytes stderr_bytes')
        require(row['stage'] == stage and row['argv'] == [str(a) for a in argv], 'attempt command mismatch')
        require(row['status'] == ('COMPLETED' if expected is None else 'RESULT_CHECKED')
                and natural(row['exit_code']) == 0 and natural(row['wall_ns']) > 0,
                'attempt status or metric invalid')
        for stream in ('stdout', 'stderr'):
            name = f'{index:03}.{stream}'
            require(row[stream] == name, 'attempt stream path mismatch')
            raw = data(name)
            require(digest(raw) == row[stream + '_sha256']
                    and len(raw) == natural(row[stream + '_bytes']), 'captured output changed')
        require(row['stdout_bytes'] + row['stderr_bytes'] <= m['resources']['output_bytes'],
                'captured output exceeds frozen limit')
        if expected is not None:
            require(all(row[k] == expected[k] for k in expected), 'attempt result mismatch')
        return index, row

    for case, product in zip(m['cases'], r['cases']):
        keys(product, 'id status samples pairs warmups chain ir_digest assembly_digest object_digest '
                      'executable_digest codegen coverage order statistics')
        require(product['id'] == case['id'] and product['status'] == 'OBSERVED_AA'
                and product['codegen'] == 'canonical-IR/checked-PIC/Darwin-arm64'
                and product['coverage'] == case['coverage'], 'case identity mismatch')
        work = directory / case['id']
        chain = work / 'chain'
        for name in ('input.evo', 'output.evo', 'chain.json'):
            data(Path(case['id']) / 'chain' / name)
        identity = verify_bundle(chain)
        require(exact(identity, product['chain']) and identity['accepted']
                and identity['verified_ir_digest'] == product['ir_digest'], 'chain audit mismatch')
        source = (root / case['source']['path']).resolve()

        def command(stage, argv, expected=None):
            return attempt(case['id'] + ':' + stage, argv, expected)

        zero = dict(exit_code=0, stdout_sha256=digest(b'0\n'), stderr_sha256=digest(b''))
        for mode in ('--interpret-c-hir', '--interpret-evoir'):
            command(mode, [tools['compiler'], mode, '-O0', source], zero)
        _, frontend = command('frontend', [tools['compiler'], '--emit=evoir', '-O0',
                                          '--eq-proof-dir=' + str(chain), source])
        require(data(frontend['stdout']) == data(Path(case['id']) / 'chain/output.evo'),
                'frontend output differs from codegen IR')
        _, backend = command('backend', [tools['ir_backend'], 'darwin-arm64', product['ir_digest'],
                                        chain / 'output.evo'])
        assembly = directory / backend['stdout']
        obj, executable = work / 'program.o', work / 'program'
        for path, field in ((assembly, 'assembly_digest'), (obj, 'object_digest'),
                            (executable, 'executable_digest')):
            require(digest(data(path.relative_to(directory))) == product[field], 'codegen artifact changed')
        command('assemble', [tools['assembler'], '-c', '-x', 'assembler', assembly, '-o', obj])
        command('link', [tools['linker'], obj, '-o', executable])
        ref_obj, reference = work / 'reference.o', work / 'reference'
        command('reference-compile', [tools['reference'], '-std=gnu11', '-O0', '-c', source, '-o', ref_obj])
        command('reference-link', [tools['linker'], ref_obj, '-o', reference])
        command('reference-result', [reference], case['result'])
        command('subject-admission', [executable], case['result'])
        warmups = []
        for repeat in range(m['protocol']['warmups']):
            for label in ('A0', 'A1'):
                index, _ = command(f'warmup-{repeat}-{label}', [executable], case['result'])
                warmups.append(index)
        samples, pairs = [], []
        order = pair_order(m['protocol']['pairs'], m['protocol']['seed'])
        require(product['order'] == [list(pair) for pair in order], 'pair ordering changed')
        for pair, labels in enumerate(order):
            values = {}
            for label in labels:
                index, row = command(f'pair-{pair}-{label}', [executable], case['result'])
                samples.append(dict(pair=pair, label=label, attempt=index, wall_ns=row['wall_ns'],
                                    executable_digest=product['executable_digest']))
                values[label] = row['wall_ns']
            pairs.append(values)
        require(exact(product['warmups'], warmups) and exact(product['samples'], samples)
                and exact(product['pairs'], pairs), 'sample population mismatch')
        require(exact(product['statistics'], summarize(pairs)), 'statistics do not match raw pairs')
    require(position == len(r['attempts']), 'extra attempts hidden from population')
    for path in tuple(observed_files):
        data(path)
    return dict(schema='evocc-native-calibration-audit-v1', status='CAPTURE_CONSISTENT',
                manifest_digest=expected_manifest_digest, receipt_digest=digest(receipt_bytes),
                attempted_commands=position, cases=len(r['cases']),
                timed_executions=sum(len(p['samples']) for p in r['cases']),
                warmups=sum(len(p['warmups']) for p in r['cases']), performance='NOT_CLAIMED',
                source_and_tool_build_provenance='NOT_REATTESTED',
                execution_observations='TRUSTED_COLLECTOR_TELEMETRY')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--manifest-sha256', required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(audit_calibration(args.directory, args.root, args.manifest_sha256),
                         sort_keys=True, indent=2))
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f'capture audit rejected: {error}\n')


if __name__ == '__main__':
    main()
