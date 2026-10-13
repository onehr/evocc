"""A frozen plan is not proof replay, correctness admission or a measurement."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from tools.ladder.eqsat_chain import digest
from tools.ladder.eqsat_manifest import validate_manifest, audit_manifest


def manifest():
    def artifact(name):
        return dict(path='inputs/' + name, sha256=digest(name.encode()))
    return dict(
        schema='evocc-performance-plan-v1', scope='r02-plan-not-admission',
        implementation_commit='a' * 40,
        tools={key: artifact(key) for key in ('compiler', 'checker', 'reference', 'assembler', 'linker')},
        specs={key: artifact(key) for key in ('evoir', 'evomir')},
        host=dict(system='Darwin', release='25.0', machine='arm64', runtime='native',
                  cpu='declared CPU', libc='declared runtime', affinity='uncontrolled'),
        environment={'LANG': 'C'},
        controls=dict(target='aarch64-apple-darwin', isa='baseline', abi='AAPCS64-Darwin',
            ub_assume=dict(requested='checked', effective='checked'),
            semantic_flags=['-fno-fast-math'],
            lowering=dict(optimization_level=0, macros=[], integer_constant_folding=False,
                          immutable_global_propagation=False, pure_value_reuse=False,
                          constant_edge_pruning=False, linear_block_joining=False),
            optimizer=dict(chain='scalar-chain-v1', extractor='selected-pair-v1',
                           counter_mapping='legacy-coupled-v1', registry='p0.scalar.v4',
                           max_nodes=100000, max_epochs=1024, max_transactions=8,
                           proof_required=True)),
        protocol=dict(phase='calibration', variants=['A0', 'A1'], primary='process_wall_ns',
                      boundary='spawn-to-stdio-drain', clock='monotonic_ns', warmups=1,
                      pairs=4, seed=7, order='balanced-interleaved', estimator='paired-median',
                      claims='calibration-only', retries=0),
        resources=dict(jobs=1, timeout_seconds=10, output_bytes=4*1024**2,
                       min_free_bytes=2*1024**3, max_retained_bytes=64*1024**2),
        cases=[dict(id='p0-control', family='p0', split='development',
            source=artifact('source'), inputs=[artifact('data')],
            compiler_argv=['--emit=evoir', '-O0', '{source}'],
            reference_argv=['-O0', '{source}', '-o', '{object}'],
            link_argv=['{object}', '-o', '{executable}'],
            run_argv=['{executable}'],
            coverage=dict(subject_objects=['main.o'], reference_objects=[]),
            result=dict(exit_code=0, stdout_sha256=digest(b''), stderr_sha256=digest(b'')),
            regime='fresh-process-no-cache-claim')])


class ManifestTests(unittest.TestCase):
    def test_complete_plan_does_not_admit_or_measure(self):
        result = validate_manifest(manifest())
        self.assertEqual(result['status'], 'PLAN_VALIDATED_EXECUTION_NOT_RUN')
        self.assertEqual(result['correctness'], 'NOT_RUN')
        self.assertEqual(result['proof'], 'NOT_RUN')
        self.assertEqual(result['performance'], 'NOT_RUN')
        self.assertEqual(result['cases'], 1)

    def test_missing_semantics_or_measurement_controls_reject(self):
        for section, key in (
            ('controls', 'ub_assume'), ('controls', 'lowering'), ('controls', 'optimizer'),
            ('protocol', 'boundary'), ('protocol', 'estimator'), ('protocol', 'seed'),
            ('resources', 'max_retained_bytes'), ('host', 'libc'), ('tools', 'checker'),
        ):
            m = manifest()
            m[section].pop(key)
            with self.subTest(section=section, key=key), self.assertRaises(ValueError):
                validate_manifest(m)

    def test_no_unimplemented_profile_or_implicit_semantic_downgrade(self):
        for mutate in (
            lambda m: m['controls']['ub_assume'].update(requested='unchecked', effective='unchecked'),
            lambda m: m['controls']['ub_assume'].update(effective='unchecked'),
            lambda m: m['controls']['optimizer'].update(proof_required=False),
            lambda m: m['controls']['optimizer'].update(max_transactions=9),
            lambda m: m['controls']['lowering'].update(pure_value_reuse=1),
            lambda m: m['controls']['optimizer'].update(counter_mapping='epochs'),
            lambda m: m['protocol'].update(phase='search'),
            lambda m: m['protocol'].update(variants=['A', 'B']),
            lambda m: m['protocol'].update(claims='IMPROVEMENT'),
            lambda m: m['host'].update(runtime='tcg'),
        ):
            m = manifest()
            mutate(m)
            with self.subTest(m=m), self.assertRaises(ValueError):
                validate_manifest(m)

    def test_population_result_argv_and_object_coverage_cannot_disappear(self):
        for mutate in (
            lambda m: m.update(cases=[]),
            lambda m: m['cases'].append(copy.deepcopy(m['cases'][0])),
            lambda m: m['cases'][0].pop('result'),
            lambda m: m['cases'][0]['result'].update(exit_code=True),
            lambda m: m['cases'][0].update(compiler_argv=[]),
            lambda m: m['cases'][0].update(run_argv=['sh', '-c', '{executable}']),
            lambda m: m['cases'][0]['coverage'].update(subject_objects=[]),
            lambda m: m['cases'][0]['coverage'].update(reference_objects=['main.o']),
            lambda m: m['cases'][0].update(split='held-out'),
            lambda m: m['cases'][0]['source'].update(path='../source'),
            lambda m: m['cases'][0]['source'].update(path='/tmp/source'),
            lambda m: m.update(unknown=True),
        ):
            m = manifest()
            mutate(m)
            with self.subTest(m=m), self.assertRaises(ValueError):
                validate_manifest(m)

    def test_only_tool_pins_can_explicitly_name_external_files(self):
        m = manifest()
        m['tools']['reference']['path'] = '/usr/bin/clang'
        validate_manifest(m)
        m['specs']['evoir']['path'] = '/outside/spec.md'
        with self.assertRaises(ValueError):
            validate_manifest(m)

    def test_resource_limits_and_metric_types_fail_closed(self):
        for mutate in (
            lambda m: m['resources'].update(jobs=2),
            lambda m: m['resources'].update(output_bytes=0),
            lambda m: m['resources'].update(min_free_bytes=0),
            lambda m: m['protocol'].update(pairs=True),
            lambda m: m['protocol'].update(warmups=-1),
            lambda m: m['protocol'].update(retries=1),
            lambda m: m['protocol'].update(primary='speedup'),
            lambda m: m['cases'][0]['result'].update(stdout_sha256='not a hash'),
        ):
            m = manifest()
            mutate(m)
            with self.subTest(m=m), self.assertRaises(ValueError):
                validate_manifest(m)

    def test_artifact_audit_is_read_only_and_detects_stale_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'inputs').mkdir()
            for name in ('compiler', 'checker', 'reference', 'assembler', 'linker',
                         'evoir', 'evomir', 'source', 'data'):
                (root / 'inputs' / name).write_text(name)
            path = root / 'plan.json'
            path.write_text(json.dumps(manifest()))
            with (patch('tools.ladder.eqsat_manifest.check_source_revision'),
                  patch('tools.ladder.eqsat_manifest.check_committed'),
                  patch('subprocess.Popen', side_effect=AssertionError('must not execute workloads'))):
                result = audit_manifest(path, root)
                self.assertEqual(result['status'], 'FROZEN_PLAN_AUDITED_EXECUTION_NOT_RUN')
                self.assertEqual(result['artifacts_checked'], 9)
                (root / 'inputs/data').write_text('stale')
                with self.assertRaisesRegex(ValueError, 'identity changed'):
                    audit_manifest(path, root)

    def test_source_recheck_cannot_hide_late_artifact_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'inputs').mkdir()
            for name in ('compiler', 'checker', 'reference', 'assembler', 'linker',
                         'evoir', 'evomir', 'source', 'data'):
                (root / 'inputs' / name).write_text(name)
            path = root / 'plan.json'
            path.write_text(json.dumps(manifest()))
            calls = 0

            def source_check(*args):
                nonlocal calls
                calls += 1
                if calls == 2:
                    (root / 'inputs/data').write_text('changed during audit')

            with (patch('tools.ladder.eqsat_manifest.check_source_revision', side_effect=source_check),
                  patch('tools.ladder.eqsat_manifest.check_committed')):
                with self.assertRaisesRegex(ValueError, 'identity changed during audit'):
                    audit_manifest(path, root)

    def test_relative_tool_symlink_cannot_escape_declared_workspace(self):
        with tempfile.TemporaryDirectory() as directory:
            outer = Path(directory)
            (outer / 'compiler').write_text('compiler')
            root = outer / 'repo'
            (root / 'inputs').mkdir(parents=True)
            (root / 'inputs/compiler').symlink_to(outer / 'compiler')
            path = root / 'plan.json'
            path.write_text(json.dumps(manifest()))
            with (patch('tools.ladder.eqsat_manifest.check_source_revision'),
                  patch('tools.ladder.eqsat_manifest.check_committed')):
                with self.assertRaisesRegex(ValueError, 'symlink leaves workspace'):
                    audit_manifest(path, root)

    def test_uncommitted_plan_is_rejected_without_reading_artifacts(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'tests/contracts').mkdir(parents=True)
            path = root / 'tests/contracts/plan.json'
            path.write_text(json.dumps(manifest()))
            with patch('subprocess.run', return_value=SimpleNamespace(returncode=0, stdout=b'old')):
                with self.assertRaisesRegex(ValueError, 'commit the frozen plan'):
                    audit_manifest(path, root)

    def test_duplicate_json_fields_reject_before_artifact_access(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'plan.json'
            path.write_text('{"schema":"a","schema":"b"}')
            with self.assertRaisesRegex(ValueError, 'duplicate'):
                audit_manifest(path, Path(directory))


if __name__ == '__main__':
    unittest.main()
