/* L07 probe: which algorithm does integer-complex division use?
 * smith() is GCC's expand_complex_div_wide sequence in the component type.
 * straight() is the textbook ((ac+bd)/(cc+dd), (bc-ad)/(cc+dd)) formula.
 * Exit value: bit 0 = every quotient equals smith(), bit 1 = every quotient
 * equals straight(). Other bits are per-CASE observations (see comments). */
typedef _Complex int ci;
typedef _Complex unsigned cu;
typedef _Complex short cs;

#define R(z) ((int)__real__ (z))
#define I(z) ((int)__imag__ (z))

static int abs_int(int v) { return v < 0 ? -v : v; }

static void smith(int a, int b, int c, int d, int *r, int *i)
{
    if (abs_int(c) < abs_int(d)) {
        int ratio = c / d, div = c * ratio + d;
        *r = (a * ratio + b) / div;
        *i = (b * ratio - a) / div;
    } else {
        int ratio = d / c, div = d * ratio + c;
        *r = (b * ratio + a) / div;
        *i = (b - a * ratio) / div;
    }
}

static void straight(int a, int b, int c, int d, int *r, int *i)
{
    int t = c * c + d * d;
    *r = (a * c + b * d) / t;
    *i = (b * c - a * d) / t;
}

__attribute__((noinline)) static ci divide(ci x, ci y) { return x / y; }

static const int vectors[][4] = {
    { 9, 7, 2, 1 },   /* |c| > |d| */
    { 9, 7, 1, 2 },   /* |c| < |d| */
    { 9, 7, 3, 3 },   /* c == d */
    { 9, 7, 3, -3 },  /* c == -d */
    { 9, 7, 4, 0 },   /* real divisor */
    { 9, 7, 0, 4 },   /* imaginary divisor */
    { -9, 7, 2, -1 }, /* signs */
    { 100, -50, -7, 5 },
    { 5, 0, 2, 1 },   /* real dividend */
    { 25, 5, 5, 0 },
};

int main(void)
{
    int smith_all = 1, straight_all = 1;
#if CASE == 0
    /* runtime division of _Complex int values */
    for (unsigned k = 0; k < sizeof vectors / sizeof vectors[0]; k++) {
        const int *v = vectors[k];
        ci x = v[0] + v[1] * 1i, y = v[2] + v[3] * 1i;
        ci q = divide(x, y);
        int sr, si, tr, ti;
        smith(v[0], v[1], v[2], v[3], &sr, &si);
        straight(v[0], v[1], v[2], v[3], &tr, &ti);
        smith_all &= R(q) == sr && I(q) == si;
        straight_all &= R(q) == tr && I(q) == ti;
    }
    return smith_all | (straight_all << 1);
#elif CASE == 1
    /* constant operands folded by the compiler: (9+7i)/(2+1i), (9+7i)/(1+2i);
     * smith() gives 4+3i and 3-4i, straight() gives 5+1i and 4-2i */
    ci q0 = (9 + 7i) / (2 + 1i), q1 = (9 + 7i) / (1 + 2i);
    int a = R(q0) == 4 && I(q0) == 3 && R(q1) == 3 && I(q1) == -4;
    int b = R(q0) == 5 && I(q0) == 1 && R(q1) == 4 && I(q1) == -2;
    return a | (b << 1);
#elif CASE == 2
    /* unsigned components: smith() in unsigned arithmetic gives 4 and 3 for
     * (9+7i)/(2+1i); the straight formula gives 5 and 1. */
    volatile unsigned a0 = 9, b0 = 7, c0 = 2, d0 = 1;
    cu x = a0 + b0 * 1i, y = c0 + d0 * 1i;
    cu q = x / y;
    return ((unsigned)__real__ q == 4 && (unsigned)__imag__ q == 3)
         | (((unsigned)__real__ q == 5 && (unsigned)__imag__ q == 1) << 1);
#elif CASE == 3
    /* _Complex short keeps short components: sizeof is 4 for the quotient */
    volatile short a0 = 9, b0 = 7, c0 = 2, d0 = 1;
    cs x = a0 + b0 * 1i, y = c0 + d0 * 1i;
    cs q = x / y;
    return (R(q) == 4 && I(q) == 3) | ((R(q) == 5 && I(q) == 1) << 1)
         | ((sizeof(x / y) == 4) << 2);
#elif CASE == 4
    /* compound assignment, complex /= complex and complex /= real */
    volatile int c0 = 2, d0 = 1;
    ci z = 9 + 7i;
    z /= c0 + d0 * 1i;
    ci w = 9 + 7i;
    w /= c0;
    return (R(z) == 4 && I(z) == 3) | ((R(z) == 5 && I(z) == 1) << 1)
         | ((R(w) == 4 && I(w) == 3) << 2);
#elif CASE == 5
    /* real / complex: 9 / (2+1i); smith gives 4+0i, straight gives 3-1i */
    volatile int n = 9, c0 = 2, d0 = 1;
    ci q = n / (c0 + d0 * 1i);
    return (R(q) == 4 && I(q) == 0) | ((R(q) == 3 && I(q) == -1) << 1);
#endif
}
