"""Small REDs for the September 30 timing/preflight review."""

import unittest
from pathlib import Path
from tools.ladder.eqsat_pilot import ub_guard_sites
from tools.ladder.eqsat_pilot_audit import audited_guard_sites
from tools.ladder.eqsat_census import live_process_group
from tools.ladder.eqsat_preflight_run import fixed_template_attempt
from tools.ladder.eqsat_native import CommandExitError


class GuardSiteTests(unittest.TestCase):
    def test_every_ub_reason_has_a_counted_native_guard_site(self):
        for immediate in (3002, 3003, 3004, 3005, 3006, 3022, 3063):
            with self.subTest(reason=immediate):
                self.assertEqual(ub_guard_sites(f'    brk #{immediate}\n'.encode()), 1)
                self.assertEqual(audited_guard_sites(f'    brk #{immediate}\n'.encode()), 1)

    def test_unrelated_traps_and_comments_are_not_ub_guard_sites(self):
        self.assertEqual(ub_guard_sites(b'    brk #1000\n    brk #3064\n// brk #3004\n'), 0)
        self.assertEqual(audited_guard_sites(b'    brk #1000\n    brk #3064\n// brk #3004\n'), 0)


class ProcessGroupTests(unittest.TestCase):
    def test_zombies_do_not_make_a_completed_group_appear_live(self):
        self.assertEqual(live_process_group('10 20 Z\n11 20 Zs\n12 20 X\n13 21 S\n', 20), [])

    def test_sleeping_running_and_stopped_descendants_are_still_live(self):
        self.assertEqual(live_process_group('10 20 R+\n11 20 S\n12 20 T\n13 20 Z\n', 20), [10, 11, 12])

    def test_failed_or_malformed_process_observation_is_not_completion(self):
        for snapshot in ('10 20\n', 'PID PGID STAT\n', '10 xx Z\n'):
            with self.subTest(snapshot=snapshot), self.assertRaises(ValueError):
                live_process_group(snapshot, 20)

    def test_unknown_states_are_conservatively_live_only_in_the_owned_group(self):
        self.assertEqual(live_process_group('10 20 ?\n11 21 ?\n', 20), [10])


class TemplateClassificationTests(unittest.TestCase):
    def test_only_completed_command_exit_is_proof_unknown(self):
        def command(*args):
            raise CommandExitError('Lean exited 1')
        proof = fixed_template_attempt('candidate', 'fixed', [], Path('/unit'), command)
        self.assertEqual(proof['status'], 'UNKNOWN')

    def test_integrity_launch_and_resource_failures_abort_the_preflight(self):
        for error in (ValueError('retention limit'), ValueError('artifact changed'),
                      ValueError('watchdog expired'), OSError('process launch failed')):
            def command(*args):
                raise error
            with self.subTest(error=error), self.assertRaises(type(error)):
                fixed_template_attempt('candidate', 'fixed', [], Path('/unit'), command)

    def test_successful_command_with_missing_theorem_is_integrity_failure(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as temp:
            directory = Path(temp)
            (directory/'stdout').write_text('nothing proved\n')
            with self.assertRaisesRegex(ValueError, 'exact theorem'):
                fixed_template_attempt('candidate', 'fixed', [], directory,
                                       lambda *args: dict(stdout='stdout'))


if __name__ == '__main__':
    unittest.main()
