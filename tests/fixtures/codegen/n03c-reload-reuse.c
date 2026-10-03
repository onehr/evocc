/* Unsigned, bounded pressure control for native allocation variants. */
unsigned long reload_pressure(unsigned rounds, unsigned long x, unsigned long y) {
#define STATE(k) unsigned long a##k = x + k, b##k = y + 3 * k;
    STATE(0) STATE(1) STATE(2) STATE(3)
    STATE(4) STATE(5) STATE(6) STATE(7)
    STATE(8) STATE(9) STATE(10) STATE(11)
    STATE(12) STATE(13) STATE(14) STATE(15)
#undef STATE
    for (unsigned i = 0; i < rounds; ++i) {
#define STEP(k, j) a##k = (a##k + b##j) ^ x; b##k = (b##k ^ a##j) + y;
        STEP(0, 1) STEP(1, 2) STEP(2, 3) STEP(3, 4)
        STEP(4, 5) STEP(5, 6) STEP(6, 7) STEP(7, 8)
        STEP(8, 9) STEP(9, 10) STEP(10, 11) STEP(11, 12)
        STEP(12, 13) STEP(13, 14) STEP(14, 15) STEP(15, 0)
#undef STEP
    }
    unsigned long result = 0;
#define USE(k) result ^= a##k + b##k;
    USE(0) USE(1) USE(2) USE(3)
    USE(4) USE(5) USE(6) USE(7)
    USE(8) USE(9) USE(10) USE(11)
    USE(12) USE(13) USE(14) USE(15)
#undef USE
    return result;
}
