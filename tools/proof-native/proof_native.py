#!/usr/bin/env python3
"""Run the proof-required output of the core-v1 pilot on Linux AArch64 (V08).

Purpose: compile the 17 pilot modules and the two optimized cases in
proof-required mode, assemble and run the checked assembly text in the sealed
Linux AArch64 image, and compare each result with the source interpreter
(`evoir-sem --run-core`). It also runs the upper-bits probe, the gcc-14 -O0
cross-check, a second assembler and linker (clang-19, ld.lld-19) and a static
scan for reads of the upper 32 bits of 32-bit arguments. It does not change
the repository.

Run:  python3 tools/proof-native/proof_native.py all WORK_DIR

Steps (each takes WORK_DIR [CASE ...]): freeze, produce, prepare, run,
compare, scan, report. WORK_DIR must be outside the repository. The binaries
come from --evocc, --evocc-checker and --evoir-sem, or from the environment
variables EVOCC, EVOCC_CHECKER and EVOIR_SEM. The defaults are
target/frontend/debug/evocc, target/frontend/debug/evocc-checker and
lean/.lake/build/bin/evoir-sem. `freeze` copies them into WORK_DIR/bin; the
other steps use only these copies.
"""
import argparse
import concurrent.futures
import hashlib
import itertools
import json
import os
import random
import re
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
FIX = REPO / "tests/fixtures/verification"
DEFAULT_BINARIES = {
    "evocc": ("EVOCC", REPO / "target/frontend/debug/evocc"),
    "evocc-checker": ("EVOCC_CHECKER", REPO / "target/frontend/debug/evocc-checker"),
    "evoir-sem": ("EVOIR_SEM", REPO / "lean/.lake/build/bin/evoir-sem"),
}
STEP_LIMIT = 10_000_000
TIMEOUT_RESULT = 20  # seconds, a vector with a source result
TIMEOUT_STEP = 5  # seconds, a vector without a source result (step limit)
STOP_GIB = 15
PATTERNS = {"P1": 0xDEADBEEF, "P2": 0xFFFFFFFF, "P3": 0x00000001, "P4": 0x80000000}

# ---------------------------------------------------------------- population

LOOP_CALL = "loop-call.cutpoints.json"
CLOSURE = "call-loop-closure.cutpoints.json"
NESTED = "call-loop-nested.cutpoints.json"
DEEP = "call-loop-deep-context.cutpoints.json"
# Proposals of call-loop.c and of the modules that include it. An entry that
# calls a cyclic function needs the call cutpoints of its closure.
CALL_LOOP = {"loop_call": LOOP_CALL, "call_loop": CLOSURE}
for _entry in ["right_call", "conditional_call", "assigned_call", "nested_call",
               "initialized_loop_call", "and_loop_call", "or_loop_call", "branch_loop_call",
               "and_right_loop_call", "or_right_loop_call", "finite_before_loop",
               "finite_loop_argument", "loop_then_finite", "call_constant_tail"]:
    CALL_LOOP[_entry] = CLOSURE
CALL_LOOP.update({"loop_calls_loop": NESTED, "call_nested_loop": NESTED,
                  "deep_saved": NESTED, "deep_short": NESTED, "deep_outer_loop": DEEP})
# Proposal values: a fixture file name, or ("element", N) for the element of
# symbol N of source-execution.cutpoints.json, extracted at run time.
MODULES = {
    "doom-fixedmul": {},
    "musl-abs": {},
    "control": {"loop_join": "loop-join.cutpoints.json"},
    "pressure": {},
    "source-execution": {"loop_join": ("element", 7), "control": ("element", 56),
                         "main": "source-execution.cutpoints.json"},
    "call-loop": CALL_LOOP,
    "call-loop-control": {"loop_branch": LOOP_CALL, "loop_checked": "loop-checked.cutpoints.json",
                          "loop_forever": "loop-forever.cutpoints.json"},
    "call-loop-context": CALL_LOOP,
    "call-loop-closure-context": CALL_LOOP,
    "call-loop-short-right": CALL_LOOP,
    "call-loop-mixed": CALL_LOOP,
    "call-loop-nested": CALL_LOOP,
    "call-loop-deep": CALL_LOOP,
    "call-loop-deep-context": CALL_LOOP,
    "call-constant-tail": CALL_LOOP,
    "initializer-self-assignment": {},
    "source-conversions": {},
}
# case: (fixture, extra options other than the pinned checker)
CASES = {name: (name, []) for name in MODULES}
CASES["opt-fixedmul-g0"] = ("doom-fixedmul", ["-Oeq-scalar", "--eq-policy=guard-known-true"])
CASES["opt-control-p0"] = ("control", ["-Oeq-scalar"])
ORDER = list(CASES)


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def free_gib(path):
    stat = os.statvfs(path)
    return stat.f_bavail * stat.f_frsize // (1 << 30)


class Work:
    def __init__(self, root):
        self.root = Path(root).resolve()
        if self.root == REPO or REPO in self.root.parents:
            sys.exit(f"refused: the work directory {self.root} is inside the repository {REPO}")
        self.bin = self.root / "bin"
        self.out = self.root / "out"
        self.run = self.root / "run"
        self.logs = self.root / "logs"
        for d in (self.root, self.logs):
            d.mkdir(parents=True, exist_ok=True)

    def binary(self, name):
        path = self.bin / name
        if not path.is_file():
            sys.exit(f"missing {path}: run the freeze step first")
        return path


# ---------------------------------------------------------------- freeze, produce

def freeze(work, args):
    work.bin.mkdir(exist_ok=True)
    lines = []
    for name, (variable, default) in DEFAULT_BINARIES.items():
        source = Path(getattr(args, name.replace("-", "_")) or os.environ.get(variable) or default)
        if not source.is_file():
            sys.exit(f"missing binary {source}")
        target = work.bin / name
        if target.exists() and sha256(target) != sha256(source):
            sys.exit(f"refused: {target} differs from {source}; use a new work directory")
        if not target.exists():
            shutil.copy2(source, target)
        lines.append(f"{sha256(target)}  {name}  (copied from {source})")
    (work.root / "digests.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines), flush=True)


def proposal_path(work, value):
    if isinstance(value, str):
        return FIX / value
    _, symbol = value
    data = json.loads((FIX / "source-execution.cutpoints.json").read_text())
    element = [item[1] for item in data[2] if item[0] == symbol]
    if len(element) != 1:
        sys.exit(f"source-execution.cutpoints.json has no single element for symbol {symbol}")
    target = work.root / "proposals" / f"source-execution-{symbol}.json"
    target.parent.mkdir(exist_ok=True)
    target.write_text(json.dumps(element[0]))
    return target


def produce(work, case, log):
    fixture, extra = CASES[case]
    evocc, checker = work.binary("evocc"), work.binary("evocc-checker")
    if "-Oeq-scalar" in extra:
        extra = extra + [f"--eq-checker={checker}", f"--eq-checker-sha256={sha256(checker)}"]
    source = FIX / f"{fixture}.c"
    plain = subprocess.run([str(evocc), "--emit=asm", "-O0", "-fno-builtin", *extra, str(source)],
                           capture_output=True, text=True)
    if plain.returncode != 0:
        log.write(f"{case}: ordinary compilation failed: {plain.stderr}\n")
        print(f"{case}: ordinary compilation failed", flush=True)
        return False
    entries = re.findall(r"^\.type ([A-Za-z_]\w*), %function$", plain.stdout, re.M)
    proposals = {e: str(proposal_path(work, v)) for e, v in MODULES[fixture].items() if e in entries}
    directory = work.out / case
    if directory.exists():
        shutil.rmtree(directory)
    directory.parent.mkdir(exist_ok=True)
    command = [str(evocc), "--emit=asm", f"--proof-required={directory}",
               f"--proof-checker={work.binary('evoir-sem')}"]
    command += [f"--core-entry={e}" for e in entries]
    command += [f"--proof-proposal={e}={p}" for e, p in proposals.items()]
    command += ["-O0", "-fno-builtin", *extra, str(source)]
    log.write("COMMAND " + " ".join(command) + "\n")
    result = subprocess.run(command, capture_output=True, text=True)
    log.write(f"EXIT {result.returncode}\nSTDERR {result.stderr.strip()}\n")
    ok = result.returncode == 0
    if ok:
        record = (directory / "record.json").read_text()
        accepted = len(set(re.findall(r"PIPELINE_ACCEPTED entry=(\w+)", record)))
        same = (directory / "output.s").read_text() == plain.stdout == result.stdout
        ok = '"result": "CHECKED_TO_FINAL_ASSEMBLY"' in record and accepted == len(entries) and same
        (directory / "entries.json").write_text(json.dumps(
            {"case": case, "fixture": fixture, "entries": entries, "proposals": proposals,
             "options": extra, "pipeline_accepted": accepted, "output_equals_ordinary": same}, indent=1))
    print(f"{case}: exit={result.returncode} entries={len(entries)} "
          f"{'CHECKED_TO_FINAL_ASSEMBLY' if ok else 'NOT ACCEPTED'}", flush=True)
    return ok


# ---------------------------------------------------------------- vectors and oracle

TYPES = {  # export name: (bits, C type)
    "int": (32, "int"), "uint": (32, "unsigned int"),
    "long": (64, "long"), "ulong": (64, "unsigned long"),
    "long_long": (64, "long long"), "ulong_long": (64, "unsigned long long"),
}
I32MAX, I32MIN = 2**31 - 1, -2**31
I64MAX, I64MIN = 2**63 - 1, -2**63
FULL = {
    "int": [0, 1, -1, 2, -2, 3, -3, 4, 5, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 100, -100,
            0x7FFF, 0x8000, -0x8000, 0xFFFF, 0x10000, -0x10000, 0x18000, -0x18000, 0x20000,
            0x12345678, -0x12345678, 0x40000000, -0x40000000, I32MAX, I32MIN, I32MAX - 1, I32MIN + 1],
    "uint": [0, 1, 2, 3, 4, 5, 6, 7, 8, 15, 16, 17, 18, 20, 21, 31, 32, 63, 64, 100, 1000, 0xFFFF,
             0x10000, 0x7FFFFFFF, 0x80000000, 0x80000001, 0xFFFFFFFE, 0xFFFFFFFF, 0x12345678,
             0xDEADBEEF, 0xAAAAAAAA, 0x55555555],
    "long": [0, 1, -1, 2, -2, 17, 100, -100, 0x7FFFFFFF, 0x80000000, -0x80000000, -0x80000001,
             0xFFFFFFFF, 0x100000000, -0x100000000, 0x123456789ABCDEF0, -0x123456789ABCDEF0,
             0x4000000000000000, I64MAX, I64MIN, I64MAX - 1, I64MIN + 1],
    "ulong": [0, 1, 2, 17, 100, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF, 0x100000000, 0x123456789ABCDEF0,
              0x7FFFFFFFFFFFFFFF, 0x8000000000000000, 0x8000000000000001, 0xFFFFFFFFFFFFFFFE,
              0xFFFFFFFFFFFFFFFF, 0xFEDCBA9876543210],
}
FULL["long_long"], FULL["ulong_long"] = FULL["long"], FULL["ulong"]
CORE = {
    "int": [0, 1, -1, 7, I32MAX, I32MIN],
    "uint": [0, 1, 2, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF],
    "long": [0, 1, -1, 0xFFFFFFFF, I64MAX, I64MIN],
    "ulong": [0, 1, 0xFFFFFFFF, 0x100000000, 0x8000000000000000, 0xFFFFFFFFFFFFFFFF],
}
CORE["long_long"], CORE["ulong_long"] = CORE["long"], CORE["ulong"]
BASE = 5
# A loop count (n, m, k): each count from 0 to a few iterations, larger counts
# and the maximum (the maximum has no source result in the step limit).
LOOP_FULL = [0, 1, 2, 3, 4, 5, 6, 7, 8, 16, 17, 18, 19, 20, 21, 22, 27, 28, 31, 32, 64, 100, 1000, 0xFFFFFFFF]
LOOP_CORE = [0, 1, 2, 3, 4, 5]
LOOP_BASE = 3
# loop_join n: the for loop stops at i == 17; the while loop runs (n - 20) / 7 times.
SOFT_UINT_FULL = LOOP_FULL[:-1] + [0xFFFF, 0x10000, 0x12345, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFE, 0xFFFFFFFF]
# control(int n) of source-execution.c: the for loop stops at i == 5; the do loop runs n - 10 times.
SOFT_INT_FULL = [0, 1, 2, 3, 4, 5, 6, 7, 10, 11, 12, 20, 100, 0x10000, -1, -2, -5, -100,
                 I32MIN, I32MIN + 1, I32MAX, I32MAX - 1]
SOFT_INT_CORE = [0, 1, 3, 6, 11, -1, I32MIN]
DIVERGE_FULL = [0, 1, 2, 3, 5, 0xFFFFFFFF]
NONLOOP = {"call_tick", "loop_branch_tick", "loop_checked_tick"}


def bits(value, width):
    return value & ((1 << width) - 1)


def load_entries(work, case):
    meta = json.loads((work.out / case / "entries.json").read_text())
    asm = (work.out / case / "output.s").read_text()
    globl = set(re.findall(r"^\.globl ([A-Za-z_]\w*)$", asm, re.M))
    entries = []
    for name in meta["entries"]:
        source = work.out / case / "inputs" / f"source-{name}.json"
        export = json.loads(source.read_text())
        function = [f for f in export[4] if f[0] == "function" and f[2] == name]
        assert len(function) == 1, (case, name)
        entries.append({"name": name, "source": str(source), "ret": function[0][3][0],
                        "params": [p[2][0] for p in function[0][5]], "global": name in globl})
    return meta, entries


def roles(fixture, entry):
    """The value set of each parameter: plain, loop, soft_uint, soft_int or diverge."""
    name, params = entry["name"], entry["params"]
    result = ["plain"] * len(params)
    if fixture.startswith(("call-loop", "call-constant-tail")) and name not in NONLOOP:
        if name == "loop_forever":
            return ["diverge"]
        for i in range(1, len(params)):
            result[i] = "loop"
    if name == "loop_join":
        result[1] = "soft_uint"
    if name == "control" and fixture == "source-execution":
        result[0] = "soft_int"
    return result


def value_sets(kind, role):
    return {"loop": (LOOP_FULL, LOOP_CORE, LOOP_BASE),
            "soft_uint": (SOFT_UINT_FULL, LOOP_CORE + [17, 21, 28], LOOP_BASE),
            "soft_int": (SOFT_INT_FULL, SOFT_INT_CORE, 6),
            "diverge": (DIVERGE_FULL, [], 1)}.get(role, (FULL[kind], CORE[kind], BASE))


def vectors(case, fixture, entry):
    """Cross product of the core sets, a sweep of each parameter, random vectors."""
    params = entry["params"]
    if not params:
        return [[]]
    rs = roles(fixture, entry)
    sets = [value_sets(k, r) for k, r in zip(params, rs)]
    rng = random.Random(f"v08:{case}:{entry['name']}")
    found, seen = [], set()

    def add(vector):
        vector = tuple(bits(v, TYPES[k][0]) for v, k in zip(vector, params))
        if vector not in seen:
            seen.add(vector)
            found.append(list(vector))

    if "diverge" in rs:
        for v in sets[0][0]:
            add([v])
        return found
    if len(params) <= 4:
        for vector in itertools.product(*[core for _, core, _ in sets]):
            add(vector)
    base = [b for _, _, b in sets]
    add(base)
    for i, (full, _, _) in enumerate(sets):
        for v in full:
            add(base[:i] + [v] + base[i + 1:])
    if len(params) > 4:  # wide: all parameters equal
        for v in FULL[params[0]]:
            add([v] * len(params))
    for _ in range(64 if len(params) > 4 else 32):
        vector = []
        for k, r, (full, _, _) in zip(params, rs, sets):
            if r == "loop":
                vector.append(rng.randrange(0, 41))
            elif r == "soft_uint":
                vector.append(rng.choice([rng.randrange(0, 0x10001), rng.choice(full[:-7])]))
            elif r == "soft_int":
                vector.append(rng.choice(full[:14]) if rng.random() < 0.5 else rng.randrange(-1000, 0x10000))
            elif rng.random() < 0.5:
                vector.append(rng.choice(full))
            else:
                vector.append(rng.getrandbits(TYPES[k][0]))
        add(vector)
    return found


def oracle_one(job):
    sem, source, name, args = job
    command = [sem, "--run-core", source, name, str(STEP_LIMIT)] + [f"0x{a:x}" for a in args]
    result = subprocess.run(command, capture_output=True, text=True, timeout=600)
    text = (result.stdout + result.stderr).strip()
    m = re.search(r"EXECUTED_NOT_VERIFIED return=0x([0-9a-fA-F]+)", text)
    if m and result.returncode == 0:
        return ["RET", int(m.group(1), 16)]
    m = re.search(r"EXECUTED_NOT_VERIFIED diagnostic=(\w+)", text)
    if m:
        return ["DIAG", m.group(1)]
    if "STEP_LIMIT_NOT_VERIFICATION" in text:
        return ["STEP", None]
    return ["ERROR", f"exit={result.returncode} {text}"]


# ---------------------------------------------------------------- driver.c

DRIVER_HEAD = r'''/* V08 driver (generated by tools/proof-native/proof_native.py). It does not
 * define any entry function: the entries are in output.s.
 * Input (stdin): one vector per line: ID ENTRY KINDS TIMEOUT NARGS ARG...
 * KINDS: N normal call, P upper-bits probe (4 patterns), R reference (gcc-14
 * -O0 build of the same C file), S no source result (not run with argument C).
 * Output (stdout): ID KIND RESULT; RESULT is RET 0x..., TRAP brk#N, SIGNAL N,
 * TIMEOUT, DIED N or NOOUTPUT. With argument C, run only the normal calls and
 * label them C (the clang-19 / ld.lld-19 build). */
#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <signal.h>
#include <unistd.h>
#include <sys/wait.h>
#include <ucontext.h>

unsigned long long strtoull(const char *, char **, int);
void *malloc(unsigned long);
'''

DRIVER_TAIL = r'''
#define V08_POOL 4
typedef uint64_t (*v08_thunk)(const uint64_t *);
struct v08_slot { pid_t pid; int fd; long id; char kind[4]; };
static struct v08_slot v08_slots[V08_POOL];
static int v08_active;
static int v08_out_fd = -1;
static char v08_altstack[65536];

static void v08_signal(int sig, siginfo_t *info, void *context)
{
    ucontext_t *uc = context;
    char text[64];
    int n;
    (void)info;
    if (sig == SIGALRM) {
        n = snprintf(text, sizeof text, "TIMEOUT");
    } else if (sig == SIGTRAP || sig == SIGILL) {
        uint32_t insn = *(const uint32_t *)(uintptr_t)uc->uc_mcontext.pc;
        if ((insn & 0xffe0001fu) == 0xd4200000u)
            n = snprintf(text, sizeof text, "TRAP brk#%u", (insn >> 5) & 0xffffu);
        else
            n = snprintf(text, sizeof text, "SIGNAL %d", sig);
    } else {
        n = snprintf(text, sizeof text, "SIGNAL %d", sig);
    }
    if (write(v08_out_fd, text, (size_t)n) < 0) {}
    _exit(0);
}

static void v08_reap(void)
{
    int status = 0;
    pid_t pid = waitpid(-1, &status, 0);
    if (pid <= 0)
        return;
    for (int i = 0; i < V08_POOL; ++i) {
        struct v08_slot *s = &v08_slots[i];
        if (s->pid != pid)
            continue;
        char text[128];
        ssize_t n = read(s->fd, text, sizeof text - 1);
        close(s->fd);
        if (n > 0)
            text[n] = 0;
        else if (WIFSIGNALED(status))
            snprintf(text, sizeof text, "DIED %d", WTERMSIG(status));
        else
            snprintf(text, sizeof text, "NOOUTPUT %d", status);
        printf("%ld %s %s\n", s->id, s->kind, text);
        s->pid = 0;
        --v08_active;
        return;
    }
}

static void v08_launch(long id, const char *kind, v08_thunk f, const uint64_t *args, unsigned timeout)
{
    while (v08_active == V08_POOL)
        v08_reap();
    int fds[2];
    if (pipe(fds) != 0) {
        perror("pipe");
        _exit(2);
    }
    fflush(stdout);
    pid_t pid = fork();
    if (pid < 0) {
        perror("fork");
        _exit(2);
    }
    if (pid == 0) {
        close(fds[0]);
        v08_out_fd = fds[1];
        stack_t ss = { .ss_sp = v08_altstack, .ss_size = sizeof v08_altstack, .ss_flags = 0 };
        sigaltstack(&ss, 0);
        struct sigaction sa;
        memset(&sa, 0, sizeof sa);
        sa.sa_sigaction = v08_signal;
        sa.sa_flags = SA_SIGINFO | SA_ONSTACK;
        int sigs[] = { SIGTRAP, SIGILL, SIGSEGV, SIGBUS, SIGFPE, SIGALRM };
        for (unsigned i = 0; i < sizeof sigs / sizeof sigs[0]; ++i)
            sigaction(sigs[i], &sa, 0);
        alarm(timeout);
        uint64_t r = f(args);
        alarm(0);
        char text[64];
        int n = snprintf(text, sizeof text, "RET 0x%llx", (unsigned long long)r);
        if (write(v08_out_fd, text, (size_t)n) < 0) {}
        _exit(0);
    }
    close(fds[1]);
    for (int i = 0; i < V08_POOL; ++i) {
        if (v08_slots[i].pid == 0) {
            v08_slots[i].pid = pid;
            v08_slots[i].fd = fds[0];
            v08_slots[i].id = id;
            snprintf(v08_slots[i].kind, sizeof v08_slots[i].kind, "%s", kind);
            ++v08_active;
            return;
        }
    }
}

static const uint64_t v08_patterns[4] = { 0xDEADBEEFull, 0xFFFFFFFFull, 0x00000001ull, 0x80000000ull };

int main(int argc, char **argv)
{
    int second = argc > 1 && argv[1][0] == 'C';
    static char line[4096];
    while (fgets(line, sizeof line, stdin)) {
        char *p = line, *end;
        long id = (long)strtoull(p, &end, 10); p = end;
        long e = (long)strtoull(p, &end, 10); p = end;
        while (*p == ' ') ++p;
        char kinds[16]; int k = 0;
        while (*p && *p != ' ' && k < 15) kinds[k++] = *p++;
        kinds[k] = 0;
        unsigned timeout = (unsigned)strtoull(p, &end, 10); p = end;
        int nargs = (int)strtoull(p, &end, 10); p = end;
        uint64_t *args = malloc(sizeof(uint64_t) * (nargs > 0 ? nargs : 1));
        for (int i = 0; i < nargs; ++i) { args[i] = strtoull(p, &end, 16); p = end; }
        const struct v08_entry *entry = &v08_entries[e];
        if (second) {
            if (!strchr(kinds, 'S'))
                v08_launch(id, "C", entry->normal, args, timeout);
            continue;
        }
        if (strchr(kinds, 'N'))
            v08_launch(id, "N", entry->normal, args, timeout);
        if (strchr(kinds, 'P') && entry->probe) {
            for (int q = 0; q < 4; ++q) {
                uint64_t *dirty = malloc(sizeof(uint64_t) * nargs);
                for (int i = 0; i < nargs; ++i)
                    dirty[i] = (entry->narrow >> i) & 1 ? (v08_patterns[q] << 32) | (args[i] & 0xffffffffull) : args[i];
                char kind[4] = { 'P', (char)('1' + q), 0, 0 };
                v08_launch(id, kind, entry->probe, dirty, timeout);
            }
        }
        if (strchr(kinds, 'R'))
            v08_launch(id, "R", entry->ref, args, timeout);
    }
    while (v08_active > 0)
        v08_reap();
    fflush(stdout);
    return 0;
}
'''


def driver_source(entries):
    def ctype(kind):
        return TYPES[kind][1]

    def masked(kind, expr):
        return f"(uint64_t)(uint32_t)({expr})" if TYPES[kind][0] == 32 else f"(uint64_t)({expr})"

    def symbol(e):
        return "subject_main" if e["name"] == "main" else e["name"]

    def narrow(e):
        return any(TYPES[k][0] == 32 for k in e["params"])

    lines = [DRIVER_HEAD, "/* The entries: the real prototypes. */"]
    for e in entries:
        lines.append(f"{ctype(e['ret'])} {symbol(e)}({', '.join(map(ctype, e['params'])) or 'void'});")
    lines.append("/* The upper-bits probe: the same symbols, each parameter declared as a 64-bit integer. */")
    for e in filter(narrow, entries):
        params = ", ".join("unsigned long" for _ in e["params"])
        lines.append(f'{ctype(e["ret"])} v08_probe_{e["name"]}({params}) __asm__("{symbol(e)}");')
    lines.append("/* The reference: the same C file compiled by gcc-14 -O0, symbols renamed ref_NAME. */")
    for e in entries:
        lines.append(f"{ctype(e['ret'])} ref_{e['name']}({', '.join(map(ctype, e['params'])) or 'void'});")
    lines.append("")
    for e in entries:
        args = ", ".join(f"({ctype(k)})a[{i}]" for i, k in enumerate(e["params"]))
        lines.append(f"static uint64_t v08_n_{e['name']}(const uint64_t *a) "
                     f"{{ (void)a; return {masked(e['ret'], f'{symbol(e)}({args})')}; }}")
        lines.append(f"static uint64_t v08_r_{e['name']}(const uint64_t *a) "
                     f"{{ (void)a; return {masked(e['ret'], 'ref_' + e['name'] + '(' + args + ')')}; }}")
        if narrow(e):
            wide = ", ".join(f"(unsigned long)a[{i}]" for i in range(len(e["params"])))
            lines.append(f"static uint64_t v08_p_{e['name']}(const uint64_t *a) "
                         f"{{ return {masked(e['ret'], 'v08_probe_' + e['name'] + '(' + wide + ')')}; }}")
    lines.append("")
    lines.append("struct v08_entry { const char *name; uint64_t (*normal)(const uint64_t *); "
                 "uint64_t (*probe)(const uint64_t *); uint64_t (*ref)(const uint64_t *); unsigned long narrow; };")
    lines.append("static const struct v08_entry v08_entries[] = {")
    for e in entries:
        mask = sum(1 << i for i, k in enumerate(e["params"]) if TYPES[k][0] == 32)
        probe = f"v08_p_{e['name']}" if mask else "0"
        lines.append(f'    {{ "{e["name"]}", v08_n_{e["name"]}, {probe}, v08_r_{e["name"]}, 0x{mask:x}ul }},')
    lines.append("};")
    lines.append(DRIVER_TAIL)
    return "\n".join(lines)


# ---------------------------------------------------------------- prepare, run, compare

def prepare(work, case):
    meta, entries = load_entries(work, case)
    fixture = meta["fixture"]
    directory = work.run / case
    directory.mkdir(parents=True, exist_ok=True)
    plan = [{"entry": e["name"], "index": i, "args": args}
            for i, e in enumerate(entries) for args in vectors(case, fixture, e)]
    sem = str(work.binary("evoir-sem"))
    jobs = [(sem, entries[p["index"]]["source"], p["entry"], p["args"]) for p in plan]
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
        for p, result in zip(plan, pool.map(oracle_one, jobs)):
            p["oracle"] = result
    lines = []
    for vid, p in enumerate(plan):
        e = entries[p["index"]]
        narrow = any(TYPES[k][0] == 32 for k in e["params"])
        cls = p["oracle"][0]
        if cls == "RET":
            kinds, timeout = "N" + ("P" if narrow else "") + "R", TIMEOUT_RESULT
        elif cls == "STEP":
            kinds, timeout = "NRS", TIMEOUT_STEP
        else:
            kinds, timeout = "N", TIMEOUT_RESULT
        p["id"], p["kinds"] = vid, kinds
        lines.append(f"{vid} {p['index']} {kinds} {timeout} {len(p['args'])} " + " ".join(f"{a:x}" for a in p["args"]))
    (directory / "jobs.txt").write_text("\n".join(lines) + "\n")
    (directory / "driver.c").write_text(driver_source(entries))
    (directory / "plan.json").write_text(json.dumps({"case": case, "fixture": fixture, "entries": entries,
                                                     "vectors": plan}))
    # The container sees only this directory: the checked text and the fixture C files.
    shutil.copyfile(work.out / case / "output.s", directory / "output.s")
    (directory / "src").mkdir(exist_ok=True)
    for source in FIX.glob("*.c"):
        shutil.copyfile(source, directory / "src" / source.name)
    (directory / "symbols.txt").write_text(
        "ENTRIES " + " ".join(e["name"] for e in entries) + "\nSTATIC "
        + " ".join(e["name"] for e in entries if not e["global"]) + f"\nFIXTURE {fixture}\n")
    counts = {}
    for p in plan:
        counts[p["oracle"][0]] = counts.get(p["oracle"][0], 0) + 1
    print(f"{case}: entries={len(entries)} vectors={len(plan)} oracle={counts}", flush=True)


def run(work, case):
    directory = work.run / case
    result = subprocess.run(["bash", str(HERE / "run-module.sh"), str(directory)], capture_output=True, text=True)
    (directory / "run.log").write_text(result.stdout + result.stderr)
    print(f"{case}: run exit={result.returncode}", flush=True)
    if result.returncode != 0:
        print(result.stdout[-3000:], result.stderr[-3000:])
    return result.returncode == 0


def parse_native(path):
    found = {}
    if path.exists():
        for line in path.read_text().splitlines():
            parts = line.split(None, 2)
            if len(parts) == 3:
                found[(int(parts[0]), parts[1])] = parts[2].strip()
    return found


STAT_KEYS = ["vectors", "compared", "skipped_ub", "skipped_step", "skipped_error", "differences",
             "probe_calls", "probe_differences", "ref_compared", "ref_differences", "llvm_compared",
             "llvm_differences", "ub_native_same_trap", "step_ref_compared", "step_ref_differences"]


def compare(work, case):
    directory = work.run / case
    plan = json.loads((directory / "plan.json").read_text())
    native = parse_native(directory / "native.txt")
    native.update(parse_native(directory / "native-llvm.txt"))
    entries = {e["name"]: e for e in plan["entries"]}
    stats, differences, probe_differences, notes, step = {}, [], [], [], []
    for p in plan["vectors"]:
        e = entries[p["entry"]]
        s = stats.setdefault(p["entry"], dict({k: 0 for k in STAT_KEYS}, step_native={}))
        s["vectors"] += 1
        cls, value = p["oracle"]
        got = native.get((p["id"], "N"), "MISSING")
        args = [f"0x{a:x}" for a in p["args"]]
        if cls == "DIAG":
            s["skipped_ub"] += 1
            if got == f"TRAP brk#{int(value.lstrip('E'))}":
                s["ub_native_same_trap"] += 1
            else:
                notes.append({"entry": p["entry"], "args": args, "oracle": value, "native": got})
            continue
        if cls == "STEP":
            s["skipped_step"] += 1
            outcome = got.split()[0]
            s["step_native"][outcome] = s["step_native"].get(outcome, 0) + 1
            ref = native.get((p["id"], "R"), "MISSING")
            step.append({"entry": p["entry"], "args": args, "native": got, "gcc_O0": ref})
            if got.startswith("RET") and ref.startswith("RET"):
                s["step_ref_compared"] += 1
                if got != ref:
                    s["step_ref_differences"] += 1
                    differences.append({"entry": p["entry"], "args": args, "oracle": "gcc-14 -O0 " + ref,
                                        "kind": "N", "actual": got, "note": "no source result (step limit)"})
            continue
        if cls != "RET":
            s["skipped_error"] += 1
            notes.append({"entry": p["entry"], "args": args, "oracle": value, "native": got})
            continue
        expected = f"RET 0x{bits(value, TYPES[e['ret']][0]):x}"
        s["compared"] += 1
        if got != expected:
            s["differences"] += 1
            differences.append({"entry": p["entry"], "args": args, "oracle": expected, "kind": "N", "actual": got})
        for q, pattern in PATTERNS.items():
            probe = native.get((p["id"], q))
            if probe is None:
                continue
            s["probe_calls"] += 1
            if probe != expected:
                s["probe_differences"] += 1
                dirty = [f"0x{(pattern << 32) | bits(a, 32):x}" if TYPES[k][0] == 32 else f"0x{a:x}"
                         for a, k in zip(p["args"], e["params"])]
                probe_differences.append({"entry": p["entry"], "args": args, "probe_args": dirty, "pattern": q,
                                          "oracle": expected, "normal": got, "actual": probe})
        for kind, key in (("R", "ref"), ("C", "llvm")):
            other = native.get((p["id"], kind))
            if other is None:
                continue
            s[f"{key}_compared"] += 1
            if other != expected:
                s[f"{key}_differences"] += 1
                record = {"entry": p["entry"], "args": args, "oracle": expected, "kind": kind, "actual": other}
                (notes if kind == "R" else differences).append(record)
    summary = {"case": case, "fixture": plan["fixture"], "entries": stats, "differences": differences,
               "probe_differences": probe_differences, "notes": notes, "step_limit": step,
               "native_lines": len(native)}
    (directory / "compare.json").write_text(json.dumps(summary, indent=1))
    total = {k: sum(v[k] for v in stats.values()) for k in STAT_KEYS}
    print(f"{case}: {total}", flush=True)


# ---------------------------------------------------------------- static scan

REG = re.compile(r"\b([xw])([0-9]|[12][0-9]|30)\b")
NO_DEST = ("str", "stp", "strb", "strh", "stur", "cmp", "cmn", "tst", "cbz", "cbnz", "tbz", "tbnz",
           "b", "bl", "blr", "br", "ret", "brk", "nop")


def asm_functions(text):
    result, name = {}, None
    for line in text.splitlines():
        line = line.split("//")[0].strip()
        if re.match(r"^[A-Za-z_]\w*:$", line):
            name = line[:-1]
            result[name] = []
        elif line.startswith(".size"):
            name = None
        elif name and re.match(r"^\.L\w+:$", line):
            result[name].append(("label", line[:-1]))
        elif name and line and not line.startswith("."):
            op, _, rest = line.partition(" ")
            operands, depth, cur = [], 0, ""
            for ch in rest:
                depth += (ch == "[") - (ch == "]")
                if ch == "," and depth == 0:
                    operands.append(cur.strip())
                    cur = ""
                else:
                    cur += ch
            if cur.strip():
                operands.append(cur.strip())
            result[name].append(("insn", op, operands))
    return result


def blocks(body):
    order, current, label = [], [], "entry"
    for item in body:
        if item[0] == "label":
            order.append((label, current))
            label, current = item[1], []
        else:
            current.append(item)
    order.append((label, current))
    graph = {}
    for i, (label, insns) in enumerate(order):
        fall = [order[i + 1][0]] if i + 1 < len(order) else []
        op, ops = (insns[-1][1], insns[-1][2]) if insns else ("", [])
        if op == "b":
            succ = [ops[0]]
        elif op.startswith("b.") or op in ("cbz", "cbnz", "tbz", "tbnz"):
            succ = [ops[-1]] + fall
        elif op in ("ret", "brk", "br"):
            succ = []
        else:
            succ = fall
        graph[label] = (insns, succ)
    return order[0][0], graph


def scan_entry(functions, name, dirty_regs):
    """Follow the control flow from the entry. A register of a 32-bit parameter
    is dirty until an instruction writes it. Report each read of the X form of a
    dirty register and each call that passes a dirty argument register."""
    first, graph = blocks(functions[name])
    state, work, seen, findings = {first: set(dirty_regs)}, [first], {}, set()
    while work:
        label = work.pop()
        if label not in graph or seen.get(label) == frozenset(state[label]):
            continue
        seen[label] = frozenset(state[label])
        dirty = set(state[label])
        insns, succ = graph[label]
        for _, op, ops in insns:
            dest, reads = ([], ops) if op in NO_DEST else ((ops[:2], ops[2:]) if op == "ldp" else (ops[:1], ops[1:]))
            for operand in reads:
                for kind, num in REG.findall(operand):
                    if kind == "x" and int(num) in dirty:
                        masked = op == "and" and ops[-1] in ("#4294967295", "#0xffffffff")
                        findings.add((f"{op} {', '.join(ops)}", f"x{num}", "masked" if masked else "READS UPPER BITS"))
            if op in ("bl", "blr"):
                for n in sorted(r for r in dirty if r <= 7):
                    findings.add((f"{op} {', '.join(ops)}", f"x{n}", "passed to callee"))
                dirty -= set(range(19))
            for operand in dest:
                for _, num in REG.findall(operand):
                    dirty.discard(int(num))
        for s in succ:
            state[s] = state.get(s, set()) | dirty
            work.append(s)
    return sorted(findings)


def scan(work, cases):
    lines, total, counts = [], 0, {}
    for case in cases:
        plan = json.loads((work.run / case / "plan.json").read_text())
        functions = asm_functions((work.out / case / "output.s").read_text())
        for e in plan["entries"]:
            narrow = [i for i, k in enumerate(e["params"]) if TYPES[k][0] == 32 and i < 8]
            if not narrow:
                continue
            total += 1
            for insn, reg, verdict in scan_entry(functions, e["name"], narrow):
                counts[verdict] = counts.get(verdict, 0) + 1
                lines.append(f"{case} {e['name']}: {insn} -> {reg} {verdict}")
    summary = {"entries_scanned": total, "findings": counts}
    (work.logs / "scan.txt").write_text(json.dumps(summary) + "\n" + "\n".join(lines) + "\n")
    print(f"static scan: {summary}", flush=True)
    return summary


# ---------------------------------------------------------------- report

def report(work):
    summaries = {c: json.loads((work.run / c / "compare.json").read_text())
                 for c in ORDER if (work.run / c / "compare.json").exists()}
    modules, pilot, entries_total = {}, {k: 0 for k in STAT_KEYS}, 0
    probe_entries = {"with_32_bit_parameter": 0, "probed": 0, "not_probed": []}
    step_outcomes = {}
    for case, c in summaries.items():
        totals = {k: sum(e[k] for e in c["entries"].values()) for k in STAT_KEYS}
        text = (work.run / case / "text.txt")
        modules[case] = dict(totals, entries=len(c["entries"]),
                             gnu_vs_llvm_text=text.read_text().strip() if text.exists() else "missing")
        if case.startswith("opt-"):
            continue
        entries_total += len(c["entries"])
        for k in STAT_KEYS:
            pilot[k] += totals[k]
        plan = json.loads((work.run / case / "plan.json").read_text())
        for e in plan["entries"]:
            if any(TYPES[k][0] == 32 for k in e["params"]):
                probe_entries["with_32_bit_parameter"] += 1
                if c["entries"][e["name"]]["probe_calls"]:
                    probe_entries["probed"] += 1
                else:
                    probe_entries["not_probed"].append(f"{case}:{e['name']}")
        for item in c["step_limit"]:
            outcome = item["native"].split()[0]
            step_outcomes[outcome] = step_outcomes.get(outcome, 0) + 1
    scan_text = (work.logs / "scan.txt")
    scan_summary = json.loads(scan_text.read_text().splitlines()[0]) if scan_text.exists() else {}
    tools = (work.run / "musl-abs" / "tools.txt")
    result = {
        "binaries": (work.root / "digests.txt").read_text().splitlines(),
        "environment": tools.read_text().splitlines() if tools.exists() else [],
        "modules": modules, "pilot_entries": entries_total, "pilot_totals": pilot,
        "pilot_step_limit_native_outcomes": step_outcomes, "pilot_probe_entries": probe_entries,
        "static_scan": scan_summary,
        "differences": {c: s["differences"] for c, s in summaries.items() if s["differences"]},
        "probe_differences": {c: s["probe_differences"] for c, s in summaries.items() if s["probe_differences"]},
        "notes": {c: s["notes"] for c, s in summaries.items() if s["notes"]},
    }
    (work.root / "results.json").write_text(json.dumps(result, indent=1))
    lines = ["# Proof-required output executed on Linux AArch64", "",
             "Oracle: the source interpreter (`evoir-sem --run-core`, step limit 10,000,000).",
             "gcc: gcc-14 -O0 build of the same C file, compared/differences against the oracle.",
             "llvm: output.s assembled by clang-19 and linked by ld.lld-19, compared/differences.", "",
             "| case | entries | vectors | compared | skipped UB | skipped step limit | differences "
             "| probe calls | probe differences | gcc | llvm |", "|---|---|---|---|---|---|---|---|---|---|---|"]
    for case, m in modules.items():
        lines.append(f"| {case} | {m['entries']} | {m['vectors']} | {m['compared']} | {m['skipped_ub']} "
                     f"| {m['skipped_step']} | {m['differences']} | {m['probe_calls']} | {m['probe_differences']} "
                     f"| {m['ref_compared']}/{m['ref_differences']} | {m['llvm_compared']}/{m['llvm_differences']} |")
    lines += ["", f"Pilot totals ({entries_total} entries): " + json.dumps(pilot),
              f"Step-limit native outcomes: {json.dumps(step_outcomes)}",
              f"Probe entries: {json.dumps(probe_entries)}", f"Static scan: {json.dumps(scan_summary)}", "",
              "## Differences", "", json.dumps(result["differences"], indent=1),
              "", "## Probe differences", "", json.dumps(result["probe_differences"], indent=1),
              "", "## Notes", "", json.dumps(result["notes"], indent=1),
              "", "## Vectors without a source result (step limit)", ""]
    for case, c in summaries.items():
        for item in c["step_limit"]:
            lines.append(f"- {case} {item['entry']}({', '.join(item['args'])}): native {item['native']}; "
                         f"gcc-14 -O0 {item['gcc_O0']}")
    lines += ["", "## Environment", "", *result["environment"], "", *result["binaries"], ""]
    (work.root / "results.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines[:30]))


# ---------------------------------------------------------------- main

def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("step", choices=["all", "freeze", "produce", "prepare", "run", "compare", "scan", "report"])
    parser.add_argument("work", help="work directory, outside the repository")
    parser.add_argument("cases", nargs="*", help=f"default: all cases ({', '.join(ORDER)})")
    for name in DEFAULT_BINARIES:
        parser.add_argument(f"--{name}", help=f"default: ${DEFAULT_BINARIES[name][0]} or {DEFAULT_BINARIES[name][1]}")
    args = parser.parse_args()
    work = Work(args.work)
    cases = args.cases or ORDER
    unknown = [c for c in cases if c not in CASES]
    if unknown:
        sys.exit(f"unknown cases: {unknown}")
    if args.step in ("all", "freeze") or (args.step == "produce" and not work.bin.exists()):
        freeze(work, args)
    if args.step in ("all", "produce"):
        with open(work.logs / "produce.log", "a") as log:
            refused = [c for c in cases if not produce(work, c, log)]
        if refused:
            sys.exit(f"not accepted: {refused}; see {work.logs / 'produce.log'}")
    for case in cases if args.step in ("all", "prepare", "run", "compare") else []:
        if args.step in ("all", "prepare"):
            prepare(work, case)
        if args.step in ("all", "run") and not run(work, case):
            sys.exit(f"{case}: the native run failed; see {work.run / case / 'run.log'}")
        if args.step in ("all", "compare"):
            compare(work, case)
        print(f"DISK {case}: {free_gib(work.root)} GiB free", flush=True)
        if free_gib(work.root) < STOP_GIB:
            sys.exit(f"stop: free disk space is below {STOP_GIB} GiB")
    if args.step in ("all", "scan"):
        scan(work, cases)
    if args.step in ("all", "report"):
        report(work)


if __name__ == "__main__":
    main()
