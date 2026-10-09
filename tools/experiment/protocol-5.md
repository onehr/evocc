# Repair effort: registered replication and scale comparison

Registration tag: `agent-repair-experiment-5`. Register before measurement.
The tag fixes both corpora, qualification, feedback, harness, scorer and analysis.
Do not change them, prompt-3.txt, the conditions or the model after registration.

## Question and populations

Test the effort pattern selected after experiment 4, not a repair-rate claim.
Part A uses all 24 unchanged agent-repair-4 items, repetitions 2 and 3: 96 sessions.
Its 18 silent items have E3001/E3010/E3011; its six controls have E3002/E3004/E3005.
Report experiment 4 repetition 1 beside these results, but exclude it from tests.
Part B uses all 12 qualified agent-repair-5 items, repetitions 1 and 2: 48 sessions.
There are four items per E3001/E3010/E3011, each 722 lines. No replacements occurred.
They adapt cJSON 1.7.19, zlib 1.3.2 and Zstandard 1.5.7; none is synthetic-only.
The items share a module bundle and four input workloads. They are not independent
projects or a random sample. Size and program structure change together.
The qualification retains the reference tests, profile, spans and detection matrix.
Feedback identity passed 96 raw-byte comparisons against experiment 4 qualification:
24 items, original and fixed source, both C1 and C3. No normalization was used.

## Agent, order and scoring

Use Kimi Code 2.0.2, kimi-code/k3, effort low, 100 steps, prompt-3.txt and Seatbelt.
Record the actual CLI version and logged model. Do not edit the Kimi home config.
Keep C1 and C3 unchanged. Expose only program.c and the opaque check client.
Each session starts in a fresh workspace. Run one session at a time, A then B.
Visit sorted items. Zero-based even indices use C1 first, odd indices C3 first;
reverse that order on the next repetition. A repetition 2 reverses repetition 1.
Wall limits are 900 seconds for A and 1,800 seconds for B.
Score the unchanged hidden driver under sealed GCC 14 O0/O2 and Clang 19 O2.
A repair must complete and pass all three builds. A timeout or step limit is not
repaired, even if its final source passes; retain its consumed steps and tokens.
Provider failures, scoring errors and escapes are unknown, not failed repairs.
Use the final authorized retry, never the best attempt. Keep all attempt costs.
Report unknown, unattempted, censored, check and audit counts separately.

## Quota and pilot

The first pilot quota was 0.113758. The package-wide stop line is 0.363758,
below the absolute 0.85 ceiling. Carry it across pilot, A and B; do not reset it.
Read before each session and at the end. Stop on a failed read. The existing
boundary check may observe usage only after a session; report the final increase.
Keep the existing five-hour pacing rule. Permit three CLI 429 overloaded retry
notices; the fourth ends that session. Retry it once after the initial cells.
A quota/limit/usage 429 or authentication refusal stops the package.
The four-session pilot used codec-controls-reset and codec-window-byte in C1/C3.
All four completed and passed. Their weekly increase was 0.011111, mean 0.00277775.
The fixed projection is 48 * 0.00277775 + 96 * 0.00075 = 0.205332, below 0.25.
Keep B at two repetitions. Including pilot, the estimate is 0.216443.
Pilot evidence: tests/contracts/agent-repair-5-pilot-20261010.json. Exclude it
from measured results. Keep each external capture below 500 MiB; scan secrets.

## Registered analysis

Average complete C1/C3 repetition pairs within each item. The item is the unit.
Include censored effort. If stopped early, use available complete pairs only and
label affected predictions incomplete; do not impute unknown or unattempted cells.
P1: on A's silent items, C3 uses fewer steps than C1.
P2: on the same items, C3 uses fewer total tokens than C1. Include cached input.
P3: on B's items, C3 uses fewer steps than C1.
For P1-P3 use exact one-sided Wilcoxon signed-rank tests of C1 minus C3 item means.
Drop zero differences, use tied midranks and enumerate all signs for the null.
A prediction holds at p < 0.05. Report the item count and nonzero count; no
multiplicity adjustment was specified. These are three individual tests.
P4: B's relative step saving exceeds A's silent-item saving. For each population,
saving = 1 - sum(C3 item means) / sum(C1 item means). This is a point comparison,
not a significance test; report no p value. It cannot isolate a causal size effect.
Repair rates, controls, per-class totals and old repetition 1 are descriptive.
Repair rates exclude unknown outcomes; also show attempted and planned counts.
Retain unfavorable results. Do not choose subgroups or add tests after observation.
