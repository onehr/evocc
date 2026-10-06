#!/usr/bin/env python3
"""Check the pinned ground-truth documents in spec/ against spec/pins.json.

Usage: python3 tools/check-spec-pins.py            check every pin; exit 1 on a mismatch
       python3 tools/check-spec-pins.py --update   rewrite spec/pins.json from the files

A pinned document is each spec/*.md except spec/README.md. A pin holds its revision (the first line
with "Revision" and a version number) and its SHA-256. To change a document: edit it, bump its
revision line, run --update, and commit both files. Standard library only.
"""

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPEC = ROOT / "spec"
PINS = SPEC / "pins.json"
FORMAT = "evocc-spec-pins-v1"
REVISION = re.compile(r"Revision\W{0,4}v?(\d+(?:\.\d+)*)")
ADVICE = "bump its revision and run tools/check-spec-pins.py --update"


def revision(path):
    for line in path.read_text(encoding="utf-8").splitlines():
        found = REVISION.search(line)
        if found:
            return found.group(1)
    return None


def current():
    documents = sorted(p for p in SPEC.glob("*.md") if p.name != "README.md")
    return {p.relative_to(ROOT).as_posix(): {"revision": revision(p),
            "sha256": hashlib.sha256(p.read_bytes()).hexdigest()} for p in documents}


def pinned():
    if not PINS.exists():
        return {}
    data = json.loads(PINS.read_text(encoding="utf-8"))
    if data.get("format") != FORMAT or not isinstance(data.get("pins"), dict):
        sys.exit(f"spec/pins.json: expected format {FORMAT} with a pins object")
    return data["pins"]


def main(argv):
    if argv not in ([], ["--update"]):
        sys.exit(__doc__.strip())
    pins, now = pinned(), current()
    errors = [f"{name} has no revision line" for name, pin in now.items() if not pin["revision"]]
    if argv == ["--update"]:
        if errors:
            sys.exit("\n".join(errors))
        for name, pin in now.items():
            old = pins.get(name)
            if old and old["sha256"] != pin["sha256"] and old["revision"] == pin["revision"]:
                print(f"warning: {name} changed but its revision is still {pin['revision']}")
        text = json.dumps({"format": FORMAT, "pins": now}, indent=2, sort_keys=True) + "\n"
        PINS.write_text(text, encoding="utf-8")
        print(f"spec pins: wrote {len(now)} pins to spec/pins.json")
        return 0
    for name, pin in now.items():
        if name not in pins:
            errors.append(f"{name} is not pinned: run tools/check-spec-pins.py --update")
        elif pins[name] != pin:
            errors.append(f"{name} changed: {ADVICE}")
    errors += [f"{name} is pinned but missing: run tools/check-spec-pins.py --update"
               for name in sorted(pins.keys() - now.keys())]
    for error in errors:
        print(error, file=sys.stderr)
    if not errors:
        print(f"spec pins: {len(now)} documents match spec/pins.json")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
