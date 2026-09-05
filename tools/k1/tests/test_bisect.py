"""The bisection must land on a minimal failing set and never on a booting one."""

from __future__ import annotations

import importlib.util
import itertools
from pathlib import Path
import random
import unittest

# Loaded by path: the file is named for the job it does, and the standard
# library already owns the name ``bisect``.
_spec = importlib.util.spec_from_file_location(
    "k1_bisect", Path(__file__).resolve().parents[1] / "bisect.py"
)
k1_bisect = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(k1_bisect)
_partition = k1_bisect._partition
ddmin = k1_bisect.ddmin


def failing_on(guilty: set[str]):
    """A kernel that fails exactly when every unit of ``guilty`` is substituted."""
    calls: list[tuple[str, ...]] = []

    def fails(units):
        calls.append(tuple(units))
        return guilty.issubset(set(units))

    return fails, calls


class DdminTest(unittest.TestCase):
    def test_a_single_guilty_unit_is_isolated(self) -> None:
        units = [f"u{index}.o" for index in range(16)]
        fails, calls = failing_on({"u11.o"})
        self.assertEqual(ddmin(units, fails), ["u11.o"])
        # Halving, not a linear scan: a 16-unit list must not need 16 boots.
        self.assertLess(len(calls), 16)

    def test_every_position_is_isolated(self) -> None:
        units = [f"u{index}.o" for index in range(9)]
        for guilty in units:
            fails, _ = failing_on({guilty})
            self.assertEqual(ddmin(units, fails), [guilty])

    def test_an_interacting_pair_is_isolated(self) -> None:
        units = [f"u{index}.o" for index in range(12)]
        fails, _ = failing_on({"u2.o", "u9.o"})
        self.assertEqual(sorted(ddmin(units, fails)), ["u2.o", "u9.o"])

    def test_an_interacting_pair_in_the_same_half_is_isolated(self) -> None:
        units = [f"u{index}.o" for index in range(12)]
        fails, _ = failing_on({"u0.o", "u1.o"})
        self.assertEqual(sorted(ddmin(units, fails)), ["u0.o", "u1.o"])

    def test_a_random_guilty_set_is_isolated(self) -> None:
        rng = random.Random(20260905)
        units = [f"u{index}.o" for index in range(14)]
        for _ in range(20):
            size = rng.randint(1, 3)
            guilty = set(rng.sample(units, size))
            fails, _ = failing_on(guilty)
            self.assertEqual(set(ddmin(units, fails)), guilty)

    def test_the_result_is_one_minimal(self) -> None:
        units = [f"u{index}.o" for index in range(10)]
        fails, _ = failing_on({"u3.o", "u7.o"})
        minimal = ddmin(units, fails)
        self.assertTrue(fails(minimal))
        for dropped in minimal:
            smaller = [unit for unit in minimal if unit != dropped]
            self.assertFalse(fails(smaller), f"{smaller} still fails, so {minimal} is not minimal")

    def test_a_single_unit_list_is_returned_unexamined(self) -> None:
        fails, calls = failing_on({"only.o"})
        self.assertEqual(ddmin(["only.o"], fails), ["only.o"])
        self.assertEqual(calls, [])

    def test_the_input_order_is_preserved(self) -> None:
        units = ["b.o", "a.o", "c.o", "d.o"]
        fails, _ = failing_on({"a.o", "d.o"})
        self.assertEqual(ddmin(units, fails), ["a.o", "d.o"])


class PartitionTest(unittest.TestCase):
    def test_chunks_cover_the_input_exactly_once(self) -> None:
        for size in range(1, 20):
            units = [f"u{index}" for index in range(size)]
            for parts in range(1, size + 3):
                chunks = _partition(units, parts)
                self.assertEqual(list(itertools.chain.from_iterable(chunks)), units)

    def test_chunk_sizes_differ_by_at_most_one(self) -> None:
        chunks = _partition([f"u{index}" for index in range(10)], 4)
        sizes = sorted(len(chunk) for chunk in chunks)
        self.assertLessEqual(sizes[-1] - sizes[0], 1)
        self.assertEqual(len(chunks), 4)

    def test_more_parts_than_units_yields_one_unit_per_chunk(self) -> None:
        chunks = _partition(["a", "b"], 9)
        self.assertEqual(chunks, [["a"], ["b"]])


if __name__ == "__main__":
    unittest.main()
