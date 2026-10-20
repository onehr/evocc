# Diagnostic Presentation

**Revision 1.4.1** | **Pinned ground truth** | **2026-10-16**

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

An input or command-line diagnostic uses the affected argument in the
command-line source, named `<command-line>`. This source is one line: the
arguments after the program name, separated by spaces. It is not a position in
the C file. An ignored option is the affected argument of its warning. A
diagnostic without one affected argument covers the complete invocation. A
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

A build can pass a hundred arguments to each compiler call. When the
`<command-line>` line is longer than 80 bytes and a span covers only part of
it, the text form shows only the bytes of that span. `...` stands for the
omitted bytes before and after them, and the markers stay under the shown
bytes. The location line and the JSON coordinates still count from the start
of the complete line. Source files are always shown in full. Revision 1.3.0
adds this rule.

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
| `repairs` | Array of source repairs, empty when no supported edit is available |

A span object has exactly `file`, `line_start`, `column_start`, `line_end`,
`column_end`, `label`, and `primary`. The file and label are strings. The four
coordinates are positive integers. `primary` is a Boolean. Exactly one span
has `primary=true`. Spans use the coordinate rules in section 2.

Empty notes and help are empty arrays. Strings use JSON escaping. Records have
no extra wrapper, embedded record separator, or trailing comma. The successful
`--check` result on standard output is not a diagnostic and keeps its separate
completion record. A completed path is not a whole-program safety proof.

A completed check states its scope in text and in JSON. The JSON completion
record has a `scope` string. The text form ends with the same three sentences:

```text
No finding from the implemented checks on the executed path. Pointer subtraction and ordering obligations are not checked. This does not check that the results are correct.
```

This statement applies only to the implemented checks on the executed path.
It does not claim complete UB detection or correct program results. Revision
1.4.1 names the pointer-relation exclusions specified in EvoIR 10.1.3;
revision 1.2.0 added `scope`. The record keeps all
other fields and values, including `claim` with the value `executed-path-only`.

These field names, types and meanings are stable. An incompatible change
requires a new major revision of this specification.

### 5.1. Source repairs

Each repair has exactly these fields. The 1.0.0 diagnostic fields keep their
types and meanings. The text form adds a `help:` line for each safe repair,
including its JSON-escaped replacement. Review repairs remain in the JSON.

| Field | Type and meaning |
|---|---|
| `repair_id` | Stable string from the table below |
| `kind` | `safe` or `review` |
| `file` | Source file to edit; equal to `span.file` |
| `span` | A span as defined in section 2; its end is exclusive |
| `replacement` | String to put in place of the span; empty means deletion |
| `title` | One action sentence |

`safe` means the edit cannot change observable program behavior. All other
edits require review. A constant initializer is not sufficient when removal
would discard volatile access, a runtime bound, a cleanup, or an observable
floating-environment effect. Macro expansion positions are not editable
spelling. Do not invent an edit when source spelling or a type is unavailable.

| ID | Edit | Normal kind |
|---|---|---|
| `W0001.remove-declaration` | Remove an unused variable with a constant initializer | safe; otherwise review |
| `W0002.void-cast` | Insert `(void)NAME;` as the first statement | safe; volatile or atomic access requires review |
| `W0003.insert-prototype` | Insert a prototype before the enclosing function, using promoted call argument types | review |
| `W0004.insert-int` | Insert `int` at the missing type | safe |
| `W0005.return-zero` | Insert `return 0;` before the closing brace | review |

Retain other declarators and any shared type definition. Adjacent unused
declarators can share one edit; it is safe only when every removed initializer
is safe. Deduplicate identical edits. Reject overlapping edits before writing.

`evo fix FILE.c --plan [--json]` only reports. `--apply` applies safe edits;
`--apply --review` also applies review edits. Edit only FILE.c, in descending
byte order. Reject parse errors before any write. Require unchanged input
bytes. At the same span, the prototype precedes the implicit `int` insertion.
Run the syntax check again. Restore the input if that check fails.
This check does not execute the program or prove the proposed edit correct.

JSON is one object on stdout with `file` (string), `repairs` (array),
`diagnostics` (array), `applied` (integer), and `exit_code` (integer).
Plan diagnostics describe the input; apply diagnostics describe the post-check
when edits were attempted. Failed or rolled-back edits have `applied=0`.
The compiler bridge is `--parse-only --repair=plan|safe|review`; its
`--error-format=json` form uses the same result record.

### 5.2. Concise check record

`evo check FILE.c --json --form=concise-1` writes one `concise-1` record on one
line. It replaces the check record, the explanation records and the plan
record. No `--form` and `--form=full-1` keep the records above byte for byte.
`full-1` stays the default. A replay selects `--form=full-1`. A change of the
default requires a new major revision of this specification.

`--form` requires `check` and `--json`, once, with the value `full-1` or
`concise-1`. Any other use is a command error. The MCP `check` tool takes an
optional `form` string with the same two values. Another value is error
-32602. Omission is `full-1`.

The record has these keys in this order. An absent key is omitted, not null.

| Key | When present | Value |
|---|---|---|
| `form` | always | `"concise-1"` |
| `status` | always | `"completed"` when main returned; otherwise `"stopped"` |
| `main_return` | completed | The `main_return` string of the completion record |
| `scope` | completed | The `scope` sentence of the completion record |
| `diagnostics` | at least one diagnostic | Concise diagnostics in report order |
| `repairs` | the plan has repairs | The repairs of `evo fix FILE.c --plan --json`, in concise form |
| `exit_code` | always | The process status of `evo`, not of the program |

`stopped` covers findings, limits, unsupported operations and errors. The
notes keep the category statement of a code that is not undefined behavior.
The record omits `rendered`, the span objects, the rest of each explanation
page and the plan diagnostics. `full-1` and `evo explain CODE` keep them.

A concise diagnostic has these keys in this order:

| Key | When present | Value |
|---|---|---|
| `code` | always | The code, or null for an unnumbered command error |
| `message` | always | The `message` of the JSON record |
| `level` | the level is not `error` | The level |
| `at` | always | `PATH:LINE:COLUMN` of the primary span start |
| `end` | the primary span covers more than one line | `LINE:COLUMN` of the exclusive end |
| `source` | the primary span has text on its first line | That text, shown as in the text form |
| `label` | the primary label differs from `message` | The primary label |
| `also` | there are secondary spans | Strings `PATH:LINE:COLUMN LABEL` |
| `notes` | a note remains | The notes except `this operation failed on the executed path` |
| `help` | nonempty | The `help` strings of the JSON record |
| `why` | the code has an explanation page | The first line of the first paragraph of the page |
| `fix` | the hint table has the code | The repair hint of the code |
| `repairs` | the diagnostic has repairs | Concise repairs |

`source` has at most 80 characters. A longer text keeps its first 77
characters and `...`. ` ...` follows when the span continues on a later line.
A concise repair has `id`, `kind`, `title`, `at`, `end` and `replacement` in
this order. `id` is the `repair_id`. `at` and `end` give the whole edit span.
For an insertion they name the same position.

`PATH` is relative to the current directory when the canonical file path is
below the canonical current directory. Otherwise it is the reported file.
Compare canonical paths: on macOS, `/var` and `/private/var` name the same
directory. Strings use the escapes of section 5.

`crates/evocc/explain/hints.txt` has one `CODE<TAB>HINT` line for each code
with a usable action. A hint is one action sentence of at most 120 bytes. The
build refuses a code outside the authority table and a code listed twice. The
explanation pages do not change, so `evo explain` keeps its text.

The compiler writes concise diagnostics for `--error-format=concise`. Its
completed `--check` result is the object
`{"status":"completed","main_return":"N","scope":SCOPE}`. `--repair` with
`--error-format=concise` is a command error. Revision 1.4.0 adds this section.

## 6. Exit status and conformance

Presentation does not change exit status. A compilation error or a failed
`--check` exits 1. A warning alone does not cause failure. Proof-required mode
keeps the exit contract of `spec/verification.md`; this specification does not
change it. A diagnostic must not convert an unsupported execution into a pass.

UI tests compare text and JSON byte for byte. Each JSON record must also pass
an independent field and type check. `UPDATE_EXPECT=1` updates the snapshots
only when explicitly requested. Tests cover all ranges, secondary spans,
multiline spans, notes, help, multiple diagnostics, and included source.
