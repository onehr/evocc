"""N03a register-description mutations and checked-in regeneration.

Each structural mutation carries its own correct digest. A digest rejection
therefore cannot substitute for checking the register relationship under test.
"""

from __future__ import annotations

import hashlib
import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GENERATOR = ROOT / "tools" / "generate-target.py"
DESCRIPTION = ROOT / "targets" / "aarch64" / "regfile"
MIR_GENERATED = ROOT / "crates" / "evocc-mir" / "src" / "isa" / "aarch64" / "regfile.rs"
EVOIR_GENERATED = ROOT / "crates" / "evocc-evoir" / "src" / "assembly_registers.rs"

spec = importlib.util.spec_from_file_location("generate_target", GENERATOR)
gen = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = gen
spec.loader.exec_module(gen)


def with_valid_digest(source: bytes) -> bytes:
    """Rebind a deliberate structural mutation without normalizing its rows."""
    lines = source.splitlines(keepends=True)
    positions = [index for index, line in enumerate(lines) if line.startswith(b"sha256 ")]
    if len(positions) != 1:
        raise AssertionError("the mutation fixture needs exactly one digest row")
    payload = b"".join(line for line in lines if not line.startswith(b"sha256 "))
    lines[positions[0]] = b"sha256 " + hashlib.sha256(payload).hexdigest().encode("ascii") + b"\n"
    return b"".join(lines)


def replace_one(source: bytes, old: bytes, new: bytes) -> bytes:
    if source.count(old) != 1:
        raise AssertionError(f"expected one fixture row: {old!r}")
    return with_valid_digest(source.replace(old, new, 1))


class RegisterDescriptionTest(unittest.TestCase):
    def setUp(self):
        self.original = DESCRIPTION.read_bytes()

    def decoded(self, source: bytes):
        description = gen.parse_description(source)
        gen.validate(description)
        return description

    def assert_structural_rejection(self, source: bytes, reason: str):
        self.assertNotEqual(source, self.original)
        digest_rows = [line for line in source.splitlines() if line.startswith(b"sha256 ")]
        self.assertEqual(len(digest_rows), 1)
        payload = b"".join(
            line for line in source.splitlines(keepends=True) if not line.startswith(b"sha256 ")
        )
        self.assertEqual(digest_rows[0], b"sha256 " + hashlib.sha256(payload).hexdigest().encode("ascii"))
        with self.assertRaisesRegex(gen.TargetDescriptionError, reason) as caught:
            self.decoded(source)
        self.assertNotIn("digest", str(caught.exception).lower())

    def test_checked_in_mir_and_evoir_outputs_regenerate_byte_for_byte(self):
        description = self.decoded(self.original)
        self.assertEqual(gen.render_mir(description).encode("utf-8"), MIR_GENERATED.read_bytes())
        self.assertEqual(
            gen.render_evoir_primitives(description).encode("utf-8"), EVOIR_GENERATED.read_bytes()
        )
        for path in [MIR_GENERATED, EVOIR_GENERATED]:
            self.assertTrue(path.read_bytes().startswith(b"// Generated"), str(path))

    def test_check_mode_is_read_only_and_accepts_the_same_generated_files(self):
        paths = [DESCRIPTION, MIR_GENERATED, EVOIR_GENERATED]
        before = {path: path.read_bytes() for path in paths}
        process = subprocess.run(
            [sys.executable, str(GENERATOR), "--check"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(process.returncode, 0, process.stdout + process.stderr)
        self.assertEqual({path: path.read_bytes() for path in paths}, before)

    def test_overlapping_physical_register_ids_are_not_only_a_digest_error(self):
        mutated = replace_one(self.original, b"bank simd 64 32\n", b"bank simd 16 32\n")
        self.assert_structural_rejection(mutated, "overlap")

    def test_duplicate_view_identity_is_not_only_a_digest_error(self):
        row = b"view gpr32 gpr 32 w\n"
        mutated = replace_one(self.original, row, row + row)
        self.assert_structural_rejection(mutated, "duplicate view")

    def test_declared_aliases_cannot_cross_physical_banks(self):
        mutated = replace_one(
            self.original,
            b"alias gpr32 gpr64\n",
            b"alias gpr32 simd32\n",
        )
        self.assert_structural_rejection(mutated, "alias.*bank")

    def test_reserved_register_cannot_enter_an_ordinary_allocator_pool(self):
        rows = [line for line in self.original.splitlines(keepends=True) if line.startswith(b"set CALLER_SAVED ")]
        self.assertEqual(len(rows), 1)
        row = rows[0]
        self.assertNotIn(b"16", row.split()[2:])
        mutated = replace_one(self.original, row, row.removesuffix(b"\n") + b" 16\n")
        self.assert_structural_rejection(mutated, "allocatable.*reserved")

    def test_wrong_digest_remains_a_separate_failure(self):
        rows = [line for line in self.original.splitlines(keepends=True) if line.startswith(b"sha256 ")]
        self.assertEqual(len(rows), 1)
        replacement = b"sha256 " + b"0" * 64 + b"\n"
        self.assertNotEqual(rows[0], replacement)
        mutated = self.original.replace(rows[0], replacement, 1)
        with self.assertRaisesRegex(gen.TargetDescriptionError, "digest"):
            self.decoded(mutated)


if __name__ == "__main__":
    unittest.main()
