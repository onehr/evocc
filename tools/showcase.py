#!/usr/bin/env python3
"""Small README captures, not a benchmark campaign or new acceptance receipt.

--capture executes existing ARM64 builds and small explicitly labeled adapters.
--render visualizes their output, never fabricating application screenshots.
Linux and Doom reuse explicitly identified, already recorded output.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs/images/showcase"
FIXTURES = ROOT / "tools/showcase"
APPS = {
    "tinycc": ("tinycc", "tcc"),
    "sqlite": ("sqlite/sqlite-src-3530400", "testfixture"),
    "lua": ("lua/lua-5.5.1", "src/lua"),
    "jq": ("jq/jq-1.8.2", "jq"),
    "zstd": ("zstd/zstd-1.5.7", "programs/zstd"),
    "cjson": ("cjson/cJSON-1.7.19", "cJSON_test"),
    "zlib": ("zlib/zlib-1.3.2", "minigzip"),
    "toybox": ("toybox-work", "toybox"),
    "mbedtls": ("mbedtls/mbedtls-4.2.0", "build/programs/test/selftest"),
    "polybench": ("polybench", "native/20260919T222638Z/evocc/fdtd-2d.exe"),
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
                            capture_output=True, timeout=30)
    if result.returncode:
        raise RuntimeError(f"demo exited {result.returncode}: {argv}\n{result.stderr.decode('utf-8')}")
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
    elif name == "toybox":
        if digest(binary) != receipt["substitutedTest"]["binarySha256"]:
            raise ValueError("Toybox executable differs from the tested build")
        expected = {}
    elif name == "polybench":
        rows = [json.loads(line) for line in (ROOT / receipt["evidence"] / "results.jsonl").read_text().splitlines()]
        expected = {"evocc-objects/" + r["unit"]: r["objectSha256"] for r in rows if r["unit"] == "obj/fdtd-2d.o"}
        if len(expected) != 1:
            raise ValueError("missing FDTD object evidence")
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
            "binaryMatchesOriginalReceipt": name in ("tinycc", "toybox"),
            "verifiedInstalledObjects": len(expected), "binary": str(binary.relative_to(ROOT)),
            "binarySha256": digest(binary), "objectHashes": list(expected.values())}


def checked_archive(library, allowed_hashes):
    names = subprocess.check_output(["ar", "t", str(library)], timeout=30).decode().splitlines()
    if not names or len(names) != len(set(names)):
        raise ValueError("empty or ambiguous library archive")
    for name in names:
        data = subprocess.check_output(["ar", "p", str(library), name], timeout=30)
        if hashlib.sha256(data).hexdigest() not in allowed_hashes:
            raise ValueError(f"archive member is not an audited EvoCC object: {name}")
    return {"sha256": digest(library), "verifiedEvoccMembers": len(names)}


def capture():
    OUT.mkdir(parents=True, exist_ok=True)
    records = {"capturedAt": datetime.now(timezone.utc).isoformat(),
               "runtime": platform.platform(), "kind": "small showcase executions, not full tests",
               "fixtures": {p.name: digest(p) for p in sorted(FIXTURES.iterdir()) if p.is_file()},
               "programs": {}}
    with tempfile.TemporaryDirectory(prefix="evocc-showcase-") as tmp:
        work = Path(tmp)
        for name, (relative, exe) in APPS.items():
            tree = ROOT / ".build/ladder" / relative
            binary = tree / exe
            record = audit(name, tree, binary)
            commands = []
            if name == "tinycc":
                commands.append(run([binary, "-B" + str(tree), "-I" + str(tree / "include"),
                                     FIXTURES / "raytrace.c", "-lm", "-o", work / "raytrace"], cwd=work))
                commands.append(run([work / "raytrace"], cwd=work))
                record["generatedExecutableSha256"] = digest(work / "raytrace")
            elif name == "sqlite":
                commands.append(run([binary, FIXTURES / "island.tcl"], cwd=work))
            elif name == "lua":
                commands.append(run([binary, FIXTURES / "fractal.lua"], cwd=work))
            elif name == "jq":
                commands.append(run([binary, "-n", "-M", "-f", FIXTURES / "telemetry.jq"], cwd=work))
                result = json.loads(commands[-1]["stdout"])
                if len(result["samples"]) != 48 or result["distance"] != 47 * 47 * 0.018:
                    raise ValueError("jq result mismatch")
            elif name in ("zstd", "zlib"):
                # Compress the real Lua-generated image, not a repeated string.
                fractal = json.loads(records["programs"]["lua"]["commands"][-1]["stdout"])
                original = bytes.fromhex(fractal["rgb"])
                (work / "pixels.rgb").write_bytes(original)
                if name == "zstd":
                    commands.append(run([binary, "-f", work / "pixels.rgb", "-o", work / "pixels.zst"], cwd=work))
                    commands.append(run([binary, "-d", "-f", work / "pixels.zst", "-o", work / "restored.rgb"], cwd=work))
                    compressed = work / "pixels.zst"
                else:
                    (work / "zlib.rgb").write_bytes(original)
                    commands.append(run([binary, work / "zlib.rgb"], cwd=work))
                    compressed = work / "zlib.rgb.gz"
                    shutil.copyfile(compressed, work / "restore.rgb.gz")
                    commands.append(run([binary, "-d", work / "restore.rgb.gz"], cwd=work))
                    shutil.copyfile(work / "restore.rgb", work / "restored.rgb")
                if (work / "restored.rgb").read_bytes() != original:
                    raise ValueError(f"{name} round-trip mismatch")
                record["roundTrip"] = {"bytes": len(original), "compressedBytes": compressed.stat().st_size,
                                       "sha256": digest(work / "restored.rgb"), "equal": True,
                                       "source": "Lua-generated RGB pixels",
                                       "width": fractal["width"], "height": fractal["height"]}
            elif name == "cjson":
                commands.append(run(["gcc-14", "-no-pie", "-I" + str(tree), FIXTURES / "scene.c",
                                     tree / "cJSON.o", "-lm", "-o", work / "scene"], cwd=work))
                commands.append(run([work / "scene"], cwd=work))
                record["adapter"] = "GCC14-built scene adapter linked to audited EvoCC cJSON.o"
                record["generatedExecutableSha256"] = digest(work / "scene")
            elif name == "toybox":
                bundle = work / "capsule"
                (bundle / "maps").mkdir(parents=True)
                (bundle / "scripts").mkdir()
                (bundle / "maps/island.json").write_text(records["programs"]["sqlite"]["commands"][-1]["stdout"])
                shutil.copyfile(FIXTURES / "fractal.lua", bundle / "scripts/fractal.lua")
                commands.append(run([binary, "tar", "-cf", "capsule.tar", "capsule"], cwd=work))
                commands.append(run([binary, "tar", "-tf", "capsule.tar"], cwd=work))
                destination = work / "unpacked"
                destination.mkdir()
                commands.append(run([binary, "tar", "-xf", work / "capsule.tar", "-C", destination], cwd=work))
                for p in bundle.rglob("*"):
                    if p.is_file() and digest(p) != digest(destination / p.relative_to(work)):
                        raise ValueError("Toybox extracted bytes differ")
                record["archive"] = {"bytes": (work / "capsule.tar").stat().st_size, "equal": True,
                                     "files": [str(p.relative_to(work)) for p in sorted(bundle.rglob("*")) if p.is_file()]}
            elif name == "mbedtls":
                library = tree / "build/tf-psa-crypto/core/libtfpsacrypto.a"
                record["archive"] = checked_archive(library, set(record["objectHashes"]))
                commands.append(run(["gcc-14", "-no-pie", "-I" + str(tree / "tf-psa-crypto/include"),
                                     "-I" + str(tree / "tf-psa-crypto/drivers/builtin/include"),
                                     "-I" + str(tree / "build/tf-psa-crypto/include"),
                                     "-I" + str(tree / "include"), FIXTURES / "seal.c", library,
                                     "-pthread", "-o", work / "seal"], cwd=work))
                commands.append(run([work / "seal"], cwd=work))
                record["adapter"] = "GCC14-built local AES-GCM adapter; EvoCC-built crypto archive"
                record["generatedExecutableSha256"] = digest(work / "seal")
            elif name == "polybench":
                commands.append(run([binary], cwd=work))
                reference = tree / "native/20260919T222638Z/gcc/fdtd-2d.stderr"
                if commands[-1]["stderr"] != reference.read_text():
                    raise ValueError("FDTD output differs from retained GCC control")
                record["referenceDumpSha256"] = digest(reference)
            else:
                commands.append(run([binary], cwd=tree))
            if digest(binary) != record["binarySha256"]:
                raise ValueError(f"binary changed while running: {name}")
            record["commands"] = commands
            record.pop("objectHashes")
            records["programs"][name] = record
            print(f"{name}: capture exit 0; {record['verifiedInstalledObjects']} installed objects verified", flush=True)
        # Publish only after every case succeeds, preserving complete raw output.
        for name, record in records["programs"].items():
            for stream in ("stdout", "stderr"):
                (OUT / f"{name}.{stream}.txt").write_text("".join(c[stream] for c in record["commands"]))
    (OUT / "captures.json").write_text(json.dumps(records, indent=2) + "\n")


def render():
    spec = importlib.util.spec_from_file_location("showcase_visuals", ROOT / "tools/showcase_visuals.py")
    visuals = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(visuals)
    records = read_json(OUT / "captures.json")
    for name in APPS:
        stdout = (OUT / f"{name}.stdout.txt").read_text()
        expected = "".join(c["stdout"] for c in records["programs"][name]["commands"])
        if stdout != expected:
            raise ValueError(f"capture text changed: {name}")
        (OUT / f"{name}.svg").write_text(visuals.draw(name, records["programs"][name], records["programs"]))
    receipt = read_json(ROOT / "tests/contracts/kernel-bpf-full-receipt.json")
    boot = receipt["boot"]
    path = Path(boot["serialLog"])
    if digest(path) != boot["serialLogSha256"] or boot["verdict"] != "PASS":
        raise ValueError("boot log does not match its accepted receipt")
    lines = path.read_text().splitlines()[-7:]
    (OUT / "linux.stdout.txt").write_text("\n".join(lines) + "\n")
    (OUT / "linux.svg").write_text(visuals.linux(lines))
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
        if not records["programs"][name]["commands"] or any(c["exitCode"] for c in records["programs"][name]["commands"]):
            raise ValueError(f"unsuccessful demo capture: {name}")
        for stream in ("stdout", "stderr"):
            expected = "".join(c[stream] for c in records["programs"][name]["commands"])
            if (OUT / f"{name}.{stream}.txt").read_text() != expected:
                raise ValueError(f"capture no longer matches command output: {name}/{stream}")
    for name, expected in records["fixtures"].items():
        if digest(FIXTURES / name) != expected:
            raise ValueError(f"demo source differs from capture: {name}")
    print(f"Showcase: {len(required)} assets and {len(APPS)} captured stdout/stderr pairs verified")


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
