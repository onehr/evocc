"""Validate the diagnostic contract without a schema dependency."""
import json


def validate(record):
    assert type(record) is dict
    assert set(record) == {"code", "level", "message", "spans", "notes", "help", "rendered"}
    code = record["code"]
    assert code is None or (
        type(code) is str and len(code) == 5
        and code[0] in "EW" and code[1:].isascii() and code[1:].isdigit()
    )
    assert record["level"] in {"error", "warning"}
    assert type(record["message"]) is str and record["message"]
    assert type(record["rendered"]) is str and record["rendered"].endswith("\n")
    assert "\x1b" not in record["rendered"]
    for name in ("notes", "help"):
        assert type(record[name]) is list
        assert all(type(item) is str and item for item in record[name])
    spans = record["spans"]
    assert type(spans) is list and spans
    assert sum(span["primary"] is True for span in spans) == 1
    for span in spans:
        assert set(span) == {
            "file", "line_start", "column_start", "line_end", "column_end", "label", "primary"
        }
        assert type(span["primary"]) is bool
        assert type(span["file"]) is str and span["file"]
        assert type(span["label"]) is str and span["label"]
        for name in ("line_start", "column_start", "line_end", "column_end"):
            assert type(span[name]) is int and span[name] > 0
        assert (span["line_end"], span["column_end"]) >= (
            span["line_start"], span["column_start"]
        )
