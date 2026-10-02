# N03c T1: bounded spill progress

Implementation/clean subject `02d6bd54`; bounded RED `74473d62`; EvoMIR 0.59.
[Plan](../../tests/contracts/codegen-n03c-termination-20261002.json),
[receipt](../../tests/contracts/codegen-n03c-t1-20261003-receipt.json).
Saturated call preparation stores already-evaluated stack arguments before
occupying the whole register pool. Each spill retry consumes a previously
unspilled pre-rewrite value or refuses; generated reload temporaries cannot
grow recursively. The independent verifier, ABI and instruction semantics
are unchanged. The historical multi-GiB reproducer was not rerun.

Both bounded REDs failed within 2,048 candidate visits before the repair.
Now **59 focused tests PASS**; full workspace **3,020/0/5 ignored**, 392 suites;
Clippy, formatting and target regeneration PASS; eight schedule tests PASS.
Darwin and Linux each execute and agree **384/384**, with byte-identical
twins and all 768 hashes equal to C2 OFF. Two six-TU A/B captures each
pass 12 checked/unchecked harness executions; all 24 object hashes and six
per-arm IR/chain/bundle bindings match C2. Zero proof rejections/bundle failures.
These are selected TUs plus reference dependencies, not full applications.

Object `.text` bytes, unchanged from C2 OFF. No size gain is claimed.

| TU | A checked | A unchecked | B checked | B unchecked |
| --- | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 128 | 128 | 128 |
| zlib deflate | 30,516 | 29,012 | 30,184 | 28,668 |
| zlib inflate | 28,404 | 26,664 | 28,264 | 26,520 |
| zstd compress | 124,024 | 121,652 | 122,972 | 120,592 |
| zstd decompress | 33,688 | 33,440 | 33,300 | 33,052 |
| cJSON | 30,228 | 30,116 | 29,496 | 29,384 |

**Disposition: correctness closed, compile bound missed.** Of 24 historical
median comparisons, 21 meet 1.25x. A unchecked deflate is 1.4046x; B checked
and unchecked zstd-compress are 1.4563x and 1.3640x. All samples and these
negative results stay frozen; their cause is not established. No performance
admission follows. All 257 step/output records are unchanged, but only 70
inputs are TV-admitted; 187 remain opaque and integer-mix's legacy 1001/1002
source-event mismatch remains NOT_VALIDATED. No kernel, latency, complete
allocator or verified-EqSat claim; broader N03c remains open.

The first measurement attempt was denied at container image inspection by
the sandbox, before any compilation; its failed receipt remains. The
authorized attempt used the same plan and limits, not new measurements of
an already-completed result.

**Disk:** retired 792 hash-checked idle intermediate objects (1,832,400
bytes). Logs/manifests/receipts and small reproducers remain; exact historical
runtime payloads are no longer retained. From the snapshot during initial
focused compilation, shared-cache growth is 196,472,832 allocated bytes and
retained-evidence net +1,622,016: scoped total **+198,094,848 bytes**. This
excludes tracked files and unrelated activity, not a global disk delta.
Free space is about 94 GiB; exact scopes are in the receipt.
