/* Fixed in-memory workload. The selected library is linked without rewriting it.
 * Each compression/decompression batch has a separate execution and validates
 * all decoded bytes. Input creation and the single opposite-direction setup
 * are explicitly included in fresh-process timing, not steady-state claims.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef USE_ZSTD
#include "zstd.h"
#else
#include "zlib.h"
#endif

static unsigned char input[65536], packed[131072], output[65536];

int main(int argc, char **argv) {
    unsigned i, state = 20260929;
    unsigned long packed_size = sizeof packed, output_size;
    unsigned long checksum = 0;
    int decompress = argc == 2 && strcmp(argv[1], "decompress") == 0;
    if (argc != 2 || (!decompress && strcmp(argv[1], "compress") != 0)) return 2;
    for (i = 0; i < sizeof input; ++i) {
        state = state * 1664525U + 1013904223U;
        input[i] = (i % 64 < 56) ? (unsigned char)(i % 17) : (unsigned char)(state >> 24);
    }
    for (i = 0; i < 64; ++i) {
        if (!decompress || i == 0) {
#ifdef USE_ZSTD
            packed_size = ZSTD_compress(packed, sizeof packed, input, sizeof input, 3);
            if (ZSTD_isError(packed_size)) return 3;
#else
            packed_size = sizeof packed;
            if (compress2(packed, &packed_size, input, sizeof input, 3) != Z_OK) return 3;
#endif
        }
        if (decompress || i == 63) {
#ifdef USE_ZSTD
            output_size = ZSTD_decompress(output, sizeof output, packed, packed_size);
            if (ZSTD_isError(output_size)) return 4;
#else
            output_size = sizeof output;
            if (uncompress(output, &output_size, packed, packed_size) != Z_OK) return 4;
#endif
            if (output_size != sizeof input || memcmp(output, input, sizeof input)) return 5;
        }
        checksum += packed_size;
    }
    /* A result predicate, not a compiler-specific compressed byte stream. */
    if (checksum == 0 || packed_size >= sizeof input) return 6;
    puts("roundtrip-ok");
    return 0;
}
