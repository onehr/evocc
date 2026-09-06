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

from e0 import IMAGE, TARGET, include_flags, sha256, sysroot  # noqa: E402,F401

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


def container(mounts: dict[str, Path], script: str, timeout: int = 3600):
    """Run one ``bash -c`` script in the sealed image with ``mounts`` bound.

    ``mounts`` maps the name under ``/work`` to the host directory. The image
    has no network and the run is removed afterwards, so the only thing that
    outlives it is what the script wrote into a mounted directory.
    """
    argv = ["container", "run", "--rm", "--platform", "linux/arm64", "--network", "none"]
    for name, host in mounts.items():
        argv += ["-v", f"{host}:/work/{name}"]
    argv += [IMAGE, "bash", "-c", script]
    return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)


# --------------------------------------------------------------------------
# The exported sysroot
# --------------------------------------------------------------------------

#: What the exported sysroot has done to it before a hosted subject is
#: compiled against it, and why. Every entry is a header the front end cannot
#: parse today; none of them changes the meaning of a program that the front
#: end *can* compile, and all of them are named in the receipt.
SYSROOT_ADJUSTMENTS = [
    {
        "file": "usr/include/aarch64-linux-gnu/bits/floatn-common.h",
        "change": "__HAVE_FLOAT32, __HAVE_FLOAT64 and __HAVE_FLOAT32X set to 0",
        "why": "the TS 18661 _FloatN types are not in the front end's type system,"
        " and their prototypes derail the parse of <stdlib.h>",
    },
    {
        "file": "usr/include/aarch64-linux-gnu/bits/floatn.h",
        "change": "__HAVE_FLOAT128 set to 0",
        "why": "same, for _Float128",
    },
    {
        "file": "usr/include/aarch64-linux-gnu/bits/math-vector.h",
        "change": "the AdvSIMD and SVE guards are forced closed",
        "why": "the SIMD math prototypes are written in GCC's built-in vector"
        " types (__Float32x4_t, __SVFloat32_t), which the front end has no"
        " syntax for",
    },
    {
        "file": "every exported header",
        "change": "form feeds and vertical tabs are deleted",
        "why": "C makes both white space (C11 6.4p1); the front end reports a"
        " form feed as a stray token, and glibc's regex.h uses one as a page"
        " break, which alone refuses every unit that includes it",
    },
    {
        "file": "gccinc/stdint.h",
        "change": "renamed out of the search path",
        "why": "it is a wrapper whose whole body is `#include_next <stdint.h>`,"
        " and the front end ignores #include_next, so keeping it hides the C"
        " library's real stdint.h and leaves intmax_t undeclared",
    },
]

#: Definitions handed to the front end for every hosted unit.
#:
#: ``__extension__`` only suppresses pedantic warnings in GCC, so erasing it
#: changes nothing a program can observe; the front end has no keyword for it
#: and the glibc thread and atomic headers use it in declaration position.
#:
#: ``__STDC_NO_VLA__`` is the standard's own way for an implementation to say
#: it has no variable-length arrays, which is true of the front end today
#: (E2203). glibc's regex.h reads it and drops the ``regmatch_t
#: __pmatch[__nmatch]`` parameter form when it is set; without it, every unit
#: that includes <regex.h> is refused. The front end should predefine this
#: itself, and that it does not is filed with the run.
SYSROOT_DEFINES = ["-D__extension__=", "-D__STDC_NO_VLA__=1"]

EXPORT = r"""
set -u
mkdir -p /work/sysroot/usr /work/sysroot/gccinc
cp -a /usr/include /work/sysroot/usr/include 2>/dev/null || true
cp -a /usr/include/. /work/sysroot/usr/include/ 2>/dev/null || true
mkdir -p /work/sysroot/usr/lib
cp -a /usr/lib/linux /work/sysroot/usr/lib/ 2>/dev/null || true
G=$(gcc-14 -print-file-name=include)
cp -a $G/. /work/sysroot/gccinc/ 2>/dev/null || true
echo exported
"""


def _adjust_sysroot(root: Path) -> None:
    """Apply :data:`SYSROOT_ADJUSTMENTS` to a freshly exported sysroot."""
    bits = root / "usr/include/aarch64-linux-gnu/bits"
    common = bits / "floatn-common.h"
    if common.exists():
        text = common.read_text(errors="replace")
        for name in ("__HAVE_FLOAT32", "__HAVE_FLOAT64", "__HAVE_FLOAT32X"):
            text = text.replace(f"#define {name} 1\n", f"#define {name} 0\n")
        common.write_text(text)
    floatn = bits / "floatn.h"
    if floatn.exists():
        floatn.write_text(
            floatn.read_text(errors="replace").replace(
                "# define __HAVE_FLOAT128 1\n", "# define __HAVE_FLOAT128 0\n"
            )
        )
    vector = bits / "math-vector.h"
    if vector.exists():
        text = vector.read_text(errors="replace")
        for guard in (
            "#if __GNUC_PREREQ(9, 0)\n",
            "#if __GNUC_PREREQ(10, 0) || __glibc_clang_prereq(11, 0)\n",
        ):
            text = text.replace(guard, "#if 0 /* EvoCC ladder */\n")
        vector.write_text(text)
    wrapper = root / "gccinc" / "stdint.h"
    if wrapper.exists():
        wrapper.rename(root / "gccinc" / "stdint-wrapper.h.disabled")
    strip_page_breaks(root, suffixes=(".h",))


def ladder_sysroot(name: str = "sysroot-l3") -> tuple[Path, list[str]]:
    """Export the container's headers once and return the include flags.

    The export is a plain copy rather than a tar pipe, because a tar pipe
    silently dropped the ``asm/`` tree in an earlier export and left every
    ``<linux/...>`` include unresolvable. Two kinds of file do not survive the
    copy onto a case-insensitive host filesystem -- ``ipt_ECN.h`` beside
    ``ipt_ecn.h`` and its two siblings under ``linux/netfilter*`` -- and
    nothing on the ladder includes them.
    """
    root = (BUILD / name).resolve()
    if not (root / "usr/include/aarch64-linux-gnu/asm/errno.h").exists():
        root.mkdir(parents=True, exist_ok=True)
        done = container({"sysroot": root}, EXPORT, timeout=1800)
        if "exported" not in done.stdout:
            raise RuntimeError(f"sysroot export failed: {done.stdout}\n{done.stderr}")
        _adjust_sysroot(root)
    return root, [
        f"-I{root}/gccinc",
        f"-I{root}/usr/include/aarch64-linux-gnu",
        f"-I{root}/usr/include",
    ]


# --------------------------------------------------------------------------
# Source adjustments
# --------------------------------------------------------------------------

WHITESPACE_CONTROLS = str.maketrans("", "", "\f\v")


def strip_page_breaks(tree: Path, suffixes=(".c", ".h", ".def")) -> list[str]:
    """Remove form feeds and vertical tabs from a subject's sources.

    C makes both of them white space (C11 5.1.1.2p1, 6.4p1), and old GNU
    sources use a form feed as a page break. The front end reports one as a
    stray token, which destroys the translation unit; deleting them is a
    change no conforming compiler can observe, so it is applied to the tree
    the reference compiler builds as well, and the files touched are listed
    in the receipt.
    """
    touched: list[str] = []
    for path in sorted(tree.rglob("*")):
        if path.suffix not in suffixes or not path.is_file():
            continue
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        if "\f" not in text and "\v" not in text:
            continue
        path.write_text(text.translate(WHITESPACE_CONTROLS))
        touched.append(str(path.relative_to(tree)))
    return touched


def escape_high_bytes(tree: Path, suffixes=(".c", ".h")) -> list[str]:
    """Re-spell bytes above 0x7F so a file is valid UTF-8 without changing it.

    The front end reads a source as UTF-8. In the primary source file a byte
    that is not valid UTF-8 is reported (E0002); in an *included* file it is
    not: the include silently yields nothing, the header's macros are all
    missing, and the unit fails somewhere else entirely or, worse, compiles
    to something different. toybox's generated ``flags.h`` holds three such
    bytes inside ``OPTSTR_ls``, where they are option markers the generator
    wrote out literally instead of as escapes.

    Each high byte inside a line that contains a string literal is rewritten
    as a three-digit octal escape, which denotes exactly the same character
    and cannot run into the following character the way ``\\x`` can; a high
    byte in a line with no string literal is a comment and becomes ``?``.
    """
    touched: list[str] = []
    for path in sorted(tree.rglob("*")):
        if path.suffix not in suffixes or not path.is_file():
            continue
        raw = path.read_bytes()
        try:
            raw.decode("utf-8")
            continue
        except UnicodeDecodeError:
            pass
        lines = []
        for line in raw.split(b"\n"):
            if any(byte > 127 for byte in line):
                quoted = b'"' in line
                rewritten = bytearray()
                for byte in line:
                    if byte <= 127:
                        rewritten.append(byte)
                    elif quoted:
                        rewritten += b"\\%03o" % byte
                    else:
                        rewritten.append(ord("?"))
                line = bytes(rewritten)
            lines.append(line)
        path.write_bytes(b"\n".join(lines))
        touched.append(str(path.relative_to(tree)))
    return touched


#: Function specifiers the front end lexes as keywords but does not accept in
#: a declaration, and which therefore cannot be defined away with ``-D``.
UNSUPPORTED_SPECIFIERS = ("_Noreturn",)


def strip_specifiers(
    tree: Path,
    tokens: tuple[str, ...] = UNSUPPORTED_SPECIFIERS,
    suffixes=(".c", ".h"),
) -> list[str]:
    """Delete function specifiers the front end cannot parse.

    ``_Noreturn`` tells the compiler a function never returns; deleting it
    can cost an optimization or a warning and cannot change what a program
    computes. It has to be deleted rather than defined away, because the
    front end lexes it as a keyword, so ``-D_Noreturn=`` never reaches it --
    and then refuses it in declaration position. As with the page breaks, the
    deletion is applied to the tree the reference compiler builds too, so the
    two compilers still see the same program.

    Two places are left alone, because the token there is data rather than a
    specifier: a string or character literal, which is how tcc spells its own
    keyword table, and a preprocessor directive, which is how musl's
    ``features.h`` defines ``_Noreturn`` for compilers that predate C11.
    """
    pattern = re.compile(r"\b(?:" + "|".join(re.escape(t) for t in tokens) + r")\b")
    touched: list[str] = []
    for path in sorted(tree.rglob("*")):
        if path.suffix not in suffixes or not path.is_file():
            continue
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        changed = False
        lines = []
        for line in text.split("\n"):
            if pattern.search(line) and not line.lstrip().startswith("#"):
                new = pattern.sub(
                    lambda match: "" if not _in_literal(line, match.start()) else match.group(0),
                    line,
                )
                changed = changed or new != line
                line = new
            lines.append(line)
        if changed:
            path.write_text("\n".join(lines))
            touched.append(str(path.relative_to(tree)))
    return touched


def _in_literal(line: str, index: int) -> bool:
    """Whether ``line[index]`` sits inside a string or character literal.

    A compiler is one of the subjects, and a compiler spells its own
    keywords in string literals: tcc's token table contains the four
    characters ``_Noreturn`` as data. Deleting that would change the program
    rather than a hint about it, so a match inside a literal is left alone.
    """
    quote = None
    position = 0
    while position < index:
        character = line[position]
        if quote:
            if character == "\\":
                position += 2
                continue
            if character == quote:
                quote = None
        elif character in "\"'":
            quote = character
        position += 1
    return quote is not None


# --------------------------------------------------------------------------
# Command capture
# --------------------------------------------------------------------------

#: A recording compiler. It writes its argv NUL-separated with a record
#: terminator, under ``>>`` so concurrent ``make -j`` writers append whole
#: records, and then becomes the real compiler, so the build it records is
#: byte-for-byte the build that would have happened without it.
RECORDER = r"""#!/bin/sh
: "${EVO_CMDLOG:=/dev/null}"
{ printf '%s\0' "$PWD" "$@"; printf '\1\n'; } >> "$EVO_CMDLOG"
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
        if not obj.endswith(".o"):
            continue
        unit = Unit(cwd=cwd, argv=argv, source=sources[0], obj=obj, root=root)
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
PASSED_THROUGH = ("-nostdinc", "-undef")

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
        elif value == "-std=gnu11":
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
        path = asm_dir / (unit.name.replace("/", "__") + ".s")
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


def assembler_flags(unit: Unit) -> list[str]:
    """The flags addressed to the assembler for this unit."""
    return [a for a in unit.argv if a.startswith("-Wa,")]


ASSEMBLE = r"""#!/bin/sh
cd /work/asm || exit 1
: > assemble.log
fail=0
while IFS='|' read -r name asmfile obj flags; do
  [ -n "$name" ] || continue
  if gcc-14 $flags -c -x assembler "$asmfile" -o "obj/$name" 2>> assemble.log; then
    echo "$name ok"
  else
    echo "$name assembler-refused"
    fail=1
  fi
done < plan.txt
exit 0
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
        flat = row["unit"].replace("/", "__")
        flags = " ".join(assembler_flags(units[row["unit"]]))
        plan.append(f"{flat}.o|{row['assembly']}|{flat}.o|{flags}")
    (asm_dir / "plan.txt").write_text("\n".join(plan) + "\n", encoding="utf-8")
    (asm_dir / "assemble.sh").write_text(ASSEMBLE, encoding="utf-8")
    done = container({"asm": asm_dir}, "sh /work/asm/assemble.sh", timeout=3600)
    verdicts = {}
    for line in done.stdout.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[0].endswith(".o"):
            verdicts[parts[0][:-2].replace("__", "/")] = parts[1]
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


def symbol_drift(rows: list[dict], tree: Path, asm_dir: Path) -> dict[str, list[str]]:
    """Global symbols an EvoCC object defines that the reference one does not.

    The front end does not yet emit the visibility marker, so a ``static``
    function -- typically a ``static inline`` helper out of a shared header --
    reaches the object file as a global symbol. One such object links; two of
    them, defining the same helper, do not. Knowing which symbols drifted is
    what lets the harness pick a set that links without asking the linker
    about every subset.
    """
    assembled = [row for row in rows if row["outcome"] == "assembled"]
    if not assembled:
        return {}
    plan = [f"{row['unit']}|{row['unit'].replace('/', '__')}.o|{row['unit']}" for row in assembled]
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
    return {
        unit: sorted(evocc.get(unit, set()) - reference.get(unit, set()))
        for unit in (row["unit"] for row in assembled)
    }


def compatible_units(drift: dict[str, list[str]]) -> tuple[list[str], dict[str, str]]:
    """Split the assembled units into a set that can link and the rest.

    Two units that drifted the same symbol cannot both be installed, so the
    units are taken in order and one is set aside as soon as a symbol it
    drifted has already been claimed. This counts the visibility gap instead
    of bisecting it: the link error it causes is understood, and delta
    debugging would only rediscover that any two such units conflict.
    """
    claimed: dict[str, str] = {}
    keep: list[str] = []
    excluded: dict[str, str] = {}
    for unit in sorted(drift):
        clash = next((s for s in drift[unit] if s in claimed), None)
        if clash is not None:
            excluded[unit] = f"{clash} also defined by {claimed[clash]}"
            continue
        keep.append(unit)
        for symbol in drift[unit]:
            claimed[symbol] = unit
    return keep, excluded


# --------------------------------------------------------------------------
# Substitution and bisection
# --------------------------------------------------------------------------


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
        for saved in self.pristine.rglob("*.o"):
            unit = saved.relative_to(self.pristine)
            (self.root / unit).write_bytes(saved.read_bytes())

    def touch(self, unit: str) -> None:
        """Make the installed object newer than everything it depends on."""
        (self.root / unit).touch()


def substitute(tree: Tree, units: list[str], asm_dir: Path) -> list[str]:
    """Install the EvoCC objects for ``units``, restoring everything else."""
    tree.restore_all()
    installed = []
    for unit in units:
        obj = asm_dir / "obj" / (unit.replace("/", "__") + ".o")
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
