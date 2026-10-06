"""Read message templates from Rust producers and records from the UI corpus.

This is a source test, not a diagnostic registry. Read literal message/detail
fields, report/usage/fail/warn calls, string errors and message/Display bodies.
Runtime substitutions (source names, user #error text and OS errors) are data,
not compiler-authored English. Check their surrounding template instead.
"""
import json
from pathlib import Path
import re
import sys


LITERAL = re.compile(r'r(?P<hashes>#{0,255})"(?P<raw>.*?)"(?P=hashes)|"(?:[^"\\]|\\.)*"', re.S)
CHARACTER = re.compile(r"'(?:\\.|[^'\\\n])'")


def literals(source):
    """Return literal extents and code with comments/literals blanked.

    Preserve offsets. Braces inside strings must not close a function body.
    Block comments can nest in Rust.
    """
    values, masked, pos = [], list(source), 0
    while pos < len(source):
        if source.startswith('/*', pos):
            end, depth = pos + 2, 1
            while depth:
                assert end < len(source), 'unterminated block comment'
                if source.startswith('/*', end):
                    depth += 1
                    end += 2
                elif source.startswith('*/', end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
        elif source.startswith('//', pos):
            end = source.find('\n', pos)
            if end < 0:
                end = len(source)
        else:
            value = LITERAL.match(source, pos)
            if value is None:
                # A character literal can contain braces or a double quote.
                char = CHARACTER.match(source, pos) if source[pos] == "'" else None
                if char is None:
                    pos += 1
                    continue
                end = pos + len(char[0])
            else:
                end = value.end()
                text = value['raw'] if value['raw'] is not None else value[0][1:-1]
                values.append((pos, end, text))
        masked[pos:end] = ' ' * (end - pos)
        pos = end
    return values, ''.join(masked)


def body_end(masked, start):
    depth = 1
    for pos in range(start + 1, len(masked)):
        if masked[pos] == '{':
            depth += 1
        elif masked[pos] == '}':
            depth -= 1
            if depth == 0:
                return pos
    raise AssertionError('unclosed Rust body')


def templates(source, path='crates/evocc/src/main.rs'):
    # Production files put their unit-test module behind this attribute.
    # UI and integration expectations are checked separately as records.
    source = source.split('#[cfg(test)]')[0]
    values, masked = literals(source)
    ranges, displays = [], []
    for match in re.finditer(r'\bfn\s+message\s*\([^;{]*\)[^{;]*\{', masked):
        ranges.append((match.end(), body_end(masked, match.end() - 1)))
    for match in re.finditer(r'\bimpl\s+[^{};]*\bDisplay\s+for\s+\w*(?:Error|Fault)\s*\{', masked):
        displays.append((match.end(), body_end(masked, match.end() - 1)))
    chosen = set()
    for begin, end, text in values:
        prefix = source[max(0, begin - 250):begin]
        direct = re.search(
            r'(?:\b(?:message|detail)\s*:|\blet\s+message\s*=|\b(?:usage|fail|warn|error|Err|ok_or)\s*\()'
            r'\s*(?:&\s*)?(?:format!\s*\(\s*)?$', prefix)
        report = re.search(r'"E\d{4}"\s*,\s*(?:&\s*)?(?:format!\s*\(\s*)?$', prefix)
        method = any(a <= begin < b for a, b in ranges)
        display = any(a <= begin < b for a, b in displays)
        if direct or report or method or display:
            # Interpolated data are not sentence punctuation or authored text.
            prose = re.sub(r'\{[^{}]*\}', '', text).strip()
            if re.search('[a-zA-Z]', prose) and not re.fullmatch(r'[EW]\d{4}', prose):
                # Library errors are details of a diagnostic, not necessarily
                # its header. For example LowerError.message adds the fixed
                # evoir-lowering-failed prefix to its Display representation.
                # Keep these fragments in the inventory instead of testing a
                # made-up standalone header that the driver never prints.
                driver = path.startswith('crates/evocc/src/')
                field = re.search(r'\bmessage\s*:\s*(?:format!\s*\(\s*)?$', prefix)
                public_field = field and any(path.startswith(f'crates/{crate}/src/') for crate in
                                            ['evocc-cpp', 'evocc-parser', 'evocc-chir', 'evocc-evoir', 'evocc-eqopt'])
                # InterpreterError.message delegates unclassified errors to
                # Display. Its fallback strings are headers too.
                fallback = display and path == 'crates/evocc-evoir/src/lib.rs'
                if fallback:
                    # The explicit message arms replace these Display strings.
                    # Derive the variants from the method, not a skip list.
                    overrides = set()
                    for a, b in ranges:
                        overrides.update(re.findall(r'\bSelf::(\w+)', masked[a:b]))
                    variants = re.findall(r'\bSelf::(\w+)', masked[:begin])
                    fallback = bool(variants) and variants[-1] not in overrides
                role = 'header' if driver or report or method or public_field or fallback else 'detail'
                if driver and prose.startswith('-'):
                    # Invocation errors pass through Diagnostic::command,
                    # which names a leading option as an option in text/JSON.
                    prose = 'option ' + prose
                chosen.add((source[:begin].count('\n') + 1, role, prose))
    return sorted(chosen)


def inventory():
    rows = []
    for source in sorted(Path('crates').glob('*/src/**/*.rs')):
        rows.extend((str(source) + ':' + str(line), role, text)
                    for line, role, text in templates(source.read_text(), str(source)))
    assert rows, 'no source message templates found'
    for path in sorted(Path('tests/ui/check').glob('*.json')):
        for line in path.read_text().splitlines():
            record = json.loads(line)
            rows.append((str(path), 'header', record['message']))
            rows.extend((str(path), 'label', span['label']) for span in record['spans'] if span['primary'])
            rows.extend((str(path), role, text) for role, key in [('note', 'notes'), ('help', 'help')]
                        for text in record[key])
    return rows


def accepts(rule, text):
    if rule == 'lowercase':
        first = re.search('[a-zA-Z]', text)
        return first is not None and first[0].islower()
    if rule == 'sentence':
        return not re.search(r'[.!?]\s+[A-Za-z]|\\n', text)
    if rule == 'period':
        return not text.endswith('.')
    if rule == 'prefix':
        return 'error:' not in text.lower()
    if rule == 'label':
        return bool(re.search('[a-zA-Z]', text)) and not re.fullmatch(r'[EW]\d{4}|error|here', text)
    action = text.split()[0] if text.split() else ''
    actions = {'add', 'change', 'check', 'choose', 'declare', 'define', 'initialize', 'pass', 'remove', 'replace', 'run', 'set', 'use'}
    if rule == 'help':
        return action in actions
    if rule == 'note':
        return bool(text) and action not in actions and not text.startswith(('maybe ', 'perhaps '))
    raise AssertionError(rule)


def self_test():
    sample = '''
// message: "not a message"
fn message(&self) -> String { format!("invalid value: {value}") }
let x = Error { message: r#"missing input"#.into() };
self.report("E1001", "expected declaration", span);
self.usage(&format!("unknown option {option}"));
#[cfg(test)] mod tests { let x = Error {message: "UPPERCASE"}; }
fn later() { #[cfg(test)] fault::check()?; Err("later production error"); }
'''
    assert [text for _, _, text in templates(sample)] == [
        'invalid value:', 'missing input', 'expected declaration', 'unknown option',
        'later production error']
    assert templates('let x = Err("TLS detail");', 'crates/evocc-lower/src/lib.rs') == [(1, 'detail', 'TLS detail')]
    assert templates('fn message(self) -> String { "lowering failed".into() }',
                     'crates/evocc-lower/src/lib.rs') == [(1, 'header', 'lowering failed')]
    for rule, good, bad in [('lowercase', 'missing input', 'Missing input'),
                            ('sentence', 'missing input', 'missing input. Add it'),
                            ('period', 'missing input', 'missing input.'),
                            ('prefix', 'invalid input', 'error: invalid input'),
                            ('label', 'divisor is zero', 'E3002'),
                            ('note', 'the divisor is zero', 'check the divisor'),
                            ('help', 'check the divisor', 'the divisor is zero')]:
        assert accepts(rule, good) and not accepts(rule, bad), rule


if __name__ == '__main__':
    rule = sys.argv[1]
    self_test()
    if rule != 'inventory':
        role = rule if rule in {'label', 'note', 'help'} else 'header'
        rows = [(where, text) for where, kind, text in inventory()
                if kind == role or (kind == 'detail' and rule in {'period', 'prefix'})]
        assert rows, f'no {role} messages checked'
        failures = [f'{where}: {text}' for where, text in rows if not accepts(rule, text)]
        assert not failures, '\n'.join(failures)
        print(f'{rule}: checked {len(rows)} messages')
