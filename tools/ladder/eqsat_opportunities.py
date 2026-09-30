"""Bounded diagnostic expressions and a finite, explicitly narrow rule grammar.

Nothing here authorizes an EvoIR rewrite. LLVM output is an opportunity hint;
fuzz survivors are unproved. The only native artifacts remain checker-admitted.
"""
from __future__ import annotations

import itertools
import json
import re

from .eqsat_chain import digest, keys, natural, require
from .eqsat_twins import canonical

OPS = ('add', 'sub', 'mul', 'and', 'or', 'xor')
LEAVES = (('x',), ('c', 0), ('c', 1))


def grammar_terms(nodes):
    """Ordered trees, no commutativity quotient or DAG sharing; one variable."""
    require(type(nodes) is int and 0 <= nodes <= 3, 'unfrozen grammar size')
    levels = [list(LEAVES)]
    for n in range(1, nodes+1):
        levels.append([(op, left, right) for op in OPS for i in range(n)
                       for left in levels[i] for right in levels[n-1-i]])
    return levels[nodes]


def evaluate(term, width, x):
    mask = (1 << width)-1
    op = term[0]
    if op == 'x':
        return x & mask
    if op == 'c':
        return term[1] & mask
    a, b = (evaluate(child, width, x) for child in term[1:])
    return {'add': lambda: a+b, 'sub': lambda: a-b, 'mul': lambda: a*b,
            'and': lambda: a&b, 'or': lambda: a|b, 'xor': lambda: a^b}[op]() & mask


def enumerate_rule_cell(nodes, *, syntax_cap):
    require(type(syntax_cap) is int and syntax_cap > 0, 'missing syntactic bound')
    # The common full-suite evaluator is a later qualification step. Syntax
    # visits and this deterministic filter must never count toward B_AI.
    candidates, visited, counterexamples = [], 0, 0
    population = grammar_terms(nodes)
    vectors = [(width, x) for width in (1, 2, 3, 8, 16, 32, 64)
               for x in sorted({0, 1, 2, 3, (1 << width)-1, (1 << (width-1)),
                                ((1 << width)-1)//3, ((1 << width)-1)//5})]
    for lhs, rhs in itertools.product(population, LEAVES):
        if visited == syntax_cap:
            return dict(status='ENUM_INCOMPLETE_ERROR', reason='SYNTAX_CAP_NOT_EVALUATION_CAP',
                        visits=visited, frontier=len(population)*len(LEAVES)-visited,
                        survivors=candidates, counterexamples=counterexamples)
        visited += 1
        if any(evaluate(lhs, w, x) != evaluate(rhs, w, x) for w, x in vectors):
            counterexamples += 1
        else:
            candidates.append(dict(lhs=lhs, rhs=rhs, proof='NOT_ATTEMPTED',
                candidate_digest=digest(canonical([lhs, rhs])), relation='eq'))
    return dict(status='ENUM_COMPLETE', visits=visited, frontier=0, survivors=candidates,
                counterexamples=counterexamples,
                coverage=dict(ordered_lhs_trees=len(population), rhs_terms=len(LEAVES),
                    lhs_operator_nodes=nodes, rhs_operator_nodes=0,
                    variable_count=1, sharing='TREE_NO_DAG', guard='true',
                    width='SYMBOLIC_N_UNIVERSAL_PROOF_REQUIRED',
                    syntax_digest=digest(canonical(population))),
                filtering='FINITE_COUNTEREXAMPLES_NOT_PROOF')


def lean_candidate(lhs, rhs):
    """Trusted fixed all-width template; no candidate-provided tactic script."""
    def pat(term):
        if term[0] == 'x':
            return '(.var "x")'
        if term[0] == 'c':
            require(term[1] in (0, 1), 'unknown constant')
            return f'(.constLike "x" {term[1]})'
        require(term[0] in OPS and len(term) == 3, 'unrecognized candidate AST')
        return f'(.bin .{term[0]} false {pat(term[1])} {pat(term[2])})'
    return ('import EvoIR.Proofs.Rules\nnamespace EvoIR\n'
        f'def preflight_candidate : Rule := ⟨"preflight", {pat(lhs)}, .pat {pat(rhs)}, .tt⟩\n'
        'set_option maxHeartbeats 200000\n'
        'set_option linter.unusedSimpArgs false\n'
        'theorem preflight_sound : RuleCovered preflight_candidate := by\n'
        '  rule_one preflight_candidate var "x"\n'
        '#check preflight_sound\n#print axioms preflight_sound\nend EvoIR\n')


def residual_candidate(residual):
    """A potential identity in this grammar, with no proof/utility assertion."""
    expression = residual['expression']
    width = expression[1]
    variables = set()
    def term(t):
        require(t[1] == width, 'MIXED_WIDTH_OUTSIDE_GRAMMAR')
        if t[0] == 'v':
            variables.add(t[2])
            return ('x',)
        if t[0] == 'c':
            require(t[2] in (0, 1), 'CONSTANT_OUTSIDE_GRAMMAR')
            return ('c', t[2])
        require(t[0] in OPS, 'OPERATOR_OUTSIDE_GRAMMAR')
        return (t[0], *(term(c) for c in t[2:]))
    try:
        lhs = term(expression)
        require(len(variables) <= 1, 'VARIABLES_OUTSIDE_GRAMMAR')
        match = re.search(r'\bret i\d+ (\S+)', residual['reference_body'])
        require(match is not None, 'UNSUPPORTED_REFERENCE_RETURN')
        output = match[1]
        if output in ('0', '1'):
            rhs = ('c', int(output))
        else:
            require(len(variables) == 1 and output == '%x'+str(next(iter(variables))),
                    'RHS_OUTSIDE_GRAMMAR')
            rhs = ('x',)
        return dict(lhs=lhs, rhs=rhs, candidate_digest=digest(canonical([lhs, rhs])))
    except ValueError as error:
        return dict(outside_grammar=str(error))


def diagnostic_expressions(census, max_nodes=3):
    keys(census, 'schema authority input_digest eligible excluded roots')
    require(census['schema'] == 'evocc-opportunity-census-v1'
            and census['authority'] == 'PRODUCER_DIAGNOSTIC_NOT_PROOF'
            and census['eligible'] == len(census['roots']), 'invalid producer census')
    definitions = {}
    for row in census['roots']:
        key = (row['function'], row['block'], row['root'])
        require(key not in definitions, 'duplicate diagnostic root')
        definitions[key] = row
    expressions, excluded = {}, {}
    for row in census['roots']:
        if row['op'] == 'const':
            continue
        inputs, nodes = {}, []
        seen = {}
        def walk(r, budget):
            op, width = r['op'], r['width']
            natural(width)
            require(1 <= width <= 64, 'outside P0 width')
            key = (r['function'], r['block'], r['root'])
            if key in seen:
                return seen[key]
            if op == 'const':
                return ('c', width, int(r['constant'], 16) & ((1 << width)-1))
            if budget <= 0:
                return seed(r['root'], width)
            if op not in (*OPS, 'icmp.eq', 'icmp.ne', 'icmp.slt', 'icmp.sle', 'icmp.ult', 'icmp.ule',
                           'shl', 'lshr', 'ashr', 'zext', 'sext', 'trunc'):
                raise ValueError('MAPPING_UNSUPPORTED_OPCODE')
            args = []
            for v in r['operands']:
                d = definitions.get((r['function'], r['block'], v['id']))
                args.append(walk(d, budget-1) if d else seed(v['id'], v['width']))
            if op in ('shl', 'lshr', 'ashr') and (args[1][0] != 'c' or args[1][2] >= width):
                raise ValueError('LLVM_SHIFT_DOMAIN_NOT_TOTAL')
            value = (op, width, *args)
            seen[key] = value
            nodes.append(value)
            return value
        def seed(ident, width):
            key = (ident, width)
            if key not in inputs:
                inputs[key] = len(inputs)
            return ('v', width, inputs[key])
        try:
            expr = walk(row, max_nodes)
            unique_nodes = {canonical(node) for node in nodes}
            if len(unique_nodes) > max_nodes:
                raise ValueError('OUTSIDE_FROZEN_NODE_BOUND')
        except ValueError as error:
            reason = str(error)
            excluded[reason] = excluded.get(reason, 0)+1
            continue
        ident = digest(canonical(expr))
        item = expressions.setdefault(ident, dict(expression=expr, occurrences=[],
            operator_nodes=len(unique_nodes), mapping='INITIALIZED_TOTAL_BITS_NO_FLAGS'))
        item['occurrences'].append(dict(function=row['function'], block=row['block'], root=row['root']))
    return dict(expressions=expressions, mapping_exclusions=excluded,
                scope='LOCAL_BOUNDED_DAG_INPUTS_ABSTRACTED_NO_INTRINSIC_FACT_ASSUMPTIONS')


def llvm_function(ident, expr):
    declarations, code, emitted = {}, [], {}
    def value(t):
        op, width = t[:2]
        if op == 'c':
            return str(t[2])
        if op == 'v':
            declarations[t[2]] = width
            return '%x'+str(t[2])
        key = canonical(t)
        if key in emitted:
            return emitted[key]
        args = [value(child) for child in t[2:]]
        result = '%t'+str(len(code))
        if op in ('zext', 'sext', 'trunc'):
            if width == t[2][1]:
                emitted[key] = args[0]
                return args[0]
            body = f'{op} i{t[2][1]} {args[0]} to i{width}'
        elif op.startswith('icmp.'):
            body = f'icmp {op[5:]} i{t[2][1]} {args[0]}, {args[1]}'
        else:
            require(op in (*OPS, 'shl', 'lshr', 'ashr'), 'unmapped LLVM operation')
            body = f'{op} i{width} {args[0]}, {args[1]}'
        code.append(f'  {result} = {body}')
        emitted[key] = result
        return result
    result = value(expr)
    args = ', '.join(f'i{width} %x{n}' for n, width in sorted(declarations.items()))
    return f'define i{expr[1]} @r{ident}({args}) {{\n'+'\n'.join(code)+f'\n  ret i{expr[1]} {result}\n}}\n'


def llvm_functions(text):
    return {match[0]: match[1] for match in re.findall(
        r'define\b[^@\n]*@r([0-9a-f]{64})\([^\n]*\)[^{]*\{(.*?)^\}', text, re.M | re.S)}
