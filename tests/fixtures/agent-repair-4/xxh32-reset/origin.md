# Source

Adapted from Zstandard 1.5.7, lib/common/xxhash.h, XXH32_finalize and XXH32_avalanche. Byte-tail recurrence only; not the full XXH32 API.
The View, generator and driver are corpus scaffolding, not upstream code.
The paired items use the same kernel with different memory defects.
They are not independent samples of unrelated applications.
The main returns a processed byte count. The hidden test reads the checksum
for twelve seeds. Exactly one line injects E3001; fix-spans.json records it.
The corresponding upstream notice is in ../LICENSE.zstd.
