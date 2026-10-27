#!/usr/bin/env python3
"""Small README captures, not a benchmark campaign or new acceptance receipt.

--capture executes existing ARM64 builds and small explicitly labeled adapters.
--render visualizes their output, never fabricating application screenshots.
Doom reuses explicitly identified, already recorded output. --render-logs quotes
the recorded test logs of accepted runs verbatim; it needs the local .evidence tree.
"""
from __future__ import annotations

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs/images/showcase"
FIXTURES = ROOT / "tools/showcase"
APPS = {
    "tinycc": ("tinycc", "tcc"),
    "lua": ("lua/lua-5.5.1", "src/lua"),
    "zstd": ("zstd/zstd-1.5.7", "programs/zstd"),
    "zlib": ("zlib/zlib-1.3.2", "minigzip"),
    "mbedtls": ("mbedtls/mbedtls-4.2.0", "build/programs/test/selftest"),
    # Each PolyBench run keeps its executables under its own run ID; see app_binary.
    "polybench": ("polybench", "native/{run}/evocc/fdtd-2d.exe"),
}
# Gallery cards that quote the recorded logs of accepted runs. Each segment is
# (pattern of one line, number of lines) or (pattern, number of lines, lines before
# the matched line); the lines are copied verbatim. The SQLite progress times
# change from run to run, so its second segment ends at the summary line.
LOGS = {
    "sqlite": ("SQLite 3.53.4: testrunner.tcl veryquick", "ladder-sqlite-receipt.json",
               [(r"^splitting work across ", 2), (r"^\d+ errors out of \d+ tests in ", 6, 5)]),
    "jq": ("jq 1.8.2: make check", "ladder-jq-receipt.json", [(r"^PASS: tests/mantest$", 11)]),
    "cjson": ("cJSON 1.7.19: ./cJSON_test", "ladder-cjson-receipt.json", [(r"^Version: 1\.7\.19$", 11)]),
    "linux": ("Linux 6.18.44: serial console", "kernel-direct-virt-receipt.json",
              [(r"\] Linux version ", 1), (r"\] smp: Brought up ", 1),
               (r"\] Run /init as init process$", 2)]),
    "toybox": ("Toybox 0.8.14: make tests", "ladder-toybox-receipt.json", [(r"^PASS: xxd -r long$", 10)]),
}
# The Linux card quotes the EvoCC kernel that Kbuild built with CC=evocc, on the host boot lane.
LINUX_LANE = "subject-boot-hvf"


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text())


def demo_inputs():
    spec = importlib.util.spec_from_file_location("pixel_scenes", FIXTURES / "pixel_scenes.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def visualizer():
    spec = importlib.util.spec_from_file_location("showcase_visuals", ROOT / "tools/showcase_visuals.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def boot_facts(log, nonce):
    """Extract visual facts from the accepted log; missing milestones fail closed."""
    patterns = (
        ("reset", r"\[\s*([0-9.]+)\] Booting Linux on physical CPU"),
        ("cpus", r"\[\s*([0-9.]+)\] smp: Brought up 1 node, (\d+) CPUs"),
        ("memory", r"\[\s*([0-9.]+)\] Memory: (\d+)K/(\d+)K available"),
        ("devices", r"\[\s*([0-9.]+)\] devtmpfs: initialized"),
        ("init", r"\[\s*([0-9.]+)\] Run /init as init process"),
        ("poweroff", r"\[\s*([0-9.]+)\] reboot: Power down"),
    )
    facts, positions = {}, []
    for key, pattern in patterns:
        match = re.search(pattern, log)
        if not match:
            raise ValueError(f"missing boot milestone: {key}")
        facts[key] = {"seconds": float(match[1]), "line": log[match.start():].splitlines()[0]}
        positions.append(match.start())
        if key == "cpus":
            facts["cpuCount"] = int(match[2])
        elif key == "memory":
            facts["availableKiB"], facts["totalKiB"] = int(match[2]), int(match[3])
    # Each marker starts a line, but the console may print the kernel's earlier
    # messages after it, even on the same line. So the markers are ordered among
    # themselves and before power-off, not after "Run /init".
    def marker(text):
        match = re.search(rf"^{re.escape(text)}(?![\w-])", log, re.M)
        return match.start() if match else -1
    alive, bound_nonce, done = (marker("EVO_INIT_ALIVE"), marker(f"EVO_BOOT_NONCE {nonce}"),
                                marker("EVO_INIT_DONE"))
    if positions != sorted(positions) or not 0 <= alive < bound_nonce < done < positions[-1]:
        raise ValueError("boot milestones or nonce are missing/out of order")
    facts["nonce"] = nonce
    return facts


def app_binary(name):
    """Return the build tree and the program of a lane, as its current receipt names them."""
    relative, exe = APPS[name]
    if "{run}" in exe:
        exe = exe.format(run=read_json(ROOT / f"tests/contracts/ladder-{name}-receipt.json")["run"])
    tree = ROOT / ".build/ladder" / relative
    return tree, tree / exe


def doom_png(ppm):
    """The Doom lane keeps PPM frames. Encode the gallery PNG of one frame losslessly, once."""
    png = ppm.with_suffix(".png")
    if not png.exists():
        header, rgb = ppm.read_bytes().split(b"\n255\n", 1)
        width, height = map(int, header.split(b"\n")[1].split())
        png.write_bytes(base64.b64decode(visualizer().png_uri(width, height, rgb).split(",", 1)[1]))
    return png


def run(argv, *, cwd, input_data=None, expected_exit=0, stage=None):
    result = subprocess.run([str(a) for a in argv], cwd=cwd, input=input_data,
                            capture_output=True, timeout=30)
    if result.returncode != expected_exit:
        raise RuntimeError(f"demo exited {result.returncode}: {argv}\n{result.stderr.decode('utf-8')}")
    record = {"argv": [str(a) for a in argv], "exitCode": result.returncode,
              "stdout": result.stdout.decode("utf-8"), "stderr": result.stderr.decode("utf-8")}
    if stage:
        record["stage"] = stage
    if expected_exit:
        record["expectedExitCode"] = expected_exit
    return record


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
            "binaryMatchesOriginalReceipt": name == "tinycc",
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
        for name in APPS:
            tree, binary = app_binary(name)
            record = audit(name, tree, binary)
            commands = []
            if name == "tinycc":
                commands.append(run([binary, "-B" + str(tree), "-I" + str(tree / "include"),
                                     FIXTURES / "raytrace.c", "-lm", "-o", work / "raytrace"], cwd=work))
                commands.append(run([work / "raytrace"], cwd=work))
                record["generatedExecutableSha256"] = digest(work / "raytrace")
            elif name == "lua":
                commands.append(run([binary, FIXTURES / "fractal.lua"], cwd=work))
            elif name in ("zstd", "zlib"):
                picture = (json.loads(records["programs"]["lua"]["commands"][-1]["stdout"])
                           if name == "zstd" else demo_inputs().sprite_sheet())
                original = bytes.fromhex(picture["rgb"])
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
                                       "source": ("Lua-generated RGB pixels" if name == "zstd"
                                                  else "Repository-owned pixel-art sprite sheet fixture"),
                                       "width": picture["width"], "height": picture["height"]}
                if name == "zlib":
                    record["picture"] = picture
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
                reference = binary.parent.parent / "gcc/fdtd-2d.stderr"
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
            print(f"{name}: capture verified; {record['verifiedInstalledObjects']} installed objects verified", flush=True)
        # Publish only after every case succeeds, preserving complete raw output.
        for name, record in records["programs"].items():
            for stream in ("stdout", "stderr"):
                (OUT / f"{name}.{stream}.txt").write_text("".join(c[stream] for c in record["commands"]))
    (OUT / "captures.json").write_text(json.dumps(records, indent=2) + "\n")


def render():
    visuals = visualizer()
    records = read_json(OUT / "captures.json")
    for name in APPS:
        stdout = (OUT / f"{name}.stdout.txt").read_text()
        expected = "".join(c["stdout"] for c in records["programs"][name]["commands"])
        if stdout != expected:
            raise ValueError(f"capture text changed: {name}")
        (OUT / f"{name}.svg").write_text(visuals.draw(name, records["programs"][name], records["programs"]))
    render_logs(records)
    source = doom_png(ROOT / ".build/ladder/doom/evocc-demo1-0/frame-350.ppm")
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
    write_records(records)


def relative(path):
    path = Path(path).resolve()
    return str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path)


def source_log(name, receipt):
    """Return an accepted run's recorded log and the hash its receipt pins, if any."""
    if name == "linux":
        lane, counts = receipt["boots"][LINUX_LANE], receipt["counts"]
        boot = read_json(ROOT / lane["receipt"])
        if (receipt["mode"] != "direct-kbuild" or lane["verdict"] != "PASS" or boot["verdict"] != "PASS"
                or counts["evoccCObjects"] <= 0 or counts["gccTargetCSourceRecords"]
                or counts["seededTargetObjects"] or counts["missingRecordedOutputs"]
                or boot["imageSha256"] != receipt["imageSha256"]
                or boot["serialLogSha256"] != lane["serialLogSha256"]):
            raise ValueError("Linux direct build, object provenance or boot did not pass")
        return ROOT / boot["serialLog"], lane["serialLogSha256"]
    evidence = ROOT / receipt["evidence"]
    if name == "toybox":
        test = receipt["substitutedTest"]
        if receipt["testResult"] != "no-regression" or test["outcomes"] != receipt["baselineTest"]["outcomes"]:
            raise ValueError("Toybox results differ from the GCC build")
        return evidence / "substituted-tests.log", test["logSha256"]
    if not receipt["test"]["passed"]:
        raise ValueError(f"accepted tests did not pass: {name}")
    return evidence / f"test-{receipt['test']['completedTrial']}.log", None


def excerpt(text, segments):
    """Copy whole lines. Only trailing spaces are removed."""
    lines = [line.rstrip() for line in text.splitlines()]
    out = []
    for pattern, count, *before in segments:
        before = before[0] if before else 0
        matches = [i for i, line in enumerate(lines) if re.search(pattern, line)]
        start = matches[0] - before if len(matches) == 1 else -1
        if start < 0 or before >= count or start + count > len(lines):
            raise ValueError(f"log excerpt is missing or ambiguous: {pattern}")
        out.append({"line": start + 1, "text": lines[start:start + count]})
    return out


def render_logs(records):
    visuals = visualizer()
    records["logs"] = {}
    for name, (title, receipt_name, segments) in LOGS.items():
        receipt = read_json(ROOT / "tests/contracts" / receipt_name)
        path, pinned = source_log(name, receipt)
        if pinned and digest(path) != pinned:
            raise ValueError(f"recorded log differs from its receipt: {name}")
        text = path.read_text()
        record = {"title": title, "receipt": f"tests/contracts/{receipt_name}", "source": relative(path),
                  "sourceSha256": digest(path), "segments": excerpt(text, segments)}
        if name == "linux":
            record["facts"] = boot_facts(text.replace("\r", ""), receipt["boots"][LINUX_LANE]["nonce"])
        records["logs"][name] = record
        (OUT / f"{name}.svg").write_text(visuals.log_card(name, record))


def write_records(records):
    records["assets"] = {p.name: digest(p) for p in sorted(OUT.iterdir()) if p.suffix in (".svg", ".png", ".txt")}
    (OUT / "captures.json").write_text(json.dumps(records, indent=2) + "\n")


def check():
    game_spec = importlib.util.spec_from_file_location("showcase_games", ROOT / "tools/showcase_games.py")
    games = importlib.util.module_from_spec(game_spec)
    game_spec.loader.exec_module(games)
    games.check()
    records = read_json(OUT / "captures.json")
    required = {"doom.png", *[f"{name}.svg" for name in APPS], *[f"{name}.svg" for name in LOGS]}
    if not required <= set(records["assets"]):
        raise ValueError("missing showcase assets")
    for name, expected in records["assets"].items():
        if digest(OUT / name) != expected:
            raise ValueError(f"asset changed since rendering: {name}")
    for name in APPS:
        commands = records["programs"][name]["commands"]
        if not commands or any(c["exitCode"] != c.get("expectedExitCode", 0) for c in commands):
            raise ValueError(f"unsuccessful demo capture: {name}")
        if any(c.get("expectedExitCode", 0) for c in commands):
            raise ValueError(f"unexpected negative test: {name}")
        for stream in ("stdout", "stderr"):
            expected = "".join(c[stream] for c in records["programs"][name]["commands"])
            if (OUT / f"{name}.{stream}.txt").read_text() != expected:
                raise ValueError(f"capture no longer matches command output: {name}/{stream}")
    for name, expected in records["fixtures"].items():
        if digest(FIXTURES / name) != expected:
            raise ValueError(f"demo source differs from capture: {name}")
    for name, (title, receipt_name, segments) in LOGS.items():
        record = records["logs"][name]
        if (record["title"], record["receipt"]) != (title, f"tests/contracts/{receipt_name}"):
            raise ValueError(f"log card source changed: {name}")
        if [len(s["text"]) for s in record["segments"]] != [count for _, count, *_ in segments]:
            raise ValueError(f"log excerpt changed: {name}")
        if visualizer().log_card(name, record) != (OUT / f"{name}.svg").read_text():
            raise ValueError(f"log card differs from its recorded excerpt: {name}")
        # The evidence tree is local. Where it exists, the excerpt must still match it.
        source = ROOT / record["source"]
        if source.exists():
            if digest(source) != record["sourceSha256"]:
                raise ValueError(f"recorded log changed: {name}")
            if excerpt(source.read_text(), segments) != record["segments"]:
                raise ValueError(f"log excerpt differs from the recorded log: {name}")
    print(f"Showcase: {len(required) + len(games.NAMES)} images, {len(APPS)} output demos, "
          f"{len(LOGS)} log excerpts and {len(games.NAMES)} paired game captures verified")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture", action="store_true")
    parser.add_argument("--render", action="store_true")
    parser.add_argument("--render-logs", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if args.capture:
        capture()
    if args.render:
        render()
    if args.render_logs:
        records = read_json(OUT / "captures.json")
        render_logs(records)
        write_records(records)
    if args.check:
        check()
    if not (args.capture or args.render or args.render_logs or args.check):
        parser.error("choose --capture (sealed ARM64 container), --render (host), --render-logs (host), or --check")
