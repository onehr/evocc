#!/bin/sh
# Export only the C header directories of the pinned sealed image, and the
# license texts of the Debian packages that own those headers.
# Usage: tools/export-sysroot.sh [DESTINATION]
# The default is sysroot/aarch64-unknown-linux-gnu, on any host volume. The
# license texts go to licenses/aarch64-unknown-linux-gnu/ beside DESTINATION,
# outside the header tree.
# The default image is evo-nexlab:sealed-20260810, the image of the contract.
# Its digest must be the pinned digest. EVOCC_SYSROOT_IMAGE names another
# image, and its export changes the contract.
# The export leaves out the directories that a C compiler for AArch64 Linux
# does not read: the C++ headers, the Clang headers and the multiarch
# directories of other architectures. The contract lists them.
set -eu
exec python3 - "$0" "$@" <<'PY'
import hashlib, json, os, re, shutil, subprocess, sys, tempfile
from pathlib import Path

script = Path(sys.argv[1]).resolve()
repo = script.parent.parent
target = "aarch64-unknown-linux-gnu"
multiarch = "aarch64-linux-gnu"
# The image of contracts/sysroot-aarch64-unknown-linux-gnu.json. Apple container
# cannot run a local image by its digest, so the export checks the digest.
pinned_image = "evo-nexlab:sealed-20260810"
pinned_digest = "sha256:2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2"
image = os.environ.get("EVOCC_SYSROOT_IMAGE", pinned_image)
args = sys.argv[2:]

def fail(message):
    raise SystemExit("export-sysroot: " + message)

def query(*argv):
    return subprocess.check_output(argv, text=True).strip()

def keep(output, data):
    # Write a new file. An existing file must already hold the same bytes.
    output.parent.mkdir(parents=True, exist_ok=True)
    if not output.exists():
        output.write_bytes(data)
    elif not output.is_file() or output.read_bytes() != data:
        fail("existing file differs; keep it intact: " + str(output))

def unread(path):
    # A directory that C code for AArch64 Linux does not include from: C++,
    # Clang, or the multiarch directory of another architecture.
    parts = path.split("/")
    if parts[:2] != ["usr", "include"] or len(parts) not in (3, 4):
        return False
    if len(parts) == 4:
        return parts[2] == multiarch and parts[3] == "c++"
    return parts[2] in ("c++", "clang") or (
        parts[2] != multiarch and re.fullmatch(r"[a-z0-9_]+-linux-gnu[a-z0-9]*", parts[2]) is not None)

if args[:1] == ["--inside"]:
    destination, licenses, digest, image = Path(args[1]), Path(args[2]), args[3], args[4]
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
    files, skipped = {}, set()
    total = 0
    for directory in includes:
        for base, subdirectories, names in os.walk(directory, followlinks=True):
            for name in sorted(subdirectories):
                path = (Path(base) / name).relative_to("/").as_posix()
                if unread(path):
                    subdirectories.remove(name)
                    skipped.add(path)
            for name in sorted(names):
                source = Path(base) / name
                if not source.is_file():
                    fail("not a readable header file: " + str(source))
                relative = source.relative_to("/")
                data = source.read_bytes()
                total += len(data)
                if total > 500 * 1024 * 1024:
                    fail("header export exceeds 500 MiB")
                keep(staging / relative, data)
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
        keep(destination / path, (staging / path).read_bytes())
    staged.cleanup()
    actual = {p.relative_to(destination).as_posix() for p in destination.rglob("*") if p.is_file()}
    if actual != files.keys():
        fail("destination holds files that this export does not select; export into an empty directory")
    # The walk follows directory links. Ask dpkg for the owner of each real path.
    real = {path: os.path.realpath("/" + path) for path in files}
    names, owners = sorted(set(real.values())), {}
    for start in range(0, len(names), 256):
        found = subprocess.run(["dpkg-query", "-S", *names[start:start + 256]],
                               capture_output=True, text=True).stdout
        for line in found.splitlines():
            owner, _, path = line.partition(": ")
            if not owner.startswith("diversion by "):
                owners[path] = [name.split(":")[0] for name in owner.split(", ")]
    packages = {}
    for path in files:
        if real[path] not in owners:
            fail("no Debian package owns " + path)
        for name in owners[real[path]]:
            packages[name] = packages.get(name, 0) + 1
    versions = dict(line.split("\t") for line in query(
        "dpkg-query", "-W", "-f", "${Package}\t${Version}\n", *packages).splitlines())
    # Each owner's copyright file, and the common license texts that it cites.
    def text(path):
        if not path.is_file():
            fail("the license text is absent: " + str(path))
        return path.read_bytes()
    texts = {name + "/copyright": text(Path("/usr/share/doc", name, "copyright"))
             for name in packages}
    for data in list(texts.values()):
        cited = r"/usr/share/common-licenses/([\w.+-]*\w)"
        for name in re.findall(cited, data.decode("utf-8", "replace")):
            texts["common-licenses/" + name] = text(Path("/usr/share/common-licenses", name))
    for name, data in texts.items():
        keep(licenses / name, data)
    actual = {p.relative_to(licenses).as_posix() for p in licenses.rglob("*") if p.is_file()}
    if actual != texts.keys():
        fail("the license directory contains other files")
    print(json.dumps({"target": target, "image": image, "image_digest": digest,
        "gcc_version": query("gcc", "--version").splitlines()[0],
        "directories": [str(p) for p in includes], "excluded_directories": sorted(skipped),
        "absent": absent,
        "description": "The bundled sysroot does not support these eight netfilter headers. On a case-insensitive volume an include of one of them resolves to the lowercase file of the same name; on a case-sensitive volume it is not found. EvoCC makes no promise about either result. The contract field excluded_case_collisions lists them.",
        "excluded_case_collisions": excluded,
        "packages": {name: {"version": versions[name], "headers": packages[name]}
                     for name in sorted(packages)},
        "licenses": {name: hashlib.sha256(data).hexdigest()
                     for name, data in sorted(texts.items())},
        "files": dict(sorted(files.items()))}, indent=2) + "\n", end="")
    sys.exit(0)

if args in (["--help"], ["-h"]):
    print("usage: tools/export-sysroot.sh [DESTINATION]\n"
          "Export the C headers of " + pinned_image + ", which must have the digest\n"
          + pinned_digest + ".\n"
          "Leave out the C++ and Clang headers, the headers of other architectures\n"
          "and uppercase case collisions.\n"
          "Export the license texts of their Debian packages to\n"
          "licenses/" + target + "/ beside DESTINATION.\n"
          "EVOCC_SYSROOT_IMAGE names another image; its export changes the contract.")
    sys.exit(0)
if len(args) > 1 or (args and args[0].startswith("-")):
    fail("use --help or one destination directory")
destination = Path(args[0] if args else repo / "sysroot" / target).resolve()
licenses = destination.parent / "licenses" / target
destination.mkdir(parents=True, exist_ok=True)
licenses.mkdir(parents=True, exist_ok=True)
runtime = "container" if sys.platform == "darwin" else "docker"
if not shutil.which(runtime):
    fail("install " + runtime + " and load " + image + " first")
try:
    metadata = json.loads(query(runtime, "image", "inspect", image))[0]
except subprocess.CalledProcessError:
    fail("load " + image + " first, or set EVOCC_SYSROOT_IMAGE to another image")
digest = (metadata["configuration"]["descriptor"]["digest"]
          if runtime == "container" else metadata["Id"])
if image == pinned_image and digest != pinned_digest:
    fail(f"{image} has digest {digest}, not the pinned {pinned_digest}; "
         "load the pinned image, or set EVOCC_SYSROOT_IMAGE to another image")
argv = [runtime, "run", "--rm", "--platform", "linux/arm64", "--network", "none",
        "--user", f"{os.getuid()}:{os.getgid()}", "-v", f"{script.parent}:/export-tools:ro",
        "-v", f"{destination}:/headers", "-v", f"{licenses}:/licenses"]
if runtime == "docker":
    argv.append("--pull=never")
else:
    argv.extend(["--progress", "none"])
argv.extend([image if runtime == "container" else digest, "sh", "/export-tools/export-sysroot.sh",
             "--inside", "/headers", "/licenses", digest, image])
result = subprocess.run(argv, stdout=subprocess.PIPE, check=True)
if json.loads(query(runtime, "image", "inspect", image))[0] != metadata:
    fail("image metadata changed during export; no contract was written")
contract = json.loads(result.stdout)
path = repo / "contracts" / f"sysroot-{target}.json"
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps(contract, indent=2) + "\n")
print(f"exported {len(contract['files'])} headers to {destination} and "
      f"{len(contract['licenses'])} license texts to {licenses}; contract: {path}")
PY
