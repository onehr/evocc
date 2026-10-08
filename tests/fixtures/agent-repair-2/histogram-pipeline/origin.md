# histogram-pipeline

Public-source adaptation, not an original-for-corpus item.

Pinned source: `zstd-1.5.7.tar.gz`, SHA-256 `eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3`. The source is the repository's
existing pinned ladder checkout. License: BSD, selected from the upstream dual license; retained in ../LICENSE.zstd.

## Function provenance and adaptation

`lib/compress/hist.c`: `HIST_add`, `HIST_count_simple`, and `HIST_count_parallel_wksp`. All three count-loop bodies are transplanted, including four-lane cached stripes and the maximum-symbol check.

The dispatcher uses the simple loop below 20 bytes so the parallel routine never forms pointers before its input. Byte copies replace libc, 32-bit loads are explicitly little endian, and fixed workspace replaces the upstream allocation interface. HIST_add updates a local copy then copies it back, exposing the concrete frequency-array bound to runtime checks. Forward and reversed input histograms exercise the same pipeline.

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

1. Interpret bytes as signed in HIST_add. The shared additive pass is safe for ordinary 7-bit inputs, but the 255 byte in the boundary path indexes before the frequency table.
2. Drop the fourth striped counter during histogram merge. This corrupts the same frequency table later updated by HIST_add.

The main calls the shared additive helper on two ordinary byte spans before
a span containing 255. The initial full-pipeline main exhausted EvoCC's default
fuel; only the unregistered main workload was reduced. Hidden scoring retains
twelve complete pipeline inputs. Both mutations affect the composed result. Fix spans
are original, one-based inclusive ranges computed from the minimal patch.

## Qualification status

Construction only. Sealed GCC-14 -O0 buggy/fixed scoring, EvoCC UB diagnosis,
sealed UBSan, hidden-test mutation ablations, and three C0 preflight responses
remain required before registration. No model preflight has been run here.
