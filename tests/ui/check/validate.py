"""Validate the diagnostic contract without a schema dependency."""
# The Rust tests that embed this file call json in the code that they append.
import json  # noqa: F401


def validate(record):
    assert type(record) is dict
    assert set(record) == {"code", "level", "message", "spans", "notes", "help", "rendered", "repairs"}
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
    repairs = record["repairs"]
    assert type(repairs) is list
    for repair in repairs:
        assert set(repair) == {"repair_id", "kind", "file", "span", "replacement", "title"}
        assert repair["repair_id"] in {
            "W0001.remove-declaration", "W0002.void-cast", "W0003.insert-prototype",
            "W0004.insert-int", "W0005.return-zero",
        }
        assert repair["kind"] in {"safe", "review"}
        assert repair["file"] == repair["span"]["file"]
        assert type(repair["replacement"]) is str
        assert type(repair["title"]) is str and repair["title"]
    for span in spans + [repair["span"] for repair in repairs]:
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
