"""Decode one Kbuild ``.o.cmd`` record into the commands the mixed link needs.

Every object in the reference tree carries the exact argv that produced it.
This module turns that argv into two derived commands:

* the host EvoCC invocation that emits assembly for the unit, reusing the
  preprocessor-flag translation the frozen reconnaissance runner already
  fixed (container paths rewritten to host paths, everything the front end
  does not consume dropped and reported);
* the in-container ``gcc -c`` invocation that assembles that assembly with
  the assembler flags Kbuild passed for this very unit.

Nothing here invents a flag. A flag the front end cannot consume is omitted
and named, never guessed at.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import shlex

CONTAINER_SOURCE_PREFIX = "/work/src/"

#: Options whose value is a separate argument.
SEPARATE_VALUE = ("-I", "-D", "-U", "-include", "-imacros", "-isystem", "-iquote")

#: Of those, the ones whose value is a path needing translation.
PATH_VALUED = ("-I", "-include", "-imacros", "-isystem", "-iquote")

#: Options the front end consumes as written.
PASSED_THROUGH = (
    "-O", "-O0", "-O1", "-O2", "-O3", "-Os", "-Og",
    "-g", "-g0", "-gdwarf-4",
    "-mcmodel=tiny", "-mcmodel=small",
    "-ffunction-sections", "-fno-function-sections",
    "-fdata-sections", "-fno-data-sections",
    "-fasynchronous-unwind-tables", "-funwind-tables",
    "-fno-asynchronous-unwind-tables", "-fno-unwind-tables",
    "-nostdinc", "-undef", "-fwrapv", "-fno-wrapv", "-fno-strict-overflow", "-fstrict-overflow",
    "-fbuiltin", "-fno-builtin",
    "-fno-strict-aliasing",
    "-ffreestanding", "-fhosted",
    "-ftrivial-auto-var-init=zero", "-ftrivial-auto-var-init=uninitialized",
    "-fstack-protector-strong", "-fno-stack-protector",
    "-mbranch-protection=pac-ret", "-mbranch-protection=none",
    "-mgeneral-regs-only", "-mno-general-regs-only",
    "-mstrict-align", "-mno-strict-align",
    "-mstack-protector-guard=sysreg", "-mstack-protector-guard=global",
    "-fshort-wchar", "-fno-short-wchar",
    "-fgnu89-inline", "-fno-gnu89-inline",
    "-fsigned-char", "-funsigned-char",
    "-fpic", "-fPIC", "-fpie", "-fPIE", "-fno-pic", "-fno-PIC", "-fno-pie", "-fno-PIE",
)

SOURCE_DECLARATION = re.compile(r"^source_[^ ]+ := (.+)$")
SAVED_COMMAND = re.compile(r"^savedcmd_([^ ]+) := (.+)$")


def omitted_option_class(option: str) -> str:
    """Explicit boot-relevance audit; unknown options are never assumed inert."""
    if option.startswith("-Wa,"):
        return "forwarded-to-assembler"
    if option.startswith("-Wp,-MMD,") or option in ("-c", "-o") or not option.startswith("-"):
        return "replaced-output-or-dependency-action"
    if option.startswith("-W"):
        return "diagnostic-policy-not-code-generation"
    if option in ("-g", "-g0", "-gdwarf-4"):
        return "source-type-debug-option-must-be-forwarded"
    if option in ("-fgnu89-inline", "-fno-gnu89-inline", "-fsigned-char", "-funsigned-char",
                  "-fno-strict-aliasing"):
        return "source-semantics-option-must-be-forwarded"
    if option.startswith(("-fmin-function-alignment=", "-fpatchable-function-entry=")):
        return "function-entry-option-must-be-forwarded"
    if option in ("-ffunction-sections", "-fno-function-sections",
                  "-fdata-sections", "-fno-data-sections"):
        return "section-placement-option-must-be-forwarded"
    if option.startswith("-femit-struct-debug-") or option == "-fno-var-tracking":
        return "debug-scope-hint-source-types-emitted-without-local-locations"
    if option in ("-fconserve-stack", "-fno-code-hoisting"):
        return "optimization-hint-no-transform-equivalence"
    if option in (
        "-mlittle-endian", "-mabi=lp64", "-ffixed-x18",
        "-fno-common",
        "-fno-omit-frame-pointer",
        "-fno-optimize-sibling-calls",
        "-fno-delete-null-pointer-checks", "-fno-allow-store-data-races",
        "-fno-stack-check", "-fno-stack-clash-protection",
        "-fstrict-flex-arrays=3", "-fno-builtin-wcslen",
    ):
        return "existing-target-or-conservative-behavior"
    return "UNREVIEWED"


class RecordError(RuntimeError):
    """The ``.o.cmd`` record does not have the shape this lane requires."""


@dataclass(frozen=True)
class Record:
    """One decoded Kbuild command record."""

    path: Path
    unit: str
    """The object path as Kbuild names it, for example ``lib/bitrev.o``."""
    argv: tuple[str, ...]
    source_token: str
    """The source operand exactly as it appears in the recorded argv."""

    @property
    def tool(self) -> str:
        return Path(self.argv[0]).name


def record_path(out_root: Path, unit: str) -> Path:
    """The ``.o.cmd`` file that records how ``unit`` was built."""
    unit_path = Path(unit)
    return out_root / unit_path.parent / f".{unit_path.name}.cmd"


def parse_record(path: Path) -> Record:
    """Decode the first saved command of one ``.o.cmd`` record."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise RecordError(f"cannot read {path}: {error}") from error
    lines = text.splitlines()
    if not lines:
        raise RecordError(f"empty command record: {path}")
    saved = SAVED_COMMAND.match(lines[0])
    if saved is None:
        raise RecordError(f"missing saved-command line in {path}")
    unit, command = saved.group(1), saved.group(2)
    try:
        argv = shlex.split(command, posix=True)
    except ValueError as error:
        raise RecordError(f"cannot decode POSIX argv in {path}: {error}") from error
    if not argv:
        raise RecordError(f"empty saved command in {path}")

    declared = None
    for line in lines:
        match = SOURCE_DECLARATION.match(line)
        if match:
            declared = match.group(1)
            break
    if declared is not None:
        if argv.count(declared) != 1:
            raise RecordError(f"declared source is absent or ambiguous in {path}")
        source_token = declared
    else:
        candidates = [
            argument
            for argument in argv[1:]
            if argument.endswith(".c") and not argument.startswith("-")
        ]
        if len(candidates) != 1:
            raise RecordError(f"cannot identify a single C source operand in {path}")
        source_token = candidates[0]
    return Record(path=path, unit=unit, argv=tuple(argv), source_token=source_token)


def is_target_c_compile(record: Record) -> bool:
    """Whether this record is a target C compilation, not a link or a host tool."""
    # CC and HOSTCC can both be gcc-14 in a native arm64 reference build.
    # The frozen Kbuild Makefile adds scripts/include to KBUILD_HOSTCFLAGS,
    # whereas target KBUILD_CPPFLAGS defines __KERNEL__. Use that positive
    # host-recipe evidence, not a source-directory or optimization-flag guess.
    # Keep ambiguous records in the ordinary option audit rather than silently
    # excluding them. Target generators also live under scripts/.
    args = record.argv[1:]
    kernel = any(arg == '-D__KERNEL__' or arg.startswith('-D__KERNEL__=')
                 for arg in args) or any(
        left == '-D' and (right == '__KERNEL__' or right.startswith('__KERNEL__='))
        for left, right in zip(args, args[1:]))
    host_include = CONTAINER_SOURCE_PREFIX + 'scripts/include'
    host = ('-I' + host_include in args or any(
        left == '-I' and right == host_include
        for left, right in zip(args, args[1:]))) and not kernel
    return (
        record.tool == "gcc-14"
        and "-c" in record.argv
        and record.source_token.endswith(".c")
        and not host
    )


def translate_path(value: str, src_root: Path, out_root: Path,
                   toolchain_root: Path | None = None) -> str:
    """Rewrite one container path to its host equivalent.

    ``/work/src/...`` is the mounted kernel source; a relative path is
    relative to the Kbuild output directory. A relative path that escapes the
    output directory is a decoding failure, not something to normalize away.
    """
    if value.startswith(CONTAINER_SOURCE_PREFIX):
        return str(src_root / value[len(CONTAINER_SOURCE_PREFIX) :])
    if value.startswith('/usr/') and toolchain_root is not None:
        root = toolchain_root.resolve()
        translated = (root / value.lstrip('/')).resolve()
        if root not in translated.parents:
            raise RecordError(f'toolchain path escapes the exported tree: {value}')
        return str(translated)
    if value.startswith("/"):
        return value
    relative = value[2:] if value.startswith("./") else value
    translated = (out_root / relative).resolve()
    root = out_root.resolve()
    if translated != root and root not in translated.parents:
        raise RecordError(f"relative Kbuild path escapes the output tree: {value}")
    return str(translated)


def source_path(record: Record, src_root: Path, out_root: Path) -> Path:
    """The host path of the unit's source file."""
    return Path(translate_path(record.source_token, src_root, out_root))


def evocc_argv(
    record: Record,
    evocc: Path,
    src_root: Path,
    out_root: Path,
    mode: str = "--emit=asm",
    target: str = "aarch64-unknown-linux-gnu",
    toolchain_root: Path | None = None,
) -> tuple[list[str], list[str]]:
    """The EvoCC command for this unit, and the flags it does not consume."""
    projected = [str(evocc), mode, "--survey", f"--target={target}"]
    omitted: list[str] = []
    argv = list(record.argv)
    index = 1
    while index < len(argv):
        value = argv[index]
        if value == record.source_token:
            index += 1
            continue
        if value in SEPARATE_VALUE:
            if index + 1 >= len(argv):
                raise RecordError(f"missing value after {value} in {record.path}")
            following = argv[index + 1]
            if value in PATH_VALUED:
                following = translate_path(following, src_root, out_root, toolchain_root)
            projected.extend((value, following))
            index += 2
            continue
        if value.startswith("-I") and len(value) > 2:
            projected.append("-I" + translate_path(value[2:], src_root, out_root, toolchain_root))
        elif value.startswith(("-D", "-U")) and len(value) > 2:
            projected.append(value)
        elif value in PASSED_THROUGH or value.startswith(("-std=", "-mstack-protector-guard-reg=", "-mstack-protector-guard-offset=", "-fpatchable-function-entry=", "-fmin-function-alignment=")):
            projected.append(value)
        elif value.startswith("-fmacro-prefix-map="):
            projected.append(
                value.replace(CONTAINER_SOURCE_PREFIX, f"{src_root}/")
            )
        else:
            omitted.append(value)
        index += 1
    projected.append(str(source_path(record, src_root, out_root)))
    return projected, omitted


def assembler_flags(record: Record) -> list[str]:
    """The flags Kbuild passed for the assembler stage of this unit.

    An assembly input reaches only the assembler, so the compiler-proper and
    preprocessor flags of the record have nothing to act on. What survives is
    every ``-Wa,`` option, because those are addressed to the assembler by
    name and select, among other things, the architecture revision.
    """
    return [argument for argument in record.argv[1:] if argument.startswith("-Wa,")]


def assemble_argv(
    record: Record,
    compiler: str,
    assembly: str,
    obj: str,
) -> list[str]:
    """The in-container command that assembles ``assembly`` into ``obj``."""
    return [
        compiler,
        *assembler_flags(record),
        "-Wa,-I,/work/build",
        "-c",
        "-x",
        "assembler",
        assembly,
        "-o",
        obj,
    ]
