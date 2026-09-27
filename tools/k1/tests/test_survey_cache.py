"""A survey answer may only be reused when nothing it depends on has moved."""

from __future__ import annotations

import importlib.util
import hashlib
import io
import json
from contextlib import redirect_stdout
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

_spec = importlib.util.spec_from_file_location(
    "k1_substitute", Path(__file__).resolve().parents[1] / "substitute.py"
)
substitute = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(substitute)

ARGV = ["/bin/evocc", "--emit=asm", "-Ifoo", "/src/lib/bitrev.c"]
DRIVER = "a" * 64
SOURCE = "b" * 64


class CacheKeyTest(unittest.TestCase):
    def test_prepared_compilation_resumes_verified_batches_without_reemitting(self):
        self._prepared_checkpoint_case()

    def test_prepared_checkpoint_invalidates_changed_inputs_and_compiler(self):
        for change in ('header', 'new-header', 'compiler', 'command', 'prepare-log', 'image', 'malformed-cache'):
            with self.subTest(change=change):
                self._prepared_checkpoint_case(change)

    def test_prepared_checkpoint_rejects_missing_corrupt_and_external_assembly_objects(self):
        for change in ('object', 'missing-object', 'external-assembly', 'no-cache', 'timeout', 'symbol-drift'):
            with self.subTest(change=change):
                self._prepared_checkpoint_case(change)

    def test_prepared_checkpoint_is_discarded_if_inputs_change_during_compilation(self):
        self._prepared_checkpoint_case('during-compile')

    def test_failed_producer_graph_never_compiles_or_installs_subject_objects(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'evocc'
            binary.write_bytes(b'compiler')
            (root / 'modules.order').write_text('module.o\n')
            args = SimpleNamespace(src_root=root, evocc=binary, k1_root=root,
                                   no_cache=False, resume_prepared_run=None, relink_jobs=2)
            with patch.object(substitute, 'relink', return_value=2) as producer, \
                 patch.object(substitute, 'emit_assembly') as compile_one, \
                 patch.object(substitute, 'install') as install:
                result = substitute.recompile_after_prepare([], {}, args, root, root, root, root)
            self.assertEqual(result['prepareExitCode'], 2)
            self.assertEqual(producer.call_args.args[-1], ('prepare', 'vmlinux_o', 'modules.order'))
            compile_one.assert_not_called()
            install.assert_not_called()

    def _prepared_checkpoint_case(self, change=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, pristine, output, assembly, first, second = [
                root / name for name in ('src', 'pristine', 'out', 'asm', 'first', 'second')]
            for path in (source, pristine, output, assembly, first, second):
                path.mkdir()
            binary = root / 'evocc'
            binary.write_bytes(b'compiler')
            (output / 'offsets.h').write_text('#define OFFSET 4\n')
            records = {}
            # 65 objects exercise one complete low-disk batch followed by an
            # interrupted batch, without a kernel tree or a compiler process.
            for i in range(65):
                unit = f'u{i:02}.o'
                (source / f'u{i:02}.c').write_text(f'int f{i}(void) {{ return {i}; }}\n')
                path = pristine / f'.{unit}.cmd'
                path.write_text(f'savedcmd_{unit} := gcc-14 -c -o {unit} /work/src/u{i:02}.c\n')
                records[unit] = substitute.parse_record(path)
                (output / unit).write_bytes(b'first-pass')
            rows = [{'unit': unit, 'outcome': 'substituted'} for unit in records]
            args = SimpleNamespace(src_root=source, evocc=binary, k1_root=root,
                                   toolchain_root=None, jobs=1, relink_jobs=1, timeout=1,
                                   low_disk=True, no_cache=False, allow_symbol_drift=False,
                                   require_all=True, resume_prepared_run=None)
            emitted = []
            interrupt = True
            def emit(unit, record, *unused):
                self.assertTrue((output / 'generated-table.h').is_file(),
                                'all producer recipes must run before EvoCC consumers')
                emitted.append(unit)
                if interrupt and unit == 'u64.o':
                    raise RuntimeError('injected interruption')
                if interrupt and change == 'timeout' and unit == 'u00.o':
                    return {'unit': unit, 'outcome': 'timeout'}
                if not interrupt and change == 'during-compile' and unit == 'u64.o':
                    (output / 'offsets.h').write_text('#define OFFSET 12\n')
                path = substitute.assembly_path(assembly, unit)
                path.write_text('.include "external.inc"\n' if change == 'external-assembly'
                                and unit == 'u00.o' else '.text\n')
                return {'unit': unit, 'record': str(record.path), 'outcome': 'emitted',
                        'assemblySha256': substitute.sha256_file(path)}
            def assemble(batch, *unused, **keywords):
                for row in batch:
                    if row['outcome'] != 'emitted':
                        continue
                    path = assembly / row['unit']
                    path.write_bytes(b'prepared object ' + row['unit'].encode())
                    row.update(outcome='assembled', object=str(path),
                               objectSha256=substitute.sha256_file(path))
            def parity(batch, *unused):
                for row in batch:
                    row.update(missingSymbols=[], extraSymbols=['unexpected'] if interrupt
                               and change == 'symbol-drift' and row['unit'] == 'u00.o' else [])
            def prepare(src, out, log, *unused):
                log.write_text('prepare completed\n')
                targets = unused[-1]
                if 'vmlinux_o' in targets:
                    (output / 'generated-table.h').write_text('#define TABLE 37\n')
                # Real Kbuild rewrites identical GEN Makefile bytes: a resume
                # must validate the old prepared state before doing that again.
                (out / 'Makefile').write_text('generated\n')
                return 0
            with patch.object(substitute, 'emit_assembly', side_effect=emit), \
                 patch.object(substitute, 'assemble_batch', side_effect=assemble), \
                 patch.object(substitute, 'check_symbol_parity', side_effect=parity) as symbols, \
                 patch.object(substitute, 'relink', side_effect=prepare) as relink, \
                 patch.object(substitute, 'sealed_image', return_value='sha256:' + 'a' * 64) as image:
                with self.assertRaisesRegex(RuntimeError, 'injected interruption'):
                    substitute.recompile_after_prepare(rows, records, args, assembly,
                                                       pristine, output, first)
                self.assertEqual((output / 'u00.o').read_bytes(), b'first-pass')
                self.assertFalse(substitute.assembly_path(assembly, 'u01.o').exists())
                progress = substitute.load_cache(first / 'prepared-progress.json')
                self.assertEqual(progress['examinedUnits'], 64)
                self.assertEqual(progress['state'], 'CHECKPOINT')
                emitted.clear()
                interrupt = False
                args.resume_prepared_run = first
                if change == 'header':
                    (output / 'offsets.h').write_text('#define OFFSET 8\n')
                if change == 'new-header':
                    (output / 'new.h').write_text('new input\n')
                if change == 'compiler':
                    binary.write_bytes(b'new compiler')
                if change == 'command':
                    record = records['u00.o']
                    records['u00.o'] = substitute.Record(record.path, record.unit,
                                                        (*record.argv, '-fwrapv'), record.source_token)
                if change == 'prepare-log':
                    (first / 'prepare.log').write_text('incomplete\n')
                if change == 'image':
                    image.return_value = 'sha256:' + 'b' * 64
                if change == 'malformed-cache':
                    (root / 'prepared-cache.json').write_text('{partial')
                if change == 'object':
                    (assembly / 'u00.o').write_bytes(b'corrupt')
                if change == 'missing-object':
                    (assembly / 'u00.o').unlink()
                if change == 'no-cache':
                    args.no_cache = True
                relink.reset_mock()
                symbols.reset_mock()
                if change == 'during-compile':
                    with self.assertRaisesRegex(ValueError, 'prepared inputs changed'):
                        substitute.recompile_after_prepare(rows, records, args, assembly,
                                                           pristine, output, second)
                    self.assertEqual(substitute.load_cache(root / 'prepared-cache.json'), {})
                    self.assertEqual((output / 'u00.o').read_bytes(), b'first-pass')
                    return
                result = substitute.recompile_after_prepare(rows, records, args, assembly,
                                                            pristine, output, second)
                fresh_all = change in ('header', 'new-header', 'compiler', 'prepare-log', 'image',
                                       'no-cache', 'malformed-cache')
                expected = 65 if fresh_all else 2 if change else 1
                self.assertEqual(len(emitted), expected,
                                 'an interrupted prepared pass must not reemit verified unchanged objects')
                self.assertEqual(result['preparedCacheHits'], 65 - expected)
                self.assertEqual(result['failedUnits'], [])
                self.assertTrue(result['preparedInputsStable'])
                self.assertEqual(relink.call_count, 2 if fresh_all else 1)
                self.assertEqual(sum(len(call.args[0]) for call in symbols.call_args_list), 65,
                                 'cached objects must still undergo current symbol comparison')
                self.assertTrue(all(row['outcome'] == 'substituted' for row in rows))

    def test_a_reconfigured_tree_cannot_count_stale_module_metadata_as_target_c(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ('live', 'former'):
                (root / f'.{name}.mod.o.cmd').write_text(
                    f'savedcmd_{name}.mod.o := gcc-14 -c -o {name}.mod.o {name}.mod.c\n'
                    f'source_{name}.mod.o := {name}.mod.c\n')
            # former became built-in. Its old generated .mod.c/.o.cmd files
            # survive an in-place Kbuild but are not this configuration's input.
            (root / 'modules.order').write_text('live.o\n')
            with self.assertRaisesRegex(ValueError, 'stale module metadata.*former.mod.o'):
                substitute.target_c_records(root)
            self.assertTrue((root / '.former.mod.o.cmd').is_file(),
                            'inventory checking must not silently delete evidence')
            (root / '.former.mod.o.cmd').unlink()
            self.assertEqual([unit for unit, _ in substitute.target_c_records(root)],
                             ['live.mod.o'])

    def test_generated_module_inventory_requires_an_unambiguous_current_order(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / '.live.mod.o.cmd').write_text(
                'savedcmd_live.mod.o := gcc-14 -c -o live.mod.o live.mod.c\n'
                'source_live.mod.o := live.mod.c\n')
            with self.assertRaisesRegex(ValueError, 'modules.order'):
                substitute.target_c_records(root)
            for text in ('live.o\nlive.o\n', '../live.o\n', '/live.o\n', 'live.c\n'):
                with self.subTest(text=text):
                    (root / 'modules.order').write_text(text)
                    with self.assertRaisesRegex(ValueError, 'module inventory'):
                        substitute.target_c_records(root)

    def test_prepared_resume_requires_complete_matching_inputs_and_installed_objects(self):
        resume = getattr(substitute, 'resume_prepared_rows', None)
        self.assertTrue(callable(resume), 'resume must validate the completed first pass')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, pristine, output, run = [root / name for name in ('src', 'pristine', 'out', 'run')]
            for path in (source, pristine, output, run):
                path.mkdir()
            binary = root / 'evocc'
            binary.write_bytes(b'compiler')
            (source / 'a.c').write_text('int a;\n')
            (pristine / '.a.o.cmd').write_text(
                'savedcmd_a.o := gcc-14 -c -o a.o /work/src/a.c\nsource_a.o := /work/src/a.c\n')
            (output / 'a.o').write_bytes(b'compiled object')
            record = substitute.parse_record(pristine / '.a.o.cmd')
            args = SimpleNamespace(evocc=binary, src_root=source, k1_root=root, toolchain_root=None)
            argv, _ = substitute.evocc_argv(record, binary, source, pristine)
            key = substitute.cache_key('a.o', substitute.sha256_file(binary),
                                       substitute.sha256_file(source / 'a.c'), argv, 'inputs')
            row = {'unit': 'a.o', 'record': str(record.path), 'outcome': 'assembled',
                   'assemblySha256': 'assembly', 'objectSha256': substitute.sha256_file(output / 'a.o')}
            cache = {key: dict(row, outcome='emitted')}
            inventory = run / 'units.jsonl'
            original = json.dumps(row) + '\n'
            inventory.write_text(original)
            recovered, detail = resume(run, {'a.o': record}, args, cache, 'inputs')
            self.assertEqual(recovered[0]['outcome'], 'substituted')
            self.assertEqual(recovered[0]['installedSha256'], row['objectSha256'])
            self.assertEqual(detail['firstPassUnits'], 1)
            self.assertEqual(inventory.read_text(), original)
            for change in ('input', 'compiler', 'source', 'object', 'assembly', 'partial', 'duplicate', 'timeout'):
                with self.subTest(change=change):
                    binary.write_bytes(b'compiler')
                    (source / 'a.c').write_text('int a;\n')
                    (output / 'a.o').write_bytes(b'compiled object')
                    inventory.write_text(original)
                    altered = dict(cache)
                    inputs = 'inputs'
                    if change == 'input':
                        inputs = 'other'
                    if change == 'compiler':
                        binary.write_bytes(b'changed compiler')
                    if change == 'source':
                        (source / 'a.c').write_text('int b;\n')
                    if change == 'object':
                        (output / 'a.o').write_bytes(b'reference object')
                    if change == 'assembly':
                        altered[key] = dict(cache[key], assemblySha256='other')
                    if change == 'partial':
                        inventory.write_text('')
                    if change == 'duplicate':
                        inventory.write_text(original * 2)
                    if change == 'timeout':
                        inventory.write_text(json.dumps(dict(row, outcome='timeout')) + '\n')
                    with self.assertRaises(ValueError):
                        resume(run, {'a.o': record}, args, altered, inputs)

    def test_full_composition_checks_module_links_before_accepting_its_object_inventory(self):
        for status in ('PASS', 'INCOMPLETE'):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                work, source = root / 'work', root / 'src'
                pristine, output = work / 'pristine', work / 'out'
                for path in (pristine, output, source):
                    path.mkdir(parents=True)
                for path in (pristine, output):
                    (path / '.config').write_text('CONFIG_MODULES=y\n')
                    (path / 'modules.order').write_text('a.o\n')
                    (path / '.a.o.cmd').write_text(
                        'savedcmd_a.o := gcc-14 -c -o a.o a.c\nsource_a.o := a.c\n')
                (output / 'a.o').write_bytes(b'EvoCC object')
                digest = substitute.sha256_file(output / 'a.o')
                binary = root / 'evocc'
                binary.write_bytes(b'compiler')
                receipt = root / 'result.json'
                argv = ['substitute', '--all', '--require-all', '--no-cache', '--jobs', '1',
                        '--k1-root', str(work), '--src-root', str(source),
                        '--evocc', str(binary), '--evidence-root', str(root / 'results'),
                        '--run-id', 'full', '--receipt', str(receipt)]
                def install(rows, *args):
                    rows[0].update(outcome='substituted', installedSha256=digest)
                with patch.object(substitute.sys, 'argv', argv), \
                     patch.object(substitute, 'REPO_ROOT', root), \
                     patch.object(substitute, 'emit_assembly', return_value={
                         'unit': 'a.o', 'outcome': 'assembled'}), \
                     patch.object(substitute, 'assemble_batch'), \
                     patch.object(substitute, 'check_symbol_parity'), \
                     patch.object(substitute, 'sealed_image', return_value='sealed'), \
                     patch.object(substitute, 'install', side_effect=install), \
                     patch.object(substitute, 'recompile_after_prepare', return_value={
                         'prepareExitCode': 0, 'verifyPrepareExitCode': 0,
                         'preparedInputsStable': True, 'failedUnits': []}), \
                     patch.object(substitute, 'recompile_export_table', return_value={
                         'required': False, 'ready': True}), \
                     patch.object(substitute, 'relink', return_value=0), \
                     patch.object(substitute, 'relink_modules', return_value={
                         'status': status}) as modules, redirect_stdout(io.StringIO()):
                    substitute.main()
                modules.assert_called_once()
                result = json.loads(receipt.read_text())
                self.assertEqual(result['moduleLink']['status'], status)
                self.assertEqual(result['completeComposition'], status == 'PASS')

    def test_supplemental_failure_does_not_overwrite_the_accepted_kernel_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            work, source = root / 'work', root / 'src'
            pristine = work / 'pristine'
            pristine.mkdir(parents=True)
            (work / 'out').mkdir()
            source.mkdir()
            (pristine / '.config').write_text('CONFIG_TEST=y\n')
            (pristine / '.x.o.cmd').write_text(
                'savedcmd_x.o := gcc-14 -c -o x.o x.c\nsource_x.o := x.c\n')
            binary = root / 'compiler'
            binary.write_bytes(b'compiler')
            default = root / 'tests/contracts/kernel-full-receipt.json'
            default.parent.mkdir(parents=True)
            default.write_bytes(b'accepted K6 receipt')
            receipt = default.with_name('kernel-bpf-full-receipt.json')
            argv = ['substitute', '--all', '--require-all', '--no-cache', '--jobs', '1',
                    '--k1-root', str(work), '--src-root', str(source),
                    '--evocc', str(binary), '--evidence-root', str(root / 'results'),
                    '--run-id', 'supplemental', '--receipt', str(receipt)]
            with patch.object(substitute.sys, 'argv', argv), \
                 patch.object(substitute, 'REPO_ROOT', root), \
                 patch.object(substitute, 'emit_assembly', return_value={
                     'unit': 'x.o', 'outcome': 'refused', 'diagnosticCodes': ['E3101']}), \
                 patch.object(substitute, 'assemble_batch'), \
                 patch.object(substitute, 'check_symbol_parity'), \
                 patch.object(substitute, 'sealed_image', return_value='sealed'), \
                 patch.object(substitute, 'relink') as relink, \
                 redirect_stdout(io.StringIO()):
                self.assertEqual(substitute.main(), 1)
            relink.assert_not_called()
            self.assertEqual(default.read_bytes(), b'accepted K6 receipt')
            result = json.loads(receipt.read_text())
            self.assertEqual(result['counts'], {'refused': 1})
            self.assertFalse(result['installed'])
            self.assertEqual(result['status'], 'INCOMPLETE')
            self.assertEqual(result['boot']['verdict'], 'NOT_RUN')

    def test_exported_toolchain_headers_participate_in_input_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, output, toolchain = [root / part for part in ('src', 'out', 'toolchain')]
            for path in (source, output, toolchain):
                path.mkdir()
            header = toolchain / 'arm_neon.h'
            header.write_bytes(b'original header')
            snapshot = substitute.input_snapshot(source, output, toolchain_root=toolchain)
            header.write_bytes(b'different header')
            self.assertNotEqual(snapshot, substitute.input_snapshot(
                source, output, toolchain_root=toolchain))

    def test_low_disk_survey_verifies_each_batch_before_emitting_the_next(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pristine = root / 'pristine'
            pristine.mkdir()
            (pristine / '.config').write_text('CONFIG_TEST=y\n')
            (root / 'out').mkdir()
            source = root / 'src'
            source.mkdir()
            binary = root / 'compiler'
            binary.write_bytes(b'compiler')
            toolchain = root / 'toolchain'
            toolchain.mkdir()
            (toolchain / 'header.h').write_bytes(b'original header')
            for i in range(129):
                (pristine / f'.x{i}.o.cmd').write_text(
                    f'savedcmd_x{i}.o := gcc-14 -isystem /usr/include -c -o x{i}.o x{i}.c\n'
                    f'source_x{i}.o := x{i}.c\n')
            batches = []
            def emit(unit, record, compiler, src, inputs, assembly, timeout, toolchain_root):
                self.assertEqual(toolchain_root, toolchain)
                path = substitute.assembly_path(assembly, unit)
                path.write_bytes(b'assembly')
                # Completed batches must not accumulate while later ones run.
                self.assertLessEqual(len(list(assembly.glob('*.s'))), 64)
                return {'unit': unit, 'outcome': 'emitted', 'assembly': str(path),
                        'assemblySha256': substitute.sha256_file(path)}
            def assemble(rows, records, assembly, inputs, **kwargs):
                if rows:
                    batches.append(len(rows))
                for row in rows:
                    obj = assembly / row['unit']
                    obj.write_bytes(b'object')
                    row.update(outcome='assembled', objectSha256=substitute.sha256_file(obj))
            argv = ['substitute', '--all', '--no-install', '--low-disk', '--no-cache',
                    '--jobs', '1', '--k1-root', str(root), '--src-root', str(source),
                    '--evocc', str(binary), '--evidence-root', str(root / 'results'),
                    '--run-id', 'bounded', '--toolchain-root', str(toolchain)]
            with patch.object(substitute.sys, 'argv', argv), \
                 patch.object(substitute, 'emit_assembly', side_effect=emit), \
                 patch.object(substitute, 'assemble_batch', side_effect=assemble), \
                 patch.object(substitute, 'check_symbol_parity'), \
                 patch.object(substitute, 'sealed_image', return_value='sealed'), \
                 redirect_stdout(io.StringIO()):
                self.assertEqual(substitute.main(), 0)
            self.assertEqual(batches, [64, 64, 1])
            summary = json.loads((root / 'results/bounded/substitution.json').read_text())
            self.assertEqual(summary['counts'], {'assembled': 129})
            self.assertEqual(summary['toolchainRoot'], str(toolchain))
            rows = [json.loads(line) for line in
                    (root / 'results/bounded/units.jsonl').read_text().splitlines()]
            self.assertEqual(len(rows), 129)
            self.assertTrue(all(row['assemblyRetained'] is False for row in rows))
            self.assertEqual(len(list((root / 'evocc-out').glob('*.o'))), 129)

    def test_successful_assembly_can_be_released_only_after_object_verification(self) -> None:
        release = getattr(substitute, 'release_successful_assembly', None)
        self.assertTrue(callable(release), 'bounded surveys must release rebuildable assembly')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assembly, obj = root / 'a.s', root / 'a.o'
            assembly.write_bytes(b'assembly')
            obj.write_bytes(b'object')
            row = {'unit': 'a.o', 'outcome': 'assembled', 'assembly': str(assembly),
                   'assemblySha256': substitute.sha256_file(assembly),
                   'objectSha256': substitute.sha256_file(obj)}
            for outcome in ('emitted', 'assembler-refused', 'refused'):
                release([dict(row, outcome=outcome)], root)
                self.assertTrue(assembly.exists(), outcome)
            for field in ('missingSymbols', 'extraSymbols'):
                release([dict(row, **{field: ['f']})], root)
                self.assertTrue(assembly.exists(), field)
            release([row], root)
            self.assertFalse(assembly.exists())
            self.assertEqual(obj.read_bytes(), b'object')
            self.assertFalse(row['assemblyRetained'])

    def test_changed_assembly_or_object_is_never_released_as_verified(self) -> None:
        release = getattr(substitute, 'release_successful_assembly', None)
        self.assertTrue(callable(release))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assembly, obj = root / 'a.s', root / 'a.o'
            assembly.write_bytes(b'assembly')
            obj.write_bytes(b'object')
            row = {'unit': 'a.o', 'outcome': 'assembled', 'assembly': str(assembly),
                   'assemblySha256': substitute.sha256_file(assembly),
                   'objectSha256': substitute.sha256_file(obj)}
            obj.write_bytes(b'changed')
            with self.assertRaises(ValueError):
                release([row], root)
            self.assertTrue(assembly.exists())
            obj.write_bytes(b'object')
            assembly.write_bytes(b'changed')
            with self.assertRaises(ValueError):
                release([row], root)
            self.assertTrue(assembly.exists())

    def test_module_linking_requires_audited_c_leaves_and_original_assembly(self) -> None:
        planner = getattr(substitute, 'module_link_plan', None)
        self.assertTrue(callable(planner), 'module links must not consume stale reference C objects')
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            pristine, output = base / 'pristine', base / 'out'
            for root in (pristine, output):
                root.mkdir()
                (root / 'modules.order').write_text('combo.o\n')
                (root / '.combo.ko.cmd').write_text(
                    'savedcmd_combo.ko := ld -r -o combo.ko combo.o combo.mod.o\n')
                (root / '.combo.o.cmd').write_text(
                    'savedcmd_combo.o := ld -r -o combo.o @combo.mod\n')
                (root / 'combo.mod').write_text('a.o\nstart.o\n')
                for unit, source in [('a.o', 'a.c'), ('start.o', 'start.S'),
                                     ('combo.mod.o', 'combo.mod.c')]:
                    (root / ('.' + unit + '.cmd')).write_text(
                        f'savedcmd_{unit} := gcc-14 -c -o {unit} {source}\n'
                        f'source_{unit} := {source}\n')
                    (root / unit).write_bytes(b'code-' + unit.encode())
            rows = [{'unit': unit, 'outcome': 'substituted',
                     'installedSha256': substitute.sha256_file(output / unit)}
                    for unit in ('a.o', 'combo.mod.o')]
            result = planner(pristine, output, rows)
            self.assertEqual(result['modules'], ['combo.ko'])
            self.assertEqual([r['unit'] for r in result['links']], ['combo.o', 'combo.ko'])
            self.assertEqual(result['blocked'], {})
            self.assertEqual(set(result['cInputs']), {'a.o', 'combo.mod.o'})
            self.assertEqual(set(result['assemblyInputs']), {'start.o'})
            for bad in [rows[:1], [dict(rows[0], outcome='refused'), rows[1]]]:
                result = planner(pristine, output, bad)
                self.assertEqual(result['modules'], [])
                self.assertIn('combo.ko', result['blocked'])
            (output / 'a.o').write_bytes(b'reference overwrite')
            self.assertEqual(planner(pristine, output, rows)['modules'], [])
            (output / 'a.o').write_bytes(b'code-a.o')
            (output / 'start.o').write_bytes(b'foreign assembly')
            self.assertEqual(planner(pristine, output, rows)['modules'], [])
            (output / 'start.o').write_bytes(b'code-start.o')
            (output / 'combo.mod').write_text('different.o\n')
            with self.assertRaises(substitute.RecordError):
                planner(pristine, output, rows)

    def test_full_kernel_coverage_requires_each_original_object_and_fresh_hash(self) -> None:
        check = getattr(substitute, "complete_coverage", None)
        self.assertTrue(callable(check), "full link must not silently use reference C objects")
        row = {"unit": "a.o", "outcome": "substituted", "installedSha256": "abc",
               "objectSha256AfterRelink": "abc"}
        self.assertTrue(check([row], ["a.o"], "substituted"))
        self.assertFalse(check([row], ["a.o", "b.o"], "substituted"))
        self.assertFalse(check([row, row], ["a.o"], "substituted"))
        self.assertFalse(check([{**row, "objectSha256AfterRelink": "reference"}], ["a.o"], "substituted"))
        self.assertFalse(check([], [], "substituted"))

    def test_module_finalization_keeps_btf_and_rechecks_compilation_leaves(self) -> None:
        finalize = getattr(substitute, 'relink_modules', None)
        self.assertTrue(callable(finalize), 'audited module leaves still need real .ko links and BTF')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output, capture = root / 'out', root / 'capture'
            output.mkdir()
            capture.mkdir()
            for name, data in [('a.o', b'EvoCC C'), ('vmlinux', b'composed ELF'),
                               ('.config', b'CONFIG_DEBUG_INFO_BTF_MODULES=y\n')]:
                (output / name).write_bytes(data)
            plan = {'modules': ['a.ko'], 'blocked': {},
                    'links': [{'unit': 'a.ko', 'argv': ['ld', '-r', '-o', 'a.ko', 'a.o'],
                               'inputs': ['a.o']}],
                    'cInputs': {'a.o': substitute.sha256_file(output / 'a.o')},
                    'assemblyInputs': {}, 'linkInputs': {}}

            def execute(command, **kwargs):
                self.assertIn('/work/src:ro', ' '.join(command))
                worker = command[command.index('-c', command.index('python3')) + 1]
                self.assertIn('scripts/Makefile.btf', worker)
                self.assertIn('resolve_btfids', worker)
                self.assertIn('--btf_base', worker)
                (output / 'a.ko').write_bytes(b'linked module with BTF')
                return SimpleNamespace(returncode=0, stdout='original links completed', stderr='')

            with patch.object(substitute, 'module_link_plan', return_value=plan), \
                 patch.object(substitute, 'verify_image'), \
                 patch.object(substitute.subprocess, 'run', side_effect=execute):
                result = finalize(root / 'src', root / 'pristine', output, [], capture, 2)
                self.assertEqual(result['status'], 'PASS')
                self.assertEqual(result['modules'][0]['sha256'],
                                 substitute.sha256_file(output / 'a.ko'))
                (output / 'a.o').write_bytes(b'GCC overwrite')
                with self.assertRaises(ValueError):
                    finalize(root / 'src', root / 'pristine', output, [], capture, 2)
            with patch.object(substitute, 'module_link_plan',
                              return_value={**plan, 'blocked': {'a.ko': ['unaudited C']}}), \
                 patch.object(substitute.subprocess, 'run') as run:
                with self.assertRaises(ValueError):
                    finalize(root / 'src', root / 'pristine', output, [], capture, 2)
                run.assert_not_called()

    def test_build_preparation_can_run_without_linking_the_kernel(self) -> None:
        import inspect
        self.assertIn("targets", inspect.signature(substitute.relink).parameters,
                      "prepare must precede final object installation and linking")

    def test_export_table_is_compiled_after_modpost_from_actual_generated_source(self) -> None:
        recompile = getattr(substitute, "recompile_export_table", None)
        self.assertTrue(callable(recompile), "modpost creates another target C input after prepare")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "out"
            output.mkdir()
            source = output / ".vmlinux.export.c"
            source.write_text("old exports\n")
            binary = root / "evocc"
            binary.write_bytes(b"measured compiler")
            args = SimpleNamespace(src_root=root / "src", evocc=binary, timeout=180,
                                   relink_jobs=2, no_cache=False, allow_symbol_drift=True)
            unit = ".vmlinux.export.o"
            records = {unit: substitute.Record(root / "record", unit, ("gcc-14",), source.name)}
            rows = [{"unit": unit, "outcome": "substituted", "installedSha256": "old"}]
            events = []
            def produce(*args):
                self.assertEqual(args[-1], ("modpost",))
                events.append("produce")
                source.write_text("actual complete-composition exports\n")
                return 0
            def emit(*args):
                self.assertEqual(args[4], output)
                self.assertEqual(source.read_text(), "actual complete-composition exports\n")
                events.append("compile")
                return {"unit": unit, "outcome": "emitted"}
            def assemble(rows, *args, **kwargs):
                events.append("assemble")
                rows[0].update(outcome="assembled", objectSha256="new")
            def install(rows, *args):
                events.append("install")
                self.assertTrue(rows[0]["recompiledAfterPrepare"])
                rows[0].update(outcome="substituted", installedSha256="new")
            with patch.object(substitute, "relink", side_effect=produce), \
                    patch.object(substitute, "emit_assembly", side_effect=emit), \
                    patch.object(substitute, "assemble_batch", side_effect=assemble), \
                    patch.object(substitute, "install", side_effect=install):
                result = recompile(rows, records, args, root / "asm", output, root)
            self.assertTrue(result["ready"])
            self.assertEqual(result["sourceSha256"], substitute.sha256_file(source))
            self.assertEqual(events, ["produce", "compile", "assemble", "install"])
            self.assertEqual(rows[0]["installedSha256"], "new")
            def changing_emit(*args):
                source.write_text("changed while compilation was running\n")
                return {"unit": unit, "outcome": "emitted"}
            with patch.object(substitute, "relink", side_effect=produce), \
                    patch.object(substitute, "emit_assembly", side_effect=changing_emit), \
                    patch.object(substitute, "install") as install_mock:
                result = recompile(rows, records, args, root / "asm", output, root)
            self.assertFalse(result["ready"])
            self.assertTrue(result["inputsChanged"])
            install_mock.assert_not_called()
            with patch.object(substitute, "relink", return_value=2), \
                    patch.object(substitute, "emit_assembly") as emit_mock:
                result = recompile(rows, records, args, root / "asm", output, root)
            self.assertFalse(result["ready"])
            emit_mock.assert_not_called()

    def test_module_descriptors_use_post_modpost_sources_not_reference_templates(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / 'out'
            output.mkdir()
            binary = root / 'evocc'
            binary.write_bytes(b'measured compiler')
            units = ['a.mod.o', 'b.mod.o']
            for unit in units:
                (output / unit.replace('.o', '.c')).write_text('reference descriptor\n')
            records = {unit: substitute.Record(root / 'record', unit, ('gcc-14',),
                        unit.replace('.o', '.c')) for unit in units}
            rows = [{'unit': unit, 'outcome': 'substituted', 'installedSha256': 'old'}
                    for unit in units]
            args = SimpleNamespace(src_root=root / 'src', evocc=binary, timeout=180,
                                   jobs=2, relink_jobs=2, no_cache=False,
                                   allow_symbol_drift=True, low_disk=True, k1_root=root)
            compiled = []
            prelinked = []
            def prelink(*args, **kwargs):
                self.assertFalse(kwargs['finalize'])
                prelinked.append(True)
                return {'status': 'PASS'}
            def produce(*args):
                self.assertEqual(prelinked, [True],
                                 'modpost must see current module aggregate symbols, not reference links')
                self.assertEqual(args[-1], ('modpost',))
                for unit in units:
                    (output / records[unit].source_token).write_text('actual ' + unit)
                return 0
            def emit(unit, record, *args):
                self.assertEqual(args[2], output)
                self.assertEqual((output / record.source_token).read_text(), 'actual ' + unit)
                compiled.append(unit)
                return {'unit': unit, 'outcome': 'emitted'}
            def install(batch, *args):
                for row in batch:
                    row.update(outcome='substituted', installedSha256='new')
            with patch.object(substitute, 'relink', side_effect=produce), \
                 patch.object(substitute, 'relink_modules', side_effect=prelink), \
                 patch.object(substitute, 'emit_assembly', side_effect=emit), \
                 patch.object(substitute, 'assemble_batch'), \
                 patch.object(substitute, 'release_successful_assembly'), \
                 patch.object(substitute, 'install', side_effect=install):
                result = substitute.recompile_export_table(
                    rows, records, args, root / 'asm', output, root)
            self.assertEqual(sorted(compiled), units,
                             'each generated module descriptor must consume actual modpost output')
            self.assertTrue(result['ready'])
            self.assertEqual(set(result['sources']), set(units))
            self.assertTrue(all(row['recompiledAfterModpost'] for row in rows))
            for unit in units:
                self.assertEqual(result['sources'][unit]['sha256'],
                                 substitute.sha256_file(output / records[unit].source_token))

    def test_relinks_keep_the_reference_build_version_and_prepare_both_headers(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            out = root / "out"
            out.mkdir()
            header = root / "pristine/include/generated/utsversion.h"
            header.parent.mkdir(parents=True)
            header.write_text('#define UTS_VERSION "#7 SMP PREEMPT fixed date"\n')
            completed = SimpleNamespace(returncode=0, stdout="", stderr="")
            with patch.object(substitute.subprocess, "run", return_value=completed) as run:
                self.assertEqual(substitute.relink(root / "src", out, root / "prepare.log", 2, ("prepare",)), 0)
            script = run.call_args.args[0][-1]
            self.assertIn("KBUILD_BUILD_VERSION='7'", script)
            self.assertIn("obj=init include/generated/utsversion.h init/utsversion-tmp.h", script)
            with patch.object(substitute.subprocess, "run", return_value=completed) as run:
                self.assertEqual(substitute.relink(root / "src", out, root / "link.log", 2), 0)
            self.assertIn("KBUILD_BUILD_VERSION='7'", run.call_args.args[0][-1])
            command = run.call_args.args[0]
            self.assertEqual(command[command.index('-c') + 1], '2')
            self.assertEqual(command[command.index('-m') + 1], '8G')
            (out / 'modules.order').write_text('a.o\n')
            with patch.object(substitute.subprocess, 'run', return_value=completed) as run:
                self.assertEqual(substitute.relink(root / 'src', out, root / 'modpost.log', 2, ('modpost',)), 0)
            self.assertIn('KBUILD_MODULES=1', run.call_args.args[0][-1],
                          'a vmlinux-only modpost leaves module symbol versions stale')
            with patch.object(substitute.subprocess, 'run', return_value=completed) as run:
                self.assertEqual(substitute.relink(
                    root / 'src', out, root / 'producers.log', 2,
                    ('prepare', 'vmlinux_o', 'modules.order')), 0)
            script = run.call_args.args[0][-1]
            self.assertIn('KBUILD_MODULES=1 prepare vmlinux_o modules.order', script)
            self.assertIn('obj=init include/generated/utsversion.h init/utsversion-tmp.h', script)
            self.assertNotIn('modpost', script, 'generated module C follows final subject inputs')

    def test_prepare_fixed_point_ignores_only_metadata_not_changed_content(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, output = root / "src", root / "out"
            source.mkdir()
            output.mkdir()
            header = output / "offsets.h"
            header.write_text("#define OFFSET 4\n")
            old = substitute.input_snapshot(source, output)
            content = substitute.input_snapshot(source, output, content_only=True)
            substitute.os.utime(header, (1, 1))
            self.assertNotEqual(old, substitute.input_snapshot(source, output))
            self.assertEqual(content, substitute.input_snapshot(source, output, content_only=True))
            header.write_text("#define OFFSET 8\n")
            self.assertNotEqual(content, substitute.input_snapshot(source, output, content_only=True))

    def test_recompiled_identical_object_is_newer_than_generated_headers(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assembly, output = root / "asm", root / "out"
            assembly.mkdir()
            output.mkdir()
            (assembly / "x.o").write_bytes(b"object")
            (output / "x.o").write_bytes(b"object")
            substitute.os.utime(output / "x.o", (1, 1))
            row = {"unit": "x.o", "outcome": "assembled",
                   "objectSha256": substitute.sha256_file(assembly / "x.o"),
                   "recompiledAfterPrepare": True}
            substitute.install([row], assembly, output, False)
            self.assertGreater((output / "x.o").stat().st_mtime_ns, 1_000_000_000)

    def key(self, **overrides) -> str:
        fields = {
            "unit": "lib/bitrev.o",
            "evocc_sha256": DRIVER,
            "source_sha256": SOURCE,
            "argv": ARGV,
        }
        fields.update(overrides)
        return substitute.cache_key(**fields)

    def test_the_same_inputs_give_the_same_key(self) -> None:
        self.assertEqual(self.key(), self.key())

    def test_a_new_driver_binary_is_a_new_key(self) -> None:
        self.assertNotEqual(self.key(), self.key(evocc_sha256="c" * 64))

    def test_a_changed_source_is_a_new_key(self) -> None:
        self.assertNotEqual(self.key(), self.key(source_sha256="c" * 64))

    def test_a_changed_compile_command_is_a_new_key(self) -> None:
        self.assertNotEqual(self.key(), self.key(argv=ARGV + ["-DEXTRA"]))

    def test_a_different_unit_is_a_new_key(self) -> None:
        self.assertNotEqual(self.key(), self.key(unit="lib/ctype.o"))

    def test_two_units_never_share_an_entry_through_their_argv(self) -> None:
        # The unit is part of the key in its own right, not only through the
        # command, so a record whose argv happens to match cannot answer for
        # the wrong object path.
        self.assertNotEqual(self.key(unit="lib/a.o"), self.key(unit="lib/b.o"))


class ReusableTest(unittest.TestCase):
    def setUp(self) -> None:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.asm_root = Path(directory.name)

    def emitted_entry(self) -> dict:
        return {"unit": "lib/bitrev.o", "outcome": "emitted", "assemblyBytes": 6,
                "assemblySha256": hashlib.sha256(b".text\n").hexdigest()}

    def write_assembly(self, unit: str) -> None:
        path = substitute.assembly_path(self.asm_root, unit)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(".text\n", encoding="utf-8")

    def test_a_refusal_is_reused_as_it_stands(self) -> None:
        entry = {"unit": "lib/x.o", "outcome": "refused", "diagnosticCodes": ["E3101"]}
        row = substitute.reusable(entry, "lib/x.o", self.asm_root)
        self.assertEqual(row["outcome"], "refused")
        self.assertEqual(row["diagnosticCodes"], ["E3101"])
        self.assertTrue(row["fromCache"])

    def test_an_emitted_answer_needs_its_assembly_on_disk(self) -> None:
        self.assertIsNone(
            substitute.reusable(self.emitted_entry(), "lib/bitrev.o", self.asm_root)
        )
        self.write_assembly("lib/bitrev.o")
        self.assertIsNotNone(
            substitute.reusable(self.emitted_entry(), "lib/bitrev.o", self.asm_root)
        )

    def test_a_timeout_is_never_reused(self) -> None:
        entry = {"unit": "lib/x.o", "outcome": "timeout", "seconds": 180}
        self.assertIsNone(substitute.reusable(entry, "lib/x.o", self.asm_root))

    def test_overwritten_or_unbound_assembly_is_not_a_cache_hit(self) -> None:
        self.write_assembly("lib/bitrev.o")
        entry = self.emitted_entry()
        self.assertIsNotNone(substitute.reusable(entry, "lib/bitrev.o", self.asm_root))
        substitute.assembly_path(self.asm_root, "lib/bitrev.o").write_text(".data\n")
        self.assertIsNone(substitute.reusable(entry, "lib/bitrev.o", self.asm_root))
        self.write_assembly("lib/bitrev.o")
        entry.pop("assemblySha256")
        self.assertIsNone(substitute.reusable(entry, "lib/bitrev.o", self.asm_root))

    def test_an_outcome_from_a_later_step_is_never_reused(self) -> None:
        for outcome in ("assembled", "substituted", "symbol-drift", "assembler-refused",
                        "rebuilt-by-kbuild", "invented"):
            self.assertIsNone(
                substitute.reusable({"outcome": outcome}, "lib/x.o", self.asm_root)
            )

    def test_a_missing_entry_is_a_miss(self) -> None:
        self.assertIsNone(substitute.reusable(None, "lib/x.o", self.asm_root))

    def test_the_reused_row_is_a_copy(self) -> None:
        entry = {"unit": "lib/x.o", "outcome": "refused", "diagnosticCodes": ["E3101"]}
        row = substitute.reusable(entry, "lib/x.o", self.asm_root)
        row["outcome"] = "substituted"
        self.assertEqual(entry["outcome"], "refused")
        self.assertNotIn("fromCache", entry)


class CacheFileTest(unittest.TestCase):
    def setUp(self) -> None:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.path = Path(directory.name) / "nested" / "survey-cache.json"

    def test_what_is_saved_is_what_is_loaded(self) -> None:
        entries = {"k": {"unit": "lib/x.o", "outcome": "refused"}}
        substitute.save_cache(self.path, entries)
        self.assertEqual(substitute.load_cache(self.path), entries)

    def test_an_absent_cache_is_empty_rather_than_an_error(self) -> None:
        self.assertEqual(substitute.load_cache(self.path), {})

    def test_a_damaged_cache_is_ignored(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text("{not json", encoding="utf-8")
        self.assertEqual(substitute.load_cache(self.path), {})

    def test_an_interrupted_write_leaves_the_previous_cache_intact(self) -> None:
        first = {"k": {"unit": "lib/x.o", "outcome": "refused"}}
        substitute.save_cache(self.path, first)
        # A checkpoint that dies mid-write leaves its scratch file behind; the
        # cache itself is only ever replaced whole.
        self.path.with_name(self.path.name + ".new").write_text("{half", encoding="utf-8")
        self.assertEqual(substitute.load_cache(self.path), first)

    def test_a_cache_of_another_version_is_ignored(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text('{"version": 0, "entries": {"k": {}}}', encoding="utf-8")
        self.assertEqual(substitute.load_cache(self.path), {})


class AssemblyReuseTest(unittest.TestCase):
    def test_frozen_source_date_survives_an_overnight_survey(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source, build = root / 'src', root / 'build'
            source.mkdir()
            build.mkdir()
            days = [substitute.dt.datetime(2026, 9, day, tzinfo=substitute.dt.timezone.utc)
                    for day in (20, 21)]
            for epoch in ('0', '+86400', '-1'):
                with self.subTest(epoch=epoch), \
                     patch.dict(substitute.os.environ, {'SOURCE_DATE_EPOCH': epoch}), \
                     patch.object(substitute.dt, 'datetime') as clock:
                    clock.now.return_value = days[0]
                    before = substitute.input_snapshot(source, build)
                    clock.now.return_value = days[1]
                    self.assertEqual(before, substitute.input_snapshot(source, build),
                                     'wall-clock midnight cannot change a frozen __DATE__')

    def test_unfrozen_or_invalid_source_date_still_invalidates_at_midnight(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source, build = root / 'src', root / 'build'
            source.mkdir()
            build.mkdir()
            days = [substitute.dt.datetime(2026, 9, day, tzinfo=substitute.dt.timezone.utc)
                    for day in (20, 21)]
            # Match the driver's Rust i64 parser, not Python's permissive int().
            for epoch in (None, '', 'bad', ' 0', '0_0', '9223372036854775808'):
                environment = {} if epoch is None else {'SOURCE_DATE_EPOCH': epoch}
                with self.subTest(epoch=epoch), \
                     patch.dict(substitute.os.environ, environment, clear=True), \
                     patch.object(substitute.dt, 'datetime') as clock:
                    clock.now.return_value = days[0]
                    before = substitute.input_snapshot(source, build)
                    clock.now.return_value = days[1]
                    self.assertNotEqual(before, substitute.input_snapshot(source, build))

    def test_input_snapshot_changes_for_headers_generated_inputs_and_new_files(self):
        snapshot = getattr(substitute, "input_snapshot", None)
        self.assertTrue(callable(snapshot), "source-only cache ignores included and generated inputs")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            src, build = root / "src", root / "build"
            src.mkdir()
            build.mkdir()
            header = src / "a.h"
            header.write_text("#define A 1\n")
            original = snapshot(src, build)
            before = header.stat()
            header.write_text("#define A 2\n")
            substitute.os.utime(header, ns=(before.st_atime_ns, before.st_mtime_ns))
            changed = snapshot(src, build)
            self.assertNotEqual(original, changed)
            (build / "generated.h").write_text("int generated;\n")
            generated = snapshot(src, build)
            self.assertNotEqual(changed, generated)
            (src / "new.h").write_text("int added;\n")
            self.assertNotEqual(generated, snapshot(src, build))
            with patch.dict(substitute.os.environ, {"SOURCE_DATE_EPOCH": "0"}):
                epoch = snapshot(src, build)
            with patch.dict(substitute.os.environ, {"SOURCE_DATE_EPOCH": "86400"}):
                self.assertNotEqual(epoch, snapshot(src, build), "__DATE__ is also a compiler input")

    def test_identical_assembly_reuses_object_but_changed_or_corrupt_inputs_do_not(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            asm = root / "asm"
            pristine = root / "pristine"
            asm.mkdir()
            pristine.mkdir()
            record_file = pristine / ".a.o.cmd"
            record_file.write_text("savedcmd_a.o := gcc-14 -c -o a.o /work/src/a.c\n")
            records = {"a.o": substitute.parse_record(record_file)}
            assembly = asm / "a.s"
            assembly.write_text(".text\n")
            calls = []

            def assemble(command, **kwargs):
                calls.append(command)
                (asm / "a.o").write_bytes(b"assembled:" + assembly.read_bytes())
                return SimpleNamespace(stdout="OK a.o\n", stderr="", returncode=0)

            def run():
                rows = [{"unit": "a.o", "outcome": "emitted",
                         "assemblySha256": substitute.sha256_file(assembly)}]
                substitute.assemble_batch(rows, records, asm, pristine)
                self.assertEqual(rows[0]["outcome"], "assembled")
                return rows[0]

            with patch.object(substitute.subprocess, "run", side_effect=assemble), patch.object(substitute, "verify_image"):
                run()
                unchanged = run()
                self.assertEqual(len(calls), 1, "identical assembly was assembled twice")
                self.assertTrue(unchanged.get("assemblyFromCache"))
                (asm / "a.o").write_bytes(b"corrupt")
                run()
                self.assertEqual(len(calls), 2)
                assembly.write_text(".text\nret\n")
                run()
                self.assertEqual(len(calls), 3)
                record_file.write_text("savedcmd_a.o := gcc-14 -Wa,-march=armv8-a -c -o a.o /work/src/a.c\n")
                records["a.o"] = substitute.parse_record(record_file)
                run()
                self.assertEqual(len(calls), 4)
                with patch.object(substitute, "sealed_image", return_value="sha256:" + "b" * 64):
                    run()
                self.assertEqual(len(calls), 5)
                assembly.write_text('.incbin "kernel/config_data.gz"\n')
                run()
                run()
                self.assertEqual(len(calls), 7, "external assembly inputs cannot use a text-only key")

    def test_identical_installed_object_preserves_its_timestamp(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            asm, out = root / "asm", root / "out"
            asm.mkdir()
            out.mkdir()
            for directory in (asm, out):
                (directory / "a.o").write_bytes(b"same object")
            substitute.os.utime(out / "a.o", ns=(1000000000, 1000000000))
            rows = [{"unit": "a.o", "outcome": "assembled", "missingSymbols": [],
                     "extraSymbols": [], "objectSha256": substitute.sha256_file(asm / "a.o")}]
            substitute.install(rows, asm, out, False)
            self.assertEqual((out / "a.o").stat().st_mtime_ns, 1000000000)
            self.assertEqual(rows[0]["outcome"], "substituted")


if __name__ == "__main__":
    unittest.main()
