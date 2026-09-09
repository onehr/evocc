#!/usr/bin/env python3
"""Rung E0 of the evaluation ladder: compiler test suites through EvoCC.

Subjects (pinned in docs/roadmap.md):
  E0a  GCC C torture, gcc/testsuite/gcc.c-torture/execute, tag releases/gcc-16.2.0
  E0b  c-testsuite, tests/single-exec, commit 5c7275656d75

For every program: EvoCC emits AArch64 assembly for aarch64-unknown-linux-gnu
with the sealed container's glibc headers; GCC-14 inside the container
assembles and links it statically; the program runs with a timeout. A
c-testsuite program passes when it exits 0 and its stdout equals the
.expected file; a torture program passes when it exits 0 (they abort on
failure). Every program that does not pass is classified by the first
diagnostic the EvoIR interpreter reports for it, so the failures rank by
construct the way the kernel surveys rank units.

Source files are copied byte for byte. In particular, invalid UTF-8 must
reach the compiler unchanged, and empty parameter lists are the compiler's
responsibility. System headers come from the container's sysroot.

Usage:
  python3 tools/ladder/e0.py [--suite ctest|torture|all] [--limit N]
Writes .evidence/ladder/e0/<run>/{results.jsonl,summary.json} and updates
tests/contracts/ladder-e0-receipt.json.
"""

from __future__ import annotations

import argparse
import collections
import datetime as dt
import fnmatch
import hashlib
import json
import re
import shlex
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / ".build" / "ladder"
EVIDENCE = ROOT / ".evidence" / "ladder" / "e0"
RECEIPT = ROOT / "tests" / "contracts" / "ladder-e0-receipt.json"
IMAGE = "evo-flight-lab:sealed"
TARGET = "aarch64-unknown-linux-gnu"
# gcc/testsuite/lib/target-supports.exp from the pinned torture source:
# these predicates describe the target, not the subject's implementation.
# In particular, missing computed-goto lowering cannot make indirect_jumps
# false and thereby remove its original tests from the campaign.
TARGET_PREDICATES = {
    'int32': True, 'int32plus': True, 'size20plus': True, 'size32plus': True,
    'long_long': True, 'longlong64': True, 'int128': True, 'lp64': True,
    'hosted': True, 'freestanding': False, 'ia32': False,
    'label_values': True, 'indirect_jumps': True, 'indirect_calls': True,
    'return_address': True, 'untyped_assembly': True,
}
PINS = {
    "ctest": {
        "repo": "https://github.com/c-testsuite/c-testsuite.git",
        "commit": "5c7275656d75",
        "subdir": "tests/single-exec",
    },
    "torture": {
        "repo": "https://github.com/gcc-mirror/gcc.git",
        "ref": "releases/gcc-16.2.0",
        "subdir": "gcc/testsuite/gcc.c-torture/execute",
    },
}


def sh(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def fetch(suite: str) -> Path:
    pin = PINS[suite]
    dest = BUILD / suite
    if not (dest / pin["subdir"]).is_dir():
        dest.parent.mkdir(parents=True, exist_ok=True)
        if suite == "ctest":
            sh(
                ["git", "clone", "-q", "--depth", "50", pin["repo"], str(dest)],
                check=True,
            )
            sh(["git", "-C", str(dest), "checkout", "-q", pin["commit"]], check=True)
        else:
            sh(
                [
                    "git",
                    "clone",
                    "-q",
                    "--filter=blob:none",
                    "--sparse",
                    "--depth",
                    "1",
                    "--branch",
                    pin["ref"],
                    pin["repo"],
                    str(dest),
                ],
                check=True,
            )
            sh(
                ["git", "-C", str(dest), "sparse-checkout", "set", pin["subdir"]],
                check=True,
            )
    if suite == "torture":
        # These execute tests include C sources from a sibling directory.
        # Read those exact pinned blobs rather than fetching the whole GCC
        # testsuite or leaving preparation failures classified as refusals.
        for relative in (
            "gcc/testsuite/gcc.dg/tree-ssa/pr105777.c",
            "gcc/testsuite/gcc.dg/tree-ssa/pr109938.c",
            "gcc/testsuite/gcc.dg/tree-ssa/pr109986.c",
            "gcc/testsuite/gcc.dg/tree-ssa/pr30314.c",
            "gcc/testsuite/gcc.dg/pr98304-1.c",
        ):
            destination = dest / relative
            if not destination.exists():
                blob = subprocess.run(
                    ["git", "-C", str(dest), "show", f"HEAD:{relative}"],
                    capture_output=True, check=True,
                ).stdout
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(blob)
    return dest / pin["subdir"]


def sysroot() -> Path:
    root = BUILD / "sysroot"
    if not (root / "usr" / "include" / "stdio.h").exists():
        root.mkdir(parents=True, exist_ok=True)
        sh(
            [
                "container",
                "run",
                "--rm",
                "--platform",
                "linux/arm64",
                "--network",
                "none",
                "-v",
                f"{root}:/work/sysroot",
                IMAGE,
                "sh",
                "-c",
                "G=$(gcc -print-file-name=include); tar -cf - /usr/include $G 2>/dev/null | tar -xf - -C /work/sysroot",
            ],
            check=True,
        )
    return root


def include_flags(root: Path) -> list[str]:
    return [
        f"-I{root}/usr/lib/gcc/aarch64-linux-gnu/14/include",
        f"-I{root}/usr/include/aarch64-linux-gnu",
        f"-I{root}/usr/include",
    ]


def prepare(
    suite: str, src_dir: Path, work: Path, limit: int | None
) -> tuple[list[str], int]:
    (work / "src").mkdir(parents=True, exist_ok=True)
    (work / "asm").mkdir(exist_ok=True)
    names = []
    for path in sorted(src_dir.glob("*.c"))[:limit]:
        shutil.copyfile(path, work / "src" / path.name)
        expected = path.with_suffix(".c.expected")
        (work / "src" / (path.stem + ".expected")).write_bytes(
            expected.read_bytes() if expected.exists() else b""
        )
        names.append(path.stem)
    return names, 0


def directive_words(text: str) -> list[str]:
    """Read a braced/quoted DejaGNU list, without executing Tcl substitutions."""
    words = []
    position = 0
    while position < len(text):
        if text[position].isspace():
            position += 1
            continue
        opening = text[position]
        if opening in '{"':
            start = position + 1
            position = start
            depth = 1
            while position < len(text):
                character = text[position]
                if character == '\\':
                    position += 2
                    continue
                if opening == '{' and character == '{':
                    depth += 1
                elif character == ('}' if opening == '{' else '"'):
                    depth -= 1
                    if depth == 0:
                        break
                position += 1
            if depth:
                raise ValueError("unterminated requirement list")
            words.append(text[start:position])
            position += 1
        else:
            start = position
            while position < len(text) and not text[position].isspace():
                position += 1
            words.append(text[start:position])
    return words


def target_matches(expression: str) -> bool:
    """The frozen AArch64 Linux profile; unknown predicates are not guessed."""
    return selector_words_match(directive_words(expression))


def selector_words_match(words: list[str]) -> bool:
    if not words:
        raise ValueError("empty target selector")
    for operator in ('||', '&&'):
        if operator in words:
            split = words.index(operator)
            left = selector_words_match(words[:split])
            right = selector_words_match(words[split + 1:])
            return left or right if operator == '||' else left and right
    if words[0] == '!':
        return not selector_words_match(words[1:])
    if len(words) > 1:
        return any(target_matches(word) for word in words)
    word = words[0]
    nested = directive_words(word)
    if nested != words:
        return selector_words_match(nested)
    if word in TARGET_PREDICATES:
        return TARGET_PREDICATES[word]
    if '-' in word or '*' in word or '?' in word:
        return fnmatch.fnmatchcase(TARGET, word)
    raise ValueError(f"unsupported target predicate: {word}")


def program_options(source: Path) -> dict:
    """Apply selected options; unknown applicability is never an exclusion."""
    supported = {
        "-fwrapv", "-fno-wrapv", "-fno-strict-overflow", "-fstrict-overflow",
        "-fbuiltin", "-fno-builtin",
        "-std=gnu89", "-std=gnu11",
    }
    base, additional, unhandled, applicability, skips = [], [], [], [], []
    tokens = re.finditer(
        r'''"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|/\*.*?\*/|//[^\n]*''',
        source.read_bytes().decode("utf-8", errors="surrogateescape"), re.S,
    )
    for token in tokens:
        comment = token.group()
        if not comment.startswith(("/*", "//")):
            continue
        for match in re.finditer(r"\{\s*dg-", comment):
            directive = comment[match.start():].removesuffix('*/').strip()
            try:
                # The first braced word is this directive; later comments or
                # directives are not part of its argument list.
                body = directive_words(directive)[0]
                words = directive_words(body)
                command, arguments = words[0], words[1:]
                selected = True
                if arguments and arguments[-1].strip().startswith('target '):
                    selector = directive_words(arguments.pop())
                    selected = target_matches(' '.join(selector[1:]))
                if command == 'dg-do':
                    if not selected:
                        applicability.append({'reason': 'target selector does not match', 'directive': body})
                    elif arguments != ['run']:
                        applicability.append({'reason': 'test is not an execution test', 'directive': body})
                elif not selected:
                    continue
                elif command in ('dg-options', 'dg-additional-options') and len(arguments) == 1:
                    flags = shlex.split(arguments[0])
                    if command == 'dg-options':
                        base = flags
                    else:
                        additional.extend(flags)
                elif command == 'dg-require-effective-target' and len(arguments) == 1:
                    if not target_matches(arguments[0]):
                        applicability.append({'reason': 'required target feature absent', 'directive': body})
                elif command == 'dg-skip-if' and 2 <= len(arguments) <= 4:
                    skips.append((body, arguments))
                else:
                    raise ValueError('unsupported requirement')
            except (ValueError, IndexError) as error:
                unhandled.append(directive)
                applicability.append({'reason': str(error), 'directive': directive, 'unresolved': True})
    flags = base + additional
    unsupported = [flag for flag in flags if flag not in supported]
    unhandled.extend(unsupported)
    applicability.extend({'reason': 'unsupported option', 'option': flag} for flag in unsupported)
    for body, arguments in skips:
        try:
            includes = directive_words(arguments[2]) if len(arguments) > 2 else []
            excludes = directive_words(arguments[3]) if len(arguments) > 3 else []
            def matches(pattern):
                return any(fnmatch.fnmatchcase(flag, pattern) for flag in flags)
            if (target_matches(arguments[1])
                    and (not includes or includes == [''] or any(matches(p) for p in includes))
                    and not any(matches(p) for p in excludes if p)):
                applicability.append({'reason': arguments[0], 'directive': body})
        except ValueError as error:
            unhandled.append(body)
            applicability.append({'reason': str(error), 'directive': body, 'unresolved': True})
    result = {"applied": [flag for flag in flags if flag in supported], "unhandled": unhandled}
    if applicability:
        result['applicability'] = applicability
    return result


def emit(evocc: Path, work: Path, names: list[str], inc: list[str]) -> dict[str, dict]:
    rows = {}
    for name in names:
        src = work / "src" / f"{name}.c"
        options = program_options(src)
        if options.get('applicability'):
            unresolved = any(reason.get('unresolved') for reason in options['applicability'])
            rows[name] = {
                'emit': 'requirement-unresolved' if unresolved else 'not-applicable', 'options': options,
                'applicability': options['applicability'], 'sourceSha256': sha256(src),
            }
            continue
        try:
            p = sh(
                [str(evocc), f"--target={TARGET}", *inc, *options["applied"], "--emit=asm", str(src)],
                timeout=60,
            )
        except subprocess.TimeoutExpired:
            rows[name] = {"emit": "timeout", "options": options}
            continue
        if p.returncode == 0 and p.stdout.strip():
            (work / "asm" / f"{name}.s").write_text(p.stdout)
            rows[name] = {"emit": "emitted", "options": options}
        else:
            m = re.search(r"error\[(E\d+)\]: ([^\n]*)", p.stderr)
            rows[name] = {
                "emit": "refused",
                "options": options,
                "code": m.group(1)
                if m
                else ("panic" if "panicked" in p.stderr else "other"),
                "message": (m.group(2) if m else p.stderr.strip()[-160:])[:160],
            }
            if m:
                rows[name]["firstDiagnostic"] = p.stderr[
                    p.stderr.rfind("\n", 0, m.start()) + 1:m.end()
                ]
    return rows


RUNNER = r"""#!/bin/sh
cd /work/run; mkdir -p bin out
for s in asm/*.s; do b=$(basename $s .s)
  [ -f "$s" ] || continue
  if ! gcc -c $s -o bin/$b.o 2>out/$b.asm.err; then echo "$b asm-fail"; continue; fi
  if ! gcc -static -o bin/$b bin/$b.o 2>out/$b.link.err; then echo "$b link-fail $(grep -m1 -o 'undefined reference to [^ ]*' out/$b.link.err | cut -d' ' -f4)"; continue; fi
  timeout 10 ./bin/$b > out/$b.stdout 2>out/$b.stderr; rc=$?
  if [ $rc -eq 0 ] && { [ ! -s src/$b.expected ] || cmp -s out/$b.stdout src/$b.expected; }; then echo "$b pass"; else echo "$b fail rc=$rc"; fi
done
"""


def run_in_container(work: Path) -> dict[str, dict]:
    (work / "run.sh").write_text(RUNNER)
    p = sh(
        [
            "container",
            "run",
            "--rm",
            "--platform",
            "linux/arm64",
            "--network",
            "none",
            "-v",
            f"{work}:/work/run",
            IMAGE,
            "sh",
            "/work/run/run.sh",
        ],
        timeout=3600,
    )
    out = {}
    for line in p.stdout.splitlines():
        parts = line.split()
        if not parts or parts[0].startswith("["):
            continue
        name, verdict = parts[0], parts[1]
        row = {"run": verdict}
        if verdict == "fail":
            row["exit"] = int(parts[2].split("=")[1])
        if verdict == "link-fail" and len(parts) > 2:
            row["undefined"] = (
                " ".join(parts[5:]) if len(parts) > 5 else " ".join(parts[2:])
            )
        out[name] = row
    return out


def interpreter_verdict(evocc: Path, work: Path, name: str, inc: list[str]) -> str:
    source = work / "src" / f"{name}.c"
    try:
        p = sh(
            [
                str(evocc),
                f"--target={TARGET}",
                *inc,
                *program_options(source)["applied"],
                "--interpret-evoir",
                str(source),
            ],
            timeout=30,
        )
    except subprocess.TimeoutExpired:
        return "interpreter-timeout"
    m = re.search(r"error\[(E\d+)\]: ([a-z-]+)", p.stderr + p.stdout)
    return (
        f"{m.group(1)} {m.group(2)}" if m else ("no-diagnostic exit=%d" % p.returncode)
    )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--suite", default="all", choices=["all", "ctest", "torture"])
    ap.add_argument("--limit", type=int)
    ap.add_argument("--evocc", default=str(ROOT / "target" / "release" / "evocc"))
    args = ap.parse_args()
    evocc = Path(args.evocc)
    if not evocc.exists():
        print(
            "build the release driver first: cargo build --release -p evocc",
            file=sys.stderr,
        )
        return 1
    run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out_dir = EVIDENCE / run_id
    out_dir.mkdir(parents=True, exist_ok=True)
    system_includes = include_flags(sysroot())
    head = sh(["git", "-C", str(ROOT), "rev-parse", "--short", "HEAD"]).stdout.strip()
    summary = {
        "run": run_id,
        "subject": head,
        "driverSha256": sha256(evocc),
        "sourcePreparation": "byte-preserving; no empty-parameter rewriting or encoding replacement",
        "optionPolicy": "target-selected supported options applied to compilation and diagnosis; unsupported options or mismatching selectors are not-applicable with source-bound evidence; unknown applicability is requirement-unresolved, never an exclusion or PASS; all original identities remain",
        "target": TARGET,
        "suites": {},
    }
    applicability_evidence = {}
    with open(out_dir / "results.jsonl", "w") as results:
        for suite in ["ctest", "torture"] if args.suite == "all" else [args.suite]:
            work = BUILD / f"run-{suite}"
            if work.exists():
                shutil.rmtree(work)
            source_dir = fetch(suite)
            names, adjusted = prepare(suite, source_dir, work, args.limit)
            # The copied translation unit retains its bytes; quoted headers
            # and cross-directory inclusions resolve in its original tree.
            inc = [*system_includes, f"-I{source_dir}"]
            rows = emit(evocc, work, names, inc)
            runs = run_in_container(work)
            counts = collections.Counter()
            refusals = collections.Counter()
            verdicts = collections.Counter()
            applied_options = collections.Counter()
            unhandled_programs = 0
            for name in names:
                row = {
                    "suite": suite,
                    "program": name,
                    **rows[name],
                    **runs.get(name, {}),
                }
                outcome = (
                    row["emit"]
                    if row["emit"] != "emitted"
                    else row.get("run", "not-run")
                )
                row["outcome"] = outcome
                row['disposition'] = (
                    'PASS' if outcome == 'pass' else
                    'not-applicable' if outcome == 'not-applicable' else 'compiler-failure'
                )
                if outcome in ('not-applicable', 'requirement-unresolved'):
                    key = f'{suite}:{name}'
                    applicability_evidence[key] = {
                        'target': TARGET,
                        'source': str((source_dir / f'{name}.c').relative_to(ROOT)),
                        'sourceSha256': row['sourceSha256'],
                        'appliedOptions': row['options']['applied'],
                        'reasons': row['applicability'],
                    }
                    row['applicabilityEvidence'] = str((out_dir / 'applicability.json').relative_to(ROOT)) + '#' + key
                counts[outcome] += 1
                applied_options.update(row["options"]["applied"])
                unhandled_programs += bool(row["options"]["unhandled"])
                if outcome == "refused":
                    refusals[row["code"]] += 1
                elif outcome in ("fail", "asm-fail", "link-fail"):
                    row["interpreter"] = interpreter_verdict(evocc, work, name, inc)
                    verdicts[row["interpreter"]] += 1
                results.write(json.dumps(row) + "\n")
            summary["suites"][suite] = {
                "pin": PINS[suite],
                "sourceDirectory": str(source_dir.relative_to(ROOT)),
                "sourceCommit": sh(["git", "-C", str(source_dir), "rev-parse", "HEAD"], check=True).stdout.strip(),
                "programs": len(names),
                "adjustedEmptyParameterLists": adjusted,
                "outcomes": dict(counts),
                "refusalsByCode": dict(refusals.most_common()),
                "failuresByInterpreterVerdict": dict(verdicts.most_common()),
                "appliedOptions": dict(applied_options),
                "programsWithUnhandledRequirements": unhandled_programs,
            }
            print(f"{suite}: {dict(counts)}")
    (out_dir / 'applicability.json').write_text(json.dumps(applicability_evidence, indent=2) + '\n')
    with open(out_dir / "summary.json", "w") as stream:
        json.dump(summary, stream, indent=1)
    receipt = {
        "schemaVersion": 1,
        "rung": "E0",
        "authority": "docs/roadmap.md, milestone ladder",
        "latest": summary,
        "evidence": str(out_dir.relative_to(ROOT)),
    }
    with open(RECEIPT, "w") as stream:
        json.dump(receipt, stream, indent=1)
    print("receipt:", RECEIPT.relative_to(ROOT), "evidence:", out_dir.relative_to(ROOT))
    return 0


if __name__ == "__main__":
    sys.exit(main())
