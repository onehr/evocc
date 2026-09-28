#!/usr/bin/env python3
"""Publish only real, provenance-checked game framebuffer captures."""
import argparse
import base64
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import struct
import zlib

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / ".build/ladder/games"
OUT = ROOT / "docs/images/showcase"
NAMES = ("opentyrian", "neverball", "quakespasm")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_text())


def validate(game):
    build, emit, native = (game[k] for k in ("build", "emit", "native"))
    expected = {r["obj"] for r in build["units"]}
    emitted = {r["object"] for r in emit["units"]}
    if (not expected or len(expected) != len(build["units"]) or expected != emitted
            or len(emitted) != len(emit["units"]) or expected != set(native["objects"])
            or emit["status"] != "COMPILED" or any(r["exitCode"] for r in emit["units"])):
        raise ValueError("incomplete game C-object provenance")
    for compiler, run in game["runs"].items():
        if (compiler not in ("gcc", "evocc") or run["compiler"] != compiler
                or run["exitCode"] != 0 or set(run["frames"]) != {f"frame-{i}.ppm" for i in (180, 360, 600)}):
            raise ValueError("unsuccessful game capture")
    if set(game["runs"]) != {"gcc", "evocc"}:
        raise ValueError("missing reference or subject game capture")
    links = native["links"]
    if not links or any(r["exitCode"] for r in links):
        raise ValueError("unsuccessful native game link")
    if not any(r["sha256"] == game["runs"]["evocc"]["binarySha256"] for r in links):
        raise ValueError("captured game is not the audited executable")
    if game["runs"]["gcc"]["adapterSha256"] != game["runs"]["evocc"]["adapterSha256"]:
        raise ValueError("capture adapter differs between compilers")
    matching = [f for f, h in game["runs"]["evocc"]["frames"].items()
                if game["runs"]["gcc"]["frames"][f] == h]
    if game["exactMatchingFrames"] != matching:
        raise ValueError("false frame-equivalence claim")


def publish():
    spec = importlib.util.spec_from_file_location("game_visuals", ROOT / "tools/showcase_visuals.py")
    visuals = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(visuals)
    records = {"scope": "bounded native rendering demos, not full game test suites",
               "platform": "AArch64 Linux; reference SDL2/Mesa llvmpipe/libc; Xvfb",
               "baseImageId": "2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2",
               "adapterSourceSha256": sha(ROOT / "tools/showcase/sdl_capture.c"),
               "captureRunnerSha256": sha(ROOT / "tools/ladder/game_capture.py"),
               "dataArchives": {name: sha(WORK / "download" / name)
                                for name in ("tyrian21.zip", "librequake-lite.zip")},
               "packagesSha256": sha(WORK / "packages.tsv"), "games": {}}
    shutil.copyfile(WORK / "packages.tsv", OUT / "games-packages.tsv")
    for filename in ("copying", "credits"):
        shutil.copyfile(WORK / f"librequake-{filename}.txt", OUT / f"LIBREQUAKE-{filename.upper()}.txt")
    neverball_source = WORK / "neverball-d9df382786d7e5635a1457b6b465ab943cf4fee6"
    shutil.copyfile(neverball_source / "LICENSE.md", OUT / "NEVERBALL-LICENSE.md")
    shutil.copyfile(neverball_source / "doc/legal/license-GPL-2.0.txt", OUT / "NEVERBALL-COPYING.txt")
    for name in NAMES:
        game = {k: read(WORK / f"{name}-{suffix}.json")
                for k, suffix in (("build", "build"), ("emit", "emit"), ("native", "native"))}
        game["runs"] = {c: read(WORK / "capture" / name / c / "run.json") for c in ("gcc", "evocc")}
        game["exactMatchingFrames"] = [f for f, h in game["runs"]["evocc"]["frames"].items()
                                      if game["runs"]["gcc"]["frames"][f] == h]
        validate(game)
        source = Path(game["build"]["directory"].replace("/work/games", str(WORK)))
        for unit, expected in game["build"]["sources"].items():
            if sha(source / unit) != expected:
                raise ValueError("game source changed after reference build")
        for obj, expected in game["native"]["objects"].items():
            if sha(WORK / "evocc" / name / obj) != expected:
                raise ValueError("installed game object changed")
        for link in game["native"]["links"]:
            if sha(Path(link["binary"].replace("/work/games", str(WORK)))) != link["sha256"]:
                raise ValueError("game executable changed")
        # C headers are retained by hash too; archive pin and patch identify
        # the complete source input, not just the translation-unit filenames.
        game["headerSha256"] = {str(p.relative_to(source)): sha(p) for p in sorted(source.rglob("*.h"))}
        if name == "neverball":
            game["referenceGeneratedLevels"] = {str(p.relative_to(source)): sha(p)
                                                 for p in sorted((source / "data").rglob("*.sol"))}
        game["patch"] = ({"file": "tools/showcase/neverball-mtrl.patch",
                          "sha256": sha(ROOT / "tools/showcase/neverball-mtrl.patch"),
                          "reason": "Parenthesize macro operand; original negative FP-to-byte cast is undefined"}
                         if name == "neverball" else None)
        for compiler in ("gcc", "evocc"):
            run_dir = WORK / "capture" / name / compiler
            run = game["runs"][compiler]
            for frame, expected in run["frames"].items():
                if sha(run_dir / frame) != expected:
                    raise ValueError("captured framebuffer changed")
            run["logs"] = {}
            for stream in ("stdout", "stderr"):
                target = OUT / f"{name}-{compiler}.{stream}.txt"
                shutil.copyfile(run_dir / f"{stream}.txt", target)
                run["logs"][target.name] = sha(target)
        ppm = WORK / "capture" / name / "evocc/frame-600.ppm"
        header, rgb = ppm.read_bytes().split(b"\n255\n", 1)
        magic, dimensions = header.split(b"\n")
        width, height = map(int, dimensions.split())
        if magic != b"P6" or len(rgb) != width * height * 3:
            raise ValueError("unexpected captured RGB shape")
        png = OUT / f"{name}.png"
        png.write_bytes(base64.b64decode(visuals.png_uri(width, height, rgb).split(",", 1)[1]))
        game["image"] = {"file": png.name, "sha256": sha(png), "frame": 600,
                         "width": width, "height": height, "rgbSha256": hashlib.sha256(rgb).hexdigest()}
        records["games"][name] = game
    (OUT / "games.json").write_text(json.dumps(records, indent=2) + "\n")
    check()


def captured_ppm(path):
    """Independent decoder for the committed unfiltered RGB PNG subset."""
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG capture")
    cursor, compressed, width, height = 8, b"", 0, 0
    while cursor < len(data):
        length, = struct.unpack_from(">I", data, cursor)
        kind = data[cursor + 4:cursor + 8]
        payload = data[cursor + 8:cursor + 8 + length]
        if kind == b"IHDR":
            width, height, depth, color, compression, filtering, interlace = struct.unpack(">IIBBBBB", payload)
            if (depth, color, compression, filtering, interlace) != (8, 2, 0, 0, 0):
                raise ValueError("unexpected PNG encoding")
        if kind == b"IDAT":
            compressed += payload
        cursor += length + 12
    rows = zlib.decompress(compressed)
    stride = width * 3 + 1
    if not width or not height or len(rows) != stride * height or any(rows[y*stride] for y in range(height)):
        raise ValueError("unexpected PNG rows")
    rgb = b"".join(rows[y*stride+1:(y+1)*stride] for y in range(height))
    return f"P6\n{width} {height}\n255\n".encode() + rgb


def check():
    records = read(OUT / "games.json")
    if set(records["games"]) != set(NAMES):
        raise ValueError("missing game capture")
    if sha(OUT / "games-packages.tsv") != records["packagesSha256"]:
        raise ValueError("changed runtime package manifest")
    if sha(ROOT / "tools/showcase/sdl_capture.c") != records["adapterSourceSha256"]:
        raise ValueError("changed capture adapter")
    if sha(ROOT / "tools/ladder/game_capture.py") != records["captureRunnerSha256"]:
        raise ValueError("changed capture runner")
    for game in records["games"].values():
        validate(game)
        if game["patch"] and sha(ROOT / game["patch"]["file"]) != game["patch"]["sha256"]:
            raise ValueError("changed application patch")
        image = game["image"]
        path = OUT / image["file"]
        if sha(path) != image["sha256"]:
            raise ValueError("changed game image")
        ppm = captured_ppm(path)
        if hashlib.sha256(ppm).hexdigest() != game["runs"]["evocc"]["frames"][f"frame-{image['frame']}.ppm"]:
            raise ValueError("image is not the captured framebuffer")
        for run in game["runs"].values():
            for name, expected in run["logs"].items():
                if sha(OUT / name) != expected:
                    raise ValueError("changed game log")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--publish", action="store_true")
    args = parser.parse_args()
    publish() if args.publish else check()
