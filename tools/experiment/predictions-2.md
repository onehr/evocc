# Second repair experiment: predictions

Status: frozen by the tag `agent-repair-experiment-2`. The two older
registration tags are unchanged. No measured request precedes this tag.

## Population and comparisons

The population is 30 programs, two models, five conditions and three repetitions.
There are at most 2,700 normal response slots. Each has at most one identical
retry, billed separately. Temperature is 0 and output budget is 16,384 tokens.
The model IDs and exact command, scoring and cost rules are in `protocol-2.json`.
The programs contain both a path-specific runtime defect and a logic defect.
No warning-only repair is counted as a semantic defect.

Preflight selects programs for which each model fails at least two of three
valid C0 first-response trials. Response errors do not count as difficult items.
This selection limits the study to this C0-hard corpus. It cannot establish a
general advantage for feedback across ordinary C programs.

Compute each comparison separately for each model. Use successful repetitions
for token, round, diff and location measures. These are conditional outcomes;
different success sets can affect a comparison. Report repair and completion
rates beside them. Do not substitute failed-response tokens with zero.

## Population decision

This decision was recorded after the preflight and before the tag.
The fresh preflight ran on 2026-10-08 with all 30 items, both models and
three C0 first responses each. Its receipt is
`tests/contracts/agent-repair-2-preflight.json`. An item is hard for a model
when at least two of its three scored responses fail the hidden test.
Response errors do not count as failures. Every other item is easy for that
model.

| Model | Hard | Easy | Easy with two or more passes | Response-error slots | Reported cost |
| --- | ---: | ---: | ---: | ---: | ---: |
| `deepseek/deepseek-v4.1-flash` | 1 | 29 | 11 | 50 of 90 | USD 2.082123 |
| `qwen/qwen3.5-9b` | 8 | 22 | 1 | 54 of 90 | USD 0.168873 |

The hard item for DeepSeek is `rolling-window-moments`. The hard items for
Qwen are `adler-stream-combine`, `fse-count-codec`, `fse-normalization`,
`huffman-weight-stats`, `json-comment-minify`, `json-member-sort`,
`json-string-escape` and `json-unicode-scan`.

More than six items are easy for the stronger model, DeepSeek. The corpus
therefore stays unchanged; no spare item replaces an easy item. Population
rule: For each model, predictions 1 to 5 are evaluated on the items that the
preflight marked hard for that model; the easy items are reported but not
used for the predictions.

DeepSeek's prediction population has one item. Three repetitions of one
item cannot support a general statement about this model or condition.

## Five predictions

1. **Token cost:** C3's median total model tokens per successful repair is
   strictly below both C1 and C2. Include input, output, prior failed rounds
   and retries. Show the median, range and successful sample count.
2. **Repair size:** C3's median final added-plus-removed line count is strictly
   below both C1 and C2. The baseline is the original program, not the last
   round's source. A smaller diff is not itself a correctness proof.
3. **Repair location:** the share of C3 successes that touch every hidden
   minimal-fix span is strictly above both C1 and C2. Also show the median
   fraction of changed lines inside spans. Span coordinates and insertion
   attribution follow `protocol-2.json`.
4. **Safe automation:** C4's median rounds and median total model tokens per
   successful repair are both strictly below C3. Record actual applied repairs
   and zero-model passes separately. An offered repair is not an applied repair.
5. **No-feedback control:** C0 is strictly worse than every other condition on
   all primary measures: tokens, rounds, diff size, all-span hit share and
   inside-span ratio. If this condition is not met, report that the proposed
   general feedback advantage did not hold on this corpus. Do not turn a tie
   or an incomplete observation into evidence of equal conditions.

There is no prediction for repair rate. Exclude response errors only from that
rate, and also show completion rate with all attempts in the denominator.

## Decision rules

Each prediction is labelled `held` or `did not hold`, with its observed values.
Ties fail a strict comparison. No successes, unknown usage, or an incomplete
population cannot establish a prediction; label the reason next to the result.
Unknown successful usage prevents a token prediction even if the known subset
looks better. Report every model and condition, including negative outcomes.

The cost cap can stop the run. Do not rerun failures, replace models, deepen the
prompt, or change the corpus after registration. Preflight failures are retained
but are not measured repairs. Predictions concern this finite corpus, not
statistical significance or general autonomous repair capability.
