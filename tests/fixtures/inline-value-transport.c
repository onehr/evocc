/* The second inlined operand can enter a new basic block. The first value
 * must cross that edge explicitly, not survive as a foreign Value ID. */
static int evaluations;
static inline __attribute__((always_inline)) double value(double x) {
    ++evaluations;
    return x;
}
int main(void) {
    if (!__builtin_isunordered(value(__builtin_nan("")), value(1.0))) return 1;
    if (evaluations != 2) return 2;
    if (__builtin_copysign(value(3.0), value(-2.0)) != -3.0) return 3;
    if (evaluations != 4) return 4;
    if (!__builtin_isless(value(-3.0), value(2.0))) return 5;
    if (evaluations != 6) return 6;
    return 0;
}
