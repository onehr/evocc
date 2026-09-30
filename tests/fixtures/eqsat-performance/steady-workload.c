/* Linux steady-state harness. GCC-built harness, unchanged EvoCC library TUs.
 * Input preparation, warmup, final validation and output are outside the clock
 * interval. Library work repeats inside one process; no LTO or timing fitness.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#ifdef USE_CJSON
#include "cJSON.h"
#elif defined(USE_ZSTD)
#include "zstd.h"
#else
#include "zlib.h"
#endif

static unsigned char input[65536], packed[131072], output[65536];
static unsigned long packed_size;

static int operation(int decompress) {
#ifdef USE_CJSON
    const char *text = "{\"name\":\"evocc\",\"items\":[1,2,3,4],\"ok\":true,\"nested\":{\"n\":1.25}}";
    cJSON *root = cJSON_Parse(text), *items, *copy;
    char *encoded;
    (void)decompress;
    if (!root) return 2;
    items = cJSON_GetObjectItemCaseSensitive(root, "items");
    if (!cJSON_IsArray(items) || cJSON_GetArraySize(items) != 4 ||
        cJSON_GetArrayItem(items, 2)->valueint != 3) return 3;
    encoded = cJSON_PrintUnformatted(root);
    if (!encoded || strcmp(encoded, text)) return 4;
    copy = cJSON_Parse(encoded);
    if (!copy || !cJSON_Compare(root, copy, 1)) return 5;
    cJSON_Delete(copy); cJSON_Delete(root); free(encoded);
    return 0;
#elif defined(USE_ZSTD)
    if (decompress) {
        size_t n = ZSTD_decompress(output, sizeof output, packed, packed_size);
        return ZSTD_isError(n) || n != sizeof input;
    }
    packed_size = ZSTD_compress(packed, sizeof packed, input, sizeof input, 3);
    return ZSTD_isError(packed_size);
#else
    if (decompress) {
        unsigned long n = sizeof output;
        return uncompress(output, &n, packed, packed_size) != Z_OK || n != sizeof input;
    }
    packed_size = sizeof packed;
    return compress2(packed, &packed_size, input, sizeof input, 3) != Z_OK;
#endif
}

int main(int argc, char **argv) {
    struct timespec begin, end;
    unsigned i, state = 20260929U;
    unsigned long iterations, warmup;
    unsigned long long elapsed;
    char *tail;
    int decompress;
    if (argc != 3) return 10;
    iterations = strtoul(argv[2], &tail, 10);
    if (*tail || !iterations || iterations > 4194304UL) return 11;
#ifdef USE_CJSON
    if (strcmp(argv[1], "json")) return 12;
    decompress = 0;
#else
    decompress = !strcmp(argv[1], "decompress");
    if (!decompress && strcmp(argv[1], "compress")) return 12;
#endif
    for (i = 0; i < sizeof input; ++i) {
        state = state * 1664525U + 1013904223U;
        input[i] = (i % 64 < 56) ? (unsigned char)(i % 17) : (unsigned char)(state >> 24);
    }
#ifndef USE_CJSON
    if (operation(0)) return 13; /* Decompression setup is not measured. */
#endif
    warmup = iterations / 32UL + 1UL;
    for (i = 0; i < warmup; ++i) if (operation(decompress)) return 14;
    if (clock_gettime(CLOCK_MONOTONIC_RAW, &begin)) return 15;
    for (i = 0; i < iterations; ++i) if (operation(decompress)) return 16;
    if (clock_gettime(CLOCK_MONOTONIC_RAW, &end)) return 17;
#ifndef USE_CJSON
    if (!decompress && operation(1)) return 18;
    if (memcmp(output, input, sizeof input) || !packed_size || packed_size >= sizeof input) return 19;
#endif
    elapsed = (unsigned long long)(end.tv_sec - begin.tv_sec) * 1000000000ULL;
    if (end.tv_nsec < begin.tv_nsec) elapsed -= (unsigned long long)(begin.tv_nsec - end.tv_nsec);
    else elapsed += (unsigned long long)(end.tv_nsec - begin.tv_nsec);
    printf("{\"result\":\"validated\",\"work\":\"%s\",\"iterations\":%lu,\"elapsed_ns\":%llu}\n",
           argv[1], iterations, elapsed);
    return 0;
}
