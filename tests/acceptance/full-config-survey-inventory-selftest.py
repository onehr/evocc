#!/usr/bin/env python3
"""Fail unless full-configuration inventory excludes host-compiler programs."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[2]
SURVEY = ROOT / "tools/survey-cohort.py"


def load_survey():
    spec = importlib.util.spec_from_file_location("full_config_survey", SURVEY)
    if spec is None or spec.loader is None:
        raise AssertionError("cannot load survey module")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def write_record(root: Path, relative: str, tool: str, source: str) -> None:
    record = root / relative
    record.parent.mkdir(parents=True, exist_ok=True)
    object_name = relative.removeprefix(".").removesuffix(".cmd")
    key = object_name.replace("/.", "/")
    record.write_text(
        f"savedcmd_{key} := {tool} -std=gnu11 -c -o {object_name} /work/src/{source}\n"
        f"source_{key} := /work/src/{source}\n",
        encoding="utf-8",
    )


def main() -> int:
    survey = load_survey()
    with tempfile.TemporaryDirectory(prefix="evocc-survey-inventory-") as temporary:
        root = Path(temporary)
        commands = root / "gcc-lane-1"
        sources = root / "source"
        (sources / "kernel").mkdir(parents=True)
        (sources / "scripts").mkdir(parents=True)
        (sources / "kernel/target.c").write_text("int target;\n", encoding="utf-8")
        (sources / "scripts/host.c").write_text("int host;\n", encoding="utf-8")
        write_record(commands, "kernel/.target.o.cmd", "gcc-14", "kernel/target.c")
        write_record(commands, "scripts/.host.o.cmd", "gcc", "scripts/host.c")

        reconnaissance = survey.load_reconnaissance_runner()
        units, rows = survey.inventory(
            reconnaissance,
            commands,
            sources,
            Path("/subject/evocc"),
        )
        assert [unit.source_logical for unit in units] == ["kernel/target.c"], units
        host = next(row for row in rows if row["record"] == "scripts/.host.o.cmd")
        assert host["included"] is False, host
        assert host["exclusionReason"] == "host-compiler-program", host
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
