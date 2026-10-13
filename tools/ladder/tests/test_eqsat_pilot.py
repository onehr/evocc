import copy
from pathlib import Path
import tempfile
import unittest

from tools.ladder.eqsat_pilot import (ROOTS, GUEST_TOOLS, COLLECTORS, LEAN_SOURCES, LEAN_IMPORTS, recipes, validate_plan,
                                     tree_identity, composite_binding, pilot_statistics, admit_pilot_timing)
from tools.ladder.eqsat_preflight_run import CONTRACT


def plan(root):
    files = set(COLLECTORS) | set(LEAN_SOURCES) | set(LEAN_IMPORTS) | {'spec/evoir.md', 'spec/evomir.md',
        'tests/fixtures/eqsat-performance/integer-mix.c',
        'tests/fixtures/eqsat-performance/compression-workload.c',
        'tests/fixtures/eqsat-performance/cjson-workload.c'}
    return dict(schema='evocc-r02-linux-pilot-v1', implementation_commit='0'*40,
        tools={k: dict(path='/unit/'+k, sha256='0'*64) for k in
               ('compiler', 'checker', 'ir_backend', 'container', 'collector_python', 'census', 'lean')},
        guest_tools={k: '0'*64 for k in GUEST_TOOLS},
        image=dict(name='evo-nexlab:sealed', digest='0'*64), host={},
        trees={k: dict(path=v, sha256='0'*64, files=1) for k, v in ROOTS.items()},
        files={k: '0'*64 for k in files},
        controls=dict(optimization=2, native_profiles=['checked', 'unchecked'], variants=['A', 'B'],
            eqsat='shared-scalar-chain-v1/default-v4', target='linux-aarch64', relocation='PIC',
            source_semantics='GNU11/default-no-fast-math',
            lowering='O2-default-fold-reuse-join-prune-immutable-globals',
            reference='gcc-14-O2/PIC', coverage='DECLARED_MIXED_LINK_NO_FALLBACK',
            interpreter='P0_BOTH_ONLY_LIBC_APPLICATIONS_NATIVE', runtime='hardware-VM-not-bare-metal'),
        protocol=dict(warmups=1, pairs=6, seed=20260929, retries=0,
                      metric='guest-fresh-process-spawn-to-stdio-drain', comparisons=['AA', 'AB'],
                      effect_floor_percent=5, precision_percent=5,
                      multiple_comparisons='BONFERRONI_ALL_PILOT_COMPARISONS'),
        resources=dict(jobs=1, timeout_seconds=300, output_bytes=16*1024**2,
                       min_free_bytes=8*1024**3, max_retained_bytes=1024**3,
                       campaign_seconds=14400, vm_cpus=2, vm_memory='3G'),
        cases=recipes(root), preflight=copy.deepcopy(CONTRACT))


class PilotTests(unittest.TestCase):
    def test_failed_preflight_never_admits_native_timing(self):
        preflight = dict(status='BOUNDED_PREFLIGHT_COMPLETED',
            policy=dict(verdict='INCONCLUSIVE', evaluations=[dict(status='OBSERVED')]),
            rule=dict(verdict='INCONCLUSIVE', proofs=[dict(status='UNKNOWN')]))
        # An honest bounded/unknown track verdict is not an execution failure.
        admit_pilot_timing(preflight)
        for mutate in (
            lambda p: p.update(status='INCOMPLETE'),
            lambda p: p['policy']['evaluations'][0].update(status='FAILED'),
            lambda p: p['policy']['evaluations'][0].update(status='STARTED'),
            lambda p: p['policy'].update(evaluations=[]),
        ):
            failed = copy.deepcopy(preflight)
            mutate(failed)
            with self.assertRaisesRegex(ValueError, 'timing is not admitted'):
                admit_pilot_timing(failed)

    def test_wide_or_underpowered_intervals_are_inconclusive(self):
        protocol = dict(effect_floor_percent=5, precision_percent=5)
        flat = [dict(A0=100, A1=100) for _ in range(12)]
        self.assertEqual(pilot_statistics(flat, protocol, 24)['calibration'], 'WITHIN_EFFECT_FLOOR')
        self.assertEqual(pilot_statistics(flat[:6], protocol, 24)['calibration'], 'INCONCLUSIVE')
        spread = [dict(A0=100, A1=50 if i%2 else 150) for i in range(12)]
        self.assertEqual(pilot_statistics(spread, protocol, 24)['calibration'], 'INCONCLUSIVE')

    def test_controls_coverage_and_resource_changes_fail_closed(self):
        with tempfile.TemporaryDirectory() as root:
            m = plan(root)
            validate_plan(m, root)
            for mutate in (
                lambda v: v['controls'].update(native_profiles=['unchecked']),
                lambda v: v['controls'].update(optimization=0),
                lambda v: v['resources'].update(jobs=True),
                lambda v: v['resources'].update(min_free_bytes=0),
                lambda v: v['protocol'].update(retries=1),
                lambda v: v['cases'][1]['subject_sources'].pop(),
                lambda v: v['cases'][1]['flags'].append('-fwrapv'),
                lambda v: v['guest_tools'].pop('/usr/bin/as'),
                lambda v: v['files'].pop('tools/bench.py'),
                lambda v: v['image'].update(name='evo-unregistered:sealed'),
            ):
                changed = copy.deepcopy(m)
                mutate(changed)
                with self.assertRaises((ValueError, KeyError)):
                    validate_plan(changed, root)

    def test_plans_frozen_before_the_tag_rename_still_validate(self):
        with tempfile.TemporaryDirectory() as root:
            m = plan(root)
            m['image']['name'] = 'evo-flight-lab:sealed'
            validate_plan(m, root)

    def test_source_tree_identity_ignores_products_not_headers(self):
        with tempfile.TemporaryDirectory() as root:
            p = Path(root)
            (p/'source.c').write_text('int x;')
            before = tree_identity(p)
            (p/'large.o').write_bytes(b'product')
            self.assertEqual(tree_identity(p), before)
            (p/'config.h').write_text('#define X 1')
            self.assertNotEqual(tree_identity(p), before)

    def test_composite_identity_cannot_drop_or_exchange_translation_units(self):
        b = dict(ir_digest='1'*64, chain_record_digest='2'*64, bundle_digest='3'*64)
        c = dict(b, ir_digest='4'*64)
        expected = composite_binding({'one': b, 'two': c})
        self.assertEqual(expected, composite_binding({'two': c, 'one': b}))
        self.assertNotEqual(expected, composite_binding({'one': b}))
        self.assertNotEqual(expected, composite_binding({'one': c, 'two': b}))


if __name__ == '__main__':
    unittest.main()
