# Proposed registered experiments for P17a and P22

Status: Protocol A is registered on 2026-10-20, with the provider fixed as
AtlasCloud; the tag `agent-repair-experiment-6` marks the registration commit.
Protocol B is not registered. No model or API request was made to write this
document. The owner must approve each pilot and each full run.

Evidence: `p17a-trace-analysis.md` and `p22-feedback-design.md` in this
directory.

## Rules for both experiments

- Register before measurement. A commit and a tag fix the corpus, the
  harness, the feedback bytes, the prompt, the order, the analysis and this
  method. No measured request may use a changed file.
- Do not rerun, tune or replace any cell of P17, experiment 4 or
  experiment 5. Do not pool new results with them.
- Keep pilot results out of the measured analysis.
- Keep every response and session. Never report the best of several
  attempts. A failed trial stays in the results.
- Keep captures outside the repository and below 500 MiB. Split transcript
  archives below 10 MiB each. Run the existing secret scans, including the
  exact credential values, before any evidence goes into Git.
- Do not start below 25 GiB of free disk space. Stop below 15 GiB.
- Commit messages, receipts and reports carry no tool or model attribution.
- Report unfavorable results in the same detail as favorable ones.

## Protocol A: P17a, E3004 guidance and the response budget

### Question

On the five P17 saturation items, does a concise E3004 guidance record make
the model's response shorter? How many of P17's undelivered responses come
from the 4,096-token response limit?

### Predictions

- A1: At the 16,384-token limit, fewer G1 responses than G0 responses are
  longer than 4,096 completion tokens.
- A2: At the 16,384-token limit, G1 responses use fewer completion tokens
  than G0 responses.
- A3, descriptive: at the 16,384-token limit, each arm has at most 3 of 150
  undelivered responses.
- A4, descriptive: in each arm, the non-delivery rate observed at 4,096 lies
  inside the exact 95% interval of the share of that arm's 16,384-limit
  responses that are longer than 4,096 tokens.
- A5, descriptive: in every cell, at least 95% of valid candidates pass the
  hidden test.

### Population

The items `ub-addition-overflow`, `ub-subtraction-overflow`,
`ub-multiplication-overflow`, `ub-negation-overflow` and
`ub-quotient-overflow` at tag `agent-repair-corpus-2`. Sources, hidden
drivers and expected outputs do not change. These five items are the whole
population. The results do not generalize to other code.

### Model, provider and request

- Model `deepseek/deepseek-v4.1-flash` through OpenRouter, as in P17.
- Temperature 0. System prompt `tools/experiment/prompt.txt`, unchanged.
  The user message keeps the P17 layout: `Current program.c:`, the source,
  `Feedback:` and the feedback records.
- Pin one provider with `provider.order` set to that provider and
  `allow_fallbacks` set to false. Proposed provider: AtlasCloud. It served
  95 of 272 P17 responses and one of the three C3 failures. The owner fixes
  the provider at registration. Do not change it during the run.
- Send no `reasoning` parameter. Keep the provider default, as in P17.
  Record `reasoning_tokens` from every response.
- Keep the full raw response of every trial, including the reasoning text,
  as P17 did. The trace analysis depends on it.
- One response per trial. There are no repair rounds: all three P17 failures
  happened in round 1.

### Conditions

- G0, frozen feedback. The current `evo` produces the three C3 records for
  the original `program.c` in a fresh scratch directory per trial, as in P17.
  Before registration, show that these bytes equal the P17 repetition 1,
  round 1 feedback for each item after scratch-path replacement.
- G1, concise E3004 guidance. G0 with the explain record replaced by this
  exact line. The check and plan records do not change.

  ```text
  {"code":"E3004","title":"signed-integer-overflow","text":"An executed signed integer operation has a result outside its permitted range. Check the operands before the operation, or compute in long long and convert back only when the result fits.","exit_code":0}
  ```

  The feedback falls from 1,654 to 1,144 bytes for the addition item: about
  161 fewer prompt tokens. The text removes the `long` example and keeps one
  general action. It does not name a saturation intent.
- L16: `max_tokens` 16,384. This is the main limit.
- L4: `max_tokens` 4,096, as in P17. It validates the prediction from L16.
- Optional Part B, with separate approval: G2 is G1 plus a plan record with
  one review repair for the item. G2 runs at L16 only. Its drafts are below.

### Sample size and order

| Cell | Trials per item | Trials |
| --- | ---: | ---: |
| G0 at L16 | 30 | 150 |
| G1 at L16 | 30 | 150 |
| G0 at L4 | 10 | 50 |
| G1 at L4 | 10 | 50 |
| Measured total | | 400 |
| Optional Part B: G2 at L16 | 30 | 150 |

- In P17, 8 of the 35 responses from providers other than Together reached
  the 4,096-token limit on these items (23%). With 150 trials per arm, a
  simulated one-sided Fisher test has 84% power to detect a fall to 11% at
  level 0.05, and 84% power to detect a fall to 10% at level 0.025, the
  strictest Holm level.
- The within-item standard deviation of log completion tokens was about 0.8
  in P17. A2 then has 80% power to detect a fall of about 20% in typical
  response length at level 0.05, and about 23% at level 0.025.
- The plausible effect from the traces is smaller: the cited-record share was
  6% to 20%. A null result is likely and is not evidence of no effect.
- Order: one random permutation of all 400 trials. Its seed is the first 16
  hexadecimal digits of the registration commit. Commit the order before the
  first request. Run one request at a time.

### Outcomes

- Delivered: `finish_reason` is `stop` and the answer has exactly one
  complete fenced `c` block, by the P17 parse rule.
- Valid: delivered, and the P17 candidate profile accepts the block.
- Correct: valid, and the unchanged hidden scorer passes it (sealed GCC 14
  `-O0`, hidden driver, exact `expected.txt`).
- Long: an L16 response with more than 4,096 completion tokens.
- Tokens: prompt, completion, reasoning, and answer (completion minus
  reasoning). Cost: the reported cost.

### Analysis

- A1: one-sided Fisher exact test of Long, G1 against G0, at L16. Items are
  balanced, so the counts are pooled.
- A2: stratified permutation test. The statistic is the sum over items of
  the mean log completion tokens of G0 minus that of G1. Permute arm labels
  within items, 100,000 times, with the registered seed. One-sided.
- Apply Holm's correction to A1 and A2 at a family level of 0.05.
- A3, A4 and A5: report counts with exact 95% intervals.
- If A4 fails for an arm, the limit changed generation. Then report A1 again
  with the L4 non-delivery counts, as a registered fallback with lower power.
- Report delivered, valid and correct counts separately for each cell. Report
  correct over valid and correct over attempted.
- Count responses that repeat an earlier response of the same cell byte for
  byte. If more than 20% of a cell repeats, add an analysis of distinct
  responses per item as a sensitivity check.
- The unit is the trial, and the five items are fixed. Make no claim about
  other items, models or providers.

### Stopping rules

- Reported-cost cap: USD 3.50 for the measured trials, USD 0.30 for the
  pilot, USD 1.50 for Part B. Check it before each request. The last request
  can cross the cap.
- Missing or invalid usage or cost stops the run. It is never zero.
- HTTP 401, 402 or 403 stops the run.
- A response from another provider or another model stops the run and is
  reported as a deviation.
- HTTP 429, 5xx or a transport error: retry the identical payload once after
  2 seconds. The retry is billed and counted.
- A length stop, an invalid candidate and a hidden failure are outcomes.
  Never retry them.
- A stopped run is partial. Label the affected predictions incomplete.

### Cost

Input tokens per request: 790 for G0 (P17: 780 to 796), 630 for G1, 816
for G2. Expected output per response: 3,000 tokens at L16 and 2,300 at L4
(P17 mean on these items: 2,153, censored at 4,096). Maxima assume every
response uses its whole limit. Prices observed in P17 for AtlasCloud: up to
USD 0.141 per million input tokens and USD 0.564 per million output tokens.
Planning maxima from `tools/experiment/protocol-2.json`: USD 0.45 and 2.40.

| Part | Requests | Input tokens | Output tokens, expected | Output tokens, maximum | USD, expected | USD, maximum | USD, maximum at planning prices |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Pilot | 10 | 7,100 | 27,200 | 114,688 | 0.02 | 0.07 | 0.28 |
| Measured | 400 | 284,000 | 1,130,000 | 5,324,800 | 0.68 | 3.04 | 12.91 |
| Part B, optional | 150 | 122,400 | 450,000 | 2,457,600 | 0.27 | 1.40 | 5.95 |

Transport retries add at most one billed request per trial. The caps stop
the run before the planning maxima can occur.

### Pilot: 10 requests

| Request | Item | Arm | Limit |
| ---: | --- | --- | --- |
| 1 | `ub-addition-overflow` | G0 | L16 |
| 2 | `ub-addition-overflow` | G1 | L16 |
| 3 | `ub-negation-overflow` | G0 | L16 |
| 4 | `ub-negation-overflow` | G1 | L16 |
| 5 | `ub-subtraction-overflow` | G0 | L16 |
| 6 | `ub-subtraction-overflow` | G1 | L16 |
| 7 | `ub-multiplication-overflow` | G0 | L4 |
| 8 | `ub-multiplication-overflow` | G1 | L4 |
| 9 | `ub-quotient-overflow` | G0 | L4 |
| 10 | `ub-quotient-overflow` | G1 | L4 |

Free steps before the pilot: the G0 byte comparison, a harness test against
a local fake HTTP server, and scoring of the Part B drafts with the sealed
GCC 14 scorer.

Go criteria:

- All 10 responses report the pinned provider and the registered model.
- Every response has usage, cost and `reasoning_tokens`.
- The provider accepts `max_tokens` 16,384 without a parameter error.
- The harness extracts, validates and scores every delivered candidate.
- The pilot cost stays below USD 0.30.

The pilot does not estimate an effect. It does not change the sample size.
A harness change after the pilot needs a new pilot.

### Part B drafts: intent-specific review repairs

Each repair replaces the operation at line 6, column 12 of `program.c`. It
has `repair_id` `E3004.saturate` and `kind` `review`. It is never applied
automatically. Its title names the intent it assumes.

| Item | Span | Replacement | Title |
| --- | --- | --- | --- |
| `ub-addition-overflow` | `a + b` | `(int)((long long)a + b > 2147483647 ? 2147483647 : (long long)a + b < -2147483647 - 1 ? -2147483647 - 1 : (long long)a + b)` | If the contract requires saturation, compute in long long and clamp to the int range |
| `ub-subtraction-overflow` | `a - b` | `(int)((long long)a - b > 2147483647 ? 2147483647 : (long long)a - b < -2147483647 - 1 ? -2147483647 - 1 : (long long)a - b)` | Same title |
| `ub-multiplication-overflow` | `a * b` | `(int)((long long)a * b > 2147483647 ? 2147483647 : (long long)a * b < -2147483647 - 1 ? -2147483647 - 1 : (long long)a * b)` | Same title |
| `ub-negation-overflow` | `-a` | `(a == -2147483647 - 1 ? 2147483647 : -a)` | If the contract requires saturation, map the minimum value to the maximum |
| `ub-quotient-overflow` | `a / b` | `(a == -2147483647 - 1 && b == -1 ? 2147483647 : a / b)` | If the contract requires saturation, map the one overflowing quotient to the maximum |

Model-free check: with each replacement applied, `program.c` passed all
hidden inputs and its own `main` under `evocc-darwin --check`, with no
undefined behavior. A human must still review the texts and run the sealed
scorer before registration. These are test fixtures, not compiler repairs.
Saturation changes behavior, so it must never become a safe repair. Adding
such a plan to `evo fix` needs a separate decision.

## Protocol B: P22, the concise check record with a coding agent

### Question

In the experiment 5 agent setup, does the `concise-1` C3 feedback keep the
repairs, and does it change agent effort, compared with the frozen `full-1`
C3 feedback?

### Precondition

- The Rust lane has implemented `concise-1` as `p22-feedback-design.md`
  describes, and its tests pass.
- `full-1` equals the EvoCC part of the 48 frozen C3 strings in
  `tests/fixtures/agent-repair-4/qualification.json`, after scratch-path
  replacement and the scope correction of the amendment below, and differs in
  no other byte.
- The 48 `concise-1` strings for the same sources are recorded before
  registration.

### Predictions

- B1, no repair loss: b counts item-repetition pairs repaired only with
  `full-1`; c counts pairs repaired only with `concise-1`. B1 holds when
  b − c is at most 2 of the 48 pairs. Report the exact two-sided McNemar
  p-value as a description.
- B2, tokens: `concise-1` uses fewer total tokens, including cached input,
  per item mean. Exact one-sided Wilcoxon signed-rank test over 24 items,
  with zero differences dropped and midranks for ties, as in experiment 5.
- B3, steps: `concise-1` uses fewer steps per item mean. Same test.
- B4, controls: on the six controls, the `concise-1` step total is lower than
  the `full-1` total. This is a point comparison without a test.
- Apply Holm's correction to B2 and B3 at a family level of 0.05.
- Descriptive: EvoCC feedback bytes per check result, carried feedback
  tokens, check runs, wall time, audit counts and per-class tables.

### Expected effect and power

- The accounting predicts a token saving of about 1.8% if the agent's
  behavior does not change.
- In experiment 5 part A, one C3 session varied within an item by a standard
  deviation of 1.73 steps and 32,690 tokens. With 24 items and two
  repetitions per arm, the smallest effect with 80% power at one-sided 0.05
  is about 16% of steps or tokens.
- B2 and B3 can therefore show only a large change in behavior. If they do
  not hold, a small effect is still possible. The main purpose is B1: a harm
  check before any change of default.

### Population, agent and conditions

- All 24 items of `tests/fixtures/agent-repair-4`, unchanged. Repetitions 1
  and 2 give 48 pairs and 96 sessions.
- Agent: Kimi Code CLI 2.0.2 if it is still available, model `kimi-code/k3`,
  thinking effort low, 100 steps, 900 seconds, `prompt-3.txt` unchanged, the
  existing Seatbelt sandbox and the opaque check client. A fresh workspace
  for each session. One session at a time. Record the actual CLI version and
  the logged model; another CLI version needs a statement at registration.
  Do not edit the Kimi home configuration.
- Provider: the Kimi service behind the CLI. The CLI chooses the endpoint.
- Response budget: the CLI default per request; the wire logs of experiments
  4 and 5 record `maxTokens` 1,048,576. The session limits of 100 steps and
  900 seconds are the effective budget. Record both in the receipt.
- F, `full-1`: the experiment 5 C3 feedback: header, check record, one
  explain record per code, plan record, with the current scope sentence of
  the amendment below.
- K, `concise-1`: the same header, the line `=== evo diagnostics`, and one
  `evo check FILE.c --json --form=concise-1` record. No explain or plan
  record. In this sandbox the agent cannot call `evo explain`, so K also
  removes the page examples. Report this as part of the treatment.
- Order: sorted items. Zero-based even indices run F first, odd indices K
  first. Repetition 2 reverses repetition 1.
- Scoring: the unchanged hidden driver under sealed GCC 14 `-O0` and `-O2`
  and Clang 19 `-O2`. A repair needs a completed session and all three
  builds. A timeout or step limit counts as not repaired and keeps its
  effort. A provider failure, scoring error or escape is unknown.

### Stopping rules

- Read the quota before every session and at the end. Stop at the initial
  7-day ratio plus 0.15, or at an absolute ratio of 0.85, whichever is lower.
  A failed read stops the package.
- Wait out a 5-hour ratio at or above 0.90, by the existing pacing rule.
- Allow three CLI 429 overload notices per session. The fourth ends the
  session as `provider_overloaded`. Retry it once after all initial sessions.
- A quota, limit or usage 429, or an authentication refusal, stops the
  package.
- Rerun no other session. Use the final authorized retry, never the best one.

### Cost

The Kimi plan has a quota, not a per-token price. The estimates use the
experiment 5 part A C3 sessions: 5.54 requests and 102,564 tokens per session
(98,096 cached input, 3,633 uncached input, 835 output). Part A raised the
7-day ratio by 0.085899 for 12,425,299 tokens: about 6.91e-9 per token.
Planning bounds use the largest part A C3 session: 13 steps and 295,444
tokens. The hard cap is 100 requests per session.

| Part | Sessions | Requests, expected | Requests, planning bound | Tokens, expected | Tokens, planning bound | 7-day ratio, expected |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Pilot | 10 | 55 | 130 | 1.03 million | 2.95 million | 0.007 |
| Full | 96 | 532 | 1,248 | 9.85 million | 28.4 million | 0.068 |
| Harm check only | 48 | 266 | 624 | 4.92 million | 14.2 million | 0.034 |

For the full run, the expected tokens are 9.42 million cached input, 0.35
million uncached input and 0.08 million output. The stop line limits the
ratio increase to 0.15 in every case.

The harm-check-only variant uses repetition 1 alone. B1 then holds when
b − c is at most 1 of 24. Drop B2 and B3, because their power is too low.

### Amendment of 2026-10-20

The owner approved this change. T03s (2026-10-16) corrected the completion
scope of a clean check, in `full-1` and `concise-1`, so that it names the
pointer relations that the check does not cover. The old scope is "No
undefined behavior was found on the executed path. This does not check that
the results are correct." The new scope is "No finding from the implemented
checks on the executed path. Pointer subtraction and ordering obligations are
not checked. This does not check that the results are correct." F is the
current `full-1`, so F differs from the experiment 4 and 5 feedback in this
sentence only, in every completed record. The precondition and the go
criteria allow exactly this difference and record the raw comparison too
(`current_scope` in `tools/experiment/agent_repair.py`, the same rule as
`crates/evocc/tests/frozen_feedback.rs`). Results of this protocol are not
pooled with experiments 4 and 5 in any case.

### Pilot: 10 sessions

Items: `adler-byte` (E3011), `crc-reset` (E3001), `minify-view` (E3010),
`increment-limit` (E3004) and `mean-empty` (E3002). Each runs once in F and
once in K, with F first for the first, third and fifth item.

Go criteria:

- Zero escapes.
- Every K check result with EvoCC output starts its EvoCC part with
  `{"form":"concise-1"`.
- When the agent checks the unchanged `program.c`, the EvoCC part of the F
  result equals the frozen string for that source after path replacement and
  the scope correction.
- Every quota read succeeds. The harness records steps, tokens and scores.
- There is no unknown outcome.

Pilot data stay out of the analysis. The five items stay in the population.

### Reporting

Report F and K side by side for all 24 items and per class. Report unknown,
unattempted and censored counts separately. State the limits: one model, one
effort setting, shared kernels and injected defects. Do not claim a causal
effect of feedback length unless B2 or B3 holds. Even then, name the whole
change: shorter records and no page examples.
