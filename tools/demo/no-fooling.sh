#!/bin/sh
# Mutate only private demonstration inputs; always delegate to the real checker.
exec python3 - "$0" "$@" <<'PY'
import hashlib, json, os, pathlib, re, shlex, shutil, subprocess, sys, tempfile
script = pathlib.Path(sys.argv[1]).resolve()
repo = script.parents[2]
if len(sys.argv) > 2:
    args = sys.argv[2:]
    if args[:2] == ["--check-stage", "source-to-evoir"] and not pathlib.Path(os.environ["DEMO_MUTATED"]).exists():
        ir = pathlib.Path(next(a[3:] for a in args if a.startswith("ir=")))
        changed, count = re.subn(r"(?m)^.*ub\.assume\..*\n", "", ir.read_text(), count=1)
        assert count == 1, "expected one deleted obligation"
        ir.write_text(changed); pathlib.Path(os.environ["DEMO_MUTATED"]).touch()
    os.execv(os.environ["DEMO_REAL_CHECKER"], [os.environ["DEMO_REAL_CHECKER"], *args])
evo = os.environ.get("EVO") or shutil.which("evo")
cc = os.environ.get("EVOCC") or shutil.which("evocc")
checker = shutil.which("evocc-checker")
lean = os.environ.get("EVOCC_PROOF_CHECKER") or shutil.which("evoir-sem")
assert all((evo, cc, checker, lean)), "put EvoCC and both checkers on PATH"
work = pathlib.Path(tempfile.mkdtemp(prefix="evocc-refusals-")).resolve()
source = work / "fixed.c"
shutil.copyfile(repo / "tools/demo/loop/buggy.c", source)
def command(args, expected=0, output=None, env=None):
    args = list(map(str, args))
    print("$ " + shlex.join(args) + (" > " + shlex.quote(str(output)) if output else ""), flush=True)
    result = subprocess.run(args, capture_output=True, text=True, env=env)
    if output and result.returncode == 0:
        output.write_text(result.stdout)
    else:
        print(result.stdout, end="", flush=True)
    print(result.stderr, end="", flush=True)
    print("exit status " + str(result.returncode), flush=True)
    assert result.returncode == expected, "unexpected command status"
    return result.stdout
command([evo, "fix", source, "--apply", "--json"])
source.write_text(source.read_text().replace("{\n", "{\n    if (a == -2147483647 - 1) return 2147483647;\n", 1))
command([evo, "check", source, "--json"])
certs, bad = work / "certs", work / "bad-certs"
digest = hashlib.sha256(pathlib.Path(checker).read_bytes()).hexdigest()
command([cc, "--target=aarch64-unknown-linux-gnu", "--emit=asm", "-Oeq-scalar", f"--eq-checker={checker}", f"--eq-checker-sha256={digest}", f"--eq-proof-dir={certs}", source], output=work / "accepted.s")
shutil.copytree(certs, bad)
proof = bad / "000.proof.evp"
data = proof.read_bytes(); offset = data.index(b"\ninput ") + 7
proof.write_bytes(data[:offset] + (b"1" if data[offset:offset+1] == b"0" else b"0") + data[offset+1:])
replay = command([sys.executable, repo / "tools/bench.py", "--verify-eqsat-chain", bad, "--checker", checker], 1)
assert "BUNDLE_MISMATCH" in replay and not (bad / "output.s").exists()
command(["ls", bad])
print("Refusal: BUNDLE_MISMATCH exit=1; no assembly published.", flush=True)
env = dict(os.environ, EVOCC_PROOF_CHECKER=str(script), DEMO_REAL_CHECKER=str(lean), DEMO_MUTATED=str(work / "obligation-removed"))
refusal = json.loads(command([evo, "prove", source, "-o", work / "output.s", "--json"], 3, env=env))
proof_dir = pathlib.Path(refusal["proof_directory"]) / "checked"
assert "STAGE_REJECTED" in refusal["stderr"] and not any(p.exists() for p in [work / "output.s", proof_dir / "output.s", proof_dir / "record.json"])
command(["ls", proof_dir])
print("Refusal: STAGE_REJECTED exit=3; no assembly published.", flush=True)
wrong = work / "wrong-digest"
out = command([cc, "--target=aarch64-unknown-linux-gnu", "--emit=asm", f"--proof-required={wrong}", f"--proof-checker={lean}", "--proof-checker-sha256=" + "0"*64, "--core-entry=magnitude", "--core-entry=main", "-O0", "-fno-builtin", source], 5)
assert not out and not wrong.exists()
command(["ls", wrong], 1)
print("Refusal: CHECKER_IDENTITY_MISMATCH exit=5; no assembly published.", flush=True)
PY
