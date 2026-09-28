#!/usr/bin/env python3
"""Small README captures, not a benchmark campaign or new acceptance receipt.

--capture runs six existing ARM64 binaries in the sealed container. --render
renders their verbatim stdout as labeled SVG panels, not terminal screenshots.
Linux and Doom reuse explicitly identified, already recorded output.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import html
import json
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import textwrap

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs/images/showcase"
FIXTURES = ROOT / "tools/showcase"
APPS = {
    "tinycc": ("tinycc", "tcc", "TinyCC", "C compiler compiling C"),
    "sqlite": ("sqlite/sqlite-src-3530400", "testfixture", "SQLite", "SQL / transactions / integrity"),
    "lua": ("lua/lua-5.5.1", "src/lua", "Lua", "Floating-point Mandelbrot script"),
    "jq": ("jq/jq-1.8.2", "jq", "jq", "JSON aggregation"),
    "zstd": ("zstd/zstd-1.5.7", "programs/zstd", "Zstandard", "Compression round trip"),
    "cjson": ("cjson/cJSON-1.7.19", "cJSON_test", "cJSON", "Upstream JSON example (excerpt)"),
}


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text())


def run(argv, *, cwd, input_data=None):
    result = subprocess.run([str(a) for a in argv], cwd=cwd, input=input_data,
                            capture_output=True, timeout=30, check=True)
    return {"argv": [str(a) for a in argv], "exitCode": result.returncode,
            "stdout": result.stdout.decode("utf-8"), "stderr": result.stderr.decode("utf-8")}


def audit(name, tree, binary):
    receipt_path = ROOT / f"tests/contracts/ladder-{name}-receipt.json"
    receipt = read_json(receipt_path)
    if name == "tinycc":
        trial = receipt["trialAudits"][-1]
        if digest(binary) != trial["binarySha256"]:
            raise ValueError("TinyCC binary no longer matches its tested receipt")
        # This receipt binds the final binary itself. Intermediate objects may
        # have since been reused by compiler diagnostics; do not relink them.
        expected = {}
    else:
        rows = [json.loads(line) for line in
                (ROOT / receipt["evidence"] / "units.jsonl").read_text().splitlines()]
        installed = set(receipt["substitutedUnits"])
        expected = {r["unit"]: r["objectSha256"] for r in rows if r["unit"] in installed}
        if set(expected) != installed:
            raise ValueError(f"incomplete installed-object evidence: {name}")
    for unit, sha in expected.items():
        if digest(tree / unit) != sha:
            raise ValueError(f"installed object differs from receipt: {name}/{unit}")
    return {"buildReceipt": str(receipt_path.relative_to(ROOT)),
            "buildReceiptSha256": digest(receipt_path), "buildRun": receipt["run"],
            "binaryMatchesOriginalReceipt": name == "tinycc",
            "verifiedInstalledObjects": len(expected), "binary": str(binary.relative_to(ROOT)),
            "binarySha256": digest(binary)}


def capture():
    OUT.mkdir(parents=True, exist_ok=True)
    records = {"capturedAt": datetime.now(timezone.utc).isoformat(),
               "runtime": platform.platform(), "kind": "small showcase executions, not full tests",
               "fixtures": {p.name: digest(p) for p in sorted(FIXTURES.iterdir()) if p.is_file()},
               "programs": {}}
    with tempfile.TemporaryDirectory(prefix="evocc-showcase-") as tmp:
        work = Path(tmp)
        for name, (relative, exe, _, _) in APPS.items():
            tree = ROOT / ".build/ladder" / relative
            binary = tree / exe
            record = audit(name, tree, binary)
            commands = []
            if name == "tinycc":
                commands.append(run([binary, "-B" + str(tree), "-I" + str(tree / "include"),
                                     FIXTURES / "hello.c", "-o", work / "hello"], cwd=work))
                commands.append(run([work / "hello"], cwd=work))
            elif name == "sqlite":
                commands.append(run([binary, FIXTURES / "query.tcl"], cwd=work))
            elif name == "lua":
                commands.append(run([binary, FIXTURES / "mandelbrot.lua"], cwd=work))
            elif name == "jq":
                commands.append(run([binary, "-M", "-f", FIXTURES / "sales.jq", FIXTURES / "sales.json"], cwd=work))
                result = json.loads(commands[-1]["stdout"])
                if result != [{"item": "coffee", "units": 5, "revenue": 22.5},
                              {"item": "tea", "units": 5, "revenue": 10}]:
                    raise ValueError("jq result mismatch")
            elif name == "zstd":
                commands.append(run([binary, "--version"], cwd=work))
                original = (FIXTURES / "sales.json").read_bytes() * 1000
                (work / "input.json").write_bytes(original)
                commands.append(run([binary, "-f", work / "input.json", "-o", work / "input.zst"], cwd=work))
                commands.append(run([binary, "-d", "-f", work / "input.zst", "-o", work / "roundtrip.json"], cwd=work))
                if (work / "roundtrip.json").read_bytes() != original:
                    raise ValueError("zstd round-trip mismatch")
                record["roundTrip"] = {"bytes": len(original), "compressedBytes": (work / "input.zst").stat().st_size,
                                       "sha256": digest(work / "roundtrip.json"), "equal": True}
            else:
                commands.append(run([binary], cwd=tree))
            if digest(binary) != record["binarySha256"]:
                raise ValueError(f"binary changed while running: {name}")
            record["commands"] = commands
            records["programs"][name] = record
            # Preserve stdout/stderr separately, without inventing shell prompts.
            for stream in ("stdout", "stderr"):
                (OUT / f"{name}.{stream}.txt").write_text("".join(c[stream] for c in commands))
            print(f"{name}: capture exit 0; {record['verifiedInstalledObjects']} installed objects verified", flush=True)
    (OUT / "captures.json").write_text(json.dumps(records, indent=2) + "\n")


def panel(title, subtitle, lines, footer):
    wrapped = [part for line in lines for part in (textwrap.wrap(line.expandtabs(2), width=70, replace_whitespace=False,
                                                               drop_whitespace=False) or [""])]
    size = min(23, 310 / max(1, len(wrapped)) / 1.3,
               744 / max(1, max((len(line) for line in wrapped), default=1)) / 0.65)
    parts = ['<svg xmlns="http://www.w3.org/2000/svg" width="800" height="500" viewBox="0 0 800 500">',
             f'<title>{html.escape(title + ": " + subtitle)}</title>',
             '<rect width="800" height="500" rx="16" fill="#0d1520"/>',
             '<path d="M24 94H776" stroke="#2b3b4f"/>']
    def text(x, y, value, font_size, color, family="monospace"):
        parts.append(f'<text x="{x}" y="{y}" font-family="{family}" font-size="{font_size}" '
                     f'fill="{color}" xml:space="preserve">{html.escape(value)}</text>')
    text(28, 42, title, 30, "#f1f5f9", "sans-serif")
    text(28, 74, subtitle, 18, "#93c5fd", "sans-serif")
    for i, line in enumerate(wrapped):
        text(28, 128 + i * size * 1.3, line, size, "#d8e4ef")
    text(28, 470, footer, 16, "#95a7bc", "sans-serif")
    return "\n".join([*parts, "</svg>", ""])


def render():
    records = read_json(OUT / "captures.json")
    for name, (_, _, title, subtitle) in APPS.items():
        stdout = (OUT / f"{name}.stdout.txt").read_text()
        expected = "".join(c["stdout"] for c in records["programs"][name]["commands"])
        if stdout != expected:
            raise ValueError(f"capture text changed: {name}")
        lines = stdout.splitlines()
        footer = "Captured stdout / ARM64 Linux / exit 0"
        if name == "cjson":
            lines = lines[:11]
            footer = "Captured stdout excerpt / reference-built example driver"
        if name == "zstd":
            measured = records["programs"][name]["roundTrip"]
            lines += ["", "Measured by capture runner:", f"Input:       {measured['bytes']:,} bytes",
                      f"Compressed:  {measured['compressedBytes']:,} bytes", "", "Decompressed bytes match original."]
            footer = "Captured version + measured round trip / exit 0"
        (OUT / f"{name}.svg").write_text(panel(title, subtitle, lines, footer))
    receipt = read_json(ROOT / "tests/contracts/kernel-bpf-full-receipt.json")
    boot = receipt["boot"]
    path = Path(boot["serialLog"])
    if digest(path) != boot["serialLogSha256"] or boot["verdict"] != "PASS":
        raise ValueError("boot log does not match its accepted receipt")
    lines = path.read_text().splitlines()[-7:]
    (OUT / "linux.stdout.txt").write_text("\n".join(lines) + "\n")
    (OUT / "linux.svg").write_text(panel("Linux 6.18.44", "Recorded boot / 7,008 EvoCC target-C objects", lines,
                                         "Serial-log excerpt / boot + poweroff, not full selftests"))
    records["linux"] = {"buildReceipt": "tests/contracts/kernel-bpf-full-receipt.json",
                        "imageSha256": boot["imageSha256"], "serialLogSha256": digest(path),
                        "flownAt": boot["flownAt"], "excerpt": "last 7 lines; CRLF normalized"}
    source = ROOT / ".build/ladder/doom/evocc-demo1-0/frame-350.png"
    doom_receipt = read_json(ROOT / "tests/contracts/ladder-doom-receipt.json")
    runtime = ROOT / ".build/ladder/doom/runtime.json"
    if doom_receipt["status"] != "PASS" or digest(runtime) != doom_receipt["artifacts"]["runtime.json"]:
        raise ValueError("Doom runtime no longer matches its passing receipt")
    runs = [r for r in read_json(runtime) if (r["compiler"], r["demo"], r["repeat"]) == ("evocc", "demo1", 0)]
    ppm = source.with_suffix(".ppm")
    header, rgb = ppm.read_bytes().split(b"\n255\n", 1)
    if header != b"P6\n320 200" or len(rgb) != 320 * 200 * 3:
        raise ValueError("unexpected captured framebuffer format")
    bgra = bytes(channel for i in range(0, len(rgb), 3) for channel in (rgb[i + 2], rgb[i + 1], rgb[i], 0))
    if len(runs) != 1 or hashlib.sha256(bgra).hexdigest() != runs[0]["frames"][349]:
        raise ValueError("Doom snapshot differs from the recorded frame hash")
    # Pixel comparison only: no generated/repainted game imagery.
    subprocess.run(["magick", "compare", "-metric", "AE", str(ppm), str(source), "null:"],
                   check=True, capture_output=True)
    shutil.copyfile(source, OUT / "doom.png")
    records["doom"] = {"buildReceipt": "tests/contracts/ladder-doom-receipt.json",
                       "source": str(source.relative_to(ROOT)), "imageSha256": digest(source),
                       "buildRun": doom_receipt["run"], "frameBgraSha256": runs[0]["frames"][349],
                       "demo": "demo1", "frame": 350, "repeat": 0, "compiler": "evocc"}
    records["assets"] = {p.name: digest(p) for p in sorted(OUT.iterdir()) if p.suffix in (".svg", ".png", ".txt")}
    (OUT / "captures.json").write_text(json.dumps(records, indent=2) + "\n")


def check():
    records = read_json(OUT / "captures.json")
    required = {"doom.png", "linux.svg", *[f"{name}.svg" for name in APPS]}
    if not required <= set(records["assets"]):
        raise ValueError("missing showcase assets")
    for name, expected in records["assets"].items():
        if digest(OUT / name) != expected:
            raise ValueError(f"asset changed since rendering: {name}")
    for name in APPS:
        for stream in ("stdout", "stderr"):
            expected = "".join(c[stream] for c in records["programs"][name]["commands"])
            if (OUT / f"{name}.{stream}.txt").read_text() != expected:
                raise ValueError(f"capture no longer matches command output: {name}/{stream}")
    print(f"Showcase: {len(required)} assets and six captured stdout/stderr pairs verified")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture", action="store_true")
    parser.add_argument("--render", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if args.capture:
        capture()
    if args.render:
        render()
    if args.check:
        check()
    if not (args.capture or args.render or args.check):
        parser.error("choose --capture (sealed ARM64 container), --render (host), or --check")
