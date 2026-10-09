# Source

Adapted from Zstandard 1.5.7: lib/common/xxhash.h, XXH32_finalize and XXH32_avalanche. Byte-tail path only.
The block view, input generator and test driver are corpus scaffolding.
The checksum or encoding loop retains the cited public algorithm.
One injected E3010 defect changes the line in fix-spans.json.
The main return is the processed byte count, not the checksum.
The hidden test reads the checksum for twelve seeds.
