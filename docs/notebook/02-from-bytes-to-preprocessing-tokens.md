---
title: From Bytes to Preprocessing Tokens
chapter: 2
day: Day 2–3
slug: from-bytes-to-preprocessing-tokens
description: Follow GNU C source through logical lines, preprocessing tokens, macro rescanning, includes, diagnostics, and a token-normalized differential oracle.
---

# 2. From Bytes to Preprocessing Tokens

> The difficult part of reading C is deciding which distinctions still exist.

Consider these two fragments:

```c
left + +right
left++ + right
```

They use nearly the same characters. They do not contain the same tokens, and
they do not mean the same thing. Now consider this one:

```c
#define PATH(name) "include/" name
#include PATH("config.h")
```

The characters naming the included file do not exist as a contiguous slice of
the original source. They are assembled by preprocessing. Later, a diagnostic
must still say which file, line, and column produced a bad declaration inside
that header.

This is the work of the C frontend before parsing begins. It is not merely
splitting a string at punctuation. The frontend must preserve boundaries that
affect meaning, erase boundaries that the language says no longer matter, and
carry source identity through transformations that manufacture new text.

Chapter 1 built a world in which a compiler run can be identified. This
chapter enters the compiler itself. It reconstructs the Day 2–3 path from
source bytes to the token stream consumed by the parser, using the current
EvoCC implementation and its 34-case preprocessing differential gate as our
guide.

## 2.1 C is translated in phases

The C standard describes translation as an ordered sequence of phases. The
exact standard wording matters, but the engineering point is simpler: **order
changes the program**.

A useful working model for EvoCC is:

1. identify the physical source bytes and source file;
2. form logical lines by handling line continuation;
3. recognize comments, whitespace, identifiers, literals, and punctuators;
4. form preprocessing tokens;
5. interpret directives and expand macros;
6. concatenate adjacent string literals and parse C syntax later.

```evocc-sketch
translation-phases
```

Each phase removes information. After a backslash-newline splice, later stages
must behave as if that physical line break did not separate the tokens. After
comments are removed, the text inside a comment is not available to macro
expansion. After maximal-munch tokenization chooses `>>=`, a parser must not
reinterpret it as `>` followed by `>=` merely because that would make a
different grammar production succeed.

This irreversibility gives us a design rule:

> Every stage should consume one explicit representation and produce another.
> It should not repeatedly guess what an earlier representation may have been.

EvoCC has not yet implemented every historical C translation feature. In
particular, the current lexer deliberately characterizes trigraph spellings
such as `??=` as three ordinary punctuators rather than translating them to
`#`. That is an honest scope boundary, not evidence of full GNU C11 lexical
conformance. The rest of this chapter distinguishes such boundaries from the
behavior that has actually been measured.

## 2.2 A preprocessing token is not yet a C token

The preprocessor must recognize enough structure to expand macros without
performing C type checking. Its vocabulary therefore differs slightly from
the parser's.

For example, a *preprocessing number* is deliberately permissive. Text such as
`1e+VALUE` can remain one preprocessing number while macros are expanded; only
later does the language frontend decide whether the final spelling is a valid
integer or floating constant. Likewise, `#` and `##` are meaningful to macro
replacement even though they are not ordinary expression operators.

EvoCC's lexer records a token as three joined facts:

```rust
pub struct Token {
    pub kind: TokenKind,
    pub span: SourceSpan,
    pub text: TokenText,
}
```

```evocc-sketch
token-anatomy
```

The three fields answer different questions:

| Field | Question | Example |
|---|---|---|
| `kind` | What grammatical category was recognized? | `Identifier`, `ShiftLeftEq`, `StringLiteral` |
| `text` | Where does its spelling live? | source-backed `Borrowed` or owned `Decoded("size_t")` |
| `span` | Which source range caused it? | `FileId(3)`, bytes `48..54`, line 4, column 9 |

`SourceSpan` treats `(FileId, byte_start, byte_end)` as primary identity. Line
and column are derived human coordinates. That distinction prevents a common
compiler bug: two files can both contain “line 4, column 9,” while their byte
ranges and file identities are unrelated.

### Borrowed and Decoded spelling

Most tokens are `Borrowed`. Their text is reconstructed by slicing the source
buffer selected by `span.file`. Generated tokens cannot do that. A macro may
produce an identifier that never appeared contiguously in the source; an
included header may have byte offsets larger than the entire parent file.
Those tokens use `Decoded(String)` and carry their own spelling.

This split repaired a real end-to-end defect. An earlier parser attempted to
slice the main source buffer using a token span from an included header. A long
identifier in the header caused an out-of-bounds panic. The repaired parser
branches on `TokenText`: `Decoded` text is used directly, while `Borrowed` text
is accepted only after its file and byte range are validated.

The important lesson is not “clone more strings.” It is that **storage
ownership and source provenance are different properties**. A token may own
its spelling while still pointing to the header location that should appear in
a diagnostic.

## 2.3 Maximal munch makes whitespace semantic

Lexers usually follow the longest-token rule: at the current byte, choose the
longest valid token. This is often called *maximal munch*.

It makes the following boundaries significant:

```text
> > =     versus     >>=
+ +       versus     ++
/ *       versus     /*
. . .     versus     ...
```

The lexer can implement the rule locally by testing longer punctuators before
shorter ones. The more subtle problem appears at the other end of
preprocessing: `evocc -E` must render a token stream back into text.

```evocc-sketch
maximal-munch
```

Simply concatenating token spellings introduced two serious errors:

```c
x + +1       // rendered as x++1
x / *(&y)    // rendered as x/*(&y)
```

The first manufactures an increment operator. The second manufactures the
opening of a block comment and can delete the rest of the program.

EvoCC's renderer now uses a bounded round-trip predicate. It tentatively
concatenates the recent output tokens with the next token, lexes that candidate,
and asks whether the resulting `TokenKind` sequence is exactly the expected
sequence. If not, it inserts a space and starts a new concatenation group.

Formally, for token spellings `s₁…sₙ` and kinds `k₁…kₙ`, omission of a separator
is allowed only when:

```text
lex(s₁ || … || sₙ).kinds = [k₁, …, kₙ]
```

This is stronger than a hand-written table of “word-like” tokens. It catches
multi-token interactions such as `. . .` and `% : % :`, where pairwise checks
alone can miss a newly formed punctuator.

## 2.4 Physical lines and logical lines

Backslash-newline continuation is processed before directives and macro calls
are understood. These are therefore one logical definition:

```c
#define SCALE(x) ((x) * \
                  16)
```

The splice is not whitespace. It disappears. If it were replaced by a newline,
the macro body would end early; if it were replaced by an ordinary space at the
wrong point, it could change a token boundary.

Line handling also affects diagnostics. The compiler needs two related views:

- a logical view for deciding whether `#` begins a directive and which tokens
  belong to the replacement list;
- a physical-source view for telling the user where those tokens were written.

EvoCC's lexer tracks byte position, line, and column while skipping whitespace
and both comment forms. It emits one `Eof` token and then behaves as a fused
iterator: repeated calls after EOF return `None`. That last property sounds
minor, but a non-fused lexer can resurrect stale tokens and make a preprocessor
loop or process a directive twice.

Comments deserve similar care. Removing `/**/` from `int/**/value` must leave
two tokens, `int` and `value`, not manufacture `intvalue`. The representation
already has the boundary; the renderer must preserve it.

## 2.5 Macro expansion is an evaluator

A macro system is often explained as textual substitution. That description
is too weak to implement C.

Consider:

```c
#define ADD(a, b) ((a) + (b))
#define TWICE(x)  ADD(x, x)

TWICE(3)
```

A correct expander must:

1. recognize that `TWICE` is function-like only because `(` follows it;
2. collect one argument while respecting nested parentheses;
3. bind the parameter `x` to the argument token sequence `[3]`;
4. substitute into `ADD(x, x)`;
5. rescan `ADD(3, 3)` so the newly exposed macro is expanded;
6. prevent `TWICE` from re-entering itself during its own expansion.

```evocc-sketch
macro-rescan
```

The expander therefore needs state:

- a macro table containing object-like and function-like definitions;
- parameter and variadic-parameter bindings;
- an expansion stack or disabled set;
- an argument reader with balanced-delimiter depth;
- a rescan step over the replacement result.

### Why the disabled set is necessary

Direct recursion is obvious:

```c
#define SELF SELF
SELF
```

Indirect recursion is the real test:

```c
#define A B
#define B A
A
```

A mere recursion-depth limit terminates this input, but it does not implement
the language rule; the result depends on an arbitrary number. EvoCC records
the names currently being expanded. A name in that set is emitted without
re-entering its definition. Once the active replacement list is complete, the
name leaves the set.

### Argument prescan is contextual

Ordinary parameter occurrences use macro-expanded arguments. The sharp
operators `#` and `##` are exceptions: their adjacent parameter spelling is
used before normal prescan. This is why wrapper macros such as `XSTR(x)` and
two-level concatenation macros exist in real C code. The order is observable
and cannot be “simplified” without changing programs.

## 2.6 `#` and `##` are separate languages inside the macro language

Stringification converts the token spelling of an argument into a string
literal:

```c
#define STR(x) #x
STR(VALUE + 1)       // "VALUE + 1", not the expansion of VALUE
```

Token pasting concatenates the spellings adjacent to `##`:

```c
#define JOIN(a, b) a ## b
JOIN(var, 7)         // var7
```

```evocc-sketch
hash-and-paste
```

The result of pasting is not automatically an identifier. It must be lexed as
a preprocessing token. A valid implementation accepts `var ## 7` as `var7`
and `1 ## 25` as the preprocessing number `125`, but rejects a concatenation
that forms zero tokens or more than one token. Empty arguments introduce a
*placemarker*: it participates in the paste rule without becoming an output
token.

This gives `##` a crisp local contract:

```text
paste(left, right):
    spelling = left.spelling || right.spelling
    tokens = lex_preprocessing_token(spelling)
    require tokens.length == 1
    return tokens[0]
```

The current EvoCC differential corpus measures identifier–identifier,
identifier–number, number–number, paste-after-expansion, stringification, space
normalization, and pre-expansion ordering. It does **not** by itself prove the
entire invalid-paste and placemarker state space. The rule above is the semantic
target; the 34 fixtures are a reviewed regression slice of it.

## 2.7 Conditional compilation is a stack machine

`#if` is not a one-line filter. Nesting means the preprocessor must remember
the state of every enclosing conditional:

```c
#if OUTER
  keep_A
  #if INNER
    maybe_B
  #else
    keep_C
  #endif
#endif
```

```evocc-sketch
conditional-stack
```

EvoCC models each frame with two facts:

- `in_true`: whether this branch is active;
- `had_true`: whether an earlier branch in this conditional group has already
  been selected.

An ordinary token is emitted only if **every** enclosing frame is active. A
directive must still be interpreted inside an inactive branch when it affects
nesting; otherwise the preprocessor could never find the matching `#else` or
`#endif`.

The expression after `#if` has its own semantics. `defined(NAME)` is evaluated
against the macro table, remaining identifiers become zero, and integer
operators follow preprocessing-expression precedence. Reusing the C expression
parser without defining this smaller language precisely tends to produce
incorrect behavior around undefined identifiers, short-circuiting, and nested
conditionals.

A missing `#endif` is not an empty branch. It is a structural error. EvoCC's
driver turns preprocessing failures into structured, nonzero diagnostics rather
than letting malformed source fall through to a parser that cannot explain the
real cause.

## 2.8 Includes turn a buffer into a source graph

Before `#include`, it is tempting to model a translation unit as one string.
After `#include`, that model is wrong.

```evocc-sketch
include-source-graph
```

The preprocessor must combine two kinds of information:

1. **search policy** — quoted sibling paths, explicit `-I` roots, optional
   standard roots, and the `-nostdinc` switch;
2. **source identity** — the canonical path and `FileId` of every file whose
   tokens enter the translation unit.

Macro-expanded include arguments make this more than path parsing:

```c
#define HEADER "config.h"
#include HEADER
```

The include argument must be macro-expanded before it is interpreted as a
quoted or angle-bracket header name. A missing header is a controlled
preprocessing error. In the official corpus, fixture
`025-include-missing.c` requires both EvoCC and GCC to return nonzero and name
the same missing subject, `nonexistent.h`.

### Why included tokens own their spelling

Suppose `main.c` is 20 bytes long and an identifier in `types.h` occupies
bytes `400..430` of that file. The token's source span is valid—but only for
`types.h`. Slicing `main.c[400..430]` is a panic.

EvoCC assigns stable file identities to included sources and converts their
token spelling to `Decoded` storage before the tokens re-enter parent
processing. The parser can therefore use the owned spelling while diagnostics
continue to point at the header's `FileId` and coordinates.

`#pragma once` and ordinary include guards add another dimension: repeated
edges in the source graph need not produce repeated tokens. That cache key must
be the resolved source identity, not merely the spelling used in one include
directive.

## 2.9 Preprocessed output is a serialization format

The output of `evocc -E` looks like text, but its correctness criterion is a
token stream. Formatting is secondary.

Two outputs may differ in whitespace while containing exactly the same
preprocessing tokens:

```text
result = (a)+(b)
result=( a ) + ( b )
```

Conversely, two outputs may look visually similar while their token boundaries
differ, as `x + +1` and `x++1` demonstrate.

This suggests two separate properties:

- **semantic serialization:** lexing the rendered output reproduces the exact
  token kinds and spellings;
- **presentation:** whitespace and line markers are readable and compatible
  with the requested flags, such as `-P`.

EvoCC prioritizes the first. The lexer round-trip renderer inserts the minimum
separator needed to preserve tokenization. Pretty-printing belongs above this
contract, never below it.

## 2.10 The differential oracle compares tokens, not typography

Day 2–3 needs an external check that the implemented slice agrees with an
existing GNU C preprocessor. Comparing raw stdout would make harmless spaces
fail. Removing all whitespace would make token fusion invisible.

The acceptance path therefore uses an independent serializer:

```evocc-sketch
differential-oracle
```

For every fixture, EvoCC and GCC 14 receive the same effective arguments:

```text
-std=gnu11 -E -P -undef -nostdinc -I. -Iinclude
```

Each successful stdout is then tokenized by
[`cpp-token-serializer.py`](../../tests/acceptance/cpp-token-serializer.py), a
small observer that trusts neither compiler. It emits canonical JSON lines:

```json
{"kind":"identifier","spelling":"result"}
{"kind":"punctuator","spelling":"="}
{"kind":"pp-number","spelling":"42"}
```

The serializer applies longest-punctuator matching, recognizes preprocessing
numbers and quoted tokens, and ignores incidental whitespace and comments. Its
own self-test is run before compiler output is accepted.

The corpus covers object-like, function-like, and variadic macros;
stringification and token pasting; `#ifdef`, `#ifndef`, `defined`, and
arithmetic `#if`; quoted and angle includes; missing includes; recursive macro
suppression; predefined macros; empty input; comments; and directive-boundary
cases.

Most importantly, the denominator is frozen:

```text
overall PASS ⇔ total = 34 ∧ every fixture.status = PASS
```

`SKIP`, `KNOWN_GAP`, missing output, exit-code disagreement, serializer failure,
or token disagreement cannot be averaged away. A suite that silently falls
from 34 cases to 33 is a different experiment.

### What GCC can and cannot establish

Agreement with GCC is strong differential evidence for this corpus. It is not
the definition of C. Both implementations can share a bug, and GCC behavior
outside the selected dialect or on undefined input is not normative truth.

This is why the gate uses only reviewed inputs and reports disagreement rather
than automatically declaring EvoCC wrong. The language contract and hand-audited
fixture intent remain above the executable oracle.

## 2.11 Diagnostics are part of the frontend protocol

Malformed input must not become an `__error__` node followed by exit code zero.
The command-line contract is:

```text
path:line:column: error[CODE]: subject-bearing message
```

with nonzero exit status, empty normal output for failed preprocessing or
parse-only operations, and no panic or timeout.

The location must use the logical command-line identity where C requires it
(`__FILE__`), while filesystem resolution may use a canonical path and the
diagnostic may need the resolved included-file identity. Conflating those three
names produces either unstable diagnostics or incorrect predefined macros.

For the missing-include fixture, the useful facts are not merely “some error
occurred.” They are:

- preprocessing, not parsing, rejected the program;
- the exit code is nonzero;
- the code is stable (`E0105` in the current CLI contract);
- the missing subject is `nonexistent.h`;
- the source location points at the responsible include directive.

Stable diagnostic codes make tests and tools robust to editorial improvements
in the prose, while subject and span keep the error useful to a human.

## 2.12 What Day 2–3 established—and what it did not

The implemented frontend slice now has measured evidence for:

- C11 and selected GNU token categories, longest punctuators, literals,
  comments, line continuation, source spans, one-shot EOF, and negative tokens;
- object-like, function-like, and variadic macro behavior across a generated
  unit campaign and the curated corpus;
- nested rescan, direct and indirect recursion suppression, stringification,
  token pasting, conditionals, predefined macros, include search, macro-expanded
  includes, `-nostdinc`, and `#pragma once` scenarios exercised by tests;
- token-boundary-preserving `-E` output;
- exact 34/34 token-normalized agreement in the foundation differential gate;
- included-header token ownership and structured nonzero diagnostics.

It does **not** follow that EvoCC is already a complete GNU preprocessor.
Current evidence does not claim:

- trigraph translation;
- every implementation-defined character-set behavior;
- the full combinatorial state space of placemarkers and invalid `##` results;
- every GNU built-in macro and extension;
- semantic correctness outside the frozen dialect, flags, and fixture domain.

That distinction is the point of a notebook tied to executable evidence. We
can say exactly what became real without turning a successful checkpoint into
a universal theorem.

## 2.13 Running the chapter checkpoint

The focused Rust suites exercise the lexer and preprocessor implementation:

```sh
cargo test -p evocc-lex -p evocc-cpp --locked --offline
```

The serializer can be checked without invoking either compiler:

```sh
bash tests/acceptance/cpp-token-serializer-selftest.sh
```

The isolated differential performs the 34-case EvoCC/GCC comparison and writes
a new evidence directory:

```sh
bash tests/acceptance/cpp-differential.sh
```

The consolidated foundation gate includes this experiment as
`isolated_cpp_differential` and requires exact `34 PASS` with zero non-PASS
outcomes:

```sh
bash tests/acceptance/foundation-conformance.sh
```

## 2.14 The boundary we hand to the parser

At the end of preprocessing, the parser does not receive “a string with the
macros removed.” It receives an ordered stream of typed tokens with stable
spellings and source identities.

That stream is a narrow waist:

```text
many physical files
    + include policy
    + conditional state
    + macro environment
             │
             ▼
ordered Token { kind, text, span } stream
             │
             ▼
declarations, expressions, and C-HIR
```

If this boundary is correct, the parser can concentrate on declarator topology,
precedence, types, and source-language meaning. If it is wrong, every later
stage inherits a fabricated program while retaining no reliable route back to
the source.

The next chapter will cross that waist. C's declarations read inside out,
function and array parameters adjust their types, and the same punctuation can
describe either an expression or a type name. That is where tokens acquire
structure.

## Challenges

1. Tokenize `a+++b` using maximal munch. Then insert one space in each possible
   position and explain which programs change token sequence.
2. Trace `#define A B`, `#define B(x) x + 1`, and `A(3)` through lookup,
   substitution, disabled-set membership, and rescan. At which exact step does
   `B` become callable?
3. Explain why `STR(VALUE)` and `XSTR(VALUE)` can intentionally produce
   different strings when `VALUE` is itself a macro.
4. Design one invalid `##` case that produces zero valid preprocessing tokens
   and one that produces more than one. What should the diagnostic record?
5. A header token has `span.file = 7` and `bytes = 400..430`; the root file is
   only 80 bytes long. Describe the correct route for obtaining its spelling
   and its diagnostic location.
6. Raw EvoCC and GCC output differ only in spaces. Give one example where that
   difference is harmless and one where removing the spaces changes tokens.
7. Add a 35th fixture locally but leave the acceptance denominator at 34. Should
   the old run still be called “34/34 PASS”? What additional corpus-identity
   contract would make the claim unambiguous?
