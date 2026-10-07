#!/bin/sh
# Export only the header directories of evo-flight-lab:sealed.
# Usage: tools/export-sysroot.sh [DESTINATION]
# The default is sysroot/aarch64-unknown-linux-gnu, on any host volume.
set -eu
exec python3 - "$0" "$@" <<'PY'
import hashlib, json, os, shutil, subprocess, sys, tempfile
from pathlib import Path

script = Path(sys.argv[1]).resolve()
repo = script.parent.parent
target = "aarch64-unknown-linux-gnu"
image = "evo-flight-lab:sealed"
args = sys.argv[2:]

def fail(message):
    raise SystemExit("export-sysroot: " + message)

def query(*argv):
    return subprocess.check_output(argv, text=True).strip()

if args[:1] == ["--inside"]:
    destination, digest = Path(args[1]), args[2]
    # Copy inside Linux first; a host volume can merge case-distinct names.
    staged = tempfile.TemporaryDirectory(prefix="evocc-sysroot-")
    staging = Path(staged.name)
    if query("gcc", "-dumpmachine") != "aarch64-linux-gnu":
        fail("the image does not contain the sealed AArch64 GCC")
    includes = [Path("/usr/include")]
    absent = []
    for name in ("include", "include-fixed"):
        path = Path(query("gcc", "-print-file-name=" + name))
        if path.is_absolute() and path.is_dir():
            includes.append(path)
        elif name == "include-fixed":
            absent.append(name)
        else:
            fail("GCC resource headers are absent")
    files = {}
    total = 0
    for directory in includes:
        sources = (Path(base) / name for base, _, names in os.walk(directory, followlinks=True)
                   for name in sorted(names))
        for source in sources:
            if not source.is_file():
                fail("not a readable header file: " + str(source))
            relative = source.relative_to("/")
            data = source.read_bytes()
            total += len(data)
            if total > 500 * 1024 * 1024:
                fail("header export exceeds 500 MiB")
            output = staging / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            if output.exists():
                if not output.is_file() or output.read_bytes() != data:
                    fail("existing file differs; keep it intact: " + str(output))
            else:
                output.write_bytes(data)
            files[relative.as_posix()] = hashlib.sha256(data).hexdigest()
    groups = {}
    for path in files:
        groups.setdefault(path.lower(), []).append(path)
    excluded = sorted(path for group in groups.values() if len(group) > 1
                      for path in group if any(c.isupper() for c in Path(path).name))
    for path in excluded:
        (staging / path).unlink()
        del files[path]
    if len(files) != len({path.lower() for path in files}):
        fail("case collisions remain after excluding uppercase basenames")
    for path in files:
        output = destination / path
        output.parent.mkdir(parents=True, exist_ok=True)
        if output.exists() and output.read_bytes() != (staging / path).read_bytes():
            fail("existing file differs; keep it intact: " + str(output))
        shutil.copyfile(staging / path, output)
    staged.cleanup()
    actual = {p.relative_to(destination).as_posix() for p in destination.rglob("*") if p.is_file()}
    if actual != files.keys():
        fail("destination contains files outside the exported header directories")
    print(json.dumps({"target": target, "image": image, "image_digest": digest,
        "gcc_version": query("gcc", "--version").splitlines()[0],
        "directories": [str(p) for p in includes], "absent": absent,
        "excluded_case_collisions": excluded,
        "files": dict(sorted(files.items()))}, indent=2) + "\n", end="")
    sys.exit(0)

if args in (["--help"], ["-h"]):
    print("usage: tools/export-sysroot.sh [DESTINATION]\n"
          "Export headers from evo-flight-lab:sealed, excluding uppercase case collisions.")
    sys.exit(0)
if len(args) > 1 or (args and args[0].startswith("-")):
    fail("use --help or one destination directory")
destination = Path(args[0] if args else repo / "sysroot" / target).resolve()
destination.mkdir(parents=True, exist_ok=True)
runtime = "container" if sys.platform == "darwin" else "docker"
if not shutil.which(runtime):
    fail("install " + runtime + " and load " + image + " first")
metadata = json.loads(query(runtime, "image", "inspect", image))[0]
digest = (metadata["configuration"]["descriptor"]["digest"]
          if runtime == "container" else metadata["Id"])
argv = [runtime, "run", "--rm", "--platform", "linux/arm64", "--network", "none",
        "--user", f"{os.getuid()}:{os.getgid()}",
        "-v", f"{script.parent}:/export-tools:ro", "-v", f"{destination}:/headers"]
if runtime == "docker":
    argv.append("--pull=never")
else:
    argv.extend(["--progress", "none"])
argv.extend([image if runtime == "container" else digest, "sh",
             "/export-tools/export-sysroot.sh", "--inside", "/headers", digest])
result = subprocess.run(argv, stdout=subprocess.PIPE, check=True)
if json.loads(query(runtime, "image", "inspect", image))[0] != metadata:
    fail("image metadata changed during export; no contract was written")
contract = json.loads(result.stdout)
path = repo / "contracts" / f"sysroot-{target}.json"
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps(contract, indent=2) + "\n")
print(f"exported {len(contract['files'])} files to {destination}; contract: {path}")
PY
