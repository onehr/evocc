#!/usr/bin/env python3
"""Serialize C preprocessing tokens without trusting either compiler."""

from __future__ import annotations

import json
import sys


PUNCTUATORS = sorted(
    {
        "%:%:",
        ">>=",
        "<<=",
        "...",
        "##",
        "->",
        "++",
        "--",
        "<<",
        ">>",
        "<=",
        ">=",
        "==",
        "!=",
        "&&",
        "||",
        "*=",
        "/=",
        "%=",
        "+=",
        "-=",
        "&=",
        "^=",
        "|=",
        "<:",
        ":>",
        "<%",
        "%>",
        "%:",
        "[",
        "]",
        "(",
        ")",
        "{",
        "}",
        ".",
        "&",
        "*",
        "+",
        "-",
        "~",
        "!",
        "/",
        "%",
        "<",
        ">",
        "^",
        "|",
        "?",
        ":",
        ";",
        "=",
        ",",
        "#",
    },
    key=len,
    reverse=True,
)


def is_identifier_start(char: str) -> bool:
    return char == "_" or char.isalpha() or ord(char) >= 128


def is_identifier_continue(char: str) -> bool:
    return is_identifier_start(char) or char.isdigit()


def scan_quoted(source: str, start: int, quote: str) -> int:
    cursor = start + 1
    while cursor < len(source):
        if source[cursor] == "\\":
            cursor += 2
            continue
        if source[cursor] == quote:
            return cursor + 1
        if source[cursor] in "\r\n":
            raise ValueError("newline in quoted preprocessing token")
        cursor += 1
    raise ValueError("unterminated quoted preprocessing token")


def scan_pp_number(source: str, start: int) -> int:
    cursor = start
    previous = ""
    while cursor < len(source):
        char = source[cursor]
        if char.isalnum() or char in "._":
            previous = char
            cursor += 1
            continue
        if char in "+-" and previous in "eEpP":
            previous = char
            cursor += 1
            continue
        break
    return cursor


def tokenize(source: str) -> list[dict[str, str]]:
    tokens: list[dict[str, str]] = []
    cursor = 0
    while cursor < len(source):
        char = source[cursor]
        if char.isspace():
            cursor += 1
            continue
        if source.startswith("//", cursor):
            newline = source.find("\n", cursor + 2)
            cursor = len(source) if newline < 0 else newline + 1
            continue
        if source.startswith("/*", cursor):
            close = source.find("*/", cursor + 2)
            if close < 0:
                raise ValueError("unterminated block comment in preprocessed output")
            cursor = close + 2
            continue

        prefix_len = 0
        for prefix in ("u8", "u", "U", "L"):
            if source.startswith(prefix, cursor) and cursor + len(prefix) < len(source):
                if source[cursor + len(prefix)] in "\"'":
                    prefix_len = len(prefix)
                    break
        quote_at = cursor + prefix_len
        if quote_at < len(source) and source[quote_at] in "\"'":
            end = scan_quoted(source, quote_at, source[quote_at])
            tokens.append({"kind": "quoted", "spelling": source[cursor:end]})
            cursor = end
            continue

        if is_identifier_start(char):
            end = cursor + 1
            while end < len(source) and is_identifier_continue(source[end]):
                end += 1
            tokens.append({"kind": "identifier", "spelling": source[cursor:end]})
            cursor = end
            continue

        if char.isdigit() or (char == "." and cursor + 1 < len(source) and source[cursor + 1].isdigit()):
            end = scan_pp_number(source, cursor)
            tokens.append({"kind": "pp-number", "spelling": source[cursor:end]})
            cursor = end
            continue

        punctuator = next((item for item in PUNCTUATORS if source.startswith(item, cursor)), None)
        if punctuator is None:
            raise ValueError(f"unrecognized preprocessing byte at offset {cursor}")
        tokens.append({"kind": "punctuator", "spelling": punctuator})
        cursor += len(punctuator)
    return tokens


def main() -> int:
    try:
        source = sys.stdin.read()
        for token in tokenize(source):
            print(json.dumps(token, sort_keys=True, separators=(",", ":")))
    except (UnicodeError, ValueError) as error:
        print(f"cpp-token-serializer: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
