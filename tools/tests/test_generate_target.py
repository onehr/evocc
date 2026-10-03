"""N03a register-description mutations and checked-in regeneration.

Each structural mutation carries its own correct digest. A digest rejection
therefore cannot substitute for checking the register relationship under test.
"""

from __future__ import annotations

import hashlib
import importlib.util
import os
import subprocess
import sys
import tempfile
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


class TargetGenerationWritesTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="evocc-target-noop-")
        self.addCleanup(self.directory.cleanup)
        folder = Path(self.directory.name)
        self.outputs = {
            "--output": folder / "regfile.rs",
            "--evoir-output": folder / "assembly_registers.rs",
            "--insts-output": folder / "insts.rs",
            "--abi-output": folder / "abi_tables.rs",
            "--patterns-output": folder / "patterns.rs",
            "--costs-output": folder / "costs.rs",
            "--encodings-output": folder / "encodings.rs",
            "--pattern-docs": folder / "evomir-spec.md",
        }
        self.outputs["--pattern-docs"].write_bytes((ROOT / "docs/evomir-spec.md").read_bytes())
        self.generate()
        for path in self.outputs.values():
            os.utime(path, ns=(946684800000000000, 946684800000000000))
        self.before = {path: (path.read_bytes(), path.stat().st_mtime_ns)
                       for path in self.outputs.values()}

    def generate(self):
        argv = [sys.executable, str(GENERATOR)]
        for option, path in self.outputs.items():
            argv.extend([option, str(path)])
        result = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def assert_unchanged(self, except_path=None):
        for path, (contents, mtime) in self.before.items():
            with self.subTest(path=path.name):
                self.assertEqual(path.read_bytes(), contents)
                if path != except_path:
                    self.assertEqual(path.stat().st_mtime_ns, mtime)

    def test_no_op_generation_preserves_all_output_bytes_and_mtimes(self):
        self.generate()
        self.assert_unchanged()

    def test_only_corrupt_output_is_rewritten(self):
        broken = self.outputs["--output"]
        broken.write_bytes(b"deliberately corrupted output\n")
        self.generate()
        self.assert_unchanged(except_path=broken)

    def test_only_missing_output_is_created(self):
        missing = self.outputs["--insts-output"]
        missing.unlink()
        self.generate()
        self.assert_unchanged(except_path=missing)


class EncodingDescriptionTest(unittest.TestCase):
    def source(self):
        return with_valid_digest(
            b"version encodings.v1\ntarget aarch64\nsha256 " + b"0" * 64 + b"\n"
            b"field add_sub unsigned 12\n"
            b"field memory_unscaled signed 9\n"
            b"field memory_scaled unsigned 12\n"
            b"field pair signed 7\n"
            b"field mov_wide unsigned 16\n"
            b"field tbz signed 14\n"
            b"field adr signed 21\n"
            b"shift add_sub 12\nshift mov_wide 16\n"
            b"scale tbz 4\nscale adr 1\ninstruction bytes 4\n"
            b"set memory_bytes 1,2,4,8,16\nset pair_bytes 4,8,16\n"
            b"set logical_elements 2,4,8,16,32,64\n"
            b"binary add_sub add,sub\nbinary logical and,orr,eor\n"
            b"binary shift lsl,lsr,asr\n"
        )

    def test_encoding_facts_have_one_checked_in_generated_owner(self):
        source = self.source()
        self.assertEqual(source, (ROOT / "targets/aarch64/encodings").read_bytes())
        rendered = gen.render_encodings(gen.parse_encodings(source))
        self.assertEqual(rendered, (ROOT / "crates/evocc-mir/src/isa/aarch64/encodings.rs").read_text())
        self.assertTrue(rendered.startswith("// Generated"))
        for consumer in ["lower.rs", "validate.rs", "emit.rs"]:
            self.assertTrue("encodings::" in (ROOT / "crates/evocc-mir/src" / consumer).read_text(), consumer)

    def test_malformed_encoding_rows_fail_after_digest_rebinding(self):
        source = self.source()
        for old, new in [
            (b"field add_sub unsigned 12", b"field add_sub unsigned 0"),
            (b"field pair signed 7", b"field pair unsigned 7"),
            (b"field mov_wide unsigned 16", b"field mov_wide unsigned 65"),
            (b"scale tbz 4", b"scale tbz 3"),
            (b"set memory_bytes 1,2,4,8,16", b"set memory_bytes 1,2,2,8,16"),
            (b"set logical_elements 2,4,8,16,32,64", b"set logical_elements 2,4,8,16,31,64"),
            (b"binary add_sub add,sub", b"binary add_sub add,mul"),
            (b"binary shift lsl,lsr,asr", b"binary shift lsl,lsr,add"),
            (b"field adr signed 21\n", b""),
            (b"scale adr 1\n", b"scale adr 1\nscale adr 1\n"),
        ]:
            with self.subTest(new=new), self.assertRaises(gen.TargetDescriptionError):
                gen.parse_encodings(with_valid_digest(source.replace(old, new, 1)))


class ConstantPatternDescriptionTest(unittest.TestCase):
    def source(self):
        return with_valid_digest(
            b"version patterns.v1\ntarget aarch64\nsha256 " + b"0" * 64 + b"\n"
            b"pattern const_movz root=mov_imm widths=32,64 sequence=movz_movk guard=any writes=dst flags=preserve memory=none claim=exact_bits_zero_upper_w\n"
            b"pattern const_movn root=mov_imm widths=32,64 sequence=movn_movk guard=any writes=dst flags=preserve memory=none claim=exact_bits_zero_upper_w\n"
            b"pattern const_orr root=mov_imm widths=32,64 sequence=orr_zero guard=logical_mask writes=dst flags=preserve memory=none claim=exact_bits_zero_upper_w\n"
        )

    def test_constant_patterns_generate_candidates_and_claim_rows(self):
        description = gen.parse_patterns(self.source())
        rust = gen.render_patterns(description)
        for name in ["const_movz", "const_movn", "const_orr"]:
            self.assertIn(name, rust)
        self.assertIn("MovzMovk", rust)
        self.assertIn("MovnMovk", rust)
        self.assertIn("OrrZero", rust)
        self.assertIn("exact_bits_zero_upper_w", gen.render_pattern_docs(description))

    def test_rebound_unsafe_patterns_are_rejected(self):
        source = self.source()
        for old, new in [
            (b"widths=32,64", b"widths=16,64"),
            (b"writes=dst", b"writes=dst,scratch"),
            (b"flags=preserve", b"flags=write"),
            (b"memory=none", b"memory=write"),
            (b"guard=logical_mask", b"guard=any"),
            (b"claim=exact_bits_zero_upper_w", b"claim=low_bits_only"),
        ]:
            with self.subTest(new=new):
                changed = with_valid_digest(source.replace(old, new, 1))
                with self.assertRaises(gen.TargetDescriptionError):
                    gen.parse_patterns(changed)

    def test_pattern_order_and_enabled_families_are_data(self):
        lines = self.source().splitlines(keepends=True)
        selected = with_valid_digest(b"".join(lines[:3] + [lines[5], lines[3]]))
        rendered = gen.render_patterns(gen.parse_patterns(selected))
        self.assertLess(rendered.index('id: "const_orr"'), rendered.index('id: "const_movz"'))
        self.assertNotIn('id: "const_movn"', rendered)

    def test_costs_count_actual_instruction_bytes_not_guessed_profit(self):
        source = with_valid_digest(
            b"version costs.v1\ntarget aarch64\nsha256 " + b"0" * 64 + b"\n"
            b"bytes mov_wide 4\nbytes logical_immediate 4\n"
        )
        self.assertIn("WIDE_BYTES: u32 = 4", gen.render_costs(gen.parse_costs(source)))
        for value in [b"0", b"1", b"8", b"-1"]:
            changed = with_valid_digest(source.replace(b"mov_wide 4", b"mov_wide " + value))
            with self.assertRaises(gen.TargetDescriptionError):
                gen.parse_costs(changed)


class LocalPatternDescriptionTest(unittest.TestCase):
    def source(self):
        return (ROOT / "targets/aarch64/patterns").read_bytes()

    def test_local_families_are_generated_not_an_unconditional_emitter_switch(self):
        source = self.source()
        description = gen.parse_patterns(source)
        local = [row for row in description.rows if row["id"].startswith("local_")]
        self.assertEqual(len(local), 6)
        rendered = gen.render_patterns(description)
        self.assertIn('ops: &["add", "sub", "and", "orr", "eor"]', rendered)
        self.assertLess(rendered.index('id: "local_indexed_memory"'), rendered.index('id: "local_shift_arithmetic"'))
        without = with_valid_digest(b"".join(line for line in source.splitlines(keepends=True)
                                             if not line.startswith(b"pattern local_shift_arithmetic ")))
        self.assertNotIn('id: "local_shift_arithmetic"', gen.render_patterns(gen.parse_patterns(without)))

    def test_local_domain_mutations_fail_after_digest_rebinding(self):
        source = self.source()
        for old, new in [
            (b"ops=add,sub,and,orr,eor", b"ops=add,sub,and,orr,eor,sdiv"),
            (b"shifts=lsl,lsr,asr", b"shifts=lsl,lsr,asr,ror"),
            (b"guard=adjacent_single_use_physical_safe", b"guard=adjacent"),
            (b"memory=same_access", b"memory=none"),
            (b"flags=original_cmp", b"flags=preserve"),
            (b"root=address_memory widths=64", b"root=address_memory widths=32,64"),
            (b"claim=equal_live_state", b"claim=equal_all_registers"),
        ]:
            with self.subTest(new=new):
                with self.assertRaises(gen.TargetDescriptionError):
                    gen.parse_patterns(with_valid_digest(source.replace(old, new, 1)))


class ExtensionPatternDescriptionTest(unittest.TestCase):
    def test_three_bounded_families_are_generated_and_can_be_disabled(self):
        source = (ROOT / "targets/aarch64/patterns").read_bytes()
        description = gen.parse_patterns(source)
        rows = [row for row in description.rows if row["id"].startswith("reuse_")]
        self.assertEqual(len(rows), 3)
        rendered = gen.render_patterns(description)
        for name in ["Extend", "LowExtract", "LowMask"]:
            self.assertIn("ExtensionKind::" + name, rendered)
        without = with_valid_digest(b"".join(
            line for line in source.splitlines(keepends=True)
            if not line.startswith(b"pattern reuse_low_mask ")))
        self.assertNotIn('id: "reuse_low_mask"', gen.render_patterns(gen.parse_patterns(without)))

    def test_rebound_extension_domains_cannot_weaken_reuse_preconditions(self):
        source = (ROOT / "targets/aarch64/patterns").read_bytes()
        old_row = next(line for line in source.splitlines() if line.startswith(b"pattern reuse_integer_extend "))
        for old, new in [
            (b"guard=block_local_unique_exact", b"guard=block_local"),
            (b"writes=none", b"writes=dst"),
            (b"flags=preserve", b"flags=write"),
            (b"memory=none", b"memory=read"),
            (b"widths=32,64", b"widths=16,32,64"),
            (b"claim=exact_value_reuse", b"claim=low_bits_only"),
            (b"root=extend", b"root=arbitrary_pure"),
        ]:
            with self.subTest(new=new), self.assertRaises(gen.TargetDescriptionError):
                gen.parse_patterns(with_valid_digest(source.replace(old_row, old_row.replace(old, new))))


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


class InstructionDescriptionTest(unittest.TestCase):
    def setUp(self):
        self.path = ROOT / "targets/aarch64/insts"
        self.generated = ROOT / "crates/evocc-mir/src/isa/aarch64/insts.rs"
        self.original = self.path.read_bytes()

    def decoded(self, source):
        description = gen.parse_instructions(source)
        gen.validate_instructions(description)
        return description

    def mutate_row(self, prefix, change):
        rows = [row for row in self.original.splitlines(keepends=True) if row.startswith(prefix)]
        self.assertEqual(len(rows), 1)
        return replace_one(self.original, rows[0], change(rows[0]))

    def assert_structural_rejection(self, source, reason):
        # These mutations must pass the digest boundary before testing facts.
        payload = b"".join(row for row in source.splitlines(keepends=True) if not row.startswith(b"sha256 "))
        stated = [row for row in source.splitlines() if row.startswith(b"sha256 ")]
        self.assertEqual(stated, [b"sha256 " + hashlib.sha256(payload).hexdigest().encode("ascii")])
        with self.assertRaisesRegex(gen.TargetDescriptionError, reason) as caught:
            self.decoded(source)
        self.assertNotIn("digest", str(caught.exception).lower())

    def test_instruction_dispatch_regenerates_byte_for_byte(self):
        self.assertEqual(gen.render_instructions(self.decoded(self.original)).encode(), self.generated.read_bytes())
        self.assertTrue(self.generated.read_bytes().startswith(b"// Generated"))

    def test_duplicate_instruction_family_is_rejected_with_valid_digest(self):
        changed = self.mutate_row(b"inst Mov ", lambda row: row + row)
        self.assert_structural_rejection(changed, "duplicate.*family")

    def test_missing_terminator_family_is_rejected_with_valid_digest(self):
        changed = self.mutate_row(b"term Indirect ", lambda row: b"")
        self.assert_structural_rejection(changed, "missing.*family")

    def test_unknown_operand_field_is_rejected_with_valid_digest(self):
        changed = self.mutate_row(b"inst Mov ", lambda row: row.replace(b"uses=src ", b"uses=missing ", 1))
        self.assert_structural_rejection(changed, "operand.*(field|selector)")

    def test_scalar_operand_cannot_be_treated_as_a_vector(self):
        changed = self.mutate_row(b"inst Mov ", lambda row: row.replace(b"defs=dst ", b"defs=dst* ", 1))
        self.assert_structural_rejection(changed, "operand.*(shape|selector)")

    def test_unsupported_flag_fact_is_rejected_with_valid_digest(self):
        changed = self.mutate_row(b"inst Mov ", lambda row: row.replace(b"flags=false ", b"flags=maybe ", 1))
        self.assert_structural_rejection(changed, "flags")

    def test_dce_fact_mutation_changes_the_generated_consumer(self):
        # A syntactically valid changed fact is not hidden by a handwritten
        # duplicate. Rust's frozen conformance vectors reject its old meaning.
        changed = self.mutate_row(b"inst Mov ", lambda row: row.replace(b"discardable=true ", b"discardable=false ", 1))
        before = gen.render_instructions(self.decoded(self.original))
        after = gen.render_instructions(self.decoded(changed))
        def body(rendered):
            return rendered.split("fn discardable(&self) -> bool {", 1)[1].split("\n    }", 1)[0]
        self.assertNotEqual(body(before), body(after))

    def test_return_operand_collection_does_not_shadow_the_values_field(self):
        rendered = gen.render_instructions(self.decoded(self.original))
        self.assertFalse("let mut values =" in rendered, "temporary shadows the Ret.values field")
        self.assertTrue("collected_operands.extend(values.iter().copied());" in rendered)
        self.assertTrue("collected_operands.extend(values.iter_mut());" in rendered)
        self.assertNotEqual(gen.collection_name("values", "values*"), "values")

    def test_indirect_edge_collection_does_not_shadow_the_edges_field(self):
        rendered = gen.render_instructions(self.decoded(self.original))
        self.assertFalse("let mut edges =" in rendered, "temporary shadows the Indirect.edges field")
        self.assertTrue("collected_edges.extend(edges.iter().map(" in rendered)
        self.assertNotEqual(gen.collection_name("edges", "edges*"), "edges")


class AbiDescriptionTest(unittest.TestCase):
    """N03a step 3: per-OS ABI facts and the kernel profile, with valid digests."""

    def setUp(self):
        self.regfile = gen.parse_description(DESCRIPTION.read_bytes())
        gen.validate(self.regfile)
        self.paths = {
            "darwin": ROOT / "targets/aarch64/abi-darwin",
            "linux": ROOT / "targets/aarch64/abi-linux",
        }
        self.sources = {name: path.read_bytes() for name, path in self.paths.items()}
        self.profile_path = ROOT / "targets/aarch64/profile-kernel"
        self.profile = self.profile_path.read_bytes()
        self.generated = ROOT / "crates/evocc-mir/src/isa/aarch64/abi_tables.rs"

    def decoded_abi(self, source):
        abi = gen.parse_abi(source)
        gen.validate_abi(abi, self.regfile)
        return abi

    def decoded_profile(self, source):
        abis = {name: self.decoded_abi(body) for name, body in self.sources.items()}
        profile = gen.parse_profile(source)
        gen.validate_profile(profile, abis)
        return profile

    def assert_structural_rejection(self, source, decode, reason):
        payload = b"".join(row for row in source.splitlines(keepends=True) if not row.startswith(b"sha256 "))
        stated = [row for row in source.splitlines() if row.startswith(b"sha256 ")]
        self.assertEqual(stated, [b"sha256 " + hashlib.sha256(payload).hexdigest().encode("ascii")])
        with self.assertRaisesRegex(gen.TargetDescriptionError, reason) as caught:
            decode(source)
        self.assertNotIn("digest", str(caught.exception).lower())

    def mutate(self, source, old, new):
        return replace_one(source, old, new)

    def test_abi_tables_regenerate_byte_for_byte(self):
        abis = [self.decoded_abi(self.sources[name]) for name in ("darwin", "linux")]
        profiles = [self.decoded_profile(self.profile)]
        rendered = gen.render_abi(self.regfile, abis, profiles).encode("utf-8")
        self.assertEqual(rendered, self.generated.read_bytes())
        self.assertTrue(self.generated.read_bytes().startswith(b"// Generated"))

    def test_the_two_abis_differ_only_in_named_rules(self):
        darwin, linux = (self.decoded_abi(self.sources[name]) for name in ("darwin", "linux"))
        differing = {key for key in darwin.facts if darwin.facts[key] != linux.facts[key]}
        self.assertEqual(differing, {"abi", "stack-arguments", "aligned-record-pair-rounding"})

    def test_argument_registers_cannot_exceed_their_bank(self):
        changed = self.mutate(self.sources["darwin"], b"arguments simd 8\n", b"arguments simd 33\n")
        self.assert_structural_rejection(changed, self.decoded_abi, "exceed.*bank")

    def test_unknown_register_set_reference_is_rejected(self):
        changed = self.mutate(self.sources["linux"], b"callee-saved @CALLEE_SAVED\n", b"callee-saved @CALLEE_SAVD\n")
        self.assert_structural_rejection(changed, self.decoded_abi, "unknown.*set")

    def test_callee_saved_registers_cannot_be_call_clobbered(self):
        changed = self.mutate(self.sources["linux"], b"callee-saved @CALLEE_SAVED\n", b"callee-saved @CALLEE_SAVED 0\n")
        self.assert_structural_rejection(changed, self.decoded_abi, "callee-saved.*clobber")

    def test_argument_registers_cannot_be_reserved(self):
        changed = self.mutate(self.sources["darwin"], b"indirect-result gpr 8\n", b"indirect-result gpr 16\n")
        self.assert_structural_rejection(changed, self.decoded_abi, "reserved")

    def test_missing_fact_is_rejected(self):
        changed = self.mutate(self.sources["darwin"], b"va-list vector-offset 28\n", b"")
        self.assert_structural_rejection(changed, self.decoded_abi, "missing")

    def test_duplicate_fact_is_rejected(self):
        changed = self.mutate(self.sources["linux"], b"stack-slot-bytes 8\n", b"stack-slot-bytes 8\nstack-slot-bytes 8\n")
        self.assert_structural_rejection(changed, self.decoded_abi, "duplicate")

    def test_stack_alignment_must_be_whole_slots(self):
        changed = self.mutate(self.sources["linux"], b"stack-alignment-bytes 16\n", b"stack-alignment-bytes 12\n")
        self.assert_structural_rejection(changed, self.decoded_abi, "alignment")

    def test_stale_register_description_revision_is_rejected(self):
        row = b"regfile " + self.regfile.digest.encode("ascii") + b"\n"
        changed = self.mutate(self.sources["darwin"], row, b"regfile " + b"1" * 64 + b"\n")
        self.assert_structural_rejection(changed, self.decoded_abi, "register description revision")

    def test_profile_feature_must_be_known(self):
        changed = self.mutate(self.profile, b"host-only tls\n", b"host-only tlss\n")
        self.assert_structural_rejection(changed, self.decoded_profile, "unknown.*feature")

    def test_profile_host_abi_revision_must_be_current(self):
        rows = [row for row in self.profile.splitlines(keepends=True) if row.startswith(b"abi linux ")]
        self.assertEqual(len(rows), 1)
        changed = self.mutate(self.profile, rows[0], b"abi linux " + b"2" * 64 + b"\n")
        self.assert_structural_rejection(changed, self.decoded_profile, "ABI revision")

    def test_wrong_abi_digest_remains_a_separate_failure(self):
        rows = [row for row in self.sources["linux"].splitlines(keepends=True) if row.startswith(b"sha256 ")]
        self.assertEqual(len(rows), 1)
        mutated = self.sources["linux"].replace(rows[0], b"sha256 " + b"0" * 64 + b"\n", 1)
        with self.assertRaisesRegex(gen.TargetDescriptionError, "digest"):
            self.decoded_abi(mutated)


if __name__ == "__main__":
    unittest.main()
