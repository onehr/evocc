#!/usr/bin/env python3
"""Mixed-link machinery shared by the ladder's whole-program subjects.

The method is the one the kernel lane uses, generalized away from Kbuild.
A reference compiler builds the subject inside the sealed container; every
compilation it performs is recorded; EvoCC re-compiles each recorded unit on
the host; the emitted assembly is assembled inside the container with the
same assembler flags; the resulting objects are installed over the reference
objects; the subject relinks and runs its own test suite. What the receipt
reports is how many units EvoCC compiled in a build that still passes.

Three pieces are subject-independent and live here.

**Command capture.** Kbuild writes every argv into a ``.o.cmd`` file, so the
kernel lane can read the build after the fact. An autoconf or hand-written
Makefile writes nothing, so the build is asked to use a recording compiler:
a two-line shell script that appends its own argv to a log and then execs the
real GCC. It is not a substitution wrapper -- it never changes what is built,
only what is known about it -- and it survives ``configure``, recursive make
and parallel make, because each invocation appends one ``NUL``-separated
record under an exclusive append.

**Translation.** A recorded argv is written in container coordinates and in
GCC's dialect. It is turned into an EvoCC command by rewriting the mounted
prefix to its host path and keeping only the options the front end consumes;
every dropped option is named in the receipt rather than guessed at, because
a silently dropped ``-D`` would change the program.

**Bisection.** When the substituted build fails its tests, the responsible
units are found by delta debugging over the set of substituted units, reusing
``tools/k1/bisect.py``'s ``ddmin`` so the two lanes cannot drift apart in what
"minimal failing set" means.

Nothing here knows what a subject is called, how it is configured, or how it
is tested; that belongs in ``tinycc.py``, ``toybox.py`` and ``musl.py``.
"""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
import dataclasses
import hashlib
import importlib.util
import json
from pathlib import Path
import posixpath
import re
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
LADDER = ROOT / "tools" / "ladder"
sys.path.insert(0, str(LADDER))

from e0 import IMAGE, TARGET, SUPPORTED_OPTIONS, SUPPORTED_OPTION_PREFIXES, include_flags, sha256, sysroot  # noqa: E402,F401
from subject import exported_sysroot  # noqa: E402

#: ``.build`` and ``.evidence`` are resolved through any symlink, because the
#: front end looks an include path up as written and a path that reaches the
#: same directory through a link does not find the headers under it.
BUILD = (ROOT / ".build" / "ladder").resolve()
EVIDENCE = (ROOT / ".evidence" / "ladder").resolve()
CONTRACTS = ROOT / "tests" / "contracts"

#: The digest of a build that never ran, so a receipt can say so in one field.
UNKNOWN = "unknown"


def load_ddmin():
    """``ddmin`` from the kernel lane, without shadowing the stdlib ``bisect``.

    ``tools/k1/bisect.py`` is importable only by path: putting its directory
    on ``sys.path`` would make ``import bisect`` find it instead of the
    standard library's, and the standard library's is what ``ddmin``'s
    callers, and the interpreter's own startup, expect to get.
    """
    path = ROOT / "tools" / "k1" / "bisect.py"
    spec = importlib.util.spec_from_file_location("k1_bisect", path)
    if spec is None or spec.loader is None:  # pragma: no cover - path is fixed
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.ddmin


# --------------------------------------------------------------------------
# The sealed container
# --------------------------------------------------------------------------


def container(mounts: dict[str, Path], script: str, timeout: int = 3600, *,
              capabilities: tuple[str, ...] = ()):
    """Run one ``bash -c`` script in the sealed image with ``mounts`` bound.

    ``mounts`` maps the name under ``/work`` to the host directory. The image
    has no network and the run is removed afterwards, so the only thing that
    outlives it is what the script wrote into a mounted directory.
    """
    argv = ["container", "run", "--rm", "--platform", "linux/arm64", "--network", "none"]
    for capability in capabilities:
        argv += ["--cap-add", capability]
    for name, host in mounts.items():
        argv += ["-v", f"{host}:/work/{name}"]
    argv += [IMAGE, "bash", "-c", script]
    return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)


# --------------------------------------------------------------------------
# The exported sysroot
# --------------------------------------------------------------------------

#: Every adapter sees the original exported headers. Unsupported declarations
#: are measured refusals, never edits to the reference preprocessor branches.
SYSROOT_ADJUSTMENTS = []

#: No source-language token is erased through command-line definitions.
SYSROOT_DEFINES = []

def ladder_sysroot() -> tuple[Path, list[str]]:
    """Reuse the common unmodified export, not a legacy rewritten copy."""
    root = exported_sysroot()
    return root, include_flags(root)


# --------------------------------------------------------------------------
# Source adjustments
# --------------------------------------------------------------------------

# --------------------------------------------------------------------------
# Command capture
# --------------------------------------------------------------------------

#: A recording compiler. It writes its argv NUL-separated with a record
#: terminator, holding one file lock across both writes so concurrent
#: ``make -j`` writers append whole records, and then becomes the real compiler,
#: so the build it records is
#: byte-for-byte the build that would have happened without it.
RECORDER = r"""#!/bin/sh
: "${EVO_CMDLOG:=/dev/null}"
{
    flock -x 9 || exit 1
    printf '%s\0' "$PWD" "$@" >&9
    printf '\1\n' >&9
} 9>> "$EVO_CMDLOG"
exec {real} "$@"
"""


def install_recorder(work: Path, log_name: str = "commands.log", real: str = "gcc-14") -> Path:
    """Write the recording compiler into ``work`` and return its container path."""
    script = work / "evo-recordcc"
    script.write_text(RECORDER.replace("{real}", real), encoding="utf-8")
    script.chmod(0o755)
    (work / log_name).write_text("", encoding="utf-8")
    return script


@dataclasses.dataclass(frozen=True)
class Unit:
    """One recorded compilation of one C source into one object."""

    cwd: str
    """The directory the compiler ran in, in container coordinates."""
    argv: tuple[str, ...]
    source: str
    """The source operand exactly as recorded."""
    obj: str
    """The ``-o`` operand exactly as recorded."""

    root: str = ""
    """The container path of the mounted subject tree, stripped from names."""

    @property
    def name(self) -> str:
        """A stable identity for the unit: its object path, mount-relative."""
        obj = self.obj_container_path()
        if self.root and obj.startswith(self.root.rstrip("/") + "/"):
            return obj[len(self.root.rstrip("/")) + 1 :]
        return obj.lstrip("/")

    def obj_container_path(self) -> str:
        return _absolute(self.cwd, self.obj)

    def source_container_path(self) -> str:
        return _absolute(self.cwd, self.source)


def _absolute(cwd: str, value: str) -> str:
    """One container-relative operand as a normalized absolute path.

    ``make -C`` and toybox's ``-o ../obj/x.o`` both produce operands with a
    ``..`` in them; leaving it in would give the same object two names and
    count it twice.
    """
    if not value.startswith("/"):
        value = f"{cwd}/{value}"
    return posixpath.normpath(value)


C_SOURCE = re.compile(r"\.c$")


def parse_commands(log: Path, root: str = "") -> list[Unit]:
    """Decode the recorder's log into the C compilations it captured.

    A record is kept only when it compiles exactly one C source to exactly
    one named object. Preprocessing (``-E``), dependency-only runs (``-M``
    without ``-c``), links, and ``configure``'s probe compiles into ``a.out``
    are not units of the program and are dropped. The last record for an
    object wins, because a rebuild supersedes what it replaced.

    ``.lo`` counts as an object. It is musl's name for the same compilation
    with ``-fPIC``, and when musl is configured without a shared library it
    is what ``libc.a`` is made of, so refusing the suffix would report a libc
    with three units in it.

    When ``root`` is given, a record that writes its object outside the
    mounted subject tree is dropped: a subject's own test suite is often
    built by the same recorded compiler, and its objects are not units of
    the subject.
    """
    units: dict[str, Unit] = {}
    raw = log.read_bytes()
    for chunk in raw.split(b"\x01\n"):
        chunk = chunk.strip(b"\n")
        if not chunk:
            continue
        fields = [field.decode("utf-8", "replace") for field in chunk.split(b"\x00") if field]
        if len(fields) < 2:
            continue
        cwd, argv = fields[0], tuple(fields[1:])
        if "-c" not in argv or "-E" in argv:
            continue
        sources = [a for a in argv if C_SOURCE.search(a) and not a.startswith("-")]
        if len(sources) != 1:
            continue
        try:
            obj = argv[argv.index("-o") + 1]
        except (ValueError, IndexError):
            continue
        if not obj.endswith((".o", ".lo")):
            continue
        unit = Unit(cwd=cwd, argv=argv, source=sources[0], obj=obj, root=root)
        if root and not unit.obj_container_path().startswith(root.rstrip("/") + "/"):
            continue
        units[unit.name] = unit
    return sorted(units.values(), key=lambda u: u.name)


# --------------------------------------------------------------------------
# Translation into an EvoCC command
# --------------------------------------------------------------------------

#: Options whose value is the following argument.
SEPARATE_VALUE = ("-I", "-D", "-U", "-include", "-imacros", "-isystem", "-iquote")

#: Of those, the ones whose value is a path that must be rewritten.
PATH_VALUED = ("-I", "-include", "-imacros", "-isystem", "-iquote")

#: Options the front end takes as written.
PASSED_THROUGH = SUPPORTED_OPTIONS | {"-nostdinc", "-undef"}

#: Options whose value is the following argument and which the front end has
#: no use for; both words are dropped together so the value is never mistaken
#: for the input file.
DROP_WITH_VALUE = ("-o", "-MF", "-MT", "-MQ", "-x", "-Xassembler", "-aux-info")


def to_host(value: str, mount: str, host: Path, cwd: str) -> str:
    """Rewrite one container path to its host equivalent."""
    prefix = f"/work/{mount}"
    value = _absolute(cwd, value)
    if value == prefix:
        return str(host)
    if value.startswith(prefix + "/"):
        return str(host / value[len(prefix) + 1 :])
    return value


def evocc_command(
    unit: Unit,
    evocc: Path,
    mount: str,
    host: Path,
    extra: list[str] | None = None,
    target: str = TARGET,
) -> tuple[list[str], list[str]]:
    """The EvoCC command for one unit, and the options it does not consume.

    ``-isystem`` and ``-iquote`` become ``-I`` because the front end has one
    include path list; the ordering within that list is preserved, which is
    what a subject that shadows a system header depends on.
    """
    argv = list(unit.argv)
    command = [str(evocc), "--emit=asm", "--survey", f"--target={target}"]
    omitted: list[str] = []
    index = 0
    while index < len(argv):
        value = argv[index]
        if value == unit.source:
            index += 1
            continue
        if value in DROP_WITH_VALUE:
            omitted.append(value)
            index += 2
            continue
        if value in SEPARATE_VALUE:
            if index + 1 >= len(argv):
                omitted.append(value)
                break
            following = argv[index + 1]
            if value in PATH_VALUED:
                following = to_host(following, mount, host, unit.cwd)
            command.extend(("-I" if value in ("-isystem", "-iquote") else value, following))
            index += 2
            continue
        if value.startswith("-I") and len(value) > 2:
            command.append("-I" + to_host(value[2:], mount, host, unit.cwd))
        elif value.startswith("-isystem") and len(value) > 8:
            command.append("-I" + to_host(value[8:], mount, host, unit.cwd))
        elif value.startswith(("-D", "-U")) and len(value) > 2:
            command.append(value)
        elif value.startswith("-include") and len(value) > 8:
            command.extend(("-include", to_host(value[8:], mount, host, unit.cwd)))
        elif value in PASSED_THROUGH:
            command.append(value)
        elif value.startswith(("-std=", *SUPPORTED_OPTION_PREFIXES)):
            command.append(value)
        elif value != "-c":
            omitted.append(value)
        index += 1
    command.extend(extra or [])
    command.append(to_host(unit.source, mount, host, unit.cwd))
    return command, omitted


DIAGNOSTIC = re.compile(r"error\[(?P<code>[A-Z]\d+)\]: (?P<message>[^\n]*)")


def diagnostics(text: str) -> list[tuple[str, str]]:
    """Every distinct ``code, message`` pair in a diagnostic stream."""
    seen: list[tuple[str, str]] = []
    for match in DIAGNOSTIC.finditer(text):
        pair = (match.group("code"), match.group("message").strip()[:160])
        if pair not in seen:
            seen.append(pair)
    return seen


# --------------------------------------------------------------------------
# The front-end pass
# --------------------------------------------------------------------------


def emit_one(unit: Unit, command: list[str], asm_dir: Path, timeout: int = 300) -> dict:
    """Run EvoCC over one unit and classify the outcome."""
    row: dict = {"unit": unit.name, "source": unit.source_container_path()}
    try:
        done = subprocess.run(command, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        row["outcome"] = "timeout"
        return row
    if done.returncode == 0 and done.stdout.strip():
        path = asm_dir / (flat_name(unit.name) + ".s")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(done.stdout, encoding="utf-8")
        row["outcome"] = "emitted"
        row["assembly"] = path.name
        return row
    found = diagnostics(done.stderr)
    row["outcome"] = "refused"
    if found:
        row["codes"] = [code for code, _ in found]
        row["code"] = found[0][0]
        row["message"] = found[0][1]
    elif "panicked" in done.stderr:
        row["code"] = "panic"
        row["message"] = done.stderr.strip().splitlines()[-1][:160]
    else:
        row["code"] = "other"
        row["message"] = (done.stderr.strip() or done.stdout.strip())[-160:]
    return row


def emit_all(
    units: list[Unit],
    evocc: Path,
    mount: str,
    host: Path,
    asm_dir: Path,
    extra: list[str] | None = None,
    jobs: int = 6,
    timeout: int = 300,
    progress: bool = True,
) -> list[dict]:
    """Run the front end over every unit, recording what each one did."""
    asm_dir.mkdir(parents=True, exist_ok=True)
    prepared = []
    for unit in units:
        command, omitted = evocc_command(unit, evocc, mount, host, extra)
        prepared.append((unit, command, omitted))
    rows: list[dict] = []
    with ThreadPoolExecutor(max_workers=jobs) as pool:
        futures = [
            pool.submit(emit_one, unit, command, asm_dir, timeout)
            for unit, command, _ in prepared
        ]
        for index, (future, (_, command, omitted)) in enumerate(zip(futures, prepared), 1):
            row = future.result()
            row["omitted"] = omitted
            row["command"] = shlex.join(command)
            rows.append(row)
            if progress and index % 25 == 0:
                print(f"  emitted {index}/{len(prepared)}", flush=True)
    return rows


# --------------------------------------------------------------------------
# Assembly and substitution inside the container
# --------------------------------------------------------------------------


def flat_name(unit: str) -> str:
    """One unit's name as a single path component, for a flat object store.

    The mapping is one-way on purpose. ``obj/src/env/__environ.lo`` and
    ``obj/src/env/`` + ``__environ.lo`` flatten to the same spelling, so a
    verdict must be reported against the unit's real name rather than
    recovered by undoing the substitution; musl has ninety-odd units whose
    basename starts with two underscores, and reading them back wrongly
    reported every one of them as assembly the assembler refused.
    """
    return unit.replace("/", "__")


def assembler_flags(unit: Unit) -> list[str]:
    """Both driver spellings, with their operands and written override order."""
    flags: list[str] = []
    index = 0
    while index < len(unit.argv):
        value = unit.argv[index]
        if value == '-Xassembler':
            if index + 1 == len(unit.argv):
                raise ValueError(f'missing -Xassembler operand in {unit.name}')
            flags.extend(unit.argv[index:index + 2])
            index += 2
            continue
        if value.startswith('-Wa,'):
            flags.append(value)
        index += 1
    return flags


ASSEMBLE = r"""#!/bin/sh
cd /work/asm || exit 1
exec python3 - <<'PY'
import json
from pathlib import Path
import subprocess

with Path('assemble.log').open('w') as log:
    for row in json.loads(Path('plan.json').read_text()):
        command = ['gcc-14', *row['flags'], '-c', '-x', 'assembler',
                   row['assembly'], '-o', 'obj/' + row['object']]
        done = subprocess.run(command, stdout=log, stderr=log)
        verdict = 'ok' if done.returncode == 0 else 'assembler-refused'
        print(row['unit'], verdict, flush=True)
PY
"""


def assemble(rows: list[dict], units: dict[str, Unit], asm_dir: Path) -> dict[str, str]:
    """Assemble every emitted unit inside the container.

    The object is named after the unit with ``/`` flattened, so a flat
    directory can hold objects from a tree with repeated basenames.
    """
    emitted = [row for row in rows if row["outcome"] == "emitted"]
    if not emitted:
        return {}
    (asm_dir / "obj").mkdir(exist_ok=True)
    plan = []
    for row in emitted:
        plan.append({'unit': row['unit'], 'object': flat_name(row['unit']) + '.o',
                     'assembly': row['assembly'], 'flags': assembler_flags(units[row['unit']])})
    (asm_dir / "plan.json").write_text(json.dumps(plan, indent=2) + "\n", encoding="utf-8")
    (asm_dir / "assemble.sh").write_text(ASSEMBLE, encoding="utf-8")
    done = container({"asm": asm_dir}, "sh /work/asm/assemble.sh", timeout=3600)
    if done.returncode != 0:
        return {}
    return _assembly_verdicts(done.stdout)


def _assembly_verdicts(output: str) -> dict[str, str]:
    """One verdict per unit, keyed by the unit name the script echoed back."""
    verdicts = {}
    for line in output.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[1] in ("ok", "assembler-refused"):
            verdicts[parts[0]] = parts[1]
    return verdicts


def multiple_definition_symbols(link_log: str) -> set[str]:
    """The symbols a link rejected for being defined more than once."""
    return {m.group(1) for m in re.finditer(r"multiple definition of `([^']+)'", link_log)}


SYMBOLS = r"""#!/bin/sh
cd /work/asm || exit 1
while IFS='|' read -r unit flat ref; do
  [ -n "$unit" ] || continue
  nm -g --defined-only "obj/$flat" 2>/dev/null | awk -v u="$unit" '{print u "|E|" $NF}'
  if [ -f "/work/tree/$ref" ]; then
    nm -g --defined-only "/work/tree/$ref" 2>/dev/null | awk -v u="$unit" '{print u "|R|" $NF}'
  fi
done < symbols.txt
"""


def symbol_parity(rows: list[dict], tree: Path, asm_dir: Path) -> dict[str, dict]:
    """How each EvoCC object's global symbols differ from the reference one's.

    ``added`` is a symbol the EvoCC object defines and the reference object
    does not. The front end does not yet emit the visibility marker, so a
    ``static`` function -- typically a ``static inline`` helper out of a
    shared header -- reaches the object file as a global symbol. One such
    object links; two of them, defining the same helper, do not.

    ``missing`` is a symbol the reference object defines and the EvoCC object
    does not. Every one of them found so far is an alias: musl writes
    ``weak_alias(__environ, environ)`` and the front end emits the target
    without the alias, so the object links but ``environ`` is never defined
    by it. A unit like that is not a substitution, it is a different program,
    and it is set aside rather than measured.
    """
    assembled = [row for row in rows if row["outcome"] == "assembled"]
    if not assembled:
        return {}
    plan = [f"{row['unit']}|{flat_name(row['unit'])}.o|{row['unit']}" for row in assembled]
    (asm_dir / "symbols.txt").write_text("\n".join(plan) + "\n", encoding="utf-8")
    (asm_dir / "symbols.sh").write_text(SYMBOLS, encoding="utf-8")
    done = container({"asm": asm_dir, "tree": tree}, "sh /work/asm/symbols.sh", timeout=1800)
    evocc: dict[str, set[str]] = {}
    reference: dict[str, set[str]] = {}
    for line in done.stdout.splitlines():
        parts = line.split("|")
        if len(parts) != 3:
            continue
        unit, side, symbol = parts
        (evocc if side == "E" else reference).setdefault(unit, set()).add(symbol)
    parity = {}
    for unit in (row["unit"] for row in assembled):
        mine, theirs = evocc.get(unit, set()), reference.get(unit, set())
        parity[unit] = {
            "added": sorted(mine - theirs),
            "missing": sorted(theirs - mine),
        }
    return parity


def compatible_units(parity: dict[str, dict]) -> tuple[list[str], dict[str, str]]:
    """Split the assembled units into a set that can link and the rest.

    A unit that does not define everything the reference object defined is
    set aside first: the link would silently lose a symbol. Of what is left,
    two units that added the same symbol cannot both be installed, so the
    units are taken in order and one is set aside as soon as a symbol it
    added has already been claimed. This counts the visibility gap instead of
    bisecting it: the link error it causes is understood, and delta debugging
    would only rediscover that any two such units conflict.
    """
    claimed: dict[str, str] = {}
    keep: list[str] = []
    excluded: dict[str, str] = {}
    for unit in sorted(parity):
        missing = parity[unit].get("missing") or []
        if missing:
            excluded[unit] = f"does not define {missing[0]}"
            continue
        added = parity[unit].get("added") or []
        clash = next((s for s in added if s in claimed), None)
        if clash is not None:
            excluded[unit] = f"{clash} also defined by {claimed[clash]}"
            continue
        keep.append(unit)
        for symbol in added:
            claimed[symbol] = unit
    return keep, excluded


# --------------------------------------------------------------------------
# Substitution and bisection
# --------------------------------------------------------------------------


def audit_objects(tree: Path, expected: dict[str, str]) -> list[str]:
    """An installed target object may not be replaced by a reference rebuild."""
    return [unit for unit, digest in expected.items()
            if not (tree / unit).is_file() or sha256(tree / unit) != digest]


class Tree:
    """One subject tree, with the reference objects kept aside.

    Substitution is by installed object file, exactly as the kernel lane does
    it: make sees an object newer than its source and relinks around it. The
    reference object is copied into ``pristine`` before the first
    substitution, so any candidate set can be installed from a clean base.
    """

    def __init__(self, root: Path, pristine: Path) -> None:
        self.root = root
        self.pristine = pristine
        self.pristine.mkdir(parents=True, exist_ok=True)

    def keep(self, unit: str) -> None:
        target = self.pristine / unit
        if not target.exists():
            source = self.root / unit
            if source.exists():
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(source.read_bytes())

    def install(self, unit: str, obj: Path) -> None:
        self.keep(unit)
        destination = self.root / unit
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(obj.read_bytes())

    def restore_all(self) -> None:
        """Put every reference object back, whatever the suffix is.

        musl's objects end in ``.lo``; a glob for ``*.o`` misses them, and a
        restore that misses an object leaves the previous round's EvoCC code
        in the archive, which makes every candidate set look guilty of the
        same failure.
        """
        for saved in self.pristine.rglob("*"):
            if saved.is_file():
                (self.root / saved.relative_to(self.pristine)).write_bytes(
                    saved.read_bytes()
                )

    def touch(self, unit: str) -> None:
        """Make the installed object newer than everything it depends on."""
        (self.root / unit).touch()


def substitute(tree: Tree, units: list[str], asm_dir: Path) -> list[str]:
    """Install the EvoCC objects for ``units``, restoring everything else."""
    tree.restore_all()
    installed = []
    for unit in units:
        obj = asm_dir / "obj" / (flat_name(unit) + ".o")
        if not obj.exists():
            continue
        tree.install(unit, obj)
        tree.touch(unit)
        installed.append(unit)
    return installed


def narrow(units: list[str], fails, journal: Path, budget: int = 200) -> dict:
    """Find every unit that has to be dropped before the subject's tests pass.

    Delta debugging answers "which units, together, break this build"; it
    does not answer "which units break it", because removing one 1-minimal
    failing set can leave another behind. So the search runs until the
    remainder passes: minimize, drop, re-measure. ``fails`` substitutes a
    candidate set, relinks, runs the tests and answers whether they failed;
    every probe is journalled, so a receipt shows the search and not only its
    answer.
    """
    ddmin = load_ddmin()
    rounds: list[dict] = []
    cache: dict[tuple[str, ...], tuple[bool, str]] = {}

    def probe(candidate) -> bool:
        key = tuple(sorted(candidate))
        if key in cache:
            return cache[key][0]
        if len(rounds) >= budget:
            raise TimeoutError("bisection budget exhausted")
        verdict, note = fails(list(key))
        cache[key] = (verdict, note)
        if not verdict:
            note_passing(key)
        record = {
            "round": len(rounds) + 1,
            "unitCount": len(key),
            "units": list(key),
            "failed": verdict,
            "note": note,
        }
        rounds.append(record)
        with journal.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(record, sort_keys=True) + "\n")
        print(
            f"  bisect round {record['round']}: {len(key)} units -> "
            f"{'FAIL' if verdict else 'PASS'} ({note})",
            flush=True,
        )
        return verdict

    remaining = list(units)
    failing: list[str] = []
    minimal_sets: list[list[str]] = []
    exhausted = False
    best: list[str] = []

    def note_passing(candidate: tuple[str, ...]) -> None:
        nonlocal best
        if len(candidate) > len(best):
            best = list(candidate)

    try:
        while remaining and probe(remaining):
            minimal = ddmin(remaining, probe)
            if not minimal:
                break
            minimal_sets.append(minimal)
            failing.extend(minimal)
            remaining = [unit for unit in remaining if unit not in set(minimal)]
    except TimeoutError:
        exhausted = True
    return {
        "passingSet": best if exhausted else remaining,
        "failingUnits": failing,
        "minimalFailingSets": minimal_sets,
        "budgetExhausted": exhausted,
        "rounds": rounds,
    }


# --------------------------------------------------------------------------
# Receipts
# --------------------------------------------------------------------------


def refusal_table(rows: list[dict]) -> dict[str, int]:
    """Refusals by first diagnostic code, most common first."""
    counts: dict[str, int] = {}
    for row in rows:
        if row["outcome"] == "refused":
            counts[row.get("code", "other")] = counts.get(row.get("code", "other"), 0) + 1
    return dict(sorted(counts.items(), key=lambda item: (-item[1], item[0])))


def omitted_table(rows: list[dict]) -> dict[str, int]:
    """How often each unconsumed GCC option was dropped from a command."""
    counts: dict[str, int] = {}
    for row in rows:
        for option in dict.fromkeys(row.get("omitted", ())):
            counts[option] = counts.get(option, 0) + 1
    return dict(sorted(counts.items(), key=lambda item: (-item[1], item[0])))


def write_receipt(subject: str, payload: dict) -> Path:
    """Seal the subject's receipt under ``tests/contracts``."""
    path = CONTRACTS / f"ladder-{subject}-receipt.json"
    path.write_text(json.dumps(payload, indent=1, sort_keys=False) + "\n", encoding="utf-8")
    return path


def summary_line(subject: str, substituted: int, total: int, refusals: dict[str, int]) -> str:
    """The one line every subject prints when it is done."""
    return (
        f"{subject}: {substituted} of {total} units by EvoCC in a passing build; "
        f"refused by code {refusals}"
    )


def digest_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()
