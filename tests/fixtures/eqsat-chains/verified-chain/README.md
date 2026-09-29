# R01a verified driver/evaluator chain

Captured September 29, 2026, from the shared chain implementation based on
`a4c5209`. Exact executable identities are in `native.json`. The source is
`tests/fixtures/verified-eqsat-chain.c`; `program.c` is its captured copy.

The first transaction changes the IR; the second is the mandatory checked
zero-region terminal transaction. `chain.json` binds every retained byte.
The independent archive audit reported two proof successes, zero malformed
bundles, zero proof rejections and zero unknowns. The driver and policy evaluator
produced identical chain records. The exact verified IR was lowered with the
existing Darwin ARM64 MIR path, assembled/linked with Apple Clang 21.0.0, and
executed with exit 0. `program.s` is the actual emitted assembly.

Small binary artifacts and the matching compiler/checker are retained locally
under `.evidence/eqsat-r01a-20260929/`, not committed here. To replay this exact
artifact subject while that checker is retained:

```sh
python3 tools/bench.py --verify-eqsat-chain tests/fixtures/eqsat-chains/verified-chain \
  --checker .evidence/eqsat-r01a-20260929/tools/evo-proofcheck
```

A newly built checker can independently replay each EVP bundle, but a different
checker hash is not the same frozen chain-admission subject. Do not rewrite the
historical record to suppress that distinction. These fixtures prove neither
native performance nor Linux/G2 whole-kernel acceptance.
