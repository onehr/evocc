#!/usr/bin/env python3
"""Check physical source sizes against a baseline that can only shrink."""

import argparse
import ast
from collections import Counter
from dataclasses import dataclass
import json
from pathlib import Path
import re
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parent.parent
LIMITS = {"file": 1500, "function": 150}
BASELINE = "tools/size-baseline.json"
RAW_STRING = re.compile(r'(?:br|cr|r)(?P<hashes>\#*)"')
CHARACTER = re.compile(r"'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|[^\r\n])|[^'\\\r\n])'")


@dataclass(frozen=True)
class Token:
    text: str
    first: int
    last: int
    kind: str = "symbol"


@dataclass(frozen=True)
class Function:
    key: str
    first: int
    last: int

    @property
    def lines(self):
        return self.last - self.first + 1


def block_comment_end(source, start):
    """Rust permits nested block comments, including inside documentation."""
    depth, position = 1, start + 2
    while depth:
        opening = source.find("/*", position)
        closing = source.find("*/", position)
        if closing < 0:
            raise ValueError("unterminated Rust block comment")
        if 0 <= opening < closing:
            depth += 1
            position = opening + 2
        else:
            depth -= 1
            position = closing + 2
    return position


def quoted_end(source, start):
    position = start + 1
    while position < len(source):
        if source[position] == "\\":
            position += 2
        elif source[position] == '"':
            return position + 1
        else:
            position += 1
    raise ValueError("unterminated Rust string")


def rust_tokens(source):
    """Keep item syntax and doc spans; make literal contents opaque."""
    result, position, line = [], 0, 1
    while position < len(source):
        first, start = line, position
        char = source[position]
        if char.isspace():
            line += char == "\n"
            position += 1
            continue
        kind = "symbol"
        if source.startswith("//", position):
            end = source.find("\n", position)
            end = len(source) if end < 0 else end
            kind = "doc" if source.startswith("///", position) and not source.startswith("////", position) else "comment"
        elif source.startswith("/*", position):
            end = block_comment_end(source, position)
            kind = "doc" if source.startswith("/**", position) and not source.startswith("/***", position) else "comment"
        elif raw := RAW_STRING.match(source, position):
            terminator = '"' + raw["hashes"]
            closing = source.find(terminator, raw.end())
            if closing < 0:
                raise ValueError("unterminated Rust raw string")
            end, kind = closing + len(terminator), "literal"
        elif char == '"' or source.startswith(('b"', 'c"'), position):
            end = quoted_end(source, position if char == '"' else position + 1)
            kind = "literal"
        elif char == "'" and (literal := CHARACTER.match(source, position)):
            end, kind = literal.end(), "literal"
        elif source.startswith("b'", position) and (literal := CHARACTER.match(source, position + 1)):
            end, kind = literal.end(), "literal"
        elif char.isidentifier():
            end = position + 1
            if source.startswith("r#", position):
                end += 1
            while end < len(source) and ("a" + source[end]).isidentifier():
                end += 1
            kind = "word"
        else:
            end = position + 1
        line += source.count("\n", start, end)
        if kind != "comment":
            result.append(Token(source[start:end], first, line, kind))
        position = end
    return result


def delimiter_pairs(tokens):
    pairs, stack = {}, []
    closing = {"(": ")", "[": "]", "{": "}"}
    for index, token in enumerate(tokens):
        if token.kind != "symbol":
            continue
        if token.text in closing:
            stack.append(index)
        elif token.text in closing.values():
            if not stack or closing[tokens[stack[-1]].text] != token.text:
                raise ValueError(f"unmatched Rust delimiter on line {token.first}")
            opening = stack.pop()
            pairs[opening] = index
            pairs[index] = opening
    if stack:
        raise ValueError(f"unclosed Rust delimiter on line {tokens[stack[-1]].first}")
    return pairs


def item_body(tokens, pairs, start, stop):
    """Skip parameter/type delimiters and const expressions in generic arguments."""
    angles, index = 0, start
    while index < stop:
        token = tokens[index]
        text = token.text
        if token.kind == "symbol":
            if text == "<":
                angles += 1
            elif text == ">" and angles and (index == 0 or tokens[index - 1].text != "-"):
                angles -= 1
            elif text in ("(", "[") or (text == "{" and angles):
                index = pairs[index]
            elif text == "{":
                return index
            elif text == ";":
                return None
        index += 1
    return None


def generic_end(tokens, pairs, start, stop):
    depth, index = 0, start
    while index < stop:
        if tokens[index].text == "<":
            depth += 1
        elif tokens[index].text == ">" and tokens[index - 1].text != "-":
            depth -= 1
            if depth == 0:
                return index + 1
        elif tokens[index].text in ("(", "[", "{"):
            index = pairs[index]
        index += 1
    return stop


def impl_owner(tokens, pairs, start, body):
    start += 1
    if start < body and tokens[start].text == "<":
        start = generic_end(tokens, pairs, start, body)
    # A top-level `for` separates the implemented trait from the self type.
    # Ignore `for` in a trait's generic arguments, and exclude its where clause.
    depth, self_start, end, index = 0, start, body, start
    while index < body:
        text = tokens[index].text
        if text == "<":
            depth += 1
        elif text == ">" and depth and tokens[index - 1].text != "-":
            depth -= 1
        elif depth == 0 and text == "where":
            end = index
            break
        elif depth == 0 and text == "for":
            self_start = index + 1
        elif text in ("(", "[", "{"):
            index = pairs[index]
        index += 1
    return "".join(token.text for token in tokens[self_start:end] if token.kind != "doc")


def function_start(tokens, pairs, fn_index):
    """Attach only outer attributes/docs, not unrelated earlier item text."""
    index, first = fn_index - 1, tokens[fn_index].first
    qualifiers = {"pub", "const", "async", "unsafe", "extern", "default", "safe"}
    while index >= 0:
        token = tokens[index]
        if token.text in qualifiers or (token.kind == "literal" and index > 0 and tokens[index - 1].text == "extern"):
            index -= 1
        elif token.text == ")" and pairs[index] > 0 and tokens[pairs[index] - 1].text == "pub":
            index = pairs[index] - 2
        elif token.text == "]" and pairs[index] > 0 and tokens[pairs[index] - 1].text == "#":
            index = pairs[index] - 1
            first = tokens[index].first
            index -= 1
        elif token.kind == "doc":
            first = token.first
            index -= 1
        else:
            break
    return first


def functions_in_rust(source, scope):
    tokens = rust_tokens(source)
    pairs = delimiter_pairs(tokens)
    functions = []

    def walk(start, stop, owner="-"):
        index = start
        while index < stop:
            token = tokens[index]
            if token.text == "fn" and index + 2 < stop and tokens[index + 1].kind == "word":
                body = item_body(tokens, pairs, index + 2, stop)
                if body is not None:
                    end = pairs[body]
                    functions.append(Function(f"{scope}::{owner}::{tokens[index + 1].text}",
                                              function_start(tokens, pairs, index), tokens[end].last))
                    walk(body + 1, end)
                    index = end + 1
                    continue
            elif token.text in ("impl", "trait"):
                body = item_body(tokens, pairs, index + 1, stop)
                if body is not None:
                    name = impl_owner(tokens, pairs, index, body) if token.text == "impl" else tokens[index + 1].text
                    walk(body + 1, pairs[body], name)
                    index = pairs[body] + 1
                    continue
            if token.kind == "symbol" and token.text in ("(", "["):
                index = pairs[index] + 1
            elif token.kind == "symbol" and token.text == "{":
                walk(index + 1, pairs[index])
                index = pairs[index] + 1
            else:
                index += 1

    walk(0, len(tokens))
    return functions


def functions_in_python(source, scope):
    functions = []

    def walk(node, owner="-"):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            first = min([node.lineno] + [item.lineno for item in node.decorator_list])
            functions.append(Function(f"{scope}::{owner}::{node.name}", first, node.end_lineno))
            owner = "-"
        elif isinstance(node, ast.ClassDef):
            owner = node.name if owner == "-" else f"{owner}.{node.name}"
        for child in ast.iter_child_nodes(node):
            walk(child, owner)

    walk(ast.parse(source, filename=scope))
    return sorted(functions, key=lambda item: item.first)


def source_path(filename):
    parts = Path(filename).parts
    return ((len(parts) >= 4 and parts[0] == "crates" and parts[2] == "src" and filename.endswith(".rs"))
            or (len(parts) >= 2 and parts[0] == "tools" and filename.endswith(".py")))


def measure_sources(sources):
    files, functions, counts = {}, {}, Counter()
    for filename, source in sorted(sources):
        files[filename] = source.count("\n") + int(bool(source) and not source.endswith("\n"))
        scope = "/".join(Path(filename).parts[:2]) if filename.endswith(".rs") else filename
        try:
            found = functions_in_rust(source, scope) if filename.endswith(".rs") else functions_in_python(source, scope)
        except (ValueError, SyntaxError) as error:
            raise ValueError(f"{filename}: {error}") from error
        for function in found:
            counts[function.key] += 1
            suffix = f"#{counts[function.key]}" if counts[function.key] > 1 else ""
            functions[function.key + suffix] = function.lines
    return {"files": files, "functions": functions}


def measure(root):
    paths = sorted(set(root.glob("crates/*/src/**/*.rs")) | set(root.glob("tools/**/*.py")))
    return measure_sources((filename.relative_to(root).as_posix(), filename.read_text(encoding="utf-8"))
                           for filename in paths if filename.is_file())


def committed_sources(root):
    """Bootstrap from HEAD, never from unreviewed current or untracked source."""
    command = ["git", "-C", str(root)]
    existing = subprocess.run(command + ["cat-file", "-e", f"HEAD:{BASELINE}"], capture_output=True)
    if existing.returncode == 0:
        raise ValueError("the committed size baseline is missing; restore it instead of initializing it")
    listed = subprocess.run(command + ["ls-tree", "-rz", "--name-only", "HEAD", "--", "crates", "tools"],
                            capture_output=True, check=True)
    names = sorted(name for name in listed.stdout.decode().split("\0") if source_path(name))
    requests = "".join(f"HEAD:{name}\n" for name in names).encode()
    result = subprocess.run(command + ["cat-file", "--batch"], input=requests, capture_output=True, check=True)
    position, sources = 0, []
    for name in names:
        end = result.stdout.index(b"\n", position)
        header = result.stdout[position:end].split()
        if len(header) != 3 or header[1] != b"blob":
            raise ValueError(f"cannot read the committed source {name}")
        size, start = int(header[2]), end + 1
        sources.append((name, result.stdout[start:start + size].decode("utf-8")))
        position = start + size + 1
    return sources


def above_limits(measured):
    return {"limits": LIMITS, **{category: {key: size for key, size in sorted(items.items())
                                         if size > LIMITS[category[:-1]]}
                                for category, items in measured.items()}}


def read_baseline(filename):
    record = json.loads(filename.read_text(encoding="utf-8"))
    if set(record) != {"limits", "files", "functions"} or record["limits"] != LIMITS:
        raise ValueError("invalid size-baseline limits or fields")
    for category in ("files", "functions"):
        if not isinstance(record[category], dict):
            raise ValueError(f"invalid size-baseline {category}")
        if any(not isinstance(key, str) or type(size) is not int or size <= LIMITS[category[:-1]]
               for key, size in record[category].items()):
            raise ValueError(f"invalid size-baseline {category} entry")
    return record


def failures(measured, baseline):
    messages = []
    for category, items in measured.items():
        for key, size in sorted(items.items()):
            cap = baseline[category].get(key, LIMITS[category[:-1]])
            if size > cap:
                messages.append(f"{key}: {size} lines exceeds {cap}")
    return messages


def save_baseline(filename, record):
    text = json.dumps(record, indent=2, sort_keys=True) + "\n"
    if filename.is_file() and json.loads(filename.read_text(encoding="utf-8")) == record:
        return
    filename.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=filename.parent, delete=False) as output:
            temporary = Path(output.name)
            output.write(text)
        temporary.replace(filename)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def report(measured):
    for category, items in measured.items():
        limit = LIMITS[category[:-1]]
        print(f"{category}: {len(items)} total; {sum(size > limit for size in items.values())} above {limit} lines")
        for key, size in sorted(items.items(), key=lambda item: (-item[1], item[0]))[:20]:
            print(f"  {size:6} {key}")
    rust_files = {key: size for key, size in measured["files"].items() if key.startswith("crates/")}
    rust_functions = {key: size for key, size in measured["functions"].items() if key.startswith("crates/")}
    print(f"Rust: {len(rust_files)} files; {sum(size > 2000 for size in rust_files.values())} above 2000 lines; "
          f"{len(rust_functions)} functions; {sum(size > 100 for size in rust_functions.values())} above 100 lines; "
          f"{sum(size > 200 for size in rust_functions.values())} above 200 lines")


def main(argv=None, *, root=ROOT):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--update", action="store_true", help="lower or remove existing exceptions")
    parser.add_argument("--report", action="store_true", help="show totals and the twenty largest items")
    arguments = parser.parse_args(argv)
    try:
        measured = measure(root)
        filename = root / BASELINE
        if filename.is_file():
            baseline = read_baseline(filename)
        elif arguments.update:
            baseline = above_limits(measure_sources(committed_sources(root)))
        else:
            raise ValueError("size baseline is missing; use --update only for its first initialization")
        if arguments.report:
            report(measured)
        errors = failures(measured, baseline)
        for error in errors:
            print(error)
        if errors:
            return 1
        if arguments.update:
            save_baseline(filename, above_limits(measured))
        return 0
    except (OSError, ValueError, SyntaxError, subprocess.CalledProcessError) as error:
        print(f"check-size: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
