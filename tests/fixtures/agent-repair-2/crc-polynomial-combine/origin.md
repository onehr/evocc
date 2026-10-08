# crc-polynomial-combine

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zlib-1.3.2.tar.gz`, SHA-256 `bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`. The source is the repository's
existing pinned ladder checkout. License: zlib, retained in ../LICENSE.zlib.

## Function provenance and adaptation

`crc32.c`: `multmodp` (lines 163-177), `x2nmodp` (184-195), power-table initialization (255-263), and `crc32_combine_op`/`crc32_combine64` (969-978). The multiply and power function bodies are transplants.

The portable scalar reflected CRC byte loop replaces upstream braided/hardware fast paths. API wrappers use unsigned 32-bit CRCs, bounded byte arrays, and unsigned 64-bit lengths. Chunking, split inspection, and sparse zero-run composition are corpus glue.

The adapter's valid input domain is an unsigned seed from 0 through 11 inclusive.
Each seed denotes the fully specified data/length/alphabet/partition construction
in `evaluate`; calls outside this bounded corpus-adapter domain are unsupported.
The transplanted helper preconditions remain as documented above.

The standalone input has no preprocessing or external calls. The public
`evaluate(seed)` composes the algorithm into deterministic input cases.
`reference_main.c` supplies 12 distinct seeds and also calls `program_main`;
`expected.txt` records the fixed program's exact output. These are construction
oracles, not evidence of GCC-14 qualification.

## Injected defects

1. Remove the power-table index wrap in x2nmodp. Ordinary lengths stay in range; the long sparse gap reaches index 33 and causes an out-of-bounds load.
2. Replace XOR by OR in polynomial CRC combination, corrupting the shared chunk/sparse composition independently of the boundary load.

The main calls the shared combination helper on ordinary short lengths before
the long sparse gap. The initial full-stream main exhausted EvoCC's default
fuel; only the unregistered main workload was reduced. Hidden scoring retains
the twelve complete stream inputs. Both mutations affect the composed result. Fix spans
are original, one-based inclusive ranges computed from the minimal patch.

## Qualification status

Construction only. Sealed GCC-14 -O0 buggy/fixed scoring, EvoCC UB diagnosis,
sealed UBSan, hidden-test mutation ablations, and three C0 preflight responses
remain required before registration. No model preflight has been run here.
