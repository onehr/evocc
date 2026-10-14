# P17a: the Protocol A pilot

Status: stopped at request 1 of 10. OpenRouter refused the request with HTTP
403, because the API key had reached its total spending limit. No model
response exists. The go criteria do not hold. The owner must decide on the key
before a new pilot. Nothing is registered.

The protocol is Protocol A in `protocols.md`. Its machine form is
`tools/experiment/protocol-6.json`, with the status `unregistered`. The receipt
is `tests/contracts/agent-repair-6-pilot-20261014.json`.

## Sources

- Date: 2026-10-14. The pilot began at 01:17:31 UTC.
- Repository commit at run time: `e74f3596`. The harness and the protocol file
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
- Corpus: the five E3004 items at tag `agent-repair-corpus-2` (`7eed5eac`). The
  working files equal the tag.
- Captures: `.evidence/p17a-pilot-20261014/`, 1.0 MiB, ignored by Git.

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
