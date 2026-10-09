# Silent undefined behavior: registered method

Registration tag: `agent-repair-experiment-4`. Status: registered before measurement.
The tag fixes the corpus, qualification, harness, scorer, prompt and this method.
No measured session may use a changed file or a different commit.

## Question and population

On the selected silent defects, does C3 repair more items than C1?
The default baseline means GCC 14 `-O0 -Wall -Wextra -fsanitize=undefined`,
not every GCC mode, Clang warning, ASan or MSan.
Use all 24 directories in tests/fixtures/agent-repair-4/, sorted by name.
There are six each of E3001, E3010 and E3011, and two each of E3002, E3004
and E3005 as controls. Eighteen adapt six public kernels; six are synthetic.
The kernels share scaffolding. This is a selected corpus, not random C code.
The two replaced qualification candidates are not measured.

## Agent and conditions

Use the existing Seatbelt sandbox, Kimi Code 2.0.2, `kimi-code/k3`, effort low,
100 steps, 900 seconds and one repetition. Do not change the Kimi home config.
Use prompt-3.txt unchanged. Expose only program.c and the opaque check client.
C1 and C3 are unchanged from protocol-3.md. Every check builds and runs at O0.
There is no C0. Each item runs in both C1 and C3, in a fresh workspace.
Alternate their order by the item's sorted index. Run one session at a time.
There are 48 initial sessions. Do not modify the source or feedback after tagging.

## Scoring and missing results

The final source must pass the same hidden driver and exact output under
sealed GCC 14 O0, GCC 14 O2 and Clang 19 O2. No one build can replace another.
Keep every attempt, final diff, check count, token count and sandbox audit.
A timeout or step limit is scored from its final source. A provider failure,
scoring failure or escape is unknown, not evidence of an incorrect repair.
Use the authorized retry's final result, not the best result of the attempts.
Report unknown and unattempted cells. An incomplete primary population cannot
establish P1 or P2. Keep pilot results separate from the measured population.

## Quota and provider policy

Read the seven-day ratio before the first measured session and before each next
session. Stop at initial ratio plus 0.30, or absolute ratio 0.85, whichever is
lower. If the read fails, stop; do not assume zero usage. Read again at the end.
The limit is checked between sessions; record the final observed increase.
Wait out a five-hour ratio at or above 0.90 using the existing pacing rule.
Allow the CLI's first three 429 overloaded retry notices in a session.
The fourth ends that session as provider_overloaded. Retry it once after all
initial sessions. A quota/limit/usage 429 or authentication error stops the run.
Keep all retry cost in the quota and descriptive counts. Do not rerun failures.
Keep each capture below 500 MiB and outside the repository. Use the existing
secret checks and the credential-value scan before committing evidence.

## Pilot and scale

The pilot used adler-reset, xxh32-view and pointer-byte, each in C1 and C3.
All six sessions completed. C1 passed 2/3; C3 passed 3/3 on all three builds.
The weekly ratio rose from 0.070627 to 0.077860: increase 0.007233.
Mean increase was 0.0012055 per session; projection for 48 is 0.057864.
This is below 0.30. Keep all 24 items. Do not select items by repair outcome.
Evidence: tests/contracts/agent-repair-4-pilot-20261009.json.

## Predictions and reporting

P1: on the 18 silent items, C3 has more repaired items than C1, and the
two-sided exact paired McNemar test gives p < 0.05. For discordant counts b,c,
p = min(1, 2 * sum(binomial(b+c,k), k=0..min(b,c)) / 2^(b+c)); p=1 at b+c=0.
P2: on the six controls, the repaired counts differ by at most one.
Steps, tokens, check runs, wall time, detection matrix and per-class counts
are descriptive. Do not promote the small pilot or a subgroup to a new test.
The matrix records findings at the fix span; other-line findings are retained
in the raw records and must not be described as absence of detection.
State the shared-kernel, selected-corpus and single-repetition limits.
No repair-rate claim implies proof of all C inputs or superiority to all tools.
