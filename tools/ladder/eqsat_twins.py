"""R02d native admission: one verified bundle, two native guard profiles.

These checks bind artifacts and trusted execution telemetry. A checked pass is
necessary, not a proof that every C undefined behavior has been modeled.
"""
from __future__ import annotations

import json
from pathlib import Path

from .eqsat_chain import digest, keys, require, unique, verify_bundle, sha256_identity


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def bind_bundle(directory, checker=None):
    directory = Path(directory)
    audit = verify_bundle(directory, checker)
    require(audit['accepted'], 'chain replay did not admit output')
    record = json.loads((directory / 'chain.json').read_bytes(), object_pairs_hook=unique)
    names = {'chain.json', 'input.evo', 'output.evo'}
    for tx in record['transactions']:
        names.update(f"{tx['index']:03}.{suffix}" for suffix in
                     ('before.evo', 'after.evo', 'proof.evp'))
    # Additional report files are allowed, but never replace a required proof.
    inventory = {}
    for name in sorted(names):
        path = directory / name
        require(path.is_file() and not path.is_symlink(), 'missing or aliased bundle artifact')
        inventory[name] = digest(path.read_bytes())
    require(inventory['input.evo'] == record['input_digest']
            and inventory['output.evo'] == record['output_digest'] == audit['verified_ir_digest']
            and inventory['chain.json'] == audit['chain_record_digest'], 'codegen bundle mismatch')
    for tx in record['transactions']:
        for suffix, field in (('before.evo', 'input_digest'), ('after.evo', 'output_digest'),
                              ('proof.evp', 'proof_digest')):
            require(inventory[f"{tx['index']:03}.{suffix}"] == tx[field], 'proof artifact changed')
    return dict(ir_digest=record['output_digest'], chain_record_digest=inventory['chain.json'],
                bundle_digest=digest(canonical(inventory)), inventory=inventory, audit=audit)


def stable_bundle(directory, binding):
    require(all(digest((Path(directory) / name).read_bytes()) == expected
                for name, expected in binding['inventory'].items()), 'shared bundle changed')


def native_identity(binding, profile, target, backend_digest, executable_digest, input_digest):
    require(profile in ('checked', 'unchecked'), 'unknown native guard profile')
    for identity in (backend_digest, executable_digest, input_digest, binding['ir_digest'],
                     binding['chain_record_digest'], binding['bundle_digest']):
        sha256_identity(identity)
    fields = dict(profile=profile, target=target, backend_digest=backend_digest,
                  executable_digest=executable_digest, input_digest=input_digest,
                  ir_digest=binding['ir_digest'], chain_record_digest=binding['chain_record_digest'],
                  bundle_digest=binding['bundle_digest'])
    return dict(**fields, native_identity=digest(canonical(fields)))


def admit_unchecked(checked, unchecked, checked_attempt, expected_result):
    fields = ('profile target backend_digest executable_digest input_digest ir_digest '
              'chain_record_digest bundle_digest native_identity')
    for identity in (checked, unchecked):
        keys(identity, fields)
        require(identity['native_identity'] == digest(canonical({
            k: v for k, v in identity.items() if k != 'native_identity'})), 'native identity forged')
    require(checked['profile'] == 'checked' and unchecked['profile'] == 'unchecked',
            'incorrect twin profiles')
    require(all(checked[k] == unchecked[k] for k in
                ('target', 'backend_digest', 'input_digest', 'ir_digest',
                 'chain_record_digest', 'bundle_digest')), 'twins do not share verified inputs')
    require(checked_attempt.get('status') == 'RESULT_CHECKED'
            and checked_attempt.get('native_identity') == checked['native_identity']
            and checked_attempt.get('exit_code') == 0
            and all(checked_attempt.get(k) == expected_result[k] for k in
                    ('exit_code', 'stdout_sha256', 'stderr_sha256')),
            'missing, failed or mismatched checked execution')
    return dict(status='CHECKED_TWIN_ADMITTED', checked_native_identity=checked['native_identity'],
                unchecked_native_identity=unchecked['native_identity'],
                checked_execution_digest=digest(canonical(checked_attempt)),
                limitation='necessary-observed-definedness-not-a-universal-C-UB-proof')
