# json-comment-minify

Public-source adaptation, not an original-for-corpus item. Altered sources.

Pinned archive: `.build/ladder/cjson/v1.7.19.tar.gz` (cJSON v1.7.19), SHA-256 `7fa616e3046edfa7a28a32d5f9eacfd23f92900fe1f8ccd988c1662f30454562`.
Exact local root: `.build/ladder/cjson/cJSON-1.7.19/`.
License: MIT; retained unmodified in `../LICENSE.cjson`. Copyright (c) 2009-2017 Dave Gamble and cJSON contributors.

## Function provenance and adaptation

Exact source locations: `cJSON.c:2858-2959`.
Transplanted/composed functions: `skip_oneline_comment, skip_multiline_comment, minify_string, cJSON_Minify`.

Transplant the comment-skipping loops and two-pointer in-place minifier. Replace static_strlen calls with the corresponding literal lengths and expose produced length. Caller-owned copies keep the same comment and quote semantics. Compose each template inside an array with a seed-dependent numeric member and a newline that ends any line comment; all twelve API inputs produce distinct actual documents. Lexical scoring and bounded hashing expose whitespace inside strings, removed comments, punctuation, and repeated idempotent minification.

The standalone source has no preprocessing, external calls, or I/O. Public
`evaluate(seed)` constructs inputs, calls shared algorithm helpers multiple
times, and returns a deterministic bounded result. `main` evaluates ordinary
seeds 0, 1, and 2 before the exceptional seed 11. `reference_main.c` evaluates
12 distinct inputs and `program_main`; `expected.txt` contains exact fixed
output. Fix spans use original one-based inclusive changed line ranges.

## Injected defects

1. Omit cJSON_Minify's null-input refusal; seed 11 passes a missing document and reads through null.
2. Treat an escaped backslash, rather than an escaped quote, as the quoted-string special pair; strings containing escaped quotes terminate early and lose meaningful spaces.

Both defects lie in the composed execution; repairing only the runtime fault
leaves semantically wrong results on hidden ordinary inputs.

## Qualification status

Construction only. Host Clang sanity checks do not establish qualification.
Sealed GCC-14 scoring and UBSan, EvoCC runtime diagnosis, mutation ablations,
and C0 model-difficulty preflight remain the root agent's acceptance work.
