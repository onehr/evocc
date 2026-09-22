#!/usr/bin/env python3
"""Shared machinery for the ladder's whole-program rungs (E1, E2, E3).

Every rung above E0 measures the same way the kernel lane measures, and for
the same reason: a real program's own test suite is the only oracle that
knows whether a translation unit was compiled correctly. GCC-14 inside the
sealed container builds the subject; EvoCC compiles as many of its units as
it can on the host; the assembled objects replace the reference ones in the
tree the reference build left behind; the subject's own make relinks and its
own suite judges. A rung's number is "units by EvoCC in a build whose tests
pass", never "units EvoCC did not refuse".

The pieces, in the order a run uses them:

  ``fetch``            the pinned tarball from its official URL, verified
                       against the SHA-256 the subject file records
  ``reference_build``  the subject's documented build inside the container,
                       with the compile commands printed
  ``parse_build_log``  one record per compiled unit: the argv, the directory
                       it ran in, and the object it wrote
  ``emit``             EvoCC on the host, one unit at a time, with the
                       unit's own preprocessor flags and the container's
                       headers; a refusal is recorded by code, never worked
                       around
  ``assemble``         one container run assembles every emitted unit with
                       the reference GCC
  ``substitute``       the assembled objects replace the reference objects,
                       the subject's make relinks, the subject's suite runs
  ``bisect``           when the suite fails, the substituted set is halved
                       until a 1-minimal failing set remains

Substitution is by installed object file, exactly as ``tools/k1`` does it: an
object newer than its source is not rebuilt by make, and the binaries that
depend on it are relinked. Nothing wraps ``CC``; the reference build stays
the reference build.

The exported headers reach the front end unchanged. Build-system options
that the compiler does not consume are dropped by name and counted; source
syntax is never rewritten to bypass a compiler refusal.
"""

from __future__ import annotations

import argparse
import collections
import dataclasses
import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import urllib.request

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def _module(name: str, path: Path):
    """Load a lane module by path.

    Nothing here goes on ``sys.path``: this directory holds files named for
    their subjects, and ``zlib`` is a standard library module, so putting the
    directory on the search path would shadow it for everything downstream.
    """
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


#: The E0 lane already fixed how the container's headers reach the host and
#: in which order they are searched; both rungs must see the same sysroot.
e0 = _module("ladder_e0", HERE / "e0.py")

#: The delta debugging in the kernel lane is the same search, and the
#: standard library already owns the name ``bisect``.
k1_bisect = _module("k1_bisect", ROOT / "tools" / "k1" / "bisect.py")
ddmin = k1_bisect.ddmin

BUILD = e0.BUILD
EVIDENCE = ROOT / ".evidence" / "ladder"
IMAGE = e0.IMAGE
TARGET = e0.TARGET
CONTAINER_BIN = "container"

DIAGNOSTIC = re.compile(
    r"^(?:(?P<where>[^\n:]+(?::[^\n:]+)*?):(?P<line>\d+):(?P<column>\d+): )?"
    r"error\[(?P<code>[^]]+)\]: (?P<message>.*)$",
    re.MULTILINE,
)
ENTERING = re.compile(r"^g?make(?:\[\d+\])?: Entering directory '(?P<dir>[^']*)'")
LEAVING = re.compile(r"^g?make(?:\[\d+\])?: Leaving directory '(?P<dir>[^']*)'")
COMPILERS = frozenset({"gcc", "gcc-14", "cc", "g++", "clang", "clang-19"})

#: Options whose value is a separate argument.
SEPARATE_VALUE = ("-I", "-D", "-U", "-include", "-imacros", "-isystem", "-iquote")

#: Of those, the ones whose value is a path that must be translated.
PATH_VALUED = ("-I", "-include", "-imacros", "-isystem", "-iquote")

#: Options the front end consumes as written.
PASSED_THROUGH = e0.SUPPORTED_OPTIONS | {"-nostdinc", "-undef"}
PASSED_PREFIXES = e0.SUPPORTED_OPTION_PREFIXES


@dataclasses.dataclass(frozen=True)
class Subject:
    """One rung's subject: what to fetch, how to build it, how to test it.

    Every field but ``name`` and the pin is a shell command run inside the
    sealed container with the unpacked tree mounted at ``/work/<name>``. The
    commands are the subject's own documented ones; this file never invents a
    build.
    """

    name: str
    rung: str
    pin: str
    url: str
    sha256: str
    tree: str
    build: str
    test: str
    relink: str
    configure: str = ""
    #: Object paths (relative to the tree) that are not worth substituting,
    #: for example a unit the subject compiles twice under different flags.
    skip_objects: tuple[str, ...] = ()
    #: Where the relink and test commands work, when that is not the mounted
    #: tree. A subject whose suite cannot run on the shared mount copies the
    #: tree into the container first, and the relink log then names its
    #: objects relative to that copy.
    workspace: str = ""
    #: Every departure from the subject's documented build, in words, so the
    #: receipt says what was changed and why rather than only what was run.
    notes: tuple[str, ...] = ()
    #: Reductions of this subject's failures: the unit, what the suite did,
    #: and the smallest program that shows the same thing.
    defects: tuple[dict, ...] = ()
    #: Explicit object consumers for adapters whose default test recompiles
    #: the reference source. Their linkage adaptations have mutation tests.
    test_objects: tuple[str, ...] = ()
    #: Source subtrees constituting the measured subject, not build tools.
    source_prefixes: tuple[str, ...] = ()
    #: Extract only these archive members when a suite ships unrelated rungs.
    archive_members: tuple[str, ...] = ()
    #: A library relink may regenerate test C sources. Reinstall replaced
    #: objects only after recompiling them to byte-identical assembly.
    refresh_generated_inputs: bool = False


@dataclasses.dataclass(frozen=True)
class Unit:
    """One compilation the reference build performed."""

    obj: str
    """The object path relative to the mounted tree, e.g. ``lib/libzstd.o``."""
    source: str
    """The source path relative to the mounted tree."""
    directory: str
    """The directory, relative to the mounted tree, the command ran in."""
    argv: tuple[str, ...]


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(argv: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(argv, capture_output=True, text=True, check=False, **kwargs)


def container(mount: Path, name: str, script: str, timeout: int = 3600):
    """Run one shell script in the sealed container with ``mount`` attached."""
    return run(
        [
            CONTAINER_BIN, "run", "--rm",
            "--platform", "linux/arm64",
            "--network", "none",
            "-v", f"{mount}:/work/{name}",
            IMAGE,
            "bash", "-c", script,
        ],
        timeout=timeout,
    )


# --------------------------------------------------------------------------
# the pinned tarball


def fetch(subject: Subject) -> tuple[Path, str]:
    """The pinned tarball, downloaded once and verified every time.

    A pin is only a pin if the bytes are checked: the receipt reports the
    digest of what was actually built, and a mismatch stops the run rather
    than quietly measuring a different release.
    """
    directory = BUILD / subject.name
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory / subject.url.rsplit("/", 1)[-1]
    if not archive.is_file():
        shared = BUILD / 'dl' / archive.name
        if shared.is_file() and sha256_file(shared) == subject.sha256:
            shutil.copyfile(shared, archive)
        else:
            with urllib.request.urlopen(subject.url) as response, archive.open('wb') as output:
                shutil.copyfileobj(response, output)
    digest = sha256_file(archive)
    if digest != subject.sha256:
        raise SystemExit(
            f"{subject.name}: tarball digest {digest} does not match the pin "
            f"{subject.sha256}"
        )
    return archive, digest


def unpack(subject: Subject) -> Path:
    """A tree extracted fresh from the pinned tarball.

    Every run starts from the released bytes rather than from whatever an
    earlier run left behind: a build that has nothing to do prints no compile
    commands, and a tree carrying yesterday's objects would measure them.
    """
    directory = BUILD / subject.name
    tree = directory / subject.tree
    archive, _ = fetch(subject)
    if tree.is_dir():
        shutil.rmtree(tree)
    completed = run(["tar", "-xf", str(archive), "-C", str(directory),
                     *[f'{subject.tree}/{part}' for part in subject.archive_members]])
    if completed.returncode != 0 or not tree.is_dir():
        raise SystemExit(f"{subject.name}: cannot unpack {archive}: {completed.stderr}")
    return tree


# --------------------------------------------------------------------------
# the sysroot the front end can read


#: Headers are measured as exported, with no source-token substitutions.
SYSROOT_ADJUSTMENTS = {}

def front_end_gaps(rows: list[dict]) -> list[dict]:
    """Receipt facts come from this run, not an obsolete global frontier."""
    return [{key: row.get(key, '') for key in ('unit', 'code', 'message', 'at')}
            for row in rows if row['outcome'] == 'refused'
            and row.get('code', '').startswith(('E0', 'E1', 'E2'))]


def exported_sysroot() -> Path:
    """All lanes share the byte-preserving, case-sensitive header export."""
    return e0.sysroot()


def adjust_sysroot(force: bool = False) -> tuple[Path, dict[str, int]]:
    """Use the original exported headers, never a cached rewritten copy.

    ``force`` remains an accepted caller option but there is no derived
    sysroot to regenerate: the preprocessor owns include_next and whitespace,
    and the parser owns extension prefixes.
    """
    return exported_sysroot(), {}


# --------------------------------------------------------------------------
# the reference build and its compile commands


def parse_build_log(text: str, tree: str) -> list[Unit]:
    """One :class:`Unit` per target C compilation the build log records.

    ``make`` announces every directory it enters, and a recursive build's
    relative paths mean nothing without that; the directory stack is tracked
    so an object and its source are named relative to the mounted tree no
    matter which sub-make compiled them. A line that is a link, an archive,
    a host tool or a compilation of something that is not C is not a unit.
    """
    prefix = tree.rstrip("/") + "/"
    units: dict[str, Unit] = {}
    stack: list[str] = [""]

    def relative(value: str, directory: str) -> str:
        if not value.startswith("/"):
            value = os.path.normpath(os.path.join("/" + directory, value)).lstrip("/")
            return value
        if value == tree.rstrip("/"):
            return ""
        if value.startswith(prefix):
            return value[len(prefix) :]
        return value

    for line in text.splitlines():
        # Libtool prints the actual compiler invocation after this prefix;
        # its wrapper command names a .lo descriptor, not the installed .o.
        if line.startswith('libtool: compile: '):
            line = line.removeprefix('libtool: compile: ')
        entering = ENTERING.match(line)
        if entering:
            stack.append(relative(entering.group("dir"), ""))
            continue
        if LEAVING.match(line):
            if len(stack) > 1:
                stack.pop()
            continue
        directory = stack[-1]
        for command in _commands(line):
            # ``cd lib && gcc ...`` is two commands on one line, and the
            # second one runs where the first one went.
            if command[0] == "cd" and len(command) >= 2:
                directory = relative(command[1], directory)
                continue
            unit = _unit_from_argv(command, directory, relative)
            if unit is not None:
                units[unit.obj] = unit
    return list(units.values())


def _commands(line: str) -> list[list[str]]:
    """The commands on one log line, split on ``&&`` and ``;``.

    A build system happily writes ``cd lib && gcc -c ...`` on one line, and a
    recipe with several steps writes them all. Anything that does not
    tokenize as a shell command is not a compile command either.
    """
    line = line.strip()
    if not line or "-c" not in line:
        return []
    try:
        tokens = shlex.split(line, comments=False)
    except ValueError:
        return []
    commands: list[list[str]] = []
    current: list[str] = []
    for token in tokens:
        if token in ("&&", "||", ";", "|"):
            if current:
                commands.append(current)
            current = []
            continue
        current.append(token)
    if current:
        commands.append(current)
    return commands


def _unit_from_argv(argv: list[str], directory: str, relative) -> Unit | None:
    index = 0
    while index < len(argv) and "=" in argv[index] and not argv[index].startswith("-"):
        index += 1  # environment assignments in front of the compiler
    if index >= len(argv):
        return None
    if os.path.basename(argv[index]) not in COMPILERS:
        return None
    argv = argv[index:]
    if "-c" not in argv:
        return None
    sources = [
        token
        for position, token in enumerate(argv[1:], start=1)
        if token.endswith(".c") and argv[position - 1] not in SEPARATE_VALUE + ("-o",)
    ]
    if len(sources) != 1:
        return None
    if "-o" in argv:
        obj = argv[argv.index("-o") + 1]
        if not obj.endswith(".o"):
            return None
    else:
        # ``gcc -c foo.c`` writes ``foo.o`` beside the command, not beside
        # the source: a Makefile that leans on the implicit rule says so.
        obj = os.path.basename(sources[0])[:-2] + ".o"
    return Unit(
        obj=relative(obj, directory),
        source=relative(sources[0], directory),
        directory=directory,
        argv=tuple(argv),
    )


def reference_build(subject: Subject, tree: Path, evidence: Path) -> dict:
    """Build with the adapter's reference compiler and keep the original log."""
    script = (
        f"set -o pipefail; cd /work/{subject.name}/{subject.tree} || exit 2\n"
        + (f"{subject.configure} > /work/{subject.name}/configure.log 2>&1 || exit 3\n"
           if subject.configure else "")
        + f"{subject.build} > /work/{subject.name}/build.log 2>&1; echo \"build-status=$?\"\n"
    )
    completed = container(BUILD / subject.name, subject.name, script)
    log = (BUILD / subject.name / "build.log")
    text = log.read_text(errors="replace") if log.is_file() else ""
    status = _status(completed.stdout, "build-status")
    for name in ("configure.log", "build.log"):
        source = BUILD / subject.name / name
        if source.is_file():
            shutil.copyfile(source, evidence / name)
    return {"status": status, "log": text, "stdout": completed.stdout[-4000:]}


def portable_build_symlinks(tree: Path, mounted_tree: str) -> dict[str, str]:
    """Keep generated in-tree links valid in both mount namespaces.

    CMake can link a generated build path to an absolute container source
    path. An equivalent relative link preserves the source bytes and include
    location while also letting the host compiler read it. External targets
    and already relative links are left alone; every change is recorded.
    """
    prefix = mounted_tree.rstrip('/') + '/'
    changes = {}
    for link in tree.rglob('*'):
        if not link.is_symlink():
            continue
        target = os.path.normpath(os.readlink(link))
        if not target.startswith(prefix):
            continue
        relative = os.path.relpath(tree / target[len(prefix):], link.parent)
        link.unlink()
        link.symlink_to(relative)
        changes[str(link.relative_to(tree))] = relative
    return changes


def _status(stdout: str, key: str) -> int:
    match = re.search(rf"^{key}=(-?\d+)$", stdout, re.MULTILINE)
    return int(match.group(1)) if match else 1


def run_tests(subject: Subject, evidence: Path, tag: str) -> dict:
    """Run the subject's own suite in the container and keep its log."""
    script = (
        f"cd /work/{subject.name}/{subject.tree} || exit 2\n"
        f"{subject.test} > /work/{subject.name}/test.log 2>&1; echo \"test-status=$?\"\n"
    )
    completed = container(BUILD / subject.name, subject.name, script, timeout=3600)
    log = BUILD / subject.name / "test.log"
    text = log.read_text(errors="replace") if log.is_file() else ""
    (evidence / f"test-{tag}.log").write_text(text, encoding="utf-8")
    status = _status(completed.stdout, "test-status")
    return {"status": status, "passed": status == 0, "tail": text[-3000:]}


def relink(subject: Subject, evidence: Path, tag: str) -> dict:
    """Relink around the installed objects with the subject's own make."""
    script = (
        f"cd /work/{subject.name}/{subject.tree} || exit 2\n"
        f"{subject.relink} > /work/{subject.name}/relink.log 2>&1; echo \"relink-status=$?\"\n"
    )
    completed = container(BUILD / subject.name, subject.name, script, timeout=1800)
    log = BUILD / subject.name / "relink.log"
    text = log.read_text(errors="replace") if log.is_file() else ""
    (evidence / f"relink-{tag}.log").write_text(text, encoding="utf-8")
    return {"status": _status(completed.stdout, "relink-status"), "log": text}


# --------------------------------------------------------------------------
# the front-end pass


def unsupported_options(evocc: Path, options: list[str], work: Path) -> list[str]:
    """Which of ``options`` the driver refuses to accept at all.

    A build's ``-std=`` is a flag the front end would otherwise be handed as
    written. Whether this driver takes it is measured once, on an empty
    translation unit. This is diagnostic inventory only: a rejected language
    mode must still reach the driver on the original unit, not silently select
    a different default dialect.
    """
    work.mkdir(parents=True, exist_ok=True)
    empty = work / "empty.c"
    empty.write_text("int evocc_probe;\n", encoding="utf-8")
    rejected = []
    for option in sorted(set(options)):
        completed = run([str(evocc), f"--target={TARGET}", "--emit=asm", option, str(empty)])
        if completed.returncode != 0 and "unsupported" in completed.stderr:
            rejected.append(option)
    return rejected


def evocc_argv(
    unit: Unit,
    evocc: Path,
    subject: Subject,
    tree: Path,
    include: list[str],
    unsupported: frozenset[str] = frozenset(),
) -> tuple[list[str], list[str]]:
    """The EvoCC command for one unit, and the flags it does not consume.

    Nothing is invented: the unit's own ``-I``/``-D``/``-include`` flags are
    carried across with container paths rewritten to host paths, the headers
    of the target's own libc are appended, and every other flag is dropped by
    name so the receipt can say what was not honoured.
    """
    prefix = f"/work/{subject.name}/{subject.tree}/"
    work = tree / unit.directory

    def path(value: str) -> str:
        if value.startswith(prefix):
            return str(tree / value[len(prefix) :])
        mount_prefix = f'/work/{subject.name}/'
        if value.startswith(mount_prefix):
            return str(BUILD / subject.name / value[len(mount_prefix):])
        if value.startswith("/"):
            return value
        return str((work / value).resolve())

    projected = [str(evocc), f"--target={TARGET}", "--emit=asm"]
    omitted: list[str] = []
    argv = list(unit.argv)
    index = 1
    while index < len(argv):
        value = argv[index]
        if value == "-o":
            index += 2
            continue
        if value == "-c" or value.endswith(".c"):
            index += 1
            continue
        if value in SEPARATE_VALUE:
            if index + 1 >= len(argv):
                break
            following = argv[index + 1]
            projected.extend((value, path(following) if value in PATH_VALUED else following))
            index += 2
            continue
        if value.startswith("-I") and len(value) > 2:
            projected.append("-I" + path(value[2:]))
        elif value.startswith(("-D", "-U")) and len(value) > 2:
            projected.append(value)
        elif value.startswith("-include") and len(value) > len("-include"):
            projected.append("-include" + path(value[len("-include") :]))
        elif value.startswith("-std=") or ((value in PASSED_THROUGH or value.startswith(PASSED_PREFIXES)) and value not in unsupported):
            projected.append(value)
        else:
            omitted.append(value)
        index += 1
    projected.extend(include)
    projected.append(str(tree / unit.source))
    return projected, sorted(set(omitted))


def diagnostics(stderr: str) -> list[dict[str, str]]:
    """Every distinct diagnostic, with where it was raised.

    Where matters as much as what: a refusal raised in the subject's own
    source is a construct to reduce, and one raised in a system header is a
    fact about the headers the whole ladder shares.
    """
    seen: list[dict[str, str]] = []
    for match in DIAGNOSTIC.finditer(stderr):
        row = {"code": match.group("code"), "message": match.group("message").strip()[:160]}
        if match.group("where"):
            where = "/".join(Path(match.group("where")).parts[-2:])
            row["at"] = f"{where}:{match.group('line')}:{match.group('column')}"
        if row not in seen:
            seen.append(row)
    return seen


def emit(
    subject: Subject,
    units: list[Unit],
    evocc: Path,
    tree: Path,
    include: list[str],
    asm_root: Path,
    timeout: int = 300,
) -> list[dict]:
    """Run EvoCC over every unit and record what it did with each one."""
    unsupported = frozenset(
        unsupported_options(
            evocc,
            [flag for unit in units for flag in unit.argv if flag.startswith("-std=")],
            asm_root / "probe",
        )
    )
    rows: list[dict] = []
    for unit in units:
        row: dict = {
            "unit": unit.obj,
            "source": unit.source,
            "assemblerFlags": [flag for flag in unit.argv[1:] if flag.startswith("-Wa,")],
        }
        argv, omitted = evocc_argv(unit, evocc, subject, tree, include, unsupported)
        row["omittedFlags"] = omitted
        try:
            completed = subprocess.run(
                argv, capture_output=True, text=True, timeout=timeout, check=False
            )
        except subprocess.TimeoutExpired:
            rows.append({**row, "outcome": "timeout", "seconds": timeout})
            continue
        if completed.returncode != 0 or not completed.stdout.strip():
            found = diagnostics(completed.stderr)
            rows.append(
                {
                    **row,
                    "outcome": "refused",
                    "diagnostics": found,
                    "code": found[0]["code"] if found else "other",
                    "message": found[0]["message"] if found else completed.stderr.strip()[-160:],
                    "at": found[0].get("at", "") if found else "",
                }
            )
            continue
        assembly = asm_root / (unit.obj[:-2] + ".s")
        assembly.parent.mkdir(parents=True, exist_ok=True)
        assembly.write_text(completed.stdout, encoding="utf-8")
        rows.append({**row, "outcome": "emitted", "assembly": str(assembly),
                     "assemblySha256": sha256_file(assembly)})
    return rows


def emission_digest(argv: list[str], timeout: int = 300) -> str | None:
    """Recompile regenerated input with the identical command and compiler.

    Do not trust .c identity alone, or a preprocessor printer that may drop
    semantic pragmas. Compare actual assembly output; refuse reuse for external
    assembler file inputs, whose bytes are not part of that output.
    """
    try:
        result = subprocess.run(argv, capture_output=True, timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        return None
    if result.returncode or not result.stdout.strip() or re.search(
        rb'(?m)^\s*\.(?:incbin|include)\b', result.stdout
    ):
        return None
    return sha256_bytes(result.stdout)


def quote(argument: str) -> str:
    if re.fullmatch(r"[A-Za-z0-9_@%+=:,./-]+", argument):
        return argument
    return "'" + argument.replace("'", "'\\''") + "'"


def assemble(rows: list[dict], asm_root: Path) -> None:
    """Assemble every emitted unit with the reference GCC, in one run."""
    emitted = [row for row in rows if row["outcome"] == "emitted"]
    if not emitted:
        return
    lines = ["#!/bin/bash", "cd /work/asm || exit 2"]
    for row in emitted:
        unit = row["unit"]
        assembly = unit[:-2] + ".s"
        # An assembly input reaches only the assembler, so what survives of
        # the unit's command is what was addressed to the assembler by name.
        flags = " ".join(quote(flag) for flag in row.get("assemblerFlags", []))
        lines.append(f"mkdir -p {quote(os.path.dirname(unit) or '.')}")
        lines.append(
            f"if gcc {flags} -c -x assembler {quote(assembly)} -o {quote(unit)}"
            f" 2>{quote(unit + '.err')};"
            f" then echo 'OK {unit}'; else echo 'FAIL {unit}'; fi"
        )
    (asm_root / "assemble.sh").write_text("\n".join(lines) + "\n", encoding="utf-8")
    completed = container(asm_root, "asm", "bash /work/asm/assemble.sh", timeout=1800)
    verdicts = {}
    for line in completed.stdout.splitlines():
        parts = line.split(None, 1)
        if len(parts) == 2 and parts[0] in ("OK", "FAIL"):
            verdicts[parts[1]] = parts[0]
    for row in emitted:
        unit = row["unit"]
        if verdicts.get(unit) != "OK":
            error = asm_root / (unit + ".err")
            row["outcome"] = "assembler-refused"
            row["assemblerError"] = (
                error.read_text(errors="replace").strip()[:600]
                if error.is_file()
                else completed.stderr.strip()[-300:]
            )
            continue
        row["outcome"] = "assembled"
        row["objectSha256"] = sha256_file(asm_root / unit)


def defined_symbols(objects: list[str], work: Path) -> dict[str, list[str]]:
    """The global symbols each object defines, read by the container's nm."""
    if not objects:
        return {}
    (work / "nm-inputs.txt").write_text("\n".join(objects) + "\n", encoding="utf-8")
    script = (
        "cd /work/nm || exit 2\n"
        "while IFS= read -r name; do\n"
        '  echo "## $name"\n'
        '  nm --defined-only --extern-only "$name" 2>/dev/null | awk \'{print $NF}\'\n'
        "done < /work/nm/nm-inputs.txt\n"
    )
    completed = container(work, "nm", script, timeout=900)
    table: dict[str, list[str]] = {}
    current = None
    for line in completed.stdout.splitlines():
        if line.startswith("## "):
            current = line[3:]
            table[current] = []
        elif current is not None and line.strip():
            table[current].append(line.strip())
    return table


# --------------------------------------------------------------------------
# substitution, relink, test, bisection


def snapshot(tree: Path, objects: list[str], pristine: Path) -> None:
    """Keep the reference object of every unit, so a trial can start clean."""
    for unit in objects:
        source = tree / unit
        if not source.is_file():
            continue
        destination = pristine / unit
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)


def restore(tree: Path, objects: list[str], pristine: Path) -> None:
    for unit in objects:
        source = pristine / unit
        if source.is_file():
            shutil.copyfile(source, tree / unit)
            os.utime(tree / unit, None)


def install(tree: Path, units: list[str], asm_root: Path) -> None:
    """Install EvoCC's objects, newer than their sources so make keeps them."""
    for unit in units:
        shutil.copyfile(asm_root / unit, tree / unit)
        os.utime(tree / unit, None)


MULTIPLE_DEFINITION = re.compile(r"multiple definition of `([^']+)'")


def multiple_definitions(log: str) -> list[str]:
    return sorted(set(MULTIPLE_DEFINITION.findall(log)))


def without_collisions(rows: list[dict]) -> tuple[list[str], list[dict]]:
    """The largest set of objects that can enter one link together.

    A ``static`` function reaches EvoCC's symbol table as a global
    definition, because the front end does not emit the visibility marker
    yet. Two objects that saw the same static then define the same global:
    glibc's inline byte swaps are in every unit that includes them, and a
    subject that compiles one source twice has its own statics twice over.
    That is one known gap rather than a miscompile, so the units that would
    collide are named and set aside instead of being bisected for. Units are
    taken in order of how many symbols they promote, so the ones that make
    the fewest claims get in first.
    """
    assembled = [row for row in rows if row["outcome"] == "assembled"]
    claimed: dict[str, str] = {}
    keep: list[str] = []
    excluded: list[dict] = []
    for row in sorted(assembled, key=lambda row: (len(row.get("extraSymbols", [])), row["unit"])):
        clashes = [
            {"symbol": symbol, "with": claimed[symbol]}
            for symbol in row.get("extraSymbols", [])
            if symbol in claimed
        ]
        if clashes:
            excluded.append(
                {"unit": row["unit"], "reason": "visibility-drift", "collisions": clashes[:8]}
            )
            continue
        keep.append(row["unit"])
        for symbol in row.get("extraSymbols", []):
            claimed[symbol] = row["unit"]
    order = [row["unit"] for row in assembled]
    return sorted(keep, key=order.index), excluded


def recompiled(log: str, tree: str, units: list[str]) -> list[str]:
    """Substituted units the relink compiled again, read out of its own log.

    A subject whose suite runs on a copy of the tree can recompile an object
    where this side cannot see the bytes change, so the relink's own record
    of what it compiled is checked as well.
    """
    wanted = set(units)
    return sorted({unit.obj for unit in parse_build_log(log, tree) if unit.obj in wanted})


def rebuilt_by_make(tree: Path, units: list[str], asm_root: Path) -> list[str]:
    """Units whose object make replaced with one of its own.

    The claim is that these objects came from EvoCC, so it is checked rather
    than assumed: an installed object is re-read after the relink, and one
    whose bytes changed was recompiled by the subject's build and does not
    count.
    """
    rebuilt = []
    for unit in units:
        installed = tree / unit
        if not installed.is_file() or sha256_file(installed) != sha256_file(asm_root / unit):
            rebuilt.append(unit)
    return rebuilt


def trial(
    subject: Subject,
    tree: Path,
    candidates: list[str],
    subset: list[str],
    asm_root: Path,
    pristine: Path,
    evidence: Path,
    tag: str,
    generated_inputs: dict[str, tuple[list[str], str | None]] | None = None,
) -> dict:
    """Substitute exactly ``subset``, relink, and run the suite."""
    restore(tree, candidates, pristine)
    install(tree, subset, asm_root)
    linked = relink(subject, evidence, tag)
    if linked["status"] != 0:
        return {
            "units": subset,
            "passed": False,
            "stage": "relink",
            "multipleDefinitions": multiple_definitions(linked["log"]),
            "signature": _signature(linked["log"]),
            "relinkStatus": linked["status"],
        }
    workspace = subject.workspace or f"/work/{subject.name}/{subject.tree}"
    rebuilt = sorted(set(rebuilt_by_make(tree, subset, asm_root))
                     | set(recompiled(linked["log"], workspace, subset)))
    regenerated = []
    if rebuilt and generated_inputs is not None:
        changed = []
        for unit in rebuilt:
            argv, expected = generated_inputs.get(unit, ([], None))
            if expected is None or emission_digest(argv) != expected:
                changed.append(unit)
        if changed:
            return {"units": subset, "passed": False, "stage": "inputs-changed",
                    "rebuiltUnits": rebuilt,
                    "signature": "regenerated input no longer emits the installed assembly: " + ", ".join(changed)}
        regenerated = rebuilt
        # Do not restore or reinstall the library objects again: that would
        # regenerate the same test sources on every trial. One second relink
        # must keep these exact subject objects; otherwise fail closed.
        install(tree, rebuilt, asm_root)
        linked = relink(subject, evidence, tag + "-generated-inputs")
        if linked["status"] != 0:
            return {"units": subset, "passed": False, "stage": "relink",
                    "relinkStatus": linked["status"],
                    "signature": _signature(linked["log"])}
        rebuilt = sorted(set(rebuilt_by_make(tree, subset, asm_root))
                         | set(recompiled(linked["log"], workspace, subset)))
    if rebuilt:
        return {"units": subset, "passed": False, "stage": "rebuilt",
                "rebuiltUnits": rebuilt,
                "signature": "relink replaced subject objects: " + ", ".join(rebuilt)}
    tested = run_tests(subject, evidence, tag)
    rebuilt = rebuilt_by_make(tree, subset, asm_root)
    if rebuilt:
        return {
            "units": subset,
            "passed": False,
            "stage": "rebuilt",
            "rebuiltUnits": rebuilt,
            "signature": "test target replaced subject objects: " + ", ".join(rebuilt),
        }
    return {
        "units": subset,
        "passed": tested["passed"],
        "stage": "test" if not tested["passed"] else "pass",
        "signature": "" if tested["passed"] else _signature(tested["tail"]),
        "status": tested["status"],
        "relinkStatus": linked["status"],
        "regeneratedInputs": regenerated,
    }


def is_compiler_failure(verdict: dict) -> bool:
    """A build replacing our input is not a miscompile to delta-debug."""
    return not verdict["passed"] and verdict["stage"] in {"test", "relink"}


def final_trial(previous: dict, subset: list[str], rerun) -> dict:
    """Reuse only the immediately completed passing trial of this exact build."""
    if previous['passed'] and previous['units'] == subset:
        return previous
    return rerun()


def _signature(log: str) -> str:
    """The last few lines of a failure, which is what names it."""
    lines = [line for line in log.strip().splitlines() if line.strip()]
    return " | ".join(lines[-4:])[:400]


# --------------------------------------------------------------------------
# the run


def receipt_path(subject: Subject) -> Path:
    return ROOT / "tests" / "contracts" / f"ladder-{subject.name}-receipt.json"


def summarise(subject: Subject, rows: list[dict], substituted: list[str], tests: dict) -> str:
    refusals = collections.Counter(
        row["code"] for row in rows if row["outcome"] == "refused"
    )
    return (
        f"{subject.name}: {len(substituted) if tests.get('passed') else 0} of "
        f"{len(rows)} units by EvoCC in a passing build; "
        f"refused by code {dict(refusals.most_common())}"
    )


def measure(subject: Subject, args: argparse.Namespace) -> dict:
    """One complete measurement of one subject, from tarball to receipt."""
    evocc = Path(args.evocc)
    if not evocc.is_file():
        raise SystemExit(f"the release driver is missing: {evocc}")
    run_id = args.run_id or dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    evidence = EVIDENCE / subject.name / run_id
    evidence.mkdir(parents=True, exist_ok=True)

    archive, digest = fetch(subject)
    tree = unpack(subject)
    sysroot, adjustments = adjust_sysroot()
    include = e0.include_flags(sysroot)

    build = reference_build(subject, tree, evidence)
    if build["status"] != 0:
        raise SystemExit(f"{subject.name}: the reference build failed; see {evidence}")
    build_units = parse_build_log(build["log"], f"/work/{subject.name}/{subject.tree}")
    units = [
        unit
        for unit in build_units
        if unit.obj not in subject.skip_objects
        and (not subject.source_prefixes or unit.source.startswith(subject.source_prefixes))
    ]
    (evidence / "compile-commands.json").write_text(
        json.dumps([dataclasses.asdict(unit) for unit in units], indent=1) + "\n",
        encoding="utf-8",
    )
    if not units:
        raise SystemExit(f"{subject.name}: no compile commands were captured")

    build_symlinks = portable_build_symlinks(tree, f"/work/{subject.name}/{subject.tree}")
    baseline = run_tests(subject, evidence, "baseline")
    if not baseline["passed"]:
        raise SystemExit(f"{subject.name}: the reference baseline suite failed; see {evidence}")

    asm_root = BUILD / subject.name / "evocc-objects"
    if asm_root.is_dir():
        shutil.rmtree(asm_root)
    asm_root.mkdir(parents=True)
    rows = emit(subject, units, evocc, tree, include, asm_root)
    rejected_options = sorted({flag for row in rows for flag in row["omittedFlags"]
                               if flag.startswith("-std=")})
    assemble(rows, asm_root)

    assembled = [row["unit"] for row in rows if row["outcome"] == "assembled"]
    pristine = BUILD / subject.name / "pristine-objects"
    if pristine.is_dir():
        shutil.rmtree(pristine)
    pristine.mkdir(parents=True)
    snapshot(tree, [unit.obj for unit in units], pristine)

    # What each object defines, against what the reference object defines:
    # a symbol only EvoCC's object exports is a static the front end
    # published, and a symbol only the reference exports is a definition
    # that went missing.
    if assembled:
        pairs = []
        work = BUILD / subject.name / "nm"
        if work.is_dir():
            shutil.rmtree(work)
        work.mkdir(parents=True)
        for unit in assembled:
            for tag, origin in (("mine", asm_root), ("theirs", pristine)):
                destination = work / tag / unit
                destination.parent.mkdir(parents=True, exist_ok=True)
                if (origin / unit).is_file():
                    shutil.copyfile(origin / unit, destination)
                    pairs.append(f"{tag}/{unit}")
        table = defined_symbols(pairs, work)
        for row in rows:
            if row["outcome"] != "assembled":
                continue
            mine = set(table.get(f"mine/{row['unit']}", []))
            theirs = set(table.get(f"theirs/{row['unit']}", []))
            row["extraSymbols"] = sorted(mine - theirs)
            row["missingSymbols"] = sorted(theirs - mine)

    candidates = [unit.obj for unit in units]
    substituted, excluded = without_collisions(rows)
    generated_inputs = None
    if subject.refresh_generated_inputs:
        by_name = {row["unit"]: row for row in rows}
        generated_inputs = {
            unit.obj: (evocc_argv(unit, evocc, subject, tree, include)[0],
                       by_name[unit.obj].get("assemblySha256"))
            for unit in units
        }

    attempts: list[dict] = []
    verdict = {"passed": False, "stage": "not-run", "signature": "", "units": []}
    for attempt in range(1, 6):
        verdict = trial(
            subject, tree, candidates, substituted, asm_root, pristine, evidence,
            f"attempt-{attempt}",
            generated_inputs,
        )
        attempts.append({k: v for k, v in verdict.items() if k != "units"} | {"unitCount": len(substituted)})
        collisions = verdict.get("multipleDefinitions") or []
        if verdict["passed"] or not collisions:
            break
        guilty = _units_defining(collisions, rows)
        if not guilty:
            break
        for unit in guilty:
            excluded.append({"unit": unit, "reason": "multiple-definition", "symbols": collisions})
        substituted = [unit for unit in substituted if unit not in guilty]

    failing: list[dict] = []
    if is_compiler_failure(verdict) and substituted:
        minimal = ddmin(
            substituted,
            lambda subset: is_compiler_failure(trial(
                subject, tree, candidates, list(subset), asm_root, pristine, evidence,
                f"bisect-{len(subset)}",
                generated_inputs,
            )),
        )
        final = trial(
            subject, tree, candidates, list(minimal), asm_root, pristine, evidence, "minimal",
            generated_inputs,
        )
        if is_compiler_failure(final):
            failing = [
                {"unit": unit, "stage": final["stage"], "signature": final["signature"]}
                for unit in minimal
            ]
            substituted = [unit for unit in substituted if unit not in set(minimal)]

    final_tests = final_trial(verdict, substituted, lambda: trial(
        subject, tree, candidates, substituted, asm_root, pristine,
        evidence, "final", generated_inputs))
    rebuilt = final_tests.get("rebuiltUnits", [])
    substituted = [unit for unit in substituted if unit not in rebuilt]

    outcomes = collections.Counter(row["outcome"] for row in rows)
    refusals = collections.Counter(row["code"] for row in rows if row["outcome"] == "refused")
    summary = {
        "schemaVersion": 1,
        "rung": subject.rung,
        "subject": subject.name,
        "authority": "docs/roadmap.md, milestone ladder",
        "run": run_id,
        "pin": subject.pin,
        "url": subject.url,
        "tarballSha256": digest,
        "tarball": archive.name,
        "driverSha256": sha256_file(evocc),
        "target": TARGET,
        "unitsTotal": len(units),
        "unmeasuredBuildUnits": [dataclasses.asdict(unit) for unit in build_units if unit not in units],
        "unitsEvoccCompiled": outcomes["assembled"] + outcomes["emitted"],
        "unitsSubstituted": len(substituted),
        "unitsRebuiltByMake": rebuilt,
        "substitutedUnits": substituted,
        "unitsExcluded": len(excluded),
        "unitsInPassingBuild": len(substituted) if final_tests["passed"] else 0,
        "outcomes": dict(outcomes),
        "refusalsByCode": dict(refusals.most_common()),
        "refusals": [
            {
                "unit": row["unit"],
                "code": row["code"],
                "message": row["message"],
                "at": row.get("at", ""),
            }
            for row in rows
            if row["outcome"] == "refused"
        ],
        "excludedUnits": excluded,
        "failingUnits": failing,
        "baselineTest": {"passed": baseline["passed"], "status": baseline["status"]},
        "test": {
            "passed": final_tests["passed"],
            "status": final_tests.get("status"),
            "relinkStatus": final_tests.get("relinkStatus"),
            "stage": final_tests["stage"],
            "signature": final_tests["signature"],
            "regeneratedInputs": final_tests.get("regeneratedInputs", []),
            "command": subject.test,
            "completedTrial": f"attempt-{attempt}" if final_tests is verdict else "final",
            **({"objectFiles": list(subject.test_objects)} if subject.test_objects else {}),
        },
        "attempts": attempts,
        "adjustments": {
            "sysroot": adjustments,
            "portableBuildSymlinks": build_symlinks,
            "sysrootAdjustmentMeaning": SYSROOT_ADJUSTMENTS,
            "includeOrder": e0.include_flags(sysroot),
            "configure": subject.configure,
            "subject": list(subject.notes),
            "buildCommand": subject.build,
            "optionsTheDriverRejects": rejected_options,
            "omittedFlags": sorted(
                {flag for row in rows for flag in row.get("omittedFlags", [])}
            ),
        },
        "frontEndGaps": front_end_gaps(rows),
        "defects": list(subject.defects),
        "evidence": str(evidence.relative_to(ROOT)),
    }
    summary["summaryLine"] = summarise(subject, rows, substituted, final_tests)
    (evidence / "units.jsonl").write_text(
        "".join(json.dumps(row, sort_keys=True) + "\n" for row in rows), encoding="utf-8"
    )
    (evidence / "summary.json").write_text(json.dumps(summary, indent=1) + "\n", encoding="utf-8")
    receipt_path(subject).write_text(json.dumps(summary, indent=1) + "\n", encoding="utf-8")
    print(summary["summaryLine"], flush=True)
    return summary


def _units_defining(symbols: list[str], rows: list[dict]) -> list[str]:
    """Which substituted units define the symbols a link collision names."""
    wanted = set(symbols)
    return sorted(
        row["unit"]
        for row in rows
        if row["outcome"] == "assembled" and wanted & set(row.get("extraSymbols", []))
    )


def default_evocc() -> Path:
    """The release driver, honouring a shared ``CARGO_TARGET_DIR``.

    A lane often runs from a worktree, which has no ``target`` directory of
    its own; the driver being measured is the one the repository built.
    """
    target = os.environ.get("CARGO_TARGET_DIR")
    if target:
        return Path(target) / "release" / "evocc"
    local = ROOT / "target" / "release" / "evocc"
    if local.is_file():
        return local
    common = Path(
        run(["git", "-C", str(ROOT), "rev-parse", "--path-format=absolute", "--git-common-dir"]).stdout.strip()
    )
    return common.parent / "target" / "release" / "evocc"


def main(subject: Subject, argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=f"ladder rung {subject.rung}: {subject.name}")
    parser.add_argument("--evocc", default=str(default_evocc()))
    parser.add_argument("--run-id")
    args = parser.parse_args(argv)
    measure(subject, args)
    return 0
