# Agent repair experiment

The registered run is complete. It does not show an overall advantage for EvoCC.

## Question and corpus

The hypothesis is that structured EvoCC feedback increases repair rate and reduces
repair rounds compared with GCC or Clang plus UndefinedBehaviorSanitizer (UBSan).

The corpus has 30 items: 10 UB, 10 frontend, and 10 logic defects. Ten UB items
were written for this corpus. Ten frontend items derive from SCC tests, and ten
logic items derive from TinyCC tests. The 20 derived items use pinned c-testsuite
sources, reductions, and injected defects. They are not observed upstream bugs.

All synthetic items are UB items. Class and origin are confounded; this test
cannot identify an independent effect of source origin. Each buggy item had to
fail its hidden test, and its fixed version had to pass. Each UB item also had
to trigger EvoCC and GCC UBSan.
Thus UB detection was an eligibility rule, not a measured coverage result.
The retained preflight records include rejected candidates and their replacements.

## Frozen method

The measured registration is `agent-repair-corpus-2`, commit `7eed5eac`.
Registration 1 is retained with zero model calls. Its file query exceeded the
131,072-byte capture limit with 137,511 bytes. A scoped query fixed that failure
before registration 2 and the first model request.

The model is DeepSeek V4.1 Flash, through OpenRouter, model ID
`deepseek/deepseek-v4.1-flash`. Every response reported that same model ID.
The reported providers, with request counts, were AtlasCloud (95), Together (63),
DigitalOcean (49), Alibaba (41), Relace (4), Sail Research (4), InferenceNet (4),
GMICloud (2), Wafer (2), SiliconFlow (2), Morph (2), StreamLake (1), DekaLLM (1),
DeepInfra (1), and Phala (1).
Temperature is 0. Each request permits 4,096 completion tokens.
The experiment does not control default reasoning behavior or provider selection.

There are three repetitions and at most three repair rounds per item and
condition. The fixed prompt is the same in all conditions. The agent returns a
complete C file and cannot call tools.

- C1 uses sealed `gcc-14 -Wall -Wextra -fsanitize=undefined`, then runs the file.
- C2 uses sealed `clang-19 -Weverything -fsanitize=undefined`, then runs the file.
- C3 uses host `evo check --json`, `evo explain CODE --json` for each distinct
  diagnostic code, and `evo fix program.c --plan --json`.

C1 and C2 return compiler stderr, runtime stderr, and exit codes. C3 returns the
JSON lines without a rewrite. C3 is a bundle of checks, explanations, and repair
plans. This comparison does not isolate the effect of JSON formatting.

Each round starts with the current file and public tool feedback. After a repair,
the harness runs that condition again and applies the same hidden score.
It stops at the first hidden-test pass or the round limit. Failed repetitions
are not rerun. Sorted item names set item order. Condition order rotates by
`(item_index + repetition - 1) modulo 3`.

The neutral score uses sealed GCC 14 at `-O0`. It compiles the candidate with
`-Dmain=program_main` and links a separate hidden driver. The driver tests seven
to nine function inputs plus the original entry point. It must exit 0 and print
exactly `expected.txt`. Entry-point results can be nonzero when specified.
Hidden inputs, expected output, and score details do not enter model prompts.
Native tools run in fresh network-disabled ARM64 containers.

The stop threshold is USD 5 in reported cost. No new request starts after it is
reached; the last request can cross it. Unknown cost stops the campaign. The plan
permits 810 model rounds. Only HTTP 429 and 5xx errors are retried, at most three
times, with 1, 2, and 4 second delays.

## Results and limits

The run completed all 270 repetitions on 2026-10-08, with 272 model requests,
139,629 input tokens and 259,683 output tokens. Reported cost was
USD 0.1982890478. No repetition was unattempted or rerun.

C3 passed 87/90 (96.7%), compared with 88/90 (97.8%) for C1 and 82/90
(91.1%) for C2. All 257 successful repetitions passed on their first response.
This run did not show an overall repair-rate or successful-round advantage
for EvoCC over both conventional baselines.

The [receipt](https://github.com/onehr/evocc/blob/main/tests/contracts/agent-repair-20261008.json) preserves the
per-repetition results. The [full transcript archive](https://github.com/onehr/evocc/raw/refs/heads/main/tests/contracts/agent-repair-transcripts-20261008.tar.gz)
contains all 270 transcripts, the original receipt, the plan and source licenses.
It is 802,549 bytes. Its SHA-256 is
`226a490f2365782314bf88a0e251ebe7bff15a6a6e2547c3cb13b8dd3c1731b8`.
The [preflight receipt](https://github.com/onehr/evocc/blob/main/tests/contracts/agent-repair-preflight-20261008.json)
records corpus eligibility before measurement.

The tables below come from `tools/experiment/report.py`. Rates use fractions
from 0 to 1. Each condition has 90 attempted repetitions; each class has 30.
“Synthetic attempts” counts repetitions, not distinct source programs.

| Model | Condition | Class | Pass / attempted | Repair rate | Mean rounds | Mean rounds (success) | Rate range | Rounds range | Attempts / planned by repetition | Synthetic attempts | Errors/stops |
| --- | --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- | ---: | ---: |
| deepseek/deepseek-v4.1-flash | c1 | all | 88/90 | 0.978 | 1.011 | 1.000 | 0.967 to 1.000 | 1.000 to 1.033 | 30/30, 30/30, 30/30 | 30 | 2 |
| deepseek/deepseek-v4.1-flash | c1 | ub | 30/30 | 1.000 | 1.000 | 1.000 | 1.000 to 1.000 | 1.000 to 1.000 | 10/10, 10/10, 10/10 | 30 | 0 |
| deepseek/deepseek-v4.1-flash | c1 | frontend | 29/30 | 0.967 | 1.000 | 1.000 | 0.900 to 1.000 | 1.000 to 1.000 | 10/10, 10/10, 10/10 | 0 | 1 |
| deepseek/deepseek-v4.1-flash | c1 | logic | 29/30 | 0.967 | 1.033 | 1.000 | 0.900 to 1.000 | 1.000 to 1.100 | 10/10, 10/10, 10/10 | 0 | 1 |
| deepseek/deepseek-v4.1-flash | c2 | all | 82/90 | 0.911 | 1.011 | 1.000 | 0.867 to 0.967 | 1.000 to 1.033 | 30/30, 30/30, 30/30 | 30 | 8 |
| deepseek/deepseek-v4.1-flash | c2 | ub | 25/30 | 0.833 | 1.000 | 1.000 | 0.800 to 0.900 | 1.000 to 1.000 | 10/10, 10/10, 10/10 | 30 | 5 |
| deepseek/deepseek-v4.1-flash | c2 | frontend | 27/30 | 0.900 | 1.033 | 1.000 | 0.800 to 1.000 | 1.000 to 1.100 | 10/10, 10/10, 10/10 | 0 | 3 |
| deepseek/deepseek-v4.1-flash | c2 | logic | 30/30 | 1.000 | 1.000 | 1.000 | 1.000 to 1.000 | 1.000 to 1.000 | 10/10, 10/10, 10/10 | 0 | 0 |
| deepseek/deepseek-v4.1-flash | c3 | all | 87/90 | 0.967 | 1.000 | 1.000 | 0.933 to 1.000 | 1.000 to 1.000 | 30/30, 30/30, 30/30 | 30 | 3 |
| deepseek/deepseek-v4.1-flash | c3 | ub | 27/30 | 0.900 | 1.000 | 1.000 | 0.800 to 1.000 | 1.000 to 1.000 | 10/10, 10/10, 10/10 | 30 | 3 |
| deepseek/deepseek-v4.1-flash | c3 | frontend | 30/30 | 1.000 | 1.000 | 1.000 | 1.000 to 1.000 | 1.000 to 1.000 | 10/10, 10/10, 10/10 | 0 | 0 |
| deepseek/deepseek-v4.1-flash | c3 | logic | 30/30 | 1.000 | 1.000 | 1.000 | 1.000 to 1.000 | 1.000 to 1.000 | 10/10, 10/10, 10/10 | 0 | 0 |

| Item | Origin | Model | Condition | Pass / attempted | Mean rounds |
| --- | --- | --- | --- | ---: | ---: |
| frontend-array-type | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 2/3 | 1.000 |
| frontend-array-type | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| frontend-array-type | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| frontend-conflicting-prototype | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| frontend-conflicting-prototype | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| frontend-conflicting-prototype | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| frontend-const-local | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| frontend-const-local | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| frontend-const-local | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| frontend-goto-label | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| frontend-goto-label | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 2/3 | 1.333 |
| frontend-goto-label | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| frontend-implicit-call | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| frontend-implicit-call | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| frontend-implicit-call | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| frontend-missing-local | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| frontend-missing-local | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| frontend-missing-local | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| frontend-missing-typedef | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| frontend-missing-typedef | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 2/3 | 1.000 |
| frontend-missing-typedef | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| frontend-pointer-type | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| frontend-pointer-type | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| frontend-pointer-type | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| frontend-struct-member | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| frontend-struct-member | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| frontend-struct-member | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| frontend-void-result | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| frontend-void-result | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 2/3 | 1.000 |
| frontend-void-result | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| logic-factorial | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| logic-factorial | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| logic-factorial | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| logic-fibonacci-limit | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| logic-fibonacci-limit | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| logic-fibonacci-limit | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| logic-for-sum | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| logic-for-sum | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| logic-for-sum | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| logic-nested-count | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| logic-nested-count | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| logic-nested-count | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| logic-piecewise | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| logic-piecewise | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| logic-piecewise | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| logic-precedence | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| logic-precedence | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| logic-precedence | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| logic-square | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| logic-square | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| logic-square | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| logic-switch-exit | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| logic-switch-exit | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| logic-switch-exit | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| logic-switch-value | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 2/3 | 1.333 |
| logic-switch-value | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| logic-switch-value | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| logic-truth-value | public source + injected defect | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| logic-truth-value | public source + injected defect | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| logic-truth-value | public source + injected defect | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| ub-addition-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| ub-addition-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c2 | 1/3 | 1.000 |
| ub-addition-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c3 | 2/3 | 1.000 |
| ub-array-bound | written for this corpus | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| ub-array-bound | written for this corpus | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| ub-array-bound | written for this corpus | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| ub-multiplication-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| ub-multiplication-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c2 | 2/3 | 1.000 |
| ub-multiplication-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| ub-negation-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| ub-negation-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| ub-negation-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c3 | 2/3 | 1.000 |
| ub-negative-shift-count | written for this corpus | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| ub-negative-shift-count | written for this corpus | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| ub-negative-shift-count | written for this corpus | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| ub-quotient-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| ub-quotient-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c2 | 2/3 | 1.000 |
| ub-quotient-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| ub-quotient-zero | written for this corpus | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| ub-quotient-zero | written for this corpus | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| ub-quotient-zero | written for this corpus | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| ub-remainder-zero | written for this corpus | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| ub-remainder-zero | written for this corpus | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| ub-remainder-zero | written for this corpus | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |
| ub-subtraction-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| ub-subtraction-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c2 | 2/3 | 1.000 |
| ub-subtraction-overflow | written for this corpus | deepseek/deepseek-v4.1-flash | c3 | 2/3 | 1.000 |
| ub-wide-shift-count | written for this corpus | deepseek/deepseek-v4.1-flash | c1 | 3/3 | 1.000 |
| ub-wide-shift-count | written for this corpus | deepseek/deepseek-v4.1-flash | c2 | 3/3 | 1.000 |
| ub-wide-shift-count | written for this corpus | deepseek/deepseek-v4.1-flash | c3 | 3/3 | 1.000 |

Stop: completed. Unattempted repetitions: 0.
Reported cost: USD 0.198289.
Rates include recorded errors in the attempted denominator. A range from incomplete repetitions is not a full-population estimate.

### Response failures and follow-up

All 13 terminal errors returned HTTP 200 with `finish_reason=length` and
4,096 completion tokens. Eleven had no final content; two had an incomplete
C block. No request needed a retry, and no tool or infrastructure failure
occurred. Two earlier candidates failed the hidden score before a later
response error: one omitted a required function, and one returned wrong results.
The small difference in all-attempt mean rounds comes from those failed
trials, not faster successful repairs.

The three C3 failures were `ub-addition-overflow` (repetition 1),
`ub-negation-overflow` (repetition 1), and `ub-subtraction-overflow`
(repetition 3). All three received an E3004 diagnostic and exhausted the
4,096-token completion budget. Two returned no final content; the third
returned a partial fenced response. These are not missing UB findings.

The frozen harness calls missing final content `malformed_response` and a
nonconforming fenced reply `invalid_candidate:fenced_file`. These labels do
not mean that the HTTP JSON was malformed. Its `acceptance` field remains
`false`, since it requires every attempted repetition to have no error.
Its `stop_reason` is `completed`: the experimental population finished,
but not every attempted repair succeeded. No field was changed to hide this.

A [follow-up in TODO](https://github.com/onehr/evocc/blob/main/TODO.md) asks whether concise E3004 guidance or a review-only
overflow repair plan helps under a separately registered response budget and
provider policy. That is an untested proposal, not an identified compiler
defect. Saturation changes behavior and cannot be a generic safe repair.
This run does not establish that an additional diagnostic or repair would
have prevented a model response from exhausting its token budget.

### Validation

The full workspace gate passed 3,228 tests with zero failures and five existing
ignored campaigns; Clippy and formatting passed. An earlier sandboxed run
failed two Linux tests because access to Apple Container was denied. The full
rerun with that access passed both. It started on clean `8f09f06c`; the later
`7eed5eac` fix changed only the Python query, its test and registration metadata,
not Rust or specification sources. The final Python checks passed 40 tests:
32 experiment checks and eight tracker checks. The archive and every external
transcript passed the credential-prefix scan. No Lean semantics changed.

Repair rate is passes divided by attempted repetitions, including recorded
errors and stops. Unattempted cells remain separate. Mean rounds includes all
attempted repetitions; report the success-only mean as well. A low mean caused
by early errors is not evidence of efficient repair. Repetition ranges are
descriptive minima and maxima, not confidence intervals. Partial groups can
contain different items and do not establish a full-population comparison.

Token-limit stops without final code remain failures; reasoning can use the budget.
Distinguish these from candidate-format errors and tool failures. The logic class
has no compiler or sanitizer findings. Differences there do not show a UB benefit.
This is one model on a small constructed corpus with bounded, self-contained C.
Finite-input output agreement is not proof of correctness or absence of UB.

## Reproduce or extend

Use a clean checkout of the tag, release `evo` and `evocc`, and the pinned image.
Use a new output directory outside the repo. The owner supplies the credential.
A dry run prints the plan; it does not validate execution prerequisites.

```sh
python3 tools/experiment/repair.py --out /private/tmp/repair-replay --dry-run
python3 tools/experiment/repair.py --corpus-tag agent-repair-corpus-2 --out /private/tmp/repair-replay
python3 tools/experiment/report.py /private/tmp/repair-replay/receipt.json
```

We invite reruns with `--model`. First update `models.txt` and the protocol,
including its request limit and new tag. Commit and tag before any request.
Run the full campaign with `--corpus-tag NEW_TAG`. Repeat `--model` for every
registered ID, in list order. Unregistered IDs are rejected. Do not rerun only
failed cases or change the frozen prompt, corpus, or conditions in place.
