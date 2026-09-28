"""The planning view must not hide open work or manufacture completion."""
from contextlib import redirect_stderr, redirect_stdout
import importlib.util
import io
from pathlib import Path
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("todo_status", Path(__file__).resolve().parents[2] / "render-status.py")
status = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(status)


def tracker():
    return """# Tracker
- [ ] **K01 — Kernel failure** — `OPEN`, P1.
- [ ] **L04 — Complex semantics** — `NEEDS_RULE`, P2.
  - [ ] **B08d10 — Word expansion** — `OPEN`, P2.
- [ ] **K13 — Full replay** — `RETEST`, P1.
- [ ] **R01 — Later proofs** — `LATER`, P3.
- [x] **B01 — Already reconciled** — `DONE`.
<!-- closure-queues:start -->
| Queue | Open IDs | Next deliverable |
| --- | --- | --- |
| KERNEL | K01 | Reduce the failure. |
| APPLICATION | B08d10 | Finish the patch. |
| REVIEW | L04 | Approve a rule. |
| AUDIT | | Audit changed paths. |
| ACCEPTANCE | K13 | Run only after prerequisites. |
| LATER | R01 | Prove the wider gate. |
<!-- closure-queues:end -->
"""


class TodoScheduleTests(unittest.TestCase):
    def test_retains_nested_entries_and_does_not_count_checked_work(self):
        output = status.render_todo(tracker())
        self.assertIn("5 open tracker entries", output)
        self.assertIn("including parents/children; not a bug count", output)
        self.assertIn("B08d10", output)
        self.assertIn("NEEDS_RULE", output)
        self.assertIn("RETEST", output)
        self.assertNotIn("B01", output)
        self.assertTrue(output.isascii())

    def test_missing_new_task_cannot_disappear_from_schedule(self):
        with self.assertRaisesRegex(ValueError, "unscheduled.*T99"):
            status.render_todo(tracker() + '\n- [ ] **T99 — New gap** — `OPEN`, P1.\n')

    def test_rejects_duplicate_unknown_and_completed_scheduled_ids(self):
        for changed in ("K01, K01", "K01, T99", "K01, B01"):
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                status.render_todo(tracker().replace("| KERNEL | K01 |", f"| KERNEL | {changed} |"))
        with self.assertRaisesRegex(ValueError, "duplicate task"):
            status.render_todo(tracker() + '\n- [ ] **K01 — Duplicate** — `OPEN`, P1.\n')

    def test_unknown_state_and_review_in_execution_queue_are_errors(self):
        with self.assertRaisesRegex(ValueError, "state"):
            status.render_todo(tracker().replace("`OPEN`", "`RUNNING`", 1))
        changed = tracker().replace("| KERNEL | K01 |", "| KERNEL | K01, L04 |")
        changed = changed.replace("| REVIEW | L04 |", "| REVIEW | |")
        with self.assertRaisesRegex(ValueError, "REVIEW"):
            status.render_todo(changed)

    def test_missing_or_repeated_schedule_markers_are_errors(self):
        for changed in (tracker().replace("<!-- closure-queues:end -->", ""), tracker() + tracker()):
            with self.assertRaises(ValueError):
                status.render_todo(changed)

    def test_real_tracker_accounts_for_every_open_entry(self):
        output = status.render_todo((status.ROOT / "TODO.md").read_text())
        self.assertIn("K01b", output)
        self.assertIn("B08d10", output)
        self.assertIn("L11c", output)

    def test_cli_only_reads_tracker_no_receipts_or_writes(self):
        with patch.object(status, "load_receipts", side_effect=AssertionError("must not read receipts")), \
                patch.object(Path, "write_text", side_effect=AssertionError("must not write")), \
                redirect_stdout(io.StringIO()) as output:
            self.assertEqual(status.main(["--todo"]), 0)
        self.assertIn("not a live process monitor", output.getvalue())

    def test_invalid_queue_cli_fails_without_reading_receipts(self):
        with patch.object(status, "load_receipts", side_effect=AssertionError("must not read receipts")), \
                patch.object(Path, "read_text", return_value=tracker().replace("K01 |", "K99 |")), \
                redirect_stderr(io.StringIO()) as errors:
            self.assertEqual(status.main(["--todo"]), 1)
        self.assertIn("unknown or already checked: K99", errors.getvalue())


if __name__ == "__main__":
    unittest.main()
