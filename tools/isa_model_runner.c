/* N07 transport only. Decoding and instruction execution are Sail-generated. */
#include <inttypes.h>
#include <stdio.h>
#include "integer_model.h"

extern void model_init(void);
extern void model_fini(void);

int main(void) {
    uint64_t a, b;
    unsigned count;
    int fields;
    model_init();
    while ((fields = scanf("%" SCNx64 " %" SCNx64 " %u", &a, &b, &count)) == 3) {
        if (count == 0 || count > 128) return 2;
        zevocc_model_reset(a, b);
        for (unsigned i = 0; i < count; i++) {
            uint32_t word;
            if (scanf("%" SCNx32, &word) != 1) return 2;
            zevocc_model_step(word);
            if (have_exception) {
                fprintf(stderr, "UNSUPPORTED word=%08" PRIx32 " index=%u\n", word, i);
                return 3;
            }
        }
        printf("%016" PRIx64 " %01" PRIx64 "\n",
               zevocc_model_result(UNIT), zevocc_model_flags(UNIT));
    }
    if (fields != EOF || ferror(stdin)) return 2;
    model_fini();
    return 0;
}
