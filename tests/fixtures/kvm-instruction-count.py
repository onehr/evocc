#!/usr/bin/env python3
"""Bounded read-only instruction diagnosis, NOT a replacement acceptance lane.

Reuse a matching Image/System.map, existing 256-KiB KVM diagnostic initramfs
and 64-MiB snapshot scratch disk. KASLR is disabled only for this measurement.
No elapsed-time comparison is valid with this instrumentation enabled.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import uuid


def sha(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def ranges(system_map, pattern):
    # Every symbol (not only selected functions) terminates the previous range.
    # Co-located aliases count once; data and section boundaries are not functions.
    symbols = {}
    for line in system_map.read_text().splitlines():
        address, kind, name = line.split()
        symbols.setdefault(int(address, 16), []).append((kind, name))
    result = []
    addresses = sorted(symbols)
    for lo, hi in zip(addresses, addresses[1:]):
        names = [name for kind, name in symbols[lo] if kind in ("t", "T")]
        chosen = [name for name in names if pattern.search(name)]
        if chosen:
            name = "|".join(sorted(set(chosen)))
            if len(name) > 255 or hi - lo > 1024 * 1024:
                raise ValueError("unbounded symbol range: " + name)
            result.append(f"{lo:x} {hi:x} {name}\n")
    if not result:
        raise ValueError("no selected symbol ranges")
    return "".join(result)


def root_counts_match(serial, rows):
    roots = (
        ("kvm_arch_flush_shadow_memslot", "kvm_free_stage2_pgd")
        if ("one-mode memslot diagnostic" in serial)
        else ("kvm_handle_guest_abort",)
    )
    for name in roots:
        traced = len(re.findall(r"\b" + name + r"\(\)", serial))
        entries = sum(row["entries"] for row in rows if name in row["name"].split("|"))
        if not traced or traced != entries:
            return False
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "image",
        "system-map",
        "qemu",
        "plugin",
        "payload",
        "scratch",
        "output",
    ):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument(
        "--filter",
        default=r"kvm|stage2|pgtable|pte_|user_mem_abort|faultin_page|folio_put|free_unref|free_pages",
    )
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    paths = {
        name: getattr(args, name).resolve()
        for name in ("image", "system_map", "qemu", "plugin", "payload", "scratch")
    }
    before = {name: sha(path) for name, path in paths.items()}
    selected = args.output.resolve() / "ranges.txt"
    selected.write_text(ranges(paths["system_map"], re.compile(args.filter)))
    nonce = "kvm-insn-" + uuid.uuid4().hex
    command = [
        str(paths["qemu"]),
        "-M",
        "virt-11.0,virtualization=on,gic-version=3,dtb-randomness=off",
        "-accel",
        "tcg",
        "-cpu",
        "neoverse-n2",
        "-smp",
        "4",
        "-m",
        "4096",
        "-L",
        "/opt/homebrew/share/qemu",
        "-nic",
        "none",
        "-kernel",
        str(paths["image"]),
        "-initrd",
        str(paths["payload"]),
        "-drive",
        f"file={paths['scratch']},format=raw,if=none,id=selftest,snapshot=on",
        "-device",
        "virtio-blk-device,drive=selftest",
        "-append",
        f"console=ttyAMA0 panic=-1 kunit.autorun=0 nokaslr evo_nonce={nonce}",
        "-nographic",
        "-no-reboot",
        "-plugin",
        f"file={paths['plugin']},ranges={selected}",
        "-d",
        "plugin",
        "-D",
        str(args.output.resolve() / "instructions.log"),
    ]
    record = {
        "scope": __doc__,
        "argv": command,
        "inputs": before,
        "paths": {name: str(path) for name, path in paths.items()},
        "nonce": nonce,
        "status": "RUNNING",
    }
    receipt = args.output / "result.json"
    receipt.write_text(json.dumps(record, indent=2) + "\n")
    env = dict(os.environ)
    # The retained private QEMU has an opt-in diagnostic intervention. Counts
    # must observe its unmodified execution path, not that intervention.
    env.pop("EVOCC_QEMU_STORE_DIAGNOSTIC", None)
    with (args.output / "serial.log").open("w") as log:
        try:
            result = subprocess.run(
                command,
                env=env,
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=180,
                check=False,
            )
            code = result.returncode
        except subprocess.TimeoutExpired:
            code = 124
    serial = (args.output / "serial.log").read_text()
    markers = [
        serial.find("EVO_INIT_ALIVE"),
        serial.find("EVO_BOOT_NONCE " + nonce),
        serial.find("EVO_INIT_DONE"),
        serial.find("reboot: Power down"),
    ]
    rows = []
    for line in (args.output / "instructions.log").read_text().splitlines():
        if line.startswith("KVM_INSN "):
            _, address, instructions, entries, name = line.split()
            rows.append(
                {
                    "name": name,
                    "address": address,
                    "instructions": int(instructions),
                    "entries": int(entries),
                }
            )
    unchanged = all(sha(path) == before[name] for name, path in paths.items())
    checks = {
        "processExitZero": code == 0,
        "inputsUnchanged": unchanged,
        "orderedBootShutdown": -1 not in markers and markers == sorted(set(markers)),
        "nonceOnce": serial.count("EVO_BOOT_NONCE " + nonce) == 1,
        "kaslrDisabled": "KASLR disabled" in serial,
        "diagnosticPassed": bool(
            re.search(
                r"^ok 1 one-mode (?:demand-paging|memslot) diagnostic", serial, re.M
            )
        ),
        "entriesMatchIndependentFtrace": root_counts_match(serial, rows),
        "nonemptyCounts": bool(rows),
    }
    record.update(
        status="PASS" if all(checks.values()) else "FAIL",
        exitCode=code,
        checks=checks,
        counts=sorted(rows, key=lambda row: -row["instructions"]),
        serialSha256=sha(args.output / "serial.log"),
        instructionLogSha256=sha(args.output / "instructions.log"),
        rangeSha256=sha(selected),
    )
    receipt.write_text(json.dumps(record, indent=2) + "\n")
    print(
        json.dumps(
            {
                "status": record["status"],
                "checks": checks,
                "top": record["counts"][:20],
            },
            indent=2,
        ),
        flush=True,
    )
    return 0 if record["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
