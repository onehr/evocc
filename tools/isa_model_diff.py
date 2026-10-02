#!/usr/bin/env python3
"""Execute the frozen N07 E2 fixtures, never substitute local ISA semantics."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess

from tools.ladder.codegen_census import INSTRUCTION, _layout
from tools.ladder.eqsat_elf import inspect_elf, inspect_object_size

ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "tests/contracts/codegen-n07-model-20261003.json"
RET = 0xd65f03c0


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def vectors(path, plan):
    rows = []
    for line in path.read_text().splitlines():
        fields = line.split()
        require(len(fields) == 5 and re.fullmatch(r"model_[a-z]+_[0-9]+", fields[0]),
                "malformed corpus row")
        require(all(re.fullmatch(r"[0-9a-f]{16}", x) for x in fields[2:]),
                "malformed 64-bit input or result")
        rows.append((fields[0], int(fields[1]), *(int(x, 16) for x in fields[2:])))
    expected = [(f"model_{op}_{width}", i) for width in plan["widths"]
                for op in plan["operations"] for i in range(plan["inputs_per_shape"])]
    expected += [(f"model_const_{i}", 0) for i in range(len(plan["constant_controls_hex"]))]
    require([(r[0], r[1]) for r in rows] == expected, "corpus differs from frozen shape order")
    return rows


def instruction_sequences(data):
    metrics = inspect_object_size(data)
    require(not any(metrics[f] for f in ("literal_pool_or_intext_data_bytes",
                                        "unclassified_text_bytes", "other_executable_bytes")),
            "model fixtures contain data or unknown executable sections")
    require(not any(s["type"] in (4, 9) and s["size"] for s in inspect_elf(data)["sections"]),
            "model fixtures have relocations")
    bodies, code = _layout(data)
    sequences = {}
    for body in bodies:
        require(body["body_bytes"] == body["instruction_bytes"], "non-instruction function bytes")
        words = [code[body["section"], a][1] for a in range(body["start"], body["end"], 4)]
        require(words and words[-1] == RET and RET not in words[:-1],
                "fixture is not one straight-line body followed by RET")
        require(1 < len(words) <= 129, "fixture outside bounded instruction count")
        require(len(body["aliases"]) == 1, "unexpected function alias")
        sequences[body["name"]] = words[:-1]
    return sequences, bodies, code, metrics


def check_results(rows, output, *, model=False):
    lines = output.decode("ascii").splitlines()
    require(len(lines) == len(rows), "incomplete execution output")
    for row, line in zip(rows, lines):
        fields = line.split()
        require(len(fields) == (2 if model else 1) and re.fullmatch(r"[0-9a-f]{16}", fields[0]),
                "malformed execution result")
        require(int(fields[0], 16) == row[4], f"result mismatch: {row[0]}/{row[1]}: {line}")
        if model:
            require(fields[1] == "0", "unexpected vendor-model NZCV modification")
    return len(lines)


def c_harness(rows):
    declarations = "\n".join(f"extern uint64_t {name}(uint64_t, uint64_t);"
                             for name in dict.fromkeys(row[0] for row in rows))
    cases = "\n".join(f"    {{{name}, UINT64_C(0x{a:016x}), UINT64_C(0x{b:016x}), "
                      f"UINT64_C(0x{expected:016x})}}," for name, _, a, b, expected in rows)
    return """/* Generated transport and expected interpreter outputs, not ISA semantics. */
#include <stdint.h>
#ifndef MODEL_SEMIHOST
#include <stdio.h>
#endif
""" + declarations + """
struct Case { uint64_t (*fn)(uint64_t, uint64_t); uint64_t a, b, expected; };
static const struct Case cases[] = {
""" + cases + """
};
static void emit(uint64_t value) {
    const char *hex = "0123456789abcdef";
    char text[18];
    for (unsigned i = 0; i < 16; ++i) text[i] = hex[(value >> (60 - 4*i)) & 15];
    text[16] = '\\n'; text[17] = 0;
#ifdef MODEL_SEMIHOST
    register uint64_t op __asm__("x0") = 4;
    register const char *arg __asm__("x1") = text;
    __asm__ volatile("hlt #0xf000" : "+r"(op), "+r"(arg) : : "memory");
#else
    fputs(text, stdout);
#endif
}
int main(void) {
    unsigned failures = 0;
    for (unsigned i = 0; i < sizeof(cases)/sizeof(cases[0]); ++i) {
        uint64_t result = cases[i].fn(cases[i].a, cases[i].b);
        emit(result);
        failures += result != cases[i].expected;
    }
    return failures ? 1 : 0;
}
"""


START = """/* Diskless test entry/exit only; corpus bodies are linked unchanged. */
.section .text.start,"ax"
.global _start
_start:
    ldr x0, =0x44000000
    mov sp, x0
    bl main
    sub sp, sp, #16
    mov x1, #0x26
    movk x1, #2, lsl #16
    str x1, [sp]
    str x0, [sp, #8]
    mov x1, sp
    mov x0, #0x20
    hlt #0xf000
    b .
.section .note.GNU-stack,"",@progbits
"""
LINK = """ENTRY(_start)
SECTIONS {
    . = 0x40000000;
    .text : { *(.text.start) *(.text .text.*) }
    .rodata : { *(.rodata .rodata.*) }
    .data : { *(.data .data.*) }
    .bss : { *(.bss .bss.*) *(COMMON) }
}
"""


def run(args):
    directory = args.capture.resolve()
    plan = json.loads(PLAN.read_text())
    require(shutil.disk_usage(directory).free >= plan["resources"]["min_start_free_bytes"],
            "disk preflight below start limit")
    commands = []

    def command(name, argv, *, stdin=None, expected=0, retain=True):
        proc = subprocess.run(argv, input=stdin, capture_output=True, timeout=120)
        if retain:
            (directory / (name + ".stdout")).write_bytes(proc.stdout)
            (directory / (name + ".stderr")).write_bytes(proc.stderr)
        commands.append(dict(name=name, argv=[str(x) for x in argv], exit_code=proc.returncode,
                             stdout_sha256=sha(proc.stdout), stderr_sha256=sha(proc.stderr)))
        require(proc.returncode == expected, f"{name}: exit {proc.returncode}; expected {expected}")
        return proc.stdout, proc.stderr

    rows = vectors(directory / "vectors.txt", plan["corpus"])
    require(sha((directory / "vectors.txt").read_bytes()) == plan["corpus"]["concrete_vectors_sha256"],
            "concrete vectors differ from frozen corpus")
    require(sha((directory / "corpus.evo").read_bytes()) == plan["corpus"]["concrete_evoir_sha256"],
            "concrete EvoIR differs from frozen corpus")
    sequences, bodies, code, metrics = instruction_sequences((directory / "corpus-linux.o").read_bytes())
    require(set(sequences) == {r[0] for r in rows}, "object symbols differ from corpus")
    # Mach-O aliases can replace the first function's disassembly label. Match
    # exact words by address, and check named symbol addresses separately.
    text, _ = command("darwin-disassembly", [args.llvm / "llvm-objdump", "-d",
                       directory / "corpus-darwin.o"], retain=False)
    actual = {int(m[1], 16): int(m[2], 16) for line in text.decode().splitlines()
              if (m := INSTRUCTION.match(line))}
    require(actual == {address: value[1] for (_, address), value in code.items()},
            "Darwin and Linux instruction words differ")
    symbols, _ = command("darwin-symbols", [args.llvm / "llvm-nm", "--numeric-sort", "--defined-only",
                          directory / "corpus-darwin.o"], retain=False)
    symbols = {parts[2][1:]: int(parts[0], 16) for line in symbols.decode().splitlines()
               if len(parts := line.split()) == 3 and parts[1] == "T"}
    require(symbols == {body["name"]: body["start"] for body in bodies}, "Darwin symbol layout differs")
    (directory / "harness.c").write_text(c_harness(rows))
    (directory / "start.s").write_text(START)
    (directory / "link.ld").write_text(LINK)
    stream = "".join(f"{a:x} {b:x} {len(sequences[name])} " +
                     " ".join(f"{word:08x}" for word in sequences[name]) + "\n"
                     for name, _, a, b, _ in rows).encode()
    (directory / "model-input.txt").write_bytes(stream)
    model_cmd = ["container", "exec", "-i", args.container, "/work/model/runner"]
    model, _ = command("model", model_cmd, stdin=stream)
    model_count = check_results(rows, model, model=True)
    for name, stream, exit_code in (
        ("unknown-opcode", b"0 0 1 00000000\n", 3),
        ("unsupported-sp", b"0 0 1 910003e0\n", 3),
        ("truncated-header", b"0 0", 2),
        ("truncated-body", b"0 0 2 8b010000\n", 2),
    ):
        command(name, model_cmd, stdin=stream, expected=exit_code)
    command("build-native", ["/usr/bin/clang", "-O2", directory / "harness.c",
                             directory / "corpus-darwin.o", "-o", directory / "native"])
    native, _ = command("native", [directory / "native"])
    native_count = check_results(rows, native)
    command("build-qemu", ["container", "exec", args.container, "gcc-14", "-O2", "-ffreestanding",
                            "-fno-builtin", "-fno-stack-protector", "-mgeneral-regs-only",
                            "-DMODEL_SEMIHOST", "-nostdlib", "-static", "-no-pie", "-Wl,--build-id=none",
                            "-Wl,-T,/work/link.ld", "/work/start.s", "/work/harness.c",
                            "/work/corpus-linux.o", "-o", "/work/qemu.elf"])
    qemu_spec = plan["source"]["qemu"]
    qemu = Path(qemu_spec["path"])
    require(sha(qemu.read_bytes()) == qemu_spec["sha256"], "frozen QEMU differs")
    _, emulated = command("qemu", [qemu, "-M", qemu_spec["machine"], "-cpu", qemu_spec["cpu"],
                          "-accel", qemu_spec["accelerator"], "-m", str(qemu_spec["memory_mib"]),
                          "-nographic", "-monitor", "none", "-serial", "none", "-nic", "none",
                          "-semihosting-config", "enable=on,target=native", "-kernel", directory / "qemu.elf"])
    qemu_count = check_results(rows, emulated)
    capture_bytes = sum(p.stat().st_size for p in directory.rglob("*") if p.is_file())
    require(capture_bytes <= plan["resources"]["capture_limit_bytes"], "capture limit exceeded")
    result = dict(status="BOUNDED_DIFFERENTIAL_PASS", shape_count=len(sequences), cases=len(rows),
                  model=model_count, native=native_count, qemu=qemu_count,
                  negative_boundaries=4, object_metrics=metrics,
                  files={name: sha((directory / name).read_bytes()) for name in
                         ("vectors.txt", "corpus.evo", "corpus-linux.o", "corpus-darwin.o",
                          "harness.c", "native", "qemu.elf", "model/runner")},
                  instruction_words={name: [f"{word:08x}" for word in words]
                                     for name, words in sequences.items()},
                  commands=commands, capture_bytes=capture_bytes, claim_ceiling=plan["claim_ceiling"])
    (directory / "differential.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: result[key] for key in ("status", "shape_count", "cases", "model", "native", "qemu")}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture", type=Path, required=True)
    parser.add_argument("--container", required=True)
    parser.add_argument("--llvm", type=Path, default=Path("/opt/homebrew/opt/llvm/bin"))
    run(parser.parse_args())
