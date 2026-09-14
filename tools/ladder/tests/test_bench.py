"""The common entry point must report benchmark verdicts, not runner exits."""

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

PATH = Path(__file__).resolve().parents[2] / "bench.py"
bench = None
if PATH.exists():
    spec = importlib.util.spec_from_file_location("compiler_bench", PATH)
    bench = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = bench
    spec.loader.exec_module(bench)


class BenchmarkEntryTest(unittest.TestCase):
    def test_targeted_selftests_refresh_only_the_runtime_and_keep_selection_explicit(self):
        options = {option for action in bench.parser()._actions for option in action.option_strings}
        self.assertIn('--refresh-selftest-runtime', options)
        self.assertIn('--selftest-select', options)
        args = bench.parser().parse_args(['kernel-selftest-all', '--refresh-selftest-runtime',
                                          '--selftest-select', 'ublk', '--selftest-select', 'mm:run_vmtests.sh'])
        command, _ = bench.command('kernel-selftest-all', args, Path('/unused/compiler'))
        self.assertIn('--refresh-runtime', command)
        self.assertIn('--install-deps', command)
        self.assertEqual(command[-4:], ['--select', 'ublk', '--select', 'mm:run_vmtests.sh'])

    def test_kernel_selftests_reuse_the_booted_image_without_a_compiler_build(self):
        self.assertIsNotNone(bench.CATALOG['kernel-selftest'][1])
        args = bench.parser().parse_args(['kernel-selftest', '--allow-qemu-drift'])
        command, env = bench.command('kernel-selftest', args, Path('/unused/compiler'))
        self.assertTrue(any(part.endswith('/k1/selftest.py') for part in command))
        self.assertNotIn('--evocc', command)
        self.assertEqual(env['K1_ALLOW_QEMU_DRIFT'], '1')

    def test_all_selftests_have_a_separate_measured_denominator(self):
        args = bench.parser().parse_args(['kernel-selftest-all', '--jobs', '2'])
        command, _ = bench.command('kernel-selftest-all', args, Path('/unused/compiler'))
        self.assertTrue(any(part.endswith('/k1/selftest_full.py') for part in command))
        self.assertNotIn('--evocc', command)
        data = {'status': 'INCOMPLETE', 'run': 'full', 'evidence': 'run', 'kernelImageSha256': 'image',
                'build': {'targets': ['mm', 'bpf']}, 'execution': {'subject': {'summary': {
                    'programs': 2, 'outcomes': {'PASS': 1, 'BUILD_UNAVAILABLE': 1},
                    'collections': 2, 'buildFailures': ['bpf']}}}}
        row = bench.metrics('kernel-selftest-all', data)
        self.assertEqual((row['passed'], row['total']), (1, 2))
        self.assertEqual(row['buildFailures'], ['bpf'])
        self.assertEqual(row['status'], 'INCOMPLETE')

    def test_full_selftest_reference_reuse_is_explicit_and_reuses_the_payload(self):
        args = bench.parser().parse_args(['kernel-selftest-all', '--reuse-selftest-reference', '/old/summary.json'])
        command, _ = bench.command('kernel-selftest-all', args, Path('/unused/compiler'))
        self.assertIn('--reuse-build', command)
        self.assertEqual(command[command.index('--reference-run') + 1], '/old/summary.json')

    def test_expanded_selftests_select_their_images_and_module_payload_explicitly(self):
        args = bench.parser().parse_args(['kernel-selftest-all', '--expanded-selftest-kernel',
                                         '--selftest-runtime', '/module/runtime', '--reuse-selftest-build',
                                         '--selftest-kernel-receipt', '/coverage/receipt.json',
                                         '--selftest-build-root', '/case-sensitive/selftests'])
        command, _ = bench.command('kernel-selftest-all', args, Path('/unused/compiler'))
        self.assertIn('--expanded-kernel', command)
        self.assertEqual(command[command.index('--runtime-root') + 1], '/module/runtime')
        self.assertEqual(command[command.index('--kernel-receipt') + 1], '/coverage/receipt.json')
        self.assertEqual(command[command.index('--work-root') + 1], '/case-sensitive/selftests')

    def test_kernel_selftest_only_invocation_does_not_require_or_build_evocc(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'tests/contracts').mkdir(parents=True)
            calls = []
            row = {'rung': 'K7', 'status': 'PASS', 'passed': 1, 'total': 1, 'kind': 'programs'}
            with patch.object(bench, 'ROOT', root), \
                 patch.object(bench, 'execute', side_effect=lambda cmd, env: (calls.append(cmd) or 0, [])), \
                 patch.object(bench, 'read_result', return_value=row), \
                 patch.object(bench.subprocess, 'check_output', return_value='checkout'), \
                 patch.object(bench.subprocess, 'run', return_value=SimpleNamespace(returncode=0)):
                self.assertEqual(bench.main(['kernel-selftest']), 0)
            self.assertEqual(len(calls), 1)
            self.assertTrue(calls[0][-1].endswith('/k1/selftest.py'))
            self.assertIsNone(json.loads((root / bench.RECEIPT).read_text())['driver'])

    def test_selftest_metrics_preserve_program_and_case_denominators(self):
        flight = {'boot': {'verdict': 'PASS', 'imageSha256': 'image'},
                  'outcomes': {'PASS': 1, 'SKIP': 1}, 'subtests': {'PASS': 3, 'SKIP': 2}}
        data = {'reference': flight, 'subject': flight, 'status': 'PASS', 'programs': 2,
                'baselineFailures': [], 'regressions': [], 'compilerComposition': [],
                'kernelImageSha256': 'image'}
        row = bench.metrics('kernel-selftest', data)
        self.assertEqual((row['passed'], row['total'], row['kind']), (1, 2, 'programs'))
        self.assertEqual(row['subtests'], {'PASS': 3, 'SKIP': 2})
        data['kernelImageSha256'] = 'different'
        self.assertEqual(bench.metrics('kernel-selftest', data)['status'], 'INCOMPLETE')

    def test_full_kernel_uses_strict_composition_not_the_mixed_link_bisection(self):
        args = bench.parser().parse_args(["kernel-full", "--jobs", "2", "--allow-qemu-drift"])
        command, env = bench.command("kernel-full", args, Path("/tmp/compiler"))
        self.assertIn("--require-all", command)
        self.assertIn("--boot", command)
        self.assertIn("--all", command)
        self.assertEqual(env["K1_ALLOW_QEMU_DRIFT"], "1")
        self.assertNotIn("--max-bisections", command)

    def test_full_kernel_pass_requires_complete_composition_and_the_same_booted_image(self):
        data = {"requestedUnits": 2, "counts": {"substituted": 2}, "completeComposition": True,
                "imageSha256": "image", "boot": {"verdict": "PASS", "imageSha256": "image"}}
        self.assertEqual(bench.metrics("kernel-full", data)["status"], "PASS")
        for change in ({"completeComposition": False}, {"counts": {"substituted": 1, "refused": 1}},
                       {"boot": {"verdict": "PASS", "imageSha256": "another"}},
                       {"boot": {"verdict": "TIMEOUT", "imageSha256": "image"}}):
            self.assertEqual(bench.metrics("kernel-full", {**data, **change})["passed"], 0)

    def setUp(self):
        self.assertIsNotNone(bench, "the benchmark entry point is not implemented")

    def test_catalog_and_multiple_selections(self):
        self.assertEqual(bench.select(["csmith,cjson", "csmith"]), ["csmith", "cjson"])
        self.assertIn("kernel", bench.select(["all"]))
        self.assertIn("mbedtls", bench.CATALOG)
        with self.assertRaises(ValueError):
            bench.select(["typo"])

    def test_command_uses_one_explicit_driver(self):
        args = bench.parser().parse_args(["csmith", "--programs", "17", "--jobs", "2"])
        command, env = bench.command("csmith", args, Path("/tmp/compiler"))
        self.assertIn("/tmp/compiler", command)
        self.assertEqual(command[command.index("--programs") + 1], "17")
        _, env = bench.command("kernel", args, Path("/tmp/compiler"))
        self.assertEqual(env["K1_EVOCC"], "/tmp/compiler")

    def test_kernel_replay_reuses_only_a_survey_of_the_selected_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binary = root / "compiler"
            binary.write_bytes(b"one compiler")
            survey = root / "survey"
            survey.mkdir()
            metadata = survey / "substitution.json"
            import hashlib
            metadata.write_text(json.dumps({
                "evoccSha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            }))
            args = bench.parser().parse_args([
                "kernel", "--reuse-kernel-survey", str(survey),
            ])
            command, env = bench.command("kernel", args, binary)
            self.assertEqual(command[command.index("--reuse-survey") + 1], str(survey.resolve()))
            self.assertEqual(env["K1_EVOCC"], str(binary))
            metadata.write_text(json.dumps({"evoccSha256": "another compiler"}))
            with self.assertRaises(ValueError):
                bench.command("kernel", args, binary)

    def test_program_failure_is_not_runner_success(self):
        receipt = {"suites": {"torture": {"programs": 4, "outcomes": {"pass": 2, "fail": 1, "refused": 1}}}}
        row = bench.metrics("torture", receipt)
        self.assertEqual((row["status"], row["passed"], row["total"]), ("INCOMPLETE", 2, 4))

    def test_kernel_survey_only_never_claims_a_boot(self):
        options = {option for action in bench.parser()._actions for option in action.option_strings}
        self.assertIn("--survey-only", options, "full compilation survey still requires link/bisection")
        args = bench.parser().parse_args(["kernel", "--survey-only", "--jobs", "6"])
        command, _ = bench.command("kernel", args, Path("/tmp/compiler"))
        self.assertIn("--no-install", command)
        self.assertIn("--all", command)
        self.assertIn("/tmp/compiler", command)
        row = bench.metrics("kernel", {"mode": "survey-only", "requestedUnits": 2,
                                      "counts": {"assembled": 2}, "refusalsByFirstCode": {}})
        self.assertEqual(row["compiled"], 2)
        self.assertEqual(row["passed"], 0)
        self.assertEqual(row["testVerdict"], "NOT_RUN")
        self.assertEqual(row["status"], "INCOMPLETE")

    def test_profile_closure_does_not_claim_deferred_programs_passed(self):
        values = {'programs': 3, 'outcomes': {'pass': 1, 'refused': 2},
                  'dispositionCounts': {'PASS': 1, 'compiler-failure': 0, 'not-applicable': 1, 'FP-deferred': 1, 'oracle-unusable': 0}}
        row = bench.metrics('torture', {'suites': {'torture': values}})
        self.assertEqual((row['status'], row['passed'], row['total']), ('PROFILE_CLEAN', 1, 3))
        self.assertEqual(row['dispositions'], values['dispositionCounts'])
        values['dispositionCounts']['compiler-failure'] = 1
        with self.assertRaises(ValueError):
            bench.metrics('torture', {'suites': {'torture': values}})


    def test_oracle_unavailable_stays_in_csmith_denominator(self):
        row = bench.metrics("csmith", {"programs": 10, "agreeing": 9, "campaigns": {
            "base": {"outcomes": {"agree": 9, "oracle-unusable": 1}}
        }})
        self.assertEqual(row["status"], "INCOMPLETE")
        self.assertEqual(row["outcomes"]["oracle-unusable"], 1)

    def test_cjson_reference_test_is_not_subject_validation(self):
        row = bench.metrics("cjson", {"unitsTotal": 2, "unitsInPassingBuild": 2,
                                      "test": {"passed": True, "relinkStatus": 0}})
        self.assertEqual(row["status"], "VALIDATION_GAP")
        self.assertEqual(row["passed"], 0)

    def test_cjson_object_test_coverage_is_required_for_every_substitution(self):
        receipt = {"unitsTotal": 2, "unitsInPassingBuild": 2,
                   "substitutedUnits": ["cJSON.o", "cJSON_Utils.o"],
                   "test": {"passed": True, "relinkStatus": 0,
                            "objectFiles": ["cJSON.o", "cJSON_Utils.o"]}}
        row = bench.metrics("cjson", receipt)
        self.assertEqual((row["status"], row["passed"]), ("PASS", 2))
        receipt["test"]["objectFiles"] = ["cJSON.o"]
        row = bench.metrics("cjson", receipt)
        self.assertEqual((row["status"], row["passed"]), ("VALIDATION_GAP", 0))
        receipt["test"]["objectFiles"].append("cJSON_Utils.o")
        receipt["test"]["passed"] = False
        self.assertEqual(bench.metrics("cjson", receipt)["status"], "FAIL")

    def test_bisection_exhaustion_and_baseline_failures_are_not_complete(self):
        for verdict in ["bisection-budget-exhausted", "libc-test-clean"]:
            row = bench.metrics("musl", {"units": 5, "unitsInPassingBuild": 5,
                                         "testResult": verdict,
                                         "baselineTest": {"libcTestFailingCases": 1}})
            self.assertNotEqual(row["status"], "PASS")

    def test_fresh_receipt_and_compiler_identity_are_required(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            receipt = root / "tests/contracts/ladder-e0-receipt.json"
            receipt.parent.mkdir(parents=True)
            value = {"latest": {"run": "new", "driverSha256": "wrong", "suites": {
                "ctest": {"programs": 1, "outcomes": {"pass": 1}}
            }}}
            receipt.write_text(json.dumps(value))
            with patch.object(bench, "ROOT", root):
                with self.assertRaisesRegex(ValueError, "unchanged"):
                    bench.read_result("ctest", receipt.read_bytes(), "wanted")
                with self.assertRaisesRegex(ValueError, "compiler"):
                    bench.read_result("ctest", b"old", "wanted")

    def test_empty_or_inconsistent_counts_cannot_pass(self):
        for total, passed in [(0, 0), (1, 2)]:
            with self.assertRaises(ValueError):
                bench.metrics("torture", {"suites": {"torture": {
                    "programs": total, "outcomes": {"pass": passed}
                }}})

    def test_kernel_boot_does_not_hide_failed_substitution_proof(self):
        row = bench.metrics("kernel", {
            "survey": {"requestedUnits": 2, "outcomeCounts": {"assembled": 2}},
            "unitsCompiledByEvoccInABootingKernel": 2,
            "mixedLink": {"boot": {"verdict": "PASS"}},
            "proofOfSubstitution": {"verdictCounts": {"different": 1}},
        })
        self.assertEqual(row["status"], "FAIL")

    def test_status_renderer_accepts_nonzero_mismatch_counts(self):
        path = PATH.parent / "render-status.py"
        spec = importlib.util.spec_from_file_location("benchmark_status", path)
        renderer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(renderer)
        self.assertTrue(hasattr(renderer, "record_count"), "integer mismatch counts must not crash status rendering")
        self.assertEqual(renderer.record_count(2), 2)
        self.assertEqual(renderer.record_count([{"program": "a"}]), 1)
        self.assertEqual(renderer.record_count(None), 0)

    def test_kunit_status_distinguishes_reference_results_from_subject_execution(self):
        spec = importlib.util.spec_from_file_location("benchmark_status", PATH.parent / "render-status.py")
        renderer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(renderer)
        self.assertTrue(hasattr(renderer, "kunit_progress"))
        self.assertIn("NOT_RUN", renderer.kunit_progress({}))
        line = renderer.kunit_progress({"reference": {"outcomes": {"PASS": 1324, "FAIL": 8, "SKIP": 108}},
                                        "subject": {"status": "NOT_RUN", "buildStatus": "RUNNING"}})
        self.assertIn("reference 1324 PASS / 8 FAIL / 108 SKIP", line)
        self.assertIn("subject NOT_RUN", line)

    def test_failed_lane_does_not_prevent_later_measurement(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "tests/contracts").mkdir(parents=True)
            binary = root / "compiler"
            binary.write_bytes(b"compiler")
            measured = []

            def execute(command, env):
                name = command[command.index("--suite") + 1]
                live = json.loads((root / bench.RECEIPT).read_text())
                self.assertEqual(live["benchmarks"][name]["status"], "RUNNING")
                measured.append(name)
                receipt = {"latest": {"run": f"{name}-{len(measured)}", "driverSha256": bench.sha256(binary),
                                      "suites": {name: {"programs": 1, "outcomes": {"pass": 1}}}}}
                (root / "tests/contracts/ladder-e0-receipt.json").write_text(json.dumps(receipt))
                return (7 if name == "ctest" else 0), ["failure detail"]

            with patch.object(bench, "ROOT", root), patch.object(bench, "execute", execute), \
                 patch.object(bench.subprocess, "check_output", return_value="commit"), \
                 patch.object(bench.subprocess, "run", return_value=SimpleNamespace(returncode=0)):
                self.assertEqual(bench.main(["ctest", "torture", "--evocc", str(binary)]), 2)
            self.assertEqual(measured, ["ctest", "torture"])
            report = json.loads((root / bench.RECEIPT).read_text())
            self.assertEqual(report["benchmarks"]["ctest"]["status"], "ERROR")
            self.assertEqual(report["benchmarks"]["torture"]["status"], "PASS")
            with patch.object(bench, "ROOT", root), patch.object(bench, "execute", execute), \
                 patch.object(bench.subprocess, "check_output", return_value="new-commit"), \
                 patch.object(bench.subprocess, "run", return_value=SimpleNamespace(returncode=0)):
                self.assertEqual(bench.main(["torture", "--evocc", str(binary)]), 0)
            latest = json.loads((root / bench.RECEIPT).read_text())
            self.assertIn("ctest", latest["benchmarks"], "a single-lane run must retain older milestone readings")
            self.assertEqual(latest["selected"], ["torture"])
            self.assertEqual(latest["benchmarks"]["ctest"]["measurement"]["run"], report["run"])
            self.assertEqual(latest["benchmarks"]["ctest"]["measurement"]["subject"], "commit")


if __name__ == "__main__":
    unittest.main()
