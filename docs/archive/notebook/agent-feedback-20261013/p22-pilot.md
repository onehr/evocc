# P22: the Protocol B pilot

Status: all 10 sessions ran on 2026-10-20. Three go criteria hold. Two fail,
and both fail on one escape record that a concurrent repository writer
caused, not the agent. Under the protocol the pilot does not pass. Nothing is
registered. Pilot data stay out of the measured analysis.

## Sources

- Protocol: Protocol B in `protocols.md`, with the amendment of 2026-10-20
  (F is the current `full-1` with the corrected T03s scope sentence). Machine
  form: `tools/experiment/protocol-7.json`, status `unregistered`.
- Harness: `tools/experiment/agent_repair.py` at commit `8d1ef06c`.
- Compiler: release `evo` `7519053b` and `evocc` `49ddc244`,
  `evocc 0.1.0 (7fec1ce553a3 2026-10-20)`. The binaries did not change during
  the pilot.
- Agent: Kimi Code CLI 2.0.2 (`c2204148`), model `kimi-code/k3`, effort low,
  100 steps, 900 seconds, `prompt-3.txt`.
- Precondition (no model request): 48 of 48 `full-1` records and 48 of 48
  `concise-1` records equal the frozen strings after the scope correction; 24
  of each are also byte-identical, the 24 without a completed record. 48 of 48
  headers are equal. Record: `.evidence/p22-pilot-20261020/precondition.json`.
- Receipt: `tests/contracts/agent-repair-7-pilot-20261020.json`; transcripts:
  `tests/contracts/agent-repair-7-pilot-20261020-wire.tar.gz` (308 KB). The run
  directory is `/Users/onehrxn/dev/30days/evocc-runs/p22-pilot-20261020`,
  outside the repository.

## Sessions

| Session | Item | Form | Steps | Tokens | Check runs | Repaired |
| ---: | --- | --- | ---: | ---: | ---: | --- |
| 1 | `adler-byte` | F | 6 | 116,026 | 2 | yes |
| 2 | `adler-byte` | K | 6 | 113,515 | 2 | yes |
| 3 | `crc-reset` | K | 5 | 94,685 | 2 | yes |
| 4 | `crc-reset` | F | 5 | 95,399 | 2 | yes |
| 5 | `minify-view` | F | 5 | 96,689 | 2 | yes |
| 6 | `minify-view` | K | 4 | 76,470 | 2 | yes |
| 7 | `increment-limit` | K | 5 | 93,231 | 2 | yes |
| 8 | `increment-limit` | F | 4 | 74,008 | 1 | yes |
| 9 | `mean-empty` | F | 5 | 94,964 | 2 | yes |
| 10 | `mean-empty` | K | 5 | 93,622 | 2 | no |

Every session completed. Session 10's repair compiled under all three scoring
builds, but its output differs from the expected output: a functional
failure, as in experiment 4, where both failures were also `mean-empty`. The
7-day quota ratio rose from 0 to 0.0066 over 11 successful reads.

## Go criteria

| Criterion | Held | Evidence |
| --- | --- | --- |
| Zero escapes | no | one filesystem record in session 10 |
| Every K result starts with `{"form":"concise-1"` | yes | 10 of 10 |
| F equals the frozen string, after the scope correction, for an unchanged source | yes | 4 of 4 |
| Every quota read succeeds; steps, tokens and scores are recorded | yes | 11 of 11 |
| No unknown outcome | no | session 10, because of the escape record |

## The escape record

The record names `tests/fixtures/integer-complex-division-boundaries.c`. The
harness backstop counts every new untracked file in the repository during a
session. A Rust agent of the lead created that file at 10:02:23, during
session 10, for TODO L07. The session's tool calls, check log and wire log
never name the path, and the Seatbelt profile denies writes to the
repository. So the record is a false positive of the backstop, caused by
running the pilot while another process wrote to the repository. The
backstop itself behaved as designed.

## Next

- Run any further session with no other writer in the repository.
- Under the protocol, the measured run needs a pilot that holds all go
  criteria. The owner decides between a new 10-session pilot (about 0.7
  percent of the weekly quota) and the measured run.
