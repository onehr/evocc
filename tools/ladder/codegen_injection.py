"""Exercise N05 on real Git commits and assembler-produced AArch64 objects.

This is a small detector/localizer test, not a six-TU compiler measurement.
The temporary Git index never changes the working tree or main index. A test
branch retains the three actual commits. Captures contain only small command
results; assembled objects are removed after S1 measurement.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from .codegen_trend import (append_observation, compare, git_commits,
                           locate_regression, object_row)
from .eqsat_chain import digest, require
from .eqsat_native import Capture


def run(root, assembler, branch):
    root, assembler = Path(root).resolve(), Path(assembler).resolve()
    free_before = shutil.disk_usage(root).free
    require(free_before >= 25 * 1024**3, "N05 injection requires 25 GiB free")
    require(branch.startswith("refs/heads/n05-injected-"), "expected an isolated N05 test branch")
    subprocess.run(["git", "-C", str(root), "check-ref-format", branch], check=True)
    existing = subprocess.run(["git", "-C", str(root), "show-ref", "--verify", "--quiet", branch])
    require(existing.returncode == 1, "ref already exists or could not be inspected")
    plan = root / "tests/contracts/codegen-n05-20261002.json"
    committed = subprocess.check_output(["git", "-C", str(root), "show", "HEAD:" + plan.relative_to(root).as_posix()])
    require(committed == plan.read_bytes(), "N05 plan must be committed before injection")
    head = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()
    index = Path(subprocess.check_output(["git", "-C", str(root), "rev-parse", "--git-path", "index"], text=True).strip())
    if not index.is_absolute():
        index = root / index
    index_before = digest(index.read_bytes())
    parent = root / ".evidence/codegen-trend"
    parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="injection-", dir=parent))
    receipt = dict(schema="evocc-codegen-injection-v1", status="STARTED", attempts=[],
                   base_commit=head, branch=branch, assembler_sha256=digest(assembler.read_bytes()),
                   plan_sha256=digest(committed), commits=[], measurements=[],
                   claim="DETECTOR_LOCALIZER_EXERCISE_NOT_SIX_TU_OR_COMPILER_ACCEPTANCE")

    def save():
        (directory / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")

    env = dict(os.environ, GIT_INDEX_FILE=str(directory / "temporary.index"))
    resources = dict(timeout_seconds=30, output_bytes=1024**2,
                     max_retained_bytes=500 * 1024**2, min_free_bytes=15 * 1024**3)
    capture = Capture(directory, resources, env, receipt, save)

    def command(stage, argv):
        row = capture.run(stage, argv)
        return (directory / row["stdout"]).read_text().strip(), row

    def git(stage, *args):
        return command(stage, ["git", "-C", root, *args])[0]

    try:
        capture.run("assembler-version", [assembler, "--version"])
        git("read-base-tree", "read-tree", head)
        previous, records = head, []
        fixture_path = "tests/fixtures/codegen-trend/injected-probe.s"
        sources = ("good.s", "regressed.s", "regressed.s")
        for number, source in enumerate(sources):
            path = root / "tests/fixtures/codegen-trend" / source
            blob = git(f"source-blob:{number}", "hash-object", "-w", path)
            git(f"stage-source:{number}", "update-index", "--add", "--cacheinfo", "100644," + blob + "," + fixture_path)
            tree = git(f"write-tree:{number}", "write-tree")
            commit = git(f"commit:{number}", "commit-tree", tree, "-p", previous,
                         "-m", f"N05 measurement exercise {number}: {source}")
            previous = commit
            receipt["commits"].append(commit)
            _, extracted = command(f"extract-source:{number}", ["git", "-C", root, "show", commit + ":" + fixture_path])
            objects = []
            for repeat in (0, 1):
                obj = directory / f"{number}-{repeat}.o"
                capture.run(f"assemble:{number}:{repeat}", [assembler, "--target=aarch64-linux-gnu", "-c", "-x",
                            "assembler", directory / extracted["stdout"], "-o", obj])
                objects.append(obj)
            row = object_row("test:n05_regression_probe", "unchecked", objects[0].read_bytes(), objects[1].read_bytes(),
                             gcc_o2_text_bytes=None)
            record = dict(schema="evocc-codegen-trend-v1", source_commit=commit,
                          compiler_sha256=receipt["assembler_sha256"], plan_sha256=receipt["plan_sha256"],
                          inputs_sha256=digest(b"N05 injected assembly population v1"),
                          capture_sha256=digest((commit + row["object_sha256"]).encode()),
                          variant="injected-assembly-test", units=[row], ladder=[], steps=None, proof=None)
            append_observation(directory / "trend.jsonl", record)
            records.append(record)
            receipt["measurements"].append(dict(commit=commit, text_bytes=row["text_bytes"],
                object_sha256=row["object_sha256"], two_builds="BYTE_IDENTICAL"))
            for obj in objects:
                obj.unlink()
            save()
        git("retain-test-branch", "update-ref", branch, previous, "0" * 40)
        result = locate_regression(git_commits(root, records[0]["source_commit"], records[-1]["source_commit"]),
                                   records, "test:n05_regression_probe", "unchecked", variant="injected-assembly-test")
        comparison = compare(records[0], records[1])
        require(comparison["status"] == "REGRESSION" and result["status"] == "REGRESSION_FOUND" and
                result["first_bad"] == records[1]["source_commit"], "injected size regression not localized")
        receipt.update(status="DETECTOR_LOCALIZER_PASS", comparison=comparison, bisect=result)
    finally:
        for path in directory.glob("*.o"):
            path.unlink()
        (directory / "temporary.index").unlink(missing_ok=True)
        receipt["main_index_unchanged"] = digest(index.read_bytes()) == index_before
        receipt["head_unchanged"] = subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip() == head
        receipt["capture_retained_bytes"] = sum(p.stat().st_size for p in directory.rglob("*") if p.is_file())
        receipt["disk_free_delta_bytes"] = shutil.disk_usage(root).free - free_before
        save()
    require(receipt["main_index_unchanged"] and receipt["head_unchanged"], "shared checkout changed during injection")
    return directory, receipt


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--assembler", type=Path, required=True)
    parser.add_argument("--branch", required=True)
    args = parser.parse_args(argv)
    directory, receipt = run(args.root, args.assembler, args.branch)
    print(json.dumps(dict(directory=str(directory), status=receipt["status"],
                          first_bad=receipt["bisect"]["first_bad"]), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
