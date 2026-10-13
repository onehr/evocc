#!/usr/bin/env python3
"""Paired expanded KUnit using the existing boot/init and upstream KTAP parser.

No builds, disk copies, assertion waivers or reference-result reuse. Preflight
requires an audited current composition and matching installed module runtimes.
The historical expanded receipt is not overwritten by this new measurement.
"""

from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import importlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

import layout
import selftest
import selftest_full

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / 'tests/contracts/kernel-expanded-kunit-contract.json'
RECEIPT = ROOT / 'tests/contracts/kernel-kunit-bench-receipt.json'


def number_cases(cases):
    counts = Counter()
    result = []
    for row in cases:
        counts[row['path']] += 1
        result.append({**row, 'occurrence': counts[row['path']]})
    return result


def summarize(parsed):
    cases = number_cases(parsed['cases'])
    complete = (bool(cases) and parsed['parserErrors'] == 0
                and parsed['parserCrashed'] == 0 and parsed['parserTests'] == len(cases)
                and parsed['parserStatus'] in ('SUCCESS', 'FAILURE', 'SKIPPED'))
    outcomes = dict(Counter(row['outcome'] for row in cases))
    return {**parsed, 'cases': cases, 'tests': len(cases), 'outcomes': outcomes,
            'complete': complete, 'status': result_status(complete, outcomes)}


def result_status(complete, outcomes):
    if not complete:
        return 'INCOMPLETE'
    if outcomes.get('FAIL') or outcomes.get('ERROR'):
        return 'FAIL'
    return 'PASS' if outcomes and set(outcomes) == {'PASS'} else 'INCOMPLETE'


def upstream_parser(source, contract):
    directory = source / 'tools/testing/kunit'
    for filename, digest in contract['parserSha256'].items():
        if selftest.sha(directory / filename) != digest:
            raise ValueError(f'upstream KUnit parser changed: {filename}')
    sys.path.insert(0, str(directory))
    try:
        parser = importlib.import_module('kunit_parser')
        printer = importlib.import_module('kunit_printer')
        for module in (parser, printer):
            if Path(module.__file__).resolve().parent != directory.resolve():
                raise ValueError('KUnit parser imported from a different source tree')
    finally:
        sys.path.remove(str(directory))

    def parse(text):
        lines, replays = selftest.normalize_printk_replays(text.replace('\r', ''))
        stream = parser.extract_tap_lines(lines)
        if not stream:
            raise ValueError('no KUnit output in the execution boundary')
        test = parser.parse_test(stream, 0, [], False, printer.null_printer)
        test.status = test.counts.get_status()
        # The upstream convenience entry point stops after the first plan.
        # Keep its grammar, but do not silently ignore extra result records.
        trailing = [line for line in stream
                    if re.match(r'^\s*(?:not ok\b|ok\s+\d|1\.\.\d|(?:K?TAP) version\b|Bail out!)', line)]
        if trailing:
            raise ValueError('unexpected KUnit records after the completed plan')
        cases = []

        def walk(node, prefix):
            path = '/'.join((*prefix, node.name))
            if node.subtests:
                for child in node.subtests:
                    walk(child, (*prefix, node.name))
            else:
                outcome = {'SUCCESS': 'PASS', 'SKIPPED': 'SKIP',
                           'TEST_CRASHED': 'ERROR'}.get(node.status.name, 'FAIL')
                cases.append({'path': path, 'outcome': outcome})
        walk(test, ())
        return {'parserStatus': test.status.name, 'parserErrors': test.counts.errors,
                'parserCrashed': test.counts.crashed, 'parserTests': test.counts.total(),
                'consoleReplays': replays, 'cases': cases}
    return parse


def parse_modules(text, nonce, names, parse):
    """Frame each load's complete dmesg separately; preserve repeated cases."""
    lines = text.replace('\r', '').splitlines()
    if (not names or len(set(names)) != len(names)
            or lines.count(f'EVO_BOOT_NONCE {nonce}') != 1):
        raise ValueError('missing/duplicate KUnit module population or boot nonce')
    active, body, rows, finished, entered = None, [], [], False, False
    for line in lines:
        if line == f'EVO_BOOT_NONCE {nonce}':
            entered = True
        if not line.startswith('KUNIT_'):
            if active:
                body.append(line)
            continue
        begin = re.fullmatch(r'KUNIT_MODULE_BEGIN (\S+) (\S+)', line)
        end = re.fullmatch(r'KUNIT_MODULE_END (\S+) (\S+) (\d+)', line)
        complete = re.fullmatch(r'KUNIT_MODULE_COMPLETE (\S+) (\d+)', line)
        if finished or not entered:
            raise ValueError('KUnit module event outside its nonce/completion boundaries')
        if begin:
            observed, name = begin.groups()
            if (active or observed != nonce or len(rows) >= len(names)
                    or name != names[len(rows)]):
                raise ValueError('foreign, duplicate or out-of-order KUnit module')
            active, body = name, []
        elif end:
            observed, name, status = end.groups()
            if not active or observed != nonce or name != active:
                raise ValueError('KUnit module end without its matching begin')
            result = summarize(parse('\n'.join(body)))
            rows.append({**result, 'module': name, 'loadExitCode': int(status)})
            active = None
        elif complete:
            observed, count = complete.groups()
            if active or observed != nonce or int(count) != len(names) or len(rows) != len(names):
                raise ValueError('KUnit module completion does not match its population')
            finished = True
        else:
            raise ValueError('unrecognized KUnit execution boundary')
    if active or not finished:
        raise ValueError('incomplete KUnit module transcript')
    cases = number_cases([{'path': row['module'] + '/' + case['path'], 'outcome': case['outcome']}
                          for row in rows for case in row['cases']])
    counts = dict(Counter(row['outcome'] for row in cases))
    complete = all(row['complete'] and row['loadExitCode'] == 0 for row in rows)
    return {'status': result_status(complete, counts), 'complete': complete,
            'filesLoaded': sum(row['loadExitCode'] == 0 for row in rows),
            'attemptedModules': len(rows), 'tests': len(cases), 'outcomes': counts,
            'cases': cases, 'modules': rows}


def compare_cases(reference, subject, expected):
    def index(side):
        rows = side['cases']
        indexed = {(r['path'], r['occurrence']): r['outcome'] for r in rows}
        if len(indexed) != len(rows):
            raise ValueError('duplicate KUnit case identity')
        return indexed
    before, after = index(reference), index(subject)
    complete = (reference['complete'] and subject['complete']
                and len(before) == len(after) == expected and before.keys() == after.keys())
    counts = Counter(before.values()) + Counter(after.values())
    differences = [{'path': path, 'occurrence': occurrence,
                    'reference': before.get((path, occurrence), 'NOT_RUN'),
                    'subject': after.get((path, occurrence), 'NOT_RUN')}
                   for path, occurrence in sorted(before.keys() | after.keys())
                   if before.get((path, occurrence)) != after.get((path, occurrence))]
    return {'status': result_status(complete, counts), 'executionComplete': complete,
            'planned': expected, 'referenceObserved': len(before), 'subjectObserved': len(after),
            'referenceCounts': dict(Counter(before.values())),
            'subjectCounts': dict(Counter(after.values())), 'differences': differences}


def runtime_inputs(folder, image, config, module_link, contract):
    modules = selftest_full.check_module_runtime(folder, image, module_link)
    build = selftest.read(folder / 'build.json')
    if (build.get('suiteModules') != contract['suiteModules']
            or modules.get('configSha256') != selftest.sha(config)
            or build.get('initramfsSha256') != selftest.sha(folder / 'initramfs.cpio')):
        raise ValueError('KUnit module runtime inventory, configuration or initramfs differs')
    return {name: selftest.sha(folder / name)
            for name in ('build.json', 'initramfs.cpio', 'scratch.img')}


def flight(image, directory, env):
    expected = selftest.sha(image)
    result = subprocess.run([str(ROOT / 'tools/k1/boot.sh'), str(image), str(directory)], env=env)
    boot = selftest.read(directory / 'boot.json')
    log = Path(boot['serialLog'])
    if (selftest.sha(image) != expected or boot['imageSha256'] != expected
            or selftest.sha(log) != boot['serialLogSha256']):
        raise ValueError('KUnit Image or serial capture changed during execution')
    text = log.read_text(errors='strict').replace('\r', '')
    if text.splitlines().count(f'EVO_BOOT_NONCE {boot["nonce"]}') != 1:
        raise ValueError('KUnit flight has no unique current nonce')
    return boot, text, result.returncode


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument('--source-root', type=Path, default=layout.SOURCE)
    result.add_argument('--kernel-receipt', type=Path,
                        default=ROOT / 'tests/contracts/kernel-expanded-kunit-full-receipt.json')
    # Each composition has its own reference boot; a default would name an
    # old Image. kernel_selection() refuses a full composition without it.
    result.add_argument('--reference-boot', type=Path, help='boot.json of the reference Image')
    for label in ('reference', 'subject'):
        result.add_argument(f'--{label}-runtime', type=Path, default=layout.KUNIT_RUNTIME / label,
                            help='payload of tools/k1/kunit_payload.py')
    result.add_argument('--builtin-initramfs', type=Path, default=ROOT / '.build/initramfs/initramfs.cpio')
    result.add_argument('--check', action='store_true', help='read-only preflight; no boot or acceptance receipt')
    result.add_argument('--allow-qemu-drift', action='store_true')
    return result


def main(argv=None):
    args = parser().parse_args(argv)
    receipt, run = None, None

    def save():
        text = json.dumps(receipt, indent=2) + '\n'
        (run / 'summary.json').write_text(text)
        RECEIPT.write_text(text)

    try:
        contract = selftest.read(CONTRACT)
        parse = upstream_parser(args.source_root, contract)
        selected = selftest_full.kernel_selection(True, receipt_path=args.kernel_receipt,
                                                  reference_boot_path=args.reference_boot)
        kernel_receipt = selected['receipt'].resolve()
        if not kernel_receipt.is_relative_to(ROOT.resolve()):
            raise ValueError('KUnit composition receipt must be inside the repository')
        inputs = {str(CONTRACT): selftest.sha(CONTRACT),
                  str(kernel_receipt): selftest.sha(kernel_receipt),
                  str(args.builtin_initramfs): selftest.sha(args.builtin_initramfs)}
        for filename in contract['parserSha256']:
            path = args.source_root / 'tools/testing/kunit' / filename
            inputs[str(path)] = selftest.sha(path)
        runtimes = {label: getattr(args, label + '_runtime') for label in selected['images']}
        for label, image in selected['images'].items():
            config = selected['configs'][label]
            if selftest.sha(config) != contract['configSha256']:
                raise ValueError('KUnit configuration differs from the fixed population')
            for path in (image, config):
                inputs[str(path)] = selftest.sha(path)
            hashes = runtime_inputs(runtimes[label], image, config,
                                    selected.get('moduleLink') if label == 'subject' else None, contract)
            inputs.update({str(runtimes[label] / name): value for name, value in hashes.items()})
        if args.check:
            print('KUnit preflight PASS; execution NOT_RUN')
            return 0
        run_id = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ')
        run = ROOT / '.evidence/kunit' / run_id
        run.mkdir(parents=True)
        receipt = {'schemaVersion': 1, 'run': run_id, 'status': 'RUNNING',
                   'executionComplete': False, 'evidence': str(run.relative_to(ROOT)),
                   'scope': contract['scope'], 'contractSha256': inputs[str(CONTRACT)],
                   'kernelReceiptPath': str(kernel_receipt.relative_to(ROOT)),
                   'kernelReceiptSha256': inputs[str(kernel_receipt)],
                   'kernelImageSha256': selftest.sha(selected['images']['subject']),
                   'inputs': inputs, 'config': {'sha256': contract['configSha256']}}
        save()
        # A caller's filtered selftest environment must not silently alter a
        # full KUnit run. Only explicit drift acknowledgement is inherited.
        env = {k: v for k, v in os.environ.items() if not k.startswith('K1_')}
        if args.allow_qemu_drift or os.environ.get('K1_ALLOW_QEMU_DRIFT') == '1':
            env['K1_ALLOW_QEMU_DRIFT'] = '1'
        env.update(K1_BOOT_LANE=contract['bootLane'], K1_KUNIT_AUTORUN='1', K1_CONSOLE_LOGLEVEL='7')
        for label, image in selected['images'].items():
            print(f'KUnit {label}: built-in suites, then {len(contract["suiteModules"])} real module loads', flush=True)
            boot, text, rc = flight(image, run / (label + '-builtin'),
                                    {**env, 'K1_INITRAMFS': str(args.builtin_initramfs),
                                     'K1_BOOT_TIMEOUT': str(contract['bootTimeoutSeconds'])})
            result = summarize(parse(text))
            result['complete'] &= rc == 0 and boot['verdict'] == 'PASS'
            result['status'] = result_status(result['complete'], result['outcomes'])
            receipt[label] = {**result, 'boot': boot}
            save()
            boot, text, rc = flight(image, run / (label + '-modules'),
                                    {**env, 'K1_INITRAMFS': str(runtimes[label] / 'initramfs.cpio'),
                                     'K1_TEST_DISK': str(runtimes[label] / 'scratch.img'),
                                     'K1_KUNIT_FILTER': '__module_suites_only__',
                                     'K1_BOOT_TIMEOUT': str(contract['moduleTimeoutSeconds'])})
            modules = parse_modules(text, boot['nonce'], [m['name'] for m in contract['suiteModules']], parse)
            modules['complete'] &= rc == 0 and boot['verdict'] == 'PASS'
            modules['status'] = result_status(modules['complete'], modules['outcomes'])
            receipt[label]['modules'] = {**modules, 'boot': boot}
            save()
        receipt['comparison'] = compare_cases(receipt['reference'], receipt['subject'], contract['builtInCases'])
        receipt['moduleComparison'] = compare_cases(receipt['reference']['modules'], receipt['subject']['modules'],
                                                     contract['moduleCases'])
        if any(selftest.sha(Path(path)) != digest for path, digest in inputs.items()):
            raise ValueError('KUnit measured inputs changed during the paired run')
        # Repeat the existing composition audit; a runtime-only PASS must not
        # hide replacement of an installed target object during the flights.
        selftest_full.kernel_selection(True, receipt_path=args.kernel_receipt,
                                       reference_boot_path=args.reference_boot)
        comparisons = (receipt['comparison'], receipt['moduleComparison'])
        receipt['executionComplete'] = all(row['executionComplete'] for row in comparisons)
        statuses = {row['status'] for row in comparisons}
        receipt['status'] = ('INCOMPLETE' if not receipt['executionComplete'] else
                             'FAIL' if 'FAIL' in statuses else
                             'PASS' if statuses == {'PASS'} else 'INCOMPLETE')
        save()
        print(f'KUnit {receipt["status"]}; receipt {RECEIPT.relative_to(ROOT)}')
        return 0  # Completed measurement; the common bench evaluates its verdict.
    except (OSError, KeyError, TypeError, ValueError, subprocess.SubprocessError) as error:
        if receipt is not None:
            receipt.update(status='ERROR', executionComplete=False, error=str(error))
            save()
        print(f'KUnit: {error}', file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        if receipt is not None:
            receipt.update(status='INTERRUPTED', executionComplete=False)
            save()
        return 130


if __name__ == '__main__':
    raise SystemExit(main())
