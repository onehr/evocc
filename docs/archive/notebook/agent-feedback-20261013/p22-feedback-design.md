# P22: a concise check record for the agent interface

This note measures the check feedback that the agent interface returns. It
designs a shorter, versioned record that keeps the code, the location, the
one-line explanation and the repair hint. It lists the code changes for the
Rust lane. It changes no code, evidence or task file.

## Sources and method

- Repository commit at analysis time: `6b1e0f10`. The rendering files named
  below have no working-tree change at that commit.
- Experiment 4: `tests/contracts/agent-repair-4-20261009.json` (SHA-256
  `a7e8c5a95a440e65b6941a378a53704385cc35af857506a285d9fb8b746e3a05`) and
  `agent-repair-4-wire-20261009.tar.gz`
  (`bf10955a4b62af68e540a674f6c61b9cb682b9657f494bda815770644febb52f`).
- Experiment 5: `agent-repair-5-replication-20261010.json`
  (`c46b15e2629b49fc52dd4451dce01d411201c446a3c3fe826433beadfa53c3fd`),
  `agent-repair-5-replication-wire-20261010.tar.gz`
  (`e7f37cbfef1992b4763fab46b990aa8aaa21f12de1af863258da4ec03c138f83`),
  `agent-repair-5-scale-20261010.json`
  (`7dfefb95e0c1c666e1bde80786b9c0828164c448300f5c0a262277bf4b65e702`) and
  `agent-repair-5-scale-wire-20261010.tar.gz`
  (`84d31cfc2f4c4a04542ed3e144489e0d1a209a3c46cc3b5b8d9671c3dca4dd62`).
- Frozen C3 feedback: `tests/fixtures/agent-repair-4/qualification.json`
  (`9c98c41cc4d9942a013e49bfa07181b112382c6e2d29a6168f7df99e19471fef`).
- The wire archives were unpacked outside the repository. A check result is
  the output of a Bash tool call that runs `check FILE.c`. The receipts count
  528 check requests; 515 results match that pattern, 218 of them in C3.
  217 C3 results contain the `=== evo diagnostics` block.
- Bytes are UTF-8 bytes of the exact text that the agent received.
- Tokens use two estimates. The simple one is bytes / 4. The calibrated one
  is bytes / 3.17: P17 prompts for the same item with EvoCC and GCC feedback
  differ by 1,371 bytes and 433 DeepSeek tokens. A step-to-step fit on the
  Kimi usage records gives about 3.16 bytes per token. That fit has a large
  negative intercept, so it is only a rough check.
- No model or API request was made. No Cargo command ran. `evo` was read,
  not run. The concise sizes come from a scratch reference model of the rules
  below, applied to every retained C3 block.

## Findings

1. With a finding, the EvoCC part of a C3 check result averaged 1,718 bytes
   (about 542 tokens). The proposed `concise-1` record averages 355 bytes
   (about 112 tokens): 79.4% less.
2. A clean check averaged 424 bytes; `concise-1` needs 189 bytes: 55.4% less.
3. Over all 217 retained C3 blocks, the size falls from 226,610 to 58,229
   bytes: 74.3% less.
4. The size of the feedback is a small part of session cost. Every agent
   request starts with about 17,720 tokens of fixed context. That context is
   about 90% of all C3 tokens. Under the observed trajectories, the C3 blocks
   carried 249,686 tokens (2.3%); `concise-1` would carry 57,052 (0.5%).
5. In experiment 4, C3 used 65,937 more tokens than C1 on the six controls.
   Check output explains at most 10,342 of them. Three extra steps explain at
   least 53,157. Neither experiment isolates a causal effect of length.
6. Recommendation: add `concise-1` as an explicitly selected form. Keep
   `full-1` as the default and byte-identical for replays.

## How the current feedback is produced

| Stage | Code at `6b1e0f10` | Output |
| --- | --- | --- |
| Diagnostic | `crates/evocc/src/diagnostics.rs`, `render()` (line 296) with `Format::Json` | One object per diagnostic: `code`, `level`, `message`, `spans`, `notes`, `help`, `rendered`, `repairs`. `rendered` is the whole text form. |
| Executed-path note | `crates/evocc/src/main.rs`, line 2293 | Adds `this operation failed on the executed path`, or the note for an imprecise location. |
| Clean check | `Format::completion()` (line 56) | `schema`, `kind`, `status`, `main_return`, `claim`, and the 103-byte `scope` sentence. |
| `evo check --json` | `crates/evocc/src/bin/evo.rs`, `execute()`, lines 552 to 584 | Parses the compiler lines and prints `{exit_code, completion, diagnostics}`. `json::Json::render()` sorts keys. `quote()` (line 37) writes each newline as `\u000a`, six bytes. |
| `evo explain --json` | `evo.rs`, lines 588 to 603; `diagnostics::explain()` (line 442) | `{code, title, text, exit_code}`; `text` is the whole page without fence lines. |
| `evo fix --plan --json` | `evo.rs`, `fix()` (line 100); `crates/evocc/src/repairs.rs` | `{file, repairs, diagnostics, applied, exit_code}`. Plan diagnostics repeat the check's syntax and semantic errors with their own `rendered`. |
| MCP `check` | `crates/evocc/src/bin/evo/mcp.rs`, `call()` (line 114) | The record once as escaped text content and once as `structuredContent`. |
| C3 harness | `tools/experiment/agent_repair.py`, `c3_diagnostics()` | Build and run header, then the check record, one explain record per code, and the plan record. |

The MCP specification asks for the text copy of structured content, so the
double copy is not a defect. Whether a client shows both copies to a model
depends on the client.

## Measured size in experiments 4 and 5

| Source | C3 blocks | With a finding | Mean bytes, finding | Mean bytes, clean | Total bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Experiment 4 | 46 | 21 | 1,533 | 424 | 42,783 |
| Experiment 5, part A | 104 | 49 | 1,795 | 424 | 111,279 |
| Experiment 5, part B | 67 | 34 | 1,722 | 424 | 72,548 |
| All | 217 | 104 | 1,718 | 424 | 226,610 |

Composition of whole C3 check results, including the 110 to 133 byte
harness header: the check record is 46% to 49%, explain records 24% to 26%,
the plan record 15% to 18% and the header 11%. Inside the check record,
`rendered` is 15% to 19% of all bytes and `spans` 9% to 10%.

One finding repeats the same facts many times. In the `adler-byte` E3011
block (1,475 bytes):

- `out-of-bounds-memory-access` occurs 6 times and `E3011` 4 times.
- The 81-byte scratch path occurs 3 times.
- The location occurs in `rendered` and in `spans`.
- The executed-path note occurs twice.
- The plan record (149 bytes) has no repair and no diagnostic.
- `rendered` (344 bytes) and the page text (468 bytes) carry 18 newline
  escapes of 6 bytes each.

Two cases are larger. E3008 and E3039 point at a whole function, so
`rendered` copies every line of it: 2,280 and 1,970 bytes on average. A
syntax or semantic error appears in the check record and again in the plan:
E2002 averages 2,850 bytes.

## Where the session tokens go

The accounting below keeps the observed trajectories fixed. A check result
stays in the context of every later request. Its carried cost is its tokens
times the number of later requests. This is accounting, not a prediction: a
different form can change what the agent does.

| C3 sessions | Sessions | Steps | Tokens | Blocks | Block bytes, `full-1` | Block bytes, `concise-1` | Carried tokens, `full-1` | Carried tokens, `concise-1` | Saving |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Exp. 4 controls | 6 | 30 | 568,169 | 12 | 12,314 | 3,299 | 10,049 | 2,407 | 1.35% |
| Exp. 4 silent | 18 | 89 | 1,705,853 | 34 | 30,469 | 8,433 | 24,488 | 5,936 | 1.09% |
| Exp. 5 A controls | 12 | 60 | 1,077,325 | 22 | 21,286 | 5,871 | 18,478 | 4,451 | 1.30% |
| Exp. 5 A silent | 36 | 206 | 3,845,772 | 82 | 89,993 | 22,589 | 107,901 | 24,197 | 2.18% |
| Exp. 5 B silent | 24 | 156 | 3,465,780 | 67 | 72,548 | 18,037 | 88,771 | 20,062 | 1.98% |
| All | 96 | 541 | 10,662,899 | 217 | 226,610 | 58,229 | 249,686 | 57,052 | 1.81% |

The first request of every session had 17,719 or 17,720 input tokens. Over
541 C3 steps, that fixed context alone is 9.59 million tokens: 89.9% of all
C3 tokens. Steps, not feedback bytes, drive the token totals.

Experiment 4 controls: C3 used 30 steps and 568,169 tokens; C1 used 27 steps
and 502,232 tokens. Carried check output was 11,012 tokens in C3 and 670 in
C1. Experiment 5 part A controls reversed the sign: 60 against 63 steps and
1,077,325 against 1,120,150 tokens.

## The `concise-1` record

`evo check FILE.c --json --form=concise-1` prints one JSON object on one
line. It replaces the C3 bundle of check, explain and plan records. Keys
appear in the order below. Absent keys are omitted, not null.

| Key | When present | Value |
| --- | --- | --- |
| `form` | always | `"concise-1"` |
| `status` | always | `"completed"` when main returned; otherwise `"stopped"` |
| `main_return` | completed | The `full-1` `completion.main_return` string |
| `scope` | completed | The exact `full-1` `scope` sentence |
| `diagnostics` | at least one | Concise diagnostics in report order |
| `repairs` | the plan has repairs | Concise repairs from `evo fix --plan` |
| `exit_code` | always | The process status, as in every `evo` record |

A concise diagnostic has these keys, in this order:

| Key | When present | Value |
| --- | --- | --- |
| `code` | always | The code, or null for a command error |
| `message` | always | The `full-1` `message` |
| `level` | not `error` | The level, for example `warning` |
| `at` | always | `PATH:LINE:COLUMN` of the primary span start |
| `end` | multiline primary span | `LINE:COLUMN` of the exclusive end |
| `source` | the span has text | The primary span text on its first line, shown as in the text form; at most 80 characters, cut to 77 plus `...`; ` ...` added when the span continues |
| `label` | label differs from `message` | The primary label |
| `also` | secondary spans | Strings `PATH:LINE:COLUMN LABEL` |
| `notes` | a note remains | Notes without the exact note `this operation failed on the executed path` |
| `help` | nonempty | The `full-1` help strings |
| `why` | the code has a page | The first line of the page's first paragraph |
| `fix` | the hint table has the code | The one-line repair hint |
| `repairs` | the diagnostic has repairs | Concise repairs |

A concise repair has `id`, `kind` (`safe` or `review`), `title`, `at`, `end`
and `replacement`. `at` and `end` give the whole edit span.

`PATH` is relative to the current directory when the canonical file path is
below the canonical current directory. Otherwise it is the reported path.
Compare canonical paths: on macOS `/var` and `/private/var` name the same
directory.

The record omits these `full-1` items. Each is constant, empty, repeated, or
available on request:

- `rendered`, the span objects and absolute paths: in `full-1`.
- The rest of the explanation page and its examples: `evo explain CODE`.
- `title` and `code` of the explain record: equal to `message` and `code`.
- `completion: null`, `level: "error"`, empty `notes`, `help` and `repairs`,
  the plan's `file`, `applied` and `exit_code`, and the completion `schema`,
  `kind` and `claim`. `form` and `status` replace them.
- Plan diagnostics. They occurred in 12 of the 217 blocks and only repeated
  check diagnostics. A lint warning without a repair stays available through
  `evo fix --plan`.

The `status` value `stopped` covers findings, limits, unsupported operations
and syntax errors. The notes keep the category statement of non-UB codes,
for example the E3008 resource-limit note. `status` states the check result
in words. `exit_code` stays, because every `evo` record has it and the MCP
server compares it with the process status. It is the status of `evo`, not
of the program. Trial 266 of P17 read `"exit_code":1` as the program's exit
status.

`why` and `fix` need a data source. The first line of each page is a usable
`why`. The last line of the second paragraph is an action in 97 of the 126
pages. In the other 29 it is a statement, for example in W0001 to W0005,
E3006, E3021 and the withdrawn codes. Add a separate table, one hint per
code. Seed it with the existing page sentence where that sentence is an
action, and write the other hints by hand. Do not edit the pages:
`evo explain` text is part of the frozen `full-1` feedback.

The P22 hint for E3004 copies the current page sentence. A different E3004
text is the P17a question and needs its own registration.

## Five diagnostics, before and after

Before: the exact EvoCC part of the frozen C3 feedback for `program.c` in
`tests/fixtures/agent-repair-4/qualification.json`. The harness header before
it is unchanged in both forms and is not counted. After: the `concise-1`
record for the same facts, with the seed hints. Byte counts include the final
newline.

| Item | Code | Before, bytes | After, bytes | Reduction | Tokens, bytes/3.17 | Tokens, bytes/4 |
| --- | --- | ---: | ---: | ---: | --- | --- |
| `adler-byte` | E3011 | 1,475 | 323 | 78.1% | 465 to 102 | 369 to 81 |
| `adler-reset` | E3001 | 1,461 | 318 | 78.2% | 461 to 100 | 365 to 80 |
| `adler-view` | E3010 | 1,554 | 332 | 78.6% | 490 to 105 | 388 to 83 |
| `increment-limit` | E3004 | 1,660 | 339 | 79.6% | 524 to 107 | 415 to 85 |
| `mean-empty` | E3002 | 1,638 | 391 | 76.1% | 517 to 123 | 410 to 98 |

### E3011, `adler-byte`

Before:

```text
{"exit_code":1,"completion":null,"diagnostics":[{"code":"E3011","help":[],"level":"error","message":"out-of-bounds-memory-access","notes":["this operation failed on the executed path"],"rendered":"error[E3011]: out-of-bounds-memory-access\u000a  --> /var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-xk1zrbhz/input.c:36:12\u000a    |\u000a 36 |     return view->data[index + 1];\u000a    |            ^^^^^^^^^^^^^^^^^^^^^ out-of-bounds-memory-access\u000a    |\u000a   = note: this operation failed on the executed path\u000a","repairs":[],"spans":[{"column_end":33,"column_start":12,"file":"/var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-xk1zrbhz/input.c","label":"out-of-bounds-memory-access","line_end":36,"line_start":36,"primary":true}]}]}
{"code":"E3011","title":"out-of-bounds-memory-access","text":"# E3011: out-of-bounds-memory-access\u000a\u000aThe executed access extends outside the object's permitted storage.\u000aA one-past pointer can describe an endpoint but cannot be dereferenced.\u000a\u000aC accesses must stay within the applicable object bounds.\u000aCheck indices and access sizes before reading or writing.\u000a\u000aint main(void) { int values[2] = {0}; return values[2]; }\u000a\u000aint main(void) { int values[2] = {0}; return values[1]; }\u000a","exit_code":0}
{"file":"/var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-xk1zrbhz/input.c","repairs":[],"diagnostics":[],"applied":0,"exit_code":0}
```

After:

```text
{"form":"concise-1","status":"stopped","diagnostics":[{"code":"E3011","message":"out-of-bounds-memory-access","at":"input.c:36:12","source":"view->data[index + 1]","why":"The executed access extends outside the object's permitted storage.","fix":"Check indices and access sizes before reading or writing."}],"exit_code":1}
```

### E3001, `adler-reset`

Before:

```text
{"exit_code":1,"completion":null,"diagnostics":[{"code":"E3001","help":[],"level":"error","message":"read-of-uninitialized-object","notes":["this operation failed on the executed path"],"rendered":"error[E3001]: read-of-uninitialized-object\u000a  --> /var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-xips3xpu/input.c:31:5\u000a    |\u000a 31 |     view->seed += seed;\u000a    |     ^^^^^^^^^^^^^^^^^^ read-of-uninitialized-object\u000a    |\u000a   = note: this operation failed on the executed path\u000a","repairs":[],"spans":[{"column_end":23,"column_start":5,"file":"/var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-xips3xpu/input.c","label":"read-of-uninitialized-object","line_end":31,"line_start":31,"primary":true}]}]}
{"code":"E3001","title":"read-of-uninitialized-object","text":"# E3001: read-of-uninitialized-object\u000a\u000aThe executed path reads object bytes that have not been initialized.\u000aA declaration without an initializer does not give an automatic scalar a value.\u000a\u000aEvoCC tracks initialized bytes instead of substituting zero for missing data.\u000aInitialize the object on every path before reading it.\u000a\u000aint main(void) { int value; return value; }\u000a\u000aint main(void) { int value = 0; return value; }\u000a","exit_code":0}
{"file":"/var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-xips3xpu/input.c","repairs":[],"diagnostics":[],"applied":0,"exit_code":0}
```

After:

```text
{"form":"concise-1","status":"stopped","diagnostics":[{"code":"E3001","message":"read-of-uninitialized-object","at":"input.c:31:5","source":"view->seed += seed","why":"The executed path reads object bytes that have not been initialized.","fix":"Initialize the object on every path before reading it."}],"exit_code":1}
```

### E3010, `adler-view`

Before:

```text
{"exit_code":1,"completion":null,"diagnostics":[{"code":"E3010","help":[],"level":"error","message":"dangling-pointer-access","notes":["this operation failed on the executed path"],"rendered":"error[E3010]: dangling-pointer-access\u000a  --> /var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-3bmpth4c/input.c:43:12\u000a    |\u000a 43 |     return view->data[index];\u000a    |            ^^^^^^^^^^^^^^^^^ dangling-pointer-access\u000a    |\u000a   = note: this operation failed on the executed path\u000a","repairs":[],"spans":[{"column_end":29,"column_start":12,"file":"/var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-3bmpth4c/input.c","label":"dangling-pointer-access","line_end":43,"line_start":43,"primary":true}]}]}
{"code":"E3010","title":"dangling-pointer-access","text":"# E3010: dangling-pointer-access\u000a\u000aThe pointer refers to an object whose lifetime has ended.\u000aKeeping the address does not keep the object alive.\u000a\u000aAn access requires live storage, not just an address that was once valid.\u000aKeep the object alive until its last use or copy its value before leaving its scope.\u000a\u000aint main(void) {\u000a    int *p;\u000a    { int value = 0; p = &value; }\u000a    return *p;\u000a}\u000a\u000aint main(void) {\u000a    int result;\u000a    { int value = 0; result = value; }\u000a    return result;\u000a}\u000a","exit_code":0}
{"file":"/var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-3bmpth4c/input.c","repairs":[],"diagnostics":[],"applied":0,"exit_code":0}
```

After:

```text
{"form":"concise-1","status":"stopped","diagnostics":[{"code":"E3010","message":"dangling-pointer-access","at":"input.c:43:12","source":"view->data[index]","why":"The pointer refers to an object whose lifetime has ended.","fix":"Keep the object alive until its last use or copy its value before leaving its scope."}],"exit_code":1}
```

### E3004, `increment-limit`

Before:

```text
{"exit_code":1,"completion":null,"diagnostics":[{"code":"E3004","help":[],"level":"error","message":"signed-integer-overflow","notes":["this operation failed on the executed path"],"rendered":"error[E3004]: signed-integer-overflow\u000a  --> /var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-nxpe8loe/input.c:39:12\u000a    |\u000a 39 |     return value + 1;\u000a    |            ^^^^^^^^^ signed-integer-overflow\u000a    |\u000a   = note: this operation failed on the executed path\u000a","repairs":[],"spans":[{"column_end":21,"column_start":12,"file":"/var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-nxpe8loe/input.c","label":"signed-integer-overflow","line_end":39,"line_start":39,"primary":true}]}]}
{"code":"E3004","title":"signed-integer-overflow","text":"# E3004: signed-integer-overflow\u000a\u000aAn executed signed integer operation has a result outside its permitted range.\u000aNegating the minimum value and dividing it by minus one can also cause overflow.\u000a\u000aThe checked profile reports overflow instead of assuming signed arithmetic wraps.\u000aCheck the range before the operation, or use a suitable wider arithmetic type.\u000a\u000aint main(void) { int value = 2147483647; return value + 1; }\u000a\u000aint main(void) {\u000a    int value = 2147483647;\u000a    long result = (long)value + 1;\u000a    return result != 2147483648L;\u000a}\u000a\u000aThe conversion must occur before addition, not after an overflowing result.\u000a","exit_code":0}
{"file":"/var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-nxpe8loe/input.c","repairs":[],"diagnostics":[],"applied":0,"exit_code":0}
```

After:

```text
{"form":"concise-1","status":"stopped","diagnostics":[{"code":"E3004","message":"signed-integer-overflow","at":"input.c:39:12","source":"value + 1","why":"An executed signed integer operation has a result outside its permitted range.","fix":"Check the range before the operation, or use a suitable wider arithmetic type."}],"exit_code":1}
```

### E3002, `mean-empty`

Before:

```text
{"exit_code":1,"completion":null,"diagnostics":[{"code":"E3002","help":["check the divisor before the division or remainder operation"],"level":"error","message":"integer-division-by-zero","notes":["this operation failed on the executed path"],"rendered":"error[E3002]: integer-division-by-zero\u000a  --> /var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-o8otu9_o/input.c:39:12\u000a    |\u000a 39 |     return value / (int)parameter;\u000a    |            ^^^^^^^^^^^^^^^^^^^^^^ integer-division-by-zero\u000a    |\u000a   = note: this operation failed on the executed path\u000a   = help: check the divisor before the division or remainder operation\u000a","repairs":[],"spans":[{"column_end":34,"column_start":12,"file":"/var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-o8otu9_o/input.c","label":"integer-division-by-zero","line_end":39,"line_start":39,"primary":true}]}]}
{"code":"E3002","title":"integer-division-by-zero","text":"# E3002: integer-division-by-zero\u000a\u000aThe executed integer division has a zero divisor.\u000aThe numerator does not make this operation valid, even when it is zero.\u000a\u000aC does not define integer division by zero.\u000aCheck the divisor and choose the required error behavior before dividing.\u000a\u000aint main(void) { int divisor = 0; return 1 / divisor; }\u000a\u000aint main(void) {\u000a    int divisor = 0;\u000a    if (divisor == 0) return 0;\u000a    return 1 / divisor;\u000a}\u000a","exit_code":0}
{"file":"/var/folders/q3/pl6qzbdd491g0ydch_kc2v4c0000gn/T/silent-feedback-o8otu9_o/input.c","repairs":[],"diagnostics":[],"applied":0,"exit_code":0}
```

After:

```text
{"form":"concise-1","status":"stopped","diagnostics":[{"code":"E3002","message":"integer-division-by-zero","at":"input.c:39:12","source":"value / (int)parameter","help":["check the divisor before the division or remainder operation"],"why":"The executed integer division has a zero divisor.","fix":"Check the divisor and choose the required error behavior before dividing."}],"exit_code":1}
```

E3002 keeps both the diagnostic help and the page hint. The page hint adds
"choose the required error behavior". Both conditions failed `mean-empty` by
choosing the wrong result for empty input.

### Other shapes

A clean check (`adler-byte`, `fixed.c`): 421 bytes before, 189 after.

```text
{"form":"concise-1","status":"completed","main_return":"16","scope":"No undefined behavior was found on the executed path. This does not check that the results are correct.","exit_code":0}
```

A whole-function span (experiment 5, part A, session 05): 2,795 bytes before,
552 after.

```text
{"form":"concise-1","status":"stopped","diagnostics":[{"code":"E3008","message":"interpreter-fuel-exhausted","at":"input.c:102:1","end":"113:2","source":"static unsigned long long ref_adler(unsigned seed) ...","notes":["execution reached a resource limit; this is not a finding of undefined behavior","the executing function is shown; this instruction has no retained expression position"],"why":"The interpreter reached its execution step limit.","fix":"Inspect long-running loops and use a finite workload when testing termination."}],"exit_code":1}
```

A secondary span (P17 `frontend-conflicting-prototype`): 2,902 bytes before,
including the plan's copy of the error; 378 after.

```text
{"form":"concise-1","status":"stopped","diagnostics":[{"code":"E2002","message":"incompatible-redeclaration: bit_or_offset","at":"program.c:3:5","source":"bit_or_offset","also":["program.c:1:5 previous declaration is here"],"why":"Two declarations of one name do not describe compatible entities.","fix":"Correct the conflicting declaration instead of casting each use."}],"exit_code":1}
```

## Measured reduction

| Blocks | Count | Before, bytes | After, bytes | Mean before | Mean after | Reduction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Experiments 4 and 5, finding | 104 | 178,702 | 36,876 | 1,718 | 355 | 79.4% |
| Experiments 4 and 5, clean | 113 | 47,908 | 21,353 | 424 | 189 | 55.4% |
| Experiments 4 and 5, all | 217 | 226,610 | 58,229 | 1,044 | 268 | 74.3% |
| P17 C3, finding | 60 | 102,801 | 17,997 | 1,713 | 300 | 82.5% |
| P17 C3, clean | 117 | 36,153 | 21,996 | 309 | 188 | 39.2% |

P17 clean records predate the `scope` field; `concise-1` adds that sentence.
By code, over experiment 4 and 5 blocks with one diagnostic: E3001 1,472 to
318 bytes (24 blocks), E3002 1,647 to 391 (5), E3004 1,679 to 338 (5), E3005
1,556 to 354 (6), E3008 2,280 to 476 (11), E3010 1,565 to 332 (19), E3011
1,486 to 323 (23), E2002 2,850 to 371 (4).

These sizes assume the relative-path rule. If a path stays absolute, each
`at` grows by the path length, about 80 bytes in the harness.

## Versioning and replays

Recommendation: select `concise-1` explicitly. Keep `full-1` as the default.

- `spec/diagnostics.md` section 5 says that the JSON field names, types and
  meanings are stable. An incompatible change needs a new major revision.
- The registered harnesses call `evo check --json` without a selector. A new
  default would change replay bytes without any visible change in the harness.
- Experiment 5 compared raw bytes against the experiment 4 qualification.
  `full-1` must stay byte-identical for that comparison to remain valid.
- P22 forbids a causal claim about feedback length. A default change would
  act on an effect that nobody has measured.

Interface:

- CLI: `evo check FILE.c --json --form=concise-1`. `--form=full-1` and no
  `--form` give today's bytes. `--form` without `--json`, on another verb, or
  with another value is a usage error.
- MCP: the `check` tool takes an optional `form` string with the values
  `full-1` and `concise-1`. The default is `full-1`. Another value is a
  -32602 error.
- Reconsider the MCP default only after the registered harm check in
  `protocols.md`, or with a deliberate major revision of the specification.
  Keep `--form=full-1` for replays in either case.

## Code changes for the Rust lane

1. `crates/evocc/src/diagnostics.rs`
   - `enum Format`: add `Concise`. `Format::parse()` accepts `concise`.
     `Format::for_usage()` recognizes `--error-format=concise`.
   - `Format::usage_at()`: print a concise object for a command error.
   - `Format::completion()`: for `Concise`, print
     `{"status":"completed","main_return":"N","scope":SCOPE}`.
   - `render()`: add a `Concise` branch after the span assertions, as for
     `Short`. Build the diagnostic keys above. Reuse `displayed()` for
     `source` and `quoted()` for strings.
   - Add `explanation_line(code)` from `CATALOGUE` and `repair_hint(code)`
     from a new `HINTS` table. Keep `explain()` byte-identical.
2. `crates/evocc/build.rs`: read `crates/evocc/explain/hints.txt`, emit
   `HINTS`, reject a code that is absent from the authority table or listed
   twice, and add `cargo:rerun-if-changed`.
3. New `crates/evocc/explain/hints.txt`: one `CODE<TAB>HINT` line per code.
4. `crates/evocc/src/main.rs`: list `concise` in the `--check` usage line
   (line 434). `crates/evocc/src/repairs.rs`: lines 400 to 406 know only the
   JSON and text plan forms. Reject `--repair` with `--error-format=concise`
   as a usage error, so that no half-concise plan appears.
5. `crates/evocc/src/bin/evo.rs`
   - `execute()`: accept `--form=full-1|concise-1` for `check` with
     `--json`; update the `check` usage string (line 492) and the argument
     count check (line 553).
   - `check` branch (lines 552 to 584): for `concise-1`, run
     `evocc --check FILE --error-format=concise`. A completion line on
     standard output gives `status` `completed`. Validate each diagnostic line
     with `json::parse()`, then embed the original line unchanged:
     `Json::render()` sorts keys and would break the fixed order. Then run the
     plan command that `fix()` uses, with `--error-format=json`, and convert
     its `repairs`. Print the record with `format!` in the fixed key order.
6. `crates/evocc/src/bin/evo/mcp.rs`: give `check` an optional `form` field.
   `tools()` emits `{"type":"string","enum":["full-1","concise-1"]}` and
   leaves it out of `required`. `command()` validates it and passes
   `--form=VALUE`. `call()` needs no change: both forms keep `exit_code`.
7. Tests
   - `crates/evocc/tests/command_json.rs`: concise snapshots for a finding, a
     clean check, a secondary span, a whole-function span and a plan repair.
     Keep the `full-1` snapshots unchanged.
   - `crates/evocc/tests/mcp_stdio.rs`: `form` accepted; a bad value gives
     -32602; omission gives the `full-1` bytes.
   - `crates/evocc/tests/determinism.rs`: concise output is byte-identical
     across runs.
   - `crates/evocc/tests/explain_pages.rs`: every authority code has at most
     one hint; a hint is one line of at most 120 bytes that ends with a period.
   - A frozen-feedback regression: regenerate the EvoCC part of the 48 C3
     strings in `tests/fixtures/agent-repair-4/qualification.json` (24 items,
     `program.c` and `fixed.c`) and compare raw bytes after replacing the
     recorded scratch path.
8. Documents: `spec/diagnostics.md` gains section 5.2 for the concise record
   (revision 1.4.0, additive) with a pin update; `book/src/agents.md` gains
   the `--form` row; `skills/evocc/SKILL.md` gains one sentence. The LSP
   server is not affected.

## Limits

- The concise sizes come from a scratch model of these rules, not from a
  built binary. The Rust tests must confirm them.
- Token counts are estimates. The Kimi tokenizer was not available.
- The accounting fixes the observed trajectories. It cannot predict how an
  agent behaves with a different form.
- The concise form removes the page examples from C3 feedback. In the
  sandbox of experiments 4 and 5 the agent could not call `evo explain`, so
  that is a real information change for a P22 experiment.
