#!/usr/bin/env python3
"""Bounded game showcase builds. External SDL/OpenGL are reference libraries.

Reuse the ladder's command projection; never substitute a refused C object.
Source archives and game data are pinned independently. This is a demo lane,
not a claim of complete upstream game tests or graphics-stack compilation.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import dataclasses
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import tarfile
import time
import zipfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("game_subject", Path(__file__).with_name("subject.py"))
subject = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = subject
spec.loader.exec_module(subject)
PINS = {
    "opentyrian": ("5a9d8daa2811347bec3a0c42f8da87682378f3cb", "e55529f4f0eac1d8c6f2aa78df10a3e49de7ff2f32f8db6bb61e5a5efd3c2a04"),
    "neverball": ("d9df382786d7e5635a1457b6b465ab943cf4fee6", "e69a1ea9553704a6e3c04f15447e151f273d1f540292d876c578e9b22c15879a"),
    "quakespasm": ("f5fe17864918239d443fe4c0d6bfb980e44d19e6", "1ca8ad2f190f9ab0aecd06d854c92637b62a3bd3f298dd877799dee0a300abd4"),
}
DATA = {"tyrian21.zip": "7790d09a2a3addcd33c66ef063d5900eb81cc9c342f4807eb8356364dd1d9277",
        "librequake-lite.zip": "428e736b2f01d953e09a08c60bee975bdc4a0ac2219e97fa095c8af41754da83"}
# Explicit demo configurations, identically applied to the reference and subject.
# This lane is not acceptance of the upstream default warning/tuning options.
OPTIONS = {"opentyrian": ["WITH_NETWORK=false", "CFLAGS=-O2"],
           "neverball": ["ENABLE_FETCH=curl", "ENABLE_NLS=0", "ALL_CFLAGS=-O2 -std=c99", "neverball", "sols"],
           "quakespasm": ["USE_SDL2=1", "USE_CODEC_MP3=0", "USE_CODEC_VORBIS=0",
                          "CFLAGS=-O2 -std=gnu11 -DUSE_SDL2 -DUSE_CODEC_WAVE -DDO_USERDIRS=1 -I/usr/include/SDL2 -D_REENTRANT"]}


def digest(path):
    return subject.sha256_file(Path(path))


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def tree(work, game):
    base = work / f"{game}-{PINS[game][0]}"
    return base / "Quake" if game == "quakespasm" else base


def prepare(work):
    for game, (commit, sha) in PINS.items():
        path = work / "download" / (game + ".tar.gz")
        if digest(path) != sha:
            raise ValueError(f"source archive differs: {game}")
        if not tree(work, game).exists():
            with tarfile.open(path) as archive:
                archive.extractall(work, filter="data")
    # One disclosed application correctness patch, shared by both compilers.
    header = tree(work, "neverball") / "share/mtrl.h"
    before = "#define tobyte(f) ((GLubyte) (f * 255.0f))"
    after = "#define tobyte(f) ((GLubyte) ((f) * 255.0f))"
    contents = header.read_text()
    if before in contents:
        header.write_text(contents.replace(before, after))
    elif after not in contents:
        raise ValueError("Neverball material patch no longer applies")
    for filename, sha in DATA.items():
        path = work / "download" / filename
        if digest(path) != sha:
            raise ValueError(f"data archive differs: {filename}")
        target = work / "data" / path.stem
        if not target.exists():
            with zipfile.ZipFile(path) as archive:
                for name in archive.namelist():
                    if Path(name).is_absolute() or ".." in Path(name).parts:
                        raise ValueError("unsafe data archive")
                archive.extractall(target)


def reference(work, game):
    directory = tree(work, game)
    log = work / f"{game}-reference.log"
    argv = ["make", "-j2", "CC=gcc-14", *OPTIONS[game]]
    with log.open("wb") as output:
        done = subprocess.run(argv, cwd=directory, stdout=output, stderr=subprocess.STDOUT, timeout=600)
    if done.returncode:
        raise RuntimeError(f"reference build failed: {game}; {log}")
    units = subject.parse_build_log(log.read_text(), str(directory))
    links = []
    for line in log.read_text().splitlines():
        if line.startswith("gcc-14 "):
            args = shlex.split(line)
            if "-o" in args and "-c" not in args:
                links.append(args)
    if not units or not links:
        raise ValueError("missing original compile/link inventory")
    write_json(work / f"{game}-build.json", {"command": argv, "directory": str(directory),
               "units": [dataclasses.asdict(u) for u in units], "links": links,
               "sources": {u.source: digest(directory / u.source) for u in units}})
    print(f"{game}: reference built; {len(units)} C units", flush=True)


def emit(work, game, evocc):
    build = json.loads((work / f"{game}-build.json").read_text())
    directory = tree(work, game)
    sysroot = work / "sysroot"
    inc = subject.e0.include_flags(sysroot)
    profile = subject.Subject(name="games", tree=str(directory.relative_to(work)), rung="demo", pin=PINS[game][0],
                              url="", sha256=PINS[game][1], build="", test="", relink="")
    output = work / "evocc" / game
    output.mkdir(parents=True, exist_ok=True)
    receipt = {"game": game, "status": "RUNNING", "compilerSha256": digest(evocc), "sourcePin": PINS[game],
               "scope": "C game/engine objects; external platform libraries", "units": []}
    result_path = work / f"{game}-emit.json"
    write_json(result_path, receipt)
    def one(raw):
        unit = subject.Unit(**raw)
        argv, omitted = subject.evocc_argv(unit, evocc, profile, directory, inc)
        argv = [arg.replace("-I/usr/include", "-I" + str(sysroot / "usr/include")) for arg in argv]
        target = output / (unit.obj + ".s")
        target.parent.mkdir(parents=True, exist_ok=True)
        start = time.monotonic()
        done = subprocess.run(argv, cwd=directory / unit.directory, capture_output=True, timeout=180)
        target.with_suffix(".log").write_bytes(done.stderr)
        row = {"object": unit.obj, "source": unit.source, "argv": argv, "omitted": omitted,
               "exitCode": done.returncode, "seconds": round(time.monotonic() - start, 3)}
        if done.returncode == 0:
            target.write_bytes(done.stdout)
            row["assemblySha256"] = digest(target)
        else:
            row["diagnostics"] = subject.diagnostics(done.stderr.decode(errors="replace"))
        print(f"{game} {unit.source}: {done.returncode}", flush=True)
        return row
    with ThreadPoolExecutor(max_workers=2) as pool:
        for row in pool.map(one, build["units"]):
            receipt["units"].append(row)
            write_json(result_path, receipt)
    if digest(evocc) != receipt["compilerSha256"]:
        raise ValueError("compiler changed during game build")
    receipt["status"] = "COMPILED" if all(r["exitCode"] == 0 for r in receipt["units"]) else "REFUSED"
    write_json(result_path, receipt)
    return receipt["status"] == "COMPILED"


def link(work, game):
    build = json.loads((work / f"{game}-build.json").read_text())
    receipt = json.loads((work / f"{game}-emit.json").read_text())
    if receipt["status"] != "COMPILED" or len(receipt["units"]) != len(build["units"]):
        raise ValueError("refused or incomplete C population; no fallback")
    directory = tree(work, game)
    output = work / "evocc" / game
    objects = {}
    for row in receipt["units"]:
        assembly = output / (row["object"] + ".s")
        obj = output / row["object"]
        if digest(assembly) != row["assemblySha256"]:
            raise ValueError("assembly changed")
        subprocess.run(["gcc-14", "-c", str(assembly), "-o", str(obj)], check=True, capture_output=True, timeout=120)
        objects[row["object"]] = digest(obj)
    results = []
    for original in build["links"]:
        argv = list(original)
        for index, arg in enumerate(argv):
            if arg.endswith(".o"):
                if arg not in objects:
                    raise ValueError(f"unaccounted link object: {arg}")
                argv[index] = str(output / arg)
        name = argv[argv.index("-o") + 1]
        binary = output / Path(name).name
        argv[argv.index("-o") + 1] = str(binary)
        done = subprocess.run(argv, cwd=directory, capture_output=True, timeout=120)
        (output / (binary.name + ".link.log")).write_bytes(done.stdout + done.stderr)
        if done.returncode:
            raise RuntimeError(f"link failed: {game} {binary.name}")
        results.append({"binary": str(binary), "sha256": digest(binary), "argv": argv, "exitCode": 0})
    write_json(work / f"{game}-native.json", {"objects": objects, "links": results})
    print(f"{game}: {len(objects)} EvoCC C objects linked; zero fallback", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=["prepare", "reference", "emit", "link"])
    parser.add_argument("game", choices=list(PINS), nargs="?")
    parser.add_argument("--work", type=Path, default=ROOT / ".build/ladder/games")
    parser.add_argument("--evocc", type=Path, default=ROOT / "target/release/evocc")
    args = parser.parse_args()
    work = args.work.resolve()
    if args.phase == "prepare":
        prepare(work)
    elif args.phase == "emit":
        return 0 if emit(work, args.game, args.evocc.resolve()) else 1
    else:
        {"reference": reference, "link": link}[args.phase](work, args.game)
    return 0


if __name__ == "__main__":
    sys.exit(main())
