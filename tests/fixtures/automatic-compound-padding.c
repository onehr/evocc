/* Poly1305 finalization uses a compound literal, then copies its representation. */
struct state {
    unsigned char prefix[16];
    unsigned int count;
    unsigned int key[4];
    union { unsigned int words[5]; unsigned long wide[3]; } limbs[2];
};

static void reset(struct state *p) {
    *p = (struct state){};
}

int main(void) {
    struct state state;
    __builtin_memset(&state, 0xa5, sizeof(state));
    reset(&state);
    const unsigned char *bytes = (const unsigned char *)&state;
    for (unsigned long i = 0; i < sizeof(state); ++i)
        if (bytes[i]) return 1;
    int calls = 0;
    for (int i = 0; i < 2; ++i) {
        state = (struct state){.count = ++calls};
        if (state.count != (unsigned int)(i + 1)) return 2;
        for (unsigned long j = 36; j < 40; ++j)
            if (bytes[j]) return 3;
    }
    return calls != 2;
}
