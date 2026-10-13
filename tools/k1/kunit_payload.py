#!/usr/bin/env python3
"""Build the per-kernel KUnit runtime payloads that tools/k1/kunit.py reads.

The recipe follows the retained module flights of 2026-09-22
(.evidence/kunit/expanded-{reference,subject-query}-modules-20260922):

- initramfs.cpio holds the common init from selftest-init.c and the
  /full-suite marker, as the full selftest runtime does;
- scratch.img is an ext4 root with the four command tools that this init
  runs (sh, dmesg, modprobe and ip), their shared libraries, the kernel's own
  installed modules, /tests/modules and /tests/kunit-modules;
- /tests/modules names fuse, the one setup module of those flights;
- /tests/kunit-modules names the 31 contract modules in contract order;
- there is no module build tree, no selftest program and no KUnit rerun list.

Each side uses its own audited tree of the T04-K composition receipt. The
subject side must install exactly the modules that the composition linked.
The build runs in the sealed image. --install-deps installs iproute2 and
e2fsprogs from the pinned snapshot; without it the container has no network.
An existing payload is never replaced.

Exit 0: each payload passes the checks of kunit.runtime_inputs().
Exit 2: an input, container or payload check error.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import stat
import subprocess
import sys

import layout
import selftest_full

ROOT = layout.ROOT
CONTRACT = ROOT / 'tests/contracts/kernel-expanded-kunit-contract.json'
SIDES = ('reference', 'subject')
# The init calls these through system() and execlp() with this PATH.
COMMAND_TOOLS = ('sh', 'dmesg', 'modprobe', 'ip')
GUEST_PATH = '/usr/sbin:/usr/bin:/sbin:/bin'
# ip and mke2fs are not in the sealed image; selftest_full.py installs the
# same packages from the same pinned snapshot for the selftest runtime.
PACKAGES = ('iproute2', 'e2fsprogs')
# The retained flights print "KSELFTEST_SETUP module fuse loaded" and no
# other setup module. Loading fuse makes the init's fusectl mount succeed.
SETUP_MODULES = (('fs/fuse/fuse.ko', 'fuse'),)
SPEC = ('dir /dev 755 0 0\ndir /newroot 755 0 0\nnod /dev/console 600 0 0 c 5 1\n'
        'file /init {payload}/init 755 0 0\nfile /full-suite {payload}/full-suite 644 0 0\n')
INIT_COMMAND = ('gcc-14', '-static', '-O2', '-Wall', '-Wextra', '-Werror')


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with Path(path).open('rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def guest_path(path: str) -> str:
    """Map a path to the merged /usr layout that install_runtime_paths creates."""
    for alias in ('/bin/', '/sbin/', '/lib/'):
        if path.startswith(alias):
            return '/usr' + path
    return path


def copy_into(rootfs: Path, path: str, metadata: dict, system: Path = Path('/')) -> list[str]:
    """Copy one file and the symlink chain that names it; return guest paths."""
    copied, seen, current = [], set(), os.path.normpath(path)
    while True:
        guest = guest_path(current)
        if guest in seen or len(seen) > 16:
            raise ValueError(f'symlink loop at {path}')
        seen.add(guest)
        origin = system / current.lstrip('/')
        destination = rootfs / guest.lstrip('/')
        destination.parent.mkdir(parents=True, exist_ok=True)
        if origin.is_symlink():
            target = os.readlink(origin)
            if destination.is_symlink():
                if os.readlink(destination) != target:
                    raise ValueError(f'conflicting runtime link: {guest}')
            else:
                destination.symlink_to(target)
            copied.append(guest)
            current = os.path.normpath(os.path.join(os.path.dirname(current), target))
            continue
        if not origin.is_file():
            raise ValueError(f'runtime file missing: {current}')
        shutil.copy2(origin, destination)
        info = origin.lstat()
        mode = stat.S_IMODE(info.st_mode)
        # The host share maps ownership and can drop mode bits; keep the original.
        if info.st_uid or info.st_gid or mode != stat.S_IMODE(destination.lstat().st_mode):
            metadata[guest.lstrip('/')] = [info.st_uid, info.st_gid, mode]
        copied.append(guest)
        return copied


def libraries(binary: str) -> list[str]:
    """The loader and the shared libraries that ldd resolves for one binary."""
    result = []
    for line in subprocess.check_output(['ldd', binary], text=True).splitlines():
        line = line.strip()
        if '=>' in line:
            target = line.split('=>', 1)[1].split(' (', 1)[0].strip()
            if not target.startswith('/'):
                raise ValueError(f'unresolved library of {binary}: {line}')
            result.append(target)
        elif line.startswith('/'):
            result.append(line.split(' (', 1)[0].strip())
    if not result:
        raise ValueError(f'no dynamic loader for {binary}')
    return result


def install_command_tools(rootfs: Path, metadata: dict, system: Path = Path('/')) -> dict:
    tools = {}
    for name in COMMAND_TOOLS:
        found = shutil.which(name, path=':'.join(str(system / part.lstrip('/')) for part in GUEST_PATH.split(':')))
        if not found:
            raise ValueError(f'{name} is not installed in the build image; use --install-deps')
        command = '/' + str(Path(found).relative_to(system))
        paths = copy_into(rootfs, command, metadata, system)
        real = os.path.realpath(found)
        linked = []
        for library in libraries(real):
            linked += copy_into(rootfs, library, metadata, system)
        tools[name] = {'command': command, 'guestPaths': paths, 'sha256': sha256(Path(real)),
                       'libraries': sorted(set(linked))}
    return tools


def tool_packages(tools: dict) -> dict:
    """The Debian package that owns each command, as dpkg records it."""
    result = {}
    for name, row in tools.items():
        owner = subprocess.run(['dpkg-query', '-S', row['command']], capture_output=True, text=True)
        package = owner.stdout.split(':', 1)[0].strip() if owner.returncode == 0 else None
        version = None
        if package:
            version = subprocess.check_output(['dpkg-query', '-W', '-f', '${Version}', package],
                                              text=True).strip()
        result[name] = {'package': package, 'version': version}
    return result


def worker(side: str, payload: Path = Path('/work/payload'), source: Path = Path('/work/src'),
           kernel: Path = Path('/work/kernel'), harness: Path = Path('/work/harness'),
           contract_path: Path = Path('/work/contract.json'),
           gen_init_cpio: Path = Path('/work/bin/gen_init_cpio'), system: Path = Path('/')) -> int:
    """Runs only inside the disposable sealed container."""
    contract = json.loads(contract_path.read_text())
    rootfs = payload / 'rootfs'
    if rootfs.exists() or (payload / 'scratch.img').exists():
        raise ValueError('the payload staging directory is not empty')
    rootfs.mkdir()
    selftest_full.install_runtime_paths(rootfs)
    metadata = {}
    tools = install_command_tools(rootfs, metadata, system)
    modules = selftest_full.install_kernel_modules(source, kernel, rootfs)
    if not modules:
        raise ValueError('the KUnit configuration installs no modules')
    installed = {row['unit'] for row in modules['modules']}
    missing = [row['unit'] for row in contract['suiteModules'] if row['unit'] not in installed]
    missing += [unit for unit, _ in SETUP_MODULES if unit not in installed]
    if missing:
        raise ValueError(f'required modules are not installed: {missing}')
    tests = rootfs / 'tests'
    tests.mkdir()
    (tests / 'modules').write_text(''.join(name + '\n' for _, name in SETUP_MODULES))
    (tests / 'kunit-modules').write_text(''.join(row['name'] + '\n' for row in contract['suiteModules']))
    commands = [[*INIT_COMMAND, str(harness / 'selftest-init.c'), '-o', str(payload / 'init')],
                [str(gen_init_cpio), '-t', '0', str(payload / 'spec.txt')]]
    subprocess.run(commands[0], check=True)
    (payload / 'full-suite').touch()
    (payload / 'spec.txt').write_text(SPEC.format(payload=payload))
    with (payload / 'initramfs.cpio').open('wb') as handle:
        subprocess.run(commands[1], stdout=handle, check=True)
    selftest_full.format_rootfs(rootfs, payload / 'scratch.img', metadata)
    build = {'schemaVersion': 1, 'side': side,
             'scope': 'KUnit module runtime: common init, four command tools, installed kernel modules '
                      'and the contract module list; no selftest program, module build tree or rerun list',
             'recipe': 'retained flights .evidence/kunit/expanded-reference-modules-20260922 and '
                       'expanded-subject-query-modules-20260922; built by tools/k1/kunit_payload.py',
             'contractSha256': sha256(contract_path), 'suiteModules': contract['suiteModules'],
             'setupModules': [name for _, name in SETUP_MODULES], 'kernelModules': modules,
             'runtimeTools': tools, 'runtimePackages': tool_packages(tools),
             'initSourceSha256': sha256(harness / 'selftest-init.c'), 'commands': commands,
             'runtimeFileMetadata': metadata,
             'initramfsSha256': sha256(payload / 'initramfs.cpio'),
             'rootfsSha256': sha256(payload / 'scratch.img')}
    (payload / 'build.json').write_text(json.dumps(build, indent=2) + '\n')
    return 0


def check_tree(tree: Path, contract: dict) -> None:
    """The contract modules and the setup module must be linked modules of the tree."""
    order = tree / 'modules.order'
    if not order.is_file():
        raise ValueError(f'kernel tree has no module inventory: {tree}')
    linked = {str(Path(line).with_suffix('.ko')) for line in order.read_text().splitlines()}
    wanted = [row['unit'] for row in contract['suiteModules']] + [unit for unit, _ in SETUP_MODULES]
    missing = [unit for unit in wanted if unit not in linked]
    if missing:
        raise ValueError(f'{tree} does not link {missing}')


def container_command(side: str, tree: Path, staging: Path, source: Path, tag: str,
                      jobs: int, install_deps: bool) -> list[str]:
    worker_command = f'exec python3 /work/harness/kunit_payload.py --worker --side {side}'
    if install_deps:
        worker_command = ('apt-get -o Acquire::Check-Valid-Until=false update > /work/payload/dependencies.log 2>&1'
                          ' && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends '
                          + ' '.join(PACKAGES) + ' >> /work/payload/dependencies.log 2>&1 && ' + worker_command)
    return [layout.CONTAINER, 'run', '--rm', '--platform', 'linux/arm64', '--cpus', str(jobs), '--memory', '4G',
            '-v', f'{source}:/work/src:ro', '-v', f'{tree}:/work/kernel:ro', '-v', f'{staging}:/work/payload',
            '-v', f'{ROOT / "tools/k1"}:/work/harness:ro', '-v', f'{CONTRACT}:/work/contract.json:ro',
            '-v', f'{layout.GEN_INIT_CPIO}:/work/bin/gen_init_cpio:ro',
            *([] if install_deps else ['--network', 'none']), tag, 'sh', '-c', worker_command]


def build_side(side: str, selected: dict, contract: dict, args, tag: str) -> Path:
    import kunit

    image, config = selected['images'][side], selected['configs'][side]
    final = args.output_root / side
    staging = args.output_root / f'.{side}.partial'
    if final.exists():
        raise ValueError(f'{final} exists; a payload is never replaced')
    if staging.exists():
        raise ValueError(f'{staging} remains from an earlier run; read its log, then remove it')
    staging.mkdir(parents=True)
    command = container_command(side, config.parent, staging, args.source_root, tag, args.jobs, args.install_deps)
    print(f'KUnit payload {side}: ' + shlex.join(command), flush=True)

    def remove_root():
        # The staging root is generated by this run and can hold several GiB.
        # Remove it only after the container has exited.
        rootfs = staging / 'rootfs'
        if rootfs.is_dir() and not rootfs.is_symlink():
            shutil.rmtree(rootfs)

    try:
        with (staging / 'build.log').open('w') as log:
            subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=7200)
    except subprocess.CalledProcessError:
        remove_root()
        raise
    remove_root()
    kunit.runtime_inputs(staging, image, config, selected.get('moduleLink') if side == 'subject' else None,
                         contract)
    staging.rename(final)
    return final


def parser():
    result = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    result.add_argument('--kernel-receipt', type=Path,
                        default=ROOT / 'tests/contracts/kernel-expanded-kunit-full-receipt.json',
                        help='T04-K composition receipt with its reference and subject trees')
    result.add_argument('--reference-boot', type=Path, help='boot.json of the reference Image; required')
    result.add_argument('--source-root', type=Path, default=layout.SOURCE,
                        help='clean case-sensitive Linux source, mounted read-only')
    result.add_argument('--output-root', type=Path, default=layout.KUNIT_RUNTIME,
                        help='new payloads go to <root>/reference and <root>/subject')
    result.add_argument('--side', choices=SIDES, action='append', help='build one side; default both')
    result.add_argument('--jobs', type=int, default=2)
    result.add_argument('--install-deps', action='store_true',
                        help='install iproute2 and e2fsprogs from the pinned snapshot; needs network')
    result.add_argument('--check', action='store_true', help='audit the inputs and print the plan only')
    result.add_argument('--worker', action='store_true', help=argparse.SUPPRESS)
    return result


def main(argv=None) -> int:
    args = parser().parse_args(argv)
    if args.worker:
        return worker(args.side[0] if args.side else 'unknown')
    sides = args.side or list(SIDES)
    args.source_root, args.output_root = args.source_root.resolve(), args.output_root.resolve()
    try:
        if args.reference_boot is None:
            raise ValueError('--reference-boot is required: name the boot.json of the reference Image')
        contract = json.loads(CONTRACT.read_text())
        selected = selftest_full.kernel_selection(True, receipt_path=args.kernel_receipt,
                                                  reference_boot_path=args.reference_boot)
        for side in sides:
            if sha256(selected['configs'][side]) != contract['configSha256']:
                raise ValueError(f'{side} configuration differs from the KUnit contract')
            check_tree(selected['configs'][side].parent, contract)
        if not (args.source_root / 'scripts/Makefile.modinst').is_file():
            raise ValueError(f'not a Linux source tree: {args.source_root}')
        environment = json.loads(layout.BUILD_ENVIRONMENT.read_text())
        tag, _ = layout.sealed_image(environment)
        if args.check:
            for side in sides:
                command = container_command(side, selected['configs'][side].parent,
                                            args.output_root / f'.{side}.partial', args.source_root, tag,
                                            args.jobs, args.install_deps)
                print(f'{side}: ' + shlex.join(command))
            print('KUnit payload preflight PASS; nothing built')
            return 0
        layout.require_attached(args.output_root)
        layout.verify_sealed_image(environment)
        for side in sides:
            final = build_side(side, selected, contract, args, tag)
            print(f'KUnit payload {side}: {final}', flush=True)
    except (OSError, KeyError, ValueError, subprocess.SubprocessError) as error:
        print(f'kunit payload: {error}', file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
