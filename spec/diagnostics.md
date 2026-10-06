# Diagnostic Presentation

**Revision 1.0.0** | **Pinned ground truth** | **2026-10-07**

## 1. Scope

This document defines diagnostic presentation. It does not change which
programs the compiler accepts, which errors it detects, or diagnostic numbers.
The diagnostic authority table remains in `spec/evoir.md` section 6.3.
Backend diagnostic meanings also remain in `spec/evomir.md`.

| Range | Area |
|---|---|
| E0xxx | Preprocessing and input |
| E1xxx | Lexing and parsing |
| E2xxx | Semantic analysis and source lowering |
| E3xxx | Execution contracts and undefined behavior |
| E4xxx | Backend validation and code generation |
| Wxxxx | Front-end lints, when implemented |

Existing exceptions keep their numbers. E0001 and E0002 are lexical errors.
E0003 reports an input failure. E0004 reports an emission failure. A range
does not prove that every diagnostic in it has the same cause. In particular,
an execution limit or an unsupported external call is not undefined behavior.
An error in command-line syntax can have no diagnostic number.

## 2. Spans and source bytes

Every diagnostic has exactly one primary span. A span has a file, a start
line and column, an end line and column, a label, and a primary flag. Lines
and columns start at 1. Columns count bytes. The end position is exclusive.
A zero-length span denotes an insertion point. A missing primary span is an
implementation error. The renderer must not guess a position.

Source snippets use the bytes read by this compilation. They do not read the
file again. This rule also applies to standard input and included files.
Invalid bytes are displayed as hexadecimal escapes, not Unicode replacement
characters. Tabs use four-column stops. Rendering does not change source bytes.

An execution diagnostic uses the origin of the executed operation. The
compiler must not locate an error by searching for a similar expression or by
running the program again. If only a function origin is available, a note must
state that the function is shown, not an exact expression position. Source
positions must not survive an IR transformation unless their correspondence
is preserved or checked.

An input or command-line error uses the affected argument in the command-line
source, named `<command-line>`. This is not a position in the C file. A
translation-unit error can refer to the complete source unit. Labels and notes
must make this distinction clear.

## 3. Text and color

`--error-format=text` is the default. For example:

```text
error[E3002]: division by zero
  --> demo.c:4:12
   |
 4 |     return a / b;
   |            ^^^^^ divisor is 0 here
   |
   = note: `b` was 0 on the executed path
   = help: check the divisor before the division
```

The header gives the level, optional code, and message. Each span has a source
location and numbered source lines. Primary spans use `-->` and `^`.
Secondary spans use `:::` and `-`. A multiline span marks each covered line.
Its label follows the last marker. An insertion point has one marker.
Notes follow the spans. Help lines follow the notes. Absent sections are omitted.

`--error-format=short` prints the legacy single-line diagnostic form.
`--color=auto` uses color only when standard error is a terminal.
`--color=always` forces color for text. `--color=never` disables color.
JSON output never contains terminal color escapes.

## 4. Message style

- Start a header with a lowercase letter. Use one sentence without a final period.
- Do not put `error:` inside a message.
- A primary label states what is wrong at its span.
- A secondary label describes related source, not a second primary error.
- A note states a fact. Do not present a guessed runtime value as a fact.
- A help line states a change the user can make. Start with an action.
- Use short sentences and the same term for the same concept.

## 5. JSON records

`--error-format=json` writes one JSON object per diagnostic per line to
standard error. Each object has exactly these keys:

| Key | Type and meaning |
|---|---|
| `code` | String such as `E3002`, or null for an unnumbered command error |
| `level` | `error` or `warning` |
| `message` | Header message, without a level or code prefix |
| `spans` | Nonempty array of span objects |
| `notes` | Array of fact strings |
| `help` | Array of action strings |
| `rendered` | The same diagnostic in color-free text form, including its final newline |

A span object has exactly `file`, `line_start`, `column_start`, `line_end`,
`column_end`, `label`, and `primary`. The file and label are strings. The four
coordinates are positive integers. `primary` is a Boolean. Exactly one span
has `primary=true`. Spans use the coordinate rules in section 2.

Empty notes and help are empty arrays. Strings use JSON escaping. Records have
no extra wrapper, embedded record separator, or trailing comma. The successful
`--check` result on standard output is not a diagnostic and keeps its separate
completion record. A completed path is not a whole-program safety proof.

These field names, types and meanings are stable. An incompatible change
requires a new major revision of this specification.

## 6. Exit status and conformance

Presentation does not change exit status. A compilation error or a failed
`--check` exits 1. A warning alone does not cause failure. Proof-required mode
keeps the exit contract of `spec/verification.md`; this specification does not
change it. A diagnostic must not convert an unsupported execution into a pass.

UI tests compare text and JSON byte for byte. Each JSON record must also pass
an independent field and type check. `UPDATE_EXPECT=1` updates the snapshots
only when explicitly requested. Tests cover all ranges, secondary spans,
multiline spans, notes, help, multiple diagnostics, and included source.
