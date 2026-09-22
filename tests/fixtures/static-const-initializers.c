#ifndef CASE
#define CASE 1
#endif

#if CASE == 1
int main(void) {
    static const unsigned long flags = 1;
    static const unsigned long combined = flags | 2;
    return combined != 3;
}
#elif CASE == 2
static const unsigned long flags = 1;
static unsigned long combined = flags * 4 + 2;
int main(void) { return combined != 6; }
#elif CASE == 3
int main(void) {
    const int flags = 1;
    static int combined = flags | 2;
    return combined != 3;
}
#elif CASE == 4
static const volatile int flags = 1;
static int combined = flags;
int main(void) { return combined; }
#elif CASE == 5
static int flags = 1;
static int combined = flags;
int main(void) { return combined; }
#elif CASE == 6
extern const int flags;
static int combined = flags;
int main(void) { return combined; }
#elif CASE == 7
int main(void) {
    static const signed char narrow = 258;
    static int combined = narrow | 4;
    return combined != 6;
}
#elif CASE == 8
static const int flags = 1;
_Static_assert(flags == 1, "not a source ICE");
int main(void) { return 0; }
#elif CASE == 9
int input(void);
int main(void) {
    const int flags = input();
    static int combined = flags;
    return combined;
}
#elif CASE == 10
static const int flags;
static int combined = flags + 2;
int main(void) { return combined != 2; }
#elif CASE == 11
int main(void) {
    static const unsigned long a = 1, b = a + 1, c = b + 1;
    return c != 3;
}
#elif CASE == 12
static const int flags = 1;
static int one(void) {
    static const int flags = 2;
    static int combined = flags + 1;
    return combined;
}
int main(void) {
    static int combined = flags + 1;
    return one() != 3 || combined != 2;
}
#elif CASE == 13
typedef __uint128_t U;
static const U high = ((U)1 << 100) + 7;
static const U combined = high | ((U)3 << 64);
int main(void) { return (combined >> 64) != (((U)1 << 36) | 3) || (unsigned)combined != 7; }
#elif CASE == 14
typedef const unsigned short C;
static C seed = 258;
struct result { int a; unsigned long b; };
static struct result values[2] = {{seed, seed << 2}, {seed - 1, seed | 7}};
int main(void) { return values[0].a != 258 || values[0].b != 1032 || values[1].a != 257 || values[1].b != 263; }
#elif CASE == 15
int main(void) {
    const int flags = 1;
    int side = 0;
    static int combined = flags | 2;
    __builtin_expect(flags, side++);
    return combined != 3 || side != 1;
}
#elif CASE == 16
int main(void) {
    const typeof(({ const int hidden = 9; hidden; })) later;
    static int combined = later;
    return combined;
}
#elif CASE == 17
static _Thread_local const int flags = 1;
static int combined = flags;
int main(void) { return combined != 1; }
#endif
