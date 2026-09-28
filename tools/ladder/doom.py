#!/usr/bin/env python3
"""Pinned headless Doom integration: complete EvoCC game objects, no fallback.

Uses the existing sealed ARM64 lab and exported headers. Native runs stream
frames to SHA-256 instead of retaining hundreds of MB of raw video. This is
bounded demo/render correctness, not interactive graphics/audio acceptance.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import threading
import time
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location("doom_subject", Path(__file__).with_name("subject.py"))
subject = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = subject
_spec.loader.exec_module(subject)
COMMIT = "dcb7a8dbc7a16ce3dda29382ac9aae9d77d21284"
SOURCE_SHA = "1bd3f7f26220494159a38d71f2847ec81b58d6bbd7c7c8d81b08993018001148"
DATA_SHA = "3f9b264f3e3ce503b4fb7f6bdcb1f419d93c7b546f4df3e874dd878db9688f59"
WAD_SHA = "7323bcc168c5a45ff10749b339960e98314740a734c30d4b9f3337001f9e703d"
IMAGE_SHA = "2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2"
FLAGS = ["-std=gnu11", "-O2", "-fwrapv", "-DNORMALUNIX", "-DLINUX", "-DSNDSERV",
         "-D_DEFAULT_SOURCE", "-DDOOMGENERIC_RESX=320", "-DDOOMGENERIC_RESY=200"]
FRAME_BYTES = 320 * 200 * 4
FRAME_COUNT = 350


def digest(path):
    return subject.sha256_file(Path(path))


def fetch(path, url, expected):
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        with urllib.request.urlopen(url, timeout=120) as inp, path.open("xb") as out:
            while block := inp.read(1024 * 1024):
                out.write(block)
    if digest(path) != expected:
        raise ValueError(f"archive identity mismatch: {path}")


def sources(tree):
    text = (tree / "Makefile").read_text()
    match = re.search(r"^SRC_DOOM = (.+)$", text, re.M)
    if not match:
        raise ValueError("missing upstream object inventory")
    objects = match.group(1).split()
    if objects.count("doomgeneric_xlib.o") != 1 or len(objects) != len(set(objects)):
        raise ValueError("unexpected upstream platform/object inventory")
    return [name[:-2] + ".c" for name in objects if name != "doomgeneric_xlib.o"]


def initialize_video(tree):
    """Explicit shared port repair: the initial melt must not read old heap bytes."""
    patch = ROOT / "tests/fixtures/doom-video-init.patch"
    relative = f"doomgeneric-{COMMIT}/doomgeneric/i_video.c"
    with tarfile.open(tree.parents[1] / "download/doomgeneric.tar.gz") as archive:
        original = archive.extractfile(relative).read()
    line = b"    memset(I_VideoBuffer, 0, SCREENWIDTH * SCREENHEIGHT);\n"
    anchor = b"\tI_VideoBuffer = (byte*)Z_Malloc (SCREENWIDTH * SCREENHEIGHT, PU_STATIC, NULL);  // For DOOM to draw on\n"
    if original.count(anchor) != 1:
        raise ValueError("video initialization patch no longer matches pinned source")
    patched = original.replace(anchor, anchor + line)
    current = (tree / "i_video.c").read_bytes()
    if current == original:
        subprocess.run(["patch", "-p1", "-i", str(patch)], cwd=tree.parent, check=True, capture_output=True)
    if (tree / "i_video.c").read_bytes() != patched:
        raise ValueError("unexpected video source change")
    return {"path": "doomgeneric/i_video.c", "originalSha256": hashlib.sha256(original).hexdigest(),
            "patchedSha256": hashlib.sha256(patched).hexdigest(), "patchSha256": digest(patch),
            "reason": "initial melt reads uninitialized prior screen; GCC self-repeats also differ"}


def verify_tree(work, tree):
    """Archive pin also constrains the unpacked inputs, not just the download."""
    files = {}
    with tarfile.open(work / "download/doomgeneric.tar.gz") as archive:
        prefix = f"doomgeneric-{COMMIT}/doomgeneric/"
        for member in archive.getmembers():
            if not member.isfile() or not member.name.startswith(prefix):
                continue
            name = member.name[len(prefix):]
            original = archive.extractfile(member).read()
            actual = (tree / name).read_bytes()
            if name != "i_video.c" and actual != original:
                raise ValueError(f"unpinned source change: {name}")
            files[name] = hashlib.sha256(actual).hexdigest()
    return files


def valid_runtime(records):
    expected = {(cc, demo, repeat) for cc in ("gcc-14", "clang-19", "evocc")
                for demo in ("demo1", "demo2") for repeat in range(2)}
    keys = [(r.get("compiler"), r.get("demo"), r.get("repeat")) for r in records]
    if len(keys) != len(expected) or set(keys) != expected:
        return False
    for row in records:
        frames = row.get("frames", [])
        if (row.get("exitCode") != 0 or row.get("complete") is not True
                or len(frames) != FRAME_COUNT or len(set(frames)) <= 100
                or any(re.fullmatch(r"[0-9a-f]{64}", x) is None for x in frames)):
            return False
    for demo in ("demo1", "demo2"):
        sequences = [r["frames"] for r in records if r["demo"] == demo]
        if any(s != sequences[0] for s in sequences):
            return False
    return True


def native_build(work, tree, names, fixture):
    """Executed inside the sealed image; no C fallback in the subject link."""
    rows = []
    for compiler in ("gcc-14", "clang-19", "evocc"):
        output = work / compiler
        output.mkdir(exist_ok=True)
        for name in [*names, "doom-headless.c"]:
            src = fixture if name == "doom-headless.c" else tree / name
            obj = output / (Path(name).stem + ".o")
            argv = (["gcc-14", "-c", str(work / "asm" / (Path(name).stem + ".s"))]
                    if compiler == "evocc" else [compiler, *FLAGS, "-I" + str(tree), "-c", str(src)])
            argv += ["-o", str(obj)]
            done = subprocess.run(argv, capture_output=True, timeout=120)
            (output / (Path(name).stem + ".log")).write_bytes(done.stdout + done.stderr)
            rows.append({"compiler": compiler, "source": name, "argv": argv, "exitCode": done.returncode})
            if done.returncode:
                print(done.stderr.decode(errors="replace"), flush=True)
                raise RuntimeError(f"object failed: {compiler} {name}")
            rows[-1]["objectSha256"] = digest(obj)
        argv = ["gcc-14", "-no-pie", *[str(output / (Path(n).stem + ".o")) for n in [*names, "doom-headless.c"]],
                "-lm", "-o", str(output / "doom")]
        done = subprocess.run(argv, capture_output=True, timeout=120)
        (output / "link.log").write_bytes(done.stdout + done.stderr)
        rows.append({"compiler": compiler, "argv": argv, "exitCode": done.returncode})
        if done.returncode:
            print(done.stderr.decode(errors="replace"), flush=True)
            raise RuntimeError(f"link failed: {compiler}")
        rows[-1]["binarySha256"] = digest(output / "doom")
        print(f"{compiler}: {len(names) + 1} objects + link PASS", flush=True)
    (work / "native-build.json").write_text(json.dumps(rows, indent=2) + "\n")


def angle_controls(work):
    """Independent minimal source evidence for the explicit wrapping profile."""
    source = ROOT / "tests/fixtures/doom-angle-wrap.c"
    rows = []
    for cc in ("gcc-14", "clang-19"):
        for wrap in (False, True):
            stem = work / f"angle-{cc}-{wrap}"
            # Clang's explicit overflow sanitizer diagnoses wrapping arithmetic
            # even under -fwrapv. Use it only for the default-profile negative;
            # the positive checks the ordinary wrapping program's actual value.
            argv = [cc, "-O2", *(["-fwrapv"] if wrap else
                    ["-fsanitize=signed-integer-overflow", "-fno-sanitize-recover=all"]),
                    "-c", str(source), "-o", str(stem) + ".o"]
            subprocess.run(argv, check=True, capture_output=True, timeout=30)
            # The sealed image lacks Clang's optional UBSan archive. GCC's
            # runtime supplies the same instrumented ABI; no missing run passes.
            subprocess.run(["gcc-14", str(stem) + ".o", "-lubsan", "-o", str(stem)],
                           check=True, capture_output=True, timeout=30)
            done = subprocess.run([str(stem)], capture_output=True, timeout=5)
            valid = ((done.returncode == 0 and not done.stderr) if wrap else
                     (done.returncode != 0 and b"signed integer overflow" in done.stderr))
            rows.append({"compiler": cc, "wrap": wrap, "argv": argv, "exitCode": done.returncode,
                         "stderr": done.stderr.decode(), "expectedOutcome": valid})
    (work / "angle-controls.json").write_text(json.dumps(rows, indent=2) + "\n")
    if not all(r["expectedOutcome"] for r in rows):
        raise RuntimeError("angle wrapping controls failed")


def native_run(work):
    records = []
    before = {str(p.relative_to(work)): digest(p) for cc in ("gcc-14", "clang-19", "evocc")
              for p in (work / cc).iterdir() if p.suffix == ".o" or p.name == "doom"}
    for demo in ("demo1", "demo2"):
        for compiler in ("gcc-14", "clang-19", "evocc"):
            for repeat in range(2):
                label = f"{compiler}-{demo}-{repeat}"
                directory = work / label
                directory.mkdir(exist_ok=True)
                argv = [str(work / compiler / "doom"), "-iwad", str(work / "freedoom-0.13.0/freedoom1.wad"),
                        "-timedemo", demo, "-nosound", "-nomouse", "-config", str(directory / "default.cfg")]
                hashes = []
                start = time.monotonic()
                with (directory / "engine.log").open("wb") as log:
                    process = subprocess.Popen(argv, cwd=directory, stdout=subprocess.PIPE, stderr=log,
                                               env={**os.environ, "HOME": str(directory)})
                    timer = threading.Timer(120, process.kill)
                    timer.start()
                    try:
                        while frame := process.stdout.read(FRAME_BYTES):
                            if len(frame) != FRAME_BYTES:
                                raise RuntimeError(f"partial frame: {label} {len(frame)}")
                            hashes.append(hashlib.sha256(frame).hexdigest())
                            if len(hashes) in (1, 175, 350):
                                # PPM snapshots are a few hundred KB, not a video archive.
                                rgb = bytearray()
                                for i in range(0, len(frame), 4):
                                    rgb.extend((frame[i + 2], frame[i + 1], frame[i]))
                                (directory / f"frame-{len(hashes)}.ppm").write_bytes(b"P6\n320 200\n255\n" + rgb)
                        code = process.wait()
                    finally:
                        timer.cancel()
                        if process.poll() is None:
                            process.kill()
                            process.wait()
                        process.stdout.close()
                valid = (code == 0 and len(hashes) == FRAME_COUNT and len(set(hashes)) > 100
                         and b"EVOCC_DOOM_FRAMES=350" in (directory / "engine.log").read_bytes())
                row = {"compiler": compiler, "demo": demo, "repeat": repeat, "argv": argv, "exitCode": code,
                       "seconds": round(time.monotonic() - start, 3), "frames": hashes,
                       "uniqueFrames": len(set(hashes)), "complete": valid}
                records.append(row)
                (work / "runtime.json").write_text(json.dumps(records, indent=2) + "\n")
                print(label, code, len(hashes), len(set(hashes)), flush=True)
    after = {name: digest(work / name) for name in before}
    (work / "object-audit.json").write_text(json.dumps({"before": before, "after": after}, indent=2) + "\n")
    if before != after or not valid_runtime(records):
        raise RuntimeError("Doom execution/frame comparison failed; see runtime.json")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evocc", type=Path, default=ROOT / "target/pinned-check/release/evocc")
    parser.add_argument("--work", type=Path, default=ROOT / ".build/ladder/doom")
    parser.add_argument("--sysroot", type=Path, default=ROOT / ".build/ladder/sysroot")
    parser.add_argument("--native", action="store_true")
    parser.add_argument("--jobs", type=int, default=2)
    args = parser.parse_args()
    work = args.work.resolve()
    tree = work / f"doomgeneric-{COMMIT}/doomgeneric"
    fixture = ROOT / "tests/fixtures/doom-headless.c"
    if args.native:
        (work / "runtime.json").write_text("[]\n")
        environment = {}
        for name, command in (("kernel", ["uname", "-a"]), ("gcc", ["gcc-14", "--version"]),
                              ("clang", ["clang-19", "--version"]), ("linker", ["ld", "--version"]),
                              ("libc", ["getconf", "GNU_LIBC_VERSION"])):
            environment[name] = subprocess.check_output(command, text=True).strip()
        (work / "native-environment.json").write_text(json.dumps(environment, indent=2) + "\n")
        native_build(work, tree, sources(tree), fixture)
        angle_controls(work)
        native_run(work)
        return 0
    work.mkdir(parents=True, exist_ok=True)
    fetch(work / "download/doomgeneric.tar.gz", f"https://codeload.github.com/ozkl/doomgeneric/tar.gz/{COMMIT}", SOURCE_SHA)
    fetch(work / "download/freedoom-0.13.0.zip", "https://github.com/freedoom/freedoom/releases/download/v0.13.0/freedoom-0.13.0.zip", DATA_SHA)
    if not tree.exists():
        with tarfile.open(work / "download/doomgeneric.tar.gz") as archive:
            archive.extractall(work, filter="data")
    if not (work / "freedoom-0.13.0/freedoom1.wad").exists():
        with zipfile.ZipFile(work / "download/freedoom-0.13.0.zip") as archive:
            archive.extractall(work)
    repair = initialize_video(tree)
    input_files = verify_tree(work, tree)
    if digest(work / "freedoom-0.13.0/freedoom1.wad") != WAD_SHA:
        raise ValueError("unpacked Freedoom data differs from the pinned release member")
    image = json.loads(subprocess.check_output(["container", "image", "inspect", subject.IMAGE]))
    if image[0]["id"] != IMAGE_SHA:
        raise RuntimeError("sealed image identity mismatch")
    names = sources(tree)
    manifest = {name: digest(tree / name) for name in names}
    manifest["doom-headless.c"] = digest(fixture)
    record = {"status": "RUNNING", "commit": COMMIT, "compilerSha256": digest(args.evocc),
              "run": datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ"),
              "evidence": str(work.relative_to(ROOT)), "inputFiles": input_files,
              "sourceArchiveSha256": SOURCE_SHA, "dataArchiveSha256": DATA_SHA,
              "wadSha256": digest(work / "freedoom-0.13.0/freedoom1.wad"),
              "imageSha256": IMAGE_SHA, "sources": manifest, "flags": FLAGS,
              "portRepair": repair,
              "profileAdjustments": ["headless platform replaces Xlib; 320x200; sound disabled",
                                     "explicit gnu11 O2 instead of upstream Os; no debug/Wall request",
                                     "fwrapv required by signed ANG45 multiplication, not guard suppression"],
              "scope": "350 frames each of two bundled demos; two repeats; no audio/window/input acceptance"}
    record["adapterSha256"] = digest(__file__)
    record["driverSha256"] = record["compilerSha256"]
    # Invalidate any prior success before doing work; interrupted/refused runs
    # must never leave a previous receipt looking like this run's PASS.
    receipt_path = ROOT / "tests/contracts/ladder-doom-receipt.json"
    receipt_path.write_text(json.dumps(record, indent=2) + "\n")
    (work / "summary.json").write_text(json.dumps(record, indent=2) + "\n")
    (work / "asm").mkdir(exist_ok=True)
    def emit(name):
        src = fixture if name == "doom-headless.c" else tree / name
        argv = [str(args.evocc.resolve()), f"--target={subject.TARGET}", "--emit=asm", *FLAGS,
                "-I" + str(tree), *subject.e0.include_flags(args.sysroot.resolve()), str(src)]
        start = time.monotonic()
        done = subprocess.run(argv, capture_output=True, timeout=120)
        (work / "asm" / (Path(name).stem + ".log")).write_bytes(done.stderr)
        row = {"source": name, "argv": argv, "exitCode": done.returncode,
               "seconds": round(time.monotonic() - start, 3)}
        if done.returncode == 0:
            out = work / "asm" / (Path(name).stem + ".s")
            out.write_bytes(done.stdout)
            row["assemblySha256"] = digest(out)
        else:
            row["diagnostic"] = done.stderr.decode(errors="replace")
        print(name, done.returncode, flush=True)
        return row
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        rows = list(pool.map(emit, [*names, "doom-headless.c"]))
    record["compilation"] = rows
    record["status"] = "COMPILED" if all(r["exitCode"] == 0 for r in rows) else "FAIL"
    (work / "summary.json").write_text(json.dumps(record, indent=2) + "\n")
    if record["status"] == "FAIL":
        receipt_path.write_text(json.dumps(record, indent=2) + "\n")
        return 1
    argv = ["container", "run", "--rm", "--platform", "linux/arm64", "--network", "none",
            "--cpus", "2", "--memory", "2G", "-v", f"{ROOT}:/repo:ro", "-v", f"{work}:/doom",
            subject.IMAGE, "python3", "/repo/tools/ladder/doom.py", "--native", "--work", "/doom"]
    with (work / "native.log").open("wb") as log:
        done = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT, timeout=2400)
    record["nativeCommand"] = argv
    record["nativeExitCode"] = done.returncode
    records = json.loads((work / "runtime.json").read_text()) if (work / "runtime.json").exists() else []
    stable = (verify_tree(work, tree) == input_files and digest(args.evocc) == record["compilerSha256"]
              and digest(__file__) == record["adapterSha256"]
              and digest(work / "freedoom-0.13.0/freedoom1.wad") == WAD_SHA)
    final_image = json.loads(subprocess.check_output(["container", "image", "inspect", subject.IMAGE]))
    stable &= final_image[0]["id"] == IMAGE_SHA
    record["status"] = "PASS" if done.returncode == 0 and stable and valid_runtime(records) else "FAIL"
    record["inputAndCompilerStable"] = stable
    (work / "summary.json").write_text(json.dumps(record, indent=2) + "\n")
    receipt = {key: value for key, value in record.items() if key not in ("compilation", "inputFiles", "nativeCommand")}
    receipt.update(unitsTotal=len(rows), unitsEvoccCompiled=sum(r["exitCode"] == 0 for r in rows),
                   unitsInPassingBuild=len(rows) if record["status"] == "PASS" else 0,
                   frameCount=FRAME_COUNT, executions=len(records), frameAgreement=valid_runtime(records),
                   testResult=record["status"], measurement={"driverSha256": record["compilerSha256"]},
                   runtime=[{**{k: r[k] for k in ("compiler", "demo", "repeat", "exitCode", "seconds", "uniqueFrames", "complete")},
                             "sequenceSha256": hashlib.sha256("\n".join(r["frames"]).encode()).hexdigest()} for r in records],
                   artifacts={name: digest(work / name) for name in
                              ("summary.json", "native-build.json", "runtime.json", "object-audit.json", "angle-controls.json",
                               "native-environment.json")
                              if (work / name).exists()})
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
    print(record["status"], work / "summary.json", flush=True)
    return 0 if record["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
