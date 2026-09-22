#!/usr/bin/env python3
"""Regenerate the typed GCC 14 target-header interface, not its implementation.

Run inside the frozen AArch64 image, with the existing table as the seed for
names constructed by token pasting. Also collect literal names from the real
header. Type queries avoid losing signatures to GCC's early builtin folding.
The temporary tree dump is deleted on exit; only the TSV is written to stdout.
"""

import argparse
from pathlib import Path
import re
import subprocess
import tempfile


def extract(compiler: str, seed: Path) -> str:
    include = Path(subprocess.check_output(
        [compiler, "-print-file-name=include"], text=True
    ).strip())
    names = {line.split("\t")[0] for line in seed.read_text().splitlines()}
    names.update(re.findall(r"\b__builtin_aarch64_\w+(?=\s*\()",
                           (include / "arm_neon.h").read_text()))
    # The pragma declares these public intrinsics directly; they have no C
    # function body in the header. Query every documented type combination.
    kinds = "bf16 f16 f32 f64 s8 s16 s32 s64 u8 u16 u32 u64 p8 p16 p64".split()
    for suffix, elements in [("", kinds), ("q", kinds + ["p128"])]:
        names.update(f"vreinterpret{suffix}_{a}_{b}" for a in elements for b in elements if a != b)
    names = sorted(names)
    source = '#include <arm_neon.h>\n'
    source += "\n".join(f"void signature_probe_{i}(void) {{ __typeof__({name}) *signature_{i}; }}"
                        for i, name in enumerate(names)) + "\n"
    with tempfile.TemporaryDirectory(prefix="evocc-neon-types-") as directory:
        dump = Path(directory) / "types.dump"
        subprocess.run([compiler, "-std=gnu11", "-O0",
                        f"-fdump-tree-original-raw={dump}", "-x", "c", "-",
                        "-c", "-o", str(Path(directory) / "types.o")],
                       input=source, text=True, check=True)
        text = dump.read_text()
    nodes = {}

    def ref(node: str, field: str) -> str | None:
        match = re.search(rf"\b{field}\s*:\s*@(\d+)", nodes[node])
        return match[1] if match else None

    def spelling(node: str) -> str:
        value = nodes[node]
        if value.startswith("identifier_node"):
            match = re.search(r"strg:\s*(.*?)\s+lngt:", value, re.S)
            assert match, value
            return " ".join(match[1].split())
        if value.startswith("pointer_type"):
            result = f"ptr({spelling(ref(node, 'ptd'))})"
        else:
            # Strip source typedef aliases but not nominal builtin identities.
            unqualified = ref(node, "unql")
            named = ref(node, "name")
            result = spelling(unqualified or named) if unqualified or named else ""
        assert result, value
        if re.search(r"\bqual:\s+c\b", value):
            result = "const " + result
        return result

    rows = {}
    for body in text.split(";; Function signature_probe_")[1:]:
        nodes = dict(re.findall(r"^@(\d+)\s+(.*?)(?=^@|\Z)", body, re.M | re.S))
        for node, value in nodes.items():
            if not value.startswith("var_decl"):
                continue
            name = spelling(ref(node, "name"))
            if not name.startswith("signature_"):
                continue
            function = ref(ref(node, "type"), "ptd")
            assert nodes[function].startswith("function_type"), names[int(name[10:])]
            result = [spelling(ref(function, "retn"))]
            parameter = ref(function, "prms")
            while parameter:
                result.append(spelling(ref(parameter, "valu")))
                parameter = ref(parameter, "chan")
            assert result[-1] == "void", result  # final prototype sentinel
            rows[names[int(name[10:])]] = result[:-1]
    assert len(rows) == len(names), (len(rows), len(names))
    return "\n".join("\t".join([name, *rows[name]]) for name in names) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", default="gcc-14")
    parser.add_argument("--seed", type=Path,
                        default=Path("crates/evocc-types/src/neon-signatures.tsv"))
    args = parser.parse_args()
    print(extract(args.compiler, args.seed), end="")
