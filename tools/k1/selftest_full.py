#!/usr/bin/env python3
"""Build every upstream kselftest collection and run installed tests in isolated VMs."""

from __future__ import annotations

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import tarfile
import time

ROOT = Path(__file__).resolve().parents[2]
RUNTIME_PACKAGES = ('procps', 'psmisc', 'udev', 'tcpdump', 'fio', 'kmod', 'dosfstools', 'sudo',
                    'e2fsprogs', 'libarchive13t64', 'netsniff-ng', 'libteam-utils',
                    'iperf3', 'traceroute', 'conntrack', 'fuse3', 'bpftrace', 'keyutils', 'fsverity',
                    'netcat-openbsd')
BUILD_PACKAGES = ('openssl', 'python3-docutils', 'xxd')
# 6.15 is the first release with --json --show-channels, required by the
# unchanged napi_threaded selftest. This is userspace test infrastructure;
# it does not change the sealed compiler image or either kernel.
RUNTIME_ETHTOOL_URL = 'https://www.kernel.org/pub/software/network/ethtool/ethtool-6.15.tar.xz'
RUNTIME_ETHTOOL_SHA256 = '9477c365114d910120aaec5336a1d16196c833d8486f7c6da67bedef57880ade'
# The source's own TEST_F_TIMEOUT is 1800 and its workload sleeps for 900s.
# The collection has no settings file, so the runner's 45s default cannot
# possibly finish it. Keep this measured policy change explicit in receipts.
PROGRAM_TIMEOUT_OVERRIDES = {
    'filesystems:file_stressor': {
        'seconds': 1800,
        'source': 'tools/testing/selftests/filesystems/file_stressor.c',
        'reason': 'TEST_F_TIMEOUT(..., 900 * 2); clock_nanosleep(..., 900)',
    },
    'ublk:test_stress_05.sh': {
        'seconds': 600,
        'source': 'tools/testing/selftests/ublk/test_stress_05.sh',
        'reason': 'Sequential recovery-mode phases launch fio --runtime=40 and wait for device removal; '
                  'both kernels hit the inherited 45-second limit mid-workload. '
                  '600 seconds is an explicit measurement budget, not an upstream declared timeout.',
    },
    'rseq:run_param_test.sh': {
        'seconds': 7200,
        'source': 'tools/testing/selftests/rseq/run_param_test.sh and rseq/settings',
        'reason': 'The upstream collection declares timeout=0. The unchanged script runs 1184 '
                  'invocations including injected yields, signals and sleeps; the 300-second '
                  'unlimited-test fallback interrupted it. 7200 seconds is an explicit '
                  'measurement budget, not a change to the test or its assertions.',
    },
    'cgroup:test_cpu': {
        'seconds': 120,
        'source': 'tools/testing/selftests/cgroup/test_cpu.c',
        'reason': 'The serial source workload requires at least 46 seconds: two 2-second '
                  'CPU-time tests, four 10-second wall-time weight tests and two 1-second '
                  'bandwidth tests. The collection default is only 45 seconds. 120 seconds '
                  'is an explicit whole-program budget; all nine original assertions remain.',
    },
    'sched_ext:runner': {
        'seconds': 120,
        'source': 'tools/testing/selftests/sched_ext/runner.c and reload_loop.c',
        'reason': 'The 23-test serial suite includes CPU hotplug and two 1024-iteration '
                  'scheduler attach/detach loops. The fixed TCG control completed its '
                  'assertions just after the inherited 45-second deadline. 120 seconds '
                  'is an explicit program budget, not an upstream timeout or changed test.',
    },
}
PROGRAM_BOOT_LANES = {
    'kvm:hello_el2': {
        'lane': 'tcg-neoverse-n2-el2',
        'contract': 'contracts/kernel-selftest-kvm-target.json',
        'kvmMode': 'nested',
        'reason': 'The frozen kernel exposes KVM_CAP_ARM_EL2 only in its explicit nested '
                  'mode on FEAT_NV2 hardware. The original test passes on Neoverse-N2 '
                  'with kvm-arm.mode=nested; apply both prerequisites to both Images.',
    },
    'kvm:aarch32_id_regs': {
        'lane': 'tcg-a64fx-el2',
        'contract': 'contracts/kernel-selftest-kvm-target.json',
        'reason': 'The original test requires no AArch32 EL0 support, not merely '
                  'an AArch64 kernel. Its RAZ/WI assertions pass on fixed A64FX; '
                  'use that CPU on both Images without changing the test or timeout.',
    },
    'kvm:host_sve': {
        'lane': 'tcg-neoverse-n2-el2',
        'contract': 'contracts/kernel-selftest-kvm-target.json',
        'reason': 'Cortex-A72 lacks the SVE capability required by the original test. '
                  'The unchanged reference program passes on fixed Neoverse-N2; '
                  'use that CPU on both Images without changing arguments or timeout.',
    },
    'kvm:demand_paging_test': {
        'lane': 'tcg-neoverse-n2-el2',
        'contract': 'contracts/kernel-selftest-kvm-target.json',
        'reason': 'The original reference workload completes on fixed Neoverse-N2 '
                  'within its unchanged 120-second limit; the Cortex-A72 control times out. '
                  'Use the same CPU on both Images, preserving every guest mode and argument. '
                  'This is a fixed-machine result, not a host-independent performance claim.',
    },
    'kvm:arch_timer': {
        'lane': 'tcg-cortex-a72-el2-icount',
        'contract': 'contracts/kernel-selftest-kvm-target.json',
        'reason': 'The original 100-us timer margin fails with wall-clock TCG scheduling; '
                  'all four reference vCPUs pass with instruction-clock execution. '
                  'Keep arguments, assertions and timeout; this tests virtual timer semantics, '
                  'not physical interrupt-delivery latency. Both Images use the same lane.',
    },
    'kvm': {
        'lane': 'tcg-cortex-a72-el2',
        'contract': 'contracts/kernel-selftest-kvm-target.json',
        'reason': 'The ordinary virt guest starts without EL2 and cannot expose /dev/kvm. '
                  'Use the separate fixed-CPU EL2/GICv3 machine with 4 GiB on both sides '
                  '(mmu_stress_test pre-faults 2 GiB before kernel/page-table overhead); '
                  'keep the original Image, program arguments and timeout.',
    },
    'sched_ext:runner': {
        'lane': 'tcg-cortex-a72',
        'reason': 'The unchanged hotplug test reaches cpu_die BUG on HVF; its fixed-CPU '
                  'TCG control completes hotplug. Both compilers use the same declared lane.',
    },
}


def program_timeout(build, ident):
    override = PROGRAM_TIMEOUT_OVERRIDES.get(ident)
    return override['seconds'] + 15 if override else build['programTimeoutSeconds']


def program_environment(ident, expanded):
    environment = {'K1_SELFTEST_COLLECTION': ident}
    if expanded:
        environment['K1_KUNIT_AUTORUN'] = '0'
    if override := PROGRAM_TIMEOUT_OVERRIDES.get(ident):
        environment['K1_SELFTEST_TIMEOUT'] = str(override['seconds'])
    if profile := (PROGRAM_BOOT_LANES.get(ident)
                   or PROGRAM_BOOT_LANES.get(ident.split(':', 1)[0])):
        environment['K1_BOOT_LANE'] = profile['lane']
        if profile.get('contract'):
            environment['K1_KERNEL_CONTRACT'] = str(ROOT / profile['contract'])
        if profile.get('kvmMode'):
            environment['K1_KVM_MODE'] = profile['kvmMode']
    return environment


def file_sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def boot_policies():
    result = {name: dict(profile) for name, profile in PROGRAM_BOOT_LANES.items()}
    for profile in result.values():
        if profile.get('contract'):
            profile['contractSha256'] = file_sha256(ROOT / profile['contract'])
    return result


def configuration_gaps(source, config, builds):
    """Report literal upstream config differences, never waive a runtime result."""
    def values(path):
        result = {}
        for line in path.read_text().splitlines():
            if match := re.fullmatch(r'(CONFIG_\w+)=(.*)', line.strip()):
                result[match[1]] = match[2]
            elif match := re.fullmatch(r'# (CONFIG_\w+) is not set', line.strip()):
                result[match[1]] = 'n'
        return result

    effective = values(config)
    result = {'configSha256': file_sha256(config), 'collections': [], 'missingFragments': [],
              'architectureFragments': [],
              'scope': 'Literal per-collection config and arm64 config.aarch64 fragments, '
                       'reported separately without hiding conflicting requests. '
                       'Kconfig dependency solving and unlisted prerequisites need separate review. '
                       'Differences are evidence, not test outcomes or inapplicability decisions.'}

    def differences(fragment):
        differences = []
        for option, requested in values(fragment).items():
            actual = effective.get(option, 'n')
            if actual == requested:
                continue
            kind = ('disabled' if requested in ('y', 'm') and actual == 'n' else
                    'module' if requested == 'y' and actual == 'm' else
                    'built-in' if requested == 'm' and actual == 'y' else 'different')
            differences.append({'option': option, 'requested': requested, 'actual': actual, 'kind': kind})
        return differences

    for build in builds:
        directory = source / 'tools/testing/selftests' / build['id']
        fragment = directory / 'config'
        if fragment.is_file():
            result['collections'].append({'id': build['id'],
                                          'fragmentSha256': file_sha256(fragment),
                                          'differences': differences(fragment)})
        else:
            result['missingFragments'].append(build['id'])
        arch_fragment = directory / 'config.aarch64'
        if effective.get('CONFIG_ARM64') == 'y' and arch_fragment.is_file():
            result['architectureFragments'].append({
                'id': build['id'], 'path': str(arch_fragment.relative_to(source)),
                'fragmentSha256': file_sha256(arch_fragment),
                'differences': differences(arch_fragment)})
    return result


def check_module_runtime(folder, image, module_link):
    """Bind installed modules to the actual Image and immutable disk payload."""
    build = json.loads((folder / 'build.json').read_text())
    modules = build.get('kernelModules') or {}
    if modules.get('status') != 'PASS' or modules.get('imageSha256') != file_sha256(image):
        raise ValueError('module runtime Image does not match its installation')
    if build.get('rootfsSha256') != file_sha256(folder / 'scratch.img'):
        raise ValueError('module runtime filesystem changed after installation')
    if any(row['id'] in ('bpf', 'livepatch') and row['available']
           for row in build.get('builds', [])):
        helpers = build.get('kernelTestModules')
        # The original build record binds its initial test helpers. A refreshed
        # subject payload instead needs their explicit per-kernel rebuild.
        identity = (helpers.get('kernelSha256', {}) if helpers else
                    build.get('kernelBuild', {}).get('sha256', {}))
        expected = {'.config': modules.get('configSha256'),
                    'vmlinux': modules.get('vmlinuxSha256'),
                    'Module.symvers': modules.get('moduleSymversSha256')}
        if ((helpers and helpers.get('status') != 'PASS')
                or not all(value and identity.get(name) == value for name, value in expected.items())):
            raise ValueError('test module helpers do not match the runtime kernel')
    if module_link is not None:
        rows = modules.get('modules', [])
        actual = {row['unit']: row['linkedSha256'] for row in rows}
        expected = {row['unit']: row['sha256'] for row in module_link['modules']}
        if len(actual) != len(rows) or actual != expected:
            raise ValueError('runtime linked module inventory differs from the composition')
    return modules


def check_composed_modules(data, reference, subject):
    """A full kernel receipt includes the actual module links, not old .ko files."""
    order = reference / 'modules.order'
    expected = ([str(Path(name).with_suffix('.ko')) for name in order.read_text().splitlines()]
                if order.is_file() else [])
    link = data.get('moduleLink') or {}
    if not expected and not link.get('required'):
        return
    if (not link.get('required') or link.get('status') != 'PASS'
            or not link.get('inputHashesUnchanged')
            or link.get('btfBaseSha256') != data['vmlinuxSha256']
            or link.get('configSha256') != data['configSha256']):
        raise ValueError('full composition module links are incomplete')
    modules = link.get('modules', [])
    names = [item['unit'] for item in modules]
    if len(set(names)) != len(names) or sorted(names) != sorted(expected):
        raise ValueError('full composition module denominator changed')
    for item in modules:
        name = Path(item['unit'])
        module = (subject / name).resolve()
        if name.is_absolute() or not module.is_relative_to(subject.resolve()):
            raise ValueError('full composition module path escapes its output')
        digest = hashlib.sha256()
        with module.open('rb') as handle:
            for chunk in iter(lambda: handle.read(1 << 20), b''):
                digest.update(chunk)
        if digest.hexdigest() != item['sha256']:
            raise ValueError(f'full composition module changed: {name}')


def kernel_selection(expanded, root=ROOT, *, receipt_path=None, reference_boot_path=None):
    """Select an already booted pair without confusing test FAIL with build FAIL."""
    def digest(path):
        value = hashlib.sha256()
        with path.open('rb') as handle:
            for block in iter(lambda: handle.read(1 << 20), b''):
                value.update(block)
        return value.hexdigest()

    if receipt_path is not None and not expanded:
        raise ValueError('a supplemental receipt requires the expanded composition checks')
    receipt = (root / receipt_path if receipt_path is not None else
               root / ('tests/contracts/kernel-kunit-receipt.json' if expanded
                       else 'tests/contracts/kernel-full-receipt.json'))
    data = json.loads(receipt.read_text())
    if expanded and data.get('mode') == 'full-kernel':
        if reference_boot_path is None:
            raise ValueError('the supplemental full composition requires its reference boot receipt')
        preparation = data.get('preparation') or {}
        export = preparation.get('exportTable', {})
        total = data.get('requestedUnits', 0)
        if (data.get('status') != 'PASS' or not data.get('completeComposition')
                or not data.get('installed') or data.get('relinkExitCode') != 0
                or not total or data.get('counts') != {'substituted': total}
                or preparation.get('prepareExitCode') != 0
                or preparation.get('verifyPrepareExitCode') != 0
                or not preparation.get('preparedInputsStable')
                or preparation.get('failedUnits') != [] or not export.get('ready')
                or (export.get('required') and not export.get('unchangedAfterLink'))):
            raise ValueError('supplemental full composition is incomplete')
        outputs = {'reference': root / data['referenceRoot'], 'subject': root / data['outputRoot']}
        check_composed_modules(data, outputs['reference'], outputs['subject'])
        records = root / data['evidence'] / 'units.jsonl'
        if digest(records) != data['unitsSha256']:
            raise ValueError('full composition inventory changed')
        rows = [json.loads(line) for line in records.read_text().splitlines()]
        units = {row['unit'] for row in rows}
        if (len(rows) != total or len(units) != total
                or sorted(units) != sorted(data['substitutedUnits'])):
            raise ValueError('full composition denominator changed')
        for row in rows:
            unit = Path(row['unit'])
            installed = (outputs['subject'] / unit).resolve()
            if (unit.is_absolute() or not installed.is_relative_to(outputs['subject'].resolve())
                    or row['outcome'] != 'substituted'
                    or row['objectSha256AfterRelink'] != row['installedSha256']
                    or digest(installed) != row['installedSha256']):
                raise ValueError(f'full composition installed object changed: {unit}')
        if digest(outputs['subject'] / 'vmlinux') != data['vmlinuxSha256']:
            raise ValueError('full composition vmlinux changed')
        reference_boot = root / reference_boot_path
        boots = {'reference': json.loads(reference_boot.read_text()), 'subject': data['boot']}
        images = {label: out / 'arch/arm64/boot/Image' for label, out in outputs.items()}
        configs = {label: out / '.config' for label, out in outputs.items()}
        for label, boot in boots.items():
            if (boot.get('verdict') != 'PASS' or digest(images[label]) != boot['imageSha256']
                    or (root / boot['image']).resolve() != images[label].resolve()
                    or digest(root / boot['serialLog']) != boot['serialLogSha256']):
                raise ValueError(f'{label} full-composition boot evidence changed or incomplete')
            if digest(configs[label]) != data['configSha256']:
                raise ValueError(f'{label} full-composition configuration changed')
        if boots['subject']['imageSha256'] != data['imageSha256']:
            raise ValueError('boot did not execute the composed subject Image')
        return {'receipt': receipt, 'referenceBoot': reference_boot, 'images': images,
                'configs': configs, 'moduleLink': data.get('moduleLink', {})}
    if reference_boot_path is not None:
        raise ValueError('a separate reference boot is only used with a supplemental full composition')
    if expanded:
        outputs = ({label: root / data['outputs'][label] for label in ('reference', 'subject')}
                   if receipt_path is not None else
                   {'reference': root / '.build/k7/pristine', 'subject': root / '.build/k7/out'})
        composition = data['builtInComposition']
        if (composition['linkExitCode'] or composition['gccFallback']
                or not composition['generatedInputsStable'] or not composition['generatedExportTableStable']
                or not composition['targetCBuiltIns']
                or composition['installedAndHashVerified'] != composition['targetCBuiltIns']):
            raise ValueError('expanded built-in composition is incomplete')
        records = root / composition['capture'] / composition['unitsFile']
        if digest(records) != composition['unitsSha256']:
            raise ValueError('expanded composition inventory changed')
        rows = [json.loads(line) for line in records.read_text().splitlines()]
        if (len(rows) != composition['targetCBuiltIns']
                or len({r['unit'] for r in rows}) != len(rows)):
            raise ValueError('expanded composition denominator changed')
        for row in rows:
            unit = Path(row['unit'])
            installed = (outputs['subject'] / unit).resolve()
            if (unit.is_absolute() or not installed.is_relative_to(outputs['subject'].resolve())
                    or row['outcome'] != 'substituted' or digest(installed) != row['installedSha256']):
                raise ValueError(f'expanded installed object changed: {unit}')
        expected = {'reference': data['reference']['imageSha256'],
                    'subject': composition['imageSha256']}
        if (data['reference']['bootVerdict'] != 'PASS'
                or data['subject']['runtime']['bootVerdict'] != 'PASS'
                or data['subject']['runtime']['imageSha256'] != expected['subject']):
            raise ValueError('expanded images have not completed boot')
        images = {label: out / 'arch/arm64/boot/Image' for label, out in outputs.items()}
    else:
        outputs = {'reference': root / '.build/reference/gcc-lane-1', 'subject': root / '.build/k1/out'}
        images = {'reference': outputs['reference'] / 'arch/arm64/boot/Image',
                  'subject': root / data['boot']['image']}
        expected = {'subject': data['imageSha256']}
        if data['status'] != 'PASS' or not data['completeComposition']:
            raise ValueError('unchanged complete booted kernel required')
    for label, value in expected.items():
        if digest(images[label]) != value:
            raise ValueError(f'{label} Image changed')
    configs = {label: out / '.config' for label, out in outputs.items()}
    if digest(configs['reference']) != digest(configs['subject']):
        raise ValueError('reference and subject kernel configurations differ')
    if expanded and digest(configs['subject']) != data['config']['sha256']:
        raise ValueError('expanded configuration no longer matches its boot receipt')
    return {'receipt': receipt, 'images': images, 'configs': configs}


def prepare_runtime_ethtool(work):
    archive = work / 'runtime-tools/ethtool-6.15.tar.xz'
    archive.parent.mkdir(parents=True, exist_ok=True)
    if not archive.is_file():
        subprocess.run(['curl', '--fail', '--location', '--max-time', '60',
                        '--output', str(archive), RUNTIME_ETHTOOL_URL], check=True)
    if file_sha256(archive) != RUNTIME_ETHTOOL_SHA256:
        raise ValueError('runtime ethtool archive checksum mismatch')
    return archive


def install_runtime_ethtool(work, rootfs):
    archive = work / 'runtime-tools/ethtool-6.15.tar.xz'
    if not archive.is_file() or file_sha256(archive) != RUNTIME_ETHTOOL_SHA256:
        raise ValueError('runtime ethtool archive checksum missing or incorrect; use --install-deps')
    source = archive.parent / 'ethtool-6.15'
    commands = [['tar', '-xf', str(archive), '-C', str(archive.parent)],
                ['./configure', '--prefix=/usr', '--sbindir=/usr/sbin'], ['make', '-j1']]
    with (archive.parent / 'ethtool-build.log').open('w') as log:
        for index, command in enumerate(commands):
            subprocess.run(command, cwd=archive.parent if index == 0 else source,
                           env={**os.environ, 'CC': 'gcc-14'}, check=True, timeout=300,
                           stdout=log, stderr=subprocess.STDOUT)
    destination = rootfs / 'usr/sbin/ethtool'
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source / 'ethtool', destination)
    if subprocess.check_output([str(destination), '--version'], text=True).strip() != 'ethtool version 6.15':
        raise ValueError('unexpected installed runtime ethtool version')
    return {'version': '6.15', 'url': RUNTIME_ETHTOOL_URL,
            'archiveSha256': RUNTIME_ETHTOOL_SHA256, 'binarySha256': file_sha256(destination),
            'compiler': 'gcc-14', 'commands': commands,
            'role': 'unmodified upstream test utility; no EvoCC target-C compilation credit'}


def install_runtime_helpers(source, installed):
    """Upstream ublk scripts source helpers omitted by their install target."""
    shutil.copy2(source / 'ublk/test_common.sh', installed / 'ublk/test_common.sh')
    shutil.copytree(source / 'ublk/trace', installed / 'ublk/trace', dirs_exist_ok=True)
    helpers = ['ublk/test_common.sh', 'ublk/trace']
    # test_progs changes into the selected flavor directory. Its upstream
    # install rule copies *.bpf.o there but omits TRUNNER_EXTRA_FILES. Reuse
    # the installed helpers, especially the modules rebuilt for this kernel,
    # rather than copying stale .ko files from a reference build directory.
    bpf = installed / 'bpf'
    extras = [bpf / name for name in (
        'urandom_read', 'liburandom_read.so', 'xdp_synproxy', 'sign-file',
        'uprobe_multi', 'ima_setup.sh', 'verify_sig_setup.sh')]
    extras += sorted(bpf.glob('*.ko')) + sorted(bpf.glob('btf_dump_test_case_*.c'))
    for flavor in ('no_alu32', 'cpuv4', 'bpf_gcc'):
        directory = bpf / flavor
        if not directory.is_dir():
            continue
        for original in extras:
            if not original.is_file():
                continue
            alias = directory / original.name
            target = Path('..') / original.name
            if alias.exists() or alias.is_symlink():
                if not alias.is_symlink() or alias.readlink() != target:
                    raise ValueError(f'unexpected BPF flavor helper: {alias}')
            else:
                alias.symlink_to(target)
            helpers.append(str(alias.relative_to(installed)))
    return helpers


def install_runtime_module_prerequisites(rootfs, kernel_modules):
    # fuse creates /sys/fs/fuse/connections when it loads. Mount's pathname
    # lookup otherwise fails before filesystem autoload can request fs-fusectl.
    installed = {row['unit'] for row in (kernel_modules or {}).get('modules', [])}
    # Without a running udev daemon, opening a not-yet-created device node
    # cannot trigger module autoload. Only preload modules actually installed
    # for this Image; a disabled driver is still a configuration coverage gap.
    required = (('fs/fuse/fuse.ko', 'fuse'), ('sound/core/snd-timer.ko', 'snd_timer'),
                ('drivers/misc/ntsync.ko', 'ntsync'),
                ('drivers/iommu/iommufd/iommufd.ko', 'iommufd'))
    names = [name for unit, name in required if unit in installed]
    (rootfs / 'tests/modules').write_text(''.join(name + '\n' for name in names))
    return names


def install_kernel_modules(source, kernel, rootfs):
    """Run only original modinst/sign/depmod recipes; never build target C."""
    config = (kernel / '.config').read_text().splitlines()
    if 'CONFIG_MODULES=y' not in config:
        return None
    if 'CONFIG_MODULE_COMPRESS_ALL=y' in config:
        raise ValueError('compressed module installation needs an explicit body audit')
    release = (kernel / 'include/config/kernel.release').read_text().strip()
    if not re.fullmatch(r'[A-Za-z0-9_+.-]+', release) or release in ('.', '..'):
        raise ValueError('unsafe kernel release in module installation')
    rootfs = rootfs.resolve()
    destination = rootfs / 'usr/lib/modules' / release
    # Makefile.modinst removes its old kernel/ directory and build symlink.
    # Both must be confined to this generated rootfs, never a build/source tree.
    if (rootfs == Path('/') or rootfs in (kernel.resolve(), source.resolve())
            or not destination.resolve().is_relative_to(rootfs)
            or (destination / 'kernel').is_symlink()
            or ((destination / 'build').exists() and not (destination / 'build').is_symlink())):
        raise ValueError('unsafe module installation path')
    names = (kernel / 'modules.order').read_text().splitlines()
    modules = []
    for name in names:
        path = Path(name)
        if (path.is_absolute() or '..' in path.parts or path.suffix != '.o'
                or not (kernel / path.with_suffix('.ko')).resolve().is_relative_to(kernel.resolve())):
            raise ValueError('unsafe module inventory path')
        modules.append(str(path.with_suffix('.ko')))
    if len(set(modules)) != len(modules):
        raise ValueError('duplicate module installation entry')
    protected = {name: file_sha256(kernel / name) for name in (
        '.config', 'modules.order', 'Module.symvers', 'System.map', 'vmlinux',
        'arch/arm64/boot/Image', *modules)}
    env = {**os.environ, 'srctree': str(source), 'objtree': str(kernel),
           'INSTALL_MOD_PATH': str(rootfs), 'KERNELRELEASE': release}
    installed_process = subprocess.run(['make', '-f', str(source / 'scripts/Makefile.modinst'), '__modinst',
                    f'srctree={source}', f'objtree={kernel}', f'MODLIB={destination}',
                    f'INSTALL_MOD_PATH={rootfs}', f'KERNELRELEASE={release}',
                    'INSTALL_MOD_STRIP=', 'KBUILD_EXTMOD=', 'DEPMOD=depmod'],
                   cwd=kernel, env=env, check=True, timeout=600, stderr=subprocess.PIPE, text=True)
    diagnostics = installed_process.stderr or ''
    if diagnostics:
        print(diagnostics, file=sys.stderr, end='')
    # depmod can exit zero despite finding symbols which prevent actual load.
    # Keep that separate from a successful modinst/signature-body audit.
    if 'needs unknown symbol' in diagnostics or 'disagrees about version' in diagnostics:
        raise ValueError('unresolved module dependencies after installation')
    result = []
    magic = b'~Module signature appended~\n'
    for name in modules:
        linked = (kernel / name).read_bytes()
        installed = (destination / 'kernel' / name).read_bytes()
        if not installed.startswith(linked):
            raise ValueError(f'installed module body changed: {name}')
        tail = installed[len(linked):]
        if 'CONFIG_MODULE_SIG_ALL=y' in config:
            trailer = tail[-len(magic) - 12:-len(magic)]
            if (not tail.endswith(magic) or len(trailer) != 12 or trailer[2] != 2
                    or len(tail) != int.from_bytes(trailer[8:12], 'big') + 12 + len(magic)
                    or not int.from_bytes(trailer[8:12], 'big')):
                raise ValueError(f'installed module signature missing or malformed: {name}')
        elif tail:
            raise ValueError(f'unsigned module installation changed: {name}')
        result.append({'unit': name, 'linkedSha256': protected[name],
                       'installedSha256': hashlib.sha256(installed).hexdigest()})
    dependencies = (destination / 'modules.dep').read_text().splitlines()
    if ({line.split(':', 1)[0] for line in dependencies if line.startswith('kernel/')}
            != {'kernel/' + name for name in modules}
            or not (destination / 'modules.dep.bin').is_file()):
        raise ValueError('installed module dependency inventory is incomplete')
    if any(file_sha256(kernel / name) != digest for name, digest in protected.items()):
        raise ValueError('linked kernel inputs changed during module installation')
    return {'status': 'PASS', 'release': release, 'modules': result,
            'imageSha256': protected['arch/arm64/boot/Image'],
            'vmlinuxSha256': protected['vmlinux'], 'configSha256': protected['.config'],
            'moduleSymversSha256': protected['Module.symvers'],
            'signatures': ('original modinst signer; body and trailer checked, execution separate'
                           if 'CONFIG_MODULE_SIG_ALL=y' in config else
                           'unsigned installation by configuration; exact body checked, execution separate'),
            'scriptSha256': file_sha256(source / 'scripts/Makefile.modinst')}


def install_module_build_tree(source, kernel, rootfs):
    """Install the kernel's own external-module headers, not a dummy KDIR."""
    config = (kernel / '.config').read_bytes()
    if b'CONFIG_MODULES=y\n' not in config:
        return None
    release = (kernel / 'include/config/kernel.release').read_text().strip()
    if not re.fullmatch(r'[A-Za-z0-9_+.-]+', release) or release in ('.', '..'):
        raise ValueError('unsafe kernel release in module build installation')
    guest = Path('/usr/src') / f'linux-headers-{release}'
    destination = rootfs / guest.relative_to('/')
    link = rootfs / 'usr/lib/modules' / release / 'build'
    if link.exists() and not link.is_symlink():
        raise ValueError('module build path is not a generated symlink')
    script = source / 'scripts/package/install-extmod-build'
    symbols = (kernel / 'Module.symvers').read_bytes()
    env = {**os.environ, 'srctree': str(source), 'SRCARCH': 'arm64',
           'CC': 'gcc-14', 'HOSTCC': 'gcc-14', 'MAKE': 'make'}
    # GNU tar's deferred symlink extraction cannot reopen mode-000 placeholders
    # on the uid-mapped host share. Package on the Linux filesystem, then copy
    # the completed header tree; no kernel sources or object tree are cloned.
    temp = Path(subprocess.check_output(
        ['mktemp', '-d', '/tmp/kselftest-module-headers.XXXXXX'], text=True).strip())
    if temp.parent != Path('/tmp') or not temp.name.startswith('kselftest-module-headers.'):
        raise ValueError('unexpected module-header staging path')
    try:
        subprocess.run([str(script), str(temp)], cwd=kernel, env=env,
                       check=True, timeout=300)
        destination.mkdir(parents=True, exist_ok=True)
        subprocess.run(['rsync', '-a', str(temp) + '/', str(destination) + '/'],
                       check=True, timeout=300)
    finally:
        shutil.rmtree(temp)  # Exact Linux-local directory just created by mktemp.
    if (not (destination / 'Makefile').is_file()
            or (destination / 'Module.symvers').read_bytes() != symbols):
        raise ValueError('prepared module headers do not match the configured kernel')
    sample = Path('samples/livepatch/livepatch-sample.ko')
    sample_sha = None
    if (kernel / sample).is_file():
        (destination / sample).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(kernel / sample, destination / sample)
        sample_sha = hashlib.sha256((destination / sample).read_bytes()).hexdigest()
    link.parent.mkdir(parents=True, exist_ok=True)
    if link.is_symlink():
        link.unlink()
    link.symlink_to(guest)
    return {'release': release, 'guestPath': str(guest),
            'role': 'original prepared external-module headers; no target-C compilation credit',
            'scriptSha256': hashlib.sha256(script.read_bytes()).hexdigest(),
            'configSha256': hashlib.sha256(config).hexdigest(),
            'moduleSymversSha256': hashlib.sha256(symbols).hexdigest(),
            'livepatchSampleSha256': sample_sha}


def rebuild_test_modules(work, kernel):
    """Rebuild only upstream test-support modules for this BTF/version base.

    These remain GCC-built test infrastructure, never EvoCC target-C credit.
    The configured kernel is read-only in the worker's container; only the
    two existing staged test-module output directories are cleaned/rebuilt.
    """
    tests = work / 'rootfs/tests'
    sources = work / 'source/tools/testing/selftests'
    groups = {'bpf': 'bpf/test_kmods', 'livepatch/test_modules': 'livepatch/test_modules'}
    installed, aliases = [], []
    for path in sorted(tests.rglob('*.ko')):
        directory = str(path.parent.relative_to(tests))
        if directory in groups:
            installed.append(path)
        elif (directory in ('bpf/no_alu32', 'bpf/cpuv4', 'bpf/bpf_gcc')
              and path.is_symlink() and path.readlink() == Path('..') / path.name
              and (tests / 'bpf' / path.name).is_file()
              and not (tests / 'bpf' / path.name).is_symlink()):
            aliases.append({'path': str(path.relative_to(tests)),
                            'target': str(Path('bpf') / path.name)})
        else:
            raise ValueError('unaccounted external selftest module directory')
    protected = {name: file_sha256(kernel / name) for name in (
        '.config', 'vmlinux', 'Module.symvers', 'arch/arm64/boot/Image')}
    result = {'status': 'PASS', 'role': 'reference-built test support; no EvoCC target-C credit',
              'kernelSha256': protected, 'modules': [], 'helperAliases': aliases,
              'sourceSha256': {}, 'commands': []}
    logs = work / 'logs'
    logs.mkdir(exist_ok=True)
    with (logs / 'kernel-test-modules.log').open('w') as log:
        for destination, directory in groups.items():
            expected = [path for path in installed if str(path.parent.relative_to(tests)) == destination]
            if not expected:
                continue
            source = sources / directory
            if (not source.resolve().is_relative_to(sources.resolve())
                    or not (source / 'Makefile').is_file()):
                raise ValueError('unsafe or missing staged test-module source')
            inputs = [path for path in source.iterdir()
                      if path.name == 'Makefile' or (path.suffix in ('.c', '.h')
                                                   and not path.name.endswith('.mod.c'))]
            hashes = {str(path.relative_to(sources)): file_sha256(path) for path in inputs}
            result['sourceSha256'].update(hashes)
            base = ['make', '-C', str(kernel), f'O={kernel}', f'M={source}', 'ARCH=arm64',
                    'CC=gcc-14', 'HOSTCC=gcc-14', '-j1']
            for target in ('clean', 'modules'):
                command = [*base, target]
                result['commands'].append(command)
                log.write(json.dumps(command) + '\n')
                log.flush()
                subprocess.run(command, env={**os.environ, 'LLVM_OBJCOPY': 'objcopy'},
                               stdout=log, stderr=subprocess.STDOUT, check=True, timeout=300)
            names = (source / 'modules.order').read_text().splitlines()
            outputs = []
            for name in names:
                path = Path(name)
                if not path.is_absolute():
                    path = source / path
                if not path.resolve().is_relative_to(source.resolve()) or path.suffix != '.o':
                    raise ValueError('external test-module output escapes its directory')
                outputs.append(path.with_suffix('.ko'))
            if (len(set(outputs)) != len(outputs)
                    or sorted(path.name for path in outputs) != sorted(path.name for path in expected)):
                raise ValueError('external test-module inventory changed during refresh')
            for path in outputs:
                destination_path = tests / destination / path.name
                shutil.copy2(path, destination_path)
                result['modules'].append({'path': str(destination_path.relative_to(tests)),
                                          'sha256': file_sha256(destination_path)})
            if any(file_sha256(sources / name) != digest for name, digest in hashes.items()):
                raise ValueError('external test-module source changed during refresh')
    if any(file_sha256(kernel / name) != digest for name, digest in protected.items()):
        raise ValueError('kernel changed while rebuilding external test modules')
    return result


def check_network_runtime(rootfs):
    """Generated binaries alone do not satisfy net/lib's upstream TEST_FILES."""
    for name in ('py/ynl.py', 'ynl/pyynl/__init__.py', 'ynl/pyynl/lib/ynl.py',
                 'specs/netdev.yaml', 'csum', 'xdp_helper'):
        if not (rootfs / 'tests/net/lib' / name).is_file():
            raise ValueError(f'incomplete upstream network library install: {name}')


def install_runtime_paths(rootfs):
    """Keep runtime aliases inside the guest root, including on the host."""
    for part in ('dev', 'proc', 'sys', 'tmp', 'mnt', 'scratch', 'run/lock', 'root', 'var/tmp', 'var/log'):
        (rootfs / part).mkdir(parents=True, exist_ok=True)
    for name, dest in (('bin', 'usr/bin'), ('sbin', 'usr/sbin'), ('lib', 'usr/lib'),
                       ('var/run', '../run'), ('var/lock', '../run/lock')):
        path = rootfs / name
        if path.is_symlink():
            if path.readlink() != Path(dest):
                raise ValueError(f'unexpected runtime link: {name}')
        else:
            path.symlink_to(dest)
    if (rootfs / 'tests/bpf/bpftool').is_file():
        path = rootfs / 'usr/sbin/bpftool'
        path.parent.mkdir(parents=True, exist_ok=True)
        target = Path('../../tests/bpf/bpftool')
        if path.exists() or path.is_symlink():
            if not path.is_symlink() or path.readlink() != target:
                raise ValueError('unexpected runtime bpftool')
        else:
            path.symlink_to(target)


def write_rootfs_tar(rootfs, output, file_metadata=None):
    """Use guest ownership and restore modes lost by the macOS bind mount."""
    file_metadata = file_metadata or {}

    def guest_owner(member):
        member.uid, member.gid, member.mode = file_metadata.get(
            member.name.removeprefix('./'), (0, 0, member.mode))
        # Numeric IDs are authoritative; do not resolve macOS account names.
        member.uname = member.gname = ''
        return member

    with tarfile.open(fileobj=output, mode='w|') as archive:
        archive.add(rootfs, arcname='.', filter=guest_owner)


def format_rootfs(rootfs, image, file_metadata=None, blocks=None):
    if blocks is None:
        payload = 0
        for path in rootfs.rglob('*'):
            info = path.lstat()
            # Do not follow guest aliases or module build/source symlinks.
            payload += 4096
            if stat.S_ISREG(info.st_mode):
                payload += info.st_size
        # Include filesystem metadata and free space without duplicating the
        # archive. Explicit MiB units avoid mke2fs block-size-dependent sizing.
        capacity = payload + payload // 4 + (64 << 20)
        quantum = 256 << 20
        capacity = max(2 << 30, ((capacity + quantum - 1) // quantum) * quantum)
        blocks = f'{capacity >> 20}M'
    # Stream the archive directly: do not keep another multi-GB payload copy.
    # Debian's formatter is built with archive support; the Homebrew build is
    # not. Run this in the same Linux worker that collected the file metadata.
    command = ['mke2fs', '-q', '-F', '-t', 'ext4', '-O', 'verity', '-d', '-', '-E',
               'root_owner=0:0,lazy_itable_init=0,lazy_journal_init=0', str(image), blocks]
    with subprocess.Popen(command, stdin=subprocess.PIPE) as child:
        try:
            write_rootfs_tar(rootfs, child.stdin, file_metadata)
            child.stdin.close()
        except BrokenPipeError:
            # Report the formatter's failure rather than an archive traceback.
            child.wait()
            raise subprocess.CalledProcessError(child.returncode, command) from None
        if child.wait():
            raise subprocess.CalledProcessError(child.returncode, command)


def selected_programs(builds, selectors):
    available = [name for build in builds for name in build['available']]
    if not selectors:
        return available
    chosen = set()
    for selector in selectors:
        matches = [name for name in available if name == selector or name.split(':', 1)[0] == selector]
        if not matches:
            raise ValueError(f'no installed selftests match {selector}')
        chosen.update(matches)
    return [name for name in available if name in chosen]


def test_manifest(text):
    rows = [line for line in text.splitlines()
            if re.fullmatch(r'[\w./+-]+:[\w./+-]+', line)]
    if len(set(rows)) != len(rows) or any('..' in r.replace(':', '/').split('/') for r in rows):
        raise ValueError('duplicate or unsafe upstream test name')
    return rows


def summarize(builds, runtime, dependencies=()):
    actual = {row['id']: row for row in runtime}
    counts = Counter()
    incomplete = []
    for build in builds:
        available = build.get('available', build['tests'])
        counts['BUILD_UNAVAILABLE'] += len(build['tests']) - len(available)
        row = actual.get(build['id'], {})
        cases = row.get('cases', [])
        if len(cases) > len(available):
            raise ValueError('runtime denominator exceeds installed test list')
        counts.update(case['outcome'] for case in cases)
        counts['NOT_RUN'] += len(available) - len(cases)
        if available and (not row.get('valid') or row.get('boot', {}).get('verdict', 'PASS') != 'PASS'):
            incomplete.append(build['id'])
    counts = +counts
    failed = [b['id'] for b in builds if b['buildExit'] != 0 or b.get('buildErrors')]
    dependency_failures = [b['id'] for b in dependencies
                           if b['buildExit'] != 0 or b['installExit'] != 0 or b.get('buildErrors')]
    empty = [b['id'] for b in builds if not b.get('available', b['tests'])]
    nested = [case['id'] for row in runtime for case in row.get('cases', [])
              if case.get('nestedFailureSummaries')]
    nested_skips = [case['id'] for row in runtime for case in row.get('cases', [])
                    if case.get('nestedSkipSummaries')]
    return {'collections': len(builds), 'programs': sum(len(b['tests']) for b in builds),
            'completedCollections': sum(bool(b.get('available', b['tests']))
                and len(actual.get(b['id'], {}).get('cases', [])) == len(b.get('available', b['tests']))
                and all(c.get('started', True) for c in actual.get(b['id'], {}).get('cases', []))
                for b in builds),
            'outcomes': dict(counts), 'buildFailures': failed, 'noRunnableTests': empty,
            'dependencyBuildFailures': dependency_failures,
            'nestedFailureReview': nested,
            'nestedSkipReview': nested_skips,
            'incompleteCollections': incomplete,
            'timedOutCollections': [r['id'] for r in runtime if r.get('outcome') == 'TIMEOUT'
                                    or r.get('boot', {}).get('verdict') == 'TIMEOUT'],
            'status': 'PASS' if counts and set(counts) == {'PASS'} and not failed and not dependency_failures
            and not empty and not incomplete and not nested and not nested_skips else 'INCOMPLETE'}


def program_case(ident, result):
    """A VM watchdog failure is one measured program, not the rest of its collection."""
    boot = result['boot']['verdict']
    row = result['programs'][0]
    outcome = row['outcome']
    if outcome == 'NOT_RUN':
        outcome = 'NOT_RUN'
    elif boot == 'TIMEOUT' or outcome == 'TIMEOUT':
        outcome = 'TIMEOUT'
    elif boot != 'PASS':
        outcome = 'INCOMPLETE'
    elif not row.get('valid') or len(row['cases']) != 1:
        outcome = 'INVALID_OUTPUT'
    elif row['cases'][0]['outcome'] == 'FAIL' and '# TIMEOUT ' in row['cases'][0].get('name', ''):
        outcome = 'TIMEOUT'
    elif outcome == 'FAIL':
        outcome = 'FAIL'
    else:
        outcome = row['cases'][0]['outcome']
    return {'id': ident, 'outcome': outcome, 'bootVerdict': boot,
            'started': row['outcome'] != 'NOT_RUN',
            'valid': row.get('valid', False) and boot == 'PASS',
            'exitCode': row.get('exitCode'),
            'nestedFailureSummaries': row.get('nestedFailureSummaries', []),
            'nestedSkipSummaries': row.get('nestedSkipSummaries', [])}


def reusable_reference(previous, current):
    """Reuse a completed control measurement, never the old subject results."""
    if previous['status'] not in ('PASS', 'INCOMPLETE'):
        raise ValueError('reference run is not finished')
    for key in ('build', 'referenceConfigSha256', 'runtimePolicy'):
        if previous[key] != current[key]:
            raise ValueError(f'reference reuse input changed: {key}')
    if 'payloads' in previous or 'payloads' in current:
        before = previous.get('payloads', {}).get('reference')
        after = current.get('payloads', {}).get('reference')
        if (not before or before != after or not before.get('initramfsSha256')
                or not before.get('rootfsSha256')):
            raise ValueError('reference runtime payload changed or missing')
    else:
        # Old receipts had one shared payload. With per-kernel payloads the
        # top-level hashes name the subject, whose new modules must be tested
        # without unnecessarily repeating an unchanged reference campaign.
        for key in ('initramfsSha256', 'rootfsSha256'):
            if previous[key] != current[key]:
                raise ValueError(f'reference reuse input changed: {key}')
    cases = [case for row in previous['execution']['reference']['collections'] for case in row['cases']]
    wanted = [ident for b in current['build']['builds'] for ident in b['available']]
    if sorted(c['id'] for c in cases) != sorted(wanted):
        raise ValueError('reference run does not account for every installed program exactly once')
    if any(c['outcome'] == 'PASS' and c.get('exitCode') != 0 for c in cases):
        raise ValueError('reference PASS disagrees with its process exit')
    return cases


def save(path, data):
    path.write_text(json.dumps(data, indent=2) + '\n')


def build_command(command, handle, timeout):
    process = subprocess.Popen(command, stdout=handle, stderr=subprocess.STDOUT, start_new_session=True)
    try:
        return process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait()
        return 124


def enumerate_collections(base):
    """Read both runnable targets and their upstream install dependencies."""
    emitted = subprocess.check_output(base + ['-s', '-f', 'Makefile', '-f', '-', 'print_collections'],
        input='print_collections:\n\t@printf "%s\\n" "$(TARGETS) $(TARGETS_HOTPLUG)" "$(INSTALL_DEP_TARGETS)"\n',
        text=True).splitlines()
    if len(emitted) != 2:
        raise ValueError('invalid upstream collection enumeration')
    targets, dependencies = [list(dict.fromkeys(line.split())) for line in emitted]
    if not targets or any(not re.fullmatch(r'[\w/-]+', t) or '..' in t.split('/')
                          for t in targets + dependencies):
        raise ValueError('invalid upstream TARGETS enumeration')
    return targets, [name for name in dependencies if name not in targets]


def prepare_btf_input(source, kernel_build, work, jobs):
    """Generate selftest BTF from the actual reference DWARF, never edit it.

    Pahole 1.30 with the sealed libbpf 1.5 silently ignores `attributes`.
    The same frozen kernel contains the required libbpf API. Build it only
    when needed, expose it only to pahole, and record that extra build input.
    This neither enables BTF in a disabled kernel nor repairs its runtime BTF.
    """
    if 'CONFIG_DEBUG_INFO_BTF=y' not in (kernel_build / '.config').read_text().splitlines():
        return None

    def sha(path):
        digest = hashlib.sha256()
        with path.open('rb') as handle:
            for block in iter(lambda: handle.read(1 << 20), b''):
                digest.update(block)
        return digest.hexdigest()

    encoder = shutil.which('pahole')
    if not encoder:
        raise ValueError('BTF selftests require pahole')
    destination = work / 'btf'
    destination.mkdir(parents=True, exist_ok=True)
    kernel = kernel_build / 'vmlinux'
    before = sha(kernel)
    library = None
    commands = []
    env = dict(os.environ)
    env.pop('LD_LIBRARY_PATH', None)
    with (destination / 'build.log').open('w') as log:
        def run(argv):
            commands.append(argv)
            log.write(json.dumps(argv) + '\n')
            log.flush()
            result = subprocess.run(argv, env=env, text=True, stdout=subprocess.PIPE,
                                    stderr=subprocess.STDOUT, timeout=600, check=False)
            log.write(result.stdout)
            log.flush()
            result.check_returncode()
            return result.stdout.strip().split(',')

        features = run([encoder, '--supported_btf_features'])
        original_features = features
        if 'attributes' not in features:
            library_dir = destination / 'libbpf'
            library_dir.mkdir(exist_ok=True)
            library = library_dir / 'libbpf.so'
            run(['make', '--no-print-directory', '-C', str(source / 'tools/lib/bpf'),
                 f'OUTPUT={library_dir}/', 'CC=gcc-14', f'-j{jobs}', str(library)])
            env['LD_LIBRARY_PATH'] = str(library_dir)
            features = run([encoder, '--supported_btf_features'])
        if 'attributes' not in features:
            raise ValueError('pahole still lacks attributes after the source libbpf build')
        output = destination / 'vmlinux.btf'
        run([encoder, f'-j{jobs}', f'--btf_encode_detached={output}',
             '--btf_features_strict=encode_force,var,float,enum64,decl_tag,type_tag,'
             'optimized_func,consistent_func,decl_tag_kfuncs,attributes', str(kernel)])
    if sha(kernel) != before:
        raise ValueError('reference kernel changed during BTF preparation')
    return {'path': str(output), 'sourceKernelSha256': before,
            'outputSha256': sha(output), 'paholeSha256': sha(Path(encoder)),
            'initialFeatures': original_features, 'features': features,
            'libbpfSha256': sha(library) if library else None,
            'commands': commands, 'log': str(destination / 'build.log')}


def collection_build_options(target, work, kernel_build=None, *, btf_input=None):
    # These upstream Makefiles recursively build kernel modules. The exported
    # O variable must name their configured kernel, not userspace scratch.
    output = kernel_build if target in ('bpf', 'livepatch') and kernel_build else work / 'obj'
    options = [f'O={output}', 'LLVM_STRIP=llvm-strip-19', 'LLVM_CONFIG=llvm-config-19']
    if kernel_build:
        options.extend([f'VMLINUX_BTF={btf_input or kernel_build / "vmlinux"}', f'KDIR={kernel_build}'])
    return options


def build_worker(jobs, kernel_build=None):
    """Runs only inside the disposable Linux build container, never on the host."""
    work, original = Path('/work/full'), Path('/work/src')
    source = work / 'source'
    source.mkdir(exist_ok=True)
    # Several upstream Makefiles ignore OUTPUT. Stage only tools (90 MiB), not
    # the kernel tree; preserve test sources and use their unmodified recipes.
    (source / 'tools').mkdir(exist_ok=True)
    subprocess.run(['rsync', '-a', str(original / 'tools') + '/', str(source / 'tools') + '/'], check=True)
    for path in original.iterdir():
        if path.name != 'tools' and not (source / path.name).exists():
            (source / path.name).symlink_to(path)
    logs, rootfs = work / 'logs', work / 'rootfs'
    logs.mkdir(exist_ok=True)
    (work / 'obj').mkdir(exist_ok=True)
    tests = rootfs / 'tests'
    base = ['make', '--no-print-directory', '-C', str(source / 'tools/testing/selftests'),
            'ARCH=arm64', 'SKIP_TARGETS=']
    targets, dependencies = enumerate_collections(base)
    metadata = {'targets': targets, 'enumerationCommand': base,
                'installDependencyTargets': dependencies, 'dependencyBuilds': [],
                'packages': subprocess.check_output(['dpkg-query', '-W'], text=True),
                'compiler': subprocess.check_output(['gcc-14', '--version'], text=True), 'builds': []}
    btf_input = None
    if kernel_build:
        from selftest import sha
        metadata['kernelBuild'] = {'root': str(kernel_build), 'role': 'reference userspace/test-module build input',
                                  'sha256': {name: sha(kernel_build / name)
                                             for name in ('.config', 'vmlinux', 'Module.symvers')}}
        metadata['btfInput'] = prepare_btf_input(original, kernel_build, work, jobs)
        if metadata['btfInput']:
            btf_input = Path(metadata['btfInput']['path'])
    if tests.exists():
        shutil.rmtree(tests)  # This lane's generated install only, never source or kernel objects.
    tests.mkdir(parents=True, exist_ok=True)
    save(work / 'build.json', metadata)
    for target in dependencies + targets:
        if shutil.disk_usage(work).free < 8 << 30:
            raise RuntimeError('less than 8 GiB free; stopping before filling the host disk')
        directory = source / 'tools/testing/selftests' / target
        output = directory
        cmd = ['make', '--no-print-directory', '-C', str(directory), '-o', 'headers', 'ARCH=arm64',
               'CC=gcc-14', 'HOSTCC=gcc-14', 'CLANG=clang-19', 'LLC=llc-19',
               f'OUTPUT={output}', *collection_build_options(target, work, kernel_build, btf_input=btf_input),
               'KHDR_INCLUDES=-isystem /work/headers/include',
               f'CPPFLAGS=-I{source / "tools/testing/selftests"} -isystem /work/headers/include',
               'CFLAGS_NOLIBC_TEST=-isystem /work/headers/include']
        log = logs / (target.replace('/', '.') + '.log')
        started = time.monotonic()
        with log.open('w') as handle:
            # The arm64 signal Makefile copies all outputs from multiple rules;
            # its parallel cp race is unrelated to kernel/compiler behavior.
            target_jobs = 1 if target == 'arm64' else jobs
            build_rc = build_command(cmd + ['-k', '-j', str(target_jobs), 'all'], handle, 600)
            build_errors = [line for line in log.read_text(errors='replace').splitlines()
                            if re.search(r'make(?:\[\d+\])?: \*\*\*', line)]
            install = cmd + ['-k', '-o', 'all', 'install', f'INSTALL_PATH={tests / target}',
                             f'SRC_PATH={source / "tools/testing/selftests"}', f'OBJ_PATH={tests}']
            try:
                install_rc = build_command(install, handle, 120)
                emitted = subprocess.run(cmd + ['-s', f'COLLECTION={target}', 'emit_tests'],
                                         capture_output=True, text=True, timeout=60)
                names = test_manifest(emitted.stdout)
                handle.write(emitted.stderr)
            except subprocess.TimeoutExpired:
                install_rc, names = 124, []
        available = [name for name in names if (tests / name.replace(':', '/', 1)).is_file()]
        row = {'id': target, 'command': cmd + ['-k', '-j', str(target_jobs), 'all'],
               'buildExit': build_rc, 'buildErrors': build_errors, 'installExit': install_rc, 'tests': names, 'available': available,
               'seconds': round(time.monotonic() - started, 2), 'log': log.name}
        settings = tests / target / 'settings'
        timeout = re.search(r'^timeout=(\d+)$', settings.read_text() if settings.exists() else '', re.M)
        row['upstreamTimeoutSeconds'] = int(timeout[1]) if timeout else 45
        row['programTimeoutSeconds'] = (row['upstreamTimeoutSeconds'] or 300) + 15
        row['collectionTimeoutSeconds'] = max(120, len(available) * (row['upstreamTimeoutSeconds'] or 300) + 60)
        metadata['dependencyBuilds' if target in dependencies else 'builds'].append(row)
        save(work / 'build.json', metadata)
        print(f'build {len(metadata["builds"]) + len(metadata["dependencyBuilds"])}/{len(targets) + len(dependencies)} {target}: rc={build_rc}, '
              f'installed={len(available)}/{len(names)}', flush=True)
    for filename in ('run_kselftest.sh',):
        shutil.copy2(source / 'tools/testing/selftests' / filename, tests / filename)
    shutil.copytree(source / 'tools/testing/selftests/kselftest', tests / 'kselftest', dirs_exist_ok=True)
    # INSTALL_DEP_TARGETS already installed the generated network helpers;
    # copying their source directory is not a substitute for building them.
    install_runtime_helpers(source / 'tools/testing/selftests', tests)
    all_tests = [name for row in metadata['builds'] for name in row['available']]
    (tests / 'kselftest-list.txt').write_text('\n'.join(all_tests) + '\n')
    (tests / 'list').write_text(''.join(f'{b["id"]} {b["programTimeoutSeconds"]}\n'
                                       for b in metadata['builds'] if b['available']))
    (tests / 'huge-pages').write_text('1\n')
    refresh_runtime(work, kernel_build)
    metadata.update(json.loads((work / 'build.json').read_text()))
    metadata['complete'] = True
    save(work / 'build.json', metadata)
    return 0


def refresh_runtime(work, kernel_build=None):
    """Update userspace prerequisites without rebuilding tests or kernel Images."""
    rootfs = work / 'rootfs'
    check_network_runtime(rootfs)
    runtime_helpers = install_runtime_helpers(Path('/work/src/tools/testing/selftests'), rootfs / 'tests')
    kernel_modules = (install_kernel_modules(Path('/work/src'), kernel_build, rootfs)
                      if kernel_build else None)
    module_build_tree = (install_module_build_tree(Path('/work/src'), kernel_build, rootfs)
                         if kernel_build else None)
    test_modules = rebuild_test_modules(work, kernel_build) if kernel_build else None
    module_prerequisites = install_runtime_module_prerequisites(rootfs, kernel_modules)
    file_metadata = {}
    # A runtime root, not another kernel tree or complete compiler installation.
    for part in ('usr/bin', 'usr/sbin', 'usr/lib/aarch64-linux-gnu', 'usr/lib/python3',
                 'usr/lib/python3.13', 'usr/lib/locale', 'usr/libexec', 'usr/share/perl5',
                 'usr/share/perl', 'usr/share/zoneinfo', 'usr/share/alsa',
                 'usr/lib/udev', 'usr/lib/systemd', 'etc'):
        if Path('/' + part).exists():
            dest = rootfs / part
            dest.mkdir(parents=True, exist_ok=True)
            subprocess.run(['rsync', '-a', '--exclude=*.a', '--exclude=*.o', '/' + part + '/',
                            str(dest) + '/'], check=True)
            # The host share maps ownership and strips set-ID mode bits. Keep
            # only exceptions to root ownership and the copied file's mode.
            for source in Path('/' + part).rglob('*'):
                name = str(source.relative_to('/'))
                copied = rootfs / name
                if not copied.exists() and not copied.is_symlink():
                    continue
                original = source.lstat()
                mode = stat.S_IMODE(original.st_mode)
                if (original.st_uid or original.st_gid
                        or mode != stat.S_IMODE(copied.lstat().st_mode)):
                    file_metadata[name] = [original.st_uid, original.st_gid, mode]
    install_runtime_paths(rootfs)
    runtime_ethtool = install_runtime_ethtool(work, rootfs)
    shutil.copy2('/usr/lib/ld-linux-aarch64.so.1', rootfs / 'usr/lib/ld-linux-aarch64.so.1', follow_symlinks=True)
    subprocess.run(['gcc-14', '-static', '-O2', '-Wall', '-Wextra', '-Werror',
                    '/work/harness/selftest-init.c', '-o', str(work / 'init')], check=True)
    (work / 'full-suite').touch()
    (work / 'spec.txt').write_text('dir /dev 755 0 0\ndir /newroot 755 0 0\n'
        'nod /dev/console 600 0 0 c 5 1\nfile /init /work/full/init 755 0 0\n'
        'file /full-suite /work/full/full-suite 644 0 0\n')
    with (work / 'initramfs.cpio').open('wb') as handle:
        subprocess.run(['/work/bin/gen_init_cpio', '-t', '0', str(work / 'spec.txt')], stdout=handle, check=True)
    metadata = json.loads((work / 'build.json').read_text())
    if kernel_build:
        metadata['moduleBuildTree'] = module_build_tree
        metadata['kernelModules'] = kernel_modules
        metadata['kernelTestModules'] = test_modules
    metadata['runtimePackages'] = subprocess.check_output(['dpkg-query', '-W'], text=True)
    metadata['runtimeHelpers'] = runtime_helpers
    metadata['runtimeTools'] = {'ethtool': runtime_ethtool}
    metadata['runtimeModulePrerequisites'] = module_prerequisites
    metadata['runtimeFilesystem'] = {'root': 'ext4', 'features': ['verity'],
                                     'temporaryFiles': '/tmp on the per-flight disk snapshot',
                                     'sharedMemory': '/dev/shm tmpfs'}
    metadata['runtimeFileMetadata'] = file_metadata
    format_rootfs(rootfs, work / 'scratch.img', file_metadata)
    metadata['rootfsSha256'] = file_sha256(work / 'scratch.img')
    save(work / 'build.json', metadata)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--jobs', type=int, default=2)
    parser.add_argument('--work-root', type=Path, default=ROOT / '.build/kselftest-full',
                        help='selftest build and installed userspace; separate from kernel output')
    parser.add_argument('--source-root', type=Path, default=ROOT / '.build/linux-src')
    parser.add_argument('--headers-root', type=Path, default=ROOT / '.build/kselftest/headers')
    parser.add_argument('--kernel-build-root', type=Path,
                        help='configured reference output for actual BTF and upstream test-module builds')
    parser.add_argument('--build-worker', action='store_true', help=argparse.SUPPRESS)
    parser.add_argument('--runtime-worker', action='store_true', help=argparse.SUPPRESS)
    parser.add_argument('--refresh-runtime', action='store_true', help='install missing runtime prerequisites without rebuilding tests')
    parser.add_argument('--build-only', action='store_true')
    parser.add_argument('--reuse-build', action='store_true')
    parser.add_argument('--expanded-kernel', action='store_true',
                        help='use the independently booted, audited expanded KUnit kernel pair')
    parser.add_argument('--kernel-receipt', type=Path,
                        help='supplemental expanded receipt naming its separate reference/subject outputs')
    parser.add_argument('--reference-boot', type=Path,
                        help='reference boot receipt paired with a supplemental kernel-full receipt')
    parser.add_argument('--runtime-root', type=Path,
                        help='use <root>/{reference,subject}/{initramfs.cpio,scratch.img}; keep the original test inventory')
    parser.add_argument('--reference-run', type=Path,
                        help='reuse a finished reference summary after checking the unchanged Image and payload; execute every subject program')
    parser.add_argument('--install-deps', action='store_true', help='install additional selftest dependencies only in the disposable build container')
    parser.add_argument('--allow-qemu-drift', action='store_true')
    parser.add_argument('--select', action='append', default=[], help='replay an installed collection or collection:program; other programs remain NOT_RUN')
    args = parser.parse_args()
    if args.reference_run and not args.reuse_build:
        parser.error('--reference-run requires --reuse-build')
    if args.reference_run and (args.refresh_runtime or args.select):
        parser.error('reference reuse requires a complete run with the unchanged payload')
    if args.refresh_runtime and not args.install_deps:
        parser.error('--refresh-runtime requires --install-deps')
    if args.runtime_root and (not args.reuse_build or args.refresh_runtime):
        parser.error('--runtime-root requires --reuse-build without a runtime refresh')
    if args.kernel_receipt and not args.expanded_kernel:
        parser.error('--kernel-receipt requires --expanded-kernel')
    if args.build_worker:
        return build_worker(args.jobs, args.kernel_build_root)
    if args.runtime_worker:
        return refresh_runtime(Path('/work/full'), args.kernel_build_root)
    import selftest
    if args.allow_qemu_drift:
        os.environ['K1_ALLOW_QEMU_DRIFT'] = '1'
    work = args.work_root.resolve()
    work.mkdir(parents=True, exist_ok=True)
    selected_kernel = kernel_selection(args.expanded_kernel, receipt_path=args.kernel_receipt,
                                       reference_boot_path=args.reference_boot)
    kernel_path = selected_kernel['receipt']
    images = selected_kernel['images']
    subject = images['subject']
    environment = selftest.read(ROOT / 'contracts/build-environment.json')['buildImage']
    image = environment['sealedImageTag']
    if not args.reuse_build or args.refresh_runtime:
        if args.install_deps:
            prepare_runtime_ethtool(work)
        installed = json.loads(subprocess.check_output(['/usr/local/bin/container', 'image', 'inspect', image], text=True))
        if installed[0]['configuration']['descriptor']['digest'] != environment['executionReference']:
            raise ValueError('selftest seed image differs from the frozen build environment')
        cmd = ['/usr/local/bin/container', 'run', '--rm', '--platform', 'linux/arm64',
               '--cpus', str(args.jobs), '--memory', '4G',
               '-v', f'{args.source_root.resolve()}:/work/src:ro', '-v', f'{work}:/work/full',
               '-v', f'{args.headers_root.resolve()}:/work/headers:ro',
               '-v', f'{ROOT / "tools/k1"}:/work/harness:ro',
               '-v', f'{ROOT / ".build/reference/gcc-lane-1/usr/gen_init_cpio"}:/work/bin/gen_init_cpio:ro',
               *([] if args.install_deps else ['--network', 'none'])]
        if args.kernel_build_root:
            kernel_build = args.kernel_build_root.resolve()
            for name in ('.config', 'vmlinux', 'Module.symvers', 'include/config/auto.conf'):
                if not (kernel_build / name).is_file():
                    raise ValueError(f'incomplete reference kernel build: {name}')
            cmd += ['-v', f'{kernel_build}:/work/kernel:ro']
        cmd.append(image)
        mode = '--runtime-worker' if args.refresh_runtime else '--build-worker'
        worker = f'exec python3 /work/harness/selftest_full.py {mode} --jobs {args.jobs}'
        if args.kernel_build_root:
            worker += ' --kernel-build-root /work/kernel'
        if args.install_deps:
            packages = ('pkgconf libcap-dev libnuma-dev libmnl-dev libasound2-dev libaio-dev liburing-dev '
                        'libnl-3-dev libnl-genl-3-dev libcap-ng-dev libunwind-dev libtraceevent-dev '
                        'libbpf-dev libpopt-dev libfuse-dev libfuse3-dev libjson-c-dev libpcap-dev libkeyutils-dev '
                        'iproute2 iputils-ping iptables nftables ethtool socat numactl bc strace jq '
                        'python3-pyroute2 python3-yaml python3-pytest python3-scapy python3-pexpect '
                        'gawk python3-jsonschema libmbedtls-dev ' + ' '.join(BUILD_PACKAGES + RUNTIME_PACKAGES))
            worker = ('apt-get -o Acquire::Check-Valid-Until=false update > /work/full/dependencies.log 2>&1 && '
                      'DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends '
                      + packages + ' >> /work/full/dependencies.log 2>&1 && ' + worker)
        cmd += ['sh', '-c', worker]
        subprocess.run(cmd, check=True)
    if args.build_only:
        return 0
    build = selftest.read(work / 'build.json')
    selected = selected_programs(build['builds'], args.select)
    runtime_work = {label: args.runtime_root / label if args.runtime_root else work for label in images}
    if ([row['id'] for row in build['builds']] != build['targets']
            or not all((folder / name).is_file() for folder in runtime_work.values()
                       for name in ('initramfs.cpio', 'scratch.img'))):
        raise ValueError('selftest build has not completed its enumeration and payload')
    module_payloads = {}
    if selected_kernel.get('moduleLink', {}).get('required'):
        module_payloads = {label: check_module_runtime(folder, images[label],
                           selected_kernel['moduleLink'] if label == 'subject' else None)
                           for label, folder in runtime_work.items()}
    run_id = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    run = ROOT / '.evidence/kselftest-full' / run_id
    run.mkdir(parents=True)
    if not args.reuse_build:
        shutil.copytree(work / 'logs', run / 'build')
    if not args.reuse_build and (work / 'dependencies.log').exists():
        shutil.copy2(work / 'dependencies.log', run / 'dependencies.log')
    receipt_path = ROOT / 'tests/contracts/kernel-selftest-full-receipt.json'
    receipt = {'schemaVersion': 1, 'run': run_id, 'status': 'RUNNING',
               'scope': 'Every upstream TARGETS plus hotplug collection, SKIP_TARGETS empty; '
                        + ('expanded audited arm64 configuration' if args.expanded_kernel else 'unchanged K6 arm64 configuration'),
               'evidence': str(run.relative_to(ROOT)), 'kernelImageSha256': selftest.sha(subject),
               'kernelReceiptSha256': selftest.sha(kernel_path),
               'kernelReceiptPath': str(kernel_path.relative_to(ROOT)), 'build': build,
               'buildLogRoot': str(work / 'logs') if args.reuse_build else str(run / 'build'),
               'kernelConfigSha256': selftest.sha(selected_kernel['configs']['subject']),
               'referenceConfigSha256': selftest.sha(selected_kernel['configs']['reference']),
               'initramfsSha256': selftest.sha(runtime_work['subject'] / 'initramfs.cpio'),
               'rootfsSha256': selftest.sha(runtime_work['subject'] / 'scratch.img'), 'execution': {}}
    receipt['payloads'] = {label: {'root': str(folder),
                                  'initramfsSha256': selftest.sha(folder / 'initramfs.cpio'),
                                  'rootfsSha256': selftest.sha(folder / 'scratch.img')}
                           for label, folder in runtime_work.items()}
    if module_payloads:
        receipt['moduleInstallations'] = module_payloads
    receipt['configurationAudit'] = configuration_gaps(
        args.source_root, selected_kernel['configs']['subject'], build['builds'])
    if 'referenceBoot' in selected_kernel:
        path = selected_kernel['referenceBoot']
        receipt['referenceBoot'] = {'path': str(path), 'sha256': selftest.sha(path)}
    receipt['runtimePolicy'] = {'isolation': 'fresh VM per program', 'workers': min(args.jobs, 2),
                                'upstreamTimeouts': 'collection settings retained except explicit source-declared or measured program overrides; outer watchdog adds 15 seconds cleanup and 60 seconds boot',
                                'unlimitedProgramWatchdogSeconds': 300,
                                'modules': 'explicit per-kernel runtime payload; see module installation receipt' if args.runtime_root
                                           else 'not built or installed; original runtime payload unchanged',
                                'kunitAutorun': '0' if args.expanded_kernel else 'unchanged'}
    receipt['runtimePolicy']['programTimeoutOverrides'] = PROGRAM_TIMEOUT_OVERRIDES
    receipt['runtimePolicy']['programBootLanes'] = boot_policies()
    if args.select:
        receipt['selection'] = {'selectors': args.select, 'programs': selected,
                                'unselected': 'NOT_RUN; original inventory retained'}
    if receipt['kernelConfigSha256'] != receipt['referenceConfigSha256']:
        raise ValueError('reference and subject kernel configurations differ')
    def checkpoint():
        save(run / 'summary.json', receipt)
        save(receipt_path, receipt)
    checkpoint()
    receipt['referenceImageSha256'] = selftest.sha(images['reference'])
    rows = {label: {b['id']: {'id': b['id'], 'cases': [], 'valid': False} for b in build['builds'] if b['available']}
            for label in images}
    for label in images:
        receipt['execution'][label] = {'collections': list(rows[label].values()),
                                       'summary': summarize(build['builds'], [], build.get('dependencyBuilds', []))}

    def execute(label, b, number, ident):
        capture = label + '/' + ident.replace(':', '/')
        contract = {'consoleLoglevel': 5, 'hugePages': 1, 'bootOverheadSeconds': 60,
                    'perProgramTimeoutSeconds': program_timeout(b, ident)}
        environment = program_environment(ident, args.expanded_kernel)
        try:
            result = selftest.flight(capture, images[label], runtime_work[label], run, contract, [ident],
                                    extra_env=environment)
            case = program_case(ident, result)
        except ValueError as error:
            case = {'id': ident, 'outcome': 'NOT_RUN', 'started': False, 'valid': False, 'error': str(error)}
        case.update(number=number, name=ident, capture=str((run / capture).relative_to(ROOT)))
        return label, b, case

    def record(label, b, case):
        row = rows[label][b['id']]
        row['cases'].append(case)
        row['cases'].sort(key=lambda c: c['number'])
        row['valid'] = len(row['cases']) == len(b['available']) and all(c['valid'] for c in row['cases'])
        receipt['execution'][label]['summary'] = summarize(build['builds'], list(rows[label].values()),
                                                          build.get('dependencyBuilds', []))
        checkpoint()

    active_images = list(images)
    if args.reference_run:
        previous = selftest.read(args.reference_run)
        cases = reusable_reference(previous, receipt)
        qemu = selftest.sha(Path(selftest.read(ROOT / 'contracts/kernel-target.json')['qemu']['binaryPath']))
        builds = {b['id']: b for b in build['builds']}
        for case in cases:
            boot = selftest.read(ROOT / case['capture'] / 'boot.json')
            if (boot['imageSha256'] != receipt['referenceImageSha256']
                    or boot['qemu']['sha256'] != qemu
                    or selftest.sha(Path(boot['serialLog'])) != boot['serialLogSha256']):
                raise ValueError('reference Image, QEMU or captured output changed')
            b = builds[case['id'].split(':', 1)[0]]
            rows['reference'][b['id']]['cases'].append(case)
        for row in rows['reference'].values():
            row['valid'] = all(case['valid'] for case in row['cases'])
        receipt['execution']['reference']['summary'] = summarize(build['builds'], list(rows['reference'].values()),
                                                                build.get('dependencyBuilds', []))
        receipt['referenceReuse'] = {'run': previous['run'], 'summary': str(args.reference_run),
                                     'summarySha256': selftest.sha(args.reference_run),
                                     'programs': len(cases), 'subjectResultsReused': 0}
        active_images = ['subject']
        checkpoint()
        print(f'Reusing {len(cases)} unchanged reference measurements from {previous["run"]}; every subject program will run', flush=True)

    tasks = [(label, b, number, ident) for b in build['builds']
             for number, ident in enumerate(b['available'], 1) if ident in selected for label in active_images]
    with ThreadPoolExecutor(max_workers=min(args.jobs, 2)) as pool:
        # A guest which cannot start the test runtime would repeat the same boot
        # failure hundreds of times. Measure it once and retain every NOT_RUN.
        bootstrap = list(pool.map(lambda task: execute(*task), tasks[:len(active_images)]))
        for result in bootstrap:
            record(*result)
        if any(not case['started'] for _, _, case in bootstrap):
            receipt.update(status='BLOCKED', blocker='Kernel could not initialize the selftest runtime; tests not reached',
                           bootstrap={label: case for label, _, case in bootstrap})
            checkpoint()
            print(receipt['blocker'], flush=True)
            return 0
        futures = [pool.submit(execute, *task) for task in tasks[len(active_images):]]
        for future in as_completed(futures):
            record(*future.result())
    receipt['status'] = ('PASS' if all(e['summary']['status'] == 'PASS' for e in receipt['execution'].values())
                         else 'INCOMPLETE')
    receipt['referenceStatus'] = receipt['execution']['reference']['summary']['status']
    previous = rows['reference']
    receipt['regressions'] = []
    for row in receipt['execution']['subject']['collections']:
        old = previous[row['id']]
        now = {c['number']: c['outcome'] for c in row['cases']}
        receipt['regressions'].extend(f'{row["id"]}:{c["name"]}' for c in old['cases']
                                      if c['outcome'] == 'PASS' and now.get(c['number']) != 'PASS')
    if (selftest.sha(subject) != receipt['kernelImageSha256']
            or selftest.sha(images['reference']) != receipt['referenceImageSha256']
            or any(selftest.sha(folder / name) != receipt['payloads'][label][key]
                   for label, folder in runtime_work.items()
                   for name, key in [('initramfs.cpio', 'initramfsSha256'), ('scratch.img', 'rootfsSha256')])):
        raise ValueError('test images changed during execution')
    if boot_policies() != receipt['runtimePolicy']['programBootLanes']:
        raise ValueError('test machine profiles changed during execution')
    checkpoint()
    print(f'full selftest receipt: {receipt_path}', flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
