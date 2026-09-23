"""The common entry point must report benchmark verdicts, not runner exits."""

import importlib.util
import copy
from contextlib import redirect_stderr, redirect_stdout
import io
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
    def test_kunit_has_a_distinct_adapter_and_receipt_without_building_a_compiler(self):
        self.assertIn('kernel-kunit', bench.CATALOG)
        args = bench.parser().parse_args(['kernel-kunit', '--allow-qemu-drift'])
        command, env = bench.command('kernel-kunit', args, Path('/unused/compiler'))
        self.assertTrue(any(part.endswith('/k1/kunit.py') for part in command))
        self.assertNotIn('--evocc', command)
        self.assertEqual(env['K1_ALLOW_QEMU_DRIFT'], '1')
        self.assertEqual(bench.lane_receipt('kernel-kunit').name, 'kernel-kunit-bench-receipt.json')
        self.assertIn('kernel-kunit', bench.select(['all']))

    def test_kunit_metrics_do_not_promote_module_load_or_shared_failures(self):
        builtin = {'complete': True, 'tests': 1504, 'outcomes': {'PASS': 1504},
                   'boot': {'verdict': 'PASS', 'imageSha256': 'image'}}
        modules = {'complete': True, 'tests': 765, 'outcomes': {'PASS': 765}, 'filesLoaded': 31,
                   'boot': {'verdict': 'PASS', 'imageSha256': 'image'}}
        data = {'status': 'PASS', 'executionComplete': True, 'kernelImageSha256': 'image',
                'comparison': {'executionComplete': True}, 'moduleComparison': {'executionComplete': True},
                'reference': {**builtin, 'modules': copy.deepcopy(modules)},
                'subject': {**builtin, 'modules': copy.deepcopy(modules)}}
        row = bench.metrics('kernel-kunit', data)
        self.assertEqual((row['status'], row['passed'], row['total']), ('PASS', 2269, 2269))
        for field, value in (('filesLoaded', 30), ('complete', False), ('tests', 764),
                             ('boot', {'verdict': 'PASS', 'imageSha256': 'old'})):
            changed = copy.deepcopy(data)
            changed['subject']['modules'][field] = value
            with self.subTest(field=field):
                self.assertEqual(bench.metrics('kernel-kunit', changed)['status'], 'INCOMPLETE')
        for outcome, expected in (('FAIL', 'FAIL'), ('SKIP', 'INCOMPLETE')):
            changed = copy.deepcopy(data)
            changed['reference']['modules']['outcomes'] = {'PASS': 764, outcome: 1}
            self.assertEqual(bench.metrics('kernel-kunit', changed)['status'], expected)
        data['moduleComparison']['executionComplete'] = False
        self.assertEqual(bench.metrics('kernel-kunit', data)['status'], 'INCOMPLETE')
        row = bench.metrics('kernel-kunit', {'status': 'RUNNING'})
        self.assertEqual((row['status'], row['passed'], row['outcomes']),
                         ('INCOMPLETE', 0, {'NOT_RUN': 2269}))

    def test_kunit_passes_explicit_inputs_and_rejects_reference_reuse_or_selection(self):
        values = ['source-root', 'kernel-receipt', 'reference-boot', 'reference-runtime',
                  'subject-runtime', 'builtin-initramfs']
        args = bench.parser().parse_args(['kernel-kunit', *[
            part for option in values for part in ('--kunit-' + option, '/chosen/' + option)]])
        command, _ = bench.command('kernel-kunit', args, Path('/unused'))
        for option in values:
            self.assertEqual(command[command.index('--' + option) + 1], '/chosen/' + option)
        for extra in (['--reuse-selftest-reference', '/old'], ['--selftest-select', 'one'],
                      ['--reuse-kernel-survey', '/old']):
            with self.subTest(extra=extra):
                args = bench.parser().parse_args(['kernel-kunit', *extra])
                with self.assertRaises(ValueError):
                    bench.command('kernel-kunit', args, Path('/unused'))

    def test_kunit_status_and_dry_run_never_start_or_record_a_measurement(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with patch.object(bench, 'ROOT', root), patch.object(bench, 'execute') as execute, \
                 patch.object(bench, 'save') as save, redirect_stdout(io.StringIO()) as output:
                self.assertEqual(bench.main(['kernel-kunit', '--status']), 0)
                self.assertIn('NOT_RUN', output.getvalue())
                self.assertEqual(bench.main(['kernel-kunit', '--dry-run']), 0)
                execute.assert_not_called()
                save.assert_not_called()
            self.assertEqual(list(root.iterdir()), [])

    def test_kunit_only_run_needs_no_compiler_and_does_not_read_an_old_receipt_as_new(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'tests/contracts').mkdir(parents=True)
            calls = []
            row = {'rung': 'K7', 'status': 'INCOMPLETE', 'passed': 1, 'total': 2, 'kind': 'cases'}
            with patch.object(bench, 'ROOT', root), \
                 patch.object(bench, 'execute', side_effect=lambda cmd, env: (calls.append(cmd) or 0, [])), \
                 patch.object(bench, 'read_result', return_value=row), \
                 patch.object(bench.subprocess, 'check_output', return_value='checkout'), \
                 patch.object(bench.subprocess, 'run', return_value=SimpleNamespace(returncode=0)):
                self.assertEqual(bench.main(['kernel-kunit']), 1)
            self.assertEqual(len(calls), 1)
            self.assertTrue(calls[0][-1].endswith('/k1/kunit.py'))
            self.assertIsNone(json.loads((root / bench.RECEIPT).read_text())['driver'])
            path = root / 'tests/contracts/kernel-kunit-bench-receipt.json'
            path.write_bytes(b'old result')
            with self.assertRaisesRegex(ValueError, 'unchanged'):
                bench.read_result('kernel-kunit', path.read_bytes(), None, receipt_path=path)

    def test_supplemental_kernel_survey_uses_its_existing_trees_and_bounded_storage(self):
        options = {option for action in bench.parser()._actions for option in action.option_strings}
        self.assertIn('--kernel-work-root', options)
        args = bench.parser().parse_args(['kernel', '--survey-only', '--low-disk',
                                         '--kernel-work-root', '/case/coverage',
                                         '--kernel-source-root', '/case/linux'])
        command, _ = bench.command('kernel', args, Path('/fixed/compiler'))
        self.assertEqual(command[command.index('--k1-root') + 1], '/case/coverage')
        self.assertEqual(command[command.index('--src-root') + 1], '/case/linux')
        self.assertIn('--low-disk', command)
        self.assertIn('--no-install', command)
        args.survey_only = False
        with self.assertRaises(ValueError):
            bench.command('kernel', args, Path('/fixed/compiler'))

    def test_status_uses_the_selftests_named_kernel_not_always_the_boot_milestone(self):
        spec = importlib.util.spec_from_file_location('render_status', PATH.parent / 'render-status.py')
        status = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(status)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'docs').mkdir()
            (root / 'docs/evoir-spec.md').write_text('**Revision 1.0.1**')
            (root / 'docs/evomir-spec.md').write_text('**Revision 0.1**')
            contracts = root / 'tests/contracts'
            contracts.mkdir(parents=True)
            (contracts / 'kernel-full-receipt.json').write_text(json.dumps({'imageSha256': 'original'}))
            (contracts / 'kernel-kunit-receipt.json').write_text(json.dumps({
                'builtInComposition': {'imageSha256': 'expanded'}}))
            for receipt_path, image, expected in (
                (None, 'original', 'INCOMPLETE'),
                ('tests/contracts/kernel-kunit-receipt.json', 'expanded', 'INCOMPLETE'),
                ('tests/contracts/kernel-kunit-receipt.json', 'changed', 'STALE_IMAGE'),
                ('tests/contracts/missing.json', 'expanded', 'STALE_IMAGE'),
            ):
                with self.subTest(receipt=receipt_path, image=image):
                    receipt = {'status': 'INCOMPLETE', 'kernelImageSha256': image}
                    if receipt_path:
                        receipt['kernelReceiptPath'] = receipt_path
                    for name in ('kernel-selftest-receipt.json', 'kernel-selftest-full-receipt.json'):
                        (contracts / name).write_text(json.dumps(receipt))
                    output = root / 'docs/status.md'
                    with patch.object(status, 'ROOT', root), patch.object(status, 'OUT', output), \
                         redirect_stdout(io.StringIO()):
                        self.assertEqual(status.main([]), 0)
                    text = output.read_text()
                    self.assertIn(f'K7 selected {expected}', text)
                    self.assertIn(f'K7 all programs {expected}', text)

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
                                         '--selftest-reference-boot', '/coverage/reference-boot.json',
                                         '--selftest-build-root', '/case-sensitive/selftests',
                                         '--selftest-source-root', '/case-sensitive/linux',
                                         '--selftest-headers-root', '/case-sensitive/headers',
                                         '--selftest-kernel-build-root', '/case-sensitive/reference'])
        command, _ = bench.command('kernel-selftest-all', args, Path('/unused/compiler'))
        self.assertIn('--expanded-kernel', command)
        self.assertEqual(command[command.index('--runtime-root') + 1], '/module/runtime')
        self.assertEqual(command[command.index('--kernel-receipt') + 1], '/coverage/receipt.json')
        self.assertEqual(command[command.index('--reference-boot') + 1], '/coverage/reference-boot.json')
        self.assertEqual(command[command.index('--work-root') + 1], '/case-sensitive/selftests')
        self.assertEqual(command[command.index('--source-root') + 1], '/case-sensitive/linux')
        self.assertEqual(command[command.index('--headers-root') + 1], '/case-sensitive/headers')
        self.assertEqual(command[command.index('--kernel-build-root') + 1], '/case-sensitive/reference')

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

    def test_full_kernel_can_resume_only_the_verified_prepared_stage(self):
        options = {option for action in bench.parser()._actions for option in action.option_strings}
        self.assertIn('--resume-kernel-prepared', options)
        args = bench.parser().parse_args(['kernel-full', '--resume-kernel-prepared', '.evidence/k1/previous'])
        command, _ = bench.command('kernel-full', args, Path('/tmp/compiler'))
        self.assertEqual(command[command.index('--resume-prepared-run') + 1],
                         str(Path('.evidence/k1/previous').resolve()))
        self.assertIn('--require-all', command)
        self.assertIn('--boot', command)

        with self.assertRaisesRegex(ValueError, 'requires kernel-full'):
            bench.command('kernel', args, Path('/tmp/compiler'))

    def test_supplemental_full_kernel_keeps_its_roots_and_separate_receipt(self):
        options = {option for action in bench.parser()._actions for option in action.option_strings}
        self.assertIn('--kernel-full-receipt', options)
        args = bench.parser().parse_args([
            'kernel-full', '--kernel-work-root', '.build/bpf',
            '--kernel-source-root', '.build/linux-src', '--low-disk', '--jobs', '2',
            '--kernel-full-receipt', 'tests/contracts/kernel-bpf-full-receipt.json'])
        command, _ = bench.command('kernel-full', args, Path('/tmp/compiler'))
        for flag, value in (('--k1-root', str(Path('.build/bpf').resolve())),
                            ('--src-root', str(Path('.build/linux-src').resolve())),
                            ('--receipt', str(bench.ROOT / 'tests/contracts/kernel-bpf-full-receipt.json')),
                            ('--relink-jobs', '2')):
            self.assertEqual(command[command.index(flag) + 1], value)
        self.assertIn('--low-disk', command)
        self.assertIn('--require-all', command)
        self.assertIn('--boot', command)
        self.assertEqual(bench.lane_receipt('kernel-full', args),
                         bench.ROOT / 'tests/contracts/kernel-bpf-full-receipt.json')
        args.kernel_full_receipt = None
        with self.assertRaisesRegex(ValueError, 'separate receipt'):
            bench.command('kernel-full', args, Path('/tmp/compiler'))

    def test_supplemental_full_kernel_reads_only_its_fresh_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            receipt = root / 'tests/contracts/kernel-bpf-full-receipt.json'
            receipt.parent.mkdir(parents=True)
            default = receipt.with_name('kernel-full-receipt.json')
            default.write_text('unchanged accepted K6')
            data = {'requestedUnits': 1, 'counts': {'substituted': 1},
                    'completeComposition': True, 'imageSha256': 'new-image',
                    'boot': {'verdict': 'PASS', 'imageSha256': 'new-image'},
                    'driverSha256': 'new-compiler', 'status': 'PASS'}
            receipt.write_text(json.dumps(data))
            with patch.object(bench, 'ROOT', root):
                row = bench.read_result('kernel-full', b'old', 'new-compiler', receipt_path=receipt)
                self.assertEqual((row['status'], row['passed']), ('PASS', 1))
                with self.assertRaisesRegex(ValueError, 'unchanged'):
                    bench.read_result('kernel-full', receipt.read_bytes(), 'new-compiler', receipt_path=receipt)
            self.assertEqual(default.read_text(), 'unchanged accepted K6')

    def test_supplemental_selftest_receipt_must_match_the_explicit_selection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            kernel = root / 'tests/contracts/kernel-bpf-receipt.json'
            kernel.parent.mkdir(parents=True)
            kernel.write_bytes(b'supplemental composition')
            path = kernel.with_name('kernel-selftest-full-receipt.json')
            data = {'status': 'INCOMPLETE', 'kernelReceiptPath': str(kernel.relative_to(root)),
                    'kernelReceiptSha256': bench.sha256(kernel), 'kernelImageSha256': 'image',
                    'build': {'targets': ['example']},
                    'execution': {'subject': {'summary': {
                        'programs': 1, 'outcomes': {'FAIL': 1}}}}}
            path.write_text(json.dumps(data))
            with patch.object(bench, 'ROOT', root):
                with self.assertRaisesRegex(ValueError, 'unrecognized'):
                    bench.read_result('kernel-selftest-all', b'old', None)
                result = bench.read_result('kernel-selftest-all', b'old', None,
                                           kernel_receipt_path=kernel)
                self.assertEqual(result['status'], 'INCOMPLETE')
                with self.assertRaisesRegex(ValueError, 'selected kernel'):
                    bench.read_result('kernel-selftest-all', b'old', None,
                                      kernel_receipt_path=kernel.with_name('other.json'))
                kernel.write_bytes(b'changed composition')
                with self.assertRaisesRegex(ValueError, 'different complete kernel'):
                    bench.read_result('kernel-selftest-all', b'old', None,
                                      kernel_receipt_path=kernel)

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

    def test_mbedtls_uses_the_existing_whole_program_measurement(self):
        self.assertEqual(bench.CATALOG['mbedtls'][1], 'subject')
        args = bench.parser().parse_args(['mbedtls'])
        command, _ = bench.command('mbedtls', args, Path('/fixed/compiler'))
        self.assertTrue(any(part.endswith('/ladder/mbedtls.py') for part in command))
        self.assertEqual(command[-2:], ['--evocc', '/fixed/compiler'])

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

    def test_expanded_kunit_status_keeps_builtin_failures_and_module_results_separate(self):
        spec = importlib.util.spec_from_file_location("benchmark_status", PATH.parent / "render-status.py")
        renderer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(renderer)
        receipt = {"status": "FAIL", "config": {"targetCRecords": 6916},
                   "reference": {"outcomes": {"PASS": 1438, "FAIL": 7, "SKIP": 59},
                                 "modules": {"filesLoaded": 31,
                                             "outcomes": {"PASS": 765, "FAIL": 0, "SKIP": 0}}},
                   "subject": {"outcomes": {"PASS": 1425, "FAIL": 19, "SKIP": 60},
                               "modules": {"status": "NOT_RUN"}}}
        line = renderer.kunit_progress(receipt)
        self.assertIn("6,916", line)
        self.assertIn("subject 1425 PASS / 19 FAIL / 60 SKIP", line)
        self.assertIn("31 files loaded", line)
        self.assertIn("765 PASS", line)
        self.assertIn("modules NOT_RUN", line)
        self.assertGreater(len(line.splitlines()), 1)
        self.assertLessEqual(max(renderer.cells(part) for part in line.splitlines()), 110)

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

    def test_interrupted_adapter_is_not_left_running_in_the_receipt(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'tests/contracts').mkdir(parents=True)
            binary = root / 'compiler'
            binary.write_bytes(b'compiler')
            with patch.object(bench, 'ROOT', root), \
                 patch.object(bench, 'execute', side_effect=KeyboardInterrupt), \
                 patch.object(bench.subprocess, 'check_output', return_value='commit'), \
                 patch.object(bench.subprocess, 'run', return_value=SimpleNamespace(returncode=0)):
                self.assertEqual(bench.main(['mbedtls', '--evocc', str(binary)]), 130)
            result = json.loads((root / bench.RECEIPT).read_text())
            self.assertEqual(result['status'], 'INTERRUPTED')
            self.assertEqual(result['benchmarks']['mbedtls']['status'], 'INTERRUPTED')


class StatusAsciiTest(unittest.TestCase):
    """The diagram must retain each frozen population and its claim boundary."""

    APPLICATIONS = (
        "ctest", "torture", "csmith", "zlib", "zstd", "cjson", "mbedtls",
        "tinycc", "toybox", "musl", "polybench", "llvm-test-suite", "jq", "lua", "sqlite",
    )

    def setUp(self):
        spec = importlib.util.spec_from_file_location("status_ascii", PATH.parent / "render-status.py")
        self.renderer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.renderer)

    def render(self, receipts=None):
        self.assertTrue(hasattr(self.renderer, "render_ascii"), "provide the receipt-driven ASCII renderer")
        return self.renderer.render_ascii(receipts or {}, evoir="1.9.104", evomir="0.34")

    @staticmethod
    def benchmark_receipts(rows):
        return {"bench-receipt.json": {
            "run": "parent-live-run", "status": "RUNNING", "selected": ["kernel-selftest-all"],
            "benchmarks": rows,
        }}

    def test_output_is_ascii_even_when_receipts_are_missing(self):
        text = self.render()
        self.assertTrue(text.isascii(), "ASCII mode must not contain box-drawing, block, or CJK characters")
        self.assertIn("1.9.104", text)
        self.assertIn("0.34", text)

    def test_every_application_lane_is_present_and_missing_lanes_are_not_run(self):
        text = self.render()
        for name in self.APPLICATIONS:
            with self.subTest(lane=name):
                rows = [line for line in text.splitlines() if name in line.split()]
                self.assertTrue(rows, f"missing roadmap application lane: {name}")
                self.assertTrue(any("NOT_RUN" in line for line in rows), name)

    def test_torture_uses_validated_passes_not_raw_execution_passes(self):
        receipts = self.benchmark_receipts({"torture": {
            "rung": "E0a", "status": "PROFILE_CLEAN", "passed": 1504, "total": 1692,
            "kind": "programs", "outcomes": {"pass": 1505},
            "dispositions": {"PASS": 1504, "FP-deferred": 39, "not-applicable": 146, "oracle-unusable": 3},
            "run": "torture-child", "measurement": {"run": "torture-frozen-measurement"},
        }})
        text = self.render(receipts).replace(",", "")
        self.assertIn("1504/1692", text)
        self.assertNotIn("1505/1692", text)

    def test_application_rows_keep_frozen_measurement_identity(self):
        receipts = self.benchmark_receipts({"zlib": {
            "rung": "E1", "status": "PASS", "passed": 19, "total": 19, "kind": "units",
            "run": "zlib-child", "measurement": {"run": "zlib-frozen-measurement"},
        }})
        text = self.render(receipts)
        self.assertIn("zlib-frozen-measurement", text)
        rows = [line for line in text.splitlines() if "zlib" in line.split()]
        self.assertTrue(rows)
        self.assertFalse(any("parent-live-run" in line for line in rows))

    def test_locked_kernel_and_current_bpf_composition_remain_distinct(self):
        def kernel(total, image, run):
            return {"status": "PASS", "run": run, "requestedUnits": total,
                    "counts": {"substituted": total}, "completeComposition": True,
                    "relinkExitCode": 0, "imageSha256": image,
                    "boot": {"verdict": "PASS", "imageSha256": image},
                    "moduleLink": {"status": "PASS", "linkExitCode": 0, "modules": []}}

        text = self.render({
            "kernel-full-receipt.json": kernel(2799, "locked-image", "locked-run"),
            "kernel-bpf-full-receipt.json": kernel(6765, "bpf-image", "bpf-run"),
        }).replace(",", "")
        locked = [line for line in text.splitlines() if "2799/2799" in line]
        current = [line for line in text.splitlines() if "6765/6765" in line]
        self.assertTrue(locked, "retain the locked 2799-object milestone")
        self.assertTrue(current, "show the independently measured BPF composition")
        self.assertTrue(any("locked" in line.lower() for line in locked))
        self.assertTrue(any("bpf" in line.lower() for line in current))

    def test_kunit_builtin_failures_are_not_hidden_by_passing_modules(self):
        text = self.render({"kernel-expanded-kunit-receipt.json": {
            "status": "FAIL", "executionComplete": True, "config": {"targetCRecords": 6916},
            "subject": {"status": "FAIL", "outcomes": {"PASS": 1432, "FAIL": 12, "SKIP": 60},
                        "modules": {"status": "PASS", "filesLoaded": 31,
                                    "outcomes": {"PASS": 765, "FAIL": 0, "SKIP": 0}}},
        }})
        self.assertIn("1432 PASS / 12 FAIL / 60 SKIP", text)
        self.assertIn("765 PASS / 0 FAIL / 0 SKIP", text)
        self.assertIn("31 files loaded", text)

    def test_incremental_kernel_is_not_described_as_a_fresh_survey(self):
        receipt = {"requestedUnits": 6765, "counts": {"substituted": 6765},
                   "notASingleCompilerSurvey": True, "freshCompilations": 1,
                   "incrementalRepair": {"unchangedObjects": 6764}}
        text = self.render({"kernel-bpf-full-receipt.json": receipt})
        self.assertIn("incremental: 1 rebuilt / 6764 unchanged; not a fresh survey", text)
        self.assertNotIn("incremental:", self.render())

    def test_filtered_kunit_success_does_not_replace_full_campaign_failure(self):
        receipt = {
            "status": "FAIL", "executionComplete": True, "config": {"targetCRecords": 6916},
            "reference": {"tests": 1504, "outcomes": {"PASS": 1438, "FAIL": 7, "SKIP": 59}},
            "subject": {"tests": 1504, "outcomes": {"PASS": 1432, "FAIL": 12, "SKIP": 60}},
            "targetedFollowups": [{
                "suite": "fortify", "tests": 26, "outcomes": {"PASS": 26, "FAIL": 0, "SKIP": 0},
                "scope": "filtered; not a full KUnit rerun",
                "boot": "followup/boot.json", "parsedResults": "followup/kunit.json",
            }],
        }
        unchanged = json.loads(json.dumps(receipt))
        text = self.render({"kernel-expanded-kunit-receipt.json": receipt})
        self.assertIn("Expanded KUnit: FAIL", text)
        self.assertIn("reference 1438 PASS / 7 FAIL / 59 SKIP", text)
        self.assertIn("subject 1432 PASS / 12 FAIL / 60 SKIP", text)
        followups = [line for line in text.splitlines() if "follow-up fortify" in line]
        self.assertEqual(len(followups), 1)
        self.assertIn("26 tests", followups[0])
        self.assertIn("26 PASS / 0 FAIL / 0 SKIP", followups[0])
        self.assertIn("filtered; not a full KUnit rerun", followups[0])
        self.assertLess(text.index("subject 1432 PASS"), text.index("follow-up fortify"))
        self.assertLessEqual(self.renderer.cells(followups[0]), 110)
        self.assertEqual(receipt, unchanged)

    def test_kunit_status_without_targeted_followups_is_unchanged(self):
        receipt = {"reference": {"outcomes": {"PASS": 1438, "FAIL": 7, "SKIP": 59}},
                   "subject": {"outcomes": {"PASS": 1432, "FAIL": 12, "SKIP": 60}}}
        expected = ("KUnit:\n  reference 1438 PASS / 7 FAIL / 59 SKIP\n"
                    "  subject 1432 PASS / 12 FAIL / 60 SKIP")
        self.assertEqual(self.renderer.kunit_progress(receipt), expected)
        self.assertEqual(self.renderer.kunit_progress({**receipt, "targetedFollowups": []}), expected)

    def test_live_selftest_progress_preserves_every_outcome_and_not_run(self):
        outcomes = {"PASS": 2, "FAIL": 1, "SKIP": 1, "TIMEOUT": 1,
                    "INVALID_OUTPUT": 1, "INCOMPLETE": 1, "NOT_RUN": 3}
        summary = {"programs": 10, "collections": 4, "completedCollections": 2,
                   "status": "INCOMPLETE", "outcomes": outcomes}
        text = self.render({
            "kernel-bpf-full-receipt.json": {"imageSha256": "bpf-image"},
            "kernel-selftest-full-receipt.json": {
                "status": "RUNNING", "run": "selftest-live-run", "kernelImageSha256": "bpf-image",
                "kernelReceiptPath": "tests/contracts/kernel-bpf-full-receipt.json",
                "build": {"targets": ["a", "b", "c", "d"]},
                "execution": {"reference": {"summary": summary}, "subject": {"summary": summary}},
            },
        })
        self.assertIn("K7 all programs RUNNING", text)
        self.assertNotIn("K7 all programs PASS", text)
        for outcome in outcomes:
            self.assertIn(f"{outcomes[outcome]} {outcome}", text)
        self.assertNotIn("7/10 PASS", text, "completed attempts are not passing programs")
        self.assertIn("selftest-live-run", text)

    def test_missing_measurement_counts_cannot_manufacture_a_zero_of_zero_pass(self):
        receipts = self.benchmark_receipts({"zlib": {"rung": "E1", "status": "PASS"}})
        text = self.render(receipts)
        rows = [line for line in text.splitlines() if "zlib" in line.split()]
        self.assertTrue(rows)
        self.assertFalse(any("0/0" in line for line in rows))
        self.assertTrue(any("NOT_RUN" in line or "UNVERIFIED" in line for line in rows))

    def test_selected_selftest_pass_requires_nonempty_passing_subtests(self):
        for subtests in (None, {}, {"PASS": 0}, {"PASS": 5, "FAIL": 1}, {"PASS": 5, "TIMEOUT": 1}):
            with self.subTest(subtests=subtests):
                text = self.render({
                    "kernel-full-receipt.json": {"imageSha256": "image"},
                    "kernel-selftest-receipt.json": {
                        "status": "PASS", "kernelImageSha256": "image",
                        "subject": {"subtests": subtests},
                    },
                })
                self.assertIn("K7 selected UNVERIFIED", text)
                self.assertNotIn("K7 selected PASS", text)

    def test_ascii_cli_prints_without_writing_status_or_receipts(self):
        for existing in (False, True):
            with self.subTest(existing=existing), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                (root / "docs").mkdir()
                (root / "docs/evoir-spec.md").write_text("**Revision 1.9.104**")
                (root / "docs/evomir-spec.md").write_text("**Revision 0.34**")
                contracts = root / "tests/contracts"
                contracts.mkdir(parents=True)
                receipt = contracts / "bench-receipt.json"
                frozen = '{"run": "frozen-cli-run", "benchmarks": {}}\n'
                receipt.write_text(frozen)
                output = root / "docs/status.md"
                if existing:
                    output.write_text("unchanged status sentinel\n")
                stdout = io.StringIO()
                with patch.object(self.renderer, "ROOT", root), patch.object(self.renderer, "OUT", output), \
                        redirect_stdout(stdout):
                    self.assertEqual(self.renderer.main(["--ascii"]), 0)
                self.assertTrue(stdout.getvalue().isascii())
                self.assertIn("EvoCC", stdout.getvalue())
                self.assertEqual(receipt.read_text(), frozen)
                self.assertEqual(output.exists(), existing)
                if existing:
                    self.assertEqual(output.read_text(), "unchanged status sentinel\n")

    def test_check_cli_detects_drift_without_overwriting_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "docs").mkdir()
            (root / "docs/evoir-spec.md").write_text("**Revision 1.9.104**")
            (root / "docs/evomir-spec.md").write_text("**Revision 0.34**")
            (root / "tests/contracts").mkdir(parents=True)
            output = root / "docs/status.md"
            with patch.object(self.renderer, "ROOT", root), patch.object(self.renderer, "OUT", output), \
                    redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                self.assertEqual(self.renderer.main([]), 0)
                expected = output.read_text()
                self.assertEqual(self.renderer.main(["--check"]), 0)
                output.write_text(expected + "unexpected drift\n")
                self.assertEqual(self.renderer.main(["--check"]), 1)
                self.assertEqual(output.read_text(), expected + "unexpected drift\n")


if __name__ == "__main__":
    unittest.main()
