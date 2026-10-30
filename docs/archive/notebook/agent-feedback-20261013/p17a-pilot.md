# P17a: the Protocol A pilot

Status: the rerun of 2026-10-20 completed all 10 requests and holds every go
criterion, for a reported USD 0.018. The first run of 2026-10-14 stopped at
request 1, because the OpenRouter key had reached its total limit; the owner
then raised the limit. Nothing is registered. The measured run needs the
owner's approval.

The protocol is Protocol A in `protocols.md`. Its machine form is
`tools/experiment/protocol-6.json`, with the status `unregistered`. The receipt
is `tests/contracts/agent-repair-6-pilot-20261014.json`.

## Sources

- Date: 2026-10-14. The pilot began at 01:17:31 UTC.
- Repository commit at run time: `177f7acf`. The harness and the protocol file
  were not committed yet. The receipt records their SHA-256.
- Harness `tools/experiment/budget.py`, SHA-256
  `a255eb7ce9a3b53efd02ddfcc47301d4f0151aee8f84470ddd627a93896778f5`. It reuses
  the unchanged `tools/experiment/repair.py` (`4979683a`): the key loader, the
  P17 message layout, the C3 feedback, the parse rule, the candidate profile,
  the usage check and the sealed scorer.
- Protocol file `tools/experiment/protocol-6.json`, SHA-256
  `efccac437430d0392df41abed273d4103af9c6a9617071a090b8a767b42d8c34`.
- Compiler: `target/frontend/release/evo` (`075508aa`) and `evocc`
  (`93eacee4`), `evocc 0.1.0 (366fd796613e 2026-10-13)`. The values were the
  same before and after the pilot. This `evocc` equals
  `.build/frozen/366fd796/evocc-darwin`.
- Sealed image `evo-nexlab:sealed`, digest `sha256:e8d52bdc`. It equals the
  execution reference in `contracts/build-environment.json`.
- Corpus: the five E3004 items at tag `agent-repair-corpus-2` (`2644dad5`). The
  working files equal the tag.
- Captures: `.evidence/p17a-pilot-20261014/`, 1.0 MiB, ignored by Git.

## Rerun of 2026-10-20

- Compiler: `evocc 0.1.0 (7fec1ce553a3 2026-10-20)`, release `evocc`
  `49ddc244`, `evo` `7519053b`, at commit `0dfede74`. The harness, protocol
  file and prompt hashes equal those of the first run.
- Free check: `compare-g0` gives 5/5 G0 feedback byte-identical to P17
  repetition 1, round 1, with the current compiler.
- Receipt: `tests/contracts/agent-repair-6-pilot-20261020.json`. Raw
  responses: `.evidence/p17a-pilot-20261020/pilot/` (ignored by Git).

| Request | Item | Arm | Limit | Finish | Completion tokens | Reasoning tokens | Delivered | Correct | USD |
| ---: | --- | --- | --- | --- | ---: | ---: | --- | --- | ---: |
| 1 | `ub-addition-overflow` | G0 | L16 | stop | 9,715 | 9,439 | yes | yes | 0.00535 |
| 2 | `ub-addition-overflow` | G1 | L16 | stop | 4,639 | 4,494 | yes | yes | 0.00259 |
| 3 | `ub-negation-overflow` | G0 | L16 | stop | 4,900 | 4,762 | yes | yes | 0.00275 |
| 4 | `ub-negation-overflow` | G1 | L16 | stop | 1,097 | 990 | yes | yes | 0.00068 |
| 5 | `ub-subtraction-overflow` | G0 | L16 | stop | 1,098 | 950 | yes | yes | 0.00070 |
| 6 | `ub-subtraction-overflow` | G1 | L16 | stop | 1,497 | 1,346 | yes | yes | 0.00089 |
| 7 | `ub-multiplication-overflow` | G0 | L4 | length | 4,096 | 4,096 | no | no | 0.00232 |
| 8 | `ub-multiplication-overflow` | G1 | L4 | stop | 3,569 | 3,427 | yes | yes | 0.00201 |
| 9 | `ub-quotient-overflow` | G0 | L4 | stop | 673 | 567 | yes | yes | 0.00047 |
| 10 | `ub-quotient-overflow` | G1 | L4 | stop | 672 | 563 | yes | yes | 0.00045 |

Every response came from AtlasCloud and the registered model, with no retry.
Go criteria: all five hold. The provider accepted `max_tokens` 16,384; the
largest completion used 9,715 tokens. Nine responses were delivered, and all
nine were valid and correct. The cost was USD 0.018 against the USD 0.30 cap,
with no unknown-cost reserve.

Descriptive only, not an effect estimate: three of the six L16 responses were
longer than 4,096 completion tokens (two of three in G0, one of three in G1),
and one L4 response stopped at the limit before its answer. Pilot data stay
out of the measured analysis.

## Free checks before the pilot

These checks made no model request. Capture paths are relative to
`.evidence/p17a-pilot-20261014/`.

| Check | Result | Evidence |
| --- | --- | --- |
| G0 bytes against P17 repetition 1, round 1 | 5/5 equal after scratch-path replacement | `g0-identity/compare-g0.json` |
| Part B drafts under the sealed GCC 14 scorer | 5/5 pass | `part-b-drafts/drafts.json` |
| The unrepaired programs under the same scorer | 5/5 fail, as required | `part-b-drafts/negative-control.json` |
| Harness tests against a local fake HTTP server | 23/23 pass | `tools/experiment/tests/test_budget.py` |
| Free disk space before the pilot | 27 GiB | `df -h /` |

- The G0 feedback has 1,654 bytes for four items and 1,648 bytes for
  `ub-negation-overflow`, as in P17. The scratch directory has the same parent
  as in P17, so the byte counts match exactly.
- The exit codes are 1, 0 and 0, and stderr is empty, as in P17.
- The G1 feedback has 1,144 bytes, and 1,138 bytes for `ub-negation-overflow`.
- Each Part B replacement was applied at line 6, column 12. Each program passed
  `gcc-14 -O0`, the hidden driver and the exact `expected.txt`. Part B stays
  unapproved.

## The pilot requests

| Request | Item | Arm | Limit | Status | HTTP | Seconds | Reported cost |
| ---: | --- | --- | --- | --- | ---: | ---: | --- |
| 1 | `ub-addition-overflow` | G0 | L16 | stopped: `provider_refused:http_403` | 403 | 0.9 | unknown |
| 2 | `ub-addition-overflow` | G1 | L16 | unattempted | | | |
| 3 | `ub-negation-overflow` | G0 | L16 | unattempted | | | |
| 4 | `ub-negation-overflow` | G1 | L16 | unattempted | | | |
| 5 | `ub-subtraction-overflow` | G0 | L16 | unattempted | | | |
| 6 | `ub-subtraction-overflow` | G1 | L16 | unattempted | | | |
| 7 | `ub-multiplication-overflow` | G0 | L4 | unattempted | | | |
| 8 | `ub-multiplication-overflow` | G1 | L4 | unattempted | | | |
| 9 | `ub-quotient-overflow` | G0 | L4 | unattempted | | | |
| 10 | `ub-quotient-overflow` | G1 | L4 | unattempted | | | |

- Request 1 sent the frozen G0 feedback: 1,654 bytes, SHA-256 `498d3980`. The
  payload pinned `atlas-cloud` with `allow_fallbacks` false and set
  `max_tokens` 16,384. It had no `reasoning` field.
- The reply came after 0.9 seconds. Its error says "Key limit exceeded (total
  limit)": USD 10.45 of the key's USD 10.00 total limit is used, and the limit
  does not reset. The error names the key limit as its source. It names no
  provider.
- The protocol stops the run on HTTP 403 and allows no retry. The harness sent
  no other request.
- The raw reply is `pilot/001-a1.body`, 491 bytes, SHA-256 `3039807e`. It has
  no credential, but it has an account link, so it stays out of Git.

## Cost

- Reported cost: USD 0. The refusal has no usage record, so its cost is
  unknown, not zero.
- The harness charged a reserve of USD 0.0397716 against the cap: 1,000 input
  tokens at USD 0.45 per million and 16,384 output tokens at USD 2.40 per
  million. The reserve is cap accounting, not a reported charge.

## Go criteria

| Criterion | Held | Evidence |
| --- | --- | --- |
| All 10 responses report the pinned provider and the registered model. | no | 0 of 10 trials completed |
| Every response has usage, cost and `reasoning_tokens`. | no | no response |
| The provider accepts `max_tokens` 16,384 without a parameter error. | no | not tested: no L16 response |
| The harness extracts, validates and scores every delivered candidate. | no | not tested: no delivered candidate |
| The pilot cost stays below USD 0.30. | yes | USD 0 reported, plus the USD 0.0397716 reserve |

The receipt computes these values in its `go` field. The third and fourth
criteria are not tested. The pilot does not refute them.

## The pinned provider during the session

The harness fetched the public endpoint list without credentials before and
after the request. An earlier fetch, for the provider slug, is in
`discovery/`.

| Time (UTC) | AtlasCloud status | Uptime, last 5 minutes | Uptime, last 30 minutes | USD per million input tokens | USD per million output tokens |
| --- | ---: | ---: | ---: | ---: | ---: |
| 00:48 | 0 | 100% | 100% | 0.192 | 0.768 |
| 01:17 | -5 | 0% | 53% | 0.30 | 1.20 |

- Both fetches list `max_completion_tokens` 393,216 for the AtlasCloud
  endpoint `atlas-cloud/fp8`.
- This pilot cannot show whether a request with a working key would have
  succeeded at 01:17.
- In P17, AtlasCloud cost up to USD 0.141 and 0.564 per million tokens. At the
  01:17 list price, the pilot maximum is about USD 0.14. The measured run would
  cost about USD 1.44 at the expected response lengths of the protocol. Its
  maximum, about USD 6.47, is above the USD 3.50 cap, so the cap stops the run
  first.

## Deviations and harness rules

- The pilot stopped after 1 of its 10 requests. The run is partial. The
  criteria that need a response are incomplete.
- The harness applies every stopping rule of Protocol A. The protocol does not
  decide some cases. `protocol-6.json` records the harness decisions under
  `harness.rules`. They are proposals for the owner to confirm at registration:
  - An HTTP 200 body with an error object counts as an HTTP error with its
    code. OpenRouter sends the status before the model finishes.
  - Any other HTTP status, invalid JSON and an oversized body stop the run.
  - An attempt of unknown cost adds the planning-maximum reserve to the cap
    comparison.
  - A retry that fails again ends the trial as `request_error` and stops the
    run.
  - Each HTTP attempt has a total deadline of 900 seconds.
  - Before each trial, a changed `evo` or `evocc` hash, or changed G0 bytes,
    stops the run before the request.
  - The response may name the model by its alias or by its canonical slug,
    `deepseek/deepseek-v4.1-flash-20260910`.
- The captures are in the ignored `.evidence/` directory of the repository, as
  the task directed. The protocol asks for a location outside the repository.
- The harness ran from uncommitted files. The receipt records their SHA-256.

## Pilot data and the measured analysis

The pilot data stay out of the measured analysis. The one refused request has
no model data. It changes no sample size, order or prediction.

## What the owner must decide

- The key: raise or remove its total limit, or supply another key. The
  harness reads `OPENROUTER_KEY` from the environment or from `.env`.
- A new pilot needs a new approval. The harness and the protocol file have not
  changed since this pilot, so the same 10 requests can run again.
- The provider: AtlasCloud had an outage and a price change during this
  session. The owner fixes the provider at registration.
- After a pilot that meets the go criteria: registration, the order and the
  measured run, as Protocol A describes.

## Reproduce

The first three commands make no model request. The last one is paid and
needs the owner's approval.

```sh
python3 -I tools/experiment/budget.py compare-g0 --out NEW_DIRECTORY
python3 -I tools/experiment/budget.py score-drafts --out NEW_DIRECTORY
python3 -m unittest tools/experiment/tests/test_budget.py
python3 -I tools/experiment/budget.py run --phase pilot --out NEW_DIRECTORY
```

The output directory must be new, and outside the repository or in an ignored
path.

## Limits

- One request ran, and the key refused it. The pilot says nothing about E3004
  guidance, the response budget, the provider or the model.
- The free checks cover the five pilot items only.
