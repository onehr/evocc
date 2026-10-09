# Larger silent-UB repair corpus

There are 12 single-file programs of 722 lines. Each has one injected defect:
four E3001, four E3010 and four E3011. There are no synthetic-only items.
The seven files in each directory have the same roles as experiment 4.

The programs adapt a shared set of public algorithms. They do not reproduce
an upstream bug. Four input workloads exercise records, repetitions, control
characters and runs. Every workload uses bounded JSON parsing and printing,
Adler-32, a nibble-table CRC, a hash, striped symbol counts, canonical prefix
codes, LZ match selection and decoding. All modules affect the fingerprint.
No dead module or long constant table exists to meet the line requirement.

The public `evaluate` function accepts any unsigned seed. The hidden driver
checks 16 seeds, including unsigned boundaries. `main` runs one short input
and returns the consumed length. A wrong fingerprint can therefore remain
silent in the default build-and-run loop. The defects are in the shared view
module, more than 300 lines before `main`, and affect several callers.

The items share both algorithms and defect shapes. They are not independent
samples of real C projects. A line-count contrast cannot isolate size from
source structure. Report that limit with the registered item-level tests.

The sources were copied from the existing ladder trees to a temporary
directory. No build ran in `.build/`. Retained source SHA-256 values:

| Source | SHA-256 |
| --- | --- |
| cJSON 1.7.19 `cJSON.c` | `298581a04a36c0165da4b0aade235c23088cb2faa58651d720ea2f3706ed0b0d` |
| zlib 1.3.2 `adler32.c` | `9cd1443a24ff2a3053961695bd432035c58347386a420d3388232376ebabe211` |
| zlib 1.3.2 `crc32.c` | `da37f3483e77c20f64c28cf55d62d7bed9d62af33eb401be906835d84539b9c9` |
| zlib 1.3.2 `trees.c` | `a16217d5ea6d8342ff08afe5207b1dab620f05610e121dda55b4f9aae9de4f68` |
| Zstandard 1.5.7 `lib/compress/hist.c` | `1a613082642b9cabdeecd3b5929f7230cbb2f8344a28f5010f3aec12cb647ab8` |
| Zstandard 1.5.7 `lib/common/xxhash.h` | `8cb837b21a8fe9a6b9dbcd0961ab16e733bfcbfa9e003f3a496ce07ae80aa8ee` |
| Zstandard 1.5.7 `lib/compress/zstd_fast.c` | `8037320f321836652e73ab6cbf7b8f1f4c97cef38835ac6b4aaf0af716aa398d` |

The first scratch bundle exceeded interpreter fuel while building a full
CRC table and counting 256 symbols. Before making the corpus, it was reduced
to a nibble CRC table and the 32-symbol alphabet used by the prefix coder.
This changes the workload, not the interpreter or the feedback. The final
qualification must show no E3008. Expected outputs were obtained only after
all three sealed reference builds agreed on five executions each.
