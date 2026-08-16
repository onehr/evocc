#!/usr/bin/env python3

"""Build deterministic source mutants for the e-graph test sensitivity gate."""

from __future__ import annotations

import pathlib
import sys


def replace_function(source: str, signature: str, body: str) -> str:
    start = source.find(signature)
    if start < 0:
        raise ValueError(f"missing function signature: {signature}")
    brace = source.find("{", start + len(signature) - 1)
    depth = 0
    end = None
    for index in range(brace, len(source)):
        char = source[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                end = index + 1
                break
    if end is None:
        raise ValueError(f"unterminated function: {signature}")
    return source[:brace] + body + source[end:]


def replace_once(source: str, old: str, new: str) -> str:
    if source.count(old) != 1:
        raise ValueError(f"expected one mutation site, observed {source.count(old)}")
    return source.replace(old, new, 1)


def main() -> int:
    if len(sys.argv) != 4:
        print("usage: egraph-mutant-builder.py INPUT OUTPUT MUTANT", file=sys.stderr)
        return 2

    input_path = pathlib.Path(sys.argv[1])
    output_path = pathlib.Path(sys.argv[2])
    mutant = sys.argv[3]
    source = input_path.read_text(encoding="utf-8")

    if mutant == "rebuild_noop":
        source = replace_function(source, "    pub fn rebuild(&mut self) {", "{}")
    elif mutant == "repair_noop":
        source = replace_function(
            source,
            "    fn repair(&mut self, node_id: ENodeId) {",
            "{ let _ = node_id; }",
        )
    elif mutant == "missing_parent_update":
        old = """        for (i, &child) in children.iter().enumerate() {
            let child = self.uf.find(child);
            self.eclasses[child as usize]
                .parents
                .push((op, i, class_id));
        }

        class_id
"""
        source = replace_once(source, old, "        class_id\n")
    elif mutant == "collapsed_constant":
        source = replace_once(
            source,
            "        let key = (op, children.clone());\n",
            "        let key = (if children.is_empty() { 0 } else { op }, children.clone());\n",
        )
    else:
        raise ValueError(f"unknown mutant: {mutant}")

    output_path.write_text(source, encoding="utf-8")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"mutant construction failed: {error}", file=sys.stderr)
        raise SystemExit(2)
