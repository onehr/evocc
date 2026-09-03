#!/usr/bin/env python3
"""Verify that no tracked contract or document still names a pre-rewrite commit.

tests/contracts/history-rewrite-map.json records, for every commit whose
message was rewritten, the identifier it had before and the one it has now, and lists the
identifiers of commits that were unreachable before the rewrite and are gone.
A reference dangles when it is a prefix (seven or more digits) of a
pre-rewrite identifier and not of a post-rewrite one. Only files git tracks are scanned. Other hexadecimal
tokens (kernel commits, digests, bit patterns) are counted, not judged.
Exit status 1 lists every dangling reference; 0 prints the counts.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HEX = re.compile(r"(?<![0-9A-Za-z])([0-9a-f]{7,40})(?![0-9A-Za-z])")
SCAN = [ROOT / "tests" / "contracts", ROOT / "README.md", ROOT / "docs"]


def resolves(candidate: str) -> bool:
    result = subprocess.run(
        [
            "git",
            "-C",
            str(ROOT),
            "rev-parse",
            "--verify",
            "--quiet",
            f"{candidate}^{{commit}}",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode == 0


def main() -> int:
    map_path = ROOT / "tests" / "contracts" / "history-rewrite-map.json"
    record = json.loads(map_path.read_text()) if map_path.exists() else {}
    mapping: dict[str, str] = record.get("commits", {})
    old_ids = set(mapping)
    new_ids = set(mapping.values()) | set(record.get("historicalIds", {}))
    tracked = subprocess.run(
        [
            "git",
            "-C",
            str(ROOT),
            "ls-files",
            "-z",
            *[str(entry.relative_to(ROOT)) for entry in SCAN],
        ],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split("\0")
    files = [ROOT / name for name in tracked if name]
    counts = {"resolved": 0, "rewritten": 0, "other": 0}
    dangling: list[tuple[Path, str]] = []
    seen: dict[str, str] = {}
    for path in files:
        if path.suffix not in {".json", ".md"} or not path.is_file():
            continue
        if path == map_path:
            continue
        for candidate in set(HEX.findall(path.read_text(errors="ignore"))):
            if candidate not in seen:
                if any(new.startswith(candidate) for new in new_ids):
                    seen[candidate] = "rewritten"
                elif any(old.startswith(candidate) for old in old_ids):
                    seen[candidate] = "dangling"
                elif resolves(candidate):
                    seen[candidate] = "resolved"
                else:
                    seen[candidate] = "other"
            kind = seen[candidate]
            if kind == "dangling":
                dangling.append((path.relative_to(ROOT), candidate))
            else:
                counts[kind] += 1
    if dangling:
        for path, candidate in sorted(dangling):
            print(f"pre-rewrite commit reference {candidate} in {path}")
        return 1
    print(
        "commit references: "
        f"{counts['rewritten']} rewritten, {counts['resolved']} resolved unchanged, "
        f"{counts['other']} other hexadecimal tokens; map entries: {len(mapping)}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
