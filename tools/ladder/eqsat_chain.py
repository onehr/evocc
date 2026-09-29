"""R01a retained-chain admission; replay, never trust a stored VERIFIED label.

This is a consumer of the producer's typed telemetry, not another EVP checker.
It does not certify measurement counters or native emission.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import re

AXES = ('add', 'sub', 'mul', 'and', 'or', 'xor', 'shl', 'lshr', 'ashr',
        'zext', 'sext', 'trunc', 'icmp')
REASONS = {'NONE', 'CERTIFICATE_MALFORMED', 'PROOF_INVALID', 'CHECKER_TIMEOUT',
           'CHECKER_UNAVAILABLE', 'CHECKER_ERROR', 'CERTIFICATE_MISSING', 'BUNDLE_MISMATCH'}
REGISTRIES = {
    'p0.scalar.v1': 'ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9',
    'p0.scalar.v2': 'c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce',
    'p0.scalar.v3': 'aca1f15b47e602ce18c593e06d77008862947c7c7252a9f76bb823c71f53fb15',
    'p0.scalar.v4': '1a5391603386e18e18bfb1b2fc61985c9eb252c6e65bcb3259104bd49b2a9d74',
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def keys(obj, expected):
    require(type(obj) is dict and set(obj) == set(expected.split()), 'unknown or missing fields')


def natural(value):
    require(type(value) is int and 0 <= value <= (1 << 64) - 1, 'invalid counter')
    return value


def sha256_identity(value):
    require(type(value) is str and re.fullmatch('[0-9a-f]{64}', value) is not None,
            'invalid SHA-256 identity')


def unique(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON field')
        result[key] = value
    return result


def valid_state(result, stop, certificate, verification, reason):
    if result not in {'CHANGED', 'UNCHANGED', 'ROLLED_BACK_ERROR'}:
        return False
    if stop not in {'NORMAL', 'POLICY_BUDGET', 'INTERNAL_ERROR'} or reason not in REASONS:
        return False
    rollback = result == 'ROLLED_BACK_ERROR'
    if rollback != (stop == 'INTERNAL_ERROR'):
        return False
    if certificate == 'NONE':
        return ((rollback and verification == 'NOT_CHECKED' and reason == 'NONE')
                or (not rollback and verification == 'REJECTED' and reason == 'CERTIFICATE_MISSING'))
    if certificate != 'EMITTED':
        return False
    if verification in {'NOT_CHECKED', 'VERIFIED'}:
        return reason == 'NONE'
    return verification == 'REJECTED' and reason not in {'NONE', 'CERTIFICATE_MISSING'}


def proof_bucket(verification, reason):
    """Completed rejections deliberately exclude malformed bundles and outages."""
    require(verification in {'NOT_CHECKED', 'VERIFIED', 'REJECTED'} and reason in REASONS,
            'unknown proof-accounting state')
    if verification == 'NOT_CHECKED':
        require(reason == 'NONE', 'unchecked proof has a rejection reason')
        return 'not_checked'
    if verification == 'VERIFIED':
        require(reason == 'NONE', 'verified proof has a rejection reason')
        return 'proof_successes'
    require(reason != 'NONE', 'rejected proof has no reason')
    if reason == 'PROOF_INVALID':
        return 'completed_proof_rejections'
    if reason in {'CERTIFICATE_MALFORMED', 'CERTIFICATE_MISSING', 'BUNDLE_MISMATCH'}:
        return 'bundle_obligation_failures'
    return 'unknown_proof_outcomes'


def validate_record(record):
    base = ('attempted_transactions chain_id checker_digest checker_timeout_ms input_digest '
            'max_transactions output_digest policy policy_digest publication schema stop transactions verified_prefix')
    rejected = record.get('publication') == 'REJECTED'
    keys(record, base + (' rejection_reason' if rejected else ''))
    require(record['schema'] == 'evocc-verified-chain-v1', 'unknown chain schema')
    require(record['publication'] in {'VERIFIED', 'REJECTED'}, 'not a completed chain')
    for field in ('chain_id', 'input_digest', 'policy_digest'):
        sha256_identity(record[field])
    for field in ('checker_digest', 'output_digest'):
        if record[field] is not None:
            sha256_identity(record[field])
    p = record['policy']
    keys(p, 'cost_weights counter_mapping extractor registry registry_digest schedule')
    require(REGISTRIES.get(p['registry']) == p['registry_digest'], 'unknown registry ceiling')
    require(p['counter_mapping'] == 'legacy-coupled-v1' and p['extractor'] == 'selected-pair-v1', 'unknown policy interpretation')
    keys(p['schedule'], 'max_nodes max_epochs max_transactions')
    nodes, epochs, count = (natural(p['schedule'][key]) for key in ('max_nodes', 'max_epochs', 'max_transactions'))
    require(nodes <= 100000 and epochs <= 1024 and 1 <= count <= 8, 'schedule exceeds profile')
    require(type(p['cost_weights']) is list and len(p['cost_weights']) == 13, 'wrong cost vector')
    weights = ','.join(f'{axis}={natural(value)}' for axis, value in zip(AXES, p['cost_weights']))
    identity = (f"scalar-chain-v1\nselected-pair-v1\ncoupled-search-admission-v1\n{p['registry']}\n"
                f"{p['registry_digest']}\n{weights}\n{nodes}\n{epochs}\n{count}\n")
    require(digest(identity.encode()) == record['policy_digest'], 'policy identity mismatch')
    require(digest(f"scalar-chain-v1\n{record['input_digest']}\n{record['policy_digest']}\n".encode()) == record['chain_id'], 'chain identity mismatch')
    require(natural(record['max_transactions']) == count, 'transaction ceiling mismatch')
    txs = record['transactions']
    require(type(txs) is list and len(txs) == natural(record['attempted_transactions']) <= count, 'wrong attempted count')
    previous = record['input_digest']
    prefix = 0
    for index, tx in enumerate(txs):
        keys(tx, 'budget_hits certificate_state index input_digest output_digest proof_digest rejection_reason result roots scoped_counters stop verification')
        require(valid_state(*(tx[key] for key in ('result', 'stop', 'certificate_state', 'verification', 'rejection_reason'))), 'illegal state/reason tuple')
        for field in ('input_digest', 'output_digest'):
            sha256_identity(tx[field])
        if tx['certificate_state'] == 'EMITTED':
            sha256_identity(tx['proof_digest'])
        else:
            require(tx['proof_digest'] is None, 'absent certificate has an artifact identity')
        require(natural(tx['index']) == index and tx['input_digest'] == previous, 'reordered or discontinuous chain')
        require((tx['result'] == 'CHANGED') == (tx['input_digest'] != tx['output_digest']), 'result/identity mismatch')
        r = tx['roots']
        keys(r, 'accepted declined_by_reason eligible excluded_by_reason proposed total unvisited visited')
        for key in ('accepted', 'eligible', 'proposed', 'total', 'unvisited', 'visited'):
            natural(r[key])
        keys(r['declined_by_reason'], 'policy witness')
        for value in r['declined_by_reason'].values():
            natural(value)
        keys(r['excluded_by_reason'], 'dependency taint unlisted')
        require(r['eligible'] + sum(natural(v) for v in r['excluded_by_reason'].values()) == r['total'], 'exclusion census mismatch')
        require(r['visited'] + r['unvisited'] == r['total'] and r['accepted'] <= r['proposed'] <= r['visited'], 'root census mismatch')
        require((r['accepted'] > 0) == (tx['result'] == 'CHANGED'), 'retained count/result mismatch')
        c = tx['scoped_counters']
        keys(c, 'admission evp matching_work search')
        require(c['matching_work'] is None, 'unimplemented matching counter is not measured')
        a, s = c['admission'], c['search']
        if c['evp'] is not None:
            keys(c['evp'], 'epoch_marker fuel fuel_bound n nodes')
            for value in c['evp'].values():
                natural(value)
            e = c['evp']
            require(e['fuel_bound'] == max(256, 8 * e['n']) and e['fuel'] <= e['fuel_bound']
                    and e['nodes'] <= 100000 and e['epoch_marker'] <= 1024, 'invalid EVP budget telemetry')
        keys(a, 'next_nodes nodes_hit proof_nodes regions regions_hit')
        keys(s, 'bridge_nodes nodes_hit rewrite_events rewrite_events_hit')
        for item, fields in ((a, ('next_nodes', 'proof_nodes', 'regions')), (s, ('bridge_nodes', 'rewrite_events'))):
            for field in fields:
                natural(item[field])
        for flag in (a['nodes_hit'], a['regions_hit'], s['nodes_hit'], s['rewrite_events_hit']):
            require(type(flag) is bool, 'invalid hit flag')
        require(a['regions'] <= epochs and a['proof_nodes'] <= nodes, 'over-admitted certificate')
        if tx['result'] != 'ROLLED_BACK_ERROR':
            require(a['regions'] == r['accepted'], 'staged/retained count mismatch')
        expected_hits = []
        for scope, unit, consumed, limit, following, hit in (
                ('search.bridge_nodes', 'bridge_nodes', s['bridge_nodes'], nodes, 0, s['nodes_hit']),
                ('search.rewrite_events', 'rewrite_events', s['rewrite_events'], epochs, 1, s['rewrite_events_hit']),
                ('admission.accepted_regions', 'regions', a['regions'], epochs, 1, a['regions_hit']),
                ('admission.proof_nodes', 'proof_nodes', a['proof_nodes'], nodes, a['next_nodes'], a['nodes_hit'])):
            if hit:
                require(consumed + following > limit, 'false budget hit')
                expected_hits.append(dict(consumed=consumed, limit=limit, next=following,
                                          reason='NEXT_ADMISSION_EXCEEDS' if following else 'CONSUMPTION_EXCEEDED', scope=scope, unit=unit))
        require(tx['budget_hits'] == expected_hits, 'lost or invented budget scope')
        require(tx['stop'] == 'INTERNAL_ERROR' or (tx['stop'] == 'POLICY_BUDGET') == bool(expected_hits), 'stop/hit mismatch')
        success = tx['verification'] == 'VERIFIED' and tx['result'] != 'ROLLED_BACK_ERROR'
        if index < len(txs) - 1:
            require(success and tx['result'] == 'CHANGED' and tx['stop'] == 'NORMAL', 'continued after terminal attempt')
        prefix += int(success)
        previous = tx['output_digest']
    require(natural(record['verified_prefix']) == prefix, 'incorrect verified prefix')
    if rejected:
        require(record['stop'] == 'ERROR' and record['output_digest'] is None and record['rejection_reason'] in REASONS, 'invalid rejected chain')
    else:
        require(txs and prefix == len(txs) and previous == record['output_digest'], 'incomplete published chain')
        last = txs[-1]
        stop = 'TRANSACTION_BUDGET' if last['stop'] == 'POLICY_BUDGET' else ('NO_CHANGE' if last['result'] == 'UNCHANGED' else 'TRANSACTION_LIMIT')
        require(record['stop'] == stop and (stop != 'TRANSACTION_LIMIT' or len(txs) == count), 'invalid chain termination')
        require(type(record['checker_digest']) is str and 1 <= natural(record['checker_timeout_ms']) <= 300000, 'missing checker identity/deadline')


def verify_bundle(directory: Path, checker: Path):
    raw = (directory / 'chain.json').read_bytes()
    record = json.loads(raw, object_pairs_hook=unique)
    validate_record(record)
    require(record['publication'] == 'VERIFIED', 'rejected chain cannot enter measurement')
    identity = digest(checker.read_bytes())
    require(identity == record['checker_digest'], 'checker identity changed')
    counts = dict(proof_successes=0, completed_proof_rejections=0,
                  bundle_obligation_failures=0, unknown_proof_outcomes=0, not_checked=0)
    replay = []
    frozen = {'chain.json': raw}
    with tempfile.TemporaryDirectory(prefix='evocc-chain-audit-') as scratch:
        for tx in record['transactions']:
            args = []
            reason = 'NONE'
            for suffix, key in (('before.evo', 'input_digest'), ('after.evo', 'output_digest'), ('proof.evp', 'proof_digest')):
                name = f"{tx['index']:03}.{suffix}"
                try:
                    data = (directory / name).read_bytes()
                except OSError:
                    reason = 'CERTIFICATE_MISSING' if suffix == 'proof.evp' else 'BUNDLE_MISMATCH'
                    break
                if digest(data) != tx[key]:
                    reason = 'BUNDLE_MISMATCH'
                    break
                frozen[name] = data
                path = Path(scratch) / suffix
                path.write_bytes(data)
                args.append(str(path))
            if reason == 'NONE':
                try:
                    result = subprocess.run([str(checker.resolve()), *args], stdin=subprocess.DEVNULL,
                                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                            timeout=record['checker_timeout_ms'] / 1000, check=False)
                    reason = {0: 'NONE', 2: 'CERTIFICATE_MALFORMED', 3: 'PROOF_INVALID'}.get(result.returncode, 'CHECKER_ERROR')
                except subprocess.TimeoutExpired:
                    reason = 'CHECKER_TIMEOUT'
                except OSError:
                    reason = 'CHECKER_UNAVAILABLE'
            if reason == 'NONE':
                proof = frozen[f"{tx['index']:03}.proof.evp"]
                budget = re.search(rb'^budget nodes=(\d+)/(\d+) epochs=(\d+)/(\d+) fuel=(\d+)/(\d+) n=(\d+) stop=\w+$', proof, re.M)
                expected = None
                if budget:
                    values = [int(v) for v in budget.groups()]
                    expected = dict(nodes=values[0], epoch_marker=values[2], fuel=values[4], fuel_bound=values[5], n=values[6])
                require(tx['scoped_counters']['evp'] == expected, 'EVP telemetry differs from replayed certificate')
            verdict = 'VERIFIED' if reason == 'NONE' else 'REJECTED'
            counts[proof_bucket(verdict, reason)] += 1
            replay.append(dict(index=tx['index'], verification=verdict, rejection_reason=reason))
            if reason != 'NONE':
                break
    require(identity == digest(checker.read_bytes()), 'checker changed during replay')
    require(all((directory / name).read_bytes() == data for name, data in frozen.items()), 'bundle changed during replay')
    return dict(schema='evocc-chain-audit-v1', accepted=counts['proof_successes'] == len(record['transactions']),
                chain_record_digest=digest(raw), verified_ir_digest=record['output_digest'],
                proof_counts=counts, replay=replay, measurement_counters='PRODUCER_TELEMETRY_NOT_PROVED')
