"""The commit maps of the history rewrites, and the rules for recorded identifiers.

The history was rewritten twice: on 2026-09-03 to drop an attribution trailer from commit
messages, and on 2026-10-30 to remove Chinese text from tracked files. Each rewrite left a map
from the identifiers before it to the identifiers after it. Records keep the identifiers they
recorded: files under tests/contracts/ and tests/fixtures/, the registered experiment protocols,
the recorded terminal transcripts of the README, a path component and a quoted compiler version.
A page that a tool renders from records names the current commits (current_text).
"""
from __future__ import annotations

import bisect
import functools
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MAPS = (ROOT / "tests" / "contracts" / "history-rewrite-map.json",
        ROOT / "tests" / "contracts" / "history-rewrite-map-2.json")
HEX = re.compile(r"(?<![0-9A-Za-z])([0-9a-f]{7,40})(?![0-9A-Za-z])")
RECORD_PREFIXES = ("tests/contracts/", "tests/fixtures/", "vendor/")
RECORD_FILES = {"docs/images/readme/highlights.json", "docs/images/showcase/captures.json"}
VERSION_QUOTE = re.compile(r"\(([0-9a-f]{12}) \d{4}-\d{2}-\d{2}\)")


def record_file(name: str) -> bool:
    return (name.startswith(RECORD_PREFIXES) or name in RECORD_FILES
            or (name.startswith("tools/experiment/protocol-") and name.endswith(".json")))


def recorded_token(line: str, match: re.Match) -> bool:
    """A path component or a quoted compiler version stays as recorded."""
    start, end = match.span(1)
    if (start and line[start - 1] == "/") or (end < len(line) and line[end] == "/"):
        return True
    return any(q.span(1) == (start, end) for q in VERSION_QUOTE.finditer(line))


@functools.lru_cache(maxsize=1)
def _maps() -> tuple[dict[str, str], frozenset[str], tuple[str, ...]]:
    mapping: dict[str, str] = {}
    gone: set[str] = set()
    for path in MAPS:
        if not path.exists():
            continue
        record = json.loads(path.read_text())
        commits = record.get("commits", {})
        for old, new in mapping.items():
            mapping[old] = commits.get(new, new)
        mapping.update(commits)
        gone |= set(record.get("historicalIds", {}))
    return mapping, frozenset(gone), tuple(sorted(mapping))


def chain() -> tuple[dict[str, str], set[str]]:
    """Each pre-rewrite identifier with its current identifier, and the identifiers of
    commits that were already unreachable before the first rewrite."""
    mapping, gone, _ = _maps()
    return dict(mapping), set(gone)


def current(token: str) -> str:
    """The current identifier of a recorded one, abbreviated to the same length."""
    mapping, _, olds = _maps()
    hits = set()
    for old in olds[bisect.bisect_left(olds, token):]:
        if not old.startswith(token):
            break
        hits.add(mapping[old])
    return next(iter(hits))[:len(token)] if len(hits) == 1 else token


def current_text(text: str) -> str:
    """The text with each pre-rewrite identifier replaced by its current one, except the
    tokens that recorded_token keeps."""
    def line_text(line: str) -> str:
        return HEX.sub(lambda m: m.group(1) if recorded_token(line, m) else current(m.group(1)),
                       line)
    return "\n".join(line_text(line) for line in text.split("\n"))
