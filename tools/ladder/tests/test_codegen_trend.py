"""N05 trends must retain regressions, missing observations and per-TU scope."""

import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from tools.ladder import codegen_trend as trend
from tools.ladder import codegen_push
from tools.ladder.eqsat_size import codegen_compile_argv, codegen_disk_preflight, cleanup_codegen_products
from tools.ladder.tests.test_eqsat_elf import size_fixture


ROOT = Path(__file__).resolve().parents[3]


def observation(commit="1" * 40, capture="a" * 64, complete=True):
    units = [trend.object_row(name, profile, size_fixture(), size_fixture(),
                             gcc_o2_text_bytes=24,
                             compile_wall_ns=[90, 100, 110] if complete else None)
             for name in trend.UNITS for profile in trend.PROFILES]
    return {
        "schema": "evocc-codegen-trend-v1",
        "source_commit": commit,
        "compiler_sha256": "b" * 64,
        "plan_sha256": "c" * 64,
        "inputs_sha256": "d" * 64,
        "capture_sha256": capture,
        "variant": "O2-p0",
        "units": units,
        "ladder": [dict(case=name, status="PASS", result_sha256="e" * 64)
                   for name in trend.LADDER_CASES] if complete else [],
        "steps": dict(plan_sha256="f" * 64, expected_cases=["integer-mix"], cases=[dict(
            case="integer-mix", input_sha256="0" * 64,
            before_output_sha256="1" * 64, after_output_sha256="1" * 64,
            before_steps=100, after_steps=100, status="PASS")]) if complete else None,
        "proof": dict(completed_rejections=0, bundle_failures=0) if complete else None,
    }


class TrendTests(unittest.TestCase):
    def test_local_selection_counts_survive_discarding_assembly(self):
        from tools.ladder import eqsat_size
        assembly = b'''f:
    add x0, x0, x1, lsl #3
// evocc-local-selected local_shift_arithmetic=2
// evocc-local-declined local_shift_compare/temporary-not-single-definition-and-use=1
g:
// evocc-local-selected local_shift_arithmetic=3
// evocc-local-selected local_indexed_memory=1
'''
        self.assertEqual(eqsat_size.codegen_selection_counts(assembly), {
            'selected': {'local_shift_arithmetic': 5, 'local_indexed_memory': 1},
            'declined': {'local_shift_compare/temporary-not-single-definition-and-use': 1},
        })
        self.assertEqual(eqsat_size.codegen_selection_counts(b'ret\n'),
                         {'selected': {}, 'declined': {}})

    def test_failed_compile_keeps_rejection_and_rollback_buckets(self):
        from tools.ladder.eqsat_size import codegen_proof_failure
        fixture = ROOT / 'tests/fixtures/eqsat-chains/verified-chain/chain.json'
        for reason in ('PROOF_INVALID', 'CERTIFICATE_MALFORMED', 'CHECKER_TIMEOUT', 'PRODUCER_ERROR'):
            with self.subTest(reason=reason), tempfile.TemporaryDirectory() as folder:
                record = json.loads(fixture.read_bytes())
                tx = record['transactions'][0]
                record.update(transactions=[tx], attempted_transactions=1, verified_prefix=0,
                              publication='REJECTED', rejection_reason=reason, stop='ERROR', output_digest=None)
                tx.update(verification='REJECTED', rejection_reason=reason)
                if reason == 'PRODUCER_ERROR':
                    tx.update(result='ROLLED_BACK_ERROR', stop='INTERNAL_ERROR', verification='VERIFIED',
                              rejection_reason='NONE', output_digest=tx['input_digest'])
                    tx['roots']['accepted'] = 0
                (Path(folder) / 'chain.json').write_text(json.dumps(record))
                receipt = dict(proof=dict(completed_rejections=0, bundle_failures=0), failure_events=[])
                codegen_proof_failure(receipt, Path(folder), ValueError('compile failed'), 'compile')
                self.assertEqual(receipt['proof']['completed_rejections'], int(reason == 'PROOF_INVALID'))
                self.assertEqual(receipt['proof']['bundle_failures'], int(reason == 'CERTIFICATE_MALFORMED'))
                event = receipt['failure_events'][0]
                self.assertEqual(event['reason'], reason)
                self.assertEqual(event['evidence'], 'PRODUCER_TELEMETRY_NOT_PROVED')
                if reason == 'PRODUCER_ERROR':
                    self.assertEqual(event['transactions'][0]['bucket'], 'recovery_proof_successes')
                if reason == 'CHECKER_TIMEOUT':
                    self.assertEqual(event['transactions'][0]['bucket'], 'unknown_proof_outcomes')

    def test_replay_rejection_preserves_existing_audit_without_second_replay(self):
        from tools.ladder.eqsat_size import codegen_proof_failure
        from tools.ladder.eqsat_twins import BundleReplayRejected, bind_bundle
        for reason, completed, bundle in (('PROOF_INVALID', 1, 0), ('CERTIFICATE_MALFORMED', 0, 1),
                                           ('CHECKER_TIMEOUT', 0, 0)):
            audit = dict(accepted=False, proof_counts=dict(completed_proof_rejections=completed,
                         bundle_obligation_failures=bundle, unknown_proof_outcomes=int(reason == 'CHECKER_TIMEOUT')),
                         replay=[dict(index=0, verification='REJECTED', rejection_reason=reason)])
            receipt = dict(proof=dict(completed_rejections=0, bundle_failures=0), failure_events=[])
            with self.subTest(reason=reason), \
                 mock.patch('tools.ladder.eqsat_twins.verify_bundle', return_value=audit) as replay:
                with self.assertRaises(BundleReplayRejected) as failure:
                    bind_bundle(Path('nonexistent-capture'), Path('checker'))
                codegen_proof_failure(receipt, Path('nonexistent-capture'), failure.exception, 'replay')
                replay.assert_called_once()
                self.assertEqual(receipt['proof'], dict(completed_rejections=completed, bundle_failures=bundle))
                self.assertEqual(receipt['failure_events'][0]['replay'][0]['rejection_reason'], reason)

    def test_disk_floor_prevents_start_and_stops_below_reserve(self):
        resources = dict(jobs=1, min_free_bytes=25 * 1024**3, max_retained_bytes=500 * 1024**2)
        for free, start, succeeds in ((24, True, False), (25, True, True),
                                     (15, False, True), (14, False, False)):
            with self.subTest(free=free, start=start), \
                 mock.patch('tools.ladder.eqsat_size.shutil.disk_usage', return_value=mock.Mock(free=free * 1024**3)):
                if succeeds:
                    codegen_disk_preflight(ROOT, resources, start=start)
                else:
                    with self.assertRaises(ValueError):
                        codegen_disk_preflight(ROOT, resources, start=start)

    def test_cleanup_removes_only_owned_products_and_preserves_evidence(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            capture = root / 'new-capture'
            capture.mkdir()
            obj, receipt, reference = capture / '0.o', capture / 'receipt.json', root / 'reference.o'
            for path in (obj, receipt, reference):
                path.write_bytes(b'evidence')
            with self.assertRaisesRegex(ValueError, 'outside this capture'):
                cleanup_codegen_products(capture, [obj, reference])
            self.assertTrue(obj.exists())
            cleanup_codegen_products(capture, [obj, capture / 'already-removed.o'])
            self.assertFalse(obj.exists())
            self.assertEqual(receipt.read_bytes(), b'evidence')
            self.assertEqual(reference.read_bytes(), b'evidence')

    def test_fresh_compile_recipe_consumes_original_c_not_old_ir(self):
        case = dict(id="zlib", source_root=".build/ladder/zlib/zlib-1.3.2",
                    flags=["-DHAVE_HIDDEN"])
        argv = codegen_compile_argv("compiler", "checker", "a" * 64, case,
                                   "deflate.c", ROOT, "unchecked", Path("new-chain"))
        self.assertEqual(argv[1], "--emit=asm")
        self.assertEqual(Path(argv[-1]), ROOT / case["source_root"] / "deflate.c")
        self.assertIn("--ub-checks=unchecked", argv)
        self.assertIn("-Oeq-scalar", argv)
        self.assertNotIn("output.evo", " ".join(map(str, argv)))

    def test_n02_report_adapter_joins_pass_execution_but_not_missing_inputs(self):
        first = dict(input_id="main", input_sha256="0" * 64, before_steps=100,
                     after_steps=90, before_outcome_sha256="1" * 64,
                     after_outcome_sha256="1" * 64, outcome="DEFINED_MATCH")
        second = dict(first, before_steps=90, after_steps=80)
        report = dict(schema="evocc-pass-report-v1", passes=[dict(executions=[first]),
                                                           dict(executions=[second])])
        steps = trend.steps_from_reports([report], "f" * 64, ["integer-mix", "generated-1"],
                                         {"main": "integer-mix"})
        self.assertEqual(steps["cases"][0]["before_steps"], 100)
        self.assertEqual(steps["cases"][0]["after_steps"], 80)
        self.assertEqual(len(steps["cases"]), 1)
        second["before_steps"] = 89
        with self.assertRaisesRegex(ValueError, "discontinuous"):
            trend.steps_from_reports([report], "f" * 64, ["integer-mix"], {"main": "integer-mix"})

    def test_diagnostic_parity_is_not_a_zero_step_success(self):
        report = dict(schema="evocc-pass-report-v1", passes=[dict(executions=[dict(
            input_id="integer-mix", input_sha256="0" * 64, before_steps=None, after_steps=None,
            before_outcome_sha256="1" * 64, after_outcome_sha256="1" * 64,
            outcome="DIAGNOSTIC_MATCH:E3003")])])
        row = observation()
        row["steps"] = trend.steps_from_reports([report], "f" * 64, ["integer-mix"])
        self.assertEqual(trend.coverage(row)["status"], "OBSERVED")
        self.assertIsNone(row["steps"]["cases"][0]["after_steps"])
        self.assertEqual(trend.compare(row, row)["steps"][0]["status"], "DIAGNOSTIC_MATCH_NO_STEP_METRIC")

    def test_read_only_opaque_measurement_is_not_dropped_or_called_tv(self):
        report = dict(schema="evocc-pass-corpus-v1", cases=[dict(
            input_id="integer-mix", input_sha256="0" * 64, before_steps=100, after_steps=100,
            before_outcome_sha256="1" * 64, after_outcome_sha256="1" * 64,
            outcome="DEFINED_MATCH", validation="NOT_ADMITTED_OPAQUE")])
        steps = trend.steps_from_reports([report], "f" * 64, ["integer-mix"])
        self.assertEqual(steps["cases"][0]["after_steps"], 100)
        self.assertNotIn("VALIDATED", json.dumps(steps))

    def test_source_bridge_mismatch_remains_visible_beside_ir_step_measurement(self):
        detail = dict(input_id="integer-mix", chir_events=1001, evoir_events=1002,
                      chir_return=0, evoir_return=0, first_difference=0)
        report = dict(schema="evocc-pass-corpus-v1", source_bridge="MISMATCH_NOT_VALIDATED",
                      source_bridge_detail=detail, cases=[dict(input_id="integer-mix", input_sha256="0" * 64,
            before_steps=100, after_steps=100, before_outcome_sha256="1" * 64,
            after_outcome_sha256="1" * 64, outcome="DEFINED_MATCH", validation="NOT_ADMITTED_OPAQUE",
            source_bridge="MISMATCH_NOT_VALIDATED")])
        row = observation()
        row['steps'] = trend.steps_from_reports([report], "f" * 64, ["integer-mix"])
        self.assertEqual(row['steps']['cases'][0]['source_bridge'], 'MISMATCH_NOT_VALIDATED')
        self.assertEqual(row['steps']['cases'][0]['source_bridge_detail'], detail)
        self.assertEqual(row['steps']['cases'][0]['validation'], 'NOT_ADMITTED_OPAQUE')
        self.assertEqual(trend.coverage(row)['status'], 'OBSERVED')
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'trend.jsonl'
            trend.append_observation(path, row)
            self.assertIn('integer-mix=MISMATCH_NOT_VALIDATED', trend.render_trend(trend.read_observations(path)))
            self.assertEqual(trend.read_observations(path)[0]['steps']['cases'][0]['source_bridge_detail'], detail)

    def test_s1_object_rows_retain_partition_and_byte_identity(self):
        row = trend.object_row(trend.UNITS[0], "unchecked", size_fixture(), size_fixture(),
                               gcc_o2_text_bytes=24, compile_wall_ns=[3, 1, 2])
        self.assertEqual(row["text_bytes"], 32)
        self.assertEqual(row["instruction_bytes"], 16)
        self.assertEqual(row["padding_bytes"], 8)
        self.assertEqual(row["literal_pool_or_intext_data_bytes"], 8)
        self.assertEqual(row["compile_wall_ns"], [3, 1, 2])
        with self.assertRaisesRegex(ValueError, "reproducible"):
            trend.object_row(trend.UNITS[0], "unchecked", size_fixture(),
                             size_fixture(pool_in_text=False), gcc_o2_text_bytes=24)

    def test_probe_without_gcc_measurement_never_invents_a_ratio(self):
        record = observation(complete=False)
        record["units"] = [trend.object_row("test:n05_regression_probe", "unchecked",
                           size_fixture(), size_fixture(), gcc_o2_text_bytes=None)]
        self.assertEqual(trend.coverage(record)["status"], "NOT_RUN")
        self.assertIsNone(trend.compare(record, record)["units"][0]["gcc_o2_ratio"])
        self.assertIn("GCC-O2", trend.render_trend([record]))

    def test_append_is_idempotent_but_never_overwrites_a_conflict(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trend.jsonl"
            record = observation()
            self.assertTrue(trend.append_observation(path, record))
            before = path.read_bytes()
            self.assertFalse(trend.append_observation(path, copy.deepcopy(record)))
            self.assertEqual(path.read_bytes(), before)
            record["units"][0]["compile_wall_ns"][0] = 101
            with self.assertRaisesRegex(ValueError, "conflicting observation"):
                trend.append_observation(path, record)
            self.assertEqual(path.read_bytes(), before)

    def test_retry_keeps_failed_observation(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trend.jsonl"
            failed, retry = observation(), observation(capture="9" * 64)
            failed["ladder"][0]["status"] = "FAIL"
            trend.append_observation(path, failed)
            trend.append_observation(path, retry)
            rows = trend.read_observations(path)
            self.assertEqual(len(rows), 2)
            self.assertEqual(rows[0]["ladder"][0]["status"], "FAIL")

    def test_missing_fields_do_not_satisfy_per_push_coverage(self):
        complete = observation()
        self.assertEqual(trend.coverage(complete)["status"], "OBSERVED")
        mutations = [
            lambda x: x["units"].pop(),
            lambda x: x["units"][0].update(compile_wall_ns=None),
            lambda x: x.update(steps=None),
            lambda x: x.update(ladder=[]),
            lambda x: x.update(proof=None),
            lambda x: x["steps"]["expected_cases"].append("generated-0001"),
        ]
        for mutation in mutations:
            row = copy.deepcopy(complete)
            mutation(row)
            self.assertEqual(trend.coverage(row)["status"], "NOT_RUN")
        self.assertEqual(trend.coverage(observation(complete=False))["status"], "NOT_RUN")

    def test_mismatched_results_and_rejected_certificates_are_failures(self):
        mutations = [
            lambda x: x["ladder"][0].update(status="FAIL"),
            lambda x: x["steps"]["cases"][0].update(after_output_sha256="2" * 64),
            lambda x: x["proof"].update(completed_rejections=1),
            lambda x: x["proof"].update(bundle_failures=1),
        ]
        for mutation in mutations:
            row = observation()
            mutation(row)
            self.assertEqual(trend.coverage(row)["status"], "FAIL")
            self.assertEqual(trend.compare(observation(), row)["status"], "FAIL")

    def test_individual_regression_cannot_be_hidden_by_other_unit_savings(self):
        before, after = observation(), observation("2" * 40, "9" * 64)
        after["units"][0]["text_bytes"] += 4
        after["units"][0]["instruction_bytes"] += 4
        after["units"][0]["static_instruction_slots"] += 1
        after["units"][2]["text_bytes"] -= 8
        after["units"][2]["padding_bytes"] -= 8
        result = trend.compare(before, after)
        self.assertEqual(result["status"], "REGRESSION")
        self.assertEqual(result["units"][0]["text_delta"], 4)
        self.assertEqual(result["units"][2]["text_delta"], -8)
        self.assertEqual(len(result["units"]), 12)
        self.assertNotIn("average", result)

    def test_compile_and_steps_bounds_are_independent(self):
        before, after = observation(), observation("2" * 40, "9" * 64)
        after["units"][0]["compile_wall_ns"] = [126, 127, 128]
        after["steps"]["cases"][0]["after_steps"] = 106
        result = trend.compare(before, after)
        self.assertIn("compile_time", result["units"][0]["regressions"])
        self.assertEqual(result["steps"][0]["status"], "TRADEOFF")

    def test_incompatible_inputs_and_profiles_are_not_compared(self):
        before, after = observation(), observation("2" * 40, "9" * 64)
        for field, changed in (("inputs_sha256", "3" * 64), ("variant", "O2-off"),
                               ("plan_sha256", "4" * 64)):
            row = copy.deepcopy(after)
            row[field] = changed
            with self.assertRaisesRegex(ValueError, "incomparable"):
                trend.compare(before, row)

    def test_duplicate_rows_bad_counters_and_raw_capture_fields_are_rejected(self):
        mutations = [
            lambda x: x["units"].append(copy.deepcopy(x["units"][0])),
            lambda x: x["units"][0].update(text_bytes=True),
            lambda x: x["units"][0].update(text_bytes=-1),
            lambda x: x.update(raw_assembly="nop\nret\n"),
            lambda x: x["units"][0].update(compile_wall_ns=[1, 2]),
        ]
        for mutation in mutations:
            row = observation()
            mutation(row)
            with self.assertRaises(ValueError):
                trend.validate_observation(row)

    def test_renderer_is_read_only_and_per_unit(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trend.jsonl"
            trend.append_observation(path, observation())
            before = path.read_bytes()
            result = subprocess.run([sys.executable, str(ROOT / "tools/render-status.py"),
                                     "--codegen-trend", str(path)],
                                    cwd=ROOT, text=True, capture_output=True, check=True)
            self.assertIn("unchecked", result.stdout)
            self.assertIn("GCC-O2", result.stdout)
            self.assertIn("p0:", result.stdout)
            self.assertEqual(path.read_bytes(), before)

    def test_direct_tools_do_not_break_standard_library_imports(self):
        command = [sys.executable, "-c", "import random, statistics; "
                   "print(statistics.median([3,1,2])); print(random.Random(1).random())"]
        result = subprocess.run(command, cwd=ROOT / "tools", capture_output=True,
                                text=True, check=True)
        self.assertIn("2\n", result.stdout)

    def test_reading_does_not_hide_an_interrupted_append(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trend.jsonl"
            path.write_text(json.dumps(observation()) + '\n{"schema":')
            with self.assertRaisesRegex(ValueError, "invalid trend line 2"):
                trend.read_observations(path)


class BisectTests(unittest.TestCase):
    def records(self, sizes):
        rows = []
        for index, size in enumerate(sizes, 1):
            row = observation(f"{index:040x}", f"{index:064x}")
            for unit in row["units"]:
                unit.update(text_bytes=size, instruction_bytes=size - 16,
                            static_instruction_slots=(size - 16) // 4)
            rows.append(row)
        return rows

    def test_real_ancestry_order_not_input_order_selects_first_bad(self):
        rows = self.records([32, 32, 36, 40])
        result = trend.locate_regression([r["source_commit"] for r in rows],
                                         list(reversed(rows)), trend.UNITS[0], "unchecked")
        self.assertEqual(result["status"], "REGRESSION_FOUND")
        self.assertEqual(result["first_bad"], rows[2]["source_commit"])
        self.assertEqual(result["rebuild_commits"], [])

    def test_recovery_does_not_erase_an_earlier_regression(self):
        rows = self.records([32, 36, 32, 40])
        result = trend.locate_regression([r["source_commit"] for r in rows], rows,
                                         trend.UNITS[0], "unchecked")
        self.assertEqual(result["first_bad"], rows[1]["source_commit"])

    def test_missing_midpoint_only_reports_an_interval(self):
        rows = self.records([32, 32, 36, 40])
        result = trend.locate_regression([r["source_commit"] for r in rows],
                                         [rows[0], rows[3]], trend.UNITS[0], "unchecked")
        self.assertEqual(result["status"], "INCOMPLETE")
        self.assertIsNone(result["first_bad"])
        self.assertEqual(result["rebuild_commits"], [r["source_commit"] for r in rows[1:3]])

    def test_later_missing_receipts_do_not_undo_a_proven_first_boundary(self):
        rows = self.records([32, 36, 40, 40])
        result = trend.locate_regression([r["source_commit"] for r in rows], rows[:2],
                                         trend.UNITS[0], "unchecked")
        self.assertEqual(result["status"], "REGRESSION_FOUND")
        self.assertEqual(result["first_bad"], rows[1]["source_commit"])

    def test_disagreeing_retries_are_ambiguous_not_cherry_picked(self):
        rows = self.records([32, 36])
        retry = copy.deepcopy(rows[1])
        retry["capture_sha256"] = "9" * 64
        for unit in retry["units"]:
            unit.update(text_bytes=32, instruction_bytes=16, static_instruction_slots=4)
        result = trend.locate_regression([r["source_commit"] for r in rows], rows + [retry],
                                         trend.UNITS[0], "unchecked")
        self.assertEqual(result["status"], "INCOMPLETE")
        self.assertEqual(result["ambiguous_commits"], [rows[1]["source_commit"]])

    def test_bad_metric_and_invalid_unit_fail_closed(self):
        rows = self.records([32, 36])
        for unit, metric in (("misspelled-unit", "text_bytes"), (trend.UNITS[0], "guess")):
            with self.subTest(unit=unit), self.assertRaises(ValueError):
                trend.locate_regression([r["source_commit"] for r in rows], rows,
                                        unit, "unchecked", metric)

    def test_steps_from_different_corpora_are_not_comparable(self):
        rows = self.records([32, 36])
        rows[1]["steps"]["plan_sha256"] = "a" * 64
        result = trend.locate_regression([r["source_commit"] for r in rows], rows,
                                         "integer-mix", "unchecked", "steps")
        self.assertEqual(result["status"], "INCOMPLETE")

    def test_nonancestor_endpoints_reject_before_measurement(self):
        with self.assertRaises((ValueError, subprocess.CalledProcessError)):
            trend.git_commits(ROOT, "HEAD", "HEAD~1")


class ObjectBaselineTests(unittest.TestCase):
    def test_codegen_failure_after_identity_chain_is_not_certificate_failure(self):
        from tools.ladder.eqsat_size import codegen_proof_failure
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            ir = (ROOT / 'tests/fixtures/eqsat-chains/verified-chain/000.before.evo').read_bytes()
            value = trend.digest(ir)
            record = dict(schema='evocc-identity-chain-v1', mode='eqsat-off', publication='IDENTITY_VERIFIED',
                          attempted_transactions=0, transactions=[], input_digest=value, output_digest=value,
                          chain_id=trend.digest(f'identity-chain-v1\n{value}\n'.encode()))
            (root / 'chain.json').write_text(json.dumps(record))
            for name in ('input.evo', 'output.evo'):
                (root / name).write_bytes(ir)
            receipt = dict(proof=dict(completed_rejections=0, bundle_failures=0), failure_events=[])
            codegen_proof_failure(receipt, root, ValueError('emit_asm failed'), 'compile')
            self.assertEqual(receipt['proof'], dict(completed_rejections=0, bundle_failures=0))
            event = receipt['failure_events'][0]
            self.assertEqual(event['reason'], 'COMPILE_FAILED_AFTER_IDENTITY_CHAIN')
            self.assertEqual(event['evidence'], 'BYTE_IDENTITY_ONLY_NOT_EVP_REPLAY')
            self.assertNotIn('proof_successes', event)
            # A missing identity artifact is still a bundle failure, not the
            # earlier frontend case in which no chain was ever published.
            (root / 'output.evo').unlink()
            broken = dict(proof=dict(completed_rejections=0, bundle_failures=0), failure_events=[])
            codegen_proof_failure(broken, root, ValueError('emit_asm failed'), 'compile')
            self.assertEqual(broken['proof'], dict(completed_rejections=0, bundle_failures=1))
            self.assertEqual(broken['failure_events'][0]['reason'], 'BUNDLE_MISMATCH')

    def test_identity_chain_baseline_disables_eqsat_but_keeps_bundle(self):
        case = dict(id='p0', source_root='.', flags=[])
        args = ('compiler', 'checker', 'a' * 64, case,
                'tests/fixtures/eqsat-performance/integer-mix.c', ROOT, 'checked', Path('chain'))
        default = codegen_compile_argv(*args)
        self.assertEqual(default, codegen_compile_argv(*args, eqsat=True))
        identity = codegen_compile_argv(*args, eqsat=False)
        self.assertNotIn('-Oeq-scalar', identity)
        self.assertFalse(any(str(arg).startswith('--eq-checker') for arg in identity))
        self.assertIn('--eq-proof-dir=chain', identity)
        self.assertIn('--ub-checks=checked', identity)
        self.assertEqual(identity[-1], default[-1])

    def test_only_explicit_first_objects_survive_cleanup(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            first, twin, executable = root / '0-checked-0.o', root / '0-checked-1.o', root / 'program'
            for path in (first, twin, executable):
                path.write_bytes(b'owned')
            cleanup_codegen_products(root, [first, twin, executable], retain=[first])
            self.assertTrue(first.exists())
            self.assertFalse(twin.exists())
            self.assertFalse(executable.exists())
            cleanup_codegen_products(root, [first])
            self.assertFalse(first.exists())

    def test_retention_cannot_select_outside_or_unowned_objects(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            capture = root / 'capture'
            capture.mkdir()
            owned, outside, unowned = capture / 'owned.o', root / 'reference.o', capture / 'unowned.o'
            for path in (owned, outside, unowned):
                path.write_bytes(b'keep')
            for retained in (outside, unowned):
                with self.subTest(path=retained), self.assertRaises(ValueError):
                    cleanup_codegen_products(capture, [owned], retain=[retained])
                self.assertTrue(owned.exists())

    def baseline(self, root):
        original = size_fixture()
        obj = root / 'first.o'
        obj.write_bytes(original)
        record = dict(schema='evocc-codegen-capture-v1', status='OBSERVED', retained_objects=[dict(
            unit=trend.UNITS[0], profile='checked', path='first.o', object_sha256=trend.digest(original))])
        path = root / 'receipt.json'
        path.write_text(json.dumps(record))
        return original, obj, record, path

    def test_candidate_must_equal_full_baseline_object_not_just_text_size(self):
        from tools.ladder.eqsat_size import compare_codegen_object_baseline
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            original, _, _, receipt = self.baseline(root)
            compare_codegen_object_baseline(root, receipt, trend.UNITS[0], 'checked', original)
            changed = original[:-1] + bytes([original[-1] ^ 1])
            with self.assertRaises(ValueError):
                compare_codegen_object_baseline(root, receipt, trend.UNITS[0], 'checked', changed)

    def test_baseline_lookup_is_per_unit_profile_and_requires_success(self):
        from tools.ladder.eqsat_size import compare_codegen_object_baseline
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            original, _, record, receipt = self.baseline(root)
            for unit, profile in ((trend.UNITS[1], 'checked'), (trend.UNITS[0], 'unchecked')):
                with self.subTest(unit=unit, profile=profile), self.assertRaises(ValueError):
                    compare_codegen_object_baseline(root, receipt, unit, profile, original)
            for status in ('FAILED', 'NOT_RUN'):
                record['status'] = status
                receipt.write_text(json.dumps(record))
                with self.subTest(status=status), self.assertRaises(ValueError):
                    compare_codegen_object_baseline(root, receipt, trend.UNITS[0], 'checked', original)

    def test_changed_retained_object_cannot_redefine_the_baseline(self):
        from tools.ladder.eqsat_size import compare_codegen_object_baseline
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            original, obj, _, receipt = self.baseline(root)
            changed = original[:-1] + bytes([original[-1] ^ 1])
            obj.write_bytes(changed)
            with self.assertRaises(ValueError):
                compare_codegen_object_baseline(root, receipt, trend.UNITS[0], 'checked', changed)


class PushTests(unittest.TestCase):
    def test_missing_step_or_ladder_data_blocks_push_admission(self):
        record = observation(complete=False)
        self.assertEqual(codegen_push.missing_measurements([record["source_commit"]],
                         [record], record["plan_sha256"], record["variant"]), [record["source_commit"]])
        record = observation()
        self.assertEqual(codegen_push.missing_measurements([record["source_commit"]],
                         [record], record["plan_sha256"], record["variant"]), [])

    def test_read_only_push_preview_does_not_build_or_checkout(self):
        record = observation(complete=False)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = dict(schema="evocc-codegen-measure-plan-v1", history="trend.jsonl", variant="O2-p0")
            plan = root / "plan.json"
            plan.write_text(json.dumps(config))
            with mock.patch.object(codegen_push, "pushed_commits", return_value=[record["source_commit"]]), \
                 mock.patch.object(codegen_push, "git", return_value=record["source_commit"]), \
                 mock.patch.object(codegen_push.subprocess, "run") as execution:
                result = codegen_push.run(root, plan, [], execute=False)
                self.assertEqual(result["status"], "NOT_RUN")
                execution.assert_not_called()


if __name__ == "__main__":
    unittest.main()
