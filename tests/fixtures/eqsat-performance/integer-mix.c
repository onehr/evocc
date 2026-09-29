/* Header-free, fixed unsigned arithmetic; calibration plumbing, not application performance. */
int main(void) {
    volatile unsigned count = 1000u;
    unsigned state = 17u;
    for (unsigned i = 0; i < count; ++i)
        state = state * 1664525u + 1013904223u;
    return state == 3976991929u ? 0 : 1;
}
