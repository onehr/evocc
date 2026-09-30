"""R02a/d Linux pilot adapter, under the existing bench lock/capture pipeline.

This measures a frozen mixed-link population, not full-library/compiler parity.
The one owned hardware VM is read-only except tmpfs and this attempt's capture.
No downloads, source edits, automatic retries or reference fallback. Disabling
DNS is not a claim that the container's network is isolated.
"""
from __future__ import annotations

import fcntl
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import statistics
import sys
import tempfile
import time

from .eqsat_chain import digest, keys, natural, require, sha256_identity, unique
from .eqsat_census import chain_census, check_source_revision
from .eqsat_elf import inspect_elf
from .eqsat_manifest import check_committed
from .eqsat_native import Capture, host_identity, pair_order, summarize
from .eqsat_twins import bind_bundle, stable_bundle, canonical, native_identity, admit_unchecked
from .eqsat_preflight_run import CONTRACT as PREFLIGHT_CONTRACT

ROOTS = dict(zlib='.build/ladder/zlib/zlib-1.3.2', zstd='.build/ladder/zstd/zstd-1.5.7',
             cjson='.build/ladder/cjson/cJSON-1.7.19',
             sysroot='.build/k7-coverage/case/ladder-sysroot')
COLLECTORS = ('tools/bench.py', 'tools/ladder/eqsat_pilot.py', 'tools/ladder/eqsat_guest.py',
              'tools/ladder/eqsat_native.py', 'tools/ladder/eqsat_census.py',
              'tools/ladder/eqsat_chain.py', 'tools/ladder/eqsat_manifest.py',
              'tools/ladder/eqsat_twins.py', 'tools/ladder/eqsat_elf.py',
              'tools/ladder/eqsat_preflight.py', 'tools/ladder/eqsat_opportunities.py',
              'tools/ladder/eqsat_preflight_run.py', 'tools/ladder/eqsat_template.py',
              'tools/ladder/eqsat_pilot_audit.py')
LEAN_SOURCES = ('lean/lean-toolchain', 'lean/lakefile.toml', 'lean/lake-manifest.json',
    'lean/EvoIR/Sem.lean', 'lean/EvoIR/Term.lean', 'lean/EvoIR/Fact.lean',
    'lean/EvoIR/Rule.lean', 'lean/EvoIR/Registry.lean',
    'lean/EvoIR/Proofs/Bits.lean', 'lean/EvoIR/Proofs/Lattice.lean', 'lean/EvoIR/Proofs/Rules.lean')
LEAN_IMPORTS = tuple('lean/.lake/build/lib/lean/'+p[len('lean/'):-len('.lean')]+'.olean'
                     for p in LEAN_SOURCES if p.endswith('.lean'))
GUEST_TOOLS = ('/usr/bin/gcc-14', '/usr/bin/clang-19', '/usr/bin/as', '/usr/bin/ld',
               '/usr/bin/python3', '/usr/bin/opt-19', '/usr/bin/strip',
               '/usr/lib/aarch64-linux-gnu/libc.so.6')
ZLIB = 'adler32.c crc32.c deflate.c infback.c inffast.c inflate.c inftrees.c trees.c zutil.c compress.c uncompr.c'.split()


def tree_identity(directory):
    path = Path(directory)
    require(path.is_dir(), 'missing frozen source/header tree')
    # Only source/header inputs; build products and old evidence are not cloned.
    entries = {p.relative_to(path).as_posix(): digest(p.read_bytes())
               for p in sorted(path.rglob('*')) if p.is_file() and p.suffix in ('.c', '.h')}
    require(entries, 'empty source/header tree')
    return dict(sha256=digest(canonical(entries)), files=len(entries))


def recipes(root):
    root = Path(root)
    fixture = 'tests/fixtures/eqsat-performance/'
    zstd = [p.relative_to(root/ROOTS['zstd']).as_posix()
            for sub in ('common', 'compress', 'decompress')
            for p in sorted((root/ROOTS['zstd']/'lib'/sub).glob('*.c'))]
    def program(name, folder, sources, selected, flags, harness, modes, output):
        return dict(id=name, source_root=folder, sources=sources, subject_sources=selected,
                    flags=flags, harness=fixture+harness, modes=modes,
                    result=dict(exit_code=0, stdout_sha256=digest(output), stderr_sha256=digest(b'')))
    return [
        program('p0', '', ['tests/fixtures/eqsat-performance/integer-mix.c'],
                ['tests/fixtures/eqsat-performance/integer-mix.c'], [], 'integer-mix.c', [[]], b''),
        program('zlib', ROOTS['zlib'], ZLIB, ['deflate.c', 'inflate.c'],
                ['-D_LARGEFILE64_SOURCE=1', '-DHAVE_HIDDEN'], 'compression-workload.c',
                [['compress'], ['decompress']], b'roundtrip-ok\n'),
        program('zstd', ROOTS['zstd'], zstd,
                ['lib/compress/zstd_compress.c', 'lib/decompress/zstd_decompress.c'],
                ['-DZSTD_DISABLE_ASM', '-DZSTD_LEGACY_SUPPORT=0', '-DXXH_NAMESPACE=ZSTD_'],
                'compression-workload.c', [['compress'], ['decompress']], b'roundtrip-ok\n'),
        program('cjson', ROOTS['cjson'], ['cJSON.c'], ['cJSON.c'], [],
                'cjson-workload.c', [[]], b'json-roundtrip-ok\n'),
    ]


def validate_plan(m, root):
    keys(m, 'schema implementation_commit tools guest_tools image host trees files controls protocol resources cases preflight')
    require(m['schema'] == 'evocc-r02-linux-pilot-v1', 'unknown pilot schema')
    require(re.fullmatch('[0-9a-f]{40}', m['implementation_commit']) is not None, 'missing source revision')
    keys(m['tools'], 'compiler checker ir_backend container collector_python census lean')
    for item in m['tools'].values():
        keys(item, 'path sha256')
        require(type(item['path']) is str and item['path'], 'missing tool path')
        sha256_identity(item['sha256'])
    keys(m['guest_tools'], ' '.join(GUEST_TOOLS))
    for value in m['guest_tools'].values():
        sha256_identity(value)
    keys(m['image'], 'name digest')
    require(m['image']['name'] == 'evo-flight-lab:sealed', 'unregistered image')
    sha256_identity(m['image']['digest'])
    keys(m['trees'], ' '.join(ROOTS))
    for name, item in m['trees'].items():
        keys(item, 'path sha256 files')
        require(item['path'] == ROOTS[name] and natural(item['files']) > 0, 'unregistered input tree')
        sha256_identity(item['sha256'])
    require(m['controls'] == dict(optimization=2, native_profiles=['checked', 'unchecked'],
        variants=['A', 'B'], eqsat='shared-scalar-chain-v1/default-v4', target='linux-aarch64',
        relocation='PIC', source_semantics='GNU11/default-no-fast-math',
        lowering='O2-default-fold-reuse-join-prune-immutable-globals',
        reference='gcc-14-O2/PIC', coverage='DECLARED_MIXED_LINK_NO_FALLBACK',
        interpreter='P0_BOTH_ONLY_LIBC_APPLICATIONS_NATIVE', runtime='hardware-VM-not-bare-metal'),
        'unimplemented or mismatched effective controls')
    p = m['protocol']
    keys(p, 'warmups pairs seed retries metric comparisons effect_floor_percent precision_percent multiple_comparisons')
    require(0 <= natural(p['warmups']) <= 4 and 6 <= natural(p['pairs']) <= 40
            and p['pairs'] % 2 == 0 and natural(p['retries']) == 0
            and p['metric'] == 'guest-fresh-process-spawn-to-stdio-drain'
            and p['comparisons'] == ['AA', 'AB'] and natural(p['effect_floor_percent']) >= 1,
            'invalid frozen protocol')
    require(natural(p['precision_percent']) >= 1
            and p['multiple_comparisons'] == 'BONFERRONI_ALL_PILOT_COMPARISONS',
            'missing precision or multiplicity contract')
    natural(p['seed'])
    r = m['resources']
    keys(r, 'jobs timeout_seconds output_bytes min_free_bytes max_retained_bytes campaign_seconds vm_cpus vm_memory')
    require(natural(r['jobs']) == 1 and 10 <= natural(r['timeout_seconds']) <= 600
            and 1024 <= natural(r['output_bytes']) <= 16*1024**2
            and natural(r['min_free_bytes']) >= 8*1024**3
            and r['output_bytes'] <= natural(r['max_retained_bytes']) <= 1024**3
            and 1 <= natural(r['campaign_seconds']) <= 14400
            and r['vm_cpus'] == 2 and r['vm_memory'] == '3G', 'unbounded resource controls')
    require(m['cases'] == recipes(root), 'pilot source/object coverage changed')
    require(m['preflight'] == PREFLIGHT_CONTRACT, 'preflight controls changed')
    required = set(COLLECTORS) | set(LEAN_SOURCES) | set(LEAN_IMPORTS) | {'docs/evoir-spec.md', 'docs/evomir-spec.md',
        'tests/fixtures/eqsat-performance/integer-mix.c',
        'tests/fixtures/eqsat-performance/compression-workload.c',
        'tests/fixtures/eqsat-performance/cjson-workload.c'}
    keys(m['files'], ' '.join(required))
    for identity in m['files'].values():
        sha256_identity(identity)


def audit_plan(path, root):
    root, path = Path(root).resolve(), Path(path).resolve()
    raw = path.read_bytes()
    m = json.loads(raw, object_pairs_hook=unique)
    validate_plan(m, root)
    check_committed(path, root, raw)
    check_source_revision(root, m['implementation_commit'])
    require(m['host'] == host_identity(), 'host changed since freeze')
    require(Path(sys.executable).resolve() == (root/m['tools']['collector_python']['path']).resolve(),
            'wrong collector interpreter')
    for item in m['tools'].values():
        require(digest((root/item['path']).read_bytes()) == item['sha256'], 'tool identity changed')
    for name, expected in m['files'].items():
        require(digest((root/name).read_bytes()) == expected, 'collector/input changed: '+name)
    for item in m['trees'].values():
        require(tree_identity(root/item['path']) == {k: item[k] for k in ('sha256', 'files')},
                'source/header tree changed: '+item['path'])
    require(path.read_bytes() == raw, 'manifest changed during audit')
    return m, raw


def composite_binding(bindings):
    # Native programs may contain several independently verified translation units.
    return {field: digest(canonical({name: b[field] for name, b in sorted(bindings.items())}))
            for field in ('ir_digest', 'chain_record_digest', 'bundle_digest')}


def pilot_statistics(pairs, protocol, family_size):
    """Fixed-sample, simultaneous median intervals, never search fitness.

    Independence/exchangeability of paired deltas is an assumption, not an
    assertion that host interference has been removed. Unknown hardware
    counters and wide intervals cannot become no-effect claims.
    """
    natural(family_size)
    require(family_size > 0, 'empty comparison family')
    result = summarize(pairs)
    delta = sorted(row['A1']-row['A0'] for row in pairs)
    n = len(delta)
    ranks = [k for k in range(1, n//2+1)
             if 40*family_size*sum(math.comb(n, j) for j in range(k)) <= 2**n]
    interval = [delta[ranks[-1]-1], delta[n-ranks[-1]]] if ranks else None
    scale = statistics.median(row['A0'] for row in pairs)
    percent = [100*x/scale for x in interval] if interval else None
    precise = percent is not None and (percent[1]-percent[0])/2 <= protocol['precision_percent']
    result.update(simultaneous_median_delta_ci95_ns=interval,
        ci_percent_of_control_median=percent, comparison_family=family_size,
        precision_target_met=precise, inference='PILOT_CHARACTERIZATION_NO_AI_OR_FINAL_Q2_CLAIM',
        calibration='WITHIN_EFFECT_FLOOR' if precise and
        all(abs(x) <= protocol['effect_floor_percent'] for x in percent) else 'INCONCLUSIVE')
    return result


def build_case(case, product, work, m, root, tools, command, guest, guest_path, pin, save):
    includes = [root/ROOTS['sysroot']/p for p in
                ('usr/lib/gcc/aarch64-linux-gnu/14/include', 'usr/include/aarch64-linux-gnu', 'usr/include')]
    folder = root/case['source_root']
    app_includes = [folder, folder/'lib'] if case['id'] != 'p0' else []
    flags = ['-std=gnu11', '-O2', '-fPIC', *case['flags']]
    host_flags = [*flags, '-nostdinc', *['-I'+str(p) for p in app_includes+includes]]
    guest_flags = [*flags, *['-I'+guest_path(p) for p in app_includes]]
    reference_objects = {}
    subject_objects = {variant: {profile: [] for profile in ('checked', 'unchecked')} for variant in ('A', 'B')}
    bindings = {variant: {} for variant in ('A', 'B')}
    frozen_bundles = []
    product['coverage'] = dict(subject_sources=case['subject_sources'],
        reference_sources=[s for s in case['sources'] if s not in case['subject_sources']],
        reference_harness=None if case['id'] == 'p0' else case['harness'],
        runtime_libraries='sealed-glibc/libm/GCC-runtime-reference-not-EvoCC')
    if case['id'] == 'p0':
        expected = dict(exit_code=0, stdout_sha256=digest(b'0\n'), stderr_sha256=digest(b''))
        for mode in ('--interpret-c-hir', '--interpret-evoir'):
            command(case['id']+':'+mode, [tools['compiler'], mode, *host_flags,
                root/case['sources'][0]], expected)
    for index, name in enumerate(case['sources']):
        source = folder/name
        reference = work/f'ref-{index:02}.o'
        guest(case['id']+':reference:'+name,
              ['/usr/bin/gcc-14', *guest_flags, '-c', guest_path(source), '-o', guest_path(reference)])
        pin(reference)
        reference_objects[name] = reference
        if name not in case['subject_sources']:
            continue
        unit = next(u for u in product['units'] if u['source'] == name)
        unit['status'] = 'STARTED'
        for variant in ('A', 'B'):
            chain = work/f'{index:02}-{variant}-chain'
            optimizer = ([] if variant == 'A' else ['-Oeq-scalar', '--eq-checker='+str(tools['checker']),
                '--eq-checker-sha256='+m['tools']['checker']['sha256'], '--eq-checker-timeout-ms=300000'])
            row = command(case['id']+':frontend:'+name+':'+variant,
                [tools['compiler'], '--emit=evoir', *host_flags, *optimizer,
                 '--eq-proof-dir='+str(chain), source])
            replay_started = time.monotonic_ns()
            binding = bind_bundle(chain, tools['checker'] if variant == 'B' else None)
            replay_wall = time.monotonic_ns() - replay_started
            require((chain/'output.evo').read_bytes() == (work.parent/row['stdout']).read_bytes(),
                    'frontend emitted different IR than the bound bundle')
            for artifact in chain.iterdir():
                pin(artifact)
            frozen_bundles.append((chain, binding))
            bindings[variant][name] = binding
            record = json.loads((chain/'chain.json').read_bytes(), object_pairs_hook=unique)
            detail = dict(binding=binding, native={}, frontend_stdout=row['stdout'],
                          admission_replay_wall_ns=replay_wall)
            # The census is the first application opportunity output, not a speedup.
            if variant == 'B':
                detail['default_caps'] = chain_census(record)
                detail['chain_resources'] = json.loads((chain/'resources.json').read_bytes(), object_pairs_hook=unique)
                require(record['input_digest'] == digest((work/f'{index:02}-A-chain/output.evo').read_bytes()),
                        'A/B source lowering differs')
            unit['variants'][variant] = detail
            for profile in ('checked', 'unchecked'):
                assembly_row = command(case['id']+':backend:'+name+':'+variant+':'+profile,
                    [tools['ir_backend'], 'linux-aarch64', binding['ir_digest'], chain/'output.evo',
                     '--ub-checks='+profile])
                assembly = work.parent/assembly_row['stdout']
                raw_asm = assembly.read_bytes()
                require(raw_asm.startswith(('// evocc-native-ub-checks='+profile+'\n').encode()),
                        'backend did not consume the requested profile')
                pin(assembly)
                obj = work/f'{index:02}-{variant}-{profile}.o'
                guest(case['id']+':assemble:'+name+':'+variant+':'+profile,
                    ['/usr/bin/gcc-14', '-c', '-x', 'assembler', guest_path(assembly), '-o', guest_path(obj)])
                pin(obj)
                subject_objects[variant][profile].append(obj)
                detail['native'][profile] = dict(object=obj.name, metrics=inspect_elf(obj.read_bytes()),
                    assembly_digest=digest(raw_asm), native_ub_profile=profile,
                    ub_brk_sites=len(re.findall(rb'\bbrk #(?:300[2456]|3063)\b', raw_asm)))
            save()
        unit['status'] = 'OBSERVED'

    dependencies = [reference_objects[s] for s in case['sources'] if s not in case['subject_sources']]
    if case['id'] != 'p0':
        harness = work/'harness.o'
        harness_flags = guest_flags + (['-DUSE_ZSTD'] if case['id'] == 'zstd' else [])
        guest(case['id']+':harness', ['/usr/bin/gcc-14', *harness_flags,
            '-c', guest_path(root/case['harness']), '-o', guest_path(harness)])
        pin(harness)
        dependencies.append(harness)
    reference = work/'reference'
    reference_link = list(reference_objects.values()) + ([] if case['id'] == 'p0' else [work/'harness.o'])
    guest(case['id']+':reference-link', ['/usr/bin/gcc-14', *map(guest_path, reference_link),
          '-Wl,--build-id=none', '-lm', '-o', guest_path(reference)])
    pin(reference)
    programs = {}
    for variant in ('A', 'B'):
        product['variants'][variant] = {}
        binding = composite_binding(bindings[variant])
        for profile in ('checked', 'unchecked'):
            executable = work/f'{variant}-{profile}'
            guest(case['id']+':link:'+variant+':'+profile, ['/usr/bin/gcc-14',
                *map(guest_path, subject_objects[variant][profile]+dependencies),
                '-Wl,--build-id=none', '-lm', '-o', guest_path(executable)])
            identity = pin(executable)
            stripped = work/f'{variant}-{profile}.stripped'
            guest(case['id']+':strip:'+variant+':'+profile,
                ['/usr/bin/strip', '--strip-all', '-o', guest_path(stripped), guest_path(executable)])
            pin(stripped)
            product['variants'][variant][profile] = dict(executable=executable.name,
                executable_digest=identity, metrics=inspect_elf(executable.read_bytes()),
                stripped_file_bytes=stripped.stat().st_size, inputs=[])
            programs[variant, profile] = executable, identity, binding

    # Check every input before any unchecked execution or timed sample.
    for mode in case['modes']:
        guest(case['id']+':reference-result:'+str(mode), [guest_path(reference), *mode], case['result'])
        for variant in ('A', 'B'):
            input_digest = digest(canonical(dict(harness=m['files'][case['harness']], argv=mode)))
            twin_ids = {profile: native_identity(programs[variant, profile][2], profile, 'linux-aarch64',
                m['tools']['ir_backend']['sha256'], programs[variant, profile][1], input_digest)
                for profile in ('checked', 'unchecked')}
            checked = guest(case['id']+':checked-admission:'+variant+':'+str(mode),
                [guest_path(programs[variant, 'checked'][0]), *mode], case['result'])
            checked['native_identity'] = twin_ids['checked']['native_identity']
            admission = admit_unchecked(twin_ids['checked'], twin_ids['unchecked'], checked, case['result'])
            unchecked = guest(case['id']+':unchecked-admission:'+variant+':'+str(mode),
                [guest_path(programs[variant, 'unchecked'][0]), *mode], case['result'])
            unchecked['native_identity'] = twin_ids['unchecked']['native_identity']
            for profile in ('checked', 'unchecked'):
                product['variants'][variant][profile]['inputs'].append(dict(identity=twin_ids[profile],
                    checked_twin=admission, argv=mode))
        save()
    for directory, binding in frozen_bundles:
        stable_bundle(directory, binding)


def measure_case(case, product, work, m, guest, guest_path, save):
    # Admission/build and the first cap table precede every timed observation.
    # Search evaluations never use these native times as adaptive fitness.
    for mode in case['modes']:
        for profile in ('checked', 'unchecked'):
            for comparison in m['protocol']['comparisons']:
                selected = {'A0': 'A', 'A1': 'A' if comparison == 'AA' else 'B'}
                row = dict(profile=profile, kind=comparison, argv=mode, samples=[], pairs=[])
                product['comparisons'].append(row)
                for repeat in range(m['protocol']['warmups']):
                    for label, variant in selected.items():
                        program = product['variants'][variant][profile]
                        guest(case['id']+f':warmup:{profile}:{comparison}:{repeat}:{label}:'+str(mode),
                            [guest_path(work/program['executable']), *mode], case['result'])
                for pair, order in enumerate(pair_order(m['protocol']['pairs'], m['protocol']['seed'])):
                    values = {}
                    for label in order:
                        variant = selected[label]
                        program = product['variants'][variant][profile]
                        measured = guest(case['id']+f':sample:{profile}:{comparison}:{pair}:{label}:'+str(mode),
                            [guest_path(work/program['executable']), *mode], case['result'])
                        require(measured['guest_executable_digest'] == program['executable_digest'],
                                'timed wrong executable')
                        row['samples'].append(dict(pair=pair, label=label, variant=variant,
                            guest_receipt=measured['guest_receipt'], guest_wall_ns=measured['guest_wall_ns'],
                            guest_resources=measured['guest_resources'], executable_digest=program['executable_digest']))
                        values[label] = measured['guest_wall_ns']
                    row['pairs'].append(values)
                    save()
                row['statistics'] = pilot_statistics(row['pairs'], m['protocol'],
                    sum(len(c['modes'])*4 for c in m['cases']))


def run_pilot(path, root):
    root = Path(root).resolve()
    m, raw = audit_plan(path, root)
    tools = {k: (root/v['path']).resolve() for k, v in m['tools'].items()}
    base = root/'.evidence/eqsat-pilot'
    base.mkdir(parents=True, exist_ok=True)
    with (root/'.build/bench.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError('another benchmark owns the lock') from error
        require(shutil.disk_usage(root).free >= m['resources']['min_free_bytes'], 'disk reserve unavailable')
        directory = Path(tempfile.mkdtemp(prefix='pilot-', dir=base))
        (directory/'manifest.json').write_bytes(raw)
        receipt = dict(schema='evocc-r02-linux-pilot-receipt-v1', status='STARTED',
            manifest_digest=digest(raw), implementation_commit=m['implementation_commit'],
            attempts=[], cases=[], host=m['host'], host_load_start=list(os.getloadavg()),
            scope='DECLARED_MIXED_LINK_LINUX_HARDWARE_VM', performance='PILOT_NOT_FINAL_Q2',
            controller_timing='NOT_PRODUCT_TIME', interference='HOST_AFFINITY_UNCONTROLLED')
        def save():
            pending = directory/'receipt.json.pending'
            pending.write_text(json.dumps(receipt, sort_keys=True, indent=2)+'\n')
            pending.replace(directory/'receipt.json')
        resources = {k: v for k, v in m['resources'].items() if k not in ('campaign_seconds', 'vm_cpus', 'vm_memory')}
        env = dict(PATH='/usr/bin:/bin:/usr/local/bin', LANG='C', LC_ALL='C', TMPDIR=str(directory))
        runner = Capture(directory, resources, env, receipt, save, measure_resources=True)
        started = time.monotonic()
        vm = 'evocc-r02-'+directory.name
        vm_started = False
        pinned = {str((root/v['path']).resolve()): v['sha256'] for v in m['tools'].values()}
        pinned.update({str(root/k): v for k, v in m['files'].items()})

        def stable():
            require(time.monotonic()-started < m['resources']['campaign_seconds'], 'campaign watchdog expired')
            require(Path(path).read_bytes() == raw, 'frozen plan changed')
            for name, expected in pinned.items():
                require(digest(Path(name).read_bytes()) == expected, 'artifact changed: '+name)
            runner.watch()

        def pin(file):
            identity = digest(Path(file).read_bytes())
            pinned[str(Path(file).resolve())] = identity
            return identity

        def command(stage, argv, expected=None):
            stable()
            row = runner.run(stage, argv, expected)
            stable()
            return row

        def guest_path(file):
            file = Path(file).resolve()
            if file.is_relative_to(directory):
                return '/runwork/'+file.relative_to(directory).as_posix()
            if file.is_relative_to(root):
                return '/work/'+file.relative_to(root).as_posix()
            raise ValueError('unmounted guest path')

        def guest(stage, argv, expected=None):
            index = len(receipt['attempts'])
            measurement = directory/f'guest-{index:03}'
            row = command(stage, [tools['container'], 'exec', vm, '/usr/bin/python3', '-B', '-m',
                'tools.ladder.eqsat_guest', '--directory', guest_path(measurement),
                '--timeout', str(resources['timeout_seconds']-5), '--output-bytes', str(resources['output_bytes']),
                '--reserve', str(resources['min_free_bytes']), '--', *argv], expected)
            capture = json.loads((measurement/'receipt.json').read_bytes(), object_pairs_hook=unique)
            require(capture['status'] == 'OBSERVED' and len(capture['attempts']) == 1, 'guest did not finish')
            measured = capture['attempts'][0]
            require(measured['argv'] == argv and all(measured[k] == row[k] for k in
                    ('exit_code', 'stdout_sha256', 'stderr_sha256')), 'guest result/command mismatch')
            row.update(guest_receipt=measurement.name+'/receipt.json',
                       guest_receipt_digest=pin(measurement/'receipt.json'), guest_wall_ns=measured['wall_ns'],
                       guest_resources=measured['resources'], guest_executable_digest=capture['executable_digest'])
            save()
            return row

        save()
        try:
            image_row = command('image-identity', [tools['container'], 'image', 'inspect', m['image']['name']])
            image = json.loads((directory/image_row['stdout']).read_bytes())
            require(image[0]['id'] == m['image']['digest'], 'sealed image changed')
            vm_started = True  # Also attempt cleanup if startup is interrupted.
            command('start-owned-vm', [tools['container'], 'run', '--rm', '-d', '--name', vm,
                '--cpus', str(m['resources']['vm_cpus']), '--memory', m['resources']['vm_memory'],
                '--read-only', '--tmpfs', '/tmp', '--no-dns',
                '--mount', f'type=bind,source={root},target=/work,readonly',
                '--mount', f'type=bind,source={directory},target=/runwork',
                m['image']['name'], '/bin/sleep', str(m['resources']['campaign_seconds'])])
            observed = command('guest-tool-identities', [tools['container'], 'exec', vm,
                '/usr/bin/sha256sum', *GUEST_TOOLS])
            hashes = {line.split()[1]: line.split()[0] for line in
                      (directory/observed['stdout']).read_text().splitlines()}
            require(hashes == m['guest_tools'], 'guest tool/runtime changed')
            for case in m['cases']:
                product = dict(id=case['id'], status='STARTED',
                    units=[dict(source=s, status='NOT_RUN', variants={}) for s in case['subject_sources']],
                    variants={}, comparisons=[])
                receipt['cases'].append(product)
                work = directory/case['id']
                work.mkdir()
                try:
                    build_case(case, product, work, m, root, tools, command, guest, guest_path, pin, save)
                    product['status'] = 'OBSERVED'
                except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
                    product.update(status='FAILED', error=str(error))
                    for unit in product['units']:
                        if unit['status'] == 'STARTED':
                            unit.update(status='FAILED', error=str(error))
                save()
            if all(p['status'] == 'OBSERVED' for p in receipt['cases']):
                from .eqsat_preflight_run import run_preflight
                run_preflight(m, receipt, directory, root, tools, command, guest, guest_path, pin, save)
                for case, product in zip(m['cases'], receipt['cases']):
                    measure_case(case, product, directory/case['id'], m, guest, guest_path, save)
            stable()
            # Recheck full source/header inputs once, not once per timed sample.
            audit_plan(path, root)
            receipt['status'] = 'OBSERVED' if all(p['status'] == 'OBSERVED' for p in receipt['cases']) else 'INCOMPLETE'
        except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
            receipt.update(status='FAILED', error=str(error))
        except BaseException:
            receipt['status'] = 'INTERRUPTED'
            raise
        finally:
            if vm_started:
                # Only the new, uniquely named container owned by this attempt.
                try:
                    cleanup = subprocess.run([str(tools['container']), 'stop', vm],
                        capture_output=True, timeout=30, check=False)
                    receipt['cleanup'] = dict(exit_code=cleanup.returncode,
                        stdout=cleanup.stdout.decode(errors='backslashreplace'),
                        stderr=cleanup.stderr.decode(errors='backslashreplace'))
                    if cleanup.returncode != 0:
                        receipt['status'] = 'FAILED'
                except (OSError, subprocess.SubprocessError) as error:
                    receipt['cleanup'] = dict(error=str(error))
                    receipt['status'] = 'FAILED'
            receipt['host_load_end'] = list(os.getloadavg())
            save()
        return directory, receipt
