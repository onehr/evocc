import copy
from pathlib import Path
import tempfile
import unittest

from tools.ladder.eqsat_chain import digest
from tools.ladder.eqsat_twins import (admit_unchecked, bind_bundle, canonical,
                                     native_identity, stable_bundle)


class TwinTests(unittest.TestCase):
    def identities(self):
        binding = dict(ir_digest='1'*64, chain_record_digest='2'*64, bundle_digest='3'*64)
        a = native_identity(binding, 'checked', 'linux-aarch64', '4'*64, '5'*64, '6'*64)
        b = native_identity(binding, 'unchecked', 'linux-aarch64', '4'*64, '7'*64, '6'*64)
        expected = dict(exit_code=0, stdout_sha256=digest(b''), stderr_sha256=digest(b''))
        run = dict(status='RESULT_CHECKED', native_identity=a['native_identity'], **expected)
        return a, b, run, expected

    def test_twin_input_identity_is_a_hard_gate(self):
        a, b, run, expected = self.identities()
        self.assertEqual(admit_unchecked(a, b, run, expected)['status'], 'CHECKED_TWIN_ADMITTED')
        for field in ('ir_digest', 'chain_record_digest', 'bundle_digest', 'input_digest',
                      'target', 'backend_digest', 'profile'):
            changed = copy.deepcopy(b)
            changed[field] = '8'*64
            changed['native_identity'] = digest(canonical({k: v for k, v in changed.items()
                                                           if k != 'native_identity'}))
            with self.subTest(field=field), self.assertRaises(ValueError):
                admit_unchecked(a, changed, run, expected)

    def test_missing_trapped_timed_out_or_wrong_artifact_twin_is_not_admission(self):
        a, b, run, expected = self.identities()
        for mutation in ({}, dict(status='FAILED'), dict(exit_code=-5), dict(exit_code=1),
                         dict(native_identity='8'*64), dict(stdout_sha256='8'*64),
                         dict(stderr_sha256='8'*64)):
            changed = {**run, **mutation} if mutation else {}
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                admit_unchecked(a, b, changed, expected)

    def test_identity_bundle_requires_actual_input_and_output_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            ir = b'identity IR\n'
            value = digest(ir)
            record = dict(schema='evocc-identity-chain-v1', mode='eqsat-off',
                          publication='IDENTITY_VERIFIED', attempted_transactions=0, transactions=[],
                          input_digest=value, output_digest=value,
                          chain_id=digest(f'identity-chain-v1\n{value}\n'.encode()))
            (path/'chain.json').write_bytes(canonical(record))
            for name in ('input.evo', 'output.evo'):
                (path/name).write_bytes(ir)
            binding = bind_bundle(path)
            stable_bundle(path, binding)
            (path/'output.evo').write_bytes(ir+b'changed')
            with self.assertRaises(ValueError):
                stable_bundle(path, binding)
            with self.assertRaises(ValueError):
                bind_bundle(path)


if __name__ == '__main__':
    unittest.main()
