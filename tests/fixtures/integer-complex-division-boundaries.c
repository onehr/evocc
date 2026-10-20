/* L07 boundaries of GNU integer-complex division (EvoIR 10.1.14).
 * The quotients are GCC 14's wide algorithm in the component type.
 * CASE 0 to 8 are defined and return 0 when every result matches.
 * CASE 10 to 15 reach an undefined step and must stop with a diagnostic.
 * CASE 16 divides by a zero constant in a static initializer. */
#define INT_MAX __INT_MAX__
#define INT_MIN (-__INT_MAX__ - 1)
#define UINT_MAX (__INT_MAX__ * 2U + 1U)

typedef _Complex int ci;
typedef _Complex unsigned cu;

#define R(z) (__real__ (z))
#define I(z) (__imag__ (z))
#define SAME(z, r, i) (R(z) == (r) && I(z) == (i))

static volatile int sink;

__attribute__((noinline)) static ci divide(ci x, ci y) { return x / y; }

int main(void)
{
#if CASE == 0
    /* Unsigned steps wrap: (9+7i)/(1+2i) has i = (0u - 9u) / 2u. */
    volatile unsigned a = 9, b = 7, c = 1, d = 2;
    cu q = (a + b * 1i) / (c + d * 1i);
    if (!SAME(q, 3u, (0u - 9u) / 2u))
        return 1;
    volatile unsigned big = UINT_MAX;
    cu w = (big + 5u * 1i) / (3u + 2u * 1i);
    /* ratio = 2/3 = 0, div = 3: r = UINT_MAX / 3, i = 5 / 3. */
    return SAME(w, UINT_MAX / 3u, 1u) ? 0 : 2;
#elif CASE == 1
    /* _Complex signed char computes at char width. */
    volatile signed char a = 100, b = 50, c = 3, d = 1;
    _Complex signed char x = a + b * 1i, y = c + d * 1i;
    _Complex signed char q = x / y;
    if (sizeof(x / y) != 2)
        return 1;
    return SAME(q, 33, 16) ? 0 : 2;
#elif CASE == 2
    /* _Complex long keeps 64-bit components. */
    volatile long a = 9000000000000L, b = 7000000000000L, c = 2, d = 1;
    _Complex long q = (a + b * 1i) / (c + d * 1i);
    if (sizeof q != 16)
        return 1;
    return SAME(q, 4500000000000L, 3500000000000L) ? 0 : 2;
#elif CASE == 3
    /* _Complex short /= _Complex int computes in int and narrows. */
    volatile int c = 0, d = 1;
    _Complex short z = -32768 + 5i;
    z /= (ci)(c + d * 1i);
    if (!SAME(z, 5, (short)-32768))
        return 1;
    /* complex /= real divides each component. */
    volatile int n = -4;
    ci w = 9 - 7i;
    w /= n;
    return SAME(w, -2, 1) ? 0 : 2;
#elif CASE == 4
    /* A real target keeps the real component of the quotient. */
    volatile int c = 2, d = 1;
    int x = 9;
    x /= c + d * 1i;
    unsigned char u = 200;
    u /= (ci)(3 + 1i);
    long g = -9;
    g /= c + d * 1i;
    return x == 4 && u == 66 && g == -4 ? 0 : 1;
#elif CASE == 5
    /* Constant operands fold with the same steps. */
    static ci q0 = (9 + 7i) / (2 + 1i);
    static ci q1 = (9 + 7i) / (1 + 2i);
    static ci q2 = 9 / (2 + 1i);
    static ci q3 = (9 + 7i) / (3 - 3i);
    if (!SAME(q0, 4, 3) || !SAME(q1, 3, -4) || !SAME(q2, 4, 0))
        return 1;
    /* c == -d: ratio = -1, div = 6, r = (-7 + 9) / 6, i = (7 + 9) / 6. */
    return SAME(q3, 0, 2) ? 0 : 2;
#elif CASE == 6
    /* The runtime path agrees with the folded constants. */
    static const int v[][6] = {
        { 9, 7, 2, 1, 4, 3 },
        { 9, 7, 1, 2, 3, -4 },
        { 9, 7, 3, -3, 0, 2 },
        { -9, 7, 2, -1, -4, 3 },
        { 100, -50, -7, 5, -14, 7 },
        { 9, 7, 0, 4, 1, -2 },
        { 25, 5, 5, 0, 5, 1 },
    };
    for (unsigned k = 0; k < sizeof v / sizeof v[0]; k++) {
        ci q = divide(v[k][0] + v[k][1] * 1i, v[k][2] + v[k][3] * 1i);
        if (!SAME(q, v[k][4], v[k][5]))
            return 1 + (int)k;
    }
    return 0;
#elif CASE == 7
    /* Only the selected branch executes: the other branch would divide by
     * zero (d / c with c == 0, or c / d with d == 0). */
    volatile int zero = 0;
    ci a = divide(8 + 6i, zero + 2i);
    ci b = divide(8 + 6i, 2 + zero * 1i);
    return SAME(a, 3, -4) && SAME(b, 4, 3) ? 0 : 1;
#elif CASE == 8
    /* The other real-target updates also keep the converted result; a
     * _Bool target tests both components. */
    volatile int one = 1;
    int x = 9, y = 5, z = 5;
    x *= 2 + one * 1i;
    y += one + 7i;
    z -= one + 7i;
    _Bool t = 1, f = 1;
    t *= 0 + 2 * one * 1i;
    f *= 0 * one + 0i;
    return x == 18 && y == 6 && z == 4 && t == 1 && f == 0 ? 0 : 1;
#elif CASE == 10
    /* A zero divisor reaches d / c with c == 0: E3002. */
    volatile int zero = 0;
    ci q = divide(9 + 7i, zero + zero * 1i);
    sink = R(q);
    return 0;
#elif CASE == 11
    /* INT_MIN / -1 through the real-divisor path: E3004. */
    volatile int m = INT_MIN, n = -1;
    ci q = divide(m + 0i, n + 0i);
    sink = R(q);
    return 0;
#elif CASE == 12
    /* |INT_MIN| in the comparison: E3004. */
    volatile int m = INT_MIN;
    ci q = divide(1 + 1i, m + 1i);
    sink = R(q);
    return 0;
#elif CASE == 13
    /* b * ratio - a with a == INT_MIN on the |c| < |d| path: E3004. */
    volatile int m = INT_MIN;
    ci q = divide(m + 0i, 1 + 2i);
    sink = I(q);
    return 0;
#elif CASE == 14
    /* b * ratio + a overflows when |c| == |d|: E3004. */
    volatile int big = INT_MAX;
    ci q = divide(big + big * 1i, 1 + 1i);
    sink = R(q);
    return 0;
#elif CASE == 15
    /* A compound division by zero: E3002. */
    volatile int zero = 0;
    ci z = 9 + 7i;
    z /= zero;
    sink = R(z);
    return 0;
#elif CASE == 16
    /* A zero divisor makes the quotient non-constant: E2208. */
    static ci q = (9 + 7i) / (0 + 0i);
    return R(q);
#endif
}
