# Agent experiments

Does EvoCC's feedback help an AI agent repair a bug in C? We ran six experiments with a measured
run, in this order. Each one fixed a flaw of an earlier one. Protocols 6 and 7 have pilots only.

| # | Model and programs | Result |
| --- | --- | --- |
| [1](#experiment-1-a-chat-model-and-30-short-programs) | DeepSeek V4.1 Flash, as a chat model; 30 short programs | no difference: every success came on the first response, a ceiling |
| [2](#experiment-2) | DeepSeek V4.1 Flash and Qwen3.5 9B; 30 longer programs | inconclusive: most answers ran out of output tokens or time, and the spending limit stopped the run |
| [3](#experiment-3-pilots-with-a-coding-agent) | Kimi K3, as a coding agent; pilots | without a sandbox, the agent downloaded the original sources; with one, a ceiling again |
| [4](#experiment-4-silent-undefined-behavior) | Kimi K3, sandboxed; 18 silent bugs, 6 controls | the same repairs, 23 of 24 with each toolchain; fewer steps with EvoCC, found after the run |
| [5](#experiment-5-repair-effort-and-program-size) | Kimi K3, registered replication; 18 short and 12 longer programs | fewer steps and fewer tokens with EvoCC, p < 0.001 |
| [8](#experiment-8-chat-models-and-silent-bugs) | DeepSeek V4.1 Flash, MiniMax M3.1 Flash and Qwen3.8 27B, as chat models; the 24 programs of experiment 4 | fewer tokens with EvoCC for all three, p < 0.01; more repairs with Qwen3.8 27B, 54 against 30 of 54, p = 0.004 |

Experiments 5 and 8 found registered differences. In experiment 5, a coding agent on Kimi K3 repaired bugs
that GCC 14's warnings and UBSan do not report: uninitialized reads, dangling pointers and
out-of-bounds accesses. With EvoCC, it used 37% fewer steps and 40% fewer tokens on 18 short
programs, and 35% fewer steps on 12 programs of 722 lines. Each of the three registered
predictions held with p < 0.001, over 144 sessions.

In experiment 8, chat models got the same bugs
with the frozen records of experiment 4 and answered once for each trial. Three of four models
completed all 144 trials. Each used fewer completion tokens with EvoCC: 76% fewer with DeepSeek
V4.1 Flash, 59% with Qwen3.8 27B and 16% with MiniMax M3.1 Flash (p < 0.01 for each). Qwen3.8 27B
also repaired 54 of 54, against 30 of 54 with GCC and UBSan (p = 0.004). The other two models
repaired nearly all bugs with both records, so their repair counts did not differ. On control bugs
that GCC and UBSan also report, the two toolchains cost about the same, and EvoCC lost at most one
trial.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="images/agent-results-dark.svg">
    <img src="images/agent-results-light.svg" alt="The same bugs, with GCC 14's warnings and UBSan or with EvoCC: the effort with EvoCC as a share of the effort with GCC and UBSan, and the share of runs that repaired the bug. Silent bugs: GCC 14's warnings and UBSan report nothing. Kimi K3, coding agent, 18 short programs: 325 to 206 steps (−37%), repaired 33 to 36 of 36. Kimi K3, coding agent, 12 programs of 722 lines: 241 to 156 steps (−35%), repaired 24 of 24 with both. Qwen3.8 27B, chat model, 18 short programs: 2,717 to 1,103 tokens (−59%), repaired 30 to 54 of 54. DeepSeek V4.1 Flash, chat model, 18 short programs: 6,421 to 1,534 tokens (−76%), repaired 50 to 54 of 54. MiniMax M3.1 Flash, chat model, 18 short programs: 1,108 to 932 tokens (−16%), repaired 54 of 54 with both. Control bugs: GCC and UBSan report them. Kimi K3, coding agent, 6 programs: 63 to 60 steps (−5%), repaired 11 of 12 with both. Qwen3.8 27B, chat model, 6 programs: 959 to 886 tokens (−8%), repaired 18 of 18 with both. DeepSeek V4.1 Flash, chat model, 6 programs: 979 to 746 tokens (−24%), repaired 18 to 17 of 18. MiniMax M3.1 Flash, chat model, 6 programs: 701 to 770 tokens (+10%), repaired 15 to 16 of 18. Kimi K3: total agent steps over 2 runs of each program. Chat models: mean output tokens per answer over 3 runs. Each saving on silent bugs is a registered prediction that held, with p < 0.01. Mistral Small 4 stopped after 6 of 144 runs and is not shown." width="862">
  </picture>
</p>

`tools/render-readme.py` draws the figure from the receipts and the registered analyses.

The first experiments did not separate the toolchains. In experiments 1 and 2, UBSan also reported
each undefined behavior, and DeepSeek V4.1 Flash did not do better with EvoCC. In experiment 1 it
repaired 87 of 90, against 88 with GCC and 82 with Clang. In experiment 2 its completion rate was
0.600, against 0.556 with GCC and 0.622 with Clang. With Qwen3.5 9B, EvoCC had the highest
completion rate in experiment 2: 10 of 44, against 6 with GCC, 2 with Clang and 1 with no feedback.
That run stopped early and registered no test of completion, so this is a hint, not a result.

Kimi K3 ran at low reasoning effort in Kimi Code 2.0.2, which logs it as `kimi-code/k3`.

## Limits

The coding-agent result comes from one model at one effort setting and two repetitions. The
chat-model result comes from three models, one response for each trial and three repetitions at
temperature 0. Both use injected bugs in programs adapted from zlib, Zstandard and cJSON. They do
not rank models or toolchains. A clean check is not a correct repair: several repairs removed the
undefined behavior and changed the required result.

## Follow-ups

Protocol 6, P17a (DeepSeek V4.1 Flash), tests shorter E3004 guidance and the response budget. It
is registered, and its pilot of 2026-10-20 met every go criterion. Protocol 7, P22 (Kimi K3), tests
the concise check record. Its pilot of 2026-10-20 stopped on an escape record, so it is not
registered. The Mistral Small 4 run of experiment 8 stopped at trial 6 of 144 on a provider rate
limit, so it tests nothing. Pilot data stay out of every analysis.

## Experiment 1: a chat model and 30 short programs

This registered run does not show an overall advantage for EvoCC. Every
successful repair came on the first response, so the corpus did not separate
the conditions.

### Question and corpus

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

### Frozen method

The measured registration is `agent-repair-corpus-2`, commit `2644dad5`.
Each registration in this chapter was a local Git tag on the commit that the
text names. The public repository has the commits, not the tags. To replay a
run, create the tag on its commit, because the harness checks the tag name
that its protocol records.
Registration 1 made no model request: its file query exceeded the capture limit.
Registration 2 fixed the query.

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

### Results and limits

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

#### Response failures

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

The receipt keeps `acceptance` false, because acceptance requires every attempted repetition
to finish without an error.

#### Measures and limits

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

### Reproduce or extend

Use a clean checkout of commit `2644dad5`, release `evo` and `evocc`, and the
pinned image. Use a new output directory outside the repo. The owner supplies
the credential. A dry run prints the plan; it does not validate execution
prerequisites.

```sh
git checkout 2644dad50c16a6c2360be1f1f133ff1e38c4b767
git tag agent-repair-corpus-2 2644dad50c16a6c2360be1f1f133ff1e38c4b767
python3 tools/experiment/repair.py --out /tmp/repair-replay --dry-run
python3 tools/experiment/repair.py --corpus-tag agent-repair-corpus-2 --out /tmp/repair-replay
python3 tools/experiment/report.py /tmp/repair-replay/receipt.json
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
frozen at commit `8ac3fa1d` (`agent-repair-experiment-2`). Each comparison is
strict and separate for each model. Tokens, rounds, diff size and span overlap use only
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

The registration `agent-repair-experiment-2` is commit `8ac3fa1d`. It holds the
corpus, prompt, predictions, protocol, harness and preflight receipt. The run
started at 2026-10-08T16:17:48Z from a clean checkout of that commit. It used
six parallel slots and the protocol's USD 160 reported-cost cap.

The provider key has its own USD 10 spending limit. The run reached it at
about 22:41Z. From then on, OpenRouter refused every request with HTTP 403 and
the message "Key limit exceeded". The protocol stops a campaign when the
provider refuses the key, but the registered harness checked only HTTP 401 and
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
- **The stop was applied after the run.** The registered harness did not stop on
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

Use a clean checkout of commit `8ac3fa1d`, the release `evo` and `evocc`
binaries, the sealed image and a provider key with enough limit for the full
plan.

```sh
git checkout 8ac3fa1dd38f3aa8e85e96a45489c927227cf170
git tag agent-repair-experiment-2 8ac3fa1dd38f3aa8e85e96a45489c927227cf170
python3 tools/experiment/repair.py --protocol tools/experiment/protocol-2.json --dry-run --out /tmp/repair-2-replay
python3 tools/experiment/repair.py --protocol tools/experiment/protocol-2.json --corpus-tag agent-repair-experiment-2 --parallel 6 --out /tmp/repair-2-replay
python3 tools/experiment/report.py /tmp/repair-2-replay/receipt.json
```

The registered harness does not stop on an HTTP 403 key refusal. Watch the key
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
The registration `agent-repair-experiment-4` is commit
`1a1e91754c54b83c687836220127c5e0eb6b156e`. Measurement began from its clean
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

**Exploratory analysis (not registered).** The totals hide a pattern by defect
class. Where only EvoCC reported the defect, C3 needed fewer steps and tokens.
Where both toolchains reported it, C3 cost the same or slightly more, because
its feedback (the explanation page and the repair plan) is longer.

| Class | Items | Steps C1 | Steps C3 | Change | Tokens C1 | Tokens C3 | Change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| E3001: uninitialized read | 6 | 48 | 30 | −38% | 936k | 575k | −39% |
| E3010: dangling pointer | 6 | 38 | 29 | −24% | 728k | 552k | −24% |
| E3011: out-of-bounds access | 6 | 54 | 30 | −44% | 1,066k | 578k | −46% |
| Controls (E3002, E3004, E3005) | 6 | 27 | 30 | +11% | 502k | 568k | +13% |

C3 sessions were uniform: four to six steps and one or two check calls on every
item. C1 sessions ranged from four to 19 steps; six silent items took C1 ten
steps or more. Over the 24 pairs, C3 used fewer steps on 12 items, more on 4
and the same on 8 (one-sided sign test p = 0.038). Over tokens, C3 used fewer
on 12 items and more on 12. The saving is in the hard cases: EvoCC bounds the
search when the default toolchain is silent. This analysis was chosen after
the data; a registered replication must confirm it.
Experiment 5 confirms lower effort on these silent items, but not larger relative savings on larger programs or the earlier control penalty.
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

## Experiment 5: repair effort and program size

The registered replication found lower effort with C3 on both selected
populations. P1, P2 and P3 held. P4 did not hold: the relative step saving
was 36.62% on the small silent items and 35.27% on the larger items.
The earlier control penalty did not repeat in the aggregate.
These results concern the fixed corpora and tool bundles, not arbitrary C
programs, all agent models or JSON formatting alone.

### Registration and method

[Protocol 5](../../tools/experiment/protocol-5.md) was committed before
measurement. The registration `agent-repair-experiment-5` is commit
`859ab6a4501f579da75306875a0ae98c0780c17b`. Both parts ran from that clean
commit. The corpora, prompt, conditions, scoring and analysis did not change
after that commit. Kimi Code 2.0.2 logged `kimi-code/k3` and low effort in every
session. Each session used a fresh Seatbelt workspace and a 100-step limit.

Part A repeated all 24 experiment 4 items twice, as repetitions 2 and 3,
with a 900-second limit: 96 sessions. Part B used 12 larger items twice,
with a 1,800-second limit: 48 sessions. Items ran in sorted order, one
session at a time. C1/C3 order alternated by item and reversed in the next
repetition. The four pilot sessions and experiment 4 repetition 1 were not
included in any registered test.

C1 remained GCC 14 O0 warnings plus UBSan. C3 remained the EvoCC build,
check, explanation and repair-plan bundle. The agent could inspect source
and write its own test drivers in either condition. Final sources had to
pass the same hidden tests under GCC 14 O0, GCC 14 O2 and Clang 19 O2.
The Part B hidden driver used 16 seeds and checked the program return value.

A timeout or step limit would retain its effort and count as not repaired,
even if the final source passed. Provider failures would remain unknown.
The registered tests average each item's complete C1/C3 repetition pairs,
then apply an exact one-sided Wilcoxon signed-rank test. They drop zero
differences, use tied midranks and enumerate all signs. The item, not each
session, is the unit. P1-P3 use individual, unadjusted p < 0.05 thresholds.
P4 is a comparison of point values with no significance test.

### Larger corpus and feedback identity

All 12 larger programs qualified, with no replacements or class shortfall:
four each of E3001, E3010 and E3011. Each program has 722 lines and one
injected defect, more than 300 lines before `main`, with several callers.
There are no preprocessor directives or I/O in the programs. No item is
synthetic-only, and none represents a defect found in upstream software.

The programs adapt modules from cJSON 1.7.19, zlib 1.3.2 and Zstandard 1.5.7:
bounded JSON parsing and printing, checksums, hashing, symbol counts,
canonical prefix codes, and LZ matching and decoding. All modules affect
the fingerprint. The four input workloads share this module bundle and
the same three defect shapes. They are not twelve independent projects.
An early scratch workload exceeded interpreter fuel; it was shortened
before making the corpus. No interpreter limit or feedback was changed.

Qualification ran each fixed build five times, for 180 fixed executions,
and retained 180 original executions. Every fixed build passed with stable
output. Every original failed its hidden test, stayed silent at the fix
span in C1, and received its requested C3 finding without E3008. Unrelated
warnings remain in the records. See the [corpus and source hashes](../../tests/fixtures/agent-repair-5/README.md)
and [qualification](../../tests/fixtures/agent-repair-5/qualification.json).

Part A feedback matched experiment 4 in all 96 raw-byte comparisons:
24 items, original and fixed sources, C1 and C3. The check reproduced the
recorded source paths; it did not normalize diagnostics. The [identity
record](../../tests/contracts/agent-repair-5-feedback-identity.json) retains
the expected and observed hashes and compiler hashes.

The detection matrix uses the same configurations as experiment 4 above.
Counts are findings at the fix span, out of four items in each class.
G0/G2 are GCC warnings at O0/O2; CW is Clang `-Weverything` at O0.
GU/CU are GCC/Clang UBSan; MS is Clang MSan; CA/GA are Clang/GCC ASan.
All nine configurations were available for all twelve programs.

| Code | G0 | G2 | CW | GU | CU | MS | CA | GA | EvoCC |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| E3001 | 0/4 | 4/4 | 0/4 | 0/4 | 0/4 | 0/4 | 0/4 | 0/4 | 4/4 |
| E3010 | 0/4 | 0/4 | 4/4 | 0/4 | 0/4 | 0/4 | 4/4 | 4/4 | 4/4 |
| E3011 | 0/4 | 0/4 | 4/4 | 0/4 | 0/4 | 0/4 | 4/4 | 4/4 | 4/4 |

MSan reported uninitialized data at a later use in all four E3001 programs.
Its zero fix-span count does not mean that it missed those defects. Clang's
warning findings include general unsafe-buffer warnings. The corpus is
silent under C1's default configuration, not under every compiler mode.

### Pilot and quota

All four pilot sessions completed and passed all three hidden builds.
Tokens include cached input. A blocked attempt is not a sandbox escape.

| Item | Condition | Steps | Tokens | Seconds | Blocked / escapes / scratch | Weekly ratio increase |
| --- | --- | ---: | ---: | ---: | --- | ---: |
| codec-controls-reset | C1 | 14 | 371,668 | 124.6 | 1 / 0 / 3 | 0.004066 |
| codec-controls-reset | C3 | 6 | 143,192 | 50.3 | 0 / 0 / 2 | 0.001554 |
| codec-window-byte | C3 | 17 | 460,856 | 109.2 | 3 / 0 / 15 | 0.003433 |
| codec-window-byte | C1 | 9 | 220,471 | 88.4 | 1 / 0 / 7 | 0.002058 |

The pilot increased the weekly ratio by 0.011111, or 0.00277775 per session.
The fixed projection was `48 * 0.00277775 + 96 * 0.00075 = 0.205332`.
It was below 0.25, so Part B retained two repetitions before registration.
Including pilot usage, the estimate was 0.216443. See the [pilot receipt](../../tests/contracts/agent-repair-5-pilot-20261010.json)
and [pilot wire logs](../../tests/contracts/agent-repair-5-pilot-20261010-wire.tar.gz).

The package began at 0.113758 and used one stop line, 0.363758, across all
three runs. The final ratio was 0.288314: an increase of 0.174556, below
both the 0.25 package limit and the absolute 0.85 ceiling. Pilot, A and B
increases were 0.011111, 0.085899 and 0.077546. The reads occur between
sessions; quota is not a dollar-cost measure.

### Measured results

Part A ran from 18:31:51 to 19:47:08 UTC; Part B ran from 19:47:09 to
20:42:10 UTC, both on 2026-10-09. All 144 sessions completed. There were
zero censored sessions, unknown outcomes, unattempted cells or escapes.
The provider issued 21 overload retry notices in A and 15 in B. No session
received more than three, so the CLI handled them within the registered
rule. No entire session was retried. No quota or authentication refusal
occurred. All retry effort and quota usage remain included.

The audit recorded 31 blocked attempts and 322 scratch uses in A, and
23 blocked attempts and 189 scratch uses in B. Neither run escaped.
The old repetition below is descriptive and is not pooled into the tests.
All totals include unsuccessful repairs, not only successful sessions.

| Population | Repetitions | C1 repaired | C3 repaired | Steps C1 | Steps C3 | Tokens C1 | Tokens C3 |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Experiment 4: all | 1 | 23/24 | 23/24 | 167 | 119 | 3,232,689 | 2,274,022 |
| Experiment 4: silent | 1 | 18/18 | 18/18 | 140 | 89 | 2,730,457 | 1,705,853 |
| Experiment 4: controls | 1 | 5/6 | 5/6 | 27 | 30 | 502,232 | 568,169 |
| A: all | 2, 3 | 44/48 | 47/48 | 388 | 266 | 7,502,202 | 4,923,097 |
| A: silent | 2, 3 | 33/36 | 36/36 | 325 | 206 | 6,382,052 | 3,845,772 |
| A: controls | 2, 3 | 11/12 | 11/12 | 63 | 60 | 1,120,150 | 1,077,325 |
| B: all silent | 1, 2 | 24/24 | 24/24 | 241 | 156 | 5,795,989 | 3,465,780 |

Repair rates are 91.67% versus 97.92% in A and 100% versus 100% in B.
There are no unknown outcomes to remove from these denominators. Repair
rate differences are descriptive; this experiment registered effort tests.

| Part / class | C1 repaired | C3 repaired | Steps C1 | Steps C3 | Tokens C1 | Tokens C3 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| A / E3001 | 10/12 | 12/12 | 111 | 72 | 2,204,488 | 1,289,223 |
| A / E3010 | 11/12 | 12/12 | 104 | 70 | 2,074,218 | 1,311,979 |
| A / E3011 | 12/12 | 12/12 | 110 | 64 | 2,103,346 | 1,244,570 |
| A / E3002 | 3/4 | 3/4 | 21 | 18 | 394,589 | 340,086 |
| A / E3004 | 4/4 | 4/4 | 22 | 21 | 353,184 | 357,774 |
| A / E3005 | 4/4 | 4/4 | 20 | 21 | 372,377 | 379,465 |
| B / E3001 | 8/8 | 8/8 | 77 | 60 | 1,768,462 | 1,356,311 |
| B / E3010 | 8/8 | 8/8 | 91 | 46 | 2,242,344 | 1,003,615 |
| B / E3011 | 8/8 | 8/8 | 73 | 50 | 1,785,183 | 1,105,854 |

A's four C1 failures changed required behavior. In repetition 2,
`adler-view` discarded the caller's seed and `xxh64-reset` changed hash
constants. In repetition 3, `crc-reset` changed the polynomial multiplier.
Both conditions also failed `mean-empty` in repetition 3: they returned
the input value for an empty mean, rather than the required -1. All five
failures failed all three scoring builds. Removing UB was not sufficient.

### Registered predictions and limits

The totals in this table sum item means, so they are half the two-repeat
session totals above. Each item contributes one paired observation.
An independent dynamic-programming calculation reproduced the exact
sign-enumeration probabilities. One zero step difference was dropped in P1.

| Prediction | Items / nonzero | C1 item-mean total | C3 item-mean total | Exact one-sided p | Result |
| --- | --- | ---: | ---: | --- | --- |
| P1: A silent steps | 18 / 17 | 162.5 | 103 | 5/131072 = 0.00003815 | Held |
| P2: A silent tokens | 18 / 18 | 3,191,026 | 1,922,886 | 7/131072 = 0.00005341 | Held |
| P3: B steps | 12 / 12 | 120.5 | 78 | 1/2048 = 0.00048828 | Held |
| P4: relative step saving | A: 18; B: 12 | A: 36.62% | B: 35.27% | No test | Did not hold |

The registered replication supports the earlier silent-item effort pattern
on this corpus: C3 used 36.62% fewer steps and 39.74% fewer tokens in A.
It also used 35.27% fewer steps in B. It did not show a larger relative
step saving with program size. P4's point difference is -1.35 percentage
points, not evidence that size has a negative causal effect.

The aggregate control penalty did not repeat: C3 used 60 versus 63 steps
and 1,077,325 versus 1,120,150 tokens. The small E3004/E3005 token increases
are descriptive. Neither these controls nor the earlier run isolate the
effect of feedback length. The separate follow-up task must keep this limit.

The small corpus shares six kernels; the large corpus shares one module
bundle and four workloads. Dependence between items limits the independent
item interpretation behind the nominal p-values. These are not independent
project-level results. Two repetitions, one model ID, one effort setting,
injected defects and fixed hidden inputs further limit generalization.
Program size and source structure change together. This experiment cannot
establish a causal size effect or the correctness of a repair for all inputs.

For autonomous repair, the functional failures show why clean diagnostics
must not replace an independent functional test. A localized UB repair can
still change required arithmetic or boundary behavior.

Sources: [registered analysis](../../tests/contracts/agent-repair-5-20261010.json),
[A receipt](../../tests/contracts/agent-repair-5-replication-20261010.json),
[B receipt](../../tests/contracts/agent-repair-5-scale-20261010.json),
[A wire logs](../../tests/contracts/agent-repair-5-replication-wire-20261010.tar.gz)
and [B wire logs](../../tests/contracts/agent-repair-5-scale-wire-20261010.tar.gz).
All 127 experiment tests passed before measurement. The existing secret
scan passed; an exact-value scan checked 3,242 capture files against four
private credential values and found none. These scans do not prove that
arbitrary transcripts contain no sensitive information.

Reproduce the registered analysis without making a model request:

```sh
python3 tools/experiment/report.py \
  tests/contracts/agent-repair-5-replication-20261010.json \
  --part-b tests/contracts/agent-repair-5-scale-20261010.json
```

## Experiment 8: chat models and silent bugs

This registered experiment puts the question of experiment 4 to chat models.
A chat model gets the program and one frozen feedback record. It answers once,
with the complete repaired program. It cannot run a tool or test its answer.

### Registration and method

[Protocol 8](../../tools/experiment/protocol-8.json) was committed before
measurement. The registration `agent-repair-experiment-8` is commit
`3e24b50950a0981c8ee34d3ada62a7faa86bd1ff`. All four runs started from its
clean tree at 10:36 UTC on 2026-10-29, one process for each model. The
harness is [`silent_chat.py`](../../tools/experiment/silent_chat.py).

The items are the 24 items of experiment 4. There are 18 silent bugs, six
each of E3001, E3010 and E3011, and 6 controls, two each of E3002, E3004 and
E3005. The feedback is frozen: each trial sends the record that the
experiment 4 qualification stored for the original program. No compiler runs
while a request is open.

- C1 is the GCC 14 record: a build at O0 with
  `-Wall -Wextra -fsanitize=undefined`, then a run. For a silent bug, it
  shows a clean build and the exit status of the run, and nothing more.
- C3 is the EvoCC record: the build and the run, the `evo check` diagnostic
  with its code and line, one `evo explain` page for each code and the
  `evo fix` plan.

The [prompt](../../tools/experiment/prompt.txt) asks for the complete
replacement `program.c` in one fenced block. Each model ran each item three
times in each condition: 144 trials. Repetitions ran in turn, with items in
sorted order. The first condition alternated by item and by repetition. Each
request went through OpenRouter to one pinned provider, with no fallback, at
temperature 0, low reasoning effort and at most 16,384 output tokens. A length
stop counts as not repaired. The scoring is that of experiment 4: the answer
must pass the hidden driver under GCC 14 at O0 and O2 and Clang 19 at O2.

| Model | Provider | USD per million tokens, input / output | Cost cap |
| --- | --- | --- | ---: |
| DeepSeek V4.1 Flash, `deepseek/deepseek-v4.1-flash` | AtlasCloud | 0.135 / 0.54 | $2 |
| Mistral Small 4, `mistralai/mistral-small-2603` | Mistral | 0.15 / 0.60 | $2 |
| MiniMax M3.1 Flash, `minimax/minimax-m3.1-flash` | MiniMax | 0.15 / 0.60 | $2 |
| Qwen3.8 27B, `qwen/qwen3.8-27b` | Cerebras | 0.99 / 1.49 | $3 |

The protocol registered three predictions for each model:

- P1: on the 18 silent items, C3 repairs more than C1.
- P2: on the 6 controls, C3 loses at most 2 of the 18 trials. This has no test.
- P3: on the 18 silent items, C3 uses fewer completion tokens than C1. A
  length stop counts its tokens.

P1 and P3 use the item as the unit and the exact one-sided Wilcoxon
signed-rank test of experiment 5. Each holds at p < 0.05. Only a run that
completes all 144 trials tests a prediction.

### Pilots

Four pilots of five trials came before registration. Each change below came
before the next pilot.

- Qwen3.5 9B received HTTP 429 from the shared pools of DeepInfra and then
  Parasail. Mistral Small 4, MiniMax M3.1 Flash and Qwen3.8 Flash replaced it.
- At 8,192 output tokens, low reasoning effort did not bound the reasoning,
  and one answer of each model stopped at the limit. The limit became
  16,384 tokens, and P3 was added.
- Qwen3.8 Flash on Alibaba used 11,000 to 13,000 reasoning tokens. Its median
  request took 296 seconds, and one request reached the 300-second deadline.
  Qwen3.8 27B on Cerebras replaced it.

The fourth pilot ran at the clean commit `9e9f9aec`. It met all six go
criteria for all four models, and the registration records the SHA-256
values of its receipts. All pilots cost $0.08. Pilot data stay out of the
analysis.

### Measured results

Each of the three models completed all 144 trials. Each request received HTTP
200 from the pinned provider with the registered model name, with no retry and
no score error.

DeepSeek V4.1 Flash had one length stop: `adler-byte`, C1, repetition 2. It
used all 16,384 output tokens, 16,192 of them for reasoning, and it counts as
not repaired. There were no deadlines and no other undelivered answers.

The Mistral Small 4 run received HTTP 429 from the shared pool of its
provider. The harness retried once after 2 seconds, as registered. Trials 1
and 3 passed on the retry. Trial 6 failed twice, and the run stopped after 6
of 144 trials. A run that stops early tests nothing.

| Model | Silent, C1 | Silent, C3 | Controls, C1 | Controls, C3 | Cost |
| --- | ---: | ---: | ---: | ---: | ---: |
| DeepSeek V4.1 Flash | 50/54 | 54/54 | 18/18 | 17/18 | $0.256 |
| MiniMax M3.1 Flash | 54/54 | 54/54 | 15/18 | 16/18 | $0.095 |
| Qwen3.8 27B | 30/54 | 54/54 | 18/18 | 18/18 | $0.527 |

The counts are repaired trials. By code, as C1 / C3, of 18 trials for each
silent code and 6 for each control code:

| Code | DeepSeek V4.1 Flash | MiniMax M3.1 Flash | Qwen3.8 27B |
| --- | --- | --- | --- |
| E3001 uninitialized read | 16 / 18 | 18 / 18 | 12 / 18 |
| E3010 dangling pointer | 18 / 18 | 18 / 18 | 3 / 18 |
| E3011 out-of-bounds access | 16 / 18 | 18 / 18 | 15 / 18 |
| E3002 division by zero | 6 / 5 | 3 / 4 | 6 / 6 |
| E3004 signed overflow | 6 / 6 | 6 / 6 | 6 / 6 |
| E3005 shift count | 6 / 6 | 6 / 6 | 6 / 6 |

No failed answer returned the program unchanged. Each failed answer changed
the source and failed the hidden test. All control failures came from
`mean-empty`: DeepSeek V4.1 Flash failed it once in C3, and MiniMax M3.1 Flash
failed it three times in C1 and twice in C3.

The token columns are means for each trial, and completion tokens include
reasoning tokens. The cost is the total of the cell. The models use
different tokenizers, so their prompt tokens differ.

| Model | Cell | Prompt | Completion | Reasoning | Cost |
| --- | --- | ---: | ---: | ---: | ---: |
| DeepSeek V4.1 Flash | C1, silent | 849 | 6,421 | 5,749 | $0.190 |
| | C3, silent | 1,357 | 1,534 | 866 | $0.048 |
| | C1, controls | 705 | 979 | 467 | $0.010 |
| | C3, controls | 1,228 | 746 | 233 | $0.008 |
| MiniMax M3.1 Flash | C1, silent | 1,000 | 1,108 | 442 | $0.041 |
| | C3, silent | 1,475 | 932 | 267 | $0.035 |
| | C1, controls | 860 | 701 | 197 | $0.009 |
| | C3, controls | 1,352 | 770 | 260 | $0.010 |
| Qwen3.8 27B | C1, silent | 952 | 2,717 | 1,941 | $0.270 |
| | C3, silent | 1,501 | 1,103 | 348 | $0.169 |
| | C1, controls | 820 | 959 | 347 | $0.040 |
| | C3, controls | 1,391 | 886 | 274 | $0.049 |

EvoCC's record is longer, so a C3 prompt has about 500 more tokens. On the
silent items, the shorter answers paid for that: C3 cost 75% less than C1
with DeepSeek V4.1 Flash, 37% less with Qwen3.8 27B and 14% less with
MiniMax M3.1 Flash. On the controls, the two conditions cost within $0.01 of
each other. The four runs cost $0.887 in
all, $0.009 of it for Mistral Small 4.

### Registered predictions and limits

| Model | P1 items / nonzero | P1 exact p | P1 | P2 C3 loss | P2 | P3 items / nonzero | P3 exact p | P3 |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- |
| DeepSeek V4.1 Flash | 18 / 4 | 1/16 = 0.0625 | Did not hold | 1 | Held | 18 / 18 | 1/262144 = 0.0000038 | Held |
| MiniMax M3.1 Flash | 18 / 0 | 1 | Did not hold | -1 | Held | 18 / 18 | 341/65536 = 0.0052 | Held |
| Qwen3.8 27B | 18 / 8 | 1/256 = 0.0039 | Held | 0 | Held | 18 / 18 | 1/262144 = 0.0000038 | Held |
| Mistral Small 4 | stopped | | Not tested | | Not tested | stopped | | Not tested |

An independent dynamic-programming calculation reproduced each exact
probability.

P1 could not hold for DeepSeek V4.1 Flash: only four items differed, and
with four nonzero items the smallest exact p is 1/16. MiniMax M3.1 Flash
repaired all 54 silent trials in both conditions. Both models were at or
near the ceiling, so their P1 results do not show that the two records help
equally. P3 held for both: with EvoCC's record, they repaired as many bugs
or more, with shorter answers. For Qwen3.8 27B, the largest gap is on
dangling pointers: 3 of 18 repaired with C1 and 18 of 18 with C3.

These are chat models with one response and no tools. Unlike the coding
agent of experiments 4 and 5, they cannot test a change. The 18 silent items
adapt six shared kernels, so they are not independent, and the nominal
p-values overstate the independent evidence. Three repetitions at
temperature 0 do not remove provider variation. The items were selected so
that GCC 14's warnings and UBSan miss each silent bug, so the C1 record is
empty by design. Other tools find some of these bugs: see the detection
matrix of experiment 4. The experiment compares two frozen records, not two
complete toolchains. Each model ran with one provider at low reasoning
effort.

Sources: [registered analysis](../../tests/contracts/agent-repair-8-20261029.json);
receipts for [DeepSeek V4.1 Flash](../../tests/contracts/agent-repair-8-deepseek-v4.1-flash-20261029.json),
[MiniMax M3.1 Flash](../../tests/contracts/agent-repair-8-minimax-m3.1-flash-20261029.json),
[Qwen3.8 27B](../../tests/contracts/agent-repair-8-qwen3.8-27b-20261029.json)
and [Mistral Small 4](../../tests/contracts/agent-repair-8-mistral-small-2603-20261029.json);
[wire logs](../../tests/contracts/agent-repair-8-wire-20261029.tar.gz) and
[pilot records](../../tests/contracts/agent-repair-8-pilot-20261029.tar.gz).
The archives leave out the bodies of the HTTP 429 responses, because they
carry the OpenRouter account ID. The receipts keep their SHA-256 values. An
exact-value scan found the private credential in none of the 1,021 capture
files.

Reproduce the registered analysis without making a model request:

```sh
python3 tools/experiment/silent_chat.py analyze \
  tests/contracts/agent-repair-8-deepseek-v4.1-flash-20261029.json \
  tests/contracts/agent-repair-8-minimax-m3.1-flash-20261029.json \
  tests/contracts/agent-repair-8-qwen3.8-27b-20261029.json \
  tests/contracts/agent-repair-8-mistral-small-2603-20261029.json
```
