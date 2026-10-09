# Agent repair experiment

The registered run is complete. It does not show an overall advantage for EvoCC.
The second experiment, below, stopped at the provider key limit after 444 of
900 repetitions. None of its five predictions held for either model.

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

Every successful repair came on the first response, so this corpus did not separate the conditions: the result is a ceiling effect, not evidence that the conditions are equal.

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

## Experiment 2

The first run reached a ceiling: every success came on the first response.
The second experiment uses harder programs and more measures. It asks what
the feedback costs and where the repairs go, not only whether they pass.

**Summary.** The run did not test the predictions. 277 of the 444
repetitions that ran ended in a response error. DeepSeek V4.1 Flash used the
whole 16,384-token output budget in 252 calls. Qwen3.5 9B did so in 206 calls
and reached the 300 s deadline in 183. The harness set no limit on reasoning
tokens. The provider key's spending limit stopped the run after 444 of 900
repetitions. Every DeepSeek answer that arrived passed (119 of 119), so the
corpus was again at the ceiling for that model. The preflight counted an item
as hard only after two scored failures, so an item with response errors
counted as easy, and DeepSeek's prediction population shrank to one item. A
third experiment must limit reasoning tokens, use fast providers, fund the key
for the whole run, and count an item as easy only after scored passes.

### Predictions

The predictions were written before any measured request. They are in
[`predictions-2.md`](https://github.com/onehr/evocc/blob/main/tools/experiment/predictions-2.md),
frozen by the tag `agent-repair-experiment-2`. Each comparison is strict and
separate for each model. Tokens, rounds, diff size and span overlap use only
successful repetitions.

1. **Token cost.** C3 has a lower median of total model tokens per successful
   repair than C1 and C2.
2. **Repair size.** C3 has a lower median of added plus removed lines than C1
   and C2. The diff is against the original program.
3. **Repair location.** C3 has a higher share of successes that touch every
   minimal-fix span than C1 and C2.
4. **Safe automation.** C4 has fewer median rounds and fewer median tokens per
   successful repair than C3.
5. **No-feedback control.** C0 is worse than every other condition on all five
   measures: tokens, rounds, diff size, all-span share and inside-span ratio.

A tie, an incomplete population, unknown token usage or no measured success
cannot establish a prediction. There is no prediction for repair rate.

### Corpus

The corpus has 30 programs of 153 to 283 lines, in three classes of ten:
JSON and text processing, integer and checksum code, and binary codecs.
Twenty programs adapt functions from pinned zlib 1.3.2, zstd 1.5.7 and cJSON 1.7.19
sources; ten were written for this corpus. Each program has two injected
defects: a runtime fault on one input path and a logic defect. Fixing only
one defect still fails the hidden test.

Each item has a hidden driver with at least ten input sets plus the original
entry point, the exact expected output, a minimal fix and the fix spans in
original line numbers. The [tool qualification receipt](https://github.com/onehr/evocc/blob/main/tests/contracts/agent-repair-2-tool-preflight-20261008.json)
records 30 original hidden failures, 30 fixed passes and 60 failures with one
of the two repairs. EvoCC, GCC UBSan and Clang UBSan each diagnose all 30
runtime faults. The original `crc-polynomial-combine` times out after its
sanitizer report; this counts as a failure.

### Conditions and harness

All five conditions use the same system prompt and a 16,384-token response
limit at temperature 0. The models are DeepSeek V4.1 Flash
(`deepseek/deepseek-v4.1-flash`) and Qwen3.5 9B (`qwen/qwen3.5-9b`) through
OpenRouter. Provider selection and default reasoning are not controlled.

- C0 gets the program and two extra sentences: "This program has defects.
  Return the complete corrected file." It gets no tool output.
- C1 and C2 are the GCC and Clang UBSan feedback of the first experiment.
- C3 is the EvoCC check, explain and repair-plan JSON of the first experiment.
- C4 applies `evo fix --apply` safe repairs to a copy before each model call.
  The model gets the edited source, the applied diff and C3 feedback. A
  changed source is scored before the model call, so a pass can use no tokens.

There are three repetitions and at most three rounds. Each round has at most
two HTTP attempts with the same payload. Two failed responses end the
repetition as a response error. Response errors leave the repair-rate
denominator; the completion rate keeps them.

The harness was changed before the tag. One preflight request had waited
about 14 minutes, although the HTTP timeout was 120 s. That timeout limits
each socket read. OpenRouter sends whitespace while the model generates, so
the read never timed out. Each attempt now has a total deadline of 300 s.
A timed-out attempt is a response error with unknown usage. Unknown usage no
longer stops the run; each such attempt adds USD 0.05 to the amount compared
with the cost cap. Slots run six at a time and start in the registered order.

### Preflight and population

Before the tag, each model gave three C0 first responses for each item.
An item is hard for a model when at least two scored responses fail the
hidden test. Response errors do not count as failures. Every other item is
easy for that model. The rule is the owner's; an earlier draft required
three valid responses, and that extra restriction was removed.

Three earlier preflight campaigns stopped early. One found a visible-contract
gap in an item, one stopped on unknown transport cost, and one stalled for
14 minutes. Their receipts are kept. The fresh preflight did not reuse them.

The fresh preflight ran all 180 slots in 2 hours 16 minutes, six at a time.
Reported cost was USD 2.2509958723. Another 78 attempts had unknown usage;
the key's aggregate usage grew by USD 2.386516 over the same period.

| Model | Hard | Easy | Easy with at least two passes | Response-error slots | HTTP attempts | Budget exhausted | Deadline reached | Reported cost |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| DeepSeek V4.1 Flash | 1 | 29 | 11 | 50/90 | 153 | 109 | 4 | USD 2.082123 |
| Qwen3.5 9B | 8 | 22 | 1 | 54/90 | 158 | 46 | 74 | USD 0.168873 |

DeepSeek passed 35 of its 40 scored responses. Most of its errors were
reasoning that used all 16,384 tokens without a final answer. Qwen failed 31
of its 36 scored responses. Most of its errors reached the 300 s deadline.
Its finished attempts took at most 253 s, and nine in ten took at most 172 s.

More than six items were easy for the stronger model. The registered
decision was therefore to keep the corpus and not to replace items. For
each model, the five predictions use only the items that the preflight marked
hard for that model. Easy items are reported but not used for predictions.
DeepSeek's prediction population is one item, `rolling-window-moments`.
Qwen's is eight items. These populations are too small for a general claim.

### Measured run

The tag `agent-repair-experiment-2` is commit `0701fea5`. It holds the corpus,
prompt, predictions, protocol, harness and preflight receipt. The run started
at 2026-10-08T16:17:48Z from a clean checkout of the tag. It used six parallel
slots and the protocol's USD 160 reported-cost cap.

The provider key has its own USD 10 spending limit. The run reached it at
about 22:41Z. From then on, OpenRouter refused every request with HTTP 403 and
the message "Key limit exceeded". The protocol stops a campaign when the
provider refuses the key, but the tagged harness checked only HTTP 401 and
402. It recorded each 403 as a response error and kept starting slots. The
raw receipt therefore shows 900 repetitions and `stop_reason=completed`;
914 of its 1,992 HTTP attempts are refusals.

The report applies the registered stop. Slots are repetitions, numbered from 0
in the registered order. The campaign ends at slot 443, the
first slot whose first request was refused. Slot 437 also met a refusal in
its third round and counts as stopped. Slots 444 to 899 are unattempted.
No model response, score or token count changes. All 914 refusals carry the
key-limit message in the transcripts.

| Measure | Value |
| --- | ---: |
| Attempted repetitions | 444 of 900 |
| Complete repetitions of the population | 1 of 3 (repetition 2 has 144 of 300 slots) |
| HTTP attempts | 1,080 (693 first attempts, 387 retries, 3 refused) |
| Passed / failed / response error / stopped | 144 / 23 / 275 / 2 |
| Attempts that exhausted 16,384 tokens | 458 (DeepSeek 252, Qwen 206) |
| Attempts that reached the 300 s deadline | 188 (DeepSeek 5, Qwen 183) |
| Reported cost | USD 5.8110284793 |
| Attempts with unknown usage | 188 |
| Known input / output tokens | 2,375,656 / 10,287,627 |
| C4 safe repairs applied | 14, in 6 repetitions; no pass before a model call |

The key's aggregate usage rose by USD 7.736067 during the run. The
difference from the reported cost, USD 1.925038, is not itemized; the
attempts with unknown usage are its likely source. The run used the whole
key limit. The protocol's USD 160 cap was not reached.

DeepSeek passed 119 repetitions, ended 104 as response errors and had one
stop. It returned 31 candidates that failed the hidden test, but each of
those repetitions later passed or ended as a response error. Qwen passed 25
repetitions, failed 23 and ended 171 as response errors. In 137 of those
171, Qwen had already returned at least one failing candidate.

### Results

The tables come from `tools/experiment/report.py` on the
[raw receipt](https://github.com/onehr/evocc/blob/main/tests/contracts/agent-repair-2-20261009.json).
Rates are fractions from 0 to 1. Repair rate leaves response errors out of the
denominator; completion rate counts every attempted repetition. Tokens,
rounds, diff and span measures use successful repetitions only. The groups
contain different items and repetitions because the run stopped in
repetition 2. Read them as descriptions of what ran, not as estimates.
The transcripts are in two archives,
[part 1](https://github.com/onehr/evocc/raw/refs/heads/main/tests/contracts/agent-repair-2-transcripts-20261009-part1.tar.gz)
and [part 2](https://github.com/onehr/evocc/raw/refs/heads/main/tests/contracts/agent-repair-2-transcripts-20261009-part2.tar.gz).

#### All items

| Model | Condition | Pass / attempted | Repair rate | Completion rate | Response errors | Other errors/stops | Mean rounds | Success rounds median | Tokens per success median (range; n) | Unknown success usage | Diff median | All-span share | Inside-span median | Auto repairs / auto-only passes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| deepseek/deepseek-v4.1-flash | c0 | 17/45 | 1 | 0.378 | 28 | 0 | 1 | 1 | 26272 ([7440, 33925]; n=17) | 0 | 6 | 0.824 | 0.667 | 0/0 |
| deepseek/deepseek-v4.1-flash | c1 | 25/45 | 1 | 0.556 | 20 | 0 | 1.089 | 1 | 16669 ([8331, 53109]; n=25) | 0 | 4 | 0.8 | 1 | 0/0 |
| deepseek/deepseek-v4.1-flash | c2 | 28/45 | 1 | 0.622 | 17 | 0 | 1.067 | 1 | 17726 ([5121, 41643]; n=28) | 0 | 6.5 | 0.857 | 0.722 | 0/0 |
| deepseek/deepseek-v4.1-flash | c3 | 27/45 | 0.964 | 0.6 | 17 | 1 | 1.289 | 1 | 18701 ([4960, 44347]; n=27) | 0 | 6 | 0.741 | 0.5 | 0/0 |
| deepseek/deepseek-v4.1-flash | c4 | 22/44 | 1 | 0.5 | 22 | 0 | 1.25 | 1 | 15460 ([3794, 38219]; n=22) | 0 | 8 | 0.682 | 0.5 | 7/0 |
| qwen/qwen3.5-9b | c0 | 1/44 | 0.5 | 0.023 | 42 | 0 | 1.432 | 2 | 34873 ([34873, 34873]; n=1) | 0 | 10 | 1 | 0.4 | 0/0 |
| qwen/qwen3.5-9b | c1 | 6/44 | 0.6 | 0.136 | 34 | 1 | 2 | 2 | 24120.5 ([4557, 36355]; n=4) | 2 | 4.5 | 0.667 | 0.75 | 0/0 |
| qwen/qwen3.5-9b | c2 | 2/44 | 0.133 | 0.045 | 29 | 0 | 2.25 | 2 | 19101 ([10187, 28015]; n=2) | 0 | 27.5 | 1 | 0.49 | 0/0 |
| qwen/qwen3.5-9b | c3 | 10/44 | 0.833 | 0.227 | 32 | 0 | 2.136 | 2 | 14691.5 ([5671, 35581]; n=8) | 2 | 6 | 0.6 | 0.45 | 0/0 |
| qwen/qwen3.5-9b | c4 | 6/44 | 0.6 | 0.136 | 34 | 0 | 2.136 | 2 | 23965 ([4718, 30714]; n=6) | 0 | 4 | 0.833 | 1 | 7/0 |

#### Class binary_codecs

| Model | Condition | Pass / attempted | Repair rate | Completion rate | Response errors | Other errors/stops | Mean rounds | Success rounds median | Tokens per success median (range; n) | Unknown success usage | Diff median | All-span share | Inside-span median | Auto repairs / auto-only passes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| deepseek/deepseek-v4.1-flash | c0 | 5/18 | 1 | 0.278 | 13 | 0 | 1 | 1 | 16113 ([14273, 32262]; n=5) | 0 | 8 | 0.4 | 0.364 | 0/0 |
| deepseek/deepseek-v4.1-flash | c1 | 6/18 | 1 | 0.333 | 12 | 0 | 1.111 | 1 | 26055 ([13176, 36986]; n=6) | 0 | 6 | 0.5 | 0.486 | 0/0 |
| deepseek/deepseek-v4.1-flash | c2 | 6/18 | 1 | 0.333 | 12 | 0 | 1.111 | 1 | 18524.5 ([15086, 41643]; n=6) | 0 | 8 | 0.667 | 0.417 | 0/0 |
| deepseek/deepseek-v4.1-flash | c3 | 7/18 | 1 | 0.389 | 11 | 0 | 1.444 | 1 | 16131 ([7157, 36662]; n=7) | 0 | 8 | 0.286 | 0.25 | 0/0 |
| deepseek/deepseek-v4.1-flash | c4 | 5/18 | 1 | 0.278 | 13 | 0 | 1.389 | 1 | 11040 ([7166, 38219]; n=5) | 0 | 8 | 0.2 | 0.25 | 6/0 |
| qwen/qwen3.5-9b | c0 | 0/18 | 0 | 0 | 17 | 0 | 1.444 | NOT OBSERVED | NOT OBSERVED (NOT OBSERVED; n=0) | 0 | NOT OBSERVED | NOT OBSERVED | NOT OBSERVED | 0/0 |
| qwen/qwen3.5-9b | c1 | 2/18 | 1 | 0.111 | 16 | 0 | 1.778 | 1.5 | 36355 ([36355, 36355]; n=1) | 1 | 5 | 0 | 0.4 | 0/0 |
| qwen/qwen3.5-9b | c2 | 0/18 | 0 | 0 | 13 | 0 | 2.222 | NOT OBSERVED | NOT OBSERVED (NOT OBSERVED; n=0) | 0 | NOT OBSERVED | NOT OBSERVED | NOT OBSERVED | 0/0 |
| qwen/qwen3.5-9b | c3 | 3/18 | 0.75 | 0.167 | 14 | 0 | 1.944 | 2 | 20626 ([5671, 35581]; n=2) | 1 | 9 | 0 | 0.222 | 0/0 |
| qwen/qwen3.5-9b | c4 | 0/18 | 0 | 0 | 17 | 0 | 2.056 | NOT OBSERVED | NOT OBSERVED (NOT OBSERVED; n=0) | 0 | NOT OBSERVED | NOT OBSERVED | NOT OBSERVED | 6/0 |

#### Class integer_and_checksum

| Model | Condition | Pass / attempted | Repair rate | Completion rate | Response errors | Other errors/stops | Mean rounds | Success rounds median | Tokens per success median (range; n) | Unknown success usage | Diff median | All-span share | Inside-span median | Auto repairs / auto-only passes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| deepseek/deepseek-v4.1-flash | c0 | 9/14 | 1 | 0.643 | 5 | 0 | 1 | 1 | 27751 ([7440, 33017]; n=9) | 0 | 4 | 1 | 1 | 0/0 |
| deepseek/deepseek-v4.1-flash | c1 | 12/14 | 1 | 0.857 | 2 | 0 | 1.143 | 1 | 15585 ([8331, 53109]; n=12) | 0 | 4 | 1 | 1 | 0/0 |
| deepseek/deepseek-v4.1-flash | c2 | 13/14 | 1 | 0.929 | 1 | 0 | 1.071 | 1 | 14989 ([5121, 30826]; n=13) | 0 | 4 | 1 | 1 | 0/0 |
| deepseek/deepseek-v4.1-flash | c3 | 11/14 | 1 | 0.786 | 3 | 0 | 1.286 | 1 | 20931 ([4960, 44347]; n=11) | 0 | 4 | 1 | 1 | 0/0 |
| deepseek/deepseek-v4.1-flash | c4 | 10/14 | 1 | 0.714 | 4 | 0 | 1.214 | 1 | 15777 ([3794, 37914]; n=10) | 0 | 4 | 1 | 1 | 1/0 |
| qwen/qwen3.5-9b | c0 | 0/14 | NOT OBSERVED | 0 | 14 | 0 | 1.429 | NOT OBSERVED | NOT OBSERVED (NOT OBSERVED; n=0) | 0 | NOT OBSERVED | NOT OBSERVED | NOT OBSERVED | 0/0 |
| qwen/qwen3.5-9b | c1 | 4/14 | 0.8 | 0.286 | 9 | 0 | 2.143 | 2 | 15917 ([4557, 32324]; n=3) | 1 | 4 | 1 | 1 | 0/0 |
| qwen/qwen3.5-9b | c2 | 1/14 | 0.2 | 0.071 | 9 | 0 | 2.357 | 2 | 10187 ([10187, 10187]; n=1) | 0 | 5 | 1 | 0.8 | 0/0 |
| qwen/qwen3.5-9b | c3 | 5/14 | 0.833 | 0.357 | 8 | 0 | 2.286 | 2 | 12708 ([8545, 33270]; n=5) | 0 | 4 | 1 | 1 | 0/0 |
| qwen/qwen3.5-9b | c4 | 4/14 | 0.667 | 0.286 | 8 | 0 | 2.357 | 2 | 14248 ([4718, 28922]; n=4) | 0 | 4 | 1 | 1 | 1/0 |

#### Class json_and_text

| Model | Condition | Pass / attempted | Repair rate | Completion rate | Response errors | Other errors/stops | Mean rounds | Success rounds median | Tokens per success median (range; n) | Unknown success usage | Diff median | All-span share | Inside-span median | Auto repairs / auto-only passes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| deepseek/deepseek-v4.1-flash | c0 | 3/13 | 1 | 0.231 | 10 | 0 | 1 | 1 | 33820 ([26076, 33925]; n=3) | 0 | 9 | 1 | 0.444 | 0/0 |
| deepseek/deepseek-v4.1-flash | c1 | 7/13 | 1 | 0.538 | 6 | 0 | 1 | 1 | 16776 ([10568, 34544]; n=7) | 0 | 4 | 0.714 | 1 | 0/0 |
| deepseek/deepseek-v4.1-flash | c2 | 9/13 | 1 | 0.692 | 4 | 0 | 1 | 1 | 17753 ([16262, 33153]; n=9) | 0 | 6 | 0.778 | 1 | 0/0 |
| deepseek/deepseek-v4.1-flash | c3 | 9/13 | 0.9 | 0.692 | 3 | 1 | 1.077 | 1 | 21170 ([6586, 36259]; n=9) | 0 | 6 | 0.778 | 0.333 | 0/0 |
| deepseek/deepseek-v4.1-flash | c4 | 7/12 | 1 | 0.583 | 5 | 0 | 1.083 | 1 | 18595 ([7299, 36300]; n=7) | 0 | 8 | 0.571 | 0.333 | 0/0 |
| qwen/qwen3.5-9b | c0 | 1/12 | 1 | 0.083 | 11 | 0 | 1.417 | 2 | 34873 ([34873, 34873]; n=1) | 0 | 10 | 1 | 0.4 | 0/0 |
| qwen/qwen3.5-9b | c1 | 0/12 | 0 | 0 | 9 | 1 | 2.167 | NOT OBSERVED | NOT OBSERVED (NOT OBSERVED; n=0) | 0 | NOT OBSERVED | NOT OBSERVED | NOT OBSERVED | 0/0 |
| qwen/qwen3.5-9b | c2 | 1/12 | 0.2 | 0.083 | 7 | 0 | 2.167 | 2 | 28015 ([28015, 28015]; n=1) | 0 | 50 | 1 | 0.18 | 0/0 |
| qwen/qwen3.5-9b | c3 | 2/12 | 1 | 0.167 | 10 | 0 | 2.25 | 2 | 35029 ([35029, 35029]; n=1) | 1 | 9.5 | 0.5 | 0.341 | 0/0 |
| qwen/qwen3.5-9b | c4 | 2/12 | 0.667 | 0.167 | 9 | 0 | 2 | 2 | 29988 ([29262, 30714]; n=2) | 0 | 12 | 0.5 | 0.55 | 0/0 |

#### Items hard for that model in preflight (prediction population)

| Model | Condition | Pass / attempted | Repair rate | Completion rate | Response errors | Other errors/stops | Mean rounds | Success rounds median | Tokens per success median (range; n) | Unknown success usage | Diff median | All-span share | Inside-span median | Auto repairs / auto-only passes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| deepseek/deepseek-v4.1-flash | c0 | 1/1 | 1 | 1 | 0 | 0 | 1 | 1 | 27751 ([27751, 27751]; n=1) | 0 | 4 | 1 | 1 | 0/0 |
| deepseek/deepseek-v4.1-flash | c1 | 1/1 | 1 | 1 | 0 | 0 | 1 | 1 | 13592 ([13592, 13592]; n=1) | 0 | 7 | 1 | 0.429 | 0/0 |
| deepseek/deepseek-v4.1-flash | c2 | 1/1 | 1 | 1 | 0 | 0 | 1 | 1 | 13770 ([13770, 13770]; n=1) | 0 | 7 | 1 | 0.429 | 0/0 |
| deepseek/deepseek-v4.1-flash | c3 | 1/1 | 1 | 1 | 0 | 0 | 1 | 1 | 12401 ([12401, 12401]; n=1) | 0 | 4 | 1 | 1 | 0/0 |
| deepseek/deepseek-v4.1-flash | c4 | 1/1 | 1 | 1 | 0 | 0 | 1 | 1 | 14442 ([14442, 14442]; n=1) | 0 | 4 | 1 | 1 | 0/0 |
| qwen/qwen3.5-9b | c0 | 0/12 | 0 | 0 | 11 | 0 | 1.667 | NOT OBSERVED | NOT OBSERVED (NOT OBSERVED; n=0) | 0 | NOT OBSERVED | NOT OBSERVED | NOT OBSERVED | 0/0 |
| qwen/qwen3.5-9b | c1 | 0/12 | 0 | 0 | 10 | 0 | 2.333 | NOT OBSERVED | NOT OBSERVED (NOT OBSERVED; n=0) | 0 | NOT OBSERVED | NOT OBSERVED | NOT OBSERVED | 0/0 |
| qwen/qwen3.5-9b | c2 | 1/12 | 0.143 | 0.083 | 5 | 0 | 2.333 | 2 | 28015 ([28015, 28015]; n=1) | 0 | 50 | 1 | 0.18 | 0/0 |
| qwen/qwen3.5-9b | c3 | 1/12 | 0.5 | 0.083 | 10 | 0 | 1.833 | 2 | 35029 ([35029, 35029]; n=1) | 0 | 11 | 0 | 0.182 | 0/0 |
| qwen/qwen3.5-9b | c4 | 1/12 | 0.333 | 0.083 | 9 | 0 | 2.083 | 2 | 30714 ([30714, 30714]; n=1) | 0 | 20 | 0 | 0.1 | 6/0 |

### Prediction results

Each model's predictions use only its preflight-hard items. For DeepSeek
that is one item, `rolling-window-moments`, which ran once per condition in
repetition 1. For Qwen it is eight items, with twelve attempted repetitions
per condition. The run stopped before either population was complete, so no
prediction can hold. The table also gives each observed comparison. `n` is
the number of successful repetitions behind a median or share.

| Model | Prediction | Result | Observed on the hard items | Reasons |
| --- | --- | --- | --- | --- |
| DeepSeek V4.1 Flash | P1 token cost | did not hold | C3 12,401 tokens; C1 13,592; C2 13,770 (n=1 each) | incomplete population |
| DeepSeek V4.1 Flash | P2 repair size | did not hold | C3 4 lines; C1 7; C2 7 (n=1 each) | incomplete population |
| DeepSeek V4.1 Flash | P3 repair location | did not hold | all-span share 1 in C3, C1 and C2 | incomplete population; tie |
| DeepSeek V4.1 Flash | P4 safe automation | did not hold | rounds C4 1, C3 1; tokens C4 14,442, C3 12,401 | incomplete population; tie in rounds; C4 used more tokens |
| DeepSeek V4.1 Flash | P5 no-feedback control | did not hold | C0 passed in one round with 27,751 tokens, a 4-line diff, every span touched and inside-span ratio 1 | incomplete population; ties in rounds and spans; C0 diff and inside-span ratio better than C1 and C2 |
| Qwen3.5 9B | P1 token cost | did not hold | C3 35,029 tokens; C2 28,015 (n=1 each); C1 no success | incomplete population; no C1 success; C3 used more tokens than C2 |
| Qwen3.5 9B | P2 repair size | did not hold | C3 11 lines; C2 50 (n=1 each); C1 no success | incomplete population; no C1 success |
| Qwen3.5 9B | P3 repair location | did not hold | all-span share C3 0, C2 1 (n=1 each); C1 no success | incomplete population; no C1 success; C3 lower than C2 |
| Qwen3.5 9B | P4 safe automation | did not hold | rounds C4 2, C3 2; tokens C4 30,714, C3 35,029 (n=1 each) | incomplete population; tie in rounds |
| Qwen3.5 9B | P5 no-feedback control | did not hold | no success in C0 or C1; one each in C2, C3 and C4 | incomplete population; no C0 or C1 success |

The proposed general feedback advantage did not hold on this corpus. That
statement follows the registered rule for P5. It is not evidence that the
conditions are equal: most comparisons rest on one success or none.

Over all items, the conditions with tool feedback completed more DeepSeek
repetitions than C0: 0.500 to 0.622 against 0.378. For Qwen, C3 had the
highest completion rate, 0.227, against 0.023 for C0. These are descriptive.
There is no registered prediction for repair or completion rate, the run
stopped in repetition 2, and the groups contain different items.

### Limits

- **One run, stopped early.** The provider key limit ended the run after 444
  of 900 repetitions. Repetition 1 is complete; repetition 2 is partial and
  repetition 3 did not start. The protocol does not permit a continuation.
- **The stop was applied after the run.** The tagged harness did not stop on
  the HTTP 403 key refusal. The report applies the registered stop; the raw
  receipt keeps the 914 refused attempts.
- **Two models.** DeepSeek V4.1 Flash and Qwen3.5 9B, through OpenRouter.
  Provider routing and default reasoning were not controlled.
- **Our corpus.** The 30 programs are adaptations and constructions with
  injected defects, ten of them written for this corpus. They are not
  observed upstream bugs.
- **Response delivery dominates.** Most failures are reasoning that used the
  16,384-token budget, or Qwen attempts that reached the 300 s deadline.
  These are delivery limits, not wrong repairs. One finished Qwen attempt
  took 299.7 s, so the deadline probably also cut some slow responses.
- **Small prediction populations.** The hard-item rule left one DeepSeek item
  and eight Qwen items. Response errors made most items easy by rule, not
  because the model repaired them.
- **Conditional measures.** Tokens, rounds, diff size and span overlap use
  successful repetitions. Different success sets can drive a difference.
- **Few safe repairs.** `evo fix --apply` changed 6 of the attempted C4
  repetitions, and none passed before a model call. C4 was mostly C3 with an
  empty applied diff.

### Reproduce

Use a clean checkout of the tag, the release `evo` and `evocc` binaries, the
sealed image and a provider key with enough limit for the full plan.

```sh
git checkout agent-repair-experiment-2
python3 tools/experiment/repair.py --protocol tools/experiment/protocol-2.json --dry-run --out /private/tmp/repair-2-replay
python3 tools/experiment/repair.py --protocol tools/experiment/protocol-2.json --corpus-tag agent-repair-experiment-2 --parallel 6 --out /private/tmp/repair-2-replay
python3 tools/experiment/report.py /private/tmp/repair-2-replay/receipt.json
```

The tagged harness does not stop on an HTTP 403 key refusal. Watch the key
limit. The report applies the registered stop when it reads the receipt.

## Experiment 3: pilots with a coding agent

These pilots used Kimi Code 2.0.2, `kimi-code/k3`, low effort, one session per
item and condition, 100 steps and a 900-second limit. The agent could edit
files and call `check FILE.c`. Scoring used the sealed GCC 14 `-O0` hidden
test. This was not a registered full experiment.

| Pilot | Attempted / planned | Hidden tests passed | Steps | Tokens, including cached input | Check calls | Time |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Initial | 9/9 | 7/9 | 81 | 2,160,549 | 23 | 1,354.9 s |
| Sandboxed re-pilot | 6/9 | 6/6 | 68 | 1,759,542 | 30 | 1,391.5 s |

The initial pilot did not isolate the agent's shell. It could fetch public
source and run host tools. Its conditions therefore did not control tool
access, and its 7/9 score is not a valid feedback comparison. The original
audit also counted scratch use among its deviations; that count is not a
count of successful escapes.

The re-pilot used a Seatbelt execution allowlist and separate workspaces.
The sandbox denied repository, hidden-test and unrelated home-file reads.
An opaque `check` client sent requests to the host harness. All conditions
could build and run a file: C0 used plain GCC, C1 added warnings and UBSan,
and C3 used EvoCC plus its check, explanation and repair-plan records.
The audit recorded four blocked attempts, zero escapes and 27 scratch uses.

All six attempted final files passed. Five sessions completed; the sixth
was stopped on an overload notice after it had already written a passing
repair. The three `json-merge-patch` sessions did not start. The two completed
item groups thus hit a ceiling again; they do not show equal capability on
other defects. The next experiment removes C0 and changes the corpus, not
the prompt or the C1/C3 feedback bundles.

Sources: [initial receipt](../../tests/contracts/agent-repair-3-pilot-20261009.json),
[initial wire logs](../../tests/contracts/agent-repair-3-pilot-wire-20261009.tar.gz),
[re-pilot receipt](../../tests/contracts/agent-repair-3-repilot-20261009.json),
[re-pilot wire logs](../../tests/contracts/agent-repair-3-repilot-wire-20261009.tar.gz).

## Experiment 4: silent undefined behavior

This experiment asks a narrower question: does EvoCC feedback help a coding
agent repair defects that the default GCC warnings and UBSan do not report?
The registered method is [protocol-4.md](../../tools/experiment/protocol-4.md).
The tag `agent-repair-experiment-4` is commit
`5b765db0467dc92a94077e3f37617c04e16f24b0`. Measurement began from its clean
tree, after qualification and the six-session pilot.

### Method and qualification

Kimi Code 2.0.2 used `kimi-code/k3`, low effort, 100 steps and 900 seconds per
session. Each of 24 items ran once in C1 and once in C3. Conditions alternated
order by sorted item name. The prompt and check command did not change from
Experiment 3. Each session had a fresh Seatbelt workspace.

C1 built and ran with GCC 14 `-O0 -Wall -Wextra -fsanitize=undefined`.
C3 built and ran with EvoCC, then supplied `evo check`, diagnostic explanations
and repair plans. In both conditions the agent could read the source, edit it
and check its own C test drivers. This compares tool bundles. It does not
isolate JSON formatting or a single diagnostic feature.

The final source had to pass the same hidden test under sealed GCC 14 at O0
and O2, and Clang 19 at O2. The hidden driver checked twelve input seeds and
the program return value. Removing a diagnostic was not enough.

The corpus has six items each for uninitialized reads (E3001), dangling-pointer
access (E3010) and out-of-bounds access (E3011). Six controls have two items
each for division by zero (E3002), signed overflow (E3004) and shift counts
(E3005). All programs have 83 to 147 lines and one injected defect.

Eighteen items adapt six public kernels from zlib 1.3.2, Zstandard 1.5.7 and
cJSON 1.7.19. Each kernel has three memory-defect variants. The XXH32 and
XXH64 items adapt the byte-tail and avalanche operations, not the full hash
APIs. The six controls were written for this corpus. These are injected
defects, not defects found upstream.

All 24 items qualified. Each fixed source passed five runs of each scoring
build with identical output: 360 fixed executions. The 360 original executions
established a hidden-test failure for each item. The 18 silent originals
produced the same C1 main output and exit code as their fixed source, without
C1 warnings or runtime findings. EvoCC reported the requested code at the
repair line for all 24 items, without E3008. Fixed sources passed `evo check`.

Two candidates were replaced once before registration. The first empty-mean
contract matched the native divide-by-zero result and did not fail its hidden
test. A remainder candidate reported E3003 rather than E3002. Their rejected
records remain in the qualification receipt. No class has a shortfall.

See the [corpus and origins](../../tests/fixtures/agent-repair-4/README.md)
and [qualification receipt](../../tests/fixtures/agent-repair-4/qualification.json).

### Detection matrix

Y means a finding at a line in the fixed repair span. N means no finding
there, not absence of a finding elsewhere. All tool builds and runtimes
were available. Columns use these exact configurations:

- G0/G2: GCC 14 `-Wall -Wextra`, at O0/O2.
- CW: Clang 19 `-Weverything -O0`.
- GU/CU: GCC 14 / Clang 19 `-fsanitize=undefined -O0`.
- MS: Clang 19 `-fsanitize=memory -O0`.
- CA: Clang 19 `-fsanitize=address -fsanitize-address-use-after-scope -O0`,
  with `ASAN_OPTIONS=detect_stack_use_after_return=1`.
- GA: GCC 14 `-fsanitize=address -O0`. E: `evo check`.

| Item | Code | G0 | G2 | CW | GU | CU | MS | CA | GA | E |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| adler-byte | E3011 | N | N | Y | N | N | N | Y | Y | Y |
| adler-reset | E3001 | N | Y | N | N | N | N | N | N | Y |
| adler-view | E3010 | N | N | Y | N | N | N | Y | Y | Y |
| crc-byte | E3011 | N | N | Y | N | N | N | Y | Y | Y |
| crc-reset | E3001 | N | Y | N | N | N | N | N | N | Y |
| crc-view | E3010 | N | N | Y | N | N | N | Y | Y | Y |
| increment-limit | E3004 | N | N | N | Y | Y | N | N | N | Y |
| left-count | E3005 | N | N | N | Y | Y | N | N | N | Y |
| magnitude-limit | E3004 | N | N | N | Y | Y | N | N | N | Y |
| mean-empty | E3002 | N | N | N | Y | Y | N | N | N | Y |
| minify-byte | E3011 | N | N | Y | N | N | N | Y | Y | Y |
| minify-reset | E3001 | N | Y | N | N | N | N | N | N | Y |
| minify-view | E3010 | N | N | Y | N | N | Y | Y | Y | Y |
| pointer-byte | E3011 | N | N | Y | N | N | N | Y | Y | Y |
| pointer-reset | E3001 | N | Y | N | N | N | N | N | N | Y |
| pointer-view | E3010 | N | N | Y | N | N | Y | Y | Y | Y |
| right-count | E3005 | N | N | N | Y | Y | N | N | N | Y |
| scale-empty | E3002 | N | N | N | Y | Y | N | N | N | Y |
| xxh32-byte | E3011 | N | N | Y | N | N | N | Y | Y | Y |
| xxh32-reset | E3001 | N | Y | N | N | N | N | N | N | Y |
| xxh32-view | E3010 | N | N | Y | N | N | N | Y | Y | Y |
| xxh64-byte | E3011 | N | N | Y | N | N | N | Y | Y | Y |
| xxh64-reset | E3001 | N | Y | N | N | N | N | N | N | Y |
| xxh64-view | E3010 | N | N | Y | N | N | N | Y | Y | Y |

MemorySanitizer reports uninitialized values at later uses in all six
`reset` items; its N cells must not be read as missed defects. The Clang
warning cells include general unsafe-buffer warnings. ASan reports the
out-of-bounds and dangling-pointer cases. The selected silence applies to
C1's default GCC warnings and UBSan, not to every other tool.

### Pilot and quota

The three pilot items cover one silent class each. PASS requires all three
hidden scoring builds. Tokens include cached input. Scratch use is allowed;
a blocked attempt is not a successful escape.

| Item | Condition | GCC O0 / O2 / Clang O2 | Steps | Tokens | Checks | Seconds | Blocked / escapes / scratch | Weekly ratio increase |
| --- | --- | --- | ---: | ---: | ---: | ---: | --- | ---: |
| adler-reset | c1 | FAIL / FAIL / FAIL | 19 | 398,911 | 6 | 143.5 | 3 / 0 / 6 | 0.003281 |
| adler-reset | c3 | PASS / PASS / PASS | 7 | 141,206 | 4 | 71.2 | 1 / 0 / 2 | 0.001300 |
| xxh32-view | c3 | PASS / PASS / PASS | 6 | 113,512 | 1 | 53.4 | 0 / 0 / 4 | 0.000604 |
| xxh32-view | c1 | PASS / PASS / PASS | 5 | 93,902 | 1 | 27.7 | 0 / 0 / 3 | 0.000542 |
| pointer-byte | c1 | PASS / PASS / PASS | 6 | 94,194 | 2 | 38.6 | 0 / 0 / 5 | 0.000658 |
| pointer-byte | c3 | PASS / PASS / PASS | 6 | 116,151 | 2 | 67.5 | 0 / 0 / 3 | 0.000848 |

The pilot completed six sessions, with C1 at 2/3 and C3 at 3/3. It recorded
49 steps, 957,876 tokens, 16 checks and 401.9 seconds, with zero escapes.
The C1 `adler-reset` repair removed the uninitialized read but changed the
seeded checksum. It failed the functional score and was not replaced.

The seven-day used ratio rose from 0.070627 to 0.077860: an increase of
0.007233. The mean per session was 0.0012055, giving a 48-session projection
of 0.057864. This was below 0.30, so all 24 items were retained.
The formal limit was the initial ratio plus 0.30, or an absolute ratio of
0.85, whichever came first. Quota was checked between sessions.

Sources: [pilot receipt](../../tests/contracts/agent-repair-4-pilot-20261009.json)
and [pilot wire logs](../../tests/contracts/agent-repair-4-pilot-wire-20261009.tar.gz).

### Measured results

All 48 registered sessions completed, from 12:11:47 to 12:55:31 UTC on
2026-10-09. No session was retried. No provider overload, scoring error,
unknown pair, unattempted cell or sandbox escape occurred. The audit recorded
nine blocked attempts and 143 allowed scratch uses.

| Population | Items | C1 repaired | C3 repaired | C1 only | C3 only |
| --- | ---: | ---: | ---: | ---: | ---: |
| E3001: uninitialized read | 6 | 6 | 6 | 0 | 0 |
| E3010: dangling pointer | 6 | 6 | 6 | 0 | 0 |
| E3011: out-of-bounds access | 6 | 6 | 6 | 0 | 0 |
| All silent items | 18 | 18 | 18 | 0 | 0 |
| E3002: division by zero | 2 | 1 | 1 | 0 | 0 |
| E3004: signed overflow | 2 | 2 | 2 | 0 | 0 |
| E3005: shift count | 2 | 2 | 2 | 0 | 0 |
| All controls | 6 | 5 | 5 | 0 | 0 |
| Total | 24 | 23 | 23 | 0 | 0 |

Each condition passed 23/24 under each of the three scoring builds.
`mean-empty` failed all three builds in both conditions. C3 inserted a zero
return for an empty mean, although the source comment required -1. C1 changed
the parameter generator so that it never produced zero. Both removed the
executed division by zero but changed the required result. These are incorrect
repairs, not provider or scoring failures.

The pilot's C1 `adler-reset` failure did not recur in the measured session.
The registered measured result stands; it is not pooled with the pilot.

| Condition | Sessions | Steps | Tokens, including cached input | Check calls | Agent wall time |
| --- | ---: | ---: | ---: | ---: | ---: |
| C1 | 24 | 167 | 3,232,689 | 62 | 1,446.4 s |
| C3 | 24 | 119 | 2,274,022 | 46 | 978.9 s |

These are descriptive totals, not separately tested efficiency claims.
The agent used 5,506,711 tokens in total, including 5,296,896 cached-input
tokens. The seven-day quota ratio rose from 0.077860 to 0.113758, an increase
of 0.035898. Neither the 0.30 increase limit nor the 0.85 absolute limit
was reached. The run stopped with `completed`, not with a quota refusal.

### Predictions and limits

**P1 did not hold.** Both conditions repaired all 18 silent items.
There were zero C1-only and zero C3-only repairs. The two-sided exact
McNemar p-value is 1. There is no observed repair-count advantage for C3
on this selected population.

**P2 held.** Both conditions repaired 5/6 controls; the difference is zero,
within the registered limit of one. This is not an equivalence proof.

The silent-item result is a ceiling in both conditions. Silent default
build-and-run feedback did not make these source-visible defects hard for
this agent. It could inspect the source and make its own tests in either
condition. A finding advantage is therefore not a measured repair advantage.

This small corpus shares six public kernels and defect scaffolding. Its
items are not independent samples of arbitrary C programs. The registered
p-value describes these fixed pairs; it does not support a general model or
tool ranking. There was one repetition, one model and one effort setting.
The line-specific matrix also shows that ASan, MSan and other compiler modes
can report defects outside the default comparison.

For autonomous repair, the failed control makes a separate point: removing
undefined behavior does not establish the required function result. The
hidden functional score must remain separate from diagnostic status.

Sources: [measured receipt](../../tests/contracts/agent-repair-4-20261009.json)
and [measured wire logs](../../tests/contracts/agent-repair-4-wire-20261009.tar.gz).
The receipt and expanded logs passed the existing secret scan and an exact
credential-value scan before publication. The scans found none of the
credential values checked; they do not prove that
an arbitrary transcript contains no sensitive information.
Reproduce the analysis without a model request:

```sh
python3 tools/experiment/report.py tests/contracts/agent-repair-4-20261009.json --json
```
