"""R02b bounded selected-pair inventory and preflight verdicts.

The inequality enumerator covers only the supplied consulted comparisons.
New comparisons on another policy/transaction invalidate that population's
coverage. Enumerating vectors does not run an optimizer or prove a rewrite.
"""
from __future__ import annotations

import itertools
import json
from pathlib import Path

from .eqsat_chain import AXES, digest, keys, natural, require, unique


def selected_pairs(path, transaction, weights):
    raw = Path(path).read_bytes()
    trace = json.loads(raw, object_pairs_hook=unique)
    keys(trace, 'schema authority input_digest decisions')
    require(trace['schema'] == 'evocc-selected-pairs-v1'
            and trace['authority'] == 'PRODUCER_TELEMETRY_NOT_PROVED'
            and trace['input_digest'] == transaction['input_digest'], 'unbound selected-pair telemetry')
    require(type(trace['decisions']) is list, 'missing selected-pair population')
    pairs, rejected = set(), 0
    cost = dict(zip(AXES, weights))
    cost[None] = 0
    for row in trace['decisions']:
        keys(row, 'function_index block root root_axis replacement_axis root_cost replacement_cost registry_admitted kept')
        for field in ('function_index', 'block', 'root', 'root_cost', 'replacement_cost'):
            natural(row[field])
        require(row['root_axis'] in cost and row['replacement_axis'] in cost
                and type(row['registry_admitted']) is bool and type(row['kept']) is bool,
                'invalid selected-pair gate')
        require(row['root_cost'] == cost[row['root_axis']]
                and row['replacement_cost'] == cost[row['replacement_axis']], 'incorrect policy costs')
        require(row['kept'] == (row['replacement_cost'] < row['root_cost'] and row['registry_admitted']),
                'incorrect selected-pair decision')
        if row['registry_admitted']:
            pairs.add((row['root_axis'], row['replacement_axis']))
        rejected += not row['kept']
    require(len(trace['decisions']) == transaction['roots']['proposed']-
            transaction['roots']['declined_by_reason']['witness']
            and rejected == transaction['roots']['declined_by_reason']['policy'], 'trace denominator mismatch')
    return dict(digest=digest(raw), comparisons=sorted(pairs, key=str), gates=len(trace['decisions']))


def feasible_vector(comparisons, outcomes):
    """Solve x_dst >= x_src + delta with a fixed zero constant.

    Longest paths yield a nonnegative integer witness or detect a positive
    cycle. Zero is fixed, so a path demanding zero >= positive is infeasible.
    This covers strict/non-strict gates, not arbitrary numeric costs.
    """
    nodes = {None} | {axis for pair in comparisons for axis in pair}
    values = {node: 0 for node in nodes}
    edges = [(rhs, lhs, 1) if keep else (lhs, rhs, 0)
             for (lhs, rhs), keep in zip(comparisons, outcomes)]
    for _ in range(len(nodes)):
        changed = False
        for source, dest, delta in edges:
            needed = values[source] + delta
            if needed > values[dest]:
                if dest is None:
                    return None
                values[dest] = needed
                changed = True
        if not changed:
            vector = [values.get(axis, 1) for axis in AXES]
            lookup = dict(zip(AXES, vector)); lookup[None] = 0
            require(all((lookup[a] > lookup[b]) == keep
                        for (a, b), keep in zip(comparisons, outcomes)), 'enumerator witness invalid')
            return vector
    return None


def enumerate_policies(comparisons, signature_budget):
    require(type(signature_budget) is int and 1 <= signature_budget <= 1_048_576, 'unbounded policy enumeration')
    pairs = sorted(set(tuple(p) for p in comparisons), key=str)
    require(all(a in (*AXES, None) and b in (*AXES, None) for a, b in pairs), 'unknown comparison axis')
    candidates = []
    visited = 0
    for signature in itertools.product((False, True), repeat=len(pairs)):
        if visited == signature_budget:
            return dict(status='ENUM_BUDGET_EXHAUSTED', comparisons=pairs, signatures_visited=visited,
                        candidates=candidates, scope='CONSULTED_PAIRS_ONLY_NOT_UNIVERSAL_POLICY_SPACE')
        visited += 1
        vector = feasible_vector(pairs, signature)
        if vector is not None:
            candidates.append(dict(signature=list(signature), weights=vector))
    return dict(status='ENUM_COMPLETE', comparisons=pairs, signatures_visited=visited,
                candidates=candidates, scope='CONSULTED_PAIRS_ONLY_NOT_UNIVERSAL_POLICY_SPACE')


def rule_verdict(cell, *, ai_budget, multiplier, useful_proved_witness, unresolved):
    """A resource stop cannot manufacture discovery-searchability."""
    keys(cell, 'status allowed consumed')
    require(cell['status'] in ('ENUM_COMPLETE', 'ENUM_BUDGET_EXHAUSTED', 'ENUM_INCOMPLETE_ERROR'),
            'unknown enumeration termination')
    natural(cell['allowed']); natural(cell['consumed'])
    require(cell['consumed'] <= cell['allowed'], 'enumeration exceeded its frozen cap')
    require(type(useful_proved_witness) is bool and type(unresolved) is bool, 'invalid witness evidence')
    if unresolved or cell['status'] == 'ENUM_INCOMPLETE_ERROR':
        return 'INCONCLUSIVE'
    if cell['status'] == 'ENUM_COMPLETE':
        return 'FINITE_ENUMERATION_SUFFICIENT' if useful_proved_witness else 'NO_OBSERVABLE_EFFECT'
    if (natural(ai_budget) == 0 or natural(multiplier) < 1
            or cell['allowed'] < ai_budget * multiplier or cell['consumed'] != cell['allowed']):
        return 'INCONCLUSIVE'
    return 'SEARCHABLE' if useful_proved_witness else 'INCONCLUSIVE'
