# Second repair experiment: predictions

Status: draft until `agent-repair-experiment-2` is created. The two older
registration tags are unchanged. No measured requests may precede this tag.

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
