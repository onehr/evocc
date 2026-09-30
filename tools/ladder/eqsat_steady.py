"""Linux-specific steady-state R02 timing, separate from deterministic preflight.

The registered harness excludes setup/startup, repeats in process, uses a guest
CPU affinity and captures one quiet batch. A/A selects repeats and sample counts
before a separately committed A/B plan. Millisecond fresh-process data is never
promoted to this protocol or used to choose a candidate.
"""
from .eqsat_chain import require


def run_plan(path, root, mode):
    require(mode in ('build', 'AA', 'AB'), 'unknown steady-state mode')
    raise ValueError('R02a1 steady-state runner is not yet qualified; no timing execution admitted')
