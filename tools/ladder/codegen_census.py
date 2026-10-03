"""N01 read-only instruction attribution; S1 remains the size authority.

Object symbols define bodies. Disassembler spellings classify instructions;
they do not prove an optimization opportunity. No full disassembly is retained.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess
import time

from .eqsat_chain import digest, require
from .eqsat_elf import inspect_object_size, require_identical_objects

CATEGORIES = ('guard', 'prologue_epilogue', 'stack_access', 'move',
              'const_materialize', 'address', 'extend_mask', 'call',
              'compare_select', 'branch', 'memory', 'compute', 'other')
SECTION = re.compile(r'^Disassembly of section (.+):$')
INSTRUCTION = re.compile(r'^\s*([0-9a-f]+):\s+([0-9a-f]{8})\s+(\S+)(?:\s+(.*))?$')
RELOCATION = re.compile(r'^\s*([0-9a-f]+):\s+(R_AARCH64_\w+)\s+(\S+)')
REGISTER = re.compile(r'^(?:[wxvdsqbh]\d+(?:\.\w+)?(?:\[\d+\])?|[wx]zr|sp|wsp)$')
COMPARE = {'cmp', 'cmn', 'tst', 'ccmp', 'ccmn', 'fcmp', 'fcmpe', 'fccmp', 'fccmpe',
           'cset', 'csetm', 'csel', 'csinc', 'csinv', 'csneg', 'cinc', 'cinv', 'cneg', 'fcsel'}
FLAG_WRITERS = {'cmp', 'cmn', 'tst', 'ccmp', 'ccmn', 'fcmp', 'fcmpe', 'fccmp',
                'fccmpe', 'adds', 'subs', 'ands', 'bics', 'adcs', 'sbcs', 'negs', 'ngcs'}
FLAG_READERS = COMPARE - {'cmp', 'cmn', 'tst', 'fcmp', 'fcmpe'} | {'adc', 'adcs', 'sbc', 'sbcs', 'ngc', 'ngcs'}
COMPUTE = {'add', 'adds', 'sub', 'subs', 'mul', 'madd', 'msub', 'mneg', 'sdiv', 'udiv',
           'smulh', 'umulh', 'smull', 'umull', 'smaddl', 'umaddl', 'smsubl', 'umsubl',
           'and', 'ands', 'orr', 'orn', 'eor', 'eon', 'bic', 'bics', 'mvn', 'neg', 'negs',
           'adc', 'adcs', 'sbc', 'sbcs', 'ngc', 'ngcs', 'lsl', 'lsr', 'asr', 'ror',
           'lslv', 'lsrv', 'asrv', 'rorv', 'ubfx', 'sbfx', 'bfxil', 'bfi', 'bfc',
           'ubfiz', 'sbfiz', 'ubfm', 'sbfm', 'bfm', 'extr', 'clz', 'cls', 'rbit',
           'rev', 'rev16', 'rev32', 'cnt', 'fadd', 'fsub', 'fmul', 'fdiv', 'fneg',
           'fabs', 'fmadd', 'fmsub', 'fnmadd', 'fnmsub', 'fcvt', 'fcvtzs', 'fcvtzu',
           'scvtf', 'ucvtf', 'dup', 'ext', 'zip1', 'zip2', 'uzp1', 'uzp2', 'trn1', 'trn2',
           'cmhi', 'cmgt', 'cmeq', 'uqsub', 'uqxtn', 'uqxtn2', 'ushl', 'xtn', 'xtn2',
           'bsl', 'bif', 'bit', 'addp', 'uaddw', 'uaddw2', 'umaxv', 'umin', 'ushr', 'fabd'}


def _layout(data):
    """Read just the ELF facts needed to assign decoded words to function bodies."""
    header = struct.unpack_from('<HHIQQQIHHHHHH', data, 16)
    offset, count, names_index = header[5], header[11], header[12]
    entries = [struct.unpack_from('<IIQQQQIIQQ', data, offset+64*i) for i in range(count)]

    def payload(section):
        return data[section[4]:section[4]+section[5]]

    def name_at(strings, offset):
        end = strings.find(b'\0', offset)
        require(0 <= offset <= end, 'invalid symbol string')
        return strings[offset:end].decode('utf-8')

    section_names = payload(entries[names_index])
    names = [name_at(section_names, row[0]) for row in entries]
    functions, mappings = defaultdict(list), defaultdict(dict)
    for row in entries:
        if row[1] != 2:
            continue
        strings = payload(entries[row[6]])
        for pos in range(row[4], row[4]+row[5], 24):
            name, info, _, index, address, size = struct.unpack_from('<IBBHQQ', data, pos)
            label = name_at(strings, name)
            if index >= count:
                continue
            if info & 15 == 2 and size:
                functions[index].append((address, address+size, label, info >> 4))
            if label in ('$x', '$d') or label.startswith(('$x.', '$d.')):
                mappings[index][address] = label[1]
    bodies, code = [], {}
    for index, row in enumerate(entries):
        section = names[index]
        if section != '.text' and not section.startswith('.text.'):
            continue
        grouped = defaultdict(list)
        for lo, hi, name, binding in functions[index]:
            require(0 <= lo < hi <= row[5], 'invalid function extent')
            grouped[(lo, hi)].append((name, binding))
        previous = 0
        for (lo, hi), aliases in sorted(grouped.items()):
            require(lo >= previous, 'overlapping function bodies')
            previous = hi
            names_and_bindings = sorted(set(aliases))
            body = dict(section=section, start=lo, end=hi,
                        name=names_and_bindings[0][0], aliases=[n for n, _ in names_and_bindings],
                        local=all(b == 0 for _, b in names_and_bindings), body_bytes=hi-lo,
                        instruction_bytes=0, direct_calls=0)
            body_id = len(bodies)
            bodies.append(body)
            boundaries = sorted({lo, hi, *(a for a in mappings[index] if lo < a < hi)})
            mode = None
            for address in sorted(mappings[index]):
                if address <= lo:
                    mode = mappings[index][address]
            for a, b in zip(boundaries, boundaries[1:]):
                mode = mappings[index].get(a, mode)
                if mode != 'x':
                    continue
                require(a % 4 == 0 and b % 4 == 0, 'unaligned code extent')
                for address in range(a, b, 4):
                    code[(section, address)] = (body_id, struct.unpack_from('<I', data, row[4]+address)[0])
                    body['instruction_bytes'] += 4
    return bodies, code


def _parse(text, code):
    section, decoded, relocations = None, {}, defaultdict(list)
    seen = set()
    for line in text.splitlines():
        match = SECTION.match(line)
        if match:
            section = match[1]
            continue
        match = RELOCATION.match(line)
        if match:
            relocations[(section, int(match[1], 16))].append((match[2], match[3]))
            continue
        match = INSTRUCTION.match(line)
        if not match:
            continue
        key = section, int(match[1], 16)
        if key not in code:
            continue  # S1 data and layout padding are not instructions.
        require(key not in seen, 'duplicate decoded instruction')
        seen.add(key)
        require(int(match[2], 16) == code[key][1], 'decoded bytes differ from object')
        mnemonic = match[3].lower()
        require(not mnemonic.startswith(('.', '<')), 'unknown instruction decoding')
        operands = (match[4] or '').split('//')[0].strip()
        decoded[key] = dict(mnemonic=mnemonic, operands=operands, body=code[key][0])
    require(decoded.keys() == code.keys(), 'missing decoded instruction')
    for key, inst in decoded.items():
        inst['relocations'] = relocations[key]
    return decoded


def _conditional(m):
    return m.startswith('b.') or m in ('cbz', 'cbnz', 'tbz', 'tbnz')


def _target(inst):
    if inst['relocations']:
        return None
    match = re.search(r'(?:^|,\s*)0x([0-9a-f]+)(?:\s|$)', inst['operands'])
    return int(match[1], 16) if match else None


def _successors(key, inst, decoded):
    section, address = key
    m = inst['mnemonic']
    addresses = []
    if m == 'b' or _conditional(m):
        target = _target(inst)
        if target is not None:
            addresses.append(target)
    if m not in ('b', 'br', 'ret', 'retaa', 'retab', 'brk', 'hlt'):
        addresses.append(address+4)
    return [(section, a) for a in addresses if (section, a) in decoded
            and decoded[(section, a)]['body'] == inst['body']]


def _guards(decoded):
    guards, guard_branches = set(), set()
    for key, inst in decoded.items():
        if not _conditional(inst['mnemonic']):
            continue
        target = key[0], _target(inst)
        if target in decoded and decoded[target]['mnemonic'] == 'brk' and decoded[target]['body'] == inst['body']:
            guards.update((key, target))
            guard_branches.add(key)
    # A compare is guard-only only if every reachable flags use is that edge.
    for key, inst in decoded.items():
        if inst['mnemonic'] not in FLAG_WRITERS:
            continue
        todo, seen, uses = _successors(key, inst, decoded), set(), set()
        while todo:
            successor = todo.pop()
            if successor in seen:
                continue
            seen.add(successor)
            current = decoded[successor]
            m = current['mnemonic']
            if m in FLAG_READERS or m.startswith('b.'):
                uses.add(successor)
            if m not in FLAG_WRITERS and m not in ('bl', 'blr'):
                todo.extend(_successors(successor, current, decoded))
        if len(uses) == 1 and uses <= guard_branches:
            guards.add(key)
    return guards


def _memory(m):
    return m.startswith(('ld', 'st')) and m not in ('stg', 'st2g')


def _callee_save(register):
    match = re.fullmatch(r'([xwdqv])(\d+)', register)
    return bool(match and (19 <= int(match[2]) <= 30 if match[1] in 'xw'
                          else 8 <= int(match[2]) <= 15))


def _call_target(inst, key, bodies):
    for kind, symbol in inst['relocations']:
        if kind in ('R_AARCH64_CALL26', 'R_AARCH64_JUMP26'):
            return symbol.split('+')[0]
    target = _target(inst)
    for body in bodies:
        if body['section'] == key[0] and body['start'] == target:
            return body['name']
    return None


def _frame_sites(decoded, bodies):
    """Recognize entry saves and exit restores, not arbitrary callee-register spills.

    Shrink-wrapped or unusual sequences outside these syntactic boundaries stay
    in stack_access. That conservative limitation is exposed in the summary.
    """
    sites = set()
    for body_id, body in enumerate(bodies):
        keys = sorted(k for k, i in decoded.items() if i['body'] == body_id)
        written, saved = set(), set()
        for key in keys:
            inst = decoded[key]
            m, operands = inst['mnemonic'], inst['operands']
            if m in ('b', 'br', 'bl', 'blr', 'ret', 'brk') or _conditional(m):
                break
            regs = operands.split('[')[0].rstrip(', ').split(', ')
            canonical = [re.sub(r'^[w]', 'x', r) for r in regs]
            stack = bool(re.search(r'\[(?:sp|x29)(?:,|\])', operands))
            if m.startswith('st') and stack and all(_callee_save(r) for r in regs) and not (set(canonical) & written):
                sites.add(key)
                saved.update(canonical)
            if not m.startswith('st') and m not in COMPARE and operands:
                destinations = canonical if m.startswith('ld') else [re.sub(r'^w', 'x', operands.split(',')[0])]
                written.update(destinations)
        for position, key in enumerate(keys):
            inst = decoded[key]
            is_exit = inst['mnemonic'] in ('ret', 'retaa', 'retab')
            if inst['mnemonic'] == 'b':
                target = _call_target(inst, key, bodies)
                is_exit = bool(target and target not in body['aliases'])
            if not is_exit:
                continue
            for previous in reversed(keys[:position]):
                current = decoded[previous]
                m, operands = current['mnemonic'], current['operands']
                regs = operands.split('[')[0].rstrip(', ').split(', ')
                canonical = {re.sub(r'^w', 'x', r) for r in regs}
                if (m.startswith('ld') and re.search(r'\[(?:sp|x29)(?:,|\])', operands)
                        and all(_callee_save(r) for r in regs) and canonical <= saved):
                    sites.add(previous)
                elif m in ('add', 'sub') and operands.startswith('sp, sp,') or m == 'mov' and operands == 'sp, x29':
                    continue
                else:
                    break
    return sites


def _classify(key, inst, guards, bodies, frame_sites):
    m, operands = inst['mnemonic'], inst['operands']
    parts = [p.strip() for p in operands.split(',')]
    relocs = inst['relocations']
    if key in guards:
        return 'guard'
    stack = bool(re.search(r'\[(?:sp|x29)(?:,|\])', operands))
    if (m in ('ret', 'retaa', 'retab') or
            m in ('add', 'sub') and parts[:2] == ['sp', 'sp'] or
            m == 'mov' and parts == ['x29', 'sp'] or
            m == 'add' and parts[:2] == ['x29', 'sp'] or
            key in frame_sites):
        return 'prologue_epilogue'
    if _memory(m) and stack:
        return 'stack_access'
    if m in ('mov', 'fmov') and len(parts) == 2 and all(REGISTER.fullmatch(p) for p in parts):
        return 'move'
    if (m in ('movz', 'movn', 'movk', 'movi', 'mvni') or m in ('mov', 'fmov') and '#' in operands or
            m in ('ldr', 'ldrsw') and '[' not in operands):
        return 'const_materialize'
    if (m in ('adr', 'adrp') or m == 'add' and any('LO12' in r for r, _ in relocs) or
            _memory(m) and any('GOT' in r or 'TLS' in r for r, _ in relocs)):
        return 'address'
    if m.startswith(('sxt', 'uxt')) or m == 'and' and re.search(r'#(?:0xff|0xffff|0xffffffff)(?:\s|$)', operands):
        return 'extend_mask'
    target = _call_target(inst, key, bodies)
    if m in ('bl', 'blr') or m == 'b' and target and target not in bodies[inst['body']]['aliases']:
        return 'call'
    if m in COMPARE:
        return 'compare_select'
    if m in ('b', 'br') or _conditional(m):
        return 'branch'
    if _memory(m):
        return 'memory'
    if m in COMPUTE:
        return 'compute'
    return 'other'


def analyze(data, disassembly):
    size = inspect_object_size(data)
    require(size['unclassified_text_bytes'] == 0, 'S1 has unclassified text')
    bodies, code = _layout(data)
    require(len(code)*4 == size['instruction_bytes'], 'unmapped function instruction bytes')
    decoded = _parse(disassembly, code)
    guards = _guards(decoded)
    frame_sites = _frame_sites(decoded, bodies)
    categories = dict.fromkeys(CATEGORIES, 0)
    other, unresolved = Counter(), Counter()
    by_name = {alias: body for body in bodies for alias in body['aliases']}
    for key, inst in decoded.items():
        category = _classify(key, inst, guards, bodies, frame_sites)
        categories[category] += 4
        if category == 'other':
            other[inst['mnemonic']] += 1
        if category == 'call':
            target = _call_target(inst, key, bodies)
            if target in by_name:
                by_name[target]['direct_calls'] += 1
        if any('__stack_chk' in s for _, s in inst['relocations']):
            unresolved['stack_canary_related_relocation_sites'] += 1
        if category == 'stack_access':
            regs = inst['operands'].split('[')[0].rstrip(', ').split(', ')
            if regs and all(_callee_save(r) for r in regs):
                unresolved['callee_register_stack_sites_outside_entry_exit_pattern'] += 1
    require(sum(categories.values()) == size['instruction_bytes'], 'category bytes differ from S1')
    return dict(size=size, category_bytes=categories, functions=bodies,
                other_mnemonics=dict(sorted(other.items())),
                attribution_limits=dict(sorted(unresolved.items())))


def validate_summary(data, disassembly, record):
    require(analyze(data, disassembly) == record, 'forged instruction category record')


def control_windows(data, disassembly):
    """Count C4's frozen local shapes, not proven or profitable rewrites.

    `selected` means only that the syntactic filters passed. Physical aliases,
    virtual liveness, flags equivalence and encoding legality remain unproved.
    Reuse N01's exact object/decoder boundary and retain no disassembly dump.
    """
    bodies, code = _layout(data)
    decoded = _parse(disassembly, code)
    guards = _guards(decoded)
    incoming = {(key[0], _target(inst)) for key, inst in decoded.items()
                if inst['mnemonic'] in ('b', 'bl') or _conditional(inst['mnemonic'])}
    names = ('boolean_branch', 'boolean_retest', 'compare_conjunction')
    result = {name: dict(raw=0, selected=0, declined=Counter(), examples=[])
              for name in names}
    compares = {'cmp', 'cmn', 'tst'}

    def parts(inst):
        return [part.strip() for part in inst['operands'].split(',')]

    def gpr(text):
        return bool(re.fullmatch(r'[wx](?:[12]?\d|30)', text))

    def register_shape(name, window):
        first = parts(window[1])
        if len(first) != 2 or not gpr(first[0]):
            return False
        boolean = first[0]
        last = parts(window[-1])
        if name == 'boolean_branch':
            if window[-1]['mnemonic'] in ('cbz', 'cbnz'):
                return len(last) == 2 and last[0] == boolean
            return len(last) == 3 and last[0] == boolean and last[1] in ('#0', '#0x0')
        if name == 'boolean_retest':
            test = parts(window[2])
            condition = (window[-1]['mnemonic'][2:] if
                         window[-1]['mnemonic'].startswith('b.') else last[-1])
            return test in ([boolean, '#0'], [boolean, '#0x0']) and condition in ('eq', 'ne')
        second = parts(window[3])
        return (len(second) == 2 and gpr(second[0]) and
                second[0][0] == boolean[0] and second[0] != boolean and
                len(last) == 3 and gpr(last[0]) and last[0][0] == boolean[0] and
                last[1:] in ([boolean, second[0]], [second[0], boolean]))

    for key in sorted(decoded):
        prefix = [decoded.get((key[0], key[1]+4*i)) for i in range(5)]
        if (not prefix[0] or not prefix[1] or
                prefix[0]['mnemonic'] not in compares or prefix[1]['mnemonic'] != 'cset'):
            continue
        shapes = []
        if prefix[2] and prefix[2]['mnemonic'] in ('cbz', 'cbnz', 'tbz', 'tbnz'):
            shapes.append(('boolean_branch', 3))
        if (prefix[2] and prefix[3] and prefix[2]['mnemonic'] == 'cmp' and
                (prefix[3]['mnemonic'] in ('cset', 'csel') or prefix[3]['mnemonic'].startswith('b.'))):
            shapes.append(('boolean_retest', 4))
        if (prefix[2] and prefix[3] and prefix[4] and
                prefix[2]['mnemonic'] in compares and prefix[3]['mnemonic'] == 'cset' and
                prefix[4]['mnemonic'] in ('and', 'orr')):
            shapes.append(('compare_conjunction', 5))
        for name, length in shapes:
            row = result[name]
            row['raw'] += 1
            window = prefix[:length]
            keys = [(key[0], key[1]+4*i) for i in range(length)]
            reason = next((reason for reject, reason in [
                (any(inst['body'] != window[0]['body'] for inst in window), 'function_boundary'),
                (any(k in incoming for k in keys[1:]), 'interior_branch_target'),
                (any(inst['relocations'] for inst in window), 'relocation'),
                (any(k in guards for k in keys), 'guard_site'),
                (not register_shape(name, window), 'register_or_shape'),
            ] if reject), None)
            if reason:
                row['declined'][reason] += 1
                continue
            row['selected'] += 1
            if len(row['examples']) < 4:
                row['examples'].append(dict(function=bodies[window[0]['body']]['name'],
                                            section=key[0], offset=key[1]))
    for row in result.values():
        row['declined'] = dict(sorted(row['declined'].items()))
    return dict(patterns=result, claim='SYNTACTIC_CANDIDATES_NOT_TRANSFORMS')


def _decode(tool, obj, resources):
    # Communicate drains both pipes and gives a bounded one-object timeout.
    # The output is discarded after classification, never persisted in captures.
    result = subprocess.run([tool, '-dr', str(obj)], capture_output=True,
                            timeout=resources['disassembler_timeout_seconds'], check=True)
    require(len(result.stdout)+len(result.stderr) <= resources['disassembler_output_limit_bytes'],
            'disassembly output exceeds the one-object cap')
    require(not result.stderr, 'unexpected disassembler diagnostic')
    return result.stdout.decode('utf-8')


def run(plan, root):
    root, plan = Path(root).resolve(), Path(plan).resolve()
    raw = plan.read_bytes()
    m = json.loads(raw)
    require(subprocess.check_output(['git', 'show', 'HEAD:'+plan.relative_to(root).as_posix()], cwd=root) == raw,
            'commit census plan before execution')
    require(shutil.disk_usage(root).free >= m['resources']['min_start_free_bytes'], 'disk reserve unavailable')
    tool = m['tools']['disassembler']
    require(digest(Path(tool).read_bytes()) == m['tools']['disassembler_sha256'], 'disassembler changed')
    receipts = {}
    for key in ('s5', 'guard'):
        data = (root/m['inputs'][key+'_receipt']).read_bytes()
        require(digest(data) == m['inputs'][key+'_receipt_sha256'], 'frozen input changed')
        receipts[key] = json.loads(data)
    result = dict(schema='evocc-codegen-census-v1', status='OBSERVED_RETAINED_OBJECTS',
                  plan_digest=digest(raw), worktree_base_commit=subprocess.check_output(
                      ['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(), units=[],
                  implementation_identity='Source and receipt committed together; base commit is not a clean-build identity',
                  claim='SYNTACTIC_ATTRIBUTION_NOT_CAUSAL_SAVINGS', additional_ladder='NOT_RUN')
    cache = {}
    for index, unit in enumerate(receipts['s5']['units']):
        rows = {}
        for arm in ('A', 'B'):
            for profile in ('checked', 'unchecked'):
                rows[arm+'-'+profile] = unit['evo'][arm][profile]
        for compiler in ('gcc-14-O2', 'clang-19-O2'):
            rows[compiler] = unit['references'][compiler]
        guard = receipts['guard']['units'][index]
        require(guard['source'] == unit['source'], 'guard/S5 TU mismatch')
        rows['G-checked'] = guard['arms']['G']['checked']
        analyzed = {}
        for name, row in rows.items():
            require(shutil.disk_usage(root).free >= m['resources']['stop_free_bytes'], 'disk stop threshold')
            obj = root/row['object']
            data = obj.read_bytes()
            twin = obj.with_name(obj.name.removesuffix('-0.o')+'-1.o')
            require(obj.name.endswith('-0.o'), 'unrecognized retained twin spelling')
            require_identical_objects(data, twin.read_bytes())
            require(row['two_builds'] == 'BYTE_IDENTICAL' and inspect_object_size(data) == row['metrics'],
                    'retained object does not match S1 receipt')
            key = digest(data)
            if key not in cache:
                cache[key] = analyze(data, _decode(tool, obj, m['resources']))
            analyzed[name] = cache[key]
        baseline, reference = analyzed['B-unchecked'], analyzed['gcc-14-O2']
        excess = {k: baseline['category_bytes'][k]-reference['category_bytes'][k] for k in CATEGORIES}
        levers = sorted(((k, v) for k, v in excess.items() if v > 0), key=lambda x: (-x[1], x[0]))[:3]
        locals_table = []
        for body in baseline['functions']:
            if not body['local']:
                continue
            local = dict(symbol=body['name'], evo_bytes=body['body_bytes'], evo_calls=body['direct_calls'])
            for compiler in ('gcc-14-O2', 'clang-19-O2'):
                match = next((b for b in analyzed[compiler]['functions']
                              if set(b['aliases']) & set(body['aliases'])), None)
                local[compiler] = dict(body_bytes=match['body_bytes'], direct_calls=match['direct_calls']) if match else None
            locals_table.append(local)
        result['units'].append(dict(case=unit['case'], source=unit['source'], eligible_roots=unit['eligible_roots'],
            arms=analyzed, ratio_to_gcc_o2=baseline['size']['text_bytes']/reference['size']['text_bytes'],
            target_ratio=1.4, local_functions=locals_table,
            top_three_positive_category_excess=[dict(category=k, estimated_byte_ceiling=v) for k, v in levers]))
    result['unique_objects_decoded'] = len(cache)
    return result


def run_ladder(plan, root):
    """Inventory the frozen historical population without manufacturing twins."""
    root, plan = Path(root).resolve(), Path(plan).resolve()
    raw = plan.read_bytes()
    m = json.loads(raw)
    require(subprocess.check_output(['git', 'show', 'HEAD:'+plan.relative_to(root).as_posix()], cwd=root) == raw,
            'commit ladder census plan before execution')
    parent = json.loads((root/m['parent_plan']).read_bytes())
    tool = parent['tools']['disassembler']
    require(digest(Path(tool).read_bytes()) == parent['tools']['disassembler_sha256'], 'disassembler changed')
    require(shutil.disk_usage(root).free >= m['resources']['min_start_free_bytes'], 'disk reserve unavailable')
    started = time.monotonic()
    result = dict(schema='evocc-codegen-ladder-inventory-v1', plan_digest=digest(raw),
                  status='HISTORICAL_INVENTORY', units=[], populations=[],
                  claim=m['claim_ceiling'])
    for source in m['inputs']:
        data = (root/source['receipt']).read_bytes()
        require(digest(data) == source['sha256'], 'ladder receipt changed')
        receipt = json.loads(data)
        family = Path(source['receipt']).name.removeprefix('ladder-').removesuffix('-receipt.json')
        if source['units'] is None:
            result['populations'].append(dict(family=family, disposition='NO_UNIT_OBJECT_INVENTORY'))
            continue
        raw_units = (root/source['units']).read_bytes()
        require(digest(raw_units) == source['units_sha256'], 'ladder units changed')
        rows = [json.loads(line) for line in raw_units.splitlines() if line.strip()]
        result['populations'].append(dict(family=family, records=len(rows)))
        for row in rows:
            item = dict(family=family, source=row.get('source'), unit=row.get('unit'),
                        reproducibility='NO_RETAINED_TWO_BUILD_EVIDENCE')
            result['units'].append(item)
            if row.get('outcome') != 'assembled':
                item['disposition'] = 'ORIGINAL_NOT_ASSEMBLED'
                continue
            assembly = Path(row.get('assembly', ''))
            obj, expected = None, row.get('objectSha256')
            if assembly.is_absolute():
                obj = assembly.with_suffix('.o')
            elif family == 'toybox':
                obj = root/'.build/ladder/toybox-work'/row['unit']
                expected = receipt.get('substitutedTest', {}).get('objectFiles', {}).get(row['unit'])
            if obj is None or expected is None:
                item['disposition'] = 'NO_RECORDED_OBJECT_IDENTITY'
                continue
            if not obj.is_relative_to(root):
                item['disposition'] = 'OBJECT_OUTSIDE_WORKSPACE'
                continue
            if not obj.is_file():
                item['disposition'] = 'RETIRED_OBJECT'
                continue
            data = obj.read_bytes()
            item['object_digest'] = digest(data)
            if item['object_digest'] != expected:
                item['disposition'] = 'REPLACED_OBJECT'
                continue
            if time.monotonic()-started > m['resources']['capture_timeout_seconds']:
                item['disposition'] = 'CAPTURE_BUDGET_EXHAUSTED'
                continue
            require(shutil.disk_usage(root).free >= m['resources']['stop_free_bytes'], 'disk stop threshold')
            try:
                size = inspect_object_size(data)
                if size['static_instruction_slots'] is None or size['static_instruction_slots'] > m['resources']['object_instruction_limit']:
                    item['disposition'] = 'UNCLASSIFIED_OR_OVERSIZE_OBJECT'
                    continue
                summary = analyze(data, _decode(tool, obj, parent['resources']))
                item.update(disposition='CLASSIFIED_HISTORICAL_OBJECT', size=size,
                            category_bytes=summary['category_bytes'], other_mnemonics=summary['other_mnemonics'],
                            attribution_limits=summary['attribution_limits'], functions=len(summary['functions']))
            except (ValueError, subprocess.SubprocessError, UnicodeError) as error:
                item.update(disposition='CLASSIFICATION_UNAVAILABLE', reason=str(error))
    result['dispositions'] = dict(sorted(Counter(row['disposition'] for row in result['units']).items()))
    return result


def compact(result):
    """Keep the complete category/inlining tables without repeating every body."""
    output = dict(result)
    output['units'] = []
    for unit in result['units']:
        copied = dict(unit, arms={})
        for arm, record in unit['arms'].items():
            copied['arms'][arm] = {key: value for key, value in record.items() if key != 'functions'}
            copied['arms'][arm]['function_bodies'] = len(record['functions'])
        output['units'].append(copied)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--root', type=Path, default=Path.cwd())
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--ladder', action='store_true')
    parser.add_argument('--compact', action='store_true')
    options = parser.parse_args()
    require(not options.output.exists(), 'do not overwrite a census receipt')
    result = run_ladder(options.plan, options.root) if options.ladder else run(options.plan, options.root)
    if options.compact:
        require(not options.ladder, 'ladder summaries are already compact')
        result = compact(result)
    encoded = json.dumps(result, indent=2, sort_keys=True).encode()+b'\n'
    resources = json.loads(options.plan.read_bytes())['resources']
    require(len(encoded) <= resources.get('max_retained_bytes_per_capture', resources.get('max_retained_bytes')),
            'summary exceeds capture cap')
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_bytes(encoded)
    print(json.dumps(dict(status=result['status'], units=len(result['units']),
                         unique_objects=result.get('unique_objects_decoded'), summary_bytes=len(encoded))))


if __name__ == '__main__':
    main()
