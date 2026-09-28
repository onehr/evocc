#!/usr/bin/env python3
"""Run one bounded, real SDL game in Xvfb. Called inside the demo container."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("game", choices=["opentyrian", "neverball", "quakespasm"])
    parser.add_argument("compiler", choices=["gcc", "evocc"])
    parser.add_argument("--work", type=Path, default=Path("/work/games"))
    args = parser.parse_args()
    work = args.work
    build = json.loads((work / f"{args.game}-build.json").read_text())
    source = Path(build["directory"])
    name = {"opentyrian": "opentyrian", "neverball": "neverball", "quakespasm": "quakespasm"}[args.game]
    binary = source / name if args.compiler == "gcc" else work / "evocc" / args.game / name
    output = work / "capture" / args.game / args.compiler
    output.mkdir(parents=True, exist_ok=True)
    for frame in (180, 360, 600):
        (output / f"frame-{frame}.ppm").unlink(missing_ok=True)
    binary_hash = sha(binary)
    # This container is dedicated to the game lane. Reset only its game's
    # settings file, with the upstream offline option, not a host user's home.
    if args.game == "neverball":
        config = Path("/root/.neverball-dev/neverballrc")
        config.parent.mkdir(parents=True, exist_ok=True)
        config.write_text("online 0\n")
    env = dict(os.environ, LIBGL_ALWAYS_SOFTWARE="1", SDL_AUDIODRIVER="dummy",
               LP_NUM_THREADS="2", XDG_CONFIG_HOME=str(output / "config"),
               EVO_CAPTURE_DIR=str(output), EVO_CAPTURE_END="610")
    argv = [str(binary)]
    if args.game == "opentyrian":
        argv += ["--no-sound", "--no-joystick", "--no-xmas", "--data=" + str(work / "data/tyrian21/tyrian21")]
    elif args.game == "neverball":
        argv += ["--data", str(source / "data"), "--level", "map-easy/coins.sol"]
        env["EVO_CAPTURE_RETURN"] = "60"
    else:
        argv += ["-basedir", str(work / "data/librequake-lite/lite"), "-userdir", str(output / "user"),
                 "-window", "-width", "800", "-height", "600", "-nosound",
                 "+host_framerate", "0.016666667", "+timedemo", "demo1"]
    # LD_PRELOAD applies only to the child game, not Xvfb or the launcher.
    command = ["xvfb-run", "-a", "-s", "-screen 0 1024x768x24", "env",
               "LD_PRELOAD=" + str(work / "sdl_capture.so"), *argv]
    started = time.monotonic()
    with (output / "stdout.txt").open("wb") as stdout, (output / "stderr.txt").open("wb") as stderr:
        process = subprocess.Popen(command, env=env, cwd=source, stdout=stdout,
                                   stderr=stderr, start_new_session=True)
        try:
            code = process.wait(timeout=120)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            code = 124
    if sha(binary) != binary_hash:
        raise ValueError("game binary changed while capturing")
    frames = {p.name: sha(p) for p in sorted(output.glob("frame-*.ppm"))}
    receipt = {"game": args.game, "compiler": args.compiler, "binarySha256": binary_hash,
               "command": command, "environment": {k: v for k, v in env.items() if k not in os.environ or os.environ[k] != v},
               "adapterSha256": sha(work / "sdl_capture.so"), "exitCode": code,
               "seconds": round(time.monotonic() - started, 3), "frames": frames,
               "scope": "bounded rendering smoke; not full gameplay or timing-equivalence proof"}
    (output / "run.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt), flush=True)
    return 0 if code == 0 and len(frames) == 3 else 1


if __name__ == "__main__":
    raise SystemExit(main())
