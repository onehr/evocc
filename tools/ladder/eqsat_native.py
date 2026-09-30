"""Bounded native A/A calibration in the existing bench infrastructure.

This is not Q2/Q3 acceptance. The initial adapter deliberately admits only the
reviewed, header-free P0 fixture, EqSat off, checked MIR and Darwin arm64.
"""
from __future__ import annotations

import json
import fcntl
import math
import os
from pathlib import Path
import platform
import random
import shutil
import stat
import statistics
import subprocess
import sys
import tempfile
import time

from .eqsat_chain import digest, natural, require, unique, verify_bundle
from .eqsat_census import bounded_capture, check_source_revision
from .eqsat_manifest import artifacts, audit_manifest, validate_manifest

FIXTURES = ('tests/fixtures/verified-eqsat-chain.c',
            'tests/fixtures/eqsat-performance/integer-mix.c')
FRONTEND = ['--emit=evoir', '-O0', '--eq-proof-dir={chain}', '{source}']
REFERENCE = ['-std=gnu11', '-O0', '-c', '{source}', '-o', '{object}']
LINK = ['{object}', '-o', '{executable}']
SDK_LINK = ['-fuse-ld={link_editor}', *LINK]


def host_identity():
    require(platform.system() == 'Darwin' and platform.machine() == 'arm64',
            'adapter requires native Darwin arm64, not Linux/TCG performance')
    cpu = subprocess.check_output(['/usr/sbin/sysctl', '-n', 'machdep.cpu.brand_string'],
                                  timeout=5, text=True).strip()
    return dict(system='Darwin', release=platform.release(), machine='arm64', runtime='native',
                cpu=cpu, libc='libSystem@Darwin-' + platform.release(), affinity='uncontrolled')


def execution_plan(m, observed_host):
    validate_manifest(m)
    require(m['schema'] == 'evocc-performance-plan-v2', 'v1 is read-only; no native recipe')
    require(m['host'] == observed_host, 'host identity/control mismatch')
    c = m['controls']
    require(c['target'] == 'aarch64-unknown-linux-gnu' and c['isa'] == 'baseline'
            and c['abi'] == 'linux-lp64-source/darwin-arm64-backend'
            and c['semantic_flags'] == [], 'unsupported source/backend controls')
    require(c['lowering'] == dict(optimization_level=0, macros=[], integer_constant_folding=False,
            immutable_global_propagation=False, pure_value_reuse=False,
            constant_edge_pruning=False, linear_block_joining=False), 'unsupported lowering recipe')
    require(c['optimizer'] == dict(chain='scalar-chain-v1', extractor='selected-pair-v1',
            counter_mapping='legacy-coupled-v1', registry='p0.scalar.v4', max_nodes=100000,
            max_epochs=1024, max_transactions=8, proof_required=True), 'nondefault inactive optimizer settings')
    environment = dict(PATH='/usr/bin:/bin', LANG='C', LC_ALL='C')
    sdk = m['native']['adapter'] == 'darwin-p0-aa-v2'
    if sdk:
        settings = Path(m['native']['sdk']['settings']['path'])
        require(settings.name == 'SDKSettings.json', 'SDK settings identity is not declared')
        sdk_root = settings.parent.resolve()
        require(Path(m['native']['sdk']['system_library']['path']).resolve() ==
                (sdk_root/'usr/lib/libSystem.tbd').resolve(), 'system library is outside the declared SDK')
        environment['SDKROOT'] = str(sdk_root)
    require(m['environment'] == environment,
            'native recipe has a closed environment')
    require(m['protocol']['pairs'] % 2 == 0, 'balanced A/A requires an even pair count')
    # Bound the planned process population as well as each child. No implicit retries.
    calls = len(m['cases']) * (10 + 2*m['protocol']['warmups'] + 2*m['protocol']['pairs'])
    require(calls * m['resources']['timeout_seconds'] <= 3600, 'campaign watchdog envelope exceeds one hour')
    for case in m['cases']:
        require(case['family'] == 'p0' and case['source']['path'] in FIXTURES
                and case['inputs'] == [] and case['run_argv'] == ['{executable}'],
                'only registered header-free P0 fixtures are implemented')
        require(case['compiler_argv'] == FRONTEND and case['reference_argv'] == REFERENCE
                and case['link_argv'] == (SDK_LINK if sdk else LINK), 'argv differs from implemented native recipe')
        require(case['coverage'] == dict(subject_objects=['program.o'], reference_objects=[]),
                'native recipe builds one subject C object, no reference substitution')
        require(case['result'] == dict(exit_code=0, stdout_sha256=digest(b''), stderr_sha256=digest(b'')),
                'registered native cases must return zero with no output')


def run_calibration(manifest_path, root):
    root, manifest_path = Path(root).resolve(), Path(manifest_path).resolve()
    audit = audit_manifest(manifest_path, root)
    raw = manifest_path.read_bytes()
    require(digest(raw) == audit['manifest_digest'], 'plan changed after audit')
    m = json.loads(raw, object_pairs_hook=unique)
    host = host_identity()
    execution_plan(m, host)
    pinned = {str((root / item['path']).resolve()): item['sha256'] for item, _ in artifacts(m)}
    tools = {name: (root / item['path']).resolve() for name, item in m['tools'].items()}
    link_flags = ['-fuse-ld=' + str(tools['link_editor'])] if 'link_editor' in tools else []
    require(Path(sys.executable).resolve() == tools['collector_python'], 'wrong collector interpreter')

    def stable():
        require(manifest_path.read_bytes() == raw, 'plan changed during execution')
        for item, _ in artifacts(m):
            actual = str((root / item['path']).resolve())
            require(actual in pinned and pinned[actual] == item['sha256'], 'artifact path changed')
        for name, expected in pinned.items():
            require(digest(Path(name).read_bytes()) == expected, 'artifact changed: ' + name)

    (root / '.build').mkdir(exist_ok=True)
    with (root / '.build/bench.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError('another benchmark owns the shared lock') from error
        stable()
        require(shutil.disk_usage(root).free >= m['resources']['min_free_bytes'], 'disk reserve unavailable')
        base = root / '.evidence/eqsat-native'
        base.mkdir(parents=True, exist_ok=True)
        directory = Path(tempfile.mkdtemp(prefix='aa-', dir=base))
        (directory / 'manifest.json').write_bytes(raw)
        receipt = dict(schema='evocc-native-calibration-v1', status='STARTED',
            manifest_digest=digest(raw), implementation_commit=m['implementation_commit'],
            host=host, scope='Darwin-P0-A/A-pipeline-only-not-Q2-Q3-or-Linux',
            python=dict(executable=str(Path(sys.executable).resolve()), version=sys.version),
            attempts=[], cases=[], performance='NOT_CLAIMED',
            collector_files=m['native']['collector_files'])

        def save():
            pending = directory / 'receipt.json.pending'
            pending.write_text(json.dumps(receipt, sort_keys=True, indent=2) + '\n')
            pending.replace(directory / 'receipt.json')

        env = {**m['environment'], 'TMPDIR': str(directory)}
        runner = Capture(directory, m['resources'], env, receipt, save)
        save()
        try:
            for case in m['cases']:
                source = (root / case['source']['path']).resolve()
                work = directory / case['id']
                work.mkdir()
                chain = work / 'chain'
                product = dict(id=case['id'], status='STARTED', samples=[], pairs=[], warmups=[])
                receipt['cases'].append(product)

                def command(stage, argv, expected=None):
                    stable()
                    row = runner.run(case['id'] + ':' + stage, argv, expected)
                    stable()
                    return directory / row['stdout']

                # Admission checks are distinct from timed native samples.
                zero = dict(exit_code=0, stdout_sha256=digest(b'0\n'), stderr_sha256=digest(b''))
                for mode in ('--interpret-c-hir', '--interpret-evoir'):
                    command(mode, [tools['compiler'], mode, '-O0', source], zero)
                emitted = command('frontend', [tools['compiler'], '--emit=evoir', '-O0',
                                              '--eq-proof-dir=' + str(chain), source])
                identity = verify_bundle(chain)
                require(emitted.read_bytes() == (chain / 'output.evo').read_bytes(),
                        'actual codegen IR differs from identity chain')
                ir_digest = identity['verified_ir_digest']
                product.update(chain=identity, ir_digest=ir_digest)
                for p in chain.iterdir():
                    pinned[str(p.resolve())] = digest(p.read_bytes())
                assembly = command('backend', [tools['ir_backend'], 'darwin-arm64', ir_digest,
                                                chain / 'output.evo'])
                obj, executable = work / 'program.o', work / 'program'
                pinned[str(assembly.resolve())] = digest(assembly.read_bytes())
                command('assemble', [tools['assembler'], '-c', '-x', 'assembler', assembly, '-o', obj])
                pinned[str(obj.resolve())] = digest(obj.read_bytes())
                command('link', [tools['linker'], *link_flags, obj, '-o', executable])
                pinned[str(executable.resolve())] = digest(executable.read_bytes())
                product.update(assembly_digest=pinned[str(assembly.resolve())],
                               object_digest=pinned[str(obj.resolve())],
                               executable_digest=pinned[str(executable.resolve())],
                               codegen='canonical-IR/checked-PIC/Darwin-arm64',
                               coverage=case['coverage'])
                ref_obj, reference = work / 'reference.o', work / 'reference'
                command('reference-compile', [tools['reference'], '-std=gnu11', '-O0', '-c', source, '-o', ref_obj])
                command('reference-link', [tools['linker'], *link_flags, ref_obj, '-o', reference])
                command('reference-result', [reference], case['result'])
                command('subject-admission', [executable], case['result'])
                protocol = m['protocol']
                for repeat in range(protocol['warmups']):
                    for label in ('A0', 'A1'):
                        command(f'warmup-{repeat}-{label}', [executable], case['result'])
                        product['warmups'].append(len(receipt['attempts'])-1)
                orders = pair_order(protocol['pairs'], protocol['seed'])
                product['order'] = orders
                for pair, order in enumerate(orders):
                    values = {}
                    for label in order:
                        command(f'pair-{pair}-{label}', [executable], case['result'])
                        row = receipt['attempts'][-1]
                        index = len(receipt['attempts'])-1
                        product['samples'].append(dict(pair=pair, label=label, attempt=index,
                            wall_ns=row['wall_ns'], executable_digest=product['executable_digest']))
                        values[label] = row['wall_ns']
                        save()
                    product['pairs'].append(values)
                product.update(status='OBSERVED_AA', statistics=summarize(product['pairs']))
            stable()
            check_source_revision(root, m['implementation_commit'])
            receipt.update(status='OBSERVED_AA', correctness='RESULT_CHECKED_EVERY_EXECUTION',
                           proof='IDENTITY_ONLY_NO_EVP_REWRITE',
                           counters='NOT_MEASURED', runtime_cache='NOT_CONTROLLED',
                           effective_controls='REGISTERED_RECIPE_ONLY')
        except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
            receipt.update(status='FAILED', error=str(error))
        except BaseException:
            receipt.update(status='INTERRUPTED')
            raise
        finally:
            save()
        return directory, receipt


def pair_order(pairs, seed):
    order = [('A0', 'A1') if n % 2 == 0 else ('A1', 'A0') for n in range(pairs)]
    random.Random(seed).shuffle(order)
    return order


def summarize(pairs):
    require(pairs, 'missing pairs')
    for row in pairs:
        require(type(row) is dict and set(row) == {'A0', 'A1'}, 'incomplete pair')
        require(all(natural(value) > 0 for value in row.values()), 'invalid sample')
    delta = sorted(row['A1'] - row['A0'] for row in pairs)
    n = len(delta)
    # Distribution-free median interval. Small populations have no finite 95%
    # interval; report that honestly instead of bootstrapping false precision.
    ranks = [k for k in range(1, n // 2 + 1)
             if 40 * sum(math.comb(n, j) for j in range(k)) <= 2**n]
    interval = [delta[ranks[-1]-1], delta[n-ranks[-1]]] if ranks else None
    med = statistics.median(delta)
    return dict(pairs=n, paired_median_delta_ns=med, median_delta_ci95_ns=interval,
                interval_assumptions='independent-identically-distributed-paired-deltas',
                paired_delta_mad_ns=statistics.median(abs(d-med) for d in delta),
                label_median_ns={label: statistics.median(p[label] for p in pairs)
                                 for label in ('A0', 'A1')},
                inference='A/A_CALIBRATION_ONLY_NO_SPEEDUP_CLAIM')


class CommandExitError(ValueError):
    """A waited command exited nonzero, with its capture/integrity bounds intact.

    A fixed proof template may classify only this failure as UNKNOWN. Launch,
    watchdog, output, retention, result and artifact failures are not proofs.
    """


class Capture:
    """Own only this run's new directory; never delete caller-owned artifacts."""
    def __init__(self, directory, resources, env, receipt, save, measure_resources=False,
                 unique_inodes=False, watch_interval_seconds=0):
        self.directory = directory
        self.resources, self.env, self.receipt, self.save = resources, env, receipt, save
        self.measure_resources = measure_resources
        self.unique_inodes = unique_inodes
        self.watch_interval_seconds = watch_interval_seconds
        self.last_watch = float('-inf')

    def watch(self):
        require(self.directory.is_dir(), 'capture directory missing')
        now = time.monotonic()
        if now-self.last_watch < self.watch_interval_seconds:
            return
        def scan_error(error):
            # A waited checker may remove its owned scratch tree concurrently.
            # Permission/I/O errors are not disappearance and must fail closed.
            if not isinstance(error, FileNotFoundError):
                raise error
        size, inodes = 0, set()
        for folder, _, names in os.walk(self.directory, onerror=scan_error):
            for name in names:
                try:
                    value = os.stat(os.path.join(folder, name))
                except FileNotFoundError:
                    continue
                if stat.S_ISREG(value.st_mode):
                    if self.unique_inodes and (value.st_dev, value.st_ino) in inodes:
                        continue
                    inodes.add((value.st_dev, value.st_ino))
                    size += value.st_size
        require(self.directory.is_dir(), 'capture directory missing')
        require(size <= self.resources['max_retained_bytes'], 'retention limit exceeded')
        # A scan slower than the interval must not immediately trigger another
        # full scan at the next pipe poll. Cool down from its completion time.
        self.last_watch = time.monotonic()

    def run(self, stage, argv, expected=None, quiet=False):
        index = len(self.receipt['attempts'])
        out = self.directory / f'{index:03}.stdout'
        err = self.directory / f'{index:03}.stderr'
        row = dict(stage=stage, argv=[str(a) for a in argv], status='STARTED',
                   stdout=out.name, stderr=err.name)
        if quiet:
            row['storage_watch'] = 'BATCH_BOUNDARIES_ONLY_PREDECLARED_CHILD_ENVELOPE'
        self.receipt['attempts'].append(row)
        self.save()
        started = time.monotonic()
        start_ns = time.monotonic_ns()
        try:
            self.watch()
            usage = {} if self.measure_resources else None
            with out.open('xb') as o, err.open('xb') as e:
                started = time.monotonic()
                start_ns = time.monotonic_ns()
                row['exit_code'] = bounded_capture(row['argv'], self.directory, o, e,
                    self.resources, started, self.env, watch=self.watch, usage=usage, quiet=quiet)
                row['wall_ns'] = time.monotonic_ns() - start_ns
            if usage is not None:
                require(usage, 'missing child resource observation')
                row['resources'] = dict(**usage, peak_rss_bytes=usage['peak_rss_raw'] *
                    (1 if platform.system() == 'Darwin' else 1024),
                    rss_unit='bytes', wall_scope='spawn-to-stdio-drain')
            self.watch()
            row.update(stdout_sha256=digest(out.read_bytes()), stderr_sha256=digest(err.read_bytes()))
            if expected is not None:
                require(all(row[k] == expected[k] for k in
                            ('exit_code', 'stdout_sha256', 'stderr_sha256')), 'result mismatch')
            else:
                if row['exit_code'] != 0:
                    row['failure_kind'] = 'COMMAND_EXIT'
                    raise CommandExitError('command exited with status ' + str(row['exit_code']))
            row['status'] = 'RESULT_CHECKED' if expected is not None else 'COMPLETED'
        except BaseException as error:
            row.update(status='FAILED', error=str(error))
            raise
        finally:
            row.setdefault('wall_ns', time.monotonic_ns() - start_ns)
            for name, path in (('stdout', out), ('stderr', err)):
                if path.is_file():
                    data = path.read_bytes()
                    row[name + '_sha256'] = digest(data)
                    row[name + '_bytes'] = len(data)
            self.save()
        return row
