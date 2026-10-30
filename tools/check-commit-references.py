#!/usr/bin/env python3
"""Verify that every commit reference in the repository resolves.

Every tracked text file must name current commits. A record may keep a pre-rewrite identifier
when the rewrite maps of tools/history_map.py list it (see that module for what counts as a
record). A token is a reference when it is seven or more hexadecimal digits and a prefix of a
current or a mapped commit; other hexadecimal tokens (kernel commits, digests, bit patterns) are
counted, not judged. Exit status 1 lists every dangling reference; 0 prints the counts.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from history_map import HEX, MAPS, ROOT, chain, record_file, recorded_token  # noqa: E402

SUFFIXES = {".json", ".md", ".rs", ".py", ".sh", ".toml", ".yml", ".yaml", ".lean", ".txt", ".tsv"}


def git(*args: str) -> str:
    return subprocess.run(["git", "-C", str(ROOT), *args], capture_output=True, text=True,
                          check=True).stdout


def main() -> int:
    mapping, gone = chain()
    old = set(mapping) | gone
    # A remote-tracking ref can still reach rewritten commits; a mapped identifier is never current.
    current = set(git("rev-list", "--all").split()) - old
    counts = {"current": 0, "recorded": 0, "other": 0}
    dangling: list[tuple[str, int, str]] = []
    kinds: dict[str, str] = {}
    for name in git("ls-files", "-z").split("\0"):
        path = ROOT / name
        if not name or Path(name).suffix not in SUFFIXES or path in MAPS or not path.is_file():
            continue
        record = record_file(name)
        for number, line in enumerate(path.read_text(errors="ignore").split("\n"), 1):
            for match in HEX.finditer(line):
                token = match.group(1)
                if token not in kinds:
                    if any(c.startswith(token) for c in current):
                        kinds[token] = "current"
                    elif any(o.startswith(token) for o in old):
                        kinds[token] = "mapped"
                    else:
                        kinds[token] = "other"
                kind = kinds[token]
                if kind == "mapped":
                    if record or recorded_token(line, match):
                        counts["recorded"] += 1
                    else:
                        dangling.append((name, number, token))
                else:
                    counts[kind] += 1
    if dangling:
        for name, number, token in sorted(dangling):
            print(f"pre-rewrite commit reference {token} in {name}:{number}")
        return 1
    print(f"commit references: {counts['current']} current, {counts['recorded']} recorded "
          f"pre-rewrite identifiers resolved through the maps, {counts['other']} other "
          f"hexadecimal tokens; map entries: {len(mapping)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
